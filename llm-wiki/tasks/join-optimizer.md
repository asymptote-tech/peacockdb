# join-optimizer — the prototype's join-order optimizer, runnable, reported and documented

Kind: production

The CPU prototype (`scripts/exec_model`) carries a join-order optimizer: it reads the engine's
own plan goldens, estimates every join from statistics sidecars and parquet footers, reorders
inner-join clusters with DPhyp (`peacockdb-dphyp`, a Rust crate called over its C ABI), prunes
fact scans with dynamic filters, and replans a run whose build lands far off its estimate.
Answers match DuckDB on every corpus query. This task makes it the only consumer of the
prototype's plans, lays the prototype out by component, makes its inputs reproducible on any
host, runs it over the whole corpus with a report of what it did and what it saved, and
documents it.

The optimizer is judged on CPU, by the prototype's `cpu.txt` renders. Nothing here measures it
on the device.

## The prototype reads only the engine's plans

The handwritten lowerings — `tests/plans*.py`, `tests/plan_helpers.py`, their goldens
`scripts/exec_model/{tpch,tpcds}.plans.txt`, and the suites that run them (`test_tpch.py`,
`test_tpch_corpus.py`, `test_tpcds.py`) — go, with their CI steps. They are the prototype's
own model of what the planner emits; the engine's `testdata/goldens/<bench>.sf1/<mode>.plans.txt`
are the real thing, and the optimizer reads nothing else. Driver tests over mock plans stay:
they are not plans to execute.

What in `operators/` only the handwritten plans used goes with them — column addressing by
name, simplified node shapes. What stays is what the engine's plans need: columns by position,
each join type's own output, the aggregate forms the planner emits (ROLLUP, Welford states,
final expressions), scans over real row groups, the expression and hashing details that make
answers match. `design.md` says which is which.

## One folder per component

    scripts/exec_model/
      engine/      plan, node, layout, runtime, batch, executors, accounting, forwarder, limit,
                   single_partition_driver, partitioned_driver, scheduler, adaptive
      operators/   less what the handwritten plans alone used
      plans/       engine_plan, engine_expr, engine_ir, engine_nodes, engine_run, plan_text
      optimizer/   stats, observed, estimator, cardinality, cost, call_cost, multijoin,
                   disassembly, join_order, dphyp, dynamic_filters, replan
      tests/{engine,operators,plans,optimizer}/

`plans/` is the text of an engine plan turned into the prototype's tree; it is neither the
runtime nor the optimizer. Moves only — no behaviour changes ride on them, so each move commit
is green by the same tests as before it.

## Statistics sidecars, one per dataset whatever its embeddings

`testdata/gen_stats.py` writes the same file on any host. For tpch it does not yet: `part`
and `partsupp` carry `*_embedding` columns that `generate_testdata.sh --embeddings` writes as
`FLOAT[8]` (synthetic, the default and CI) or as DEEP1B/GloVe vectors (external), so their NDV
and the whole-footer fingerprint differ, and a sidecar written on one host is refused on the
other. The sidecar drops vector columns — no estimate reads them — and the fingerprint is the
row count and the stock columns' footer statistics, not the footer's bytes. A test generates
both ways and gets one file.

## run.py: the corpus through the optimizer

`scripts/exec_model/run.py` runs every corpus query of both benchmarks with the optimizer on,
filterable by `--bench`, `--query` and `--mode`, parallel by `--jobs`. It writes, under
`scripts/exec_model/testdata/goldens/<bench>/`:

- `<mode>.cpu.txt` — the format of the engine's `cpu.txt`, rendered from the optimized run.
  Generation only: nothing checks these against a previous copy.
- `<mode>.optimizer.txt` — a `== <query>` section each, in the same order, holding every rule
  that changed the plan: dynamic filters (candidate, key bounds, row groups before and after);
  each DPhyp call (relations, edges, the estimated sets it priced, the tree it returned);
  orientation (which joins flipped); replans (the event, estimate against truth, what was kept);
  and the plan's diff before and after, which `<mode>.plandiff.txt` holds, a section each.
- `cost_report.html` — one row per query: per mode, non-optimized cost | optimized cost | ratio;
  DuckDB cost; final ratio, min over modes of the peacockdb cost / DuckDB cost; the rules that
  fired. Cost is the engine's own function (`.cost.txt`: bytes per node kind, summed to
  `peacockdb_cost`) applied to two runs of the prototype, without and with the optimizer: the
  prototype's rows match the engine's and its bytes are pandas', so only two prototype runs
  compare. The engine's `.cost.txt` figure is a column of its own.

A full run's wall time on shad-gpu is measured and stated in `README.md`. If it is past forty
minutes, profile first; Polars replaces pandas only if the profile puts the time in pandas.

## The calibration record lives in S3

`testdata/calibration/records.tsv` leaves git: the sf40 record is 175 thousand rows, 17 MB,
and grows with every case the benchmark gains. It goes to the `calibration` bucket, keyed by
dataset and scale; `build-test-shadgpu.sh` uploads what a run pulls home, and one script
fetches it. `testdata/.gitignore` stops excepting it. No CI test reads a file that is not in
git: the preamble check runs against a record the test writes, and the per-call model's check
against the measured record (`test_call_cost`) runs where the record is fetched, beside the
corpus suites.

## Benchmarks: every query enabled on the device

`corpus_benchmark_cases.inc` gains every (query, mode) enabled on the device in
`corpus_cases.inc`: the named tpch queries (`aggregate_groupby`, `filter_project`, …) and
tpcds q82, q84, q85, each at every mode it is enabled at. All at sf40, for the reason the case
file gives: at sf1 a call sits below the clock's resolution. tpcds sf40 does not exist on
shad-gpu yet and is generated there, beside tpch's. Measured when the device is free; trees
committed, records to S3.

## Cardinality accuracy, per join rather than in aggregate

`test_cardinality_corpus` holds the corpus to a median q-error and a share within 2× — two
thresholds read off one state of the corpus. With tpcds q64 in the corpus (37 joins, median
q-error 4.9) the share is 69.5 % against 70 %, though the estimator is the same. The
aggregate becomes a golden of each join's estimate and truth, so a new query is new lines in a
diff and a worse estimate on an old one is named. Why q64's estimates miss — correlated
filters across repeated tables — is investigated and either fixed or written into `design.md`
as a known limit.

## Documents

- `scripts/exec_model/design.md` — the design as it stands, in English, no history: the
  pipeline from plan text to optimized run, each component and why it has its shape, what the
  prototype does not model.
- `scripts/exec_model/README.md` — how to work with it: environment, building DPhyp and
  `PEACOCK_DPHYP_LIB`, the test tiers and what data each needs, `run.py`.
- `peacockdb-dphyp/design.md` — at most 200 lines: the Rust API and the C ABI, inputs and
  outputs (relation masks, edges, the cost callback, the budget, the postfix tree, return
  codes), the algorithm in brief, how it is tested.

## Forwarders cost nothing, scatters cost their bytes

`testdata/cost_model.conf` charges `GpuEmitPartitions`, `GpuMergePartitions`, `GpuUnion` and
`GpuInterleave` their full output bytes under `cuda_shuffle_bytes`. Merge, Union and Interleave
relabel lanes and move batches without a copy; Emit scatters rows. The conf gains
`cuda_scatter_bytes 1.0` (`GpuEmitPartitions`) and `cuda_forward_bytes 0.0` (`GpuMergePartitions`,
`GpuUnion`, `GpuInterleave`), and `cuda_shuffle_bytes` goes. The conf is the engine's and the
prototype's alike, so the engine's `.cost.txt` goldens and its cost report move with it. DuckDB's
cost (`testdata/duckdb_cost.py`) already charges streaming operators nothing; `design.md` says
so, so the two sides are known to price the same work.

## Join clusters are named

A multi-join is a cluster of joins inside which the order may change. Each join node in
`<mode>.cpu.txt` carries `mj=MJ<n>`, numbered per query in plan order, outermost first; a join
in no cluster (a cross join, a join no rule may move) carries none. `.optimizer.txt` names its
DPhyp calls by the same id.

## Non-inner joins reorder under TES

A cluster holds inner joins only, so a left, semi, anti or mark join is a wall: in tpcds q58 the
`RightSemi` that restricts dates to one week sits above `date_dim ⋈ catalog_sales`, and the
restriction reaches the fact table after a 1.4-million-row join. Each non-inner join gets a
total eligibility set by conflict detection (CD-C, Moerkotte et al. 2013) and enters the cluster
as a hyperedge, which `peacockdb-dphyp` already accepts; a cross join stays out. Named tpch
queries that reorder only this way join the corpus, among them

    SELECT c_name, o_orderkey
    FROM customer
    LEFT JOIN orders ON o_custkey = c_custkey
    JOIN nation      ON n_nationkey = c_nationkey
    WHERE n_name = 'FRANCE';

as written `(customer ⟕ orders) ⋈ σ(nation)`, ≈ 1.55M rows then 62k; reordered
`(customer ⋈ σ(nation)) ⟕ orders`, 6k rows then 62k. Answers stay equal to DuckDB's.

## Dynamic filters from any build, through unions

A dynamic filter is taken today only from a build that holds a filter and no join, and its key
lifts through wiring, filters, plain projects and inner joins. tpch q5's `o_orderkey ⋈
lineitem` has a join in its build; tpcds q5's date filter stops at the `GpuUnion` of each
channel's sales and returns, over fact columns ordered by date. Filters run in two passes. The
first, on the engine's plan, keeps today's build rule; a key lifts through `GpuUnion` and
`GpuInterleave` into every branch, each branch's scan pruned on its own footers, and through a
non-inner join into the side it outputs. The second runs on the plan DPhyp ordered, where a
build may hold a filter or a join — its values are the probe plan's measured output, as now —
so a restriction a reorder brought down, as q58's week, reaches the fact scan; nothing is ordered
again after it. A probe plan runs adaptively: a build that misses inside it replans, as in the
main run. tpch q5 gains nothing: `l_orderkey` is not ordered in lineitem, which is sorted by
ship date. q5 of both benchmarks and q58 are run before and after, and the costs go in the
detail file.

## DuckDB's cost for the named queries

`testdata/gen_duckdb_cost.sh` profiles q1..q22 and q1..q99 only, so no named tpch query has a
DuckDB figure or a final ratio. It profiles every corpus query, the named ones included; the
report reads them by name already. Which named queries leave room for the optimizer is written
into the detail file.

## `.optimizer.txt` that reads without the code

Each section says what it measures. A DPhyp call is headed by its multi-join id; each relation
`rN` lists its rows (exact, estimated or measured), row width and the NDV of its edge columns;
an edge whose column is an aggregate's output says so rather than printing the aggregate's name
as if it were a function; the dynamic filter's ordering score and the build read from memory are
worded as what they are; `C_out` names its units and that the second figure is the plan's own
order under the same estimates. The plan diff stays as it is, in a file of its own,
`<mode>.plandiff.txt`, which `.optimizer.txt` points to: with it the tpcds log was too large for
GitHub to display.

## A markdown page per mode

`run.py` writes `<mode>.optimizer.md` beside `<mode>.optimizer.txt`, from the same report data:
a `## <query>` section each; per multi-join that changed order a table whose columns are the
plan's order and then each call that changed it, and whose rows are the join tree drawn, what
changed it (empty in the first column: DPhyp, a flip, a replan with its q-error) and its cost, the
calls that kept the order named under it; a multi-join that kept its order is one line. A tree is
written in words with the build side in brackets and a filtered relation marked:
`[[nation (f)] join customer] left join orders`, the join kinds as SQL names them (join, left
join, full join, semi join, anti join), the side a non-inner join keeps written first. The dynamic
filters applied follow, each as scan, column, the build and key that pruned it, key range or
values, and row groups before and after.

## The report links to its evidence

`cost_report.html` takes the commit it describes (`--sha`). A ratio cell other than 1.00 links
to that query's section of `<mode>.optimizer.md` at that commit on GitHub. Rules fired becomes
per mode: how many multi-joins changed order and how many dynamic filters applied, each linking
to its lines in `<mode>.optimizer.txt`; DPhyp and flips count together. The `cost-report` job
renders it with the commit's sha from the committed `.costs.txt` and publishes it beside the
engine's page.

## `optimizer.md`: the heuristics in one place

`scripts/exec_model/optimizer.md`, at most 300 lines, every rule the optimizer applies and when:
what triggers a dynamic filter and which nodes its key lifts through; what a multi-join is and
how TES admits a non-inner join; the cost DPhyp is given — its formula, units, and which
selectivities enter it (filters below a relation, join containment, the flat share for a residual
predicate); orientation; what triggers a replan and what it re-optimizes. `design.md` keeps the
why and points here for the rules.

## On master as it stands

master moved under the branch. The engine's joins left the recipe architecture for a C++
session, and the prototype's pandas joins followed: the recipe backend is gone, `null_equals_null`
is one rule for every join type, anti and mark included, and closed tickets lifted the walls the
optimizer was built around (#152 build copies, #59 and #80 NULL equality, #153 and #159
residuals on outer and right-semi joins). Plans gained `key IS NOT NULL` filters under a shuffled
side at tp4 (#137), `__rowmarker__` projections and `GpuEmpty`; a third dataset, pbench, and a
ninth `corpus_query!` argument arrived. The branch moves onto master in a few commits, each green,
and the prototype is brought to master's execution model before its outputs are made again.

- **The move.** The branch's own commits are squashed into a handful by component, then rebased:
  DPhyp; the benchmarks and calibration; the prototype and optimizer with their goldens; the
  engine side (the cost conf, cost goldens, cost-report, the named queries, DuckDB's cost); the
  wiki. Where master already holds a change, its version stands. Ticket numbers taken on master
  move to free ones.
- **The prototype under master's model.** The plan reader reads every node and type master's
  plans carry, the joins are master's with the optimizer's additions, and a join's NULL equality
  is its flag alone.
- **The optimizer's rules under the closed tickets.** Orientation prices a build as the session
  holds it; a residual on a non-inner join no longer walls a cluster where the engine runs it; a
  key's `IS NOT NULL` filter under an exchange is wiring a cluster reads through. Each is measured
  before it is kept.
- **The engine side.** The named queries take master's case format and devices; the cost goldens,
  pbench's included, follow the conf; cost-report reads three datasets.
- **Made again and checked.** One full `run.py`, the cardinality golden and the corpus answer suites
  at every mode, and every move against the outputs before the rebase explained in the detail file.

## Not in this task

The optimizer in Rust; real parallelism in the scheduler or the driver — the scheduler names
what may run together, and nothing executes it concurrently yet; statistics for intermediate
results beyond the exact key NDV of a finished build; measuring the optimizer on the device.

## Scope

- `scripts/exec_model/**`, `testdata/gen_stats.py`, `testdata/stats/*.json`,
  `testdata/.gitignore`, `testdata/calibration/`.
- `scripts/build-test-shadgpu.sh`, `scripts/lib/shadgpu-env.sh`, a fetch script for the record.
- `peacockdb-core/tests/common/corpus_benchmark_cases.inc`,
  `peacockdb-core/tests/peacock_gpu_benchmarks.rs`, the record's preamble test,
  `testdata/benchmark-results/**`.
- `peacockdb-dphyp/design.md`; `.github/workflows/{pipeline,exec-model-corpus}.yml`.
- `llm-wiki/build-test.md`.
- `testdata/cost_model.conf` and what it moves: the engine's `.cost.txt` goldens, the
  `cost-report` crate, `peacockdb-core/src/test_support/cost_model.rs` if it names a category.
- `testdata/gen_duckdb_cost.sh`, `testdata/duckdb_cost.py`, the DuckDB profiles and
  `.duckdb_cost.txt` goldens.
- New named queries: `testdata/tpch-queries/`, `peacockdb-core/tests/common/corpus_cases.inc`,
  their engine goldens.
- `peacockdb-dphyp/` where a hyperedge TES needs is refused.
- Nothing else in the engine: `peacockdb-core/src/`, `cpp/`, the wire format.

## Done when

`grep` finds no handwritten lowering and no `exec_model/*.plans.txt`; the layout is the one
above; every prototype tier is green, the CI ones in CI and the corpus ones on shad-gpu;
`gen_stats.py` writes one file from both embedding modes; `run.py` over the whole corpus writes
the goldens, the `.optimizer.txt` files and the report, its wall time stated; `records.tsv` is
in S3 and not in git, and nothing in CI reads it; the device-enabled benchmark cases are
measured and committed; the cardinality golden is in and q64 is explained; the three documents
exist, the DPhyp one within 200 lines; the wiki counts add up. Forwarders cost nothing in the
conf; every join node carries its multi-join id; non-inner joins reorder under TES and the named
queries that show it are in the corpus; dynamic filters come from builds with joins and lift
through unions; every named query has a DuckDB cost; `.optimizer.txt` reads without the code,
`<mode>.optimizer.md` holds each multi-join's evolution, and the published report links to both
at its commit; `optimizer.md` exists within 300 lines; every answer equals DuckDB's. The branch
sits on master in a few green commits, the prototype reads and runs master's plans by master's
join rules, the optimizer's rules no longer encode closed tickets, and every output is made
again on master with its moves explained.
