// The plan-node dispatch switch (run_op), the input resolver every operator calls for
// its children (execute_node) and the single-node driver (execute_one). Co-located
// deliberately: they are one dispatch mechanism.

#include "peacock/operators.h"
#include "peacock/expr.h"

#include <cudf/table/table.hpp>

#include <stdexcept>
#include <string>
#include <vector>

namespace peacock {

static const char* plan_node_kind_name(fb::PlanNodeKind k) {
  switch (k) {
    case fb::PlanNodeKind_CudfScan:                return "CudfScan";
    case fb::PlanNodeKind_CudfFilter:              return "CudfFilter";
    case fb::PlanNodeKind_CudfProject:             return "CudfProject";
    case fb::PlanNodeKind_CudfAggregate:           return "CudfAggregate";
    case fb::PlanNodeKind_CudfHashJoin:            return "CudfHashJoin";
    case fb::PlanNodeKind_CudfCrossJoin:           return "CudfCrossJoin";
    case fb::PlanNodeKind_CudfNestedLoopJoin:      return "CudfNestedLoopJoin";
    case fb::PlanNodeKind_CudfSort:                return "CudfSort";
    case fb::PlanNodeKind_CudfCoalesceBatches:     return "CudfCoalesceBatches";
    case fb::PlanNodeKind_CudfCoalescePartitions:  return "CudfCoalescePartitions";
    case fb::PlanNodeKind_CudfRepartition:         return "CudfRepartition";
    case fb::PlanNodeKind_CudfSortPreservingMerge: return "CudfSortPreservingMerge";
    case fb::PlanNodeKind_CudfUnion:               return "CudfUnion";
    case fb::PlanNodeKind_CudfLimit:               return "CudfLimit";
    case fb::PlanNodeKind_CudfWindow:              return "CudfWindow";
    default:                                       return "Unknown";
  }
}


// Run one node's op (the dispatch switch). Each op resolves its children through
// `execute_node`, which hands back an input the caller already made resident.
static TableResult run_op(const fb::PlanNode* node, NodeInputs* in) {
  if (!node) throw std::runtime_error("null PlanNode");

  const char* kind = plan_node_kind_name(node->node_type());
  PCK_TRACE("enter %s", kind);

  TableResult result;
  try {
    switch (node->node_type()) {
      case fb::PlanNodeKind_CudfScan:
        result = execute_scan(node->node_as_CudfScan()); break;
      case fb::PlanNodeKind_CudfFilter:
        result = execute_filter(node->node_as_CudfFilter(), in); break;
      case fb::PlanNodeKind_CudfProject:
        result = execute_project(node->node_as_CudfProject(), in); break;
      case fb::PlanNodeKind_CudfAggregate:
        result = execute_aggregate(node->node_as_CudfAggregate(), in); break;
      case fb::PlanNodeKind_CudfHashJoin:
        result = execute_hash_join(node->node_as_CudfHashJoin(), in); break;
      case fb::PlanNodeKind_CudfCrossJoin:
        result = execute_cross_join(node->node_as_CudfCrossJoin(), in); break;
      case fb::PlanNodeKind_CudfNestedLoopJoin:
        result = execute_nested_loop_join(node->node_as_CudfNestedLoopJoin(), in); break;
      case fb::PlanNodeKind_CudfSort:
        result = execute_sort(node->node_as_CudfSort(), in); break;
      case fb::PlanNodeKind_CudfCoalesceBatches:
        result = execute_passthrough(node->node_as_CudfCoalesceBatches()->input(), in); break;
      case fb::PlanNodeKind_CudfCoalescePartitions:
        result = execute_passthrough(node->node_as_CudfCoalescePartitions()->input(), in); break;
      case fb::PlanNodeKind_CudfRepartition:
        result = execute_passthrough(node->node_as_CudfRepartition()->input(), in); break;
      case fb::PlanNodeKind_CudfSortPreservingMerge:
        result = execute_passthrough(node->node_as_CudfSortPreservingMerge()->input(), in); break;
      case fb::PlanNodeKind_CudfUnion:
        result = execute_union(node->node_as_CudfUnion(), in); break;
      case fb::PlanNodeKind_CudfLimit:
        result = execute_limit(node->node_as_CudfLimit(), in); break;
      case fb::PlanNodeKind_CudfWindow:
        result = execute_window(node->node_as_CudfWindow(), in); break;
      default:
        throw std::runtime_error(
            "unsupported PlanNodeKind: " + std::to_string(node->node_type()));
    }
  } catch (const std::exception& e) {
    std::string msg = e.what();
    if (msg.find("[in ") == std::string::npos) {
      throw std::runtime_error(std::string("[in ") + kind + "] " + msg);
    }
    throw;
  }

  debug_sync(kind);
  if (debug_enabled()) {
    auto tv = result.table->view();
    PCK_TRACE("leave %s rows=%d cols=%d", kind, tv.num_rows(), tv.num_columns());
  }
  return result;
}

// One operator's next input. `node` is unread: it names the child in the plan, and what
// comes back is the table the caller already put there — which is the whole of what a
// node-by-node driver means. It stays a parameter because the passthrough arms have
// nothing else to name the child they forward.
TableResult execute_node(const fb::PlanNode* node, NodeInputs* in) {
  (void)node;
  if (!in || !in->items) {
    throw std::runtime_error("execute_node: no inputs were provided for this node");
  }
  if (in->idx >= in->items->size()) {
    throw std::runtime_error("execute_one: not enough input handles for node");
  }
  return std::move((*in->items)[in->idx++]);
}

TableResult execute_one(const fb::PlanNode* node, std::vector<TableResult> inputs) {
  const size_t provided = inputs.size();
  NodeInputs in{&inputs, 0};
  TableResult result = run_op(node, &in);

  // INVARIANT: a node handed inputs must consume ALL of them.
  //
  // Under-consumption means an operator did not resolve one of its children, so the
  // table the caller made resident for it is dropped and the node ran against
  // whatever it did read — an answer computed from the wrong inputs.
  //
  // Safe for every dispatch case: scans are leaves (provided == 0) and every other
  // op resolves its children unconditionally.
  //
  // Deliberately `!= provided`, NOT `> 0 && idx == 0`: the weaker form covers only
  // single-child ops. A hash join threading `in` to its left child but nullptr to
  // its right consumes 1 of 2 and passes the weak check.
  if (in.idx != provided) {
    throw std::runtime_error(
        "execute_one: node was given " + std::to_string(provided) +
        " input(s) but consumed " + std::to_string(in.idx) +
        " — an operator did not resolve one of its children, so it ran against inputs "
        "the caller did not give it.");
  }
  return result;
}

}  // namespace peacock
