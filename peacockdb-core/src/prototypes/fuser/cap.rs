//! The concurrency cap: how many roots run at once so the estimated live bytes stay under the
//! pool less a margin, its inputs off a fused plan and the baseline, and settling a batch so
//! every root the cap makes wait is buffered or re-reads. The rule is `cap_for`'s doc comment.

#[cfg(test)]
mod tests;

use std::num::NonZeroUsize;

use crate::plan::{GpuNode, NodeRef, PlanDag, PlanError, as_node_ref};

use super::resolve_cap_holds;
use super::{Baseline, CapDecision, CapInputs, Choice, Fused, HoldLedger, MARGIN_PERCENT};

pub(crate) fn budget_of(pool_bytes: u64) -> u64 {
    pool_bytes - pool_bytes * MARGIN_PERCENT / 100
}

pub(crate) fn cap_for(inputs: &CapInputs) -> CapDecision {
    let budget_bytes = budget_of(inputs.pool_bytes);
    let fixed: u64 =
        inputs.shared_bytes.iter().sum::<u64>() + inputs.buffer_bytes.iter().sum::<u64>();
    let mut roots = inputs.root_bytes.clone();
    roots.sort_unstable_by(|a, b| b.cmp(a));
    let mut estimate_bytes = fixed + roots.first().copied().unwrap_or(0);
    let mut cap = 1;
    for root in roots.iter().skip(1) {
        if estimate_bytes + root > budget_bytes {
            break;
        }
        estimate_bytes += root;
        cap += 1;
    }
    CapDecision {
        cap: NonZeroUsize::new(cap).expect("at least one root runs"),
        budget_bytes,
        estimate_bytes,
        over_budget: fixed + roots.first().copied().unwrap_or(0) > budget_bytes,
    }
}

pub(crate) fn cap_inputs(fused: &Fused, baseline: &Baseline, pool_bytes: u64) -> CapInputs {
    let roots: Vec<&dyn GpuNode> = fused.roots.iter().map(|root| root.as_ref()).collect();
    let dag = PlanDag::of(&roots);
    let recorded = |query: usize, node: usize| baseline.per_query[query].batch_bytes[node];
    let mut shared_bytes = Vec::new();
    let mut root_bytes: Vec<u64> = baseline.per_query.iter().map(|q| q.peak_bytes).collect();
    for at in (0..dag.nodes.len()).filter(|at| dag.nodes[*at].node.sharing_key().is_some()) {
        let members = &fused.origins[at];
        let largest = members
            .iter()
            .max_by_key(|member| recorded(member.query, member.node))
            .expect("a shared load stands for its members");
        shared_bytes.push(scaled_bytes(
            recorded(largest.query, largest.node),
            planned_bytes(fused.original(*largest)),
            planned_bytes(dag.nodes[at].node),
        ));
        for member in members {
            root_bytes[member.query] =
                root_bytes[member.query].saturating_sub(recorded(member.query, member.node));
        }
    }
    let buffer_bytes = fused
        .holds
        .iter()
        .filter(|record| matches!(record.decision.choice, Choice::Buffer { .. }))
        .map(|record| record.decision.bytes)
        .collect();
    CapInputs {
        pool_bytes,
        shared_bytes,
        buffer_bytes,
        root_bytes,
    }
}

/// What a load plans to read: its surviving row groups' bytes over its columns.
fn planned_bytes(node: &dyn GpuNode) -> u64 {
    match as_node_ref(node) {
        NodeRef::LoadParquet(load) => load.bytes(),
        _ => unreachable!("{} stands where a load does", node.name()),
    }
}

pub(crate) fn scaled_bytes(measured: u64, plan_from: u64, plan_to: u64) -> u64 {
    if plan_from == 0 {
        return measured;
    }
    (u128::from(measured) * u128::from(plan_to) / u128::from(plan_from)) as u64
}

pub(crate) fn settle(
    mut fused: Fused,
    baseline: &Baseline,
    ledger: &mut HoldLedger<'_>,
) -> Result<(Fused, CapDecision), PlanError> {
    let pool_bytes = ledger.pool_bytes;
    let mut decision = cap_for(&cap_inputs(&fused, baseline, pool_bytes));
    while decision.cap.get() < fused.roots.len() {
        fused = resolve_cap_holds(fused, decision.cap, &mut |f, h| ledger.choose(f, h))?;
        let next = cap_for(&cap_inputs(&fused, baseline, pool_bytes));
        // Resolved for a cap no larger than `next`'s, every root that waits under it is too.
        if next.cap >= decision.cap {
            return Ok((fused, next));
        }
        decision = next;
    }
    Ok((fused, decision))
}
