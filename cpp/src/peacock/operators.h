#pragma once
// PRIVATE header -- must stay under src/, NOT include/ (see plan_types.h).
//
// The per-operator entry points, non-static so each operator can live in its own
// translation unit under src/operators/.

#include "peacock/plan_types.h"
#include "plan_executor.h"

#include <cudf/utilities/span.hpp>

#include <vector>

namespace peacock {

// Explicit input channel, threaded through the call chain. Must stay a parameter, never
// ambient state: an anonymous-namespace thread_local forks when the file is split, so one
// half would read the other's inputs (coding-style.md has the case). A parameter also
// nests without save/restore.
//
// CONTRACT: children are already resident and are consumed positionally, in the same
// post-order the caller pushed them.
struct NodeInputs {
  std::vector<TableResult>* items = nullptr;
  size_t idx = 0;
};

// CudfScan is the one LEAF: it reads Parquet and takes no NodeInputs, which is why
// execute_one's consume-all invariant is trivially satisfied for zero-input nodes.
//
// Default argument on the DECLARATION only -- repeating it on the definition is a
// hard error.
// The override is a caller array rather than the node's own vector because the
// per-batch loader supplies one; EMPTY means "no override".
TableResult execute_scan(const fb::CudfScan* scan,
                         cudf::host_span<const uint32_t> row_groups_override = {});
TableResult execute_filter(const fb::CudfFilter* filter, NodeInputs* in);
TableResult execute_project(const fb::CudfProject* proj, NodeInputs* in);
TableResult execute_aggregate(const fb::CudfAggregate* agg, NodeInputs* in);
TableResult execute_hash_join(const fb::CudfHashJoin* join, NodeInputs* in);
TableResult execute_cross_join(const fb::CudfCrossJoin* join, NodeInputs* in);
TableResult execute_nested_loop_join(const fb::CudfNestedLoopJoin* join, NodeInputs* in);
TableResult execute_sort(const fb::CudfSort* sort, NodeInputs* in);
TableResult execute_union(const fb::CudfUnion* u, NodeInputs* in);
TableResult execute_limit(const fb::CudfLimit* limit, NodeInputs* in);
TableResult execute_window(const fb::CudfWindow* win, NodeInputs* in);

// Hands an operator its next already-resident input, consuming it. Every operator TU
// calls it to resolve its children, so it has to be header-declared; its companions
// run_op and plan_node_kind_name stay STATIC in dispatch.cpp — nothing outside that TU
// calls them, and they are one dispatch mechanism with it. It takes no node: which child
// is being resolved is the call's position, and NodeSession::execute_node is the one that
// executes a node.
TableResult take_input(NodeInputs* in);

// `inputs` BY VALUE: execute_one owns them for the duration of the call and hands
// a NodeInputs pointing at that local down the chain.
TableResult execute_one(const fb::PlanNode* node, std::vector<TableResult> inputs);

// Kept `inline` so it does not become a real call through the .so; per-node
// overhead is measured in llm-wiki/reports/benchmark-minimal.md.
inline TableResult execute_passthrough(NodeInputs* in) {
  return take_input(in);
}

}  // namespace peacock
