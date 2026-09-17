// Union (UNION ALL / interleave) -- concatenate the rows of all inputs.

#include "peacock/operators.h"

#include <cudf/concatenate.hpp>
#include <cudf/table/table.hpp>

#include <stdexcept>

namespace peacock {

TableResult execute_union(const fb::CudfUnion* u, NodeInputs* in) {
  if (!u->inputs() || u->inputs()->size() == 0)
    throw std::runtime_error("CudfUnion has no inputs");

  // Execute each input fully, then concatenate the materialized tables.
  std::vector<TableResult> inputs;
  inputs.reserve(u->inputs()->size());
  for (flatbuffers::uoffset_t i = 0; i < u->inputs()->size(); ++i) {
    inputs.push_back(take_input(in));
  }

  // A single input needs no copy.
  if (inputs.size() == 1) return std::move(inputs[0]);

  std::vector<cudf::table_view> views;
  views.reserve(inputs.size());
  for (auto& in : inputs) views.push_back(in.table->view());

  auto out = cudf::concatenate(views);
  return {std::move(out), std::move(inputs[0].column_names)};
}


}  // namespace peacock
