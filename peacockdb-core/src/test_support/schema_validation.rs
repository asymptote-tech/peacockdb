//! Every batch a node emits, held to the schema its node declares: the driver's output
//! hook composed from the device-schema comparator. The device corpus installs the gpu
//! flavour on every row that says `schema_validation_enabled`; the cpu flavour is the same
//! comparison over an arrow batch, for the end-to-end tier and for symmetry, since the CPU
//! backend's `declared_as` already holds every stage on its own.

#[cfg(not(feature = "rust-only"))]
use crate::executor::GpuBackend;
use crate::executor::{CpuBackend, OutputHook, PlanIndex};

use super::{DeviceSchema, MODES, Mode, device_schema};

#[cfg(test)]
mod tests;

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

/// A `corpus_query!` line's last argument, decoded against the mode being run: whether
/// every batch is held to its node's declaration. The disabled form may name the modes it
/// applies to — `schema_validation_disabled(tp4_single | tp4_sized)` — so a query red on
/// schema at some modes keeps the hook at the rest. Exhaustive on both halves, since a
/// misspelling of either reads as a legal line and runs a cell unvalidated.
///
/// Its only caller is the device corpus, which `rust-only` compiles out, so that lib
/// build sees this and `mask_names` as dead.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn is_validated_at(declared: &str, mode: &Mode, what: &str) -> bool {
    let (keyword, mask) = match declared.split_once('(') {
        Some((keyword, rest)) => (keyword.trim(), Some(rest.trim().trim_end_matches(')'))),
        None => (declared.trim(), None),
    };
    match (keyword, mask) {
        ("schema_validation_enabled", None) => true,
        ("schema_validation_disabled", None) => false,
        ("schema_validation_disabled", Some(mask)) => !mask_names(mask, mode, what),
        ("schema_validation_enabled", Some(_)) => panic!(
            "{what}: corpus_query!: schema_validation_enabled takes no mode mask — the hook \
             is on at every mode, or off at the ones a disabled mask names"
        ),
        (other, _) => panic!(
            "{what}: corpus_query!: unknown schema validation '{other}' \
             (expected schema_validation_enabled|schema_validation_disabled)"
        ),
    }
}

/// Whether a disabled mask names this mode. Every entry has to be one of the five: a
/// misspelled one would otherwise read as "some other mode" and leave the hook on at the
/// mode the line meant to excuse, which is a red cell blamed on the engine.
#[cfg_attr(not(test), allow(dead_code))]
fn mask_names(mask: &str, mode: &Mode, what: &str) -> bool {
    let mut names = false;
    for entry in mask.split('|') {
        let entry = entry.trim();
        let found = MODES
            .iter()
            .find(|m| m.ident() == entry)
            .unwrap_or_else(|| {
                panic!(
                    "{what}: corpus_query!: schema validation mask entry '{entry}' names no \
                     mode of the five"
                )
            });
        names |= found.name == mode.name;
    }
    names
}
