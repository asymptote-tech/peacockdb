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
  and the plan's diff before and after.
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
- Nothing in the engine: `peacockdb-core/src/`, `cpp/`, the wire format.

## Done when

`grep` finds no handwritten lowering and no `exec_model/*.plans.txt`; the layout is the one
above; every prototype tier is green, the CI ones in CI and the corpus ones on shad-gpu;
`gen_stats.py` writes one file from both embedding modes; `run.py` over the whole corpus writes
the goldens, the `.optimizer.txt` files and the report, its wall time stated; `records.tsv` is
in S3 and not in git, and nothing in CI reads it; the device-enabled benchmark cases are
measured and committed; the cardinality golden is in and q64 is explained; the three documents
exist, the DPhyp one within 200 lines; the wiki counts add up.
