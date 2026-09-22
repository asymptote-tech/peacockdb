#include "peacock_gpu.h"
#include "peacock/partitioning.hpp"
#include "peacock/rmm_pool.hpp"
#include "plan_executor.h"
#include "plan_executor_internal.h"

#include <cudf/copying.hpp>
#include <cudf/interop.hpp>
#include <cudf/null_mask.hpp>
#include <cudf/utilities/traits.hpp>

#include <arrow/buffer.h>
#include <arrow/c/bridge.h>
#include <arrow/io/memory.h>
#include <arrow/ipc/writer.h>
#include <arrow/type.h>

#include <cuda_runtime.h>

#include <cstddef>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <new>
#include <string>
#include <vector>

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

struct peacock_executor {
  uint64_t memory_limit;
  std::string last_error;
  // Holds the parsed plan + resident intermediate handles for node-by-node
  // execution; null when no plan is loaded.
  std::unique_ptr<peacock::NodeSession> session;
};

// The stats and region entry points hand C++'s structs back as the C ones by cast,
// which is sound only while each pair is laid out identically. Adding a member to one
// and not the other, or reordering either, fails here rather than silently handing Rust
// fields from the wrong offsets.
#define PCK_SAME_OFFSET(a, b, field)                      \
  static_assert(offsetof(a, field) == offsetof(b, field), \
                "the two definitions of " #field " must sit at the same offset")

static_assert(sizeof(PeacockNodeStats) == sizeof(peacock::NodeStats));
PCK_SAME_OFFSET(PeacockNodeStats, peacock::NodeStats, rows);
PCK_SAME_OFFSET(PeacockNodeStats, peacock::NodeStats, varlen_content_bytes);

static_assert(sizeof(PeacockNodeRegion) == sizeof(peacock::NodeRegion));
PCK_SAME_OFFSET(PeacockNodeRegion, peacock::NodeRegion, seq);
PCK_SAME_OFFSET(PeacockNodeRegion, peacock::NodeRegion, partition);
PCK_SAME_OFFSET(PeacockNodeRegion, peacock::NodeRegion, call_index);
PCK_SAME_OFFSET(PeacockNodeRegion, peacock::NodeRegion, host_us);
PCK_SAME_OFFSET(PeacockNodeRegion, peacock::NodeRegion, device_us);

/// The IPC stream `bytes` as a malloc'd buffer the caller frees with peacock_result_free.
static void hand_over(const arrow::Buffer& bytes, uint8_t** out_bytes, uint64_t* out_len) {
  *out_len = static_cast<uint64_t>(bytes.size());
  *out_bytes = static_cast<uint8_t*>(std::malloc(*out_len));
  if (!*out_bytes) throw std::runtime_error("malloc failed for result buffer");
  std::memcpy(*out_bytes, bytes.data(), *out_len);
}

/// The table's schema as Arrow sees it: names from `column_names`, every decimal at the
/// width's maximum precision, since cuDF stores none.
static std::shared_ptr<arrow::Schema> arrow_schema_of(
    const cudf::table_view& tview, const std::vector<std::string>& column_names) {
  std::vector<cudf::column_metadata> col_meta;
  col_meta.reserve(column_names.size());
  for (const auto& name : column_names) col_meta.push_back({name});
  auto c_schema = cudf::to_arrow_schema(tview, col_meta);
  return arrow::ImportSchema(c_schema.get()).ValueOrDie();
}

namespace peacock {

void export_table_to_ipc(const cudf::table_view& tview,
                         const std::vector<std::string>& column_names,
                         const int32_t* decimal_precisions, uint8_t** out_bytes,
                         uint64_t* out_len) {
  for (cudf::size_type i = 0; i < tview.num_columns(); ++i) {
    auto t = tview.column(i).type();
    bool fixed = cudf::is_fixed_point(t);
    if (fixed && t.id() != cudf::type_id::DECIMAL128)
      throw std::runtime_error("export: column " + column_names[i] +
                               " is a narrow fixed_point; the loader widens every decimal to "
                               "DECIMAL128 and nothing may narrow one");
    if (decimal_precisions && decimal_precisions[i] != 0 && !fixed)
      throw std::runtime_error("export: column " + column_names[i] +
                               " is not a decimal but was given precision " +
                               std::to_string(decimal_precisions[i]));
  }
  auto schema = arrow_schema_of(tview, column_names);
  // The label the plan declared. cuDF stores no precision (25.02 cannot even be told one),
  // so the schema message is where it is written; the buffers below are untouched.
  for (int i = 0; i < schema->num_fields(); ++i) {
    if (!decimal_precisions || decimal_precisions[i] == 0) continue;
    auto scale = -tview.column(i).type().scale();
    auto field = schema->field(i)->WithType(arrow::decimal128(decimal_precisions[i], scale));
    schema = schema->SetField(i, field).ValueOrDie();
  }
  auto c_array = cudf::to_arrow_host(tview);
  auto batch = arrow::ImportRecordBatch(&c_array->array, schema).ValueOrDie();

  auto sink = arrow::io::BufferOutputStream::Create().ValueOrDie();
  auto writer = arrow::ipc::MakeStreamWriter(sink.get(), schema).ValueOrDie();
  auto st = writer->WriteRecordBatch(*batch);
  if (!st.ok()) throw std::runtime_error("IPC write: " + st.ToString());
  st = writer->Close();
  if (!st.ok()) throw std::runtime_error("IPC close: " + st.ToString());
  hand_over(*sink->Finish().ValueOrDie(), out_bytes, out_len);
}

}  // namespace peacock

// ---------------------------------------------------------------------------
// Versioning
// ---------------------------------------------------------------------------

const char* peacock_gpu_version() {
  // Trivial cudf call to ensure libcudf.so appears as a dynamic dependency.
  (void)cudf::num_bitmask_words(0);
  return "0.1.0";
}

// ---------------------------------------------------------------------------
// Benchmark instrumentation
// ---------------------------------------------------------------------------

int peacock_install_rmm_pool(uint64_t bytes, PeacockRmmPoolInfo* out_info) {
  if (!out_info) return 1;

  // install_rmm_pool() is idempotent and already degrades to the default resource on a
  // failed reservation, so there is nothing here that can throw in the ordinary case. The
  // catch is for the extraordinary one — and it returns UNAVAILABLE rather than an error
  // code for the reason argued on the declaration: no pool is a slower run, not a dead one.
  peacock::RmmPoolStatus status;
  try {
    status = peacock::install_rmm_pool(static_cast<std::size_t>(bytes));
  } catch (const std::exception& e) {
    std::fprintf(stderr, "[peacock_install_rmm_pool] error: %s\n", e.what());
  } catch (...) {
    std::fprintf(stderr, "[peacock_install_rmm_pool] unknown exception\n");
  }

  switch (status.state) {
    case peacock::RmmPoolStatus::State::Installed:
      out_info->state = PEACOCK_RMM_POOL_INSTALLED;
      break;
    case peacock::RmmPoolStatus::State::Unavailable:
      out_info->state = PEACOCK_RMM_POOL_UNAVAILABLE;
      break;
  }
  out_info->integrated = status.integrated ? 1 : 0;
  out_info->free_bytes = static_cast<uint64_t>(status.free_bytes);
  out_info->initial_bytes = static_cast<uint64_t>(status.initial_bytes);
  out_info->maximum_bytes = static_cast<uint64_t>(status.maximum_bytes);
  return 0;
}

int peacock_set_node_timing(int mode) {
  // A value this build does not name is refused rather than read as off: a caller that
  // asked for a mode and got silence measures nothing and has no way to find out.
  switch (mode) {
    case PEACOCK_NODE_TIMING_OFF:
      peacock::set_node_timing(peacock::NodeTiming::Off);
      return 0;
    case PEACOCK_NODE_TIMING_EVENTS:
      peacock::set_node_timing(peacock::NodeTiming::Events);
      return 0;
    default:
      return 1;
  }
}

void peacock_set_nvtx_ranges(int on) { peacock::set_nvtx_ranges(on != 0); }

void peacock_nvtx_push_range(const char* name) { peacock::push_harness_range(name); }

void peacock_nvtx_pop_range() { peacock::pop_harness_range(); }

// ---------------------------------------------------------------------------
// Executor lifecycle
// ---------------------------------------------------------------------------

int peacock_executor_create(uint64_t gpu_memory_limit,
                            peacock_executor_t** out_executor) {
  if (!out_executor) return 1;

  auto* ex = new (std::nothrow) peacock_executor{gpu_memory_limit, {}};
  if (!ex) return 1;

  *out_executor = ex;
  return 0;
}

void peacock_executor_destroy(peacock_executor_t* executor) {
  delete executor;
}

// ---------------------------------------------------------------------------
// Results and errors
// ---------------------------------------------------------------------------

void peacock_result_free(uint8_t* result_bytes) {
  std::free(result_bytes);
}

const char* peacock_last_error(peacock_executor_t* executor) {
  if (!executor) return "";
  return executor->last_error.c_str();
}

// ---------------------------------------------------------------------------
// Node-by-node execution (unified CPU/GPU node-executor interface)
// ---------------------------------------------------------------------------

int peacock_executor_begin_plan(peacock_executor_t* executor,
                                const uint8_t* plan_bytes, uint64_t plan_len,
                                uint64_t* out_node_count) {
  if (!executor || !plan_bytes || !out_node_count) return 1;
  try {
    executor->session = std::make_unique<peacock::NodeSession>(plan_bytes, plan_len);
    *out_node_count = static_cast<uint64_t>(executor->session->node_count());
    return 0;
  } catch (const std::exception& e) {
    executor->last_error = e.what();
    return 1;
  } catch (...) {
    executor->last_error = "unknown exception";
    return 1;
  }
}

int peacock_executor_execute_node(peacock_executor_t* executor, uint64_t seq,
                                  const uint64_t* input_handles,
                                  const uint64_t* input_child_counts, uint64_t n_children,
                                  uint64_t* out_handles, uint64_t out_cap,
                                  uint64_t* out_count, PeacockNodeStats* out_stats) {
  if (!executor || !out_handles || !out_count) return 1;
  if (!executor->session) {
    executor->last_error = "no plan loaded (call peacock_executor_begin_plan first)";
    return 1;
  }
  try {
    // out_stats is a caller array of out_cap, filled per partition (parallel to
    // out_handles); the layout the cast relies on is checked at the top of this file.
    size_t n_out = 0;
    executor->session->execute_node(
        seq, input_handles, input_child_counts, static_cast<size_t>(n_children),
        out_handles, static_cast<size_t>(out_cap), &n_out,
        reinterpret_cast<peacock::NodeStats*>(out_stats));
    *out_count = static_cast<uint64_t>(n_out);
    return 0;
  } catch (const std::exception& e) {
    executor->last_error = e.what();
    // On failure mid-walk, drop the whole plan + all resident intermediates.
    executor->session.reset();
    return 1;
  } catch (...) {
    executor->last_error = "unknown exception";
    executor->session.reset();
    return 1;
  }
}

int peacock_executor_execute_scan_rowgroups(peacock_executor_t* executor, uint64_t seq,
                                            const uint32_t* row_groups, uint64_t n,
                                            uint64_t* out_handle, PeacockNodeStats* out_stats) {
  if (!executor || !row_groups || !out_handle) return 1;
  if (!executor->session) {
    executor->last_error = "no plan loaded (call peacock_executor_begin_plan first)";
    return 1;
  }
  if (n == 0) {
    // Refused rather than read as "every group": the flat buffers' convention, where an
    // empty row-group map means legacy single-partition, must not leak into this call.
    executor->last_error = "peacock_executor_execute_scan_rowgroups: empty row-group list";
    return 1;
  }
  try {
    *out_handle =
        executor->session->execute_scan_rowgroups(seq, {row_groups, static_cast<std::size_t>(n)},
                                                  reinterpret_cast<peacock::NodeStats*>(out_stats));
    return 0;
  } catch (const std::exception& e) {
    executor->last_error = e.what();
    executor->session.reset();
    return 1;
  } catch (...) {
    executor->last_error = "unknown exception";
    executor->session.reset();
    return 1;
  }
}

int peacock_executor_slice_handle(peacock_executor_t* executor, uint64_t handle, uint64_t offset,
                                  uint64_t length, uint64_t* out_handle) {
  if (!executor || !out_handle) return 1;
  if (!executor->session) {
    executor->last_error = "no plan loaded";
    return 1;
  }
  try {
    *out_handle = executor->session->slice_handle(handle, offset, length);
    return 0;
  } catch (const std::exception& e) {
    executor->last_error = e.what();
    executor->session.reset();
    return 1;
  } catch (...) {
    executor->last_error = "unknown exception";
    executor->session.reset();
    return 1;
  }
}

int peacock_executor_collect_node_regions(peacock_executor_t* executor, PeacockNodeRegion* out,
                                          uint64_t cap, uint64_t* out_count) {
  if (!executor || !out_count) return 1;
  if (!executor->session) {
    executor->last_error = "no plan loaded";
    return 1;
  }
  try {
    const uint64_t recorded = static_cast<uint64_t>(executor->session->recorded_regions());
    *out_count = recorded;
    // Neither of these drains: asking the count is the first half of one call, and a
    // buffer too small is a caller's mistake rather than a reason to lose the regions it
    // asked for.
    if (!out && cap == 0) return 0;
    if (recorded > cap) {
      executor->last_error = "collect_node_regions: buffer holds " + std::to_string(cap) + " of " +
                             std::to_string(recorded) + " recorded regions";
      return 1;
    }
    auto regions = executor->session->collect_node_regions();
    *out_count = static_cast<uint64_t>(regions.size());
    // Copied whole, because a field-by-field copy has a line to forget and a size assert
    // cannot see that: a field added to both structs keeps the sizes equal, and the copy
    // that skipped it reports zeros. The per-field offsets above are what make the
    // whole-struct copy safe.
    if (!regions.empty())
      std::memcpy(out, regions.data(), regions.size() * sizeof(PeacockNodeRegion));
    return 0;
  } catch (const std::exception& e) {
    executor->last_error = e.what();
    return 1;
  } catch (...) {
    executor->last_error = "unknown exception";
    return 1;
  }
}

int peacock_result_from_handle(peacock_executor_t* executor, uint64_t handle, uint64_t offset,
                               uint64_t length, const int32_t* decimal_precisions,
                               uint64_t n_columns, uint8_t** out_ipc, uint64_t* out_ipc_len) {
  if (!executor || !out_ipc || !out_ipc_len) return 1;
  if (!executor->session) {
    executor->last_error = "no plan loaded";
    return 1;
  }
  try {
    const auto& result = executor->session->table_for(handle);
    auto view = result.table->view();
    if (n_columns != 0 && n_columns != static_cast<uint64_t>(view.num_columns())) {
      executor->last_error = "result_from_handle: " + std::to_string(n_columns) +
                             " declared precisions for a table of " +
                             std::to_string(view.num_columns()) + " columns";
      return 1;
    }
    if (n_columns == 0) decimal_precisions = nullptr;
    auto [begin, end] = peacock::clamp_row_range(offset, length, view.num_rows());
    // A range naming no rows of a non-empty table ships nothing — through time_export like
    // the arm below, since the driver journals the call either way and join_regions refuses
    // a journalled call no region answered. An empty table takes the whole-table arm
    // instead, so a caller asking for all of one still gets its schema-only stream.
    if (begin == end && view.num_rows() > 0) {
      executor->session->time_export(handle, [&] {
        *out_ipc = nullptr;  // so "nothing to free" is a pointer the caller can act on
        *out_ipc_len = 0;
      });
      return 0;
    }
    if (begin != 0 || end != view.num_rows()) view = cudf::slice(view, {begin, end}).front();
    executor->session->time_export(handle, [&] {
      peacock::export_table_to_ipc(view, result.column_names, decimal_precisions, out_ipc,
                                   out_ipc_len);
    });
    return 0;
  } catch (const std::exception& e) {
    executor->last_error = e.what();
    return 1;
  } catch (...) {
    executor->last_error = "unknown exception";
    return 1;
  }
}

int peacock_handle_schema(peacock_executor_t* executor, uint64_t handle, uint8_t** out_ipc,
                          uint64_t* out_len) {
  if (!executor || !out_ipc || !out_len) return 1;
  if (!executor->session) {
    executor->last_error = "no plan loaded";
    return 1;
  }
  try {
    const auto& result = executor->session->table_for(handle);
    auto schema = arrow_schema_of(result.table->view(), result.column_names);
    auto sink = arrow::io::BufferOutputStream::Create().ValueOrDie();
    auto writer = arrow::ipc::MakeStreamWriter(sink.get(), schema).ValueOrDie();
    auto st = writer->Close();
    if (!st.ok()) throw std::runtime_error("IPC close: " + st.ToString());
    hand_over(*sink->Finish().ValueOrDie(), out_ipc, out_len);
    return 0;
  } catch (const std::exception& e) {
    executor->last_error = e.what();
    return 1;
  } catch (...) {
    executor->last_error = "unknown exception";
    return 1;
  }
}

void peacock_handle_release(peacock_executor_t* executor, uint64_t handle) {
  if (executor && executor->session) executor->session->release(handle);
}

void peacock_executor_end_plan(peacock_executor_t* executor) {
  if (executor) executor->session.reset();
}

int peacock_spark_partition_ids(const void* schema, const void* array,
                                const uint32_t* key_cols, uint64_t num_keys,
                                uint32_t num_partitions, uint32_t seed,
                                int32_t* out_pids, uint64_t out_cap, uint64_t* out_n) {
  if (!schema || !array || !key_cols || !out_pids || !out_n) return 1;
  try {
    // Import the Arrow C-Data struct array (= the key table) into cuDF. The ABI is
    // stable, so arrow-rs's FFI_ArrowSchema/FFI_ArrowArray reinterpret directly to
    // cuDF's ArrowSchema/ArrowArray.
    auto table = cudf::from_arrow(reinterpret_cast<const ArrowSchema*>(schema),
                                  reinterpret_cast<const ArrowArray*>(array));
    std::vector<cudf::size_type> keys;
    keys.reserve(num_keys);
    for (uint64_t i = 0; i < num_keys; ++i) {
      keys.push_back(static_cast<cudf::size_type>(key_cols[i]));
    }
    auto pid       = peacock::partitioning::spark_partition_ids(
        table->view(), keys, static_cast<cudf::size_type>(num_partitions), seed);
    auto const view = pid->view();
    auto const n    = static_cast<uint64_t>(view.size());
    if (n > out_cap) return 1;
    cudaMemcpy(out_pids, view.data<int32_t>(), n * sizeof(int32_t), cudaMemcpyDeviceToHost);
    *out_n = n;
    return 0;
  } catch (const std::exception& e) {
    std::fprintf(stderr, "peacock_spark_partition_ids: %s\n", e.what());
    return 1;
  } catch (...) {
    return 1;
  }
}

int peacock_handle_from_arrow(peacock_executor_t* executor, const void* schema, const void* array,
                              uint64_t* out_handle) {
  if (!executor || !schema || !array || !out_handle) return 1;
  if (!executor->session) {
    executor->last_error = "no plan loaded (call peacock_executor_begin_plan first)";
    return 1;
  }
  try {
    auto const* c_schema = reinterpret_cast<const ArrowSchema*>(schema);
    auto table = cudf::from_arrow(c_schema, reinterpret_cast<const ArrowArray*>(array));
    std::vector<std::string> names;
    names.reserve(static_cast<size_t>(c_schema->n_children));
    for (int64_t i = 0; i < c_schema->n_children; ++i) {
      names.emplace_back(c_schema->children[i]->name);
    }
    *out_handle =
        executor->session->adopt(peacock::TableResult{std::move(table), std::move(names)});
    return 0;
  } catch (const std::exception& e) {
    // Nothing was consumed, so the session stays usable — unlike execute_node's reset.
    executor->last_error = e.what();
    return 1;
  } catch (...) {
    executor->last_error = "unknown exception";
    return 1;
  }
}
