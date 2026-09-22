//! The GPU backend's executors: a node's recipe is the instruction set, and running it is
//! all these do.
//!
//! Nothing here builds a cuDF call of its own, deliberately: the recipe already says which
//! seqs to address and in what order, so an executor that also built calls would be a
//! second path to the same kernels, and the two would drift where the plan golden pins
//! only the recipe.
//!
//! So an executor holds a borrowed session pointer, its recipe's calls, and the schema its
//! output is priced by. Handles thread from one call to the next.

mod accumulate;
mod backend;
mod emit;
mod join;
mod source;

#[cfg(all(test, feature = "gpu"))]
mod gpu_tests;

use std::collections::VecDeque;
use std::sync::Arc;

use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::compute::concat_batches;
use datafusion::arrow::datatypes::{Schema as ArrowSchema, SchemaRef};
use datafusion::arrow::ipc::reader::StreamReader;
use datafusion::common::JoinType;

use peacockdb_ffi::raw::{
    PeacockExecutor, PeacockNodeRegion, PeacockNodeStats, peacock_executor_collect_node_regions,
    peacock_executor_execute_node, peacock_last_error, peacock_result_free,
    peacock_result_from_handle,
};

use crate::common::{declared_precisions, logical_size_from_schema};

use crate::executor::Batch;
use crate::executor::CpuBatch;
use crate::executor::GpuBatch;
use crate::executor::errors::schema_divergence;
use crate::executor::node_timing_on;
use crate::executor::{AbiCall, AbiCalls, AbiTarget, BackendError, CallResult, CallStats};
use crate::executor::{Region, RowRange};
use crate::plan::PlanError;
use crate::wire::{AbiSymbol, CallPattern, FbKind, Input, Recipe, Seq};

/// A `BatchAccumulator` node's executor. What it holds between calls is one of four
/// things, and `accumulate` owns them: a public variant would hand a caller the state a
/// private module keeps.
pub(crate) struct GpuAccumulator {
    state: accumulate::State,
}

/// The one node of the partition-accumulator category: every lane's sorted run merged into
/// one at the last lane's done, and nothing where no lane sent anything — answered here
/// before any call, since a merge of no runs would be a collapse of nothing.
///
/// One call per lane event, since that is what round-robin driving produces, and the call
/// carrying the last `Done` is the emitting one. The handles go into the merge in lane
/// order, which is what makes a tie partition-major rather than arrival-ordered.
pub(crate) struct GpuPartitionAccumulator {
    site: CallSite,
    merge: (Seq, FbKind),
    per_lane: Vec<Vec<GpuBatch>>,
    live: usize,
    schema: SchemaRef,
}

/// The scatter's executor: one call per batch, and the node's lane count of handles out.
pub(crate) struct GpuEmitter {
    site: CallSite,
    seq: Seq,
    kind: FbKind,
    lanes: usize,
    schema: SchemaRef,
}

/// A join before its build side arrives.
pub(crate) struct GpuJoin {
    site: CallSite,
    /// The node's own type, and `None` for the two joins that have none — cross and
    /// nested-loop. What a finish over no keys owes is decided by this rather than read
    /// back off the call list: two different nodes publish a LeftAnti at done, and one of
    /// them owes padded rows.
    join_type: Option<JoinType>,
    per_probe: Vec<join::JoinCall>,
    at_done: Vec<join::JoinCall>,
    keys_schema: Option<SchemaRef>,
    output: SchemaRef,
}

/// A join with its build side set, taking probe batches.
pub(crate) struct GpuProbingJoin {
    join: GpuJoin,
    /// `None` once a call has consumed it, which is the last probe call for a join that
    /// hands it over and the finish call for one that does not.
    build: Option<GpuBatch>,
    /// The probe keys each batch contributed, held until the finish concatenates them.
    accumulated: Vec<GpuBatch>,
    probes: u64,
}

/// A lane's reads, in the order the mapping named them.
///
/// Declared here because `Backend::Source` names it, so a caller holding a `GpuBackend`
/// reaches it without any file importing the type.
pub(crate) struct GpuSource {
    pub(crate) site: CallSite,
    pub(crate) seq: Seq,
    pub(crate) kind: FbKind,
    /// The row groups per batch this lane still owes, front first.
    pub(crate) batches: VecDeque<Vec<u32>>,
    pub(crate) schema: SchemaRef,
}

/// A node's calls, in order — the batch into the first, each output into the next.
///
/// The session pointer is BORROWED, as everywhere on the GPU path: the session outlives
/// every executor drawn from it, and the handles it hands back.
pub(crate) struct GpuExec {
    site: CallSite,
    calls: Vec<(Seq, FbKind)>,
    /// What every call but the last produces. `None` where the node makes one call, which
    /// is every exec node but an aggregate with a finalize.
    intermediate: Option<SchemaRef>,
    schema: SchemaRef,
}

impl GpuExec {
    /// `schema` is what the node declares it produces, which is what prices the batch —
    /// the ABI reports rows and varlen content, and the fixed width per row is the
    /// schema's. `intermediate` prices the calls before the last, whose output no batch is
    /// ever built from: an aggregate's state, read by the finalize above it.
    pub(crate) fn new(
        site: CallSite,
        recipe: &Recipe,
        intermediate: Option<&ArrowSchema>,
        schema: &ArrowSchema,
    ) -> Result<Self, PlanError> {
        let mut calls = Vec::with_capacity(recipe.calls.len());
        for (position, call) in recipe.calls.iter().enumerate() {
            if call.when != CallPattern::PerBatch {
                return Err(PlanError::Invalid(format!(
                    "an exec node calls per batch and this recipe's call {position} is \
                     {:?} — a node whose calls wait for done is an accumulator",
                    call.when
                )));
            }
            let expected = if position == 0 {
                Input::Batch
            } else {
                Input::PriorOutput
            };
            if call.inputs.as_slice() != [expected] {
                return Err(PlanError::Invalid(format!(
                    "an exec node's calls are a straight line — the batch into the first \
                     and each output into the next — and call {position} takes {:?}",
                    call.inputs
                )));
            }
            let (seq, kind) = call.target.ok_or_else(|| {
                PlanError::Invalid(format!(
                    "call {position} takes runtime bounds rather than a seq, which no exec \
                     node does"
                ))
            })?;
            calls.push((seq, kind));
        }
        if calls.len() > 1 && intermediate.is_none() {
            return Err(PlanError::Invalid(format!(
                "a chain of {} calls prices all but the last by the state schema, and this \
                 node declares none — only an aggregate with a finalize chains",
                calls.len()
            )));
        }
        Ok(Self {
            site,
            calls,
            intermediate: intermediate.map(|state| Arc::new(state.clone())),
            schema: Arc::new(schema.clone()),
        })
    }

    /// What the call at `position` produced, priced by the schema that output belongs to.
    fn out_schema(&self, position: usize) -> &SchemaRef {
        match position + 1 == self.calls.len() {
            true => &self.schema,
            false => self
                .intermediate
                .as_ref()
                .expect("a chaining node declares its state schema, checked at construction"),
        }
    }

    /// One batch in, one batch out. The input handle is consumed by the first call and
    /// every intermediate by the call after it, so what is released here is nothing: a
    /// failed call ends the query, and the session it belonged to is torn down with it.
    pub(crate) fn exec(&mut self, batch: GpuBatch) -> CallResult<GpuBatch> {
        // The session is this executor's, not the batch's: a batch carries the pointer so
        // that dropping it can release its handle, and every batch reaching a node was
        // drawn from the session the node was built against.
        let mut calls = AbiCalls::armed(node_timing_on());
        // Only the first call reads a batch this side priced; every later one reads the
        // one before it, whose price this loop computed.
        let mut input = Consumed::of(&batch);
        let (_, mut handle) = batch.consume();
        let mut last = (0u64, 0u64);
        for (position, (seq, kind)) in self.calls.iter().enumerate() {
            let (out, stats) = execute_node(self.site, *seq, *kind, &[vec![handle]])?;
            let out_bytes = priced(stats, self.out_schema(position));
            calls.record(AbiCall {
                seq: *seq,
                target: AbiTarget::Node(*kind),
                call_index: 0,
                in_rows: input.rows,
                in_bytes: input.bytes,
                out_rows: stats.rows,
                out_bytes,
            });
            handle = out;
            input = Consumed {
                rows: stats.rows,
                bytes: out_bytes,
            };
            last = (stats.rows, out_bytes);
        }
        let seq = self
            .calls
            .last()
            .expect("an exec node makes at least one call")
            .0;
        Ok((
            GpuBatch::new(
                self.site.executor,
                handle,
                seq,
                last.0 as usize,
                last.1 as usize,
            ),
            CallStats {
                scratch_bytes: None,
                calls,
            },
        ))
    }
}

/// Where the data leaves the device: one export per handle, over the row range the driver
/// supplies. Named for the call rather than for the node, since `GpuUnload` is the node.
pub(crate) struct GpuExport {
    site: CallSite,
    schema: SchemaRef,
}

impl GpuExport {
    /// A sink declares no schema of its own, so this is its input's — the columns that
    /// cross the boundary.
    pub(crate) fn new(site: CallSite, schema: &ArrowSchema) -> Self {
        Self {
            site,
            schema: Arc::new(schema.clone()),
        }
    }

    /// The export does not consume the handle, so the batch is released here by going out
    /// of scope — which is the whole of what the row range buys: the rows wanted cross
    /// PCIe rather than the batch they sit in.
    pub(crate) fn unload(&mut self, batch: GpuBatch, rows: RowRange) -> CallResult<CpuBatch> {
        let precisions = declared_precisions(&self.schema);
        let taken = Consumed::of(&batch);
        // The export carries no seq of its own, so the region C++ opens is charged to the
        // node that produced the handle — and so is the call journalled here, or the two
        // records do not meet.
        let seq = batch.producer();
        let mut ipc: *mut u8 = std::ptr::null_mut();
        let mut len = 0u64;
        let rc = unsafe {
            peacock_result_from_handle(
                self.site.executor,
                batch.handle(),
                rows.offset,
                rows.length,
                precisions.as_ptr(),
                precisions.len() as u64,
                &mut ipc,
                &mut len,
            )
        };
        if rc != 0 {
            return Err(BackendError::new(format!(
                "result_from_handle({}, {}..+{}): {}",
                batch.handle(),
                rows.offset,
                rows.length,
                last_error(self.site.executor)
            )));
        }
        // A range naming no rows exports nothing at all, and there is nothing to free — an
        // empty batch of the sink's schema, not a missing one. The call was still made, so
        // C++ opened a region for it and the journal entry below has to claim that region.
        let exported = match len {
            0 => CpuBatch::new(RecordBatch::new_empty(self.schema.clone())),
            _ => {
                let decoded = decode(unsafe { std::slice::from_raw_parts(ipc, len as usize) });
                unsafe { peacock_result_free(ipc) };
                let batches = decoded?;
                let batch = concat_batches(&self.schema, batches.iter()).map_err(|error| {
                    // concat_batches is Ok on no batches, so a failing stream decoded at
                    // least one, and that one carries the device's schema — the IPC schema
                    // message precedes it.
                    let diverging = schema_divergence(&self.schema, &batches[0].schema());
                    let mut message =
                        format!("the exported stream is not the sink's rows: {error}");
                    if !diverging.is_empty() {
                        message.push_str(&format!(" (declared vs exported: {diverging})"));
                    }
                    BackendError::new(message)
                })?;
                CpuBatch::new(batch)
            }
        };
        let mut calls = AbiCalls::armed(node_timing_on());
        calls.record(AbiCall {
            seq,
            target: AbiTarget::Bare(AbiSymbol::ResultFromHandle),
            call_index: 0,
            in_rows: taken.rows,
            in_bytes: taken.bytes,
            out_rows: exported.num_rows() as u64,
            out_bytes: exported.byte_size() as u64,
        });
        Ok((
            exported,
            CallStats {
                scratch_bytes: None,
                calls,
            },
        ))
    }
}

fn decode(bytes: &[u8]) -> Result<Vec<RecordBatch>, BackendError> {
    StreamReader::try_new(std::io::Cursor::new(bytes), None)
        .and_then(|stream| stream.collect::<Result<Vec<RecordBatch>, _>>())
        .map_err(|error| BackendError::new(format!("decoding the exported IPC stream: {error}")))
}

/// What a call produced, priced by the schema the node declares: the ABI reports rows and
/// varlen content, and the fixed width per row is the schema's. `seq` is the call it came
/// out of, which is what a slice or an export downstream names itself by.
pub(crate) fn produced(
    executor: *mut PeacockExecutor,
    seq: Seq,
    handle: u64,
    stats: PeacockNodeStats,
    schema: &SchemaRef,
) -> GpuBatch {
    GpuBatch::new(
        executor,
        handle,
        seq,
        stats.rows as usize,
        priced(stats, schema) as usize,
    )
}

/// The byte price of what a call answered with — the one formula, for the call in a chain
/// whose output no batch is ever built from.
pub(crate) fn priced(stats: PeacockNodeStats, schema: &SchemaRef) -> u64 {
    logical_size_from_schema(
        schema,
        stats.rows as usize,
        stats.varlen_content_bytes as usize,
    ) as u64
}

/// Where an executor's ABI calls are made from: the session they go through, and the
/// place in the plan they belong to.
///
/// One value rather than a pointer and two numbers threaded separately: every executor
/// already carried the pointer, so this costs no argument, and a call that knows only its
/// seq cannot say which lane it was for. C++ answers with `(seq, partition, call_index)`,
/// where `partition` is the output slot inside one call — at four lanes every one of them
/// reports 0, so the region alone cannot tell lane 1 batch 0 from lane 0 batch 1.
#[derive(Clone, Copy)]
pub(crate) struct CallSite {
    pub(crate) executor: *mut PeacockExecutor,
    /// Post-order position in the plan tree — the index recipes and the report share.
    pub(crate) node: usize,
    pub(crate) lane: usize,
}

/// How many regions the session has recorded, draining none — the `(NULL, 0)` form of the
/// ABI call, and what makes the buffer below exactly the right size.
pub(crate) fn recorded_regions(executor: *mut PeacockExecutor) -> Result<usize, BackendError> {
    let mut count = 0u64;
    let rc = unsafe {
        peacock_executor_collect_node_regions(executor, std::ptr::null_mut(), 0, &mut count)
    };
    match rc {
        0 => Ok(count as usize),
        _ => Err(BackendError::new(format!(
            "collect_node_regions(count): {}",
            last_error(executor)
        ))),
    }
}

/// Drain what the session recorded, after the root export and before the plan ends —
/// the events die with the plan, and the device times do not exist until then.
///
/// Asked, allocated and then taken, because C++ fails a buffer it overruns rather than
/// truncating, and by then the drain has happened: a caller that guessed too small loses
/// the measurement instead of reporting less of it.
pub(crate) fn collect_regions(executor: *mut PeacockExecutor) -> Result<Vec<Region>, BackendError> {
    let cap = recorded_regions(executor)?;
    let mut buf = vec![PeacockNodeRegion::default(); cap];
    let mut count = 0u64;
    let rc = unsafe {
        peacock_executor_collect_node_regions(executor, buf.as_mut_ptr(), cap as u64, &mut count)
    };
    if rc != 0 {
        return Err(BackendError::new(format!(
            "collect_node_regions(cap={cap}): {}",
            last_error(executor)
        )));
    }
    Ok(buf[..count as usize]
        .iter()
        .map(|region| Region {
            seq: region.seq as Seq,
            partition: region.partition as usize,
            call_index: region.call_index,
            host_us: region.host_us,
            device_us: region.device_us,
        })
        .collect())
}

/// The handles a call is about to consume, and the batches they came from dropped without
/// releasing: C++ erases each registry entry as it takes it.
///
/// Priced on the way through, because `consume` is the last place the sizes exist — the far
/// side cannot price what it has already erased (#152).
pub(crate) fn hand_over(batches: Vec<GpuBatch>) -> (Vec<u64>, Consumed) {
    let taken = Consumed::sum(&batches);
    let handles = batches.into_iter().map(|batch| batch.consume().1).collect();
    (handles, taken)
}

/// One `execute_node` over a whole handover, journalled: what the batches held on the way
/// in, and what the call answered with on the way out, priced by the schema that output
/// belongs to. The shape every accumulator's call has, in one place.
pub(crate) fn one_call(
    site: CallSite,
    (seq, kind): (Seq, FbKind),
    batches: Vec<GpuBatch>,
    schema: &SchemaRef,
    calls: &mut AbiCalls,
) -> Result<GpuBatch, BackendError> {
    let (handles, taken) = hand_over(batches);
    let (handle, stats) = execute_node(site, seq, kind, &[handles])?;
    let out = produced(site.executor, seq, handle, stats, schema);
    calls.record(AbiCall {
        seq,
        target: AbiTarget::Node(kind),
        call_index: 0,
        in_rows: taken.rows,
        in_bytes: taken.bytes,
        out_rows: stats.rows,
        out_bytes: out.byte_size() as u64,
    });
    Ok(out)
}

/// What a call reports when it made no ABI call of its own — an accumulator that only
/// took the batch, a scan that had nothing left to read.
///
/// Not `CallStats::default()`: that says nobody was measuring, and on a measured run an
/// empty list is the true answer rather than the absent one.
pub(crate) fn no_abi_calls() -> CallStats {
    CallStats {
        scratch_bytes: None,
        calls: AbiCalls::armed(node_timing_on()),
    }
}

/// What a call was handed, as the caller priced it. Read where the handles are given
/// up: `GpuBatch::consume` is where a batch's own figures stop being reachable.
#[derive(Clone, Copy, Default)]
pub(crate) struct Consumed {
    pub(crate) rows: u64,
    pub(crate) bytes: u64,
}

impl Consumed {
    pub(crate) fn of(batch: &GpuBatch) -> Self {
        Self {
            rows: batch.num_rows() as u64,
            bytes: batch.byte_size() as u64,
        }
    }

    /// A whole handover priced in one pass.
    pub(crate) fn sum(batches: &[GpuBatch]) -> Self {
        batches.iter().fold(Self::default(), |mut total, batch| {
            total.rows += batch.num_rows() as u64;
            total.bytes += batch.byte_size() as u64;
            total
        })
    }
}

/// One `execute_node` against the seq a recipe named, its handles grouped by the child
/// slot each fills. The call CONSUMES them, so a caller hands over batches it will not
/// release itself.
pub(crate) fn execute_node(
    site: CallSite,
    seq: Seq,
    kind: FbKind,
    inputs: &[Vec<u64>],
) -> Result<(u64, PeacockNodeStats), BackendError> {
    let [one] = <[(u64, PeacockNodeStats); 1]>::try_from(execute_node_many(
        site, seq, kind, inputs, 1,
    )?)
    .map_err(|produced| {
        BackendError::new(format!(
            "execute_node(#{seq} {kind}) answered with {} handles — a node driven here maps \
             one call to one output",
            produced.len()
        ))
    })?;
    Ok(one)
}

/// The same call where the output count is a plan value: a scatter's N lanes. Every other
/// node this backend drives takes the one-output form above.
pub(crate) fn execute_node_many(
    site: CallSite,
    seq: Seq,
    kind: FbKind,
    inputs: &[Vec<u64>],
    out_cap: usize,
) -> Result<Vec<(u64, PeacockNodeStats)>, BackendError> {
    // Grouped by the child slot each fills, not flattened: the C++ reads its output count
    // off child 0's, so a join's two handles in one group would ask for two outputs and be
    // refused for a buffer it never needed.
    let counts: Vec<u64> = inputs.iter().map(|group| group.len() as u64).collect();
    let inputs: Vec<u64> = inputs.concat();
    let mut handles = vec![0u64; out_cap];
    let mut stats = vec![PeacockNodeStats::default(); out_cap];
    let mut produced = 0u64;
    let rc = unsafe {
        peacock_executor_execute_node(
            site.executor,
            seq as u64,
            inputs.as_ptr(),
            counts.as_ptr(),
            counts.len() as u64,
            handles.as_mut_ptr(),
            handles.len() as u64,
            &mut produced,
            stats.as_mut_ptr(),
        )
    };
    if rc != 0 {
        return Err(BackendError::new(format!(
            "execute_node(#{seq} {kind}) for node {} lane {}: {}",
            site.node,
            site.lane,
            last_error(site.executor)
        )));
    }
    handles.truncate(produced as usize);
    stats.truncate(produced as usize);
    Ok(handles.into_iter().zip(stats).collect())
}

pub(crate) fn last_error(executor: *mut PeacockExecutor) -> String {
    let message = unsafe { peacock_last_error(executor) };
    if message.is_null() {
        return "no message".to_string();
    }
    unsafe { std::ffi::CStr::from_ptr(message) }
        .to_string_lossy()
        .into_owned()
}
