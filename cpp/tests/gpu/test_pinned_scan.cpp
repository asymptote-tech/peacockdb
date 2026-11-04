/// The scan through the process's pinned host cache: a read from the pinned bytes equals the
/// read from the path, and a prefilled file is read with no fill. Both skip when
/// PEACOCK_PINNED_HOST_CACHE_BYTES is set, since that fixes the capacity these cases move.

#include "peacock_gpu.h"
#include "plan_executor.h"
#include "generated/gpu_plan_generated.h"

#include <cudf/aggregation.hpp>
#include <cudf/binaryop.hpp>
#include <cudf/reduction.hpp>
#include <cudf/scalar/scalar.hpp>
#include <cudf/table/table_view.hpp>

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

/// One CudfScan over customer.parquet: every row group, four columns of four kinds.
std::vector<uint8_t> customer_scan() {
  flatbuffers::FlatBufferBuilder fbb;
  auto paths = fbb.CreateVector(
      std::vector<flatbuffers::Offset<flatbuffers::String>>{fbb.CreateString(customer_path())});
  std::vector<flatbuffers::Offset<fb::Field>> fields;
  for (auto [name, type] :
       {std::pair{"c_custkey", fb::DataType_Int64}, std::pair{"c_name", fb::DataType_Utf8},
        std::pair{"c_nationkey", fb::DataType_Int32},
        std::pair{"c_acctbal", fb::DataType_Decimal128}})
    fields.push_back(fb::CreateField(fbb, fbb.CreateString(name), type, /*nullable=*/true));
  auto schema = fb::CreateSchema(fbb, fbb.CreateVector(fields));
  auto scan = fb::CreateCudfScan(fbb, paths, schema);
  auto root = fb::CreatePlanNode(fbb, fb::PlanNodeKind_CudfScan, scan.Union());
  fbb.Finish(fb::CreateGpuPlan(fbb, root));
  return {fbb.GetBufferPointer(), fbb.GetBufferPointer() + fbb.GetSize()};
}

/// The scan executed in a session of its own, which keeps the table alive.
struct Scanned {
  std::vector<uint8_t> plan = customer_scan();
  peacock::NodeSession session{plan.data(), plan.size()};
  uint64_t handle = 0;

  Scanned() {
    std::size_t count = 0;
    peacock::NodeStats stats{};
    session.execute_node(0, nullptr, nullptr, 0, &handle, /*out_cap=*/1, &count, &stats);
    EXPECT_EQ(count, 1u);
  }
  peacock::TableResult const& table() const { return session.table_for(handle); }
};

void expect_equal(peacock::TableResult const& a, peacock::TableResult const& b) {
  ASSERT_EQ(a.column_names, b.column_names);
  ASSERT_EQ(a.num_rows(), b.num_rows());
  ASSERT_GT(a.num_rows(), 0);
  auto all = cudf::make_all_aggregation<cudf::reduce_aggregation>();
  const cudf::data_type bool8{cudf::type_id::BOOL8};
  for (cudf::size_type i = 0; i < a.view().num_columns(); ++i) {
    auto same = cudf::binary_operation(a.view().column(i), b.view().column(i),
                                       cudf::binary_operator::NULL_EQUALS, bool8);
    auto every = cudf::reduce(same->view(), *all, bool8);
    EXPECT_TRUE(static_cast<cudf::numeric_scalar<bool>&>(*every).value())
        << a.column_names[i] << " differs between the two sources";
  }
}

PeacockPinnedCacheStats stats_now() {
  PeacockPinnedCacheStats stats{};
  EXPECT_EQ(peacock_pinned_host_cache_stats(&stats), 0);
  return stats;
}

PeacockPinnedCacheStats set_capacity(uint64_t bytes) {
  PeacockPinnedCacheStats stats{};
  EXPECT_EQ(peacock_set_pinned_host_cache_bytes(bytes, &stats), 0);
  return stats;
}

bool capacity_fixed_by_env() { return std::getenv("PEACOCK_PINNED_HOST_CACHE_BYTES") != nullptr; }

}  // namespace

TEST(PinnedHostCache, ScanFromTheCacheEqualsThePath) {
  if (capacity_fixed_by_env()) GTEST_SKIP() << "PEACOCK_PINNED_HOST_CACHE_BYTES is set";
  ASSERT_EQ(set_capacity(PEACOCK_PINNED_CACHE_DEFAULT_BYTES).state, PEACOCK_PINNED_CACHE_ON);
  Scanned pinned;
  EXPECT_GE(stats_now().files, 1u) << "the scan left nothing resident";
  ASSERT_EQ(set_capacity(0).state, PEACOCK_PINNED_CACHE_OFF);
  const auto fallbacks = stats_now().fallbacks;
  Scanned path;
  EXPECT_EQ(stats_now().fallbacks, fallbacks) << "a cache that is off is no fallback";
  expect_equal(pinned.table(), path.table());
  set_capacity(PEACOCK_PINNED_CACHE_DEFAULT_BYTES);
}

TEST(PinnedHostCache, PrefilledScanFillsNothing) {
  if (capacity_fixed_by_env()) GTEST_SKIP() << "PEACOCK_PINNED_HOST_CACHE_BYTES is set";
  ASSERT_EQ(set_capacity(PEACOCK_PINNED_CACHE_DEFAULT_BYTES).state, PEACOCK_PINNED_CACHE_ON);
  peacock_executor_t* executor = nullptr;
  ASSERT_EQ(peacock_executor_create(0, &executor), 0);
  std::unique_ptr<peacock_executor_t, void (*)(peacock_executor_t*)> owner(
      executor, peacock_executor_destroy);
  const std::string customer = customer_path();
  const char* paths[] = {customer.c_str()};
  peacock_prefill_stats_t prefill{};
  ASSERT_EQ(peacock_pinned_cache_prefill(executor, paths, 1, &prefill), PEACOCK_PREFILL_OK)
      << peacock_last_error(executor);
  EXPECT_GT(prefill.needed, 0u);

  const auto before = stats_now();
  std::vector<std::unique_ptr<Scanned>> runs;
  for (int i = 0; i < 3; ++i) runs.push_back(std::make_unique<Scanned>());
  const auto after = stats_now();
  EXPECT_EQ(after.fills, before.fills);
  EXPECT_EQ(after.fallbacks, before.fallbacks);
  EXPECT_EQ(after.reloads, before.reloads);

  ASSERT_EQ(set_capacity(0).state, PEACOCK_PINNED_CACHE_OFF);
  Scanned path;
  expect_equal(runs.front()->table(), path.table());
  set_capacity(PEACOCK_PINNED_CACHE_DEFAULT_BYTES);
}
