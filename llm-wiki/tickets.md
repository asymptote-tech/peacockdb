# peacockdb tickets

Migrated from GitHub issues on 2026-07-31; GitHub issues are closed and this file is the
registry. The number is the permanent ticket ID — each ticket carries an `<a id="tNN">`
anchor that the cost widget links to. Device labels are `tp<N>-<tier>` (micro=100MiB,
mini=2GiB, standard=12GiB).

A ticket carries a **Priority** line only when it is not medium; medium is the default.
New tickets take the next free number (currently 200), which is also the counter for
`tasks/active-tickets.md` — the rollout's own list, separate file, one ID space. Finished and lapsed tickets move to
`llm-wiki/archive/archived-tickets.md` (Done / Stale) — numbers are never reused, so an old
reference still resolves there.

## Contents

| Section | Open | Tickets |
|---|--:|---|
| [Critical correctness](#critical-correctness) | 16 | #199 #198 #166 #153 #80 #59 #46 #47 #60 #121 #122 #123 #118 #119 #120 #117 |
| [Blockers for disabled coverage](#blockers-for-disabled-coverage) | 14 | #169 #168 #158 #175 #173 #23 #65 #62 #95 #57 #45 #63 #56 #55 |
| [Performance / architecture](#performance--architecture) | 27 | #179 #177 #170 #155 #154 #152 #150 #149 #148 #19 #16 #20 #71 #101 #73 #75 #136 #137 #138 #139 #140 #141 #147 #146 #145 #144 #142 |
| [Infrastructure / process](#infrastructure--process) | 23 | #197 #196 #195 #178 #176 #174 #167 #164 #163 #159 #160 #161 #162 #113 #134 #129 #128 #127 #125 #13 #94 #69 #49 |

## Critical correctness

<a id="t199"></a>
### #199 — a global aggregate on an empty lane drops its identity row on the device

`gpu_backend/accumulate.rs:307` answers an empty lane with nothing. The CPU counterpart has a
`!self.grouped` clause and answers with the identity row — `count` is 0, not absent.

So a global aggregate whose lane received no rows disagrees between the engines: the CPU emits one
row and the device emits none. A wrong answer rather than a refusal, and nothing refuses it.
Reachability is unverified — found by reading, not by a run — so the first thing it needs is a
query that reaches an empty lane under a global aggregate.

<a id="t198"></a>
### #198 — a typed NULL inside an AST expression is a typed zero on the device

`ScalarValue.is_null` is read in `build_scalar` (`expr.cpp:456`) and assumed `true` in
`build_expr`'s ten literal arms (`:158`), so which answer a literal gives depends on which
path evaluated it.

A bare literal short-circuits to `build_scalar` and is null, as do `CASE` and `LIKE`, which
`is_ast_able` refuses. What reaches the bug is `col <op> NULL::T` for a numeric `T` matching the
column. In arithmetic that is a wrong value; in a comparison it is a wrong **row count**, since
`col = NULL` is true wherever `col` is 0 and SQL says the row does not survive.

No cell is disabled against this — it is a wrong answer inside cells that pass. Fixed by
`tasks/typed-nulls.md`, which removes the second scalar builder rather than correcting it.

<a id="t166"></a>
### #166 — physical planning drops a LIMIT interval, and the answer changes

DataFusion 45 loses a limit in two shapes, both measured against DuckDB 1.5.4 on the same sf1
parquet: the interval is absent from the physical plan, so both engines compute the same wrong answer.

A limit inside a `UNION ALL` branch survives only as an `AggregateExec … lim=[n]` early-stop hint,
which applies neither the offset nor the truncation: two branch limits holding 18 rows under an outer
`LIMIT 40 OFFSET 5` answer 40 where DuckDB answers 13, at tp1 and tp4 alike. The hint is why a golden
carrying it looks like coverage — it reads as a limit in plan text and is not one. Separately, at tp4
only, an outer limit above an aggregate drops the mid-plan limit below it and the aggregate then counts
its whole input. No corpus query has either shape, so nothing is wrong today; `nested-limits.sql` was
reshaped rather than canonized against it. Upstream
[#14406](https://github.com/apache/datafusion/issues/14406) is the same class — a global limit removed
above children that keep only a local one — and its fix landed after 45.0.0 and is in 46.0.0, so #23's
upgrade is the experiment; a residual after it would need the logical limit set compared to the physical.

<a id="t153"></a>
### #153 — equi-join residual filter is applied after the outer gather
**Priority: high**

`join.cpp` (~L353) masks a `CudfHashJoin.filter` over the gathered table *after* the join
null-padded its unmatched rows, so Left, Right and Full demote the ON condition to a WHERE.

`… LEFT JOIN b ON a.k = b.k AND b.v > 50` returns an inner join: a padded row's NULLs make the
predicate NULL, and a left row whose only matches fail is dropped rather than null-padded.
`execute_nested_loop_join` (~L453) refuses this exact shape with the argument written out, so
the fix direction is settled here rather than proposed; that path is unaffected. Latent because
DataFusion pushes an ON predicate reading one side below the join (tpch q13) — it needs one
referencing both sides, which no corpus query has. Fix: evaluate the residual during the join
(`mixed_*` covers Left, Right is its swap, Full needs inner-plus-re-add), plus a `PlanExecutor`
gtest with a filtered Left join. The same commit drops the prototype's deliberate reproduction.

<a id="t80"></a>
### #80 — Anti-join NOT IN three-valued logic + independent DuckDB result oracle
`NOT IN` with any NULL in the build side must yield the empty set; neither
`null_equality::EQUAL` nor `UNEQUAL` implements that, so ANTI/mark joins stay hardcoded
EQUAL (`cpp/src/operators/join.cpp`). Needs a planner/serializer flag distinguishing
NOT IN from NOT EXISTS, a nullable-anti-key test (no corpus query exercises it), and a
DuckDB final-result oracle — today's validation is circular (goldens vs DataFusion, GPU
vs CPU). Semi half done (q33; semi honors per-join `null_equals_null`).

<a id="t59"></a>
### #59 — Nullable-key semantics for semi/anti/mark joins
Anti/mark keep `null_equality::EQUAL` deliberately; a blind UNEQUAL flip is wrong for
`NOT IN`. Latent — no enabled query has a nullable anti/mark key. Wants a dedicated
analysis plus expr/join goldens covering nullable IN / NOT IN / EXISTS before defaults
change. Anti remainder overlaps #80.

<a id="t46"></a>
### #46 — q61 GPU: 'promotions' sum subtree returns the wrong value
Cross join is correct; `promotions` sum returns 2855378.83 vs CPU 2894907.87 — an
upstream filtered-sum / projection / aggregate bug. Bisect that subtree node-by-node,
CPU vs GPU. q61 stays off on a device.

<a id="t47"></a>
### #47 — q77 GPU returns 40 rows vs CPU 45
Cross join correct; an upstream outer-join/aggregate branch drops 5 rows. Bisect
per-node row counts. q77 stays off on a device.

<a id="t60"></a>
### #60 — q78 GPU diverges in anti-join + top-N; possibly memory-borderline
3-CTE anti-join (`LEFT JOIN … IS NULL`) + multi-key DESC LIMIT 100. `round` is proven
fine (q54). The isolated diff harness segfaults under GPU memory pressure on a shared
H200 — re-check on a free GPU to separate wrong-result from memory-induced.

<a id="t121"></a>
### #121 — Multi-GPU q3 frees GPU-p memory on the calling thread
`cpp/tests/gpu/test_multi_gpu_tpch.cpp` (~L634): `shuffled` (from `hash_shuffle`) is a
local of the `execute` lambda, so it is destroyed on the **calling** thread. Each
`shuffled[p]` for p≠0 is a `cudf::table` from GPU p's RMM pool, so this frees device-p
memory while device 0 is current — the exact worker-per-GPU destruction rule this file
follows everywhere else (every other buffer is explicitly released on its worker). Pool
deallocation is stream-ordered per device, so this is the class of mistake that shows up
later as a corrupted pool or a teardown crash, not immediately. Release each
`shuffled[p]` on `pool[p]` like the neighbouring `local_top` block.

<a id="t122"></a>
### #122 — Multi-GPU q6 host merge doesn't check partial scales agree
`cpp/tests/gpu/test_multi_gpu_tpch.cpp` (~L198): the host `__int128` sum overwrites
`scale` with each partial's scale without verifying they match. All partials share a
scale today, so the result is correct — but a future divergence would silently produce a
wrong sum instead of failing. Assert equality.

<a id="t123"></a>
### #123 — Unused static helper in test_plan_executor.cpp
`cpp/tests/gpu/test_plan_executor.cpp:53`: `make_float64_literal(...)` is defined and
never called — a `-Wunused-function` warning waiting on the next warning-level bump.
Remove it or use it.

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

<a id="t119"></a>
### #119 — Unchecked cudaMemcpy in peacock_spark_partition_ids
`cpp/src/gpu_executor.cpp` (~L250): the device→host `cudaMemcpy` return value is
discarded, so a failed copy still returns success with `out_pids` holding garbage. Check
it and surface the error.

<a id="t120"></a>
### #120 — peacock_spark_partition_ids' documented error path is unreachable
`cpp/include/peacock_gpu.h` documents retrieving the failure message via
`peacock_last_error(NULL)`, but `gpu_executor.cpp` returns `""` for a null executor and
the implementation only `fprintf`s to stderr — the message cannot be obtained through the
documented API. Either store it where `peacock_last_error(NULL)` can return it, or fix
the doc.

<a id="t117"></a>
### #117 — register_tables_for silently mis-handles non-parquet files
`peacockdb-core/src/lib.rs` (~L87): the extension check `if path.extension() != Some("parquet") { () }`
is a no-op, so non-parquet dir entries are not skipped; and `read_table` never returns
`Err` (failures panic), making the `else { continue }` dead. A stray non-parquet file in
a data dir panics instead of being skipped. Found during the comment audit.


## Blockers for disabled coverage

<a id="t169"></a>
### #169 — a recipe plan is a chain, so its depth is its length, and the verifier caps depth

fb children are nested, so the recipe plan for a query is one deep chain rather than a broad
tree: depth equals the number of addressed nodes plus its stubs. The C++ verifier caps depth at
1024, and the Rust reader had to have the same limit raised to parse what it had just written.

Deepest today is tpcds at `tp4-rowgroup`, seq 382, so nothing is near it. What makes it worth
recording is the failure mode: a plan of roughly a thousand addressed nodes fails at
`begin_plan` — the whole query refused before a call is made — rather than degrading at the call
that overruns.

The fix belongs here rather than in the verifier. Raising a limit to fit a shape that grows
without bound only moves the number; splitting one recipe plan into several, loaded in turn, ends
it. Not urgent at a factor of two and a half of headroom, and it wants measuring before it wants
designing: nothing yet says a thousand-node plan is a shape this mode should produce.

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

<a id="t158"></a>
### #158 — an aggregate DataFusion answers from statistics reaches no executor
`SELECT count(*) FROM nation` never reaches an `AggregateExec`: DataFusion's
`AggregateStatistics` rule answers it from parquet metadata and emits `PlaceholderRowExec`
holding the result.

The mode refuses it at plan time, so nothing here runs it. No corpus query reaches it either,
since all 31 using `count(*)` carry a WHERE, GROUP BY or JOIN and the rule cannot fire.

The fix is small on the CPU and unavailable on a device: the node is a source of constant rows,
and a table of literals made from no input is what the frozen surface has no call for — the same
wall as [#173](#t173) and [#175](#t175). T17 was to have discharged it and did not: writing the
CPU half alone makes the oracle answer a query the device refuses, and the oracle is what the
device is checked against. Waits on the make-a-table-of-literals call all three want.

<a id="t175"></a>
### #175 — an empty build side leaves three join types owing rows they cannot make
`empty_build_answers_nothing` decides what a lane answers when its build side produced no batch.
Six types owe nothing and end the lane; Right, Full and RightAnti owe their probe side.

Owing the probe side means a call over a build table that does not exist, which the frozen surface
has no way to express — the same wall as [#173](#t173), reached from the join instead of the
accumulator. Both backends refuse by name rather than inventing an answer.

The corpus reaches it twice: q21 at tp4-single, and tpcds q77, whose Right outer at four lanes
gets no build side and owes its probe rows padded with NULLs. q77 is therefore out of the
end-to-end list, with q2 carrying the union-that-cannot-interleave claim in its place — writing
the CPU pad alone would make the oracle answer a query the device refuses.
Unfreezing buys a pass-through of the probe side and the refusal goes.

<a id="t173"></a>
### #173 — the frozen surface cannot build a table out of nothing
Every entry point loads a table by reading one, so a node owing rows it did not receive has no
call to make. Three places hit it: a collapse of no handles, a merge of no runs, and a finish
whose probe produced no keys and which owes an empty table or one of literals.

Each refuses by name rather than inventing rows, and the CPU backend emits nothing in the same
places so the two stay one engine. The exception is a global aggregate, which owes its identity
row whatever arrived.

Unfreezing buys a make-empty-of-schema call and the refusals go. Until then the refusal is the
contract, and the shapes that reach it are the ones a lane can be empty in.

<a id="t23"></a>
### #23 — Upgrade DataFusion 45→46+ to unblock q27/q70/q72/q86
Four TPC-DS queries fail to physical-plan on DataFusion 45 (`plan_status=fail`): q27
SanityCheckPlan vs ROLLUP SortPreservingMerge ordering; q70/q86 `GROUPING()` aggregate
not planned; q72 `Date32 + Int64` coercion. Whole rows dead until the upgrade. See #114.

<a id="t65"></a>
### #65 — __grouping_id encoding doesn't match DataFusion's GROUPING()
Grouping-set expansion (`cpp/src/operators/aggregate.cpp`) emits a gid that is
distinct-per-set but not DataFusion's positional bitmask. Safe while no enabled query
projects or sorts `GROUPING()`; must be fixed before one does (q70/q86 after #23).
9 rollup rows carry this ticket.

<a id="t62"></a>
### #62 — count(DISTINCT) ignores the DISTINCT flag in GpuAggregate
`cpp/src/operators/aggregate.cpp` ignores `AggregateFuncNode.distinct`; a guard now
throws rather than silently miscomputing. Standalone count-distinct works via
DataFusion's `SingleDistinctToGroupBy` rewrite (q16×2/q94/q95 green); the flag survives
only for mixed distinct + non-distinct, blocking q28. Fix: map count+distinct → cuDF
`nunique` (`null_policy::EXCLUDE`) in the grouped and global paths without regressing
the rewrite queries.

<a id="t95"></a>
### #95 — Decimal partition keys for real-8-way murmur3
murmur3 covers int/date/timestamp/composite/null; decimal deferred (float indefinitely).
Needed by the first shuffle on a decimal key (tpch q18 `o_totalprice`, q22 `c_acctbal`,
tpcds `i_current_price`). Dispatch by *logical* precision (≤18 → low 8 LE bytes of int128;
>18 → raw 16B LE) and thread precision through the partition FFI. Until then
`spark_hash_partition.cu` throws a loud "decimal partition key unsupported".

<a id="t57"></a>
### #57 — Value-form CASE produces wrong results on the GPU column path
`CASE x WHEN v THEN …` in `build_column_case` (`cpp/src/expr.cpp`) returned all-0/null
(the per-branch EQUAL condition comes back all-null through `copy_if_else`); reverted to
a throw guard. Sole corpus user is q39 — this is what keeps q39 off the GPU. Fix plus a
direct gtest, then enable q39 (stddev itself works; q17 passes).

<a id="t45"></a>
### #45 — q24 GpuHashJoin: 'Unary cast type must be fixed-width'
A join-key cast targets string; cuDF's unary cast rejects non-fixed-width. Diagnose the
emitted cast and handle string keys by hashing rather than casting
(`cpp/src/operators/join.cpp`).

<a id="t63"></a>
### #63 — q9 GpuProject copy_if_else size mismatch (CASE over scalar subqueries)
A top-level CASE over ~15 scalar-subquery comparisons fails in cuDF `copy_if_else`
(1-row scalar branch vs other-sized branch). Needs scalar-subquery branches broadcast to
the row count, or a different CASE lowering (`cpp/src/expr.cpp`).

<a id="t56"></a>
### #56 — q2: CASE-over-string-equality inside a partial-phase sum
Partial GpuAggregate builds an AST for `sum(CASE WHEN <string equality> …)` → cuDF
binaryop "Unsupported operator" (string comparand in the aggregate AST path). Support it
or lower it before the aggregate (`cpp/src/operators/aggregate.cpp`).

<a id="t55"></a>
### #55 — q66: two-phase decimal aggregate ignores the partial-phase divisor cast
DataFusion evaluates `sum(decimal/int)`'s division (divisor cast to Decimal128) only in
the Partial phase; GpuAggregate re-evaluates against Final-phase inputs → cast failure.
Honor the partial-phase operand cast / state schema.


## Performance / architecture

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

A second thing falls out: `boundary()` in `test_cpu_end_to_end.rs` searches upward from
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

<a id="t170"></a>
### #170 — a source whose lanes each hold one batch could say so, and three shortcuts would fire

The loader declares `MultipleBatches` unconditionally
([architecture.md](architecture.md#modes-and-knobs)), so no downstream node may assume one
batch per partition. That was incremental simplicity rather than a missing fact: `scan_mapping/partition.rs`
computes the row-group → (partition, batch) mapping once at plan time and everything downstream
consumes it verbatim, so the batch count per lane is `partition_groups[lane].len()` — known in all
three batching forms, `Sized` included, since the planner cuts by bytes and the loader only
executes what it was handed.

The condition is that every lane holds exactly one batch, `SingleBatch` being a property of the
node rather than of a lane: a source with lanes of one and two batches stays `MultipleBatches`.

Saying it fires shortcuts the aggregate sequence already specifies: a 1-partition single-batch
input needs one `GpuAggregate` carrying both `aggs` and `final`, and a single-batch-per-partition
input skips the first `GpuAggregateBatches`. Join build sides need nothing new — `translator/nodes.rs`
already elides their coalesce when the input is `SingleBatch`. So the change is one declaration
and the plans get smaller by themselves. Every plan golden moves, which is its real cost.

<a id="t155"></a>
### #155 — umbrella: join execution through a wider C and FlatBuffers API
Every join mode already runs on the frozen surface (`scripts/exec_model`, and the capability
matrix in `architecture.md`); open is what running it there costs.

| Cost | Ticket | What removes it | Surface change |
|---|--:|---|---|
| build side copied per probe batch | [#152](#t152) | refcounted handles | none — a handle stays a `u64` |
| probe batch copied per batch (Left/Full) | [#152](#t152) | refcount, or a node returning its input | none / fbs semantics |
| build side re-hashed per probe batch | [#136](#t136) | a stateful join session | 3 new symbols |
| probe keys resident + a finish join | [#136](#t136) | a match bitmap out-param, or that session | 1 argument / 3 symbols |
| a symbol per runtime-varying field | — | per-call overrides on `execute_node` | 1 symbol, replacing 3 |

Each follows from a handle being consumed by its reader, or an fb node's fields being plan
constants, and they overlap. The session subsumes the bitmap; the top two rows need no ABI
change. Land [#154](#t154) first, or the numbers are inflated by per-call copies.

<a id="t154"></a>
### #154 — every operator exit path deep-copies its output into a fresh table
`std::make_unique<cudf::column>(view)` deep-copies the device buffer, and 18 sites under
`cpp/src/` do it — 10 in `join.cpp` — mostly to a table the same function just produced.

`execute_hash_join` is worst: `cudf::gather` returns an owning table, the code copies each
column into `all_cols` (~L337, ~L342), then copies the kept ones again if the node projects
(~L376). `release()` moves instead; `scan.cpp` L108, `union.cpp` L38, `join.cpp` L254 are the
pattern, and it is C++-internal — no header, fbs, Rust or golden moves. Four kinds: whole table
freshly produced (`join.cpp` 202, 337, 342, 512, 515), mechanical; ordinal subset (`join.cpp`
211, 270, 376, 525, `filter.cpp` 41), needing an assert the ordinals are distinct; a column of
an **input** table (`join.cpp` 259, `project.cpp` 44, `window.cpp` 46, `expr.cpp` 850), changing
who destroys what under `NodeInputs`; and `aggregate.cpp` 407, 602, 604, 682, unresolved without
reading. Traps: a view taken before the release dangles (`ftv` ~L372), and a repeated projection
ordinal moves one column twice leaving a hole — a wrong answer, not a throw, which is why it
needs the assert and not the observation. Land before [#155](#t155).

<a id="t152"></a>
### #152 — GpuHashJoin: the build handle does not survive a streamed probe
`NodeSession::execute_node` erases every input handle it reads (`node_session.cpp` ~L250, ~L339,
~L427), but a streamed probe calls the join seq once per batch and needs it B times.

[#136](#t136) assumes the table is there and so does the recipe mapping in
`architecture.md`; neither says how. [#140](#t140) is the same constraint one
axis over and [#145](#t145) is the mechanism both want.

T16 confirmed both halves on a device. A build-side shape refuses its second probe batch, so only
the semi family streams; and Left and Full outer have no device path at all, because their key
project and their per-call join read the same probe batch and nothing copies it either. The finish
pass's pad is proved on the CPU alone until #145.

Whether the copy is tolerable is answerable from the goldens: each join's two
`GpuCoalescePartitionsExec` lines carry both sides' `output_bytes`, and B copies cost `B ×
build_bytes` against one probe stream. Take the ratio on **bytes, not rows** — tpch q3 is 24:1
by rows and 73:1 by bytes. Decide before T16, under [#155](#t155).

<a id="t150"></a>
### #150 — store the embedding columns uncompressed; Snappy costs a third of a vector query to save 3%
The sf40 embedding columns are written SNAPPY and do not compress: `ps_image_embedding`
12306/12661 MB and `p_text_embedding` 3205/3293 MB, both 1.03x against ~1.6x elsewhere.

Float32 embeddings are high-entropy, so that is the data rather than the writer — and the GPU
decompresses them anyway. On q11v (`nsys`, share of GPU kernel time) `nvcomp::unsnap_kernel` is
564.7 ms / 37.9% on H200 and 419.5 ms / 24.9% on GB10, against 60.5 ms / 4.0% for the cuVS
distances and top-k the query exists to do; loading is 93.8% of H200 kernel time.

The change is `compression=NONE` for those two columns in `testdata/generate_testdata.sh`.
Parquet compression is lossless, so no value changes and no golden moves — only file size (~440
MB more) and the load path. Not free to do, though: sf40 is generated, uploaded and mirrored to
shad-gpu, so it means re-uploading 40 GB and re-verifying the 16 sf40 goldens. Measure with
`load_ms` per vector probe and the `unsnap` line from `nsys stats`.

<a id="t149"></a>
### #149 — the parquet load must use pinned host memory
**Priority: high**

Nothing in the engine sets a host memory resource for IO, so parquet loads from pageable host
memory — 10.6 GB/s H2D on the H200 against 47.3 GB/s pinned, 2 GiB buffers, 2nd-min of 5.

A discrete GPU's DMA engine transfers by physical address and cannot be handed a page the OS may
move, so a pageable source is bounced through an internal pinned staging buffer: a host memcpy
of every byte, which is what bounds the rate — H200's 10.6 GB/s sits just under its 11.8 GB/s
single-core memcpy, nowhere near its link rate. That is 4.4x on every byte the loader moves, and
the load dominates: 400-690 ms against 19-48 ms of execute on sf40. cuDF exposes
`cudf::io::set_host_memory_resource` and defaults to pageable — [#148](#t148)'s shape one side
over. Condition it on the device: GB10 shows 59.5 vs 59.2 because it has one physical pool, so
`pageableMemoryAccess` is the branch. Tests: compare the existing `[bench] … load_ms=` on both
hosts, asserting the discrete host improves and the integrated one does not regress.

<a id="t148"></a>
### #148 — the engine installs no RMM allocator, and `gpu_memory_limit` is accepted and ignored
**Priority: high**

Nothing under `cpp/src/` or `cpp/include/` calls `set_current_device_resource`, so every cuDF
intermediate takes rmm's default: a `cudaMalloc`/`cudaFree` driver round trip each.

Measured: TPC-H q1 over sf40 whole-table on GB10 is 76.5 s execute (2nd-min of 5, all runs
inside [75.4, 78.8], so steady state) against 3.9 s streamed through bounded batches for the
same answer. The gap was fixed three times already — `multi_gpu.cpp`, the gtest mains and the
benchmark harness, the last two sharing `cpp/include/peacock/rmm_pool.hpp`
([#151](archive/archived-tickets.md#t151)); the engine is the only one of the four that ships.
The second half is the same fix: `gpu_memory_limit` is documented as a bound, stored at
`gpu_executor.cpp:99` and never read — the #132 shape one level up — and a pool's `maximum` IS
that bound. Care: install per device before any cuDF call, tear down on the owning thread
(`set_per_device_resource(id, nullptr)` misses the ref map), and size by host kind, an
integrated part's reservation sharing the page cache's pool. Tests: the GPU tiers stay
byte-identical, plus a case asserting a small limit is honoured.

<a id="t19"></a>
### #19 — the planner has no cardinality estimate, and the memory model pays for it
Widths are facts and source rows are facts — the schema, and the `rows`/`bytes` a scan reads
off its surviving row groups at plan time (`planner/translator/scan_mapping/parquet_meta.rs`).
What a query
does to them is guessed: `estimator.rs::rows` has a filter pass every row, an aggregate emit
one group per input row, and a join emit its larger side.

It shows twice, both in `estimate`. An accumulator is charged what it holds whatever the batch
size, so an aggregate's state is priced at its whole input — tpch q1 groups six million rows
into four and is billed six million — and that total comes off the budget before anything is
divided, so `share_per_source` and every batch size derived from it come out smaller than the
plan needs. `rows_are_certain` is what keeps the error one-directional: only the part of the
accumulator total that rests on facts can refuse a plan, so a guess shrinks batches and never
rejects a query.

Not part of this any more: build/probe order is DataFusion's own JoinSelection over its own
nodes, which nothing of ours now sits between — the 55-of-103 measurement this ticket opened
with was of a wrapper tree that is deleted. #147 is the mechanism a real estimate would arrive
through; #73 and #20 are what it unlocks.

<a id="t16"></a>
### #16 — Dynamic / runtime filters: build-side keys → probe-side scan
Star-schema fact scans read 100% of rows while the joined dimension is filtered to ~30%. Build
an IN-set / min-max (later Bloom) at build completion and feed the probe-side GpuScan.

Applies to 76/99 TPC-DS queries; validate on q3/q19/q33. Best after #19. **Design it as the
groundwork for CTEs, not a join-to-scan special case.** A dynamic filter is the first thing here
whose producer has two consumers: the build side feeds the join, and it also feeds a replanning
consumer that turns those keys into a predicate on a scan below. That is a diamond, and every
plan model here is a tree. What serves it — a fork handing one batch stream to N consumers, plus
a consumer that plans rather than executes — is what a materialized CTE needs ([#101](#t101))
and what [#147](#t147) calls refinement in flight. Do it after the streamed lanes, which
suit the shape: with refcounted handles ([#145](#t145)) a tee costs nothing on the device, the
accountant already models a fork's residency as the slowest consumer's backlog, and a consumer
blocking its producer is the join hold, one rule already mutation-tested. A diamond in the plan
is then routing rather than scheduling.

<a id="t20"></a>
### #20 — Join enumeration: DPccp/DPhyp cost-based tree reshaping
DataFusion 45 has no join enumerator — trees come out in FROM-clause order, and ~70/99
TPC-DS queries have 4+ joins (q64 ≈ 18 tables). Implement DPccp (extend to DPhyp, IKKBZ
fallback beyond 14 tables) as a logical rule after PushDownFilter, cost = Σ intermediate
cardinality. Blocked by #19. Landing rewrites all plan goldens.

<a id="t71"></a>
### #71 — GPU scan: no predicate pushdown into the cuDF read
Partly addressed: stats-based row-group pruning exists
(`planner/translator/scan_mapping/rowgroup_prune.rs` → cuDF `set_row_groups`, parity with
ParquetExec). Remaining: serialize the predicate itself
into the cuDF `read_parquet` filter AST (page pruning / pre-filter during decode),
multi-file scans, dynamic ranges (#16). Cause of red widget ratios on selective queries.

<a id="t101"></a>
### #101 — No CSE / CTE materialization: identical subexpressions recomputed N times
**Priority: post-MVP**

DataFusion inlines every CTE reference and peacock re-scans each copy. Worst: tpcds q23
(CTEs ×5/×3/×2 → 6 scans), q4/q11 (`year_total` ×6/×4), q31; tpch q15 (`revenue0` ×2),
q2. Direction: CTE materialization or physical CSE; at minimum make the cost model aware.

The mechanism a materialized CTE needs — one producer, N consumers of the same batch
stream — is the one [#16](#t16) has to build first for dynamic filters, and it is cheap in
a streamed-batch model and expensive in a single-resident-table one. Sequence them
that way round.

<a id="t73"></a>
### #73 — Cost-based optimizer (CBO) umbrella
Move physical planning from static heuristics to cost-based, validated against the DuckDB
cost oracle and goldens. In scope: adaptive filter placement, join enumeration (#20),
stats (#19, the load-bearing prereq), runtime filters (#16).

<a id="t75"></a>
### #75 — Refactor duckdb_cost.py: separate cost formula from extraction
**Priority: low** — not started (867 lines, 29 top-level defs as of 2026-08-05), and it buys
readability, not behaviour.

~870 lines where the ~80-line cost formula hides inside JSON parsing, predicate parsing
and row-group pruning. Shape: preprocessor → flat per-node intermediate
(op/rows_read/bytes_read/out_rows/out_bytes/breaker) → one-line cost. Pure refactor:
`.duckdb_cost.txt` numbers must not move.

<a id="t136"></a>
### #136 — GpuHashJoin: build-side match tracking when the probe side streams
Left-outer, full, semi, anti and mark need "which build rows matched across all probe batches",
and that never crosses the ABI — every call rebuilds the join from scratch.

`peacock_executor_execute_node` returns only the joined table plus rows/varlen stats, and there
is no persistent `cudf::hash_join` in `join.cpp`. Inner composes per-batch and right-outer emits
its unmatched probe rows batch-locally, so those are unaffected. No-ABI-change plan (v1):
accumulate each probe batch's key columns only, keys being small next to rows, and at the finish
call run one `left_anti_join(build, accumulated_keys)`, semi form for semi/mark, then null-pad
the probe columns with a synthesized project. Correct for pure equi-joins, `null_equals_null`
applying to the finish join too and #80's NOT IN caveat carrying over. A keys-only input cannot
evaluate a residual filter, so filtered semi/anti keep a single-batch probe. Cost: the keys stay
resident and the build side is built once more at finish. If that bites, the ABI options are a
per-call match bitmap out-param or a per-seq join session that also removes the rebuild — weigh
them with the rest of the join surface, [#155](#t155).

<a id="t137"></a>
### #137 — the planner does not drop null join keys before the shuffle
With `null_equals_null=false` an all-null key matches nothing, and `spark_hash_partition.cu`
skips null columns, so every such row lands in the one partition `pmod(seed, N)`.

On a null-dominated input that is pure shuffle skew carrying rows the join discards anyway. Fix,
for the sides whose unmatched rows are never emitted — both sides of an inner join, the probe
side of left-outer/semi/anti, and the capability matrix knows which: the translation layer
inserts `GpuFilter(<key> IS NOT NULL)` under the feeding `GpuEmitPartitions`. Existing node and
serialization, cost-accounted in the plan, and `GpuEmitPartitions` keeps its one routing.

Two halves are out of scope deliberately. Scattering null-keyed rows on placement-free sides
(outer/anti's preserved side) needs a kernel knob and a conformance-gate extension, and no
corpus query exercises it. The adaptive form — insert the filter at replan time — waits on
adaptive replanning existing at all.

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

<a id="t139"></a>
### #139 — GpuCoalesceBatches(target): compact post-filter fragments
Dropped from v1. After a selective filter, batches shrink to a few rows and every
downstream kernel pays per-launch overhead on each fragment. A `BatchAccumulator` that
concatenates to a minimum target size (DataFusion semantics: merge only, never split),
streaming out one batch whenever the threshold is crossed. `cudf::concatenate` via the
existing collapse arm — no C++ change; target size from the same budget rule that sizes
loader batches. The T0 prototype has the node
(`scripts/exec_model/operators/accumulators.py`, `ReBatchToTarget`) so the drivers are
shown to tolerate one at any tree position; it also splits, which the ticket's node does
not need, because the prototype uses it to make a stream's batches any shape.

<a id="t140"></a>
### #140 — broadcast joins (1:N partition broadcast)
Deferred by the design. Lets one partition (small dimension side) be broadcast to all N
partitions of the other side without shuffling the big side; also unblocks partitioned
cross/nested-loop joins. The blocker is consume-once: a GPU handle feeds exactly one
call, so a broadcast build needs either an explicit device copy (`GpuBatch::copy()` — 
keep `!Clone` so the cost stays visible at call sites) or a C++-side non-consuming/
refcounted handle. Interacts with #136's persistent-build option, which would solve both
at once.

<a id="t141"></a>
### #141 — the planner cannot skip the shuffle for small group-key sets
v1 skips `GpuMergePartitions` + `GpuEmitPartitions` around an aggregate only when the
input is already one partition or the aggregate is keyless. Skipping when the key set is
merely small (collapse to one partition, run `GpuAggregateBatches[final]` once, avoid the
shuffle) needs a cardinality estimate that does not exist — the estimators are constants
(#19). When stats land, add the rule and regenerate the affected plan goldens.

<a id="t147"></a>
### #147 — PlanEstimates: a tree the planner emits and the runtime refines
The planner's `target_batch_bytes` walk already computes a per-node maximum resident size and
throws all but one number away. Keep it, as a tree shaped like the plan, one estimate per node.

`ParquetBatchPartitioner` emits it beside the row-group mapping. Nothing in the plan's
executability depends on it, but a wrong estimate is not free: too low and the query dies at the
accountant's `scratch_bytes` pre-check, or as a cuda OOM below that. Neither is a wrong answer,
and #142 handles both gracefully later; a better estimate makes fewer queries reach either. Two
consumers, neither existing yet. **Placement** moves subtrees onto the CPU where the GPU cannot
hold them — the `Backend` trait already makes that a matter of choosing per node. **Refinement
in flight** extrapolates from one batch actually read, since the estimates otherwise rest on
constants (#19). The first version rewrites only what needs no replanning, a still-reading
loader's remaining batch sizes; later revisions may replace the plan outright, killing
in-progress GPU work and rebuilding the driver rather than editing the running tree — which is
why the driver owns no state a caller must survive it.

<a id="t146"></a>
### #146 — aggregate shaping beyond the fixed sequence
**Priority: low** — each part optimizes an already-correct plan and needs the same estimate.

The aggregate sequence applies one shape everywhere — per-batch init, merge per lane,
shuffle, finalizing merge — right where group cardinality is far below row count.

Three shapes it cannot express or choose. **(a) A merge accepting raw rows.**
`GpuAggregateBatches` requires pre-aggregated state, so the per-batch `GpuAggregate` is
mandatory: a lane with few batches pays B groupbys where one over the concatenation would do,
and a non-reducing aggregate pays an init that shrinks nothing. A raw-accepting merge holds
rows where a state merge holds groups, so it fits only the non-reducing case. **(b) A loader
emitting one batch per partition**, where its consumer materializes the whole partition anyway
— decide it from the subtree beneath the loader. **(c) The cardinality estimate all three
want** — does this aggregate reduce? — which the constant estimators (#19) cannot answer and
which gates [#141](#t141). Land after #19.

<a id="t145"></a>
### #145 — Refcounted handles: stop copying every partition out of a scatter
`spark_hash_partition` returns one table whose N partitions are already contiguous, and
`node_session.cpp` (~L265-272) deep-copies each range out, because a handle owns its memory.

So every shuffle copies its whole input a second time and peaks at twice the data — the concrete
form of [#91](#t91)'s repartition spike, once per aggregate and once per join side. The change:
`TableResult` (`plan_executor.h:13`) becomes a `shared_ptr<cudf::table> owner` plus a
`cudf::table_view view`, and the scatter registers N handles sharing one owner. Mechanical but
wide — 35 sites across 11 files touch `.table` / `->table`. **No ABI change**: a handle stays a
`u64`. The cost to weigh: a slice pins its whole parent, so a skewed hash leaves one hot lane
holding the pre-scatter table — the peak halves and the tail lengthens. Also unlocks
[#140](#t140). Tests: the GPU tiers stay byte-identical, plus a gtest releasing N−1 handles and
reading the survivor. A streamed join waits on it too: a handle is erased by its reader
(`node_session.cpp:254`), so `Input::BuildSideCopy` has no build side after the first probe batch,
and T16 refuses a second until this lands ([#152](#t152)).

<a id="t144"></a>
### #144 — multiple DISTINCT arguments need a gid-multiplying expand
**Priority: low** — no query in either benchmark has this shape.

`count(DISTINCT a), count(DISTINCT b)` over different expressions is the one distinct shape the
lowering cannot express.

Single-distinct lowers to grouping on the distinct argument, and non-distinct companions ride
along because Σ over the inner groups recovers each total. Two distinct arguments break it: one
grouping can dedup `a` or `b`, not both, and grouping by `(a, b)` dedups neither. The standard
fix is Spark's `RewriteDistinctAggregates` — an expand multiplies each row into one per distinct
argument tagged with a group id, the inner aggregate groups by `(keys, gid, args)`, and the
outer computes each count from the rows carrying its own gid. That needs a new row-multiplying
node, which is why it is a ticket rather than a planner tweak. Not [#65](#t65), whose gid is the
ROLLUP/CUBE `__grouping_id` — the two would coexist as separate columns. Until it lands the
planner refuses the shape at plan time.

<a id="t142"></a>
### #142 — no recourse for oversized batches
Nothing downstream of the loader can split a batch: minimum load granularity is one row
group, `GpuCoalesceAllBatches` before a join build side can exceed any budget, and the
planner deliberately still produces a plan — `executor/driver/accounting.rs` then trips at run time and
the query dies cleanly. Recourse options, deferred until better estimators and adaptive execution: a split
operator (needs a C++ slice-to-handles entry point), or adaptive replanning on trip (re-plan
with more partitions or smaller batches) — the second being the only one that would make a trip
anything other than the end of the query.

Both checks abort today, pre-call and post-call alike, and the pre-call one refuses on an
estimate rather than on a fact. Recording it and letting the call proceed is the cheaper
recourse and is deliberately not taken: `RunError::BudgetExceeded` now carries which check
tripped, so something can branch on it, but there is nowhere to record into — `RunReport` has no
trip log, and `Underestimate` is the precedent for what one would look like. Related: #91.

## Infrastructure / process

<a id="t197"></a>
### #197 — the repartition arm still concatenates a child it can only be handed one of
`node_session.cpp`'s Hash-repartition arm concatenates `child[0]`'s handles before scattering,
and the planner puts a `GpuCoalesceAllBatches` above the merge feeding an emit, so it gets one.

The comment there said to retire the branch when the legacy modes retired. They have, so the
condition is met and nothing left in the tree can hand this arm two handles — the concat is a
copy of a single table on every call. Removing it needs a device run to prove, which is why it
is a ticket rather than part of the rename that found it.

<a id="t196"></a>
### #196 — the table registrar's non-parquet guard does nothing, so a stray file panics
`read_table` in `lib.rs` opens with `if path.extension() != Some("parquet") { () }` — the
condition is computed and discarded, so a non-parquet entry falls through to
`ListingTableUrl::parse` and four `unwrap`s. The caller's `let Ok(..) else { continue }` says
the intent was an `Err` there.

Nothing in the tree provokes it: every dataset dir holds parquet and nothing else, and
`.duckdb_cache/` is a sibling rather than a child. The CLI is what makes it reachable by a
user, since it registers whatever directory it is pointed at. The fix is the `return Err(())`
the shape already asks for, with a case putting a non-parquet file in the dir.

<a id="t195"></a>
### #195 — the corpus is numeric-aggregate heavy, and six shapes have no query at all
Measured off `tp1-single.plans.txt` over the 61 enabled queries and the four largest held
back: node counts run 33 to 267 while distinct node kinds run 6 to 12, median 9. More corpus
tests scale, not surface, so each shape below wants a hand-written query over the existing
tables, no new dataset, plus the engine work it needs.

- `ORDER BY … LIMIT n OFFSET m`: every corpus limit has `skip=0`, so both lowerings' offset
  half is synthetic-only; shape it away from [#166](#t166)'s two droppers.
- `min`/`max` over a string or date column: zero uses, and the merge is a string reduce.
- a join on a nullable key: [#59](#t59), [#80](#t80) and [#137](#t137) rest on there being none.
- a shuffle keyed on a decimal: not a query but [#95](#t95)'s kernel work, and the murmur3
  conformance gate extended to cover it.
- two `DISTINCT` args over different expressions: [#144](#t144) has no refusal of its own, and
  `count_distinct` marks queries this mode handles, so a grep for one finds the wrong two.
- a wide `SELECT DISTINCT`: dedup whose state is the whole row, the compaction worst case.

<a id="t178"></a>
### #178 — two CI runs share the GPU host, and the pool is sized for one
`gpu-tests` uses a per-run `REMOTE_DIR` so two runs do not overwrite each other's files, and
nothing stops their test binaries running at the same time.

Single-tenant GPU is an invariant this repo states and enforces *within* a process — GPU binaries
run `--test-threads=1` because cuDF and RMM share one process-wide pool. Across runs it is enforced
by nothing, and the RMM pool master installed makes the collision loud: two jobs overlapping by
under two minutes gave `std::bad_alloc: out_of_memory` in `pool_memory_resource` on three sf40
tests, at 14.38 GiB peak on a 139.7 GiB device — not a full device, two pools.

It reads as a flaky GPU tier, which is the expensive way to meet it: the failure is in whichever
run started second and re-running it alone passes.

**Fixed** by `15209636`: the job carries `concurrency: group: shad-gpu, cancel-in-progress: false`
at `pipeline.yml:448`, so GPU jobs queue across runs and branches. Queued rather than cancelled,
because a cancelled run leaves its `REMOTE_DIR` and the device's state behind.

The same `std::bad_alloc` in `pool_memory_resource` still reaches CI from a different cause, so
read the pool line before reaching for this ticket. Each gtest main sizes its pool at 95% of
*free* device memory (`cpp/include/peacock/rmm_pool.hpp:141`) and prints what it got, so a
neighbour on the device sets our ceiling: 132.3 GiB max on an idle device, 40.0 GiB when
something else held 97.5 GiB. `TpchSf40.Q1GroupByAggregates` needs 67.42 GiB and is the first to
die. Two of our runs colliding gives a different signature — the failure moves to whichever
started second, and the free figure differs between them. A foreign tenant gives the same free
figure to every run and fails them all identically.

<a id="t176"></a>
### #176 — the CI coverage guard checks one direction only
`every_rust_test_target_is_named_by_ci` fails when a target exists that no workflow runs. Nothing
fails when a workflow names a target that does not exist.

That way round is not silent, but it is expensive and late: cargo errors inside the cuDF leg after
the C++ build and the dataset generation, so a typo or a step added ahead of its test file costs a
full run to discover. The case: a `--test test_cpu_end_to_end` step was added three commits
before the file, and both legs went red on it.

The converse is nearly free — `workspace_test_targets()` and the `--test` line parsing both exist,
so it is one assertion that every target pipeline.yml names is in the workspace set. The exemption
list already gets this treatment; the step lines do not.

<a id="t174"></a>
### #174 — two clamps for one rule, and nothing compares them
`RowRange::clamp` (executor.rs:139) and C++ `clamp_row_range` (node_session.cpp:498) implement the
same row-range rule for the two backends, and no test reads both.

Its own doc says the risk: the two answering differently "would be a divergence no test of either
one alone could see". They are not even comparable as written — one returns `(offset, length)`,
the other `(begin, end)` — so the four Rust cases and the ten C++ cases each prove one side.
`executor_cases.inc` is this repo's answer to that shape: one table of inputs and expected answers
that both engines read. The claim that landed with the second clamp, "RowRange::clamp is now the
one clamp", is what this corrects.

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

<a id="t164"></a>
### #164 — a column ordinal reaches cuDF unchecked, and a bad one degrades rather than throws

The C++ half of [#135](archive/archived-tickets.md#t135), which the planner
closed on the Rust side by checking a reference's name against the field at its position.

`TableResult` is a `cudf::table` plus a name vector with no invariant that the two are the same
length, and the six sites indexing names use `operator[]`, so a short vector is undefined
behaviour rather than an exception — `filter.cpp` ~L42 reads `fv.column(idx)` and
`input.column_names[idx]` in one iteration and only the first is checked. Assert
`num_columns() == column_names.size()` where `TableResult` is built. Separately `expr.cpp` ~L349
returns `type_id::EMPTY` for an out-of-range `ColumnRef` instead of throwing, turning a bad
ordinal into a confusing type error further along. The third closure #135 named is unstarted and
belongs here too: a per-node type check in the GPU tiers, the only thing that would surface a
wrong-order subtree before the root.

<a id="t163"></a>
### #163 — a declared type is never checked against the expression that produces it

Union's branch check, the root against the DataFusion plan, and `types_across_the_edge` each
compare one declared schema against another rather than deriving one from an expression, so an
aggregate's declared state types are checked nowhere — `state_fields` is DataFusion's answer, not
the producing expression's.

Both engines price a node from the declared schema, so a wrong type moves no golden byte. T16
confirmed it on a device: cuDF's Welford count exports Int64 where every plan declares UInt64.

T17 closed the widening arm only (`widened_decimal`, `executor/cpu_backend/`). The signed arm remains:
`avg` declares its count state UInt64 and DataFusion's accumulator produces Int64 — no widening, and
it must not be escaped the same way, since accepting it masks what the device showed. The queries
disabled on this return with the fix, not by loosening the guard. Its column runs 1 to 10 over T19's
seventeen `avg` queries — wherever the count state sits in that query's own state row — so no fix
special-casing a position can work.

Fix: derive each expression's output type and compare it against the declared field, for the
nodes that compute rather than carry. Same class as [#135](archive/archived-tickets.md#t135).

<a id="t159"></a>
### #159 — RightSemi/RightAnti with a residual filter has no cuDF path
The mixed_* family evaluates a residual during the join, and no swapped variant exists — so a
right-semi form carrying one cannot be expressed and the planner refuses it.

Reachable: `SELECT b.v FROM big b WHERE EXISTS (SELECT 1 FROM tiny t WHERE t.k = b.k AND
t.v < b.v)` plans as RightSemi with a residual once statistics make DataFusion swap the sides,
so this is not a shape only a constructor produces. Pinned by the refusal test in
`test_planner_join_capability.rs`. Two ways out: keep the emitted side as the build so the
join stays a Left form and the existing `mixed_left_*` applies, which is a planner change; or
a swapped `mixed_*` in cuDF, which is not ours. The first is cheap and has not been costed.

<a id="t160"></a>
### #160 — nested-loop join supports Inner and Left only
`execute_nested_loop_join` handles Inner and Left; every other type is refused at plan time
rather than throwing in the executor.

`SELECT * FROM tiny t FULL JOIN big b ON t.v > b.v` is the reachable case. The C++ builds the
full cartesian and applies a mask, and a mask cannot re-emit the unmatched rows an outer form
owes — the same argument [#153](#t153) makes for the equi path, which `join.cpp` already
states in a comment beside the guard. Semi and anti forms would need the mask plus a
distinct-on-the-preserved-side pass. No corpus query has one; the refusal is what keeps that
true rather than discovering it at run time.

<a id="t161"></a>
### #161 — aggregate shapes the planner refuses: FILTER, and functions with no decomposition
Two refusals in the aggregate arm. A `FILTER (WHERE …)` clause has no lowering, and an
aggregate function outside the decomposition registry is refused by name.

Only the second is reachable: `SELECT median(v) FROM tiny` is refused by name, while
`sum(v) FILTER (WHERE v > 0)` does not parse in DataFusion 45 at all, so that refusal is
constructor-only until the parser gains the clause. The FILTER form would lower to a CASE
inside the aggregate argument and needs no new node; the registry gap is per function and each
wants its merge aggregator stated. Neither shape appears in either benchmark, which is why
they are refusals rather than work.

<a id="t162"></a>
### #162 — expression forms the planner refuses
`TRY_CAST`, the regex match operator, an unrecognized binary operator, and an unrecognized
expression kind are each refused by name at translation.

Every one is a gap in `planner/translator/expr.rs` rather than a limit of the surface: the C++ has
`build_expr` cases for most of them, and what is missing is our mapping. They are refusals
because no corpus query carries one, so the cost of each is one arm and its test. `IN ()`
belongs to this family but does not parse, so it is reachable only from a constructor and is
covered as a unit test rather than by a query.

<a id="t113"></a>
### #113 — Provision GPU-host testdata by sweeping git-tracked files
Hand-maintained rsync lists (`pipeline.yml` gpu-tests, `scripts/build-test-shadgpu.sh`)
bit four times during the refactor. Fix: sweep
`git ls-files --cached --others --exclude-standard testdata`. Lessons from the parked
draft: `--exclude=*.parquet` is wrong (tpch.minimal commits 5 needed .parquet files);
tracked-only misses new uncommitted fixtures. Needs its own clean verification run.

Bit a fifth time in T19: the script pushed no query directory where `pipeline.yml:461` rsyncs both,
so a T17 query was absent on the host. An absent file is loud; a stale one runs old text and writes
cells describing a query the repo does not contain, with nothing red.

<a id="t134"></a>
### #134 — begin_plan's out_node_count is unused on the prod path
The C++ reports how many fb nodes it indexed; `RecipePlan::wire_nodes()` is what the writer
created. Comparing them is the one free check that both sides number one tree, which every
handle's seq rests on — and no library code reads it: `src/` never calls `begin_plan`, so
outside the tests the number is returned and dropped. Three of the four test openers compare
(`corpus_gpu.rs`, `test_gpu_recipe_walk`, `test_gpu_executors`); `test_gpu_abi` does not. Fix:
a helper beside `RecipePlan` that opens a plan and errors naming both numbers, used by all.

<a id="t129"></a>
### #129 — The "26.02" CI leg builds against a 25.10a image; the GPU job has no fork guard
Two unrelated smells in `pipeline.yml`, both found auditing the CI section of
build-test.md. (a) The dataset-matrix matrix leg labelled `cudf: "26.02"` runs
`rapidsai/base:25.10a-cuda12-py3.12`, so the compile-only 26.02 coverage the wiki and
`#94` both rely on is actually 25.10a coverage; the label is the only place 26.02 appears.
Either bump the image or rename the leg — as it stands, "26.02 compiles" is a claim no job
makes. (b) `s3-datasets` explains its fork guard as mirroring "the GPU job's fork guard",
but `gpu-tests` has no job-level `if:` — on a fork PR `secrets.SHAD_GPU_SSH_KEY` is empty,
so Setup SSH writes an empty key and the job goes red on ssh instead of skipping. Moot
while the repo has no forks, which is exactly why it will bite later.

<a id="t128"></a>
### #128 — Doctests run nowhere, and the meta guard cannot see them
No step in `pipeline.yml` passes `--doc`, and `test_ci_coverage.rs` enumerates `--test`
targets plus `--lib`, so a doctest is invisible to the guard whose whole job is finding
targets CI does not run. The crate has none today: the one it had documented an entry point
that no longer exists.

There is now one pipeline to document, and it is three calls in a fixed order —
`planner::plan` for the tree, `wire::attach_recipes` where a device is involved, and
`executor::run` over a backend. `peacockdb/src/main.rs` is the only place that
sequence is written down, and a reader of the crate meets the three functions separately. A
doctest on the entry it documents is the natural fix and the reason to close both halves at
once: write it, run `cargo test --features rust-only -p peacockdb-core --doc` in the
dataset-matrix tier, and teach the guard that `--doc` is a target class it must see named
(the `--lib` check at `line_runs_lib_tests` is the pattern).

<a id="t127"></a>
### #127 — Unowned testdata dirs on shad-gpu
shad-gpu carries `testdata/plans.sf1` (10 files) and `testdata/plans` (3) that no local
checkout has, that the git-derived fixture sweep does not produce, and that nothing in
the test suite references. Same shape as the binary orphans that `--delete` just closed:
state on a remote host no provisioning path owns, so nobody can say whether it is stale
or load-bearing. Not deleted, because something outside this repo may read it. Establish
what wrote them and either bring them under the sweep or remove them.

<a id="t125"></a>
### #125 — `elsewhere` parameter in assert_registry_matches_csv is dead
`peacockdb-core/tests/common/registry.rs`: after the by-mode file split every CSV column
is owned wholly by one binary, so all four callers pass `&[]` and the ~20 lines of
staleness checking can no longer fire. Reviewer's read is *delete* (coding-style.md: no
fallbacks the task didn't ask for); the parameter was kept only to hold branch scope, and
its doc paragraph was rewritten to stop justifying it with a now-false example. Re-add it
in the commit that actually splits a column across binaries again.

<a id="t13"></a>
### #13 — Hermetic builds: system-library whitelist + CI audit
`ld` silently prefers system libs over the conda env (seen as
`libarrow.so.2300: undefined reference to curl_easy_getinfo@CURL_OPENSSL_4`). Whitelist
glibc/libgcc_s/libcuda only; everything else from `$CUDF_ROOT`. Enforce via CMake
find-root pinning, build.rs link-search order, and a post-link `ldd` audit that fails CI.

<a id="t94"></a>
### #94 — MERGE_M2 count-child type is cuDF-version-specific
The stddev/var Final path (`cpp/src/operators/aggregate.cpp`) hardcodes INT32
`valid_count` for the 25.02 GPU runtime; 25.10/26.02 want INT64/FLOAT64. The 26.02 CI leg
is build-only so it stays green — this bites at the next GPU-remote cuDF bump. Switch or
version-gate the type then; the comment marks the site.

<a id="t69"></a>
### #69 — DuckDB cost oracle: multi-threaded golden generation for larger SF
`gen_duckdb_cost.sh` pins `PRAGMA threads=1` because `operator_rows_scanned` scales with
thread count (`output_bytes`/`output_rows` are thread-invariant). Fine at sf1, too slow
at sf10/sf100. Simplest fix: parallelize across queries, keep per-query threads=1; or
drop/normalize the thread-sensitive field.

<a id="t49"></a>
### #49 — Test crates bake CARGO_MANIFEST_DIR for testdata
**Priority: low**

Some tests read testdata through `env!("CARGO_MANIFEST_DIR")` instead of `testdata_root()`,
so they only find their data where the build tree stood.

It matters because a test binary is built on one host and run on another: remote CPU runs ship
binaries, goldens and data but never source, so a compile-time path is a path the remote does
not have. `tests/common/mod.rs testdata_root()` solves that by honouring `PEACOCK_TESTDATA_DIR`
first, which `build-test.sh` sets for remote runs. The residual is the crate's own unit tests
— five files under `peacockdb-core/src/` reaching `tpch.minimal`: `planner/memory_estimation.rs`,
`scan_mapping/parquet_meta.rs`, `plan_text/tests.rs` and `translator/{tests,schema_tests}.rs` — which is
exactly why a remote CPU host needs a `/media/data/peacockdb` symlink and why `--gpu` runs,
which set the env var, do not.

The sweep is not the one the integration tests got: a unit test cannot reach
`tests/common/mod.rs`, so the fix is a `#[cfg(test)]` helper in `src` honouring
`PEACOCK_TESTDATA_DIR` with the same fallback, and the two spellings then have to be held to
each other or they are the drift this ticket is about one layer down.
`test_ci_coverage.rs` and the fbs reader in `test_plan_goldens` are not this: their
`CARGO_MANIFEST_DIR` resolves the repo root to read committed source, not testdata, and no env
var should redirect that.
