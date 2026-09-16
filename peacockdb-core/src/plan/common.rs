//! What the plan's implementation modules share: the input a node declares it consumes,
//! and the three checks more than one node makes.

use datafusion::arrow::datatypes::DataType;

use super::{
    ColumnOrder, Expr, GpuNode, KeyDistribution, PartitionLayout, PlanError, Schema, SortOrder,
};

/// The arrow layouts cuDF has no counterpart for: it holds one string layout and exports it
/// as `Utf8`, and cannot import a view array at all.
pub(crate) fn is_view_type(data_type: &DataType) -> bool {
    matches!(
        data_type,
        DataType::Utf8View
            | DataType::BinaryView
            | DataType::ListView(_)
            | DataType::LargeListView(_)
    )
}

/// Every type an expression names — a literal's, a cast's target, a scalar function's return,
/// a binary's out type — is one the device can hold. The parquet option that produced view
/// types is off, so one that appears here was minted upstream, and this is where it is caught
/// rather than at the sink (#183).
pub(crate) fn check_expr_types(expr: &Expr, site: &str) -> Result<(), PlanError> {
    let refuse = |what: &str, data_type: &DataType| {
        Err(PlanError::Invalid(format!(
            "{site}: {what} is {data_type}, a view type the device cannot hold"
        )))
    };
    match expr {
        Expr::Column(_) => Ok(()),
        Expr::Literal(value) if is_view_type(&value.data_type()) => {
            refuse("a literal", &value.data_type())
        }
        Expr::Literal(_) => Ok(()),
        Expr::Binary {
            left,
            right,
            out_type,
            ..
        } => {
            if is_view_type(out_type) {
                return refuse("a binary's type", out_type);
            }
            check_expr_types(left, site)?;
            check_expr_types(right, site)
        }
        Expr::Unary { arg, .. } => check_expr_types(arg, site),
        Expr::Cast { expr, target } => {
            if is_view_type(target) {
                return refuse("a cast target", target);
            }
            check_expr_types(expr, site)
        }
        Expr::Like { expr, pattern, .. } => {
            check_expr_types(expr, site)?;
            check_expr_types(pattern, site)
        }
        Expr::Case {
            comparand,
            when_then,
            else_expr,
        } => {
            for part in comparand.iter().chain(else_expr.iter()) {
                check_expr_types(part, site)?;
            }
            for (when, then) in when_then {
                check_expr_types(when, site)?;
                check_expr_types(then, site)?;
            }
            Ok(())
        }
        Expr::ScalarFunction {
            args, return_type, ..
        } => {
            if is_view_type(return_type) {
                return refuse("a scalar function's return type", return_type);
            }
            for arg in args {
                check_expr_types(arg, site)?;
            }
            Ok(())
        }
    }
}

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
    check_expr_types(expr, site)?;
    column_refs_in_range(expr, against, site)
}

fn column_refs_in_range(expr: &Expr, against: &Schema, site: &str) -> Result<(), PlanError> {
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
            column_refs_in_range(left, against, site)?;
            column_refs_in_range(right, against, site)
        }
        Expr::Unary { arg, .. } => column_refs_in_range(arg, against, site),
        Expr::Cast { expr, .. } => column_refs_in_range(expr, against, site),
        Expr::Like { expr, pattern, .. } => {
            column_refs_in_range(expr, against, site)?;
            column_refs_in_range(pattern, against, site)
        }
        Expr::Case {
            comparand,
            when_then,
            else_expr,
        } => {
            for part in comparand.iter().chain(else_expr.iter()) {
                column_refs_in_range(part, against, site)?;
            }
            for (when, then) in when_then {
                column_refs_in_range(when, against, site)?;
                column_refs_in_range(then, against, site)?;
            }
            Ok(())
        }
        Expr::ScalarFunction { args, .. } => {
            for arg in args {
                column_refs_in_range(arg, against, site)?;
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
