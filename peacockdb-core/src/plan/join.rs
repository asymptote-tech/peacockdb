//! The joins. All three take their build side as one batch per lane and stream the probe;
//! the equi-join is co-partitioned, while cross and nested-loop need both inputs in one
//! lane, having no key to co-locate on (#140).

use super::{GpuCrossJoin, GpuHashJoin, GpuNestedLoopJoin, JoinFilterColumn, JoinSide};
use std::any::Any;

use super::BinaryOp;
use super::ColumnRef;
use super::Expr;
use super::PlanError;
use datafusion::arrow::datatypes::DataType;
use datafusion::common::JoinType;

use super::GpuNode;
use super::Schema;
use super::{BatchLayout, KeyDistribution, NodeKind, PartitionLayout, SortOrder};
use super::{check_expr_types, input_layout, input_schema};

impl GpuNode for GpuCrossJoin {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        vec![self.build.as_ref(), self.probe.as_ref()]
    }

    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        check_join_inputs("GpuCrossJoin", self.build.as_ref(), self.probe.as_ref())?;
        check_projection(
            "GpuCrossJoin",
            JoinType::Inner,
            self.projection.as_ref(),
            &input_schema(self.build.as_ref()),
            &input_schema(self.probe.as_ref()),
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl GpuNode for GpuNestedLoopJoin {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        vec![self.build.as_ref(), self.probe.as_ref()]
    }

    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        check_join_inputs(
            "GpuNestedLoopJoin",
            self.build.as_ref(),
            self.probe.as_ref(),
        )?;
        check_filter_columns(
            "GpuNestedLoopJoin",
            &self.filter,
            &self.filter_columns,
            &input_schema(self.build.as_ref()),
            &input_schema(self.probe.as_ref()),
        )?;
        check_projection(
            "GpuNestedLoopJoin",
            self.join_type,
            self.projection.as_ref(),
            &input_schema(self.build.as_ref()),
            &input_schema(self.probe.as_ref()),
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The distribution the output carries. A join does not move a row between lanes, so a
/// claim survives — but only the claim of a side whose rows are never padded, and only
/// where that side earned it by being hashed on this join's keys. Minting one instead,
/// from the keys and the output names alone, declares a hash over lanes nothing ever
/// scattered.
///
/// Padding is what decides the side, not hashing: a Left join emits its unmatched build
/// rows with the probe columns NULL, so only the build column holds the value that placed
/// every output row; Right is the mirror; Full pads both ways and so has no such column.
fn joined_key_distribution(
    join_type: JoinType,
    keys: &[(u32, u32)],
    build: &PartitionLayout,
    probe: &PartitionLayout,
    build_width: u32,
    projection: Option<&Vec<u32>>,
) -> KeyDistribution {
    let sides = unpadded_sides(join_type);
    if sides.is_empty() {
        return KeyDistribution::NotSpecified;
    }
    let ordinals = |side: JoinSide| -> Vec<u32> {
        match side {
            JoinSide::Build => keys.iter().map(|(b, _)| *b).collect(),
            JoinSide::Probe => keys.iter().map(|(_, p)| *p).collect(),
        }
    };
    // Every unpadded side must carry the same fact, or the two disagree about where the
    // rows are and neither claim describes the output.
    for side in sides.iter().copied() {
        let layout = match side {
            JoinSide::Build => build,
            JoinSide::Probe => probe,
        };
        if layout.key_distribution
            != (KeyDistribution::ByHash {
                hash_keys: ordinals(side),
            })
        {
            return KeyDistribution::NotSpecified;
        }
    }
    // Re-numbered structurally rather than by name: the join knows its side widths and
    // its projection, so an output ordinal is derivable even where the key name appears
    // twice in the output or not at all.
    for side in sides.iter().copied() {
        let renumbered: Option<Vec<u32>> = ordinals(side)
            .iter()
            .map(|ordinal| output_ordinal(join_type, side, *ordinal, build_width, projection))
            .collect();
        if let Some(hash_keys) = renumbered {
            return KeyDistribution::ByHash { hash_keys };
        }
    }
    KeyDistribution::NotSpecified
}

/// The sides whose rows this join type emits unpadded. Empty for Full, which pads both.
fn unpadded_sides(join_type: JoinType) -> &'static [JoinSide] {
    match join_type {
        JoinType::Inner => &[JoinSide::Build, JoinSide::Probe],
        JoinType::Left | JoinType::LeftSemi | JoinType::LeftAnti | JoinType::LeftMark => {
            &[JoinSide::Build]
        }
        JoinType::Right | JoinType::RightSemi | JoinType::RightAnti => &[JoinSide::Probe],
        JoinType::Full => &[],
    }
}

/// Where one side's column lands in the output: into the emitted table first — which is
/// the crossed pair, or the one side a semi form emits — and then through the projection,
/// where a dropped column has no output ordinal at all.
fn output_ordinal(
    join_type: JoinType,
    side: JoinSide,
    ordinal: u32,
    build_width: u32,
    projection: Option<&Vec<u32>>,
) -> Option<u32> {
    // Exhaustive rather than defaulted: a side the output does not hold has no ordinal,
    // and a join type reaching a catch-all would silently lose a claim it earned.
    let emitted = match (emits(join_type), side) {
        (Emits::BothSides, JoinSide::Build) => ordinal,
        (Emits::BothSides, JoinSide::Probe) => build_width + ordinal,
        (Emits::BuildSide | Emits::BuildSideAndMark, JoinSide::Build) => ordinal,
        (Emits::ProbeSide, JoinSide::Probe) => ordinal,
        (Emits::BuildSide | Emits::BuildSideAndMark, JoinSide::Probe) => return None,
        (Emits::ProbeSide, JoinSide::Build) => return None,
    };
    match projection {
        Some(columns) => columns
            .iter()
            .position(|column| *column == emitted)
            .map(|at| at as u32),
        None => Some(emitted),
    }
}

/// Which columns a join type emits, before any projection of its own. A semi or anti join
/// is a filter written as a join — it emits the side it decides about — and a mark join
/// emits that side plus the boolean it computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Emits {
    BothSides,
    BuildSide,
    /// The build side and one more column, the mark.
    BuildSideAndMark,
    ProbeSide,
}

fn emits(join_type: JoinType) -> Emits {
    match join_type {
        JoinType::Inner | JoinType::Left | JoinType::Right | JoinType::Full => Emits::BothSides,
        JoinType::LeftSemi | JoinType::LeftAnti => Emits::BuildSide,
        JoinType::LeftMark => Emits::BuildSideAndMark,
        JoinType::RightSemi | JoinType::RightAnti => Emits::ProbeSide,
    }
}

/// One lane, many batches, no order and no key: cross and nested-loop joins have no key to
/// co-locate on, so nothing about the inputs' layout survives them.
pub(crate) fn joined_layout() -> PartitionLayout {
    PartitionLayout {
        n: 1,
        key_distribution: KeyDistribution::NotSpecified,
        sort_order: SortOrder::NotSpecified,
        batch_layout: BatchLayout::MultipleBatches,
    }
}

/// A key pair is one ordinal into each side. An out-of-range one does not merely read the
/// wrong column: `joined_key_distribution` cannot name it, so the node quietly declares no
/// distribution at all and a co-partitioned join above it loses the claim it earned.
fn check_keys(keys: &[(u32, u32)], build: &Schema, probe: &Schema) -> Result<(), PlanError> {
    if keys.is_empty() {
        return Err(PlanError::Invalid(
            "GpuHashJoin: an equi-join with no keys is a cross join — the planner emits \
             GpuCrossJoin for that shape"
                .to_string(),
        ));
    }
    for (side, schema, ordinals) in [
        (
            "build",
            build,
            keys.iter().map(|(b, _)| *b).collect::<Vec<_>>(),
        ),
        (
            "probe",
            probe,
            keys.iter().map(|(_, p)| *p).collect::<Vec<_>>(),
        ),
    ] {
        let width = schema.fields.fields().len();
        for ordinal in ordinals {
            if ordinal as usize >= width {
                return Err(PlanError::Invalid(format!(
                    "GpuHashJoin: {side} key @{ordinal} is past the {width} columns that side has"
                )));
            }
        }
    }
    Ok(())
}

/// A projected ordinal indexes the table the join emits before its own projection — both
/// sides for the four that pair rows, one side for a semi or anti join, and that side plus
/// the mark for a mark join. Bounding it by both sides instead would leave the five
/// one-sided types checked against a table wider than the one they have.
fn check_projection(
    node: &str,
    join_type: JoinType,
    projection: Option<&Vec<u32>>,
    build: &Schema,
    probe: &Schema,
) -> Result<(), PlanError> {
    let width = emitted_columns(
        join_type,
        build.fields.fields().len(),
        probe.fields.fields().len(),
    );
    for ordinal in projection.into_iter().flatten() {
        if *ordinal as usize >= width {
            return Err(PlanError::Invalid(format!(
                "{node}: projected column @{ordinal} is past the {width} a {join_type:?} join \
                 emits"
            )));
        }
    }
    Ok(())
}

fn check_join_inputs(
    node: &str,
    build: &dyn GpuNode,
    probe: &dyn GpuNode,
) -> Result<(), PlanError> {
    let (build_layout, probe_layout) = (input_layout(build), input_layout(probe));
    if build_layout.n != 1 || probe_layout.n != 1 {
        return Err(PlanError::Invalid(format!(
            "{node}: with no key to co-locate on both inputs must be one lane, not {} and {} \
             — the planner inserts GpuMergePartitions",
            build_layout.n, probe_layout.n
        )));
    }
    if build_layout.batch_layout != BatchLayout::SingleBatch {
        return Err(PlanError::Invalid(format!(
            "{node}: its build side must be one batch — the planner inserts \
             GpuCoalesceAllBatches below it"
        )));
    }
    Ok(())
}

/// Every reference in a join filter is an ordinal into the filter's own table, so each
/// one has to name a mapped column AND find its own name on the side that column comes
/// from — a mapping that points at the wrong side otherwise reads a valid column of the
/// wrong table, which nothing downstream can detect.
fn check_filter_columns(
    node: &str,
    filter: &Expr,
    columns: &[JoinFilterColumn],
    build: &Schema,
    probe: &Schema,
) -> Result<(), PlanError> {
    check_expr_types(filter, node)?;
    let mut refs = Vec::new();
    collect_column_refs(filter, &mut refs);
    for reference in refs {
        let mapped = columns.get(reference.index as usize).ok_or_else(|| {
            PlanError::Invalid(format!(
                "{node}: filter column {}@{} is past the {} its map has",
                reference.name,
                reference.index,
                columns.len()
            ))
        })?;
        let source = match mapped.side {
            JoinSide::Build => build,
            JoinSide::Probe => probe,
        };
        let field = source
            .fields
            .fields()
            .get(mapped.index as usize)
            .ok_or_else(|| {
                PlanError::Invalid(format!(
                    "{node}: filter column {}@{} maps past its side's columns",
                    reference.name, reference.index
                ))
            })?;
        if field.name() != &reference.name {
            return Err(PlanError::Invalid(format!(
                "{node}: filter column {}@{} maps to {} on the {:?} side",
                reference.name,
                reference.index,
                field.name(),
                mapped.side
            )));
        }
    }
    Ok(())
}

fn collect_column_refs<'a>(expr: &'a Expr, into: &mut Vec<&'a ColumnRef>) {
    match expr {
        Expr::Column(reference) => into.push(reference),
        Expr::Literal(_) => {}
        Expr::Binary { left, right, .. } => {
            collect_column_refs(left, into);
            collect_column_refs(right, into);
        }
        Expr::Unary { arg, .. } => collect_column_refs(arg, into),
        Expr::Cast { expr, .. } => collect_column_refs(expr, into),
        Expr::Like { expr, pattern, .. } => {
            collect_column_refs(expr, into);
            collect_column_refs(pattern, into);
        }
        Expr::Case {
            comparand,
            when_then,
            else_expr,
        } => {
            for part in comparand.iter().chain(else_expr.iter()) {
                collect_column_refs(part, into);
            }
            for (when, then) in when_then {
                collect_column_refs(when, into);
                collect_column_refs(then, into);
            }
        }
        Expr::ScalarFunction { args, .. } => {
            for arg in args {
                collect_column_refs(arg, into);
            }
        }
    }
}

/// Whether a join owes rows only its last probe batch can settle: the build rows nothing
/// matched (Left, Full) or the build side's own answer (the semi family). These five are
/// what a session's `join_finish` answers for, whatever the node's keys or residual. Every
/// other type answers each probe row in its own call: the build side is complete before the
/// first probe call, so a probe row unmatched in this batch is unmatched everywhere.
pub(crate) fn needs_finish(join_type: JoinType) -> bool {
    match join_type {
        JoinType::Left
        | JoinType::Full
        | JoinType::LeftSemi
        | JoinType::LeftAnti
        | JoinType::LeftMark => true,
        JoinType::Inner | JoinType::Right | JoinType::RightSemi | JoinType::RightAnti => false,
    }
}

/// The sides whose unmatched rows are never emitted, as `(build, probe)`. There a NULL key
/// can match nothing under SQL equality, so it is dropped before the shuffle rather than
/// skewing the one lane every NULL hashes to (#137).
pub(crate) fn null_key_droppable(join_type: JoinType) -> (bool, bool) {
    match join_type {
        JoinType::Inner | JoinType::LeftSemi | JoinType::RightSemi => (true, true),
        JoinType::Left | JoinType::LeftAnti | JoinType::LeftMark => (false, true),
        JoinType::Right | JoinType::RightAnti => (true, false),
        JoinType::Full => (false, false),
    }
}

/// Whether a join whose build side holds no row — no batch, or batches of none — owes no
/// rows at all.
///
/// True where every output row is built from a build row, so an empty build side is an
/// empty answer and the driver drops the lane's probe batches without a call. False for the
/// three types that preserve unmatched PROBE rows: what they owe is the probe side, padded or
/// not.
pub(crate) fn empty_build_owes_nothing(join_type: JoinType) -> bool {
    match join_type {
        JoinType::Inner
        | JoinType::Left
        | JoinType::LeftSemi
        | JoinType::LeftAnti
        | JoinType::LeftMark
        | JoinType::RightSemi => true,
        JoinType::Right | JoinType::Full | JoinType::RightAnti => false,
    }
}

/// How many columns a join emits before any projection of its own — the count half of
/// [`emits`], which is the same fact its carry-over rule reads.
pub(crate) fn emitted_columns(join_type: JoinType, build: usize, probe: usize) -> usize {
    match emits(join_type) {
        Emits::BothSides => build + probe,
        Emits::BuildSide => build,
        Emits::BuildSideAndMark => build + 1,
        Emits::ProbeSide => probe,
    }
}

/// The sides whose columns a join type emits, as `(build, probe)` — the side half of
/// [`emits`]; a mark comes after both.
pub(crate) fn emitted_sides(join_type: JoinType) -> (bool, bool) {
    match emits(join_type) {
        Emits::BothSides => (true, true),
        Emits::BuildSide | Emits::BuildSideAndMark => (true, false),
        Emits::ProbeSide => (false, true),
    }
}

impl GpuNode for GpuHashJoin {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        vec![self.build.as_ref(), self.probe.as_ref()]
    }

    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        let (build, probe) = (
            input_layout(self.build.as_ref()),
            input_layout(self.probe.as_ref()),
        );
        if build.n != probe.n {
            return Err(PlanError::Invalid(format!(
                "GpuHashJoin: lane p of one side must hold what can match lane p of the other, \
                 but the sides carry {} and {} lanes",
                build.n, probe.n
            )));
        }
        if build.n > 1 {
            let hashed_on = |layout: &PartitionLayout, keys: Vec<u32>| {
                layout.key_distribution == KeyDistribution::ByHash { hash_keys: keys }
            };
            if !hashed_on(&build, self.keys.iter().map(|(b, _)| *b).collect())
                || !hashed_on(&probe, self.keys.iter().map(|(_, p)| *p).collect())
            {
                return Err(PlanError::Invalid(
                    "GpuHashJoin: several lanes are co-partitioned only if both sides were \
                     hashed on the join keys, in key order — the planner inserts \
                     GpuEmitPartitions on each side"
                        .to_string(),
                ));
            }
        }
        if build.batch_layout != BatchLayout::SingleBatch {
            return Err(PlanError::Invalid(
                "GpuHashJoin: its build side must be one batch per lane — the planner inserts \
                 GpuCoalesceAllBatches below it"
                    .to_string(),
            ));
        }
        if let Some(filter) = &self.filter {
            check_filter_columns(
                "GpuHashJoin",
                filter,
                &self.filter_columns,
                &input_schema(self.build.as_ref()),
                &input_schema(self.probe.as_ref()),
            )?;
        }
        let (build_schema, probe_schema) = (
            input_schema(self.build.as_ref()),
            input_schema(self.probe.as_ref()),
        );
        check_keys(&self.keys, &build_schema, &probe_schema)?;
        check_projection(
            "GpuHashJoin",
            self.join_type,
            self.projection.as_ref(),
            &build_schema,
            &probe_schema,
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn new_hash_join(
    build: Box<dyn GpuNode>,
    probe: Box<dyn GpuNode>,
    join_type: JoinType,
    keys: Vec<(u32, u32)>,
    filter: Option<Expr>,
    filter_columns: Vec<JoinFilterColumn>,
    null_equals_null: bool,
    projection: Option<Vec<u32>>,
    schema: Schema,
) -> GpuHashJoin {
    let build_layout = input_layout(build.as_ref());
    let mut layout = input_layout(probe.as_ref());
    // The lane count survives, and so does a hash the input earned: a join does not
    // move a row between lanes. Dropping that is what makes a correct plan fail the
    // co-location guard above an aggregate.
    layout.key_distribution = joined_key_distribution(
        join_type,
        &keys,
        &build_layout,
        &layout,
        input_schema(build.as_ref()).fields.fields().len() as u32,
        projection.as_ref(),
    );
    // The output is the join's own rows in the order its probe batches arrive.
    layout.sort_order = SortOrder::NotSpecified;
    layout.batch_layout = BatchLayout::MultipleBatches;
    GpuHashJoin {
        kind: NodeKind::Intermediate { layout, schema },
        join_type,
        keys,
        filter,
        filter_columns,
        null_equals_null,
        projection,
        build,
        probe,
    }
}

/// `join_session.cpp` takes a semi join's keys distinct only where no conjunct of its residual
/// reads both sides or neither; any other goes to `mixed_left_semi_join` or the pairs path,
/// which take none. The split is mirrored here because the price is of the session it builds.
pub(crate) fn distinct_key_width(join: &GpuHashJoin) -> u64 {
    let (side, ordinals): (&dyn GpuNode, Vec<u32>) = match join.join_type {
        JoinType::RightSemi | JoinType::RightAnti => (
            join.build.as_ref(),
            join.keys.iter().map(|(build, _)| *build).collect(),
        ),
        JoinType::LeftSemi | JoinType::LeftAnti | JoinType::LeftMark => (
            join.probe.as_ref(),
            join.keys.iter().map(|(_, probe)| *probe).collect(),
        ),
        JoinType::Inner | JoinType::Left | JoinType::Right | JoinType::Full => return 0,
    };
    let mut conjuncts = Vec::new();
    if let Some(filter) = &join.filter {
        conjuncts_of(filter, &mut conjuncts);
    }
    let crosses = conjuncts.iter().any(|conjunct| {
        let mut refs = Vec::new();
        collect_column_refs(conjunct, &mut refs);
        let reads = |side: JoinSide| {
            refs.iter()
                .any(|r| join.filter_columns[r.index as usize].side == side)
        };
        reads(JoinSide::Build) == reads(JoinSide::Probe)
    });
    if crosses {
        return 0;
    }
    let schema = input_schema(side);
    ordinals
        .iter()
        .map(|ordinal| width_on_device(schema.fields.field(*ordinal as usize).data_type()))
        .sum()
}

/// The residual's top-level `AND` operands, as `join_session.cpp`'s `conjuncts_of` splits it.
fn conjuncts_of<'a>(expr: &'a Expr, into: &mut Vec<&'a Expr>) {
    match expr {
        Expr::Binary {
            left,
            op: BinaryOp::And,
            right,
            ..
        } => {
            conjuncts_of(left, into);
            conjuncts_of(right, into);
        }
        other => into.push(other),
    }
}

/// A column's bytes per row as the session counts them (`filter_row_bytes`): the fixed width
/// cuDF holds it at, and 16 for a variable-width one.
pub(crate) fn width_on_device(data_type: &DataType) -> u64 {
    match data_type {
        DataType::Boolean => 1,
        other => other.primitive_width().map_or(16, |width| width as u64),
    }
}

/// cuDF's hash table per build row: a `static_multiset` of 8-byte slots at load factor 0.5,
/// whatever the key width, and `distinct_hash_join`'s priced the same (design §4.3).
const HASH_TABLE_BYTES_PER_ROW: u64 = 16;

pub(crate) fn session_bytes(
    join_type: JoinType,
    keyed: bool,
    distinct_key_width: u64,
    rows: u64,
) -> u64 {
    let table = match keyed {
        true => HASH_TABLE_BYTES_PER_ROW * rows,
        false => 0,
    };
    let distinct_keys = match join_type {
        JoinType::RightSemi | JoinType::RightAnti => distinct_key_width * rows,
        _ => 0,
    };
    let matched = match needs_finish(join_type) {
        true => rows,
        false => 0,
    };
    table + distinct_keys + matched
}
