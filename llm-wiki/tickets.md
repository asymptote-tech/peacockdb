# peacockdb tickets

Migrated from GitHub issues on 2026-07-31; GitHub issues are closed and this file is the
registry. The number is the permanent ticket ID — each ticket carries an `<a id="tNN">`
anchor that the cost widget links to. Device labels are `tp<N>-<tier>` (micro=100MiB,
mini=2GiB, standard=12GiB).

A ticket carries a **Priority** line only when it is not medium; medium is the default.
New tickets take the next free number (currently 228), which is also the counter for
`tasks/active-tickets.md` — the rollout's own list, separate file, one ID space. Finished and lapsed tickets move to
`llm-wiki/archive/archived-tickets.md` (Done / Stale) — numbers are never reused, so an old
reference still resolves there.

## Contents

| Section | Open | Tickets |
|---|--:|---|
| [Critical correctness](#critical-correctness) | 28 | #225 #224 #223 #222 #221 #219 #218 #217 #216 #215 #214 #211 #210 #208 #207 #205 #204 #202 #200 #199 #166 #153 #80 #59 #60 #121 #122 #118 |
| [Blockers for disabled coverage](#blockers-for-disabled-coverage) | 12 | #212 #206 #203 #169 #168 #158 #173 #23 #65 #62 #95 #45 |
| [Performance / architecture](#performance--architecture) | 28 | #226 #179 #177 #170 #155 #154 #152 #150 #149 #148 #19 #16 #20 #71 #101 #73 #75 #136 #137 #138 #139 #140 #141 #147 #146 #145 #144 #142 |
| [Infrastructure / process](#infrastructure--process) | 18 | #201 #197 #196 #195 #178 #176 #167 #164 #159 #160 #161 #162 #134 #129 #128 #13 #94 #69 |

## Critical correctness

<a id="t204"></a>
### #204 — the device's sorted merge drops its fetch when it is handed one input

`CudfSortPreservingMerge` with a `fetch` over a single input table answers every row: 16 where the
plan asked for 5.

`node_session.cpp`'s collapse arm merges and slices only under `views.size() > 1`; one view falls
to the plain `cudf::concatenate`, which applies no fetch. Both accumulating sorts reach it — an
`AccumulateBatchesAndSort` lane that received one batch, and a `MergeSortedPartitions` with one
populated lane — and the wire puts the fetch on the merge alone (`accumulating_sort` writes
`fetch: -1` per batch). From SQL the per-batch `GpuSort` carries the same fetch, so one batch
already holds at most n rows and the loss is masked; the operator's contract is still broken.
Pinned by `bug_a_fetch_over_one_sorted_batch_is_not_applied_on_the_device` and
`bug_a_fetch_over_one_populated_lane_is_not_applied_on_the_device` (`gpu_tests/accumulate_cases.rs`).

<a id="t118"></a>
### #118 — SortPreservingMerge concat fallback ignores fetch (LIMIT dropped)
`cpp/src/node_session.cpp` (~L208): the k-way-merge branch applies `spm->fetch()` after
merging, but the fallback branch — taken when the SPM has no sort keys **or only one
input partition** — is a plain `cudf::concatenate` with no fetch applied. A
single-partition SortPreservingMerge carrying a fetch therefore returns **all** rows
instead of the top-N, i.e. a silently dropped LIMIT. Apply the same slice in both
branches. Found during the comment audit; not yet reproduced against a corpus query
(most SPMs arrive multi-partition), so severity depends on whether any enabled plan hits
the single-partition path.


## Blockers for disabled coverage

<a id="t168"></a>
### #168 — the fbs ScalarValue has no interval, so one join residual has no payload

`ScalarValue` has no interval variant, and `testdata/tpch-queries/mixed-join.sql` adds one to a
column, so folding cannot reach it and that join's recipe has no writable payload.

It is the only query in either bench with the shape — every other corpus interval folds away
before serialization — and no GPU path here has ever carried one. What changed is that the
golden says so, not an `Err` nobody reads.

That node's payload reads `unavailable:` with the reason, and the placeholder adopts the children
already taken for the node it replaces: a leaf would orphan those subtrees and shift every seq
above the failure while the rendering said nothing.

Closing it is a third appended `ScalarValue` variant plus the C++ arm, on the terms the other two
took, both appended so no ordinal moves. Not
proposed: one query is a thin case for a surface change, and T21 does not need it.

<a id="t65"></a>
### #65 — __grouping_id encoding doesn't match DataFusion's GROUPING()
Grouping-set expansion (`cpp/src/operators/aggregate.cpp`) emits a gid that is
distinct-per-set but not DataFusion's positional bitmask: the device sets bit `i` for masked key
`i`, DataFusion sets the first key highest, so a two-key rollup is 0, 2, 3 there and 0, 1, 3 here;
and the device's column is Int32 where DataFusion declares UInt8. Safe while no enabled query
projects or sorts `GROUPING()`; must be fixed before one does (q70/q86 after #23).
9 rollup rows carry this ticket; pinned by the `bug_grouping_sets_…` cases in
`gpu_tests/aggregate_cases.rs`, and the `Int32` read at the handle by
`aggregate_schema_cases.rs`'s `bug_grouping_sets_hold_an_int32_grouping_id_…` and the walk's
`bug_a_rollup_partial_holds_an_int32_grouping_id_…` (`wire/gpu_tests/mod.rs`).

The width is wrong beside the encoding. The gid is built `INT32`
(`cudf::numeric_scalar<int32_t>`), where DataFusion sizes the column to the group count:
`UInt8` up to 8 grouping expressions, `UInt16` to 16, `UInt32` to 32, `UInt64` beyond. Too
wide for every corpus query and too narrow past 32 groups. The fix reads the declared output
schema rather than picking a type; nothing timed reaches a plan with grouping sets.

<a id="t62"></a>
### #62 — count(DISTINCT) ignores the DISTINCT flag in GpuAggregate
`cpp/src/operators/aggregate.cpp` ignores `AggregateFuncNode.distinct`; a guard now
throws rather than silently miscomputing. Standalone count-distinct works via
DataFusion's `SingleDistinctToGroupBy` rewrite (q16×2/q94/q95 green); the flag survives
only for mixed distinct + non-distinct, blocking q28. Fix: map count+distinct → cuDF
`nunique` (`null_policy::EXCLUDE`) in the grouped and global paths without regressing
the rewrite queries.

## Performance / architecture


<a id="t138"></a>
### #138 — sort: ranged merge emission
`GpuAccumulateBatchesAndSort` and `GpuMergeSortedPartitions` run one `cudf::merge` over all
sorted inputs and materialize the whole output, so the local peak is inputs + output.

cuDF has no streaming merge. The hand-rolled alternative is a ranged merge: pick split keys,
`cudf::slice` each sorted batch at `upper_bound` boundaries (zero-copy views), merge range by
range, emit and release each. That bounds the output term to one range; the inputs stay resident
either way, so the win is at most ~2x on the sort's local peak. Do it only if sort peaks bind
after the mode ships — it also unlocks multi-batch output from merge nodes.

Landing it re-introduces a third `SortOrder` state. The enum is two-valued today because every
node that orders a whole stream emits one batch, so "stream sorted" is `BatchSorted` meeting
`SingleBatch` and is derived rather than declared. Ranged emission produces the one shape that
breaks that — a stream ordered across several batches — so it must add `PartitionSorted` back
and teach the limit-after-sort validation to accept it alongside the derived form.

## Infrastructure / process
