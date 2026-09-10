#pragma once
//
// Internal declarations exposed solely so tests can reach them: the pure host-only
// helpers from src/expr.cpp that the CPU unit tests exercise without a GPU, and the
// session's own child order, which a test driving a hand-built plan node by node needs
// in order to hand each node its children's handles.
// NOT part of the stable FFI surface — do not depend on it outside tests.

#include "generated/gpu_plan_generated.h"

#include <cudf/table/table_view.hpp>
#include <cudf/types.hpp>

#include <memory>
#include <vector>

// Forward-declared rather than included: this header is read by a target that does not
// link Arrow, and a shared_ptr parameter needs no definition.
namespace arrow {
class Schema;
}

namespace peacock {
namespace fb = peacock::plan;

// The exported schema with each decimal's precision set to what the plan declared, or the
// schema unchanged where it cannot be: a width that disagrees, a non-decimal field, a 0, or
// a precision already equal. Defined in src/gpu_executor.cpp.
std::shared_ptr<arrow::Schema> with_declared_precision(
    const std::shared_ptr<arrow::Schema>& schema, const std::vector<int32_t>& precisions);

// Output cuDF type for a binary op: BOOL8 for predicates; for decimal
// arithmetic the DataFusion-matching fixed_point result scale (ADD/SUB take
// min(scale), MUL adds scales, DIV subtracts); otherwise the lhs type.
cudf::data_type binop_output_type(fb::BinaryOp op, cudf::data_type lhs,
                                  cudf::data_type rhs);

// Whether an expression can be evaluated through the cuDF AST fast path (true)
// or must take the column-producing path (false). Routes decimal operands and
// un-inferrable / mismatched-type binary ops to the column path.
bool is_ast_able(const fb::Expr* expr, cudf::table_view const& table);

// Children of a plan node in canonical order — the order NodeSession indexes
// post-order in, so a caller walking the tree with this produces the same seqs.
std::vector<const fb::PlanNode*> node_children(const fb::PlanNode* node);

}  // namespace peacock
