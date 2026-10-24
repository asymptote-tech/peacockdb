// CudfProject -- column selection / renaming / computed columns.

#include "peacock/operators.h"
#include "peacock/expr.h"

#include <cudf/table/table.hpp>
#include <cudf/copying.hpp>
#include <cudf/transform.hpp>

#include <algorithm>
#include <stdexcept>
#include <string>

namespace peacock {

TableResult execute_project(const fb::CudfProject* proj, NodeInputs* in) {
  if (!proj->exprs() || proj->exprs()->size() == 0)
    throw std::runtime_error(
        "CudfProject with no expression: the plan declares __rowmarker__ (#63)");
  auto input = take_input(in);

  auto tv = input.view();
  // Assembled from parts rather than through `with`: a project reorders and renames, so
  // nothing it emits is the input handle plus a column. `of` checks the three agree (#164).
  std::vector<std::shared_ptr<cudf::column const>> owners;
  std::vector<cudf::column_view> columns;
  std::vector<std::string> names;

  for (flatbuffers::uoffset_t i = 0; i < proj->exprs()->size(); ++i) {
    auto* expr = proj->exprs()->Get(i);

    // Fast path: a simple column reference is the input's own column, shared. No copy,
    // and a column projected twice is two entries over one owner (#154).
    if (expr->node_type() == fb::ExprNode_ColumnRef) {
      auto idx = column_ordinal(expr->node_as_ColumnRef(), input.column_names);
      owners.push_back(input.owners.at(idx));
      columns.push_back(input.columns.at(idx));
    } else {
      std::unique_ptr<cudf::column> made;
      if (cudf_ast_can_evaluate(expr, tv, input.column_names)) {
        // Pure AST expression: fuse via cudf::compute_column.
        ExprContext ctx;
        auto& ast = build_expr(expr, ctx);
        made = cudf::compute_column(tv, ast);
      } else {
        // Not AST-able (cudf_ast_can_evaluate): column-producing path.
        made = build_column(expr, tv, input.column_names);
      }
      std::shared_ptr<cudf::column const> owner{std::move(made)};
      columns.push_back(owner->view());
      owners.push_back(std::move(owner));
    }

    if (proj->aliases() && i < proj->aliases()->size()) {
      names.push_back(proj->aliases()->Get(i)->str());
    } else {
      names.push_back("col" + std::to_string(i));
    }
  }

  return TableResult::of(std::move(owners), std::move(columns), std::move(names));
}

}  // namespace peacock
