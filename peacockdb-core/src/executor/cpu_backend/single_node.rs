//! One DataFusion node over batches that are already in hand.
//!
//! DataFusion has no entry point for "run this operator on these rows": a node executes
//! against its children's streams. So the children are replaced by a source that replays
//! the batches the call was handed, and the node is executed against those.

use std::any::Any;
use std::fmt;
use std::sync::{Arc, Mutex};

use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::error::{DataFusionError, Result};
use datafusion::execution::{SendableRecordBatchStream, TaskContext};
use datafusion::physical_expr::EquivalenceProperties;
use datafusion::physical_plan::execution_plan::{Boundedness, EmissionType};
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, Partitioning, PlanProperties, execute_stream,
};
use futures::StreamExt;

/// Execute exactly one plan node, fed by its children's already-computed batches in child
/// order.
pub(crate) async fn execute_single_node(
    node: &Arc<dyn ExecutionPlan>,
    inputs: Vec<Vec<RecordBatch>>,
    task_ctx: Arc<TaskContext>,
) -> Result<Vec<RecordBatch>> {
    let children = node.children();
    if children.len() != inputs.len() {
        return Err(DataFusionError::Internal(format!(
            "execute_single_node: {} expects {} inputs, got {}",
            node.name(),
            children.len(),
            inputs.len()
        )));
    }

    let mut stubs: Vec<Arc<dyn ExecutionPlan>> = Vec::with_capacity(children.len());
    for (child, batches) in children.iter().zip(inputs.into_iter()) {
        let schema = child.schema();
        let eq = child.properties().equivalence_properties().clone();
        let stream = Box::pin(RecordBatchStreamAdapter::new(
            schema.clone(),
            futures::stream::iter(batches.into_iter().map(Ok)),
        )) as SendableRecordBatchStream;
        stubs.push(Arc::new(StreamSourceExec::new(schema, eq, stream)));
    }

    let mut stream = execute_stream(node.clone().with_new_children(stubs)?, task_ctx)?;
    let mut out = Vec::new();
    while let Some(batch) = stream.next().await {
        out.push(batch?);
    }
    Ok(out)
}

/// An `ExecutionPlan` that hands out one pre-built stream. Single-partition and
/// single-use: the stream is taken on the first `execute`, and a second call errors
/// rather than returning an empty one, which would read as a node that produced nothing.
struct StreamSourceExec {
    schema: SchemaRef,
    stream: Mutex<Option<SendableRecordBatchStream>>,
    cache: PlanProperties,
}

impl fmt::Debug for StreamSourceExec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamSourceExec")
            .field("schema", &self.schema)
            .finish()
    }
}

impl StreamSourceExec {
    fn new(
        schema: SchemaRef,
        eq_properties: EquivalenceProperties,
        stream: SendableRecordBatchStream,
    ) -> Self {
        let cache = PlanProperties::new(
            eq_properties,
            Partitioning::UnknownPartitioning(1),
            EmissionType::Incremental,
            Boundedness::Bounded,
        );
        Self {
            schema,
            stream: Mutex::new(Some(stream)),
            cache,
        }
    }
}

impl DisplayAs for StreamSourceExec {
    fn fmt_as(&self, _t: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "StreamSourceExec")
    }
}

impl ExecutionPlan for StreamSourceExec {
    fn name(&self) -> &str {
        "StreamSourceExec"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn schema(&self) -> SchemaRef {
        Arc::clone(&self.schema)
    }
    fn properties(&self) -> &PlanProperties {
        &self.cache
    }
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        vec![]
    }
    fn with_new_children(
        self: Arc<Self>,
        _children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        Ok(self)
    }
    fn execute(
        &self,
        _partition: usize,
        _context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream> {
        self.stream
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| DataFusionError::Internal("StreamSourceExec executed twice".into()))
    }
}
