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

#include <cstdint>
#include <string>
#include <vector>

namespace peacock {
namespace fb = peacock::plan;

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

// The export behind peacock_result_from_handle: the table as an Arrow IPC stream (malloc'd;
// free with peacock_result_free) with each non-zero `decimal_precisions[i]` written as
// column i's precision. Throws, naming the column, on a fixed_point that is not DECIMAL128
// and on a precision for a column that is not a decimal. Reachable here so a test can hand
// it a table no C-API path can build — a narrow fixed_point column — and see the refusal.
void export_table_to_ipc(const cudf::table_view& tview,
                         const std::vector<std::string>& column_names,
                         const int32_t* decimal_precisions, uint8_t** out_bytes, uint64_t* out_len);

}  // namespace peacock
