# Every seq-bearing operator through the harness

Kind: production

Ninth in the chain, after [`operator-harness.md`](operator-harness.md), which is the whole of
its mechanism. This task adds cases and nothing else: no helper, no production change. Each
row below is either a green comparison or a `bug_` test naming a ticket, and only running it
says which.

## Why two tasks

The harness is proven on operators that cannot hide a wrong helper. Once it holds, every
other operator is a script and a node, and a case that goes red is a fact about the engine
rather than about the harness. Keeping the cases out of the harness task also keeps that diff
reviewable: a reviewer reading the comparator should not be reading sixty cases beside it.

## The matrix

One synthetic batch or a few, a hand-built node over `Given` leaves, `run_both`, `assert_same`.
The right column names the tickets a row may land on; a row with none may still find one.

| Operator | Cases | May land on |
|---|---|---|
| `GpuFilter` | predicate on an int, on a string, on a null-yielding column; with its projection; every row passes; no row passes | |
| `GpuProject` | column copy; int, float and decimal arithmetic; a cast; CASE in both forms; LIKE; a scalar function; a typed NULL literal | [#57](../tickets.md#t57), [#198](../tickets.md#t198) |
| `GpuSort` | asc and desc; nulls first and last; two keys; `fetch` | |
| `GpuAccumulateBatchesAndSort` | several batches; one; none; `fetch` | [#173](../tickets.md#t173) |
| `GpuMergeSortedPartitions` | N lanes each sorted; `fetch`; one lane empty; `Done` before any batch | [#173](../tickets.md#t173) |
| `GpuCoalesceAllBatches` | several batches; one; none | [#173](../tickets.md#t173) |
| `GpuAggregate` | each `PlanAgg`, grouped and global; grouping sets; a decimal sum's scale; a global aggregate over zero rows | [#199](../tickets.md#t199) |
| `GpuAggregateBatches` | merge with and without its finalize; arrivals crossing the compaction threshold; `merge_m2`; a count merging by sum; an average's digits | [#163](../tickets.md#t163) |
| `GpuEmitPartitions` | 4 lanes and 64; the lane each row lands in against the murmur3 of its key; lanes that receive nothing; null keys; two keys; a decimal key; one lane in and four out | [#184](active-tickets.md#t184), [#95](../tickets.md#t95) |
| `GpuHashJoin` | each of the nine types × one probe batch and two × `null_equals_null` both ways × a residual filter where the matrix allows one; an empty build side; an empty probe | [#152](../tickets.md#t152), [#181](active-tickets.md#t181), [#175](../tickets.md#t175), [#159](../tickets.md#t159) |
| `GpuCrossJoin` | two batches; one side empty | |
| `GpuNestedLoopJoin` | Inner and Left with a predicate; with a projection | [#190](active-tickets.md#t190), [#160](../tickets.md#t160) |
| `GpuLoadParquet` | both backends read one parquet the test wrote from a synthetic batch: one batch per row group; a limit; row groups and a limit together | [#186](active-tickets.md#t186), [#188](active-tickets.md#t188) |

The shapes the planner refuses are out of reach here too: the hash-join recipe arm asks the
node's capability and panics without one, so an outer join with a residual filter ([#153](../tickets.md#t153))
never reaches an executor and has no row.

The source row is the one that needs a file rather than an upload: a scan's input is a path.
The parquet writer is the test's, over `synthetic`, with the row-group size chosen so several
row groups exist.

Two of those tickets are closed by tasks below this one — [#198](../tickets.md#t198) by
`typed-nulls`, [#175](../tickets.md#t175) by `empty-build`. Their `bug_` tests here are the
record those tasks turn red and delete, which is the regression test each would otherwise
have to write.

## Constraints

Those of the harness task, and:

- No new mechanism. A case that needs a helper the harness lacks is a finding against the
  harness task, not a helper added here.
- Every row above exists as a case. The kind guard the harness carries extends to every
  `NodeRef` kind but the three forwarders, and a kind with no case is red.
- Every divergence gets a ticket before it gets a `bug_` test, and a `bug_` test asserts the
  wrong behaviour precisely — the wrong value, the refusal's message — not merely that the two
  sides differ.
- The join scripts follow the capability matrix: build side one batch and always first, probe
  streamed, `without_build` where the build produced nothing.

## Verification bar

- Every row present; each case green or `bug_` with its ticket in the comment above it.
- The kind guard green with only the forwarders excluded.
- New tickets in `tickets.md`, within the fifteen-line cap, one per distinct defect.
- `build-test.md`'s row for the harness carries the new count; the grand total moves with it.
- Green on shad-gpu; the rust-only lib unchanged.
