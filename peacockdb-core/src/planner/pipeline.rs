//! Planning a query in this mode: DataFusion's physical plan in, a node tree and its
//! memory model out.
//!
//! Two passes, because the two halves define each other — a batch size needs the tree it
//! flows through, and the tree's mapping needs a batch size. The first pass assumes one
//! number for every source and is used only to derive the real ones; the second is the
//! plan. The tree's shape does not move between them: lane counts come from row counts
//! and pushed-down limits, never from the batch size.

use std::sync::Arc;

use datafusion::physical_plan::ExecutionPlan;

use super::translator::Translator;
use super::{BatchSizing, MemoryModel, PlanKnobs, estimate, refuse_null_unsafe_joins};
use crate::plan::Batching;
use crate::plan::GpuNode;
use crate::plan::PlanError;
use crate::plan::{check_output_schema, validate};

pub(crate) fn plan(
    root: &Arc<dyn ExecutionPlan>,
    knobs: PlanKnobs,
) -> Result<(Box<dyn GpuNode>, MemoryModel), PlanError> {
    let translator = |targets: Vec<u64>| {
        Translator::new(knobs.target_partitions, seed_batching(knobs))
            .with_small_table_bytes(knobs.small_table_bytes)
            .with_source_targets(targets)
    };

    let first = translator(Vec::new()).translate(root)?;
    if knobs.sizing != BatchSizing::Budgeted {
        checked(first, root, knobs.budget)
    } else {
        // Post-order sequence rises left to right across the sources, which is the order
        // translation reaches them, so sorting by it is the mapping between the two passes.
        let derived = {
            validate(first.as_ref())?;
            estimate(first.as_ref(), knobs.budget)?
        };
        let targets: Vec<u64> = derived
            .sources
            .iter()
            .map(|source| source.target_batch_bytes)
            .collect();
        let expected = targets.len();
        let second = translator(targets);
        let tree = second.translate(root)?;
        // The two passes map to each other by that order alone, so a second pass reaching
        // a source the first did not would plan its tail at the seed size — a different
        // plan from the one the estimator priced, arrived at silently.
        if second.sources_reached() != expected {
            return Err(PlanError::Invalid(format!(
                "the sizing pass reached {} sources and the planning pass {} — the two \
                 passes address sources by the order they are reached, so they no longer \
                 describe the same plan",
                expected,
                second.sources_reached()
            )));
        }
        checked(tree, root, knobs.budget)
    }
}

/// Validation before the model: the estimator walks the same tree and reads a node's
/// declarations as facts, so a malformed tree meets a message naming the fix rather than
/// an `expect` inside the walk.
fn checked(
    tree: Box<dyn GpuNode>,
    root: &Arc<dyn ExecutionPlan>,
    budget: u64,
) -> Result<(Box<dyn GpuNode>, MemoryModel), PlanError> {
    validate(tree.as_ref())?;
    check_output_schema(tree.as_ref(), &root.schema())?;
    refuse_null_unsafe_joins(tree.as_ref())?;
    let model = estimate(tree.as_ref(), budget)?;
    Ok((tree, model))
}

/// What the first pass assumes. Two of the three forms are the plan already; only the
/// budgeted one needs a number to start from, and the smallest the mapping can express is
/// the one that assumes least.
fn seed_batching(knobs: PlanKnobs) -> Batching {
    match knobs.sizing {
        BatchSizing::OneBatchPerLane => Batching::Off,
        BatchSizing::OneBatchPerRowGroup => Batching::PerRowGroup,
        BatchSizing::Budgeted => Batching::Sized {
            target_batch_bytes: super::MIN_TARGET_BATCH_BYTES as usize,
        },
    }
}
