#pragma once
// PRIVATE header -- must stay under src/, NOT include/ (see plan_types.h).
//
// A join's residual condition: its conjuncts, which sides each reads, how one is evaluated
// over a side's rows or over a pair's, what the cuDF AST can take of it, and the chunk
// budget that bounds the pairs (join-rewrite-design.md §3.4 and §3.6). Knows nothing of any
// join type's recipe; the session decides which of these a type uses.

#include "operators/join_columns.h"
#include "peacock/expr.h"

#include <cstdint>
#include <memory>
#include <utility>
#include <vector>

namespace peacock {
namespace join {

/// A side's condition table: its own columns, then one per expression hoisted onto it
/// (§3.6). The hoisted columns are what the AST's references past the side's width name;
/// they never reach an output, which the session gathers from B and P.
struct CondTable {
  std::vector<std::unique_ptr<cudf::column>> owned;
  std::vector<cudf::column_view> cols;
  cudf::table_view view() const { return cudf::table_view{cols}; }
};

CondTable cond_table(cudf::table_view side, fb::JoinSide which, JoinFilterColMap map,
                     std::vector<const fb::Expr*> const& hoisted);

/// A zero-row table of the filter's columns in filter-schema order: what
/// cudf_ast_can_evaluate reads, which is types only.
struct TypeTable {
  std::vector<std::unique_ptr<cudf::column>> owned;
  std::vector<cudf::column_view> cols;
  cudf::table_view view() const { return cudf::table_view{cols}; }
};

TypeTable filter_type_table(JoinFilterColMap map, cudf::table_view B,
                            std::vector<cudf::data_type> const& probe_types);

/// One operand of a hoisted comparison: a column of a side's condition table. A bare
/// ColumnRef is that side's own ordinal; anything else is the k-th column hoisted onto that
/// side, whose index is the side's own width plus k.
struct Operand {
  bool build = false;
  bool hoisted = false;
  cudf::size_type ordinal = 0;
};

/// A conjunct the cuDF AST can evaluate: the plan's own expression, or a comparison whose
/// operands were hoisted into condition columns and which is built as AST nodes directly --
/// no FlatBuffer is rewritten.
struct AstConjunct {
  const fb::Expr* expr = nullptr;  // null for a hoisted comparison
  fb::BinaryOp op = fb::BinaryOp_Eq;
  Operand lhs, rhs;
};

/// The residual's top-level AND-chain, flattened.
void conjuncts_of(const fb::Expr* e, std::vector<const fb::Expr*>& out);

/// Which sides a conjunct reads, through the filter-column map.
void sides_read(const fb::Expr* e, JoinFilterColMap map, bool& build, bool& probe);

bool is_comparison(fb::BinaryOp op);

/// Whether the AST can compare two columns of these types.
bool hoisted_is_ast_able(fb::BinaryOp op, cudf::data_type lt, cudf::data_type rt);

/// What one pair of the residual's columns costs, for the chunk budget.
std::size_t filter_row_bytes(JoinFilterColMap map, cudf::table_view B, cudf::table_view P);

/// AND over `cs` of `c IS TRUE`, evaluated over the rows of one side.
std::unique_ptr<cudf::column> eval_on(std::vector<const fb::Expr*> const& cs, cudf::table_view side,
                                      fb::JoinSide which, JoinFilterColMap map);

/// The residual over the key matches, evaluated once per pair on the filter's columns alone.
std::unique_ptr<cudf::column> residual_mask(cudf::table_view B, cudf::table_view P,
                                            cudf::column_view bi, cudf::column_view pi,
                                            std::vector<const fb::Expr*> const& cs,
                                            JoinFilterColMap map);

/// AND of the AST-able conjuncts as one cuDF AST. `flip` swaps which side cuDF calls its
/// left, and `bw`/`pw` are the sides' own widths, which a hoisted reference sits past.
cudf::ast::expression const& cross_ast(std::vector<AstConjunct> const& cs, ExprContext& ctx,
                                       JoinFilterColMap const& map, bool flip, cudf::size_type bw,
                                       cudf::size_type pw);

/// Row ranges of the probe batch, so each range's pairs fit `budget` bytes (0 = 1 GiB).
std::vector<std::pair<cudf::size_type, cudf::size_type>> chunks(std::size_t pairs,
                                                                std::size_t row_bytes,
                                                                cudf::size_type rows,
                                                                uint64_t budget);

}  // namespace join
}  // namespace peacock
