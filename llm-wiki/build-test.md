# peacockdb build & test

Code and tests are authoritative; this page maps them.

## Test categories

**Grand total: 8219 test cases — Rust 6304, C++ 286, Python 1629.** The Python figure includes the 425 runs of the engine's own plans and the per-call model's check on the measured record, which only a manual dispatch runs. It also includes three sets run by hand: the external-embeddings sidecar recount, the 287 image and VM tests under `scripts/env/tests/`, and the 9 tests of `duckdb_minimal.sh`, which need `duckdb` and sf1. The header is the sum of the N columns of the two tables below, and the rows count cases: a target's own `--list` total is larger, because its registry test is counted once in Registry ↔ CSV rather than again in each tier it belongs to. Comparing a row against a target total is how this page gets mistakenly reported as drifting.

**Runs** — `dataset-matrix` = pipeline.yml's job with the generated dataset and the cuDF
matrix, both legs unless a step says one · `cost-report` = the cost-report job · `shad-gpu` =
CI GPU job, `--test-threads=1`, on nebius-gpu while shad-gpu is down (*Nebius host*); the sf40
benchmark cases run by hand on nebius-h100, through `run-gpu.sh … run bench` (the fused ones
through `run prototype_fused_benchmarks`), and the sf40 C++ pair
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

#### cpu — `--features rust-only`: no FFI, no device. 3731 cases: `--lib` 1211, `test_cpu_corpus` 2451, `test_corpus_goldens` 66, `test_cost_model` 3

*crate integration, external*

| Corpus, cpu | [test_cpu_corpus](../peacockdb-core/tests/test_cpu_corpus.rs) | 2450 |
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

Then the sf40 tier, 4 of the count. `every_sf40_device_answer_meets_duckdb` reads the H100's
`cases.csv` and holds each row's answer at sf40, `goldens/<ds>.sf40/gpu-result.txt` at
`tp1-single`, to `duckdb-result.txt` beside it, under the sf1 registry row's `duckdb_oracle`. The
cells' kinds are planned at sf1, so CI needs no sf40 data. An `ok` row must meet DuckDB. A `wrong`
row must still differ, so a mark that outlived its fix fails. A `skipped` row is not judged. A row
with no recorded answer, or whose DuckDB answer `failed:`, fails either way. A tree without its
`cases.csv` fails naming the file; with neither, the case reads nothing. Three cases pin the list
rule, a stale `wrong` mark, and an `ok` row with no answer

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

| Corpus goldens, self-consistency | [test_corpus_goldens](../peacockdb-core/tests/test_corpus_goldens.rs), [benchmark](../peacockdb-core/tests/test_corpus_goldens/benchmark.rs), [benchmark_checks](../peacockdb-core/tests/test_corpus_goldens/benchmark_checks.rs), [benchmark_lists](../peacockdb-core/tests/test_corpus_goldens/benchmark_lists.rs), [benchmark_record](../peacockdb-core/tests/test_corpus_goldens/benchmark_record.rs), [benchmark_run](../peacockdb-core/tests/test_corpus_goldens/benchmark_run.rs) | 66 |
|---|---|--:|

the committed sections against their own arithmetic, with no dataset and no run: `consumed +
abandoned == emitted` at every node, lane counts against `lanes=N`, the loader's `batch_rows` a
prefix of its own lane's `partition_groups`, the root's `out_rows` against `.result.txt`, and the
tree the indentation draws. The golden is written by the run it will later check, so a file that
contradicts itself is the only witness to a renderer that is wrong. Each `*.fused.txt` section
passes `fused_problem`, its holds, cap, tree, runs and pipelining estimate against each other,
and a measured one must say `answers=match` (both H100 files, and two doctored cases). The benchmark tree gets the
same reading, through its one Rust reader, `parse_benchmark_section`, and each timed section through
`section_problem`: every `total_us` is the sum of the `time_us` beside it and `device_us` the sum
of the totals. Where a section carries them, `load_us` is the `GpuLoadParquet` totals,
`load_us + execute_us` is `device_us`, each node's `batch_rows` and `batch_bytes` have one shape,
and `peak_bytes` is at least every `batch_bytes` entry. Every timed section outside `h200/` must
carry the batches and the seven keys of the load, the peak, the cache and the prefill;
`h200/`'s own section under another device's name is refused, naming `batch_rows`. The committed
`h100-80gb-hbm3/` tree passes both checks, and a section that fails one is never committed, so
the H100's q3 section is also held as a literal: it passes both checks, and each is shown
refusing it with `load_us` or `execute_us` one off, `peak_bytes` one byte below its largest
batch, one `batch_bytes` entry gone, or `prefill_us` deleted. Every
committed tree reports a release build, every timed (query, mode) is enabled on a device in
`corpus_cases.inc`, and the case list is, byte for byte, what the registry's enabled device cells
for tpch and tpcds write. `testdata/benchmark-report.html` is, byte for byte, what the trees render
through that reader, and each fused file's table through `fused_report_tables`; a page that is
missing or stale fails, naming its first moved line and the command that rewrites it.
Every case the list declares has its section, timed or a failure's record, in every device's tree
that holds no `cases.csv`. A tree that holds one is held to its `ok` and `wrong` rows instead:
`h200/` is held to `h200/cases.csv`, the list it was measured from. Those rows must be main-list
cases, at modes the main list carries for them; `h200/` alone is exempt, measured from an older
list. Every device list parses. `duckdb-cpu128/`, DuckDB's results, is no device tree: no check
here lists it or holds it to the main list.
Every file lies at
`<device>/<dataset>.sf<sf>/<mode>.benchmark.txt` under its device's slug, its timed trailers ending
with the five provenance keys and a failure's record holding its status line and the five. Only
`h200/`, from before provenance, may have timed trailers without them. Over synthesized trees: its
sections under another device's name are refused, and are short of the list there; a tree with
its own list answers to its rows, a `skipped` row declaring no section; a list naming a query
the main list lacks, or a mode its line lacks, is refused naming it, though the tree holds the
section; a `duckdb-cpu128/` beside `h200/` is no device; a file short of
one declared section is named; a trailer naming another card than its directory is refused, and one
naming its own passes; a failure's record without the five is refused even under `h200/`. Over a
temp file, a run's write path: a case that panics is recorded on one line and the next case is
measured into the same file; a resumed run keeps the section this run's five measured, byte for
byte and without running the case, and runs again one measured under another cuDF, one that failed
and one never reached, and one under this run's five short of the batches or the load; a run
that does not resume measures a section with this run's five again;
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

| End to end | [tests::end_to_end](../peacockdb-core/src/tests/end_to_end.rs), with `limits`, `dimensions`, `accounting`, `schema_validation` and `dag` beneath it | 52 |
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
over a scan no row group survives answering 0. Eight run fused plans (`dag`), at tp1-single
and tp4-sized: two tpch roots over one shared lineitem, and three over a shared lineitem and a
shared orders, each root answering as its query alone; the three holding each shared batch
once; and TPC-DS q95 at tp1-single, its four `web_sales` loads one source, refused without
buffers and answering as q95 alone with its four. Two of the 52 are `#[ignore]`d against
[#182](tickets/memory.md#t182) — the budget boundary and the rebatcher's peak, both
properties that pricing a batch from the plan's schema took away — so 50 run. The first tier
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

| Recipes per join type | [every_join_node_writes_one_cudf_join_leaf](../peacockdb-core/src/wire/tests.rs) | 37 |
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
CASE operand or as `date_bin`'s argument is refused naming #249. A hold buffer crosses as
nothing: the same bytes and wire nodes as without it, and no recipe of its own. Several roots:
one buffer and one seq space, a shared source written once and read by each parent at one
offset, a buffered shared reader taking the source's own, the reader indexing it once, and one
root still written as `root` in the same bytes. The trivial kinds are not here — the plan goldens run them over every
corpus query

| Plan text | [every_column_reference_renders_name_at_ordinal](../peacockdb-core/src/plan_text/tests.rs) | 20 |
|---|---|--:|

the renderer against what the planner emits from real SQL: every column reference prints name
at ordinal, every node carrying a fetch prints it, a join prints its keys and projection by
name — a mark join's mark among them — a null decimal prints as `NULL`, a source prints its
mapping verbatim, a name that is not a token is backquoted, no node name carries an exec
suffix, and a hold buffer prints a line of its own. Over several roots: one root printing no
header, a shared node printed once and referenced as `@1` after, a buffered reader's buffer above
the reference, and the memory section drawn the same way

| One driver, two backends | [one_generic_driver_serves_two_backends_with_different_batch_types](../peacockdb-core/src/executor/tests.rs) | 2 |
|---|---|--:|

one source step written once over `Backend` and driven against two backends with different
batch types, so the trait's associated types are exercised the way both engines use them; and
a `CpuBatch`'s share holding the same Arrow buffers as the batch, column by column

*subcomponent*

| Drivers over a mock backend | [executor::driver::tests](../peacockdb-core/src/executor/driver/tests/mod.rs) | 169 |
|---|---|--:|

flow, backpressure, limits and accounting, asserted on calls rather than rows — pull counts,
queue bounds, batch release, the trace, the output hook (called once per batch a node queues
as its own and not at a forwarder, a refusal failing the run at that node and lane with the
hook's words, and `None` leaving the report as `run` makes it): the schedule and the two holds, both limit lowerings by
the calls not made and a mid-plan limit judged by the rows it emitted, never its input, what
each node emitted and consumed (the two records the corpus goldens
read), the accountant through the drivers, a backend failure or a call refused before it runs
stopping the query with the accounting still reconciling, a satisfied limit excusing no root
that never ended, a limit satisfied before any step still letting the next capped root in, a
fused root answering its rows alone with their order pinned where it unions a shared source
with its own,
the execution golden's text with every number chosen by the
script — over several roots a shared node rendered once — every empty scatter lane dropped while a lane that owes its probe reaches `SetBuild` with no
batch, a zero-row build draining like none, and the mock against its own
script; then the measurement side: the ABI-call record indexed by the lanes a node is driven on,
holding the backend's calls and not the driver's own, a backend that names no seq leaving every
entry unmeasured, the timing tree rendered with what each node cost, its batch tokens the
execution golden's node for node, and its load every loader's total rather than the first one's,
a call without a region and
a region without a call each refused, a region the clock rounded to nothing still counted as
ran, and the recorded node index the post-order rather than the walk order; and a sink that
received nothing answering one zero-row batch under its input's columns, which no node emitted.
Then the hold buffer under a join's hold: the subtree below it running during the build, held
taking only what it is offered, emitting in arrival order once the hold lifts, one store per
lane at 4 lanes, unheld forwarding as if it were not there, its batches held in the accountant,
and a failed build with batches kept still reconciling. Then several roots over shared
sources: each root answering what it answers alone, a shared batch loaded, held and released
once and taken by every reader in order, the hook seeing it once, the two deadlock shapes
answering with buffers, an unbuffered hold refused by `run_roots` before any executor is built,
every reader of a four-lane source taking every lane, a root that received nothing answering
its own empty batch, a batch's bytes leaving at its last reader or after it, at a buffer
keeping a share, and a failure with a share queued still reconciling. A limit per root: a
satisfied root retiring its reader of a shared source the other root still reads in full,
`LIMIT 0` retiring it before any step, a retired reader giving back what it alone still wanted,
a source with every reader satisfied stopping, a buffer under a satisfied limit giving back its
store, and a buffer keeping its charge after a limit retired the other reader. The cap: a cap
of one running roots one at a time, a waiting root's buffer taking all of a shared source while
nothing queues, a satisfied root freeing its slot, every cap giving the same answers, an
uncapped schedule deterministic, and capped roots with joins accepted and answering as alone.
And `validate_dag` tied
to the driver over 500 random DAGs — several roots, shared sources, joins, buffers on random
edges, and no cap or one below the root count: every DAG it accepts finishes, reconciles and never queues more than a batch per lane;
every DAG the driver cannot finish it refused; every DAG with an accumulator below a waiting
root's buffer refused as `UncappedAccumulator`; and every other DAG with each unbuffered hold
buffered is accepted and finishes

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

| Fused plans at sf1 | [tpch_q3_q5_q10_fused_answer_as_alone](../peacockdb-core/src/prototypes/fuser/tests/end_to_end.rs), [fused_batches_answer_the_corpus_goldens_on_the_cpu](../peacockdb-core/src/prototypes/fuser/tests/end_to_end.rs), [a_capped_first10_answers_the_corpus_goldens_on_the_cpu](../peacockdb-core/src/prototypes/fuser/tests/end_to_end.rs) | 3 |
|---|---|--:|

query-fuser's prototype over real plans, about 16 s for the three: tpch q3, q5 and q10 planned
at tp1_single, fused, and run on the CPU backend once with every held reader re-read and once
with each buffered on its lowest edge, each root answering as its query alone; and the `first10`
batch of the H100 case list, tpch and tpcds at sf1, each answer meeting its `.result.txt`
section in the CPU tier's row order, uncapped and again under a cap of one root at a time

*module unit*

| Driver internals | [executor::driver::scheduler::tests](../peacockdb-core/src/executor/driver/scheduler/tests.rs) | 63 |
|---|---|--:|

the accountant's formula, cache and two checks on plain figures, and a charge released only by
its last holder; a reader queue's cursors — one reader a FIFO with no share, a non-last reader
taking a share, a retired last reader handing back the originals, a retired reader abandoning
every row it never took, a drain counting the readers left; the plan index's numbering,
readers, regions and per-lane slots, on a tree as before and over shared sources; the
scheduler's corners enumerated, holds over regions a second root reads into, a held hold
buffer still picked for intake and a limit still holding one, a satisfied consumer retiring its
reader and a source with none left satisfied in turn, none retired on a tree, a cap holding the
roots past it, admitting the next as a root finishes and stopping at a buffer, and then a
differential test against a naive rescan on randomized DAG shapes with buffers among them; the lane
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

| Benchmark trailer | [test_support::benchmark_trailer::tests](../peacockdb-core/src/test_support/benchmark_trailer/tests.rs) | 3 |
|---|---|--:|

the `--- run ---` trailer a section ends with: its seventeen keys in order, the five provenance
keys last, each with the run's own value; `execute_us` is `device_us` less `load_us`; the
pinned cache's line as it prints, and the prefill's three numbers apart from `load_us`

| Benchmark tree reader | [test_support::benchmark_tree::tests](../peacockdb-core/src/test_support/benchmark_tree/tests.rs) | 6 |
|---|---|--:|

`parse_benchmark_section`, the tree's one reader: a timed section with every field, two loaders
under a join and the trailer's seventeen keys, reads its batches, its children and its load, execute and
peak, provenance last; an `h200/` section reads with no batches and no `load_us` or `peak_bytes`,
its load the loaders' totals; a tree with no loader loads `0`; a marker and a failed case's
record are not timed; an unknown key beneath a node and a trailer key held twice are refused

| Benchmark report page | [test_support::benchmark_report::tests](../peacockdb-core/src/test_support/benchmark_report/tests.rs) | 18 |
|---|---|--:|

the page over temp trees: a timed section's load, execute, total and peak, the total linked to its
`== ` line; an `h200/`-shaped section's load derived and its peak `—`; a load without the
pinned cache, its `pinned_cache` off or absent, marked `unpinned`; a failure's and a marker's
reason, and the device list's `skipped` note over both; a `wrong` row's `wrong answer #NN`; two
devices side by side, a case one lacks `—`; a directory with no mode file is no column; rows in
the main list's order, then by name, modes in `MODES` order; text escaped; the same page twice,
with no temp path in it; a later task's extra table under its heading; a file holding one query
twice refused, naming both lines; a temp tree's name taken once per process, so parallel tests
never share one; `section_href`'s line; a DuckDB tree a column of its own after the devices, its
time on each mode's row linked to its line, a time not warm saying its verdict, `failed` and `—`,
and its version, threads, host and `SELECT 1` floor named in the intro, with what one time holds

| DuckDB results reader | [test_support::duckdb_results::tests](../peacockdb-core/src/test_support/duckdb_results/tests.rs) | 9 |
|---|---|--:|

`parse_duckdb_results`, `duckdb.tsv`'s Rust reader (the query-batching generator's is held to the
same literal): the shared fixture to a literal, the values
`benchmarks/tests/test_duckdb_minimal.py` holds the script's output to; a working set it could not
size reads as none; a short row, an unknown status, a failed row with a time, an `ok` row without
one, a missing preamble, a column line out of order and a query listed twice refused, naming the line

| Benchmark prefill | [test_support::benchmark_prefill::tests](../peacockdb-core/src/test_support/benchmark_prefill/tests.rs) | 5 |
|---|---|--:|

which cases one benchmark process runs, by libtest's rules over the macro's test names: a filter
matches part of a name, `--exact` makes filters and skips whole names, `--skip` drops a match,
`--test-threads 1` and `--test-threads=1` are no filter, and a case timed at no mode never
runs; `aggregate_groupby` plans `aggregate-groupby.sql`. A case fails when the cache filled,
fell back, reloaded or evicted during its rounds, naming each count; when the prefill did not
fit or could not read a file, with its sizes (`prefill: needs 60 > limit 49`), but not when the
cache is off; and when one of its files is outside the prefill, naming it

| Benchmark case timeout | [test_support::benchmark_section::tests](../peacockdb-core/src/test_support/benchmark_section/tests.rs) | 8 |
|---|---|--:|

`PEACOCK_BENCH_CASE_TIMEOUT_S`: whole seconds or unset, an empty value unset, `30m` and `0`
refused naming the variable. The record a timed-out case leaves is a failed case's, which the
tree's reader takes as no measurement. A case still running past its timeout has its section
written as that record, beside the file's other sections, before the watch ends the process;
a case that ends in time drops its watch, which writes nothing. A measured case's answer is
recorded before its section, while the file holds none, and a refused answer is the case's
failed section

| Device case lists | [test_support::device_cases::tests](../peacockdb-core/src/test_support/device_cases/tests.rs) | 24 |
|---|---|--:|

a device's `cases.csv`, through its one reader and writer: a list round-trips, and a note keeps
its commas. A skipped row names a reason and no mode. An `ok` or `wrong` row names its modes. A
`wrong` row's note is its `#NN` ticket, and an `ok` row has none. Refused, naming the line: a
wrong header, an unknown mode or status, a row short of six fields, and a case listed twice.
`testdata/fixtures/device-cases.csv` reads to a literal, the one `test_device_cases.py` asserts.
And the rows a tree makes of the main list's candidates (`device_lines`): a timed section is an
`ok` row; a query failed in every mode is `skipped` with the first reason; a mode failed beside a
timed one drops from the row; a mode whose file is absent, or which the line does not carry, is
not considered. A missing section is refused naming dataset, query and mode, a missing
`tp1_single` file too, except a later mode of a query failed at `tp1_single`. A `wrong` mark comes
back on its measured row; one on a skipped query, or on no candidate, is refused

| Golden merge | [test_support::corpus_golden::tests](../peacockdb-core/src/test_support/corpus_golden/tests.rs) | 11 |
|---|---|--:|

sixteen writers merging one golden file at once each keep their section: the lock is on the
directory, because the rename that publishes replaces the file's inode (#213). And the
mode-keyed merge `gpu-result.txt` takes: one cell replaced in an ordered file, and a section
dropped once the registry stops enabling its cell — what makes the coverage guard's advice to
regenerate true in both directions. Then the `cudf=` provenance line: written first, dropping
whatever another cuDF recorded rather than carrying it under this one's stamp; read back against
the path, both ways; refused where two such lines disagree; and the same rule at the writer, so a
26.02 cycle told to record the committed file fails before it writes. At sf40, where the registry
has no rows, a recorded answer keeps the others: the cells are read at sf1, the benchmark's; at sf1
a cell the registry leaves off still loses its section

| DuckDB oracle | [test_support::duckdb_oracle::tests](../peacockdb-core/src/test_support/duckdb_oracle/tests.rs) | 50 |
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
cpu's fingerprinted section still refused on its line. At sf40, where no line is written, the
two sections compared by fingerprint when either holds one, the device's or DuckDB's, and as rows
when neither does, whatever the line declares. Plus the
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

| Pinned host cache, no FFI | [executor::pinned_cache::tests](../peacockdb-core/src/executor/pinned_cache/tests.rs) | 8 |
|---|---|--:|

`peacock.pinned_host_cache_bytes`: unset in a fresh session, `SET` read back, a non-number
refused naming the key; the cache's trailer value in its four forms, on one line; a prefill's
refusal as its two sizes; and the files a plan reads, from tpch sf1's q3 and q6 planned whole:
each file once over several roots, a load of no column skipped

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

| Plan types | [plan::validate::tests](../peacockdb-core/src/plan/validate/tests.rs), [plan::layout::tests](../peacockdb-core/src/plan/layout/tests.rs), [plan::aggregate::tests](../peacockdb-core/src/plan/aggregate/tests.rs) | 60 |
|---|---|--:|

validation's shape rules — a plan ends in a crossing, every root is a sink and nothing reads
one, under one root or several, no root given twice, a shared node is checked once, a limit has a
real consumer, no node declares no column ([#63](archive/archived-tickets.md#t63)), no schema — an aggregate's intermediate included — literal, cast or function
names a view type the device cannot hold; a pass-through
node declares its input's column count and its fields;
layout canonical forms and equality; whether a hash survives a regrouping. Then holds over
several roots: an unbuffered reader of a shared source inside a join's hold refused by name, a
join reading one source on both sides refused at its probe edge, two roots whose builds wait on
each other's probe refused, each passing with a buffer on the edge, no tree ever needing one;
the wait-for graph's cycle finder and its message, a join waiting on the hold its build sits
in, `validate_dag` refusing what its parts refuse, and the fixture that buffers every refused
edge. Then the cap: a waiting root's unbuffered reader of a shared source refused naming the
first root that waits, a cap at or above the root count needing no buffer, a waiting root
never waiting on its own region nor a join on its own root's cap, and an accumulator below a
waiting root's buffer refused by name while stateless nodes there are accepted

| Plan DAG walk | [plan::dag::tests](../peacockdb-core/src/plan/dag/tests.rs) | 10 |
|---|---|--:|

the walk over several roots: on every `every_kind()` fixture it numbers as the tree walk did,
pre-order, post-order, depth as height, regions as contiguous subtrees; a shared source is one
node with a reader per edge, the roots number in the order given, a shared node's height is
below its deepest reader, a region skips a node read from outside it, a join reading one source
on both sides holds nothing below its probe edge; a hold stops at a `GpuHoldBuffer` and a node
also read around the buffer is still held; only a `SharedSource` has a sharing key, and
`share_sources` makes equal loads one node

| Memory estimation | [planner::memory_estimation::tests](../peacockdb-core/src/planner/memory_estimation/tests.rs) | 19 |
|---|---|--:|

the model's rules from SQL over the minimal dataset: an accumulator ends the walk and what it
holds comes off the budget first, a batch is charged once per lane in force, a loader is priced
by the batches its mapping makes, amplification is the widest point on the path, a target lands
on the coarse grid, constants that cannot be less and exceed the budget are a plan-time
error, a join that answers at the finish — hash or nested loop — is charged its `matched` byte a
build row, a RightSemi its distinct build keys, an empty relation is no source the budget sizes,
and a hold buffer is charged everything its input emits while passing its batches on unchanged.
Over several roots: a shared source charged and given a share once, left at one batch per lane
when a buffered reader keeps the rest, amplified by its widest reader, and two roots' accumulators
summing to what each charges alone

| Translator expressions and scan mapping | [planner::translator::expr::tests](../peacockdb-core/src/planner/translator/expr/tests.rs), [scan_mapping::partition::tests](../peacockdb-core/src/planner/translator/scan_mapping/partition/tests.rs) | 31 |
|---|---|--:|

a column keeps its ordinal and name, a literal DataFusion's scalar, a binary op its declared
type, the four unaries one for one; from parquet metadata, rows, bytes and compressed bytes are
the surviving row groups' own totals over the projected columns, a scan over several files is refused, a declared
schema that renames or reorders a column the scan reads is refused naming the ordinal and both
names, and the lanes deal the survivors as the mapping says

| Expression text | [plan_text::expr_text::tests](../peacockdb-core/src/plan_text/expr_text/tests.rs) | 4 |
|---|---|--:|

an interval prints the parts that are not zero, a decimal as a value and a null one as `NULL`,
every form readably

| Fused batch lists | [prototypes::fuser::lists::tests](../peacockdb-core/src/prototypes/fuser/lists/tests.rs) | 8 |
|---|---|--:|

which queries a fused batch holds: a case list's `ok` and `wrong` rows at tp1_single in file
order, tpch's q1-q22 only; a missing list or an empty set panics naming it; each batch size a
prefix, `first3` always, a larger one marked as the same set as `all` once the list is no longer

| Fused format and benchmark rules | [prototypes::fuser::section::tests](../peacockdb-core/src/prototypes/fuser/section/tests.rs), [prototypes::fuser::benchmark::tests](../peacockdb-core/src/prototypes/fuser/benchmark/tests.rs) | 20 |
|---|---|--:|

a `.fused.txt` section read back, measured and out of memory, its shared tree nodes once and
its `@k` references as edges, a marker untimed and a key held twice refused; a section rendered
from a fused plan reads back as written; an out-of-memory error is recognised by its text; the
fused file and record lie where the device tree's do; a rerun batch's record written whole, a
captured run's never into it, and its rows named as the batch's case range names it;
a fused batch's files are its queries' union; each load's `CudfScan` seq, `loads.tsv`'s key; a
batch the list does not exceed declared a marker; a resumed run keeping a section only under its
own provenance, an out-of-memory one by its five keys; the second-smallest run; a batch's sf40
baseline its queries' sections in batch order, a missing one panicking naming it, and every
batched query of both lists holding a baseline section and `loads.tsv` rows

| Fused batches on the report page | [prototypes::fuser::report::tests](../peacockdb-core/src/prototypes/fuser/report/tests.rs) | 6 |
|---|---|--:|

a table per fused file, by device, dataset and sf; a measured batch's load, execute and peak in
the page's units, its batch linked to its section's line; an out-of-memory batch showing its cap;
a marker `same set as all`, a failed batch its reason; no fused file, no table

| Fused batch baseline | [prototypes::fuser::baseline::tests](../peacockdb-core/src/prototypes/fuser/baseline/tests.rs) | 5 |
|---|---|--:|

a `.benchmark.txt` section read per node in pre-order, `batch_rows` and `batch_bytes` summed
over lanes and batches, the trailer's load, execute and peak; a section without one panics
naming it, and a baseline of another plan is refused naming the query and the node

| Hold rule and concurrency cap | [prototypes::fuser::holds::tests](../peacockdb-core/src/prototypes/fuser/holds/tests.rs), [prototypes::fuser::cap::tests](../peacockdb-core/src/prototypes/fuser/cap/tests.rs), [prototypes::fuser::loads::tests](../peacockdb-core/src/prototypes/fuser/loads/tests.rs) | 30 |
|---|---|--:|

the fuser's memory rules on chosen numbers and hand-built fused plans: a buffer goes on the
smallest allowed recorded edge, the lower on a tie, never above a join that grew the rows, and
charges the builds of the joins it runs early, a build an earlier buffer holds priced as what
it buffers; within the budget it is chosen where it fits the
headroom, over it where it keeps less than the re-read adds to its root, and a re-read
otherwise, the ledger charging the plan as it stands at each decision, so a cap hold fits in
what a group a re-read dissolved freed; a re-read's bytes
are its own load's; `loads.tsv` read back by case, a
missing row a panic; the cap admits the largest roots that fit, shared loads and buffers
charged whatever it is, a root its peak less its shared loads; settling a batch buffers or
re-reads every root the cap makes wait, never above a waiting root's state, a join's buffer
placed above state in a root that comes to wait taken out and decided again, and validates

| Merge hint | [prototypes::fuser::merge::tests](../peacockdb-core/src/prototypes/fuser/merge/tests.rs) | 6 |
|---|---|--:|

identical subtrees above a shared source across queries: two equal filters are one entry saving
the cheaper copy, a different predicate is none, only the largest identical subtree is an entry,
a nested group of fewer members counts once in the batch's total, a buffer splits no group,
and a query repeating a subtree with itself is none

| Pipeline hint | [prototypes::fuser::pipeline::tests](../peacockdb-core/src/prototypes/fuser/pipeline/tests.rs) | 6 |
|---|---|--:|

the schedule of one loader beside one executor: the cheapest root loads first, a shared source
loads once, a root waits for every source and for the executor; its inputs off a fused plan
take a shared source's largest member load, and count a re-read as a source of its own

| The fuser | [prototypes::fuser::fuse::tests](../peacockdb-core/src/prototypes/fuser/fuse/tests.rs) | 17 |
|---|---|--:|

query-fuser's sharing rules over hand-built plans on real sf1 files: two scans of one file read
one load of the union of their columns and row groups, each consumer through a column-ref
project or the source itself; a limit-cut scan, a scan whose columns would gain NULLs and a
rows-only scan beside column scans stay unshared, each with its reason; a held reader is re-read
or buffered on the edge the chooser names, each named once; a group a re-read leaves loads only
what its remaining readers read, and one left with a single reader reads its own load; origins
map every fused node back, sized batching is refused, a root waiting on the cap is resolved, a
chooser buffering a waiting reader above state is refused naming it, and the fused roots answer
as each plan alone

| Expression writer | [wire::expr_writer::tests](../peacockdb-core/src/wire/expr_writer/tests.rs) | 21 |
|---|---|--:|

every variant, every operator, and the literals the corpus actually produces — a wrong scalar
is invisible in plan text and wrong on a device. Each of DataFusion's three interval literals
crosses as months, days and nanoseconds, a NULL one as a typed null, and an interval anywhere but
a date or naive timestamp ± it is refused naming #249

#### ffi — default features: FFI linked, no device. 9 cases: `--lib -- ffi_tests::` 5, `peacockdb-ffi --test test_ffi` 4

*crate integration, external*

| FFI smoke | [test_executor_lifecycle](../peacockdb-ffi/tests/test_ffi.rs) | 4 |
|---|---|--:|

the crate links; executor lifecycle; `install_rmm_pool` refuses a zero request; with no device the
pinned host cache is `NO_DEVICE` at capacity 0 whatever is set, a prefill answers `OFF`, and a
null output is refused

*component*

| GpuBatch surface | [executor::ffi_tests](../peacockdb-core/src/executor/ffi_tests/mod.rs) | 4 |
|---|---|--:|

what the batch reports, that `consume` hands the handle over without releasing it, and that an
NVTX range name with an interior NUL is refused before the C side sees it. Needs no device: the
release is null-guarded on the executor

*module unit*

| Pinned host cache wrappers | [executor::pinned_cache::ffi_tests](../peacockdb-core/src/executor/pinned_cache/ffi_tests.rs) | 1 |
|---|---|--:|

the Rust side of the same no-device answer: `NoDevice`, `off no-device`, a session's option
reaching the C side as `Session` and pinning nothing, a prefill `Err(Off)`

#### gpu — `--features gpu`: shad-gpu only, but for one by-hand target. 2425 cases: `--lib -- gpu_tests::` 748, `test_gpu_corpus` 1009, `peacock_gpu_benchmarks` 659, `test_node_timing` 1, `prototype_fused_benchmarks` 8

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

| Fused batches, by hand | [prototype_fused_benchmarks](../peacockdb-core/tests/prototype_fused_benchmarks.rs) | 8 |
|---|---|--:|

query-fuser's eight `bench_fused_<ds>_sf40_<batch>_tp1_single` runs, sf40 on the H100 only:
no CI step and no shad-gpu list runs them, which the CI wiring guard checks (*Fused batches*,
below)

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

| Fused plans on a device | [tests::gpu_tests::dag_cases](../peacockdb-core/src/tests/gpu_tests/dag_cases.rs) | 5 |
|---|---|--:|

`end_to_end::dag`'s fused plans on the device. Each query runs alone in its own session, then
all of them as one plan in one session (`Session::open_roots`), and each root's answer equals its
query's alone, sorted. Two tpch roots over one shared lineitem and three over two shared sources,
at tp1-single and tp4-sized; TPC-DS q95 at tp1-single, with its four buffers

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

| Per-call ABI | [executor::gpu_backend::gpu_tests::abi](../peacockdb-core/src/executor/gpu_backend/gpu_tests/abi.rs) | 5 |
|---|---|--:|

three of the four per-call symbols on a live GPU — a scan's row groups, an export range, a
slice; `handle_schema` is `test_support`'s schema read — and
the release skipped exactly where a call consumed the handle; and a `GpuBatch`'s share, through
`peacock_handle_duplicate`, reading the same rows under the same producer after the original
is dropped

*module unit*

| Pinned host cache on a device | [executor::pinned_cache::gpu_tests](../peacockdb-core/src/executor/pinned_cache/gpu_tests.rs) | 2 |
|---|---|--:|

a session's `pinned_host_cache_bytes` reaching the cache, `0` as `Off` and unset as the default;
a prefill's files and bytes, a repeat reading nothing, a refusal of what does not fit that pins
nothing, a missing file as `Io` naming it, and `Off` at 0. With
`PEACOCK_PINNED_HOST_CACHE_BYTES` set, each checks only that the environment won


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
| CI wiring guard (Rust) | every Rust target must be named by a CI step — CI does not glob, the three lists that decide where a GPU target runs must agree, and both GPU runners must pass `--test-threads=1`, which is the whole of the single-tenant invariant inside a process and is what a device test's `unsafe { set_var }` rests on. The rungs the `--test` sweep cannot see get one assertion each: cpu (`--lib` under `rust-only`), ffi (`--lib -- ffi_tests::`), device (the staged lib binary and the loop line handing it `gpu_tests::` — shad-gpu runs prebuilt binaries, so there is no command line), plus the CLI build, which has no test target. A target exempt as a manual prototype is named by no workflow line, build or run, and by neither staging list. The three runners must agree on the lib's staged name and rung too: pipeline.yml with `nebius-test.sh`, `build-test-shadgpu.sh`, and the image path, whose name is the one `stage-install.sh` stages and whose rung is `run-tests.sh`'s. The rung must reach the binary: through `rung_args` in shad-gpu's loop, and in `run-tests.sh` appended to the arguments under its `LIB_STAGED`, which must be the staged name; that reader refuses a commented-out rung, a run without the arguments and another name. The reader is scoped to the rust loop: a file-wide search passes on the comment above the command, and the C++ loop above it shares the loop variable and rightly carries no flag; and the benchmark binary runs on CI without its `bench_` cases, `--skip bench_` read off the same loop. Two scripted guards ride here too, for want of anywhere else that reads the workflows: `generate_pbench.sh --check`, whose single `run:` line is all that holds pbench's committed parquet to `gen.sql`, and `batching_report.py … --report llm-wiki/reports/query-batching.md --check`, all that holds the query-batching page's tables to the committed results. Each matcher rejects a commented-out line — `fold_continuations` keeps a comment verbatim, and commenting a step out is the likelier way it stops running than deleting it — and the page's also a line without `--check`, which rewrites the page instead of checking it | [every_rust_test_target_is_named_by_ci](../cost-report/tests/test_source_formatting/ci_coverage.rs), [the_batching_report_check_is_named_by_a_ci_step](../cost-report/tests/test_source_formatting/ci_coverage.rs), [each_rung_has_its_ci_line_and_the_cli_is_built](../cost-report/tests/test_source_formatting/ci_coverage.rs), [the_three_gpu_target_lists_agree](../cost-report/tests/test_source_formatting/runners.rs), [the_run_tests_reader_refuses_a_rung_that_cannot_reach_the_lib](../cost-report/tests/test_source_formatting/runners.rs), [a_manual_prototype_is_wired_nowhere](../cost-report/tests/test_source_formatting/ci_coverage.rs) | cost-report | 15 |
| Module layout rules (Rust) | the component walls, which the compiler mostly cannot check: where a `pub` may appear, that a subcomponent is declared `mod`, that no `super::` chain climbs out of its component, that no public signature names a type from a private module — `private_interfaces` reads a type's nominal visibility, so an unreachable type spelled `pub` passes it silently — and that no `pub` in `test_support/mod.rs` or `prototypes/mod.rs` names a type from an engine component, which is what keeps the harness a facade and not a rename. No file outside `prototypes/` and the tests names `prototypes`, `lib.rs`'s declaration aside, and the component builds only with `test-support`. Sibling reach is the case rustc refuses to have an opinion on at all: no visibility level means "my parent but not my siblings". Five more rules say where test code may live: a test module is named for the build rung it needs and gated for it, in both directions; its body is a file of its own; a `#[cfg(test)]` sits on nothing but a test-module declaration; and a path compiled only under a test gate says `test` in its name. Bare `pub` is checked against `SURFACE`, the CLI's API by file and name, in both directions, so a dropped-and-added pair cannot pass a count; `pub mod` is pinned to `lib.rs`, `test_support` and `prototypes` the two behind a feature. One register remains, the items that keep a `#[cfg(test)]` because no test module can hold them, checked in the reverse direction too, so an entry outliving its reason goes red. One rule here is a task spec's rather than the style guide's, and the same kind of claim: the device corpus tier records `gpu-result.txt` ahead of both comparisons against the cpu, read as the order of three calls in `gpu_case`, because the divergence DuckDB settles is the one that makes those comparisons panic. Two more read the tree: a doc block a split left behind above another item's, and no peacockdb-core test naming `CARGO_MANIFEST_DIR` outside `testdata.rs` and `build.rs`, so none reads the checkout a staged binary's host does not have. Needs no dataset and no device | [no_public_signature_names_a_type_from_a_private_module](../cost-report/tests/test_source_formatting/privacy.rs), [the_device_case_records_its_answer_before_it_compares_with_the_cpu](../cost-report/tests/test_source_formatting/write_order.rs), [no_core_test_reads_the_checkout](../cost-report/tests/test_source_formatting/checkout_reads.rs), [no_engine_file_names_prototypes](../cost-report/tests/test_source_formatting/prototypes.rs) | cost-report | 27 |
| Crate privacy probe (Rust) | a probe crate compiled against the built library proves `wire::generated` is unreachable from outside, with a positive control so a probe that fails for the wrong reason cannot pass as proof. Its own binary because it needs the rlib, so it is never staged | [a_private_module_is_unreachable_from_outside_the_crate](../peacockdb-core/tests/test_crate_privacy.rs) | cost-report | 1 |
| Cost-report renderer (Rust) | glyphs, links and the anchors they resolve to, ratio bucket, regression gate, history, every tpch and tpcds registry query has DuckDB's figure, every micro-query PeacockDB's, by its hyphenated section; the PR comment under the body cap with no Features column and pbench as its summary line, while the html page and the `--widget-md` tables keep both. No comment row is wider than its header. The checked-in `testdata/report.html` has a Features header per dataset and a row per pbench query, so a page laid out from the comment's `--md` goes red | [bucket_threshold_is_1_4](../cost-report/src/main.rs), [regression_count_drives_exit_decision](../cost-report/src/main.rs), [every_refusal_names_a_ticket_that_exists](../cost-report/src/main.rs), [a_registry_row_with_a_quoted_oracle_parses](../cost-report/src/main.rs), [the_checked_in_report_keeps_features_and_every_pbench_row](../cost-report/src/main.rs) | cost-report | 49 |
| DPhyp join enumeration (Rust) | the optimum against a brute-force DP over every split, on chains, stars, cliques, cycles and hypergraphs; the connected pairs it enumerates against the paper's closed forms; the pair budget, malformed input, and the C ABI's postfix tree | [dphyp_finds_the_brute_force_optimum_on_every_shape_up_to_six_relations](../peacockdb-dphyp/src/tests.rs), [each_connected_pair_is_enumerated_once_as_the_paper_counts](../peacockdb-dphyp/src/tests.rs) | cost-report | 6 |
| DuckDB cost extraction (Python) | classifier / pruning / dynamic-filter logic — fails CI before generation | [scan_count_mismatch_fails_loud](../testdata/test_duckdb_cost.py), [compute_pruning_from_rowgroups](../testdata/test_duckdb_cost.py) | cost-report | 41 |
| DuckDB result rendering (Python) | the only side that can get the result oracle's rendering wrong on its own: a timestamp must read as arrow-rs prints it (`…00.001`, not `isoformat()`'s `…00.001000`) or every row carrying one is a false divergence, and the over-cap fingerprint must be the text `test_golden_format.rs` pins, byte for byte, or the two writers compare nothing; a `no_sort` query's rows kept in DuckDB's order, read off a quoted registry cell. sf40's answers database renders byte for byte as the sf1 parquet does, a query with no table `failed: no answer recorded`, an empty database refused, the file written under `goldens/<ds>.sf40/`; `--answers` needs `--sf` other than 1 and one `--dataset`, refused before DuckDB is imported. An answer over the cap by its row count alone (sf40's 96M-row filter-project) fingerprinted in batches through an outside sort and never rendered, the streamed fingerprint byte for byte the in-memory one. The five that render need the duckdb wheel and `tpch.sf1`, and skip in CI, which has neither | [test_a_timestamp_renders_as_arrow_rs_prints_it](../testdata/test_duckdb_result.py), [test_both_writers_fingerprint_the_same_rows_the_same_way](../testdata/test_duckdb_result.py), [test_render_keeps_order_when_asked](../testdata/test_duckdb_result.py) | cost-report | 29 |
| Report widget (Python) | the registry read — a quoted oracle cell is one field, a registry older than `duckdb_oracle` or `stored` still renders, and the include fallback reads a line on the grammar — and the four columns after Query: Result stored (the registry's `stored`), DuckDB, cpu ↔ DataFusion and gpu ↔ cpu, each header's hover text saying what its comparison does. q2's stored and DuckDB cells divide into labelled halves, a one-word cell does not, a row with no line keeps four cells, and the fingerprint's text says what it compares. The widget's input flag is `--widget-md`; the old `--md` is refused before a page is written | [test_a_quoted_oracle_cell_is_one_field](../scripts/test_ticket_widget.py), [test_q2s_two_cells_split](../scripts/test_ticket_widget.py), [test_the_old_md_flag_is_refused_before_the_page_is_written](../scripts/test_ticket_widget.py) | cost-report | 11 |
| Exec-model prototype (Python) | the scheduler over mock traits, plus pandas-backed operators checked against a single-shot oracle at five partitioning configs, both limit lowerings, the scalar expressions pinned to what `expr.cpp` does rather than what pandas defaults to, and every join mode against an oracle written from the SQL — the hash join and the nested loop at all nine types, each streaming its probe and answering a call as the device's join session does, a residual on every family, `null_equals_null` counted by hand, a guard that the prototype names every join type `join_session.cpp` dispatches on, the hash table's bytes a row held to the Rust, and a residual crossing where a conjunct reads both sides — no project code. Over the committed `tpch.minimal`: `LayoutInjector`'s own guarantees (each preset a different layout, an unshuffled join's lanes kept, empty batches really emitted, a seed that reproduces), and a stream fitting a budget the collected table does not. And the optimizer's half, still without data: the engine's plan goldens of all three datasets read back, lowered and built node for node — pbench's types, `GpuEmpty`, a scan of no column, a LIKE against a column, a cast between timestamp units, a month interval, whole days of one on a date, and a remainder among them — the layout each node declares, statistics, selectivity, row estimates and C_out, every other mode's joins among the one mode the cardinality golden holds, the per-call model (a semi join's keys taken distinct per probe call at the device width, that width's rule held to the Rust), dynamic-filter candidates — a key lifted into every branch of a union and through every join type, a join equating NULLs pruning nothing by a build holding one and an anti or mark join without the flag pruning as an inner join does, a build filtered only by the planner's null-key filter no candidate, nested builds and builds over a scan another filter prunes probed after them, a second pass's names after the first's — and the second pass over toy tables: a joined build its estimate overcounts still pruning, a build that misses replanning inside its probe, a probe whose build holds its whole cluster costing no more than the plan without it, a probe's refused miss, a probe whose root is hashed on four lanes keeping that hash through its replan, and a probe with no miss equal to its side run whole; join clusters assembled, a non-inner join with a residual among them, named — their ids kept through a reorder, a flip and a replan — read through the planner's null-key filter below a shuffle, which reassembly puts back on the sides `null_key_droppable` names (that table held to the Rust), where it tests only columns the shuffle hashes and the join keys on, on a side that drops NULL keys of a join not equating them, any other filter rooting a relation, tp1 and tp4 holding the same clusters over every plan golden, and disassembled, non-inner joins in them placed by their TES — every order the conflicts admit over three relations and a sample over four, with NULLs, NULL held equal or not and residuals sampled, answering as the plan does — and the four named non-inner queries' shapes reordered over toy tables, a left join's estimate no lower than its preserved side and an anti join's above 0, build side or probe, where the key domain is wider than both key counts, the adaptive loop — its build events, the replan they stop a run for, and the memory sources it feeds the builds back through — and DPhyp through its C ABI — the files that call it need a build, the crate's `.so` named by `PEACOCK_DPHYP_LIB`. The optimizer's report of a run over toy tables, each rule recorded where it fires and nothing where it does not — each relation's rows, how they are known, width and key NDVs, an aggregate's output column, a non-inner join's type on both trees — its `.optimizer.txt` rendering, and its `.optimizer.md` page: a cluster reordered, flipped and replanned as one table of the orders that changed it, a kept one as a line, a probe's part of a cluster, legends closed over the sources they name, both passes' filters by scan and build, and the anchors GitHub gives its headings. The report's names: a relation by its table, filter and aggregate, the planner's null-key filter no filter, a table the plan reads twice numbered, an aggregate's column kept through a memory source, a join tree with its build bracketed and its kind as SQL names it, a missed join in words. A clustered join's id in the `cpu.txt` render, the answer compare `run.py` holds an optimized run to, and `run.py`'s section merge, a page's preamble and headings, corpus order, mode table, checks before any work and a failing query's section, a report no renderer can write among them. The engine's cost function read from `cost_model.conf` — a hand-priced run, the total summed and rounded as the Rust does, lines split as Rust's `lines()`, a kind the conf lacks, a memory source free, and every committed `-mini.cost.txt` derived again from its `cpu.txt` — the optimized cost as the final run's, the probe plans' and the stopped runs', and `cost_report.html` from hand-built rows: its columns, a skipped and a failed run, the ratios, a ratio other than 1.00 linking to its query's section of the mode's page, relatively or at a commit, the rules a line per mode — the multi-joins reordered, the filters applied per pass, the replans, or that the plan changed — each linking to its lines of the mode's log, and "nothing changed the plan" only where no log says it did, the summary counted by totals rather than the shown ratio, a cost file priced under another conf refused naming it; those counts and lines read from a `.optimizer.txt` section and from what `report_text` writes, a call inside a probe's replan and a replan keeping its order among them, and the committed page rendered with pandas, pyarrow and numpy blocked. A cost that raises stops DPhyp's solve with its exception. A scan's columns read once per task, and the shuffle's digests pinned per dtype | [test_a_join_in_its_build_phase_holds_back_its_probe_subtree](../scripts/exec_model/tests/engine/test_scheduling.py), [test_every_join_type_matches_the_oracle](../scripts/exec_model/tests/operators/test_join_capability.py), [test_a_dphyp_call_records_its_input_the_sets_it_priced_its_tree_and_each_flip](../scripts/exec_model/tests/optimizer/test_report.py), [test_the_engine_s_cost_goldens_are_what_this_function_derives_from_their_cpu_runs](../scripts/exec_model/tests/plans/test_cost_model.py) | cost-report | 523 |
| Exec-model statistics and estimates (Python) | the committed NDV sidecars counted again from the generated data by `gen_stats.py` and compared byte for byte, each column they list served by the reader, and every join's row estimate against the engine's own CPU run, one golden line per join. And `run.py` over two small tpch queries and a refused tpcds one, by two workers into a temp directory: each optimized answer the planned one, its three files per bench and the cost report, the engine's tree and join rows where no rule changed the plan, the optimized cost the planned one where nothing fired, and a filtered run of the query DPhyp is called for that leaves the other sections alone. And every tpch and tpcds golden plan at every mode ordered without a run (pbench has no sidecar): it builds, and no final aggregate at more than one lane meets its rows unhashed on its group keys. They ride dataset-matrix because that is where sf1 and its DuckDB are | [test_the_tpch_sidecar_is_what_gen_stats_counts_from_its_data](../scripts/exec_model/tests/optimizer/test_stats_sidecar.py), [test_tpcds_join_estimates_match_the_golden](../scripts/exec_model/tests/optimizer/test_cardinality_corpus.py), [test_run_corpus.py](../scripts/exec_model/tests/test_run_corpus.py), [test_ordered_corpus.py](../scripts/exec_model/tests/optimizer/test_ordered_corpus.py) | dataset-matrix (25.02 leg) | 10 |
| Exec-model sidecar across embeddings (Python) | tpch sf1 generated with `--embeddings external`, counted by `gen_stats.py`, gives the committed sidecar byte for byte — the synthetic default is the dataset tier's own recount. It generates the dataset, a minute or two, and needs the embeddings cache, which no CI host may fetch; without the cache it fails rather than skips | [test_tpch_with_external_embeddings_gives_the_committed_sidecar](../scripts/exec_model/tests/optimizer/test_stats_embeddings.py) | manual — a host with the embeddings cache | 1 |
| Exec-model engine plans (Python) | the engine's own plan goldens run by the prototype over sf1 against DuckDB: as planned (130), pruned by their dynamic filters before and after join order (73), with every join cluster disassembled and rebuilt (111), and reordered by DPhyp and oriented (111) | [test_engine_answers.py](../scripts/exec_model/tests/plans/test_engine_answers.py), [test_engine_optimized.py](../scripts/exec_model/tests/optimizer/test_engine_optimized.py) | manual — exec-model-corpus.yml, 3 shards | 425 |
| Exec-model per-call model, measured (Python) | the per-call model fitted from the sf40 calibration record orders each query's modes as the device did: at least fifteen pairs of modes whose measured times differ by more than 10 %, none the wrong way round — 25 of the record's 30 today. The record is not in git: the job fetches it from S3, and the test fails naming the fetch script when it is absent | [test_the_measured_record_orders_each_query_s_modes_as_they_were_measured](../scripts/exec_model/tests/optimizer/test_call_cost_measured.py) | manual — exec-model-corpus.yml, call-cost job | 1 |
| Calibration scripts (Python) | the three capture readers and the plotter over a synthetic two-case Nsight export written by the test: two cases stay two row sets, each case is checked against its own dataset's plan golden, a join's build, probe and finish calls checked as the `CudfJoin` its recipe declares, a region's walk reading its own calls and not the rest of its thread, by index and never by a slice or an `islice`, and the reader walking every region that way, a golden's first section found, `hbm_bytes` lands on the right tuple, a call is the median of its executions, a capture of two datasets joins onto their two records, a case captured alone is refused under another case's name and parts merge into one file only when they cover the record, or that mode's calls of it under `--mode`, and a missing `--peak-bw` exits non-zero, a gap in the sampling after the case's last range accepted and one inside it refused, and a ten-row record draws the six panel directories and `index.html`, two datasets as two series in every pooled panel, a kind one dataset makes in that dataset's run-wide colour, an hbm file a row per execution draws the same per-call points as its fold, and a fold takes rows of both shapes each by its own. And the split a record is published by: a run of one dataset is its file byte for byte, two datasets are two files under one heading, a query is spelled as the benchmark macro spells it, and a run missing a declared case, timing an undeclared one, captured under Nsight or naming no device or two is refused naming why, with nothing written; each file lies under its device's slug, the harness's rule. And the fetch: a device's record from its own key into its own directory, the H200's pin, which names no device, as it was, and a name that is not a pin path refused before any fetch. A capture is refused unless every GPU it lists is the card the record's heading names, a record naming none being the H200's, and refused naming the table when it has no GPU table or lists no GPU. And the load breakdown: a load's copies split from its decompression and decode, pinned from pageable, with the reader thread's work counted as the load's; work after the load is not its own, a read or kernel reaching past it is clipped to it, and a launch where two loads are open is neither's, printed as ambiguous; the median over executions and the sum of a batched load's calls; OS reads preferred over kvikio's ranges; a load with no copies at zero rates; and a capture without kernel names, without a load, or with copies but no memory-kind enum refused, with nothing written; a copy's kind read off the export's own enum. And the device list's reader for scripts: the shared fixture reads to the values Rust reads, a note keeps its commas, the rows Rust refuses are refused, and the CLI prints the `ok` and `wrong` rows in file order, narrowed by `--dataset` and `--mode`, and exits 1 naming a refused line; the split reading a device list's `ok` and `wrong` rows as its cases. And a run's segments joined, every segment after one `--record` or each after its own: a case's rows from its highest attempt, by the number in its name and never by argument order, a joined record counting as attempt 0; the pool's free memory spanned as `<min>-<max>`; another device, a capture, an allocator that differs otherwise and two segments of one attempt refused, with nothing written. And the publish from dev, over a fake aws: each file under its sha256, read back, pinned in name order with the other pins kept and moved home; a republished name replaces its own line; a missing file or a bad name refused before any upload; a read-back that differs not pinned. And the query-batching page's generator, over a fixture tree: a batch is its fused section's own queries, and today's prefix of the device list's counted queries, the TPC-H micro queries out, is what it is held to. A section that is not that prefix (reordered, a query gained or lost, a marker either way) is named stale under its table, never refused. A missing fused section, and a batched query with no timed unfused section or no profile row, are refused, naming them. A batch the list does not exceed reads `same set as all`, and an out-of-memory batch its cap. A shared load's folded scans count a query's own repeats apart. A failed DuckDB or tp4_sized query is counted out as `(n/N)` and named, and a DuckDB time not warm named. DuckDB's `SELECT 1` floor is named, a file without it refused, and a ratio of its own takes it off each query. A failed profile's reason is the row when its loads file is missing, and a line under the table when both exist. The largest prefill names every section that ties. Its DuckDB reader reads the shared fixture as Rust does. The provenance lists every distinct value, and names each section whose cache was not on, was not prefilled, or fell back. `--check` fails a stale page. No project code, no device | [test_calls.py](../scripts/calibration/tests/test_calls.py), [test_hbm.py](../scripts/calibration/tests/test_hbm.py), [test_loads.py](../scripts/calibration/tests/test_loads.py), [test_plot.py](../scripts/calibration/tests/test_plot.py), [test_split.py](../scripts/calibration/tests/test_split.py), [test_fetch.py](../scripts/calibration/tests/test_fetch.py), [test_device_cases.py](../scripts/calibration/tests/test_device_cases.py), [test_join_records.py](../scripts/calibration/tests/test_join_records.py), [test_publish_record.py](../scripts/calibration/tests/test_publish_record.py), [test_batching_report.py](../scripts/calibration/tests/test_batching_report.py) | cost-report | 121 |
| Nebius scripts (Python) | `nebius.sh` against fake nebius, ssh and ssh-keyscan: every exit code, and the host-key rule; `--instance-id`/`--profile`, the per-instance alias, and the soft stop on a CPU VM; `install-watch`'s one profile, its embedded key and the install file; the idle watcher's decision over a process snapshot and a clock, for both kinds, a fused benchmark run among the test processes, and the hourly checker's verdict over the watcher's last log line; `watch.sh` and the hourly workflow's three legs, each skipped with a notice rather than failed; that only an actor on `GPU_ALLOWED_ACTORS` reaches the host; `ci-ship.sh` with real rsync into a local home — `lib/`'s symlinks and exec bits, a cache mirroring each run, retries that end red; and `nebius-test.sh`'s list over stub binaries — what runs, with what, and what is red. No VM, no secret | [test_nebius.py](../scripts/nebius/tests/test_nebius.py), [test_idle_watch.py](../scripts/nebius/tests/test_idle_watch.py), [test_install_watch.py](../scripts/nebius/tests/test_install_watch.py), [test_watch.py](../scripts/nebius/tests/test_watch.py), [test_ci_ship.py](../scripts/nebius/tests/test_ci_ship.py), [test_nebius_instances.py](../scripts/nebius/tests/test_nebius_instances.py), [test_idle_watch_kinds.py](../scripts/nebius/tests/test_idle_watch_kinds.py), [test_watch_workflow.py](../scripts/nebius/tests/test_watch_workflow.py) | cost-report | 171 |
| Images (Python) | the images' committed inputs and the scripts that build and run them: `docker/pins.env` against the specs, the specs' hashes and the venv lock; `image.sh`'s build line, its tags, its `peacockdb.inputs` label holding every `docker/` file its Dockerfile reads and only the pins it declares, a pin left out only where another version's stage declares it, the same in every locale, and the pushes it refuses; the cmake wrapper's redirects to the prefetched rapids-cmake and flatbuffers; the DuckDB extension directory; the static-cudart guard; `images.yml` started by a dispatch alone, its four steps unconditional and of a fixed shape, building and pushing through `image.sh` with the job's own token and nothing else, its disk gate run over fakes, and the reader it is parsed with refusing what it does not read. The embeddings cache over its pinned origins, with a fake curl: CI, GITHUB_ACTIONS and PEACOCK_NO_FETCH refusing before any call; nothing fetched that a full cache holds, and a missing query set fetched from its origin, resumed; through a symlink, the cache beside the link; an unzip killed midway leaving no GloVe text a re-run accepts; and the committed pin naming each artifact once. The calibration bucket's endpoint and region from `AWS_ENDPOINT_URL` and `AWS_REGION`, keeping the old storage when no endpoint is named and refusing an endpoint without a region. `bootstrap-vm.sh` over a fake docker and sudo: what it refuses before its first install, root among it; a `--ref` the clone is not on refused, naming both branches; one `apt-get update` before any install; the machine-wide data directory, `/var/lib/peacock` unless `PEACOCK_DATA_DIR` names another, before any download; and a whole run: no marker, `DISABLE_AUTOUPDATER=1` in `~/.bashrc` and no `CLAUDE_CONFIG_DIR`, no other script named; an image of other inputs or without the label refused before any run in it; the registry fetched with the clone read-only; sf1 chosen by generator inputs and generated once, offline; one cuDF checkout a commit, sparse, reused, the clone's own submodule never read, and a commit it cannot fetch refused; what it writes in the data directory group-writable; two runs at once generating once and fetching in turn. `provision-vm.sh` over a fake ssh and VM: the laptop's files and their modes, the developer's user, groups and keys, bootstrap on every run, the GPU key `--gpu-key` names copied to the VM and a missing key or `.pub` refused with no key made, the secrets on ssh's stdin alone, the PAT in no argv and not in the VM's env file, another key in `peacock.env` refused by name, an incomplete env file, a missing git identity and a token it cannot use refused, the env file kept with no CR and a final newline, `--env` naming the laptop's own file too, and its comments skipped, a `--ref` the VM's clone is not on refused, a rerun skipping what is done, without a terminal Claude's login named as a command that works rather than run, `--new-pat` on every VM it reaches, help printed without sed and a bash older than 4.4 refused. `fetch-sf40.sh` over a fake aws and buckets: the endpoint and region from the env file, the keys in no argv and in no environment but aws's, from a file or stdin, one missing refused; a second run fetching nothing; a short file refetched, a short download refused and not kept; a parquet the manifest does not name, a manifest naming a path, an unknown dataset refused; the aggregate in C order against its pin, a wrong one refused, an unpinned one printed; the pinned aws CLI installed when there is none, a zip of another sha refused. `install-duckdb.sh` over a fake curl: the pinned CLI installed, a zip of another sha or version refused with nothing installed, a failed download named, the pinned version skipped, another replaced, sudo where the prefix is not writable. `run-gpu.sh` over a fake `nebius.sh` and VM: every call through `nebius.sh`, with `--instance-id` and `--profile` as given, none without `--instance-id`, and never `start` or `stop`; its exit 64 as 2, and 69 and 75 passed on; a stale build or run image refused naming the file, `run` too, reading the label without a pull, and a VM without the run image refused as `prepare` first, an old driver as 3 naming both versions, admin-only counters as 4 after `update-initramfs`; the storage keys on stdin alone; the cuDF prefix copied once per image; the install shipped by `--dir`, the staged one by default, a missing one refused; the committed testdata shipped, the datasets linked at their absolute paths and mounted at their own; the run's hostname, filter, resume, case timeout, answer switch and the pinned cache's and scan reader's switches passed on when set, a run in progress refused, its exit as the run's; results, record, the device's answers and log pulled home; a binary staged in `rust-benchmarks/` run by name as the run image's entrypoint with run-tests.sh's environment and no mode word, a filter or `--exact`, judged as run-tests.sh judges (a failed exit, a segfault, no test run), an unstaged name refused on the VM and `--exact` beside a filter before any call. `check-build-image.sh` over the same fakes: the build image pulled, then its label read, on the VM; one of other inputs or without a label refused; none without `--instance-id`. `stage-install.sh` over a fake cargo: four gpu binaries and two release benchmarks, the fused prototype's with `gpu,test-support`, staged from empty and stripped, a failed build named, another image's cuDF refused. `create_nsys_profile.sh --ssh`: every call and rsync through the given command, the two switches exported on the host, and the load breakdown written when the call breakdown refuses the capture; `--cases` a device list's `ok` and `wrong` rows as both passes' cases, the trace pass by name and narrowed by a filter, a missing list refused, and `--device` the records from the device's directory; `--mode` one mode of a two-mode list for both passes, with the merge held to that mode, refused without `--cases`, with `--fused`, or naming a mode the list does not know or no row carries. `--fused`: a capture per batch by `--exact`, in batch order, a batch's derived files beside its record, each batch joined against its own record with the captured run's rows elsewhere, the merge over the batches that joined, a failed capture or join listed in `failed.tsv` while the next batch runs, and a filter, `--cases` or `--device` beside it refused before any call. `bench-each.sh` over a fake `run-gpu.sh` and `nebius.sh`: one process per query in file order, at its mode's lines only and within a device list's `ok` and `wrong` rows; resume, the case timeout and the answer switch on every run, `--dir` to `run-gpu.sh` alone; a failed section kept on a rerun; each record set aside by attempt, the VM's removed; a record a killed run left set aside first; a killed case measured again and exit 1 until it is; a host it cannot reach stopping the run with that exit; bad input, and a run without `--instance-id`, refused before any call. `image-ref.sh` over a fake docker: the local image of the tree's inputs tag before the published one, the published one as its digest, and none, a label of other inputs (each named) or none, and the run image of a build-only version refused. `build-test.sh` over a fake docker and cargo: outside the image one `docker run` as the caller, the tree and a worktree's git dir at their own paths, the target dir per version and inside the tree, the forwarded variables and `--pass-env` names by name and never by value, `RUNNER_TEMP` mounted when set, a stale image refused before any run, the removed flags each naming what replaced it; inside, `cpp/build` stamped with its version and one of another version or a host configure refused, the cpu build's C++ then every derived target and the CLI, `--rust-only` leaving `cpp/build` alone, `--gpu --build` as `stage-install.sh`, `--gpu --run` refused naming `run-gpu.sh`, every target run after a failure, a target with no test failed unless a filter excuses it, and a command's exit as the script's. With the images built: each image as a uid it does not know with no network, every library the staged binaries need resolving in the run image, and `run-tests.sh` over fake binaries | [test_image.py](../scripts/env/tests/test_image.py), [test_image_ref.py](../scripts/env/tests/test_image_ref.py), [test_build_test.py](../scripts/env/tests/test_build_test.py), [test_images_workflow.py](../scripts/env/tests/test_images_workflow.py), [test_embeddings.py](../scripts/env/tests/test_embeddings.py), [test_s3.py](../scripts/env/tests/test_s3.py), [test_bootstrap.py](../scripts/env/tests/test_bootstrap.py), [test_provision.py](../scripts/env/tests/test_provision.py), [test_fetch_sf40.py](../scripts/env/tests/test_fetch_sf40.py), [test_install_duckdb.py](../scripts/env/tests/test_install_duckdb.py), [test_run_gpu.py](../scripts/env/tests/test_run_gpu.py), [test_check_build_image.py](../scripts/env/tests/test_check_build_image.py), [test_stage_install.py](../scripts/env/tests/test_stage_install.py), [test_run_tests.py](../scripts/env/tests/test_run_tests.py), [test_bench_each.py](../scripts/env/tests/test_bench_each.py) | manual | 287 |
| DuckDB benchmark script (Python) | `duckdb_minimal.sh` against the real DuckDB CLI 1.5.4 over the sf1 parquet: a device list's `ok` and `wrong` rows timed in file order, the `skipped` row not, saying once that nothing is verified; the answers one table per query, in DuckDB's row order, under the oracle's integer division; the results file's preamble (version, threads as DuckDB reports them, host, a one-word CPU, the `SELECT 1` floor in whole ms) and its columns, in the shared fixture's shape; tpcds's tables every parquet file; `--answers` without `--cases` or with `--skip-verify`, tpcds without a query set and a missing list refused with nothing written; `--build-only` checking the row counts and timing nothing; the default run's minimal set unchanged. Needs `duckdb` on PATH and the generated sf1 | [test_duckdb_minimal.py](../benchmarks/tests/test_duckdb_minimal.py) | manual | 9 |
| C++ CPU/FFI unit | decimal binop typing, AST routability, lifecycle, the row-range clamp rule over the same shared case table the Rust clamp reads, the two refusals of the test-only upload symbol, `peacock_handle_duplicate` refused with no plan loaded, a session indexing a node two parents hold once and several roots in order and refusing a plan that sets both `root` and `roots`, and the timing switch — its mode round-trips, the ABI refuses a mode it does not name and a null region buffer with a capacity, and a second harness NVTX push replaces the first rather than nesting; `TableResult::of`'s two refusals, Final and Single modes refused by name, and a `ColumnRef` resolved by index and name, and `MERGE_M2`'s count type — `INT32` before cuDF 25.06, `INT64` from it, and the type this build's headers pick. The pinned host file cache over a counting `malloc` allocator: a hit reuses the buffer; eviction is least recently used, never of a leased entry, also while another thread reads one; a second reader waits for a fill, and a failed fill wakes its waiters; concurrent fills stay under the capacity; a changed file is reloaded while a reader keeps the old bytes; short and interrupted reads are looped, a short read at EOF is a changed file; a scan that does not fit, a refused allocation and a capacity of 0 read every path, the fallback said once; the default capacity and the cgroup parse; the device classification; the prefill: idempotent, refusing before pinning anything with leases counted against its limit, reloading, evicting, and its `Io` and `Off` outcomes; and the process's cache: `PEACOCK_PINNED_HOST_CACHE_BYTES` a decimal count or refused by name, the environment over the session over the default, nothing pinned without a device or on an integrated one, and the line its first use says. The scan's reader switch, `PEACOCK_SCAN_READER`: unset or `chunked` is the chunked reader, `read_parquet` the whole-file call, any other value refused by name, and the variable read once per process; no GPU needed | [DecimalScale.BinopOutputType](../cpp/tests/cpu/test_executor.cpp), [AstRouting.CudfAstCanEvaluate](../cpp/tests/cpu/test_executor.cpp), [PinnedHostCache.ConcurrentReadAndEviction](../cpp/tests/cpu/test_pinned_host_cache.cpp), [ScanReader.TheSwitchIsReadOnce](../cpp/tests/cpu/test_scan_reader_switch.cpp) | dataset-matrix (`ctest -L cpu`) + shad-gpu | 67 |
| cuDF GPU smoke (C++) | the GPU is alive; the RMM pool reserves the budget the binary declared; `peacock_device_info` names the device, gives the runtime's version, and refuses a null output; the pinned cache's allocator returns host-pinned memory, and a refused `cudaHostAlloc` leaves no CUDA error for cuDF's next read. The two hardcoded-comet partition-id cases are gone: they were a third copy of the lane rule, and the live gate proves the kernel against the production rule instead | [CudfGpu.SequenceSum](../cpp/tests/gpu/test_cudf.cpp), [RmmPool.ReservesTheDeclaredBudget](../cpp/tests/gpu/test_cudf.cpp), [PeacockGpu.DeviceInfoDescribesTheDevice](../cpp/tests/gpu/test_cudf.cpp), [PinnedHostCache.RefusedAllocationClearsTheCudaError](../cpp/tests/gpu/test_cudf.cpp) | shad-gpu | 5 |
| Plan-executor (C++) | hand-built plan IR through the C++ executor, node by node — a join through the session's build, probe and finish, laid out as the wire lays it, its two sides and the `CudfJoin` leaf under one structural union — plus the per-call entry points at their contract edges (row-group override, export range, slice), a scan of no column answering its row count from the footer — whole, one row group, under a limit — and a project of no expression refused, the sqrt arm on both evaluators, a value-form CASE with and without ELSE — the first match winning, a NULL comparand or WHEN never matching — `date_part` answering in the type the wire names for three fields, a merge that emits state rather than a value, the literal arm — a typed null on the AST path, the decimal literal's scaled double, a decimal literal under a cast to `Float64` or `Int64` kept on the AST, every wire type walked through the dispatch, the LIKE guard, `IS [NOT] DISTINCT FROM` against a typed null on each evaluator — and the timed regions: `Off` records none, every entry point opens one per output partition, a second call of one seq counts up, a slice and an export are charged to the node that produced the handle, a handle from before timing was on and an adopted one each refused a charge, an export of no rows opens one too, and collecting drains; and the NVTX switch on its own: ranges without timing record no region. `peacock_handle_duplicate` is four: it allocates nothing and its handle reads the rows after the original is released, either handle is released on its own, an export of the duplicate is charged to the original's producer and the duplicate opens no region, and an unknown handle is refused by number and ends the session. The handle's own rules are ten of these: a slice and a selection share their columns' owners, and five shapes are refused — a table of no columns, a name count that does not match, an ordinal past the columns, a selection of no ordinals, and a column appended at the wrong row count, a slice's string bytes are its own rows' and not its parent's, a scatter's partitions share one table and outlive their siblings and concatenate back to the input, an empty lane is a zero-row slice of no string bytes, two input handles are refused rather than concatenated, a scatter's peak and its requested total are those of partitioning the same input alone, measured with the RMM statistics adaptor over a fixed-width table and a string one; the total is the bound that discriminates on both shapes, since a string column's gather in cuDF already costs more than the copies did and so set the peak all along — and `slice_handle` still owns its rows; and the operator exit paths, nine of these: a bare `ColumnRef` evaluates to a view of the input's own column and allocates nothing, a non-AST predicate allocates no more than the comparison and the filter alone, a fused filter projection no more than the same filter without one, a project of column references nothing at all, a window no more than its own result column, and an aggregate no more than cuDF's groupby — each bound measured on the same input, since a hand formula would bake a cuDF cost into a constant; plus a filter whose projection repeats an ordinal, which answers both columns over one owner, and two that bound a release rather than an allocation, since no allocation bound can see an operator retaining what it should drop: a filter narrowed to one column gives back the dropped column's own bytes, and a project holds its shared column exactly once and frees the rest; and the grouping id's value and width from one key to sixty-four, each width boundary from both sides, and its refusal past them; and an aggregate's init handed no input answering over its declared input's zero-row table, where a merge handed none still refuses; the aggregate's request builder per function and phase, the Welford state with no NULL over an all-NULL group or merge, and a keyless Welford through the builder, with rows and over none, and a `ColumnRef` refused past the width, with no name, or under another column's name; and a date or timestamp ± an interval literal, each expected value DataFusion's: the month-end clamp in a leap and a common year, both ways, a mixed interval at every timestamp unit in the months-days-nanoseconds order, a sub-day part truncated on a date and floored on a timestamp, the interval on the left, a NULL interval, zero rows keeping the type, and any other interval use refused naming #249; a scan from the pinned host cache equal to the path's, column by column, and a prefilled file scanned three times with no fill, fallback or reload, a cached scan that reads the cached bytes and not the file overwritten under it; and the scan's reader: the one-chunk chunked reader equal to `read_parquet` column by column, over every row group and over one, and several chunks from a small limit concatenating to the same table | [PlanExecutor.HashJoinNationRegion](../cpp/tests/gpu/test_plan_executor.cpp), [ExitCopies.AProjectOfColumnRefsAllocatesNothing](../cpp/tests/gpu/test_plan_executor.cpp), [test_grouping_id.cpp](../cpp/tests/gpu/test_grouping_id.cpp), [test_aggregate_no_input.cpp](../cpp/tests/gpu/test_aggregate_no_input.cpp), [test_column_refs.cpp](../cpp/tests/gpu/test_column_refs.cpp), [test_aggregate_builder.cpp](../cpp/tests/gpu/test_aggregate_builder.cpp), [test_interval_arithmetic.cpp](../cpp/tests/gpu/test_interval_arithmetic.cpp), [test_pinned_scan.cpp](../cpp/tests/gpu/test_pinned_scan.cpp), [test_scan_reader.cpp](../cpp/tests/gpu/test_scan_reader.cpp), [test_handle_duplicate.cpp](../cpp/tests/gpu/test_handle_duplicate.cpp) | shad-gpu | 136 |
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
  Two scripts write the bucket: `build-test-shadgpu.sh --pull-benchmarks` through shad-gpu, and
  `scripts/calibration/publish_record.sh` from dev, with the keys from `peacock.env`.
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
| `duckdb-result.txt` | `testdata/duckdb_result.py`<br>(DuckDB 1.5.4, `threads=1`); timestamps rendered as arrow-rs prints them, rows sorted or in DuckDB's order where the registry's `duckdb_oracle` says `no_sort`, and a **fingerprint** over the cap. At sf40, `--dataset <ds> --sf 40 --answers <db>` renders the tables `duckdb_minimal.sh --answers` stored, one per query, by the same rules; a query with no table is `failed: no answer recorded` | sf1 parquet, the query text in `<bench>-queries/`; at sf40 the answers database | `test_cpu_corpus`'s `duckdb_<ds>_<q>` cases, per the line's `duckdb_oracle`; `the_registry_stored_column_matches_the_goldens_in_both_directions` |
| `gpu-result.txt` | the corpus **device** tier under `PCK_WRITE_GPU_RESULT=1`, **before** it asserts, so an answer the cpu rejects is still recorded; one section per (query, mode), a fingerprint over the cap; `--pull-results` brings it home. At sf40 the benchmark under the same variable, its chosen run's answer, which `run-gpu.sh pull` brings home. A record, never an authority | sf1 parquet, the device's cuDF — the committed file holds cuDF 25.02's answers and says so in a `cudf=` first line, read from `$CUDF_ROOT`'s `version_config.hpp` at build time. It was recorded on nebius-gpu's L40S, not shad-gpu, which was down; the stamp carries the cuDF and not the card, so a regenerator on a third card has only this sentence to tell it the baseline moved. `PCK_WRITE_GPU_RESULT=<v>` writes `gpu-result-<v>.txt` beside it, gitignored | the `duckdb_gpu_<ds>_<q>_<mode>` cases , `every_enabled_device_cell_has_its_gpu_result_section_and_no_other` and `the_registry_stored_column_matches_the_goldens_in_both_directions`; at sf40, `every_sf40_device_answer_meets_duckdb` |
| `stats/<bench>.sf<N>.json` | `gen_stats.py --bench B --sf N`<br>(DuckDB: exact `count(DISTINCT)`, strings' mean `strlen`) | the parquet's flat columns. Each table records its rows and a sha256 of those columns' footer statistics (null count, min, max per row group); the reader refuses a table whose statistics moved. Data can change with none of them moving, so only a recount catches every stale NDV. Vector columns are left out, so both `--embeddings` modes give one file | `test_stats_sidecar` recounts it from the data in dataset-matrix; `test_stats_embeddings` from external embeddings, by hand |
| `<bench>/tp1-single.cardinality.txt`, in `scripts/exec_model/testdata/goldens/` | `test_cardinality_corpus.py` with `UPDATE_CANONICAL=1` | the plan golden, `tp1-single-mini.cpu.txt`, the stats sidecar and the sf1 footers, `optimizer/cardinality.py`. One line per join: its estimate, its rows in the engine's run, the q-error | `test_cardinality_corpus` in dataset-matrix, the whole file |
| `<bench>/<mode>.cpu.txt`, `<mode>.optimizer.txt`, `<mode>.plandiff.txt`, `<mode>.costs.txt` and `<mode>.optimizer.md`, in `scripts/exec_model/testdata/goldens/` | `scripts/exec_model/run.py`; with `--query` it rewrites only its own sections | the plan golden, the sf1 tables and sidecar, the whole optimizer, `cost_model.conf`, and pandas' byte counts. A `== <query>` section each: the optimized run in the `.cpu.txt` format plus each clustered join's `mj=`, what each rule did, the plan's diff, and both runs priced by the engine's cost function with the rules counted. The page has a `## <query>` section each, from the same report as `.optimizer.txt`: each cluster's successive orders and the dynamic filters. `.costs.txt` is not a function of the `.cpu.txt` beside it: it prices the planned run, the probe plans and the stopped runs too | nothing: generation only |
| `cost_report.html`, in `scripts/exec_model/testdata/goldens/` | `run.py` at the end of every run, or `scripts/exec_model/cost_report.py` alone; committed with relative links | every `<bench>/<mode>.costs.txt` and `.optimizer.txt` beside it, the engine's `-mini.cost.txt` and the `<q>.duckdb_cost.txt` | the cost-report job renders it again with `--sha`, its links at the commit, and on master publishes it at `join-optimizer/` |
| `benchmark-report.html` | `the_benchmark_report_matches_the_trees` in `test_corpus_goldens` with `UPDATE_CANONICAL=1`, published by rename | every device tree under `testdata/benchmark-results/` and its `cases.csv`, and its fused files; the registry's row order, which is the main list's | that test, byte for byte |
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
          │           at sf40: --sf 40 --answers <db>, the tables duckdb_minimal.sh stored on
          │           the 128-core VM ──► <ds>.sf40/duckdb-result.txt
          │
          │   test_cpu_corpus, rust-only (CI), per the line's duckdb_oracle
          │   (exact | approx | rel | date | gpu: fingerprint | fingerprint | none):
          │     duckdb_<ds>_<q>             <tier>.result.txt ↔ duckdb-result.txt
          │     duckdb_gpu_<ds>_<q>_<mode>  gpu-result.txt   ↔ duckdb-result.txt
          │     coverage   every enabled device cell has its gpu-result section, and no other;
          │                and the committed file's cudf= line is 25.02's
          │     every_sf40_device_answer_meets_duckdb   at sf40, per the H100's cases.csv row:
          │                <ds>.sf40/gpu-result.txt ↔ <ds>.sf40/duckdb-result.txt
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
  │     ├──► goldens/<dataset>.sf<sf>/gpu-result.txt    the chosen run's answer, under PCK_WRITE_GPU_RESULT=1
  │     └──► calibration/records.tsv                one row per cuDF call × execution
  │           └── --pull-benchmarks brings both home  (the record gitignored), and
  │               split_record.py: a whole, plain, passing run per dataset, or nothing
  │                 ├──► s3://calibration/<device>/<dataset>.sf<sf>/<sha256>.tsv
  │                 └──► calibration/records.sha256      that sha256, committed with the tree
  │                       └── fetch_record.sh <device>/<dataset>.sf<sf>
  │                             └──► calibration/<device>/<dataset>.sf<sf>/records.tsv   (gitignored)
  │
  ├── bench-each.sh over run-gpu.sh (the H100), one process per query: the same files, and
  │     each process's record set aside as a segment; join_records.py joins the segments (after
  │     the device's published records, so a run of one mode keeps the other's rows),
  │     split_record.py --cases <the device's cases.csv> cuts the result, and publish_record.sh
  │     uploads and pins it from dev, as above. PCK_WRITE_GPU_RESULT=1 on every run:
  │     └──► goldens/<dataset>.sf40/gpu-result.txt   the H100's answers, held to DuckDB's below
  │
  └── create_nsys_profile.sh [--host …]             (the same binary, under Nsight)
        ├── --trace     nvtx and cuda only; PEACOCK_BENCHMARK_CAPTURE=trace
        │     └──► calibration/capture.sqlite          (not committed)
        │           ├── nsys_calls.py × goldens/<dataset>.sf1
        │           │     └──► calibration/calls.tsv   what one ABI call splits into, per case
        │           └── nsys_loads.py
        │                 └──► calibration/loads.tsv   where each parquet load's time goes (committed per device only)
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

create_nsys_profile.sh --cases <ds>.csv --mode <mode> --device <device>   (by hand, a pass per mode)
  └──► calls.tsv, loads.tsv, hbm.tsv: the mode's rows into calibration/<device>/<dataset>.sf<sf>/   (committed)

prototype_fused_benchmarks over run-gpu.sh (the H100), one process per batch (Fused batches, below)
  ├──► benchmark-results/<device>/fused/<dataset>.sf40/tp1_single.fused.txt
  └──► calibration/fused/<dataset>.sf40/<batch>.records.tsv   one run's record per batch (gitignored)
        └── create_nsys_profile.sh --fused <dataset>.sf40   one capture per batch and pass, by --exact
              ├── --trace   ──► calibration/fused/<dataset>.sf40/capture-<batch>.sqlite   (not committed)
              │               └── nsys_loads.py, nsys_calls.py (no goldens)
              │                     └──► <batch>.loads.tsv, <batch>.calls.tsv
              ├── --metrics: each batch's capture × its <batch>.records.tsv, on the host; the
              │             captured run's own rows to records-metrics.tsv there
              │               └── nsys_hbm.py --parts over the batches that joined ──► hbm.tsv
              └──► failed.tsv   batch, pass, reason: a batch whose capture or join failed
                    └── copied, but for the records and captures, to
                        calibration/h100-80gb-hbm3/fused/<dataset>.sf40/   (committed)

tpch.sf40, tpcds.sf40 on nebius-cpu128, as DuckDB databases (duckdb_minimal.sh --build-only)
  └── duckdb_minimal.sh --cases <the H100's cases.csv> --answers <db> --results <tsv>
        ├──► benchmark-results/duckdb-cpu128/<dataset>.sf40/duckdb.tsv   warm 2nd-min of six, 128 threads
        └──► answers-<dataset>.duckdb (threads=1), copied to dev
              └── duckdb_result.py --sf 40 --answers <db>
                    └──► goldens/<dataset>.sf40/duckdb-result.txt
                          └── every_sf40_device_answer_meets_duckdb ↔ the H100's gpu-result.txt
```

`<device>` is the card's slug, `h100-80gb-hbm3` for `NVIDIA H100 80GB HBM3`. The H200's two
records predate it: they are keyed, pinned and fetched as `<dataset>.sf<sf>`, one level up.
`create_nsys_profile.sh` reads the pinned records at that level,
`calibration/<dataset>.sf<sf>/records.tsv`, one per dataset the case list names; `--device <slug>`
reads them a level down, under the device's slug.

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
  node alone, three cases fold into one row set. Each case is checked against its dataset's sf1
  plan golden, so a case the planner orders differently at sf40 cannot be read. The H100's
  tpcds `calls.tsv` leaves out 14 such queries ("sf40 baseline").
- **`loads.tsv` splits each parquet load.** `calls.tsv` gives a cuDF call one `device_us`, so a
  load's copy cannot be told from its decode there. `nsys_loads.py` writes one row per load node
  per case: every partition region of a `CudfScan` call. Each number is computed per execution,
  then the median over executions is taken, column by column. A device operation is the load's
  when its launching API call starts inside one of the load's windows, on any thread. The
  parquet reader launches from threads of its own, which `calls.tsv` leaves as `(off-thread)`.
  A launch where two loads are open is credited to neither; the run prints that total as
  `ambiguous`. A capture without kernel names (`demangledName`) or without a `CudfScan` region is
  refused, and nothing is written. So is one with copies but no `ENUM_CUDA_MEM_KIND` table.

  | column | meaning |
  |---|---|
  | `dataset sf query mode` | the case, as in `calls.tsv` |
  | `recipe_seq` | the load's fb seq: the `GpuLoadParquet` node's `CudfScan` recipe |
  | `executions` | executions in the capture |
  | `calls` | call indices per execution: the batches the load was driven for |
  | `load_us` | the sum of the load's partition region spans in one execution. The same quantity as the tree's `GpuLoadParquet` `time_us` |
  | `host_read_source` | `osrt` if the capture has `OSRT_API` rows; else `kvikio` if it has a `libkvikio` domain; else `none` |
  | `host_read_us` | wall time inside the windows, a union over threads, in `pread*` and `read` OS calls, or else in `libkvikio` ranges. kvikio's ranges contain its bounce copies, which the `h2d_*` columns also count |
  | `h2d_count h2d_bytes h2d_us` | the HtoD copies launched in the windows, from any thread. `h2d_us` sums their durations |
  | `h2d_gbps` | `h2d_bytes / (h2d_us·1000)`: the per-copy rate, comparable with a probe's pinned and pageable rates. `0` with no copies |
  | `h2d_pinned_bytes h2d_pinned_gbps h2d_pageable_bytes h2d_pageable_gbps` | the same, split by `srcKind`, read through the export's `ENUM_CUDA_MEM_KIND`: nsys numbers pinned 1 and pageable 0, not CUPTI's 2 and 1. A copy of another kind counts in the totals only |
  | `h2d_max_bytes` | the largest single copy. It says whether a copy was too small to reach the link rate |
  | `decompress_us decode_us other_device_us` | kernel durations matching `DECOMPRESS_RE`, then `DECODE_RE`; then everything else: other kernels, memsets, copies that are not HtoD |
  | `off_thread_device_us` | the part of the device time above launched from a thread other than the load's own. It is included in the columns above |
  | `pinned_alloc_count pinned_alloc_us` | the `cudaHostAlloc`-family calls (`PINNED_ALLOC_RE`) that start in the windows, from any thread, and their summed host span |
  | `unattributed_us` | `load_us` less the union of host reads, device operations and pinned allocations, each clipped to the windows |
- **Every derived file has exactly one producer, and the scripts above run it.** A file that
  only a human regenerates goes stale in silence: nothing checks it against the capture it
  claims to describe, and a panel drawn from a stale one looks exactly like a fresh one.
  `plot.py` is the only thing that draws, for the same reason. One exception: the H100's
  tp4-sized rows, whose tpcds passes ran a case at a time on the VM by uncommitted drivers
  ("sf40 baseline", step 7, says how).
- **The derived text is committed; the record and the captures are not.** The
  `.benchmark.txt` tree, `calls.tsv`, `hbm.tsv` and every panel are in git, rewritten in place
  by each collection, so `git diff` shows how the numbers moved. The top-level `loads.tsv` is
  not: it is one host's scratch profile; a device's own `loads.tsv` is (below). The record is 26 MB at sf40 and grows with
  every case, so it lives in S3 and git holds its sha256: `--pull-benchmarks` writes both, and
  the commit that moves the tree moves the pin. The two `.sqlite` exports are
  undiffable binary that only the scripts above read. The counters one grows ~27 MB a second
  of run with the whole gh100 set at 20 kHz, ~100 GB over the corpus, so it is taken a case at a
  time and never leaves the host: each case is joined there and deleted before the next, and
  only `hbm.tsv` comes home.
  `testdata/.gitignore` is deny-by-default over `calibration/`, re-including the pin and the
  committed derived files only, so `loads.tsv` stays out without a line of its own. A device's
  directory, `calibration/<device>/<dataset>.sf<sf>/`, re-includes its `calls.tsv`, `loads.tsv`
  and `hbm.tsv`, and keeps the fetched `records.tsv` out ("sf40 baseline").
  `calls.tsv` and `hbm.tsv` in git cover three cases of an earlier run — q6 at `tp1-single` and
  `tp4-sized`, q19 at `tp1-single` — whose record is in git's history.
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
  is refused too: its rows are filed under the device that measured them. shad-gpu publishes
  through `build-test-shadgpu.sh`. A run under `bench-each.sh` is published from dev, joined,
  split and uploaded ("The H100's device run"). `build-test.sh` keeps what it pulls, and
  nebius-gpu runs no benchmark.

## CI structure (`.github/workflows/pipeline.yml`)

`pipeline.yml` runs on pushes to master and on every PR, but not on documentation —
`**.md` and `llm-wiki/**`, less `llm-wiki/reports/query-batching.md`, which a step checks
against its inputs. It takes two layers. The triggers'
`paths:` lists skip a wholly-documentation diff. Each is `'**'`, the two docs patterns negated,
then the page re-included; GitHub documents `!` for `paths:`, not for `paths-ignore`. The
**changes** job skips a documentation-only push to a PR that carries code, which the trigger
cannot see because it judges the whole PR diff. The job's grep and the lists must name the same files. Both fail
open. A `workflow_dispatch` always runs in full.

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
  `test_golden_format` and `test_corpus_goldens`, the exec-model python, which reads the
  generated sf1 tables and builds DPhyp for itself, and `batching_report.py --check`, which
  fails when `llm-wiki/reports/query-batching.md` is not what its committed inputs make.
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

**A third feature, `test-support`, is passed by one script only.** It gates `src/prototypes/` and
`src/test_support/`, the shared harness — the testdata root, the modes, the golden-text reader, the link-time
registry, the result comparators, the corpus goldens and the corpus case itself — which the
crate's unit tests reach as `crate::test_support::…` and the `tests/*.rs` binaries as
`peacockdb_core::test_support::…`. The crate's dev-dependency on itself turns it on, so `cargo
test` in every shape sees the module and `cargo build` cannot name it. No CI step passes the
flag; `stage-install.sh` does, to build `prototype_fused_benchmarks` ("Fused batches"). The corpus binaries and `test_corpus_goldens` reach it directly through
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
| rust-only, in the build image | `scripts/build-test.sh --rust-only --build --run` | — | `target-img-<v>` |
| C++ + Rust with the FFI (cpu) | `scripts/build-test.sh --build --run`; `--cudf <v>` picks the image, 25.02 by default | `cpp/build`, and cargo's OUT_DIR | `target-img-<v>` |
| the install a device run ships | `scripts/build-test.sh --gpu --build` (`scripts/env/stage-install.sh`) | `cpp/build` | `target-img-<v>` |
| one command in the build image | `scripts/build-test.sh -- <command>`, e.g. `-- cargo test -p cost-report` | — | `target-img-<v>` |

## Inspecting the tree

Four scripts answer questions about the shape of the tree rather than about a build. They take
no arguments beyond what is shown and need no dataset or device.

| Script | Question | Invocation |
|---|---|---|
| `scripts/case-inventory.sh` | every test case a shape compiles, one per line — `--lib` and every `--test` binary for `rust-only` and `cudf`, `--lib` alone for `gpu`, since the only `--test` target that reads the `gpu` feature, `prototype_fused_benchmarks`, is not a `test_*.rs` file it lists, and the others' `cudf` listing stands | `scripts/build-test.sh -- scripts/case-inventory.sh rust-only\|cudf\|gpu`; `cudf` and `gpu` refuse to run outside the build image |
| `scripts/compare-inventory.sh` | did any case stop compiling into its target | `scripts/compare-inventory.sh <shape> <baseline> <current>` — the baseline is an argument, so it outlives whichever task took it |
| `scripts/visibility-dump.py` | every `pub`/`pub(...)` item with its declaring file; `--items` drops the file and visibility, giving the set a move cannot change | `scripts/visibility-dump.py peacockdb-core/src` |
| `scripts/residue-gate.sh` | does a retired name survive anywhere, tracked or not | `scripts/residue-gate.sh` |

`residue-gate.sh` resolves paths against the repo root and checks that the result is non-empty.
It reported a clean tree from the wrong directory once, because `git` rejected the pathspec and
every section came back empty — a gate that finds nothing and a gate that cannot look are the
same output otherwise.

`build-test.sh` runs itself in the build image of `<v>` (`scripts/env/image-ref.sh build <v>`, or
`$PEACOCK_BUILD_IMAGE`), as the caller's uid, with the tree at its own path. It refuses an image
built from other inputs than the tree, naming them. The cargo registry and ccache live under
`~/.cache/peacock/`, ccache one directory per version; ccache wraps the host compilers only
(ccache + nvcc is unreliable). HOME in the image is `<tree>/.home-img`. Run `--help` for the rest.

Note the C++ side is built *twice* on the FFI path, into two dirs that are both used:
`build-test.sh --build` runs cmake into `cpp/build` for the installable lib and the C++ tests,
while `peacockdb-ffi/build.rs` runs cmake again into cargo's `OUT_DIR` for the copy the Rust
binaries link against.

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
  the cudf side recompiles the DataFusion stack at opt-3). Run a one-off cudf/FFI cargo
  command in the image: `scripts/build-test.sh -- cargo test -p peacockdb-core --lib --features gpu --no-run`.
  The image sets `CUDF_ROOT` and its `CC`/`CXX`, and the target dir is `target-img-<v>`, the one
  `--build` uses, so it shares its warm cache. Both matter: cc-rs emits
  `rerun-if-env-changed` on `CC`/`CXX`, so entering a target dir with a different C compiler
  re-runs every native build script (zstd-sys, bzip2-sys, lzma-sys, psm, blake3) and rebuilds
  the whole DataFusion stack above them.
- **A cudf-shape binary needs `LD_LIBRARY_PATH` to run at all.** `libpeacock_gpu.so` lives in
  the FFI crate's `OUT_DIR` (`target-img-<v>/debug/build/peacockdb-ffi-*/out/lib`) and nothing
  puts it on the loader path, so the binary exits 127 with a loader error on stderr and prints
  nothing on stdout. It is loud, but a tool that reads stdout and ignores the status sees an
  empty list, which reads exactly like a target the `rust-only` cfg compiled out. Prepend that
  dir and the cuDF root's `lib` before listing or running one by hand.
- The FFI crate caches its cmake `cudf_DIR` in OUT_DIR. In the image the root is always
  `/opt/cudf` and the target dir is per version, so nothing needs cleaning.
  `build-test-shadgpu.sh` cleans it **only when the cuDF root changed** (stamp file
  `.peacock-ffi-cudf-root`); `PEACOCK_FFI_CLEAN=1` forces.
- `cpp/build` is the image's, stamped with its cuDF version in `cpp/build/.peacock-cudf`.
  `build-test.sh --build` refuses a `cpp/build` of another version, naming both, and one with
  no stamp, which is a host configure (`build-test-shadgpu.sh` makes one). It never
  reconfigures over either: remove `cpp/build` and `cpp/install` first. `--rust-only` does not
  touch `cpp/build`.
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
  Then it runs the run image's `run-tests.sh <mode>` as container `peacock-<i>`, and follows its
  log. The log is kept in `~/peacock/<i>/logs/<mode>.log`. `PCK_TEST_FILTER`,
  `PEACOCK_BENCH_RESUME`, `PEACOCK_BENCH_CASE_TIMEOUT_S`, `PCK_WRITE_GPU_RESULT`,
  `PEACOCK_PINNED_HOST_CACHE_BYTES` and `PEACOCK_SCAN_READER` pass on when set, and
  `create_nsys_profile.sh` exports the last two on the host. If ssh drops, the container goes on,
  and the next `run` refuses, exit 2, until it ends.
- `run <name> [--exact <test>]` runs a binary staged in the install's `rust-benchmarks/`, today
  `prototype_fused_benchmarks`. The run image is unchanged: the binary is its `--entrypoint`, with
  `run`'s mounts, user and hostname and run-tests.sh's library path and testdata. Its arguments
  are `--nocapture --test-threads=1`, then `--exact <test>` or `PCK_TEST_FILTER`; giving both is
  refused, exit 2. `PEACOCK_BENCH_RESUME` passes on when set. A name with no executable there is
  refused on the VM, exit 2. It is judged as run-tests.sh judges: a failed exit, a segfault or
  `running 0 tests` in the log is exit 1. The log is `~/peacock/<i>/logs/<name>.log`.
- `pull` brings `testdata/benchmark-results/`, `testdata/calibration/` and the device's answers,
  `testdata/goldens/*/gpu-result*.txt`, into this tree. A file edited here since `ship` is kept.
  The runs' logs go to `~/.cache/peacock/run-gpu/<i>/`. `pull`
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
`cpp/install/rust-tests/` and the release `peacock_gpu_benchmarks` and
`prototype_fused_benchmarks` into `cpp/install/rust-benchmarks/`, each stripped of debug info. cargo builds into `target-img-<v>`
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

**The benchmark, one process per query.** `scripts/env/bench-each.sh --instance-id <id>
[--profile <name>] [--dir <i>] --cases <list> --mode <mode> [--within <cases.csv>] --timeout <s>
--records <dir>` runs each case of the main list at `<mode>` as its own `run bench`, so no process
nears the watcher's 4-hour cap and a crash leaves the next case a fresh CUDA context.
`--instance-id` and `--profile` go to every `run-gpu.sh` and `nebius.sh` call, `--dir` to
`run-gpu.sh` alone. `--instance-id` is required, as for `run-gpu.sh`: without it `nebius.sh` means
nebius-gpu, which runs no sf40, so `bench-each.sh` refuses, exit 2, before any call. `--within`
keeps the queries with an `ok` or `wrong` row in a device list. Per case:

- a section already in the device tree on dev, timed or failed, is kept, so a rerun resumes and a
  timed-out case is not run again;
- else `run bench` with `PCK_TEST_FILTER` naming the case, `PEACOCK_BENCH_RESUME=1`,
  `PEACOCK_BENCH_CASE_TIMEOUT_S=<s>` and `PCK_WRITE_GPU_RESULT=1`, then `pull`;
- the pulled `calibration/records.tsv` moves to `<dir>/<ds>-<q>-<mode>.<attempt>.tsv`, one attempt
  past the highest there, and is removed on the VM. A record a killed run left on the VM is pulled
  and set aside the same way first, under its last row's case.

The device tree is the one the first measured case's pull changed; its slug is kept in
`<dir>/device` for a rerun. Exit 0 when every case has its section, 1 when one has none (rerun),
2 on bad input, else the `run-gpu.sh` exit that stopped it. The segments then go home as one
record:

```
scripts/calibration/fetch_record.sh <device>/tpch.sf40; scripts/calibration/fetch_record.sh <device>/tpcds.sf40
python3 scripts/calibration/join_records.py --record testdata/calibration/<device>/*.sf40/records.tsv <dir>/*.tsv --out <joined>
python3 scripts/calibration/split_record.py --record <joined> --cases <the device's cases.csv> --out-dir <split>
scripts/calibration/publish_record.sh --from <split> <device>/tpch.sf40 <device>/tpcds.sf40
```

The published records go first: the split refuses a record that lacks any (query, mode) the list
declares, so a run of one mode of a two-mode list needs the other mode's rows. Both modes must then
share one heading (driver, CUDA, cuDF, host); re-measuring one after any of those changes means
re-measuring both. `join_records.py` takes a case's rows from its highest attempt, a name without
one counting as 0, and refuses segments whose headings differ but for the pool's free memory, which it writes as
`<min>-<max>`. `split_record.py --cases` reads a device list's `ok` and `wrong` rows as the cases.
`publish_record.sh` uploads each file under its sha256, reads it back, rewrites its line in
`records.sha256` and moves it to where `fetch_record.sh` puts it.

Nsight runs on the VM itself, over the same door:
`create_nsys_profile.sh --ssh "$N rsh" --host nebius-h100 --remote-dir /home/dmitry/peacock/<i>
--remote-cudf-root /home/dmitry/peacock/cudf-25.02`. Its HBM defaults are an H200's. The
metric set `gh100` fits the H100 too, but the peak does not: set `PCK_BENCH_HBM_PEAK_BW=3.35e12`,
and `PCK_BENCH_HBM_FREQ=10000` ("sf40 baseline").
That is the H100's 2619 MHz memory clock × 2 × 5120 bits, NVIDIA's 3.35 TB/s. `--cases
<cases.csv>` profiles a device list's `ok` and `wrong` rows instead of the main list, the trace
pass by their names; `--device <slug>` reads the pinned records from
`calibration/<slug>/<dataset>.sf<sf>/`.

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
  Two more variables win over the engine the same way: `PEACOCK_PINNED_HOST_CACHE_BYTES=<bytes>`
  sizes the pinned host file cache (`0` is off), and `PEACOCK_SCAN_READER=read_parquet` swaps the
  scan's one-chunk reader for `read_parquet`. A device tier with the cache off is still green, but
  `peacock_plan_tests`' three cache-on cases skip. A benchmark prefills its cases' files first
  ("The prefill" under Benchmarks).
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

A build image and a run image per cuDF version, built from `docker/` and published on
`ghcr.io/asymptote-tech/`. `<v>` is a version with a committed spec in `docker/conda/`: 25.02,
25.10 or 26.02. 25.10 is for CI only: a build image and no run image. A version is build-only
when `docker/pins.env` has no `DRIVER_MINIMUM_<v>`. Sizes are docker's, in GB of 10^9 bytes.

| image | holds | size (25.02, 25.10, 26.02) |
|---|---|---|
| `peacockdb-build:cudf-<v>` | every build mode: rust-only, the FFI build, `--features gpu`, the staged binaries | 10.0 GB, 10.8 GB, 9.74 GB |
| `peacockdb-run:cudf-<v>` | the runtime of the same cuDF and `run-tests.sh`; nothing to build with | 5.09 GB, none, 6.17 GB |

Each image carries three tags. `:cudf-<v>` is the latest published from master.
`:cudf-<v>-in-<h>`, the inputs tag, names what it was built from: `<h>` is the first 12 hex digits
of the sha256 of its `peacockdb.inputs` lines (`image_inputs_tag` in `scripts/lib/image-inputs.sh`).
A tree finds the image of its own inputs by it. `:cudf-<v>-<commit>` names the commit it was built
from, 12 hex digits; the label `org.opencontainers.image.revision` holds the same commit.
Pulling needs no login: `docker pull ghcr.io/asymptote-tech/peacockdb-run:cudf-25.02`.
No image holds a secret, a host or Claude Code. Both run as any uid: pass
`--user "$(id -u):$(id -g)"`. The default user is `peacock`, uid 1000, never root.

### The build image

- `/opt/cudf` is the cuDF prefix (`CUDF_ROOT`), package for package the version's spec. For 25.02
  and 26.02 that is the nebius-gpu environment the committed device answers came from. Its `bin/`
  is not on PATH, and its `lib/` is not on the loader path: an FFI test binary run in the image
  needs `-e LD_LIBRARY_PATH=/opt/cudf/lib`, or it exits 127 on `libcudf.so`.
- nvcc 12.9.86 in `/opt/nvcc`, gcc 12 (25.02) or 14 (25.10, 26.02) as `CC`/`CXX`, cmake, ninja,
  rust, ccache, patchelf. `scripts/build.sh` needs `--cudf_ROOT /opt/cudf --gcc-version "$GCC_VERSION"`.
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
  `lib/libpeacock_gpu.so`, `bin/peacock_*_tests`, `rust-tests/` and, for `bench` and
  `run prototype_fused_benchmarks`, `rust-benchmarks/`. Read-only.
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
  included. A dirty build gets `:cudf-<v>` and the inputs tag, and the revision `<commit>-dirty`.
- `scripts/env/image.sh push <build|run> <v>` pushes the inputs tag and the commit tag.
  `--moving` pushes `:cudf-<v>` too. It refuses unless HEAD's commit tag, the inputs tag and
  `:cudf-<v>` all exist locally and are one image. So: commit, build, push.
- `image.sh build run <v>` and `push run <v>` refuse a build-only version, exit 2.
- **From a host**, docker must be logged in to ghcr.io with a token holding `write:packages`:
  `gh auth refresh -s write:packages`, then
  `gh auth token | docker login ghcr.io -u <github user> --password-stdin`. A package pushed for
  the first time can be private: check its visibility in its settings and set it public.
- **From GitHub**, `.github/workflows/images.yml`:
  `gh workflow run images.yml -f kind=<build|run|all> -f cudf=<v|all>`. GitHub can dispatch it
  only once master holds it. A dispatch runs the workflow and the commit of the ref it names
  (`--ref`, master by default). Only a dispatch on master moves `:cudf-<v>`; one on another
  branch pushes the inputs and commit tags alone, so the chain's earlier trees keep their image.
  One job per image, pushing with the job's own token. A build-only version gets no run job. If
  the job's push is denied, give this repository the Write role under the package's "Manage
  Actions access".
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
  `--dataset tpcds --build-only` builds `/var/lib/peacock/tpcds.sf40.duckdb` the same way, every
  parquet file a table, and times nothing. The corpus run, `--cases`, `--answers` and
  `--results`, is under "Corpus benchmarks".
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
holds one is held to it; a tree without is held to the whole main list. A list's `ok` and
`wrong` rows are main-list cases, at modes the main list carries for them (`h200/` exempt).
`duckdb-cpu128/` holds DuckDB's results, not a device's: the tree checks pass it by (DuckDB's
side, below). A list's
first line is `dataset,sf,query,modes,status,note`, and a row splits at its first five commas:
- `dataset`, `sf`: as the main list writes them, `tpch` and `40`;
- `query`: the main list's ident, `_` kept (`aggregate_groupby`);
- `modes`: mode idents `|`-joined, no spaces (`tp1_single|tp4_sized`); empty when skipped;
- `status`: `ok`, `skipped` or `wrong`;
- `note`: the rest of the line, commas allowed. A skipped row's reason, a wrong row's `#NN`
  ticket, empty for `ok`.

A case appears once. `parse_device_cases` and `render_device_cases` in `test_support` are the one
reader and writer; `every_device_list_parses` reads each committed list. Scripts read a list
through `scripts/calibration/device_cases.py`, which prints its `ok` and `wrong` rows
(`<csv> [--dataset <ds>] [--mode <mode>]`, tab-separated, file order). Both readers are tested
against `testdata/fixtures/device-cases.csv`. `device_lines` writes a device's rows from its tree:
an `ok` row at the modes with a timed section, a `skipped` row with the first failure's reason
when there is none, and a `wrong` row where the list as it stands marked one, since the tree knows
no tickets. The main list,
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
resume instead: a section is kept, its case not run, and a `kept` line printed, when its trailer
is the seventeen keys below in order with this run's five last, and its every node carries its
batches. A failed case, a case never reached, one measured on another device, driver, CUDA, cuDF
or host, and an older binary's section without the batches or the load are run again. Use it only
to finish a run that was cut short — preempted, or stopped by the idle watcher — with the same
binary: the five do not name the build, so a resumed run after a rebuild of the same format keeps
the old binary's numbers. `0` or unset
measures everything; any other value is refused. A captured run writes no section and keeps
none. A resumed run's record holds only the cases it ran, and `split_record.py` refuses a record
short of the list, so a resumed run's record is not published today.

`PEACOCK_BENCH_CASE_TIMEOUT_S=<s>` bounds each case of a plain run, prefill included. A case still
running after `<s>` seconds gets the section `status=failed reason=timeout: the case ran past <s> s`
with the five, and then the process aborts: a hung case cannot be unwound, and an abort runs no
C++ or CUDA static destructor, which could block on the hung device work. `run-tests.sh` reads the
SIGABRT as a failed binary, exit 1. The cases after it in that
process do not run. A resumed run measures a failed case again, a timed-out one included. Unset
means no bound; anything but whole seconds above 0 is refused.

`PCK_WRITE_GPU_RESULT=1` makes a measured case record the chosen run's answer in
`goldens/<dataset>.sf<sf>/gpu-result.txt`, section `== <query> mode=<mode>`, before its section is
written: a resumed run keeps the section, so a recording that failed or was killed leaves the case
to be measured again, and a refused recording is the case's failure. The rules are the device tier's ("Golden files"): the row order is the line's, read from
the sf1 registry row the case comes from, and a cuDF other than the file's is refused. A failed or
kept case records nothing. `run-gpu.sh pull` brings the file home.

The run's own refusals fail before anything is recorded: a capture or resume variable naming no
setting, no device to name, no rmm pool, a debug build, a dataset that is not there, and a record
at `PEACOCK_RECORD_PATH` that another run wrote.

**The prefill.** No measured round reads a file from disk. Before its first case's warm-up, the
process plans every case it will run (libtest's filters, `--exact` and `--skip`, read from its
own arguments), collects the files their loads read, and prefills them into the pinned host
cache once, under the first case's session option. Every warm-up and measured round of every case
then reads those pinned buffers. A case whose dataset is missing, or whose query does not plan, is
left out and fails on its own. The run prints one line on stderr:

```
[pinned-cache] prefill: cases=<k> files=<n> bytes=<n> us=<n> needed=<n> limit=<n>
[pinned-cache] prefill refused: needs <needed> > limit <limit>
[pinned-cache] prefill skipped: the pinned host cache is off
```

Unlike the refusals above, a refused prefill is recorded: every case's section is a failed one,
`status=failed reason=prefill: needs <needed> > limit <limit>` (or `prefill: <path>: <error>`), so
a section without numbers says why. With the cache off (`pinned_host_cache_bytes = 0`, or no
discrete device) the cases measure from the path. A case also fails if one of its files is outside
the prefill (`prefill: <path> was not prefilled`), or if the cache filled, fell back, reloaded or
evicted during its rounds (`pinned cache: <f> fills, <b> fallbacks, <r> reloads, <e> evictions
during the rounds`). After its runs a case prints one stderr line,
`[load-bytes] <case>: compressed=<n> decoded=<n>`; the cache's counters are in its trailer
(`pinned_cache`, under **The tree**). `compressed` is the footer's compressed size of its loads'
row groups over the projected columns; `decoded` is the bytes of the batches its loads emitted in
the reported run.

It asserts nothing about an answer, so it can never gate a merge. Its nine harness assertions
run on every `gpu-tests` job under `--skip bench_`; the timed cases run only here. The prefill's
case selection and refusals are tested in the cpu rung (`test_support::benchmark_prefill::tests`).

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
# 5. the derived records: both Nsight passes, calls.tsv, loads.tsv and hbm.tsv, the counters a
#    case at a time and joined onto the pinned records (fetch_record.sh <dataset>.sf<sf> for each
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

    time_us=[[22,37,40],[55,1,30]] total_us=185 batch_rows=[[6001215],[5998785]] batch_bytes=[[96019440],[95980560]]

`time_us` is by driving lane and call; `batch_rows` and `batch_bytes` are by output lane and
emitted batch, as in `.cpu.txt` and from the same reported run. The two axes differ: an
accumulator makes many calls and emits one batch. The `h200/` sections predate the two batch
keys and carry only `time_us` and `total_us`. In `time_us`, lanes are outermost, with one entry
per call the lane made, each entry the call's device microseconds summed over its output
partitions; `1` where the clock rounded a region to zero, since every call opens
one; `0` where the call crossed no ABI — the backend answered from what it held, as an accumulator
taking a batch or a join's `set_build` does. A lane the node was never driven on is `[]`.

Then a `--- run ---` trailer, one `key=value` a line, seventeen keys in this order:

- `run_us`: the chosen execution end to end, after planning.
- `device_us`: Σ of the tree's `total_us`. The gap to `run_us` is the host.
- `load_us`: Σ of the `GpuLoadParquet` nodes' `total_us`, 0 in a plan with none. It is the
  device-side load. The pinned cache's host IO is never in it.
- `execute_us`: `device_us` − `load_us`.
- `peak_bytes`: the most bytes the run's memory accountant held at once: driver-held batches
  plus every live executor's `resident_bytes()`. It is exact for that figure. As device memory it
  is a lower bound: it leaves out a call's scratch (no device executor measures it), cuDF and rmm
  temporaries inside a call, pool fragmentation and the parquet reader's staging. It is at least
  every `batch_bytes` entry of its section, since every emitted batch is held first.
- `runs=[…]`: every measured execution, in order.
- `build=release`.
- `allocator=`: what `install_rmm_pool` reported. With rmm's default every cuDF intermediate is a
  `cudaMalloc`/`cudaFree` round trip billed to the node that allocated it, which moves the
  profile and not just the scale.
- `pinned_cache=`: the pinned host cache as it prints at the end of the case, nothing appended:
  `on cap=… resident=… files=… fills=… fill_us=… fallbacks=… evictions=… reloads=… source=…`,
  `off source=<default|session|env>`, `off integrated` or `off no-device`. Its counters are the
  process's, cumulative.
- `prefill_files`, `prefill_bytes`, `prefill_us`: the process's one prefill (**The prefill**,
  above), what it read from disk and its wall time. Every section one process writes carries the
  same three; under one process per query they are that query's. All three are `0` with the cache
  off. There is no per-round key: a round that filled, reloaded, evicted or fell back fails its
  case, so a timed section whose `pinned_cache` starts `on` read pinned buffers only.
- the five provenance keys, `device=` to `host=`, always last: a resumed run keeps a section only
  when its trailer is these seventeen with this run's five, and its nodes carry their batches.

The `h200/` sections predate `load_us`, `execute_us`, `peak_bytes`, `pinned_cache` and the three
`prefill_*` keys. Only `h200/` may lack them (`PREDATES_ROWS` in `test_corpus_goldens`); a section
in any other tree without them is refused. One Rust reader, `parse_benchmark_section` in
`test_support`, reads a section in either shape (the query-batching generator reads the H100 tree
in Python, held to it only by its `--check`): a field a section lacks reads as `None`, never as
a derived value, and `tree_load_us()` gives an old section's load. A failed case's section is its
record instead (above).

**The page**, `testdata/benchmark-report.html`, is committed: every device tree side by side. One
row per (dataset, query, mode) that some tree has a section for, a table per (dataset, sf), in the
main list's order; four columns per device, by name. The test takes that order from the registry,
which writes the list; it never reads the list's text. A timed section shows load, execute, total
(each in ms, the total `device_us` linked to the section) and `peak_bytes` in GiB, labelled the
accountant's peak, a lower bound on device memory. An `h200/` section's load is `tree_load_us()`
and its peak `—`. A load is marked `unpinned` when its section's `pinned_cache` is not `on`, or
is absent: its rounds did not read pinned memory. Every `h200/` load is, since that tree predates
the cache. A failure shows `failed: <reason>`, a marker `skipped: <reason>`; where the
device's `cases.csv` has a `skipped` row for the query, its note is shown instead, and a `wrong`
row adds `wrong answer #NN`. Any other cell is `—`. After the devices, one column per DuckDB tree
(below), `warm (ms)`: on every mode's row of a query its file holds, `ms_2nd_min` with one decimal,
linked to its line, or `failed`; else `—`. A time whose `warm` verdict is not `yes` adds
`not warm: <verdict>`. The intro says what one DuckDB time is, a CLI process's wall time, and
names each DuckDB file's `duckdb`, `threads`, `host` and floor (`floor_ms`). Last, one
fused-batches table per `<device>/fused/<ds>.sf<sf>/tp1_single.fused.txt` (*Fused batches*,
below), read through the fused format's own reader: per batch its queries, outcome, fused load
and execute (ms), the accountant's peak (GiB, a lower bound), cap and reads per table, the batch
linked to its section. An out-of-memory batch shows `out of memory (cap k)`, a marker `same set as
all`. No fused file, no table. `render_benchmark_report` in `test_support` writes it through `parse_benchmark_section`
and `parse_duckdb_results`, with no time and no absolute path, in `testdata/report.html`'s look.
**A commit that changes a results tree, a fused file or the registry's device cells regenerates
the page**, since the registry gives its row order:

```
UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens \
    -- the_benchmark_report_matches_the_trees
```

The same test without the variable enforces it: a page that differs from the render fails. The reported execution is the
second-smallest by `run_us` of ten, after one discarded warm-up: the fastest run is the one most
likely to have caught a scheduling accident, and a whole run is reported rather than a per-node
minimum, which would be a tree belonging to no execution. Both counts are constants in
`src/test_support/` (`MEASURED_RUNS` in `mod.rs`, the warm-up in `corpus_benchmark.rs`), so every
file in the tree was taken at the same counts. `--pull-benchmarks` is additive and no push
`--delete`s the directory.

**DuckDB's side.** `benchmarks/duckdb_minimal.sh` times DuckDB 1.5.4 over its own storage, the page
cache warmed and its residency checked, all cores, the second-smallest of six. `--dataset tpch|tpcds`
picks the dataset; `--cases <cases.csv>` runs the corpus text, `testdata/<ds>-queries/<q>.sql`, of
the list's `ok` and `wrong` rows of that dataset, in file order, instead of the TPC-H minimal set,
which alone is verified against goldens. A corpus query runs under the oracle's two settings,
`default_null_order` and `integer_division`, each in a `-c` before the query's: the CLI parses a
`-c` whole, so a parser setting in the query's own `-c` is read too late. `--answers <db>` stores
each query's threads=1 answer as table `"<q>"` (`<q>` the file's stem), which `duckdb_result.py
--sf 40 --answers <db>` renders. `--results <tsv>` writes `duckdb.tsv`: a `# ` line of one-word
`key=value`s (`duckdb threads host cpu cores memory_gib db resident_pct floor_ms`; `floor_ms` is
`SELECT 1` timed as a query is, its second-smallest), the column line `query
ms_2nd_min runs_ms workset_bytes disk_read_bytes warm status`, then a row per query, `runs_ms`
comma-joined, a failed row `-` for its times, its disk read and its warm verdict. Its readers are
`parse_duckdb_results` in `test_support` and the query-batching generator's, both tested over
`testdata/fixtures/duckdb-results.tsv`, which
`benchmarks/tests/test_duckdb_minimal.py` holds the script's output to. The results live in
`testdata/benchmark-results/duckdb-cpu128/<ds>.sf40/duckdb.tsv`.

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

### sf40 baseline

The H100's tp1_single tree at sf40, and DuckDB on 128 cores for the same queries: the two baselines
query batching is measured against. sf40-baseline measured them on 2026-11-05.

**The one ordered list** is `testdata/benchmark-results/h100-80gb-hbm3/cases.csv`. query-fuser,
tp4-sized and batching-report read it, and no second list is kept. Nobody writes it by hand.
`the_h100_list_is_the_lists_measured`, in `test_corpus_goldens`, writes it from the tree:

```
UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens \
    -- the_h100_list_is_the_lists_measured
```

- Its candidates are the main list's lines carrying `tp1_single`, in the list's order: tpch 48
  (q1-q22 and 26 micro queries) and tpcds 82. `H100_MODES` names the modes it writes:
  tp1_single and tp4_sized. A row carries each mode its query is timed at, so a query whose
  tp4_sized section failed keeps `tp1_single` alone. A mode with no file in the tree is not read.
- A query with a timed section is an `ok` row. A query whose every section failed is a `skipped`
  row, its note the failure's reason.
- A `wrong` row is kept from the file as it stands, since the tree knows no tickets. Mark one by
  hand, `wrong` and `#NN`, and the writer carries it.
- Without the variable the test holds the file to the tree. It fails naming the first line that
  differs, or a tree with no file.
- Scripts read the list through `scripts/calibration/device_cases.py`.

On 2026-11-05 the list held 124 `ok`, 4 `skipped` and 2 `wrong` rows:

- skipped: tpch `anti_join`, whose export's `o_comment` comes back LargeUtf8, not Utf8
  ([#307](tickets/system-hardening.md#t307)); tpcds
  q17, q25 and q29, which tp1_single's planner refuses because their accumulators need more than
  the 2 GiB budget;
- wrong, [#306](tickets/testinfra.md#t306): tpcds q18 and q73. Both answers hold DuckDB's rows;
  only rows tied on every sort key come in another order, which a `no_sort` comparison refuses.

**The protocol.** peacockdb's warm numbers read files resident in pinned host memory. Each
query's process prefills them once, before any round. DuckDB's read its own storage through its
buffer manager, over a warm page cache. Neither counts the first read from disk. One DuckDB time
is one CLI process's wall time: the process starts, opens the database and runs the query.
`duckdb_minimal.sh` times `SELECT 1` the same way and writes it as `floor_ms`, what each time holds
besides its query. It is not taken off: a reader subtracts it. On 2026-11-05 the floor was 30 ms
over tpch.sf40 and 36 ms over tpcds.sf40 (nebius-cpu128, second-smallest of six), so tpch's
125-row `cross_join` at 32 ms is nearly all floor. The prefill's
bytes and time are in each section's trailer (`prefill_bytes`, `prefill_us`), apart from the load.
A timed section with `pinned_cache` other than `on`, or with `prefill_files=0`, is no baseline.

**The H100 side, again.** Every VM starts right before its first command and stops after its last:

1. The install, built on the CPU VM ("The CPU-VM build"), into
   `/home/dmitry/build/peacock/sf40-baseline/install`.
2. Set aside dev's `testdata/calibration/records.tsv` if a run other than this one left it. Clear
   the H100's `~/peacock/<i>/testdata/benchmark-results/h100-80gb-hbm3/`, so nothing stale comes home.
3. `$H start`; `$G prepare`; `$G ship --install /home/dmitry/build/peacock/sf40-baseline/install`.
4. `scripts/env/bench-each.sh $VM --cases peacockdb-core/tests/common/corpus_benchmark_cases.inc --mode tp1_single --timeout 1800 --records <scratch>/records`,
   then, for the list's second mode, the same with `--mode tp4_sized --within
   testdata/benchmark-results/h100-80gb-hbm3/cases.csv` (without `--within` it also times
   anti_join, q17, q25 and q29, and the list's writer then changes their rows); then `$H stop`.
   On 2026-11-05 the tp1_single run took 65 min for 130 cases, without a break.
5. The list and the page, as above and under "The page".
6. The record: `fetch_record.sh h100-80gb-hbm3/<ds>.sf40` for both datasets, then
   `join_records.py --record testdata/calibration/h100-80gb-hbm3/*.sf40/records.tsv <scratch>/records/*.tsv --out <joined>`;
   `split_record.py --record <joined> --cases testdata/benchmark-results/h100-80gb-hbm3/cases.csv --out-dir <split>`;
   `publish_record.sh --from <split> h100-80gb-hbm3/tpch.sf40 h100-80gb-hbm3/tpcds.sf40`, with dev's
   `peacock.env` exported.
7. The profiles, a pass per dataset and mode. A cases file is the header and that dataset's
   rows. The list is written at two modes, so a row makes a case per mode, and `--mode <mode>`
   keeps one. The metrics merge is then held to that mode's calls of the record, which holds
   both modes. With
   `P="PCK_BENCH_HBM_SET=gh100 PCK_BENCH_HBM_PEAK_BW=3.35e12 PCK_BENCH_HBM_FREQ=10000"` and
   `C="scripts/create_nsys_profile.sh --device h100-80gb-hbm3 --host nebius-h100 --ssh "$H rsh" --remote-dir /home/dmitry/peacock/<i> --remote-cudf-root /home/dmitry/peacock/cudf-25.02"`:
   - **tp1_single.** tpch: `$P $C --cases <ds>.csv --mode tp1_single` (both passes). tpcds takes
     three runs, because `nsys_calls.py` refuses the 14 cases whose sf40 join order differs from
     the sf1 goldens, and `set -e` then skips the metrics pass. `$C --trace --cases <ds>.csv
     --mode tp1_single` keeps `loads.tsv`. `$C --trace --cases <ds>-65.csv --mode tp1_single`
     (those 14 left out) writes `calls.tsv` and overwrites `loads.tsv`, so set the first aside.
     `$P $C --metrics --cases <ds>.csv --mode tp1_single` writes `hbm.tsv`.
   - **tp4_sized.** A capture is too large to come home (below), so both passes run on the VM a
     case at a time, outside `create_nsys_profile.sh`. Each case runs as the script runs it: the
     same environment, nsys flags, binary and `<test> --exact`. Trace: capture, `nsys export
     --type=sqlite`, then `nsys_loads.py` and `nsys_calls.py --plans-dir testdata/goldens` on the
     VM, then delete the capture. Metrics: capture with `$P`'s counters, export, then
     `nsys_hbm.py --capture … --case "<ds>.sf40 <q> tp4-sized" --peak-bw 3.35e12 --out <part>`.
     Its `--record` is the case's own rows of the record (the notes, the header and the rows with
     that query), not the whole 541 MB file. Delete the capture. Only the per-case files and the
     parts come home. On dev, join each file's per-case files under one header, in the list's
     order. Then merge the parts: `nsys_hbm.py --parts … --record <records.tsv> --mode tp4-sized`.
     A case left out of the pass must be cut from that record too, or the merge refuses its
     calls. tpch's trace was two captures, each read on the VM: all 45 cases for `loads.tsv`,
     and the 42 the recipe check accepts for `calls.tsv`. Its metrics pass fits the script:
     `$P $C --metrics --cases <ds>.csv --mode tp4_sized`.
   - q14, q61, q80 and q88 were left out of tpcds's tp4_sized passes on an estimate, not after a
     failed attempt. Their raw nsys data was estimated past 40 GB, half the H100's 80 GB free.

   Move `testdata/calibration/{calls,loads,hbm}.tsv` to `testdata/calibration/h100-80gb-hbm3/<ds>.sf40/`,
   adding a mode's rows to the others', and restore the H200's `calls.tsv` and `hbm.tsv` from git.
   Then `$H stop`.

`$VM`, `$H` and `$G` are the H100's `--instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu`,
`scripts/nebius.sh $VM` and `scripts/env/run-gpu.sh $VM`.

**The profiles' files.** A device's derived files lie at
`testdata/calibration/<device>/<ds>.sf<sf>/{calls,loads,hbm}.tsv`, committed. The fetched
`records.tsv` beside them is not. The H100 needs `PCK_BENCH_HBM_FREQ=10000`: at the default 20 kHz
a sample read 101 % of peak. `nsys_hbm.py` counts only sampling gaps over the case's ranges. Each
H100 process ends with a gap of about 1.5 s, which is likely the pinned cache being freed.

**What `calls.tsv` does not cover.** `nsys_calls.py` checks each case's capture against
`goldens/<ds>.sf1`'s recipes, since plan goldens exist at sf1 only. At tp1-single, sf40's planner
orders the joins of 14 tpcds queries differently, so their captures cannot be checked: q18, q30, q31, q32,
q33, q56, q60, q61, q71, q81, q83, q85, q91 and q92. The H100's tpcds `calls.tsv` covers the other
65, from a trace pass over those alone. Its tp1-single `loads.tsv` and `hbm.tsv` rows cover all 79,
since neither reads a plan. tpch's three files cover all 47 at tp1-single. No sf40 plan goldens are kept.

At `tp4-sized` the files hold tpch's 45 timed queries, but `calls.tsv` holds 42: the recipe check
refuses `global_stddev`, q15 and q22 at sf40. tpcds's `loads.tsv` and `hbm.tsv` hold 72 of its 76
timed queries, and `calls.tsv` holds 58. The recipe check refuses 14: q9, q30, q31, q32, q33, q54,
q56, q60, q71, q81, q83, q85, q91 and q92. q14, q61, q80 and q88 have no profile, since their
captures are estimated not to fit on the H100's disk. A tp4_sized capture is far larger than a tp1_single
one, since sized batches make a call per batch. tpch's 45 cases made 32 GB of raw nsys data and a
5.9 GB export. tpcds q13 alone made 51 GB of raw data, about 160 KB per record row. So tp4-sized
captures are read on the VM, one case at a time for tpcds, and only the derived files come home.
tpcds's metrics pass joined each case against that case's record rows alone, on the VM, and
merged the parts on dev.

**DuckDB's side, again**, on nebius-cpu128 (`$C`, "Nebius host"):

1. `$C start`; `D=$(scripts/nebius-sync.sh --instance-id computeinstance-e01t2mm4265y8x88ey --profile eu-west1 --no-data)`.
   `sha256sum` of the H100's `cases.csv` must agree on dev and on the VM.
2. Once per database: `benchmarks/duckdb_minimal.sh --dataset tpcds --dir /var/lib/peacock --sf 40 --skip-download --duckdb /usr/local/bin/duckdb --build-only`
   (tpch's database is built the same way, without `--dataset`).
3. Per dataset, in `~/$D`: `benchmarks/duckdb_minimal.sh --dataset <ds> --dir /var/lib/peacock --sf 40 --skip-download --duckdb /usr/local/bin/duckdb --cases testdata/benchmark-results/h100-80gb-hbm3/cases.csv --answers /var/lib/peacock/answers-<ds>.duckdb --results /tmp/duckdb-<ds>.tsv`.
   Run it detached: tpch took 11 min, tpcds 9. Copy both files home with `rsync -e "$C rsh"`, then `$C stop`.
4. On dev: `python3 testdata/duckdb_result.py --dataset <ds> --sf 40 --answers <answers-<ds>.duckdb>`
   writes `goldens/<ds>.sf40/duckdb-result.txt`. The TSV goes to
   `testdata/benchmark-results/duckdb-cpu128/<ds>.sf40/duckdb.tsv`, and the page is regenerated.
   An answer over the cap by its row count alone is fingerprinted in batches and never rendered.
   tpch's `filter-project` is 96 M rows: 9 min, 13.5 GB.

**The sf40 answer check**, `every_sf40_device_answer_meets_duckdb` in `test_cpu_corpus`. It holds
each `ok` and `wrong` row's tp1_single `goldens/<ds>.sf40/gpu-result.txt` section to DuckDB's
(a row's tp4_sized answer is held by nothing), under the
oracle of the query's sf1 registry line. An `ok` row must meet DuckDB and a `wrong` row must still
differ; a `skipped` row is not judged. No registry line is written for sf40, so the two sections
are compared by fingerprint whenever either holds one: at sf40 an answer passes the cap by scale,
whatever the sf1 line declares. Near the cap DuckDB's longer floats can pass it alone.

### Fused batches — `prototype_fused_benchmarks` (by hand)

query-fuser's prototype (`src/prototypes/fuser/`, spec `llm-wiki/tasks/query-fuser.md`). Eight
cases, `bench_fused_<tpch|tpcds>_sf40_<first3|first10|first20|all>_tp1_single`. Each fuses that
prefix of the H100 `cases.csv` list, read when the case runs, into one plan with one shared load per
file, plus a load per re-read the hold rule chooses. It runs at sf40 only: the hold rule and the cap read sf40-baseline's
`tp1_single.benchmark.txt` and `calibration/h100-80gb-hbm3/<ds>.sf40/loads.tsv`. Any other scale
panics.

The protocol is the tree benchmark's: the 69 GiB pool, the pinned cache prefilled with the
batch's files, one warm-up, then `MEASURED_RUNS` runs (`FUSED_ALL_RUNS` = 3 for `all`), the
second-smallest reported. Every answer is checked against its query run alone on the device. A
batch that runs out of device memory writes its section and passes.

- Results: `testdata/benchmark-results/h100-80gb-hbm3/fused/<ds>.sf40/tp1_single.fused.txt`, one
  section per batch. The format is `llm-wiki/tasks/query-fuser-detail.md`, "The fused format".
  Each file is a table on the report page (*The page*, above), regenerated in the commit that
  adds or changes it.
- Record: `testdata/calibration/fused/<ds>.sf40/<batch>.records.tsv`, gitignored, one per batch
  and written whole by each run: one process runs one batch, and its heading (the pool's free
  memory among it) is that process's. Its `query` is the batch, as the NVTX case range
  `<ds>.sf40 <batch> tp1-single` names it. A captured run (`PEACOCK_BENCHMARK_CAPTURE`) writes
  no section and its rows only to `PEACOCK_RECORD_PATH`, never the batch's record.
- Profiles: `scripts/create_nsys_profile.sh --fused <ds>.sf40`, one capture per batch and pass
  (*Benchmark data flow*). Its derived files are committed under
  `testdata/calibration/h100-80gb-hbm3/fused/<ds>.sf40/`: `<batch>.calls.tsv`,
  `<batch>.loads.tsv`, `hbm.tsv` and `failed.tsv`.
- `PEACOCK_BENCH_RESUME=1` keeps a section this device, driver, CUDA, cuDF and host wrote.
- The comparison page, `llm-wiki/reports/query-batching.md`: fused against unfused and DuckDB per
  batch, with the hints, the load split and the provenance. Every number between its markers comes
  from `scripts/calibration/batching_report.py --testdata testdata --report llm-wiki/reports/query-batching.md`;
  the Reading below them is written by hand from those tables, and `--check` does not cover it.
  Rerun it in the commit that changes any of its inputs; `--check` exits 1 while the page is
  stale, and dataset-matrix runs it on the 25.02 leg, beside `generate_pbench.sh --check`. The
  page is not documentation to the pipeline's path filters (*CI structure*), so a push that
  changes only it still runs the check.
- Each batch on the page is its fused section's own `queries=`, each query held to a timed
  section of `tp1_single.benchmark.txt`. A section that is no longer today's `cases.csv`
  prefix (a query the list gained, lost or reordered, or a batch that became or stopped being
  `same set as all`) is not refused: a line under its table names it stale, to rerun on the
  H100. Regenerating the page then passes `--check`; the rerun is what clears the line.

No CI step builds or runs it. `stage-install.sh` stages it into `cpp/install/rust-benchmarks/`
(release, `gpu,test-support`). On the H100, one batch per process:

```bash
scripts/env/run-gpu.sh --instance-id <id> --profile editor-eu run prototype_fused_benchmarks \
  --exact bench_fused_tpch_sf40_first3_tp1_single
```

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
