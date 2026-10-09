#pragma once
// PRIVATE header -- must stay under src/, NOT include/ (see plan_types.h).
//
// Expression building: the AST fast path and the column-producing path.
//
// plan_executor_internal.h is included, NOT folded in: it is the narrow contract
// the host-only CPU tests compile against (binop_output_type, cudf_ast_can_evaluate) and
// carries the rationale for exposing those two at all.

#include "peacock/plan_types.h"
#include "plan_executor_internal.h"

#include <cudf/ast/expressions.hpp>
#include <cudf/column/column.hpp>
#include <cudf/scalar/scalar.hpp>
#include <cudf/table/table_view.hpp>
#include <cudf/types.hpp>
#include <cudf/utilities/span.hpp>

#include <memory>
#include <vector>

namespace peacock {

/// Owns all AST sub-expressions so references remain valid for cuDF.
struct ExprContext {
  std::vector<std::unique_ptr<cudf::ast::expression>> owned;
  std::vector<std::unique_ptr<cudf::scalar>> scalars;

  cudf::ast::expression& keep(std::unique_ptr<cudf::ast::expression> e) {
    owned.push_back(std::move(e));
    return *owned.back();
  }
};

// When non-null (join-filter context), a ColumnRef(i) in the expression is
// remapped to column_reference(col_map[i].index, LEFT|RIGHT) so a mixed
// semi/anti join's AST predicate can address its two conditional tables.
//
// A span rather than the FlatBuffers vector itself, so a caller may pass a map it built:
// the join session flips the sides for the call that hands cuDF the probe as its left, and
// appends an entry per hoisted condition column. `col_map_of` is the plan's own map.
using JoinFilterColMap = cudf::host_span<fb::JoinFilterColumn const>;

// A FlatBuffers vector of structs stores them inline, but its `data()` is typed
// `const S* const*`; `Data()` is the bytes. A `true` filter carries no filter_columns, so a
// null vector is an empty span.
inline JoinFilterColMap col_map_of(const flatbuffers::Vector<const fb::JoinFilterColumn*>* v) {
  if (!v) return {};
  return {reinterpret_cast<const fb::JoinFilterColumn*>(v->Data()), v->size()};
}

// Default argument lives on the DECLARATION only -- repeating it on the definition
// is a hard error.
cudf::ast::expression& build_expr(const fb::Expr* expr, ExprContext& ctx,
                                  const JoinFilterColMap* col_map = nullptr);

// Materialize an expression into a column (the non-AST path).
std::unique_ptr<cudf::column> build_column(const fb::Expr* expr,
                                           cudf::table_view const& table);

cudf::type_id fb_to_type_id(fb::DataType dt);

// binop_output_type, cudf_ast_can_evaluate, EvaluatedColumn and evaluate_column come from
// plan_executor_internal.h, above.

}  // namespace peacock
