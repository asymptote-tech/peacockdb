//! The joins on a device: a session per lane, built once from the build side, probed once
//! per batch and finished once — the four `peacock_join_*` symbols, and nothing else.
//!
//! The session holds the build table, the cuDF join object and the build-matched column
//! between calls, so nothing here copies a handle or keeps probe keys: every handle a call
//! is given is consumed, and every handle it answers is fresh.

use std::sync::Arc;

use datafusion::arrow::datatypes::{DataType, Schema as ArrowSchema};
use datafusion::common::JoinType;

use peacockdb_ffi::raw::{
    PeacockNodeStats, peacock_join_build, peacock_join_finish, peacock_join_probe,
    peacock_join_release,
};

use super::{CallSite, Consumed, GpuJoin, GpuProbingJoin, last_error, produced};
use crate::common::logical_size_from_schema;
use crate::executor::Batch;
use crate::executor::GpuBatch;
use crate::executor::node_timing_on;
use crate::executor::{AbiCall, AbiCalls, AbiTarget, BackendError, CallResult, CallStats};
use crate::plan::PlanError;
use crate::plan::{JoinFilterColumn, JoinSide, empty_build_owes_nothing, needs_finish};
use crate::wire::{AbiSymbol, CallPattern, FbKind, Recipe, Seq};

/// The cuDF hash table per build row: a `static_multiset` of 8-byte slots at load factor
/// 0.5, whatever the key width (design §4.3). The planner's resident model uses the same.
const HASH_TABLE_BYTES_PER_ROW: u64 = 16;

/// Each key-match pair's index maps, before the residual's own columns are gathered for it.
const INDEX_BYTES_PER_PAIR: u64 = 8;

/// What `chunk_bytes = 0` means to the session.
const DEFAULT_CHUNK_BYTES: u64 = 1 << 30;

/// What one join call may spend on pairs, as the session bounds it: `pairs × (8 + the
/// residual's per-row bytes)`, chunked at `chunk_bytes` — `gpu_plan.fbs` has the formula.
#[derive(Clone, Copy)]
pub(crate) struct PairScratch {
    /// Keyed pairs are one per match; a keyless join pairs every build row with every
    /// probe row before its condition keeps any.
    keyed: bool,
    bytes_per_pair: u64,
    chunk_bytes: u64,
}

impl PairScratch {
    /// `chunk_bytes` is the plan's: what every `CudfJoin` in the buffer carries.
    pub(crate) fn new(
        keyed: bool,
        filter_columns: &[JoinFilterColumn],
        build: &ArrowSchema,
        probe: &ArrowSchema,
        chunk_bytes: u64,
    ) -> Self {
        let residual: u64 = filter_columns
            .iter()
            .map(|column| {
                let side = match column.side {
                    JoinSide::Build => build,
                    JoinSide::Probe => probe,
                };
                width_on_device(side.field(column.index as usize).data_type())
            })
            .sum();
        Self {
            keyed,
            bytes_per_pair: INDEX_BYTES_PER_PAIR + residual,
            chunk_bytes: match chunk_bytes {
                0 => DEFAULT_CHUNK_BYTES,
                bytes => bytes,
            },
        }
    }

    /// A keyed probe is priced at one match per probe row, which is the estimate and not a
    /// bound; a keyless one at the whole product. Either way the session never spends more
    /// than one chunk at once.
    pub(crate) fn bytes(&self, build_rows: u64, probe_rows: u64) -> u64 {
        let pairs = match self.keyed {
            true => probe_rows,
            false => build_rows.saturating_mul(probe_rows),
        };
        pairs
            .saturating_mul(self.bytes_per_pair)
            .min(self.chunk_bytes)
    }
}

/// A column's bytes per row as the session counts them (`filter_row_bytes`): the fixed width
/// cuDF holds it at, and 16 for a variable-width one.
fn width_on_device(data_type: &DataType) -> u64 {
    match data_type {
        DataType::Boolean => 1,
        other => other.primitive_width().map_or(16, |width| width as u64),
    }
}

impl GpuJoin {
    /// `recipe` is the session line the wire published: `join_build`, `join_probe` and, for
    /// a finishing type, `join_finish`, all on one `CudfJoin` seq.
    pub(crate) fn new(
        site: CallSite,
        recipe: &Recipe,
        join_type: JoinType,
        scratch: PairScratch,
        output: &ArrowSchema,
    ) -> Result<Self, PlanError> {
        let expected: &[(AbiSymbol, CallPattern)] = if needs_finish(join_type) {
            &[
                (AbiSymbol::JoinBuild, CallPattern::JoinBuild),
                (AbiSymbol::JoinProbe, CallPattern::PerProbeBatch),
                (AbiSymbol::JoinFinish, CallPattern::AtDone),
            ]
        } else {
            &[
                (AbiSymbol::JoinBuild, CallPattern::JoinBuild),
                (AbiSymbol::JoinProbe, CallPattern::PerProbeBatch),
            ]
        };
        let published: Vec<(AbiSymbol, CallPattern)> = recipe
            .calls
            .iter()
            .map(|call| (call.symbol, call.when))
            .collect();
        if published != expected {
            return Err(PlanError::Invalid(format!(
                "a {join_type:?} join is a session — {expected:?} — and its recipe is {recipe}"
            )));
        }
        let seq = match recipe.calls.first().and_then(|call| call.target) {
            Some((seq, FbKind::Join)) => seq,
            other => {
                return Err(PlanError::Invalid(format!(
                    "a join session addresses one CudfJoin, and its recipe names {other:?}"
                )));
            }
        };
        if recipe
            .calls
            .iter()
            .any(|call| call.target != Some((seq, FbKind::Join)))
        {
            return Err(PlanError::Invalid(format!(
                "every call of a join session addresses its one seq, #{seq}: {recipe}"
            )));
        }
        Ok(Self {
            site,
            seq,
            join_type,
            scratch,
            output: Arc::new(output.clone()),
        })
    }

    /// The session's hash table and `matched` column, for `rows` build rows.
    pub(crate) fn built_bytes(&self, rows: u64) -> u64 {
        let table = match self.scratch.keyed {
            true => HASH_TABLE_BYTES_PER_ROW * rows,
            false => 0,
        };
        let matched = match needs_finish(self.join_type) {
            true => rows,
            false => 0,
        };
        table + matched
    }

    /// The session begins here, with this lane's build batch or with none — the session
    /// answers an absent build side itself, so every type takes this call.
    pub(crate) fn set_build(self, build: Option<GpuBatch>) -> CallResult<GpuProbingJoin> {
        let mut calls = AbiCalls::armed(node_timing_on());
        let (handle, taken) = match build {
            Some(batch) => {
                let taken = Consumed::of(&batch);
                (batch.consume().1, taken)
            }
            None => (0, Consumed::default()),
        };
        let mut session = 0u64;
        let mut stats = PeacockNodeStats::default();
        let rc = unsafe {
            peacock_join_build(
                self.site.executor,
                self.seq as u64,
                handle,
                &mut session,
                &mut stats,
            )
        };
        if rc != 0 {
            return Err(self.failed(AbiSymbol::JoinBuild));
        }
        let owes_nothing = taken.rows == 0 && empty_build_owes_nothing(self.join_type);
        calls.record(AbiCall {
            seq: self.seq,
            target: AbiTarget::Node(FbKind::Join),
            call_index: 0,
            in_rows: taken.rows,
            in_bytes: taken.bytes,
            out_rows: 0,
            out_bytes: 0,
        });
        Ok((
            GpuProbingJoin {
                join: self,
                session,
                build_rows: taken.rows,
                build_bytes: taken.bytes,
                owes_nothing,
            },
            CallStats {
                scratch_bytes: None,
                calls,
            },
        ))
    }

    fn failed(&self, symbol: AbiSymbol) -> BackendError {
        BackendError::new(format!(
            "{}(#{}) node {} lane {}: {}",
            symbol.name(),
            self.seq,
            self.site.node,
            self.site.lane,
            last_error(self.site.executor)
        ))
    }
}

impl GpuProbingJoin {
    /// Read off the join type at `set_build`: a build side of no rows, batch or none, and a
    /// type every row of which is built from a build row.
    pub(crate) fn owes_nothing(&self) -> bool {
        self.owes_nothing
    }

    /// What the session holds between calls: the build table, the hash table its keys went
    /// into, and one byte a row saying which matched.
    pub(crate) fn held_bytes(&self) -> u64 {
        self.build_bytes + self.join.built_bytes(self.build_rows)
    }

    /// A probe call: the batch, an output of as many rows, and its pairs. A finish (no
    /// batch): the unmatched build rows, priced as every one of them.
    pub(crate) fn call_bytes(&self, n_rows: u64, n_bytes: u64) -> u64 {
        let output = &self.join.output;
        if n_rows == 0 && n_bytes == 0 {
            return match needs_finish(self.join.join_type) {
                true => logical_size_from_schema(output, self.build_rows as usize, 0) as u64,
                false => 0,
            };
        }
        n_bytes
            + logical_size_from_schema(output, n_rows as usize, 0) as u64
            + self.join.scratch.bytes(self.build_rows, n_rows)
    }

    /// One probe batch, consumed. `None` where the type answers only at finish — LeftSemi,
    /// LeftAnti and LeftMark — and one table, possibly of no rows, for every other.
    pub(crate) fn probe_and_fetch(&mut self, batch: GpuBatch) -> CallResult<Option<GpuBatch>> {
        let taken = Consumed::of(&batch);
        let (_, handle) = batch.consume();
        let mut out = 0u64;
        let mut stats = PeacockNodeStats::default();
        let rc = unsafe {
            peacock_join_probe(
                self.join.site.executor,
                self.session,
                handle,
                &mut out,
                &mut stats,
            )
        };
        if rc != 0 {
            return Err(self.join.failed(AbiSymbol::JoinProbe));
        }
        Ok(self.answered(taken, out, stats))
    }

    /// After the last probe: one table for the five types that finish, `None` for the rest.
    /// The session is released when this returns, by `Drop`.
    pub(crate) fn finish_and_fetch(self) -> CallResult<Option<GpuBatch>> {
        let mut out = 0u64;
        let mut stats = PeacockNodeStats::default();
        let rc = unsafe {
            peacock_join_finish(self.join.site.executor, self.session, &mut out, &mut stats)
        };
        if rc != 0 {
            return Err(self.join.failed(AbiSymbol::JoinFinish));
        }
        Ok(self.answered(Consumed::default(), out, stats))
    }

    /// The handle a call answered, priced by the node's output, and the call journalled —
    /// handle 0 is a call that answered nothing, and it was still made.
    fn answered(
        &self,
        taken: Consumed,
        handle: u64,
        stats: PeacockNodeStats,
    ) -> (Option<GpuBatch>, CallStats) {
        let seq: Seq = self.join.seq;
        let out = (handle != 0).then(|| {
            produced(
                self.join.site.executor,
                seq,
                handle,
                stats,
                &self.join.output,
            )
        });
        let mut calls = AbiCalls::armed(node_timing_on());
        calls.record(AbiCall {
            seq,
            target: AbiTarget::Node(FbKind::Join),
            call_index: 0,
            in_rows: taken.rows,
            in_bytes: taken.bytes,
            out_rows: out.as_ref().map_or(0, |batch| batch.num_rows() as u64),
            out_bytes: out.as_ref().map_or(0, |batch| batch.byte_size() as u64),
        });
        (
            out,
            CallStats {
                scratch_bytes: None,
                calls,
            },
        )
    }
}

/// A lane that drained, finished or failed gives its session back; release is idempotent,
/// so the one after a finish costs nothing.
impl Drop for GpuProbingJoin {
    fn drop(&mut self) {
        unsafe { peacock_join_release(self.join.site.executor, self.session) }
    }
}
