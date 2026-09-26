

<a id="t179"></a>
### #179 — nothing shows a rebatcher moving an enforced budget boundary

Whether batch sizes can reach the accountant's binding pre-call check at all is open: two
candidates failed structurally rather than by accident, so this is about the model, not a gap.

`GpuCoalesceAllBatches` carries the largest estimate in none of the 120 `--- memory ---`
sections at `tp4-rowgroup` — `GpuEmitPartitions` in 77, `GpuHashJoin` in 20, `GpuUnload` in 12 —
so a rebatcher grows a node beside the binding one. `nested-loop-join`'s coalescer is 115 bytes
against a 2,679-byte join. Of the two queries carrying their largest at a loader,
`tpch/nested-limits` does move its peak under `Rebatch::AboveSources` (4,915,680 to 8,000,480,
the 1.63x its goldens predict) while its budget is peak+1 both times: `limit=28` means the
modelled megabytes are never the transient that binds.

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
no RMM pool ([#148](tickets.md#t148)). The drivers reach it often: a node runs once per batch
per lane, so one query has thousands of chances to throw. Their own error path is covered by a
mock, and a mock frees nothing. Wants a gtest that fails a node mid-walk and asserts the
executor is reusable, plus one Rust FFI case on shad-gpu. Retry with a smaller batch is
[#142](tickets.md#t142) and is not this.
