#pragma once

#include <cudf/table/table.hpp>

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

namespace peacock {

/// Result of executing a plan node: a cuDF table plus column names.
struct TableResult {
  std::unique_ptr<cudf::table> table;
  std::vector<std::string> column_names;
};

/// Execute a FlatBuffer-encoded GPU plan and return the result table.
///
/// @throws std::runtime_error on parse or execution errors.
TableResult execute_plan(const uint8_t* plan_bytes, uint64_t plan_len);

}  // namespace peacock
