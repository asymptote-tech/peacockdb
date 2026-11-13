/// `peacock_handle_duplicate`: a second reader of one table. A shared source's batch goes to
/// every consumer, and each consumer consumes its handle. A duplicate is a second registry entry
/// over the same owners: no device bytes, and each handle released on its own.
///
/// Part of `peacock_plan_tests`, whose `main()` (test_plan_executor.cpp) installs the pool and
/// the statistics adaptor `allocated_by` reads.

#include "peacock_gpu.h"
#include "plan_executor.h"
#include "generated/gpu_plan_generated.h"
#include "peacock/rmm_pool.hpp"

#include <arrow/api.h>
#include <arrow/buffer.h>
#include <arrow/c/bridge.h>
#include <arrow/io/memory.h>
#include <arrow/ipc/reader.h>
#include <arrow/record_batch.h>

#include <flatbuffers/flatbuffers.h>
#include <gtest/gtest.h>

#include <cstdint>
#include <cstdlib>
#include <memory>
#include <string>
#include <utility>
#include <vector>

namespace {
namespace fb = peacock::plan;

std::string customer_path() {
  const char* env = std::getenv("PEACOCK_TESTDATA_DIR");
  return std::string(env ? env : PEACOCK_TESTDATA_DIR) + "/tpch.minimal/customer.parquet";
}

/// customer.parquet's declared columns, as test_plan_executor.cpp's `customer_fields`.
flatbuffers::Offset<fb::Schema> customer_schema(flatbuffers::FlatBufferBuilder& fbb) {
  const std::vector<std::pair<std::string, fb::DataType>> fields{
      {"c_custkey", fb::DataType_Int64},   {"c_name", fb::DataType_Utf8},
      {"c_address", fb::DataType_Utf8},    {"c_nationkey", fb::DataType_Int32},
      {"c_phone", fb::DataType_Utf8},      {"c_acctbal", fb::DataType_Decimal128},
      {"c_mktsegment", fb::DataType_Utf8}, {"c_comment", fb::DataType_Utf8},
  };
  std::vector<flatbuffers::Offset<fb::Field>> offsets;
  for (auto& [name, type] : fields)
    offsets.push_back(fb::CreateField(fbb, fbb.CreateString(name), type, /*nullable=*/true));
  return fb::CreateSchema(fbb, fbb.CreateVector(offsets));
}

flatbuffers::Offset<fb::PlanNode> customer_scan(flatbuffers::FlatBufferBuilder& fbb,
                                                const std::vector<uint32_t>& projection) {
  auto path = fbb.CreateString(customer_path());
  auto paths = fbb.CreateVector(std::vector<flatbuffers::Offset<flatbuffers::String>>{path});
  auto scan = fb::CreateCudfScan(fbb, paths, customer_schema(fbb), fbb.CreateVector(projection));
  return fb::CreatePlanNode(fbb, fb::PlanNodeKind_CudfScan, scan.Union());
}

std::vector<uint8_t> finish_plan(flatbuffers::FlatBufferBuilder& fbb,
                                 flatbuffers::Offset<fb::PlanNode> root) {
  fbb.Finish(fb::CreateGpuPlan(fbb, root));
  auto* ptr = fbb.GetBufferPointer();
  return {ptr, ptr + fbb.GetSize()};
}

/// customer projected to `projection`, scanned alone.
std::vector<uint8_t> customer_scan_plan(flatbuffers::FlatBufferBuilder& fbb,
                                        const std::vector<uint32_t>& projection) {
  return finish_plan(fbb, customer_scan(fbb, projection));
}

/// customer's (c_custkey, c_name) under a hash repartition on c_custkey into `lanes`, so a
/// handle can come from seq 1 rather than the seq-0 default.
std::vector<uint8_t> customer_scatter_plan(flatbuffers::FlatBufferBuilder& fbb, uint32_t lanes) {
  auto scan = customer_scan(fbb, {0, 1});
  auto name = fbb.CreateString("c_custkey");
  auto key = fb::CreateExpr(fbb, fb::ExprNode_ColumnRef, fb::CreateColumnRef(fbb, 0, name).Union());
  auto keys = fbb.CreateVector(std::vector<flatbuffers::Offset<fb::Expr>>{key});
  auto rp = fb::CreateCudfRepartition(fbb, fb::PartitioningKind_Hash, lanes, keys, scan);
  return finish_plan(fbb, fb::CreatePlanNode(fbb, fb::PlanNodeKind_CudfRepartition, rp.Union()));
}

/// The C ABI over one loaded plan, released in the order the header requires.
class CApiPlan {
 public:
  explicit CApiPlan(const std::vector<uint8_t>& plan) {
    EXPECT_EQ(peacock_executor_create(/*gpu_memory_limit=*/0, &ex_), 0);
    uint64_t nodes = 0;
    EXPECT_EQ(peacock_executor_begin_plan(ex_, plan.data(), plan.size(), &nodes), 0);
  }
  ~CApiPlan() {
    peacock_executor_end_plan(ex_);
    peacock_executor_destroy(ex_);
  }
  peacock_executor_t* get() { return ex_; }
  std::string last_error() { return peacock_last_error(ex_); }

 private:
  peacock_executor_t* ex_ = nullptr;
};

/// A timing loan, so a failing expectation cannot leave the process-global switch on for
/// every later test in this binary.
struct TimingOn {
  TimingOn() { peacock::set_node_timing(peacock::NodeTiming::Events); }
  ~TimingOn() { peacock::set_node_timing(peacock::NodeTiming::Off); }
};

/// What `fn` allocated in total, from the statistics adaptor main() installs.
template <class F>
int64_t allocated_by(F&& fn) {
  auto& mr = peacock::stats_mr();
  if (!mr) {
    ADD_FAILURE() << "main() installs the pool and its statistics adaptor";
    return 0;
  }
  mr->push_counters();
  fn();
  auto [bytes, calls] = mr->pop_counters();
  return bytes.total;
}

/// `rows` of (int64, string) uploaded through the harness's symbol, and the batch they came
/// from, which an export of any handle over them must read back as.
std::pair<uint64_t, std::shared_ptr<arrow::RecordBatch>> upload_keys_and_names(CApiPlan& plan,
                                                                               int64_t rows) {
  arrow::Int64Builder keys;
  arrow::StringBuilder names;
  for (int64_t i = 0; i < rows; ++i) {
    EXPECT_TRUE(keys.Append(i * 7).ok());
    EXPECT_TRUE(names.Append("name " + std::to_string(i)).ok());
  }
  auto batch = arrow::RecordBatch::Make(
      arrow::schema({arrow::field("k", arrow::int64()), arrow::field("n", arrow::utf8())}), rows,
      {keys.Finish().ValueOrDie(), names.Finish().ValueOrDie()});
  auto strukt = batch->ToStructArray().ValueOrDie();
  ArrowArray c_array;
  ArrowSchema c_schema;
  EXPECT_TRUE(arrow::ExportArray(*strukt, &c_array, &c_schema).ok());
  uint64_t handle = 0;
  EXPECT_EQ(peacock_handle_from_arrow(plan.get(), &c_schema, &c_array, &handle), 0)
      << plan.last_error();
  // The upload copies to the device and borrows neither struct.
  c_array.release(&c_array);
  c_schema.release(&c_schema);
  return {handle, batch};
}

/// Every row of `batch` as a `k|n` line, whatever string width the export chose.
std::vector<std::string> lines_of(const arrow::RecordBatch& batch) {
  std::vector<std::string> out;
  for (int64_t r = 0; r < batch.num_rows(); ++r) {
    std::string line;
    for (int c = 0; c < batch.num_columns(); ++c)
      line += (c ? "|" : "") + batch.column(c)->GetScalar(r).ValueOrDie()->ToString();
    out.push_back(line);
  }
  return out;
}

/// What `peacock_result_from_handle` reads back from `handle`, as `lines_of` writes it.
std::vector<std::string> exported_lines(CApiPlan& plan, uint64_t handle) {
  uint8_t* ipc = nullptr;
  uint64_t len = 0;
  EXPECT_EQ(peacock_result_from_handle(plan.get(), handle, 0, UINT64_MAX, nullptr, 0, &ipc, &len),
            0)
      << plan.last_error();
  std::vector<std::string> out;
  if (len == 0) return out;
  {
    auto input = std::make_shared<arrow::io::BufferReader>(
        std::make_shared<arrow::Buffer>(ipc, static_cast<int64_t>(len)));
    auto reader = arrow::ipc::RecordBatchStreamReader::Open(input).ValueOrDie();
    std::shared_ptr<arrow::RecordBatch> batch;
    while (reader->ReadNext(&batch).ok() && batch) {
      auto lines = lines_of(*batch);
      out.insert(out.end(), lines.begin(), lines.end());
    }
  }
  peacock_result_free(ipc);
  return out;
}

TEST(HandleDuplicate, SharesOwnersAndAllocatesNothing) {
  flatbuffers::FlatBufferBuilder fbb;
  CApiPlan plan(customer_scan_plan(fbb, {0}));
  auto [original, batch] = upload_keys_and_names(plan, 1000);

  uint64_t duplicate = 0;
  auto total = allocated_by([&] {
    ASSERT_EQ(peacock_handle_duplicate(plan.get(), original, &duplicate), 0) << plan.last_error();
  });
  EXPECT_EQ(total, 0) << "the duplicate asked the allocator for bytes";
  EXPECT_NE(duplicate, original);

  // The original is gone and its columns are not: the duplicate owns them as much as it did.
  peacock_handle_release(plan.get(), original);
  EXPECT_EQ(exported_lines(plan, duplicate), lines_of(*batch));
}

TEST(HandleDuplicate, EachHandleIsReleasedOnItsOwn) {
  flatbuffers::FlatBufferBuilder fbb;
  CApiPlan plan(customer_scan_plan(fbb, {0}));
  auto [original, batch] = upload_keys_and_names(plan, 1000);
  uint64_t duplicate = 0;
  ASSERT_EQ(peacock_handle_duplicate(plan.get(), original, &duplicate), 0) << plan.last_error();

  peacock_handle_release(plan.get(), duplicate);
  EXPECT_EQ(exported_lines(plan, original), lines_of(*batch));
}

TEST(HandleDuplicate, IsChargedToTheOriginalsProducer) {
  TimingOn timing;
  flatbuffers::FlatBufferBuilder fbb;
  // A scatter, so the handle duplicated comes from seq 1 and not from the seq-0 default.
  CApiPlan plan(customer_scatter_plan(fbb, 4));
  std::vector<uint32_t> groups{1};
  uint64_t scanned = 0;
  ASSERT_EQ(peacock_executor_execute_scan_rowgroups(plan.get(), 0, groups.data(), groups.size(),
                                                    &scanned, nullptr),
            0)
      << plan.last_error();
  uint64_t counts[1] = {1};
  uint64_t parts[4] = {};
  PeacockNodeStats stats[4] = {};
  uint64_t produced = 0;
  ASSERT_EQ(
      peacock_executor_execute_node(plan.get(), 1, &scanned, counts, 1, parts, 4, &produced, stats),
      0)
      << plan.last_error();
  ASSERT_EQ(produced, 4u);

  uint64_t duplicate = 0;
  ASSERT_EQ(peacock_handle_duplicate(plan.get(), parts[2], &duplicate), 0) << plan.last_error();
  peacock_handle_release(plan.get(), parts[2]);
  exported_lines(plan, duplicate);

  // The scan, the scatter's four partitions, and the export: the duplicate is no call of
  // its own, and the export of its rows is the scatter's, as an export of the original is.
  uint64_t count = 0;
  std::vector<PeacockNodeRegion> regions(8);
  ASSERT_EQ(
      peacock_executor_collect_node_regions(plan.get(), regions.data(), regions.size(), &count), 0)
      << plan.last_error();
  ASSERT_EQ(count, 6u);
  EXPECT_EQ(regions[5].seq, 1u);
  EXPECT_EQ(regions[5].call_index, 1u);
}

TEST(HandleDuplicate, AnUnknownHandleIsRefused) {
  flatbuffers::FlatBufferBuilder fbb;
  CApiPlan plan(customer_scan_plan(fbb, {0}));

  uint64_t out = 0;
  EXPECT_NE(peacock_handle_duplicate(plan.get(), /*handle=*/999, &out), 0);
  EXPECT_NE(plan.last_error().find("999"), std::string::npos) << plan.last_error();

  // As slice_handle does: a handle the driver thinks it holds and C++ does not is a
  // broken query, and the session goes with every handle it held.
  std::vector<uint32_t> groups{1};
  uint64_t handle = 0;
  EXPECT_NE(peacock_executor_execute_scan_rowgroups(plan.get(), 0, groups.data(), groups.size(),
                                                    &handle, nullptr),
            0);
  EXPECT_NE(plan.last_error().find("no plan loaded"), std::string::npos) << plan.last_error();
}

}  // namespace
