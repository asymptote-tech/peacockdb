#include <cudf/aggregation.hpp>
#include <cudf/filling.hpp>
#include <cudf/io/parquet.hpp>
#include <cudf/reduction.hpp>
#include <cudf/scalar/scalar.hpp>
#include <cudf/scalar/scalar_factories.hpp>
#include <cudf/types.hpp>

#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <string>

#include <gtest/gtest.h>

#include <peacock_gpu.h>
#include "plan_executor.h"
#include "peacock/pinned_host_cache.h"
#include "peacock/rmm_pool.hpp"

// What a benchmark's provenance reads: a named device, and the runtime's version as
// 1000 * major + 10 * minor. The null refusal is asserted here, not on the cpu tier: with no
// device the runtime refuses first, so a missing check passes there.
TEST(PeacockGpu, DeviceInfoDescribesTheDevice) {
  EXPECT_NE(peacock_device_info(nullptr), 0);
  PeacockDeviceInfo info{};
  ASSERT_EQ(peacock_device_info(&info), 0);
  const std::size_t length = strnlen(info.name, sizeof info.name);
  EXPECT_GT(length, 0u);
  EXPECT_LT(length, sizeof info.name);
  EXPECT_GE(info.runtime, 11000);
}

TEST(CudfGpu, SequenceSum) {
  // Generate [1, 2, 3, ..., 100] on the GPU.
  constexpr cudf::size_type N = 100;
  auto init = cudf::make_fixed_width_scalar<int64_t>(1);
  auto step = cudf::make_fixed_width_scalar<int64_t>(1);
  auto col  = cudf::sequence(N, *init, *step);

  ASSERT_EQ(col->size(), N);
  ASSERT_EQ(col->type().id(), cudf::type_id::INT64);

  // Sum on the GPU; expected = N*(N+1)/2
  auto agg    = cudf::make_sum_aggregation<cudf::reduce_aggregation>();
  auto result = cudf::reduce(col->view(), *agg, cudf::data_type{cudf::type_id::INT64});

  auto* scalar = dynamic_cast<cudf::numeric_scalar<int64_t>*>(result.get());
  ASSERT_NE(scalar, nullptr);
  ASSERT_TRUE(scalar->is_valid());
  EXPECT_EQ(scalar->value(), static_cast<int64_t>(N) * (N + 1) / 2);
}

// A hundred-row sequence and no dataset at all; measured peak 912 bytes
// (llm-wiki/tasks/rmm-pool-budget-detail.md). 1 GiB is a floor rather than a measurement:
// nothing here can approach it, and it leaves the timing-floor tests room.
constexpr std::size_t kPoolBytes = 1ull << 30;

// The one place the tree checks that a pool is the budget its binary declared, rather than a
// share of what the device had free. That is what #178 was, it is invisible in a passing run,
// and this binary is in every gpu-tests job. install_rmm_pool is idempotent, so this reads
// main()'s installation back rather than making a second one.
TEST(RmmPool, ReservesTheDeclaredBudget) {
  const peacock::RmmPoolStatus& status = peacock::install_rmm_pool(kPoolBytes);
  ASSERT_EQ(status.state, peacock::RmmPoolStatus::State::Installed);
  EXPECT_EQ(status.initial_bytes, peacock::pool_align_down(peacock::pool_budget_bytes(kPoolBytes)));
  EXPECT_EQ(status.maximum_bytes, status.initial_bytes);
}

TEST(PinnedHostCache, CudaAllocatorPinsHostMemory) {
  constexpr std::size_t kBytes = 1 << 20;
  const peacock::HostAllocator pinned = peacock::cuda_pinned_allocator();
  void* p = pinned.allocate(kBytes);
  ASSERT_NE(p, nullptr);
  cudaPointerAttributes attributes{};
  ASSERT_EQ(cudaPointerGetAttributes(&attributes, p), cudaSuccess);
  EXPECT_EQ(attributes.type, cudaMemoryTypeHost);
  pinned.deallocate(p, kBytes);
}

// A refused cudaHostAlloc leaves the runtime's last error set, and cuDF's next
// cudaPeekAtLastError check would throw it from an unrelated call.
TEST(PinnedHostCache, RefusedAllocationClearsTheCudaError) {
  EXPECT_EQ(peacock::cuda_pinned_allocator().allocate(1ull << 50), nullptr);
  EXPECT_EQ(cudaPeekAtLastError(), cudaSuccess);
  const char* env = std::getenv("PEACOCK_TESTDATA_DIR");
  const std::string customer =
      std::string(env ? env : PEACOCK_TESTDATA_DIR) + "/tpch.minimal/customer.parquet";
  auto options = cudf::io::parquet_reader_options::builder(cudf::io::source_info{customer}).build();
  EXPECT_GT(cudf::io::read_parquet(options).tbl->num_rows(), 0);
}

int main(int argc, char** argv) {
  ::testing::InitGoogleTest(&argc, argv);
  peacock::install_rmm_pool(kPoolBytes);
  return RUN_ALL_TESTS();
}
