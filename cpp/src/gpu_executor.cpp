#include "peacock_gpu.h"
#include "plan_executor.h"

#include <cudf/interop.hpp>
#include <cudf/null_mask.hpp>

#include <cudf/aggregation.hpp>
#include <cudf/filling.hpp>
#include <cudf/reduction.hpp>
#include <cudf/scalar/scalar.hpp>
#include <cudf/scalar/scalar_factories.hpp>
#include <cudf/types.hpp>


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
  
  constexpr cudf::size_type N = 100;
  auto init = cudf::make_fixed_width_scalar<int64_t>(1);
  auto step = cudf::make_fixed_width_scalar<int64_t>(1);
  auto col  = cudf::sequence(N, *init, *step);

  assert(col->size() == N);
  assert(col->type().id() == cudf::type_id::INT64);

  // Sum on the GPU; expected = N*(N+1)/2
  auto agg    = cudf::make_sum_aggregation<cudf::reduce_aggregation>();
  auto result = cudf::reduce(col->view(), *agg, cudf::data_type{cudf::type_id::INT64});

  auto* scalar = dynamic_cast<cudf::numeric_scalar<int64_t>*>(result.get());
  assert(scalar);
  assert(scalar->is_valid());
  assert(scalar->value() == static_cast<int64_t>(N) * (N + 1) / 2);
  

  try {
    auto result = peacock::execute_plan(plan_bytes, plan_len);

    // Build column metadata (names) for the Arrow schema.
    std::vector<cudf::column_metadata> col_meta;
    col_meta.reserve(result.column_names.size());
    for (const auto& name : result.column_names) {
      col_meta.push_back({name});
    }
    auto tview = result.table->view();

    // Export schema via the Arrow C Data Interface.
    auto c_schema = cudf::to_arrow_schema(tview, col_meta);
    auto schema   = arrow::ImportSchema(c_schema.get()).ValueOrDie();

    // Copy table data to host and export as an Arrow record batch.
    auto c_array = cudf::to_arrow_host(tview);
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
