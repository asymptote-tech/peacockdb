//! Every batch a node emits, held to the schema its node declares: the driver's output
//! hook composed from the device-schema comparator. The device corpus installs the gpu
//! flavour on every row that says `schema_validation_enabled`; the cpu flavour is the same
//! comparison over an arrow batch, for the end-to-end tier and for symmetry, since the CPU
//! backend's `declared_as` already holds every stage on its own.
//!
//! Two comparisons: the types the device holds, and #227's NULL where a field declares none.
//! The second needs the batch's per-column null counts, which only the cpu flavour can read
//! today — see [`NullsHeld`].

use datafusion::arrow::array::Array;
use datafusion::arrow::datatypes::Schema as ArrowSchema;

#[cfg(not(feature = "rust-only"))]
use crate::executor::GpuBackend;
use crate::executor::{CpuBackend, OutputHook, PlanIndex};

use super::{DeviceSchema, device_schema};

#[cfg(test)]
mod tests;

/// What a batch holds in NULLs per column, where the flavour can read it — stated at the
/// call site rather than inferred, so which half of the comparison runs is visible there.
///
/// `peacock_handle_schema` hands back a schema message and cuDF stores no nullability, so
/// nothing reads a device handle's null counts today and the gpu flavour has none to offer.
/// That is the half of #227 still open; see `llm-wiki/tasks/pbench-detail.md`.
#[cfg_attr(feature = "rust-only", allow(dead_code))]
enum NullsHeld<'a> {
    Unread,
    PerColumn(&'a [usize]),
}

/// The sink is the one node with no schema, and its host batches never reach a hook of
/// this type — so a node the driver offers a batch for always declares one.
fn held_to_declaration(
    index: &PlanIndex<'_>,
    node: usize,
    actual: &DeviceSchema,
    nulls: NullsHeld<'_>,
) -> Result<(), String> {
    let node = index.nodes[node].node;
    let declared = node
        .kind()
        .schema()
        .unwrap_or_else(|| panic!("{} emitted a batch and declares no schema", node.name()));
    let mut findings: Vec<String> = Vec::new();
    findings.extend(device_schema::device_divergence(&declared.fields, actual));
    if let NullsHeld::PerColumn(counts) = nulls {
        findings.extend(nulls_where_none_declared(&declared.fields, counts));
    }
    match findings.is_empty() {
        true => Ok(()),
        false => Err(findings.join("; ")),
    }
}

/// #227: every column holding a NULL where its field declares none, by position, name and
/// count — or `None`. Every violating column rather than the first, like the comparator
/// beside it, and positional over the columns the two sides both have.
fn nulls_where_none_declared(declared: &ArrowSchema, counts: &[usize]) -> Option<String> {
    let broken: Vec<String> = declared
        .fields()
        .iter()
        .zip(counts)
        .enumerate()
        .filter(|(_, (field, count))| !field.is_nullable() && **count > 0)
        .map(|(at, (field, count))| format!("{at} {}: {count} NULL(s)", field.name()))
        .collect();
    (!broken.is_empty())
        .then(|| format!("{} where the node declares none (#227)", broken.join("; ")))
}

#[cfg(not(feature = "rust-only"))]
pub(crate) fn gpu_schema_validator<'a>(index: &'a PlanIndex<'a>) -> OutputHook<'a, GpuBackend> {
    Box::new(move |node, _lane, batch| {
        let actual = super::schema_of(batch);
        held_to_declaration(index, node, &actual, NullsHeld::Unread)
    })
}

pub(crate) fn cpu_schema_validator<'a>(index: &'a PlanIndex<'a>) -> OutputHook<'a, CpuBackend> {
    Box::new(move |node, _lane, batch| {
        let batch = batch.record_batch();
        let actual = device_schema::device_schema_of(&batch.schema());
        let counts: Vec<usize> = batch.columns().iter().map(|c| c.null_count()).collect();
        held_to_declaration(index, node, &actual, NullsHeld::PerColumn(&counts))
    })
}
