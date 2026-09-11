# Every seq-bearing operator through the harness

Kind: production

**Closes no ticket and fixes nothing.** Cases only: every one is green or a `bug_` test with a
ticket, and the production tree is not touched — not to make a case pass, not to close a ticket
a case happens to reach. The findings are the deliverable; a later task fixes what they name.

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
| `GpuProject` | column copy; int, float and decimal arithmetic; a cast; CASE in both forms; LIKE; a scalar function; a typed NULL literal | [#57](../tickets.md#t57), [#198](../tickets.md#t198), [#187](active-tickets.md#t187) |
| `GpuSort` | asc and desc; nulls first and last; two keys; `fetch` | |
| `GpuAccumulateBatchesAndSort` | several batches; one; none; `fetch` | [#173](../tickets.md#t173) |
| `GpuMergeSortedPartitions` | N lanes each sorted; `fetch`; one lane empty; `Done` before any batch | [#173](../tickets.md#t173) |
| `GpuCoalesceAllBatches` | several batches; one; none | [#173](../tickets.md#t173) |
| `GpuAggregate` | each `PlanAgg`, grouped and global; the single-node shortcut carrying a finalize; grouping sets; a decimal sum's scale | [#187](active-tickets.md#t187) |
| `GpuAggregateBatches` | merge with and without its finalize; arrivals sized to cross the compaction threshold — 1 MiB on the cpu, 64 MiB on the device, so the device's is the one to size for; `merge_m2`; a count merging by sum; an average's digits | [#163](../tickets.md#t163), [#187](active-tickets.md#t187) |
| `GpuEmitPartitions` | 4 lanes and 64, each lane compared as a slot; null keys; two keys; a string key; a decimal key; one lane in and four out | [#184](active-tickets.md#t184), [#95](../tickets.md#t95), [#187](active-tickets.md#t187) |
| `GpuHashJoin` | each of the nine types × one probe batch and two × `null_equals_null` both ways × a residual filter where the matrix allows one; with a projection | [#152](../tickets.md#t152), [#159](../tickets.md#t159) |
| `GpuCrossJoin` | two batches; with a projection | |
| `GpuNestedLoopJoin` | Inner and Left with a predicate; with a projection | [#190](active-tickets.md#t190), [#160](../tickets.md#t160) |
| `GpuLoadParquet` | both backends read one parquet the test wrote from a synthetic batch: one batch per row group; a limit; row groups and a limit together | [#186](active-tickets.md#t186), [#188](active-tickets.md#t188) |

### Empty inputs

Every shape below is its own case, named for the shape, never folded into a loop over types:
a red one must say which combination reached the limit. The frozen surface cannot make a table
out of nothing ([#173](../tickets.md#t173)), an empty build side leaves three join types owing
rows ([#175](../tickets.md#t175)), and a global aggregate over nothing owes its identity row
([#199](../tickets.md#t199)) — so several of these are expected to land as `bug_` tests, and
which ones is the finding.

| Operator | Empty shapes |
|---|---|
| `GpuFilter`, `GpuProject`, `GpuSort` | a zero-row batch; a zero-row batch between two with rows |
| `GpuCoalesceAllBatches`, `GpuAccumulateBatchesAndSort` | no batch at all; one zero-row batch; a zero-row batch among others; a `fetch` over zero rows |
| `GpuMergeSortedPartitions` | every lane `Done` with nothing; one lane a zero-row batch beside lanes with rows; every lane a zero-row batch; lane 0 `Done` before lane 1's rows arrive |
| `GpuAggregate` | a zero-row batch, grouped; global (#199); grouping sets over zero rows |
| `GpuAggregateBatches` | no arrival; one zero-row arrival; a zero-row arrival among others; no arrival under a finalize |
| `GpuEmitPartitions` | a zero-row batch in — N zero-row lanes out, and the lane count is the assertion; a batch whose every row carries one key, so N−1 lanes get nothing; a batch of all-null keys; a stream of zero-row, rows, zero-row |
| `GpuHashJoin`, each of the nine types | a zero-row build batch with probe rows (#175 for Right, Full, RightAnti); build rows with one zero-row probe batch; both zero-row; `build: None`, never probed; a zero-row probe batch between two with rows; only zero-row probe batches then the finish — the finish whose probe produced no keys, #173's one refusing site |
| `GpuCrossJoin` | build empty; probe empty; both |
| `GpuNestedLoopJoin`, Inner and Left | build empty; probe empty |
| `GpuLoadParquet` | a parquet of zero rows |

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

## Scope

Code expected to change:

- `peacockdb-core/src/tests/gpu_tests/`: six case files — exec, aggregate, accumulate, emit,
  join, source, the aggregate split from exec because it carries its own state fixtures — and
  the kind registry entries. The source file carries the one helper this task adds, a parquet
  writer over `synthetic`, local to it.
- `llm-wiki/tickets.md`: a ticket per new defect; `llm-wiki/build-test.md`: the count.
- Nothing under `cpp/`, `peacockdb-ffi/`, or `peacockdb-core/src/` outside `tests/gpu_tests/`.

Component-level API expected to change: none. A case that would need a production change to
pass is a ticket and a `bug_` test, never the change.

## Constraints

Those of the harness task, and:

- No new mechanism beyond the parquet writer above. A case that needs a helper the harness
  lacks is a finding against the harness task, not a helper added here.
- Every row above exists as a case. The kind guard the harness carries extends to every
  `NodeRef` kind but the three forwarders, and a kind with no case is red.
- Every divergence gets a ticket before it gets a `bug_` test, and a `bug_` test asserts the
  wrong behaviour precisely — the wrong value, the refusal's message — not merely that the two
  sides differ. The ticket is where the fix is designed, later and by another task; nothing
  here repairs, works around, or casts away what a case finds. A ticket a case cannot reproduce
  stays open — it closes when its corpus cells run — and the detail file says which.
- The join scripts follow the capability matrix: build side one batch and always first, probe
  streamed, `without_build` where the build produced nothing.

## Verification bar

- Every row present; each case green or `bug_` with its ticket in the comment above it.
- The kind guard green with only the forwarders excluded.
- New tickets in `tickets.md`, within the fifteen-line cap, one per distinct defect.
- `build-test.md`'s row for the harness carries the new count; the grand total moves with it.
- Green on shad-gpu; the rust-only lib unchanged.
