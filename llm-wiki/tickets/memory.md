

<a id="t182"></a>
### #182 — two accounting properties are out of reach, and no budgeted run survives

Pricing a batch from the plan's schema disabled two cases in `test_cpu_end_to_end.rs`,
both `#[ignore]`d rather than deleted so they stay in `--list`. Neither property stopped being
true. Deferred deliberately: T18's bar was results and node stats, not memory.

**What it costs meanwhile.** Those two sites were the only ones in the tree passing `Some(budget)`
over a planned query, so the accountant's enforcement path is now exercised by unit cases over the
mock backend and by nothing else — eight `Executor` impls report a residency and a transient on
real plans and nothing budgeted reads them. That is the state the budget case was written to end,
and it is the reason to pick this up rather than either property on its own.

**The budget case is [#179](memory.md#t179).** `boundary()` sets `(low, high) = (peak, peak * 8)`
and searches upward, so its byte-below arm fails whenever `fits(peak)` — and "10058 fits, 10057
also fits" says the peak is 10,057 and the trip is somewhere below it. Logical pricing raised the
peak (8,222 to 10,057, a validity bitmap arrow never allocated) without raising the modelled
transient as much, so the boundary fell under the floor the search starts from.

Fix: when `fits(peak)`, search `(0, peak)` instead. Same bisection, same assertions, and the
byte-below arm then means what it says. `boundary()`'s doc claims the peak is a floor because a
pre-call check tests a modelled transient that can exceed what was held — true, and this is the
case where it does not, so the doc wants the other direction named beside it.

**The rebatcher case lost its query, not its premise.** A rebatcher cannot move a *total* built
from logical bytes, but a peak is what is resident at once, and `GpuCoalesceAllBatches` holds its
whole lane before emitting one batch — both live at the emit, a rows fact logical bytes carry.
`nested-loop-join` cannot show it: one batch per lane, so the rebatcher merges one into one, and
its old 318-byte move was arrow reallocating a single batch.

Fix: the second pair needs a query whose loader maps two batches into one lane, and **the
corpus no longer has one.** `tpch/nested-limits` was it — `part` mapped
`partition_groups=[[[0],[1]]]` at `tp4-rowgroup`, 983,071 estimated against 1,600,062 source
bytes — until limits (chain K, [#186](../archive/archived-tickets.md#t186)) trimmed a limited
scan's survivors to the prefix the cut needs. That loader is `[[[0]]]` at every mode now, with
`source_bytes` equal to its estimate, so the rebatcher would merge one into one, which is the
same reason `nested-loop-join` cannot show it. So this half wants a query written for it rather
than one repointed at, and the two `#[ignore]`d cases stay ignored until there is one.

T17a's drain half is untouched: a drained lane changes rows per lane, so q16's 104.7 MB against
77.9 MB stands.

<a id="t179"></a>
### #179 — nothing shows a rebatcher moving an enforced budget boundary

Whether batch sizes can reach the accountant's binding pre-call check at all is open: two
candidates failed structurally rather than by accident, so this is about the model, not a gap.

`GpuCoalesceAllBatches` carries the largest estimate in none of the 120 `--- memory ---`
sections at `tp4-rowgroup` — `GpuEmitPartitions` in 77, `GpuHashJoin` in 20, `GpuUnload` in 12 —
so a rebatcher grows a node beside the binding one. `nested-loop-join`'s coalescer is 115 bytes
against a 2,679-byte join. `tpch/nested-limits` was the one positive example — its peak moved
4,915,680 to 8,000,480 under `Rebatch::AboveSources`, the 1.63x its goldens predicted, which was
exactly the ratio of its two mapped row groups to one — and limits (chain K) removed it by
trimming that scan to one row group, so there is nothing left for the rebatcher to coalesce
there. Its budget was peak+1 both times anyway: the 28-row cut, now a `GpuLimit` rather than the
loader's own `limit=28`, means the modelled megabytes were never the transient that binds.

A second thing falls out: `boundary()` in `src/tests/end_to_end/accounting.rs` searches upward from
the observed peak, so a query whose trip is below it reports an untested floor — the trip assert
catches that rather than passing. Answering this needs a downward search, a different claim.

<a id="t177"></a>
### #177 — the finish join's intermediate is priced by the node's row, not by what it emits
`schema_of` in `gpu_backend/join.rs` prices the finish join's output by the node's output schema.
The finish emits the whole build side — plus the appended boolean for a mark join — so wherever the
node carries a projection the resident model sees a narrower row than the device holds.

It under-prices, which is the direction that matters: a budget that should refuse the call instead
lets it run, and the failure arrives from the allocator rather than as the named refusal the
accounting exists to produce. Pre-dates the narrowing project ([#175](#t175)'s neighbour work),
which only makes it legible — before it, the finish emitted every build column and the node
declared fewer, silently.

Needs a device to check, which is why it is a ticket rather than T17's: a pricing fix nothing can
run is a second guess on top of the first.

<a id="t167"></a>
### #167 — nothing proves a failed query gives its device memory back

A failed `execute_node` resets the session, which frees every resident table by destruction, and
the handles that outlive it release into a null-guarded no-op. Neither half is tested.

What is unverified is the whole lifecycle after a failure rather than any one call: that
`peacock_executor_end_plan` on an already-reset session is safe, that the same executor can
`begin_plan` again and answer a second query, and that device memory is actually back rather
than merely unreferenced — which today means cuDF's default resource, since the engine installs
no RMM pool ([#148](performance.md#t148)). The drivers reach it often: a node runs once per batch
per lane, so one query has thousands of chances to throw. Their own error path is covered by a
mock, and a mock frees nothing. Wants a gtest that fails a node mid-walk and asserts the
executor is reusable, plus one Rust FFI case on shad-gpu. Retry with a smaller batch is
[#142](optimizer.md#t142) and is not this.

<a id="t229"></a>
### #229 — a mid-plan limit's sliced device batch is priced without its string bytes

A `GpuLimit` that slices a straddling batch on the device prices the slice at its fixed-width
size alone. Its string payload counts as zero, so the accountant's resident total runs low.

`LimitStream` (`gpu_backend/accumulate.rs`) builds the slice with
`logical_size_from_schema(&self.schema, rows.length, 0)`; the third argument is the var-length
content. Every other device batch is priced through `produced()` (`gpu_backend/mod.rs`) from the
ABI's `varlen_content_bytes`, but `peacock_executor_slice_handle` reports no stats. So a
budgeted run can pass a boundary it should trip on, and `peak_bytes` in a golden is low. Holds
and releases still reconcile, because `Held::of` reads the figure once, which is why nothing
notices. Found by `reports/hacks-audit.md` (production bug 1). Every limit DataFusion pushes into
a scan below the root reaches it too, as a `GpuLimit` over the scan (#186).

**Corpus queries:** `tpch/nested-limits` slices `part(p_partkey)`, an Int64, at both of its
part-side limits. Any `LIMIT` subquery over a string column reaches it, e.g. `select
count(p_name) from (select p_name from part limit 10);` (tpch), unconfirmed.

**Fix proposed:** measure the slice the way every other batch is measured: `slice_handle`
reports `NodeStats` as `execute_node` does, and the slice is priced through `produced()`. That
appends an out-parameter to a frozen ABI symbol. The fallback without an ABI change scales the
input batch's measured var-length bytes by the row ratio, with the approximation named at the
site. Test: a device limit over a string column whose slice's `byte_size` equals the export's.
