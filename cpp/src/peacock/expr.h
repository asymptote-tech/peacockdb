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
using JoinFilterColMap = flatbuffers::Vector<const fb::JoinFilterColumn*>;

// Default argument lives on the DECLARATION only -- repeating it on the definition
// is a hard error.
cudf::ast::expression& build_expr(const fb::Expr* expr, ExprContext& ctx,
                                  const JoinFilterColMap* col_map = nullptr);

/// What an expression evaluated to: a column the evaluation made, or one of the input
/// table's own columns, borrowed — a bare ColumnRef is the second. Copying it was #154's
/// costliest site: a whole column per batch on every predicate cuDF's AST refuses.
struct EvaluatedColumn {
  std::unique_ptr<cudf::column> owned;  // null when borrowed
  cudf::column_view borrowed;           // valid while the input table lives

  [[nodiscard]] cudf::column_view view() const { return owned ? owned->view() : borrowed; }

  /// Ownership, for a caller that keeps the column past the input: the made column, or a
  /// copy of the borrowed one — the one place a borrowed column is copied.
  [[nodiscard]] std::unique_ptr<cudf::column> take() && {
    return owned ? std::move(owned) : std::make_unique<cudf::column>(borrowed);
  }
};

// Evaluate an expression, borrowing the input's column for a bare ColumnRef (the non-AST
// path). The borrowed view is valid only while `table` lives.
EvaluatedColumn evaluate_column(const fb::Expr* expr, cudf::table_view const& table);

// Materialize an expression into a column (the non-AST path).
std::unique_ptr<cudf::column> build_column(const fb::Expr* expr,
                                           cudf::table_view const& table);

cudf::type_id fb_to_type_id(fb::DataType dt);

// binop_output_type and cudf_ast_can_evaluate come from plan_executor_internal.h, above.

}  // namespace peacock
