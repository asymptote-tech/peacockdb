//! What a plan is: the eighteen nodes, the vocabulary they are written in, and the rules a
//! tree has to satisfy.
//!
//! The IR is neither planner nor executor — it is what passes between them, and consumers
//! span both halves. Every declaration the component offers is here; the implementation
//! modules hold the trait impls, the constructors' bodies and the checks.
//!
//! The tree is heterogeneous and planning is not hot, so this is the one place trait objects
//! are used; everything on the per-batch path is static (see `executor/mod.rs`).

mod accumulators;
mod aggregate;
mod aggregates;
mod common;
mod error;
mod exec_ops;
mod interval;
mod join;
mod layout;
mod partition_ops;
mod source;
mod union;
mod unload;
mod validate;

#[cfg(test)]
mod tests;

use std::any::Any;
use std::sync::Arc;

use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::common::{JoinType, ScalarValue};

use crate::executor::RowRange;

use accumulators::{new_accumulate_batches_and_sort, new_coalesce_all_batches, new_limit};
use aggregate::{new_aggregate, new_aggregate_batches};
use common::{
    check_column_refs, check_merge_keys, input_layout, input_schema, rebase_through_projection,
};
use exec_ops::{new_filter, new_project, new_sort};
use join::{joined_layout, new_hash_join};
use partition_ops::{new_emit_partitions, new_merge_partitions, new_merge_sorted_partitions};
use source::{largest_batch_bytes, new_load_parquet};
use union::{new_interleave, new_union};

/// What planning and validation return. Both are plan-time: a shape the engine cannot run
/// is refused where the planner can see it, rather than throwing mid-query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// A shape the engine does not implement — window functions (#143), a mixed
    /// distinct (#62), a value-form CASE (#57). Names the shape, not the node's fix.
    Unsupported(String),
    /// A plan that violates what a node requires of its children. Names the fix, since
    /// the node knows it: "the planner inserts `GpuMergePartitions` below it".
    Invalid(String),
}

// The expression IR. Types and literals are DataFusion's own — the coercions and the
// decimal precision and scale it derived are exactly what must not be re-derived (#55/#56/#63
// are what re-deriving them costs). What is not reused is the shape: a column reference is an
// ordinal into a child whose column order this engine decides, so every reference is rebased
// at each node the translation layer inserts.

/// The name rides beside the ordinal so a plan can be checked against the schema at that
/// position rather than trusting it — #135's class, caught at plan time here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnRef {
    pub index: u32,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,
    And,
    Or,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    BitwiseShiftLeft,
    BitwiseShiftRight,
    StringConcat,
    IsDistinctFrom,
    IsNotDistinctFrom,
}

/// `Sqrt` is not a DataFusion unary: it is what a stddev's finalize expression needs, and
/// cuDF's `unary_operator::SQRT` is what the hardwired finalize already calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    IsNull,
    IsNotNull,
    Negative,
    Sqrt,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Column(ColumnRef),
    Literal(ScalarValue),
    /// `out_type` is DataFusion's declared output type. cuDF derives its own fixed-point
    /// result scale — division most visibly — so the declared one travels with the op.
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
        out_type: DataType,
    },
    Unary {
        op: UnaryOp,
        arg: Box<Expr>,
    },
    Cast {
        expr: Box<Expr>,
        target: DataType,
    },
    Like {
        expr: Box<Expr>,
        pattern: Box<Expr>,
        negated: bool,
        case_insensitive: bool,
    },
    /// Search form leaves `comparand` `None`; the value form sets it.
    Case {
        comparand: Option<Box<Expr>>,
        when_then: Vec<(Expr, Expr)>,
        else_expr: Option<Box<Expr>>,
    },
    ScalarFunction {
        name: String,
        args: Vec<Expr>,
        return_type: DataType,
        nullable: bool,
    },
}

impl Expr {
    pub fn column(index: u32, name: &str) -> Self {
        Self::Column(ColumnRef {
            index,
            name: name.to_string(),
        })
    }

    pub fn binary(left: Expr, op: BinaryOp, right: Expr, out_type: DataType) -> Self {
        Self::Binary {
            left: Box::new(left),
            op,
            right: Box::new(right),
            out_type,
        }
    }

    pub(crate) fn unary(op: UnaryOp, arg: Expr) -> Self {
        Self::Unary {
            op,
            arg: Box::new(arg),
        }
    }
}

/// An expression with the name its column takes in the node's output — a project list
/// entry, or one column of an aggregate's `final` list.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedExpr {
    pub expr: Expr,
    pub name: String,
}

impl NamedExpr {
    pub fn new(expr: Expr, name: &str) -> Self {
        Self {
            expr,
            name: name.to_string(),
        }
    }
}

// What a node declares about its output columns: arrow types, plus the semantics a consumer
// can check. Types are the planner's own, so decimal precision and scale are not a second
// copy that can drift. The annotations are what a merging or finalizing node checks before
// trusting a position — the class #135 describes, where an ordinal read in the wrong order
// produces identical per-node numbers everywhere and surfaces only in the final result.

/// One aggregate's state columns in a partial's output. `positions` index the declaring
/// node's fields; `func` and `ddof` are carried so a merge can confirm it is merging the
/// aggregate it thinks it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggStateColumns {
    pub output: String,
    pub func: AggFunc,
    pub ddof: u32,
    pub positions: Vec<u32>,
}

/// Column types in order — the index *is* the ordinal every plan reference uses — plus
/// what those columns mean.
#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    pub fields: Arc<ArrowSchema>,
    /// ordinals of the group-by keys, including a synthesized `__grouping_id`
    pub group_keys: Vec<u32>,
    /// one entry per aggregate whose state this output carries
    pub agg_state: Vec<AggStateColumns>,
}

impl Schema {
    /// Columns with no group keys and no aggregate state — a scan, a filter, a project.
    pub fn new(fields: Arc<ArrowSchema>) -> Self {
        Self {
            fields,
            group_keys: Vec::new(),
            agg_state: Vec::new(),
        }
    }

    /// The state columns an aggregate output carries, or `None` once the schema is
    /// finalized. `#[cfg(test)]` for `planner/translator/schema_tests.rs`, whose component
    /// cannot see `agg_state`.
    #[cfg(test)]
    pub(crate) fn state_for(&self, output: &str) -> Option<&AggStateColumns> {
        self.agg_state.iter().find(|s| s.output == output)
    }
}

/// A sink structurally has no layout and no schema; everything else always has both,
/// which is why they live inside the kind rather than beside it as two `Option`s that
/// have to be `None` together.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    Source {
        layout: PartitionLayout,
        schema: Schema,
    },
    Intermediate {
        layout: PartitionLayout,
        schema: Schema,
    },
    Sink,
}

impl NodeKind {
    /// `None` for a sink, which structurally has neither.
    pub fn layout(&self) -> Option<&PartitionLayout> {
        match self {
            Self::Source { layout, .. } | Self::Intermediate { layout, .. } => Some(layout),
            Self::Sink => None,
        }
    }

    pub fn schema(&self) -> Option<&Schema> {
        match self {
            Self::Source { schema, .. } | Self::Intermediate { schema, .. } => Some(schema),
            Self::Sink => None,
        }
    }
}

/// One sort key: a column ordinal into the declaring node's schema, and its direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnOrder {
    pub column: u32,
    pub ascending: bool,
    pub nulls_first: bool,
}

/// How rows were routed into lanes. `ByHash` is Spark murmur3 seed 42 — the only
/// routing `GpuEmitPartitions` has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyDistribution {
    NotSpecified,
    ByHash { hash_keys: Vec<u32> },
}

impl KeyDistribution {
    /// The `hash_keys ⊆ group columns` rule a final aggregate's input must satisfy:
    /// rows of one group are co-located only when the shuffle keyed on a subset of
    /// the columns being grouped.
    pub(crate) fn is_subset_of(&self, group_columns: &[u32]) -> bool {
        match self {
            Self::NotSpecified => false,
            Self::ByHash { hash_keys } => hash_keys.iter().all(|k| group_columns.contains(k)),
        }
    }
}

/// Two-valued on purpose. A whole-stream order is `BatchSorted` meeting `SingleBatch`,
/// derived by [`PartitionLayout::is_stream_sorted`] rather than declared, so there is no
/// second way to say it. It becomes a real third state only under #138's ranged merge
/// emission, which orders a stream across several batches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortOrder {
    NotSpecified,
    BatchSorted { columns: Vec<ColumnOrder> },
}

impl SortOrder {
    /// An order on no columns is no order, so it canonicalizes — otherwise two layouts
    /// that mean the same thing compare unequal.
    pub(crate) fn batch_sorted(columns: Vec<ColumnOrder>) -> Self {
        if columns.is_empty() {
            Self::NotSpecified
        } else {
            Self::BatchSorted { columns }
        }
    }

    pub fn is_batch_sorted(&self) -> bool {
        matches!(self, Self::BatchSorted { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchLayout {
    SingleBatch,
    MultipleBatches,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionLayout {
    pub n: usize,
    pub key_distribution: KeyDistribution,
    pub sort_order: SortOrder,
    pub batch_layout: BatchLayout,
}

impl PartitionLayout {
    /// N lanes, nothing else declared — what a scan or a shuffle-free chain emits.
    pub fn new(n: usize) -> Self {
        Self {
            n,
            key_distribution: KeyDistribution::NotSpecified,
            sort_order: SortOrder::NotSpecified,
            batch_layout: BatchLayout::MultipleBatches,
        }
    }

    /// Whole stream ordered, not merely each batch — what a top-N after a sort needs.
    pub(crate) fn is_stream_sorted(&self) -> bool {
        self.sort_order.is_batch_sorted() && self.batch_layout == BatchLayout::SingleBatch
    }
}

/// What sql asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggFunc {
    Sum,
    Min,
    Max,
    Count,
    Avg,
    Stddev,
    Var,
}

/// What a node runs. `Avg` is never one — decomposing it is the point — and `MergeM2`
/// is never an `AggFunc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanAgg {
    Sum,
    Min,
    Max,
    Count,
    Mean,
    M2,
    MergeM2,
}

/// How a state merges. `Combined` exists only because `merge_m2` is not a per-column
/// reduction: it needs the count-weighted mean and the cross term.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Merge {
    PerColumn(&'static [PlanAgg]),
    Combined(PlanAgg),
}

/// `state` pairs each column's name suffix with the aggregator producing it, so a column
/// and its aggregator cannot desync. `merge` is listed rather than derived: the rule
/// would be "the same aggregator, except count merges by sum", and that exception is the
/// whole content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decomposition {
    pub state: &'static [(&'static str, PlanAgg)],
    pub merge: Merge,
}

/// One aggregate as sql wrote it: the function, and the `ddof` that separates the sample
/// forms from the population ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AggSpec {
    pub func: AggFunc,
    pub ddof: u32,
}

impl PlanAgg {
    /// The name the aggregator goes by, in a plan line and in DataFusion's own state
    /// field names (`avg(x)[count]`), which is how our state columns find their types.
    pub(crate) fn tag(self) -> &'static str {
        match self {
            Self::Sum => "sum",
            Self::Min => "min",
            Self::Max => "max",
            Self::Count => "count",
            Self::Mean => "mean",
            Self::M2 => "m2",
            Self::MergeM2 => "merge_m2",
        }
    }
}

/// One aggregator call: what it runs, over which expressions, and the columns it
/// produces. `merge_m2` is the reason `outputs` is a list — it returns its three state
/// columns together.
#[derive(Debug, Clone, PartialEq)]
pub struct AggCall {
    pub func: PlanAgg,
    pub args: Vec<Expr>,
    pub outputs: Vec<Field>,
}

// The plan nodes, grouped by family; the downcast registry over them is at the end of this
// file.

#[derive(Debug)]
pub struct GpuFilter {
    kind: NodeKind,
    pub predicate: Expr,
    /// DataFusion's filter projects as well as filtering, and the wire format carries the
    /// same pair. Dropping it would leave this node declaring its child's columns while
    /// emitting fewer, and every ordinal above it reading the wrong one.
    pub projection: Option<Vec<u32>>,
    input: Box<dyn GpuNode>,
}

impl GpuFilter {
    pub fn new(
        input: Box<dyn GpuNode>,
        predicate: Expr,
        projection: Option<Vec<u32>>,
        schema: Schema,
    ) -> Self {
        new_filter(input, predicate, projection, schema)
    }
}

#[derive(Debug)]
pub struct GpuProject {
    kind: NodeKind,
    pub exprs: Vec<NamedExpr>,
    input: Box<dyn GpuNode>,
}

impl GpuProject {
    /// `schema` is DataFusion's own for this projection — the types it coerced to, not a
    /// second derivation of them.
    pub fn new(input: Box<dyn GpuNode>, exprs: Vec<NamedExpr>, schema: Schema) -> Self {
        new_project(input, exprs, schema)
    }
}

/// Sorts each input batch independently, so the batches are individually ordered and
/// collectively not — an accumulator above it is what makes a stream sorted. `fetch` is
/// DataFusion's, replicated onto every stage of the decomposition, which is sound
/// because the top n of a union is the top n of each part's top n.
#[derive(Debug)]
pub struct GpuSort {
    kind: NodeKind,
    pub keys: Vec<ColumnOrder>,
    pub fetch: Option<usize>,
    input: Box<dyn GpuNode>,
}

impl GpuSort {
    pub fn new(input: Box<dyn GpuNode>, keys: Vec<ColumnOrder>, fetch: Option<usize>) -> Self {
        new_sort(input, keys, fetch)
    }
}

/// Concatenates a lane's batches into one at done.
#[derive(Debug)]
pub struct GpuCoalesceAllBatches {
    kind: NodeKind,
    input: Box<dyn GpuNode>,
}

impl GpuCoalesceAllBatches {
    pub fn new(input: Box<dyn GpuNode>) -> Self {
        new_coalesce_all_batches(input)
    }
}

/// Accumulates a lane's sorted batches and merges them into one at done, so its output
/// is stream-sorted rather than batch-sorted. Streaming emission is #138.
#[derive(Debug)]
pub struct GpuAccumulateBatchesAndSort {
    kind: NodeKind,
    pub keys: Vec<ColumnOrder>,
    pub fetch: Option<usize>,
    input: Box<dyn GpuNode>,
}

impl GpuAccumulateBatchesAndSort {
    pub fn new(input: Box<dyn GpuNode>, keys: Vec<ColumnOrder>, fetch: Option<usize>) -> Self {
        new_accumulate_batches_and_sort(input, keys, fetch)
    }
}

/// A mid-plan limit: `skip..skip+fetch` over a one-lane stream of any number of batches.
/// It streams and holds nothing — a batch outside the interval is released uncalled, one
/// inside is forwarded untouched, and only the two straddling its ends are sliced.
#[derive(Debug)]
pub struct GpuLimit {
    kind: NodeKind,
    pub interval: RowInterval,
    input: Box<dyn GpuNode>,
}

impl GpuLimit {
    pub fn new(input: Box<dyn GpuNode>, interval: RowInterval) -> Self {
        new_limit(input, interval)
    }
}

/// Which side of the decomposition a node runs: state built from raw values, or state
/// merged from state. `Partial` and `Merge` on the wire — never `Final`, which also
/// finalizes, and in this mode a finalize is a project of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Init,
    Merge,
}

/// One aggregator as both engines name it: the SQL name the executor knows, the call whose
/// arguments it reads, and the output column it fills.
///
/// Order is load-bearing and not cosmetic. A state-shaped input is read positionally, by a
/// cursor walking each aggregate's state width, so an aggregate listed out of order reads
/// another one's columns. Everything but the Welford triple is one of ours to one SQL name;
/// the triple folds into the `stddev`/`var` it decomposes, at the position of the first of
/// its three, because neither engine has an `m2` of its own.
pub(crate) struct StateFunc<'a> {
    pub name: &'static str,
    pub call: &'a AggCall,
    pub alias: String,
    /// Whether this one is a folded triple, which is what makes its state three columns
    /// wide rather than one.
    pub welford: bool,
}

/// The aggregators and the optional `final` list every aggregate node carries. A node
/// with no `final` emits its state; one with a `final` emits the finalized columns, and
/// that is the only thing distinguishing the positions — the single-node shortcut is
/// init aggregators and finalize expressions on the same node.
#[derive(Debug)]
pub struct AggregateBody {
    pub group_by: Vec<Expr>,
    /// One mask per grouping set, in key order — true where that key is NULL in that set.
    /// Empty unless this node expands grouping sets, which only an init node does: it
    /// emits `__grouping_id` as an ordinary column and every node above groups on the
    /// keys plus that column.
    pub grouping_sets: Vec<Vec<bool>>,
    /// The NULL substituted for each key a set excludes, in key order.
    pub null_exprs: Vec<Expr>,
    pub aggs: Vec<AggCall>,
    /// One expression per aggregate output column, and not per output column: a group key is
    /// not finalized and is not here, so this list is shorter than the node's output
    /// schema by the number of keys. The project that carries it emits the keys first, and
    /// `recipe::aggregate_writer::finalize_project` is the one place that rule lives.
    pub finalize: Option<Vec<NamedExpr>>,
}

impl AggregateBody {
    /// References inside `aggs` index the node's input; references inside `finalize`
    /// index the node's own intermediate table, `[group keys…, state columns…]`.
    fn validate(&self, node: &str, input: &Schema, intermediate: &Schema) -> Result<(), PlanError> {
        for key in &self.group_by {
            check_column_refs(key, input, node)?;
        }
        for call in &self.aggs {
            for arg in &call.args {
                check_column_refs(arg, input, node)?;
            }
        }
        for column in self.finalize.iter().flatten() {
            check_column_refs(&column.expr, intermediate, node)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct GpuAggregate {
    kind: NodeKind,
    pub body: AggregateBody,
    intermediate: Schema,
    input: Box<dyn GpuNode>,
}

impl GpuAggregate {
    /// `intermediate` is `[group keys…, state columns…]` — what the aggregators produce,
    /// and what a finalize expression reads. It is also the output schema where there is
    /// no finalize.
    pub fn new(
        input: Box<dyn GpuNode>,
        body: AggregateBody,
        intermediate: Schema,
        schema: Schema,
    ) -> Self {
        new_aggregate(input, body, intermediate, schema)
    }
}

impl GpuAggregate {
    /// `[group keys…, state columns…]` — what the aggregators produce, and where the
    /// state annotations live. The output schema is the finalized one where this node
    /// finalizes, so a consumer of the state reads this instead.
    pub fn intermediate(&self) -> &Schema {
        &self.intermediate
    }
}

#[derive(Debug)]
pub struct GpuAggregateBatches {
    kind: NodeKind,
    pub body: AggregateBody,
    intermediate: Schema,
    input: Box<dyn GpuNode>,
}

impl GpuAggregateBatches {
    pub fn new(
        input: Box<dyn GpuNode>,
        body: AggregateBody,
        intermediate: Schema,
        schema: Schema,
    ) -> Self {
        new_aggregate_batches(input, body, intermediate, schema)
    }
}

impl GpuAggregateBatches {
    /// The state this node merges into, before any finalize of its own — see
    /// [`GpuAggregate::intermediate`].
    pub fn intermediate(&self) -> &Schema {
        &self.intermediate
    }
}

/// DataFusion's join type, restricted to what a nested-loop join can run: the C++ rejects
/// anything else outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NestedLoopJoinType {
    Inner,
    Left,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinSide {
    Build,
    Probe,
}

/// Where one column of a join filter's own table comes from. The filter is written
/// against a schema of its own — neither side's, and not the joined one — so its
/// ordinals mean nothing without this map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoinFilterColumn {
    pub side: JoinSide,
    pub index: u32,
}

#[derive(Debug)]
pub struct GpuCrossJoin {
    kind: NodeKind,
    /// Ordinals into the crossed table, `[build columns…, probe columns…]`. `None` is
    /// every column of it — a `CrossJoinExec` has no projection, and a predicate-free
    /// nested-loop join that lands here may.
    pub projection: Option<Vec<u32>>,
    build: Box<dyn GpuNode>,
    probe: Box<dyn GpuNode>,
}

impl GpuCrossJoin {
    pub fn new(
        build: Box<dyn GpuNode>,
        probe: Box<dyn GpuNode>,
        projection: Option<Vec<u32>>,
        schema: Schema,
    ) -> Self {
        Self {
            kind: NodeKind::Intermediate {
                layout: joined_layout(),
                schema,
            },
            projection,
            build,
            probe,
        }
    }
}

/// The predicate is the join: `conditional_inner_join` evaluates it per pair, or a cross
/// join and a mask where it is not AST-able.
#[derive(Debug)]
pub struct GpuNestedLoopJoin {
    kind: NodeKind,
    pub join_type: NestedLoopJoinType,
    pub filter: Expr,
    /// One entry per column the filter's own schema has, in its order.
    pub filter_columns: Vec<JoinFilterColumn>,
    /// Ordinals into the crossed table, as DataFusion computed them. Dropping it leaves
    /// the node declaring the projected columns and emitting all of them, so every
    /// ordinal above it reads one column of some other one (#135).
    pub projection: Option<Vec<u32>>,
    build: Box<dyn GpuNode>,
    probe: Box<dyn GpuNode>,
}

impl GpuNestedLoopJoin {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        build: Box<dyn GpuNode>,
        probe: Box<dyn GpuNode>,
        join_type: NestedLoopJoinType,
        filter: Expr,
        filter_columns: Vec<JoinFilterColumn>,
        projection: Option<Vec<u32>>,
        schema: Schema,
    ) -> Self {
        Self {
            kind: NodeKind::Intermediate {
                layout: joined_layout(),
                schema,
            },
            join_type,
            filter,
            filter_columns,
            projection,
            build,
            probe,
        }
    }
}

/// What the capability matrix says about one join mode: whether the probe side can stream
/// batch by batch, and whether the lane owes a pass at done for what a streamed probe
/// cannot know — which build rows matched at least once (#136).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoinCapability {
    pub probe_streams: bool,
    pub needs_finish: bool,
}

impl JoinCapability {
    /// Whether the whole join is one call: no probe keys kept and no finish pass.
    ///
    /// True where nothing needs a finish, and also where the probe cannot stream — the
    /// planner makes that probe a single batch, and one call over the whole of it is the
    /// plain join node the wire has always carried. Asked by the recipe writer for what to
    /// publish and by an executor for what to build, because answering it twice is how a
    /// filtered semi join ended up on the finish path with its residual dropped.
    pub(crate) fn answers_in_one_call(&self) -> bool {
        !self.probe_streams || !self.needs_finish
    }
}

/// An equi-join: the build side is one batch per lane, the probe streams unless the
/// capability matrix says otherwise, and lane p of each side holds exactly the rows that
/// can match lane p of the other.
#[derive(Debug)]
pub struct GpuHashJoin {
    kind: NodeKind,
    pub join_type: JoinType,
    /// (build ordinal, probe ordinal) per key, in the order the join hashes them.
    pub keys: Vec<(u32, u32)>,
    pub filter: Option<Expr>,
    pub filter_columns: Vec<JoinFilterColumn>,
    /// From DataFusion, per join: `false` — the SQL default — means a NULL key matches
    /// nothing, `true` is what a set operation lowered to a join needs.
    pub null_equals_null: bool,
    pub projection: Option<Vec<u32>>,
    build: Box<dyn GpuNode>,
    probe: Box<dyn GpuNode>,
}

impl GpuHashJoin {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        build: Box<dyn GpuNode>,
        probe: Box<dyn GpuNode>,
        join_type: JoinType,
        keys: Vec<(u32, u32)>,
        filter: Option<Expr>,
        filter_columns: Vec<JoinFilterColumn>,
        null_equals_null: bool,
        projection: Option<Vec<u32>>,
        schema: Schema,
    ) -> Self {
        new_hash_join(
            build,
            probe,
            join_type,
            keys,
            filter,
            filter_columns,
            null_equals_null,
            projection,
            schema,
        )
    }

    pub fn capability(&self) -> Result<JoinCapability, PlanError> {
        capability(self.join_type, self.filter.is_some())
    }
}

/// N lanes into 1, forwarding each batch as it is visited, round-robin. It accumulates
/// nothing and makes no backend call — the driver owns the rotation.
#[derive(Debug)]
pub struct GpuMergePartitions {
    kind: NodeKind,
    input: Box<dyn GpuNode>,
}

impl GpuMergePartitions {
    pub fn new(input: Box<dyn GpuNode>) -> Self {
        new_merge_partitions(input)
    }
}

/// One lane into N, by Spark murmur3 on the hash keys — the only routing there is, and
/// the same one both engines use, so a row lands in the same lane on either.
/// Streaming: one scatter call per input batch, N outputs, some of them empty.
#[derive(Debug)]
pub struct GpuEmitPartitions {
    kind: NodeKind,
    pub hash_keys: Vec<u32>,
    input: Box<dyn GpuNode>,
}

impl GpuEmitPartitions {
    pub fn new(input: Box<dyn GpuNode>, hash_keys: Vec<u32>, n: usize) -> Self {
        new_emit_partitions(input, hash_keys, n)
    }
}

/// N lanes of sorted batches into one sorted batch: every k·m batch goes into one merge
/// at done, and the `fetch` is applied to the result.
#[derive(Debug)]
pub struct GpuMergeSortedPartitions {
    kind: NodeKind,
    pub keys: Vec<ColumnOrder>,
    pub fetch: Option<usize>,
    input: Box<dyn GpuNode>,
}

impl GpuMergeSortedPartitions {
    pub fn new(input: Box<dyn GpuNode>, keys: Vec<ColumnOrder>, fetch: Option<usize>) -> Self {
        new_merge_sorted_partitions(input, keys, fetch)
    }
}

/// Reads its lane's row groups out of the partitioner's mapping. The mapping is stored
/// verbatim — partitions outermost, batches within, row groups innermost — because the
/// loader executes it, the golden prints it and validation counts lanes off it.
#[derive(Debug)]
pub struct GpuLoadParquet {
    kind: NodeKind,
    pub table: String,
    /// The one file the row-group indices below are numbered in — taken from the metadata
    /// read that produced them, never assembled beside it.
    pub file: String,
    pub projection: Vec<u32>,
    pub partition_groups: Vec<Vec<Vec<u32>>>,
    /// The row groups the mapping addresses, with their rows and their parquet bytes over
    /// the projected columns — the only real numbers a plan-time model has, and what lets
    /// the estimator price the batches this mapping actually produces rather than the ones
    /// a budget would have afforded.
    pub survivors: Vec<RowGroupMeta>,
    /// Per projected column: whether the surviving row groups hold a NULL in it. The leaf
    /// of the null analysis, and a statistic rather than a declaration.
    pub can_be_null: Vec<bool>,
    /// A limit pushed into the scan by DataFusion, not one this mode derived.
    pub limit: Option<usize>,
}

impl GpuLoadParquet {
    pub fn new(
        table: String,
        projection: Vec<u32>,
        partition_groups: Vec<Vec<Vec<u32>>>,
        scan: &ScanMetadata,
        limit: Option<usize>,
        schema: Schema,
    ) -> Self {
        new_load_parquet(table, projection, partition_groups, scan, limit, schema)
    }
}

impl GpuLoadParquet {
    pub fn rows(&self) -> u64 {
        self.survivors.iter().map(|group| group.rows).sum()
    }

    pub fn bytes(&self) -> u64 {
        self.survivors.iter().map(|group| group.bytes).sum()
    }

    /// The largest batch the mapping produces. One batch per lane, one per row group and a
    /// budgeted size are three different answers to this, which is why the model reads it
    /// off the mapping rather than off the budget.
    pub(crate) fn largest_batch_bytes(&self) -> u64 {
        largest_batch_bytes(self)
    }
}

/// Output lanes are the sum of its branches', and output lane k is served by exactly one
/// of them, so no row changes lane and no lane waits on another. The hash a branch
/// carried says nothing about the union's numbering, so it goes.
#[derive(Debug)]
pub struct GpuUnion {
    kind: NodeKind,
    branches: Vec<Box<dyn GpuNode>>,
}

impl GpuUnion {
    pub fn new(branches: Vec<Box<dyn GpuNode>>, schema: Schema) -> Self {
        new_union(branches, schema)
    }
}

/// Output lane p is lane p of each branch, which is why every branch must carry the same
/// hash: the distribution is what makes lane p of one branch belong beside lane p of the
/// next, and it survives because no row changes lane.
#[derive(Debug)]
pub struct GpuInterleave {
    kind: NodeKind,
    branches: Vec<Box<dyn GpuNode>>,
}

impl GpuInterleave {
    pub fn new(branches: Vec<Box<dyn GpuNode>>, schema: Schema) -> Self {
        new_interleave(branches, schema)
    }
}

/// Carries a root-adjacent limit's `skip`/`fetch`, because the interval belongs to the
/// crossing: it is a statement about which rows are worth moving over PCIe, and trimming
/// after the transfer ships an unbounded prefix to drop it.
#[derive(Debug)]
pub struct GpuUnload {
    kind: NodeKind,
    pub interval: Option<RowInterval>,
    input: Box<dyn GpuNode>,
}

impl GpuUnload {
    pub fn new(input: Box<dyn GpuNode>, interval: Option<RowInterval>) -> Self {
        Self {
            kind: NodeKind::Sink,
            interval,
            input,
        }
    }
}

/// One surviving row group, in file order. `index` is its index in the file, which is
/// what the scan passes to `set_row_groups` — survivors are post-pruning, so it is not
/// the position in this slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowGroupMeta {
    pub index: u32,
    pub rows: u64,
    pub bytes: u64,
}

/// How a lane's row groups are cut into batches. Three named forms rather than one number
/// with special values: which one a mode uses is a statement about the mode, and a target
/// of one byte reaching the same place would hide it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Batching {
    /// One batch per lane — the whole chunk arrives at once.
    Off,
    /// One batch per row group: the finest the mapping can express, since a row group is
    /// its minimum granularity, and the only form that needs no budget.
    PerRowGroup,
    /// Batches packed to the size the estimator solved for this source.
    Sized { target_batch_bytes: usize },
}

/// What one read of a scan's metadata tells the planner: the row groups it will read, and
/// whether each projected column has a NULL in any of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanMetadata {
    /// The one file the groups below are numbered in. DataFusion's file groups are byte
    /// ranges rather than files, so several of them carry one path; a row-group index means
    /// nothing without the file it indexes, and the node reads its own from here.
    pub file: String,
    pub groups: Vec<RowGroupMeta>,
    /// Per projected column, in projection order. Declared nullability says nothing — every
    /// column in both benchmarks is declared nullable, primary keys included — so this is
    /// the statistic instead, and an absent count reads as "yes" rather than "no".
    pub can_be_null: Vec<bool>,
}

/// A limit's `skip`/`fetch`, carried by whichever node owns the interval: a mid-plan
/// `GpuLimit`, or the `GpuUnload` that absorbed a root-adjacent one. Intervals nest — each
/// counts the stream its own node is handed — and the spec's limit lowering rule says why
/// only the non-adjacent form ever arrives that way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowInterval {
    pub skip: u64,
    pub fetch: Option<u64>,
}

impl RowInterval {
    /// The row after the last one wanted, counting from the start of this node's stream.
    /// `None` is a pure offset: no prefix determines the answer, so it never satisfies.
    pub fn stop(&self) -> Option<u64> {
        self.fetch.map(|fetch| self.skip + fetch)
    }

    /// True once no further row could change the answer — what `is_satisfied` asks.
    pub(crate) fn satisfied_by(&self, seen: u64) -> bool {
        self.stop().is_some_and(|stop| seen >= stop)
    }

    /// Which rows of the next batch are wanted, or `None` to release it uncalled. `seen`
    /// is how many rows of the stream have already gone past this node, which is a count
    /// across lanes and therefore the driver's rather than an executor's.
    pub(crate) fn range_of(&self, seen: u64, n_rows: u64) -> Option<RowRange> {
        interval::range_of(self, seen, n_rows)
    }
}

/// What a plan node offers the driver and the validator.
pub trait GpuNode: std::fmt::Debug {
    /// Layout and schema live inside the kind.
    fn kind(&self) -> &NodeKind;

    /// What a plan line and a validation message call this node. The registry is the
    /// mapping, so a node kind is named in one place; a node outside it — a hand-built
    /// one under test — says its own name rather than reaching a registry it is not in.
    fn name(&self) -> &'static str {
        node_name(self.as_any())
    }

    fn children(&self) -> Vec<&dyn GpuNode>;

    /// Checks children's schemas, partition topology, key distribution, sortedness and
    /// batch layout against this node's requirements, and captured column indices
    /// against child schemas. Runs before the generic structural rules, because a node
    /// can name the fix where a generic rule can only say what is wrong.
    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError>;

    /// `Some` only where a limit's interval landed — see the limit lowering rule.
    fn row_interval(&self) -> Option<RowInterval> {
        None
    }

    /// The one downcast point. Consumers that need a node's own parameters — the
    /// renderer, a backend's executor match, the serializer — go through
    /// [`as_node_ref`] rather than downcasting here.
    fn as_any(&self) -> &dyn Any;
}

/// Every node kind, as a borrow of the concrete node. Adding a node is one line here and
/// an exhaustive match everywhere it is consumed — the renderer, a backend's executor
/// match, the serializer — rather than a downcast chain per consumer.
pub enum NodeRef<'a> {
    LoadParquet(&'a GpuLoadParquet),
    Filter(&'a GpuFilter),
    Project(&'a GpuProject),
    Sort(&'a GpuSort),
    CoalesceAllBatches(&'a GpuCoalesceAllBatches),
    AccumulateBatchesAndSort(&'a GpuAccumulateBatchesAndSort),
    Limit(&'a GpuLimit),
    Aggregate(&'a GpuAggregate),
    AggregateBatches(&'a GpuAggregateBatches),
    Join(&'a GpuHashJoin),
    CrossJoin(&'a GpuCrossJoin),
    NestedLoopJoin(&'a GpuNestedLoopJoin),
    MergePartitions(&'a GpuMergePartitions),
    EmitPartitions(&'a GpuEmitPartitions),
    MergeSortedPartitions(&'a GpuMergeSortedPartitions),
    Union(&'a GpuUnion),
    Interleave(&'a GpuInterleave),
    Unload(&'a GpuUnload),
}

pub fn as_node_ref(node: &dyn GpuNode) -> NodeRef<'_> {
    node_ref_of(node.as_any())
}

/// The registry without its panic, for the generic validation pass: a node it does not
/// know is a hand-built one under test, which declares none of the parameters that pass
/// reads. Every consumer of those parameters goes through `as_node_ref` instead.
pub(crate) fn try_as_node_ref(node: &dyn GpuNode) -> Option<NodeRef<'_>> {
    try_node_ref_of(node.as_any())
}

/// Off the erased value rather than the node, so a node can reach the registry from a
/// default trait method — where `Self` is not yet known to be sized.
fn node_ref_of(any: &dyn std::any::Any) -> NodeRef<'_> {
    try_node_ref_of(any).expect("a plan node outside the registry reached a consumer of it")
}

fn try_node_ref_of(any: &dyn std::any::Any) -> Option<NodeRef<'_>> {
    if let Some(n) = any.downcast_ref::<GpuLoadParquet>() {
        Some(NodeRef::LoadParquet(n))
    } else if let Some(n) = any.downcast_ref::<GpuFilter>() {
        Some(NodeRef::Filter(n))
    } else if let Some(n) = any.downcast_ref::<GpuProject>() {
        Some(NodeRef::Project(n))
    } else if let Some(n) = any.downcast_ref::<GpuSort>() {
        Some(NodeRef::Sort(n))
    } else if let Some(n) = any.downcast_ref::<GpuCoalesceAllBatches>() {
        Some(NodeRef::CoalesceAllBatches(n))
    } else if let Some(n) = any.downcast_ref::<GpuAccumulateBatchesAndSort>() {
        Some(NodeRef::AccumulateBatchesAndSort(n))
    } else if let Some(n) = any.downcast_ref::<GpuLimit>() {
        Some(NodeRef::Limit(n))
    } else if let Some(n) = any.downcast_ref::<GpuAggregate>() {
        Some(NodeRef::Aggregate(n))
    } else if let Some(n) = any.downcast_ref::<GpuAggregateBatches>() {
        Some(NodeRef::AggregateBatches(n))
    } else if let Some(n) = any.downcast_ref::<GpuHashJoin>() {
        Some(NodeRef::Join(n))
    } else if let Some(n) = any.downcast_ref::<GpuCrossJoin>() {
        Some(NodeRef::CrossJoin(n))
    } else if let Some(n) = any.downcast_ref::<GpuNestedLoopJoin>() {
        Some(NodeRef::NestedLoopJoin(n))
    } else if let Some(n) = any.downcast_ref::<GpuMergePartitions>() {
        Some(NodeRef::MergePartitions(n))
    } else if let Some(n) = any.downcast_ref::<GpuEmitPartitions>() {
        Some(NodeRef::EmitPartitions(n))
    } else if let Some(n) = any.downcast_ref::<GpuMergeSortedPartitions>() {
        Some(NodeRef::MergeSortedPartitions(n))
    } else if let Some(n) = any.downcast_ref::<GpuUnion>() {
        Some(NodeRef::Union(n))
    } else if let Some(n) = any.downcast_ref::<GpuInterleave>() {
        Some(NodeRef::Interleave(n))
    } else {
        any.downcast_ref::<GpuUnload>().map(NodeRef::Unload)
    }
}

/// Which executor trait drives a node. Read before an executor exists, since runnability
/// asks for it, so it is derived from the node rather than from what a backend returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorCategory {
    Source,
    Exec,
    BatchAccumulator,
    PartitionAccumulator,
    PartitionEmitter,
    Join,
    BatchForwarder,
    Unload,
}

impl ExecutorCategory {
    /// One instance per (node, lane); the rest are one per node, being the cross-lane
    /// points. A forwarder has no executor at all — the driver owns its rotation.
    pub(crate) fn is_lane_scoped(&self) -> bool {
        matches!(
            self,
            Self::Source | Self::Exec | Self::BatchAccumulator | Self::Join | Self::Unload
        )
    }
}

/// Off the registry rather than beside it: the node set and the category set are one
/// mapping, and a second source of truth for it is a second thing to keep true.
pub fn category_of(node: &dyn GpuNode) -> ExecutorCategory {
    match as_node_ref(node) {
        NodeRef::LoadParquet(_) => ExecutorCategory::Source,
        NodeRef::Filter(_) | NodeRef::Project(_) | NodeRef::Sort(_) | NodeRef::Aggregate(_) => {
            ExecutorCategory::Exec
        }
        NodeRef::CoalesceAllBatches(_)
        | NodeRef::AccumulateBatchesAndSort(_)
        | NodeRef::AggregateBatches(_)
        | NodeRef::Limit(_) => ExecutorCategory::BatchAccumulator,
        NodeRef::MergeSortedPartitions(_) => ExecutorCategory::PartitionAccumulator,
        NodeRef::EmitPartitions(_) => ExecutorCategory::PartitionEmitter,
        NodeRef::Join(_) | NodeRef::CrossJoin(_) | NodeRef::NestedLoopJoin(_) => {
            ExecutorCategory::Join
        }
        NodeRef::MergePartitions(_) | NodeRef::Union(_) | NodeRef::Interleave(_) => {
            ExecutorCategory::BatchForwarder
        }
        NodeRef::Unload(_) => ExecutorCategory::Unload,
    }
}

/// What a node is called, in a plan line and in the validation message that names it.
/// No `Exec` suffix: these are not DataFusion nodes, and after the wire-format rename a
/// line from either family says which mode produced it without a caption.
pub(crate) fn node_name(any: &dyn std::any::Any) -> &'static str {
    match node_ref_of(any) {
        NodeRef::LoadParquet(_) => "GpuLoadParquet",
        NodeRef::Filter(_) => "GpuFilter",
        NodeRef::Project(_) => "GpuProject",
        NodeRef::Sort(_) => "GpuSort",
        NodeRef::CoalesceAllBatches(_) => "GpuCoalesceAllBatches",
        NodeRef::AccumulateBatchesAndSort(_) => "GpuAccumulateBatchesAndSort",
        NodeRef::Limit(_) => "GpuLimit",
        NodeRef::Aggregate(_) => "GpuAggregate",
        NodeRef::AggregateBatches(_) => "GpuAggregateBatches",
        NodeRef::Join(_) => "GpuHashJoin",
        NodeRef::CrossJoin(_) => "GpuCrossJoin",
        NodeRef::NestedLoopJoin(_) => "GpuNestedLoopJoin",
        NodeRef::MergePartitions(_) => "GpuMergePartitions",
        NodeRef::EmitPartitions(_) => "GpuEmitPartitions",
        NodeRef::MergeSortedPartitions(_) => "GpuMergeSortedPartitions",
        NodeRef::Union(_) => "GpuUnion",
        NodeRef::Interleave(_) => "GpuInterleave",
        NodeRef::Unload(_) => "GpuUnload",
    }
}

/// The aggregators a body declares over a state schema, in the order their state columns
/// appear. One rule, read by the recipe writer for the wire and by the CPU backend for
/// DataFusion — a second copy of it would be a second answer to which column is whose.
pub(crate) fn state_funcs<'a>(
    body: &'a AggregateBody,
    state: &'a Schema,
) -> Result<Vec<StateFunc<'a>>, PlanError> {
    aggregate::state_funcs(body, state)
}

/// The columns a state leads with before the first aggregate's: the group list, plus the
/// `__grouping_id` an init expanding grouping sets emits beside the keys and every node
/// above it groups on. The group list alone is one short of the state exactly there, and
/// a state position read one column early names the aggregator before the right one.
pub fn key_width(body: &AggregateBody) -> usize {
    aggregate::key_width(body)
}

/// The finalize as the project it becomes: the group keys straight through, then one
/// expression per aggregate output column, named as the node declares its output.
///
/// A project replaces the row, so the finalize list alone would answer with the finalized
/// columns and no keys to read them by. Both the width and the key positions are checked
/// rather than assumed: the keys are taken by position, so a state whose first columns are
/// not the keys would pass the width check and project state columns as keys.
pub(crate) fn finalize_columns(
    body: &AggregateBody,
    state: &Schema,
    output: &Schema,
) -> Result<Vec<NamedExpr>, PlanError> {
    aggregate::finalize_columns(body, state, output)
}

/// Whether the join's output carries a column from each side. False for the two semi
/// families and for a mark join, whose row is one side's — read by an executor deciding
/// whether the columns it owes have to be invented or only selected.
pub(crate) fn emits_both_sides(join_type: JoinType) -> bool {
    join::emits_both_sides(join_type)
}

/// The three refused shapes are refusals of a defect or a missing cuDF variant, not of
/// this mode: an outer join's residual filter is applied after the outer gather and drops
/// the padded rows (#153), and no swapped `mixed_*` variant exists for the right-handed
/// semi family.
pub fn capability(join_type: JoinType, has_filter: bool) -> Result<JoinCapability, PlanError> {
    join::capability(join_type, has_filter)
}

/// Whether a lane whose build side produced no batch owes no rows at all.
///
/// True where every output row is built from a build row, so an empty build side is an
/// empty answer and the lane can end without a call. False for the three types that
/// preserve unmatched PROBE rows: what they owe is the probe side, padded or not, and
/// making it takes a call over a build table that does not exist.
pub fn empty_build_answers_nothing(join_type: JoinType) -> bool {
    join::empty_build_answers_nothing(join_type)
}

/// What the per-probe-batch join emits, which is not what the node is: a Left emits this
/// batch's matches and waits for the finish, and a Full also emits the probe rows this
/// batch had no match for — batch-local, because the build side was complete before the
/// first call.
///
/// `None` is the build-side semi family, whose probe call is only the key project.
pub fn per_call_join_type(join_type: JoinType) -> Option<JoinType> {
    join::per_call_join_type(join_type)
}

/// The join the finish pass runs against the accumulated keys. Left and Full ask which
/// build rows nothing ever matched; the semi family asks its own question, and asks it
/// with the node's own NULL semantics, so the pass substitutes for a legacy single call
/// rather than improving on it (#59, #80).
pub(crate) fn finish_join_type(join_type: JoinType) -> JoinType {
    join::finish_join_type(join_type)
}

/// How many columns a join emits before any projection of its own — the count half of
/// [`emits`], which is the same fact its carry-over rule reads.
pub(crate) fn emitted_columns(join_type: JoinType, build: usize, probe: usize) -> usize {
    join::emitted_columns(join_type, build, probe)
}

pub fn resolve(name: &str) -> Result<AggSpec, PlanError> {
    aggregates::resolve(name)
}

pub fn decomposition(func: AggFunc) -> Decomposition {
    aggregates::decomposition(func)
}

/// The expression that turns merged state into the aggregate's output column. A rename
/// for the five simple aggregates, a divide for `avg`, and a `CASE` over a `sqrt` for
/// the Welford pair — all of them ordinary IR, which is what replaces the hardwired
/// `avg_div` and `std_finalize` arms.
pub fn finalize(spec: AggSpec, state: &[Field], state_at: u32, out_type: &DataType) -> Expr {
    aggregates::finalize(spec, state, state_at, out_type)
}

/// Post-order, so a child's complaint comes before its parent's.
///
/// Public because the planner is not the only thing that builds a tree: a test that
/// rewrites a planned one into a shape no planner emits needs the same check the planner
/// ran, and the driver does not make it — [`check_canonical_form`] is all it asks for.
pub fn validate(root: &dyn GpuNode) -> Result<(), PlanError> {
    validate::validate(root)
}

/// The canonical-form rules a driver needs to have been applied before it runs, so a mock
/// plan meets the same refusal a planned one would.
pub(crate) fn check_canonical_form(root: &dyn GpuNode) -> Result<(), PlanError> {
    validate::check_canonical_form(root)
}

/// What the plan promised its caller. The layer's contract is that the rows it hands back
/// are the ones DataFusion planned, and nothing else states it: every node below is
/// checked against its own children, so a whole tree can be internally consistent and
/// answer a different query.
pub(crate) fn check_output_schema(root: &dyn GpuNode, planned: &ArrowSchema) -> Result<(), PlanError> {
    validate::check_output_schema(root, planned)
}
