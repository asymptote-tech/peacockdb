//! The pipelining hint: what overlapping loads with execution would save, as a schedule over
//! the baseline's load and execute times. The model is `pipeline_hint`'s.

#[cfg(test)]
mod tests;

use crate::plan::{GpuNode, NodeRef, PlanDag, as_node_ref};

use super::{Baseline, Fused, PipelineHint, RootNeeds};

pub(crate) fn pipeline_hint(load_us: &[u64], roots: &[RootNeeds]) -> PipelineHint {
    let load_order = greedy_load_order(load_us, roots);
    let mut loaded_at = vec![0; load_us.len()];
    let mut clock = 0;
    for source in &load_order {
        clock += load_us[*source];
        loaded_at[*source] = clock;
    }
    let ready = |root: &RootNeeds| {
        root.sources
            .iter()
            .map(|s| loaded_at[*s])
            .max()
            .unwrap_or(0)
    };
    let mut by_readiness: Vec<usize> = (0..roots.len()).collect();
    by_readiness.sort_by_key(|at| (ready(&roots[*at]), *at));
    let mut executor_free = 0;
    for at in by_readiness {
        executor_free = executor_free.max(ready(&roots[at])) + roots[at].execute_us;
    }
    let serial_us = load_us.iter().sum::<u64>() + roots.iter().map(|r| r.execute_us).sum::<u64>();
    let makespan_us = executor_free.max(clock);
    PipelineHint {
        load_order,
        serial_us,
        makespan_us,
        saved_us: serial_us.saturating_sub(makespan_us),
    }
}

/// Root by root, the one whose missing sources cost least to load next (ties by root order),
/// its missing sources appended in ordinal order.
fn greedy_load_order(load_us: &[u64], roots: &[RootNeeds]) -> Vec<usize> {
    let mut loaded = vec![false; load_us.len()];
    let mut waiting: Vec<usize> = (0..roots.len()).collect();
    let mut order = Vec::new();
    while !waiting.is_empty() {
        let missing = |root: usize| -> Vec<usize> {
            let mut sources: Vec<usize> = roots[root]
                .sources
                .iter()
                .copied()
                .filter(|s| !loaded[*s])
                .collect();
            sources.sort_unstable();
            sources.dedup();
            sources
        };
        let cost = |root: usize| missing(root).iter().map(|s| load_us[*s]).sum::<u64>();
        let at = (0..waiting.len())
            .min_by_key(|at| (cost(waiting[*at]), waiting[*at]))
            .expect("a root waits");
        let next = missing(waiting.remove(at));
        for source in next {
            loaded[source] = true;
            order.push(source);
        }
    }
    order
}

pub(crate) fn pipeline_inputs(fused: &Fused, baseline: &Baseline) -> (Vec<u64>, Vec<RootNeeds>) {
    let roots: Vec<&dyn GpuNode> = fused.roots.iter().map(|root| root.as_ref()).collect();
    let dag = PlanDag::of(&roots);
    let sources: Vec<usize> = (0..dag.nodes.len())
        .filter(|at| matches!(as_node_ref(dag.nodes[*at].node), NodeRef::LoadParquet(_)))
        .collect();
    let load_us = sources
        .iter()
        .map(|at| {
            fused.origins[*at]
                .iter()
                .map(|origin| baseline.per_query[origin.query].total_us[origin.node])
                .max()
                .expect("a load stands for a load of some query")
        })
        .collect();
    let needs = dag
        .roots
        .iter()
        .enumerate()
        .map(|(query, root)| RootNeeds {
            execute_us: baseline.per_query[query].execute_us,
            sources: reached(&dag, *root)
                .into_iter()
                .filter_map(|at| sources.iter().position(|s| *s == at))
                .collect(),
        })
        .collect();
    (load_us, needs)
}

/// The nodes under `root`, each once, ascending.
fn reached(dag: &PlanDag<'_>, root: usize) -> Vec<usize> {
    let mut seen = vec![false; dag.nodes.len()];
    let mut stack = vec![root];
    while let Some(at) = stack.pop() {
        if !std::mem::replace(&mut seen[at], true) {
            stack.extend(&dag.nodes[at].children);
        }
    }
    (0..seen.len()).filter(|at| seen[*at]).collect()
}
