//! The hold rule: for one reader of a shared load inside a hold, where a buffer would go, what
//! it would keep, what reading the table again would cost, and which of the two to do. The
//! rule is `decide`'s and `hold_inputs`'s doc comments in `mod.rs`.

#[cfg(test)]
mod tests;

use crate::plan::{GpuNode, NodeRef, PlanDag, as_node_ref, holds_state};

use super::{Baseline, Choice, EdgeCost, Fused, HeldReader, HoldDecision, HoldInputs};
use super::{HoldLedger, LoadTimes, budget_of, cap_inputs};

pub(crate) fn place_buffer(edges: &[EdgeCost]) -> Option<&EdgeCost> {
    edges
        .iter()
        .filter(|edge| edge.allowed)
        .min_by_key(|edge| edge.bytes)
}

pub(crate) fn decide(inputs: &HoldInputs, headroom_bytes: u64) -> HoldDecision {
    let decision = |choice, bytes, why: String| HoldDecision {
        choice,
        bytes,
        reread_us: inputs.reread_us,
        why,
    };
    match place_buffer(&inputs.edges) {
        Some(edge) if edge.bytes <= headroom_bytes => decision(
            Choice::Buffer { edge: edge.edge },
            edge.bytes,
            format!("fits: {} <= {headroom_bytes}", edge.bytes),
        ),
        Some(edge) => decision(
            Choice::ReRead,
            edge.bytes,
            format!("too large: {} > {headroom_bytes}", edge.bytes),
        ),
        None if inputs.edges.is_empty() => decision(
            Choice::ReRead,
            0,
            "no edge: the reader takes the shared load directly".to_string(),
        ),
        None => decision(
            Choice::ReRead,
            0,
            "no edge: each lies above a join that grew the rows, or above state a waiting root \
             may not keep"
                .to_string(),
        ),
    }
}

pub(crate) fn hold_inputs(
    fused: &Fused,
    held: &HeldReader,
    baseline: &Baseline,
    loads: &LoadTimes,
    seqs: &[Vec<Option<u64>>],
    names: &[String],
) -> HoldInputs {
    let roots: Vec<&dyn GpuNode> = fused.roots.iter().map(|root| root.as_ref()).collect();
    let dag = PlanDag::of(&roots);
    let member = held.member;
    // `(Σ batch_rows, Σ batch_bytes)` recorded for what a fused node stands for; the shared
    // load and a project selecting from it stand for the reader's own load.
    let recorded = |at: usize| {
        let origin = match fused.origins[at].as_slice() {
            [origin] if dag.nodes[at].node.sharing_key().is_none() => *origin,
            _ => member,
        };
        let times = &baseline.per_query[origin.query];
        (
            times.batch_rows[origin.node],
            times.batch_bytes[origin.node],
        )
    };
    let capped = dag.roots.contains(&held.hold);
    let (mut grown, mut builds, mut below) = (false, 0, None);
    let mut edges = Vec::new();
    for (parent, slot) in &held.path {
        let child = dag.nodes[*parent].children[*slot];
        if is_join(dag.nodes[child].node) {
            let (input, _) = recorded(below.expect("a join on the path reads the node below"));
            grown |= recorded(child).0 > input;
            builds += recorded(dag.nodes[child].children[0]).1;
        }
        below = Some(child);
        if dag.nodes[child].node.sharing_key().is_some() {
            continue;
        }
        edges.push(EdgeCost {
            edge: (*parent, *slot),
            bytes: recorded(child).1 + builds,
            allowed: !grown && !(capped && keeps_state(&dag, child)),
        });
    }
    let seq = seqs[member.query][member.node].expect("the reader's own load has a scan seq");
    HoldInputs {
        reread_us: loads.load_us(&names[member.query], seq),
        edges,
    }
}

fn is_join(node: &dyn GpuNode) -> bool {
    matches!(
        as_node_ref(node),
        NodeRef::Join(_) | NodeRef::CrossJoin(_) | NodeRef::NestedLoopJoin(_)
    )
}

/// Whether anything at or under `at` keeps state: what a waiting root may not have below its
/// buffer (`validate_cap`).
fn keeps_state(dag: &PlanDag<'_>, at: usize) -> bool {
    let mut stack = vec![at];
    while let Some(next) = stack.pop() {
        if holds_state(dag.nodes[next].node) {
            return true;
        }
        stack.extend(&dag.nodes[next].children);
    }
    false
}

pub(crate) fn choose(
    ledger: &mut HoldLedger<'_>,
    fused: &Fused,
    held: &HeldReader,
) -> HoldDecision {
    let charged = *ledger.charged.get_or_insert_with(|| {
        let at_first = cap_inputs(fused, ledger.baseline, ledger.pool_bytes);
        let largest_root = at_first.root_bytes.iter().max().copied().unwrap_or(0);
        at_first.shared_bytes.iter().sum::<u64>()
            + at_first.buffer_bytes.iter().sum::<u64>()
            + largest_root
    });
    let headroom = budget_of(ledger.pool_bytes).saturating_sub(charged + ledger.spent);
    let inputs = hold_inputs(
        fused,
        held,
        ledger.baseline,
        ledger.loads,
        ledger.seqs,
        ledger.names,
    );
    let decision = decide(&inputs, headroom);
    if let Choice::Buffer { .. } = decision.choice {
        ledger.spent += decision.bytes;
    }
    decision
}
