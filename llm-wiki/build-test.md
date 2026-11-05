# peacockdb build & test

Code and tests are authoritative; this page maps them.

## Test categories

**Grand total: 7668 test cases — Rust 5959, C++ 233, Python 1476.** The Python figure includes the 425 runs of the engine's own plans and the per-call model's check on the measured record, which only a manual dispatch runs, the external-embeddings sidecar recount, which is run by hand, and the 231 image and VM tests under `scripts/env/tests/`, run by hand too. The header is the sum of the N columns of the two tables below, and the rows count cases: a target's own `--list` total is larger, because its registry test is counted once in Registry ↔ CSV rather than again in each tier it belongs to. Comparing a row against a target total is how this page gets mistakenly reported as drifting.

**Runs** — `dataset-matrix` = pipeline.yml's job with the generated dataset and the cuDF
matrix, both legs unless a step says one · `cost-report` = the cost-report job · `shad-gpu` =
CI GPU job, `--test-threads=1`, on nebius-gpu while shad-gpu is down (*Nebius host*); the sf40
benchmark cases run by hand on nebius-h100, through `run-gpu.sh … run bench`, and the sf40 C++ pair
has no host while shad-gpu is down (its rows say so) · `manual` = no CI step runs it · `2gpu` =
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

#### cpu — `--features rust-only`: no FFI, no device. 3414 cases: `--lib` 917, `test_cpu_corpus` 2447, `test_corpus_goldens` 47, `test_cost_model` 3

*crate integration, external*

| Corpus, cpu | [test_cpu_corpus](../peacockdb-core/tests/test_cpu_corpus.rs) | 2446 |
|---|---|--:|

one `corpus_query!` line per query declaring its cpu and gpu modes, its THREE oracles —
DuckDB's, the cpu's, the device's — and whether its device run is schema-validated, expanded
to a case per (query, mode): planned, run on `CpuBackend`, validated, and the answer
checked against plain DataFusion at `target_partitions = 1` — except under
`data_fusion_disabled`, for a query DataFusion 45 answers wrong or refuses. Such a line checks no
answer against DataFusion, so it must compare its whole answer with DuckDB. `tpch/distinct-functions`
is the one: DataFusion refuses `stddev(DISTINCT)` and answers a grouped decimal `avg(DISTINCT)`
as the plain average, and `end_to_end.rs` holds it against a hand-lowered oracle as well. 205
lines, 204 of them running at the modes each is correct at — one of pbench's runs at the two tp1
modes only: `not-in-uncorrelated`, which tp4 refuses at plan time on
[#267](tickets/complete-coverage.md#t267). **1 line is out entirely**, the one `duckdb_none` line:
pbench's `in-is-null`, refused at plan time on [#250](tickets/joins.md#t250). 1017 cells,
plus twelve checks
that every declaration's oracles suit each other and every device cell has a cpu cell,
that the registry's `duckdb_oracle`, `cpu_oracle`, `gpu_oracle` and `schema_validation` columns
copy each line, that its `stored` column is what the three result goldens hold, in both
directions, and a fingerprint on the device's side wherever the line says `gpu: fingerprint`, that a hyphenated query resolves its authority and has its result section, that
an undetermined line's rows are compared only against committed files, that
`duckdb-result.txt`'s sections are the registry's rows in both directions, every
`data_fusion_disabled` line comparing its whole answer with DuckDB, and the order guard: a line
says `no_sort`, in its `duckdb(…)` and its `golden(…)`, exactly when its query's plan at one
partition ends in a sort (under any limit or projection) — a fingerprinted line, tpch q11
among them, and a `live_cpu` gpu oracle are exempt, and a `duckdb_none` line is held by its
golden half alone, unless our side has no plan; three cases pin the exemptions and the golden
half on its own

Then the DuckDB tier, 1215 of the count and the only independent oracle over the answers
([#235](archive/archived-tickets.md#t235)): one `duckdb_<ds>_<q>` case per line comparing
`mini.result.txt` with `duckdb-result.txt` by column position under the line's
`duckdb_oracle`, and one `duckdb_gpu_<ds>_<q>_<mode>` per enabled device cell doing the same
for `gpu-result.txt`. Today 76 lines are `duckdb(sort, exact)`, 78 `duckdb(no_sort, exact)` (two
of them with `date(…)`, pbench's `interval-mixed-units` and `interval-month-clamp`, and one with
`gpu: fingerprint`, tpcds q2), 11
`duckdb(sort, approx)`, 11 `duckdb(no_sort, approx)`, 3 `duckdb(no_sort, rel(1e-5, …))` (tpcds
q58, q61 and q66, #288's decimal division digits), 25 `duckdb_fingerprint` (the over-cap
sections) and 1 `duckdb_none` (one side does not answer). Three more cases: every facet of the
grammar is used by some line, every
enabled device cell has its `gpu-result.txt` section and no other, and the committed
`gpu-result.txt` carries cuDF 25.02's `cudf=` line — a dataset with no enabled device cell has
no file, and neither guard asks for one

Then the order tier, 202 of the count: one `device_order_<ds>_<q>` case per line with a device
cell. It refuses the line if its answer depends on an order a device join does not keep. A hash
or nested-loop join on the device emits its rows in a different order each run. So a limit with
no sort between it and such a join, or a sorted cut between two tied rows that differ, would make
the cell flap. Read off DataFusion's plan at one partition; a sorted cut is re-run one row wider
to read the rows either side of it. A cross join keeps its order, and a keyless aggregate
answers one row, so neither is a hazard. Run once over every line DataFusion plans, all pass: 34 lines cut a
sorted answer over a join, 28 of them among the 157 lines that had a device cell then, and no cut falls on a
tie

| Registry ↔ CSV, cpu | [the_registry_matches_the_cpu_corpus_in_both_directions](../peacockdb-core/tests/test_cpu_corpus.rs) | 1 |
|---|---|--:|

the `cost-registry.csv` cpu column matches the cases the corpus expands to, both directions;
one binary per engine, since `inventory` collects per linked binary

| Corpus goldens, self-consistency | [test_corpus_goldens](../peacockdb-core/tests/test_corpus_goldens.rs), [benchmark](../peacockdb-core/tests/test_corpus_goldens/benchmark.rs), [benchmark_run](../peacockdb-core/tests/test_corpus_goldens/benchmark_run.rs) | 47 |
|---|---|--:|

the committed sections against their own arithmetic, with no dataset and no run: `consumed +
abandoned == emitted` at every node, lane counts against `lanes=N`, the loader's `batch_rows` a
prefix of its own lane's `partition_groups`, the root's `out_rows` against `.result.txt`, and the
tree the indentation draws. The golden is written by the run it will later check, so a file that
contradicts itself is the only witness to a renderer that is wrong. The benchmark tree gets the
same reading: every `total_us` is the sum of the `time_us` beside it, every committed tree reports
a release build, every timed (query, mode) is enabled on a device in `corpus_cases.inc`, and the
case list is, byte for byte, what the registry's enabled device cells for tpch and tpcds write.
Every case the list declares has its section, timed or a failure's record, in every device's tree
that holds no `cases.csv`. A tree that holds one is held to its `ok` and `wrong` rows instead:
`h200/` is held to `h200/cases.csv`, the list it was measured from. Every device list parses.
Every file lies at
`<device>/<dataset>.sf<sf>/<mode>.benchmark.txt` under its device's slug, its timed trailers ending
with the five provenance keys and a failure's record holding its status line and the five. Only
`h200/`, from before provenance, may have timed trailers without them. Over synthesized trees: its
sections under another device's name are refused, and are short of the list there; a tree with
its own list answers to its rows, a `skipped` row declaring no section; a file short of
one declared section is named; a trailer naming another card than its directory is refused, and one
naming its own passes; a failure's record without the five is refused even under `h200/`. Over a
temp file, a run's write path: a case that panics is recorded on one line and the next case is
measured into the same file; a resumed run keeps the section this run's five measured, byte for
byte and without running the case, and runs again one measured under another cuDF, one that failed
and one never reached; a run that does not resume measures a section with this run's five again;
the rows go in before the section, and rows the record refuses fail the case.
`PEACOCK_BENCH_RESUME` is unset, `0` or `1`, and anything else is refused. Before any case, a
missing dataset is refused, and a record another run wrote. The record, which lives in S3, is held
to its writer instead: a row that lost a cell is refused, a bare `calls` row names the seq it was
handed, and a record the harness's own writer produces carries `record_header()`'s preamble, read
as its readers read it. The enabled-on-a-device check and the section check compile the case
lists in, through the binary's own `corpus_query_benchmark!` and `corpus_query!`, and so do two
more: every schema-validation mask
entry is one of its own line's device modes, and a mask entry for a cell its line never runs is
refused as dead. The decoder is asked only about a mode a run reached, so an entry for a cell that
is off is never decoded and would arm itself when that cell is enabled

| Cost-model goldens | [cost_goldens_match_and_categories_hold_every_byte](../peacockdb-core/tests/test_cost_model.rs) | 3 |
|---|---|--:|

`.cost.txt` derivation from `.cpu.txt` × `cost_model.conf`

*crate integration, internal*

| End to end | [tests::end_to_end](../peacockdb-core/src/tests/end_to_end.rs), with `limits`, `dimensions`, `accounting` and `schema_validation` beneath it | 44 |
|---|---|--:|

SQL in, rows out: 18 queries planned and run at all five modes against DataFusion on the same
SQL, eleven of them also at injected layouts no planner would emit, plus `in_flight_bytes` back
to zero and holds equal releases at the end of every run — and twenty-four cases no query list
can carry: that DataFusion's partial aggregate does not skip grouping here, that a relation
DataFusion proved empty, planned alone, answers one zero-row batch that no node emitted at every mode, the call and pull
counts a limit makes, the smallest budget a query fits in completing where the byte below it
trips, and that boundary under a drained lane, the model compared against what the calls
measured, an answer under the wrong column names not being the same answer, the injected set
keeping the shapes only one query has, and a degenerate hash under a Right outer and under a
RightAnti answering like the oracle from the empty build lanes it leaves
([#175](archive/archived-tickets.md#t175)), and the schema validator as the driver's output
hook — `tpch/q6` at every mode passing under it, an index over the same tree with one
project's field retyped refused naming the field, and the same tree with a field declared
non-nullable over a column holding NULLs refused naming the nulls and their count
([#227](archive/archived-tickets.md#t227)); a scan's limit answering its count from one row group
at every mode, a limited subquery under an aggregate, and a limit stopping a scan whose mapping
offers every row group it has; and five on the DISTINCT lowering — a
`count(DISTINCT)` beside an `avg` and a `count`, a grouped `count` and `sum` DISTINCT beside
companions, a DISTINCT argument holding NULLs, an empty keyless input answering `0, 0`, and
`tpch/distinct-functions` against a hand-lowered oracle through `sql_answers_match_oracle`;
and two on an answer with no rows — an ordered query answering zero rows under its one column at
every mode, and the empty sort's and merge's own emissions read off the run, which the answer
cannot show; and an aggregate beside a full outer join over an input no row survives, and a count
over a scan no row group survives answering 0. Two of the 44 are `#[ignore]`d against
[#182](tickets/memory.md#t182) — the budget boundary and the rebatcher's peak, both
properties that pricing a batch from the plan's schema took away — so 42 run. The first tier
where the planner, the recipes, the executors and both drivers run together rather than each
against a fixture of the last one's shape — so what it tests is the joins between them

| Harness helpers | [tests::compare](../peacockdb-core/src/tests/compare.rs), [tests::synthetic](../peacockdb-core/src/tests/synthetic.rs) | 22 |
|---|---|--:|

the synthetic batch and the comparator against themselves, with no engine: `synthetic(rows,
seed)` deterministic from its seed, null in every column but the id, dyadic in every float, legal at zero
rows; `assert_same` red on a differing value, a differing type, a differing row count, and a
slot one side left empty against a zero-row batch on the other — the failures the operator
harness rests on, each shown for the reason it names; and `same_within_welford`, the one
inexact comparison, red on a renamed and on a retyped column before it reads a value

*component*

| Plan rules, hand-built | [a_limit_over_several_lanes_names_the_node_that_fixes_it](../peacockdb-core/src/plan/tests/mod.rs) | 32 |
|---|---|--:|

one input per rule, each built to break the rule it is aimed at: a plan that violates one is
unreachable from SQL because translation inserts the fix, so a hand-built input is the only
thing that shows the guard going red

| Join declarations | [a_join_whose_sides_carry_different_lane_counts_is_refused](../peacockdb-core/src/plan/tests/joins.rs) | 25 |
|---|---|--:|

what each of the three join kinds requires of its inputs, and the distribution a join declares
about its own output — the claim nothing downstream re-checks; and which keys a semi join's
session takes distinct, which a conjunct reading both sides or neither takes away

| Aggregate state columns | [plan::tests::aggregate](../peacockdb-core/src/plan/tests/aggregate.rs) | 7 |
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

every hash join type crossed with a residual filter and every nested-loop type, each planning
with its probe streamed; the co-partitioning and lane rules; anti and mark joins planning over
NULL keys, and a padded side's NULL seen by #137's filter; and the session config's own
registration path declaring strings `Utf8`; writes its own parquet, so no dataset

| Null analysis rules | [a_scalar_function_can_be_null_even_over_operands_that_cannot](../peacockdb-core/src/planner/tests/null_analysis.rs) | 12 |
|---|---|--:|

every rule in the can-this-column-be-NULL pass, on hand-built nodes — a source declares a
not-nullable column here, which no corpus fixture can

| Logical-plan nullability | [a_key_whose_row_groups_hold_no_null_is_not_nullable](../peacockdb-core/src/planner/tests/logical_nullability.rs) | 5 |
|---|---|--:|

whether a column of a DataFusion logical plan can hold a NULL, traced to the parquet footers —
what the `NOT IN` rewrite and #137's key filters ask before a physical plan exists. Against real
files, since every corpus column is declared nullable; the tpcds case is the only one whose column
really holds NULLs

| `NOT IN` as SQL means it | [correlated_not_in_answers_as_sql](../peacockdb-core/src/planner/tests/not_in.rs) | 17 |
|---|---|--:|

the `NOT IN` rewrite, over two in-memory tables whose columns the footer reader cannot reach, so
every operand counts as possibly-NULL and every form rewrites: the four scratch probe shapes, the
three negation-normal-form folds, the nested `NOT IN` inside an `EXISTS`, the uncorrelated count
that must not become `count(*)`, the tpch keys that hold no NULL and so plan untouched, and the
refusals this chain leaves — #250's off-spine `IN`, #247's seven DataFusion limits, and #267's
count over pbench's four-lane `sub` at tp4. Every expected row is DuckDB 1.5.4's over the same
tables

| #137's NULL-key filters | [an_inner_join_drops_null_keys_below_both_shuffles_where_the_data_holds_them](../peacockdb-core/src/planner/tests/null_key_filters.rs) | 7 |
|---|---|--:|

which side of a join drops its NULL keys before the shuffle, read off the plan text over real
footers — pbench's `f_k` holds 1,034 NULLs and `d_k` 50, where every tpch scan key holds none, so
the tpch case is what proves the footer reader is consulted at all. Three answers a query can show
(Inner both sides, Left its probe, Full neither), one lane showing that no shuffle means no filter,
and two over the hand-built fixture for what no corpus SQL reaches: the `null_equals_null` flag,
and whether the filter landed below the shuffle

| Planner join refusals | [planner::tests::join_refusals](../peacockdb-core/src/planner/tests/join_refusals.rs) | 12 |
|---|---|--:|

every shape the planner refuses, from the SQL that provokes it; each asserts its ticket is in
the message a user sees. Three are `bug_` tests rather than refusals the engine means to keep:
two DISTINCT arguments ([#144](tickets/complete-coverage.md#t144)), a `stddev` companion beside
a DISTINCT ([#261](tickets/complete-coverage.md#t261)), and one `Float32` argument that reaches
#144's refusal because `keeps_distinct` admits `Float64` from an integer and not from a
`Float32`, so widening that list turns the test red. Five once refused, each now run on the cpu against DataFusion's own
answer: a residual on Left, Right and Full joins and on the right-handed semi pair, anti and
mark joins over NULL keys, and a nested loop of each of seven types

| `IS NOT DISTINCT FROM` as a join key | [planner::tests::distinct_from_keys](../peacockdb-core/src/planner/tests/distinct_from_keys.rs) | 4 |
|---|---|--:|

a nested loop whose condition holds `IS NOT DISTINCT FROM` over one column of each side plans as
a hash join. That conjunct is its key, under `null_equals_null`; any other conjunct is its
residual. Full, Inner with the probe's column first, semi and anti each answer as DataFusion. Two
shapes stay as they were: with `=` beside it, DataFusion keys the join itself under the SQL
default; and `IS DISTINCT FROM`, or an expression operand, keeps the loop. Over the hand-built
fixture

| A scan of nothing | [planner::tests::empty_scan](../peacockdb-core/src/planner/tests/empty_scan.rs) | 2 |
|---|---|--:|

a scan no row group survives — every one pruned, or a file with none — plans one lane holding no
batch at every mode, and a scan with survivors maps as before ([#282](archive/archived-tickets.md#t282))

| Recipe patterns | [only_an_aggregate_init_calls_at_done_if_nothing_went_out_and_every_init_does](../peacockdb-core/src/planner/tests/recipe_patterns.rs) | 1 |
|---|---|--:|

across the corpus plans, `AtDoneIfNothingOut` belongs to every aggregate init and to nothing else

| Plan goldens, tp1-single | [tpch_tp1_single](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 3 |
|---|---|--:|

1 lane, one batch per chunk — the mode every other is read against; plan tree + `--- recipes
---` + `--- memory ---` per query, one file per bench

| Plan goldens, tp1-rowgroup | [tpch_tp1_rowgroup](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 3 |
|---|---|--:|

1 lane, one batch per row group: the finest the mapping expresses, and no budget

| Plan goldens, tp4-single | [tpch_tp4_single](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 3 |
|---|---|--:|

4 lanes, one batch per chunk — the shuffle shapes with batching inert

| Plan goldens, tp4-rowgroup | [tpch_tp4_rowgroup](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 3 |
|---|---|--:|

4 lanes at row-group granularity: lanes and many batches at once

| Plan goldens, tp4-sized | [tpch_tp4_sized](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 3 |
|---|---|--:|

4 lanes, the estimator's target — **the only mode a budget tier moves**, recorded in-band

| pbench's plan shapes | [every_pbench_join_plans_as_its_spec_says](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 2 |
|---|---|--:|

the two claims pbench's data exists to make, read off `tp4-single.plans.txt`: every pbench join
plans as the shape its spec names, and the three ways a join collapses four lanes to one. A plan
golden records what the planner did; these two say what it was supposed to do, which is what makes
a pbench golden's diff readable by somebody who did not write the query

| Plan goldens, meta | [the_registry_matches_the_goldens_in_both_directions](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 3 |
|---|---|--:|

the registry and the goldens agree both ways; every mode has a golden and every golden a mode;
every refusal in a golden carries no host path. That each refusal names a ticket that exists is
cost-report's `every_refusal_names_a_ticket_that_exists`, since it reads the wiki

| Join projection names | [every_join_projection_in_a_golden_names_the_column_its_ordinal_selects](../peacockdb-core/src/planner/tests/join_projection_names.rs) | 1 |
|---|---|--:|

every `name@ordinal` in a join's projection, over the ten goldens, names the column the ordinal
selects from what that join type emits — rebuilt from the children's schemas, not from the
renderer — and every join type the corpus projects is among them

| Recipe plan structure | [every_published_seq_addresses_the_kind_its_recipe_claims](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 4 |
|---|---|--:|

the claims the payload golden cannot make: every published seq resolves to the kind its recipe
names, over every corpus query rather than only the payload subset; the payload set covers
every fb kind and call shape the fifteen goldens hold; the queries that cannot cross the wire are
declared; no plan approaches the verifier's depth cap ([#169](tickets/system-hardening.md#t169)); and the
index's post-order agrees with the numbering `attach_recipes` gave, over the corpus, since the
two are separate walks in separate files and [#134](archive/archived-tickets.md#t134) is the same pair one
boundary over

| Recipe payloads golden | [the_payload_golden_carries_what_each_call_hands_the_executor](../peacockdb-core/src/planner/tests/plan_goldens.rs) | 1 |
|---|---|--:|

the payload subset at tp4-rowgroup — chosen as a cover over every fb kind and call shape the
mode goldens hold, and asserted to be one — plus `nested-limits`, for `CudfScan.rows_only` — so the membership grows when the mapping does; pbench's `empty-side-left-join` is the one whose `GpuEmpty` holds its slot with a stub scan — with
every payload rendered and a sha256 over the bytes beside it

| Recipes per join type | [every_join_node_writes_one_cudf_join_leaf](../peacockdb-core/src/wire/tests.rs) | 31 |
|---|---|--:|

the kinds whose recipe is more than one call, the joins first: the one `CudfJoin` each join
node writes, per join type, and the session line it renders — build once, probe per batch,
finish at done; plus a leaf outside the
registry emitting the writer's stub and no recipe, which is how a hand-built node gets its
recipe with no plan, and an empty relation doing the same, so a plan of it alone has a root.
`chunk_bytes`: a budgeted mode's sixteenth capped at `JoinLimits.DefaultChunkBytes`, an
unbudgeted mode writing that default, the enum and the field default agreeing, and the plan text
printing only a value that is not the default; an interval literal printing as its three parts.
Three plan pbench SQL to the writer: a date plus an interval literal crosses, and one inside a
CASE operand or as `date_bin`'s argument is refused naming #249. The trivial kinds are not here — the plan goldens run them over every
corpus query

| Plan text | [every_column_reference_renders_name_at_ordinal](../peacockdb-core/src/plan_text/tests.rs) | 15 |
|---|---|--:|

the renderer against what the planner emits from real SQL: every column reference prints name
at ordinal, every node carrying a fetch prints it, a join prints its keys and projection by
name — a mark join's mark among them — a null decimal prints as `NULL`, a source prints its
mapping verbatim, a name that is not a token is backquoted, and no node name carries an exec
suffix

| One driver, two backends | [one_generic_driver_serves_two_backends_with_different_batch_types](../peacockdb-core/src/executor/tests.rs) | 1 |
|---|---|--:|

one source step written once over `Backend` and driven against two backends with different
batch types, so the trait's associated types are exercised the way both engines use them

*subcomponent*

| Drivers over a mock backend | [executor::driver::tests](../peacockdb-core/src/executor/driver/tests/mod.rs) | 123 |
|---|---|--:|

flow, backpressure, limits and accounting, asserted on calls rather than rows — pull counts,
queue bounds, batch release, the trace, the output hook (called once per batch a node queues
as its own and not at a forwarder, a refusal failing the run at that node and lane with the
hook's words, and `None` leaving the report as `run` makes it): the schedule and the two holds, both limit lowerings by
the calls not made and a mid-plan limit judged by the rows it emitted, never its input, what
each node emitted and consumed (the two records the corpus goldens
read), the accountant through the drivers, a backend failure stopping the query with the
accounting still reconciling, the execution golden's text with every number chosen by the
script, every empty scatter lane dropped while a lane that owes its probe reaches `SetBuild` with no
batch, a zero-row build draining like none, and the mock against its own
script; then the measurement side: the ABI-call record indexed by the lanes a node is driven on,
holding the backend's calls and not the driver's own, a backend that names no seq leaving every
entry unmeasured, the timing tree rendered with what each node cost, a call without a region and
a region without a call each refused, a region the clock rounded to nothing still counted as
ran, and the recorded node index the post-order rather than the walk order; and a sink that
received nothing answering one zero-row batch under its input's columns, which no node emitted

| CPU backend executors | [executor::cpu_backend::tests](../peacockdb-core/src/executor/cpu_backend/tests/mod.rs) | 92 |
|---|---|--:|

one hand-built node per executor, one hand-written expected result: the exec executors, the
accumulators over state batches written down rather than produced, the loader over parquet the
test writes, a read of no column answered as `__rowmarker__` — which relabels the reader's batch to the declared schema and refuses a column
the file holds in another type rather than casting it — the scatter, the join capability
matrix run per mode, what `executors_for` builds and reports holding, every
decomposition's init emitting exactly the state `PlanAgg::state_type` declares, an init
declaring a narrower decimal than it produces refused, a merge over a decimal sum
DataFusion widens still building, and an accumulating sort or merge over zero-row batches
answering one of zero rows where a lane that received nothing answers nothing. Then `declared_as` against its declaration: #227's NULL in a
column declared without one refused by name and count, the rule itself over the columns, and a
batch of another width refused for the width — at a differing count column *i* is not field
*i*'s column, so a violation read off that pairing would name a NULL the batch does not hold
there and hide the count that explains it. Then the join stream: one batch per call however DataFusion
chunks it, nothing per probe for the build-side semi family, a zero-row probe answered by a zero-row
batch, the nested-loop and cross projections, the cross join over a zero-row build, and a build
side with no rows — no batch or an empty one — joined as an empty table, `owes_nothing` only for
the types that owe no probe row. And `null_equals_null` for all nine types, a NULL key on each
side matching under `true` and nothing under `false`

| Executor contract, both engines | [executor::cpu_backend::tests::contract](../peacockdb-core/src/executor/cpu_backend/tests/contract.rs) | 1 |
|---|---|--:|

one table of input, calls and expected answer
([`executor_cases.rs`](../peacockdb-core/src/tests/executor_cases.rs)) run by the CPU backend
here and by the device in `gpu_backend::gpu_tests::contract`, because a table one side does not
read proves that side twice. Twelve rows: filter, project, the lane sorted with and without a
fetch, coalesce, a merge with and without its finalize, a merge over state whose keys carry a
grouping id, a two-key rollup whose id is DataFusion's in value and type, and the scatter at 4 lanes and at 64 — the lane each key lands in is a golden,
since co-partitioning is what every partitioned join rests on

| Translator, one rule at a time | [planner::translator::tests](../peacockdb-core/src/planner/translator/tests.rs) | 50 |
|---|---|--:|

one test per node kind, per expression kind and per planner rule, each from the smallest plan
that shows it — the corpus goldens are a regression net over whole plans and a different
question from whether a rule is right

| Schema annotations | [planner::translator::schema_tests](../peacockdb-core/src/planner/translator/schema_tests.rs) | 10 |
|---|---|--:|

what every node declares about its columns: the arrow types, and the annotations a merging or
finalizing node reads

*module unit*

| Driver internals | [executor::driver::scheduler::tests](../peacockdb-core/src/executor/driver/scheduler/tests.rs) | 43 |
|---|---|--:|

the accountant's formula, cache and two checks on plain figures; the plan index's numbering
and per-lane slots; the scheduler's corners
enumerated and then a differential test against a naive rescan on randomized shapes; the lane
state machine one call at a time with no tree around it

| Rollup's shuffle keys, and DISTINCT reaching decompose | [planner::translator::aggregate::tests](../peacockdb-core/src/planner/translator/aggregate/tests.rs) | 4 |
|---|---|--:|

a grouping-set aggregate's shuffle hashes the user keys and not the id, which is
[#189](archive/archived-tickets.md#t189): the subset rule, `hashKeys ⊆ group columns`, makes
hashing the id unnecessary. Plus `decompose` handed a distinct `AggregateFunctionExpr` directly: no SQL reaches it, since the classifier lowers or
refuses every DISTINCT it is shown, so this is the net under it for a shape it is not shown, which
would otherwise run as non-distinct on both engines where the cpu-vs-device comparison cannot see it

| Arrow to the wire enum | [wire::serialize::tests](../peacockdb-core/src/wire/serialize/tests.rs) | 3 |
|---|---|--:|

which fb `DataType` member each Arrow type maps to, over strings and one built buffer: every
timestamp unit crosses as its own member, zoned or naive, and the name a payload golden would
print is read back off the buffer. One `bug_`: a schema holding an `Interval` is refused at plan
time naming the column, since the wire's interval types a literal and never a column
([#249](tickets/complete-coverage.md#t249))

| The lane rule's own properties | [executor::cpu_backend::spark_partitioning::tests](../peacockdb-core/src/executor/cpu_backend/spark_partitioning/tests.rs) | 1 |
|---|---|--:|

which rows the rule puts together, with no device: every NaN shares a lane whatever its sign or
payload, and so do `0.0` and `-0.0`. comet hashes a float by its bits, so this is the
canonicalization and not comet's own rule — without it `NaN` and `-NaN` split at tp4 while the
device equates them at tp1. That the *kernel* follows the rule is the murmur gate's, on the
device rung; this is the half that needs no card

| Aggregate state types | [plan::aggregates::tests](../peacockdb-core/src/plan/aggregates/tests.rs) | 10 |
|---|---|--:|

`state_type`'s table against the accumulators it was read off: each arm's DataFusion
accumulator over four rows produces the type the table declares, `Sum`'s decimal rule and its
refusal of a string, the walk over every decomposition that keeps `MergeM2` out of every state
slice, and the `avg` finalize's shape — the sum divided at its own scale, the quotient cast to
the declared output

| Declared precisions | [common::tests](../peacockdb-core/src/common/tests.rs) | 1 |
|---|---|--:|

what the export is told per column: a `Decimal128(p, _)` its `p`, every other column `0`

| Record steps | [test_support::record::tests](../peacockdb-core/src/test_support/record/tests.rs) | 4 |
|---|---|--:|

the record's reading of a plan: a row's `node_seq` names the steps its `recipes` line prints,
so the record and the plan golden agree on what a call is; every cell of a written row sits
under the column named for it, since `row()` is positional and the heading is not; and the
heading's `# run:` lines end with the five provenance keys, `device=` to `host=`, in order; and
an append that stops partway leaves the file as it was, so no cut-off row is continued

| Benchmark provenance | [test_support::provenance::tests](../peacockdb-core/src/test_support/provenance/tests.rs) | 4 |
|---|---|--:|

the directory a device's results go to, over `testdata/device_slugs.tsv`, the table
`scripts/calibration/record.py`'s copy is held to as well: `NVIDIA H100 80GB HBM3` is `h100-80gb-hbm3`, and no
name gives a slug with a `/` or a space, or one that slugs to something else. The runtime's
`1000 * major + 10 * minor` read as `12.8`. The driver's release out of
`/proc/driver/nvidia/version` in both module flavours' wording, nothing out of a text with no
module line, two, or one naming no release or two, and an unreadable file refused naming it

| Benchmark trailer | [test_support::benchmark_trailer::tests](../peacockdb-core/src/test_support/benchmark_trailer/tests.rs) | 1 |
|---|---|--:|

the `--- run ---` trailer a section ends with: its keys in order, the five provenance keys
last, each with the run's own value

| Device case lists | [test_support::device_cases::tests](../peacockdb-core/src/test_support/device_cases/tests.rs) | 11 |
|---|---|--:|

a device's `cases.csv`, through its one reader and writer: a list round-trips, and a note keeps
its commas. A skipped row names a reason and no mode. An `ok` or `wrong` row names its modes. A
`wrong` row's note is its `#NN` ticket, and an `ok` row has none. Refused, naming the line: a
wrong header, an unknown mode or status, a row short of six fields, and a case listed twice

| Golden merge | [test_support::corpus_golden::tests](../peacockdb-core/src/test_support/corpus_golden/tests.rs) | 9 |
|---|---|--:|

sixteen writers merging one golden file at once each keep their section: the lock is on the
directory, because the rename that publishes replaces the file's inode (#213). And the
mode-keyed merge `gpu-result.txt` takes: one cell replaced in an ordered file, and a section
dropped once the registry stops enabling its cell — what makes the coverage guard's advice to
regenerate true in both directions. Then the `cudf=` provenance line: written first, dropping
whatever another cuDF recorded rather than carrying it under this one's stamp; read back against
the path, both ways; refused where two such lines disagree; and the same rule at the writer, so a
26.02 cycle told to record the committed file fails before it writes

| DuckDB oracle | [test_support::duckdb_oracle::tests](../peacockdb-core/src/test_support/duckdb_oracle/tests.rs) | 47 |
|---|---|--:|

the grammar parsed — each order and value, `rel` taking a column name with spaces, dots and
parentheses, a position refused, the old spellings refused — and what each spelling accepts:
under `sort` rows as a multiset by position whatever the names, under `no_sort` row for row,
failing on two swapped rows; a float held to its relative tolerance and a decimal to one unit in
our last place; `rel` passing inside its tolerance, failing just outside it on a named column and
on an unnamed one at `approx`; `date` comparing DuckDB's timestamp by its date part, failing on
another day and pairing rows by the date under `sort`; a column name the header lacks, or holds
twice, failing by name; a fingerprinted section and the fingerprint oracle each refusing the
other, and `duckdb_none` refusing two sections that both exist. Then `gpu: fingerprint`: parsed
last and with `fingerprint` only, refused on the two bare oracles; a fingerprinted device section
compared under it and failing without it, a device section of rows failing under it, and the
cpu's fingerprinted section still refused on its line. Plus the
`gpu-result.txt` coverage rule both ways, over a file handed in as text: a cell with no section
and a section no cell enables each failing, and an absent file passing where no cell is
enabled

| Result comparison | [test_support::result_text::tests](../peacockdb-core/src/test_support/result_text/tests.rs) | 9 |
|---|---|--:|

the two arms a corpus case compares its answer with, a `data_fusion_subset` line reaching
neither: the tolerant one, over parsed rows, accepting a float a reassociated sum would move and
failing past its tolerance, on a missing row, a duplicated one and one the oracle does not have;
the exact one, a digest, failing on the drift the tolerant one accepts; a NaN against a number a
difference under both; the panicking wrapper still naming its query; and a `no_sort` rendering
keeping the batches' row order

| Device answer comparison | [test_support::device_answer::tests](../peacockdb-core/src/test_support/device_answer/tests.rs) | 14 |
|---|---|--:|

the same for the device helper, ungated so it runs without a card: a wrong row and a missing one
failing under either golden oracle, a digit held to the tolerance under `approx_std`, a
NaN against a number a difference, `live_cpu` not being a section comparison at all, an unknown
spelling naming the accepted set, both axes decoded through `stringify!`'s spacing,
`golden(sort, …)` sorting a section written in engine order and `golden(no_sort, …)` failing on
two swapped rows, an empty `PCK_WRITE_GPU_RESULT` reading as "record nothing", and
cuDF's own `version_config.hpp` read as `25.02` — the minor padded to two digits, which is the
spelling every path in the tree uses rather than as a version suffix; and the same header read
as the release `25.02.02` a benchmark's `cudf=` names, the patch padded too

| Harness datasets | [test_support::tests](../peacockdb-core/src/test_support/tests.rs) | 4 |
|---|---|--:|

the three datasets the corpus knows, held to the tree: each has its data, its queries and its
goldens, the list is exactly three, and pbench is the one the small-table rule is off for — the
knob `small_table_bytes_for` sets, without which every pbench scan would plan as one lane and the
dataset would demonstrate nothing. Then pbench's data itself, read off the committed parquet:
`dim` carries a key column of every type `fact` does, name and arrow type both, so a regeneration
that drops one or writes a timestamp at the wrong unit goes red here rather than at the query
somebody writes a year later

| Source rewrite | [test_support::testdata::tests](../peacockdb-core/src/test_support/testdata/tests.rs) | 1 |
|---|---|--:|

the one write into the checkout, `rewrite_source_file`, which `UPDATE_CANONICAL` uses to rewrite a
compiled-in list: the new text replaces the file whole, and the staged `<file>.new` is gone after
the rename. Nothing hands out a path into the checkout, so no test can read source through it

| Corpus helper | [test_support::corpus::tests](../peacockdb-core/src/test_support/corpus/tests.rs) | 10 |
|---|---|--:|

the cpu helper shown failing, which is what keeps the corpus honest: a wrong row, a missing row,
a digit past the tolerance, a section another mode wrote, and under `no_sort` a section holding
the right rows in the wrong order; plus every `CpuOracle` and `GpuResultMode` variant being named
by some registry row, `data_fusion` decoded through `stringify!`'s spacing, and
`data_fusion(no_sort, …)` refused; and the line's one row order, a `duckdb_none` line's being its
golden's

| Registry CSV split | [test_support::registry::tests](../peacockdb-core/src/test_support/registry/tests.rs) | 9 |
|---|---|--:|

a quoted registry cell holding commas, parentheses and doubled quotes reads as one field, the
same rule cost-report, `ticket_widget.py` and `duckdb_result.py` read the file by. Then the
`stored` column: a `skipped:` or `failed:` marker read as no section, each shape of cell from
the sides' formats, a side with no section left out of the one-word rule, a query with no
section refused, a wrong cell naming the side that differs, a line declaring `gpu: fingerprint`
whose cell stores no fingerprint on the device's side, and the column's spelling

| Order hazard | [test_support::order_hazard::tests](../peacockdb-core/src/test_support/order_hazard/tests.rs) | 18 |
|---|---|--:|

what the order tier refuses, over two four-row tables: a limit over a hash join, over an
aggregate over one, and over a cross join fed by one; a sorted cut inside a tie that holds rows
that differ, at the fetch and at the offset — the whole tie read, past the row after the cut,
and an offset read through a projection to a sort with no fetch. And what it lets through: a
limit over a cross join of scans, over no join, over a keyless aggregate; a cut between rows
that do not tie, or inside a tie of one row repeated; a tie over no join. Plus the order
guard's predicate: a plan ends in a sort through a projection, and through a limit an offset
keeps above the sort (a bare `LIMIT` folds into the sort's fetch), and a `GROUP BY` does not, and a line declaring `sort` over a sorted plan, or `no_sort` over an unsorted one, fails

| Sink divergence message | [executor::errors::tests](../peacockdb-core/src/executor/errors/tests.rs) | 3 |
|---|---|--:|

what the sink says when the device's schema is not the declared one: every diverging column
named with its index and both types, so a year and a narrow decimal read as two findings
rather than `try_new`'s first; nullability is never a clause, since `try_new` does not check it

| Schema validator null counts | [test_support::schema_validation::tests](../peacockdb-core/src/test_support/schema_validation/tests.rs) | 3 |
|---|---|--:|

a NULL in a column the node declares non-nullable, which neither validator saw before: the cpu
flavour hands the comparison real counts off the arrow batch, the device flavour says `Unread`
because nothing exports the device's counts yet — cuDF holds them, the schema message does not
carry them. Half of [#227](archive/archived-tickets.md#t227), which carries the two routes to the
other half

| Device schema projection | [test_support::device_schema::tests](../peacockdb-core/src/test_support/device_schema/tests.rs) | 16 |
|---|---|--:|

an arrow schema projected onto what cuDF stores — a `type_id` and a decimal's scale, cuDF's
own interop table for the types the wire admits, a type outside it a panic by name — and the
comparator over it: every diverging column in the sink's spelling, a renamed one and a width
mismatch each a finding, precision and nullability never one; the schema-only IPC stream
`peacock_handle_schema` answers, decoded with no device

| Schema validation, declared | [test_support::schema_validation::tests](../peacockdb-core/src/test_support/schema_validation/tests.rs) | 6 |
|---|---|--:|

the last `corpus_query!` argument decoded against the mode being run: the hook on at every
mode, off at every mode, and off at only the modes a `schema_validation_disabled(…)` mask
names — plus the three refusals that keep a mask honest, since a misspelled keyword, a mask on
the enabled form and a mask entry that is not one of the five would each read as a legal line
and run a cell unvalidated

| Forwarders and row ranges | [interleave_serves_lane_p_from_lane_p_of_every_child](../peacockdb-core/src/executor/forwarder/tests.rs) | 4 |
|---|---|--:|

which lanes a merge, a union and an interleave serve from which child; and the row-range clamp
over every line of `testdata/fixtures/row-range-clamp.txt`, the case table C++'s
`clamp_row_range` test reads too, so a drift between the two clamps fails the drifting side

| Expression round trip | [executor::cpu_backend::expr_physical::tests](../peacockdb-core/src/executor/cpu_backend/expr_physical/tests.rs) | 13 |
|---|---|--:|

DataFusion's expression, this engine's, and DataFusion's again must all read the same column
out of the same rows — asserted on the array each produces, not on shape

| Plan types | [plan::validate::tests](../peacockdb-core/src/plan/validate/tests.rs) | 43 |
|---|---|--:|

validation's shape rules — a plan ends in a crossing, a sink sits at the root, a limit has a
real consumer, no node declares no column ([#63](archive/archived-tickets.md#t63)), no schema — an aggregate's intermediate included — literal, cast or function
names a view type the device cannot hold; a pass-through
node declares its input's column count and its fields;
layout canonical forms and equality; whether a hash survives a regrouping

| Memory estimation | [planner::memory_estimation::tests](../peacockdb-core/src/planner/memory_estimation/tests.rs) | 14 |
|---|---|--:|

the model's rules from SQL over the minimal dataset: an accumulator ends the walk and what it
holds comes off the budget first, a batch is charged once per lane in force, a loader is priced
by the batches its mapping makes, amplification is the widest point on the path, a target lands
on the coarse grid, constants that cannot be less and exceed the budget are a plan-time
error, a join that answers at the finish — hash or nested loop — is charged its `matched` byte a
build row, a RightSemi its distinct build keys, and an empty relation is no source the budget sizes

| Translator expressions and scan mapping | [planner::translator::expr::tests](../peacockdb-core/src/planner/translator/expr/tests.rs), [scan_mapping::partition::tests](../peacockdb-core/src/planner/translator/scan_mapping/partition/tests.rs) | 30 |
|---|---|--:|

a column keeps its ordinal and name, a literal DataFusion's scalar, a binary op its declared
type, the four unaries one for one; from parquet metadata, rows and bytes are the surviving row
groups' own totals over the projected columns, a scan over several files is refused, a declared
schema that renames or reorders a column the scan reads is refused naming the ordinal and both
names, and the lanes deal the survivors as the mapping says

| Expression text | [plan_text::expr_text::tests](../peacockdb-core/src/plan_text/expr_text/tests.rs) | 4 |
|---|---|--:|

an interval prints the parts that are not zero, a decimal as a value and a null one as `NULL`,
every form readably

| Expression writer | [wire::expr_writer::tests](../peacockdb-core/src/wire/expr_writer/tests.rs) | 21 |
|---|---|--:|

every variant, every operator, and the literals the corpus actually produces — a wrong scalar
is invisible in plan text and wrong on a device. Each of DataFusion's three interval literals
crosses as months, days and nanoseconds, a NULL one as a typed null, and an interval anywhere but
a date or naive timestamp ± it is refused naming #249

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

#### gpu — `--features gpu`: shad-gpu only. 2409 cases: `--lib -- gpu_tests::` 740, `test_gpu_corpus` 1009, `peacock_gpu_benchmarks` 659, `test_node_timing` 1

*crate integration, external*

| Corpus, device | [test_gpu_corpus](../peacockdb-core/tests/test_gpu_corpus.rs) | 1008 |
|---|---|--:|

the same `corpus_query!` lines read from the other side: each enabled (query, mode) runs on a
device with every batch held to its node's declared schema through the driver's output hook
(the line's `schema_validation_enabled`, which every enabled cell now carries),
and asserts,
read-only, against the section the cpu authored — plan shape, `in_rows`, the per-batch lists
and the bytes — plus the result where `gpu_oracle` names a golden.
1007 cells today. From tpch, 240: every line at every mode. From tpcds, 410: every line wherever it runs on the cpu.
`q78` compares within 1e-11 relative, since a
float `round(x, 2)` on the device is one ulp from DataFusion's in some cells
([#60](archive/archived-tickets.md#t60), won't fix). `q2` does too, and those cells push its
recording over the cap, so its DuckDB check on the device's side is `gpu: fingerprint`. tpcds is where #137's NULL-key filters run on
a device, at every tp4 cell. From pbench, 357: every line wherever it runs on the cpu, but for
two. `like-column-pattern` is off on [#246](tickets/joins.md#t246), a `LIKE` whose pattern is a
column, and `timestamp-s-key-group` on [#264](tickets/complete-coverage.md#t264), a cast group key.
pbench is where the device meets NULL
join keys on both sides, an empty build lane under Right, Full and RightAnti, every join type
with a residual, every nested-loop type, and `NOT IN` after the rewrite. Its six `sort-*` queries
sort a nullable key descending, nulls at either end, through the merge at tp4. The 1008th case is that a
device run under a regeneration writes no golden

| Registry ↔ CSV, device | [the_registry_matches_the_gpu_corpus_in_both_directions](../peacockdb-core/tests/test_gpu_corpus.rs) | 1 |
|---|---|--:|

the gpu column of the registry, the other half of the pair

| Corpus benchmark harness | [peacock_gpu_benchmarks](../peacockdb-core/tests/peacock_gpu_benchmarks.rs) | 659 |
|---|---|--:|

the nine assertions the harness makes about its own output and its run. They run on every
`gpu-tests` job under `--skip bench_`: a mode's results go to one file per (device, dataset, mode), sections ordered
numerically, a filtered run keeping the sections it did not produce, every declared mode naming
the queries its file will hold, the record written only when `PEACOCK_RECORD_PATH` names it and
checked against what the plan declares before a row lands, an append under a different `# run:`
heading refused, and `PEACOCK_BENCHMARK_CAPTURE` naming neither pass refused. The ninth reads this
run's provenance the way a `bench_` case does. The device slugs to a clean directory name, the
driver is a release, CUDA is 11.0 or later, cuDF is the release the binary was built against, and
the host is named. The 650
`bench_` cases — the case list in `tests/common/corpus_benchmark_cases.inc`, written from the
registry — are the sf40 measurement itself and run only under `--run-benchmarks` (*Corpus
benchmarks*, below)

| GPU timing method | [events_are_free_and_land_where_they_claim](../peacockdb-core/tests/test_node_timing.rs) | 1 |
|---|---|--:|

the instrument's structural test, one sf1 query at one mode: `Off` records nothing, `Events`
records one region per recorded call and partition, Σ `device_us` is above zero and at most
the wall, and both modes make the same calls. No percentage ceiling on the instrument's cost —
a bound loose enough never to flake on a shared host proves nothing — so the effect is reported
from the sf40 run instead

*crate integration, internal*

| Operator harness | [an_unload_hands_the_whole_batch_over_on_both_backends](../peacockdb-core/src/tests/gpu_tests/harness_cases.rs), [a_descending_key_with_nulls_last_agrees](../peacockdb-core/src/tests/gpu_tests/exec_cases.rs), [every_kind_has_a_case_or_is_a_forwarder](../peacockdb-core/src/tests/gpu_tests/coverage.rs) | 476 |
|---|---|--:|

one hand-built node over stub leaves, a script of synthetic batches, both backends through
`executors_for`, the outputs compared slot by slot and exactly — schema included, so a type the
device changes is red here rather than at a query's root. The upload round trip, `GpuUnload`
— including the sink naming a column whose exported type is not the declared one — and
`GpuLimit` proved the harness on the operators whose recipe carries no seq; then every
seq-bearing operator — the exec three, both aggregates, the three accumulators, the scatter,
nine hash-join types with and without a residual, cross, a nested loop of every type, the
scan, and the empty relation, alone and planned whole under a join through the driver on both
engines — each case green or a `bug_` case
asserting the wrong answer or the refusal with its ticket above it, every empty shape its own
case. Then the joins along the dimensions the corpus varies: a crossing projection on every
type; composite, `Int64`, `Utf8` and `Date32` keys under three answer shapes, the composite's
null second key answered as SQL's; every `key_types` column as an Inner and a LeftSemi key,
the floats without their -0.0 and NaN rows (#243's pin owns those); a `Date32` plus an interval
literal in a keyed residual and in a keyless condition the session hoists;
the residual with a projection, under `null_equals_null`, on a string and on a decimal; and
the nested loop's cross-then-mask path. Every join shape pbench shows has a node-level case,
and a guard reads pbench's plan golden and names a join query without one. Among them: NULL
keys on both sides of a Left and a Full join under both null rules, counted by hand; a semi
join over repeated keys answering each build row once; a NULL preserved-side condition, kept
by an anti join and marked `false`; `IS NULL` residuals on one side; a lane with no probe batch
for every type; a predicate-free outer nested loop over rows and over an empty side; and the
pairs chunked to one probe row a call on each path that chunks. The cross join's emitted order
is compared as emitted, since `tpch/nested-limits` keeps rows of it by position. `IS [NOT] DISTINCT FROM` has its four arms (both NULL,
one NULL either way, equal, unequal) on each evaluator. It is a nested loop's condition on either
path too, and pbench's `indf-full-join` runs whole at every mode as its null-equal hash join.
String keys are `Utf8`; the join cases declare no
view type. Then the aggregates,
expressions and sorted merges along the same dimensions. Group keys on dates, `Int64`, strings
and pairs, each carried through the merge; `count(*)` and expression arguments; every merge
arm with rows; the global Welford init, merge and dispersion finalize; the Welford state over
all-NULL groups, a NaN and lanes that disagree; a keyless Welford beside other
aggregates, with rows and over nothing; the project expressions,
casts and functions the corpus uses; sorts and merges on descending, nullable and composite
keys, and the merge at four lanes. The group keys are `Utf8` too; the aggregate cases declare no
view type. A guard reads the kind each case declares and names every kind with none, the three
forwarders excluded

| Operator harness, what the device holds | [a_divide_over_a_decimal_declares_the_scale_6_the_device_holds](../peacockdb-core/src/tests/gpu_tests/exec_schema_cases.rs), [stddev_holds_its_three_declared_state_names](../peacockdb-core/src/tests/gpu_tests/aggregate_schema_cases.rs) | 171 |
|---|---|--:|

the same nodes on the device alone, every output handle read where it sits through
`peacock_handle_schema` and held to the node's declared schema under cuDF's projection — a
`type_id` and a decimal's scale, never a precision — so a type the device changes is red at
the node rather than at the export, and an intermediate's type is read without moving a row.
One `<family>_schema_cases.rs` beside each family: the reader against the uploader and the
mid-plan limit; a scan of each fixture column type; a filter with and without its projection;
arithmetic on each numeric type, the divide's declared scale, each cast the corpus emits, the
union's two branch casts, each scalar function the dispatch admits, CASE, LIKE and the unary
forms; a sort; the three accumulators; the scatter on each key type; all nine hash-join types,
with and without a crossing projection, Left and Full also through a finish that pads build rows
no probe matched, and Inner, Right and LeftAnti on each key type; the cross join, and the nested
loop of every type, each with and without a projection; and each aggregate's init, the merges,
and the sum, avg, stddev and var finalizes, grouped on each key type and global

*component*

| Recipe walk on a device | [an_average_finalizes_to_the_digits_the_oracle_computes](../peacockdb-core/src/wire/gpu_tests/mod.rs), [an_avg_partial_holds_a_string_key_a_scale_2_sum_and_an_int64_count](../peacockdb-core/src/wire/gpu_tests/mod.rs)  | 28 |
|---|---|--:|

the recipe plan driven by hand — begin_plan, the calls each recipe names, handles threaded
between them, DataFusion on the same SQL as the oracle. One partition and one batch except the
aggregates, which take two so a merge happens; `avg` asserts digits, since cuDF takes a
divide's scale from its operands where arrow takes it from the declared type. A ROLLUP is here
because its masks and NULL placeholders are the one payload the plan line does not imply;
`GROUPING()` over every key, a summed quotient, a two-stage DISTINCT beside `avg` and `count`, a
keyless aggregate's identity row on a lane that saw no batch and over a scan of nothing, and no
done call over lanes with rows are walked too. One read re-walks every query to check the kinds a
device has run against the kinds the file claims, in both directions. Twelve spot-checks read what the device holds between calls, each
a literal schema written from the plan before any ran — `avg`'s partial, merge and finalize,
`sum`'s partial, the filter, the project, the inner and semi joins' session build, probe and finish, the nested
aggregates' inner finalize and outer `max`, the rollup's partial holding the `UInt8` id the plan
declares, and both inits of a DISTINCT under an eight-key rollup holding a `UInt16` id, whose `GROUPING()`
over the eight the oracle reads through the narrowing

*subcomponent*

| GPU↔comet murmur3 | [gpu_spark_partition_ids_match_rule_live](../peacockdb-core/src/executor/cpu_backend/gpu_tests/murmur_conformance.rs) | 19 |
|---|---|--:|

the linchpin gate: both sides place every row in the same partition, bit-exact, over every key
type the planner emits — and against production `rows_per_lane`, not a copy of the rule, which is
what [#201](archive/archived-tickets.md#t201) was. Two of the nineteen need no device, the comet
call and `pmod` on a negative hash, and ride here anyway, because the module is the gate and
splits nowhere

| Executors on a device | [an_aggregate_that_finalizes_runs_both_of_its_calls](../peacockdb-core/src/executor/gpu_backend/gpu_tests/exec.rs) | 42 |
|---|---|--:|

each one handed its node's recipe — the exec nodes one batch at a time (filter, project,
per-batch sort, the export with a row range, and an accumulator's recipe refused), the
accumulators a stream of them (the aggregate's init with and without its finalize, and refused
without its done calls, coalesce, the accumulating sort, the state merge, the Welford merge, the mid-plan limit), and the joins what the matrix says a device
runs on the session: its lifecycle, `Drop` releasing it, `owes_nothing`, the journal and region
per call and no finish call for the four types that owe nothing at done, its residency (a
RightSemi's distinct build keys among it) and call scratch (a LeftSemi probe's distinct keys), and
the scatter's N handles; plans hand-built over six rows the test writes itself, since the ABI
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

The C++ suites, the Python sets, `cost-report`, and the repo guards — cost-report's
`test_source_formatting` reads the workflow yaml and the source tree, `test_crate_privacy`
compiles a probe against the library, `test_golden_format` tests the harness's own format
reader — none of which runs engine code. `test_source_formatting` lives in cost-report because
that crate is never staged: no peacockdb-core test reads the checkout beyond testdata, and its
`no_core_test_reads_the_checkout` holds that.

| Category (lang) | Why | Examples | Runs | N |
|---|---|---|---|--:|
| Golden text format (Rust) | the one reader of the format, over strings: the differ must name what moved, what is missing and what is out of order, and a node line must yield its name, depth and fields — every golden assertion in the tree is a comparison through it. The comma traps are cases, since `on=[(a@0, b@1)]` and `Decimal128(38, 15)` each carry one inside brackets. The over-cap fingerprint round-trips here too: a column of either class, a row order that must not reach it, a mis-paired join that only the hash catches, and the triple's number format, which `testdata/test_duckdb_result.py` asserts from the Python side so the two writers' text is one | [a_section_that_moved_is_named_with_the_column_that_moved](../peacockdb-core/tests/test_golden_format.rs) | dataset-matrix (25.02 leg) | 41 |
| CI wiring guard (Rust) | every Rust target must be named by a CI step — CI does not glob, the three lists that decide where a GPU target runs must agree, and both GPU runners must pass `--test-threads=1`, which is the whole of the single-tenant invariant inside a process and is what a device test's `unsafe { set_var }` rests on. The rungs the `--test` sweep cannot see get one assertion each: cpu (`--lib` under `rust-only`), ffi (`--lib -- ffi_tests::`), device (the staged lib binary and the loop line handing it `gpu_tests::` — shad-gpu runs prebuilt binaries, so there is no command line), plus the CLI build, which has no test target. The three runners must agree on the lib's staged name and rung too, and the rung must reach the binary through `rung_args`. The reader is scoped to the rust loop: a file-wide search passes on the comment above the command, and the C++ loop above it shares the loop variable and rightly carries no flag; and the benchmark binary runs on CI without its `bench_` cases, `--skip bench_` read off the same loop. One scripted guard rides here too, for want of anywhere else that reads the workflows: `generate_pbench.sh --check`, whose single `run:` line is all that holds pbench's committed parquet to `gen.sql`, and whose matcher rejects a commented-out line — `fold_continuations` keeps a comment verbatim, and commenting a step out is the likelier way it stops running than deleting it | [every_rust_test_target_is_named_by_ci](../cost-report/tests/test_source_formatting/ci_coverage.rs), [each_rung_has_its_ci_line_and_the_cli_is_built](../cost-report/tests/test_source_formatting/ci_coverage.rs), [the_three_gpu_target_lists_agree](../cost-report/tests/test_source_formatting/runners.rs) | cost-report | 11 |
| Module layout rules (Rust) | the component walls, which the compiler mostly cannot check: where a `pub` may appear, that a subcomponent is declared `mod`, that no `super::` chain climbs out of its component, that no public signature names a type from a private module — `private_interfaces` reads a type's nominal visibility, so an unreachable type spelled `pub` passes it silently — and that no `pub` in `test_support/mod.rs` names a type from a component, which is what keeps the harness a facade and not a rename. Sibling reach is the case rustc refuses to have an opinion on at all: no visibility level means "my parent but not my siblings". Five more rules say where test code may live: a test module is named for the build rung it needs and gated for it, in both directions; its body is a file of its own; a `#[cfg(test)]` sits on nothing but a test-module declaration; and a path compiled only under a test gate says `test` in its name. Bare `pub` is checked against `SURFACE`, the CLI's API by file and name, in both directions, so a dropped-and-added pair cannot pass a count; `pub mod` is pinned to `lib.rs`. One register remains, the items that keep a `#[cfg(test)]` because no test module can hold them, checked in the reverse direction too, so an entry outliving its reason goes red. One rule here is a task spec's rather than the style guide's, and the same kind of claim: the device corpus tier records `gpu-result.txt` ahead of both comparisons against the cpu, read as the order of three calls in `gpu_case`, because the divergence DuckDB settles is the one that makes those comparisons panic. Two more read the tree: a doc block a split left behind above another item's, and no peacockdb-core test naming `CARGO_MANIFEST_DIR` outside `testdata.rs` and `build.rs`, so none reads the checkout a staged binary's host does not have. Needs no dataset and no device | [no_public_signature_names_a_type_from_a_private_module](../cost-report/tests/test_source_formatting/privacy.rs), [the_device_case_records_its_answer_before_it_compares_with_the_cpu](../cost-report/tests/test_source_formatting/write_order.rs), [no_core_test_reads_the_checkout](../cost-report/tests/test_source_formatting/checkout_reads.rs) | cost-report | 21 |
| Crate privacy probe (Rust) | a probe crate compiled against the built library proves `wire::generated` is unreachable from outside, with a positive control so a probe that fails for the wrong reason cannot pass as proof. Its own binary because it needs the rlib, so it is never staged | [a_private_module_is_unreachable_from_outside_the_crate](../peacockdb-core/tests/test_crate_privacy.rs) | cost-report | 1 |
| Cost-report renderer (Rust) | glyphs, links and the anchors they resolve to, ratio bucket, regression gate, history, every tpch and tpcds registry query has DuckDB's figure, every micro-query PeacockDB's, by its hyphenated section; the PR comment under the body cap with no Features column and pbench as its summary line, while the html page and the `--widget-md` tables keep both. No comment row is wider than its header. The checked-in `testdata/report.html` has a Features header per dataset and a row per pbench query, so a page laid out from the comment's `--md` goes red | [bucket_threshold_is_1_4](../cost-report/src/main.rs), [regression_count_drives_exit_decision](../cost-report/src/main.rs), [every_refusal_names_a_ticket_that_exists](../cost-report/src/main.rs), [a_registry_row_with_a_quoted_oracle_parses](../cost-report/src/main.rs), [the_checked_in_report_keeps_features_and_every_pbench_row](../cost-report/src/main.rs) | cost-report | 49 |
| DPhyp join enumeration (Rust) | the optimum against a brute-force DP over every split, on chains, stars, cliques, cycles and hypergraphs; the connected pairs it enumerates against the paper's closed forms; the pair budget, malformed input, and the C ABI's postfix tree | [dphyp_finds_the_brute_force_optimum_on_every_shape_up_to_six_relations](../peacockdb-dphyp/src/tests.rs), [each_connected_pair_is_enumerated_once_as_the_paper_counts](../peacockdb-dphyp/src/tests.rs) | cost-report | 6 |
| DuckDB cost extraction (Python) | classifier / pruning / dynamic-filter logic — fails CI before generation | [scan_count_mismatch_fails_loud](../testdata/test_duckdb_cost.py), [compute_pruning_from_rowgroups](../testdata/test_duckdb_cost.py) | cost-report | 41 |
| DuckDB result rendering (Python) | the only side that can get the result oracle's rendering wrong on its own: a timestamp must read as arrow-rs prints it (`…00.001`, not `isoformat()`'s `…00.001000`) or every row carrying one is a false divergence, and the over-cap fingerprint must be the text `test_golden_format.rs` pins, byte for byte, or the two writers compare nothing; a `no_sort` query's rows kept in DuckDB's order, read off a quoted registry cell | [test_a_timestamp_renders_as_arrow_rs_prints_it](../testdata/test_duckdb_result.py), [test_both_writers_fingerprint_the_same_rows_the_same_way](../testdata/test_duckdb_result.py), [test_render_keeps_order_when_asked](../testdata/test_duckdb_result.py) | cost-report | 22 |
| Report widget (Python) | the registry read — a quoted oracle cell is one field, a registry older than `duckdb_oracle` or `stored` still renders, and the include fallback reads a line on the grammar — and the four columns after Query: Result stored (the registry's `stored`), DuckDB, cpu ↔ DataFusion and gpu ↔ cpu, each header's hover text saying what its comparison does. q2's stored and DuckDB cells divide into labelled halves, a one-word cell does not, a row with no line keeps four cells, and the fingerprint's text says what it compares. The widget's input flag is `--widget-md`; the old `--md` is refused before a page is written | [test_a_quoted_oracle_cell_is_one_field](../scripts/test_ticket_widget.py), [test_q2s_two_cells_split](../scripts/test_ticket_widget.py), [test_the_old_md_flag_is_refused_before_the_page_is_written](../scripts/test_ticket_widget.py) | cost-report | 11 |
| Exec-model prototype (Python) | the scheduler over mock traits, plus pandas-backed operators checked against a single-shot oracle at five partitioning configs, both limit lowerings, the scalar expressions pinned to what `expr.cpp` does rather than what pandas defaults to, and every join mode against an oracle written from the SQL — the hash join and the nested loop at all nine types, each streaming its probe and answering a call as the device's join session does, a residual on every family, `null_equals_null` counted by hand, a guard that the prototype names every join type `join_session.cpp` dispatches on, the hash table's bytes a row held to the Rust, and a residual crossing where a conjunct reads both sides — no project code. Over the committed `tpch.minimal`: `LayoutInjector`'s own guarantees (each preset a different layout, an unshuffled join's lanes kept, empty batches really emitted, a seed that reproduces), and a stream fitting a budget the collected table does not. And the optimizer's half, still without data: the engine's plan goldens of all three datasets read back, lowered and built node for node — pbench's types, `GpuEmpty`, a scan of no column, a LIKE against a column, a cast between timestamp units, a month interval, whole days of one on a date, and a remainder among them — the layout each node declares, statistics, selectivity, row estimates and C_out, every other mode's joins among the one mode the cardinality golden holds, the per-call model (a semi join's keys taken distinct per probe call at the device width, that width's rule held to the Rust), dynamic-filter candidates — a key lifted into every branch of a union and through every join type, a join equating NULLs pruning nothing by a build holding one and an anti or mark join without the flag pruning as an inner join does, a build filtered only by the planner's null-key filter no candidate, nested builds and builds over a scan another filter prunes probed after them, a second pass's names after the first's — and the second pass over toy tables: a joined build its estimate overcounts still pruning, a build that misses replanning inside its probe, a probe whose build holds its whole cluster costing no more than the plan without it, a probe's refused miss, a probe whose root is hashed on four lanes keeping that hash through its replan, and a probe with no miss equal to its side run whole; join clusters assembled, a non-inner join with a residual among them, named — their ids kept through a reorder, a flip and a replan — read through the planner's null-key filter below a shuffle, which reassembly puts back on the sides `null_key_droppable` names (that table held to the Rust), where it tests only columns the shuffle hashes and the join keys on, on a side that drops NULL keys of a join not equating them, any other filter rooting a relation, tp1 and tp4 holding the same clusters over every plan golden, and disassembled, non-inner joins in them placed by their TES — every order the conflicts admit over three relations and a sample over four, with NULLs, NULL held equal or not and residuals sampled, answering as the plan does — and the four named non-inner queries' shapes reordered over toy tables, a left join's estimate no lower than its preserved side and an anti join's above 0, build side or probe, where the key domain is wider than both key counts, the adaptive loop — its build events, the replan they stop a run for, and the memory sources it feeds the builds back through — and DPhyp through its C ABI — the files that call it need a build, the crate's `.so` named by `PEACOCK_DPHYP_LIB`. The optimizer's report of a run over toy tables, each rule recorded where it fires and nothing where it does not — each relation's rows, how they are known, width and key NDVs, an aggregate's output column, a non-inner join's type on both trees — its `.optimizer.txt` rendering, and its `.optimizer.md` page: a cluster reordered, flipped and replanned as one table of the orders that changed it, a kept one as a line, a probe's part of a cluster, legends closed over the sources they name, both passes' filters by scan and build, and the anchors GitHub gives its headings. The report's names: a relation by its table, filter and aggregate, the planner's null-key filter no filter, a table the plan reads twice numbered, an aggregate's column kept through a memory source, a join tree with its build bracketed and its kind as SQL names it, a missed join in words. A clustered join's id in the `cpu.txt` render, the answer compare `run.py` holds an optimized run to, and `run.py`'s section merge, a page's preamble and headings, corpus order, mode table, checks before any work and a failing query's section, a report no renderer can write among them. The engine's cost function read from `cost_model.conf` — a hand-priced run, the total summed and rounded as the Rust does, lines split as Rust's `lines()`, a kind the conf lacks, a memory source free, and every committed `-mini.cost.txt` derived again from its `cpu.txt` — the optimized cost as the final run's, the probe plans' and the stopped runs', and `cost_report.html` from hand-built rows: its columns, a skipped and a failed run, the ratios, a ratio other than 1.00 linking to its query's section of the mode's page, relatively or at a commit, the rules a line per mode — the multi-joins reordered, the filters applied per pass, the replans, or that the plan changed — each linking to its lines of the mode's log, and "nothing changed the plan" only where no log says it did, the summary counted by totals rather than the shown ratio, a cost file priced under another conf refused naming it; those counts and lines read from a `.optimizer.txt` section and from what `report_text` writes, a call inside a probe's replan and a replan keeping its order among them, and the committed page rendered with pandas, pyarrow and numpy blocked. A cost that raises stops DPhyp's solve with its exception. A scan's columns read once per task, and the shuffle's digests pinned per dtype | [test_a_join_in_its_build_phase_holds_back_its_probe_subtree](../scripts/exec_model/tests/engine/test_scheduling.py), [test_every_join_type_matches_the_oracle](../scripts/exec_model/tests/operators/test_join_capability.py), [test_a_dphyp_call_records_its_input_the_sets_it_priced_its_tree_and_each_flip](../scripts/exec_model/tests/optimizer/test_report.py), [test_the_engine_s_cost_goldens_are_what_this_function_derives_from_their_cpu_runs](../scripts/exec_model/tests/plans/test_cost_model.py) | cost-report | 523 |
| Exec-model statistics and estimates (Python) | the committed NDV sidecars counted again from the generated data by `gen_stats.py` and compared byte for byte, each column they list served by the reader, and every join's row estimate against the engine's own CPU run, one golden line per join. And `run.py` over two small tpch queries and a refused tpcds one, by two workers into a temp directory: each optimized answer the planned one, its three files per bench and the cost report, the engine's tree and join rows where no rule changed the plan, the optimized cost the planned one where nothing fired, and a filtered run of the query DPhyp is called for that leaves the other sections alone. And every tpch and tpcds golden plan at every mode ordered without a run (pbench has no sidecar): it builds, and no final aggregate at more than one lane meets its rows unhashed on its group keys. They ride dataset-matrix because that is where sf1 and its DuckDB are | [test_the_tpch_sidecar_is_what_gen_stats_counts_from_its_data](../scripts/exec_model/tests/optimizer/test_stats_sidecar.py), [test_tpcds_join_estimates_match_the_golden](../scripts/exec_model/tests/optimizer/test_cardinality_corpus.py), [test_run_corpus.py](../scripts/exec_model/tests/test_run_corpus.py), [test_ordered_corpus.py](../scripts/exec_model/tests/optimizer/test_ordered_corpus.py) | dataset-matrix (25.02 leg) | 10 |
| Exec-model sidecar across embeddings (Python) | tpch sf1 generated with `--embeddings external`, counted by `gen_stats.py`, gives the committed sidecar byte for byte — the synthetic default is the dataset tier's own recount. It generates the dataset, a minute or two, and needs the embeddings cache, which no CI host may fetch; without the cache it fails rather than skips | [test_tpch_with_external_embeddings_gives_the_committed_sidecar](../scripts/exec_model/tests/optimizer/test_stats_embeddings.py) | manual — a host with the embeddings cache | 1 |
| Exec-model engine plans (Python) | the engine's own plan goldens run by the prototype over sf1 against DuckDB: as planned (130), pruned by their dynamic filters before and after join order (73), with every join cluster disassembled and rebuilt (111), and reordered by DPhyp and oriented (111) | [test_engine_answers.py](../scripts/exec_model/tests/plans/test_engine_answers.py), [test_engine_optimized.py](../scripts/exec_model/tests/optimizer/test_engine_optimized.py) | manual — exec-model-corpus.yml, 3 shards | 425 |
| Exec-model per-call model, measured (Python) | the per-call model fitted from the sf40 calibration record orders each query's modes as the device did: at least fifteen pairs of modes whose measured times differ by more than 10 %, none the wrong way round — 25 of the record's 30 today. The record is not in git: the job fetches it from S3, and the test fails naming the fetch script when it is absent | [test_the_measured_record_orders_each_query_s_modes_as_they_were_measured](../scripts/exec_model/tests/optimizer/test_call_cost_measured.py) | manual — exec-model-corpus.yml, call-cost job | 1 |
| Calibration scripts (Python) | the two capture readers and the plotter over a synthetic two-case Nsight export written by the test: two cases stay two row sets, each case is checked against its own dataset's plan golden, `hbm_bytes` lands on the right tuple, a call is the median of its executions, a capture of two datasets joins onto their two records, a case captured alone is refused under another case's name and parts merge into one file only when they cover the record, and a missing `--peak-bw` exits non-zero, and a ten-row record draws the six panel directories and `index.html`, two datasets as two series in every pooled panel, a kind one dataset makes in that dataset's run-wide colour, an hbm file a row per execution draws the same per-call points as its fold, and a fold takes rows of both shapes each by its own. And the split a record is published by: a run of one dataset is its file byte for byte, two datasets are two files under one heading, a query is spelled as the benchmark macro spells it, and a run missing a declared case, timing an undeclared one, captured under Nsight or naming no device or two is refused naming why, with nothing written; each file lies under its device's slug, the harness's rule. And the fetch: a device's record from its own key into its own directory, the H200's pin, which names no device, as it was, and a name that is not a pin path refused before any fetch. A capture is refused unless every GPU it lists is the card the record's heading names, a record naming none being the H200's, and refused naming the table when it has no GPU table or lists no GPU; no project code, no device | [test_calls.py](../scripts/calibration/tests/test_calls.py), [test_hbm.py](../scripts/calibration/tests/test_hbm.py), [test_plot.py](../scripts/calibration/tests/test_plot.py), [test_split.py](../scripts/calibration/tests/test_split.py), [test_fetch.py](../scripts/calibration/tests/test_fetch.py) | cost-report | 41 |
| Nebius scripts (Python) | `nebius.sh` against fake nebius, ssh and ssh-keyscan: every exit code, and the host-key rule; `--instance-id`/`--profile`, the per-instance alias, and the soft stop on a CPU VM; `install-watch`'s one profile, its embedded key and the install file; the idle watcher's decision over a process snapshot and a clock, for both kinds, and the hourly checker's verdict over the watcher's last log line; `watch.sh` and the hourly workflow's three legs, each skipped with a notice rather than failed; that only an actor on `GPU_ALLOWED_ACTORS` reaches the host; `ci-ship.sh` with real rsync into a local home — `lib/`'s symlinks and exec bits, a cache mirroring each run, retries that end red; and `nebius-test.sh`'s list over stub binaries — what runs, with what, and what is red. No VM, no secret | [test_nebius.py](../scripts/nebius/tests/test_nebius.py), [test_idle_watch.py](../scripts/nebius/tests/test_idle_watch.py), [test_install_watch.py](../scripts/nebius/tests/test_install_watch.py), [test_watch.py](../scripts/nebius/tests/test_watch.py), [test_ci_ship.py](../scripts/nebius/tests/test_ci_ship.py), [test_nebius_instances.py](../scripts/nebius/tests/test_nebius_instances.py), [test_idle_watch_kinds.py](../scripts/nebius/tests/test_idle_watch_kinds.py), [test_watch_workflow.py](../scripts/nebius/tests/test_watch_workflow.py) | cost-report | 170 |
| Images (Python) | the images' committed inputs and the scripts that build and run them: `docker/pins.env` against the specs, the specs' hashes and the venv lock; `image.sh`'s build line, its tags, its `peacockdb.inputs` label holding every `docker/` file its Dockerfile reads and only the pins it declares, a pin left out only where another version's stage declares it, the same in every locale, and the pushes it refuses; the cmake wrapper's redirects to the prefetched rapids-cmake and flatbuffers; the DuckDB extension directory; the static-cudart guard; `images.yml` started by a dispatch alone, its four steps unconditional and of a fixed shape, building and pushing through `image.sh` with the job's own token and nothing else, its disk gate run over fakes, and the reader it is parsed with refusing what it does not read. The embeddings cache over its pinned origins, with a fake curl: CI, GITHUB_ACTIONS and PEACOCK_NO_FETCH refusing before any call; nothing fetched that a full cache holds, and a missing query set fetched from its origin, resumed; through a symlink, the cache beside the link; an unzip killed midway leaving no GloVe text a re-run accepts; and the committed pin naming each artifact once. The calibration bucket's endpoint and region from `AWS_ENDPOINT_URL` and `AWS_REGION`, keeping the old storage when no endpoint is named and refusing an endpoint without a region. `bootstrap-vm.sh` over a fake docker and sudo: what it refuses before its first install, root among it; a `--ref` the clone is not on refused, naming both branches; one `apt-get update` before any install; the machine-wide data directory, `/var/lib/peacock` unless `PEACOCK_DATA_DIR` names another, before any download; and a whole run: no marker, `DISABLE_AUTOUPDATER=1` in `~/.bashrc` and no `CLAUDE_CONFIG_DIR`, no other script named; an image of other inputs or without the label refused before any run in it; the registry fetched with the clone read-only; sf1 chosen by generator inputs and generated once, offline; one cuDF checkout a commit, sparse, reused, the clone's own submodule never read, and a commit it cannot fetch refused; what it writes in the data directory group-writable; two runs at once generating once and fetching in turn. `provision-vm.sh` over a fake ssh and VM: the laptop's files and their modes, the developer's user, groups and keys, bootstrap on every run, the GPU key `--gpu-key` names copied to the VM and a missing key or `.pub` refused with no key made, the secrets on ssh's stdin alone, the PAT in no argv and not in the VM's env file, another key in `peacock.env` refused by name, an incomplete env file, a missing git identity and a token it cannot use refused, the env file kept with no CR and a final newline, `--env` naming the laptop's own file too, and its comments skipped, a `--ref` the VM's clone is not on refused, a rerun skipping what is done, without a terminal Claude's login named as a command that works rather than run, `--new-pat` on every VM it reaches, help printed without sed and a bash older than 4.4 refused. `fetch-sf40.sh` over a fake aws and buckets: the endpoint and region from the env file, the keys in no argv and in no environment but aws's, from a file or stdin, one missing refused; a second run fetching nothing; a short file refetched, a short download refused and not kept; a parquet the manifest does not name, a manifest naming a path, an unknown dataset refused; the aggregate in C order against its pin, a wrong one refused, an unpinned one printed; the pinned aws CLI installed when there is none, a zip of another sha refused. `install-duckdb.sh` over a fake curl: the pinned CLI installed, a zip of another sha or version refused with nothing installed, a failed download named, the pinned version skipped, another replaced, sudo where the prefix is not writable. `run-gpu.sh` over a fake `nebius.sh` and VM: every call through `nebius.sh`, with `--instance-id` and `--profile` as given, none without `--instance-id`, and never `start` or `stop`; its exit 64 as 2, and 69 and 75 passed on; a stale build or run image refused naming the file, `run` too, reading the label without a pull, and a VM without the run image refused as `prepare` first, an old driver as 3 naming both versions, admin-only counters as 4 after `update-initramfs`; the storage keys on stdin alone; the cuDF prefix copied once per image; the install shipped by `--dir`, the staged one by default, a missing one refused; the committed testdata shipped, the datasets linked at their absolute paths and mounted at their own; the run's hostname, filter and resume passed on, a run in progress refused, its exit as the run's; results, record and log pulled home. `check-build-image.sh` over the same fakes: the build image pulled, then its label read, on the VM; one of other inputs or without a label refused; none without `--instance-id`. `stage-install.sh` over a fake cargo: four gpu binaries and a release benchmark staged from empty and stripped, a failed build named, another image's cuDF refused. `create_nsys_profile.sh --ssh`: every call and rsync through the given command. With the images built: each image as a uid it does not know with no network, every library the staged binaries need resolving in the run image, and `run-tests.sh` over fake binaries | [test_image.py](../scripts/env/tests/test_image.py), [test_images_workflow.py](../scripts/env/tests/test_images_workflow.py), [test_embeddings.py](../scripts/env/tests/test_embeddings.py), [test_s3.py](../scripts/env/tests/test_s3.py), [test_bootstrap.py](../scripts/env/tests/test_bootstrap.py), [test_provision.py](../scripts/env/tests/test_provision.py), [test_fetch_sf40.py](../scripts/env/tests/test_fetch_sf40.py), [test_install_duckdb.py](../scripts/env/tests/test_install_duckdb.py), [test_run_gpu.py](../scripts/env/tests/test_run_gpu.py), [test_check_build_image.py](../scripts/env/tests/test_check_build_image.py), [test_stage_install.py](../scripts/env/tests/test_stage_install.py), [test_run_tests.py](../scripts/env/tests/test_run_tests.py) | manual | 231 |
| C++ CPU/FFI unit | decimal binop typing, AST routability, lifecycle, the row-range clamp rule over the same shared case table the Rust clamp reads, the two refusals of the test-only upload symbol, and the timing switch — its mode round-trips, the ABI refuses a mode it does not name and a null region buffer with a capacity, and a second harness NVTX push replaces the first rather than nesting; `TableResult::of`'s two refusals, Final and Single modes refused by name, and a `ColumnRef` resolved by index and name, and `MERGE_M2`'s count type — `INT32` before cuDF 25.06, `INT64` from it, and the type this build's headers pick; no GPU needed | [DecimalScale.BinopOutputType](../cpp/tests/cpu/test_executor.cpp), [AstRouting.CudfAstCanEvaluate](../cpp/tests/cpu/test_executor.cpp) | dataset-matrix (`ctest -L cpu`) + shad-gpu | 26 |
| cuDF GPU smoke (C++) | the GPU is alive; the RMM pool reserves the budget the binary declared; `peacock_device_info` names the device, gives the runtime's version, and refuses a null output. The two hardcoded-comet partition-id cases are gone: they were a third copy of the lane rule, and the live gate proves the kernel against the production rule instead | [CudfGpu.SequenceSum](../cpp/tests/gpu/test_cudf.cpp), [RmmPool.ReservesTheDeclaredBudget](../cpp/tests/gpu/test_cudf.cpp), [PeacockGpu.DeviceInfoDescribesTheDevice](../cpp/tests/gpu/test_cudf.cpp) | shad-gpu | 3 |
| Plan-executor (C++) | hand-built plan IR through the C++ executor, node by node — a join through the session's build, probe and finish, laid out as the wire lays it, its two sides and the `CudfJoin` leaf under one structural union — plus the per-call entry points at their contract edges (row-group override, export range, slice), a scan of no column answering its row count from the footer — whole, one row group, under a limit — and a project of no expression refused, the sqrt arm on both evaluators, a value-form CASE with and without ELSE — the first match winning, a NULL comparand or WHEN never matching — `date_part` answering in the type the wire names for three fields, a merge that emits state rather than a value, the literal arm — a typed null on the AST path, the decimal literal's scaled double, a decimal literal under a cast to `Float64` or `Int64` kept on the AST, every wire type walked through the dispatch, the LIKE guard, `IS [NOT] DISTINCT FROM` against a typed null on each evaluator — and the timed regions: `Off` records none, every entry point opens one per output partition, a second call of one seq counts up, a slice and an export are charged to the node that produced the handle, a handle from before timing was on and an adopted one each refused a charge, an export of no rows opens one too, and collecting drains; and the NVTX switch on its own: ranges without timing record no region. The handle's own rules are ten of these: a slice and a selection share their columns' owners, and five shapes are refused — a table of no columns, a name count that does not match, an ordinal past the columns, a selection of no ordinals, and a column appended at the wrong row count, a slice's string bytes are its own rows' and not its parent's, a scatter's partitions share one table and outlive their siblings and concatenate back to the input, an empty lane is a zero-row slice of no string bytes, two input handles are refused rather than concatenated, a scatter's peak and its requested total are those of partitioning the same input alone, measured with the RMM statistics adaptor over a fixed-width table and a string one; the total is the bound that discriminates on both shapes, since a string column's gather in cuDF already costs more than the copies did and so set the peak all along — and `slice_handle` still owns its rows; and the operator exit paths, nine of these: a bare `ColumnRef` evaluates to a view of the input's own column and allocates nothing, a non-AST predicate allocates no more than the comparison and the filter alone, a fused filter projection no more than the same filter without one, a project of column references nothing at all, a window no more than its own result column, and an aggregate no more than cuDF's groupby — each bound measured on the same input, since a hand formula would bake a cuDF cost into a constant; plus a filter whose projection repeats an ordinal, which answers both columns over one owner, and two that bound a release rather than an allocation, since no allocation bound can see an operator retaining what it should drop: a filter narrowed to one column gives back the dropped column's own bytes, and a project holds its shared column exactly once and frees the rest; and the grouping id's value and width from one key to sixty-four, each width boundary from both sides, and its refusal past them; and an aggregate's init handed no input answering over its declared input's zero-row table, where a merge handed none still refuses; the aggregate's request builder per function and phase, the Welford state with no NULL over an all-NULL group or merge, and a keyless Welford through the builder, with rows and over none, and a `ColumnRef` refused past the width, with no name, or under another column's name; and a date or timestamp ± an interval literal, each expected value DataFusion's: the month-end clamp in a leap and a common year, both ways, a mixed interval at every timestamp unit in the months-days-nanoseconds order, a sub-day part truncated on a date and floored on a timestamp, the interval on the left, a NULL interval, zero rows keeping the type, and any other interval use refused naming #249 | [PlanExecutor.HashJoinNationRegion](../cpp/tests/gpu/test_plan_executor.cpp), [ExitCopies.AProjectOfColumnRefsAllocatesNothing](../cpp/tests/gpu/test_plan_executor.cpp), [test_grouping_id.cpp](../cpp/tests/gpu/test_grouping_id.cpp), [test_aggregate_no_input.cpp](../cpp/tests/gpu/test_aggregate_no_input.cpp), [test_column_refs.cpp](../cpp/tests/gpu/test_column_refs.cpp), [test_aggregate_builder.cpp](../cpp/tests/gpu/test_aggregate_builder.cpp), [test_interval_arithmetic.cpp](../cpp/tests/gpu/test_interval_arithmetic.cpp) | shad-gpu | 126 |
| Join session (C++) | the four `peacock_join_*` symbols through the C ABI alone — plan bytes in, Arrow batches up, IPC answers down — over the matrix of [join-rewrite-design.md](tasks/join-rewrite-design.md) §5.6: all nine join types against keys only, a cross residual the AST takes and one it refuses, a residual reading one side alone, a nested loop whose conjuncts are all AST-able, partly so, or not at all, and a cross join; NULL keys on both sides under either `null_equals_null`, many-to-many keys, a composite key with a NULL in its second column; a build that is absent, zero rows or all-NULL keys, a probe that is absent, one batch, or three with a zero-row batch between; a projection absent, crossing sides, or empty and refused. Left and Full pad a NULL-key build row rather than dropping it. Five cases probe three batches with a zero-row batch between, no batch matching what another did, because `matched` is an OR folded across calls and a single batch cannot tell that from the last call's answer — which is the whole of #136. Four properties no answer can show get a case each — hoisting a one-side operand keeps a nested loop off the cross product, bounded by the same join whose condition needs no hoisting plus the build's own bytes; a probe hands its gathered columns over without a copy (#154), its peak bounded by the same cuDF calls on tables of the same shape and its `net` bounded *both* ways, since an upper bound is the only thing that sees a session retaining the batch it consumed; releasing a session, and ending the plan under a live one, give back the build side's own measured bytes; each call opens exactly one region; and the two `size_type` ceilings refuse by name rather than wrapping, asked on sizes because no test host holds a table that big | [JoinSession.LeftPadsEveryBuildRowNoProbeMatchedIncludingANullKey](../cpp/tests/gpu/test_join_session.cpp), [JoinSession.HoistingKeepsANestedLoopOffTheCrossProduct](../cpp/tests/gpu/test_join_session.cpp) | shad-gpu | 56 |
| TPC-H sf40 bare-cuDF (C++) | hand-written cuDF pipelines vs DuckDB sf40; the benchmark vehicle | [TpchSf40.Q1GroupByAggregates](../cpp/tests/gpu/test_tpch.cpp), [Q3JoinsGroupByTopN](../cpp/tests/gpu/test_tpch.cpp) | manual: shad-gpu alone, which holds sf40 and is down; `run-gpu.sh` skips it | 4 |
| TPC-H+V sf40 bare-cuDF (C++) | the same for the vector-embedding queries | [TpchSf40.Q11VectorBruteForce](../cpp/tests/gpu/test_tpchv.cpp) | manual: shad-gpu alone, which holds sf40 and is down; `run-gpu.sh` skips it | 4 |
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
  `golden(sort|no_sort, exact|approx_std)` | `live_cpu` on the device,
  `data_fusion(sort, exact|approx)` | `data_fusion_subset` | `data_fusion_disabled` against
  DataFusion, and `duckdb(sort|no_sort, exact|approx|rel(<tol>, "<column>", …)[, date("<column>",
  …)][, gpu: fingerprint])` | `duckdb_fingerprint` | `duckdb_none` against DuckDB. The first argument says whether
  both answers are sorted before comparing. `exact` is every cell equal; DuckDB's `approx` is a
  decimal within one unit in our last place and a float within 1e-11 relative, DataFusion's 1e-12
  relative, and `approx_std` 1e-11 relative on every numeric cell. `rel` holds the named columns
  to its tolerance relative to the larger magnitude, for a known numeric divergence, and takes no
  ticket; `date` compares the named columns' DuckDB timestamps by their date part. `gpu:
  fingerprint` compares the device's sections by fingerprint, for a query whose device rendering
  alone crosses the cap, and leaves the cpu's under the rest of the spelling. Neither side falls
  back silently: a fingerprinted device section without the part fails, and so does one holding
  rows with it. Columns are
  named as our result header prints them. Each grammar is held to the lines by an `ALL` or
  `FACETS` test, so a value nothing uses is deleted rather than carried.
- **Row order is compared for an `ORDER BY` query.** The cpu at its authoring mode: its
  rendering equals the committed section text for text, and that section meets DuckDB's row for
  row, except under `duckdb_fingerprint`. The device at every mode through `golden(no_sort, …)`,
  and against DuckDB as of the last recording of `gpu-result.txt`, except under
  `gpu: fingerprint` (tpcds q2), whose fingerprint carries no order. The other four cpu modes are compared through `data_fusion`
  only, and `live_cpu` compares as a multiset, so both stay order-blind. There is no tie rule: no
  corpus answer compared row for row has rows tying on its sort keys.
- The crate has no doctest today, so `--doc` runs nothing. The gap #128 names is still open:
  no CI step passes it and the meta guard enumerates only `--test` targets plus `--lib`, so
  the first doctest written would be unrun with nothing saying so.

## Datasets

Host columns are audited, not assumed: they say whether the bytes were actually found
there (`ls` + `sha256sum`, 2026-08-05). tpch.minimal and pbench.sf1 are in git; everything
else is generated or fetched per host. S3 is Nebius object storage, endpoint
`https://storage.eu-north1.nebius.cloud:443`. It accepts region `eu-north-1` and `eu-north1`
alike; both were checked against it. `scripts/lib/calibration-bucket.sh` takes the endpoint and
region from `AWS_ENDPOINT_URL` and `AWS_REGION` (`scripts/lib/s3.sh`). `upload_datasets.sh` and `check_s3_datasets.py` still name the
endpoint themselves. With `AWS_ENDPOINT_URL` unset, the calibration bucket uses the endpoint and
region above, for `build-test-shadgpu.sh`.

| Dataset | Shape | <sub>local</sub> | <sub>verda</sub> | <sub>shad-gpu</sub> | <sub>nebius-h100</sub> | <sub>nebius-cpu128</sub> | S3 bucket | Used by |
|---|---|:-:|:-:|:-:|:-:|:-:|---|---|
| tpch.minimal | 5 tables, 19 MB, git-committed | ✓ | ✓ | ✓ |  |  | — | C++ plan tests, plan serializer, node executor |
| pbench.sf1 | 5 tables, 588 KB, git-committed | ✓ | ✓ | ✓ |  |  | — | plan + CPU corpus subsets, the join-rewrite chain's shapes |
| TPC-H+V sf1, external vectors | 8 tables, 987 MB; GloVe 100-d + DEEP1B 96-d | ✓ | ✓ | ✓ |  |  | — | most CPU/GPU rust tests |
| TPC-H+V sf1, synthetic vectors | same tables, hash-generated FLOAT[8] | — | — | — |  |  | — | CI only — regenerated every run |
| TPC-DS sf1 | 24 tables, 764 MB | ✓ | ✓ | ✓ |  |  | — | plan + CPU + GPU subsets |
| TPC-H+V sf40 | 8 tables, 40 GB | — | — | ✓ | ✓ | ✓ | `tpch-sf40` | `peacock_tpch(v)_tests`, `peacock_gpu_benchmarks`, multi-GPU (manual) |
| TPC-DS sf40 | 24 tables, 15.8 GB | — | — | ✓ | ✓ | ✓ | `tpcds-sf40` | `peacock_gpu_benchmarks` |
| TPC-DS sf200 | 24 tables, 80 GB | — | — | ✓ |  |  | `tpcds-sf200` | no tests — S3 check, validate-large |
| TPC-H sf200 | not generated yet | — | — | — |  |  | `tpch-sf200` | nothing yet |
| embeddings cache | 1.8 GB local / 129 GB shad-gpu | ✓ | — | ✓ |  |  | — | generator input, not a test input |
| calibration record | the sf40 benchmark run's per-call rows, a file per dataset: tpch 254 thousand, 26 MB; tpcds 1,060, 106 kB | — | — | — |  |  | `calibration` | `test_call_cost_measured`, `plot.py` |

Paths: `testdata/{tpch.minimal,pbench.sf1,tpch.sf1,tpcds.sf1,embeddings-cache}`; sf40 and sf200 live
outside the repo on shad-gpu, under `/home/info/peacock-datasets/testdata/`. The two Nebius
columns track sf40 alone. `fetch-sf40.sh` puts both sets in `/var/lib/peacock/<dataset>/` there
("Containers"). Each ✓ there is a fetch checked against both pins, on 2026-11-04. **+V** means
the TPC-H tables carry vector columns — `part.p_text_embedding` and
`partsupp.ps_image_embedding` + `ps_text_embedding`; TPC-DS does not have embeddings.

The three sf1-class datasets are byte-identical on all three hosts (aggregate parquet
sha256 matches local). Notes on the rows above:

- **The sf1 parquet is generated** — `testdata/generate_testdata.sh` drives DuckDB, and
  `/tpch.sf*/`, `/tpcds.sf*/` are gitignored, so CI regenerates it every run. That is why
  the two sf1 rows differ: `--embeddings synthetic` is the default, so a vector query in
  CI runs against hash-generated FLOAT[8], not the vectors a dev host has.
- **pbench is the exception, and is committed** — `testdata/generate_pbench.sh` writes it from
  `testdata/pbench/gen.sql`, and dataset-matrix runs `generate_pbench.sh --check` rather than
  regenerating, so the bytes under review are the bytes the tests read. It is in no S3 bucket, and
  `check_s3_datasets.py` and `dataset_checks.py` key on the tpch and tpcds table specs, so neither
  sees it. A dataset this small is committed because its point is the values in it — NULL keys on
  both sides, every hashable key type, skew, empty sides — and a regenerated one would let those
  move without a diff.
- **The embeddings cache is a per-host intermediate and is NOT syncable** — no
  `--push`/`--pull` kind covers it. `fetch_embeddings.sh` fetches it from the origins by hand
  only: CI, GITHUB_ACTIONS and PEACOCK_NO_FETCH refuse it. The check is on the environment, not
  the host, so a manual run on shad-gpu passes, and automation that shells into a host sets
  PEACOCK_NO_FETCH=1. Regenerate it where you need it, or ship the augmented parquet instead.
- `testdata/embeddings.sha256`, committed, pins the four files `fetch_embeddings.sh` checks.
- **The calibration record is fetched, not generated**, and fetched where it is used, so no host
  column holds it. Objects are keyed by device and content,
  `<device>/<dataset>.sf<sf>/<sha256>.tsv`, and never replaced. The H200's predate devices and
  are `<dataset>.sf<sf>/<sha256>.tsv` until [#226](tickets/benchmarks.md#t226) moves them.
  `testdata/calibration/records.sha256`, committed, names the sha256 each device's dataset tree
  was measured beside. `scripts/calibration/fetch_record.sh tpch.sf40` fetches the H200's object
  into `testdata/calibration/tpch.sf40/records.tsv` and checks its sha256;
  `fetch_record.sh <device>/tpch.sf40` fetches another device's into
  `calibration/<device>/tpch.sf40/`. The aws CLI it runs is `$AWS`, `aws` by default: on shad-gpu `/home/info/bin/aws` with credentials in `~/.aws`; in
  CI the runner's own with the `S3_*` secrets; a dev box without credentials runs it on
  shad-gpu, `AWS=/home/info/bin/aws scripts/calibration/fetch_record.sh --ssh shad-gpu tpch.sf40`.
  Only `build-test-shadgpu.sh --pull-benchmarks` writes the bucket.
- **sf40 lives on shad-gpu**, and `fetch-sf40.sh` fetches it to nebius-h100 and nebius-cpu128.
  tpch.sf40's aggregate is
  `d9e4a8fe74e1923fc6f5fc47c0fc4b20d7d4e55b82e0a94090e2e37071bb8713`. It was pinned from the CPU
  VM's fetch, and the H100's own fetch matched it.
  CI's `gpu-tests` runs on the L40S and reads no sf40; the sf40 goldens are committed (see
  below), so a comparison never touches 40 GB.
- **tpcds sf40 is the benchmark's alone**: no test tier reads it, only the s3-datasets check
  (`check_s3_datasets.py`) checks its bucket, `tpcds-sf40`. It is made by `generate_testdata.sh
  --bench tpcds --sf 40` (DuckDB 1.5.4, one thread). On shad-gpu: 9 min 41 s, peak RSS 60 GiB,
  nothing spilled. With 62 GB of RAM DuckDB holds about 50 GiB and spills the rest to its temp
  directory (8 GB at most): 14 min 37 s, peak RSS 50.5 GiB, 15.8 GB of parquet. Single-threaded
  with total-order sorts, the bytes follow from the script and DuckDB alone; the bucket's copy is
  published by `upload_datasets.sh tpcds.sf40`, and the sha256 of its 24 files, sorted, sums to
  `58610fc16152a12ea9769d645a6e72a4e03002e2498ec8d357862fcbd9bebb36` (`sha256sum *.parquet | sha256sum`). The benchmark run links
  each sf40 dataset the case list names into its testdata, as it does tpch's.
- verda reaches the tree through a `/media/data/peacockdb` symlink to
  `~/peacockdb`. Every test binary honours `PEACOCK_TESTDATA_DIR`, but the plain cpu mode
  of `build-test.sh` does not set it, so those binaries fall back to the build box's path.

## Golden files

All goldens are committed, under `testdata/goldens/`: `tpch.sf1` (61 files), `tpcds.sf1`
(117), `pbench.sf1` (18), `tpch.sf40` (16), plus `recipe-payloads.txt` at the top, the recipe
payloads with a digest each. pbench's 18 are the five `.plans.txt`, the five `-mini.cpu.txt`, the
five `-mini.cost.txt`, `mini.result.txt`, `duckdb-result.txt` and `gpu-result.txt`: `gen_duckdb_cost.sh`
profiles tpch and tpcds only, so it has no per-query DuckDB cost oracle. Most of each sf1 count is the
per-query DuckDB cost oracle (43 + 99), one per query file; the engine's own are 16 apiece, one plan
and one execution set per mode. Outside that bucket sits one file per engine that answers:
`duckdb-result.txt` and `gpu-result.txt` in all three sf1 dirs, every one of them now having a device
cell on. The committed DuckDB profile inputs live beside them in `testdata/duckdb-profiles/{tpch,tpcds}`
(43 + 99) and `testdata/duckdb-dynfilters/{tpch,tpcds}` (43 + 99), and the per-column statistics
sidecars (NDV, strings' mean length) in `testdata/stats/<bench>.sf<N>.json`. The prototype's goldens
live in its own tree, `scripts/exec_model/testdata/goldens/<bench>/`.

The generator scripts live in `testdata/`.

| Golden | Produced by | Depends on | Asserted by |
|---|---|---|---|
| `<mode>.plans.txt` | `planner::tests::plan_goldens` (in `--lib`) with `UPDATE_CANONICAL=1` | sf1 parquet — row-group counts and per-column bytes decide `partition_groups` and the lane rules | the plan golden tiers, section by section |
| `recipe-payloads.txt` | `planner::tests::plan_goldens` with `UPDATE_CANONICAL=1`<br>**and** `PEACOCK_REWRITE_RECIPE_BYTES=1` | sf1 parquet, and a fixed `/tmp` symlink for the testdata root — without it the payloads carry this machine's paths and so does the digest | <sub>the_payload_golden_carries_<br>what_each_call_hands_the_executor</sub> |
| `<mode>-<tier>.cpu.txt` | the corpus cpu tier under `UPDATE_CANONICAL=1` (merge and prune) or `PCK_UPDATE_SECTIONS=1` (merge only) — never the GPU | sf1 parquet; one file per mode, a `== <query>` section each | the corpus cpu tier writes and verifies; the device tier verifies read-only |
| `<mode>-<tier>.cost.txt` | derived from the sibling `.cpu.txt` **section**, × `cost_model.conf` | that `.cpu.txt`, `cost_model.conf` | the corpus cpu tier + `test_cost_model`, which re-derives every section independently |
| `<tier>.result.txt` | the last mode a query declares, under either variable; a run without that mode leaves the section alone. Its rows are in engine order where the line's row order is `no_sort` — its `duckdb(…)`'s where that compares rows, else its `golden(…)`'s — and sorted otherwise; `gpu-result.txt` follows the same rule. Over the 262144-byte cap the section holds a **fingerprint** instead of its rows | sf1 parquet; one section per query, its `mode=` line naming the author | the corpus cpu tier; the device tier where `gpu_oracle` names a golden (`golden(sort|no_sort, exact|approx_std)`; `live_cpu` reads none); the DuckDB `duckdb_<ds>_<q>` cases; `the_registry_stored_column_matches_the_goldens_in_both_directions` |
| `<q>.duckdb_cost.txt` | `gen_duckdb_cost.sh --gen`<br>(DuckDB 1.5.4, `threads=1`, pyarrow 19.0.1) | committed pass-1 profiles ∩ pass-2 dynamic-filter bounds ∩ parquet row-group stats | the cost-report widget (directional signal); `every_registry_query_has_a_duckdb_cost` asserts each tpch and tpcds registry query has one |
| `duckdb-result.txt` | `testdata/duckdb_result.py`<br>(DuckDB 1.5.4, `threads=1`); timestamps rendered as arrow-rs prints them, rows sorted or in DuckDB's order where the registry's `duckdb_oracle` says `no_sort`, and a **fingerprint** over the cap | sf1 parquet, the query text in `<bench>-queries/` | `test_cpu_corpus`'s `duckdb_<ds>_<q>` cases, per the line's `duckdb_oracle`; `the_registry_stored_column_matches_the_goldens_in_both_directions` |
| `gpu-result.txt` | the corpus **device** tier under `PCK_WRITE_GPU_RESULT=1`, **before** it asserts, so an answer the cpu rejects is still recorded; one section per (query, mode), a fingerprint over the cap; `--pull-results` brings it home. A record, never an authority | sf1 parquet, the device's cuDF — the committed file holds cuDF 25.02's answers and says so in a `cudf=` first line, read from `$CUDF_ROOT`'s `version_config.hpp` at build time. It was recorded on nebius-gpu's L40S, not shad-gpu, which was down; the stamp carries the cuDF and not the card, so a regenerator on a third card has only this sentence to tell it the baseline moved. `PCK_WRITE_GPU_RESULT=<v>` writes `gpu-result-<v>.txt` beside it, gitignored | the `duckdb_gpu_<ds>_<q>_<mode>` cases , `every_enabled_device_cell_has_its_gpu_result_section_and_no_other` and `the_registry_stored_column_matches_the_goldens_in_both_directions` |
| `stats/<bench>.sf<N>.json` | `gen_stats.py --bench B --sf N`<br>(DuckDB: exact `count(DISTINCT)`, strings' mean `strlen`) | the parquet's flat columns. Each table records its rows and a sha256 of those columns' footer statistics (null count, min, max per row group); the reader refuses a table whose statistics moved. Data can change with none of them moving, so only a recount catches every stale NDV. Vector columns are left out, so both `--embeddings` modes give one file | `test_stats_sidecar` recounts it from the data in dataset-matrix; `test_stats_embeddings` from external embeddings, by hand |
| `<bench>/tp1-single.cardinality.txt`, in `scripts/exec_model/testdata/goldens/` | `test_cardinality_corpus.py` with `UPDATE_CANONICAL=1` | the plan golden, `tp1-single-mini.cpu.txt`, the stats sidecar and the sf1 footers, `optimizer/cardinality.py`. One line per join: its estimate, its rows in the engine's run, the q-error | `test_cardinality_corpus` in dataset-matrix, the whole file |
| `<bench>/<mode>.cpu.txt`, `<mode>.optimizer.txt`, `<mode>.plandiff.txt`, `<mode>.costs.txt` and `<mode>.optimizer.md`, in `scripts/exec_model/testdata/goldens/` | `scripts/exec_model/run.py`; with `--query` it rewrites only its own sections | the plan golden, the sf1 tables and sidecar, the whole optimizer, `cost_model.conf`, and pandas' byte counts. A `== <query>` section each: the optimized run in the `.cpu.txt` format plus each clustered join's `mj=`, what each rule did, the plan's diff, and both runs priced by the engine's cost function with the rules counted. The page has a `## <query>` section each, from the same report as `.optimizer.txt`: each cluster's successive orders and the dynamic filters. `.costs.txt` is not a function of the `.cpu.txt` beside it: it prices the planned run, the probe plans and the stopped runs too | nothing: generation only |
| `cost_report.html`, in `scripts/exec_model/testdata/goldens/` | `run.py` at the end of every run, or `scripts/exec_model/cost_report.py` alone; committed with relative links | every `<bench>/<mode>.costs.txt` and `.optimizer.txt` beside it, the engine's `-mini.cost.txt` and the `<q>.duckdb_cost.txt` | the cost-report job renders it again with `--sha`, its links at the commit, and on master publishes it at `join-optimizer/` |
| `peacockdb-core/tests/common/corpus_benchmark_cases.inc` (source, not testdata) | `the_benchmark_cases_follow_the_registry` in `test_corpus_goldens` with `UPDATE_CANONICAL=1`, published by rename | `testdata/cost-registry.csv`'s `gpu_*` columns, tpch and tpcds rows, timed at sf40 | that test, byte for byte |
| `tpch.sf40/duckdb_<q>.csv`, `.count.csv` | `gen_duckdb_goldens.sh --sf 40`<br>on shad-gpu | sf40 parquet, query text from `tpch_query_sql.sh` | `peacock_tpch_tests` / `peacock_tpchv_tests` |

How they hang together — parquet at the top, goldens derived left to right:

```
testdata/{tpch,tpcds}-queries/*.sql        testdata/pbench/gen.sql
        │                                         │
        └── generate_testdata.sh                  └── generate_pbench.sh
              (DuckDB, --embeddings                     (DuckDB; CI runs --check
               synthetic|external)                        and never regenerates)
                 ▼                                         ▼
        tpch.sf1 / tpcds.sf1   (parquet, gitignored)   pbench.sf1  (parquet, committed)
          │
          │   all three feed every branch below, except the duckdb_cost
          │   one: pbench's queries are named rather than qN
          │
          ├── planner::tests::plan_goldens (--lib), UPDATE_CANONICAL=1
          │     ├──► <mode>.plans.txt
          │     └──► recipe-payloads.txt  [also needs PEACOCK_REWRITE_RECIPE_BYTES=1,
          │                                   and the fixed /tmp symlink]
          │
          ├── the corpus cpu tier, UPDATE_CANONICAL=1   (the author; a device never writes)
          │     ├──► <mode>-<tier>.cpu.txt        (a == <query> section each)
          │     │       └──× cost_model.conf ──► <mode>-<tier>.cost.txt
          │     └──► <tier>.result.txt   (one section per query, from its last mode; over
          │               │               256 KB a fingerprint, asserted like the rows it stands for)
          │               ▼  read-only, never written
          │     the corpus device tier (test_gpu_corpus, shad-gpu)
          │       ├── <mode>-<tier>.cpu.txt, .cost.txt ──► every device cell, per node
          │       ├── <tier>.result.txt ──► the device's answer, per gpu_oracle:
          │       │     golden(sort, exact)     text for text, both sides' rows sorted
          │       │     golden(no_sort, …)      row for row in engine order
          │       │     golden(…, approx_std)   numeric cells within 1e-11
          │       │     live_cpu                not read: a cpu run at the same mode instead,
          │       │                             for a query whose section holds no rows
          │       └──► gpu-result.txt   only under PCK_WRITE_GPU_RESULT=1, before asserting:
          │                              a cudf= line, then one section per (query, mode), a
          │                              fingerprint over the cap; --pull-results brings it
          │                              home; a record, never an authority (=<v>:
          │                              gpu-result-<v>.txt, gitignored)
          │
          ├── duckdb_result.py  (DuckDB 1.5.4, threads=1; timestamps as arrow-rs renders them)
          │     └──► duckdb-result.txt   (one section per query; a fingerprint over the cap)
          │
          │   test_cpu_corpus, rust-only (CI), per the line's duckdb_oracle
          │   (exact | approx | rel | date | gpu: fingerprint | fingerprint | none):
          │     duckdb_<ds>_<q>             <tier>.result.txt ↔ duckdb-result.txt
          │     duckdb_gpu_<ds>_<q>_<mode>  gpu-result.txt   ↔ duckdb-result.txt
          │     coverage   every enabled device cell has its gpu-result section, and no other;
          │                and the committed file's cudf= line is 25.02's
          │
          ├── gen_stats.py ──► stats/<bench>.sf1.json   (NDV, string length; committed)
          │     └── test_cardinality_corpus.py, UPDATE_CANONICAL=1, with tp1-single's
          │         .plans.txt and -mini.cpu.txt
          │           └──► scripts/exec_model/testdata/goldens/<bench>/tp1-single.cardinality.txt
          │
          ├── scripts/exec_model/run.py, with each <mode>.plans.txt, the sidecar and cost_model.conf
          │     └──► scripts/exec_model/testdata/goldens/<bench>/<mode>.cpu.txt, .optimizer.txt, .plandiff.txt, .costs.txt, .optimizer.md
          │           └── with each -mini.cost.txt and <q>.duckdb_cost.txt
          │                 └──► scripts/exec_model/testdata/goldens/cost_report.html
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

- **The registry is the write-side authority, so it moves first.** `declared_sections` reads
  `cost-registry.csv`, not `corpus_cases.inc`, so a golden section for a newly enabled cell cannot
  be written until the CSV says `enabled` — and a regeneration run reports that case **`ok` while
  writing nothing**, which reads exactly like a cell that had nothing to write. Order is registry,
  then regenerate, then read the file back. This cost `repartition-keys` a round.
- **Read `git diff --numstat` after any scripted edit of `cost-registry.csv`.** Python's
  `csv.writer` rewrites every line with CRLF, and the whole suite still passes: nothing in the tree
  reads the line endings. The numstat is what shows 198 lines changed where you meant 11.
- **The registry's `stored` column says what each side's result section holds**, after
  `tickets`: `result` or `fingerprint` where every side with a section agrees, else
  `cpu|duckdb|gpu` with `—` for a side with none (`skipped:` and `failed:` count as none) and
  `mixed` for device modes that disagree; `na` for a row with no corpus line. A recording that
  moves a section across the cap moves the cell, and
  `the_registry_stored_column_matches_the_goldens_in_both_directions` names the side until it
  does. The same test refuses a line that says `gpu: fingerprint` while its cell's gpu part is
  anything but `fingerprint`, such as a line whose device cells are off. The report shows it as
  Result stored.
- **The prototype's cardinality goldens follow the engine's** —
  `scripts/exec_model/testdata/goldens/<bench>/tp1-single.cardinality.txt` holds each join's
  true rows from tp1-single's `-mini.cpu.txt` beside the estimate from the plan and the sidecar,
  so regenerating that `.plans.txt` or `.cpu.txt`, or a sidecar, obliges it too:
  `UPDATE_CANONICAL=1` over the sf1 tables.
- **A plan golden answers to the prototype too.** Adding or regenerating a `<mode>.plans.txt`, of
  any dataset, goes through these gates, and a node kind, type or expression the prototype's reader
  (`scripts/exec_model/plans/`) does not know turns them red:
  - the cheap tier, in the cost-report job: every section parses, every expression renders back and
    lowers, every plan builds with the engine's layouts, every join cluster disassembles to the
    same layout, and every other mode's joins are among tp1-single's.
  - dataset-matrix: `test_ordered_corpus` orders every tpch and tpcds plan at every mode, and the
    cardinality golden above is compared whole, so a new query with a join is red until rewritten.
  - the cost-report crate: `every_registry_query_has_a_duckdb_cost` wants a `.duckdb_cost.txt` for
    each tpch and tpcds registry row, so a new query there needs `gen_duckdb_cost.sh --gen` first.

  With `PEACOCK_DPHYP_LIB` set as `scripts/exec_model/README.md` builds it; the three after the
  loop read the sf1 tables, and `gen_duckdb_cost.sh` wants DuckDB 1.5.4 and pyarrow 19.0.1:

  ```
  for f in plans/test_engine_plan plans/test_engine_expr plans/test_engine_ir \
           plans/test_engine_corpus optimizer/test_disassembly_corpus \
           optimizer/test_cardinality_modes; do
    python3 scripts/exec_model/tests/$f.py || break
  done
  python3 scripts/exec_model/tests/optimizer/test_ordered_corpus.py
  UPDATE_CANONICAL=1 python3 scripts/exec_model/tests/optimizer/test_cardinality_corpus.py
  DUCKDB=<duckdb 1.5.4> testdata/gen_duckdb_cost.sh --gen    # a new tpch or tpcds query only
  cargo test -p cost-report every_registry_query_has_a_duckdb_cost
  ```
- **The engine's `<mode>-<tier>.cost.txt` is a pure function of its `.cpu.txt` and
  `cost_model.conf`** — regenerating one `.cpu.txt` obliges the sibling `.cost.txt`, and
  `test_cost_model` re-derives every one of them, so a hand-edited `.cost.txt` goes red. The
  prototype's `<mode>.costs.txt` is not: it prices runs no file holds, so only `run.py` makes it.
- **`cost_model.conf`'s category list is read back from the prototype's committed costs.**
  Each run's line in a `<mode>.costs.txt` names the conf's categories in order, and
  `cost_report.py` refuses a file whose names or order differ. So renaming, adding, dropping or
  reordering a category turns `test_cost_report.py` and the cost-report job's "Generate report"
  step red until a full `python3 scripts/exec_model/run.py` writes them again: 620 runs, about
  five hours at `--jobs 1`, over the sf1 tables with `PEACOCK_DPHYP_LIB`, under Python 3.13,
  pandas 3.0.6 and pyarrow 25.0.1. Moving a node kind between existing categories raises nothing:
  the report re-totals the bytes each category held before the move, so its figures are stale
  until that run.
- **A device never authors a golden.** The device tier reads the sections the cpu tier
  wrote, which is what makes a divergence a red test rather than a quietly rewritten
  expectation.
- **`golden(sort, exact)` on a `data_fusion_subset` line is the extra claim that the query's own modes
  return the same rows**, and only the device compare tests it — the cpu tier asserts the result
  section at one mode, `authoritative_mode`'s. So that pairing is unchecked until a card runs,
  which is worth knowing while the `gpu-tests` job is red. `tpch/scan-limit` is the only such line,
  measured on 2026-10-09 and again on master on 2026-10-16.
- **The byte-level golden needs a second, deliberate variable**
  (`PEACOCK_REWRITE_RECIPE_BYTES=1`). Under plain `UPDATE_CANONICAL=1` the test verifies
  instead of rewriting, and says so — a bulk regen that moved the wire format goes red
  during the regen, before the goldens are pulled home. It pins what the C++ is handed, and
  it is the file a bulk regen must not quietly rewrite: the diff would come home among the
  others.
- **Two writers can lose a section** ([#213](archive/archived-tickets.md#t213)): the merge locks the inode
  it opened and the publish renames over it, so a whole-corpus regeneration can drop one
  query's section from a `.cpu.txt`. Read the regeneration's `git diff --stat` for a section
  that vanished, and refill it with `PCK_UPDATE_SECTIONS=1` and `--exact <case>`.
- **The `.duckdb_cost.txt` path is re-runnable without DuckDB**: `--extract-only` rebuilds
  the goldens from the committed profiles plus the parquet, so only a genuine oracle change
  needs the 1.5.4 pin.
- **`gpu-result.txt` is regenerated whenever a device answer might move.** After a device
  code change, a cell turned on or off, or a cuDF update, run a cycle with
  `PCK_WRITE_GPU_RESULT=1`, then `./scripts/build-test-shadgpu.sh --pull-results`, which brings
  home only a file newer than the gate run's launch and dies if none is — the push mirrors the
  committed copy up, so existence alone would report a refresh that never happened. Read
  `git diff testdata/goldens/*/gpu-result.txt` before committing it, the `cudf=` line included. A
  value that does not name the cuDF the binaries were built against is refused before anything is
  written, so `PCK_WRITE_GPU_RESULT=1` on a 26.02 host fails rather than overwriting 25.02's
  answers. A moved section the
  change did not intend is a finding, not a regeneration. Float cells move a little from run
  to run — GPU reductions are not reproducible, nor across cuDF versions — which is why
  nothing compares the file with its previous version; the `duckdb_gpu_*` cases and
  `every_enabled_device_cell_has_its_gpu_result_section_and_no_other` are what CI checks. The
  writer keeps a section the registry still enables and drops one it does not, so a cell turned
  off loses its section on the next recording cycle — read the diff for a disappearance too.
- **An over-cap result section is a fingerprint**, written by both result writers and
  compared under `duckdb_fingerprint`, or on the device's side alone under `gpu: fingerprint`: `rows=`, each column's `nonnull` and — where the
  column is a declared float — its `sum`/`min`/`max`, then a SHA-256 over the rest. A float
  is the one type the two engines print to their own precision; a decimal renders at its
  declared scale on both sides, so it is hashed. That is what makes an over-cap join checked
  row for row rather than by sums.
  Regenerating one is `PCK_UPDATE_SECTIONS=1` with a filter naming the query, as any other
  section is.
- **`duckdb_result.py` needs the `duckdb` Python module at 1.5.4** and refuses any other. No CI
  step installs it: `python3 -m venv <dir>`, `<dir>/bin/pip install duckdb==1.5.4`, and run the
  script with `<dir>/bin/python`.

### Benchmark data flow

A second tree with its own producers. Nothing here is a golden — no run asserts against it
— so the arrows say which script writes each file rather than which test reads it.

```
tpch.sf40, tpcds.sf40 (on the GPU host, outside the repo; each symlinked in as testdata/<dataset>.sf40)
  │
  ├── build-test-shadgpu.sh --run-benchmarks        (peacock_gpu_benchmarks, event timing;
  │   build-test.sh --gpu --run-benchmarks           the same binary on a 26.02 host)
  │     ├──► benchmark-results/<device>/<dataset>.sf<sf>/<mode>.benchmark.txt   the chosen run, per node
  │     └──► calibration/records.tsv                one row per cuDF call × execution
  │           └── --pull-benchmarks brings both home  (the record gitignored), and
  │               split_record.py: a whole, plain, passing run per dataset, or nothing
  │                 ├──► s3://calibration/<device>/<dataset>.sf<sf>/<sha256>.tsv
  │                 └──► calibration/records.sha256      that sha256, committed with the tree
  │                       └── fetch_record.sh <device>/<dataset>.sf<sf>
  │                             └──► calibration/<device>/<dataset>.sf<sf>/records.tsv   (gitignored)
  │
  └── create_nsys_profile.sh [--host …]             (the same binary, under Nsight)
        ├── --trace     nvtx and cuda only; PEACOCK_BENCHMARK_CAPTURE=trace
        │     └──► calibration/capture.sqlite          (not committed)
        │           └── nsys_calls.py × goldens/<dataset>.sf1
        │                 └──► calibration/calls.tsv   what one ABI call splits into, per case
        │
        └── --metrics   the same cases under the memory counters, one capture per case;
              │         PEACOCK_BENCHMARK_CAPTURE=metrics
              ├──► calibration/capture-metrics.sqlite  on the host, one case's, deleted after its join
              ├──► calibration/records-metrics.tsv     stays on the host; its microseconds are unusable
              └── nsys_hbm.py --case, on the host (that capture × that case's pinned rows)
                    └──► calibration/hbm-parts/bench_<dataset>_sf<sf>_<query>_<mode>.tsv
                          └── nsys_hbm.py --parts (every part × every pinned record)
                                └──► calibration/hbm.tsv   a row per call, its executions' median

calibration/<dataset>.sf<sf>/records.tsv (one --record each) + calibration/hbm.tsv
  └── plot.py ──► calibration/plots/{load,compute,spread,query,icicle,hbm}/*.png
                  calibration/plots/index.html      one call, every panel
```

`<device>` is the card's slug, `h100-80gb-hbm3` for `NVIDIA H100 80GB HBM3`. The H200's two
records predate it: they are keyed, pinned and fetched as `<dataset>.sf<sf>`, one level up.
`create_nsys_profile.sh` reads the pinned records at that level only,
`calibration/<dataset>.sf<sf>/records.tsv`, one per dataset the case list names.

What the arrows are there to make checkable:

- **Every file says what it ran on.** A trailer and a record heading end with `device=`,
  `driver=`, `cuda=`, `cudf=` and `host=`, in that order:
  - the device is `cudaDeviceProp::name` and CUDA is `cudaRuntimeGetVersion` as `major.minor`,
    both read through `peacock_device_info`;
  - the driver is the kernel module's release, `580.95.05`, from `/proc/driver/nvidia/version`;
  - cuDF is the release the binary was built against, `25.02.02`;
  - the host is the kernel's host name. Inside a container that is the container's own name,
    unless the container was started with `--hostname`.

  The tree is one directory per device, so two card models never write one file. An append to a
  record under another device, driver, CUDA, cuDF or host is refused. `nsys_hbm.py` refuses a
  capture unless every GPU it lists is the record's device. A record naming no device predates
  provenance and is taken as the H200's (`PREDATES_PROVENANCE`): both hosts that ran the
  benchmark before, shad-gpu and verda-gpu, are H200s, and only shad-gpu published.
  The H200 files predate all this and carry none of the five.
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
- **The derived text is committed; the record and the captures are not.** The
  `.benchmark.txt` tree, `calls.tsv`, `hbm.tsv` and every panel are in git, rewritten in place
  by each collection, so `git diff` shows how the numbers moved. The record is 26 MB at sf40
  and grows with every case, so it lives in S3 and git holds its sha256: `--pull-benchmarks`
  writes both, and the commit that moves the tree moves the pin. The two `.sqlite` exports are
  undiffable binary that only the scripts above read. The counters one grows ~27 MB a second
  of run with the whole gh100 set at 20 kHz, ~100 GB over the corpus, so it is taken a case at a
  time and never leaves the host: each case is joined there and deleted before the next, and
  only `hbm.tsv` comes home.
  `testdata/.gitignore` is deny-by-default over `calibration/`, re-including the pin and the
  derived files only. `calls.tsv` and `hbm.tsv` in git cover three cases of an earlier run —
  q6 at `tp1-single` and `tp4-sized`, q19 at `tp1-single` — whose record is in git's history.
  The panels are drawn from the two published records, all twenty-six cases; `hbm.tsv` joins a
  record by call, not by execution, so its three cases are drawn beside them, and it is still
  a row per execution. The two derived files cover the H200's twenty-six only after step 5 runs
  over them, and that step runs from a checkout carrying the 26-case list: a5c852e4 on
  master, whose case lines are `h200/cases.csv`'s rows. `create_nsys_profile.sh --metrics`
  takes every case of the list and checks its merge against the pinned records, which hold the
  twenty-six.
- **`hbm.tsv` is a row per call, not per execution.** Each reading is the median over the
  call's executions, and `executions` says how many. A row per execution would be ~255
  thousand rows and 25 MB over the corpus; a row per call is the record's 25,522 calls.
  Nothing the file is read for is lost. Across the ten executions of the three-case capture, a
  call with ten samples or more sits a median 2.3 % from its own median in any one execution.
  Weighted by bytes, one execution sits 0.5 % from the ten's median. Under ten samples a call
  moves by up to its whole value from one execution to the next, at any count. The median and
  not the mean, since one execution in ten can read several times the others. `plot.py` folds
  a file still a row per execution the same way, so either shape draws the same picture.
- **A record is published whole or not at all, and keeping it is a commit.** Every pull
  uploads under the record's own sha256, so nothing in the bucket is replaced: committing
  `records.sha256` beside the tree keeps the run, and `git checkout` of it drops the run,
  leaving an unused object. A run truncates the record when it starts, so a filtered run's is
  a fraction of the full one under the same name; `split_record.py` cuts the pulled record
  into one file per dataset and refuses the lot unless each holds exactly the (query, mode)
  set `corpus_benchmark_cases.inc` declares, under `capture=none`. Before that, the pull
  requires the host's record to be the one it pulled (no run started since) and the run to have
  exited 0, since a failed case is missing from it. A refused record leaves the
  pin as it was, and the pull exits 1 with the tree home. A record that names no device, or two,
  is refused too: its rows are filed under the device that measured them. Only shad-gpu publishes,
  through `build-test-shadgpu.sh`. `build-test.sh` keeps what it pulls, and nebius-gpu runs no
  benchmark, so while shad-gpu is down no record is published.

## CI structure (`.github/workflows/pipeline.yml`)

`pipeline.yml` runs on pushes to master and on every PR, but not on documentation —
`**.md` and `llm-wiki/**`. It takes two layers: `paths-ignore` skips a wholly-documentation
diff, and the **changes** job skips a documentation-only push to a PR that carries code,
which `paths-ignore` cannot see because it judges the whole PR diff. Both fail open. A
`workflow_dispatch` always runs in full.

No peacockdb-core test reads `llm-wiki/`, so a tickets-only commit cannot turn the rust tier
red. cost-report's tests do read the ticket files; a ticket move that breaks one fails on the
next code push, which runs the cost-report job.

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
  The steps that do not need cuDF run on the 25.02 leg alone, since a rust-only test cannot see
  cuDF and a second leg would report the same failure twice: `test_cost_model`,
  `test_golden_format` and `test_corpus_goldens`, and the exec-model python, which reads the
  generated sf1 tables and builds DPhyp for itself.
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
- **gpu-tests**, "GPU Tests (nebius-gpu)" (needs cpp-build-2502) — runs on a push to master
  and on a `workflow_dispatch`, never on a PR. A PR still runs the build job. A dispatch is
  `gh workflow run pipeline.yml --ref <branch>`, and it runs the whole pipeline. The job sits in
  the `nebius-gpu` Environment and the `nebius-gpu` concurrency group, one run at a time across
  every ref, with a 150-minute limit. Its steps:
  - the actor check against `GPU_ALLOWED_ACTORS` (*Nebius host*), before any project code runs;
  - the `cpp-install-25.02` artifact, unpacked, and `nebius-setup`. The artifact is a tarball
    of `cpp/install/`, because upload-artifact copies every symlink's target in full and drops
    the exec bits;
  - `nebius.sh start`, retried on exit 75 for 30 minutes, then a failure;
  - `scripts/nebius/ci-ship.sh`, with a 50-minute limit: `cpp/install/`, `nebius-test.sh`
    and the testdata subset rsynced into `~/peacockdb-ci/cache` on the VM. The cache persists
    between runs, so only what changed crosses. The runner-to-VM path is slow and lossy, 0.24
    MB/s for one stream when measured, so the script runs four rsync streams at once, with
    zstd, `--checksum`, and retries that resume through `--partial-dir`. Then a copy of the
    cache becomes this run's `~/peacockdb-ci/run-<run id>-<attempt>`, and sf1 is generated there;
  - `nebius-test.sh run --foreground`: the L40S list (*Nebius host*). Its guards each closed a
    hole that shipped. Every `peacock_*_tests` binary runs by glob, because a hand-written list
    once let `peacock_tpchv_tests` be built and shipped but never run. A binary that runs 0
    tests fails, because one whose tests all skip exits 0. The rust binaries run with
    `--test-threads=1`, because cuDF/RMM share one process-wide pool. `peacockdb_core_gpu_lib`
    alone takes `gpu_tests::`, which selects the device rung and leaves its CPU and FFI rungs
    to dataset-matrix. `peacock_gpu_benchmarks` runs under `--skip bench_`;
  - whenever the start step ran: kill the run's processes on the VM, remove the run directory
    and soft-stop the VM. A cancelled run's tests would otherwise keep running, and a running
    test holds off the idle watcher.

  No glibc patch: the VM has glibc 2.39 and CI's binaries need 2.35. No sf40 check: the sf40
  pair is out on an L40S.
- **nebius-watch.yml** — hourly and on dispatch: the hourly checker, which hard-stops an
  unused Nebius VM, one matrix leg per VM (*Nebius host*). GitHub reads its schedule from master
  only.
- **images.yml** — on dispatch only: builds and publishes the build and run images (*Images*).
  GitHub can dispatch it only once master holds it.
- **cost-report** — the report crate and its inputs, and the cheap script tests: python
  `testdata/test_*.py` by glob (the two DuckDB generators' own tests), the report widget's
  `scripts/test_ticket_widget.py`, the `scripts/exec_model/tests/**/test_*.py` prototype set
  less the files its step names as run elsewhere (with DPhyp built for it), the
  `scripts/calibration/tests/test_*.py` cases over a synthetic capture, the Nebius
  script tests `scripts/nebius/tests/test_*.py` with `shellcheck` over the Nebius scripts
  (*Nebius host*), `cargo test -p cost-report --bin cost-report`, `-p peacockdb-dphyp`,
  `--test test_source_formatting` (the CI wiring, layout and checkout guards, named so the
  wiring guard counts it) and peacockdb-core's `--test test_crate_privacy`. Then report
  generation, the PR-comment upsert and the cost-regression gate against the base SHA. Every
  query whose PeacockDB CPU Σout grew is a regression and shows 🔴 in the comment, but the job
  fails only on growth of more than 10% (`REGRESSION_FAIL_PCT` in `cost-report/src/main.rs`), or
  on growth from a zero base. A change to how rows split across lanes moves Σout by a few bytes
  of padding without changing an answer, and the tolerance is what lets that through.
  Generation also renders the prototype's `cost_report.html` from the committed files, its links
  at the commit, into `report/join-optimizer/`. On a master push it uploads the Pages artifact,
  so the join optimizer's page is published beside the engine's. The comment has a 65,536-byte
  body to fit — `COMMENT_MAX_BYTES` in `cost-report/src/main.rs`, asserted over the real
  registry — so every anchor but the query cell is dropped from the markdown. The file's FIRST
  LINE is its sentinel and the workflow reads it from there, so the string exists once.
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

Which Rust targets run where is not folklore — `test_source_formatting` fails when a target
exists that no workflow step names. Doctests are the one class outside that guard (#128).
The python steps need no such guard: `testdata/test_*.py`, the prototype step's
`**/test_*.py`, and the calibration and GPU host script steps glob, and each errors when its glob
matches nothing; the prototype step also fails on any `elsewhere` name that no longer exists, and the
dataset-matrix step names its files, so a missing one fails it. Inside a prototype file the equivalent
hole — a test defined below the `__main__` footer, which pytest collects and direct execution would
not — is closed by `tests/harness.py`, which reads the source back and fails naming what it missed.

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
`cpu_case`, `gpu_case`, `duckdb_case`, `duckdb_gpu_case` and `authoritative_mode`, whose
signatures name no engine type; the other suites name `test_support` directly. `tests/common/` holds only the two case
lists, `corpus_cases.inc` and `corpus_benchmark_cases.inc`: each corpus binary `include!`s its
own so `corpus_query!` expands, and `inventory::submit!` runs, in that binary alone.

## Local build workflows and caches

One cargo target dir per workflow — this is what prevents cache thrashing (feature flags
and `cudf_ROOT` changes bust fingerprints; sharing a dir means recompiling the DataFusion
stack on every switch):

| Workflow | Command | C++ build dir | Cargo target dir |
|---|---|---|---|
| rust-only (golden regen/verify, fastest loop) | `cargo test --features rust-only -p peacockdb-core --test <target>` (drop `--test` and it runs every target that compiles, not a tier — see above) | — | `target/` |
| cost-report | `cargo test -p cost-report`; preview: `scripts/cost-report-preview.sh`; checked-in report: `python3 scripts/ticket_widget.py` → `testdata/report.html`, laid out from `cost-report --widget-md` (the comment's tables whole), regenerated by any commit that changes `testdata/cost-registry.csv` | — | `target/` |
| C++ only, cudf 25.02 | `scripts/build.sh --cudf_ROOT <rapids-cuda-12.2> --gcc-version 12 ...` | `cpp/build` | — |
| C++ + staged Rust bins, cudf 26.02 | `scripts/build-test.sh --build` (drives cmake directly) | `cpp/build26` | `target-cudf-<basename cudf_ROOT>` |
| Rust + cudf (FFI) | via build-test scripts, or manually with `scripts/cargo-cudf.sh` | cargo OUT_DIR | `target-cudf-<basename cudf_ROOT>` |

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
  `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run`
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
- cuDF version scope: a functional GPU run on **25.02** (the GPU host) is sufficient
  verification; **26.02 needs only to compile** (CI builds both legs).

## Memory constraints (15 GiB-class hosts)

- **At most one binary links at a time**: the cmake configures set a Ninja
  `link_pool=1` job pool (parallel links OOM the host; compiles stay parallel). The
  build-test scripts also serialize rust test-binary builds one `--test` per invocation.
- Cold cudf cargo builds are throttled to `CARGO_BUILD_JOBS=3` on <20 GiB hosts
  (override via env).
- Full local CPU suite: run with `-- --test-threads=2` or less; the heavy golden suites
  OOM at default parallelism.

## Nebius host

Three Nebius VMs, each with its own role. A chain needs no host override: these roles are the rule.

| VM | instance | region | profile on dev | machine | role |
|---|---|---|---|---|---|
| nebius-gpu | `computeinstance-e00tnrse7ayntnzcyt` | eu-north1 | the active one, `editor-eu` | L40S, 46 GB | correctness: the L40S list below; no sf40, no benchmarks |
| nebius-h100 | `computeinstance-e00e6yena4nzjgaym0` | eu-north1 | `editor-eu` | H100 80GB HBM3, 16 vCPU, 196 GB | sf40, the benchmarks and Nsight, through `scripts/env/run-gpu.sh` |
| nebius-cpu128 | `computeinstance-e01t2mm4265y8x88ey` | eu-west1 | `eu-west1` | 128 cores, no card | DuckDB, and the H100's staged install ("The CPU-VM build") |

Each is a paid VM. A VM starts right before its first remote command and stops right after its
last pull. It is never left up across a local build, CI, a review or the human. Two guards stop
it when nobody does. The login is `dmitry` on all three, with one key: `~/.ssh/id_ed25519` on
dev, `NEBIUS_GPU_SSH_KEY` in CI.

Every script names a VM by `nebius.sh`'s two options. With neither, it is nebius-gpu:

```
scripts/nebius.sh <verb>                                                                    # nebius-gpu
scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu <verb>  # nebius-h100
scripts/nebius.sh --instance-id computeinstance-e01t2mm4265y8x88ey --profile eu-west1 <verb>   # nebius-cpu128
```

**nebius-gpu** is in project `project-e00tdypyen10mmyep61c8q`. Ubuntu 24.04 and glibc 2.39, so no
binary needs a glibc patch. cuDF 25.02 in `~/data/miniforge3/envs/rapids-cuda-12.2`;
`. ~/peacock-env.sh` sets up a shell. cuDF 26.02 there is out ([#260](tickets/system-hardening.md#t260)).
The device tier runs there while shad-gpu is down. Only GPU work goes there: the device build and
the device runs. Every CPU build and run stays local, or on verda.

- **It is preemptible.** It can stop, change IP or vanish mid-step. Preemption is a stop: the
  boot disk keeps its trees, builds and data.
- **An IP can change on every start**, on any of the three. Never write one down. `nebius.sh`
  asks the API for it on every call.

**nebius-h100** holds no tree. `run-gpu.sh` ships each run's install and testdata to it.
Ubuntu's glibc 2.39, driver 580.173.02. Its image already has Docker, the NVIDIA Container Toolkit
and nsys, and its counters are open to non-root, so its first `prepare` needed no restart.
**nebius-cpu128** is a containers VM: `provision-vm.sh` readies it ("Containers"). It ran clean
there on 2026-11-04, with no fix.

### The scripts

`scripts/nebius.sh [--instance-id <id>] [--profile <name>] <verb>` is the one door to every VM.
The options come before the verb. `--instance-id` takes a `computeinstance-…` id; without it the
VM is nebius-gpu. `--profile` goes to every `nebius` CLI call; without it the CLI uses the active
profile. A malformed or repeated option is exit 64.

- `status`: the state, and the IP when RUNNING. `ip`: the IP alone.
- `start`: starts the VM and waits for RUNNING, then for ssh. On a running VM it only waits for
  ssh. It waits out a STOPPING VM first. If the VM is RUNNING but ssh never answers, it exits
  1, and it stops the VM again if this call started it.
- `stop`: a soft stop. It refuses, exit 2, while a process is on the card. On a VM without
  `nvidia-smi` it refuses while `duckdb` runs. An unreachable VM is stopped on the API's word.
  `stop --force` stops it regardless.
- `ssh <cmd…>`: runs a command on the VM. `rsh` is the same for `rsync -e`.
- `install-watch [--key-file <pem>]`: installs the idle watcher and its API credentials on the
  VM ("The two idle guards").

| exit | meaning | what to do |
|---|---|---|
| 0 | done | |
| 2 | soft stop refused: a process is on the card, or DuckDB on a CPU VM | leave it; someone's run is on it |
| 75 | no capacity or quota | retry later |
| 69 | the instance no longer exists | stall; rebuilding it is the human's |
| 1 | anything else; from `start`, most often the VM up but unreachable over ssh | read `~/.config/peacock/nebius/nebius.log`. From `start`: stall with that reason, do not retry |

**Host keys.** Every ssh uses a per-instance alias in one known-hosts file under
`~/.config/peacock/nebius/`, so a new IP never warns. The alias is `nebius-gpu` for nebius-gpu
and `nebius-<instance id>` for any other VM, so one VM's new key never touches another's. When
the key itself changes, the script asks the API whether the IP is this instance's. Only then does
it replace the stored key, that VM's line alone, and it logs both fingerprints. Never edit
`~/.ssh/config` for a VM, and never accept a key by hand.

**install-watch gives the VM one profile**: `--profile`'s, or the active one. It becomes root's
only profile. It is cut from a temp copy of `~/.nebius/config.yaml` on dev, so no other profile
ever reaches the VM. Only the CLI reads the copy. A profile keeps its key in one of two ways:

- **Embedded** (editor-eu). Run install-watch without `--key-file`. Given one, it refuses, exit 1,
  before any VM work: another PEM would replace a working key.
- **In a PEM file** (eu-west1, `~/.nebius/eu-west1-sa.pem`). Pass that file as `--key-file`. The
  CLI embeds it into the copy and drops the path, so the VM needs no PEM, and the PEM never
  leaves dev. Without `--key-file` such a profile is refused.

install-watch writes the install file, `/etc/peacock-gpu-idle.json`, before it starts the timer.
It ends by running root's profile the way the watcher does, and fails loudly if that profile
cannot reach the API. For the three VMs:

```
scripts/nebius.sh install-watch
scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu install-watch
scripts/nebius.sh --instance-id computeinstance-e01t2mm4265y8x88ey --profile eu-west1 install-watch --key-file ~/.nebius/eu-west1-sa.pem
```

`scripts/nebius-sync.sh [--instance-id <id>] [--profile <name>] [--dest <dir>] [--no-data]`
copies the working tree, uncommitted, to `~/<dir>` on the VM, and prints `<dir>`. The two
options go to every `nebius.sh` call. An unknown option is exit 64. The default dir is
`peacockdb-<the worktree's directory name>`. Ignored build dirs, target dirs and generated data
survive on both sides. It retries a failed rsync. It generates the sf1 tpch and tpcds data on the
VM if it is missing; `--no-data` skips that. Do not commit to build.

Build on nebius-gpu with `build-test-shadgpu.sh --build` only. Its `--run`, `--push-binaries` and
`--patch` go to shad-gpu.

`scripts/nebius-test.sh` runs on the VM, against a tree that holds `cpp/install/`:

- `run <tree>` runs the list detached under `nohup`, so a dropped ssh does not kill it. Logs
  and rc files go to `<tree>/.gpu-test/`. `run --foreground <tree>` waits; CI uses it.
- `status <tree>` prints each binary's rc, or `running`, then `ALL rc=<n>`, `ALL running` or
  `ALL died`. `ALL died` means a preemption or a reboot killed the run: rerun it.

A GPU step, end to end, on nebius-gpu (`<dest>` is what the sync printed):

```
scripts/nebius.sh start
scripts/nebius-sync.sh
scripts/nebius.sh ssh 'cd ~/<dest> && . ~/peacock-env.sh && ./scripts/build-test-shadgpu.sh --build'
scripts/nebius.sh ssh '~/<dest>/scripts/nebius-test.sh run ~/<dest>'
scripts/nebius.sh ssh '~/<dest>/scripts/nebius-test.sh status ~/<dest>'   # until ALL rc=
scripts/nebius.sh stop
```

If the VM is lost mid-step: `status`, `start` again, sync, rerun the step.

### The L40S list

`nebius-test.sh` holds the one list, for the developer and for CI:

- every `cpp/install/bin/peacock_*_tests`, except the sf40 pair, `peacock_tpch_tests` and
  `peacock_tpchv_tests`;
- every staged binary in `cpp/install/rust-tests/`, with `--test-threads=1`;
  `peacockdb_core_gpu_lib` takes `gpu_tests::`, and `peacock_gpu_benchmarks` takes
  `--skip bench_`. `PCK_TEST_FILTER` narrows every rust binary but the lib.

A binary that runs 0 tests fails, and so does an empty glob or a segfault in any log. Every
binary runs even after one fails.

The sf40 benchmark cases (`run-gpu.sh … run bench`) and Nsight captures run on the H100, never on
the L40S ("The H100's device run"). The sf40 C++ pair, `peacock_tpch_tests` and
`peacock_tpchv_tests`, has no H100 path yet: `run-tests.sh` skips it in every mode. Only shad-gpu
runs it, and shad-gpu is down. H200 timing is out.

### The H100's device run

`scripts/env/run-gpu.sh --instance-id <id> [--profile <name>] [--dir <i>] [--cudf <v>] <verb>`
runs the device tiers and the benchmark in the run image ("Images"). The two options go, as
given, to every `nebius.sh` call. `--instance-id` is required: without it `nebius.sh` means
nebius-gpu, which runs no sf40, so `run-gpu.sh` refuses, exit 2, before any call.
`run-gpu.sh` reaches the VM only through `nebius.sh … ssh` and `rsh`, and never starts or stops
it: the caller does. `<i>` names the VM's directory, `~/peacock/<i>/`: this tree's directory name
by default. `<v>` is the cuDF version, 25.02 by default.

- `prepare` readies the VM. It installs Docker, the NVIDIA Container Toolkit and nsys if missing,
  and opens the GPU's counters to non-root. It pulls the build and run images, each refused unless
  its `peacockdb.inputs` label is this tree's. It generates sf1 in the build image, and fetches
  sf40 with `fetch-sf40.sh`, dev's `~/.config/peacock/peacock.env` on stdin. It copies the run
  image's `/opt/cudf` to `~/peacock/cudf-<v>/`, for Nsight on the VM itself.
- `ship [--install <dir>]` sends the staged install (this tree's `cpp/install` by default) and the
  tree's committed testdata to `~/peacock/<i>/`, with the datasets linked in.
- `run <list|filter|bench>` first reads the VM's run image's `peacockdb.inputs` label, with no
  pull, and refuses, exit 2, an image built from other inputs than this tree: `prepare` again.
  Then it runs the run image's `run-tests.sh <mode>` as container `peacock-<i>`, and follows its log. The log is kept in `~/peacock/<i>/logs/<mode>.log`. `PCK_TEST_FILTER` and
  `PEACOCK_BENCH_RESUME` pass on. If ssh drops, the container goes on, and the next `run` refuses,
  exit 2, until it ends.
- `pull` brings `testdata/benchmark-results/` and `testdata/calibration/` into this tree. A file
  edited here since `ship` is kept. The runs' logs go to `~/.cache/peacock/run-gpu/<i>/`. `pull`
  leaves the VM's `calibration/records.tsv`, and `bench` refuses one with rows: remove it on the VM
  after the pull.

| exit | meaning |
|---|---|
| 0 | done |
| 1 | the run failed |
| 2 | bad input (`nebius.sh`'s 64 too), no `--instance-id`, a stale image, a run in progress, or no `prepare` or `ship` yet |
| 3 | the VM's driver is older than the run image needs |
| 4 | the counters open only after a restart: `nebius.sh … stop --force`, `start`, then `prepare` again |
| 75, 69 | `nebius.sh`'s |

`scripts/env/stage-install.sh [--cudf <v>]` makes the install `ship` sends. It runs at the tree
root inside the build image of `<v>`, and refuses another image (exit 2). It builds and installs
the C++ into `cpp/install`. Then it stages, from empty, the gpu test binaries into
`cpp/install/rust-tests/` and the release `peacock_gpu_benchmarks` into
`cpp/install/rust-benchmarks/`, each stripped of debug info. cargo builds into `target-img-<v>`
unless `CARGO_TARGET_DIR` is set. Exit 1 is a failed build. It runs on nebius-cpu128 ("The CPU-VM
build").

A run, end to end, once the CPU-VM build has staged the install on dev:

```
N="scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu"
G="scripts/env/run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu"
$N start
$G prepare          # exit 4 the first time: $N stop --force; $N start; $G prepare again
$G ship --install <the staged cpp/install>
$G run list         # or: PCK_TEST_FILTER=<f> $G run bench
$G pull
$N stop
```

Nsight runs on the VM itself, over the same door:
`create_nsys_profile.sh --ssh "$N rsh" --host nebius-h100 --remote-dir /home/dmitry/peacock/<i>
--remote-cudf-root /home/dmitry/peacock/cudf-25.02`. Its HBM defaults are an H200's. The
metric set `gh100` fits the H100 too, but the peak does not: set `PCK_BENCH_HBM_PEAK_BW=3.35e12`.
That is the H100's 2619 MHz memory clock × 2 × 5120 bits, NVIDIA's 3.35 TB/s.

On 2026-11-04 the first `prepare` took about 21 min: 8 for the two images, 12 for sf40. A second
took about 1 min. The sf1 list ran green in 14 min, each binary at the count this page gives. One
benchmark case, `bench_tpch_sf40_q6_tp1_single`, took 97 ms a run.

### The CPU-VM build

Binaries for H100 runs are built on nebius-cpu128, from the workspace as it stands, uncommitted,
never from a `git pull`: a clone on the VM holds only what was last pushed. Local and verda builds
stay for development.

```bash
C="scripts/nebius.sh --instance-id computeinstance-e01t2mm4265y8x88ey --profile eu-west1"
$C start
D=$(scripts/nebius-sync.sh --instance-id computeinstance-e01t2mm4265y8x88ey --profile eu-west1 --no-data)
scripts/env/check-build-image.sh --instance-id computeinstance-e01t2mm4265y8x88ey --profile eu-west1
$C ssh "cd ~/$D && docker run --rm --user \$(id -u):\$(id -g) -e HOME=/tmp -v \$PWD:\$PWD -w \$PWD ghcr.io/asymptote-tech/peacockdb-build:cudf-25.02 scripts/env/stage-install.sh"
rsync -a -e "$C rsh" "nebius-cpu128:$D/cpp/install/" <local install dir>/
$C stop
```

`check-build-image.sh` pulls the build image on the VM and refuses it, exit 2, unless its
`peacockdb.inputs` label is this tree's. The comparison runs on dev: the synced tree has no `.git`,
and the build image's inputs include the `third_party/cudf` commit the git index pins. Exit 2 is a
refused image or bad input; any other failure is `nebius.sh`'s exit.

Any other tree file a CPU-VM step reads reaches the VM the same way, with `nebius-sync.sh …
--no-data`. No cargo registry is mounted and ccache is in `/tmp`, so every build fetches the
crates and compiles the C++ cold. `target-img-25.02/` is gitignored, so it survives the next sync.

### The two idle guards

**The watcher** is `scripts/nebius/idle_watch.py`, run every minute on the VM by the
systemd timer `peacock-gpu-idle.timer`. The timer survives reboots. It reads the VM's instance
id and kind from the install file, `/etc/peacock-gpu-idle.json`:
`{"instance_id": …, "kind": "gpu"|"cpu"}`. install-watch writes it: kind `gpu` if the VM has
`nvidia-smi`, else `cpu`. Without the file the watcher is nebius-gpu's, kind `gpu`. A file it
cannot read or trust gives no id: the watcher stops nothing, logs why, and the hourly checker is
the stop.

On a `gpu` VM, test activity is a process on the card or a running test binary. Build activity
is `rsync`, `cargo`, `cmake`, `ninja`, `nvcc` or `cc1plus`. A `cpu` VM has no card and no
`nvidia-smi` probe. There, test activity is `duckdb` or `aws` (a data fetch), and input on an
ssh terminal (`/dev/pts/<n>`) counts too. Build activity adds `apt-get`, `dpkg`, `git`, `curl`
and the `docker` client; `dockerd` is not a build. The watcher stops the VM through the API:

- after 1 hour with no test activity;
- after 2 hours, measured from the last test, if a build was seen since that test;
- when one test process has run for 4 hours. That is a process on the card, or a test process
  found by name, DuckDB included. A hung CPU-side test is never on the card, and without the
  second it would keep the VM up forever. A terminal has no cap.

The clocks never start before boot, so a fresh start is never stopped on old state. Each tick
appends one JSON line to `/var/log/peacock-gpu-idle.log`, with the card's processes (`card`),
the test processes found by name (`test_procs`), each with its start time, the terminals' last
input (`sessions`), and the `instance` and `kind`. State lives in `/var/lib/peacock-gpu-idle/`.

**The hourly checker** is `.github/workflows/nebius-watch.yml`, scheduled hourly and
dispatchable. It keeps no state. It checks the three VMs, one matrix leg each. Each leg runs
`scripts/nebius/watch.sh <vm> [--instance-id <id>] --profile <name>`:

| leg | options | secrets |
|---|---|---|
| nebius-gpu | `--profile ci` | `NEBIUS_*` |
| nebius-h100 | `--instance-id computeinstance-e00e6yena4nzjgaym0 --profile ci` | `NEBIUS_*` |
| nebius-cpu128 | `--instance-id computeinstance-e01t2mm4265y8x88ey --profile eu-west1` | `NEBIUS_EU_WEST1_*` |

One leg's failure does not stop another's. A leg is skipped with a notice, not failed, when:

- its secrets are absent: `<vm>: <names> absent; skipped`;
- its profile cannot be created: `<vm>: the <profile> profile could not be created; skipped`;
- its instance no longer exists (`nebius.sh` exit 69): `<vm>: instance no longer exists; skipped`.

So the human may delete a VM, and the watch stays green. If the VM is RUNNING, the leg reads the
watcher's last log line over ssh and passes it to `scripts/nebius/check.py`. It hard-stops the VM
when:

- the line is missing, or more than an hour old (the watcher is dead);
- the line, or the watcher's rule applied to it now, says stop;
- ssh does not answer for 15 minutes.

Any non-zero exit of `check.py` counts as a stop, a crash included. A failed hard stop is an
error, and the leg exits 1. A VM up for less than 10 minutes is skipped until the next hour. The
dispatch input `clock_offset` adds that many seconds to the checker's clock. A test dispatch with
`clock_offset=7200` against an idle VM that has been up for more than 10 minutes should hard-stop
it.

- **Run `install-watch` on each VM before the hourly checker is live**, and again after the VM is
  rebuilt. Without it, the checker finds no watcher log and hard-stops a running VM.
- **Run `install-watch` again after any change under `scripts/nebius/`** once it reaches
  master. The VM's watcher and master's checker must apply the same rule. A checker newer than
  the watcher can read its lines as malformed, and then it hard-stops the VM.
- The tests live in `scripts/nebius/tests/`. Each runs as `python3 <file>`.
- GitHub runs the schedule from master's copy of the file only.
- GitHub delays top-of-hour scheduled runs, and sometimes drops them.
- GitHub disables a public repo's schedules after 60 days with no activity. That silently
  removes the hourly guard. Re-enable the workflow in the Actions tab.

### CI and its secrets

`gpu-tests` ("GPU Tests (nebius-gpu)", in *CI structure*) and `nebius-watch.yml` are the only
jobs in the GitHub Environment `nebius-gpu`. `gpu-tests` runs on nebius-gpu only. The Environment
holds nine secrets:

- `NEBIUS_SA_ID`, `NEBIUS_PUBLIC_KEY_ID`, `NEBIUS_PROJECT_ID`, `NEBIUS_PRIVATE_KEY`: eu-north1,
  for nebius-gpu and nebius-h100;
- `NEBIUS_EU_WEST1_SA_ID`, `NEBIUS_EU_WEST1_PUBLIC_KEY_ID`, `NEBIUS_EU_WEST1_PROJECT_ID`,
  `NEBIUS_EU_WEST1_PRIVATE_KEY`: eu-west1, for nebius-cpu128 (the watch only);
- `NEBIUS_GPU_SSH_KEY`, the login on all three VMs.

`.github/actions/nebius-setup` turns one region's four into a CLI profile, named by its `profile`
input (default `ci`), with the key embedded. It installs the ssh key too. `gpu-tests` uses the
default.

The repo variable `GPU_ALLOWED_ACTORS` is a comma-separated list of logins.
`scripts/nebius/check-actor.sh` fails `gpu-tests` unless both the actor and the triggering
actor are on it. An empty list admits nobody. The coordinator's gh account must be on it.

The Environment has no required-reviewers rule. The writers (alephonea, romak00) are trusted,
and the allowlist with the actor check is the gate. A reviewer rule would block the
coordinator's unattended dispatch.

### Disk

nebius-gpu's boot disk is 96 GB in all, and two chains and CI share it.

- Run `df -h /` on the VM before every build.
- Below 20 GB free, clean what can be rebuilt: stale build and target dirs of your own tree,
  `~/miniforge3/bin/conda clean -a`, and finished `~/peacockdb-ci/run-*` dirs. `du -xh -d2 ~ |
  sort -rh | head` shows what is using it. `~/peacockdb-ci/cache` may go too, but the next CI
  run then sends the whole tree again, which takes most of its 50 minutes.
- Below 10 GB free, do not start a build. Record it in the task's detail file as an obstacle.
- Never delete the cuDF envs under `~/data/miniforge3/envs`, any chain's `testdata/`, or
  another chain's tree.
- Record what you removed in the task's detail file.

On nebius-h100 and nebius-cpu128, run `df -h / /var/lib/peacock` before `prepare` or a fetch. sf40
is about 56 GB, and the two images about 15 GB.

- nebius-h100's root is 174 GB. After `prepare` 81 GB is free.
- nebius-cpu128's root is 213 GB. After sf40, the tpch database and the build image 89 GB is free.
- A grown disk's root grew at the next boot on both. If `df -h /` still shows the old size, run
  `sudo growpart /dev/vda 1 && sudo resize2fs /dev/vda1`.

### Who stops it

- **Developers and CI stop softly**: `nebius.sh stop`. Exit 2 means another party's run is
  on the card, or DuckDB on the CPU VM; leave it, and the guards take the VM down later.
- **The coordinator stops hard, all three VMs**: `nebius.sh … stop --force` for each, whichever
  chain started it. It does so on entering `completeness approved`, on entering `done`, and before
  every exit. This can cut another chain's run short. The human accepts that (2026-11-03); the cut
  run resumes with `PEACOCK_BENCH_RESUME=1`.
- **The guards stop hard**: the watcher through the API, the checker with `stop --force`.

Contention is accepted. Chains and CI share the VMs, with no lock. One party's stop can kill
another's run; the loser reruns its step and does not diagnose it.

### Credential files

Never open a Nebius credential file: `~/.nebius/config.yaml`, `~/.nebius/credentials.yaml` and
any `*.pem` under `~/.nebius/`, on any host. That includes the VM's copy in `/root/.nebius/`.
No `cat`, `grep`, `sed`, Read, or redaction attempt: a redaction leaked a private key into a
transcript on 2026-10-14. Reach the cloud through `nebius.sh`. Learn the CLI from
`nebius … --help`.

### The human's one-time setup

nebius-cpu128 is in eu-west1, on its own service account. Its credentials were set up once, by
hand. A new region repeats these steps.

1. Make a key pair for the service account. Upload its public key in the Nebius web console; the
   console gives the public key id. Keep the private key on dev as `~/.nebius/eu-west1-sa.pem`.
2. On dev, the profile:
   `nebius profile create eu-west1 --endpoint api.nebius.cloud --service-account-id <sa> --public-key-id <key> --private-key-file-path <pem> --parent-id <project>`.
   It keeps a path to the PEM, so install-watch needs `--key-file` for it.
3. The four CI secrets, each value on stdin:

   ```
   gh secret set NEBIUS_EU_WEST1_SA_ID --env nebius-gpu
   gh secret set NEBIUS_EU_WEST1_PUBLIC_KEY_ID --env nebius-gpu
   gh secret set NEBIUS_EU_WEST1_PROJECT_ID --env nebius-gpu
   gh secret set NEBIUS_EU_WEST1_PRIVATE_KEY --env nebius-gpu < ~/.nebius/eu-west1-sa.pem
   ```

4. install-watch on the VM, with `--key-file` (above).

## Remote hosts

| Host | Use | Managed by |
|---|---|---|
| **nebius-gpu** | GPU test suite while shad-gpu is down (L40S, cuDF 25.02, glibc 2.39) | `scripts/nebius.sh`, `nebius-sync.sh`, `nebius-test.sh`; see *Nebius host* |
| **shad-gpu** | Down until further notice; see *Nebius host*. When up: GPU test suite (cudf 25.02, H200-class; old glibc → patch step) | `scripts/build-test-shadgpu.sh` (`--build --push-binaries --patch --run[-detached]` / `--all`, `--run-status`). Resilient rsync + retries — the link is flaky |
| **verda** (when available) | large CPU runs, golden regen | `scripts/build-test.sh --host <user@ip> --all` (add `--rust-only` to skip the C++/FFI half) |
| **verda-gpu** | same root volume as verda, with an H200 attached; cuDF 26.02, modern glibc, nsys 2024.5 | `scripts/build-test.sh --host <user@ip> --gpu --all`. No benchmark or Nsight pass: those run on nebius-h100 (*Nebius host*) |
| **nebius-h100** | sf40, the corpus benchmark and its Nsight passes (H100 80GB) | `scripts/env/run-gpu.sh`; see *Nebius host* |
| **nebius-cpu128** | DuckDB, and the CPU-VM build of the H100's install (128 cores, no card) | `scripts/nebius.sh`, `provision-vm.sh`; see *Nebius host* |

- **Testing the regen *mechanism* is not a full regen.** Scope it with `PCK_TEST_FILTER`
  (forwarded to every staged binary; the gpu lib binary takes it inside its `gpu_tests::`
  rung, as the exact names `--list` reports under both) — two or three queries prove the path as well as
  918 do, and a full `--update-canonical` run rewrites every golden on the remote and
  pulls the whole set back into a git working tree, where an unrelated diff can ride
  home in an unrelated commit. Note that a binary whose tests all filter out runs zero
  tests and passes, so say which binaries actually executed; and a filter naming a query
  matches no test whose name is a property rather than a query, so exercising those takes a
  filter that matches them or a separate run.
- **A GPU binary needs a fixed amount of free VRAM.** Each measuring binary reserves a
  byte budget: `peacock_tpch_tests` 69 GiB, `peacock_tpchv_tests` 30, `peacock_cudf_node_tests`
  10, `peacock_gpu_tests`, `peacock_plan_tests` and `peacock_join_session_tests` 1 each,
  `test_node_timing` 2, and
  `peacock_gpu_benchmarks` 69 — the tpch figure taken for the same sf40 data, not yet measured
  for the engine's own run. Below its budget a binary does not
  shrink: the pool is not built, and the sf40 pair goes red on rmm's default resource. Read
  `[rmm] pool of N GiB could not be built` at the top of the log first; the
  `cudaErrorMemoryAllocation` failures under it are its consequence. The GPU host is shared,
  so check `nvidia-smi` before debugging a red GPU tier ([#178](tickets/testinfra.md#t178)). Budgets are
  H200 numbers; `PEACOCK_RMM_POOL_BYTES=<bytes>` replaces one for a sweep or another host.
- **`verda` and `verda-gpu` are names for a host, not addresses of one.** They are ephemeral
  Verda Cloud instances, reprovisioned with a new IP, so `--host` takes `<user@ip>` and
  `scripts/list_verda_instances.sh` is what finds it (`VERDA_CLIENT_ID`, `VERDA_CLIENT_SECRET`).
  `ssh verda` resolves nowhere, so a probe of the bare name reports "down" whether the instance
  is up or not. Only shad-gpu has a stanza in `~/.ssh/config`.
- **Prefer verda for large CPU runs** (whole suite or big selections). It is not always
  up (the human starts it manually) — falling back to a local run is completely fine.
- **Comparing files across hosts: use checksums, and `LC_ALL=C sort` for any listing.**
  Two hosts collate `ls`/`sort` differently, so a manifest diff reports differences that
  are pure collation — this produced two false alarms in one session before checksums
  settled it. Compare `sha256sum`/`md5sum` output, not directory listings.
- Rented hosts change SSH host keys on reprovision: `ssh-keygen -R <host>` + re-keyscan
  rather than fighting the mismatch. nebius-gpu is the exception: `nebius.sh` handles its key.
- **The shad-gpu patch step uses the build host's glibc version.** shad-gpu runs glibc 2.31,
  so shipped binaries are patched to a prefix under `~/glibc-<version>` there: 2.35 for a
  22.04 build host and CI's container, 2.39 for a 24.04 host such as dev. The version is read
  from `getconf` where the binaries are built, and the prefix is built once on the first patch
  from that host class. A binary patched to a glibc older than its own dies at load with
  `version GLIBC_2.38 not found`.
- **Golden regen**: on verda via `scripts/build-test.sh --host <user@ip> --rust-only
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
- **Large CPU + GPU batches run in parallel** — kick off the GPU host run and the
  verda/local CPU run concurrently; neither waits for the other.
- **The device's own answers are a knob, not a flag**: `PCK_WRITE_GPU_RESULT=1` on a gate
  run has the device corpus tier write `gpu-result.txt` beside the cpu's goldens, before it
  asserts against them — so an answer the cpu rejects is still recorded for DuckDB to judge.
  `--pull-results` brings the file home afterwards — only one the gate run itself wrote, by
  mtime against the run-id file — and it refuses while a detached gate run is still going.
  On nebius-gpu there is no pull: rsync the file home through `nebius.sh rsh`, and do it
  before the next `nebius-sync.sh`, which overwrites the VM's copy with the local one.
  Any other non-empty value (`PCK_WRITE_GPU_RESULT=26.02`) writes
  `gpu-result-<value>.txt` beside it, which `testdata/.gitignore` lists: one cuDF version per
  committed file. An empty value records nothing, and that is the ordinary case: the gate script
  exports the variable on every cycle, so empty is how a cycle that is not recording says so. The read side is a second knob: `PCK_GPU_RESULT_VERSION=<v>` points the
  `duckdb_gpu_*` cases at `gpu-result-<v>.txt`, which is how a non-25.02 run gets compared at all.
  The coverage guard reads the committed file whatever that is set to, so a versioned file can
  exercise the comparison and can never satisfy the guard. Every one of these files opens with a
  `cudf=` line naming the cuDF that recorded it, and a file whose line disagrees with its name is
  refused by every reader — which is what holds "one version per file" without trusting the value
  somebody typed.
- **A run that outlives your ssh session** is `--run-detached`, read back with
  `--run-status`. That flag exits 0 only when the latest run finished with 0 — still
  going, died without writing its code, and a completion belonging to an earlier run are
  all non-zero, because the alternative is a status command that reports someone else's
  success as yours. Run state lives in `$REMOTE_REPO/.run-state/gate.{sh,log,rc,id}`,
  deliberately outside `cpp/install/` — that tree is mirrored with `--delete`, so a marker
  kept there is erased by the next push.
- **A run beside the gate's checkout** sets `PEACOCK_REMOTE_REPO=<dir>`: every phase then
  pushes, patches, runs and pulls under that directory instead of `/home/info/peacockdb`, which
  a measurement taken while the gate is in use must leave alone. Create `<dir>/cpp` and
  `<dir>/testdata` first — rsync makes no parent directories, and a push into a fresh one
  retries a hundred times before it fails.

## Images

Two images per cuDF version, built from `docker/` and published on `ghcr.io/asymptote-tech/`.
`<v>` is a version with a committed spec in `docker/conda/`: 25.02 or 26.02. Sizes are docker's,
in GB of 10^9 bytes.

| image | holds | size (25.02, 26.02) |
|---|---|---|
| `peacockdb-build:cudf-<v>` | every build mode: rust-only, the FFI build, `--features gpu`, the staged binaries | 10.0 GB, 9.74 GB |
| `peacockdb-run:cudf-<v>` | the runtime of the same cuDF and `run-tests.sh`; nothing to build with | 5.09 GB, 6.17 GB |

Each image carries two tags. `:cudf-<v>` is the latest published. `:cudf-<v>-<commit>` names the
commit it was built from, 12 hex digits; the label `org.opencontainers.image.revision` holds the
same commit. Pulling needs no login: `docker pull ghcr.io/asymptote-tech/peacockdb-run:cudf-25.02`.
No image holds a secret, a host or Claude Code. Both run as any uid: pass
`--user "$(id -u):$(id -g)"`. The default user is `peacock`, uid 1000, never root.

### The build image

- `/opt/cudf` is the cuDF prefix (`CUDF_ROOT`), package for package the nebius-gpu environment the
  committed device answers came from. Its `bin/` is not on PATH, and its `lib/` is not on the
  loader path: an FFI test binary run in the image needs `-e LD_LIBRARY_PATH=/opt/cudf/lib`, or
  it exits 127 on `libcudf.so`.
- nvcc 12.9.86 in `/opt/nvcc`, gcc 12 (25.02) or 14 (26.02) as `CC`/`CXX`, cmake, ninja, rust,
  ccache, patchelf. `scripts/build.sh` needs `--cudf_ROOT /opt/cudf --gcc-version "$GCC_VERSION"`.
- DuckDB 1.5.4 with its tpch and tpcds extensions in `/opt/duckdb-ext`. The image sets
  `PEACOCK_DUCKDB_EXTENSION_DIR`, so `generate_testdata.sh` installs them offline.
- Python 3.13 in `/opt/venv`, exactly `docker/venv.lock`: the versions the committed costs ran on.
- git, gh, ssh, rsync, jq, aws and nebius, with no configuration.
- What a configure fetches is prefetched: rapids-cmake, flatbuffers and gtest. With the tree's
  `third_party/cudf` checked out and a cargo registry mounted at `/opt/rust/cargo/registry` (filled
  once by `cargo fetch` with the network on), the build and data generation run under
  `--network none`.
- HOME is not writable for a uid the image does not know. Set `-e HOME=<a mounted directory>`
  when a tool needs one.

### The run image

The binaries and data are mounted, never in the image:

    docker run --rm --gpus all --user "$(id -u):$(id -g)" --hostname "$(hostname)" \
      -v <staged cpp/install>:/peacock/install:ro -v <testdata>:/peacock/testdata \
      ghcr.io/asymptote-tech/peacockdb-run:cudf-25.02 list

- `--hostname` gives the container the machine's name, which a benchmark writes as `host=`.
  Without it the container's own name, its id, is written instead. A run split into several
  `docker run`s needs the same name in each: a resumed segment keeps only the sections whose
  `host=` is its own. Resuming needs `-e PEACOCK_BENCH_RESUME=1` on the `docker run`, which
  passes none of the caller's environment through.
- `/peacock/install` is a staged `cpp/install`, built in the build image of the same version:
  `lib/libpeacock_gpu.so`, `bin/peacock_*_tests`, `rust-tests/` and, for `bench`,
  `rust-benchmarks/`. Read-only.
- `/peacock/testdata` is the repository's `testdata/` with its datasets. Read-write: a run writes
  `benchmark-results/` and the calibration record.
- The entrypoint is `/opt/run/run-tests.sh <list|filter|bench>`. `list` is the L40S list, by
  `nebius-test.sh`'s rules. `filter` runs the list with every rust binary but
  `peacockdb_core_gpu_lib` narrowed by `-e PCK_TEST_FILTER=<f>`; the C++ suites and the lib's
  device rung run whole. Beyond the usual failures, it fails when the filter matches no test in the
  narrowed binaries. `bench` runs the benchmark binary and refuses a
  `calibration/records.tsv` that still has rows: pull it home, then remove it.
- Exit 0 is green, 1 a binary failed, ran no test or crashed, 2 a bad argument or input.
- **The driver.** The label `peacockdb.driver-minimum` is the oldest driver the image's CUDA
  runtime runs on: 535.104.05 for 25.02, 575.57.08 for 26.02. Read it with
  `docker image inspect --format '{{index .Config.Labels "peacockdb.driver-minimum"}}' <image>`
  and compare it with `nvidia-smi --query-gpu=driver_version --format=csv,noheader`. A card newer
  than compute capability 9.x JIT-compiles the PTX and needs 575.51.03 or later with either image.
- **A binary run by hand** needs the library path: the shipped binaries carry no RUNPATH. Add
  `--entrypoint /peacock/install/bin/<binary>`,
  `-e LD_LIBRARY_PATH=/peacock/install/lib:/opt/cudf/lib` and
  `-e PEACOCK_TESTDATA_DIR=/peacock/testdata`.

### Rebuilding and publishing

- `scripts/env/image.sh build <build|run> <v>` builds from the working tree. `--dry-run` prints
  the `docker build` line instead.
- **The commit tag comes only from a clean tree**: `git status --porcelain` empty, untracked files
  included. A dirty build gets `:cudf-<v>` alone and the revision `<commit>-dirty`.
- `scripts/env/image.sh push <build|run> <v>` pushes both tags. It refuses unless HEAD's commit
  tag exists locally and `:cudf-<v>` is the same image. So: commit, build, push.
- **From a host**, docker must be logged in to ghcr.io with a token holding `write:packages`:
  `gh auth refresh -s write:packages`, then
  `gh auth token | docker login ghcr.io -u <github user> --password-stdin`. A package pushed for
  the first time can be private: check its visibility in its settings and set it public.
- **From GitHub**, `.github/workflows/images.yml`:
  `gh workflow run images.yml -f kind=<build|run|all> -f cudf=<v|all>`. GitHub can dispatch it
  only once master holds it. A dispatch runs the workflow and the commit of the ref it names
  (`--ref`, master by default), and moves `:cudf-<v>`. One job per image, pushing with the job's
  own token. If the job's push is denied, give this repository the Write role under the package's
  "Manage Actions access".
- **The runner's disk.** A standard runner has 14 GB. The largest build holds 12.2 GB at once:
  the 26.02 run image holds its 6.09 GB prefix twice, in its stage and in the final copy. A build
  image peaks at about 10.6 GB, its 7.5 GB conda layer beside micromamba's downloads. So a job
  first removes the runner's preinstalled SDKs, and stops if less than 16 GB is left.
- The tests are `scripts/env/tests/`, each run as `python3 <file>`, locally or in the build image.
  No CI step runs them. `test_image_any_user.py`, `test_run_image.py` and `test_run_tests.py` need
  the images built; `test_run_image.py` also a staged install per version, named by
  `PEACOCK_STAGED_INSTALLS=<dir holding cudf-<v>/>`.

## Containers

A developer works on a CPU VM: builds run in the build image there, and so do the DuckDB runs that
want its 128 cores. The developer readies it from their own machine, the laptop, with
`scripts/env/provision-vm.sh`, which runs `scripts/env/bootstrap-vm.sh` on the VM. Each has
`--help`. nebius-cpu128 is such a VM (*Nebius host*).

### Trust

The VM holds only what every run there may use: the S3 read key, a fine-grained PAT scoped to the
repository, and the developer's key to the GPU hosts. Never the S3 writer key, never another
secret. The developer's user is in the docker group, which is root.

The PAT's Contents and Actions write let work on the VM push its branch and dispatch
`pipeline.yml` on it. That code then runs in CI with CI's secrets: the nebius service-account key,
the GPU hosts' ssh key and `S3_*`. The `nebius-gpu` environment has no protection rule, so the
ensemble runs device steps without a person. So CI's S3 secret stays a read-only key too.

- **Developers on one VM** are separate Linux users. But every one of them is in the docker group,
  which is root, and has sudo. So each can read the others' PAT and keys. Share a VM only among
  people who trust each other.
- **`peacock.env` holds only the storage keys** and may be passed between developers. The PAT and
  the GPU key are personal: never forward them. `provision-vm.sh` refuses a `peacock.env` with any
  other key, naming the key and never its value. Use the read key: nothing tells it from the
  writer key.

### A fresh VM

Once per developer:

1. Get `peacock.env` from whoever administers the storage: the four storage keys (below).
2. Have someone create the VM, with a login user that has sudo and your ssh key.
3. On the laptop, in a clone of the repository:
   `scripts/env/provision-vm.sh <login>@<address> --env <the file you got> --gpu-key <your key>`.
   The laptop needs bash 4.4 or later; on macOS, `brew install bash`.
   It prints a GitHub token recipe: make the token on GitHub and paste it. It ends in Claude
   Code's login: open the URL it prints and finish there.

**What `provision-vm.sh` does.** Every argument is checked before anything is written. A rerun
skips what is done.
- On the laptop, in `~/.config/peacock/` (mode 700, each file 600): `peacock.env`, copied from
  `--env`, which is needed only while it is missing, with every CR dropped and a final newline;
  `github-pat`, the token, asked for when missing or with `--new-pat`. The token must be
  `github_pat_…` and able to read the repository; gh checks it with the token in its environment,
  never its argv.
- The GPU key is taken, never made. `--gpu-key` names a private key the GPU hosts already accept,
  with its `.pub` beside it; `~/.config/peacock/gpu_ed25519` by default. A missing key or `.pub`
  is refused, naming `--gpu-key`. On dev the key is `~/.ssh/id_ed25519`.
- As `<login>`, with sudo: groups `docker` and `peacock`; your Linux user (`--user`, your laptop
  user name by default) in both, with `<login>`'s `authorized_keys` and passwordless sudo.
- As your user, over ssh, on every run: `bootstrap-vm.sh`, `--ref <branch>` passed on, master by
  default. It skips what is done, and refuses a `--ref` the VM's clone is not on.
  Then the secrets, on ssh's stdin alone: `peacock.env`, and the GPU key pair as
  `~/.config/peacock/gpu_ed25519` and `.pub`, the VM's `peacock.env` gaining `GPU_SSH_KEY_FILE`;
  the PAT into `gh auth login --with-token --insecure-storage`, then `gh auth setup-git`. The PAT
  is never in the VM's `peacock.env`. Then git's identity, your laptop's `user.name` and
  `user.email`.
- The VM goes into the laptop's `~/.config/peacock/vm` as `<name> <user>@<address>`; `--name` is
  the address by default.
- Last, `claude auth login` over `ssh -t`, unless Claude is logged in there already. It needs a
  person at a terminal. With stdin not a terminal (`</dev/null`, a script) the login is skipped and
  the run still exits 0. It prints the command to run instead, on stderr:
  `Claude Code is not logged in on <user>@<address>: run ssh -t <user>@<address> 'DISABLE_AUTOUPDATER=1 ~/.local/bin/claude auth login'`.
  The path is spelled out because a plain `ssh <host> <command>` has no `~/.local/bin` on its PATH.
- `provision-vm.sh --new-pat` replaces the PAT on the laptop and on every VM in `vm` it reaches.
  It names each VM it cannot reach.

**`bootstrap-vm.sh [--ref <branch>] [--cudf <v>]`** prepares the machine, as a user with sudo,
never root. `provision-vm.sh` runs it; it is also one file you can fetch from the repository's raw
view and run by hand. After one `apt-get update` it installs what is missing: git, curl and
python3; Docker (`docker.io`); gh at `GH_VERSION` in `/usr/local/bin`, from the tarball the image
takes; Claude Code at `CLAUDE_CODE_VERSION` with `superpowers@claude-plugins-official`. It makes
the machine's data directory (below). It clones the repository into `~/peacockdb`, at `--ref` or
master; a `--ref` other than the branch an existing clone is on is refused, naming both. It
fetches the cuDF checkout the clone pins. It pulls the build image of cuDF `--cudf`, 25.02 by
default, and refuses one whose `peacockdb.inputs` label differs from the clone's inputs, naming
each (`scripts/lib/image-inputs.sh`). In that image it fills cargo's registry and generates sf1.
A step already done is skipped.
- The plugin cannot be installed at a commit: the marketplace pins it. The install is checked
  afterwards: it must record `SUPERPOWERS_VERSION` at `SUPERPOWERS_SHA`. When the marketplace
  moves on, the check fails, and the two pins must be bumped.
- `~/.bashrc` gets `DISABLE_AUTOUPDATER=1`, so Claude Code stays at its pin.

**By hand, without `provision-vm.sh`:** run `bootstrap-vm.sh`, log out and in again (for the
docker group), then: `claude`, then `/login`; gh with a fine-grained PAT, below;
`git config --global user.name` and `user.email`; the storage keys in
`~/.config/peacock/peacock.env`.

**gh's token** is a fine-grained PAT, never the OAuth token plain `gh auth login` makes, which
reaches every repository the user can. On GitHub: resource owner `asymptote-tech`, only the
`peacockdb` repository, an expiry; permissions Contents read-write, Pull requests read-write,
Workflows read-write, Actions read-write, Commit statuses read, Metadata read. Then
`gh auth login --with-token --insecure-storage`, paste the token, Enter, Ctrl-D: it is read from
stdin, so it is in no argv and no shell history. `--insecure-storage` writes it to
`~/.config/gh/hosts.yml` rather than a desktop keyring. Then `gh auth setup-git`.

### The env file

The storage keys, in a docker env file, `NAME=value` a line:

    AWS_ACCESS_KEY_ID=…
    AWS_SECRET_ACCESS_KEY=…
    AWS_ENDPOINT_URL=https://storage.eu-north1.nebius.cloud:443
    AWS_REGION=eu-north1

That is the whole of the laptop's `peacock.env`; blank and `#` lines are skipped, as docker
skips them, and any other key is refused by name. On a VM, `~/.config/peacock/peacock.env` is the
same with one more line, which `provision-vm.sh` writes:

    GPU_SSH_KEY_FILE=/home/<user>/.config/peacock/gpu_ed25519   # the key to GPU hosts

### What a VM holds

- **The data directory** is the machine's, shared by its developers: `/var/lib/peacock`, or
  `$PEACOCK_DATA_DIR`. `bootstrap-vm.sh` makes it with sudo: group `peacock`, setgid, mode 2775,
  owned by the user who ran it. `provision-vm.sh` puts every developer in `peacock`. What
  `bootstrap-vm.sh` writes there is written under umask 002, the generator in the build image
  too. So one developer reuses another's sf1 and cuDF checkouts, takes their locks, and clears a
  crashed `.partial` they left. cargo's registry stays per user.
- **third_party/cudf.** The FFI build's configure reads cuDF's cmake files. The machine keeps one
  checkout a commit, `<data dir>/cudf/<commit>/`, with the paths the build image's prefetch takes
  (`/cmake/ /cpp/cmake/ /VERSION /RAPIDS_BRANCH`). `bootstrap-vm.sh` fetches the one the clone's
  gitlink names, under a lock, when the machine has none. The clone's own `third_party/cudf` is
  never read. An old checkout is removed by hand.
- **The registry** is `~/.cache/peacock/cargo-registry`. `bootstrap-vm.sh` runs
  `cargo fetch --locked` for the clone in the build image, the clone mounted read-only, under a
  lock, so two runs never write it at once.
- **The data.** `<data dir>/<hash>/` holds `tpch.sf1` and `tpcds.sf1`. The hash is over the
  clone's `generate_testdata.sh`, the `DUCKDB_VERSION` pin and the generator's arguments
  (`scripts/lib/data-inputs.sh`). With none for the hash, `bootstrap-vm.sh` generates one in the
  build image, with `--network none`, under a lock. Two runs at once generate it once. A set
  appears only complete. An old set is removed by hand.
- **sf40**, on the H100 and the CPU VM: `scripts/env/fetch-sf40.sh tpch.sf40 tpcds.sf40` fetches
  each from its bucket into `<data dir>/<dataset>/`. The keys come from `peacock.env`, or stdin
  with `--env -`, and reach aws alone. Each file is checked against the bucket's `manifest.json`
  size, and a second run fetches nothing it holds. Then the set's aggregate sha256 is checked
  against `scripts/env/sf40.sha256`, one line a dataset; an unpinned one is printed. Without an
  aws CLI it installs the pinned one into `~/.local/aws-cli`.
- **DuckDB**, on the CPU VM: `scripts/env/install-duckdb.sh` installs `/usr/local/bin/duckdb` at
  `DUCKDB_VERSION`, from the zip the build image takes, checked against `DUCKDB_SHA256`.
- **The DuckDB database**, on the CPU VM:
  `benchmarks/duckdb_minimal.sh --dir /var/lib/peacock --sf 40 --skip-download` imports the
  fetched `tpch.sf40` parquet into `/var/lib/peacock/tpch.sf40.duckdb`. `--skip-download` is
  needed: without it the script lists the bucket with a bare `aws` and aws's own credentials, and
  the VM keeps its keys in `peacock.env` alone. On 2026-11-04 the database was 32.7 GiB. The
  import took 392 s, and the whole run, with its golden checks and timings, 9 min 16 s.
- **Fetch times**, on 2026-11-04: on the CPU VM 6 min 41 s, on the H100 about 12 min. A second
  run fetches nothing: 28 s on the CPU VM, with the parquet in the page cache.

## Benchmarks

### Corpus benchmarks — `peacock_gpu_benchmarks` (scripted)

Its own case list, `peacockdb-core/tests/common/corpus_benchmark_cases.inc`, and deliberately not
the correctness gate's: the two disagree about sf on purpose. Correctness runs at sf1, where a wrong
answer is legible in six million rows; at sf1 a query is mostly the host prologue, so the rows worth
timing are at sf40 and the rows worth checking are not. The list is written from the registry:
every tpch and tpcds row's enabled device cells (`gpu_*`), timed at sf40. Today that is 650 cases
over 130 queries, the two rowgroup modes among them. Nobody edits it by hand. A rust-only test
holds it to the registry, mode by mode, and rewrites it from the registry under `UPDATE_CANONICAL`:

```
UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens \
    -- the_benchmark_cases_follow_the_registry
```

A second test there holds each timed (query, mode) to `corpus_cases.inc`'s device column.

The committed `h200/` tree holds the 26 cases the list named when it was measured, before the list
followed the registry: tpch q1, q6 and `shuffle_additive_avg` at all five modes; q17, q19,
`aggregate_groupby`, `filter_project`, `shuffle_additive`, `shuffle_stddev` and `nested_loop_join`
at `tp1_single`; tpcds q37, q82, q84 and q85 at `tp1_single`. Those are its cases as the
hand-written list stood at a5c852e4 on master, fixed since: `h200/cases.csv`,
and `h200/` is held to it. It goes with `PREDATES_PROVENANCE` once the H200 is run again (#226).

Every per-device case list is `testdata/benchmark-results/<device>/cases.csv`. A device tree that
holds one is held to it; a tree without is held to the whole main list. Its first line is
`dataset,sf,query,modes,status,note`, and a row splits at its first five commas:
- `dataset`, `sf`: as the main list writes them, `tpch` and `40`;
- `query`: the main list's ident, `_` kept (`aggregate_groupby`);
- `modes`: mode idents `|`-joined, no spaces (`tp1_single|tp4_sized`); empty when skipped;
- `status`: `ok`, `skipped` or `wrong`;
- `note`: the rest of the line, commas allowed. A skipped row's reason, a wrong row's `#NN`
  ticket, empty for `ok`.

A case appears once. `parse_device_cases` and `render_device_cases` in `test_support` are the one
reader and writer; `every_device_list_parses` reads each committed list. The main list,
`corpus_benchmark_cases.inc`, is in macro form.
`nested_loop_join` reads region and nation, which do not grow with sf, so its rows describe the
host prologue. At the two rowgroup modes a scan is one
call per row group, a thousand-odd per pass, so those six cases are most of the H200's record — 254
thousand rows, 26 MB for tpch, too large for git; tpcds's is 1,060 rows. The records live in the
`calibration` bucket under their sha256, and `testdata/calibration/records.sha256` names the ones
that go with the committed tree. The 26 on an empty H200 took an hour (3,602 s), most of it q1 and
`shuffle_additive_avg` at the rowgroup modes, ~90 s and ~44 s a pass, eleven passes each.

**A failed case, and a rerun.** A case that panics on the device writes its section as the record
of the failure, and its test fails; the next case runs. The record is one status line, the panic
message on one line, then the run's five provenance lines:

```
== q11
status=failed reason=<the panic message>
device=NVIDIA H100 80GB HBM3
driver=580.95.05
cuda=12.9
cudf=25.02.02
host=gpu-host
```

A measured case's rows are appended before its section is written, since the section is what a
resumed run keeps: a crash between the two leaves no section, and the case is measured again. A
case that fails appends no rows. If the record refuses a measured case's rows, the case is
recorded as failed with the record's reason, and the record holds none of its rows: a write
that stops partway is cut back to the length before it. A case that kills the process, a
segfault or an abort, writes nothing.

A run measures every case it is given, whatever its file holds. `PEACOCK_BENCH_RESUME=1` makes it
resume instead: a section whose trailer ends with this run's five is kept, its case is not run,
and the case prints a `kept` line. A failed case, a case never reached, and one measured on
another device, driver, CUDA, cuDF or host are run again. Use it only to finish a run that was
cut short — preempted, or stopped by the idle watcher — with the same binary: the five do not
name the build, so a resumed run after a rebuild keeps the old binary's numbers. `0` or unset
measures everything; any other value is refused. A captured run writes no section and keeps
none. A resumed run's record holds only the cases it ran, and `split_record.py` refuses a record
short of the list, so a resumed run's record is not published today.

The run's own refusals fail before anything is recorded: a capture or resume variable naming no
setting, no device to name, no rmm pool, a debug build, a dataset that is not there, and a record
at `PEACOCK_RECORD_PATH` that another run wrote.

It asserts nothing about an answer, so it can never gate a merge. Its nine harness assertions
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
./scripts/build-test-shadgpu.sh --build-benchmarks
# 2. ship and glibc-patch  3. time the corpus  4. bring the tree and the record home,
#    publish the record and pin it in records.sha256 — commit that beside the tree
./scripts/build-test-shadgpu.sh --push-binaries --patch --run-benchmarks --pull-benchmarks
# or, for a run that outlives your ssh session (the suite takes hours):
./scripts/build-test-shadgpu.sh --push-binaries --patch --run-benchmarks-detached
./scripts/build-test-shadgpu.sh --benchmark-status     # going? finished? log tail
./scripts/build-test-shadgpu.sh --pull-benchmarks      # once it reports finished
# 5. the derived records: both Nsight passes, calls.tsv and hbm.tsv, the counters a case at a
#    time and joined onto the pinned records (fetch_record.sh <dataset>.sf<sf> for each
#    first). No flag runs both passes; --trace or --metrics picks one. Over the H200's
#    records, from a5c852e4 on master, whose case lines are
#    h200/cases.csv's rows.
./scripts/create_nsys_profile.sh
# 6. every panel and index.html, from one call: a --record per dataset
/usr/bin/python3 scripts/calibration/plot.py \
    --record testdata/calibration/tpch.sf40/records.tsv \
    --record testdata/calibration/tpcds.sf40/records.tsv \
    --hbm testdata/calibration/hbm.tsv --out-dir testdata/calibration/plots
```

Step 4 publishes only a whole, plain run that exited 0 and is still the host's record, a file
per dataset; it names what it refused, leaves the pin alone and exits 1. It uploads through
shad-gpu's aws CLI, reads each object back and pins its sha256 only when that matches, so the
pin never names an object the bucket lacks; *Benchmark data flow* has why.

Steps 5 and 6 are separate scripts because they are separate measurements: a capture
serializes what it traces and the counters pass costs several percent, so neither may write
the tree step 3 produced. What each writes is the diagram under *Benchmark data flow*.
`--benchmark-status` exits 0 only when the latest run finished with 0, as `--run-status` does.
Check `nvidia-smi` for a neighbour before step 3: a process holding the card inflates every
number here without failing anything.

**The same six steps on a 26.02 host** (verda-gpu) go through `build-test.sh`, which builds
against the local `rapids` env and needs no glibc patch. Its pull publishes nothing; only
shad-gpu's does. The three benchmark
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

Two things the shad-gpu path handles that this one leaves to the host: `testdata/tpch.sf40` and
`testdata/tpcds.sf40` must already be symlinks to the datasets (the script checks that tpch's
resolves and creates nothing; a missing tpcds one is refused by the first tpcds case, before it
records anything, and so by every tpcds case after it), and the GPU's performance counters must be
open to non-root for step 5's `--metrics` pass (`options nvidia
NVreg_RestrictProfilingToAdminUsers=0` in `/etc/modprobe.d/`, then a module reload — verda-gpu has
it; nsys says `ERR_NVGPUCTRPERM` when a host does not). The profile script's HBM defaults (`gh100`,
4.8e12 B/s) are an H200's; another card overrides `PCK_BENCH_HBM_SET` and `PCK_BENCH_HBM_PEAK_BW`.
`--push-binaries` mirrors `cpp/install/` with `--delete`, so a push from a checkout that never ran
`--build-benchmarks` removes the benchmark binary from the host, and a push from one that never ran
`--build` removes the gate's. Two card models write two trees, and each file names its device,
driver, CUDA, cuDF and host. Two cuDF versions on one card model write the same files, and only the
trailers' `cudf=` tells them apart.

**The tree**: one file per (device, dataset, mode) at
`testdata/benchmark-results/<device>/<dataset>.sf<sf>/<mode>.benchmark.txt`, a `== <query>`
section each. The committed H200 run is under `h200/`. A section is the plan tree with one
line under every node:

    time_us=[[22,37,40],[55,1,30]] total_us=185

Lanes outermost, one entry per call the lane made, each entry the call's device microseconds summed
over its output partitions; `1` where the clock rounded a region to zero, since every call opens
one; `0` where the call crossed no ABI — the backend answered from what it held, as an accumulator
taking a batch or a join's `set_build` does. A lane the node was never driven on is `[]`. Then a
`--- run ---` trailer: `run_us` (the chosen execution end to end, after planning), `device_us` (Σ
of the tree's `total_us`; the gap to `run_us` is the host), `runs=[…]` (every measured execution,
in order), `build=release`, `allocator=` (what `install_rmm_pool` reported: with rmm's default
every cuDF intermediate is a `cudaMalloc`/`cudaFree` round trip billed to the node that allocated
it, which moves the profile and not just the scale), then the five provenance keys, `device=` to
`host=`. A failed case's section is its record instead (above). The reported execution is the
second-smallest by `run_us` of ten, after one discarded warm-up: the fastest run is the one most
likely to have caught a scheduling accident, and a whole run is reported rather than a per-node
minimum, which would be a tree belonging to no execution. Both counts are constants in
`src/test_support/` (`MEASURED_RUNS` in `mod.rs`, the warm-up in `corpus_benchmark.rs`), so every
file in the tree was taken at the same counts. `--pull-benchmarks` is additive and no push
`--delete`s the directory.

**The record**, `testdata/calibration/records.tsv` where a run's pull puts it, and
`testdata/calibration/<device>/<dataset>.sf<sf>/records.tsv` where `fetch_record.sh` puts the
published one (the H200's one level up; *Datasets* says how); neither is in git. One row per
cuDF call — one (plan node, recipe step, call index), not one node and not one output
partition — for every measured
execution, so the same call recurs once per `run_index` and the spread is data. Seventeen
columns: `dataset sf query mode node_seq node_type lane recipe_seq recipe_kind call_index
run_index in_rows in_bytes out_rows out_bytes host_us device_us`. `node_seq` is post-order,
`lane` the driving lane, `call_index` what C++ counted to for that seq, `recipe_kind` the fb
kind or the ABI symbol for a slice and an export. No cell is empty: `out_*` come from the
call's own `NodeStats` priced by the schema the executor holds, and a middle call's `in_*` is
the call before it. The `#` heading carries what is constant across a run —
`timing_mode=events`, `build=release`, `allocator=`, `capture=`, then `device=` to `host=` —
and an append under a different heading is refused. The harness checks each execution's rows
against the plan before it writes them; the rest of the format is in the heading itself and in `src/test_support/record.rs`.

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
- **A synced source older than the binary built from it.** `rsync -a` preserves mtimes, so
  pushing the tree over a host-side edit can restore a *younger* file as an *older* one. Cargo
  then sees nothing newer and skips the rebuild, and the run reports the previous build's
  behaviour as the current one — a green that belongs to code the host no longer holds. Never
  edit sources on a remote host; where one has been edited, `touch` the synced files before
  building. Confirm which binary you are about to trust with `strings` on it when the change is
  a literal the test reads.
