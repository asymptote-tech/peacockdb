#pragma once
//
// Internal declarations exposed solely so tests can reach them: the pure host-only
// helpers from src/expr.cpp that the CPU unit tests exercise without a GPU, and the
// session's own child order, which a test driving a hand-built plan node by node needs
// in order to hand each node its children's handles.
// NOT part of the stable FFI surface — do not depend on it outside tests.

#include "generated/gpu_plan_generated.h"

#include <cudf/column/column.hpp>
#include <cudf/table/table_view.hpp>
#include <cudf/types.hpp>

#include <cstdint>
#include <memory>
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
bool cudf_ast_can_evaluate(const fb::Expr* expr, cudf::table_view const& table);

/// What an expression evaluated to: a column the evaluation made, or one of the input
/// table's own columns, borrowed — a bare ColumnRef is the second. Copying it was #154's
/// costliest site: a whole column per batch on every predicate cuDF's AST refuses.
struct EvaluatedColumn {
  std::unique_ptr<cudf::column> owned;  // null when borrowed
  cudf::column_view borrowed;           // valid while the input table lives

  [[nodiscard]] cudf::column_view view() const { return owned ? owned->view() : borrowed; }

  /// Ownership, for a caller that keeps the column past the input: the made column, or a
  /// copy of the borrowed one — the one place a borrowed column is copied.
  [[nodiscard]] std::unique_ptr<cudf::column> take() && {
    return owned ? std::move(owned) : std::make_unique<cudf::column>(borrowed);
  }
};

// Evaluate an expression, borrowing the input's column for a bare ColumnRef (the non-AST
// path); the borrowed view is valid only while `table` lives. Declared here rather than in
// the private expr.h because ExitCopies.ABareColumnRefIsBorrowedNotCopied asserts the borrow
// directly — that nothing is allocated and the view is the input's own buffer — which no
// operator-level call can show.
EvaluatedColumn evaluate_column(const fb::Expr* expr, cudf::table_view const& table);

// Whether a harness range is open. Observable only so that push_harness_range's one-level
// rule can be tested — NvtxRanges.ASecondPushReplacesTheFirstRatherThanNesting is the only
// caller, since NVTX itself reports nothing back.
bool harness_range_is_open();

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
