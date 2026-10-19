#pragma once
// Hand-built inputs for operator gtests: device columns from host values and back, and the
// flat-buffer pieces an operator reads. An operator runs through execute_one with these as its
// resident inputs, as NodeSession hands them.

#include "generated/gpu_plan_generated.h"
#include "peacock/operators.h"
#include "plan_executor.h"

#include <cudf/column/column.hpp>
#include <cudf/table/table.hpp>
#include <cudf/utilities/default_stream.hpp>
#include <cudf/utilities/type_dispatcher.hpp>
#include <rmm/device_buffer.hpp>

#include <cuda_runtime.h>
#include <flatbuffers/flatbuffers.h>

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

namespace hand {
namespace fb = peacock::plan;

template <typename T>
std::unique_ptr<cudf::column> column_of(std::vector<T> const& values) {
  rmm::device_buffer data(values.data(), values.size() * sizeof(T), cudf::get_default_stream());
  return std::make_unique<cudf::column>(cudf::data_type{cudf::type_to_id<T>()},
                                        static_cast<cudf::size_type>(values.size()),
                                        std::move(data), rmm::device_buffer{}, 0);
}

template <typename T>
std::vector<T> values_of(cudf::column_view const& column) {
  std::vector<T> host(column.size());
  cudaMemcpy(host.data(), column.data<T>(), host.size() * sizeof(T), cudaMemcpyDeviceToHost);
  return host;
}

inline peacock::TableResult table_of(std::vector<std::unique_ptr<cudf::column>> columns,
                                     std::vector<std::string> names) {
  return peacock::TableResult::owning(std::make_unique<cudf::table>(std::move(columns)),
                                      std::move(names));
}

/// A ColumnRef carrying its column's name.
inline flatbuffers::Offset<fb::Expr> ref(flatbuffers::FlatBufferBuilder& fbb, uint32_t index,
                                         const char* name) {
  auto text = fbb.CreateString(name);
  return fb::CreateExpr(fbb, fb::ExprNode_ColumnRef, fb::CreateColumnRef(fbb, index, text).Union());
}

inline flatbuffers::Offset<fb::Expr> null_of(flatbuffers::FlatBufferBuilder& fbb,
                                             fb::DataType type) {
  fb::ScalarValueBuilder sb(fbb);
  sb.add_type(type);
  sb.add_is_null(true);
  auto value = sb.Finish();
  return fb::CreateExpr(fbb, fb::ExprNode_LiteralExpr, fb::CreateLiteralExpr(fbb, value).Union());
}

/// `node` as the root of `fbb`'s plan, run once over `inputs`.
inline peacock::TableResult run(flatbuffers::FlatBufferBuilder& fbb,
                                flatbuffers::Offset<fb::PlanNode> node,
                                std::vector<peacock::TableResult> inputs) {
  fbb.Finish(fb::CreateGpuPlan(fbb, node));
  return peacock::execute_one(fb::GetGpuPlan(fbb.GetBufferPointer())->root(), std::move(inputs));
}

}  // namespace hand
