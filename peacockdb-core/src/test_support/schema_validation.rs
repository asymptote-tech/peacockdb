//! Every batch a node emits, held to the schema its node declares: the driver's output
//! hook composed from the device-schema comparator. The device corpus installs the gpu
//! flavour on every row that says `schema_validation_enabled`; the cpu flavour is the same
//! comparison over an arrow batch, for the end-to-end tier and for symmetry, since the CPU
//! backend's `declared_as` already holds every stage on its own.

#[cfg(not(feature = "rust-only"))]
use crate::executor::GpuBackend;
use crate::executor::{CpuBackend, OutputHook, PlanIndex};

use super::{DeviceSchema, device_schema};

/// The sink is the one node with no schema, and its host batches never reach a hook of
/// this type — so a node the driver offers a batch for always declares one.
fn held_to_declaration(
    index: &PlanIndex<'_>,
    node: usize,
    actual: &DeviceSchema,
) -> Result<(), String> {
    let node = index.nodes[node].node;
    let declared = node
        .kind()
        .schema()
        .unwrap_or_else(|| panic!("{} emitted a batch and declares no schema", node.name()));
    match device_schema::device_divergence(&declared.fields, actual) {
        Some(divergence) => Err(divergence),
        None => Ok(()),
    }
}

#[cfg(not(feature = "rust-only"))]
pub(crate) fn gpu_schema_validator<'a>(index: &'a PlanIndex<'a>) -> OutputHook<'a, GpuBackend> {
    Box::new(move |node, _lane, batch| held_to_declaration(index, node, &super::schema_of(batch)))
}

pub(crate) fn cpu_schema_validator<'a>(index: &'a PlanIndex<'a>) -> OutputHook<'a, CpuBackend> {
    Box::new(move |node, _lane, batch| {
        let actual = device_schema::device_schema_of(&batch.record_batch().schema());
        held_to_declaration(index, node, &actual)
    })
}
