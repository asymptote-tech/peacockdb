#include "peacock_gpu.h"
#include "plan_executor.h"

#include <cudf/interop.hpp>
#include <cudf/null_mask.hpp>
#include <cudf/unary.hpp>

#include <arrow/buffer.h>
#include <arrow/c/bridge.h>
#include <arrow/io/memory.h>
#include <arrow/ipc/writer.h>

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <new>
#include <string>

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

struct peacock_executor {
  uint64_t memory_limit;
  std::string last_error;
};

// ---------------------------------------------------------------------------
// Versioning
// ---------------------------------------------------------------------------

const char* peacock_gpu_version() {
  // Trivial cudf call to ensure libcudf.so appears as a dynamic dependency.
  (void)cudf::num_bitmask_words(0);
  return "0.1.0";
}

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
// Query execution
// ---------------------------------------------------------------------------

int peacock_execute(peacock_executor_t* executor,
                    const uint8_t* plan_bytes,
                    uint64_t plan_len,
                    uint8_t** out_result_bytes,
                    uint64_t* out_result_len) {
  if (!executor || !plan_bytes || !out_result_bytes || !out_result_len)
    return 1;

  try {
    auto result = peacock::execute_plan(plan_bytes, plan_len);

    // Build column metadata (names) for the Arrow schema.
    std::vector<cudf::column_metadata> col_meta;
    col_meta.reserve(result.column_names.size());
    for (const auto& name : result.column_names) {
      col_meta.push_back({name});
    }
    auto tview = result.table->view();

    // arrow-ipc on the Rust side rejects DECIMAL32/DECIMAL64 ("Unexpected
    // decimal bit width 64"). Widen to DECIMAL128 (same scale) before export
    // so the wire format only carries types the consumer can decode.
    std::vector<std::unique_ptr<cudf::column>> widened;
    std::vector<cudf::column_view> widened_views;
    widened_views.reserve(tview.num_columns());
    for (cudf::size_type i = 0; i < tview.num_columns(); ++i) {
      auto col = tview.column(i);
      auto t = col.type();
      if (t.id() == cudf::type_id::DECIMAL32 ||
          t.id() == cudf::type_id::DECIMAL64) {
        auto w = cudf::cast(col, cudf::data_type{cudf::type_id::DECIMAL128,
                                                  t.scale()});
        widened_views.push_back(w->view());
        widened.push_back(std::move(w));
      } else {
        widened_views.push_back(col);
      }
    }
    cudf::table_view export_view{widened_views};

    // Export schema via the Arrow C Data Interface.
    auto c_schema = cudf::to_arrow_schema(export_view, col_meta);
    auto schema   = arrow::ImportSchema(c_schema.get()).ValueOrDie();

    // Copy table data to host and export as an Arrow record batch.
    auto c_array = cudf::to_arrow_host(export_view);
    auto batch   = arrow::ImportRecordBatch(&c_array->array, schema).ValueOrDie();

    // Serialize as an Arrow IPC stream into a memory buffer.
    auto sink   = arrow::io::BufferOutputStream::Create().ValueOrDie();
    auto writer = arrow::ipc::MakeStreamWriter(sink.get(), schema).ValueOrDie();

    auto st = writer->WriteRecordBatch(*batch);
    if (!st.ok()) throw std::runtime_error("IPC write: " + st.ToString());
    st = writer->Close();
    if (!st.ok()) throw std::runtime_error("IPC close: " + st.ToString());

    auto buffer = sink->Finish().ValueOrDie();

    *out_result_len  = static_cast<uint64_t>(buffer->size());
    *out_result_bytes = static_cast<uint8_t*>(std::malloc(*out_result_len));



    if (!*out_result_bytes) throw std::runtime_error("malloc failed for result buffer");
    std::memcpy(*out_result_bytes, buffer->data(), *out_result_len);

    return 0;
  } catch (const std::exception& e) {
    executor->last_error = e.what();
    std::fprintf(stderr, "[peacock_execute] error: %s\n", e.what());
    return 1;
  } catch (...) {
    executor->last_error = "unknown exception";
    std::fprintf(stderr, "[peacock_execute] unknown exception\n");
    return 1;
  }
}

void peacock_result_free(uint8_t* result_bytes) {
  std::free(result_bytes);
}

const char* peacock_last_error(peacock_executor_t* executor) {
  if (!executor) return "";
  return executor->last_error.c_str();
}