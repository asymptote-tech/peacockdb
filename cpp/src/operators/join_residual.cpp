// A join's residual condition (join-rewrite-design.md §3.4, §3.6). See join_residual.h.

#include "operators/join_residual.h"

#include <cudf/binaryop.hpp>
#include <cudf/copying.hpp>
#include <cudf/replace.hpp>
#include <cudf/utilities/traits.hpp>

#include <algorithm>
#include <stdexcept>
#include <string>

namespace peacock {
namespace join {
namespace {

/// The operands of an expression node, so a walk can ask which sides a conjunct reads.
/// Exhaustive over `ExprNode`: a kind with no arm is a refusal naming it, never a silent
/// "reads nothing", which would classify a conjunct reading both sides as one-sided.
template <class F>
void for_each_child(const fb::Expr* e, F&& f) {
  switch (e->node_type()) {
    case fb::ExprNode_ColumnRef:
    case fb::ExprNode_LiteralExpr:
      return;
    case fb::ExprNode_BinaryExprNode: {
      auto* b = e->node_as_BinaryExprNode();
      f(b->left());
      f(b->right());
      return;
    }
    case fb::ExprNode_UnaryExprNode:
      f(e->node_as_UnaryExprNode()->arg());
      return;
    case fb::ExprNode_CastExprNode:
      f(e->node_as_CastExprNode()->expr());
      return;
    case fb::ExprNode_AggregateFuncNode: {
      if (auto* args = e->node_as_AggregateFuncNode()->args())
        for (auto const* a : *args) f(a);
      return;
    }
    case fb::ExprNode_LikeExprNode: {
      auto* l = e->node_as_LikeExprNode();
      f(l->expr());
      f(l->pattern());
      return;
    }
    case fb::ExprNode_CaseExprNode: {
      auto* c = e->node_as_CaseExprNode();
      if (c->expr()) f(c->expr());
      if (c->when_thens())
        for (auto const* w : *c->when_thens()) {
          f(w->when());
          f(w->then());
        }
      if (c->else_expr()) f(c->else_expr());
      return;
    }
    case fb::ExprNode_ScalarFunctionExprNode: {
      if (auto* args = e->node_as_ScalarFunctionExprNode()->args())
        for (auto const* a : *args) f(a);
      return;
    }
    default:
      throw std::runtime_error(std::string("CudfJoin: a filter holding a ") +
                               fb::EnumNameExprNode(e->node_type()) +
                               " has operands this join cannot read");
  }
}

}  // namespace

/// The residual's top-level AND-chain, flattened.
void conjuncts_of(const fb::Expr* e, std::vector<const fb::Expr*>& out) {
  if (e->node_type() == fb::ExprNode_BinaryExprNode &&
      e->node_as_BinaryExprNode()->op() == fb::BinaryOp_And) {
    conjuncts_of(e->node_as_BinaryExprNode()->left(), out);
    conjuncts_of(e->node_as_BinaryExprNode()->right(), out);
    return;
  }
  out.push_back(e);
}

/// Which sides a conjunct reads, through the filter-column map.
void sides_read(const fb::Expr* e, JoinFilterColMap map, bool& build, bool& probe) {
  if (e->node_type() == fb::ExprNode_ColumnRef) {
    auto i = e->node_as_ColumnRef()->index();
    if (i >= map.size())
      throw std::runtime_error("CudfJoin: a filter ColumnRef out of range of filter_columns");
    (map[i].side() == fb::JoinSide_Left ? build : probe) = true;
    return;
  }
  for_each_child(e, [&](const fb::Expr* c) { sides_read(c, map, build, probe); });
}

/// What one pair of the residual's columns costs, for the chunk budget. A variable-width
/// column is priced at 16 bytes: its content is not known before the gather.
std::size_t filter_row_bytes(JoinFilterColMap map, cudf::table_view B, cudf::table_view P) {
  std::size_t total = 0;
  for (auto const& fc : map) {
    auto t = (fc.side() == fb::JoinSide_Left ? B : P).column(fc.index()).type();
    total += cudf::is_fixed_width(t) ? static_cast<std::size_t>(cudf::size_of(t)) : 16;
  }
  return total;
}

/// One side's columns in filter-schema order, so a one-side expression's ColumnRef(i) reads
/// the right column. The other side's slots hold this side's column 0, which such an
/// expression never reads, only so the view has one row count.
static std::vector<cudf::column_view> side_slots(cudf::table_view side, fb::JoinSide which,
                                                 JoinFilterColMap map) {
  std::vector<cudf::column_view> cols;
  for (auto const& fc : map)
    cols.push_back(fc.side() == which ? side.column(fc.index()) : side.column(0));
  return cols;
}

CondTable cond_table(cudf::table_view side, fb::JoinSide which, JoinFilterColMap map,
                     std::vector<const fb::Expr*> const& hoisted) {
  CondTable ct;
  for (auto const& c : side) ct.cols.push_back(c);
  if (hoisted.empty()) return ct;
  auto slot_cols = side_slots(side, which, map);
  cudf::table_view slots{slot_cols};
  for (auto const* e : hoisted) {
    ct.owned.push_back(build_column(e, slots));
    ct.cols.push_back(ct.owned.back()->view());
  }
  return ct;
}

/// AND over `cs` of `c IS TRUE`, evaluated over the rows of one side.
std::unique_ptr<cudf::column> eval_on(std::vector<const fb::Expr*> const& cs, cudf::table_view side,
                                      fb::JoinSide which, JoinFilterColMap map) {
  auto cols = side_slots(side, which, map);
  cudf::table_view ft{cols};
  auto acc = bools(side.num_rows(), true);
  cudf::numeric_scalar<bool> f(false);
  for (auto const* e : cs) {
    // R IS TRUE: a NULL condition is no match, so an anti join keeps the row and a mark
    // reads false rather than NULL (design §3.4, D3).
    auto v = cudf::replace_nulls(build_column(e, ft)->view(), f);
    acc = cudf::binary_operation(acc->view(), v->view(), cudf::binary_operator::LOGICAL_AND, kBool);
  }
  return acc;
}

/// The residual over the key matches, evaluated once per pair on the filter's columns
/// alone -- the full rows are gathered for the survivors only.
std::unique_ptr<cudf::column> residual_mask(cudf::table_view B, cudf::table_view P,
                                            cudf::column_view bi, cudf::column_view pi,
                                            std::vector<const fb::Expr*> const& cs,
                                            JoinFilterColMap map) {
  std::vector<std::unique_ptr<cudf::table>> owned;
  std::vector<cudf::column_view> cols;
  for (auto const& fc : map) {
    const bool build = fc.side() == fb::JoinSide_Left;
    auto src = (build ? B : P).select({static_cast<cudf::size_type>(fc.index())});
    owned.push_back(cudf::gather(src, build ? bi : pi, cudf::out_of_bounds_policy::DONT_CHECK));
    cols.push_back(owned.back()->get_column(0).view());
  }
  cudf::table_view ft{cols};
  auto acc = bools(bi.size(), true);
  cudf::numeric_scalar<bool> f(false);
  for (auto const* e : cs) {
    auto v = cudf::replace_nulls(build_column(e, ft)->view(), f);
    acc = cudf::binary_operation(acc->view(), v->view(), cudf::binary_operator::LOGICAL_AND, kBool);
  }
  return acc;
}

TypeTable filter_type_table(JoinFilterColMap map, cudf::table_view B,
                            std::vector<cudf::data_type> const& probe_types) {
  TypeTable tt;
  for (auto const& fc : map) {
    if (fc.side() == fb::JoinSide_Left) {
      tt.cols.push_back(cudf::slice(B.column(fc.index()), {0, 0}).front());
      continue;
    }
    if (fc.index() >= probe_types.size())
      throw std::runtime_error("CudfJoin: a filter column names probe column " +
                               std::to_string(fc.index()) + " of a probe_schema of " +
                               std::to_string(probe_types.size()));
    tt.owned.push_back(null_column(probe_types[fc.index()], 0));
    tt.cols.push_back(tt.owned.back()->view());
  }
  return tt;
}

bool is_comparison(fb::BinaryOp op) {
  switch (op) {
    case fb::BinaryOp_Eq:
    case fb::BinaryOp_NotEq:
    case fb::BinaryOp_Lt:
    case fb::BinaryOp_LtEq:
    case fb::BinaryOp_Gt:
    case fb::BinaryOp_GtEq:
      return true;
    default:
      return false;
  }
}

static cudf::ast::ast_operator ast_op_of(fb::BinaryOp op) {
  switch (op) {
    case fb::BinaryOp_Eq:
      return cudf::ast::ast_operator::EQUAL;
    case fb::BinaryOp_NotEq:
      return cudf::ast::ast_operator::NOT_EQUAL;
    case fb::BinaryOp_Lt:
      return cudf::ast::ast_operator::LESS;
    case fb::BinaryOp_LtEq:
      return cudf::ast::ast_operator::LESS_EQUAL;
    case fb::BinaryOp_Gt:
      return cudf::ast::ast_operator::GREATER;
    default:
      return cudf::ast::ast_operator::GREATER_EQUAL;
  }
}

/// Whether the AST can compare two columns of these types -- asked of
/// `cudf_ast_can_evaluate` over a two-column table rather than restated here, so the one
/// routing rule answers for a hoisted comparison too.
bool hoisted_is_ast_able(fb::BinaryOp op, cudf::data_type lt, cudf::data_type rt) {
  flatbuffers::FlatBufferBuilder fbb;
  auto l = fb::CreateExpr(fbb, fb::ExprNode_ColumnRef, fb::CreateColumnRef(fbb, 0).Union());
  auto r = fb::CreateExpr(fbb, fb::ExprNode_ColumnRef, fb::CreateColumnRef(fbb, 1).Union());
  fbb.Finish(fb::CreateExpr(fbb, fb::ExprNode_BinaryExprNode,
                            fb::CreateBinaryExprNode(fbb, l, op, r).Union()));
  auto lc = null_column(lt, 0), rc = null_column(rt, 0);
  cudf::table_view tt{{lc->view(), rc->view()}};
  return cudf_ast_can_evaluate(flatbuffers::GetRoot<fb::Expr>(fbb.GetBufferPointer()), tt);
}

/// `lhs OP rhs` as cuDF AST nodes owned by `ctx`. `flip` swaps which side cuDF calls its
/// left, for the calls that pass the probe first.
static cudf::ast::expression const& hoisted_ast(AstConjunct const& c, ExprContext& ctx, bool flip,
                                                cudf::size_type bw, cudf::size_type pw) {
  auto ref = [&](Operand o) -> cudf::ast::expression const& {
    auto table =
        (o.build != flip) ? cudf::ast::table_reference::LEFT : cudf::ast::table_reference::RIGHT;
    auto idx = o.hoisted ? (o.build ? bw : pw) + o.ordinal : o.ordinal;
    return ctx.keep(std::make_unique<cudf::ast::column_reference>(idx, table));
  };
  auto const& l = ref(c.lhs);
  auto const& r = ref(c.rhs);
  return ctx.keep(std::make_unique<cudf::ast::operation>(ast_op_of(c.op), l, r));
}

/// AND of the AST-able conjuncts as one cuDF AST, so several conjuncts make one `mixed_*`
/// or `conditional_*` predicate. NULL_LOGICAL_AND, so a NULL operand beside a false one is
/// false; a NULL result is no match, as mixed_*'s header says.
cudf::ast::expression const& cross_ast(std::vector<AstConjunct> const& cs, ExprContext& ctx,
                                       JoinFilterColMap const& map, bool flip, cudf::size_type bw,
                                       cudf::size_type pw) {
  auto one = [&](AstConjunct const& c) -> cudf::ast::expression const& {
    return c.expr ? build_expr(c.expr, ctx, &map) : hoisted_ast(c, ctx, flip, bw, pw);
  };
  cudf::ast::expression const* acc = &one(cs.front());
  for (std::size_t i = 1; i < cs.size(); ++i) {
    auto const& next = one(cs[i]);
    acc = &ctx.keep(std::make_unique<cudf::ast::operation>(
        cudf::ast::ast_operator::NULL_LOGICAL_AND, *acc, next));
  }
  return *acc;
}

/// Row ranges of the probe batch, so each range's pairs fit `budget` bytes (0 = 1 GiB).
/// Every per-row outcome is decided within one range, so chunking never moves an answer.
std::vector<std::pair<cudf::size_type, cudf::size_type>> chunks(std::size_t pairs,
                                                                std::size_t row_bytes,
                                                                cudf::size_type rows,
                                                                uint64_t budget) {
  if (budget == 0) budget = 1ull << 30;
  const std::size_t bytes = pairs * (8 + row_bytes);
  std::size_t k = std::max<std::size_t>(1, (bytes + budget - 1) / budget);
  k = std::min<std::size_t>(k, static_cast<std::size_t>(std::max(rows, 1)));
  std::vector<std::pair<cudf::size_type, cudf::size_type>> out;
  for (std::size_t i = 0; i < k; ++i)
    out.emplace_back(static_cast<cudf::size_type>(rows * i / k),
                     static_cast<cudf::size_type>(rows * (i + 1) / k));
  return out;
}

}  // namespace join
}  // namespace peacock
