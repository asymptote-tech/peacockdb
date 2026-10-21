// A date or timestamp column ± an interval literal: the column path's one arm for an
// interval operand (expr.cpp, `build_column_binary`). cuDF has no interval type, so the
// literal's three parts are applied one call each.

#include "peacock/expr.h"

#include <cudf/binaryop.hpp>
#include <cudf/column/column_factories.hpp>
#include <cudf/datetime.hpp>
#include <cudf/scalar/scalar.hpp>
#include <cudf/scalar/scalar_factories.hpp>
#include <cudf/utilities/type_dispatcher.hpp>
#include <cudf/wrappers/durations.hpp>

#include <stdexcept>
#include <string>

namespace peacock {

namespace {

constexpr int64_t kDayNanos = 86'400'000'000'000;

/// `count` of the timestamp type's own unit, as a duration cuDF adds to it.
std::unique_ptr<cudf::scalar> duration_in_unit_of(cudf::data_type ts, int64_t count) {
  switch (ts.id()) {
    case cudf::type_id::TIMESTAMP_DAYS:
      return std::make_unique<cudf::duration_scalar<cudf::duration_D>>(static_cast<int32_t>(count),
                                                                       true);
    case cudf::type_id::TIMESTAMP_SECONDS:
      return std::make_unique<cudf::duration_scalar<cudf::duration_s>>(count, true);
    case cudf::type_id::TIMESTAMP_MILLISECONDS:
      return std::make_unique<cudf::duration_scalar<cudf::duration_ms>>(count, true);
    case cudf::type_id::TIMESTAMP_MICROSECONDS:
      return std::make_unique<cudf::duration_scalar<cudf::duration_us>>(count, true);
    case cudf::type_id::TIMESTAMP_NANOSECONDS:
      return std::make_unique<cudf::duration_scalar<cudf::duration_ns>>(count, true);
    default:
      throw std::runtime_error("an interval shifts a date or a timestamp, not " +
                               cudf::type_to_name(ts) + " (#249)");
  }
}

/// Nanoseconds in one unit of the timestamp type.
int64_t unit_nanos(cudf::data_type ts) {
  switch (ts.id()) {
    case cudf::type_id::TIMESTAMP_DAYS:
      return kDayNanos;
    case cudf::type_id::TIMESTAMP_SECONDS:
      return 1'000'000'000;
    case cudf::type_id::TIMESTAMP_MILLISECONDS:
      return 1'000'000;
    case cudf::type_id::TIMESTAMP_MICROSECONDS:
      return 1'000;
    default:
      return 1;
  }
}

/// `ts` shifted as arrow's `add_month_day_nano` shifts it, by parts already signed for `+` or
/// `-`: months, a day past the month's end clamping to its last day, then days, then
/// nanoseconds. The nanoseconds floor to the column's unit, as arrow converts them, except on
/// a date, where chrono counts whole days toward zero.
std::unique_ptr<cudf::column> add_interval(cudf::column_view ts, int64_t months, int64_t days,
                                           int64_t nanos) {
  auto type = ts.type();
  bool is_date = type.id() == cudf::type_id::TIMESTAMP_DAYS;
  int64_t unit = unit_nanos(type);
  // C++ division truncates toward zero, which is chrono's rule on a date; a timestamp floors.
  int64_t sub_unit = nanos / unit - (!is_date && nanos % unit != 0 && nanos < 0);

  std::unique_ptr<cudf::column> out;
  auto current = [&] { return out ? out->view() : ts; };
  if (months != 0)
    out = cudf::datetime::add_calendrical_months(
        current(), cudf::numeric_scalar<int32_t>(static_cast<int32_t>(months)));
  for (int64_t count : {days * (kDayNanos / unit), sub_unit}) {
    if (count == 0) continue;
    out = cudf::binary_operation(current(), *duration_in_unit_of(type, count),
                                 cudf::binary_operator::ADD, type);
  }
  return out ? std::move(out) : std::make_unique<cudf::column>(ts);
}

}  // namespace

bool is_interval_literal(const fb::Expr* expr) {
  if (expr->node_type() != fb::ExprNode_LiteralExpr) return false;
  auto* sv = expr->node_as_LiteralExpr()->value();
  return sv && sv->type() == fb::DataType_IntervalMonthDayNano;
}

std::unique_ptr<cudf::column> shift_by_interval(const fb::BinaryExprNode* bin,
                                                cudf::table_view const& table,
                                                std::vector<std::string> const& names) {
  bool on_left = is_interval_literal(bin->left()), on_right = is_interval_literal(bin->right());
  bool shifts = on_left != on_right &&
                (bin->op() == fb::BinaryOp_Plus || (bin->op() == fb::BinaryOp_Minus && on_right));
  if (!shifts)
    throw std::runtime_error("interval arithmetic other than date/timestamp ± literal (#249)");
  // The timestamp side gives the answer's type, whichever side it is on.
  auto ts = evaluate_column(on_right ? bin->left() : bin->right(), table, names);
  auto* interval = (on_right ? bin->right() : bin->left())->node_as_LiteralExpr()->value();
  if (!literal_is_valid(interval)) {
    auto null = cudf::make_timestamp_scalar(ts.view().type());
    null->set_valid_async(false);
    return cudf::make_column_from_scalar(*null, ts.view().size());
  }
  int64_t sign = bin->op() == fb::BinaryOp_Minus ? -1 : 1;
  return add_interval(ts.view(), sign * interval->interval_months(),
                      sign * interval->interval_days(), sign * interval->interval_nanos());
}

}  // namespace peacock
