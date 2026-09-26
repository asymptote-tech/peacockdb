
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
Lay the foundation for #140 - broadcast joins - in this task.

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

<a id="t45"></a>
### #45 — q24 GpuHashJoin: 'Unary cast type must be fixed-width'
A join-key cast targets string; cuDF's unary cast rejects non-fixed-width. Diagnose the
emitted cast and handle string keys by hashing rather than casting
(`cpp/src/operators/join.cpp`).

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
`NOT IN`. No enabled query has a nullable anti/mark key. Wants a dedicated analysis plus
expr/join goldens covering nullable IN / NOT IN / EXISTS before defaults change. Anti
remainder overlaps #80.

The two engines disagree today, not latently: the cpu's `HashJoinExec` takes the node's
`null_equals_null` for every type, so under the SQL default a LeftAnti keeps its null-key build
rows, a RightAnti its null-key probe rows, and a LeftMark marks them `false`, while the device
drops or marks them `true`. The device's answer under `false` is exactly the cpu's under `true`,
which is how the nine `bug_…null_key…` cases in `gpu_tests/join_cases.rs` pin it — a finish pass,
a residual filter and a streamed probe all included. The composite-key form is
`bug_a_left_anti_join_on_a_composite_key_matches_a_null_in_the_second_column_on_the_device`
in `gpu_tests/join_dimension_cases.rs`: a null in the second key column alone is a match.

<a id="t60"></a>
### #60 — q78 GPU diverges in anti-join + top-N; possibly memory-borderline
3-CTE anti-join (`LEFT JOIN … IS NULL`) + multi-key DESC LIMIT 100. `round` is proven
fine (q54). The isolated diff harness segfaults under GPU memory pressure on a shared
H200 — re-check on a free GPU to separate wrong-result from memory-induced.

<a id="t215"></a>
### #215 — a left nested-loop join over a predicate the AST cannot take is refused on the device

A `LEFT JOIN` with no equi-key whose predicate has a decimal operand or a string literal answers
on the cpu and throws on the device: `non-AST-able NestedLoopJoin filter is only supported for Inner joins`.

`plan/join.rs` admits a `Left` nested loop over any predicate — it checks the probe's batch
layout and nothing about the expression — and `join.cpp` (`execute_nested_loop_join`) has two
paths: a cuDF AST conditional join, which knows Left, and the cross-then-mask path for what
`is_ast_able` refuses (a `CAST` to `Decimal128`, a string literal), which is written for Inner
alone and throws at the guard. A mask over a cross product cannot re-emit the unmatched build
rows an outer form owes, which is #160's argument for refusing the other types at plan time;
this shape is the one the planner lets through. Pinned by
`bug_a_left_nested_loop_join_with_a_decimal_predicate_is_refused_on_the_device` and its
projected neighbour (`gpu_tests/nested_cases.rs`). Numbered past #214.

<a id="t208"></a>
### #208 — the cpu's cross join answers nothing over a zero-row build side

A `GpuCrossJoin` whose build batch has zero rows emits no batch on the cpu, where the device
emits one of zero rows; a zero-row probe batch is zero rows on both.

DataFusion's `CrossJoinExec` ends its stream without a batch when its left side is empty
(`cross_join.rs`, `left_data.num_rows() == 0`), and `CpuProbingJoin::probe_and_fetch` hands that
empty answer up as the call producing nothing; `cudf::cross_join` over a zero-row left is a
zero-row table. `NestedLoopJoinExec` over the same shape emits a zero-row batch, so the cpu's two
predicate-free joins disagree with each other as well as with the device. Nothing and a zero-row
batch are different arrivals downstream, as #205 says. Pinned by
`bug_a_cross_join_over_a_zero_row_build_is_nothing_on_the_cpu` and its both-sides-empty neighbour
(`gpu_tests/nested_cases.rs`).

<a id="t207"></a>
### #207 — both backends drop a cross join's projection

A `GpuCrossJoin` carrying a projection emits every column of the crossed table on both engines:
the cpu refuses at `declared_as`, the device hands the wider table up under the narrower one.

The planner writes one: a predicate-free `NestedLoopJoinExec` with a projection becomes a
`GpuCrossJoin` with it (`translator/nodes.rs`), and `check_projection` validates it. Then nobody
applies it — `CrossJoinExec::new` takes none (`cpu_backend/join.rs`), `CudfCrossJoin` has no
projection field (`gpu_plan.fbs`) so `cross_join_payload` writes none and `execute_cross_join`
applies none. #190 is the cpu half of the same defect for the nested-loop join, where the device
does apply it. On the device every ordinal above the join then reads one column of some other
(#135's shape). Pinned by `bug_a_cross_join_projection_is_dropped_on_both`
(`gpu_tests/nested_cases.rs`) and, read at the handle, by `nested_schema_cases.rs`'s
`bug_a_cross_join_with_a_projection_holds_every_column_on_the_device`.

<a id="t63"></a>
### #63 — a zero-column placeholder survives the cross join and shifts every ordinal above it
A project with no expressions has rows but no columns, and a cuDF table cannot say that:
`num_rows()` reads column 0. So `execute_project` (`cpp/src/operators/project.cpp`) emits one INT8
column, `__rowcount__`. `execute_cross_join` (`cpp/src/operators/join.cpp`) concatenates both
sides' columns and names and drops nothing, so the placeholder reaches its output. The plan
declares N columns, the device holds N+1, and every ordinal past the placeholder is one off.

Filed as a `copy_if_else` size mismatch between a one-row scalar branch and a full-size one. That
was a misreading. The 2026-09-17 run, the first since the cpu stopped refusing q9, fails at
`copy.cu:367: Both inputs must be of the same type`: the CASE's branches read shifted columns of
different types. Where the shifted columns share a type the answer is silently wrong instead,
since nothing between device nodes checks a column count ([#164](corpus-coverage.md#t164)).
`reports/corpus-fixes.md` (fix 4) gives a two-subquery shape that answers 25 on the device and
24 on the cpu.

**Corpus queries:** `tpcds/q9` — a CASE over fifteen scalar subqueries (a `count(*)` and two
`avg`s per bucket) on `FROM reason WHERE r_reason_sk = 1`, planned as a chain of cross joins over
seven `GpuProject: exprs=[]` (`tpcds.sf1/tp1-single.plans.txt`). All five gpu cells are off and
registry row 10 tags `63` alone. Only tp1-single has run on a device (`corpus_cases.inc:257`);
the other four may meet [#152](#t152) next.

**Fix proposed:** fix 4 of `reports/corpus-fixes.md`. Give the placeholder one name in
`cpp/src/peacock/operators.h`: `row_count_table(rows)` and `is_row_count_only(t)`, and have
`execute_project` use them. Give the cross join a rows-only arm in cuDF's own row order: both
sides rows-only → `row_count_table(l × r)`; left rows-only → `cudf::tile(right, l)`; right
rows-only → `cudf::repeat(left, r)`; else `cudf::cross_join`; one overflow check above them all.
No wire or ABI change, no golden moves. Proof: gtests in `cpp/tests/gpu/test_plan_executor.cpp`
for each arm, the report's two-subquery shape as a corpus query at every mode, and `tpcds/q9`
enabled at tp1-single on shad-gpu. The report's second arm — a scan projected to no columns,
which cuDF's cross join refuses with "Left table is empty" — shares the helper and lands with it.

<a id="t212"></a>
### #212 — a build side that emits no batch at all still refuses Right, Full and RightAnti
A Right, Full or RightAnti join whose build side hands the lane no batch is refused by name
in `without_build`, where the answer owed is every probe row, padded or not.

The scatter route to this is gone: `driver/partitioned.rs` keeps a zero-row scatter output
where the join above owes rows ([#175](archive/archived-tickets.md#t175)), and the join then computes the answer. What
remains is an upstream that emits nothing at all. Two shapes reach it. A limit that skips
everything: `(SELECT ... FROM nation OFFSET 100) n RIGHT JOIN region r` plans
`GpuCoalesceAllBatches <- GpuLimit skip=100` under the build side, at every mode. And tpcds
q77 at the three tp4 modes: its Right outer's build side is a grouped aggregate over an Inner
join, the Inner join's empty scatter lane drops as it should, its lane emits nothing, and the
aggregate emits nothing where nothing arrived. Pinned by
`bug_right_with_no_build_batch_is_refused_on_both` and its Full and RightAnti siblings
(`gpu_tests/join_cases.rs`), and on the driver by
`a_join_that_owes_its_probe_side_without_a_build_side_is_refused` (`driver/tests/flow.rs`).

<a id="t173"></a>
### #173 — a finish whose probe produced no keys cannot make the table it owes
`finish_without_keys` (`gpu_backend/join.rs`) refuses Left, Full, LeftSemi and LeftMark on the
device when a lane's probe side accumulated no keys: what each owes cannot be loaded from nothing.

One site, on the probe side. LeftAnti hands its build side up and agrees with the cpu, which
answers all five. The accumulators are not here: a collapse of no handles and a merge of no
runs answer nothing on the Rust side before any call, on both engines, and the C++ guard in
`node_session.cpp` behind them is unreachable. Unfreezing buys a make-empty-of-schema call and
the refusal goes; until then it is the contract. Blocks no registry cell. Pinned by
`bug_…_finishing_with_no_probe_batch_is_refused_on_the_device` (`gpu_tests/join_cases.rs`).

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

<a id="t159"></a>
### #159 — RightSemi/RightAnti with a residual filter has no cuDF path
The mixed_* family evaluates a residual during the join, and no swapped variant exists — so a
right-semi form carrying one cannot be expressed and the planner refuses it.

Reachable: `SELECT b.v FROM big b WHERE EXISTS (SELECT 1 FROM tiny t WHERE t.k = b.k AND
t.v < b.v)` plans as RightSemi with a residual once statistics make DataFusion swap the sides,
so this is not a shape only a constructor produces. Pinned by the refusal test in
`src/planner/tests/join_capability.rs`. Two ways out: keep the emitted side as the build so the
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
