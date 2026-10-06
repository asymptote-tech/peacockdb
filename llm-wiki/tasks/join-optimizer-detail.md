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
- `test_call_cost`'s check against the measured record is not on the branch; it needs the
  sf40 record → Task 7.

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
