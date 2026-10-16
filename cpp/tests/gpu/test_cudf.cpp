#include <cudf/aggregation.hpp>
#include <cudf/filling.hpp>
#include <cudf/reduction.hpp>
#include <cudf/scalar/scalar.hpp>
#include <cudf/scalar/scalar_factories.hpp>
#include <cudf/types.hpp>

#include <cstdint>

#include <gtest/gtest.h>

#include "plan_executor.h"
#include "peacock/rmm_pool.hpp"

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

int main(int argc, char** argv) {
  ::testing::InitGoogleTest(&argc, argv);
  peacock::install_rmm_pool(kPoolBytes);
  return RUN_ALL_TESTS();
}
