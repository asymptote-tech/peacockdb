# peacockdb build & test

Code and tests are authoritative; this page maps them.

## Test categories

**Grand total: 2325 test cases — Rust 1850, C++ 94, Python 381.** The Python figure includes the 93 corpus queries, which only a manual dispatch runs. The header is the sum of the N columns of the two tables below, and the rows count cases: a target's own `--list` total is larger, because its registry test is counted once in Registry ↔ CSV rather than again in each tier it belongs to. Comparing a row against a target total is how this page gets mistakenly reported as drifting.

**Runs** — `dataset-matrix` = pipeline.yml's job with the generated dataset and the cuDF
matrix, both legs unless a step says one · `cost-report` = the cost-report job · `shad-gpu` =
CI GPU job on the remote host, `--test-threads=1` · `manual` = no CI step runs it · `2gpu` =
manual, host with ≥2 GPUs (verda-gpu) · `validate-large` = validate-large.yml manual dispatch.
The first table says it once per block: cpu and ffi are dataset-matrix, gpu is shad-gpu. Everything
except the `shad-gpu` and `2gpu` rows also runs locally; large CPU batches go to verda.

### Rust tests against the engine

Three blocks, one per build rung; a block's header sums its CI lines, one per binary. A test module declares
the lowest rung it needs and is named for it — `tests`, `ffi_tests`, `gpu_tests` — so the block a
row sits in is read off the module's declaration, not off what the test does. Inside a block, rows
are grouped by tier: crate integration external (a `--test` binary), crate integration internal
(`src/tests/`), component (`<component>/tests/`), subcomponent (`<component>/<sub>/tests/`), module
unit (`foo.rs` beside `foo/tests.rs`).

#### cpu — `--features rust-only`: no FFI, no device. 1179 cases: `--lib` 600, `test_cpu_corpus` 550, `test_corpus_goldens` 26, `test_cost_model` 3

*crate integration, external*

| Corpus, cpu | [test_cpu_corpus](../peacockdb-core/tests/test_cpu_corpus.rs) | 549 |
|---|---|--:|

one `corpus_query!` line per query declaring its cpu and gpu modes, its two oracles and
whether its device run is schema-validated, expanded to a case per (query, mode): planned, run on `CpuBackend`, validated, and the answer
checked against plain DataFusion at `target_partitions = 1`. 115 queries at the modes each is
correct at — `tpcds/q96`, `tpcds/q88` and `tpcds/q90` carry three disabled by
[#180](tasks/active-tickets.md#t180), `tpcds/q77` three by [#212](tickets.md#t212),
`tpcds/q80`, `tpcds/q18`, `tpcds/q22`, `tpcds/q5` and `tpch/rollup-over-join` three by
[#189](tasks/active-tickets.md#t189), `tpch/scan-limit` two by
[#186](tasks/active-tickets.md#t186), and five queries are out entirely: `tpch/q11`,
`tpch/q22` and `tpcds/q24` on
[#190](tasks/active-tickets.md#t190), `tpcds/q54` and `tpcds/q64`. 546 cells, plus three checks
that every declaration's two oracles suit each other and every device cell has a cpu cell

| Registry ↔ CSV, cpu | [the_registry_matches_the_cpu_corpus_in_both_directions](../peacockdb-core/tests/test_cpu_corpus.rs) | 1 |
|---|---|--:|

the `cost-registry.csv` cpu column matches the cases the corpus expands to, both directions;
one binary per engine, since `inventory` collects per linked binary

| Corpus goldens, self-consistency | [test_corpus_goldens](../peacockdb-core/tests/test_corpus_goldens.rs), [benchmark](../peacockdb-core/tests/test_corpus_goldens/benchmark.rs) | 26 |
|---|---|--:|

the committed sections against their own arithmetic, with no dataset and no run: `consumed +
abandoned == emitted` at every node, lane counts against `lanes=N`, the loader's `batch_rows` a
prefix of its own lane's `partition_groups`, the root's `out_rows` against `.result.txt`, and
the tree the indentation draws. The golden is written by the run it will later check, so a file
that contradicts itself is the only witness to a renderer that is wrong. The benchmark tree and
the record get the same reading: every `total_us` is the sum of the `time_us` beside it, every
committed tree reports a release build, every timed (query, mode) is enabled on a device in
`corpus_cases.inc`, a row that lost a cell is refused, a bare `calls` row names the seq it was
handed, and the record's preamble is what `record_header()` writes

| Cost-model goldens | [cost_goldens_match_and_total_is_byte_identical](../peacockdb-core/tests/test_cost_model.rs) | 3 |
|---|---|--:|

`.cost.txt` derivation from `.cpu.txt` × `cost_model.conf`

*crate integration, internal*

| End to end | [tests::end_to_end](../peacockdb-core/src/tests/end_to_end.rs), with `limits`, `dimensions`, `accounting` and `schema_validation` beneath it | 29 |
|---|---|--:|

SQL in, rows out: 17 queries planned and run at all five modes against DataFusion on the same
SQL, eleven of them also at injected layouts no planner would emit, plus `in_flight_bytes` back
to zero and holds equal releases at the end of every run — and ten cases no query list can
carry: that DataFusion's partial aggregate does not skip grouping here, the call and pull
counts a limit makes, the smallest budget a query fits in completing where the byte below it
trips, and that boundary under a drained lane, the model compared against what the calls
measured, an answer under the wrong column names not being the same answer, the injected set
keeping the shapes only one query has, and a degenerate hash under a Right outer and under a
RightAnti answering like the oracle from the empty build lanes it leaves
([#175](archive/archived-tickets.md#t175)), and the schema validator as the driver's output
hook — `tpch/q6` at every mode passing under it, and an index over the same tree with one
project's field retyped refused naming the field. Two of the 29 are `#[ignore]`d against
[#182](tasks/active-tickets.md#t182) — the budget boundary and the rebatcher's peak, both
properties that pricing a batch from the plan's schema took away — so 27 run. The first tier
where the planner, the recipes, the executors and both drivers run together rather than each
against a fixture of the last one's shape — so what it tests is the joins between them

| Harness helpers | [tests::compare](../peacockdb-core/src/tests/compare.rs), [tests::synthetic](../peacockdb-core/src/tests/synthetic.rs) | 16 |
|---|---|--:|

the synthetic batch and the comparator against themselves, with no engine: `synthetic(rows,
seed)` deterministic from its seed, null in every column but the id, dyadic in every float, legal at zero
rows; `assert_same` red on a differing value, a differing type, a differing row count, and a
slot one side left empty against a zero-row batch on the other — the failures the operator
harness rests on, each shown for the reason it names; and `same_within_welford`, the one
inexact comparison, red on a renamed and on a retyped column before it reads a value

*component*

| Plan rules, hand-built | [a_limit_over_several_lanes_names_the_node_that_fixes_it](../peacockdb-core/src/plan/tests/mod.rs) | 31 |
|---|---|--:|

one input per rule, each built to break the rule it is aimed at: a plan that violates one is
unreachable from SQL because translation inserts the fix, so a hand-built input is the only
thing that shows the guard going red

| Join declarations | [a_join_whose_sides_carry_different_lane_counts_is_refused](../peacockdb-core/src/plan/tests/joins.rs) | 23 |
|---|---|--:|

what each of the three join kinds requires of its inputs, and the distribution a join declares
about its own output — the claim nothing downstream re-checks

| Aggregate state columns | [plan::tests::aggregate](../peacockdb-core/src/plan/tests/aggregate.rs) | 5 |
|---|---|--:|

which aggregator owns which state column and what the finalize project emits — read twice, by
the recipe writer and by the CPU backend, so one wrong answer here is wrong on both engines

| Layout injection mechanism | [plan::tests::layout_injection](../peacockdb-core/src/plan/tests/layout_injection.rs) | 4 |
|---|---|--:|

the rewrite against itself, with no dataset: a plan rebuilt from its own children is identical
in debug — the renderer prints neither a loader's survivors nor `can_be_null`, so the fields a
corpus plan never varies are the ones it cannot check — every field name a node's debug prints
takes two distinct values across the fixtures, and the selector's output is a cover rather than
a prefix

| Planner join capability | [planner::tests::join_capability](../peacockdb-core/src/planner/tests/join_capability.rs) | 14 |
|---|---|--:|

every hash join type crossed with a residual filter, the co-partitioning and lane rules, the
null analysis both ways, and the session config's own registration path declaring strings
`Utf8`; writes its own parquet, so no dataset

| Null analysis rules | [a_scalar_function_can_be_null_even_over_operands_that_cannot](../peacockdb-core/src/planner/tests/null_analysis.rs) | 8 |
|---|---|--:|

every rule in the can-this-column-be-NULL pass, on hand-built nodes — a source declares a
not-nullable column here, which no corpus fixture can

| Planner join refusals | [planner::tests::join_refusals](../peacockdb-core/src/planner/tests/join_refusals.rs) | 10 |
|---|---|--:|

every shape the planner refuses, from the SQL that provokes it; each asserts its ticket is in
the message a user sees

| Plan goldens, tp1-single | [tpch_tp1_single](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 2 |
|---|---|--:|

1 lane, one batch per chunk — the mode every other is read against; plan tree + `--- recipes
---` + `--- memory ---` per query, one file per bench

| Plan goldens, tp1-rowgroup | [tpch_tp1_rowgroup](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 2 |
|---|---|--:|

1 lane, one batch per row group: the finest the mapping expresses, and no budget

| Plan goldens, tp4-single | [tpch_tp4_single](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 2 |
|---|---|--:|

4 lanes, one batch per chunk — the shuffle shapes with batching inert

| Plan goldens, tp4-rowgroup | [tpch_tp4_rowgroup](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 2 |
|---|---|--:|

4 lanes at row-group granularity: lanes and many batches at once

| Plan goldens, tp4-sized | [tpch_tp4_sized](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 2 |
|---|---|--:|

4 lanes, the estimator's target — **the only mode a budget tier moves**, recorded in-band

| Plan goldens, meta | [the_registry_matches_the_goldens_in_both_directions](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 4 |
|---|---|--:|

the registry and the goldens agree both ways; every mode has a golden and every golden a mode;
every refusal in a golden names a ticket that exists and carries no host path

| Recipe plan structure | [every_published_seq_addresses_the_kind_its_recipe_claims](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 4 |
|---|---|--:|

the claims the payload golden cannot make: every published seq resolves to the kind its recipe
names, over every corpus query rather than only the payload subset; the payload set covers
every fb kind and call shape the ten goldens hold; the queries that cannot cross the wire are
declared; no plan approaches the verifier's depth cap ([#169](tickets.md#t169)); and the
index's post-order agrees with the numbering `attach_recipes` gave, over the corpus, since the
two are separate walks in separate files and [#134](tickets.md#t134) is the same pair one
boundary over

| Recipe payloads golden | [the_payload_golden_carries_what_each_call_hands_the_executor](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 1 |
|---|---|--:|

the payload subset at tp4-rowgroup — chosen as a cover over every fb kind and call shape the
mode goldens hold, and asserted to be one, so the membership grows when the mapping does — with
every payload rendered and a sha256 over the bytes beside it

| Recipes per join type | [an_outer_join_that_preserves_its_build_side_keeps_the_keys_and_finishes_with_an_anti_join](../peacockdb-core/src/wire/tests.rs) | 23 |
|---|---|--:|

the kinds whose recipe is more than one call, `GpuHashJoin` first: per join type, the seq set
it emits and when each call is made, against the capability matrix, and whether the CPU
executor makes the finish pass the recipe's `AtDone` says it does; plus a leaf outside the
registry emitting the writer's stub and no recipe, which is how a hand-built node gets its
recipe with no plan. The trivial kinds are not here — the plan goldens run them over every
corpus query

| Plan text | [every_column_reference_renders_name_at_ordinal](../peacockdb-core/src/plan_text/tests.rs) | 13 |
|---|---|--:|

the renderer against what the planner emits from real SQL: every column reference prints name
at ordinal, every node carrying a fetch prints it, a join prints its keys and projection by
name, a source prints its mapping verbatim, a name that is not a token is backquoted, and no
node name carries an exec suffix

| One driver, two backends | [one_generic_driver_serves_two_backends_with_different_batch_types](../peacockdb-core/src/executor/tests.rs) | 1 |
|---|---|--:|

one source step written once over `Backend` and driven against two backends with different
batch types, so the trait's associated types are exercised the way both engines use them

*subcomponent*

| Drivers over a mock backend | [executor::driver::tests](../peacockdb-core/src/executor/driver/tests/mod.rs) | 109 |
|---|---|--:|

flow, backpressure, limits and accounting, asserted on calls rather than rows — pull counts,
queue bounds, batch release, the trace, the output hook (called once per batch a node queues
as its own and not at a forwarder, a refusal failing the run at that node and lane with the
hook's words, and `None` leaving the report as `run` makes it): the schedule and the two holds, both limit lowerings by
the calls not made, what each node emitted and consumed (the two records the corpus goldens
read), the accountant through the drivers, a backend failure stopping the query with the
accounting still reconciling, the execution golden's text with every number chosen by the
script, the empty build lane a join owes rows for reaching `SetBuild` while every other empty
lane still drops ([#175](archive/archived-tickets.md#t175)), and the mock against its own
script; then the measurement side: the ABI-call record indexed by the lanes a node is driven on,
holding the backend's calls and not the driver's own, a backend that names no seq leaving every
entry unmeasured, the timing tree rendered with what each node cost, a call without a region and
a region without a call each refused, a region the clock rounded to nothing still counted as
ran, and the recorded node index the post-order rather than the walk order

| CPU backend executors | [executor::cpu_backend::tests](../peacockdb-core/src/executor/cpu_backend/tests/mod.rs) | 68 |
|---|---|--:|

one hand-built node per executor, one hand-written expected result: the exec executors, the
accumulators over state batches written down rather than produced, the loader over parquet the
test writes — which relabels the reader's batch to the declared schema and refuses a column
the file holds in another type rather than casting it — the scatter, the join capability
matrix run per mode, what `executors_for` builds and reports holding, every
decomposition's init emitting exactly the state `PlanAgg::state_type` declares, an init
declaring a narrower decimal than it produces refused, and a merge over a decimal sum
DataFusion widens still building

| Executor contract, both engines | [executor::cpu_backend::tests::contract](../peacockdb-core/src/executor/cpu_backend/tests/contract.rs) | 1 |
|---|---|--:|

one table of input, calls and expected answer
([`executor_cases.rs`](../peacockdb-core/src/tests/executor_cases.rs)) run by the CPU backend
here and by the device in `gpu_backend::gpu_tests::contract`, because a table one side does not
read proves that side twice. Eleven rows: filter, project, the lane sorted with and without a
fetch, coalesce, a merge with and without its finalize, a merge over state whose keys carry a
grouping id, and the scatter at 4 lanes and at 64 — the lane each key lands in is a golden,
since co-partitioning is what every partitioned join rests on

| Translator, one rule at a time | [planner::translator::tests](../peacockdb-core/src/planner/translator/tests.rs) | 29 |
|---|---|--:|

one test per node kind, per expression kind and per planner rule, each from the smallest plan
that shows it — the corpus goldens are a regression net over whole plans and a different
question from whether a rule is right

| Schema annotations | [planner::translator::schema_tests](../peacockdb-core/src/planner/translator/schema_tests.rs) | 9 |
|---|---|--:|

what every node declares about its columns: the arrow types, and the annotations a merging or
finalizing node reads

*module unit*

| Driver internals | [executor::driver::scheduler::tests](../peacockdb-core/src/executor/driver/scheduler/tests.rs) | 44 |
|---|---|--:|

the accountant's formula, cache and two checks on plain figures; the plan index's numbering,
per-lane slots and which lanes feed a build side that owes rows; the scheduler's corners
enumerated and then a differential test against a naive rescan on randomized shapes; the lane
state machine one call at a time with no tree around it

| Aggregate state types | [plan::aggregates::tests](../peacockdb-core/src/plan/aggregates/tests.rs) | 9 |
|---|---|--:|

`state_type`'s table against the accumulators it was read off: each arm's DataFusion
accumulator over four rows produces the type the table declares, `Sum`'s decimal rule and its
refusal of a string, the walk over every decomposition that keeps `MergeM2` out of every state
slice, and the `avg` finalize's shape — the sum divided at its own scale, the quotient cast to
the declared output

| Declared precisions | [common::tests](../peacockdb-core/src/common/tests.rs) | 1 |
|---|---|--:|

what the export is told per column: a `Decimal128(p, _)` its `p`, every other column `0`

| Record steps | [test_support::record::tests](../peacockdb-core/src/test_support/record/tests.rs) | 2 |
|---|---|--:|

the record's reading of a plan: a row's `node_seq` names the steps its `recipes` line prints,
so the record and the plan golden agree on what a call is; and every cell of a written row
sits under the column named for it, since `row()` is positional and the heading is not

| Sink divergence message | [executor::errors::tests](../peacockdb-core/src/executor/errors/tests.rs) | 3 |
|---|---|--:|

what the sink says when the device's schema is not the declared one: every diverging column
named with its index and both types, so a year and a narrow decimal read as two findings
rather than `try_new`'s first; nullability is never a clause, since `try_new` does not check it

| Device schema projection | [test_support::device_schema::tests](../peacockdb-core/src/test_support/device_schema/tests.rs) | 16 |
|---|---|--:|

an arrow schema projected onto what cuDF stores — a `type_id` and a decimal's scale, cuDF's
own interop table for the types the wire admits, a type outside it a panic by name — and the
comparator over it: every diverging column in the sink's spelling, a renamed one and a width
mismatch each a finding, precision and nullability never one; the schema-only IPC stream
`peacock_handle_schema` answers, decoded with no device

| Forwarders and row ranges | [interleave_serves_lane_p_from_lane_p_of_every_child](../peacockdb-core/src/executor/forwarder/tests.rs) | 5 |
|---|---|--:|

which lanes a merge, a union and an interleave serve from which child; a range clamped to what
is there

| Expression round trip | [executor::cpu_backend::expr_physical::tests](../peacockdb-core/src/executor/cpu_backend/expr_physical/tests.rs) | 13 |
|---|---|--:|

DataFusion's expression, this engine's, and DataFusion's again must all read the same column
out of the same rows — asserted on the array each produces, not on shape

| Plan types | [plan::validate::tests](../peacockdb-core/src/plan/validate/tests.rs) | 38 |
|---|---|--:|

validation's shape rules — a plan ends in a crossing, a sink sits at the root, a limit has a
real consumer, no schema — an aggregate's intermediate included — literal, cast or function
names a view type the device cannot hold;
layout canonical forms and equality; whether a hash survives a regrouping

| Memory estimation | [planner::memory_estimation::tests](../peacockdb-core/src/planner/memory_estimation/tests.rs) | 11 |
|---|---|--:|

the model's rules from SQL over the minimal dataset: an accumulator ends the walk and what it
holds comes off the budget first, a batch is charged once per lane in force, a loader is priced
by the batches its mapping makes, amplification is the widest point on the path, a target lands
on the coarse grid, and constants that cannot be less and exceed the budget are a plan-time
error

| Translator expressions and scan mapping | [planner::translator::expr::tests](../peacockdb-core/src/planner/translator/expr/tests.rs), [scan_mapping::partition::tests](../peacockdb-core/src/planner/translator/scan_mapping/partition/tests.rs) | 27 |
|---|---|--:|

a column keeps its ordinal and name, a literal DataFusion's scalar, a binary op its declared
type, the four unaries one for one; from parquet metadata, rows and bytes are the surviving row
groups' own totals over the projected columns, a scan over several files is refused, and the
lanes deal the survivors as the mapping says

| Expression text | [plan_text::expr_text::tests](../peacockdb-core/src/plan_text/expr_text/tests.rs) | 3 |
|---|---|--:|

an interval prints the parts that are not zero, a decimal as a value, every form readably

| Expression writer | [wire::expr_writer::tests](../peacockdb-core/src/wire/expr_writer/tests.rs) | 16 |
|---|---|--:|

every variant, every operator, and the literals the corpus actually produces — a wrong scalar
is invisible in plan text and wrong on a device

#### ffi — default features: FFI linked, no device. 7 cases: `--lib -- ffi_tests::` 4, `peacockdb-ffi --test test_ffi` 3

*crate integration, external*

| FFI smoke | [test_executor_lifecycle](../peacockdb-ffi/tests/test_ffi.rs) | 3 |
|---|---|--:|

the crate links; executor lifecycle; `install_rmm_pool` refuses a zero request

*component*

| GpuBatch surface | [executor::ffi_tests](../peacockdb-core/src/executor/ffi_tests/mod.rs) | 4 |
|---|---|--:|

what the batch reports, that `consume` hands the handle over without releasing it, and that an
NVTX range name with an interior NUL is refused before the C side sees it. Needs no device: the
release is null-guarded on the executor

#### gpu — `--features gpu`: shad-gpu only. 575 cases: `--lib -- gpu_tests::` 535, `test_gpu_corpus` 28, `peacock_gpu_benchmarks` 11, `test_node_timing` 1

*crate integration, external*

| Corpus, device | [test_gpu_corpus](../peacockdb-core/tests/test_gpu_corpus.rs) | 27 |
|---|---|--:|

the same `corpus_query!` lines read from the other side: each enabled (query, mode) runs on a
device with every batch held to its node's declared schema through the driver's output hook
(the line's `schema_validation_enabled`; `tpch/shuffle-stddev` says `disabled` against
[#225](tickets.md#t225), its Welford state columns named for the alias), and asserts,
read-only, against the section the cpu authored — plan shape, `in_rows`, the per-batch lists
and the bytes — plus the result where `gpu_oracle` names a golden.
Twenty-six cells today: `tpch/q6`, `tpch/q1` and `tpch/shuffle-additive-avg` at every mode,
and `q17`, `q19`, `nested-loop-join`, `shuffle-stddev`, `tpcds/q84`, `tpch/aggregate-groupby`,
`tpch/filter-project`, `tpch/shuffle-additive`, `tpcds/q37`, `tpcds/q82` and `tpcds/q85` at
`tp1-single`; the rest are off against [#152](tickets.md#t152),
[#184](tasks/active-tickets.md#t184), [#185](tasks/active-tickets.md#t185),
[#191](tickets/corpus-coverage.md#t191), [#220](tasks/active-tickets.md#t220) and the device's
own tickets (#57, #63, #205). The twenty-seventh case is that a device run under a
regeneration writes no golden

| Registry ↔ CSV, device | [the_registry_matches_the_gpu_corpus_in_both_directions](../peacockdb-core/tests/test_gpu_corpus.rs) | 1 |
|---|---|--:|

the gpu column of the registry, the other half of the pair

| Corpus benchmark harness | [peacock_gpu_benchmarks](../peacockdb-core/tests/peacock_gpu_benchmarks.rs) | 11 |
|---|---|--:|

the eight assertions the harness makes about its own output, run on every `gpu-tests` job
under `--skip bench_`: a mode's results go to one file per (dataset, mode), sections ordered
numerically, a filtered run keeping the sections it did not produce, every declared mode naming
the queries its file will hold, the record written only when `PEACOCK_RECORD_PATH` names it and
checked against what the plan declares before a row lands, an append under a different `# run:`
heading refused, and `PEACOCK_BENCHMARK_CAPTURE` naming neither pass refused. The three `bench_` cases — the
case list in `tests/common/corpus_benchmark_cases.inc` — are the sf40 measurement itself and
run only under `--run-benchmarks` (*Corpus benchmarks*, below)

| GPU timing method | [events_are_free_and_land_where_they_claim](../peacockdb-core/tests/test_node_timing.rs) | 1 |
|---|---|--:|

the instrument's structural test, one sf1 query at one mode: `Off` records nothing, `Events`
records one region per recorded call and partition, Σ `device_us` is above zero and at most
the wall, and both modes make the same calls. No percentage ceiling on the instrument's cost —
a bound loose enough never to flake on a shared host proves nothing — so the effect is reported
from the sf40 run instead

*crate integration, internal*

| Operator harness | [an_unload_hands_the_whole_batch_over_on_both_backends](../peacockdb-core/src/tests/gpu_tests/harness_cases.rs), [bug_a_descending_key_with_nulls_last_puts_them_first_on_the_device](../peacockdb-core/src/tests/gpu_tests/exec_cases.rs), [every_kind_has_a_case_or_is_a_forwarder](../peacockdb-core/src/tests/gpu_tests/coverage.rs) | 334 |
|---|---|--:|

one hand-built node over stub leaves, a script of synthetic batches, both backends through
`executors_for`, the outputs compared slot by slot and exactly — schema included, so a type the
device changes is red here rather than at a query's root. The upload round trip, `GpuUnload`
— including the sink naming a column whose exported type is not the declared one — and
`GpuLimit` proved the harness on the operators whose recipe carries no seq; then every
seq-bearing operator — the exec three, both aggregates, the three accumulators, the scatter,
nine hash-join types, cross, nested-loop and the scan — each case green or a `bug_` case
asserting the wrong answer or the refusal with its ticket above it, every empty shape its own
case. Then the joins along the dimensions the corpus varies: a crossing projection on every
type the device reaches; composite, `Int64`, `Utf8` and `Date32` keys on the three key paths,
the composite's null second key pinned under #59;
the residual with a projection, under `null_equals_null`, on a string and on a decimal; and
the nested loop's cross-then-mask path. String keys are `Utf8`; the join cases declare no
view type. Then the aggregates,
expressions and sorted merges along the same dimensions. Group keys on dates, `Int64`, strings
and pairs, each carried through the merge; `count(*)` and expression arguments; every merge
arm with rows; the global Welford init and the dispersion finalize; the project expressions,
casts and functions the corpus uses; sorts and merges on descending, nullable and composite
keys, and the merge at four lanes. The group keys are `Utf8` too; the aggregate cases declare no
view type. A guard reads the kind each case declares and names every kind with none, the three
forwarders excluded

| Operator harness, what the device holds | [a_divide_over_a_decimal_declares_the_scale_6_the_device_holds](../peacockdb-core/src/tests/gpu_tests/exec_schema_cases.rs), [bug_stddev_holds_three_identically_named_columns](../peacockdb-core/src/tests/gpu_tests/aggregate_schema_cases.rs) | 134 |
|---|---|--:|

the same nodes on the device alone, every output handle read where it sits through
`peacock_handle_schema` and held to the node's declared schema under cuDF's projection — a
`type_id` and a decimal's scale, never a precision — so a type the device changes is red at
the node rather than at the export, and an intermediate's type is read without moving a row.
One `<family>_schema_cases.rs` beside each family: the reader against the uploader and the
mid-plan limit; a scan of each fixture column type; a filter with and without its projection;
arithmetic on each numeric type, the divide's declared scale, each cast the corpus emits, the
union's two branch casts, each scalar function the dispatch admits, CASE, LIKE and the unary
forms; a sort; the three accumulators; the scatter on each key type; the seven hash-join
types that run past a first probe batch (Left and Full refuse it, #152), with and without a
crossing projection, and the three key-path types on each key type; the cross and nested
loops; and each aggregate's init, the merges, and the sum, avg, stddev and var finalizes,
grouped on each key type and global. Eight are `bug_` pins: the grouping id held `Int32` under a
`UInt8` declaration (#65), `date_part`'s year, month and day held `INT16` under `Int32` (#191),
the cross join's dropped projection (#207), the keyless Welford init's one finished column
(#216), and the Welford state named three times by its alias at the init and the merge (#225)

*component*

| Recipe walk on a device | [an_average_finalizes_to_the_digits_the_oracle_computes](../peacockdb-core/src/wire/gpu_tests/mod.rs), [an_avg_partial_holds_a_string_key_a_scale_2_sum_and_an_int64_count](../peacockdb-core/src/wire/gpu_tests/mod.rs) | 21 |
|---|---|--:|

the recipe plan driven by hand — begin_plan, the calls each recipe names, handles threaded
between them, DataFusion on the same SQL as the oracle. One partition and one batch except the
aggregates, which take two so a merge happens; `avg` asserts digits, since cuDF takes a
divide's scale from its operands where arrow takes it from the declared type. A ROLLUP is here
because its masks and NULL placeholders are the one payload the plan line does not imply, and
one read re-walks every query to check the kinds a device has run against the kinds the file
claims, in both directions. Eleven spot-checks read what the device holds between calls, each
a literal schema written from the plan before any ran — `avg`'s partial, merge and finalize,
`sum`'s partial, the filter, the project, the inner and semi joins' one call, the nested
aggregates' inner finalize and outer `max` — and one `bug_`: the rollup's grouping id held
`Int32` where the plan says `UInt8` (#65)

*subcomponent*

| GPU↔comet murmur3 | [gpu_spark_partition_ids_match_comet_live](../peacockdb-core/src/executor/cpu_backend/gpu_tests/murmur_conformance.rs) | 10 |
|---|---|--:|

the linchpin gate: both sides place every row in the same partition, bit-exact. Three of the
ten need no device — the comet call, `pmod` on a negative hash, the two-column CPU reference —
and ride here anyway, because the module is the gate and splits nowhere

| Executors on a device | [an_aggregate_that_finalizes_runs_both_of_its_calls](../peacockdb-core/src/executor/gpu_backend/gpu_tests/exec.rs) | 32 |
|---|---|--:|

each one handed its node's recipe — the exec nodes one batch at a time (filter, project,
per-batch sort, aggregate with and without its finalize, the export with a row range, and an
accumulator's recipe refused), the accumulators a stream of them (coalesce, the accumulating
sort, the state merge, the Welford merge, the mid-plan limit), and the joins what the matrix says a device
runs: Inner at one probe batch, LeftAnti streamed through its finish pass, the scatter's N
handles, and the refusals — Left and Full outright, a second probe batch, a zero-input collapse
— each naming its ticket; plans hand-built over six rows the test writes itself, since the ABI
loads a table only by reading one. `backend.rs` is the caller `executors_for` otherwise has
none of: six of the seven categories built from a live session's recipes, and a node asked for
at the wrong post-order refused, so the number is an address. The partition accumulator is the
seventh and has no caller; its arm reads the child's lane count. The contract table's device
half is the one case in `contract.rs`; and under `Events`, a slice and an export are charged to
the node that produced the handle

| Per-call ABI | [executor::gpu_backend::gpu_tests::abi](../peacockdb-core/src/executor/gpu_backend/gpu_tests/abi.rs) | 4 |
|---|---|--:|

three of the four per-call symbols on a live GPU — a scan's row groups, an export range, a
slice; `handle_schema` is `test_support`'s schema read — and
the release skipped exactly where a call consumed the handle


### Everything else

The C++ suites, the Python sets, `cost-report`, and the three repo guards — `test_ci_coverage`
reads the workflow yaml, `test_module_layout` reads the source tree, `test_golden_format` tests
the harness's own format reader — none of which runs engine code.

| Category (lang) | Why | Examples | Runs | N |
|---|---|---|---|--:|
| Golden text format (Rust) | the one reader of the format, over strings: the differ must name what moved, what is missing and what is out of order, and a node line must yield its name, depth and fields — every golden assertion in the tree is a comparison through it. The comma traps are cases, since `on=[(a@0, b@1)]` and `Decimal128(38, 15)` each carry one inside brackets | [a_section_that_moved_is_named_with_the_column_that_moved](../peacockdb-core/tests/test_golden_format.rs) | dataset-matrix (25.02 leg) | 26 |
| CI wiring guard (Rust) | every Rust target must be named by a CI step — CI does not glob, the three lists that decide where a GPU target runs must agree, and both GPU runners must pass `--test-threads=1`, which is the whole of the single-tenant invariant inside a process and is what a device test's `unsafe { set_var }` rests on. The rungs the `--test` sweep cannot see get one assertion each: cpu (`--lib` under `rust-only`), ffi (`--lib -- ffi_tests::`), device (the staged lib binary and the loop line handing it `gpu_tests::` — shad-gpu runs prebuilt binaries, so there is no command line), plus the CLI build, which has no test target. The three runners must agree on the lib's staged name and rung too, and the rung must reach the binary through `rung_args`. The reader is scoped to the rust loop: a file-wide search passes on the comment above the command, and the C++ loop above it shares the loop variable and rightly carries no flag; and the benchmark binary runs on CI without its `bench_` cases, `--skip bench_` read off the same loop | [every_rust_test_target_is_named_by_ci](../peacockdb-core/tests/test_ci_coverage.rs), [each_rung_has_its_ci_line_and_the_cli_is_built](../peacockdb-core/tests/test_ci_coverage.rs), [the_three_gpu_target_lists_agree](../peacockdb-core/tests/test_ci_coverage/runners.rs) | cost-report | 9 |
| Module layout rules (Rust) | the component walls, which the compiler mostly cannot check: where a `pub` may appear, that a subcomponent is declared `mod`, that no `super::` chain climbs out of its component, that no public signature names a type from a private module — `private_interfaces` reads a type's nominal visibility, so an unreachable type spelled `pub` passes it silently — and that no `pub` in `test_support/mod.rs` names a type from a component, which is what keeps the harness a facade and not a rename. Sibling reach is the case rustc refuses to have an opinion on at all: no visibility level means "my parent but not my siblings". One case compiles a probe crate against the built library to prove `wire::generated` is unreachable from outside, with a positive control so a probe that fails for the wrong reason cannot pass as proof. Five more rules say where test code may live: a test module is named for the build rung it needs and gated for it, in both directions; its body is a file of its own; a `#[cfg(test)]` sits on nothing but a test-module declaration; and a path compiled only under a test gate says `test` in its name. Bare `pub` is checked against `SURFACE`, the CLI's API by file and name, in both directions, so a dropped-and-added pair cannot pass a count; `pub mod` is pinned to `lib.rs`. One register remains, the items that keep a `#[cfg(test)]` because no test module can hold them, checked in the reverse direction too, so an entry outliving its reason goes red. Needs no dataset and no device | [no_public_signature_names_a_type_from_a_private_module](../peacockdb-core/tests/test_module_layout/privacy.rs) | cost-report | 17 |
| Cost-report renderer (Rust) | glyphs, links and the anchors they resolve to, ratio bucket, regression gate, history | [bucket_threshold_is_1_4](../cost-report/src/main.rs), [regression_count_drives_exit_decision](../cost-report/src/main.rs) | cost-report | 37 |
| DuckDB cost extraction (Python) | classifier / pruning / dynamic-filter logic — fails CI before generation | [scan_count_mismatch_fails_loud](../testdata/test_duckdb_cost.py), [compute_pruning_from_rowgroups](../testdata/test_duckdb_cost.py) | cost-report | 41 |
| Exec-model prototype (Python) | the scheduler over mock traits, plus pandas-backed operators checked against a single-shot oracle at five partitioning configs, both limit lowerings, the scalar expressions pinned to what `expr.cpp` does rather than what pandas defaults to, and every join mode run on two backends — one pandas, one emitting FlatBuffers nodes and interpreting them as the C++ does — no project code | [test_a_join_in_its_build_phase_holds_back_its_probe_subtree](../scripts/exec_model/tests/test_scheduling.py), [test_every_join_type_matches_the_oracle_on_both_backends](../scripts/exec_model/tests/test_join_capability.py) | cost-report | 216 |
| Exec-model prototype, TPC-H plan shapes (Python) | the same drivers over real sf1 tables under a live resident budget, each plan re-run at every layout `LayoutInjector` can produce; needs the generated dataset, so it rides dataset-matrix rather than cost-report | [test_the_accumulator_is_what_makes_the_budget_bind](../scripts/exec_model/tests/test_tpch.py), [test_every_layout_gives_the_same_shuffled_join](../scripts/exec_model/tests/test_tpch.py) | dataset-matrix (25.02 leg) | 19 |
| Exec-model corpus (Python) | every TPC-H query and every TPC-DS query the engine runs that needs no window function, lowered by hand and run over whole sf1 tables at three layouts each — TPC-H against a pandas oracle per query, TPC-DS against DuckDB running the query's own text. Minutes, not seconds, so manual dispatch; `PCK_BACKEND=recipe` re-runs the whole set with every join going through the FlatBuffers emulation | [test_corpus_q21_suppliers_who_kept_orders_waiting](../scripts/exec_model/tests/test_tpch_corpus.py), [plans_tpcds.py](../scripts/exec_model/tests/plans_tpcds.py) | manual — exec-model-corpus.yml, 3 shards | 93 |
| Calibration scripts (Python) | the two capture readers and the plotter over a synthetic two-case Nsight export written by the test: two cases stay two row sets, `hbm_bytes` lands on the right tuple and a missing `--peak-bw` exits non-zero, and a ten-row record draws the six panel directories and `index.html`; no project code, no device | [test_calls.py](../scripts/calibration/tests/test_calls.py), [test_plot.py](../scripts/calibration/tests/test_plot.py) | cost-report | 12 |
| C++ CPU/FFI unit | decimal binop typing, AST routability, lifecycle, the row-range clamp rule, the two refusals of the test-only upload symbol, and the timing switch — its mode round-trips, the ABI refuses a mode it does not name and a null region buffer with a capacity, and a second harness NVTX push replaces the first rather than nesting; no GPU needed | [DecimalScale.BinopOutputType](../cpp/tests/cpu/test_executor.cpp), [AstRouting.IsAstAble](../cpp/tests/cpu/test_executor.cpp) | dataset-matrix (`ctest -L cpu`) + shad-gpu | 15 |
| cuDF GPU smoke (C++) | the GPU is alive; the Spark-murmur3 kernel matches comet in C++; the RMM pool reserves the budget the binary declared | [CudfGpu.SparkPartitionIdsMatchComet2ColWithNulls](../cpp/tests/gpu/test_cudf.cpp), [RmmPool.ReservesTheDeclaredBudget](../cpp/tests/gpu/test_cudf.cpp) | shad-gpu | 4 |
| Plan-executor (C++) | hand-built plan IR through the C++ executor, node by node, plus the per-call entry points at their contract edges (row-group override, export range, slice), the sqrt arm on both evaluators, a merge that emits state rather than a value, the literal arm — a typed null on the AST path, the decimal literal's scaled double, every wire type walked through the dispatch, the LIKE guard — and the timed regions: `Off` records none, every entry point opens one per output partition, a second call of one seq counts up, a slice and an export are charged to the node that produced the handle, a handle from before timing was on and an adopted one each refused a charge, an export of no rows opens one too, and collecting drains; and the NVTX switch on its own: ranges without timing record no region | [PlanExecutor.HashJoinNationRegion](../cpp/tests/gpu/test_plan_executor.cpp), [NodeRegions.EveryCallOpensOneRegionPerOutputPartition](../cpp/tests/gpu/test_plan_executor.cpp) | shad-gpu | 53 |
| TPC-H sf40 bare-cuDF (C++) | hand-written cuDF pipelines vs DuckDB sf40; the benchmark vehicle | [TpchSf40.Q1GroupByAggregates](../cpp/tests/gpu/test_tpch.cpp), [Q3JoinsGroupByTopN](../cpp/tests/gpu/test_tpch.cpp) | shad-gpu (sf40 is a hard precondition) | 4 |
| TPC-H+V sf40 bare-cuDF (C++) | the same for the vector-embedding queries | [TpchSf40.Q11VectorBruteForce](../cpp/tests/gpu/test_tpchv.cpp) | shad-gpu | 4 |
| TPC-H sf40 streamed (C++) | the same four queries and the same DuckDB goldens with nothing held resident — a chunked reader under a byte budget, so it answers whether the query fits rather than how fast the operators are | [TpchSf40Streamed.Q1Streamed](../cpp/tests/gpu/test_tpch_streamed.cpp) | manual | 4 |
| Per-operator cuDF timings (C++) | one operator at a time over real sf40 columns, so a query cost reads as a sum of parts; asserts row counts only, so a benchmark cannot time an empty column | [CudfNodes.OperatorTimings](../cpp/tests/gpu/test_cudf_nodes.cpp) | manual | 1 |
| Multi-GPU TPC-H (C++) | WorkerPool, hash_shuffle, per-device RMM pools across GPUs | [TpchSf40.Q3MultiGpu](../cpp/tests/gpu/test_multi_gpu_tpch.cpp) | manual, 2gpu | 4 |
| Multi-GPU TPC-H+V (C++) | the same for the vector queries | [TpchSf40.Q10VectorCustomerTopNMultiGpu](../cpp/tests/gpu/test_multi_gpu_tpchv.cpp) | manual, 2gpu | 4 |
| Multi-GPU basics (C++) | device-local streams + destruction on the owning worker | [BasicMultiGpu.CudfAndCuvsAcrossTwoGpus](../cpp/tests/gpu/test_basic_multi_gpu.cpp) | manual, 2gpu | 1 |
| Dataset validators (Python) | row counts / clustering / embedding stats over whatever dataset they are pointed at; each check tags itself `EXHAUSTIVE` or `SAMPLED` | [validate_tpch.py](../scripts/validate_tpch.py), [check_s3_datasets.py](../scripts/check_s3_datasets.py) | dataset-matrix (sf1) · validate-large (sf40/sf200) | n/a¹ |

¹ Data-driven — the check count depends on the dataset and SF, so these are excluded from
the total above. Everything else in the repo that can be enumerated as a test case is counted.

Notes

- A mode is one of the five `tp<N>-<sizing>` shapes. A `corpus_query!` line declares
  which of them a query is correct at, per engine, and whether the device run is
  schema-validated; the mode name is the golden's name;
  the tier (`mini`) is the budget those plans are priced at, and it rides in the execution
  goldens' filenames so a budget and a filename cannot name different tiers.
- Execution goldens are per mode, not per query: `<mode>-<tier>.cpu.txt` with a
  `== <query>` section each, the derived `.cost.txt` beside it, and `<tier>.result.txt`
  for the answers. A device run reads the cpu's sections read-only. Result validation is
  `golden_exact` | `golden_approx` | `golden_approx_std` | `live_cpu` | `skip`.
- The crate has no doctest today, so `--doc` runs nothing. The gap #128 names is still open:
  no CI step passes it and the meta guard enumerates only `--test` targets plus `--lib`, so
  the first doctest written would be unrun with nothing saying so.

## Datasets

Host columns are audited, not assumed: they say whether the bytes were actually found
there (`ls` + `sha256sum`, 2026-08-05). Only tpch.minimal is in git; everything else is
generated or fetched per host. S3 is Nebius object storage, endpoint
`https://storage.eu-north1.nebius.cloud:443`, region `eu-north-1`.

| Dataset | Shape | <sub>local</sub> | <sub>verda</sub> | <sub>shad-gpu</sub> | S3 bucket | Used by |
|---|---|:-:|:-:|:-:|---|---|
| tpch.minimal | 5 tables, 19 MB, git-committed | ✓ | ✓ | ✓ | — | C++ plan tests, plan serializer, node executor |
| TPC-H+V sf1, external vectors | 8 tables, 987 MB; GloVe 100-d + DEEP1B 96-d | ✓ | ✓ | ✓ | — | most CPU/GPU rust tests |
| TPC-H+V sf1, synthetic vectors | same tables, hash-generated FLOAT[8] | — | — | — | — | CI only — regenerated every run |
| TPC-DS sf1 | 24 tables, 764 MB | ✓ | ✓ | ✓ | — | plan + CPU + GPU subsets |
| TPC-H+V sf40 | 8 tables, 40 GB | — | — | ✓ | `tpch-sf40` | `peacock_tpch(v)_tests`, multi-GPU (manual) |
| TPC-DS sf200 | 24 tables, 80 GB | — | — | ✓ | `tpcds-sf200` | no tests — S3 check, validate-large |
| TPC-H sf200 | not generated yet | — | — | — | `tpch-sf200` | nothing yet |
| embeddings cache | 1.8 GB local / 129 GB shad-gpu | ✓ | — | ✓ | — | generator input, not a test input |

Paths: `testdata/{tpch.minimal,tpch.sf1,tpcds.sf1,embeddings-cache}`; sf40 and sf200 live
outside the repo on shad-gpu, under `/home/info/peacock-datasets/testdata/`. **+V** means
the TPC-H tables carry vector columns — `part.p_text_embedding` and
`partsupp.ps_image_embedding` + `ps_text_embedding`; TPC-DS does not have embeddings.

The three sf1-class datasets are byte-identical on all three hosts (aggregate parquet
sha256 matches local). Notes on the rows above:

- **The sf1 parquet is generated** — `testdata/generate_testdata.sh` drives DuckDB, and
  `/tpch.sf*/`, `/tpcds.sf*/` are gitignored, so CI regenerates it every run. That is why
  the two sf1 rows differ: `--embeddings synthetic` is the default, so a vector query in
  CI runs against hash-generated FLOAT[8], not the vectors a dev host has.
- **The embeddings cache is a per-host intermediate and is NOT syncable** — no
  `--push`/`--pull` kind covers it; `fetch_embeddings.sh` is local-only and hard-guards
  against running on CI/verda/shad-gpu. Regenerate it where you need it, or ship the
  augmented parquet instead.
- **sf40 lives only on shad-gpu**, and its presence there is a hard precondition of the
  GPU job; the sf40 goldens are committed (see below), so CI compares without touching
  40 GB.
- verda reaches the tree through a `/media/data/peacockdb` symlink to
  `~/peacockdb`. Every test binary honours `PEACOCK_TESTDATA_DIR`, but the plain cpu mode
  of `build-test.sh` does not set it, so those binaries fall back to the build box's path.

## Golden files

All goldens are committed, under `testdata/goldens/`: `tpch.sf1` (38 files), `tpcds.sf1`
(115), `tpch.sf40` (16), plus `recipe-payloads.txt` at the top, the recipe payloads with
a digest each. Most of each sf1 count is the per-query DuckDB cost oracle (22 + 99); the
engine's own are 16 apiece, one plan and one execution set per mode. The committed DuckDB
profile inputs live beside them in `testdata/duckdb-profiles/{tpch,tpcds}` (22 + 99) and
`testdata/duckdb-dynfilters/{tpch,tpcds}` (22 + 99).

The generator scripts live in `testdata/`.

| Golden | Produced by | Depends on | Asserted by |
|---|---|---|---|
| `<mode>.plans.txt` | `planner::tests::plan_goldens` (in `--lib`) with `UPDATE_CANONICAL=1` | sf1 parquet — row-group counts and per-column bytes decide `partition_groups` and the lane rules | the plan golden tiers, section by section |
| `recipe-payloads.txt` | `planner::tests::plan_goldens` with `UPDATE_CANONICAL=1`<br>**and** `PEACOCK_REWRITE_RECIPE_BYTES=1` | sf1 parquet, and a fixed `/tmp` symlink for the testdata root — without it the payloads carry this machine's paths and so does the digest | <sub>the_payload_golden_carries_<br>what_each_call_hands_the_executor</sub> |
| `<mode>-<tier>.cpu.txt` | the corpus cpu tier under `UPDATE_CANONICAL=1` (merge and prune) or `PCK_UPDATE_SECTIONS=1` (merge only) — never the GPU | sf1 parquet; one file per mode, a `== <query>` section each | the corpus cpu tier writes and verifies; the device tier verifies read-only |
| `<mode>-<tier>.cost.txt` | derived from the sibling `.cpu.txt` **section**, × `cost_model.conf` | that `.cpu.txt`, `cost_model.conf` | the corpus cpu tier + `test_cost_model`, which re-derives every section independently |
| `<tier>.result.txt` | the last mode a query declares, under either variable; a run without that mode leaves the section alone | sf1 parquet; one section per query, its `mode=` line naming the author | the corpus cpu tier; the device tier where `gpu_oracle` names a golden |
| `<q>.duckdb_cost.txt` | `gen_duckdb_cost.sh --gen`<br>(DuckDB 1.5.4, `threads=1`, pyarrow 19.0.1) | committed pass-1 profiles ∩ pass-2 dynamic-filter bounds ∩ parquet row-group stats | the cost-report widget (directional signal, not a test) |
| `tpch.sf40/duckdb_<q>.csv`, `.count.csv` | `gen_duckdb_goldens.sh --sf 40`<br>on shad-gpu | sf40 parquet, query text from `tpch_query_sql.sh` | `peacock_tpch_tests` / `peacock_tpchv_tests` |

How they hang together — parquet at the top, goldens derived left to right:

```
testdata/{tpch,tpcds}-queries/*.sql
        │
        └── generate_testdata.sh (DuckDB, --embeddings synthetic|external)
                 ▼
        tpch.sf1 / tpcds.sf1   (parquet, gitignored)
          │
          ├── planner::tests::plan_goldens (--lib), UPDATE_CANONICAL=1
          │     ├──► <mode>.plans.txt
          │     └──► recipe-payloads.txt  [also needs PEACOCK_REWRITE_RECIPE_BYTES=1,
          │                                   and the fixed /tmp symlink]
          │
          ├── the corpus cpu tier, UPDATE_CANONICAL=1   (the author; a device never writes)
          │     ├──► <mode>-<tier>.cpu.txt        (a == <query> section each)
          │     │       └──× cost_model.conf ──► <mode>-<tier>.cost.txt
          │     └──► <tier>.result.txt         (one section per query)
          │
          └── gen_duckdb_cost.sh --gen  (DuckDB 1.5.4, threads=1, pyarrow 19.0.1)
                ├── pass 1, JFP off ──► duckdb-profiles/<bench>/<q>.json     (committed)
                ├── pass 2, JFP on  ──► duckdb-dynfilters/<bench>/<q>.json   (committed)
                └── duckdb_cost.py extract
                      (pass-1 profile ∩ pass-2 bounds ∩ parquet row-group stats)
                        └──► <q>.duckdb_cost.txt

embeddings-cache ──► generate_testdata.sh --embeddings external   (local only, per host)

tpch.sf40 (shad-gpu only) + testdata/tpch_query_sql.sh
    └── gen_duckdb_goldens.sh --sf 40 ──► goldens/tpch.sf40/duckdb_<q>.csv (+ .count.csv)
```

Consequences worth knowing before you regenerate:

- **`.cost.txt` is a pure function of `.cpu.txt` and `cost_model.conf`** — regenerating one
  `.cpu.txt` obliges the sibling `.cost.txt`, and `test_cost_model` re-derives every one of
  them, so a hand-edited `.cost.txt` goes red.
- **A device never authors a golden.** The device tier reads the sections the cpu tier
  wrote, which is what makes a divergence a red test rather than a quietly rewritten
  expectation.
- **The byte-level golden needs a second, deliberate variable**
  (`PEACOCK_REWRITE_RECIPE_BYTES=1`). Under plain `UPDATE_CANONICAL=1` the test verifies
  instead of rewriting, and says so — a bulk regen that moved the wire format goes red
  during the regen, before the goldens are pulled home. It pins what the C++ is handed, and
  it is the file a bulk regen must not quietly rewrite: the diff would come home among the
  others.
- **Two writers can lose a section** ([#213](tickets.md#t213)): the merge locks the inode
  it opened and the publish renames over it, so a whole-corpus regeneration can drop one
  query's section from a `.cpu.txt`. Read the regeneration's `git diff --stat` for a section
  that vanished, and refill it with `PCK_UPDATE_SECTIONS=1` and `--exact <case>`.
- **The `.duckdb_cost.txt` path is re-runnable without DuckDB**: `--extract-only` rebuilds
  the goldens from the committed profiles plus the parquet, so only a genuine oracle change
  needs the 1.5.4 pin.

### Benchmark data flow

A second tree with its own producers. Nothing here is a golden — no run asserts against it
— so the arrows say which script writes each file rather than which test reads it.

```
tpch.sf40 (on the GPU host, outside the repo; symlinked in as testdata/tpch.sf40)
  │
  ├── build-test-shadgpu.sh --run-benchmarks        (peacock_gpu_benchmarks, event timing;
  │   build-test.sh --gpu --run-benchmarks           the same binary on a 26.02 host)
  │     ├──► benchmark-results/<dataset>.sf<sf>/<mode>.benchmark.txt   the chosen run, per node
  │     └──► calibration/records.tsv                one row per cuDF call × execution
  │           └── --pull-benchmarks brings both home
  │
  └── create_nsys_profile.sh [--host …]             (the same binary, under Nsight)
        ├── --trace     nvtx and cuda only; PEACOCK_BENCHMARK_CAPTURE=trace
        │     └──► calibration/capture.sqlite          (not committed)
        │           └── nsys_calls.py × goldens/<dataset>.sf1
        │                 └──► calibration/calls.tsv   what one ABI call splits into, per case
        │
        └── --metrics   the same cases under the memory counters; PEACOCK_BENCHMARK_CAPTURE=metrics
              ├──► calibration/capture-metrics.sqlite  (not committed)
              ├──► calibration/records-metrics.tsv     stays on the host; its microseconds are unusable
              └── nsys_hbm.py --peak-bw (capture × records.tsv)
                    └──► calibration/hbm.tsv           hbm_bytes on records.tsv's coordinates

calibration/records.tsv + calibration/hbm.tsv
  └── plot.py ──► calibration/plots/{load,compute,spread,query,icicle,hbm}/*.png
                  calibration/plots/index.html      one call, every panel
```

What the arrows are there to make checkable:

- **Three runs, and only one of them is timed.** A capture serializes what it traces and the
  counters pass costs several percent more, so neither writes the tree, and the metrics pass's
  own record is read for its coordinates and never for its microseconds. The passes meet on the
  record's tuple — `(dataset, sf, query, mode, node_seq, recipe_seq, call_index, run_index)` —
  which is why the record carries one rather than a node number.
- **`calls.tsv` is keyed by case.** The harness names each `(dataset, sf, query, mode)` in an
  NVTX range around the run, and `nsys_calls.py` reads the case off that range; keyed on the
  node alone, three cases fold into one row set.
- **Every derived file has exactly one producer, and the scripts above run it.** A file that
  only a human regenerates goes stale in silence: nothing checks it against the capture it
  claims to describe, and a panel drawn from a stale one looks exactly like a fresh one.
  `plot.py` is the only thing that draws, for the same reason.
- **The text is committed and the captures are not.** The `.benchmark.txt` tree, the three
  `.tsv` files and every panel are in git, rewritten in place by each collection, so `git diff`
  shows how the numbers moved. The two `.sqlite` exports are hundreds of megabytes of
  undiffable binary that only the scripts above read; `testdata/.gitignore` is deny-by-default
  over `calibration/` for that, and `records-metrics.tsv` is not re-included.

## CI structure (`.github/workflows/pipeline.yml`)

`pipeline.yml` runs on pushes to master and on every PR, but not on documentation —
`**.md` and `llm-wiki/**`, which nothing builds, tests or reads. It takes two layers:
`paths-ignore` skips a wholly-documentation diff, and the **changes** job skips a
documentation-only push to a PR that carries code, which `paths-ignore` cannot see because
it judges the whole PR diff. Both fail open, and a doc file that ever becomes an input to
something has to come off both lists.

Five independent job chains:

```
changes ──► everything below
dataset-matrix (2 legs)          cpp-build-2502 ──► gpu-tests
cost-report ──► deploy-pages (master push only)          s3-datasets
```

- **changes** — the documentation gate above; every other job carries `needs: changes`
  and runs only on its `code == 'true'`.
- **dataset-matrix** — two matrix legs, each in a RAPIDS container: `cudf: 25.02`
  (`rapidsai/base:25.02-cuda12.0-py3.12`) and `cudf: 26.02`
  (`rapidsai/base:25.10a-cuda12-py3.12` — the leg's label is ahead of its image, #129).
  Both legs do the C++ build through the shared `.github/actions/cpp-build` composite
  (conda gcc + ccache + pinned cmake/ninja), `ctest --test-dir cpp/build -L cpu`,
  generate + validate sf1 (pinned DuckDB — the TPC-DS dsdgen column types drift between
  releases and would move every plan golden), then the CPU rust tiers:
  the ffi rung as `--lib -- ffi_tests::` (which links the FFI but
  touches no device), `peacockdb-ffi --test test_ffi`, plus rust-only `--lib` (which carries
  the plan goldens, the planner's own tests and the executor contract's cpu half),
  and `test_cpu_corpus`. The `peacockdb` CLI is built here
  and not run: it has no test target, so this is the only thing that compiles it.
  The steps needing neither the generated dataset nor a device run on the 25.02 leg alone,
  since a rust-only test cannot see cuDF and a second leg would report the same failure twice:
  the exec-model python, `test_cost_model`, `test_golden_format` and `test_corpus_goldens`.
  `CMAKE_GENERATOR` and `RUSTFLAGS` are job-level: cargo's fingerprint includes RUSTFLAGS,
  so a step carrying its own recompiles the dependency tree, and the image has ninja and no
  make, which flatc-fork's cmake needs told. Build and run are still separate steps, and
  their remaining env must stay byte-identical or the run step recompiles.
- **cpp-build-2502** — builds the 25.02 C++ side, bundles the Arrow/Parquet runtime libs,
  and stages the device rung, built `--features gpu`, as the `cpp-install-25.02` artifact:
  `test_gpu_corpus`, `test_node_timing`, `peacock_gpu_benchmarks` and the crate's own
  unit-test binary as `peacockdb_core_gpu_lib`, whose `gpu_tests` modules exist only under
  that feature. Separate from dataset-matrix so the GPU
  job can start without waiting for the CPU tests.
- **gpu-tests** (needs cpp-build-2502) — ssh to **shad-gpu** into a per-run `REMOTE_DIR`:
  rsync artifact + testdata, patch the binaries for glibc 2.35, generate sf1 on the host
  if absent, then run. Three guards, each closing a hole that shipped: sf40 presence is
  asserted by CI (`tpch.sf40/lineitem.parquet`) because the binaries themselves skip and
  exit 0, which would be green having verified nothing; every `peacock_*_tests` binary runs
  by glob with a ran-any assertion (a hand-written list once let `peacock_tpchv_tests` be
  built, shipped and patched but never run); and any binary reporting `PASSED 0 tests` is
  an error. The staged rust binaries then run with `--test-threads=1` (cuDF/RMM share one
  process-wide pool); `peacockdb_core_gpu_lib` alone also takes `gpu_tests::`, the path
  filter that selects the device rung and leaves the CPU and FFI rungs it also holds to
  dataset-matrix, and a rust binary reporting `running 0 tests` is the same error as the
  C++ one; `peacock_gpu_benchmarks` runs under `--skip bench_`, so its harness assertions
  run on every job and its timed cases only under `--run-benchmarks`. No `set -e` — statuses are OR'd so one failure cannot skip the rest — and
  `REMOTE_DIR` is removed on `always()`.
- **cost-report** — the report crate and its inputs, and nothing else: python
  `testdata/test_duckdb_cost.py`, the `scripts/exec_model/tests/test_*.py` prototype set,
  the `scripts/calibration/tests/test_*.py` cases over a synthetic capture, and
  `cargo test -p cost-report`. Then report generation, the PR-comment upsert and the
  cost-regression gate against the base SHA, which fails the job on a regression. On a
  master push it uploads the Pages artifact. The comment has a 65,536-byte body to fit —
  `COMMENT_MAX_BYTES` in `cost-report/src/main.rs`, asserted over the real registry — so
  every anchor but the query cell is dropped from the markdown. The file's FIRST LINE is its
  sentinel and the workflow reads it from there, so the string exists once.
- **deploy-pages** (needs cost-report; master pushes only) — publishes that artifact.
- **s3-datasets** — reads parquet footers of the large S3 datasets over ranged GETs
  (schema, embedding dims, row counts). Skipped on fork PRs, where the secrets do not
  exist, and exits 0 loudly if the credentials are absent anyway.
- **validate-large.yml** — `workflow_dispatch` only, gated on the same repo: validates the
  named datasets in place on shad-gpu with the scale-safe validators, then runs the S3
  metadata check. The `datasets` input defaults to `tpch.sf40 tpcds.sf200`; `tpch.sf200` is
  allow-listed but deliberately not defaulted, because it does not exist yet.

The pipeline runs neither a `rustfmt --check` nor a clippy step, the repo has no `rustfmt.toml`,
and the tree is not rustfmt-clean — files are formatted one at a time as a task touches them.
Closing that takes three things together: a config, a one-time sweep of the whole tree, and a
step; the sweep rides in a commit of its own, never in a behaviour change, so a diff stays
readable. Not a ticket, since nothing behaves wrongly.

Two traps before adding a step: a container job defaults `run:` to `sh`, not bash, so the
two of them declare `defaults.run.shell: bash` to get `-o pipefail` at all; and the python
steps install the current pandas — 3 on the runner's 3.12, 2.3 on a dev box's 3.10 — so the
prototype has to hold across that boundary, unpinned on purpose.

Which Rust targets run where is not folklore — `test_ci_coverage` fails when a target
exists that no workflow step names. Doctests are the one class outside that guard (#128).
The two python steps need no such guard because neither names files: the extractor test is
one file, and the prototype step globs `test_*.py` and errors when the glob matches
nothing. Inside a prototype file the equivalent hole — a test defined below the
`__main__` footer, which pytest collects and direct execution would not — is closed by
`tests/harness.py`, which reads the source back and fails naming what it missed.

## What `rust-only` means

A cargo feature (`peacockdb-core/rust-only` → `peacockdb-ffi/rust-only`, an empty marker),
and the definition lives in `peacockdb-ffi/build.rs`: under it the build script **skips
cmake entirely**, so nothing links `libpeacock_gpu` and neither cuDF nor a CUDA toolchain
is needed. Everything that reaches the FFI is compiled out behind
`#[cfg(not(feature = "rust-only"))]` — the GPU backend and its batch type, the extern
declarations, `test_gpu_corpus` at file level, and the `ffi_tests` modules. The device tests
sit one rung higher, in `gpu_tests` modules that exist only under `--features gpu`
(`coding-style.md`, the rung ladder).

So it is the Rust half built against DataFusion alone. That is why it is the fast loop,
and why anything a rust-only binary can do is by definition CPU-only.

**It selects a *build*, not a set of tests.** `cargo test --features rust-only -p
peacockdb-core` runs every target that compiles under it — including the full CPU
execution suite — not just the golden/meta tier. Naming a tier takes `--test`. This has
been mis-transcribed at least once; see the "refactor is verified with a subset" rule
below.

**A third feature, `test-support`, is never passed by hand.** It gates `src/test_support/`,
the shared harness — the testdata root, the modes, the golden-text reader, the link-time
registry, the result comparators, the corpus goldens and the corpus case itself — which the
crate's unit tests reach as `crate::test_support::…` and the `tests/*.rs` binaries as
`peacockdb_core::test_support::…`. The crate's dev-dependency on itself turns it on, so `cargo
test` in every shape sees the module and `cargo build` cannot name it; no CI step and no script
passes the flag. The corpus binaries and `test_corpus_goldens` reach it directly through
`cpu_case`, `gpu_case`, `authoritative_mode` and `over_cap`, whose signatures name no engine
type; the other suites name `test_support` directly. `tests/common/` holds only the two case
lists, `corpus_cases.inc` and `corpus_benchmark_cases.inc`: each corpus binary `include!`s its
own so `corpus_query!` expands, and `inventory::submit!` runs, in that binary alone.

## Local build workflows and caches

One cargo target dir per workflow — this is what prevents cache thrashing (feature flags
and `cudf_ROOT` changes bust fingerprints; sharing a dir means recompiling the DataFusion
stack on every switch):

| Workflow | Command | C++ build dir | Cargo target dir |
|---|---|---|---|
| rust-only (golden regen/verify, fastest loop) | `cargo test --features rust-only -p peacockdb-core --test <target>` (drop `--test` and it runs every target that compiles, not a tier — see above) | — | `target/` |
| cost-report | `cargo test -p cost-report`; preview: `scripts/cost-report-preview.sh` | — | `target/` |
| C++ only, cudf 25.02 | `scripts/build.sh --cudf_ROOT <rapids-cuda-12.2> --gcc-version 12 ...` | `cpp/build` | — |
| C++ + staged Rust bins, cudf 26.02 | `scripts/build-test.sh --build` (drives cmake directly) | `cpp/build26` | `target-cudf-<basename cudf_ROOT>` |
| Rust + cudf (FFI) | via build-test scripts, or manually with `scripts/cargo-cudf.sh` | cargo OUT_DIR | `target-cudf-<basename cudf_ROOT>` |
| Any of the above, containerized | `scripts/docker-build.sh [--no-image] -- <command>` | `<cache-dir>/cpp-build` | `<cache-dir>/cargo-target` |

## Inspecting the tree

Four scripts answer questions about the shape of the tree rather than about a build. They take
no arguments beyond what is shown and need no dataset or device.

| Script | Question | Invocation |
|---|---|---|
| `scripts/case-inventory.sh` | every test case a shape compiles, one per line — `--lib` and every `--test` binary for `rust-only` and `cudf`, `--lib` alone for `gpu`, since no `--test` target reads the `gpu` feature and their `cudf` listing stands | `scripts/case-inventory.sh rust-only`, or `CUDF_ROOT=<rapids env> scripts/case-inventory.sh cudf\|gpu` |
| `scripts/compare-inventory.sh` | did any case stop compiling into its target | `scripts/compare-inventory.sh <shape> <baseline> <current>` — the baseline is an argument, so it outlives whichever task took it |
| `scripts/visibility-dump.py` | every `pub`/`pub(...)` item with its declaring file; `--items` drops the file and visibility, giving the set a move cannot change | `scripts/visibility-dump.py peacockdb-core/src` |
| `scripts/residue-gate.sh` | does a retired name survive anywhere, tracked or not | `scripts/residue-gate.sh` |

`residue-gate.sh` resolves paths against the repo root and checks that the result is non-empty.
It reported a clean tree from the wrong directory once, because `git` rejected the pathspec and
every section came back empty — a gate that finds nothing and a gate that cannot look are the
same output otherwise.

The container and the native path deliberately do **not** share a cargo cache
(`/cache/cargo-target` + `RUSTFLAGS=-C debuginfo=0` vs `$PWD/target-cudf-*`), so
alternating between them costs a cold rebuild each way. That is the price of having
both entry points, not thrash to be diagnosed.

ccache is auto-enabled for both C++ workflows when the binary is present (host
compilers only — ccache + nvcc is unreliable). Rust caching is the per-workflow target
dir below.

Note the C++ side is built *twice* on the cudf path, into two dirs that are both used:
`build-test.sh` drives cmake into `cpp/build26` for the installable libs and the C++
test binaries it ships, while `peacockdb-ffi/build.rs` runs cmake again into cargo's
`OUT_DIR` for the copy the Rust binaries link against. The `.peacock-ffi-cudf-root`
stamp is what keeps the second one from rebuilding on every `--build`.

Rules that keep this healthy:

- **A refactor that must not change behavior is verified with a representative subset**
  — one query per mode/tier per binary, plus the full rust-only tier — not a full
  CPU/GPU suite run: the goldens are the invariant, so unchanged golden bytes and a
  green rust-only tier prove more per minute than re-running everything.
- **Day-to-day iteration is the rust-only loop** — plain cargo into `./target`, no
  wrapper, no C++/CUDA:
  `cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens`
- **Never run cudf-feature cargo builds in `./target`** — they would evict the rust-only
  cache and vice versa (the `ffi` feature and `cudf_ROOT` both change fingerprints, and
  the cudf side recompiles the DataFusion stack at opt-3). For one-off cudf/FFI cargo
  commands use `scripts/cargo-cudf.sh`, which requires `CUDF_ROOT` and derives *both*
  `CARGO_TARGET_DIR=target-cudf-$(basename "$CUDF_ROOT")` AND `CC`/`CXX` from that same
  basename (25.02 → gcc-12, 26.02 → gcc-14; an unknown root fails and asks for
  `GCC_VERSION`) — the same dir and the same compiler the build-test scripts use, so it
  shares their warm cache. Both halves matter: cc-rs emits `rerun-if-env-changed` on
  `CC`/`CXX`, so entering a target dir with a different C compiler re-runs every native
  build script (zstd-sys, bzip2-sys, lzma-sys, psm, blake3) and rebuilds the whole
  DataFusion stack above them — before this, alternating between the two entry points
  thrashed the cache each way:
  `CUDF_ROOT=~/data/miniforge3/envs/rapids scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run`
  For anything more than a one-off command, use `build-test.sh` / `build-test-shadgpu.sh`
  instead — they handle build, staging, shipping and running.
- **A cudf-shape binary needs `LD_LIBRARY_PATH` to run at all.** `libpeacock_gpu.so` lives in
  the FFI crate's `OUT_DIR` (`target-cudf-*/debug/build/peacockdb-ffi-*/out/lib`) and nothing
  puts it on the loader path, so the binary exits 127 with a loader error on stderr and prints
  nothing on stdout. It is loud, but a tool that reads stdout and ignores the status sees an
  empty list, which reads exactly like a target the `rust-only` cfg compiled out. Prepend that
  dir and the cuDF root's `lib` before listing or running one by hand.
- The FFI crate caches its cmake `cudf_DIR` in OUT_DIR; both build-test scripts clean it
  **only when the cuDF root changed** (stamp file `.peacock-ffi-cudf-root`);
  `PEACOCK_FFI_CLEAN=1` forces.
- `cpp/build` stays 25.02, `cpp/build26` stays 26.02 — separate dirs also dodge cmake's
  stale `cudf_DIR` cache. Keep both even when stale.
- `-Dcudf_ROOT` is the one prefix the scripts pass. `cpp/CMakeLists.txt` copies it into
  `CMAKE_PREFIX_PATH`, so cuVS and Arrow come from the same env and a fresh build dir needs
  nothing on PATH. Before that, a fresh dir configured only where the env's `bin` was on PATH,
  which CI's image has and a dev host does not.
- cuDF version scope: a functional GPU run on **25.02** (shad-gpu) is sufficient
  verification; **26.02 needs only to compile** (CI builds both legs).

## Memory constraints (15 GiB-class hosts)

- **At most one binary links at a time**: the cmake configures set a Ninja
  `link_pool=1` job pool (parallel links OOM the host; compiles stay parallel). The
  build-test scripts also serialize rust test-binary builds one `--test` per invocation.
- Cold cudf cargo builds are throttled to `CARGO_BUILD_JOBS=3` on <20 GiB hosts
  (override via env).
- Full local CPU suite: run with `-- --test-threads=2` or less; the heavy golden suites
  OOM at default parallelism.

## Remote hosts

| Host | Use | Managed by |
|---|---|---|
| **shad-gpu** (most used) | GPU test suite (cudf 25.02, H200-class; old glibc → patch step) | `scripts/build-test-shadgpu.sh` (`--build --push-binaries --patch --run[-detached]` / `--all`, `--run-status`). Resilient rsync + retries — the link is flaky |
| **verda** (when available) | large CPU runs, golden regen | `scripts/build-test.sh --host verda --all` (add `--rust-only` to skip the C++/FFI half) |
| **verda-gpu** | same root volume as verda, with an H200 attached; cuDF 26.02, modern glibc, nsys 2024.5 | `scripts/build-test.sh --host verda-gpu --gpu --all`; the corpus benchmark and its Nsight passes too — see *Corpus benchmarks* |
| **nebius** | large CPU-only VM | manual |

- **Testing the regen *mechanism* is not a full regen.** Scope it with `PCK_TEST_FILTER`
  (forwarded to every staged binary; the gpu lib binary takes it inside its `gpu_tests::`
  rung, as the exact names `--list` reports under both) — two or three queries prove the path as well as
  903 do, and a full `--update-canonical` run rewrites every golden on the remote and
  pulls the whole set back into a git working tree, where an unrelated diff can ride
  home in an unrelated commit. Note that a binary whose tests all filter out runs zero
  tests and passes, so say which binaries actually executed; and a filter naming a query
  matches no test whose name is a property rather than a query, so exercising those takes a
  filter that matches them or a separate run.
- **A GPU binary needs a fixed amount of free VRAM.** Each measuring binary reserves a
  byte budget: `peacock_tpch_tests` 69 GiB, `peacock_tpchv_tests` 30, `peacock_cudf_node_tests`
  10, `peacock_gpu_tests` and `peacock_plan_tests` 1 each, `test_node_timing` 2, and
  `peacock_gpu_benchmarks` 69 — the tpch figure taken for the same sf40 data, not yet measured
  for the engine's own run. Below its budget a binary does not
  shrink: the pool is not built, and the sf40 pair goes red on rmm's default resource. Read
  `[rmm] pool of N GiB could not be built` at the top of the log first; the
  `cudaErrorMemoryAllocation` failures under it are its consequence. shad-gpu is a shared H200,
  so check `nvidia-smi` before debugging a red GPU tier ([#178](tickets.md#t178)). Budgets are
  H200 numbers; `PEACOCK_RMM_POOL_BYTES=<bytes>` replaces one for a sweep or another host.
- **Prefer verda for large CPU runs** (whole suite or big selections). It is not always
  up (the human starts it manually) — falling back to a local run is completely fine.
- **Comparing files across hosts: use checksums, and `LC_ALL=C sort` for any listing.**
  Two hosts collate `ls`/`sort` differently, so a manifest diff reports differences that
  are pure collation — this produced two false alarms in one session before checksums
  settled it. Compare `sha256sum`/`md5sum` output, not directory listings.
- Rented hosts change SSH host keys on reprovision: `ssh-keygen -R <host>` + re-keyscan
  rather than fighting the mismatch.
- **The shad-gpu patch step uses the build host's glibc version.** shad-gpu runs glibc 2.31,
  so shipped binaries are patched to a prefix under `~/glibc-<version>` there: 2.35 for a
  22.04 build host and CI's container, 2.39 for a 24.04 host such as dev. The version is read
  from `getconf` where the binaries are built, and the prefix is built once on the first patch
  from that host class. A binary patched to a glibc older than its own dies at load with
  `version GLIBC_2.38 not found`.
- **Golden regen**: on verda via `scripts/build-test.sh --host verda --rust-only
  --update-canonical`, then `--pull-goldens` to bring regenerated goldens back; or
  locally with `UPDATE_CANONICAL=1 cargo test --features rust-only ...`. Sync is one
  flag per kind per direction — `--push-<kind>` / `--pull-<kind>` for
  `parquet | goldens | duckdb-profiles | duckdb-dynfilters | queries`.
  **Pushes mirror (`--delete`), pulls are additive**, deliberately: the remote is a
  *partial* mirror (`testdata/goldens/` holds `tpch.sf40/`, which lives only on shad-gpu),
  so mirroring downward would delete fixtures the source host never had, out of a git
  working tree. (The sf40 *dataset* is what lives only there — its 16 CSV goldens are
  committed.) `recipe-payloads.txt` rides along safely because its test refuses to
  regenerate without `PEACOCK_REWRITE_RECIPE_BYTES=1`.
- **A golden that pins a producer against itself proves consistency, not correctness.** Where
  an artifact's only consumer is on the far side of an ABI, the first read by that consumer is
  the first check — until then the text and the digest can agree with each other and with the
  code, and all three be wrong.
  The tpch **embeddings cache is NOT syncable** — a per-host intermediate
  (`fetch_embeddings.sh`, ~1.8 GB, gitignored); regenerate it where you need it.
- Remote CPU runs ship built binaries + goldens + data only — never source. The plain cpu
  mode does not set `PEACOCK_TESTDATA_DIR`, so its binaries fall back to the compile-time
  path and the remote needs a `/media/data/peacockdb` symlink.
- **Large runs (test or regen): arm a monitor that reports progress every 5 minutes** —
  progress may stall (flaky links, OOM kills), and a silent stall looks identical to a
  long run.
- **Large CPU + GPU batches run in parallel** — kick off the shad-gpu run and the
  verda/local CPU run concurrently; neither waits for the other.
- **A run that outlives your ssh session** is `--run-detached`, read back with
  `--run-status`. That flag exits 0 only when the latest run finished with 0 — still
  going, died without writing its code, and a completion belonging to an earlier run are
  all non-zero, because the alternative is a status command that reports someone else's
  success as yours. Run state lives in `$REMOTE_REPO/.run-state/gate.{sh,log,rc,id}`,
  deliberately outside `cpp/install/` — that tree is mirrored with `--delete`, so a marker
  kept there is erased by the next push.

## Benchmarks

### Corpus benchmarks — `peacock_gpu_benchmarks` (scripted)

Its own case list, `peacockdb-core/tests/common/corpus_benchmark_cases.inc`, and
deliberately not the correctness gate's: the two disagree about sf on purpose. Correctness
runs at sf1, where a wrong answer is legible in six million rows; at sf1 a query is mostly
the host prologue, so the rows worth timing are at sf40 and the rows worth checking are not.
A (query, mode) timed here must still be enabled on a device in `corpus_cases.inc`, and a
rust-only test in `test_corpus_goldens` reads both files and says so. Today: tpch q6 at
`tp1_single` and `tp4_sized`, q19 at `tp1_single`.

It asserts nothing about an answer, so it can never gate a merge. Its eight harness assertions
run on every `gpu-tests` job under `--skip bench_`; the timed cases run only here.

The run is measured under event timing — a CUDA event pair around every call, no sync inside
a region — and what that costs was measured once at sf40 by `test_node_timing`'s procedure
with `SF = "40"`: tpch q19 at `tp1-single` on shad-gpu's H200 (cuDF 25.02, a 69 GiB pool, a
62 GB neighbour on the card throughout), the second-smallest of ten walls each — off
1308313 µs, events 1293402 µs, within about 1 % of each other, which is the run-to-run spread
on that host. Nothing in the tree is corrected for it.

Six steps, three scripts:

```
# 1. a release build of the harness into cpp/install/rust-benchmarks/ — the run refuses a debug build
scripts/docker-build.sh --no-image --cache-dir /build/peacock -- ./scripts/build-test-shadgpu.sh --build-benchmarks
# 2. ship and glibc-patch  3. time the corpus  4. bring the tree and the record home
./scripts/build-test-shadgpu.sh --push-binaries --patch --run-benchmarks --pull-benchmarks
# or, for a run that outlives your ssh session (the suite takes tens of minutes):
./scripts/build-test-shadgpu.sh --push-binaries --patch --run-benchmarks-detached
./scripts/build-test-shadgpu.sh --benchmark-status     # going? finished? log tail
./scripts/build-test-shadgpu.sh --pull-benchmarks      # once it reports finished
# 5. the derived records: both Nsight passes, their captures, calls.tsv and hbm.tsv.
#    No flag runs both; --trace or --metrics picks one.
./scripts/create_nsys_profile.sh
# 6. every panel and index.html, from one call
python3 scripts/calibration/plot.py \
    --record testdata/calibration/records.tsv --hbm testdata/calibration/hbm.tsv \
    --out-dir testdata/calibration/plots
```

Steps 5 and 6 are separate scripts because they are separate measurements: a capture
serializes what it traces and the counters pass costs several percent, so neither may write
the tree step 3 produced. What each writes is the diagram under *Benchmark data flow*.
`--benchmark-status` exits 0 only when the latest run finished with 0, as `--run-status` does.
Check `nvidia-smi` for a neighbour before step 3: a process holding the card inflates every
number here without failing anything.

**The same six steps on a 26.02 host** (verda-gpu) go through `build-test.sh`, which builds
against the local `rapids` env and needs no docker and no glibc patch. The three benchmark
flags need `--gpu`; `--all` does not imply them, and `--run` with `--run-benchmarks` is
refused — one exit code cannot mean both "gate green" and "measurement completed".

```
./scripts/build-test.sh --gpu --build-benchmarks                       # 1: release build, cold ~35 min at 3 jobs
./scripts/build-test.sh --host verda-gpu --gpu --push-binaries          # 2: cpp/install, mirrored with --delete
./scripts/build-test.sh --host verda-gpu --gpu --run-benchmarks         # 3: attached; keep the ssh session; PCK_TEST_FILTER narrows
./scripts/build-test.sh --host verda-gpu --gpu --pull-benchmarks        # 4
./scripts/create_nsys_profile.sh --host verda-gpu --remote-dir /home/dmitry/peacockdb \
    --remote-cudf-root /home/dmitry/miniforge3/envs/rapids-26.02        # 5
```

Two things the shad-gpu path handles that this one leaves to the host: `testdata/tpch.sf40`
must already be a symlink to the dataset (the script checks it resolves and creates
nothing), and the GPU's performance counters must be open to non-root for step 5's
`--metrics` pass (`options nvidia NVreg_RestrictProfilingToAdminUsers=0` in
`/etc/modprobe.d/`, then a module reload — verda-gpu has it; nsys says
`ERR_NVGPUCTRPERM` when a host does not). The profile script's HBM defaults (`gh100`,
4.8e12 B/s) are an H200's; another card overrides `PCK_BENCH_HBM_SET` and
`PCK_BENCH_HBM_PEAK_BW`. `--push-binaries` mirrors `cpp/install/` with `--delete`, so a
push from a checkout that never ran `--build-benchmarks` removes the benchmark binary from
the host, and a push from one that never ran `--build` removes the gate's. Both hosts write
the same files and nothing in them says which host it was ([#226](tickets.md#t226)).

**The tree**: one file per (dataset, mode) at
`testdata/benchmark-results/<dataset>.sf<sf>/<mode>.benchmark.txt`, a `== <query>` section
each. A section is the plan tree with one line under every node:

    time_us=[[22,37,40],[55,1,30]] total_us=185

Lanes outermost, one entry per call the lane made, each entry the call's device microseconds
summed over its output partitions; `1` where the clock rounded a region to zero, since every
call opens one; `0` where the call crossed no ABI — the backend answered from what it held,
as an accumulator taking a batch or a join's `set_build` does. A lane the node was never
driven on is `[]`. Then a `--- run ---` trailer:
`run_us` (the chosen execution end to end, after planning), `device_us` (Σ of the tree's
`total_us`; the gap to `run_us` is the host), `runs=[…]` (every measured execution, in order),
`build=release`, `allocator=` (what `install_rmm_pool` reported: with rmm's default every cuDF
intermediate is a `cudaMalloc`/`cudaFree` round trip billed to the node that allocated it,
which moves the profile and not just the scale). The reported execution is the second-smallest
by `run_us` of ten, after one discarded warm-up: the fastest run is the one most likely to
have caught a scheduling accident, and a whole run is reported rather than a per-node minimum,
which would be a tree belonging to no execution. Both counts are constants in
`src/test_support/` (`MEASURED_RUNS` in `mod.rs`, the warm-up in `corpus_benchmark.rs`), so
every file in the tree was taken at the same counts.
`--pull-benchmarks` is additive and no push `--delete`s the directory.

**The record**, `testdata/calibration/records.tsv`: one row per cuDF call — one (plan node,
recipe step, call index), not one node and not one output partition — for every measured
execution, so the same call recurs once per `run_index` and the spread is data. Seventeen
columns: `dataset sf query mode node_seq node_type lane recipe_seq recipe_kind call_index
run_index in_rows in_bytes out_rows out_bytes host_us device_us`. `node_seq` is post-order,
`lane` the driving lane, `call_index` what C++ counted to for that seq, `recipe_kind` the fb
kind or the ABI symbol for a slice and an export. No cell is empty: `out_*` come from the
call's own `NodeStats` priced by the schema the executor holds, and a middle call's `in_*` is
the call before it. The `#` heading carries what is constant across a run — `timing_mode=events`,
`build=release`, `allocator=`, `capture=` — and an append under a different heading is
refused. The harness checks each execution's rows against the plan before it writes them;
the rest of the format is in the heading itself and in `src/test_support/record.rs`.

Two variables. `PEACOCK_RECORD_PATH` names the record; unset, none is written, so the tree
and the record never depend on each other. `PEACOCK_BENCHMARK_CAPTURE=trace|metrics` turns
NVTX on, writes `capture=` into the heading and leaves the tree alone — a captured run is
never the published one; unset means `capture=none`, any other value fails naming the two.
`PEACOCK_GPU_DEBUG` is not forwarded: its per-operator sync is the thing being measured.

### Wall-time C++ suites (currently unscripted)

Wall-time runs are manual; the protocol: `PEACOCK_BENCHMARK=1`
(`PEACOCK_BENCHMARK_RUNS=5`), execute-only timing, all-device-synced, report the
**2nd-minimum** of the runs. Current numbers: `llm-wiki/reports/benchmark-minimal.md`.

- **TPC-H / TPC-H+V, single GPU**: build `peacock_tpch_tests` / `peacock_tpchv_tests`
  (25.02 leg) and run on shad-gpu with the sf40 env vars
  (`PEACOCK_TPCH_SF40_DIR`, `PEACOCK_TPCH_GOLDEN_DIR`, `PEACOCK_TPCH_VEC_PARAMS`).
- **Multi-GPU**: build `peacock_multi_gpu_tpch_tests` / `peacock_multi_gpu_tpchv_tests`
  explicitly (EXCLUDE_FROM_ALL) from `cpp/build26` on a multi-GPU host; G=1 baseline via
  `CUDA_VISIBLE_DEVICES=0`. Benchmark **each query in its own process**
  (`--gtest_filter=...`) — multiple benchmark queries in one process are flaky at G≥2
  (process-global cudf stream state across WorkerPool teardowns). Correctness (single
  execute) is fine in one process. Mean-type aggregates must decompose to partial
  SUM+COUNT — never average partial means.

## Antipatterns

- **A wait loop that matches itself.** `timeout 900 bash -c 'until ! pgrep -f "cargo test …";
  do sleep 15; done'` can never exit: the wrapper's own command line contains the pattern, so
  `pgrep -f` finds the waiter and the loop waits for itself. It ends only when the timeout
  fires. Where a pattern is unavoidable, exclude the waiter's own pid.
