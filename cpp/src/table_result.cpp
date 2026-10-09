// TableResult's four constructors (plan_executor.h): every handle's table is built by one
// of these, so the owner-per-column rule and its refusals live in one place.

#include "plan_executor.h"

#include <cudf/copying.hpp>

#include <stdexcept>
#include <string>

namespace peacock {

TableResult TableResult::owning(std::unique_ptr<cudf::table> table,
                                std::vector<std::string> names) {
  if (!table || table->num_columns() == 0)
    throw std::runtime_error(
        "TableResult: a table of no columns reads as no rows; the plan's placeholder "
        "column exists so that none is ever made");
  if (names.size() != static_cast<size_t>(table->num_columns()))
    throw std::runtime_error("TableResult: " + std::to_string(names.size()) + " names for " +
                             std::to_string(table->num_columns()) + " columns");
  TableResult out;
  out.column_names = std::move(names);
  out.columns.reserve(static_cast<size_t>(table->num_columns()));
  out.owners.reserve(static_cast<size_t>(table->num_columns()));
  for (auto& column : table->release()) {
    std::shared_ptr<cudf::column const> owner{std::move(column)};
    out.columns.push_back(owner->view());
    out.owners.push_back(std::move(owner));
  }
  return out;
}

TableResult TableResult::slice(cudf::size_type begin, cudf::size_type end) const {
  TableResult out;
  out.owners = owners;
  out.column_names = column_names;
  out.columns.reserve(columns.size());
  for (auto const& column : columns)
    out.columns.push_back(cudf::slice(column, {begin, end}).front());
  return out;
}

/// An ordinal may repeat: the two entries share one owner and both read the same column,
/// which is why an operator's fused projection selects rather than moving a column twice.
TableResult TableResult::select(std::vector<cudf::size_type> const& ordinals) const {
  TableResult out;
  for (auto i : ordinals) {
    if (i < 0 || i >= num_columns())
      throw std::runtime_error("TableResult::select: ordinal " + std::to_string(i) + " of " +
                               std::to_string(num_columns()) + " columns");
    out.owners.push_back(owners[i]);
    out.columns.push_back(columns[i]);
    out.column_names.push_back(column_names[i]);
  }
  if (out.columns.empty())
    throw std::runtime_error("TableResult::select: no ordinals is a table of no columns");
  return out;
}

TableResult TableResult::with(std::unique_ptr<cudf::column> column, std::string name) const {
  if (column->size() != num_rows())
    throw std::runtime_error("TableResult::with: a column of " + std::to_string(column->size()) +
                             " rows beside " + std::to_string(num_rows()));
  TableResult out = *this;
  std::shared_ptr<cudf::column const> owner{std::move(column)};
  out.columns.push_back(owner->view());
  out.owners.push_back(std::move(owner));
  out.column_names.push_back(std::move(name));
  return out;
}

}  // namespace peacock
