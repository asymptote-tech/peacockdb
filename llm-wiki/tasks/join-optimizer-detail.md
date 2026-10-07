# join-optimizer — detail

## Where the task starts

`ENS-optimizer`, PR #165, three commits over `ENS-plan-text-names` (PR #166, the plan-text
fixes #236 and #237):

- `peacockdb-dphyp` — DPhyp as a dependency-free crate with a C ABI, six tests in CI.
- benchmarks — tpch q1 and q6 at every mode, q17 at `tp1_single`, their sf40 trees. Their
  per-call record (174,741 lines, 17 MB) is not in git: it is
  `s3://calibration/tpch.sf40/records.tsv`, sha256
  `3aab8a668a606f8bf523b6ff688427c278473f09feb530f939816878446e2a7c`.
  `testdata/calibration/records.tsv` in git is still the q6/q19 record.
- the optimizer in `scripts/exec_model`, its statistics sidecars and `gen_stats.py`, CI steps.

## What each task rests on

- Task 1. tpch's `*_embedding` columns differ between `--embeddings synthetic` and
  `external`; between two such hosts only the `part`/`partsupp` fingerprints and those
  columns' NDV differ. The stock columns' NDV agree.
- Task 2. `scripts/exec_model/tpcds.plans.txt` last changed because a union of single-batch
  branches now declares `single_batch`; tpch has no union, so its file did not move.
- Task 3. `operators/` grew to run the engine's plans: columns by position, each join type's
  own output, the planner's aggregate forms (ROLLUP, Welford states, final expressions), scans
  over real row groups, `IS NULL` and casts over NULL, an integer shuffle key hashed as the
  device hashes it.
- Task 6. DPhyp's input is a MultiJoin — not a plan node but a cluster of adjacent inner joins,
  its relations and key edges — and its output tree is rebuilt into `GpuHashJoin`s with the
  translator's wiring.
- Task 7. The bucket is reachable with the credentials on shad-gpu
  (`/home/info/bin/aws`, the endpoint `upload_datasets.sh` uses).

## Known state

- `test_cardinality_corpus` compares a per-join golden since Task 5; q64's misses are a recorded
  limit, not a threshold (Task 5 below).
- Green otherwise: rust-only, DPhyp, the cheap tier (34 files), and q64 in the corpus answer,
  reassembled and optimized suites at every mode.
- CI run `37233204588`'s GPU job failed only because a neighbour held the card (the pool could
  not be built); its re-run passed.
- tpch q8 at `tp4-rowgroup` is green since Task 3a: 137,760 steps and 169,049 calls under a
  derived cap of 169,049. `exec-model-corpus.yml` runs `tp4-single` only, so CI does not run that mode.
- The record is out of git since Task 7: keyed by content in the `calibration` bucket,
  published by `build-test-shadgpu.sh --pull-benchmarks`, pinned by
  `testdata/calibration/records.sha256`, fetched by `scripts/calibration/fetch_record.sh`; the
  measured call-cost check runs where it is fetched.

## Hosts

- Local: `/usr/bin/python3` has pandas and pyarrow; the linuxbrew `python3` does not.
- shad-gpu: shared with work outside this repo. Corpus suites run from a scratch copy in
  `/tmp/peacock-opt` with its own Python 3.13 venv, DuckDB 1.5.4 and DPhyp built on the host
  (its glibc 2.31 is older than a local build needs). `/home/info/peacockdb` is the gate's
  checkout and is not touched. tpcds exists there at sf1 only.

## Task 1

- The fingerprint is `stats.fingerprint(metadata)`: the rows plus a sha256 over each flat
  column's per-row-group null count and raw min/max. `gen_stats.py` imports it, with
  `flat_columns`, from `scripts/exec_model/optimizer/stats.py`, so the writer and the reader share one
  definition. A flat column is a footer path without a dot; a list leaf is `x.list.element`.
  The sidecar format is 2.
- Per row group, not whole-file aggregates: measured, external and synthetic tpch sf1 have the
  same row groups (part 2, partsupp 7, lineitem 49) and the same stock-column statistics, so
  the stricter hash still matches across modes. HEAD's `gen_stats.py` over the external data
  differed from the committed file only in part/partsupp's fingerprints and the three
  `*_embedding` NDVs. After the change, every other NDV and composite count is unchanged.
- The fingerprint cannot catch every stale NDV: data can change with no row group's null
  count, min or max moving (ps_tag with ten tags between the same extremes). So
  `test_stats_sidecar` recounts both sidecars with `gen_stats.py --out` in dataset-matrix
  and compares byte for byte, with the DuckDB the generator step unpacked (`DUCKDB` in the
  step's `env`). That also runs `gen_stats.py`'s import of `stats.py` in CI. Because of it,
  `test_stats_embeddings` keeps only the external case.
- Decimals: pyarrow before 23 cannot decode a decimal stored as INT32/INT64 (22 fails, 23
  works), and DuckDB stores them so. This was red in CI too, not only locally: dataset-matrix's
  image has an older pyarrow, `test_stats_sidecar` failed at `Statistics.column`, and `set -e`
  skipped `test_cardinality_corpus`. The reader now builds decimal bounds from
  `min_raw`/`max_raw` and the column's scale. The value is built from a string, because
  `Decimal.scaleb` rounds past the context's 28 digits. It matches pyarrow 25 on all 844
  decimal chunks of both datasets. Pinned by `test_a_decimal_s_bounds_are_read_whatever_its_storage`.
- `test_stats_embeddings.py` is run by hand. It links the generator into a temp dir and
  generates there (about 2 min locally), so `testdata/tpch.sf1` is left alone:
  `TMPDIR=/build/peacock/tmp DUCKDB=/build/peacock/duckdb-1.5.4/duckdb
  PEACOCK_EMBEDDINGS_CACHE=/build/peacock/embeddings/testdata/embeddings-cache
  /usr/bin/python3 scripts/exec_model/tests/optimizer/test_stats_embeddings.py`.
  The cache was fetched by a symlinked `fetch_embeddings.sh`, which caches beside itself.
  It is not linked into the tree: the root `.gitignore` entry `testdata/embeddings-cache/`
  matches a directory only, so a symlink would show as untracked.
- Local tools: no DuckDB CLI is on the PATH; `/build/peacock/duckdb-1.5.4/duckdb` is the
  release build CI uses (08e34c447b). `/usr/bin/python3` has pyarrow 18.1;
  `/build/peacock/venv-exec-model` has pyarrow 25.0.1 and pandas 2.2.2.

## Task 2

- Gone: `tests/plans*.py` (15 files), `tests/plan_helpers.py`, `test_tpch.py`,
  `test_tpch_corpus.py` (22 cases), `test_tpcds.py` (71), `scripts/exec_model/{tpch,tpcds}.plans.txt`.
- `test_tpch.py` was not a lowering. Five of its 19 cases test what stays, so they were ported
  into the cheap tier over the committed `testdata/tpch.minimal`:
  - The injector's four guards went to `test_injection.py`, with its `aggregate_plan`,
    `shuffled_join_plan` and `streamed_join_plan`.
  - The budget claim went to `test_accounting.py`.
  - Their customer and nation columns are sf1's exactly (`pyarrow.Table.equals`), and the two
    measured peaks are unchanged: 1,280,000 and 2,400,000 bytes.
  - The other 14 cases re-ran the same operators over real rows: what `test_end_to_end` and
    `test_join_capability` cover, or the lowered corpus did.
- Each port was shown red:
  - An `apply` that returns the plan unchanged turns three of the injector tests red.
  - No empty batches turns the empty-batch test red.
  - An ignored seed turns the reproducibility test red.
  - Repartitioning any join turns the lane-count test red.
  - For the budget test, dropping only `CoalesceAllBatches`' held bytes stays green. The
    collected peak is the coalesced output batch, counted in flight. The test pins that a
    collected table is counted in full and that a stream is bounded per batch.
  - A `release` that does not subtract fails it at the streamed assertion. `hold`/`release`
    and the accumulator's held bytes all inert fail it at `raises`.
- Counts after: cheap tier 35 files, 363 cases; Python 802.
- Rust in `build-test.md` was corrected to the code: 1875, not 1861. Grand total 2771.
  - Rust-only `--list`: `--lib` 606, because `wire/tests.rs` has 24 where the page said 23.
    `cost-report` has 36, not 37.
  - From source:
    - "Operator harness" stays 334: 453 `operator_case!` and 15 `#[test]` less the 134 of
      "what the device holds" (the `#[test]` at `coverage.rs:30` is the macro's template).
    - `peacock_gpu_benchmarks` has 8 harness tests and 12 `bench_` cases (q6 and q1 at five
      modes, q17 and q19 at one), so 20, not 11. The header was not moved when the case list
      grew.
  - The block headers now match their rows.
- No surviving suite imported a lowering or `plan_helpers.py`. Only the deleted files imported
  them.
- `plan_text.py` is gone on purpose. It rendered only the deleted lowerings' goldens. The plan
  diff Task 6 needs is in the engine's own golden format, which this renderer was not.
  `join-optimizer.md`'s layout still lists it under `plans/`.
- `tests/corpus.py` lost what only the lowerings called: the table reader and cache,
  `schema_reader`, `build_join`, `run_layouts`/`PCK_LAYOUT`, `check_plan`, `schema_of`,
  `agg_schemas`, `same`, `in_order`, and the `PCK_BACKEND` switch. That switch re-ran the
  lowered corpus on the recipe backend, which refuses the engine's positional joins. So
  exec-model-corpus.yml lost its `backend` input and its unread `SHARDS` env. Its shard rule
  names the matrix list and the `/3`.
- `scripts/calibration/tests/harness.py` is outside the spec's scope but was edited. It is a
  copy of the exec-model harness, kept identical below its header, and the comments changed in
  one had to change in the other.
- `test_cardinality_corpus` is still red on tpcds (median 1.29, 69.5 % within 2×). That is the
  q64 state above, not this change: `cpu_rows`/`_sections` are AST-identical to HEAD → Task 5.
- For Task 3:
  - Nothing in `operators/nodes.py` or `aggregates.py` lost its last caller by name, so the
    residue has to be found by coverage.
  - `LayoutInjector` and `nodes.Recipe` now serve `test_join_capability.py` and
    `test_injection.py`.
  - The recipe backend (`RecipeJoinBackendSelector`, `operators/recipe*.py`, `cudf_calls.py`)
    is reached only from `test_join_capability.py`.
  - `corpus.dataset_dir`'s `tpch.minimal` fallback serves only `duckdb_answer`.
  - The harness's `-substring` exclusion has no CI caller.

## Task 3

- Measured with coverage.py 7.16.2 on shad-gpu: the cheap tier (35 files), and the corpus files
  over the whole corpus — answers and optimized at `tp4-single`, `tp1-single` and `tp4-rowgroup`,
  dynamic filters and reassembled at `tp4-single` (80 shards). Then each function the corpus
  never ran was read. The runners lived in a scratch copy, `/tmp/peacock-opt/t3`.
- Coverage of `scripts/exec_model/*.py` and `operators/` (tests excluded), in statements:

  | | package | corpus reaches | either tier reaches | `operators/` | corpus | either |
  |---|---|---|---|---|---|---|
  | before | 5,612 | 4,316 | 5,429 | 2,296 | 1,418 | 2,191 |
  | after | 5,272 | 4,196 | 5,134 | 2,005 | 1,329 | 1,942 |

  Either tier reaches 96.7 % before and 97.4 % after (`operators/`: 95.4 % → 96.9 %). The corpus
  count falls because the deleted `def` and field lines ran at import. What no test reaches
  after is listed under "Unreached but kept" below.
- Proven after the change. Cheap tier, local and on shad-gpu: 35 files, 349 passed. Corpus on
  shad-gpu, whole corpus under coverage:
  - answers 120 at each of the five modes;
  - optimized 100 at `tp4-single`, `tp1-single` and `tp4-rowgroup`;
  - dynamic filters 58 and reassembled 100 at `tp4-single`.

  All passed except tpch q8 at `tp4-rowgroup`, which fails the same way before the change
  (below). Dataset tier: `test_stats_sidecar` 4/4, and `test_cardinality_corpus` red only for
  q64 (636 joins, median 1.29, 69.5 %).

- Gone. The corpus ran none of it; only its own tests did, or tests that used it as a fixture:
  - The handwritten plans' aggregate. `aggregates.Agg` and its phases: `partial`, `merge`,
    `finalize`, `final`, `single`, `finalize_exprs`, and the grouping-set expansion
    (`partial_over_sets`, `single_over_sets`, `rollup_masks`, `grouping_set_id`). Also
    `exec_ops.PartialAggregateExec` and `nodes.partial_aggregate`/`aggregate_batches`.
    `AggregateBatches` is folded into `PlanAggregateBatches`, which keeps its compaction.
  - The column annotations that aggregate declared. `schema.py`, `GpuNode.output_schema` and
    `PandasNode`'s schema plumbing, and `validation.merges_its_own_partial`/
    `hash_keys_subset_of_groups`. `layout.UniqueKeys`/`UniqueScope`/`unique_keys` were declared
    by those aggregates alone and read by nothing. `KeyDistribution.is_subset_of` goes too.
  - Expression names. Every expression's `alias` field, and every `name()` but `Alias`'s. An
    engine plan names each output through an `Alias` (`engine_ir`), so only the lowerings'
    unaliased columns were named this way. No test reached them. `project` now requires an
    `Alias`; an unnamed expression raises `TypeError`, not the `NotImplementedError` of a
    join refusal. The three refusal tests in `test_join_capability` now carry a `match=` on
    their own message, and each goes red with its refusal removed.
  - Unreferenced: `validation.single_batch_in`, `frame.empty_like`, `node.GpuBackendSelector`.
  - 807 production lines, 33 added. Cases: cheap tier 363 → 349 (`test_operators` 66 → 60,
    `test_end_to_end` 18 → 14, `test_plan` 22 → 18).
- Ported rather than dropped, because they pin what the engine's aggregate does:
  - `test_operators`: the null group, `count(1)` against `count(v)`, a finalizing aggregate
    over no rows, an empty partial's key type, the three compaction-policy tests, the typed
    zero-input batch and the loud one.
  - `test_end_to_end`: grouped, keyless, every corpus function (Welford included), ROLLUP and
    DISTINCT, each at every partitioning config; the accountant's engagement and config
    agreement. They are built with `plan_aggregate`/`plan_aggregate_batches` as the planner
    decomposes them (`aggregate_over`).
  - `test_injection`'s and `test_accounting`'s aggregate plans take the same builders.
  - Dropped with the shape: mean-of-means, the grouping-set unit tests, the single-group
    stddev, the id carried through the sequence, the two count-distinct lowerings, the three
    merge-validation tests and the uniqueness declaration. The planner decides each of these;
    the prototype only runs them.
- Each port was shown red by a mutation of the code it pins:
  - `dropna=True` reddens the null group; `count` as `size` the count test.
  - Skipping `final` reddens the empty finalize; a float empty partial the key-type test.
  - No doubling, no compaction, or dropping the held state each redden their compaction test.
  - An untyped empty batch reddens both empty-batch tests.
  - `merge_m2` without its spread reddens every-function; a zero grouping id reddens ROLLUP.
  - An undeduplicated DISTINCT reddens DISTINCT; `sum` as `max` reddens grouped, keyless and
    config agreement.
  - An `apply` that returns its input reddens the presets test; no budget trip reddens the
    accountant test.
  - The key-type test went red only once it also asserted the empty partial's own dtype:
    `frame.concatenate` drops empty frames, so a concatenation alone cannot show it.
  - Mutations were run without bytecode (`PYTHONDONTWRITEBYTECODE=1`). A same-size edit
    restored within one second otherwise leaves a stale `.pyc` of the mutant.
- What stays, for `design.md`'s operators section:
  - Columns by position. `engine_ir` resolves ordinals to frame names. A join renames its
    inputs to `Positional.joined` and picks its projection by ordinal from the join type's own
    output (`own_output`). It declares its hash where every row keeps the keys unpadded
    (`_join_distribution`). A one-to-one node carries its child's hash to where `sources` puts
    it (`_copied_layout`, `_kept_hash`).
  - Each join type's own output. `HashJoin` covers nine types: per-call emission for the
    probe-local half, a finish pass over `matched` for the build-preserving half, and a
    single-batch probe as the whole join. Null keys are held out unless `null_equals_null`;
    anti and mark stay EQUAL. `probe_schema` pads an outer finish that saw no probe batch, and
    the positional join always passes it. Nested loop is Inner and Left; cross streams. The
    name-based classes are the positional ones' base and the recipe backend's oracle.
  - The planner's aggregate forms: `PlanAggregate`/`PlanCall`. Init calls are
    sum/count/min/max/mean/m2 and merge calls sum/min/max/merge_m2 (Chan's form). Grouping
    sets carry DataFusion's id, first key in the high bit; `final` holds aliased
    expressions. A global aggregate over no rows is one row, no calls is a DISTINCT, the null
    group is kept, and a sum over nulls is NULL. `PlanAggregateBatches` compacts on a doubling
    threshold and emits one typed batch even from no input.
  - Real row groups. `parquet_scan` reads a file's own row groups as the planner mapped
    lanes → batches (`source.row_group_ranges`, `TableSource`, with fetch-ahead). A scan
    limit sits on one lane. `MemorySource` is a materialized build, one batch per lane.
  - Expression and hash details. Comparisons are three-valued; LIKE is translated to a regex
    with two metacharacters; literals broadcast. Cast, round, substring and date_part follow
    the C++. A shuffle key hashes as its SQL value (5.0 lands where 5 does), and null key
    columns are skipped.
  - On purpose, for the tests the spec keeps (iii):
    - `LayoutInjector` needs the synthetic `nodes.scan` with `split_row_groups`,
      `partition_row_groups` and `drain_lanes`, plus `rebatch`/`ReBatchToTarget`, the
      degenerate hash placements, `TableSource`'s empty batches and `Recipe`.
    - `test_join_capability` needs the name-based `hash_join`/`cross_join`/`nested_loop_join`
      with their recipe factories, `recipe*.py`, `cudf_calls.py` and
      `expressions.columns_of`.
    - The driver tests need the mocks.
  - Unreached but kept: interface defaults (`Batch.slice_rows`, `SourceExecutor.prefetch`,
    `GpuNode.row_interval`), refusals and validation raises (`plan._validate_structure`,
    `NodeExecutors`, the join refusals), a nested-loop join without a predicate, and
    `PandasBatch.__repr__`.
- Found on the way, not this task: tpch q8 at `tp4-rowgroup` fails `test_engine_answers` on
  this branch's base as well. It needs 137,760 driver steps against `DEFAULT_MAX_STEPS`
  (100,000). With the cap raised it answers, matching DuckDB, in 486 s on shad-gpu.
  `exec-model-corpus.yml` runs `tp4-single` only, which is why CI never saw it.
- For Task 4: `schema.py` no longer exists. The impl plan's Task 4 still names it.

## Task 3a

- The cap is `PartitionedDriver.step_cap()`, the calls the run owes so far; `run()` trips when
  the steps or the calls (`calls`, counted in `_record`, prefetches excluded) pass it. Every
  call is owed by a source lane's batch (`SourceExecutor.max_batches`, new and abstract), by a
  batch queued below the root (one call takes it), or by a readiness index's closing call
  (`_readiness_count` is exactly that count), each before or in the step that makes the call.
  So steps ≤ calls ≤ the cap for any run that progresses, and a run ends with calls equal to
  the cap when every source returns all it declares (q8: 169,049 both), below it otherwise. README, "The step cap".
- Steps alone miss over-production: one step runs every lane, so a source producing past its
  count over two lanes owes four calls for every three steps and never trips. Steps catch a
  stall (no calls); calls catch over-production.
- Why not the plan's shape. Each `GpuEmitPartitions` turns a batch into one per lane, and q8
  stacks six shuffles on its probe path. (source batches + lanes) × nodes is 16,002 for q8,
  which takes 137,760 steps; a static bound that holds would be ×4 per stacked shuffle, too
  loose to trip in time. So the batch term grows as batches are queued.
- When a source's count is not known before the run: a lane's `max_batches` joins the cap when
  its executor is first made, which is at its first call (or at `__init__` when reading ahead).
  No eager construction, because a `MemorySource` build side's resident bytes would then show
  in `AdaptiveDriver._accumulated` before its first step. Memory sources declare 1; a
  `TableSource` its batches plus its injection budget when it injects empties; a replan is a
  new driver with its own cap. `AdaptiveDriver` inherits the check; `run_adaptive` passes no
  cap. Nothing else called `run(max_steps=…)`, so the parameter went.
- The Rust driver keeps a fixed `DEFAULT_MAX_STEPS` of 1,000,000
  (`executor/driver/partitioned.rs`), outside this task's files. Whether an sf40 row-group run
  reaches it is not measured.
- Tests in `test_partitioned_driver.py`, each red against its mutation (files restored after):
  - An honest run three shuffles fragment takes 513 steps and passes; a `step_cap` of its
    shape alone, (8 + 27) × 9 = 315, trips on it. Red under a cap of 100, and under a cap
    without the queued-batch term (15 other driver tests go red with it).
  - A source declaring two batches that never ends raises `DriverError`, on one lane and on
    two. Red with the source term unbounded, with the check removed, with each step owing a
    call, and — the two-lane case — with steps checked and calls not.
  - A driver whose `_run` does nothing raises at step 23, past 22 owed. Red with the check
    removed and with each step owing a call.
  - Each never-ending case asserts out at 10,000 calls, so a red run fails instead of hanging.
  - Counting prefetches as calls false-trips 19 honest driver tests: the exclusion is needed.
  - `test_parquet_scan`: a `TableSource` injecting empties (probability 1) over three batches
    returns 7 = `max_batches()`; red with the injection-budget term dropped.
- Proven with steps alone checked. Cheap tier local and on shad-gpu: 35 files, 352 passed. On
  shad-gpu, from `/tmp/peacock-opt/t3a/work` (the worktree's `scripts/exec_model`, testdata linked to
  `/tmp/peacock-opt/repo/testdata`, identical to the worktree's by `rsync -c`):
  `test_engine_answers` at `tp4-rowgroup` in 40 shards, 120 passed, q8 included, 16 min wall;
  at `tp4-single` shards 0, 13 and 26 of 40, 9 passed. q8 alone: 137,760 steps, cap 169,049,
  483 s.
- Proven, with the call check. Cheap tier local: 35 files, 353 passed. On shad-gpu, same
  scratch copy re-synced: answers at `tp4-rowgroup` 120 passed and at `tp4-single` 120
  passed, dynamic filters (memory sources) at `tp4-single` 58 passed, 40 shards each. q8
  alone: 137,760 steps, 169,049 calls, cap 169,049, 666 s beside the shards.

## Task 4

- Layout. Files were moved with `mv`; the coordinator stages them, and git pairs the renames.
  - `engine/`: plan, node, layout, runtime, batch, executors, accounting, forwarder, limit,
    single_partition_driver, partitioned_driver, scheduler, adaptive.
  - `operators/`: unchanged.
  - `plans/`: engine_plan, engine_expr, engine_ir, engine_nodes, engine_run. There is no
    `plan_text`: Task 2 removed it, though the spec's layout still lists it.
  - `optimizer/`: stats, observed, estimator, cardinality, cost, call_cost, multijoin,
    disassembly, join_order, dphyp, dynamic_filters, replan.
  - Root: `errors.py` and `__init__.py`. `errors.py` stays at the root because every component
    raises from it: engine (plan, accounting, both drivers, adaptive), plans (engine_plan,
    engine_expr, engine_ir), optimizer (stats, cardinality) and operators (validation).
  - Each new folder has an `__init__.py`. The production folders get a one-line docstring; the
    test folders get an empty file, as `tests/__init__.py` is.
- Tests, placed by the module they test:
  - `tests/engine/`: accounting, adaptive, determinism, limit, partitioned_driver, plan,
    scheduler, scheduling, single_partition_driver, and `mocks.py`, whose users are all here.
  - `tests/operators/`: operators, end_to_end ("whole queries through the real operators"),
    join_capability, injection, parquet_scan.
  - `tests/plans/`: engine_plan, engine_expr, engine_ir, engine_nodes, engine_run,
    engine_corpus, engine_answers.
  - `tests/optimizer/`: stats, stats_sidecar, stats_embeddings, observed, estimator,
    cardinality, cardinality_corpus, cost, call_cost, multijoin, disassembly,
    disassembly_corpus, join_order, dphyp, dynamic_filters, optimize, replan, memory_source,
    engine_dynamic_filters, engine_reassembled, engine_optimized.
  - Left in `tests/` because more than one folder uses them: `harness.py` (all), `corpus.py`
    (all four), `rescan.py` (four engine files and `operators/test_end_to_end`).
  - Tests import helpers from other folders' tests. `engine/test_accounting` imports
    `aggregate_over` from `operators/test_end_to_end`. `engine/test_adaptive` and several
    optimizer tests import from `plans/test_engine_nodes`. The optimizer corpus files import
    `MODE`/`ROOT` and the rest from `plans/test_engine_answers`.
- Did not fit cleanly. Code stays as it was; a later task may revisit these. The layout implies
  optimizer → plans → engine, with operators implementing engine's traits. Two modules import
  against that direction:
  - `engine/adaptive.py` imports `plans` (`engine_ir`, `engine_expr.parse_columns`/
    `parse_join_keys`, `engine_plan.EngineNode`). It names a build's keys from the engine plan.
    It also imports `optimizer.observed` (`NdvBounds`, `observed_ndv`) to measure a finished
    build's key NDV.
  - `plans/engine_nodes.py` imports `optimizer.cardinality.estimate` and
    `optimizer.stats.Statistics`: a hash join's fanout comes from the estimate.
  - Every other cross-folder import points the expected way: operators → engine, plans → engine
    and operators, optimizer → plans, engine and operators.
- What changed in each moved file, besides the move. Checked against `git show HEAD:<old path>`:
  - Relative imports.
  - Each test header: `parents[3]` → `[4]`, `__package__` gains the folder, and the comment's
    path gains the folder.
  - Paths from `__file__` to the repo root, one level deeper:
    - `optimizer/call_cost.py`'s `RECORD`: `parents[2]` → `[3]`.
    - In the tests, `parents[3]` → `[4]`: `GOLDENS` in the four plans files that read the
      goldens, `ROOT` in `test_engine_answers`, `TABLES` in `test_injection`, `test_accounting`'s
      `tpch.minimal`, and `JOIN_CPP` in `test_join_capability`.
  - Unmoved files changed only in their imports: `operators/*` (they import engine), and
    `tests/corpus.py` and `tests/rescan.py`.
  - `tests/harness.py` changed one docstring example's path. `scripts/calibration/tests/harness.py`
    took the same edit, so the two stay identical below their headers.
  - Lines changed per folder (diff `<`/`>` lines, so 2 = one line replaced):
    | folder | files | lines changed |
    |---|---|---|
    | engine | 13 | 18 |
    | plans | 5 | 28 |
    | optimizer | 12 | 58 |
    | tests/engine | 10 | 192 |
    | tests/operators | 5 | 116 |
    | tests/plans | 7 | 130 |
    | tests/optimizer | 21 | 338 |

    The most in any one file is 34 lines (`test_accounting`).
- Outside the package:
  - `testdata/gen_stats.py` imports `scripts.exec_model.optimizer.stats`.
  - `pipeline.yml`'s cheap loop uses `shopt -s nullglob globstar` and
    `tests/**/test_*.py`; `**` also matches `tests/` itself. Every `elsewhere` name and the two
    dataset-matrix lines gained their folder.
  - `exec-model-corpus.yml`'s file list gained the folders.
  - `README.md`: the run loop, the pytest example, a paragraph on the folders, and the module
    table, prefixed and grouped by folder.
  - `build-test.md`: the seven test links and two globs.
  - The path in `tickets/system-hardening.md` #238.
- Counts, before → after, identical file by file:
  - Cheap tier, rendered from `pipeline.yml`: 35 files, 353 passed → 35 files, 353 passed.
  - Every test file, counted by import: 42 files, 739 cases, at `tp4-single`, `tp1-single`
    and `tp4-rowgroup`. pytest `--collect-only` also finds 739.
  - The corpus files: answers 120, dynamic filters 58, reassembled 100, optimized 100.
  - Dataset tier: `test_stats_sidecar` 4/4. `test_cardinality_corpus` is 2 passed, 1 failed,
    the same as before: q64, 636 joins, median 1.2912, 0.6950 within 2×.

## Task 5

- The golden. `scripts/exec_model/testdata/goldens/<bench>/tp1-single.cardinality.txt`, the tree
  `run.py`'s `<mode>.cpu.txt` and `.optimizer.txt` will share. One line per join: query, its
  ordinal among the query's joins in pre-order (`n`), kind/type, `joins` (the joins in its
  subtree, itself included), true rows, estimate, q-error, the join keys by the plan's names.
  Lines sorted by query (natural order) and ordinal. A six-line header: the join count, median
  and share within 2×, the queries the engine did not run at the mode (tpch q11 q22 scan-limit,
  tpcds q24 q54), those whose run's tree differs from the plan (none), a gloss of `n` and
  `joins`, the column names. `UPDATE_CANONICAL` set rewrites both files, as the Rust goldens
  do; a mismatch fails with a zero-context unified diff ending in how to regenerate, so an
  estimate that moved is a named `-`/`+` pair and a new query is `+` lines. Both shown: the column-equality experiment below printed its eleven
  moved joins by name, and a golden with q3's lines removed printed them as two `+` lines.
- One mode, tp1-single, as the old test's default. Every join the other four modes run as planned
  is one of tp1-single's by query, kind, type, keys, `joins` and true rows: `test_cardinality_modes`
  holds it, from the plan goldens and `-mini.cpu.txt` alone, so it is in the cheap tier (2 cases;
  shown green in a copy with no sf1). Shown red in a temp copy: a tp4-single join's
  `output_rows` one higher, and a tp4-sized join made Left, each failed it naming the join. The
  estimates were also equal when measured, which the guard does not check — it needs the data.
  The tp4 modes have fewer joins:
  their `cpu.txt` leaves more queries unrun (`skipped: not enabled at this mode`; tpcds 10 against
  2). Five files would print every estimator change five times.
- Formatting. Estimates `.1f`, q-error `.2f`, fixed widths but the query's, which is the bench's
  longest name (21 for tpch's `nested-loop-left-join`), so a line moves when its join does or a
  longer name joins the corpus. The estimator is pure float arithmetic over footer and sidecar values; the only libm
  call is `pow` in Cardenas, and a last-bit difference there cannot reach the first decimal.
  Byte-identical and green on four: `/usr/bin/python3` 3.12.3, pandas 2.2.2, pyarrow 18.1;
  the venv with pyarrow 25.0.1; CI's `rapidsai/base:25.02-cuda12.0-py3.12` (3.12.9, pandas
  2.2.3, pyarrow 18.1, run by docker over the same data); shad-gpu 3.13.15, pandas 3.0.6,
  pyarrow 25.0.1 (`/tmp/peacock-opt/t5`, synced with `rsync --delete`).
- q64: where the error is born. The spec's guess, correlated filters across repeated tables, is
  not what the numbers show. Each half (store_sales for 1999, then 2000; 18 joins each) misses
  by three independence and default assumptions, none of them a correlation:

  | node | estimate | true | why |
  |---|---|---|---|
  | `cs_ui` HAVING `sum(cs_ext_list_price) > 2 * sum(refunds)` | 3,572 (0.20) | 17,157 (0.96) | an aggregate against an aggregate: DuckDB's default 0.2. ×4.80 under |
  | `cs_ui ⋈ (store_returns ⋈ store_sales)` | 57,142 | 279,021 | carries it: q 4.88 |
  | `d_year = 1999`, store, customer, two dates, `cd1` | 11,373 throughout | 53,265 → 48,301 | each FK join keeps every row; the truth loses 0.7–3.5 % per join to NULL foreign keys (×0.91 over five) |
  | `cd2` with `cd1.cd_marital_status != cd2.cd_marital_status` | 2,275 (×0.20) | 38,203 (×0.79) | a column against a column: the default 0.2. Five values, so independence says 0.80 — the truth, so no correlation. ×3.96 under |
  | promo, `hd` ×2, `ca` ×2, `ib` ×2 | 2,275 | 38,096 → 37,840 | q 16.6 (1999: 4.88 × 0.959 `d_year` × 0.907 NULLs × 3.955 marital × 0.991 tail) and 17.1 (2000: 4.88 × 0.972 × 0.912 × 3.982 × 0.990) |
  | item: six colours and `i_current_price` in [65, 74] | 105.6 | 12 | colours 6/92 (1,174; true 1,188); price as uniform over 0.09–99.99 is 9.0 % (NDV 2,688), true 0.92 % (165 items): ×9.8. The two are independent (1,188 × 165 / 18,000 = 10.9). ×8.8 over in all: skew, no histogram |
  | `ss_item_sk = i_item_sk`, the aggregate above | 13.3 | 37 (1999), 2 (2000) | the item overestimate half cancels the 17× under: q 2.77 and 6.67 |
  | the root, `cs1 ⋈ cs2` on item, store name, zip, `cnt2 <= cnt1` | 2.7 | 2 | q 1.33 |

  q64's 37 joins: median q-error 4.88, 5 within 2×. The rest of tpcds: 599 joins, median 1.21,
  73.3 % within 2×.
- Tried and not kept, each a correct rule locally that loses elsewhere:
  - `a = b` / `a != b` over two columns with statistics as one value of the larger NDV, among
    rows NULL in neither (`estimator._Estimate`). Locally right: q64's residual 0.80 against a
    true 0.79, q46's `bought_city != ca_city` 0.94 against 0.93. tpcds: median 1.29 → 1.26,
    69.5 → 70.0 % within 2×, joins past 10× 69 → 55, 22 better, 4 worse; q64's 37 joins median
    4.88 → 4.27; q16 13.6 → 3.4, q94 9.5 → 2.4, q95's three 3.9–4.2 → 1.04–1.06. But it unmasks
    what the 0.2 was cancelling: tpch q21's anti join 2.82 → 4,141 (estimate 0.9 rows: `1 −
    matched × residual` with a residual near 1 leaves none, while the truth keeps 5.7 %: it is a
    self-join, each row matches itself and `l_suppkey != l_suppkey` always rejects that pair,
    which the independence model cannot see); q46 and q68's top joins 2.5 → 11.9 and 12.2 (the aggregate under them, keyed on
    `ss_ticket_number` with three columns it determines, is estimated at its input, 123 k
    against 10.7 k); q64's root 1.33 → 5.34 and its 2000 item join 6.67 → 26.7 (the price
    skew). tpch's geometric-mean q-error 1.97 → 2.14.
  - NULL foreign keys in an equi-join's rows (× each key's non-NULL share on both sides). tpcds
    86 joins worse, 64 better, geometric mean 2.373 → 2.382, median 1.29 → 1.25; 27 of q64's
    joins worse, its middle ones 16.6 → 26.4, because they are under already. tpch unchanged. q93 10.7 → 1.03.
- So the limit, for `design.md`: the estimator has no histograms, so a skewed column's range is
  read as uniform (q64's price, ×9.8); a comparison of two aggregates, or of two columns, is
  DuckDB's 0.2 whatever the NDVs say; and a semi or anti join's residual scales the matched
  share as if each row had one partner. Fixing the second alone exposes the third and an
  aggregate's NDV under functional dependence (q46, q68). The estimator is unchanged by Task 5,
  so no corpus suite was re-run for it.

## Task 6a

- `.optimizer.txt` does not list every set DPhyp priced, which the spec's "the estimated sets
  it priced" would read as: per call it prints the relations, the edges, `priced N sets`, the
  chosen tree's joins with their rows and cost, and that tree's C_out beside the plan order's.
  The priced sets were 97 % of q64's section (28,663 of 29,572 lines at tp4-single), most of
  them from calls the budget stopped, and they move by thousands of lines on any estimator
  change. `JoinOrder.priced` keeps every set.

- `run.py --bench --query --mode --jobs [--out]`, every filter optional (both benches, every
  query of the plan golden, all five modes, one job). One task per (query, mode) over `--jobs`
  `ProcessPoolExecutor` processes runs the plan as planned, then
  `optimizer/pipeline.run_optimized`, holds the optimized answer to the planned one
  (`plans/answers.same_answer`), and returns a `QueryRun`: both runs in the `cpu.txt` format (so
  6b can price per-node rows and bytes), each probe plan's run and each stopped run the same way,
  wall seconds of the two, the report. The parent alone writes, after every task is done: one
  writer, so no lock. `main` checks the sf1 tables and loads DPhyp (`preflight`) before any work;
  `generate` is the pool and the writing.
- Merge, as the engine's goldens do (`corpus_golden.rs::merged_text`). Without `--query` a run
  owns its files (`Regeneration.WHOLE`): sections no query accounts for go. With it
  (`SECTIONS`) only its sections change; the rest stay byte for byte, a stale one included. A
  fresh file from a filtered run would drop every other query, and the full run is the one that
  takes an hour. A file the run made no section for is not touched.
- Corpus order is the engine's registry order, read from `<mode>-mini.cpu.txt`; the plan golden
  is sorted by name. The 18 tpcds queries the planner refused have no section there. Each gets its
  refusal's first line as `skipped:`, placed after the query that sorts before it. That is
  natural order for tpcds, whose registry is q1..q99. The full text can be DataFusion's plan,
  20 KB for q27. A query that raises gets `failed: <error>` in both files and the run exits 1:
  an optimized answer that is not the planned one is that.
- No corpus suite runs the whole optimizer: each runs one rule. `pipeline.run_optimized` composes
  them: `dynamic_filters.apply`, then `join_order.ordered` with `kept_estimates` known, then
  `run_adaptive(kept=…)`. That is the order `run_adaptive`'s `kept` parameter is shaped for.
- The report is data (`optimizer/report.py`); `optimizer/report_text.py` renders it.
  - Dynamic filters: `FilterCandidate`s from `candidates(plan)`, `apply`'s own `Pruned` list, and
    the probes' builds kept as memory sources (name, rows).
  - Each DPhyp call is a `JoinOrder`: relation labels, key edges (DPhyp's input), every set
    priced, the tree or `unsolved`, the oriented tree, and the flips. A priced set is the mask,
    rows and cost. A flip is a join whose two sides the plan joined too, with the other side
    building.
  - Replans are `Replan(BuildMiss, kept, orders)`. Refused ones are `BuildMiss`es.
  - The plan before and after, as `plan_text`.
- Changed in the optimizer, no decision with it:
  - `optimize` is now `ordered(...)[0]`. `ordered` wraps the cost callback. The wrapper returns
    `sets.cost(mask)` unchanged and appends the set, with `sets.rows(mask)`; `rows` reads
    `_estimate`'s cache, which `cost` has just filled. The C ABI is untouched.
  - `run_adaptive` builds a `BuildMiss` per `BuildDone`. The threshold test reads the same two
    numbers as before. `AdaptiveRun.refused` holds misses, not events; its one reader
    (`test_replan`) reads `.rows`, which both have. It gains `replans`.
  - `plans/engine_plan.plan_text` is `test_engine_plan`'s former `rendered` helper, moved into
    production. The round-trip test now runs over it.
- Shown unchanged on shad-gpu, from `/tmp/peacock-opt/t6a` (rsync `--delete`, testdata linked to
  the existing sf1 copies; `.py` sha256 equal to the worktree's), tp4-single, 10 shards each,
  19 min: answers 120, optimized 100, dynamic filters 58, reassembled 100, no failures. The
  cardinality golden is unchanged (dataset tier, local).
- `-mini.cpu.txt` rows are the engine's at joins only. A partial `GpuAggregate` emits once per
  batch, so its rows follow the backend's batching: join-int's is 4,875 in the engine's run and
  400 here. `test_run_corpus` compares the tree, the answer's rows and the joins' rows.
- Tests: cheap tier 36 → 40 files, 355 → 380 cases (`test_report` 8, `test_report_text` 3,
  `test_run` 9, `plans/test_answers` 5; counts after the review rounds below). Dataset tier gains `tests/test_run_corpus.py` (1): about 20 s locally, DPhyp
  built in the step. It is on the cheap loop's `elsewhere` list. Each shown red:
  - recording: no priced sets; the flip test inverted; `JoinOrder`s not appended; replans not
    appended; refusals not appended; pipeline candidates, scans or probed builds dropped; the
    after plan taken as the before;
  - rendering: `fired` ignoring orders; the diff reversed; the flipped line, the range
    compaction, the replan's indent, a date's ISO form, a replan's orders or the refused line
    dropped;
  - `run.py`: `SECTIONS` dropping, `WHOLE` keeping, the existing file winning the merge, missing
    queries appended at the end, a refusal run as work, the missing-query check removed, a
    query one of two benches names refused, a refusal's whole text;
  - `test_run_corpus`: a writer ignoring the existing file, a renderer word changed. A writer
    always `WHOLE` stays green there, correctly: `WHOLE` keeps declared sections, as the engine's
    does. `test_run` holds that difference.
- Sample on shad-gpu, out of the tree in `/tmp/peacock-opt/t6a/out`: tpch q3 q5 q9 and tpcds q3
  q64 at tp4-single and tp1-rowgroup, two invocations side by side (`--jobs 6` and `4`), 2:21 and
  4:20 wall, peak RSS 4.6 and 7.0 GB per worker. Seconds as planned / optimized:

  | | tp4-single | tp1-rowgroup |
  |---|---|---|
  | tpch q3 | 18.7 / 18.5 | 5.5 / 5.3 |
  | tpch q5 | 34.4 / 30.6 | 6.3 / 5.5 |
  | tpch q9 | 96.3 / 44.5 | 13.8 / 13.8 |
  | tpcds q3 | 12.1 / 10.6 | 1.7 / 2.2 |
  | tpcds q64 | 102.7 / 155.3 | 11.3 / 45.9 |

  The corpus suites spent about one process-minute per run at tp4-single. A full `run.py` makes
  two runs per (query, mode) plus probes and replans. Estimate for 6b: 600 × ~2.5 process-min ÷ 20
  jobs ≈ 75 min, plus q8 at `tp4-rowgroup` (≈ 500 s a run). That is past the spec's forty
  minutes, so 6b profiles first.
- `.optimizer.txt` size, before the format decision below: q64 at tp4-single was 29,572 lines,
  2.1 MB, 28,663 of them one line per priced set. Its 18-relation clusters exceed `MAX_PAIRS`
  (6,626 sets priced, then the plan's order kept and orientation alone), again in each of its 7
  replans, until memory sources cut the clusters to 8 relations. No set repeated within a call
  (all 72 calls of the sample).
- q64 has no dynamic-filter candidate at tp4-single; tpcds q3 has one that prunes nothing (24 → 24).
- Review round 1, every finding fixed:
  - Moved out of `tests/corpus.py` into production: the parquet reader into `plans/tables.py`,
    since `engine_nodes.build` is what reads scans through it. Running a plan went into
    `plans/engine_run.py` (`run_plan`, `execute`, `execute_lanes`, `by_lane`, `answer`,
    `CORPUS_BUDGET`), and the answer compare with `UNORDERED_LIMIT` into `plans/answers.py`.
    `tests/corpus.py` imports them and keeps the tests' 256 MB default on `execute` and
    `execute_lanes`. `pipeline`'s probe is `run_plan` + `by_lane`, not a copy.
  - `run_query` holds the optimized answer to the planned one (`same_answer`: column count, then
    names by position, then `matches_oracle`'s multiset with its tolerance, or the row count for
    `UNORDERED_LIMIT`). A mismatch raises `AssertionError`: a `failed:` section and exit 1. Shown
    red on tpcds q3 at tp1-rowgroup, two ways. A stubbed result one row short gives `88 rows vs
    89`. A dynamic filter dropping the first 12 surviving row groups gives `53 rows vs 89`.
    Dropping only group 0 or only group 23 leaves the answer as it is: group 0 holds no November
    sale of manufacturer 128's items, and group 23 is the NULL-date tail.
  - One mode table, `pipeline.MODES`/`mode_shape` (exhaustive; an unknown name raises naming the
    five). It is used by `run.py` and the three corpus suites that decoded `PCK_MODE` themselves.
  - `main` runs `preflight` (sf1 directories, `dphyp.load()`) before any work; `generate` is the
    pool and the writes. Both are tested cheaply: a task for a query no plan golden has fails
    with `KeyError` in both files next to an intact `skipped:` section and exits 1, and a
    missing dataset or library is refused.
  - Format, the human's decision: per DPhyp call, `priced N sets`, the tree or `DPhyp stopped at
    its budget of N pairs after pricing M sets: the plan's order kept`, the oriented tree, one
    line per chosen join (sides, rows, C_out), the tree's C_out beside the plan order's, the
    flips. `JoinOrder` gains `max_pairs`, `joins` (`ChosenJoin`) and `plan_cost`. Those sets
    are estimated after DPhyp returns, so they decide nothing. `priced` still holds every set.
  - Shown red: the compare (unordered-limit branch either way, no rename, no tolerance), the mode
    table (a wrong entry, no refusal), `failed:` (not counted, one file only), the preflight
    (each check removed), `max_pairs`, chosen joins over DPhyp's tree, `plan_cost` over the
    chosen tree (a plan DPhyp reorders, test_optimize's), and each new rendered line.
  - Proven again: cheap tier 40 files, 379 passed; dataset tier 4 + 3 + 1. On shad-gpu (`.py`
    sha256 equal to the worktree's) the four suites at tp4-single: 120, 100, 58, 100, none
    failed. The sample again, every answer equal to the planned one, 2:19 and 4:02 wall. tpch
    tp4-single q3 17.8 / 17.3, q5 31.4 / 28.7, q9 94.5 / 43.9; tp1-rowgroup q3 5.7 / 5.4, q5
    6.4 / 5.6, q9 13.8 / 14.2. tpcds tp4-single q3 12.1 / 10.7, q64 98.4 / 142.2; tp1-rowgroup
    q3 1.5 / 2.3, q64 11.3 / 43.9.
  - q64's section is now 1,178 lines and 198 KB at tp4-single (337 of them the plan diff), and
    954 lines and 116 KB at tp1-rowgroup. The tpcds sample file is 206 KB, the tpch one 40 KB.
- Review round 2: `plans/answers.py` raises `AssertionError` by hand (`_require`) instead of
  `assert`, which `python -O` strips. Under `-O` the old compare passed a dropped row;
  `test_a_mismatch_is_raised_under_python_dash_o_too` runs it under `-O` in a subprocess and was
  red before. With `_require` disabled, three of the file's five tests go red. `_canonical` and
  `matches_oracle` docstrings are cut to the cap; the README's "The corpus" carries the
  argument. `engine_ir` points at `plans/tables.typed`. Cheap tier 40 files, 380 passed;
  `test_run_corpus` 1 passed.

## Task 6b

- Cost is `plans/cost_model.py`: `test_support/cost_model.rs` in Python, reading
  `testdata/cost_model.conf` (no table copied) and node lines by `golden_text.rs`'s rule. It
  matches the Rust where Python differs. Lines split at `\n` alone, as `str::lines`, not
  `splitlines`, which also splits at a form feed. The total is added term by term, because `sum`
  compensates on 3.12+ (1e16 + 1 + 1 is 1e16 in Rust and 1e16 + 2 under `sum`). It rounds as
  `f64::round` on the double (0.49999999999999994 → 0, where `floor(t + 0.5)` gives 1), and a
  negative total is 0, as `as u64` saturates. Pinned by deriving all ten committed
  `-mini.cost.txt` again from their `cpu.txt`, byte for byte, and by cases for each of these.
  Stripping a `\r` before `\n` is Rust's too, but no count can show it: a field value is trimmed,
  and a bare kind has no `output_bytes`.
- `GpuMemorySource`, the one prototype kind (`engine_plan.PROTOTYPE_KINDS` = `cost_model.FREE`),
  costs nothing, outside the conf (the human's decision). A memory source hands back a build that a
  probe plan or a stopped run made and was priced for, kept where the plan reads it: free by
  nature, not because `ram_to_vram_bytes` is 0.0 today. Any other kind the conf lacks raises, as
  the engine panics.
- Optimized = the final plan's run + every probe plan's run + every run a replan stopped
  (`run.priced`). All of it is work done. Check: q64's optimized `storage_read_bytes` equals the
  planned run's exactly at every mode (805,720,426 at tp4-single), so nothing is read twice.
- A fourth output, `<bench>/<mode>.costs.txt` (the human kept it, under a name that does not
  promise what the engine's `.cost.txt` is, a function of its `.cpu.txt`): per query `planned`,
  `optimized`, `probes`, `stopped` (total and bytes per category) and `fired` (`report.Fired`:
  probes, scans narrowed, DPhyp calls, calls whose tree's C_out is below the plan order's
  (`reordered`), calls stopped at budget, flips, replans, refusals). It merges like the other two,
  and `cost_report.html` is rendered from every such file. So a `--query` run re-renders the whole
  page instead of a page of three rows, and a cost that moves is one line in a diff.
  `cost_report.py` alone renders the page again from the files.
- The page (`cost_report.py`, the published report's CSS and its 1.4 threshold): a row per query.
  Per mode it shows non-optimized | optimized | ratio, where ratio = optimized / non-optimized,
  green below 1 and red above. A tooltip on optimized gives the probe and stopped parts. Then the
  engine figure, DuckDB, the final ratio — titled and defined as projected — and the rules. A
  rule's chip counts the modes where it changed the plan, of the modes run: dynamic filters where
  a scan was narrowed, DPhyp where a tree beat the plan order's C_out, flips, replans. Calls,
  probes, budget hits and refusals are in the hover with every other count. A query skipped at
  every mode gets one cell across all fifteen columns. A
  skipped or failed (query, mode) gets one cell over its three columns.
- Final ratio = min over modes of E × (O / P) / D. E is the engine's `-mini.cost.txt` figure,
  O / P the prototype's ratio at that mode, D DuckDB's `duckdb_cost=`. The engine column shows E
  at that mode. Why not O / D: the spec says only two prototype runs compare, and the numbers
  agree. The prototype's planned cost over the engine's figure has median 0.80 (tpch) and 0.85
  (tpcds), range 0.61–1.33 in tpcds. tpch `scan-limit` is 3.4e-6: the prototype's scan stops at
  the limit and the engine's reads 1 GB (#186). O / D would shift each row by that factor. E × O / P
  keeps the published report's scale and takes from the prototype only its own ratio. The cost:
  no final ratio where the engine has no figure at any mode run here. That is tpch q11 and q22,
  tpcds q24 and q54.
- Profile, cProfile in `run_query` on shad-gpu, wall under the profiler. q64 tp1-rowgroup: 67.6 s,
  of which 39 s in `engine_nodes.build`. Every one of 9 builds (1 planned, 7 stopped, final)
  re-read every scan's parquet: 197 reads, `table_to_dataframe` 26.5 s. DPhyp's cost callback
  (estimation) is 14 s over 30 calls. q64 tp4-single: 1013 s, of which 888 s is
  `partition_ops.row_digests`, 59 M `.iloc` reads of one value each. tpch q9 tp4-single: 461 s,
  316 s in `row_digests`. tpch q8 tp4-rowgroup: 974 s, 860 s of it the planned run: 139 k driver
  steps, `row_digests` 209 s, `memory_usage(deep=True)` behind `byte_size` 220 s over 609 k calls.
  Pandas' own operations (merge factorize, take) are a few seconds each. The time is the driver's
  Python, so no Polars.
- Two wins, no behaviour change. `row_digests` iterates `Series.array`, whose scalars are `.iloc`'s
  (a float32 stays a numpy float32, which `tolist` would turn into a float and move to another
  lane): 3–4× per row. `ParquetTables` reads a (table, columns) once per instance and hands each
  caller a shallow copy to rename. `run_query` makes one instance per task, so the planned run, the
  probes and every replan share it; nothing writes into a scan's columns (joins copy their build,
  rename their probe). Not done: caching `byte_size`, because the accountant adds and removes a
  batch by it and a cached value could change a budget decision; and caching `Statistics.column`,
  which is worth a few seconds per q64 replan.
- Same output, shown: the 6a sample's sections (tpch q3 q5 q9, tpcds q3 q64, at tp4-single and
  tp1-rowgroup, `cpu.txt` and `optimizer.txt`, 20 sections) are byte-identical to the full run's.
  That includes every lane's `batch_rows`/`batch_bytes`, which the hash decides. The per-dtype
  digests are pinned by a test that passed before the change. The cardinality golden is unchanged.
  The four corpus suites at tp4-single on shad-gpu, from the same copy (`.py` sha256 equal to the
  worktree's, 120 files), 10 shards each over 16 processes: answers 120, optimized 100, dynamic
  filters 58, reassembled 100, none failed, 10 min wall (19 in 6a).
- Seconds as planned / optimized, before → after: q64 tp4-single 98.4 / 142.2 → 34.7 / 52.7;
  q64 tp1-rowgroup 11.3 / 43.9 → 7.1 / 12.5; tpch q9 tp4-single 94.5 / 43.9 → 73.2 / 10.8.
- q64 is slower optimized, and the cost does not show it. It replans 7 times at every mode. The
  stopped runs come in two triples with near-equal bytes (1.0 MB, 148 MB, 1.39 GB at tp4-single).
  These are the two copies of the `cross_sales` CTE (cs1, cs2), whose builds are alike; nothing
  runs twice. The optimized cost is lower at every mode (0.83 to 0.93). The extra seconds were the
  rebuild of every scan per replan, gone with the read-once tables, and the Python estimation and
  DPhyp of 30 calls (9 stopped at the budget), which remain. Bytes cannot see either, so the report
  shows q64 as a saving. It is a cost of the prototype's replanning, not of the plan.
- Full run on shad-gpu, `/tmp/peacock-opt/t6d` (rsync `--delete` from the worktree, sf1 linked as
  in 6a), `run.py --jobs 16` on 22 cores (another user held about four), both benches, all five
  modes: 600 runs, 0 failed, 15:59 wall, peak RSS 7.7 GB in one worker. The first run, before the
  review, took 16:27 and 7.5 GB; its `cpu.txt` and `optimizer.txt` are byte-identical to this
  one's. Each optimized answer held to the planned one. tpch: 39 run at each mode. tpcds: 81 run
  and 18 skipped (refused by the planner) at each mode. The tail is tpch q9 at tp4-rowgroup, 818 s
  planned (901 s in the first run) and 13.9 s optimized; then, in the first run, tpch q8
  tp4-rowgroup 445 s and tpcds q24 tp4-rowgroup 420 s.
- Results. tpch: 195 runs, optimized cheaper at 50, dearer at 1, equal at 144. tpcds: 405 runs,
  cheaper at 364, dearer at none; median ratio 0.62, least 0.02 (q39 tp4-rowgroup). The one dearer
  run is tpch q18 at tp1-rowgroup, 1.0098: an orientation flip and a replan after a build of 57
  rows against 300,000 estimated. Final ratio within 1.4 for 17 of 20 tpch rows and 64 of 79
  tpcds rows; medians 0.86 and 0.81. The rules by what they changed, of the 600 runs: DPhyp was
  called in 500 and its tree beat the plan order's C_out in 325; a probe plan ran in 290 and
  narrowed a scan in 270; joins flipped in 191; replans in 311; a call stopped at the budget in 5
  (q64, every mode); a replan refused in 37. No memory source is priced: `ram_to_vram_bytes` is 0
  in every section.
- Output sizes: 30 text files, 20.2 MB together, and `cost_report.html` 174 KB. The largest is
  tpcds `tp4-single.optimizer.txt`, 2.6 MB; tpcds `tp4-rowgroup.cpu.txt` is 1.9 MB, each
  `.costs.txt` is at most 125 KB. The engine's largest committed golden is 7.8 MB (tpcds
  `tp4-rowgroup-mini.cpu.txt`), and its sf1 goldens total 52 MB.
- Tests: cheap tier 40 → 43 files, 380 → 399 cases: `plans/test_cost_model` 8, `test_cost_report`
  8, `plans/test_tables` 1, `test_run` +1, `operators/test_operators` +1. `test_run_corpus` (1)
  now checks the costs file and the page. Each shown red against a mutation of the code it pins:
  - cost function: Python's round, `floor(t + 0.5)`, `sum`, a negative total kept, `splitlines`,
    a memory source priced, nesting or quotes ignored in the field split, an unknown kind passed,
    multipliers ignored, the placeholder comment, a capitalized non-node line read as a node;
  - report: replan orders or narrowed scans miscounted, a marker kept whole, the ratio inverted,
    the best mode as max or without the ratio, the final ratio without the ratio, the engine column
    at the first mode, a skipped query per mode, a failed run dropped, a `<link>`, the caveat's
    words, `stopped` left out of the section, DuckDB's footer misread, the row's colour, the ratio
    cell's class, a chip over all modes, `reordered` as every call or with `<=`, the DPhyp chip by
    calls, the filters chip by probes, a budget chip, "projected" dropped from the title or the
    definition;
  - `run.py`: probes or stopped runs left out of optimized, no cost file, no page;
  - `test_run_corpus`: no page, planned priced into optimized;
  - tables: the cached frame handed out shared; digests: `tolist` for `.array`.
- Review round 1: no blocking or important finding; the human's decisions applied. The output is
  `<mode>.costs.txt`. The final ratio stays E × (O / P) / D and the page calls it projected, in
  its title and its one-sentence definition. The chips count effects (`Fired.reordered`,
  `narrowed`), with calls and probes in the hover. `GpuMemorySource` is free outside the conf.
  Nits: the `run.py` docstring within its cap; Rust's line split, sum and rounding (above);
  `cost_text` moved into its one caller, the test; the engine's `.cost.txt`, not the prototype's
  file, is a function of its `.cpu.txt` (build-test.md, README); "once per task" for the table
  cache; the digest test named for what it pins; the README names the generating environment
  (shad-gpu, Python 3.13.15, pandas 3.0.6) and that pandas' `memory_usage(deep=True)` is the byte
  count. Regenerated as above; cheap tier 43 files, 398 passed; `test_run_corpus` 1; cardinality
  golden unchanged.
- Review round 2: a row whose rules ran and changed nothing shows `nothing changed the plan` with
  its counts in the hover; `nothing fired` is kept for a row where no rule ran, as in
  `.optimizer.txt`. The re-render from the committed files changed only the rules cell of 10 rows
  (tpch q3, q19, q20, hash-join, join-int, mixed-join, rollup-over-join; tpcds q2, q78, q93). The
  total saturates as `as u64` does: NaN or negative 0, at or past 2^64 or infinite u64::MAX. Each
  new case shown red: the hover dropped without a chip, the old label, NaN through `< 0`, no cap.
  Cheap tier 43 files, 399 passed.

## Task 7

- Keys are by content, `<dataset>.sf<sf>/<sha256>.tsv` (the human's choice in review, over
  the plan's fixed key): the bucket keeps no versions (`get-bucket-versioning` is empty), and a
  content key is never replaced. `testdata/calibration/records.sha256`, committed, maps each
  dataset to its sha256 in sha256sum's format (`<sha>  tpch.sf40/records.tsv`), so
  `sha256sum -c` also checks the fetched files. Bucket, endpoint, pin, `pinned_sha` and
  `record_key` are in `scripts/lib/calibration-bucket.sh`.
- `--pull-benchmarks` publishes and pins (`publish_record` in `build-test-shadgpu.sh`). Keeping
  a run is committing the pin beside the tree; dropping it is `git checkout` of the pin, which
  leaves an unused object. Before splitting, one ssh call checks that the host's record still
  has the pulled file's sha256 (no run started since) and that `.rc` reads `<current id> 0` (a
  failing case appends its rows before it panics). Then `split_record.py` writes one file per
  dataset into a `mktemp -d`, refusing everything unless the heading has `capture=none` and each
  dataset's (query, mode) set is exactly what `corpus_benchmark_cases.inc` declares. Queries and
  modes lose their `_` as the macro does (`stringify!($query).replace('_', "-")`). A refusal
  leaves the pin alone and exits 1 with the tree home. Each file is uploaded through
  `REMOTE_AWS=/home/info/bin/aws` on stdin and read back under `bash -o pipefail`. Only then is
  it pinned and moved to `testdata/calibration/<dataset>.sf<sf>/records.tsv`. The benchmark
  tree cannot be the reference for completeness: it accumulates across runs, and a record is one.
- Migration: `tpch.sf40/records.tsv` copied server-side to
  `tpch.sf40/3aab8a668a606f8bf523b6ff688427c278473f09feb530f939816878446e2a7c.tsv`, read back
  with that sha256 (16,947,451 bytes, `text/tab-separated-values`). The old fixed-key object
  `tpch.sf40/records.tsv` was then deleted by the coordinator.
- `scripts/calibration/fetch_record.sh <dataset>.sf<sf> [--ssh HOST]` fetches the pinned key
  through a `.partial` and checks the sha256. `$AWS` (default `aws`) is the CLI on whichever host
  makes the call: a CI runner's with the `S3_*` secrets, `/home/info/bin/aws` on shad-gpu (not
  on a non-interactive ssh's PATH), the same through `--ssh shad-gpu` from a dev box. This
  workstation has no aws CLI and no `~/.aws`, so it fetched through shad-gpu.
- Readers of the old path: the preamble test writes its own record (below); `call_cost.RECORD`
  is the fetched `tpch.sf40/records.tsv`; `create_nsys_profile.sh` and `nsys_hbm.py` keep the
  pulled `calibration/records.tsv`; `build-test.sh` keeps what it pulls and publishes nothing
  (the key names no host, #226); `plot.py`'s usage names both records (`hbm.tsv` joins by call,
  so it may come from another run of the same cases); the calibration tests and
  `test_call_cost.py` write temp records.
- The preamble test writes one row through `append_records` into a temp dir, with
  `PEACOCK_RECORD_PATH` set and restored around a `catch_unwind`, so a panic cannot leave it
  set. It reads the record back as the readers do: every `#` line, then the column line. The
  allocator carries `=` and spaces and must read back as the run's; `capture=none` is not read
  back. Shown red by dropping the `#` from the last note line. `EnvLoan` was not moved into
  `test_support`: that would change `src/` and `peacock_gpu_benchmarks.rs`, which this host
  cannot build.
- The measured check is `tests/optimizer/test_call_cost_measured.py`, `dc8d33bf`'s body over
  `call_cost.RECORD`; absent, it fails naming the fetch script. It is a file of its own so the
  two synthetic cases stay in the cheap tier. It is on `pipeline.yml`'s `elsewhere` list and runs
  in `exec-model-corpus.yml`'s `call-cost` job: unsharded, only with no filter, fetching with the
  `S3_*` secrets. Whether those can read `calibration` is not checked from here: the bucket is in
  the account `tpch-sf40` is in (one `s3 ls` lists both), and Nebius shows no ACL or policy to
  compare. Over the sf40 record: 12 cases, 17 mode pairs as measured, 0 wrong, 3 within 10 %.
- Found: `build-test.md`'s header said Python 820 while its rows summed to 839 since `86f0d62d`;
  now 848 with this task's 9 (`test_split.py` 8, the measured check 1), total 2817. The
  committed `calls.tsv`, `hbm.tsv` and panels cover three cases of an earlier q6/q19 run, whose
  record is in git's history; the page says so.
- Review round 1 (redesign above, and): `split_record.py` spelled queries with `_` (red with a
  `scan_limit` case, then fixed); the host-record and `.rc` checks; the `capture=none` refusal
  (red with the check removed); the fetch script's usage takes the `AWS=` form, refuses a
  second `--ssh` and resolves its path before `cd`; the env var restored on panic; comment caps
  in `.gitignore`, `exec-model-corpus.yml` and `call_cost.py`; `plot.py` and the page on what
  `hbm.tsv` joins; the split into a temp dir, moved only after pinning; `pipefail` on the
  read-back (a missing key fails the read, not the comparison).
- Proving, no `records*.tsv` under `testdata/`: rust-only lib 604 passed (2 ignored),
  `test_ci_coverage` 9, `test_corpus_goldens` 26, no warnings; cheap tier, the step's own
  selection, 43 files, 399 passed; calibration tests 20. `publish_record` was run alone against
  shad-gpu under a scratch `REMOTE_REPO` and a case file declaring `scratch.sf40`. Each refusal
  fired: rc 101, an earlier run's rc, the host record changed, `capture=trace`, a declared mode
  missing. Each left the pin alone. The success path uploaded `scratch.sf40/<sha>.tsv`, pinned
  it, and `fetch_record.sh --ssh shad-gpu scratch.sf40` fetched the same bytes; the object was
  then deleted, leaving the bucket as it was. `fetch_record.sh tpch.sf40` by the
  new key gave 174,690 rows, sha256 OK, through shad-gpu from here and on shad-gpu
  (`/tmp/peacock-opt/t7`). The measured check passed in both places. `split_record.py` gives the
  sf40 record back byte for byte. Workflows parsed; every touched `run:` block and script passes
  `bash -n`; `--upload-record` is now an unknown flag.
- Review round 2: `pull_one` (`shadgpu-env.sh`) returns 2 when the transfer fails and 1 only
  when the host has no file. The record's call site dies on 2 ("pulling … failed, so the copy
  here is broken and nothing was published") instead of reaching the host check, which would
  have called it a newer run. Shown with `resilient_rsync` stubbed to fail against a scratch
  `REMOTE_REPO`; the missing-file case still gives `record_home=0`. `create_nsys_profile.sh`'s
  two calls die on any non-zero code, as before, now after a "the transfer failed" line. The
  two `record_home` blocks are merged and `names` is local to `publish_record`. No tree+pin
  guard (the human's decision).
