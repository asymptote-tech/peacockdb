# exec_model — the Python prototype

A model of the engine's execution model and a join-order optimizer over the engine's own plans,
run on pandas. Not production code. Why it has the shape it has is [`design.md`](design.md);
this page is how to work with it.

## Environment

- **Python 3.12 or 3.13, with pandas and pyarrow** (numpy comes with pandas). Seen green:
  3.12.3 with pandas 2.2.2 and pyarrow 18.1; CI's `rapidsai/base:25.02-cuda12.0-py3.12`, 3.12.9
  with pandas 2.2.3 and pyarrow 18.1; and 3.13.15 with pandas 3.0.6 and pyarrow 25.0.1 on
  shad-gpu. The tests run with whatever `python3` is on the PATH, so that one needs both
  packages. Files that need them fail rather than skip without them: a skipped operator suite
  reads exactly like a passing one. pytest is optional (below).

  ```
  python3 -c "import pandas, pyarrow; print(pandas.__version__, pyarrow.__version__)"
  ```

- **DuckDB 1.5.4**, the GitHub release build (08e34c447b) every golden is generated with — the
  dsdgen column types drift between releases, and another build may write other parquet bytes.
  The corpus oracle and `gen_stats.py` find it by `DUCKDB`, else `duckdb` on the PATH:

  ```
  curl -fsSL https://github.com/duckdb/duckdb/releases/download/v1.5.4/duckdb_cli-linux-amd64.zip -o duckdb.zip
  python3 -c "import zipfile; zipfile.ZipFile('duckdb.zip').extractall('duckdb-bin')"
  chmod +x duckdb-bin/duckdb && export DUCKDB=$PWD/duckdb-bin/duckdb && "$DUCKDB" --version
  ```

- **The sf1 tables**, `testdata/{tpch,tpcds}.sf1`, from `testdata/generate_testdata.sh`
  ([`build-test.md`](../../llm-wiki/build-test.md) has the recipe). Only the dataset and corpus
  tiers read them.

## DPhyp

The optimizer orders joins with the `peacockdb-dphyp` crate through its C ABI
([`peacockdb-dphyp/design.md`](../../peacockdb-dphyp/design.md)). Build the library and name it
with `PEACOCK_DPHYP_LIB`; every file that calls DPhyp refuses to run without it, naming the
build:

```
cargo build --release -p peacockdb-dphyp
export PEACOCK_DPHYP_LIB=${CARGO_TARGET_DIR:-$PWD/target}/release/libpeacockdb_dphyp.so
```

The library links against the host's glibc, so build it on the host that loads it: one built
on a newer system does not load on shad-gpu.

## The tests

Every test file runs on its own with `python3 <file>`, no pytest: `tests/harness.py` runs a
module's `test_*` functions and provides `raises`. A test file's five-line header puts the repo
root on `sys.path`, so the relative imports resolve either way. Arguments select tests: a test's
full name selects it alone, any other word every test whose name contains it, and `-word`
drops those. `PCK_SHARD=k/n` (k from 0) takes every n-th test from the k-th. A selection that
matches nothing fails.

| tier | files | needs | where it runs |
|---|---|---|---|
| cheap | every `tests/**/test_*.py` but the nine below | pandas, pyarrow, `PEACOCK_DPHYP_LIB`; committed files only | `pipeline.yml`, cost-report job |
| dataset | `optimizer/test_stats_sidecar.py`, `optimizer/test_cardinality_corpus.py`, `test_run_corpus.py` | + the sf1 tables and DuckDB | `pipeline.yml`, dataset-matrix (cuDF 25.02 leg) |
| corpus | `plans/test_engine_answers.py`, `optimizer/test_engine_dynamic_filters.py`, `optimizer/test_engine_reassembled.py`, `optimizer/test_engine_optimized.py` | + the sf1 tables, DuckDB, `PEACOCK_DPHYP_LIB` | `exec-model-corpus.yml`, manual dispatch; whole files on shad-gpu |
| manual | `optimizer/test_stats_embeddings.py`; `optimizer/test_call_cost_measured.py` | the embeddings cache and DuckDB; the fetched sf40 record | by hand; the record check also in `exec-model-corpus.yml`'s call-cost job |

**Cheap.** One file, then the tier as CI selects it, in bash or zsh:

```
python3 scripts/exec_model/tests/engine/test_determinism.py
shopt -s globstar 2>/dev/null  # bash; zsh globs ** without it
heavy='test_engine_answers|test_engine_dynamic_filters|test_engine_reassembled|test_engine_optimized|test_stats_sidecar|test_cardinality_corpus|test_run_corpus|test_stats_embeddings|test_call_cost_measured'
for f in scripts/exec_model/tests/**/test_*.py; do
  [[ $(basename "$f" .py) =~ ^($heavy)$ ]] || python3 "$f" || break
done
```

pytest collects the same files and selects by node id. Do not point it at the whole `tests/`
folder: it would import and run the corpus files, every query of them.

```
python3 scripts/exec_model/tests/engine/test_plan.py test_height_is_distance_to_root
python3 -m pytest scripts/exec_model/tests/engine/test_plan.py::test_height_is_distance_to_root -q
```

**Dataset.** Seconds to a minute each. `test_stats_sidecar` recounts the committed sidecars
(`testdata/stats/`) with `gen_stats.py` and compares byte for byte; `test_cardinality_corpus`
compares every join's estimate with the golden `testdata/goldens/<bench>/tp1-single.cardinality.txt`
here, and `UPDATE_CANONICAL=1` rewrites it; `test_run_corpus` runs `run.py` over three queries
into a temp directory.

```
python3 scripts/exec_model/tests/optimizer/test_stats_sidecar.py
python3 scripts/exec_model/tests/optimizer/test_cardinality_corpus.py
UPDATE_CANONICAL=1 python3 scripts/exec_model/tests/optimizer/test_cardinality_corpus.py
python3 scripts/exec_model/tests/test_run_corpus.py
```

**Corpus.** Each file runs the planned queries it applies to, from one mode's plan golden,
`PCK_MODE` (`tp4-single` by default), over whole sf1 tables against DuckDB: every one as planned
(120), those with a dynamic-filter candidate pruned (58), and those with a join cluster
disassembled and rebuilt (100) and reordered by DPhyp (100). A whole file is about a
process-minute per query at `tp4-single`, so run whole files on shad-gpu, in shards, and single
queries locally:

```
PCK_MODE=tp4-single PCK_SHARD=0/3 python3 scripts/exec_model/tests/plans/test_engine_answers.py
python3 scripts/exec_model/tests/optimizer/test_engine_optimized.py test_tpch_q3
```

The workflow runs the four files in three shards at `tp4-single`, its `filter` input passed as
the selecting argument. The other modes run by hand: tpch q8 and q9 at `tp4-rowgroup` take
eight to fifteen minutes each.

**Manual.** `test_stats_embeddings` generates tpch sf1 with external embeddings in a temp
directory — a minute or two and about 1.5 GB — and checks it gives the committed sidecar. It
needs the embeddings cache (`testdata/fetch_embeddings.sh`), which no CI host may fetch, named by
`PEACOCK_EMBEDDINGS_CACHE` when it is kept outside the tree:

```
PEACOCK_EMBEDDINGS_CACHE=/path/to/embeddings-cache python3 scripts/exec_model/tests/optimizer/test_stats_embeddings.py
```

`test_call_cost_measured` fits `optimizer/call_cost.py` to tpch sf40's calibration record and
asks that it order each query's modes as the device did: at least fifteen pairs whose measured
times differ by more than 10 %, none reversed. The record is not in git; it lives in the
`calibration` bucket by its sha256, which `testdata/calibration/records.sha256` pins, and the
fetch puts it in `testdata/calibration/tpch.sf40/records.tsv`, checked. The test fails naming
the fetch when the record is absent. With an aws CLI and the bucket's credentials on this host,
or through shad-gpu, whose CLI is not on a non-interactive ssh's PATH:

```
scripts/calibration/fetch_record.sh tpch.sf40
AWS=/home/info/bin/aws scripts/calibration/fetch_record.sh --ssh shad-gpu tpch.sf40
python3 scripts/exec_model/tests/optimizer/test_call_cost_measured.py
```

## run.py: the corpus through the optimizer

`run.py` runs each (query, mode) twice over the sf1 tables, as planned and through the optimizer
(`optimizer/pipeline.py`), holds the optimized answer to the planned one, and writes what it did.
It checks the sf1 tables and loads DPhyp before any work. Every filter is optional: both benches,
every planned query, all five modes; `--jobs` worker processes, one task per (query, mode).

```
python3 scripts/exec_model/run.py --bench tpch --query q3 join-int --mode tp1-single --jobs 2 --out "${TMPDIR:-/tmp}/exec-model-run"
python3 scripts/exec_model/cost_report.py --out "${TMPDIR:-/tmp}/exec-model-run"
```

Under `--out`, by default `testdata/goldens/` here, it writes per bench and mode:

- `<bench>/<mode>.cpu.txt` — the optimized plan's run, in the engine's `cpu.txt` format;
- `<bench>/<mode>.optimizer.txt` — what each rule did: dynamic filters, each DPhyp call and its
  orientation, each replan, then a unified diff of the plan text; `nothing fired` where no rule
  ran;
- `<bench>/<mode>.costs.txt` — the planned and the optimized run priced by the engine's cost
  function, the optimized cost's probe-plan and stopped-run parts, and how often each rule fired;
- `cost_report.html` — one page from every `.costs.txt` there, so a filtered run still renders
  the whole corpus. A row per query: per mode non-optimized | optimized | ratio; the engine's own
  `-mini.cost.txt` figure at the mode the final ratio takes; DuckDB's `duckdb_cost=` (the named
  tpch queries have none); the final ratio, projected; the rules that changed the plan, every
  count in the hover. `cost_report.py` renders it again from the files alone; without `--out` it
  re-renders the committed page here.

One `== <query>` section each, in the engine's registry order (its `-mini.cpu.txt`); a query the
planner refused has a `skipped:` line. A run without `--query` owns its files and drops sections
no query accounts for; with it, only its own sections change — the rule `UPDATE_CANONICAL` and
`PCK_UPDATE_SECTIONS` follow for the engine's goldens. A query that raises, an optimized answer
that is not the planned one among them, gets a `failed:` section, and the run exits 1. Nothing
compares the files with a previous copy.

The committed files are one whole run, on shad-gpu from a scratch copy of the tree with the sf1
tables beside it, under Python 3.13.15, pandas 3.0.6 and pyarrow 25.0.1. A batch's bytes are
pandas' `memory_usage(deep=True)`, so another pandas moves every cost line: regenerate in that
environment, and send exploratory runs elsewhere with `--out`.

```
python3 scripts/exec_model/run.py --jobs 16
```

That is 600 (query, mode) runs in 15:59 wall at `--jobs 16` on shad-gpu's 22 cores, at most
7.7 GB per worker. The tail is one task, tpch q9 at `tp4-rowgroup`, whose planned run takes
about 14 minutes alone; the optimized one takes 14 seconds.

## What is here

One folder per component: `engine/` the runtime, `operators/` the pandas-backed operators,
`plans/` an engine plan's text turned into the prototype's tree, `optimizer/` statistics,
estimates, cost and the rules that change a plan. `errors.py` sits at the root, since all four
raise from it. The tests mirror the folders under `tests/`; `harness.py`, `corpus.py` and
`rescan.py` serve more than one and sit in `tests/` itself, and `tests/engine/mocks.py` holds the
driver tests' mock executors.

| File | Holds |
|---|---|
| `engine/layout.py` | `NodeKind`, `KeyDistribution`, `SortOrder`, `BatchLayout`, `PartitionLayout` |
| `engine/batch.py` | the `Batch` value type and `CallStats` |
| `engine/executors.py` | `Executor` plus the seven executor traits and `LaneEvent` |
| `engine/forwarder.py` | `BatchForwarder` and the merge / union / interleave mappings |
| `engine/node.py` | `GpuNode`, `NodeExecutors`, `ExecutorBackends`, `BackendSelector` |
| `engine/plan.py` | heights, left-to-right order, whole-tree structural validation, which limit lowering applies |
| `engine/accounting.py` | `ResidentAccountant` — the resident formula, the cached-delta executor total, and the budget trip (`ResidentBudgetExceeded`) |
| `engine/limit.py` | `RowInterval`, `RowRange`, and the per-batch decision behind the two lowerings |
| `engine/runtime.py` | per-node queue state and one lane's view of its inputs |
| `engine/single_partition_driver.py` | one lane of one lane-scoped node |
| `engine/partitioned_driver.py` | everything cross-partition, and the events it tells the scheduler: a readiness index, a join lane leaving build, a limit satisfied |
| `engine/scheduler.py` | which node runs next — `executor/driver/scheduler.rs`'s interface: events in, `pick() -> Pick{run, prefetch}` out, holds as counters; with the `prefetch` policy the pick also names unheld source lanes to fetch ahead |
| `engine/adaptive.py` | the adaptive loop's events: a driver that reports each join's `BuildDone` before it sets a build (true rows, bytes, key NDV) — and `BuildExceeds` where a build passes its cap — to a hook that may stop the run with the build left queued; a stopped run's `extraction` — the build just done and those set but not probed, where nothing else has started |
| `errors.py` | `PlanError`, `DriverError`, `ResidentBudgetExceeded`, `EnginePlanFormatError`, `StatsError` — what each failure class means |
| `operators/frame.py` | the pandas batch, and the rules that keep pandas inside cuDF's vocabulary |
| `operators/expressions.py` | the expression IR — what `cudf::ast` accepts, nothing more |
| `operators/aggregates.py` | an engine plan's aggregates run as the plan decomposed them (`PlanAggregate`): its init and merge calls over expressions — Welford's `m2` and `merge_m2` among them — grouping sets with DataFusion's id, and its `final` expressions |
| `operators/source.py` | the loader and the row-group → (partition, batch) policy; an engine scan's real row groups as row ranges, a batch's bytes fetchable ahead of its decode (read once either way); a memory source's lane, its one kept batch |
| `operators/exec_ops.py` | filter, project, sort, an engine plan's aggregate, unload |
| `operators/accumulators.py` | coalesce-all, limit, re-batch, an engine plan's aggregate-batches, accumulate-and-sort, merge-sorted |
| `operators/partition_ops.py` | the hash scatter |
| `operators/join_types.py` | `JoinType` in the fbs vocabulary, and the capability matrix as a function both join backends read |
| `operators/joins.py` | the pandas join backend — nine hash-join types, cross, nested loop; their positional forms for engine plans, and what each type emits before its projection |
| `operators/cudf_calls.py` | the cuDF calls `cpp/src/operators/join.cpp` makes, modelled: joins that return gather maps, `gather` with its out-of-bounds policy, `scatter`, `apply_boolean_mask` |
| `operators/recipe.py` | the FlatBuffers node structs, the handle registry with consume-on-use, and the C++ that reads them |
| `operators/recipe_join.py` | the second join backend: answers every call by emitting fb nodes and making `execute_node` calls |
| `operators/nodes.py` | `GpuNode` implementations wiring the operators into plans; the builders an engine plan's nodes need — its scan, positional joins, plan-level aggregates — and the layouts they declare |
| `operators/validation.py` | the checks a node's `_validator` is composed from with `all_of` — what it needs of its children's layouts. The method is abstract on `GpuNode` (`node.py`) and implemented once, on `PandasNode` (`operators/nodes.py`), which just runs that validator |
| `operators/injection.py` | `LayoutInjector` — rewrite a plan's partitioning, batching and hash placement |
| `plans/engine_plan.py` | the engine planner's `<mode>.plans.txt` goldens read back into trees, and `plan_text` writing one back — node lines and refusals, field values verbatim, the schema as (name, type) pairs with names unquoted; `GpuMemorySource`, the prototype's own kind the engine does not have yet, is read too |
| `plans/engine_expr.py` | the engine's expression text parsed back into a syntax tree and rendered again (`expr_text`) — `plan_text/expr_text.rs` both ways; a literal stays text, since the rendering does not print its type |
| `plans/engine_ir.py` | a node's expression fields in the expression IR — ordinals resolved to frame names (`name@ordinal` where a schema repeats a name), each literal typed by what it meets |
| `plans/engine_nodes.py` | an engine plan tree built as a prototype plan, one engine node to one prototype node; each output named by its node's schema; a hash join's fanout from the estimate, where one is given; a `GpuMemorySource` from the frames `tables.materialized` holds, laid out as they were made |
| `plans/engine_run.py` | a prototype run of an engine plan — `run_plan`, read as one answer or by lane, under `CORPUS_BUDGET` for a corpus query — rendered as the engine renders its own (`cpu.txt`): node lines, `output_rows`, per-lane `in_rows`/`batch_rows` |
| `plans/tables.py` | the generated parquet tables as an engine plan's scans read them, decimals as float64 and dates as `datetime64[ns]`; a scan's columns read once per `ParquetTables`, since a replan builds every scan again |
| `plans/goldens.py` | where the engine's goldens and this prototype's live, and the `== <query>` sections every golden is cut into |
| `plans/cost_model.py` | the engine's cost function (`test_support/cost_model.rs`) over a run in the `cpu.txt` format, by `testdata/cost_model.conf`, read, summed and rounded as the Rust is; `GpuMemorySource` free, outside the conf |
| `plans/answers.py` | two answers to one query compared as strictly as SQL allows — a multiset, the ORDER BY columns by position, money within a tolerance — and the queries whose LIMIT leaves which rows open |
| `optimizer/dynamic_filters.py` | dynamic filters: which joins can prune a fact scan's row groups (filtered build, key lifted to an ordered scan column), the probe plan — the build side itself, whose lanes the main plan then reads as a memory source, so the side is read once — the host reducer, and the replan that re-maps each pruned scan as the engine's partitioner would |
| `optimizer/stats.py` | table statistics for the estimator — NDV and strings' mean length from the committed sidecar (`testdata/gen_stats.py`), rows, min/max and nulls from the footer over the row groups a scan reads; a sidecar that is missing or no longer matches its file is refused |
| `optimizer/observed.py` | the NDV of an intermediate result: a materialized build side's from per-lane counts — exact where the lanes are hashed on the counted columns, bounds otherwise — and a stream's cap |
| `optimizer/estimator.py` | filter selectivity from NDV, min/max and nulls — equality as 1/NDV, a range as the share of evenly spaced values, bounds on one column merged, DuckDB's 20% where statistics cannot speak |
| `optimizer/cardinality.py` | every node's row estimate over an engine plan — column lineage and NDV carried up, a join over containment and the key domain its sides share, semi / anti / outer by the share of rows that match; a memory source is `known`, `measured` from its frames — true rows, NDV clamped into what the lanes' counts bound |
| `optimizer/cost.py` | C_out in bytes — each join's estimated rows times the width of what it passes on, a string by its mean length from the sidecar |
| `optimizer/join_order.py` | what DPhyp is asked of a set of a cluster's relations — its rows by `cardinality`'s formulas in one canonical order, and its C_out bytes — the orientation pass: each join's cheaper build, the build copied per probe batch while #152 is open, a probe shuffled onto the join's lanes cut once per lane — and `optimize`: every cluster of a plan reordered by DPhyp on C_out, oriented and disassembled |
| `optimizer/replan.py` | the adaptive loop: a build further than `threshold` times off its estimate stops the run, every build made so far becomes a memory source, the plan is optimized again with their sizes known and run by a new driver; a replan that would run started work again is refused. `WithMaterialized`, the tables such a plan reads its memory sources from |
| `optimizer/call_cost.py` | the cost of one device call by kind, `fixed + slope × volume`, fitted from the benchmark's calibration record — what lanes and batch size would be chosen by where C_out sees no difference; no rule reads it yet. The record is fetched into `RECORD` (above) |
| `optimizer/dphyp.py` | DPhyp from the `peacockdb-dphyp` crate through its C ABI (`ctypes`): the library by `PEACOCK_DPHYP_LIB`, refused without it; the tree as disassembly takes it |
| `optimizer/multijoin.py` | a cluster of inner joins as DPhyp takes it in — relations, edges between relation masks, every column an identity `(relation, ordinal)`; `clusters` finds them all, nested ones included |
| `optimizer/disassembly.py` | a join order back into an engine plan — keys, residual and projection from the MultiJoin's identities, wiring as the translator derives it; `baseline` is the order the plan already has |
| `optimizer/pipeline.py` | the optimizer end to end over one plan — dynamic filters, then DPhyp and orientation with the probes' builds known, then the adaptive run — and its report; each mode's lanes and batching, by name |
| `optimizer/report.py` | what the optimizer did to a plan, as data: dynamic-filter candidates and pruned scans, each DPhyp call (relations, edges, the sets priced through the cost callback, its budget, the tree, the chosen joins and the plan order's C_out, the flips), each replan and refusal, the plan text before and after; `Fired`, how often each rule acted |
| `optimizer/report_text.py` | that report as a `.optimizer.txt` section |
| `run.py` | the corpus through the optimizer, [above](#runpy-the-corpus-through-the-optimizer) |
| `cost_report.py` | each (query, mode)'s costs as a section of `<mode>.costs.txt` and back, and `cost_report.html` rendered from those files |
| `tests/corpus.py` | the tests' default budget, the generated datasets found or the generator named, DuckDB's answer to a query's text, and an engine `cpu.txt` golden's rows per node |
