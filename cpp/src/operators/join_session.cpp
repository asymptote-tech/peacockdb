// The join session: join-rewrite-design.md §3. Names follow its pseudocode.

#include "operators/join_session.h"
#include "operators/join_columns.h"
#include "operators/join_residual.h"
#include "peacock/expr.h"

#if __has_include(<cudf/join/join.hpp>)
#include <cudf/join/join.hpp>
#include <cudf/join/hash_join.hpp>
#include <cudf/join/distinct_hash_join.hpp>
#include <cudf/join/conditional_join.hpp>
#include <cudf/join/mixed_join.hpp>
#else
#include <cudf/join.hpp>
#endif
#include <cudf/binaryop.hpp>
#include <cudf/column/column_factories.hpp>
#include <cudf/copying.hpp>
#include <cudf/filling.hpp>
#include <cudf/reshape.hpp>
#include <cudf/reshape.hpp>
#include <cudf/stream_compaction.hpp>
#include <cudf/table/table.hpp>

#include <limits>
#include <stdexcept>
#include <string>
namespace peacock {

// The session's arms are written in the vocabulary of join_columns.h and join_residual.h,
// which exist for it alone; qualifying every one of them would say nothing a reader of this
// file does not already know.
using namespace join;

struct JoinSession::State {
  const fb::CudfJoin* d = nullptr;
  fb::JoinType type = fb::JoinType_Inner;
  cudf::null_equality cmp = cudf::null_equality::UNEQUAL;
  TableResult B;                              // the build side, owned by the session
  std::vector<cudf::size_type> bkeys, pkeys;  // key ordinals per side
  std::unique_ptr<cudf::hash_join> hj;        // built once
  /// The build's distinct keys, owned here because `dhj` only views them, and the object
  /// built over them. Declared after `B` and before `dhj`, so each outlives what views it.
  std::unique_ptr<cudf::table> Bd;
  std::unique_ptr<cudf::distinct_hash_join> dhj;
  /// No build row can ever match: no batch, zero rows, or every key NULL under UNEQUAL.
  /// No cuDF join object is made, and §3.8's answers stand in.
  bool empty_build = false;
  bool finished = false;
  /// Which build rows some probe batch matched. Only the types whose finish reads it.
  std::unique_ptr<cudf::column> matched;
  /// The probe side's own types and names, from its first batch: a pad matches what the
  /// matched rows carry. Empty until one arrives, and then `probe_schema` answers instead.
  std::vector<cudf::data_type> probe_types;
  std::vector<std::string> probe_names;
  /// The residual, split once. `cross` reads both sides; the two one-side vectors are only
  /// filled for the semi family, since an outer type's one-side conjunct decides whether a
  /// row is unmatched and so belongs with the pairs.
  std::vector<const fb::Expr*> cross, build_only, probe_only;
  JoinFilterColMap map;  // the plan's filter-column map, as a span
  /// The same map with the sides swapped, for the RightSemi/RightAnti call that hands cuDF
  /// the probe as its left. Owned here, so it outlives every AST built from it.
  std::vector<fb::JoinFilterColumn> flipped_store;
  JoinFilterColMap flipped;
  /// `cross` split by what the cuDF AST can evaluate: `A` goes into one AST predicate, `R`
  /// is evaluated as columns over the candidate pairs (§3.6's split).
  std::vector<AstConjunct> A;
  std::vector<const fb::Expr*> R;
  /// Expressions hoisted into a side's condition table, in append order, and the build's
  /// table once it is made. A hoisted column never reaches an output.
  std::vector<const fb::Expr*> hoist_build, hoist_probe;
  CondTable B_cond;
  /// Whether one `mixed_*` call can answer `cross` -- a keyed semi join whose every cross
  /// conjunct is AST-able. The pair types take the pairs path whatever their residual is.
  bool cross_is_ast = false;

  /// The build's conditional table: its own columns while nothing was hoisted onto it.
  cudf::table_view B_cond_view() const { return B_cond.cols.empty() ? B.view() : B_cond.view(); }

  cudf::table_view Bk() const { return B.view().select(bkeys); }
  cudf::table_view keys_of(TableResult const& P) const { return P.view().select(pkeys); }
};

JoinSession::JoinSession(const fb::CudfJoin* d, std::optional<TableResult> build)
    : s_(std::make_unique<State>()) {
  s_->d = d;
  s_->type = d->join_type();
  s_->cmp = d->null_equals_null() ? cudf::null_equality::EQUAL : cudf::null_equality::UNEQUAL;
  // No build batch is an empty build side (#212), typed from the plan's own schema so the
  // output's build columns carry the types the matched rows would have carried. A side of no
  // columns is refused by `TableResult::owning`, which every route into a handle passes
  // through, so there is no second check for it here.
  s_->B = build ? std::move(*build)
                : null_table(types_of(d->build_schema()), names_of(d->build_schema()), 0);
  if (d->keys()) {
    for (auto const* k : *d->keys()) {
      if (k->left()->node_type() != fb::ExprNode_ColumnRef ||
          k->right()->node_type() != fb::ExprNode_ColumnRef)
        throw std::runtime_error("CudfJoin: only ColumnRef keys");
      s_->bkeys.push_back(static_cast<cudf::size_type>(k->left()->node_as_ColumnRef()->index()));
      s_->pkeys.push_back(static_cast<cudf::size_type>(k->right()->node_as_ColumnRef()->index()));
    }
  }
  // Refused here rather than at the first emit, where a type answering only at finish would
  // reach it one call late. The ordinals themselves are checked by `select`, which needs the
  // output's width and so cannot run before a batch.
  if (d->projection() && d->projection()->size() == 0)
    throw std::runtime_error(
        "CudfJoin: a projection of zero columns; the plan keeps __rowmarker__ instead");
  if (d->filter()) split_residual();
  // No keys and no condition is the cross join, §3.7, and that is Inner alone: every other
  // predicate-free type arrives as a nested loop over the literal true (design §4.1).
  if (s_->bkeys.empty() && !d->filter() && s_->type != fb::JoinType_Inner)
    throw std::runtime_error(std::string("CudfJoin: a predicate-free ") +
                             fb::EnumNameJoinType(s_->type) +
                             " join arrives with the literal true as its condition, not as a "
                             "cross join");
  // A conjunct reading only the side a semi join does not emit filters that side before any
  // key work; for RightSemi/RightAnti that side is the build.
  if (is_probe_side_semi(s_->type) && !s_->build_only.empty()) {
    auto keep = eval_on(s_->build_only, s_->B.view(), fb::JoinSide_Left, s_->map);
    s_->B = TableResult::owning(cudf::apply_boolean_mask(s_->B.view(), keep->view()),
                                s_->B.column_names);
  }
  if (s_->type == fb::JoinType_Left || s_->type == fb::JoinType_Full ||
      is_build_side_semi(s_->type))
    s_->matched = bools(s_->B.num_rows(), false);
  // After `matched`, which an empty build's finish still reads: at zero rows it answers the
  // build padded, zero rows, or a zero-row mark without an arm of its own.
  if (s_->B.num_rows() == 0) {
    s_->empty_build = true;
    return;
  }
  // A keyed pair type takes the pairs path whatever its residual is, so there is nothing to
  // split or hoist for it: its conjuncts are evaluated as columns over the key matches.
  const bool pairs_only = !s_->bkeys.empty() && emits_pairs(s_->type);
  if (!s_->cross.empty() && !pairs_only) {
    auto tt = filter_type_table(s_->map, s_->B.view(), types_of(d->probe_schema()));
    for (auto const* e : s_->cross) {
      if (cudf_ast_can_evaluate(e, tt.view())) {
        s_->A.push_back(AstConjunct{e});
      } else if (!hoist(e, tt.view())) {
        s_->R.push_back(e);
      }
    }
    flip_map();
    // A semi-family cross residual the AST can take whole is one mixed_* call per probe and
    // needs no hash table of ours; one it cannot takes the pairs path over `hj` below.
    s_->cross_is_ast = s_->R.empty() && !emits_pairs(s_->type) && !s_->bkeys.empty();
    const bool uses_ast = !s_->A.empty() && (s_->bkeys.empty() || s_->cross_is_ast);
    if (uses_ast && !s_->hoist_build.empty())
      s_->B_cond = cond_table(s_->B.view(), fb::JoinSide_Left, s_->map, s_->hoist_build);
    if (s_->cross_is_ast) return;
  }
  if (s_->bkeys.empty()) return;  // a nested loop: cuDF's conditional joins, no hash table
  if (is_probe_side_semi(s_->type) && s_->cross.empty()) {
    s_->Bd = distinct_keys(s_->Bk(), s_->cmp);
    if (s_->Bd->num_rows() == 0) {
      s_->empty_build = true;  // every build key was NULL under UNEQUAL
      return;
    }
    // Two arguments: the explicit-stream form does not compile on 26.02, where a stream
    // cannot convert to the load factor that sits between them.
    s_->dhj = std::make_unique<cudf::distinct_hash_join>(s_->Bd->view(), s_->cmp);
    return;
  }
  s_->hj = std::make_unique<cudf::hash_join>(s_->Bk(), s_->cmp);  // the portable ctor
}

JoinSession::~JoinSession() = default;

/// The residual's conjuncts by the sides they read. An outer type pushes nothing: a
/// one-side conjunct there decides whether a row is unmatched, so it belongs with the
/// pairs. The semi family emits one side's rows, and a conjunct reading one side alone
/// never needs the pairs at all (design §3.4's last rule).
void JoinSession::split_residual() {
  const fb::CudfJoin* d = s_->d;
  s_->map = col_map_of(d->filter_columns());
  if (s_->map.empty()) throw std::runtime_error("CudfJoin: a filter with no filter_columns map");
  std::vector<const fb::Expr*> parts;
  conjuncts_of(d->filter(), parts);
  // Only a keyed semi join pushes a one-side conjunct to its side: a nested loop evaluates
  // every conjunct over the pairs it is already making (§3.6's split is A against R).
  const bool semi =
      (is_build_side_semi(s_->type) || is_probe_side_semi(s_->type)) && !s_->bkeys.empty();
  for (auto const* e : parts) {
    if (!semi) {
      s_->cross.push_back(e);
      continue;
    }
    bool reads_build = false, reads_probe = false;
    sides_read(e, s_->map, reads_build, reads_probe);
    if (reads_build && !reads_probe)
      s_->build_only.push_back(e);
    else if (reads_probe && !reads_build)
      s_->probe_only.push_back(e);
    else
      s_->cross.push_back(e);  // both sides, or a constant: cheapest where the pairs are
  }
}

/// §3.6's `hoist`, bounded to a comparison each of whose operands reads one side: the
/// operand that is not a bare ColumnRef becomes a column of its side's condition table, and
/// the comparison is then two column references the AST can take. False when the conjunct is
/// not of that shape, or when the comparison is still refused after it -- it then goes to
/// `R`, where the column path evaluates anything.
bool JoinSession::hoist(const fb::Expr* e, cudf::table_view tt) {
  if (e->node_type() != fb::ExprNode_BinaryExprNode) return false;
  auto const* b = e->node_as_BinaryExprNode();
  if (!is_comparison(b->op())) return false;
  const fb::Expr* sides[2] = {b->left(), b->right()};
  Operand ops[2];
  bool reads_a_side[2] = {false, false};
  for (int i = 0; i < 2; ++i) {
    bool rb = false, rp = false;
    sides_read(sides[i], s_->map, rb, rp);
    if (rb && rp) return false;  // one operand already spans both sides
    reads_a_side[i] = rb || rp;
    ops[i].build = rb;
    ops[i].hoisted = sides[i]->node_type() != fb::ExprNode_ColumnRef;
    if (!ops[i].hoisted)
      ops[i].ordinal =
          static_cast<cudf::size_type>(s_->map[sides[i]->node_as_ColumnRef()->index()].index());
  }
  // A literal operand becomes a constant column on the other operand's side, so both
  // operands are always column references. Two literals are no join condition.
  if (!reads_a_side[0] && !reads_a_side[1]) return false;
  if (!reads_a_side[0]) ops[0].build = !ops[1].build;
  if (!reads_a_side[1]) ops[1].build = !ops[0].build;
  auto type_of = [&](const fb::Expr* x) {
    if (x->node_type() == fb::ExprNode_ColumnRef)
      return tt.column(x->node_as_ColumnRef()->index()).type();
    return build_column(x, tt)->type();
  };
  if (!hoisted_is_ast_able(b->op(), type_of(sides[0]), type_of(sides[1]))) return false;
  for (int i = 0; i < 2; ++i) {
    if (!ops[i].hoisted) continue;
    auto& dst = ops[i].build ? s_->hoist_build : s_->hoist_probe;
    ops[i].ordinal = static_cast<cudf::size_type>(dst.size());
    dst.push_back(sides[i]);
  }
  s_->A.push_back(AstConjunct{nullptr, b->op(), ops[0], ops[1]});
  return true;
}

/// The filter-column map with every side swapped. The RightSemi/RightAnti mixed_* call
/// passes the probe as cuDF's left, so the AST's LEFT must name the probe's columns.
void JoinSession::flip_map() {
  s_->flipped_store.clear();
  for (auto const& fc : s_->map)
    s_->flipped_store.emplace_back(
        fc.index(), fc.side() == fb::JoinSide_Left ? fb::JoinSide_Right : fb::JoinSide_Left);
  s_->flipped = JoinFilterColMap{s_->flipped_store.data(), s_->flipped_store.size()};
}

std::optional<TableResult> JoinSession::probe(TableResult P) {
  if (s_->finished) throw std::invalid_argument("join_probe: the join is finished");
  if (s_->probe_types.empty()) {
    s_->probe_types = types_of(P.view());
    s_->probe_names = P.column_names;
  }
  if (s_->empty_build) return probe_empty_build(std::move(P));
  // A build-side semi type's probe-only conjunct filters the batch before its keys are
  // taken: a probe row failing it cannot witness a match.
  if (is_build_side_semi(s_->type) && !s_->probe_only.empty()) {
    auto keep = eval_on(s_->probe_only, P.view(), fb::JoinSide_Right, s_->map);
    P = TableResult::owning(cudf::apply_boolean_mask(P.view(), keep->view()), P.column_names);
  }
  if (s_->bkeys.empty()) return s_->d->filter() ? probe_nested(P) : probe_cross(P);
  if (!s_->cross.empty()) return s_->cross_is_ast ? probe_mixed(P) : probe_pairs(P);
  auto Pk = s_->keys_of(P);
  const auto keep = cudf::out_of_bounds_policy::DONT_CHECK;
  switch (s_->type) {
    case fb::JoinType_Inner:
    case fb::JoinType_Left: {
      auto n = s_->hj->inner_join_size(Pk);  // both versions; also the output-size hint
      fits_or_throw(n, "an Inner or Left probe");
      auto [pi, bi] = s_->hj->inner_join(Pk, n);
      if (s_->matched) s_->matched = set_true(s_->matched->view(), idx_view(*bi));
      return projected(s_->d, side_by_side(gathered(s_->B, idx_view(*bi), keep),
                                           gathered(P, idx_view(*pi), keep)));
    }
    case fb::JoinType_Right:
    case fb::JoinType_Full: {
      auto n = s_->hj->left_join_size(Pk);
      fits_or_throw(n, "a Right or Full probe");
      // The probe is cuDF's left, so every probe row comes back once. Never full_join:
      // per batch it would re-emit every unmatched build row.
      auto [pi, bi] = s_->hj->left_join(Pk, n);
      if (s_->matched) {
        auto hit = in_range(idx_view(*bi), s_->B.num_rows());
        auto hits = cudf::apply_boolean_mask(cudf::table_view{{idx_view(*bi)}}, hit->view());
        s_->matched = set_true(s_->matched->view(), hits->get_column(0).view());
      }
      return projected(
          s_->d, side_by_side(gathered(s_->B, idx_view(*bi), cudf::out_of_bounds_policy::NULLIFY),
                              gathered(P, idx_view(*pi), keep)));
    }
    case fb::JoinType_LeftSemi:
    case fb::JoinType_LeftAnti:
    case fb::JoinType_LeftMark: {
      // The batch's distinct keys, so each build row is matched at most once per batch and
      // the index map is bounded by |B| rather than by the pairs.
      auto dk = distinct_keys(Pk, s_->cmp);
      auto [pi, bi] = s_->hj->inner_join(dk->view());
      s_->matched = set_true(s_->matched->view(), idx_view(*bi));
      return std::nullopt;
    }
    case fb::JoinType_RightSemi:
    case fb::JoinType_RightAnti: {
      auto bi = s_->dhj->left_join(Pk);  // one entry per probe row
      auto hit = in_range(idx_view(*bi), s_->Bd->num_rows());
      return projected(s_->d, kept_probe_rows(P, std::move(hit)));
    }
    default:
      throw std::runtime_error("CudfJoin: this type arrives in a later task");
  }
}

/// The probe rows a RightSemi keeps, or a RightAnti's complement of them. `hit` is the key
/// match; the preserved side's own conjuncts are ANDed in here, so a NULL one is no match.
TableResult JoinSession::kept_probe_rows(TableResult const& P, std::unique_ptr<cudf::column> hit) {
  if (!s_->probe_only.empty()) {
    auto R = eval_on(s_->probe_only, P.view(), fb::JoinSide_Right, s_->map);
    hit = cudf::binary_operation(hit->view(), R->view(), cudf::binary_operator::LOGICAL_AND, kBool);
  }
  // Anti is semi's complement, here and everywhere: design §3.4 D13.
  auto mask = s_->type == fb::JoinType_RightSemi ? std::move(hit) : negate(hit->view());
  return TableResult::owning(cudf::apply_boolean_mask(P.view(), mask->view()), P.column_names);
}

/// §3.4's row-wise matcher for the semi family: one `mixed_left_semi_join` per batch, the
/// cross conjuncts ANDed into its one AST predicate. Never mixed_left_anti_join, whose
/// header drops a row whose predicate is NULL whereas NOT EXISTS keeps it (D13).
std::optional<TableResult> JoinSession::probe_mixed(TableResult const& P) {
  ExprContext ctx;
  auto Pk = s_->keys_of(P);
  auto Pc = cond_table(P.view(), fb::JoinSide_Right, s_->map, s_->hoist_probe);
  const auto bw = s_->B.num_columns(), pw = P.num_columns();
  if (is_build_side_semi(s_->type)) {
    auto const& pred = cross_ast(s_->A, ctx, s_->map, false, bw, pw);
    auto bi = cudf::mixed_left_semi_join(s_->Bk(), Pk, s_->B_cond_view(), Pc.view(), pred, s_->cmp);
    s_->matched = set_true(s_->matched->view(), idx_view(*bi));
    return std::nullopt;
  }
  // RightSemi/RightAnti: the probe as cuDF's left, so the predicate reads the flipped map.
  auto const& pred = cross_ast(s_->A, ctx, s_->flipped, true, bw, pw);
  auto pi = cudf::mixed_left_semi_join(Pk, s_->Bk(), Pc.view(), s_->B_cond_view(), pred, s_->cmp);
  auto hit = set_true(bools(P.num_rows(), false)->view(), idx_view(*pi));
  return projected(s_->d, kept_probe_rows(P, std::move(hit)));
}

/// §3.8, what a session answers when no build row can match: no batch, zero rows, or
/// every build key NULL under UNEQUAL. The finish needs no arm of its own.
std::optional<TableResult> JoinSession::probe_empty_build(TableResult P) {
  switch (s_->type) {
    case fb::JoinType_LeftSemi:
    case fb::JoinType_LeftAnti:
    case fb::JoinType_LeftMark:
      return std::nullopt;
    case fb::JoinType_Right:
    case fb::JoinType_Full: {
      auto n = P.num_rows();
      return projected(
          s_->d,
          side_by_side(null_table(types_of(s_->B.view()), s_->B.column_names, n), std::move(P)));
    }
    case fb::JoinType_RightAnti:
      return projected(s_->d, std::move(P));  // every row
    case fb::JoinType_RightSemi:
      return projected(s_->d, P.slice(0, 0));
    default:  // Inner, Left: one zero-row table carrying the output's columns
      return projected(s_->d, side_by_side(s_->B.slice(0, 0), P.slice(0, 0)));
  }
}

/// §3.4's pairs path: the key matches, the residual over them, then the per-type
/// derivation. The probe batch is cut into row ranges whose pairs fit the scratch budget,
/// and the ranges' outputs are concatenated into the one table the call answers with.
std::optional<TableResult> JoinSession::probe_pairs(TableResult const& P) {
  if (!s_->hj)
    throw std::logic_error(
        "CudfJoin: the pairs path without a hash table -- a constructor arm is missing");
  const auto row_bytes = filter_row_bytes(s_->map, s_->B.view(), P.view());
  const auto total = s_->hj->inner_join_size(s_->keys_of(P));
  auto ranges = chunks(total, row_bytes, P.num_rows(), s_->d->chunk_bytes());
  std::vector<TableResult> parts;
  for (auto [lo, hi] : ranges) {
    TableResult Pc = P.slice(lo, hi);
    auto Pck = s_->keys_of(Pc);
    // A sizer is a whole hash probe, so the usual single range reuses the count above.
    auto n = ranges.size() == 1 ? total : s_->hj->inner_join_size(Pck);
    fits_or_throw(n, "a chunk of residual pairs");  // one probe row's matches can overflow
    auto [pi, bi] = s_->hj->inner_join(Pck, n);
    keep_and_derive(Pc, idx_view(*bi), idx_view(*pi), s_->cross, parts);
  }
  if (is_build_side_semi(s_->type)) return std::nullopt;
  return concat_rows(std::move(parts));
}

/// §3.4's per-type derivation over one chunk's surviving pairs, `bk` into the build and
/// `pk` into `Pc`. Shared by the keyed pairs path and the nested loop's candidates, so the
/// two cannot answer one type differently.
void JoinSession::derive(TableResult const& Pc, cudf::column_view bk, cudf::column_view pk,
                         std::vector<TableResult>& parts) {
  // #153: a build row counts as matched only once the condition has kept its pair.
  if (s_->matched) s_->matched = set_true(s_->matched->view(), bk);
  if (is_build_side_semi(s_->type)) return;  // `matched` is the whole answer
  if (is_probe_side_semi(s_->type)) {
    auto hit = set_true(bools(Pc.num_rows(), false)->view(), pk);
    parts.push_back(projected(s_->d, kept_probe_rows(Pc, std::move(hit))));
    return;
  }
  auto Bg = TableResult::owning(cudf::gather(s_->B.view(), bk), s_->B.column_names);
  auto Pg = TableResult::owning(cudf::gather(Pc.view(), pk), Pc.column_names);
  if (s_->type == fb::JoinType_Right || s_->type == fb::JoinType_Full) {
    auto hitP = set_true(bools(Pc.num_rows(), false)->view(), pk);
    auto UP = cudf::apply_boolean_mask(Pc.view(), negate(hitP->view())->view());
    auto unmatched = UP->num_rows();
    Bg = concat_rows(std::move(Bg),
                     null_table(types_of(s_->B.view()), s_->B.column_names, unmatched));
    Pg = concat_rows(std::move(Pg), TableResult::owning(std::move(UP), Pc.column_names));
  }
  parts.push_back(projected(s_->d, side_by_side(std::move(Bg), std::move(Pg))));
}

/// §3.6, no keys: cuDF's conditional joins over the AST-able conjuncts where the type
/// allows a row-wise answer, candidates and a residual otherwise.
std::optional<TableResult> JoinSession::probe_nested(TableResult const& P) {
  // Full takes the candidates recipe even with nothing left over: conditional_full_join has
  // no streaming form, and the left-join sentinel cannot be told from a real index (D8).
  if (s_->R.empty() && !s_->A.empty() && s_->type != fb::JoinType_Full)
    return probe_nested_rowwise(P);
  if (s_->A.empty()) return probe_nested_cross(P);
  return probe_nested_candidates(P);
}

std::optional<TableResult> JoinSession::probe_nested_rowwise(TableResult const& P) {
  ExprContext ctx;
  auto Pcond = cond_table(P.view(), fb::JoinSide_Right, s_->map, s_->hoist_probe);
  auto Bc = s_->B_cond_view();
  auto Pc = Pcond.view();
  const auto bw = s_->B.num_columns(), pw = P.num_columns();
  auto const& a = cross_ast(s_->A, ctx, s_->map, false, bw, pw);
  auto const& a_swapped = cross_ast(s_->A, ctx, s_->flipped, true, bw, pw);
  const auto keep = cudf::out_of_bounds_policy::DONT_CHECK;
  switch (s_->type) {
    case fb::JoinType_Inner:
    case fb::JoinType_Left: {
      // Never conditional_left_join here: per batch it would re-emit every build row that
      // this batch did not match, which the finish answers once instead.
      auto n = cudf::conditional_inner_join_size(Bc, Pc, a);
      fits_or_throw(n, "a nested-loop probe");
      auto [bi, pi] = cudf::conditional_inner_join(Bc, Pc, a, n);
      if (s_->matched) s_->matched = set_true(s_->matched->view(), idx_view(*bi));
      return projected(s_->d, side_by_side(gathered(s_->B, idx_view(*bi), keep),
                                           gathered(P, idx_view(*pi), keep)));
    }
    case fb::JoinType_Right: {
      auto n = cudf::conditional_left_join_size(Pc, Bc, a_swapped);
      fits_or_throw(n, "a nested-loop Right probe");
      auto [pi, bi] = cudf::conditional_left_join(Pc, Bc, a_swapped, n);
      return projected(
          s_->d, side_by_side(gathered(s_->B, idx_view(*bi), cudf::out_of_bounds_policy::NULLIFY),
                              gathered(P, idx_view(*pi), keep)));
    }
    case fb::JoinType_LeftSemi:
    case fb::JoinType_LeftAnti:
    case fb::JoinType_LeftMark: {
      auto bi = cudf::conditional_left_semi_join(Bc, Pc, a);
      s_->matched = set_true(s_->matched->view(), idx_view(*bi));
      return std::nullopt;
    }
    default: {  // RightSemi, RightAnti: anti is semi's complement, never the anti form
      auto pi = cudf::conditional_left_semi_join(Pc, Bc, a_swapped);
      auto hit = set_true(bools(P.num_rows(), false)->view(), idx_view(*pi));
      return projected(s_->d, kept_probe_rows(P, std::move(hit)));
    }
  }
}

/// `A` non-empty with something left over, and every Full: the conditional join makes the
/// candidate pairs, `R` is evaluated over them as columns, and the survivors take §3.4's
/// derivation. Chunked by the candidate count, as the keyed pairs path is.
std::optional<TableResult> JoinSession::probe_nested_candidates(TableResult const& P) {
  const auto row_bytes = filter_row_bytes(s_->map, s_->B.view(), P.view());
  ExprContext ctx;
  // One AST for every chunk: a column reference names an ordinal, not a table.
  auto const& a = cross_ast(s_->A, ctx, s_->map, false, s_->B.num_columns(), P.num_columns());
  auto Bc = s_->B_cond_view();
  const auto candidates = cudf::conditional_inner_join_size(
      Bc, cond_table(P.view(), fb::JoinSide_Right, s_->map, s_->hoist_probe).view(), a);
  auto ranges = chunks(candidates, row_bytes, P.num_rows(), s_->d->chunk_bytes());
  std::vector<TableResult> parts;
  for (auto [lo, hi] : ranges) {
    TableResult Pc = P.slice(lo, hi);
    auto Pcond = cond_table(Pc.view(), fb::JoinSide_Right, s_->map, s_->hoist_probe);
    // A sizer evaluates the condition over every pair, so one range reuses the count above.
    auto n =
        ranges.size() == 1 ? candidates : cudf::conditional_inner_join_size(Bc, Pcond.view(), a);
    fits_or_throw(n, "a chunk of nested-loop candidates");
    auto [bi, pi] = cudf::conditional_inner_join(Bc, Pcond.view(), a, n);
    keep_and_derive(Pc, idx_view(*bi), idx_view(*pi), s_->R, parts);
  }
  if (is_build_side_semi(s_->type)) return std::nullopt;
  return concat_rows(std::move(parts));
}

/// Nothing the AST can take: the chunked cross product of INDICES, never of rows, with `R`
/// over the pairs. The probe-major order is free -- a join's answer is a multiset.
std::optional<TableResult> JoinSession::probe_nested_cross(TableResult const& P) {
  const auto row_bytes = filter_row_bytes(s_->map, s_->B.view(), P.view());
  const cudf::size_type nb = s_->B.num_rows();
  const std::size_t pairs = static_cast<std::size_t>(nb) * static_cast<std::size_t>(P.num_rows());
  std::vector<TableResult> parts;
  for (auto [lo, hi] : chunks(pairs, row_bytes, P.num_rows(), s_->d->chunk_bytes())) {
    TableResult Pc = P.slice(lo, hi);
    const cudf::size_type nc = hi - lo;
    fits_or_throw(static_cast<std::size_t>(nb) * static_cast<std::size_t>(nc),
                  "a chunk of the nested loop's cross product");
    cudf::numeric_scalar<cudf::size_type> zero(0);
    auto pseq = cudf::sequence(nc, zero);
    auto bseq = cudf::sequence(nb, zero);
    auto pr = cudf::repeat(cudf::table_view{{pseq->view()}}, nb);
    auto br = cudf::tile(cudf::table_view{{bseq->view()}}, nc);
    keep_and_derive(Pc, br->get_column(0).view(), pr->get_column(0).view(), s_->R, parts);
  }
  if (is_build_side_semi(s_->type)) return std::nullopt;
  return concat_rows(std::move(parts));
}

/// §3.7, no keys and no condition. cuDF's own cross_join materializes the product once;
/// neither side has zero columns, which the constructor and `TableResult::owning` refuse,
/// so there is no rows-only arm.
std::optional<TableResult> JoinSession::probe_cross(TableResult P) {
  join::cross_rows_or_throw(s_->B.num_rows(), P.num_rows());
  if (P.num_rows() == 0)  // an empty build is `empty_build`, which answers above
    return projected(s_->d, side_by_side(s_->B.slice(0, 0), P.slice(0, 0)));
  // One table whose owners both halves share, so the projection selects rather than copies
  // and a dropped placeholder costs nothing (#207).
  auto out = TableResult::owning(cudf::cross_join(s_->B.view(), P.view()),
                                 concat_names(s_->B.column_names, P.column_names));
  return projected(s_->d, std::move(out));
}

/// `cs` over one chunk's candidate pairs, then §3.4's derivation over the survivors. The
/// keyed pairs path passes every cross conjunct, since the key match alone made the
/// candidates; the nested loop passes `R` alone, the AST having judged the rest already.
void JoinSession::keep_and_derive(TableResult const& Pc, cudf::column_view bi, cudf::column_view pi,
                                  std::vector<const fb::Expr*> const& cs,
                                  std::vector<TableResult>& parts) {
  auto mask = residual_mask(s_->B.view(), Pc.view(), bi, pi, cs, s_->map);
  auto kept = cudf::apply_boolean_mask(cudf::table_view{{bi, pi}}, mask->view());
  derive(Pc, kept->get_column(0).view(), kept->get_column(1).view(), parts);
}

std::optional<TableResult> JoinSession::finish() {
  if (s_->finished) throw std::invalid_argument("join_finish: the join is finished");
  s_->finished = true;
  switch (s_->type) {
    case fb::JoinType_Left:
    case fb::JoinType_Full: {
      // Read here rather than above: only these two pad, and a type that does not should not
      // be refused for a plan whose probe_schema it never needed.
      auto pad_types = s_->probe_types.empty() ? types_of(s_->d->probe_schema()) : s_->probe_types;
      auto pad_names = s_->probe_names.empty() ? names_of(s_->d->probe_schema()) : s_->probe_names;
      auto U = cudf::apply_boolean_mask(s_->B.view(), negate(s_->matched->view())->view());
      auto n = U->num_rows();
      return projected(s_->d, side_by_side(TableResult::owning(std::move(U), s_->B.column_names),
                                           null_table(pad_types, pad_names, n)));
    }
    case fb::JoinType_LeftSemi:
    case fb::JoinType_LeftAnti:
    case fb::JoinType_LeftMark: {
      // The preserved side's own conjuncts, over the build's rows: `matched AND R`.
      auto m = std::move(s_->matched);
      if (!s_->build_only.empty()) {
        auto R = eval_on(s_->build_only, s_->B.view(), fb::JoinSide_Left, s_->map);
        m = cudf::binary_operation(m->view(), R->view(), cudf::binary_operator::LOGICAL_AND, kBool);
      }
      // `with` shares B's column owners and owns the mark, so the build is not copied.
      if (s_->type == fb::JoinType_LeftMark)
        return projected(s_->d, s_->B.with(std::move(m), "mark"));
      auto mask = s_->type == fb::JoinType_LeftSemi ? std::move(m) : negate(m->view());
      return projected(s_->d,
                       TableResult::owning(cudf::apply_boolean_mask(s_->B.view(), mask->view()),
                                           s_->B.column_names));
    }
    default:
      return std::nullopt;
  }
}

}  // namespace peacock
