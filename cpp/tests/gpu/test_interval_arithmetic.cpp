/// A date or timestamp column ± an interval literal, on the column path (`shift_by_interval`,
/// interval.cpp). Every expected value is DataFusion 45's answer to the SQL in the comment beside
/// it: arrow's `add_month_day_nano` is the rule, applied months, then days, then nanoseconds.

#include "hand_tables.hpp"

#include <cudf/wrappers/timestamps.hpp>

#include <gtest/gtest.h>

#include <cstdint>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

namespace {
namespace fb = peacock::plan;

flatbuffers::Offset<fb::Expr> interval(flatbuffers::FlatBufferBuilder& fbb, int32_t months,
                                       int32_t days, int64_t nanos) {
  fb::ScalarValueBuilder sb(fbb);
  sb.add_type(fb::DataType_IntervalMonthDayNano);
  sb.add_interval_months(months);
  sb.add_interval_days(days);
  sb.add_interval_nanos(nanos);
  auto value = sb.Finish();
  return fb::CreateExpr(fbb, fb::ExprNode_LiteralExpr, fb::CreateLiteralExpr(fbb, value).Union());
}

constexpr int64_t kHour = 3'600'000'000'000;

/// One column `t` run through a project of `t <op> iv`, or `iv <op> t` where `left` is set.
/// `iv` is built by the caller into the same builder, so a NULL interval goes through too.
template <typename MakeInterval>
peacock::TableResult shifted(std::unique_ptr<cudf::column> column, fb::BinaryOp op,
                             MakeInterval make_interval, bool interval_left = false) {
  flatbuffers::FlatBufferBuilder fbb;
  auto iv = make_interval(fbb);
  auto col = hand::ref(fbb, 0, "t");
  auto binary = interval_left ? fb::CreateBinaryExprNode(fbb, iv, op, col)
                              : fb::CreateBinaryExprNode(fbb, col, op, iv);
  auto expr = fb::CreateExpr(fbb, fb::ExprNode_BinaryExprNode, binary.Union());
  auto exprs = fbb.CreateVector(std::vector<flatbuffers::Offset<fb::Expr>>{expr});
  auto aliases = fbb.CreateVector(
      std::vector<flatbuffers::Offset<flatbuffers::String>>{fbb.CreateString("shifted")});
  auto project = fb::CreateCudfProject(fbb, exprs, aliases);
  auto node = fb::CreatePlanNode(fbb, fb::PlanNodeKind_CudfProject, project.Union());
  std::vector<std::unique_ptr<cudf::column>> columns;
  columns.push_back(std::move(column));
  std::vector<peacock::TableResult> in;
  in.push_back(hand::table_of(std::move(columns), {"t"}));
  return hand::run(fbb, node, std::move(in));
}

auto by(int32_t months, int32_t days, int64_t nanos) {
  return [=](flatbuffers::FlatBufferBuilder& fbb) { return interval(fbb, months, days, nanos); };
}

template <typename Stamp>
std::unique_ptr<cudf::column> stamps(std::vector<int64_t> const& counts) {
  std::vector<Stamp> values;
  for (auto c : counts) values.push_back(Stamp{typename Stamp::duration{c}});
  return hand::column_of(values);
}

template <typename Stamp>
std::vector<int64_t> counts_of(cudf::column_view const& column) {
  std::vector<int64_t> out;
  for (auto v : hand::values_of<Stamp>(column)) out.push_back(v.time_since_epoch().count());
  return out;
}

// Days since the epoch.
constexpr int32_t k2024_01_31 = 19753, k2023_01_31 = 19388, k2024_03_31 = 19813;
constexpr int32_t k2024_02_29 = 19782, k2023_02_28 = 19416, k2024_04_30 = 19843;
constexpr int32_t k2023_12_31 = 19722, k2022_12_31 = 19357;
constexpr int32_t k2024_03_09 = 19791, k2024_03_10 = 19792, k2024_03_11 = 19793;
}  // namespace

// SELECT d + INTERVAL '1 month', d - INTERVAL '1 month' over 2024-01-31, 2023-01-31,
// 2024-03-31 and NULL: 2024-02-29, 2023-02-28, 2024-04-30, NULL; 2023-12-31, 2022-12-31,
// 2024-02-29, NULL.
TEST(IntervalArithmetic, MonthsClampAtMonthEnd) {
  using D = cudf::timestamp_D;
  auto dates = [] {
    std::vector<D> v{D{cudf::duration_D{k2024_01_31}}, D{cudf::duration_D{k2023_01_31}},
                     D{cudf::duration_D{k2024_03_31}}, D{cudf::duration_D{0}}};
    return hand::nullable_column_of(v, {true, true, true, false});
  };
  for (auto [op, want] :
       {std::pair{fb::BinaryOp_Plus, std::vector<int64_t>{k2024_02_29, k2023_02_28, k2024_04_30}},
        std::pair{fb::BinaryOp_Minus,
                  std::vector<int64_t>{k2023_12_31, k2022_12_31, k2024_02_29}}}) {
    auto out = shifted(dates(), op, by(1, 0, 0));
    auto got = out.view().column(0);
    ASSERT_EQ(got.type().id(), cudf::type_id::TIMESTAMP_DAYS);
    ASSERT_EQ(got.size(), 4);
    EXPECT_EQ(got.null_count(), 1) << "a NULL date stays NULL through every step";
    auto counts = counts_of<D>(got);
    EXPECT_EQ(std::vector<int64_t>(counts.begin(), counts.begin() + 3), want);
  }
}

// SELECT arrow_cast(TIMESTAMP '2024-01-30 23:00:00.000000123', 'Timestamp(<unit>, None)')
//   + INTERVAL '1 month 2 days 3 hours'
// is 2024-03-03T02:00:00 at every unit, and keeps .000000123 at nanoseconds. Days first, or
// the hours first, would land on March 2.
template <typename Stamp>
void expect_mixed(int64_t from, int64_t want, cudf::type_id unit) {
  auto out = shifted(stamps<Stamp>({from}), fb::BinaryOp_Plus, by(1, 2, 3 * kHour));
  auto got = out.view().column(0);
  EXPECT_EQ(got.type().id(), unit);
  EXPECT_EQ(counts_of<Stamp>(got), std::vector<int64_t>{want}) << cudf::type_to_name(got.type());
}

TEST(IntervalArithmetic, MixedOnEachTimestampUnit) {
  expect_mixed<cudf::timestamp_s>(1706655600, 1709431200, cudf::type_id::TIMESTAMP_SECONDS);
  expect_mixed<cudf::timestamp_ms>(1706655600000, 1709431200000,
                                   cudf::type_id::TIMESTAMP_MILLISECONDS);
  expect_mixed<cudf::timestamp_us>(1706655600000000, 1709431200000000,
                                   cudf::type_id::TIMESTAMP_MICROSECONDS);
  expect_mixed<cudf::timestamp_ns>(1706655600000000123, 1709431200000000123,
                                   cudf::type_id::TIMESTAMP_NANOSECONDS);
}

// SELECT DATE '2024-03-10' + INTERVAL '36 hours', + '12 hours', - '12 hours', - '36 hours':
// 2024-03-11, 2024-03-10, 2024-03-10, 2024-03-09. chrono counts whole days toward zero.
TEST(IntervalArithmetic, SubDayOnADateTruncates) {
  struct Case {
    fb::BinaryOp op;
    int64_t hours;
    int64_t want;
  };
  for (auto c :
       {Case{fb::BinaryOp_Plus, 36, k2024_03_11}, Case{fb::BinaryOp_Plus, 12, k2024_03_10},
        Case{fb::BinaryOp_Minus, 12, k2024_03_10}, Case{fb::BinaryOp_Minus, 36, k2024_03_09}}) {
    auto out = shifted(stamps<cudf::timestamp_D>({k2024_03_10}), c.op, by(0, 0, c.hours * kHour));
    auto got = out.view().column(0);
    EXPECT_EQ(got.type().id(), cudf::type_id::TIMESTAMP_DAYS);
    EXPECT_EQ(counts_of<cudf::timestamp_D>(got), std::vector<int64_t>{c.want})
        << (c.op == fb::BinaryOp_Plus ? "+ " : "- ") << c.hours << " hours";
  }
}

// SELECT arrow_cast(TIMESTAMP '2024-01-01 00:00:01', 'Timestamp(Millisecond, None)')
//   ± INTERVAL '0.0000015 seconds'
// is 1704067201000 for +, and 1704067200999 for -: the 1.5 microseconds floor to the unit.
TEST(IntervalArithmetic, SubUnitOnATimestampFloors) {
  for (auto [op, want] : {std::pair{fb::BinaryOp_Plus, int64_t{1704067201000}},
                          std::pair{fb::BinaryOp_Minus, int64_t{1704067200999}}}) {
    auto out = shifted(stamps<cudf::timestamp_ms>({1704067201000}), op, by(0, 0, 1500));
    EXPECT_EQ(counts_of<cudf::timestamp_ms>(out.view().column(0)), std::vector<int64_t>{want});
  }
}

// SELECT INTERVAL '1 day' + DATE '2024-03-10' is 2024-03-11, a Date32.
TEST(IntervalArithmetic, IntervalOnTheLeft) {
  auto out = shifted(stamps<cudf::timestamp_D>({k2024_03_10}), fb::BinaryOp_Plus, by(0, 1, 0),
                     /*interval_left=*/true);
  auto got = out.view().column(0);
  EXPECT_EQ(got.type().id(), cudf::type_id::TIMESTAMP_DAYS);
  EXPECT_EQ(counts_of<cudf::timestamp_D>(got), std::vector<int64_t>{k2024_03_11});
}

// SELECT DATE '2024-03-10' + CAST(NULL AS INTERVAL) is NULL, a Date32.
TEST(IntervalArithmetic, ANullIntervalAnswersNulls) {
  auto null_interval = [](flatbuffers::FlatBufferBuilder& fbb) {
    return hand::null_of(fbb, fb::DataType_IntervalMonthDayNano);
  };
  auto out = shifted(stamps<cudf::timestamp_D>({k2024_03_10, k2024_03_11}), fb::BinaryOp_Plus,
                     null_interval);
  auto got = out.view().column(0);
  EXPECT_EQ(got.type().id(), cudf::type_id::TIMESTAMP_DAYS);
  EXPECT_EQ(got.size(), 2);
  EXPECT_EQ(got.null_count(), 2);
}

// A hoisted keyless residual learns its operand's type from a zero-row table.
TEST(IntervalArithmetic, ZeroRowsKeepTheType) {
  auto out = shifted(stamps<cudf::timestamp_D>({}), fb::BinaryOp_Plus, by(1, 2, 3 * kHour));
  auto got = out.view().column(0);
  EXPECT_EQ(got.type().id(), cudf::type_id::TIMESTAMP_DAYS);
  EXPECT_EQ(got.size(), 0);
}

// The planner refuses every other use of an interval (#249); the device says the same.
TEST(IntervalArithmetic, AnyOtherUseIsRefusedNamingTheTicket) {
  try {
    shifted(stamps<cudf::timestamp_D>({k2024_03_10}), fb::BinaryOp_Minus, by(0, 1, 0),
            /*interval_left=*/true);
    FAIL() << "interval - date was answered";
  } catch (std::runtime_error const& e) {
    EXPECT_NE(std::string(e.what()).find("(#249)"), std::string::npos) << e.what();
  }
}
