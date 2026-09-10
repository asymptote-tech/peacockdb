//! What the plan's implementation modules share: the input a node declares it consumes,
//! and the three checks more than one node makes.

use super::{
    ColumnOrder, Expr, GpuNode, KeyDistribution, PartitionLayout, PlanError, Schema, SortOrder,
};

/// The layout a node inherits from its input. A sink is the root, so it is never one.
pub(crate) fn input_layout(input: &dyn GpuNode) -> PartitionLayout {
    input
        .kind()
        .layout()
        .expect("a sink cannot be an input")
        .clone()
}

pub(crate) fn input_schema(input: &dyn GpuNode) -> Schema {
    input
        .kind()
        .schema()
        .expect("a sink cannot be an input")
        .clone()
}

/// Every column reference must be in range of the schema it reads AND carry the name of
/// the field at that position. The name is redundant on purpose: an ordinal read in the
/// wrong order is otherwise invisible until the final result (#135), and the layer
/// rebases ordinals at every node it inserts, so a stale reference is the likely slip.
pub(crate) fn check_column_refs(
    expr: &Expr,
    against: &Schema,
    site: &str,
) -> Result<(), PlanError> {
    match expr {
        Expr::Column(reference) => {
            let field = against
                .fields
                .fields()
                .get(reference.index as usize)
                .ok_or_else(|| {
                    PlanError::Invalid(format!(
                        "{site}: column {}@{} is past the {} columns its input has",
                        reference.name,
                        reference.index,
                        against.fields.fields().len()
                    ))
                })?;
            if field.name() != &reference.name {
                return Err(PlanError::Invalid(format!(
                    "{site}: column {}@{} reads {} at that position",
                    reference.name,
                    reference.index,
                    field.name()
                )));
            }
            Ok(())
        }
        Expr::Literal(_) => Ok(()),
        Expr::Binary { left, right, .. } => {
            check_column_refs(left, against, site)?;
            check_column_refs(right, against, site)
        }
        Expr::Unary { arg, .. } => check_column_refs(arg, against, site),
        Expr::Cast { expr, .. } => check_column_refs(expr, against, site),
        Expr::Like { expr, pattern, .. } => {
            check_column_refs(expr, against, site)?;
            check_column_refs(pattern, against, site)
        }
        Expr::Case {
            comparand,
            when_then,
            else_expr,
        } => {
            for part in comparand.iter().chain(else_expr.iter()) {
                check_column_refs(part, against, site)?;
            }
            for (when, then) in when_then {
                check_column_refs(when, against, site)?;
                check_column_refs(then, against, site)?;
            }
            Ok(())
        }
        Expr::ScalarFunction { args, .. } => {
            for arg in args {
                check_column_refs(arg, against, site)?;
            }
            Ok(())
        }
    }
}

/// A k-way merge reads one row per input at a time and takes the smallest, so the order it
/// merges on has to be the order its input's batches already carry: merging on a key the
/// batches are not sorted by emits rows in no order at all — and then declares them sorted,
/// which is the claim a top-N above it acts on.
///
/// A prefix is enough: batches sorted on `[a, b]` merged on `[a]` come out ordered by `a`.
pub(crate) fn check_merge_keys(
    node: &str,
    keys: &[ColumnOrder],
    input: &PartitionLayout,
) -> Result<(), PlanError> {
    let SortOrder::BatchSorted { columns } = &input.sort_order else {
        return Err(PlanError::Invalid(format!(
            "{node}: a merge needs sorted batches — the planner puts a GpuSort below it"
        )));
    };
    for (position, key) in keys.iter().enumerate() {
        match columns.get(position) {
            Some(sorted) if sorted == key => {}
            Some(sorted) => {
                return Err(PlanError::Invalid(format!(
                    "{node}: it merges on @{} {} at position {position} where its input's \
                     batches are sorted on @{} {}",
                    key.column,
                    direction(key),
                    sorted.column,
                    direction(sorted)
                )));
            }
            None => {
                return Err(PlanError::Invalid(format!(
                    "{node}: it merges on {} keys and its input's batches are sorted on {}",
                    keys.len(),
                    columns.len()
                )));
            }
        }
    }
    Ok(())
}

fn direction(order: &ColumnOrder) -> String {
    format!(
        "{} {}",
        if order.ascending { "asc" } else { "desc" },
        if order.nulls_first {
            "nulls first"
        } else {
            "nulls last"
        }
    )
}

/// Carry a layout's key distribution and sort order through a projection, keeping only
/// what a bare column reference re-exposes: a projected-away or computed column takes
/// its property with it, and a declaration that outlived its column would be a lie the
/// nodes above it act on.
pub(crate) fn rebase_through_projection(
    layout: &PartitionLayout,
    projected: &[Expr],
) -> PartitionLayout {
    let new_index = |old: u32| -> Option<u32> {
        projected
            .iter()
            .position(|expr| match expr {
                Expr::Column(reference) => reference.index == old,
                _ => false,
            })
            .map(|position| position as u32)
    };

    let key_distribution = match &layout.key_distribution {
        KeyDistribution::NotSpecified => KeyDistribution::NotSpecified,
        KeyDistribution::ByHash { hash_keys } => {
            match hash_keys
                .iter()
                .map(|k| new_index(*k))
                .collect::<Option<Vec<_>>>()
            {
                Some(hash_keys) => KeyDistribution::ByHash { hash_keys },
                None => KeyDistribution::NotSpecified,
            }
        }
    };

    let sort_order = match &layout.sort_order {
        SortOrder::NotSpecified => SortOrder::NotSpecified,
        SortOrder::BatchSorted { columns } => {
            let mapped: Option<Vec<_>> = columns
                .iter()
                .map(|order| new_index(order.column).map(|column| ColumnOrder { column, ..*order }))
                .collect();
            // A prefix of the keys would still hold, but a sort key that vanished mid-list
            // leaves an order nothing downstream can name.
            mapped
                .map(SortOrder::batch_sorted)
                .unwrap_or(SortOrder::NotSpecified)
        }
    };

    PartitionLayout {
        key_distribution,
        sort_order,
        ..layout.clone()
    }
}
