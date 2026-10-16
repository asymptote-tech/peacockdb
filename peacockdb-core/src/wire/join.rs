//! The joins: one `CudfJoin` per join node, whichever of the three it is, and a recipe that
//! is a session rather than a choreography — built once, probed per batch, finished once.
//!
//! The node is a leaf on the wire. Its inputs arrive through the session symbols, never
//! through `execute_node`, so it takes no child slot; what it was handed stays in the
//! writer's pool and is gathered under it by the walk's own reduce.

use datafusion::arrow::datatypes::SchemaRef;
use datafusion::common::JoinType;
use flatbuffers::{FlatBufferBuilder, WIPOffset};

use super::generated::peacock::plan as fb;

use super::expr_writer::write_expr;
use super::node_writer;
use super::serialize::serialize_schema;
use super::writer::{Payload, Writer};
use super::{Call, CallPattern, Recipe};
use crate::plan::Expr;
use crate::plan::PlanError;
use crate::plan::Schema;
use crate::plan::needs_finish;
use crate::plan::{GpuCrossJoin, GpuHashJoin, GpuNestedLoopJoin};
use crate::plan::{JoinFilterColumn, JoinSide};

/// The three plan nodes as the one thing the wire writes for each. A cross join is an
/// Inner join with no key and no condition, which is exactly how the session reads it.
#[derive(Clone, Copy)]
pub(crate) enum JoinNode<'a> {
    Hash(&'a GpuHashJoin),
    NestedLoop(&'a GpuNestedLoopJoin),
    Cross(&'a GpuCrossJoin),
}

impl JoinNode<'_> {
    fn join_type(&self) -> JoinType {
        match self {
            Self::Hash(node) => node.join_type,
            Self::NestedLoop(node) => node.join_type,
            Self::Cross(_) => JoinType::Inner,
        }
    }

    fn keys(&self) -> &[(u32, u32)] {
        match self {
            Self::Hash(node) => &node.keys,
            Self::NestedLoop(_) | Self::Cross(_) => &[],
        }
    }

    fn filter(&self) -> Option<&Expr> {
        match self {
            Self::Hash(node) => node.filter.as_ref(),
            Self::NestedLoop(node) => Some(&node.filter),
            Self::Cross(_) => None,
        }
    }

    fn filter_columns(&self) -> &[JoinFilterColumn] {
        match self {
            Self::Hash(node) => &node.filter_columns,
            Self::NestedLoop(node) => &node.filter_columns,
            Self::Cross(_) => &[],
        }
    }

    fn null_equals_null(&self) -> bool {
        match self {
            Self::Hash(node) => node.null_equals_null,
            Self::NestedLoop(_) | Self::Cross(_) => false,
        }
    }

    fn projection(&self) -> Option<&[u32]> {
        match self {
            Self::Hash(node) => node.projection.as_deref(),
            Self::NestedLoop(node) => node.projection.as_deref(),
            Self::Cross(node) => node.projection.as_deref(),
        }
    }
}

/// One `CudfJoin`, and the session's three calls on it: `join_build` once, `join_probe` per
/// probe batch, and `join_finish` at done for the types that owe unmatched build rows.
pub(crate) fn join(
    node: JoinNode<'_>,
    inputs: &[&Schema],
    writer: &mut Writer,
) -> Result<Option<Recipe>, PlanError> {
    let [build, probe] = inputs else {
        unreachable!("a join declares two inputs")
    };
    let chunk_bytes = writer.join_scratch_bytes();
    let seq = writer.node(0, |b, _| cudf_join(b, node, build, probe, chunk_bytes))?;
    let mut calls = vec![
        Call::join(seq, CallPattern::JoinBuild),
        Call::join(seq, CallPattern::PerProbeBatch),
    ];
    if needs_finish(node.join_type()) {
        calls.push(Call::join(seq, CallPattern::AtDone));
    }
    Ok(Some(Recipe::of(calls)))
}

fn cudf_join<'a>(
    b: &mut FlatBufferBuilder<'a>,
    node: JoinNode<'_>,
    build: &Schema,
    probe: &Schema,
    chunk_bytes: u64,
) -> Result<Payload, PlanError> {
    let mut keys = Vec::with_capacity(node.keys().len());
    for (build_ordinal, probe_ordinal) in node.keys() {
        let left = column(b, *build_ordinal, build)?;
        let right = column(b, *probe_ordinal, probe)?;
        keys.push(fb::JoinKey::create(
            b,
            &fb::JoinKeyArgs {
                left: Some(left),
                right: Some(right),
            },
        ));
    }
    let keys = b.create_vector(&keys);
    let filter = match node.filter() {
        Some(expr) => Some(write_expr(b, expr)?),
        None => None,
    };
    let filter_columns = (!node.filter_columns().is_empty()).then(|| {
        let columns: Vec<fb::JoinFilterColumn> =
            node.filter_columns().iter().map(filter_column).collect();
        b.create_vector(&columns)
    });
    let build_schema = serialize_join_schema(b, &build.fields, "build")?;
    let probe_schema = serialize_join_schema(b, &probe.fields, "probe")?;
    // Present-and-empty is written as such rather than dropped: the session refuses it,
    // where an absent one means every column.
    let projection = node.projection().map(|columns| b.create_vector(columns));
    let join = fb::CudfJoin::create(
        b,
        &fb::CudfJoinArgs {
            join_type: wire_join_type(node.join_type()),
            keys: Some(keys),
            filter,
            filter_columns,
            null_equals_null: node.null_equals_null(),
            build_schema: Some(build_schema),
            probe_schema: Some(probe_schema),
            projection,
            chunk_bytes,
        },
    );
    Ok(Payload {
        kind: fb::PlanNodeKind::CudfJoin,
        value: join.as_union_value(),
    })
}

/// A join side's schema, every field typed. It is what the session reads an absent side's
/// columns and every pad's type from, so a type the wire cannot name is refused at plan
/// time, naming the side, rather than at run time on whichever lane has no batch (#249).
fn serialize_join_schema<'a>(
    b: &mut FlatBufferBuilder<'a>,
    schema: &SchemaRef,
    side: &str,
) -> Result<WIPOffset<fb::Schema<'a>>, PlanError> {
    serialize_schema(b, schema).map_err(|why| match why {
        PlanError::Unsupported(what) => {
            PlanError::Unsupported(format!("a join's {side} side: {what}"))
        }
        PlanError::Invalid(what) => PlanError::Invalid(format!("a join's {side} side: {what}")),
    })
}

fn column<'a>(
    b: &mut FlatBufferBuilder<'a>,
    ordinal: u32,
    schema: &Schema,
) -> Result<WIPOffset<fb::Expr<'a>>, PlanError> {
    let field = node_writer::field_at(schema, ordinal);
    write_expr(b, &Expr::column(ordinal, field.name()))
}

fn filter_column(column: &JoinFilterColumn) -> fb::JoinFilterColumn {
    fb::JoinFilterColumn::new(
        column.index,
        match column.side {
            JoinSide::Build => fb::JoinSide::Left,
            JoinSide::Probe => fb::JoinSide::Right,
        },
    )
}

fn wire_join_type(join_type: JoinType) -> fb::JoinType {
    match join_type {
        JoinType::Inner => fb::JoinType::Inner,
        JoinType::Left => fb::JoinType::Left,
        JoinType::Right => fb::JoinType::Right,
        JoinType::Full => fb::JoinType::Full,
        JoinType::LeftSemi => fb::JoinType::LeftSemi,
        JoinType::RightSemi => fb::JoinType::RightSemi,
        JoinType::LeftAnti => fb::JoinType::LeftAnti,
        JoinType::RightAnti => fb::JoinType::RightAnti,
        JoinType::LeftMark => fb::JoinType::LeftMark,
    }
}
