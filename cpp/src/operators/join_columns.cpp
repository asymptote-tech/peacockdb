// The column-level pieces of a join (join-rewrite-design.md §3.0). See join_columns.h.

#include "operators/join_columns.h"

#include <cudf/binaryop.hpp>
#include <cudf/column/column_factories.hpp>
#include <cudf/concatenate.hpp>
#include <cudf/filling.hpp>
#include <cudf/scalar/scalar_factories.hpp>
#include <cudf/search.hpp>
#include <cudf/stream_compaction.hpp>
#include <cudf/unary.hpp>

#include <limits>
#include <numeric>
#include <stdexcept>
#include <string>

namespace peacock {
namespace join {

/// One cuDF table holds at most size_type rows; a pair count past that would wrap silently
/// into a short table (design §3.9). Every path that makes pairs or an output calls this
/// first, with the sizer's count, and names the join in the refusal.
void fits_or_throw(std::size_t n, char const* what) {
  if (n > static_cast<std::size_t>(std::numeric_limits<cudf::size_type>::max()))
    throw std::runtime_error(std::string("CudfJoin: ") + what + " makes " + std::to_string(n) +
                             " rows, past what one cuDF table holds");
}

/// The cross product's own ceiling, counted in int64 so the product itself cannot wrap.
cudf::size_type cross_rows_or_throw(std::int64_t nb, std::int64_t np) {
  if (nb * np > std::numeric_limits<cudf::size_type>::max())
    throw std::runtime_error("CudfJoin: a cross join of " + std::to_string(nb) + " x " +
                             std::to_string(np) + " rows exceeds one table");
  return static_cast<cudf::size_type>(nb * np);
}

cudf::column_view idx_view(rmm::device_uvector<cudf::size_type> const& v) {
  fits_or_throw(v.size(), "an index map");  // the cast below would otherwise wrap
  return cudf::column_view{cudf::data_type{cudf::type_id::INT32},
                           static_cast<cudf::size_type>(v.size()), v.data(), nullptr, 0};
}

std::vector<std::string> concat_names(std::vector<std::string> a,
                                      std::vector<std::string> const& b) {
  a.insert(a.end(), b.begin(), b.end());
  return a;
}

/// [build..., probe...] as one handle. Assembled by hand through the public fields, as
/// project.cpp is: no TableResult constructor combines two tables, and both sides keep
/// their own owners, so nothing is copied.
TableResult side_by_side(TableResult build_part, TableResult probe_part) {
  TableResult t = std::move(build_part);
  t.owners.insert(t.owners.end(), probe_part.owners.begin(), probe_part.owners.end());
  t.columns.insert(t.columns.end(), probe_part.columns.begin(), probe_part.columns.end());
  t.column_names = concat_names(std::move(t.column_names), probe_part.column_names);
  return t;
}

/// `projection` over what a call emits -- the two sides for the pair types, one side for
/// the semi family. Absent keeps every column; present names the ones to keep. A present
/// empty one is refused at construction, so there is no arm for it here.
TableResult projected(const fb::CudfJoin* d, TableResult t) {
  if (!d->projection()) return t;
  std::vector<cudf::size_type> ords(d->projection()->begin(), d->projection()->end());
  return t.select(ords);
}

TableResult gathered(TableResult const& t, cudf::column_view idx,
                     cudf::out_of_bounds_policy policy) {
  return TableResult::owning(cudf::gather(t.view(), idx, policy), t.column_names);
}

/// The parts of one call's answer as one table -- the chunk loop's outputs, or a chunk's
/// matched rows above its padded ones. A single part is returned as it is.
TableResult concat_rows(std::vector<TableResult> parts) {
  if (parts.size() == 1) return std::move(parts.front());
  std::vector<cudf::table_view> views;
  views.reserve(parts.size());
  for (auto const& p : parts) views.push_back(p.view());
  return TableResult::owning(cudf::concatenate(views), parts.front().column_names);
}

TableResult concat_rows(TableResult a, TableResult b) {
  std::vector<TableResult> parts;
  parts.push_back(std::move(a));
  parts.push_back(std::move(b));
  return concat_rows(std::move(parts));
}

std::unique_ptr<cudf::column> bools(cudf::size_type n, bool v) {
  cudf::numeric_scalar<bool> s(v);
  return cudf::make_column_from_scalar(s, n);
}

/// col OR (row i appears in idx). `contains`, not a scatter: idx repeats a row once per
/// match, and cuDF leaves a scatter map with duplicate indices undefined (copying.hpp).
/// idx holds in-range rows only.
std::unique_ptr<cudf::column> set_true(cudf::column_view col, cudf::column_view idx) {
  cudf::numeric_scalar<cudf::size_type> zero(0);
  auto rows = cudf::sequence(col.size(), zero);
  auto hit = cudf::contains(idx, rows->view());
  return cudf::binary_operation(col, hit->view(), cudf::binary_operator::LOGICAL_OR, kBool);
}

/// 0 <= idx < n. The unmatched value is "an unspecified out-of-bounds value" for
/// hash_join::left_join and conditional_left_join on 25.02; only distinct_hash_join
/// promises JoinNoneValue, so a match is found by range and never by comparing a sentinel.
std::unique_ptr<cudf::column> in_range(cudf::column_view idx, cudf::size_type n) {
  cudf::numeric_scalar<cudf::size_type> lo(0), hi(n);
  auto ge = cudf::binary_operation(idx, lo, cudf::binary_operator::GREATER_EQUAL, kBool);
  auto lt = cudf::binary_operation(idx, hi, cudf::binary_operator::LESS, kBool);
  return cudf::binary_operation(ge->view(), lt->view(), cudf::binary_operator::LOGICAL_AND, kBool);
}

std::unique_ptr<cudf::column> negate(cudf::column_view c) {
  return cudf::unary_operation(c, cudf::unary_operator::NOT);
}

std::unique_ptr<cudf::column> null_column(cudf::data_type t, cudf::size_type n) {
  auto s = cudf::make_default_constructed_scalar(t);
  s->set_valid_async(false);
  return cudf::make_column_from_scalar(*s, n);
}

/// The only maker of padded columns, and it builds no literal, so the typed-null defects
/// of the expression path cannot reach a pad. Types come from the side's own table when
/// the session has seen one and from the plan's schema otherwise, so a pad always matches
/// what the matched rows carry.
TableResult null_table(std::vector<cudf::data_type> const& types,
                       std::vector<std::string> const& names, cudf::size_type n) {
  std::vector<std::unique_ptr<cudf::column>> cols;
  for (auto t : types) cols.push_back(null_column(t, n));
  return TableResult::owning(std::make_unique<cudf::table>(std::move(cols)), names);
}

std::vector<cudf::data_type> types_of(cudf::table_view t) {
  std::vector<cudf::data_type> out;
  for (auto const& c : t) out.push_back(c.type());
  return out;
}

std::vector<cudf::data_type> types_of(const fb::Schema* s) {
  std::vector<cudf::data_type> out;
  if (!s || !s->fields()) throw std::runtime_error("CudfJoin: a side with no schema to pad from");
  for (auto const* f : *s->fields()) {
    auto id = fb_to_type_id(f->data_type());
    if (id == cudf::type_id::EMPTY)
      throw std::runtime_error("CudfJoin: schema field " + f->name()->str() + " has no cuDF type");
    out.push_back(id == cudf::type_id::DECIMAL128
                      ? cudf::data_type{id, -static_cast<int32_t>(f->decimal_scale())}
                      : cudf::data_type{id});
  }
  return out;
}

std::vector<std::string> names_of(const fb::Schema* s) {
  std::vector<std::string> out;
  if (!s || !s->fields()) throw std::runtime_error("CudfJoin: a side with no schema to pad from");
  for (auto const* f : *s->fields()) out.push_back(f->name()->str());
  return out;
}

/// The build's distinct keys, which `distinct_hash_join` is undefined without. A NULL key
/// matches nothing under UNEQUAL, so its rows go before the dedupe rather than hashing a
/// NULL that nothing may match.
std::unique_ptr<cudf::table> distinct_keys(cudf::table_view K, cudf::null_equality cmp) {
  std::vector<cudf::size_type> all(K.num_columns());
  std::iota(all.begin(), all.end(), 0);
  std::unique_ptr<cudf::table> nonnull;
  cudf::table_view src = K;
  if (cmp == cudf::null_equality::UNEQUAL) {
    nonnull = cudf::drop_nulls(K, all);
    src = nonnull->view();
  }
  return cudf::distinct(src, all, cudf::duplicate_keep_option::KEEP_ANY, cmp,
                        cudf::nan_equality::ALL_EQUAL);
}

bool is_build_side_semi(fb::JoinType t) {
  return t == fb::JoinType_LeftSemi || t == fb::JoinType_LeftAnti || t == fb::JoinType_LeftMark;
}

bool is_probe_side_semi(fb::JoinType t) {
  return t == fb::JoinType_RightSemi || t == fb::JoinType_RightAnti;
}

bool emits_pairs(fb::JoinType t) {
  return t == fb::JoinType_Inner || t == fb::JoinType_Left || t == fb::JoinType_Right ||
         t == fb::JoinType_Full;
}

}  // namespace join
}  // namespace peacock
