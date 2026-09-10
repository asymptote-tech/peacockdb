//! Union and interleave: lane relabeling, and nothing else. Neither touches a row — the
//! branch type normalization the union kernel does inside the executor is a per-branch `GpuProject`
//! the planner inserts, which is what leaves these two as pure routing.

use super::{GpuInterleave, GpuUnion};
use std::any::Any;

use super::GpuNode;
use super::PlanError;
use super::Schema;
use super::input_layout;
use super::{BatchLayout, KeyDistribution, NodeKind, PartitionLayout, SortOrder};

impl GpuNode for GpuUnion {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        self.branches.iter().map(|b| b.as_ref()).collect()
    }

    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        check_branch_schemas("GpuUnion", self.kind.schema(), &self.branches)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl GpuNode for GpuInterleave {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        self.branches.iter().map(|b| b.as_ref()).collect()
    }

    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        check_branch_schemas("GpuInterleave", self.kind.schema(), &self.branches)?;
        let first = input_layout(self.branches[0].as_ref());
        for branch in &self.branches[1..] {
            let layout = input_layout(branch.as_ref());
            if layout.n != first.n || layout.key_distribution != first.key_distribution {
                return Err(PlanError::Invalid(
                    "GpuInterleave: lane p is lane p of every branch, so all of them must \
                     carry the same hash distribution — otherwise this is a GpuUnion"
                        .to_string(),
                ));
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The order every branch agrees on, or none. Routing does not touch a batch, so an order
/// that holds within each branch's batches holds within the output's.
fn agreed_sort_order(branches: &[Box<dyn GpuNode>]) -> SortOrder {
    let first = input_layout(branches.first().expect("union has branches").as_ref()).sort_order;
    let agreed = branches
        .iter()
        .all(|branch| input_layout(branch.as_ref()).sort_order == first);
    if agreed {
        first
    } else {
        SortOrder::NotSpecified
    }
}

fn agreed_batch_layout(branches: &[Box<dyn GpuNode>]) -> BatchLayout {
    if branches
        .iter()
        .all(|branch| input_layout(branch.as_ref()).batch_layout == BatchLayout::SingleBatch)
    {
        BatchLayout::SingleBatch
    } else {
        BatchLayout::MultipleBatches
    }
}

/// Routing cannot retype anything, so a branch whose columns differ from the declared
/// output is a missing cast rather than something the executor should fix up (#41).
fn check_branch_schemas(
    node: &str,
    declared: Option<&Schema>,
    branches: &[Box<dyn GpuNode>],
) -> Result<(), PlanError> {
    let declared = declared.expect("a union is not a sink");
    for (index, branch) in branches.iter().enumerate() {
        let schema = branch.kind().schema().expect("a sink cannot be a branch");
        if schema.fields.fields().len() != declared.fields.fields().len() {
            return Err(PlanError::Invalid(format!(
                "{node}: branch {index} has {} columns and the output declares {}",
                schema.fields.fields().len(),
                declared.fields.fields().len()
            )));
        }
        // Names as well as types: the declared names are what every `name@ordinal` check
        // above this node resolves against, so a branch naming its columns differently
        // makes one of the two readings wrong wherever they are compared.
        let mismatched = schema
            .fields
            .fields()
            .iter()
            .zip(declared.fields.fields().iter())
            .find(|(branch_field, out)| {
                branch_field.data_type() != out.data_type() || branch_field.name() != out.name()
            });
        if let Some((branch_field, out)) = mismatched {
            return Err(PlanError::Invalid(format!(
                "{node}: branch {index} emits {} as {:?} where the output declares {} as \
                 {:?} — the planner inserts a casting GpuProject on that branch",
                branch_field.name(),
                branch_field.data_type(),
                out.name(),
                out.data_type()
            )));
        }
    }
    Ok(())
}

pub(crate) fn new_union(branches: Vec<Box<dyn GpuNode>>, schema: Schema) -> GpuUnion {
    let n = branches.iter().map(|b| input_layout(b.as_ref()).n).sum();
    // Output lane k is one branch's lane, forwarded batch for batch, so an order and a
    // one-batch lane survive wherever every branch has them. The hash does not: lane k
    // means something different in each branch's numbering.
    let layout = PartitionLayout {
        n,
        key_distribution: KeyDistribution::NotSpecified,
        sort_order: agreed_sort_order(&branches),
        batch_layout: agreed_batch_layout(&branches),
    };
    GpuUnion {
        kind: NodeKind::Intermediate { layout, schema },
        branches,
    }
}

pub(crate) fn new_interleave(branches: Vec<Box<dyn GpuNode>>, schema: Schema) -> GpuInterleave {
    let first = input_layout(branches.first().expect("interleave has branches").as_ref());
    // Lane p holds every branch's lane p, so each batch is still whatever it was — an
    // order within a batch survives — but k branches make k batches out of one lane.
    let batch_layout = if branches.len() == 1 {
        agreed_batch_layout(&branches)
    } else {
        BatchLayout::MultipleBatches
    };
    let layout = PartitionLayout {
        n: first.n,
        key_distribution: first.key_distribution.clone(),
        sort_order: agreed_sort_order(&branches),
        batch_layout,
    };
    GpuInterleave {
        kind: NodeKind::Intermediate { layout, schema },
        branches,
    }
}
