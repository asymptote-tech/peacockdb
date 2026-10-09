#pragma once
// PRIVATE header -- must stay under src/, NOT include/ (see plan_types.h).
//
// The column-level pieces a join session's arms are assembled from
// (join-rewrite-design.md §3.0): index maps, the build-matched bit, typed pads, and the
// side-by-side output. Everything here is about columns and knows nothing of the session's
// state or of any join type's recipe.

#include "peacock/expr.h"
#include "plan_executor.h"

#include <cudf/column/column.hpp>
#include <cudf/copying.hpp>
#include <cudf/table/table.hpp>
#include <cudf/table/table_view.hpp>

#include <memory>
#include <string>
#include <vector>

namespace peacock {
namespace join {

inline const cudf::data_type kBool{cudf::type_id::BOOL8};

/// An index map as a column cuDF's gather and compaction calls can take.
cudf::column_view idx_view(rmm::device_uvector<cudf::size_type> const& v);

std::vector<std::string> concat_names(std::vector<std::string> a,
                                      std::vector<std::string> const& b);

/// [build..., probe...] as one handle, the two sides keeping their own owners.
TableResult side_by_side(TableResult build_part, TableResult probe_part);

/// `projection` over what a call emits. Absent keeps every column.
TableResult projected(const fb::CudfJoin* d, TableResult t);

TableResult gathered(TableResult const& t, cudf::column_view idx,
                     cudf::out_of_bounds_policy policy);

/// The parts of one call's answer as one table. A single part is returned as it is.
TableResult concat_rows(std::vector<TableResult> parts);
TableResult concat_rows(TableResult a, TableResult b);

std::unique_ptr<cudf::column> bools(cudf::size_type n, bool v);

/// col OR (row i appears in idx) -- how a build-matched bit is set.
std::unique_ptr<cudf::column> set_true(cudf::column_view col, cudf::column_view idx);

/// 0 <= idx < n: whether an index map's entry is a match rather than a sentinel.
std::unique_ptr<cudf::column> in_range(cudf::column_view idx, cudf::size_type n);

std::unique_ptr<cudf::column> negate(cudf::column_view c);

std::unique_ptr<cudf::column> null_column(cudf::data_type t, cudf::size_type n);

/// The only maker of padded columns.
TableResult null_table(std::vector<cudf::data_type> const& types,
                       std::vector<std::string> const& names, cudf::size_type n);

std::vector<cudf::data_type> types_of(cudf::table_view t);
std::vector<cudf::data_type> types_of(const fb::Schema* s);
std::vector<std::string> names_of(const fb::Schema* s);

/// The distinct keys `distinct_hash_join` is undefined without.
std::unique_ptr<cudf::table> distinct_keys(cudf::table_view K, cudf::null_equality cmp);

/// The three types that answer no table per probe and everything at finish.
bool is_build_side_semi(fb::JoinType t);
/// The two that answer per probe from the probe's own rows.
bool is_probe_side_semi(fb::JoinType t);
/// The four whose output is pairs of rows from both sides.
bool emits_pairs(fb::JoinType t);

// `fits_or_throw` and `cross_rows_or_throw`, the two size_type ceilings, are defined in
// join_columns.cpp and declared in plan_executor_internal.h, which is the route a test
// reaches them by; this header's includes make them visible to every consumer of it.

}  // namespace join
}  // namespace peacock
