//! Reading the recipe plan back: the node at a seq, by the same post-order the C++
//! indexes with.
//!
//! Beside the writer rather than beside the renderer, because it is the other half of one
//! claim — a seq means the same node on both sides — and the check that it holds
//! ([`check_seq_kinds`]) is not a rendering concern.

use std::collections::HashSet;

use super::generated::peacock::plan as fb;

use super::RecipePlan;
use crate::plan::PlanError;

/// The node at `seq`, by the post-order the C++ indexes with. `None` where the tree is
/// shorter than `seq`; an error where the walk meets a node kind it does not know.
pub(crate) fn node_at<'a>(
    plan: &fb::GpuPlan<'a>,
    seq: u32,
) -> Result<Option<fb::PlanNode<'a>>, PlanError> {
    Ok(indexed(plan)?.get(seq as usize).copied())
}

/// Every node in seq order, as `NodeSession::index_post_order` indexes them: the roots in
/// order, children in `node_children` order, then the node — and a node several parents
/// hold, which is one offset in the buffer, only at its first visit.
pub(crate) fn indexed<'a>(plan: &fb::GpuPlan<'a>) -> Result<Vec<fb::PlanNode<'a>>, PlanError> {
    let mut seen = HashSet::new();
    let mut order = Vec::new();
    for root in roots_of(plan)? {
        index(root, &mut seen, &mut order)?;
    }
    Ok(order)
}

/// `roots` where it is set, else the one `root`: a plan setting both is refused, as the C++
/// refuses it.
fn roots_of<'a>(plan: &fb::GpuPlan<'a>) -> Result<Vec<fb::PlanNode<'a>>, PlanError> {
    match (plan.root(), plan.roots()) {
        (Some(_), Some(_)) => Err(PlanError::Invalid(
            "GpuPlan sets both root and roots".to_string(),
        )),
        (_, Some(roots)) => Ok(roots.iter().collect()),
        (root, None) => Ok(root.into_iter().collect()),
    }
}

fn index<'a>(
    node: fb::PlanNode<'a>,
    seen: &mut HashSet<usize>,
    order: &mut Vec<fb::PlanNode<'a>>,
) -> Result<(), PlanError> {
    if !seen.insert(node._tab.loc()) {
        return Ok(());
    }
    for child in children(&node)? {
        index(child, seen, order)?;
    }
    order.push(node);
    Ok(())
}

/// The child order `NodeSession::node_children` walks, which is what makes a seq mean the
/// same node on both sides.
///
/// Total, as the C++ twin is: every kind is named, a leaf included, and an unknown one is an
/// error rather than a leaf. FlatBuffers generates `PlanNodeKind` as a struct over a `u8`, so
/// the compiler cannot see a kind missing here; `the_child_walk_names_every_node_kind`
/// (`wire/tests.rs`) is what goes red when the schema gains one.
pub(crate) fn children<'a>(node: &fb::PlanNode<'a>) -> Result<Vec<fb::PlanNode<'a>>, PlanError> {
    Ok(match node.node_type() {
        fb::PlanNodeKind::CudfScan => Vec::new(),
        fb::PlanNodeKind::CudfFilter => one(node.node_as_cudf_filter().and_then(|n| n.input())),
        fb::PlanNodeKind::CudfProject => one(node.node_as_cudf_project().and_then(|n| n.input())),
        fb::PlanNodeKind::CudfAggregate => {
            one(node.node_as_cudf_aggregate().and_then(|n| n.input()))
        }
        fb::PlanNodeKind::CudfSort => one(node.node_as_cudf_sort().and_then(|n| n.input())),
        fb::PlanNodeKind::CudfCoalesceBatches => {
            one(node.node_as_cudf_coalesce_batches().and_then(|n| n.input()))
        }
        fb::PlanNodeKind::CudfCoalescePartitions => one(node
            .node_as_cudf_coalesce_partitions()
            .and_then(|n| n.input())),
        fb::PlanNodeKind::CudfRepartition => {
            one(node.node_as_cudf_repartition().and_then(|n| n.input()))
        }
        fb::PlanNodeKind::CudfSortPreservingMerge => one(node
            .node_as_cudf_sort_preserving_merge()
            .and_then(|n| n.input())),
        fb::PlanNodeKind::CudfLimit => one(node.node_as_cudf_limit().and_then(|n| n.input())),
        fb::PlanNodeKind::CudfWindow => one(node.node_as_cudf_window().and_then(|n| n.input())),
        // A leaf, as the C++ twin has it: a session's two tables arrive through
        // peacock_join_build and peacock_join_probe, so the node carries no child stubs.
        fb::PlanNodeKind::CudfJoin => Vec::new(),
        fb::PlanNodeKind::CudfUnion => node
            .node_as_cudf_union()
            .and_then(|n| n.inputs())
            .map(|inputs| inputs.iter().collect())
            .unwrap_or_default(),
        other => {
            return Err(PlanError::Invalid(format!(
                "the recipe plan holds a node kind the child walk does not know: {other:?} — \
                 name it in `wire/read.rs`'s `children`, in `NodeSession::node_children`'s order"
            )));
        }
    })
}

fn one<'a>(child: Option<fb::PlanNode<'a>>) -> Vec<fb::PlanNode<'a>> {
    child.into_iter().collect()
}

/// Every published seq resolves to a node, and to a node of the kind its recipe claims.
///
/// The two halves are separate failures. A seq that resolves to nothing means the tree is
/// shorter than the numbering — something the walk built was left unreachable, and every
/// seq above the gap addresses the wrong node. A seq that resolves to the wrong kind means
/// the claim and the buffer disagree about what a call runs, which no golden would catch:
/// both are rendered from the same recipe.
pub(crate) fn check_seq_kinds(plan: &RecipePlan) -> Result<(), PlanError> {
    // The depth the C++ verifier allows, since a recipe plan is a chain and is as deep as
    // it is long (#169).
    let options = flatbuffers::VerifierOptions {
        max_depth: 1024,
        ..Default::default()
    };
    let buffer = flatbuffers::root_with_opts::<fb::GpuPlan>(&options, plan.bytes())
        .map_err(|e| PlanError::Invalid(format!("the recipe plan does not verify: {e}")))?;
    let indexed = indexed(&buffer)?;
    for node in 0..plan.nodes() {
        let Some(recipe) = plan.get(node) else {
            continue;
        };
        for call in &recipe.calls {
            let Some((seq, kind)) = call.target else {
                continue;
            };
            let Some(written) = indexed.get(seq as usize) else {
                return Err(PlanError::Invalid(format!(
                    "seq #{seq} is published as {kind} and the plan has no node there — the \
                     numbering is longer than the tree, so every seq above it addresses the \
                     wrong node"
                )));
            };
            if written.node_type() != kind.wire_kind() {
                return Err(PlanError::Invalid(format!(
                    "seq #{seq} is published as {kind} and holds {:?}",
                    written.node_type()
                )));
            }
        }
    }
    Ok(())
}

/// How deep the recipe plan is. A chain of nodes is as deep as it is long, and the C++
/// verifier refuses a plan past its depth limit at `begin_plan` — the whole query, before
/// any call, which is #169. Read here rather than guessed so a test can watch the headroom.
pub(crate) fn depth(plan: &RecipePlan) -> Result<usize, PlanError> {
    let options = flatbuffers::VerifierOptions {
        max_depth: 1024,
        ..Default::default()
    };
    let buffer = flatbuffers::root_with_opts::<fb::GpuPlan>(&options, plan.bytes())
        .map_err(|e| PlanError::Invalid(format!("the recipe plan does not verify: {e}")))?;
    let mut deepest = 0;
    for root in roots_of(&buffer)? {
        deepest = deepest.max(depth_of(root)?);
    }
    match deepest {
        0 => Err(PlanError::Invalid(
            "the recipe plan has no root".to_string(),
        )),
        deepest => Ok(deepest),
    }
}

fn depth_of(node: fb::PlanNode<'_>) -> Result<usize, PlanError> {
    let mut deepest = 0;
    for child in children(&node)? {
        deepest = deepest.max(depth_of(child)?);
    }
    Ok(1 + deepest)
}
