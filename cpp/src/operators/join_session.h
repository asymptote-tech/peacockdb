#pragma once
// PRIVATE header -- must stay under src/, NOT include/ (see plan_types.h).
//
// A join answered by calls rather than by execute_node: built once, probed per batch,
// finished once (join-rewrite-design.md §1-§3). `CudfJoin` is a plan leaf, so the tables
// arrive through NodeSession::join_build and join_probe.

#include "peacock/plan_types.h"
#include "plan_executor.h"

#include <cudf/column/column.hpp>
#include <cudf/table/table_view.hpp>

#include <memory>
#include <optional>
#include <vector>

// Declared, not included: the join headers moved between cuDF layouts and the
// __has_include switch for that lives in join_session.cpp. Both layouts declare these as
// non-template classes.
namespace cudf {
class hash_join;
class distinct_hash_join;
}  // namespace cudf

namespace peacock {

class JoinSession {
 public:
  JoinSession(const fb::CudfJoin* desc, std::optional<TableResult> build);
  ~JoinSession();
  JoinSession(const JoinSession&) = delete;
  JoinSession& operator=(const JoinSession&) = delete;

  /// One probe batch, consumed; nullopt for the types that answer only at finish.
  std::optional<TableResult> probe(TableResult batch);
  /// Once, after the last probe; nullopt for the types with no finish.
  std::optional<TableResult> finish();

 private:
  /// The residual's conjuncts by the sides they read, once, at construction.
  void split_residual();
  /// §3.6's `hoist`: one cross conjunct's one-side operands into condition columns, so the
  /// AST can take a comparison it otherwise refuses. False when it cannot be.
  bool hoist(const fb::Expr* conjunct, cudf::table_view types);
  /// The filter-column map with the sides swapped, built once and owned.
  void flip_map();
  /// §3.4's row-wise matcher for the semi family: one `mixed_*` call per batch.
  std::optional<TableResult> probe_mixed(TableResult const& batch);
  /// One probe batch against a build no row can match (§3.8).
  std::optional<TableResult> probe_empty_build(TableResult batch);
  /// §3.4's pairs path: the key matches, the residual over them, the per-type derivation,
  /// chunked by the scratch budget.
  std::optional<TableResult> probe_pairs(TableResult const& batch);
  /// The probe rows a RightSemi keeps, or a RightAnti's complement of them.
  TableResult kept_probe_rows(TableResult const& batch, std::unique_ptr<cudf::column> hit);
  /// §3.7, no keys and no condition.
  std::optional<TableResult> probe_cross(TableResult batch);
  /// §3.6, no keys: cuDF's conditional joins, or a chunked cross of indices.
  std::optional<TableResult> probe_nested(TableResult const& batch);
  std::optional<TableResult> probe_nested_rowwise(TableResult const& batch);
  std::optional<TableResult> probe_nested_candidates(TableResult const& batch);
  std::optional<TableResult> probe_nested_cross(TableResult const& batch);
  /// `conjuncts` over one chunk's candidate pairs, then the derivation over the survivors.
  void keep_and_derive(TableResult const& chunk, cudf::column_view bi, cudf::column_view pi,
                       std::vector<const fb::Expr*> const& conjuncts,
                       std::vector<TableResult>& parts);
  /// §3.4's per-type derivation over one chunk's surviving pairs.
  void derive(TableResult const& chunk, cudf::column_view bk, cudf::column_view pk,
              std::vector<TableResult>& parts);

  struct State;  // every cuDF object and column the session owns
  std::unique_ptr<State> s_;
};

}  // namespace peacock
