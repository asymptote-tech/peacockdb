//! The joins on the CPU: one DataFusion join stream per lane, alive from `set_build` to
//! `finish_and_fetch` — the device's session in DataFusion's terms.
//!
//! The build side is collected and hashed once, at the first poll; each probe call feeds
//! one batch through a channel and reads what the stream has ready; the finish closes the
//! channel and reads the rest, which is where a Left, Full or build-side semi join emits
//! what only the last probe batch can settle. Every call answers one batch or none, as the
//! session does (#220).

use std::sync::Arc;

use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::compute::concat_batches;
use datafusion::arrow::datatypes::{Field, Schema as ArrowSchema, SchemaRef};
use datafusion::common::{JoinSide as DfJoinSide, JoinType};
use datafusion::execution::{SendableRecordBatchStream, TaskContext};
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_expr::expressions::Column;
use datafusion::physical_plan::joins::utils::{ColumnIndex, JoinFilter};
use datafusion::physical_plan::joins::{
    CrossJoinExec, HashJoinExec, NestedLoopJoinExec, PartitionMode,
};
use datafusion::physical_plan::projection::ProjectionExec;
use datafusion::physical_plan::{ExecutionPlan, execute_stream};
use futures::channel::mpsc;
use futures::{FutureExt, StreamExt, TryStreamExt};

use super::expr_physical::physical_expr;
use super::single_node::fed;
use super::{CpuJoin, CpuProbingJoin, declared_as, placeholder};
use crate::executor::CpuBatch;
use crate::executor::{BackendError, CallResult, CallStats};
use crate::plan::GpuNode;
use crate::plan::PlanError;
use crate::plan::{GpuCrossJoin, GpuHashJoin, GpuNestedLoopJoin};
use crate::plan::{JoinSide, empty_build_owes_nothing, needs_finish};

/// The DataFusion join a lane will run, over stand-ins for its two sides until the build
/// side arrives.
pub(crate) struct Plan {
    /// A two-child join: the build side first, the probe side second.
    join: Arc<dyn ExecutionPlan>,
    /// The cross join's kept columns, over the crossed table — the one join DataFusion
    /// gives no projection of its own (#207).
    projection: Option<Arc<dyn ExecutionPlan>>,
    /// The node's own type: a cross join's is Inner.
    join_type: JoinType,
    output: SchemaRef,
    ctx: Arc<TaskContext>,
}

/// One lane's join, running.
pub(crate) struct Stream {
    /// The probe side's channel; closed by the finish, which is what ends the stream.
    probe: Option<mpsc::UnboundedSender<RecordBatch>>,
    joined: SendableRecordBatchStream,
    join_type: JoinType,
    output: SchemaRef,
}

impl CpuJoin {
    pub(crate) fn hash(
        node: &GpuHashJoin,
        build: &ArrowSchema,
        probe: &ArrowSchema,
        ctx: Arc<TaskContext>,
    ) -> Result<Self, PlanError> {
        let mut on: Vec<(Arc<dyn PhysicalExpr>, Arc<dyn PhysicalExpr>)> =
            Vec::with_capacity(node.keys.len());
        for (build_ordinal, probe_ordinal) in &node.keys {
            on.push((
                key_column(build, *build_ordinal)?,
                key_column(probe, *probe_ordinal)?,
            ));
        }
        let filter = match &node.filter {
            Some(expr) => Some(join_filter(
                expr,
                &node.filter_columns,
                build,
                probe,
                ctx.as_ref(),
            )?),
            None => None,
        };
        let join = HashJoinExec::try_new(
            placeholder(build),
            placeholder(probe),
            on,
            filter,
            &node.join_type,
            kept(&node.projection),
            // The build side is one resident table, which is what CollectLeft means here as
            // well as on the device.
            PartitionMode::CollectLeft,
            node.null_equals_null,
        )
        .map_err(|error| PlanError::Invalid(format!("GpuHashJoin: {error}")))?;
        Ok(Self::of(Arc::new(join), None, node.join_type, node, ctx))
    }

    pub(crate) fn cross(
        node: &GpuCrossJoin,
        build: &ArrowSchema,
        probe: &ArrowSchema,
        ctx: Arc<TaskContext>,
    ) -> Result<Self, PlanError> {
        let cross = CrossJoinExec::new(placeholder(build), placeholder(probe));
        let projection = match &node.projection {
            Some(columns) => {
                let crossed = cross.schema();
                let mut exprs: Vec<(Arc<dyn PhysicalExpr>, String)> =
                    Vec::with_capacity(columns.len());
                for ordinal in columns {
                    let field = field_at(crossed.as_ref(), *ordinal)?;
                    exprs.push((
                        key_column(crossed.as_ref(), *ordinal)?,
                        field.name().clone(),
                    ));
                }
                let project = ProjectionExec::try_new(exprs, placeholder(crossed.as_ref()))
                    .map_err(|error| {
                        PlanError::Invalid(format!("GpuCrossJoin's projection: {error}"))
                    })?;
                Some(Arc::new(project) as Arc<dyn ExecutionPlan>)
            }
            None => None,
        };
        Ok(Self::of(
            Arc::new(cross),
            projection,
            JoinType::Inner,
            node,
            ctx,
        ))
    }

    pub(crate) fn nested_loop(
        node: &GpuNestedLoopJoin,
        build: &ArrowSchema,
        probe: &ArrowSchema,
        ctx: Arc<TaskContext>,
    ) -> Result<Self, PlanError> {
        let filter = join_filter(
            &node.filter,
            &node.filter_columns,
            build,
            probe,
            ctx.as_ref(),
        )?;
        let join = NestedLoopJoinExec::try_new(
            placeholder(build),
            placeholder(probe),
            Some(filter),
            &node.join_type,
            kept(&node.projection),
        )
        .map_err(|error| PlanError::Invalid(format!("GpuNestedLoopJoin: {error}")))?;
        Ok(Self::of(Arc::new(join), None, node.join_type, node, ctx))
    }

    fn of(
        join: Arc<dyn ExecutionPlan>,
        projection: Option<Arc<dyn ExecutionPlan>>,
        join_type: JoinType,
        node: &dyn GpuNode,
        ctx: Arc<TaskContext>,
    ) -> Self {
        Self {
            plan: Plan {
                join,
                projection,
                join_type,
                output: node
                    .kind()
                    .schema()
                    .expect("a join is not a sink")
                    .fields
                    .clone(),
                ctx,
            },
        }
    }

    /// The stream starts here, over this lane's build batch or over none: an absent build
    /// side is an empty build table (#212), so every type takes this call and the type and
    /// the row count decide what the lane owes, as on the device.
    pub(crate) fn set_build(self, batch: Option<CpuBatch>) -> CallResult<CpuProbingJoin> {
        let build: Vec<RecordBatch> = batch.map(CpuBatch::into_record_batch).into_iter().collect();
        let build_rows: usize = build.iter().map(RecordBatch::num_rows).sum();
        let build_bytes = build.iter().map(RecordBatch::get_array_memory_size).sum();
        let plan = self.plan;
        let (probe, fed_probe) = mpsc::unbounded::<RecordBatch>();
        let sides = vec![
            futures::stream::iter(build.into_iter().map(Ok)).boxed(),
            fed_probe.map(Ok).boxed(),
        ];
        let running = fed(&plan.join, sides)
            .and_then(|join| match &plan.projection {
                Some(project) => project.clone().with_new_children(vec![join]),
                None => Ok(join),
            })
            .and_then(|joined| execute_stream(joined, plan.ctx.clone()))
            .map_err(|error| BackendError::new(format!("starting the join: {error}")))?;
        Ok((
            CpuProbingJoin {
                stream: Stream {
                    probe: Some(probe),
                    joined: running,
                    join_type: plan.join_type,
                    output: plan.output,
                },
                build_bytes,
                owes_nothing: build_rows == 0 && empty_build_owes_nothing(plan.join_type),
            },
            CallStats::default(),
        ))
    }
}

impl CpuProbingJoin {
    /// The build side, held by the stream from `set_build` until the join is dropped.
    pub(crate) fn build_bytes(&self) -> usize {
        self.build_bytes
    }

    /// Read off the join type and the build side's rows at `set_build`, as the device does.
    pub(crate) fn owes_nothing(&self) -> bool {
        self.owes_nothing
    }

    /// One probe batch through the stream. `None` for LeftSemi, LeftAnti and LeftMark, which
    /// answer at the finish alone; one batch, possibly of no rows, for every other type.
    pub(crate) fn probe_and_fetch(&mut self, batch: CpuBatch) -> CallResult<Option<CpuBatch>> {
        let stream = &mut self.stream;
        stream
            .probe
            .as_ref()
            .expect("the probe side stays open until the finish consumes the join")
            .unbounded_send(batch.into_record_batch())
            .map_err(|error| BackendError::new(format!("feeding the probe side: {error}")))?;
        let chunks = ready(&mut stream.joined)?;
        if answers_at_finish_only(stream.join_type) {
            stream.owed_nothing(&chunks, "a probe call")?;
            return Ok((None, CallStats::default()));
        }
        Ok((Some(stream.one_batch(chunks)?), CallStats::default()))
    }

    /// After the last probe: one batch for the five types that finish, `None` for the rest.
    pub(crate) fn finish_and_fetch(mut self) -> CallResult<Option<CpuBatch>> {
        let stream = &mut self.stream;
        drop(stream.probe.take());
        let chunks: Vec<RecordBatch> =
            futures::executor::block_on((&mut stream.joined).try_collect())
                .map_err(|error| BackendError::new(format!("finishing the join: {error}")))?;
        if !needs_finish(stream.join_type) {
            stream.owed_nothing(&chunks, "the finish")?;
            return Ok((None, CallStats::default()));
        }
        Ok((Some(stream.one_batch(chunks)?), CallStats::default()))
    }
}

impl Stream {
    /// What one call answers: every chunk DataFusion split it into, as one batch under the
    /// names the node declares, and a batch of no rows where it yielded none (#208).
    fn one_batch(&self, chunks: Vec<RecordBatch>) -> Result<CpuBatch, BackendError> {
        let joined = concat_batches(&self.joined.schema(), chunks.iter())
            .map_err(|error| BackendError::new(format!("joining a call's chunks: {error}")))?;
        declared_as(joined, &self.output).map(CpuBatch::new)
    }

    /// A call the device answers with handle 0 must have nothing to drop here, or the two
    /// engines part on a row the cpu would lose.
    fn owed_nothing(&self, chunks: &[RecordBatch], call: &str) -> Result<(), BackendError> {
        let rows: usize = chunks.iter().map(RecordBatch::num_rows).sum();
        match rows {
            0 => Ok(()),
            _ => Err(BackendError::new(format!(
                "{call} of a {:?} join answers nothing, and DataFusion emitted {rows} rows there",
                self.join_type
            ))),
        }
    }
}

/// Every chunk the stream has ready. Polling until it is pending answers one probe batch
/// whole only because the build side is already in hand and nothing is spawned beneath the
/// stream, which is how `set_build` builds it.
fn ready(stream: &mut SendableRecordBatchStream) -> Result<Vec<RecordBatch>, BackendError> {
    let mut chunks = Vec::new();
    while let Some(Some(chunk)) = stream.next().now_or_never() {
        chunks.push(chunk.map_err(|error| BackendError::new(format!("probing: {error}")))?);
    }
    Ok(chunks)
}

/// The build-side semi family: its rows are build rows, so none is settled before the last
/// probe batch has gone past.
fn answers_at_finish_only(join_type: JoinType) -> bool {
    matches!(
        join_type,
        JoinType::LeftSemi | JoinType::LeftAnti | JoinType::LeftMark
    )
}

fn kept(projection: &Option<Vec<u32>>) -> Option<Vec<usize>> {
    projection
        .as_ref()
        .map(|columns| columns.iter().map(|column| *column as usize).collect())
}

/// The residual, rebuilt against the intermediate schema its column map names — the same
/// reconstruction the wire's own reader makes, since the map is the same map.
fn join_filter(
    filter: &crate::plan::Expr,
    columns: &[crate::plan::JoinFilterColumn],
    build: &ArrowSchema,
    probe: &ArrowSchema,
    registry: &TaskContext,
) -> Result<JoinFilter, PlanError> {
    let mut fields: Vec<Field> = Vec::with_capacity(columns.len());
    let mut indices = Vec::with_capacity(columns.len());
    for column in columns {
        let (side, schema) = match column.side {
            JoinSide::Build => (DfJoinSide::Left, build),
            JoinSide::Probe => (DfJoinSide::Right, probe),
        };
        fields.push(field_at(schema, column.index)?.as_ref().clone());
        indices.push(ColumnIndex {
            index: column.index as usize,
            side,
        });
    }
    let intermediate = Arc::new(ArrowSchema::new(fields));
    let expression = physical_expr(filter, &intermediate, registry)?;
    Ok(JoinFilter::new(expression, indices, intermediate))
}

fn key_column(schema: &ArrowSchema, ordinal: u32) -> Result<Arc<dyn PhysicalExpr>, PlanError> {
    let field = field_at(schema, ordinal)?;
    Ok(Arc::new(Column::new(field.name(), ordinal as usize)))
}

fn field_at(schema: &ArrowSchema, ordinal: u32) -> Result<&Arc<Field>, PlanError> {
    schema.fields().get(ordinal as usize).ok_or_else(|| {
        PlanError::Invalid(format!(
            "a join reads column {ordinal} of an input with {} columns",
            schema.fields().len()
        ))
    })
}
