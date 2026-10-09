// CudfFilter -- apply a boolean predicate.

#include "peacock/operators.h"
#include "peacock/expr.h"

#include <cudf/stream_compaction.hpp>
#include <cudf/transform.hpp>
#include <cudf/table/table.hpp>

#include <algorithm>
#include <stdexcept>
#include <string>

namespace peacock {

TableResult execute_filter(const fb::CudfFilter* filter, NodeInputs* in) {
  auto input = take_input(in);

  // AST fast path when the predicate has no LIKE / CASE / ScalarFunction nodes;
  // otherwise produce the bool mask via the column-producing evaluator.
  std::unique_ptr<cudf::column> mask;
  if (cudf_ast_can_evaluate(filter->predicate(), input.view())) {
    ExprContext ctx;
    auto& predicate = build_expr(filter->predicate(), ctx);
    mask = cudf::compute_column(input.view(), predicate);
  } else {
    mask = build_column(filter->predicate(), input.view());
  }
  auto filtered = cudf::apply_boolean_mask(input.view(), mask->view());
  auto result = TableResult::owning(std::move(filtered), std::move(input.column_names));

  // Optional projection, set when the planner fused a downstream ProjectionExec
  // into the filter. Skipping it leaves every input column in place and shifts all
  // downstream column indices by the number of columns that should have dropped.
  //
  // A selection over the filtered table, not a copy of it: kept columns are shared,
  // dropped ones freed with `result`, and a repeated ordinal is two entries over one
  // owner rather than one column moved twice (#154).
  if (filter->projection() && filter->projection()->size() > 0) {
    std::vector<cudf::size_type> ordinals;
    ordinals.reserve(filter->projection()->size());
    for (auto idx : *filter->projection()) ordinals.push_back(static_cast<cudf::size_type>(idx));
    return result.select(ordinals);
  }

  return result;
}


}  // namespace peacock
