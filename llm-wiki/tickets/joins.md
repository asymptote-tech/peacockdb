# Join tickets

**These tickets are expected to be fixed in bulk by a complete rewrite of the GPU backend's join
nodes, with accompanying minor fixes in the CPU backend, not one at a time.** The join findings
of `reports/hacks-audit.md` (its Joins section) belong to the same rewrite. The driver's
`feeds_owing_build` exception (`executor/driver/index.rs`), which keeps a zero-row scatter output
only where a join above owes rows over an empty build side, is scaffolding for today's joins and
should go with the rewrite, once joins answer an empty or missing build side themselves.

The Repartitioning tickets in [`corpus-coverage.md`](corpus-coverage.md#repartitioning) are a
prerequisite: a join's sides are shuffled into lanes before it runs, so the scatter they share
has to be settled first.

Recipe planning will no longer happen for joins after the rewrite. The new backend implementation will
not need to look at a recipe when executing a join, instead encoding the logic as plain Rust.

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
referencing both sides, which `pbench`'s `left-join-residual`, `right-join-residual` and
`full-join-residual` now have (`ON d_k = f_k AND f_qty > d_w`); all three are refused on this
ticket at all five modes. Fix: evaluate the residual during the join
(`mixed_*` covers Left, Right is its swap, Full needs inner-plus-re-add), plus a `PlanExecutor`
gtest with a filtered Left join. The same commit drops the prototype's deliberate reproduction.

<a id="t80"></a>
### #80 — Anti-join NOT IN three-valued logic + independent DuckDB result oracle
`NOT IN` with any NULL in the build side must yield the empty set; neither
`null_equality::EQUAL` nor `UNEQUAL` implements that, so ANTI/mark joins stay hardcoded
EQUAL (`cpp/src/operators/join.cpp`). Needs a planner/serializer flag distinguishing
NOT IN from NOT EXISTS, a nullable-anti-key test (no corpus query exercises it), and a
DuckDB final-result oracle, now [#235](../archive/archived-tickets.md#t235) — today's validation is circular
(goldens vs DataFusion, GPU vs CPU). Semi half done (q33; semi honors per-join `null_equals_null`).

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

<a id="t215"></a>
### #215 — a left nested-loop join over a predicate the AST cannot take is refused on the device

A `LEFT JOIN` with no equi-key whose predicate has a decimal operand or a string literal answers
on the cpu and throws on the device: `non-AST-able NestedLoopJoin filter is only supported for Inner joins`.

`plan/join.rs` admits a `Left` nested loop over any predicate — it checks the probe's batch
layout and nothing about the expression — and `join.cpp` (`execute_nested_loop_join`) has two
paths: a cuDF AST conditional join, which knows Left, and the cross-then-mask path for what
`cudf_ast_can_evaluate` refuses (a `CAST` to `Decimal128`, a string literal), which is written for Inner
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

**Corpus queries:** `pbench/cross-empty-build` was written for this ticket and does not reach it.
The planner refuses the query earlier, because a scan over a parquet file with no row groups is
rejected as an invalid plan — [#256](#t256) — so its cells are off on `208 256` and this ticket
has no corpus evidence until #256 clears.

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

<a id="t190"></a>
### #190 — the CPU backend drops a nested-loop join's projection

`tpch/q11` at all five modes: `the node declares Schema { … 2 fields } and DataFusion answered with
Schema { … 3 fields }`. The extra column is the build side's scalar, which the node's projection
drops.

`cpu_backend/join.rs:140` builds `NestedLoopJoinExec::try_new(build, probe, Some(filter),
&join_type, None)` — that last argument is DataFusion's projection, passed `None`. Forty lines
down, the hash-join path at :302 reads `node.projection` and passes it. One join family applies the
projection the plan declares and the other ignores it.

The projection is not missing from the plan: `check_projection` validates it, the plan golden
carries it, and the node's declared schema is derived from it. Only the executor ignores it.

**It refuses rather than answering wrongly by luck.** `declared_as` compares column counts before
anything reads a value, so a projection that drops a column changes the count and is caught. A
projection that reorders columns, or drops one and leaves the same count, would have produced a
wrong answer with matching shapes and nothing to catch it.

Why the corpus took until T19's sixth batch to reach it: `q11` is the first query whose nested-loop
join projects at all. `nested-loop-join`, `nested-loop-left-join` and `cross-join` are `SELECT *`,
so their projection is `None` and passing `None` is correct for every one of them.

Device half untested — the CPU refuses first, as with [#189](corpus-coverage.md#t189).
`aggregate-state-types`'s rollout, 2026-09-17, added `tpch/q22` and `tpcds/q24` at every mode —
four registry rows with `tpch/q11` and `tpcds/q54`.

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
seven `GpuProject: exprs=[]` (`tpcds.sf1/tp1-single.plans.txt`), and `pbench`'s
`scalar-subquery-cross`, which is that shape by construction and carries this ticket. All five of
q9's gpu cells are off and registry row 10 tags `63` alone. Only tp1-single has run on a device (`corpus_cases.inc:257`);
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
(`gpu_tests/join_cases.rs`); on the driver, only its propagation, by
`a_join_that_owes_its_probe_side_without_a_build_side_is_refused` (`driver/tests/flow.rs`).

<a id="t173"></a>
### #173 — a finish whose probe produced no keys refuses what it could answer from the build side

`finish_without_keys` (`gpu_backend/join.rs`) refuses Left, Full, LeftSemi and LeftMark on the
device when a lane's probe side accumulated no keys, and answers LeftAnti with the raw build side,
ignoring the join's `projection`. The cpu answers all five.

The refusal assumes the owed table cannot be made without a make-empty-of-schema call. It can:
the finish still holds the build handle (`GpuProbingJoin::build`), and the at-done pad and narrow
projects read exactly the build schema (`wire/join.rs`). LeftAnti, Left and Full are the build side
through those projects; LeftSemi is `slice_handle(build, 0, 0)`. Only LeftMark needs a table of the
keys' schema that nothing holds. LeftAnti's raw hand-up is `reports/hacks-audit.md` finding 11. The
accumulators are not here: a collapse of no handles and a merge of no runs answer nothing on the
Rust side before any call, on both engines. Pinned by
`bug_…_finishing_with_no_probe_batch_is_refused_on_the_device` (`gpu_tests/join_cases.rs`).

**Corpus queries:** none. `pbench/finish-without-probe` was written for this ticket and does not
reach it: DataFusion plans it `CollectLeft` and #140 merges both sides, so it plans `lanes=1` at all
five modes and one probe lane over `tiny`'s 8 rows always accumulates keys. Two further shapes reach it
(`reports/corpus-fixes.md` fix 15), neither in tpch or tpcds: `select count(*) from orders where
o_orderkey in (select case when l_quantity > 100 then l_orderkey end from lineitem);` (tpch) at the
tp4 modes, where every probe key is NULL
and lands in one lane, so three lanes refuse; expected 0. And `select count(*) from region r left
join (select n_regionkey from nation limit 5 offset 100) n on r.r_regionkey = n.n_regionkey;`, where
the limit emits nothing: the device refuses, the cpu answers 5.

**Fix proposed:** fix 15 of `reports/corpus-fixes.md`. Thread the build's schema into the join
(`gpu_backend/backend.rs`), and seed `finish_without_keys`'s at-done projects from the held build
handle: LeftAnti, Left and Full from the build, LeftSemi from `slice_handle(build, 0, 0)`, LeftMark
still refused. LeftAnti applies its projection. The `self.build.take()` arm becomes `ok_or_else`.
The Left and Full arms read the pad project's typed NULLs, which #198's fix made correct. No wire
change, no golden. Tests: Left, LeftSemi, LeftAnti-with-projection and LeftMark-refused cases with
cpu twins. `architecture.md`'s two passages framing this as the frozen surface's limit change when
it lands. Left open: LeftMark through a merge whose every lane drained, and the one-call finishing
joins (filtered LeftSemi/LeftAnti/LeftMark, nested-loop Left), which emit nothing over no probe
batch on both engines, a separate wrong answer.

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
distinct-on-the-preserved-side pass. `pbench`'s `nl-left-semi`, `nl-left-anti`, `nl-right-semi`,
`nl-right-anti` and `nl-mark` are those forms, all refused; the refusal is what keeps the claim
true rather than discovering it at run time.

<a id="t220"></a>
### #220 — the cpu's joins answer several batches per call where the device answers one

A cpu join hands back DataFusion's output stream for one probe call as it came: 8192-row chunks,
plus an empty batch per probe batch that matched nothing. The device answers one table per call.
Every node above the join carries the extra batches, so the per-node goldens disagree.

`probe_and_fetch` and `finish_and_fetch` (`cpu_backend/join.rs`) pass each chunk through
`declared`, one `CpuBatch` apiece. `ProbingJoin` allows a `Vec`, so no trait breaks, but
`architecture.md`'s rule does: no executor returns more than one batch per call per output lane.
`CpuExec::exec` keeps that rule by concatenating. `tpch/q15` at `tp1-single`: the cpu's join has
`batch_rows=[[8192,1808]]`, 946045 bytes, the device 946033. `tpch/q4`'s LeftSemi emits
`[[0×18, 52523]]` on the cpu, one batch on the device.

The difference usually surfaces higher up, at the aggregate over the join. A per-batch aggregate
makes one partial per chunk, so the merge above it takes more rows on the cpu. `tpcds/q93`: the
cpu's Right join emits 36 batches for one probe batch, and the merge's `in_rows` reads `[[7486]]`
against the device's `[[7169]]`. A keyless one reads `[[34]]` against `[[1]]` (`tpcds/q96`), and
`tpcds/q38` reads `[[12446]]` against `[[11788]]`. This was filed as #185, "the merge counts its
own output", because the golden comparison (`line_difference`, `golden_text.rs`) prints only the
first differing line, and the merge renders above the join. `in_rows` is the driver's sum over the
batches a node takes, the same code for both engines. These were the first cells where the device
completed a plan and only the golden caught the difference.

**Corpus queries:** 82 registry rows carry `220`, its cause at `tp1-single`, where the device gets
past #152. The first ones seen: `tpcds` q93 q96 q38 q48 q4 q18, `tpch` q3 q4 q14 q15. Plus
`tpch/hash-join`, `cross-join`, `nested-loop-left-join`, `anti-join` and `semi-join`. Every
device cell through a join lands here once #152 clears.

**Fix proposed:** on the cpu. `declared` in `cpu_backend/join.rs` returns one batch: each chunk
through `declared_as`, then `concat_batches`. No chunks gives `RecordBatch::new_empty(schema)`,
as the device answers a zero-row table. Used by both `probe_and_fetch` and `finish_and_fetch`;
the early returns with no call stay empty. Not the device: its side is the rule, and chunking it
would need an ABI change. Not the comparator: that would hide a real difference in batches and
bytes. Test: a join of 10000 matches through one probe call answers one batch, not `[8192,1808]`.
Every `.cpu.txt` section with a join regenerates. After it, re-run the `220` cells at `tp1-single`
on a device; one that fails past the join gets a fresh ticket.

## Complete Join Coverage

Join shapes the chain-J rewrite leaves refused or wrong, each with a pbench query that shows it.
Not part of the bulk rewrite above: each is its own fix.

<a id="t243"></a>
### #243 — the cpu treats -0.0 and 0.0, and NaNs of different bits, as different keys
A float join or group key equates `-0.0` with `0.0` and every NaN with every other NaN on the
device and in DuckDB, and does not on the cpu, so the two engines answer a float-keyed join or
`GROUP BY` differently wherever those values occur. At the tp4 modes the device can also disagree
with itself.

Measured on 2026-10-07 over keys `0.0, -0.0, NaN, -NaN, NULL, 1.0` (scratch probes, parquet
written by DuckDB):

| engine | `-0.0 = 0.0` | `NaN = NaN` | `NaN = -NaN` | groups of the six |
|---|---|---|---|---|
| DuckDB 1.5.4 | yes | yes | yes | 4 |
| DataFusion 45, 1 and 4 partitions, Partitioned and CollectLeft | no | yes (same bits) | no | 6 |
| cuDF 25.02 and 26.02 (`inner_join`, `hash_join`, `distinct_hash_join`, `groupby`) | yes | yes | yes | 4 |

DataFusion compares and hashes floats by their bits; cuDF and DuckDB by value, with NaNs equal.
The lane rule adds a third behaviour: comet's hasher, the cpu's lane rule and the one the device
kernel must match, maps `-0.0` to `0` but hashes a NaN by its raw bits
(`datafusion-comet-spark-expr-0.6.0/src/hash_funcs/utils.rs:78-105`). So at tp4 `NaN` and `-NaN`
land in different lanes and stop matching or grouping on the device, while tp1 merges them
(inferred from the hasher's code, not run).

Spark avoids all three by normalizing float keys before a join or an aggregate
(`NormalizeFloatingNumbers`: `-0.0` to `0.0`, every NaN to one canonical NaN), so its hash only
ever sees canonical values. The fix here is the same: the planner normalizes every float join and
group key under the key, on both engines, and the lane rule then never sees `-0.0` or a
non-canonical NaN.

**Corpus queries:** none in tpch or tpcds, whose float columns hold no `-0.0` or NaN. pbench's
`float64-key-join`, `float64-key-group` and `float32-key-group` show it (`tasks/pbench.md`); they
land as commented-out `corpus_query!` lines naming this ticket. Pins: `bug_` cases in the operator
harness for a float-keyed join and a float-keyed aggregate, cpu against device (`repartition-keys`).

<a id="t245"></a>
### #245 — a nested-type key cannot cross a shuffle
A join or `GROUP BY` keyed on a struct or list column is refused at run time at the tp4 modes, on
both engines: comet's hasher has no struct or list arm, and neither has the device kernel
(`cpp/src/spark_hash_partition.cu`). The tp1 modes, which do not shuffle, answer.

The lane rule needs a definition for nested values that both engines share — hashing each child
in order, as Spark does for a struct, with a list's elements in order — and the conformance gate
extended to it.

**Corpus queries:** none. pbench's `struct-key-join`
(`SELECT f_id, d_id FROM fact JOIN dim ON f_kstruct = d_kstruct`) was written for this ticket and
could not land: the planner panics on a Struct column before it refuses, which is
[#255](complete-coverage.md#t255). The query arrives with that ticket's fix.

<a id="t246"></a>
### #246 — a `LIKE` whose pattern is a column is refused on the device
`expr.cpp:873-877` takes a `LIKE` pattern only as a literal. A join condition that matches one
side's strings against the other side's patterns — a nested loop, since it has no key — is
refused on the device; the cpu answers. The same holds for any `LIKE` over two columns.

cuDF's `strings::like` takes a scalar pattern; a column of patterns needs a per-row arm (one
`like` per distinct pattern, scattered back, or a regex per row).

**Corpus queries:** none in tpch or tpcds. pbench's `like-column-pattern`
(`SELECT d_id, t_id FROM dim JOIN tiny ON d_name LIKE t_pat`), its device cells off on this ticket.

<a id="t250"></a>
### #250 — an `IN` subquery whose NULL answer is read is refused when its data holds NULLs
`x IN (S)` is three-valued: NULL when `x` is NULL and `S` is non-empty, or when `S` holds a NULL
and `x` matches nothing. Where a filter reads that NULL as a value — `IS [NOT] NULL`,
`IS [NOT] TRUE/FALSE/UNKNOWN`, a comparison (`(x IN S) = false`), `COALESCE`, a function argument
— the answer needs a NULL the engine cannot produce: DataFusion 45 plans the `IN` as a mark join,
and a mark is never NULL (`join_type.rs:56-69`). The join-rewrite chain's `NOT IN` rule (design
§3.5) answers every `IN`/`NOT IN` reached through `AND`, `OR` and `NOT` alone (it puts the
predicate in negation normal form first), and refuses these forms at plan time, by name, wherever
the data says `x` or `y` can be NULL; where neither can, `IN` is two-valued and plans.

The fix is the three-valued rewrite: `CASE WHEN EXISTS (S AND y = x) THEN true WHEN (x IS NULL AND
EXISTS (S)) OR EXISTS (S AND y IS NULL) THEN NULL ELSE false END`, planned as two mark joins and a
count, or a nullable mark join type of our own.

**Corpus queries:** none yet. pbench's `in-is-null`
(`SELECT f_id FROM fact WHERE (f_k IN (SELECT s_y FROM sub)) IS NULL`) was written for this ticket
and does not reach it: DataFusion 45 folds the whole query to an `EmptyExec`
([#257](df-upgrade.md#t257)), so our planner refuses `plan node EmptyExec` on
[#155](#t155) and the `IN` never reaches a join. The row carries `155 250 257`, and this ticket
cannot be demonstrated until #257 is fixed.


<a id="t256"></a>
### #256 — a scan whose row groups all prune is refused as an invalid plan
`scan_mapping::partition` returns `PlanError::Invalid` when the survivor list is empty
(`partition.rs:27`): "no surviving row groups: what an empty scan means is the caller's decision,
not an empty map". Nothing ever made that decision, so the caller gets a refusal where SQL has an
answer — zero rows.

Two ways in, and the second is the common one. A genuinely empty table: pbench's `empty.parquet`
holds `tiny`'s schema and no rows, so DuckDB writes a file with zero row groups and
`SELECT e.t_id, t.t_id FROM empty e, tiny t` is refused at plan time. And a filter that prunes
every row group: `WHERE f_id > 1000000` over any parquet table reaches the same line, which is a
selective query over real data rather than a corner.

DataFusion plans both: `CrossJoinExec` over a `ParquetExec` of the empty file, answering zero rows.
What the mapping cannot express is "no partitions", because the wire reads an empty map as one
unmapped partition — so the fix is a representation for an empty scan (a lane with an empty batch
list, or a `GpuEmpty` the way [#155](#t155)'s `EmptyExec` arm will need one), not a laxer check.

Not [#208](#t208), which is this shape at run time: the cpu's cross join emitting nothing where the
device emits a zero-row batch. #208 cannot be reached from the corpus until this closes, because
the plan is refused before either backend sees it.

**Corpus queries:** pbench's `cross-empty-build`, its plan cells disabled on this ticket. The row
carries `208` as well, the ticket it is written for and will show once this lifts.
