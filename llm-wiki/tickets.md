# peacockdb tickets

Migrated from GitHub issues on 2026-07-31; GitHub issues are closed and this file is the
registry. The number is the permanent ticket ID — each ticket carries an `<a id="tNN">`
anchor that the cost widget links to. Device labels are `tp<N>-<tier>` (micro=100MiB,
mini=2GiB, standard=12GiB).

A ticket carries a **Priority** line only when it is not medium; medium is the default.
New tickets take the next free number (currently 227), which is also the counter for
`tasks/active-tickets.md` — the rollout's own list, separate file, one ID space. Finished and lapsed tickets move to
`llm-wiki/archive/archived-tickets.md` (Done / Stale) — numbers are never reused, so an old
reference still resolves there.

## Contents

| Section | Open | Tickets |
|---|--:|---|
| [Critical correctness](#critical-correctness) | 28 | #225 #224 #223 #222 #221 #219 #218 #217 #216 #215 #214 #211 #210 #208 #207 #205 #204 #202 #200 #199 #166 #153 #80 #59 #60 #121 #122 #118 |
| [Blockers for disabled coverage](#blockers-for-disabled-coverage) | 16 | #212 #206 #203 #169 #168 #158 #173 #23 #65 #62 #95 #57 #45 #63 #56 #55 |
| [Performance / architecture](#performance--architecture) | 28 | #226 #179 #177 #170 #155 #154 #152 #150 #149 #148 #19 #16 #20 #71 #101 #73 #75 #136 #137 #138 #139 #140 #141 #147 #146 #145 #144 #142 |
| [Infrastructure / process](#infrastructure--process) | 21 | #213 #201 #197 #196 #195 #178 #176 #174 #167 #164 #159 #160 #161 #162 #134 #129 #128 #125 #13 #94 #69 |

## Critical correctness

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

<a id="t211"></a>
### #211 — a typed null argument to substr or round is read as 0 on the device

`build_column_scalar_fn` (`expr.cpp`) reads a literal argument's `int_val()` for `substr`'s
start and `round`'s places without asking `is_null`, so `substr(s, NULL)` runs as `substr(s, 0)`
and `round(x, NULL)` as `round(x, 0)` where SQL answers NULL. The literal arms of `build_expr`
and `build_scalar` were made to share one reader of the flag by `tasks/typed-nulls.md`; these
two positional reads are outside them and were found on the way. Reachability through the
planner is unconfirmed — DataFusion may fold a null argument before serialization — so this has
no `bug_` pin yet; the C++ side is unguarded whatever the planner does. `date_part`'s field
argument refuses an empty string, so it is a refusal there, not a wrong answer.

<a id="t210"></a>
### #210 — a bare decimal literal on the AST path comes back as a Float64 column

cuDF's AST has no fixed-point literal, so `ast_scalar` (`expr.cpp`) rewrites a `Decimal128`
literal as a scaled double before the one scalar builder; under a `CAST(… AS Float64)` that is
the type the plan asked for, but a bare or unary-wrapped decimal literal is AST-able too, and
`SELECT 1.5 FROM t` then computes a `FLOAT64` column on the device where the plan declares
`Decimal128(2, 1)` — and since `decimal-precision-at-export` the export refuses it by name rather
than answering it, the AST path still computing a double. Same class as
[#191](tickets/corpus-coverage.md#t191): a declared type produced as another. Pre-existing, carried
through `typed-nulls.md` by that spec's own instruction, and pinned by
`bug_a_bare_decimal_literal_is_a_float64_column_on_the_device` (`gpu_tests/exec_cases.rs`);
the walk `Literals.EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot` names it on its
`Decimal128` row. The likely fix is `is_ast_able` refusing a bare decimal literal as it already
refuses a decimal operand, so the column path builds a real `fixed_point_scalar`.

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

<a id="t202"></a>
### #202 — a descending sort key puts its nulls on the wrong end on the device

On a descending key the device places nulls at the end the plan did not declare: `i32 DESC
NULLS LAST` comes back nulls first, and `DESC NULLS FIRST` comes back nulls last.

`sort.cpp` and the merge in `node_session.cpp` map `nulls_first` to `cudf::null_order::BEFORE`
and its absence to `AFTER`, and cuDF applies that before it flips a `DESCENDING` key. Ascending
keys are right, which is why every corpus ORDER BY has agreed: no cell's descending key carries
a null. DataFusion's default for `DESC` is nulls first, so a query sorting a nullable column
descending gets its null rows first on the cpu and last on the device. The mapping has to be
relative to the direction — `BEFORE` when `nulls_first == asc` — at both sites, since a merge
over sorted runs must order as the sort did, and the two sites have to move together: runs
sorted as the plan says under a merge that reads them the other way break cuDF's merge
precondition, and the device answers duplicated and dropped rows — a shape no plan reaches
today, since every run the merge sees was sorted by the same mapping. Pinned by `bug_a_descending_key_with_nulls_last_puts_them_first_on_the_device`
(`gpu_tests/exec_cases.rs`), the two `…_descending_key_nulls_first_puts_them_last…` merge pins
and `…_over_runs_each_carrying_a_null_duplicates_and_drops_rows…` (`gpu_tests/accumulate_cases.rs`).


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

<a id="t203"></a>
### #203 — the device cannot cast a number to text

`CAST(key AS VARCHAR)` in a select list answers on the cpu and is refused on the device:
"cast to STRING from a non-string type not supported in column path".

`build_column`'s cast arm (`expr.cpp`) refuses every cast whose target is `STRING` unless the
input is already a string. `cudf::cast` has no string target, so the arm needs
`cudf::strings::from_integers`, `from_floats`, `from_booleans` and the datetime converters,
chosen by the input's type.
Neighbour of #45, where a join key's cast to string is the same refusal on the join path; a fix
here answers a projection and does not by itself answer #45, whose fix hashes rather than casts.
Pinned by `bug_a_cast_to_text_is_refused_on_the_device` and
`bug_a_date_cast_to_text_is_refused_on_the_device` (`gpu_tests/exec_cases.rs`).

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
the row count, or a different CASE lowering (`cpp/src/expr.cpp`). 2026-09-17, the first run
since the cpu stopped refusing q9: the same site now fails as `copy.cu:367: Both inputs must be
of the same type` at `tp1-single`, so the two branches reach `copy_if_else` as different cuDF
types; not re-diagnosed.

<a id="t56"></a>
### #56 — q2: CASE-over-string-equality inside a partial-phase sum
Partial GpuAggregate builds an AST for `sum(CASE WHEN <string equality> …)` → cuDF
binaryop "Unsupported operator" (string comparand in the aggregate AST path). Support it
or lower it before the aggregate (`cpp/src/operators/aggregate.cpp`). 2026-09-16: the operator
harness runs that shape green on the device (`a_grouped_sum_of_a_case_over_a_string_equality_agrees`,
`gpu_tests/aggregate_dimension_cases.rs`), so re-check q2 before fixing anything here.

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

## Infrastructure / process

<a id="t213"></a>
### #213 — a golden regeneration can publish a file missing another writer's section
`corpus_golden::merge_section` locks the inode it opened and `publish` renames a staged sibling
onto the path, so a writer that opened before another's rename holds a lock on the old inode,
reads stale text, and publishes without the other's section.

Seen once on `ENS-empty-build`: a whole-corpus `UPDATE_CANONICAL=1` run published
`tp4-single-mini.cpu.txt` without q16's section while its `.cost.txt` kept it; refilled with
`PCK_UPDATE_SECTIONS=1 … --exact cpu_tpch_q16_tp4_single`. The doc on `merge_section` says the
read inside the critical section prevents exactly this, which holds only while the path keeps
one inode. A fix is a lock on a sibling lock file rather than on the file the rename replaces,
or an open-after-lock. Test infrastructure, not the engine: no cell, no golden's content is
wrong once the regeneration is re-read, which is why the rule to read a regeneration's diff exists.

<a id="t201"></a>
### #201 — the murmur gate proves a copy of the lane rule, not the rule
`executor/cpu_backend/gpu_tests/murmur_conformance.rs` re-derives the lane rule (seed-42 pre-fill,
comet murmur3, `pmod`) in its own `cpu_partition_ids`, so only that copy is held against the device.

The production copy is `rows_per_lane` in `executor/cpu_backend/spark_partitioning.rs`, the one
the CPU backend's repartition actually runs. A drift there — a seed, a `%` for `pmod`, a key
cast — leaves the gate green while every CPU lane assignment moves off the device's and the
goldens'. The fix is the gate calling `rows_per_lane` over the same columns and comparing lane
by lane, and the local helper going; not done in the visibility task that found it, since a
test whose subject changes is not a demotion.

<a id="t197"></a>
### #197 — the repartition arm still concatenates a child it can only be handed one of
`node_session.cpp`'s Hash-repartition arm concatenates `child[0]`'s handles before scattering,
and the planner puts a `GpuCoalesceAllBatches` above the merge feeding an emit, so it gets one.

The comment there said to retire the branch when the legacy modes retired. They have, so the
condition is met and nothing left in the tree can hand this arm two handles — the concat is a
copy of a single table on every call. Removing it needs a device run to prove, which is why it
is a ticket rather than part of the rename that found it.

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

<a id="t174"></a>
### #174 — two clamps for one rule, and nothing compares them
`RowRange::clamp` (executor.rs:139) and C++ `clamp_row_range` (node_session.cpp:498) implement the
same row-range rule for the two backends, and no test reads both.

Its own doc says the risk: the two answering differently "would be a divergence no test of either
one alone could see". They are not even comparable as written — one returns `(offset, length)`,
the other `(begin, end)` — so the four Rust cases and the ten C++ cases each prove one side.
`src/tests/executor_cases.rs` is this repo's answer to that shape: one table of inputs and expected answers
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
wrong-order subtree before the root. 2026-09-17: chain B's `device-schema-harness` and
`driver-output-hook` are that check for the operator and corpus tiers — every device batch held
to its node's names and `{type_id, scale}`; the two C++ items above stand.

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

<a id="t134"></a>
### #134 — begin_plan's out_node_count is unused on the prod path
The C++ reports how many fb nodes it indexed; `RecipePlan::wire_nodes()` is what the writer
created. Comparing them is the one free check that both sides number one tree, which every
handle's seq rests on — and no library code reads it: `src/` never calls `begin_plan`, so
outside the tests the number is returned and dropped. Three of the four test openers compare
(`corpus_gpu.rs`, `wire::gpu_tests`, `gpu_backend::gpu_tests`); `gpu_backend::gpu_tests::abi` does not. Fix:
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

<a id="t125"></a>
### #125 — `elsewhere` parameter in assert_registry_matches_csv is dead
`peacockdb-core/tests/common/registry.rs`: after the by-mode file split every CSV column
is owned wholly by one binary, so all four callers pass `&[]` and the ~20 lines of
staleness checking can no longer fire. Reviewer's read is *delete* (coding-style.md: no
fallbacks the task didn't ask for); the parameter was kept only to hold branch scope, and
its doc paragraph was rewritten to stop justifying it with a now-false example. Re-add it
in the commit that actually splits a column across binaries again.
