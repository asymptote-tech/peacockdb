// A ColumnRef resolved against the table it reads (#164): by index AND name, as the planner's
// own check does (expr_physical.rs). Every ColumnRef reader in cpp/src comes through here.

#include "plan_executor_internal.h"

#include <stdexcept>
#include <string>

namespace peacock {

cudf::size_type column_ordinal(const fb::ColumnRef* ref, std::vector<std::string> const& names) {
  const auto index = ref->index();
  const std::string name = ref->name() ? ref->name()->str() : std::string();
  if (index >= names.size())
    throw std::runtime_error("ColumnRef " + name + "@" + std::to_string(index) + " is past the " +
                             std::to_string(names.size()) + " columns its input has");
  if (!ref->name())
    throw std::runtime_error("ColumnRef @" + std::to_string(index) +
                             " carries no name, so nothing checks it reads the column the plan "
                             "meant");
  if (name != names[index])
    throw std::runtime_error("ColumnRef " + name + "@" + std::to_string(index) + " reads " +
                             names[index] + " at that position");
  return static_cast<cudf::size_type>(index);
}

cudf::column_view column_at(cudf::table_view const& table, std::vector<std::string> const& names,
                            const fb::ColumnRef* ref) {
  return table.column(column_ordinal(ref, names));
}

}  // namespace peacock
