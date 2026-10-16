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
  (shad-gpu's aws CLI as build-test.md's *Datasets* names it, the endpoint `upload_datasets.sh`
  uses).

## Known state

- `exec-model-corpus.yml` was dispatched on `8c846434` (run `37599528455`): the three corpus
  shards and the `call-cost` job green; the CI `S3_*` keys read the `calibration` bucket (the
  record fetched, 174,690 rows, sha256 as pinned).
- Completeness reading: the corpus suites had run at `tp4-single` only after Tasks 6a, 6b and
  9; at `dd9fa865` they ran at the other four modes, and `run.py` over all five reproduced the
  committed outputs byte for byte (Corpus verification below).
- Task 8 measured every device-enabled case at sf40 (26, tpcds sf40 generated on shad-gpu) on an
  empty card; the trees and the two record pins are on the branch, the records in S3
  (Task 8 below). The Nsight passes over the 26 cases have not run: `calls.tsv` and `hbm.tsv`
  hold three cases of an earlier run.
- Task 10: the conf prices forwarders at 0.0 and the scatter at its bytes; the engine's cost
  goldens, the prototype's `.costs.txt` and `cost_report.html`, both tests and the documents
  moved with it (Task 10 below).
- Task 11: every join of a cluster carries its id, `MJ<n>`, kept through reorder, flip and
  replan; `.cpu.txt` renders `mj=MJ<n>` and `.optimizer.txt` heads each DPhyp call with it. The
  committed outputs carry both since Task 12's full `run.py`.
- Task 12: four named tpch queries with a non-inner join are registered. Non-inner joins enter
  clusters on CD-A's TES and keep their plan type; a rebuilt root keeps a hash something above
  reads. The full `run.py` regenerated every committed prototype output, and the corpus tier ran
  at every mode (Task 12 below). Task 18 is not started.
- #239: `equi_join`'s matched shares are over the key domain. The 39 queries whose plans hold a
  join that reads a share ran again at every mode into the committed outputs; no other query's
  output can move (#239 below).
- Task 13: dynamic filters run twice, on the engine's plan and on the order DPhyp made; a key lifts
  through unions and non-inner joins; probe plans run adaptively. The full `run.py` regenerated
  every committed prototype output, and the answer suites passed over every (query, mode) that
  differs from HEAD; tpcds q84 is the one ratio above HEAD, +0.2–0.5 % (Task 13 below).
- Task 14: every named tpch query has a DuckDB cost, and `cost-report` prices the named rows on
  both sides (Task 14 below).
- Tasks 15 and 16: `.optimizer.txt` reads without the code, its plan diff in `<mode>.plandiff.txt`,
  and `run.py` writes `<mode>.optimizer.md` beside it. The full `run.py` regenerated every
  committed prototype output; every `.cpu.txt` and `.costs.txt` section and `cost_report.html` are
  HEAD's byte for byte (Tasks 15 and 16 below).
- Task 17: `cost_report.html` links each ratio other than 1.00 to the query's `.optimizer.md`
  section and its rules fired, per mode, to their lines of `.optimizer.txt`, relatively as
  committed and with `--sha` at a commit; the cost-report job renders it so and publishes it at
  `join-optimizer/` on master. Whether the published links resolve is checked after a push
  (Task 17 below).
- `test_cardinality_corpus` compares a per-join golden since Task 5; q64's misses are a recorded
  limit, not a threshold (Task 5 below).
- Green otherwise: rust-only, DPhyp, the cheap tier (48 files, 488 cases), and q64 in the corpus answer,
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
- shad-gpu: shared with work outside this repo. Corpus suites run from a scratch copy on the
  host with its own Python 3.13 venv, DuckDB 1.5.4 and DPhyp built there (its glibc 2.31 is
  older than a local build needs); the gate's checkout is not touched. tpcds exists there at
  sf1 and sf40.

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
- `test_stats_embeddings.py` runs in no CI job. It links the generator into a temp dir and
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
  never ran was read. The runners lived in a scratch copy on the host.
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
  shad-gpu, from a scratch copy (the branch's `scripts/exec_model`, its testdata identical to
  the branch's by `rsync -c`):
  `test_engine_answers` at `tp4-rowgroup` in 40 shards, 120 passed, q8 included, 16 min wall;
  at `tp4-single` shards 0, 13 and 26 of 40, 9 passed. q8 alone: 137,760 steps, cap 169,049,
  483 s.
- Proven, with the call check. Cheap tier local: 35 files, 353 passed. On shad-gpu, same
  scratch copy re-synced: answers at `tp4-rowgroup` 120 passed and at `tp4-single` 120
  passed, dynamic filters (memory sources) at `tp4-single` 58 passed, 40 shards each. q8
  alone: 137,760 steps, 169,049 calls, cap 169,049, 666 s beside the shards.

## Task 4

- Layout. Files were moved with `mv`, and git pairs the renames once they are staged.
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
  pyarrow 25.0.1 (a scratch copy synced with `rsync --delete`).
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
- Shown unchanged on shad-gpu, from a scratch copy (rsync `--delete`, testdata linked to the
  existing sf1 copies; `.py` sha256 equal to the branch's), tp4-single, 10 shards each,
  19 min: answers 120, optimized 100, dynamic filters 58, reassembled 100, no failures. The
  cardinality golden is unchanged (dataset tier, local).
- `-mini.cpu.txt` rows are the engine's at joins only. A partial `GpuAggregate` emits once per
  batch, so its rows follow the backend's batching: join-int's is 4,875 in the engine's run and
  400 here. `test_run_corpus` compares the tree, the answer's rows and the joins' rows.
- Tests: cheap tier 36 → 40 files, 355 → 380 cases (`test_report` 8, `test_report_text` 3,
  `test_run` 9, `plans/test_answers` 5; counts as of the end of this task). Dataset tier gains `tests/test_run_corpus.py` (1): about 20 s locally, DPhyp
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
- Sample on shad-gpu, out of the tree, in the scratch copy: tpch q3 q5 q9 and tpcds q3
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
- `.optimizer.txt` size, before the format below: q64 at tp4-single was 29,572 lines,
  2.1 MB, 28,663 of them one line per priced set. Its 18-relation clusters exceed `MAX_PAIRS`
  (6,626 sets priced, then the plan's order kept and orientation alone), again in each of its 7
  replans, until memory sources cut the clusters to 8 relations. No set repeated within a call
  (all 72 calls of the sample).
- q64 has no dynamic-filter candidate at tp4-single; tpcds q3 has one that prunes nothing (24 → 24).
- Then:
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
  - Format: per DPhyp call, `priced N sets`, the tree or `DPhyp stopped at
    its budget of N pairs after pricing M sets: the plan's order kept`, the oriented tree, one
    line per chosen join (sides, rows, C_out), the tree's C_out beside the plan order's, the
    flips. `JoinOrder` gains `max_pairs`, `joins` (`ChosenJoin`) and `plan_cost`. Those sets
    are estimated after DPhyp returns, so they decide nothing. `priced` still holds every set.
  - Shown red: the compare (unordered-limit branch either way, no rename, no tolerance), the mode
    table (a wrong entry, no refusal), `failed:` (not counted, one file only), the preflight
    (each check removed), `max_pairs`, chosen joins over DPhyp's tree, `plan_cost` over the
    chosen tree (a plan DPhyp reorders, test_optimize's), and each new rendered line.
  - Proven again: cheap tier 40 files, 379 passed; dataset tier 4 + 3 + 1. On shad-gpu (`.py`
    sha256 equal to the branch's) the four suites at tp4-single: 120, 100, 58, 100, none
    failed. The sample again, every answer equal to the planned one, 2:19 and 4:02 wall. tpch
    tp4-single q3 17.8 / 17.3, q5 31.4 / 28.7, q9 94.5 / 43.9; tp1-rowgroup q3 5.7 / 5.4, q5
    6.4 / 5.6, q9 13.8 / 14.2. tpcds tp4-single q3 12.1 / 10.7, q64 98.4 / 142.2; tp1-rowgroup
    q3 1.5 / 2.3, q64 11.3 / 43.9.
  - q64's section is now 1,178 lines and 198 KB at tp4-single (337 of them the plan diff), and
    954 lines and 116 KB at tp1-rowgroup. The tpcds sample file is 206 KB, the tpch one 40 KB.
- `plans/answers.py` raises `AssertionError` explicitly (`_require`) instead of
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
  costs nothing, outside the conf. A memory source hands back a build that a
  probe plan or a stopped run made and was priced for, kept where the plan reads it: free by
  nature, not because `ram_to_vram_bytes` is 0.0 today. Any other kind the conf lacks raises, as
  the engine panics.
- Optimized = the final plan's run + every probe plan's run + every run a replan stopped
  (`run.priced`). All of it is work done. Check: q64's optimized `storage_read_bytes` equals the
  planned run's exactly at every mode (805,720,426 at tp4-single), so nothing is read twice.
- A fourth output, `<bench>/<mode>.costs.txt` (named so that it does not promise what the
  engine's `.cost.txt` is, a function of its `.cpu.txt`): per query `planned`,
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
  branch's, 120 files), 10 shards each over 16 processes: answers 120, optimized 100, dynamic
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
- Full run on shad-gpu, from a scratch copy (rsync `--delete` from the branch, sf1 linked as
  in 6a), `run.py --jobs 16` on 22 cores (another user held about four), both benches, all five
  modes: 600 runs, 0 failed, 15:59 wall, peak RSS 7.7 GB in one worker. An earlier run, before
  this task's last changes, took 16:27 and 7.5 GB; its `cpu.txt` and `optimizer.txt` are byte-identical to this
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
- The output is
  `<mode>.costs.txt`. The final ratio stays E × (O / P) / D and the page calls it projected, in
  its title and its one-sentence definition. The chips count effects (`Fired.reordered`,
  `narrowed`), with calls and probes in the hover. `GpuMemorySource` is free outside the conf.
  Also: the `run.py` docstring within its cap; Rust's line split, sum and rounding (above);
  `cost_text` moved into its one caller, the test; the engine's `.cost.txt`, not the prototype's
  file, is a function of its `.cpu.txt` (build-test.md, README); "once per task" for the table
  cache; the digest test named for what it pins; the README names the generating environment
  (shad-gpu, Python 3.13.15, pandas 3.0.6) and that pandas' `memory_usage(deep=True)` is the byte
  count. Regenerated as above; cheap tier 43 files, 398 passed; `test_run_corpus` 1; cardinality
  golden unchanged.
- A row whose rules ran and changed nothing shows `nothing changed the plan` with
  its counts in the hover; `nothing fired` is kept for a row where no rule ran, as in
  `.optimizer.txt`. The re-render from the committed files changed only the rules cell of 10 rows
  (tpch q3, q19, q20, hash-join, join-int, mixed-join, rollup-over-join; tpcds q2, q78, q93). The
  total saturates as `as u64` does: NaN or negative 0, at or past 2^64 or infinite u64::MAX. Each
  new case shown red: the hover dropped without a chip, the old label, NaN through `< 0`, no cap.
  Cheap tier 43 files, 399 passed.

## Task 7

- Keys are by content, `<dataset>.sf<sf>/<sha256>.tsv`, not a fixed key: the bucket keeps no versions (`get-bucket-versioning` is empty), and a
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
  `REMOTE_AWS` (shad-gpu's aws CLI) on stdin and read back under `bash -o pipefail`. Only then is
  it pinned and moved to `testdata/calibration/<dataset>.sf<sf>/records.tsv`. The benchmark
  tree cannot be the reference for completeness: it accumulates across runs, and a record is one.
- Migration: `tpch.sf40/records.tsv` copied server-side to
  `tpch.sf40/3aab8a668a606f8bf523b6ff688427c278473f09feb530f939816878446e2a7c.tsv`, read back
  with that sha256 (16,947,451 bytes, `text/tab-separated-values`). The old fixed-key object
  `tpch.sf40/records.tsv` was then deleted.
- `scripts/calibration/fetch_record.sh <dataset>.sf<sf> [--ssh HOST]` fetches the pinned key
  through a `.partial` and checks the sha256. `$AWS` (default `aws`) is the CLI on whichever host
  makes the call: a CI runner's with the `S3_*` secrets, shad-gpu's own named in full (it is not
  on a non-interactive ssh's PATH; build-test.md's *Datasets* has it), the same through
  `--ssh shad-gpu` from a dev box. A dev box without an aws CLI or credentials fetches through
  shad-gpu.
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
- Also: `split_record.py` spelled queries with `_` (red with a
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
  (in a scratch copy). The measured check passed in both places. `split_record.py` gives the
  sf40 record back byte for byte. Workflows parsed; every touched `run:` block and script passes
  `bash -n`; `--upload-record` is now an unknown flag.
- `pull_one` (`shadgpu-env.sh`) returns 2 when the transfer fails and 1 only
  when the host has no file. The record's call site dies on 2 ("pulling … failed, so the copy
  here is broken and nothing was published") instead of reaching the host check, which would
  have called it a newer run. Shown with `resilient_rsync` stubbed to fail against a scratch
  `REMOTE_REPO`; the missing-file case still gives `record_home=0`. `create_nsys_profile.sh`'s
  two calls die on any non-zero code, as before, now after a "the transfer failed" line. The
  two `record_home` blocks are merged and `names` is local to `publish_record`. Nothing guards
  the tree and the pin against being committed apart.

## Task 9

- Written: `scripts/exec_model/design.md` (new), `scripts/exec_model/README.md` (rewritten as
  how to work with the prototype), `peacockdb-dphyp/design.md` (new, 172 lines). The README's
  design sections — the strategy, the step cap, the two join backends, pandas inside cuDF's
  vocabulary, the corpus oracle, layout injection, the limit, the findings, the prototype as a
  model — moved into `design.md`, condensed and without their history. The module table stays.
- Comment-only edits that follow: four comments pointed at README sections that moved
  (`engine/partitioned_driver.py`'s `step_cap`, `plans/answers.py`'s `matches_oracle`,
  `tests/corpus.py`, `__init__.py`); they now name `design.md`, "The step cap" and "DuckDB, the
  oracle".
- What the documents revealed, code against text:
  - The committed `.costs.txt` files give tpch 55 cheaper, 1 dearer, 139 equal and tpcds 379
    cheaper, 5 dearer (q5 at every mode, by at most 0.03 %), 21 equal. Task 6b's 50/1/144 and
    364/0/41 are the same files counted at the page's two-decimal ratio, which is how
    `cost_report._summary` counted; it now counts by the integer totals, and the committed page's
    summary line reads 55/1/139 and 379/5/21. `design.md` has the exact counts. The rule counts (500, 325, 290, 270, 191, 311, 5, 37) and the final ratios
    (17 of 20, 64 of 79; medians 0.86, 0.81) recount as stated.
  - `peacockdb-dphyp/Cargo.toml` said peacockdb-core links the rlib; no crate depends on it. The
    comment now says the rlib is for a Rust caller, which nothing is yet.
  - `optimizer/call_cost.py`'s docstring said it decides lanes, batch size and shuffles; only
    `test_call_cost*.py` import it. Docstring and the README's row now say no rule reads it.
  - `plans/engine_nodes.py`'s import of `optimizer.cardinality`/`stats` serves only
    `estimated_fanouts`, whose one caller is `tests/optimizer/test_cost.py`: `run.py` builds every
    join at fanout 1. `design.md` says so, and that the helper can move once something in
    production calls it.
  - The README's loop ran every test file, the four corpus files included, and its
    `pytest scripts/exec_model/tests` collects 787 tests, the 120 + 100 corpus queries among them.
    The README now selects the cheap tier as `pipeline.yml` does and warns off whole-folder
    pytest.
  - The README described a workflow for editing the prototype that `prompts.md` contradicts;
    the paragraph went.
  - A Python cost callback that raises is swallowed by ctypes ("Exception ignored on calling
    ctypes callback function"), answers 0, and `dphyp.solve` returns a tree: shown with a raising
    callback over a three-relation chain. A `StatsError` inside `SetEstimates.cost` would not stop
    the optimizer. Fixed in `optimizer/dphyp.py`: the crate has no cost that stops it (a NaN is
    kept like any other), so the wrapper holds the first exception, answers every later set 0
    without calling the cost, and raises it after `dphyp_solve` returns. With no exception it
    returns `float(set_cost(mask))` as before, so no tree moves. Pinned by
    `test_a_cost_that_raises_stops_the_solve_with_its_exception` (`test_dphyp.py`, a chain of
    four whose cost raises on the first set): red before the fix (`_Priced was not raised`, six
    "Exception ignored"), green after; red again with the re-raise removed, and with the
    short-circuit removed (the cost asked after the failure). `test_dphyp` 4 passed,
    `test_optimize` 2. Both design documents state the contract and the wrapper. What ctypes
    hands the crate is not promised; it was 0 in every run seen.
  - DPhyp's boundaries, measured through the ABI: a budget of exactly the pair count solves (chain
    of three, 4 pairs: 4 solves, 3 is `budget`); `out_len` is untouched on any non-zero code;
    65 relations return -3 with a capacity below 129 and -1 above, since the capacity is checked
    first.
- README commands, each run once as written (`/usr/bin/python3` first on the PATH, since the
  linuxbrew `python3` has no pandas; DuckDB the downloaded v1.5.4 release, 08e34c447b):
  - environment check (pandas 2.2.2, pyarrow 18.1.0); the DuckDB download, in a scratch directory;
    `cargo build --release -p peacockdb-dphyp` with `CARGO_TARGET_DIR=/build/peacock/rust-only-target`
    and the export after it; `cargo test -p peacockdb-dphyp`, 6 passed.
  - cheap: `test_determinism.py`, then the loop in bash: 43 files, 399 passed, 1:59 wall; after
    the callback fix, 400 passed, 1:56, no "Exception ignored" in the log. zsh selects the same 43
    files. The harness's name selection and the pytest node id, 1 passed each.
  - dataset: `test_stats_sidecar` 4 passed, `test_cardinality_corpus` 3, again under
    `UPDATE_CANONICAL=1` 3 with the goldens byte-identical, `test_run_corpus` 1.
  - corpus: `test_engine_optimized.py test_tpch_q3` 1 passed, 18 s; the shard form narrowed to
    `test_tpch_q3`, 1 passed. The whole-file shard form was not run here: the four suites ran
    whole on shad-gpu at `tp4-single` (Task 6b, 10 shards each), and nothing under
    `scripts/exec_model` but `call_cost.py`, its new test, the README and the callback wrapper
    (identical for a cost that does not raise) has changed since.
  - manual: `test_stats_embeddings` 1 passed, 1:12; `fetch_record.sh --ssh shad-gpu tpch.sf40` and
    then `test_call_cost_measured`, 1 passed, both in a `git archive HEAD` extraction so the
    checkout's `testdata/` was not written; the plain `fetch_record.sh tpch.sf40` on shad-gpu
    with its aws CLI on the PATH, in a scratch directory removed after. Both fetches: 174,690
    rows, the pinned sha256.
  - `run.py --bench tpch --query q3 join-int --mode tp1-single --jobs 2 --out …` into a scratch
    directory, 2 runs, 0 failed, 12 s; `cost_report.py` re-rendered the committed page
    byte-identical. The full `run.py --jobs 16` is Task 6b's run (shad-gpu, 15:59), not repeated.
- Corrections from a second reading against the code:
  - `peacockdb-dphyp/design.md`'s chain(3) example showed `0 1 -1 2 -2`; the call returns
    `0 1 2 -2 -1`, `(0 ⋈₀ (1 ⋈₁ 2))`, after 4 pairs, the tie kept as first found (run through the
    ABI with popcount costs; asked `{1,2}`, `{0,1}`, `{0,1,2}`). Cycles in the tests start at 3.
  - `ParquetBatchPartitioner` names nothing in the code: `design.md` and `operators/source.py`'s
    docstring now name `scan_mapping/partition.rs` (`balanced_chunks`).
  - `cost_report._summary` classified at two decimals, so the page said tpcds "dearer at 0" with
    five runs dearer. It counts `optimized.total` against `planned.total`; the cells keep their
    two-decimal colouring. `test_the_summary_counts_a_run_cheaper_or_dearer_by_its_totals_not_its_shown_ratio`
    (0.996, 1.0002 and 1.0, each shown as 1.00): red under rounding (0/0/3), green after (1/1/1).
    `cost_report.py` re-rendered the committed page from the committed files: one line changed,
    the summary, to 55/1/139 and 379/5/21.
  - `design.md` rewritten shorter (452 → 415 lines): each pipeline step as its role and what it was
    chosen over, the mechanics left to the module docstrings; the #139 and #186 observations
    folded into the step cap and the projected final ratio; the residual-filter streaming finding
    added under the operators. The footer does carry a per-row-group distinct count for some
    DuckDB columns, so `design.md` says why the NDV still comes from the sidecar.
  - README: the corpus files run the queries each applies to (58 with a filter candidate, 100
    with a cluster); `cost_report.py` is shown with the run's `--out`, and without it re-renders the
    committed page.
  - Proven: cheap tier 43 files, 401 passed (399 + `test_dphyp` 1 + `test_cost_report` 1), so
    `build-test.md`'s exec-model row goes 399 → 401; `test_run_corpus` 1 passed; the README's
    `run.py` and `cost_report.py --out` commands run as written; no-history scan clean.
  - `test_dphyp`'s 14-clique case bounded wall time (< 1 s) in the cheap tier. It now counts the
    cost callback's calls: 2^14 − 15 = 16,369, each set once (in a clique every set of two or more
    relations is connected), measured through the ABI before pinning; renamed
    `test_a_clique_of_fourteen_prices_each_set_once_and_a_budget_stops_it`, budget half kept. Red
    with a wrapper that asks twice. Cheap tier 43 files, 401 passed: the count does not move.

## Corpus verification

- `dd9fa865` checked again on shad-gpu after Tasks 6a, 6b and 9, with no code changed. Source: a
  `git archive` of `scripts/exec_model`, `peacockdb-dphyp` and the testdata it reads, in a
  scratch directory on the host. sf1 is linked as in Task 6b's full run. Python 3.13.15, pandas 3.0.6, pyarrow 25.0.1,
  DuckDB 1.5.4. DPhyp was built there and is byte-identical to the host's earlier build.
- `run.py --jobs 16 --out full`, both benches, all five modes: 600 runs, 0 failed, 90 skipped
  (tpcds, 18 per mode), exit 0. Wall 16:48, peak RSS 7.0 GB. All 31 outputs are byte-identical to
  the committed `scripts/exec_model/testdata/goldens/` (30 `.cpu`/`.optimizer`/`.costs.txt` and
  `cost_report.html`). Checked with `cmp` and against the `dd9fa865` blobs' sha256. So the
  outputs are deterministic and Task 9's callback fix moved nothing.
- Corpus suites, sharded with `PCK_SHARD`. Passed counts, and the longest shard in seconds:

  | suite | tp1-single | tp1-rowgroup | tp4-rowgroup | tp4-sized |
  |---|---|---|---|---|
  | `test_engine_answers` | 120 (142) | 120 (164) | 120 (1337) | 120 (673) |
  | `test_engine_dynamic_filters` | 58 (81) | 58 (49) | 58 (225) | 58 (274) |
  | `test_engine_reassembled` | 100 (93) | 100 (95) | 100 (1183) | 100 (583) |

  0 failed, and no log has a `FAIL`, `Traceback`, `Error` or `Killed`. `test_engine_optimized`
  ran at `tp4-single` only. At the other four modes `run.py` stands in: it holds every optimized
  answer to the planned one, and the planned one is checked against DuckDB above.

## Task 8

- tpcds sf40 is on shad-gpu beside tpch's, where build-test.md's *Datasets* places the sf40
  data: 24 files, 15,787,933,629 bytes (15 GB). Made with `generate_testdata.sh --bench tpcds
  --sf 40` (DuckDB 1.5.4, threads=1, spill pinned on the dataset volume, sampled every 5 s,
  aborting under 300 GiB free): 9 min 41 s, peak RSS 60 GiB, nothing spilled; 735 GB free
  before, load ~1 on 22 cores. The 24 files' sha256 list hashes to `58610fc1…bb36`. The host's
  copy of `generate_testdata.sh` differs from the repo's in comments only.
- The case list is the device column of `corpus_cases.inc` whole: 14 new cases, 26 in all.
  tpch `shuffle_additive_avg` at all five modes; `aggregate_groupby`, `filter_project`,
  `shuffle_additive`, `shuffle_stddev`, `nested_loop_join` at `tp1_single`; tpcds q37, q82, q84,
  q85 at `tp1_single`. q37 is not in the spec's list, but its device cell is `tp1_single`, so
  "every query enabled on the device" includes it. `scan_limit` and `cross_join` are `none` on
  the device and stay out.
- `build-test-shadgpu.sh`'s run links every `<dataset>.sf<sf>` the case list names from
  `DATASETS_DIR` (`shadgpu-env.sh`) rather than tpch's alone. The names come from the case
  file by `sed`, checked non-empty before any phase runs. `build-test.sh` (verda-gpu) still
  checks `tpch.sf40` only, and the page says what that means.
- Red: the pin in `the_sections_of_a_file_are_ordered_numerically` moved to the ten tpch
  `tp1_single` names before the cases were added; on shad-gpu it failed with left
  `[q1, q6, q17, q19]`. Green after: 8 harness assertions pass and `--list` gives 26 `bench_`.
- Before the full run, filtered runs beside a neighbour (not published) showed every new shape
  plans and runs at sf40: the four tpcds cases (31.9 s for all four), filter-project (78 s,
  most of it unloading ~96 M rows), nested-loop-join, shuffle-stddev.
- First full run (`20261007T190853-1362626`, started 16:08 host time on an empty card, load 0.4)
  was stopped at 17:08, rc 143, nothing pulled or published: a neighbour (30-70 GB, 99 %)
  arrived during `shuffle-additive-avg` at `tp1-rowgroup`, whose last three runs went 47 s → 96,
  177, 166 s, and whose `tp1-single` read 1.70 s against a 0.79 s run in the same ten. The record
  is one run, so a filtered re-run cannot replace those rows; the whole run is repeated. The 22
  cases before it took 57 min; the tpcds q84 case read 0.19 s here against 0.56 s in the smoke
  run beside a 19 GB neighbour.
- Second full run `20261007T212158-1467410`: started 18:22 host time after two polls 140 s apart
  with no compute process on the card (load 0.04); 139.2 GiB free at the pool's install; a
  monitor every 2 min saw no other process to the end. 34 passed in 3,601.56 s, rc 0, 255,220
  record rows. `--pull-benchmarks` published `tpch.sf40/1013141a…def2.tsv` (254,160 rows,
  26,092,745 bytes) and `tpcds.sf40/b6a0372e…0c90.tsv` (1,060 rows, 105,809 bytes) and pinned both;
  `fetch_record.sh --ssh shad-gpu` fetched each as pinned, and `split_record.py` over the pulled
  record gives the same bytes.
- Trees: tpch's five files rewritten (26 sections in all with tpcds's), tpcds's
  `tp1_single.benchmark.txt` new, 406,391 bytes over the six. Every trailer `build=release`,
  `allocator=rmm-pool … 69.0GiB … of 139.2GiB free`.
- `test_call_cost_measured` over the new tpch record: 22 cases, 25 mode pairs as measured,
  0 wrong, 5 within 10 % (was 12, 17, 0, 3).
- Proving: harness assertions 8/8 on shad-gpu (`--skip bench_`); rust-only `--lib` 604 passed
  (2 ignored), `test_ci_coverage` 9, `test_corpus_goldens` 26 (the trees' trailer arithmetic, both
  datasets); calibration tests 20; every touched script `bash -n`, the remote bench script rendered
  and `bash -n`.
- Step 5 over the tpcds cases: `create_nsys_profile.sh` named `goldens/tpch.sf1` and
  `nsys_calls.py` matched cases to a golden on mode alone, so a capture holding tpcds cases would
  have been refused after an hour's capture. Now `--plans-dir` is the goldens root and each (dataset, mode) is checked
  against `<dataset>.sf1/<mode>.plans.txt`; the script checks each case-list dataset's directory
  before the capture. `nsys_hbm.py` and `plot.py` already key on the whole case. Red first:
  `test_each_case_is_checked_against_its_own_datasets_goldens` (a tpch and a tpcds case in one
  capture, and a tpcds golden the run disagrees with refused naming q37) failed with
  "goldens/tp1-single.plans.txt does not exist". The case-list reader is `bench_datasets` in
  `shadgpu-env.sh`, shared by both scripts, whitespace-tolerant and refusing when it parses fewer
  lines than `^\s*corpus_query_benchmark!` counts (dry runs: the committed list, an indented
  `corpus_query_benchmark! ( tpcds , 1 , …)` read as `tpcds.sf1`, a `forty` sf refused 15/14, an
  empty list refused).
  `every_declared_case_has_a_section_in_the_tree` (rust-only), red with the tpcds tree moved
  aside ("never measured") and with q85's section renamed ("no section for q85"); both restored,
  sha256 unchanged. Proving at that point: calibration 21, rust-only `--lib` 604 (2 ignored),
  `test_corpus_goldens` 27, `test_ci_coverage` 9; `bash -n` on the three scripts and the rendered
  remote bench script.
- Derived files. The panels are drawn again from the two pinned records (255,220 rows, 25,522
  calls) with the committed three-case `hbm.tsv`: 67 panels and `index.html`, 18 files rewritten
  and 50 added, none removed (icicle/ and query/ +23 each, compute/ +4: `CudfNestedLoopJoin`,
  `CudfRepartition{Hash, 1→4}`, `CudfSort`, `CudfSortPreservingMerge`), 4.6 MB, matplotlib
  3.9.2 on the dev box, 72 s. The pooled panels (load/, compute/, spread/, hbm/) now draw each
  `<dataset>.sf<sf>` as a series with a legend. `calls.tsv` and `hbm.tsv` are not regenerated:
  shad-gpu stopped answering ssh before either Nsight pass could start (port 22 timing out from
  22:35 UTC for over two hours; its card was held by other users' jobs until then).
- Sizes measured on shad-gpu before that, on one small case each: the counters capture's sqlite
  grows 27 MB per second of run (q6 `tp1-single`, 13 s: 351 MB, 19 metrics at 20 kHz), ~100 GB
  for the hour-long corpus; the trace capture of q6 `tp1-rowgroup` is 182 MB. A row per
  execution, `hbm.tsv` over the corpus would be ~255 thousand rows and 25 MB, 97 % of it the six
  rowgroup cases.
- So the counters pass is a capture per case (`<test> --exact`), joined on the host with
  `nsys_hbm.py --case` (python 3.8 there, `test_hbm.py` green under it) and deleted before the
  next case, so the host holds one case's capture at a time; `nsys_hbm.py --parts` merges the
  per-case outputs there and checks them against the whole record both ways; only `hbm.tsv`
  comes home and stays in git. It is a row per call: each reading the median over the
  executions, with `executions` and `samples` kept, 25,522 rows for the corpus.
- Why the median over all ten and not fewer executions. From the committed three-case capture
  (45 calls × 10 executions): a call with ≥ 10 samples sits a median 2.3 % from its own
  10-execution median in any one execution (p90 26 %, all of that in small aggregates near the
  10-sample line), and byte-weighted the error of one execution is 0.5 %, of a median of three
  0.3 %, of five 0.2 %. By kind, a median of three: `CudfFilter` p90 1.1 %, `CudfScan` 3.5 %,
  `CudfHashJoin{Inner}` 5.3 %, `CudfAggregate{Partial}` 17 %, `CudfAggregate{Merge}` 43 %.
  Under 10 samples a call moves by up to its whole value at any count (p90 100 %). Three
  executions would be enough for the bytes that matter, but fewer executions needs a count the
  harness does not expose — `MEASURED_RUNS` is a constant in `src/test_support/` — and this task
  changes nothing under `peacockdb-core/src/`, so the counters pass keeps ten and its median is
  the reference itself. The case-at-a-time capture is what bounds the disk.
- `hbm.tsv` has no S3 path: it stays in git, and the record's publish and `fetch_record.sh` are
  as Task 7 left them.
- Code: `nsys_hbm.py --record` repeats (one per published dataset file), reading through
  `record.read_records`, which `plot.py` now shares; red first with a tpch and a tpcds case in one
  capture joined onto their two records ("1 captured calls have no row"). The plot test with two
  datasets read the saved figures' legends and was red with empty legends. Per call: red first
  was `executions` missing from the join's output; shown red with the code changed under them,
  the median of three executions (20/40/90 % of peak gives the 40 % call; with a mean it fails)
  and a per-case join under another case's name (fails with its check removed); a merge missing
  a case's part is refused naming it; `plot.hbm_points` gives the same points from a file a
  row per execution and from its fold (`record.hbm_per_call`), and a different list when the
  panel does not fold.
- Colours: `plot.py` builds one series → colour map in `main` from every series in the records
  and the hbm rows and hands it to every pooled panel, and every pooled panel carries a legend,
  one series or two. So tpch is the same colour in a panel only tpch makes (`CudfNestedLoopJoin`,
  `CudfRepartition{Hash, 1→4}`, hbm/) as beside tpcds. Red first: a kind only tpcds makes drew no
  legend. Re-rendered: those three panels changed, the other 64 and `index.html` did not.
- The counters pass covers the whole corpus: `create_nsys_profile.sh --metrics` under a
  `PCK_TEST_FILTER` selecting fewer cases than the list is refused before any ssh, naming both
  counts (dry run: `_q8` selects 3 of 26), since the merge is checked against every pinned
  record; a case list of `none` lines alone is refused as declaring no timed case. `hbm_per_call`
  decides per row, so files of both shapes fold together, and `plot.py` folds each `--hbm` file
  as it reads it.
- Proving: calibration tests 29 (`test_calls` 6, `test_hbm` 8, `test_plot` 7, `test_split` 8),
  rust-only `test_corpus_goldens` 27 and `test_ci_coverage` 9, `bash -n` on the five touched
  scripts, `pipeline.yml` parsed. `build-test.md`: Rust 1890, C++ 94, Python 859, total 2843.

## Task 10

- The conf: `cuda_shuffle_bytes 1.0` (Emit, Merge, Union, Interleave) is now `cuda_scatter_bytes
  1.0 GpuEmitPartitions` and `cuda_forward_bytes 0.0 GpuMergePartitions,GpuUnion,GpuInterleave`,
  in the old line's place. `cuda_passthrough_bytes` is unchanged.
- Neither parser names a category (`test_support/cost_model.rs`, `plans/cost_model.py`).
  `test_cost_model.rs` did, in its synthetic tree, and held every golden's total to Σ
  `output_bytes`, which a 0.0 category breaks. It now holds the category lines to Σ
  `output_bytes` — every node's bytes still land in one — as
  `cost_goldens_match_and_categories_hold_every_byte`.
- Red first, over the committed conf: `a_forwarder_costs_nothing_and_a_scatter_costs_its_bytes`
  (Rust) and `test_the_conf_prices_a_forwarder_at_nothing_and_a_scatter_at_its_bytes` (Python), a
  scan's 30 bytes under one node of each kind. Both failed at `GpuMergePartitions`, 50 against 30.
- The engine's goldens: `UPDATE_CANONICAL=1 cargo test -p peacockdb-core --features rust-only
  --test test_cost_model` rewrote the ten `-mini.cost.txt`. Against the parent commit, all 551
  sections: the old shuffle bytes are scatter + forward, every other category is unchanged, and the
  total is lower by exactly the forward bytes; 332 sections moved. `cost-report --cost-diff --base
  HEAD`: 551 compared, 332 changed, 0 regressions.
- The `cost-report` crate names no category; its prose now says a forwarder counts nothing. Its
  ratio, PeacockDB over DuckDB, green at ≤ 1.4:

  | bench | rows with a ratio | ≤ 1.4 | median ratio |
  |---|---|---|---|
  | tpch | 20 | 5 → 9 | 1.955 → 1.45 |
  | tpcds | 79 | 9 → 17 | 3.77 → 2.79 |

  The biggest movers: tpcds q5 15.23 → 7.00, q6 49.74 → 32.72, q71 9.05 → 6.06, q42 14.56 → 9.75;
  tpch q5 2.06 → 1.38, q8 1.80 → 1.22, q9 2.03 → 1.41, q2 3.96 → 2.90. Green now: tpch q5, q8,
  q10, q17; tpcds q10, q13, q30, q35, q68, q69, q85, q94.
- The prototype's outputs cannot be derived from the committed files. `<mode>.costs.txt` prices
  the planned run, the probe plans and the stopped runs, and `<mode>.cpu.txt` holds only the final
  run, so the per-node kinds that split the old shuffle bytes are in no file. Those bytes are
  non-zero in 373 planned, 174 probe and 195 stopped rows of the ten files. So `run.py` ran over
  the whole corpus, off shad-gpu, which did not answer ssh.
- The local host gives shad-gpu's bytes when the versions match: Python 3.13.0, pandas 3.0.6, pyarrow
  25.0.1. Under pandas 2.2.2 every string column differs (tpch q3's customer scan 9,899,610 bytes
  against 3,749,610).
- The run: one `run.py --bench B --mode M` per (bench, mode), tpch at `--jobs 2`, tpcds at
  `--jobs 1`. `--jobs 4` took free memory to 0.5 GB within 90 s, and tpcds `tp1-single` at
  `--jobs 2` to 1.3 GB; both were stopped. Wall 51:22 for tpch and 3:48:01 for tpcds, 4:39:23 in
  all; one worker peaked at 8.0 GB (tpcds `tp1-single`), the lowest free memory was 2.8 GB; the
  outputs are text, 16 MB of bytecode aside nothing else was written. Every run but the last
  ended in `KeyError: 'cuda_scatter_bytes'` after writing its three files: the cost report it
  renders reads every bench's and mode's `.costs.txt`, and those not yet rerun carried the old
  categories. The last rendered it; `cost_report.py` alone renders the same bytes.
- Against the parent commit, all 600 runs and 90 skipped sections: every `cpu.txt` and
  `optimizer.txt` section byte-identical, every skipped section identical, and in each
  `.costs.txt` row (planned, optimized, probes, stopped) the old shuffle bytes equal scatter +
  forward, every other category is equal, the total is lower by exactly the forward bytes, and
  `fired` is unchanged. 3,870 comparisons, 0 differ.
- The prototype, before → after: tpch 195 runs, optimized cheaper at 55 → 55 and dearer at 1 → 1,
  median optimized/planned 1.0000 → 1.0000, final ratio ≤ 1.4 for 17 → 17 of 20; tpcds 405 runs,
  cheaper at 379 → 379, dearer at 5 → 5, median 0.6218 → 0.6240, final ≤ 1.4 for 64 → 64 of 79.
  The final ratio takes the cheapest projected mode, a `tp1` one for most queries, where only a
  union forwards. No tpch final ratio moved; tpcds q5 15.23 → 7.00, q2 3.53 → 2.53, q71 0.89 →
  0.76, q14 2.10 → 1.86, and none rose. tpcds q5 is still dearer optimized, now by under 0.06 %;
  q64 is cheaper at every mode, 0.83–0.92.
- Proving: rust-only `--lib` 604 (2 ignored), `test_corpus_goldens` 27, `test_ci_coverage` 9,
  `test_cost_model` 4, `test_cpu_corpus` 555; `cargo test -p cost-report` 36; `peacockdb-dphyp` 6;
  cheap tier 43 files, 402 passed, again after the run; `test_run_corpus.py` 1; `run.py` over the
  corpus, 600 runs, 0 failed. `build-test.md`: Rust 1891, C++ 94, Python 860, total 2845.
- The golden check also holds each total to Σ `output_bytes` less the forwarders' bytes: the
  category lines alone stay green when two multipliers trade places. Red with `vram_to_ram` at 0.0
  and `ram_to_vram` at 1.0 (`tp1-rowgroup-mini.cpu.txt`: total 44,136,825,402 against
  45,497,867,394 less 1,360,484,033), green with the conf restored, its sha256 unchanged.
- `cost_report.py` refuses a `.costs.txt` section whose categories are not the conf's, naming the
  file and query and saying `run.py` must run over it again, rather than a bare `KeyError`. Red
  first: `test_a_cost_file_priced_under_another_conf_is_refused_naming_it` failed on
  `KeyError: 'join_bytes'`. The committed page renders byte for byte as before.
- Proving after that: `test_cost_model` 4, rust-only `--lib` 604 (2 ignored),
  `test_corpus_goldens` 27, `test_ci_coverage` 9; `cargo test -p cost-report` 36; cheap tier 43
  files, 403 passed; `test_duckdb_cost.py` 41. `build-test.md`: Rust 1891, C++ 94, Python 861,
  total 2846.

## Task 11

- The id lives on the join node, `EngineNode.mj`, not in the plan text and not in a table
  keyed by `id(node)`. Every rewrite copies nodes — `replaced` along the path to a swap, a replan's
  memory sources, disassembly's final `replace` — and `dataclasses.replace` copies the attribute
  with them; an identity table would lose every node a rewrite copied. `plan_text` writes `fields`
  only, so the `.optimizer.txt` plan diff is unchanged.
- `multijoin.named` gives the ids, in `clusters`' order, outermost first. A plan is named whole or
  not at all, so a named plan comes back unchanged; there is no numbering on past existing ids.
  The id has one name, `mj`, on `EngineNode`, `MultiJoin` and `JoinOrder`, as it is rendered.
  `MultiJoin` gained `joins`, the nodes `_collect` takes as the cluster's joins, and `mj`, the
  root's id; naming reads `joins`, never the join type, so a non-inner join Task 12 admits is
  named with its cluster. `named` copies the tree and
  leaves the plan given alone: `run.py` runs that plan as planned too.
- `join_order.ordered` names first, so the pipeline's call and every replan's go through it. A
  replan's plan is already named and keeps its ids; a cluster read from memory since leaves its
  number unused (tpcds q64 ends with `MJ1` and `MJ4`). Disassembly gives each join it makes the
  cluster's `mj`. `JoinOrder.mj` carries the id into the report.
- The first naming sees the plan after the dynamic filters. Today no filter's build holds a join,
  so this is the engine's plan order. Once Task 13 lets a build hold joins, a cluster inside it is
  read from memory before naming and gets no number.
- Red first, all five on the missing feature: `test_multijoin` (two tests) and `test_replan`,
  `test_engine_run` on `ImportError: cannot import name 'named'`; `test_optimize`'s new test on
  `'JoinOrder' object has no attribute 'multijoin'`; `test_report_text` on `JoinOrder.__init__()
  got an unexpected keyword argument 'multijoin'` (the field's name then). Green after.
- The tests: `test_each_cluster_is_named_in_plan_order_outermost_first_on_every_join_it_holds`
  (`MJ1` on both outer joins, `MJ2` under the aggregate, a named plan returned as it is);
  `test_a_cross_join_a_nested_loop_join_and_a_semi_join_carry_no_id`;
  `test_each_cluster_keeps_its_id_through_a_reorder_and_a_flip` (the outer cluster flips, the
  inner one reorders; the aggregate's keys, estimated at 100 and 10 in the run, are shifted onto
  the orders' so that key 0 meets eight orders, and the optimized answer is those eight rows);
  `test_a_replanned_cluster_keeps_its_id_when_one_before_it_is_read_from_memory` (a `RightSemi`
  whose build is `MJ1` runs whole, `MJ2` stops the run, the replan has `MJ2` alone);
  `test_a_join_in_a_cluster_names_it_and_no_other_join_does` (`mj=` on the inner hash join, not
  on the cross, nested-loop or `LeftSemi` join). Three mutations are caught: `named` numbering a
  named plan again fails the replan test; disassembly dropping the id fails the reorder test;
  rendering `mj=` on every `GpuHashJoin` fails the render test.
- The committed `<mode>.cpu.txt` and `<mode>.optimizer.txt` predate the field and the headings and
  were not regenerated. A full `run.py` takes 4 h 40 min off shad-gpu. A sample ran instead into
  a temporary `--out`: tpch q2, q5, q8 and tpcds q5, q58, q64 at `tp4-single`, `--jobs 1`, Python
  3.13.0, pandas 3.0.6 and pyarrow 25.0.1, as the committed outputs; 6 runs, 0 failed, 12 min.
  With `, mj=MJ<n>` removed and each `join order <id>:` heading reduced to `join order #:`, every
  `.cpu.txt` and `.optimizer.txt` section is byte-identical to the committed one, and every
  `.costs.txt` section identical as it stands. `mj=` is on exactly the inner `GpuHashJoin` lines
  (tpcds q5's left join carries none). Replans keep their ids: q58's headings run `MJ1`…`MJ7`,
  then `MJ1, MJ4–MJ7` in replan 1.
- Proving: cheap tier 43 files, 408 passed; `test_run_corpus.py` 1 (its heading is now
  `join order MJ1:`); the sample above. `build-test.md`: Python 866, total 2851.
- After the one-name rename to `mj`, `named` without the numbering-on path, the non-empty answer
  in the reorder-and-flip test and the `LeftSemi` in the render test: cheap tier 43 files, 408
  passed. The rendered text is unchanged (the key was `mj=` before), so the sample was not run
  again. `build-test.md` counts unchanged: no test was added.

## Task 12

### The named queries

Four tpch queries, each with one non-inner join under one inner join. Three are written in the
costly order and may move under TES; `left-join-fixed` may not.

| query | the engine's plan, every mode | DuckDB rows | the non-inner join's rows |
|---|---|--:|--:|
| `left-join-reorder` | `Inner(σ nation, Left(customer, orders))` | 63,551 | 1,550,004 |
| `semi-join-reorder` | `Inner(σ nation, LeftSemi(customer, orders))` | 4,149 | 99,996 |
| `anti-join-reorder` | `Inner(σ nation, LeftAnti(customer, orders))` | 1,951 | 50,004 |
| `left-join-fixed` | `Inner(σ lineitem, Left(customer, orders))` | 2,528 | 1,550,004 |

The build side is listed first. `customer ⋈ σ(nation)` is 6,100 rows, so the three reorders
would feed 6,100 customers to the non-inner join instead of 150,000.

- `left-join-reorder` is the spec's query as written.
- `semi-join-reorder` and `anti-join-reorder` put `[NOT] EXISTS` in a derived table over
  `customer`, joined to `nation` outside it. Written with `EXISTS` in the outer `WHERE` beside
  `n_name = 'FRANCE'`, DataFusion decorrelates it over the whole `FROM`: the plan is
  `LeftSemi(Inner(σ nation, customer), orders)`, already the cheap order, with nothing to
  gain.
- `left-join-fixed` joins `σ(lineitem)`, one ship date, on `o_orderkey`, a column of the
  null-supplying side. `customer ⟕ (orders ⋈ σ lineitem)` would keep all 150,000 customers,
  padded, so conflict detection must refuse the move. The plan keeps the join `Left` at every mode: DataFusion
  eliminates an outer join under a null-rejecting filter above it, and an inner join's equi-key
  is not a filter. `lineitem`'s scan reads one row group of 49.
  It is a negative case only while outer joins are not simplified. The equi-key rejects nulls
  from `orders`, so the `Left` is an `Inner` in meaning, and a detector that simplified outer joins
  first would legally produce `customer ⋈ (orders ⋈ σ lineitem)`. The second half's test
  asserts that `customer ⟕ (orders ⋈ σ lineitem)` is never produced, not that the order never
  changes.
- No `ON` reads both sides of the non-inner join beyond its equi-key, so none meets #153 or #159.

Registration: the four `.sql` files; a `cost-registry.csv` row each, plan and cpu cells
enabled, gpu cells disabled on `152` for the two Left joins, `152 220` for semi and anti; a
`corpus_query!` line each, gpu modes `none`, `data_fusion_exact`. The device oracle is
`live_cpu` for `left-join-reorder`, whose answer exceeds the 256 KiB cap and whose section in
`mini.result.txt` and `duckdb-result.txt` says so, and `golden_exact` for the other three.
Those three sections in `mini.result.txt` equal DuckDB's, rows and text. No device ran them.

### What the optimizer does with them

Nothing yet. Each query is one cluster, `MJ1`, of two relations: the filtered scan, and the
non-inner join as an opaque relation. DPhyp has one order to give, and the plan is unchanged at
every mode, cost ratio 1.00. `left-join-fixed` replans once at every mode: the `σ lineitem`
build has 2,528 rows against about 49 estimated (q-error 52). The replan reads the build from
memory as `m0` and keeps the order.

Cardinality golden: eight lines added (77 joins, median q-error 1.07, 70.1 % within 2×). Each
non-inner join's estimate is exact; `left-join-fixed`'s inner join is 50.3 against 2,528.

The four sections of `<mode>.cpu.txt` and `<mode>.optimizer.txt` carry `mj=MJ1` and
`join order MJ1:`; the others will once a full `run.py` runs. `run.py --query` with the four
names touched only their sections, and `cost_report.html` gained their rows (tpch 43 queries).
It ran locally with Python 3.13.0, pandas 3.0.6 and pyarrow 25.0.1, `--jobs 2`: 20 runs, 0
failed, 75 s.

### Proving, first half

- Red first: `plan_goldens::tpch_tp1_single` on the four sections missing, the registry test
  on a section with no registry row, `the_registry_matches_the_cpu_corpus_in_both_directions` on
  twenty cells with no test, each new corpus case on no `== <query>` section,
  `test_cardinality_corpus` on eight lines missing.
- Goldens regenerated added sections only: `git diff --numstat` shows no deleted line in any
  engine golden, `duckdb-result.txt` or prototype `.cpu.txt`, `.optimizer.txt` and `.costs.txt`.
  The cardinality golden and `cost_report.html` change one summary line each. No `.plans.txt`
  section other than the four moved; `recipe-payloads.txt` did not move.
- rust-only: `--lib` 604 passed, 2 ignored; `test_cpu_corpus` 575; `test_corpus_goldens` 27;
  `test_cost_model` 4; `test_ci_coverage` 9. Cheap tier 43 files, 408 passed.
  `test_cardinality_corpus` 3. The four queries in `test_engine_answers`,
  `test_engine_reassembled` and `test_engine_optimized` at every mode: 4 each.
- `test_engine_dynamic_filters` gains none of the four, so it stays at 58.
- `build-test.md`: `test_cpu_corpus` 575 (row 574, 120 queries, 571 cells), the engine-plans
  row 390 (124, 58, 104, 104); Rust 1911, Python 878, total 2883.
- After the Left rows' tickets became `152`, the `--` headers on the four `.sql` files and the
  counts moved in `scripts/exec_model/README.md`, #235 and #220: rust-only `--lib` 604 passed,
  2 ignored, `test_cpu_corpus` 575, `test_corpus_goldens` 27, `test_cost_model` 4,
  `test_ci_coverage` 9; `cost-report` 36 and its render; cheap tier 43 files, 408 passed;
  `test_cardinality_corpus` 3; the four queries in `test_engine_answers`,
  `test_engine_reassembled` and `test_engine_optimized` at `tp4-single`, 4 each.
  `duckdb_result.py --dataset tpch` wrote `duckdb-result.txt` byte-identical, and no golden
  holds a query's text, so the headers moved nothing.

### Conflict detection, second half

- **CD-A's TES as hyperedges, no crate change.** The spec names CD-C. `optimizer/conflicts.py` is
  CD-A of Moerkotte, Fender and Eich, §5. A join's TES is the relations its predicate reads,
  plus, for each join below that the two may not swap past by `assoc`, `l-asscom` and
  `r-asscom`, that join's side which would be left behind. There are no conflict rules. CD-B and
  CD-C keep the conflicts as rules tested per (left, right) pair. `peacockdb-dphyp` prices per
  set and cannot refuse a pair, so they would need a pair callback in the crate. CD-A is correct
  and not complete, and every corpus case Task 12 needs is still found: the three named reorders,
  `left-join-fixed` kept, q58's date restriction first. It was first labelled CD-B; extending
  the TES with no rules is CD-A.
- **One edge per join.** In a cluster holding a non-inner join, each join, inner ones included, is
  one hyperedge, its whole predicate. A set S then holds at most |S| − 1 edges, so each pair DPhyp
  joins has exactly one edge across it, its sides unsplit. So a non-inner join never takes another
  join's predicate as a residual (#153), and no guard is kept for it. Over every golden at every
  mode, DPhyp's trees for the clusters holding a non-inner join have 995 joins, each with exactly
  one edge across it, unsplit. `conflicts.joining` returns the edges with one side within each set, as
  DPhyp joins two sets. `SetEstimates` raises where no split makes a set DPhyp asked for, and
  `orient` unpacks exactly one edge. A cluster of inner joins only keeps its edges per relation
  pair and is not tested for conflicts.
- The tables' conditional entries (`⟕⟕`, `⟗⟕`, `⟕⟗`, `⟗⟗`) need a predicate that rejects NULLs.
  `strong` is equi-keys with no residual and NULL equal to nothing.
- Walls: a mark join (its mark column is no relation's); a non-inner join with a residual (its
  NULL rejection is not read, and its `RightSemi`/`RightAnti` mirrors are refused, #159); cross
  and nested-loop joins, as before.
- **Orientation.** A non-inner join keeps the plan's type and build side: `orient` builds the side
  holding the relations its TES has on the side the plan's join built (`Edge.builds_left`). Only
  inner joins are oriented by the copy model, which would charge a semi or anti join for copies
  it never makes. Disassembly reads the type off the tree: `Left` where the preserved side
  builds, `Right` where it probes. Before this rule the copy model flipped non-inner types in
  about 17 queries; `left-join-fixed` and tpcds q5 rose to 1.11 and 1.02 at tp4-rowgroup.
- **The root's hash.** A non-inner join that used to be a wall is now a cluster root. Moved
  down, it leaves an inner join on other keys at the root. tpcds q10 and q35 then refused to build
  (a `LeftMark` above wanted `c_customer_sk`), and q56 and q60 answered wrong (a per-lane final
  aggregate on `i_item_id` split its groups). So a rebuilt root is shuffled back onto the plan
  root's lanes and `hashed_on` where something reads that hash before the next exchange
  (`MultiJoin.hash_read_above`, set by `clusters`):
  - a hash join, or an aggregate with a `final` step;
  - an enclosing cluster, when the relation holding this cluster declares a hash on columns among
    its keys there: a rebuilt join takes such a relation as hashed without a shuffle.
- Per-lane nodes — sorts, partial aggregates, projects, a union — read no hash. Over every tp4
  golden the shuffle-back fires in exactly q10, q35, q56 and q60. A first version shuffled back
  wherever the plan root declared a hash. That fired in six inner-only queries HEAD answered right
  (tpch q15; tpcds q50, q59, q62, q76, q99). q76's was pure waste: its new root is hashed on
  `ss_sold_date_sk`, equal in value to `d_date_sk`, then scattered again.
- q59's MJ3 sits under a projected relation of MJ1 hashed on `d_week_seq`. MJ1 joins that
  relation only on other columns, so no rebuilt join trusts the hash, and it is not shuffled.
  Disassembly writes the hash before the root takes the plan root's names, so the text can name a
  column by its relation's name; the ordinals are the plan's.
- Estimates: `cardinality.typed` is the tree estimator's per-type rule, factored out of
  `Estimator._joined`. A left join adds the preserved side's unmatched rows, a semi join keeps
  the matched share of its side, an anti join the rest. `SetEstimates` applies it over a
  canonical split: the highest relation whose removal leaves a set the edges make, else the
  first two such sets.
- Disassembly: the non-inner edge gives the join its type; the output is that type's own;
  `hashed_on` is read only off a side the join does not pad, and a `Full` join's off neither;
  `null_equals_null` is written where the plan had it, and keys that disagree on it are refused.
- **Red first.**
  - `test_conflicts` failed on the missing module.
  - Also red first: the two set-estimate tests; `test_a_rebuilt_cluster_root_keeps_the_hash_…`
    (5 rows came back 14); `test_a_non_inner_join_keeps_its_plan_type_and_build_side_…` (the
    Left join came back `Right`); the null-equality refusal.
  - `test_ordered_corpus` raised `PlanError` on tpcds without the root fix. It now checks the
    finished plan, not the flag: with `hash_read_above` forced off it finds 27 problems, refusals
    and split groups in q10, q35, q56 and q60 at the tp4 modes.
  - The four reorder tests passed when written. Walling non-inner joins off again fails three.
- **Mutants the brute force catches.** Over every three-relation plan, with two NULLs per table
  and `null_equals_null` per join: `r-asscom` always true; `assoc` ignoring `strong`;
  `l-asscom` ignoring `strong`; `assoc` true for a semi join; `l-asscom` true with a full join;
  `assoc` true for inner over full; disassembly dropping `null_equals_null`. One NULL per table
  was not enough: a padded row matched on that NULL looks like a padded row, so the two `strong`
  mutants passed. Dropping the right-subtree conflicts fails `left-join-fixed`. A `Full` join
  hashed on its build keys fails its own test.
- **Corrected.** An earlier note here said the brute force caught `assoc` ignoring `strong`;
  with no NULLs in its tables it caught neither `strong` mutant.
- **Tests moved off the old wall.** `test_multijoin` and `test_engine_run` now use a semi join
  with a residual. `test_replan`'s `SEMI` puts an aggregate under the semi join's build, so two
  clusters remain.
- The three-relation brute force draws one way round per join for each order (seeded), 35 s;
  every orientation takes two minutes. The four-relation sample draws the same way.

### Sample, second half

`run.py` into a temporary `--out`, then the three answer suites, both at every mode, over tpch's
four named queries, q2, q4, q5, q8, q13 and q15 and tpcds q5, q10, q35, q50, q56, q58, q59, q60,
q62, q64, q76 and q99: 50 and 60 runs, none failed; 22 queries × 3 suites × 5 modes, 330 answer
cases against DuckDB, all passed. q10, q35, q56 and q60 build and answer at every mode. tpch q15
and tpcds q50, q59, q62, q76 and q99 have HEAD's shuffle count at every tp4 mode and HEAD's ratio
at every mode.

### Full run

`run.py` into the committed outputs, locally, Python 3.13.0, pandas 3.0.6, pyarrow 25.0.1: tpch
215 runs in 48 min at `--jobs 2`, peak 7.4 GB resident; tpcds 405 runs in 3 h 36 min at
`--jobs 1`, peak 8.0 GB; none failed. Every section now carries Task 11's `mj=` and
`join order MJ<n>:` headings.

Against HEAD's committed outputs, the optimized `.cpu.txt` compared with `mj=` stripped:

- 710 sections. 90 are skipped or failed markers, as before. 503 keep their plan, and their
  costs are identical. In 2 of those, tpcds q8 at tp4-rowgroup and tp4-sized, only the stopped
  run's cost moves (−0.35 %). Its cluster now holds a null-equal semi join, so the pre-replan
  plan is a rebuild of the same order, and the replan ends in HEAD's plan.
- 117 sections in 24 queries change. Ordering changes no join type in any of the 620 plans.
  Join types differ in some final plans only where a replan makes a different build, joins
  included, into a memory source.

| query | modes | ratio at tp1-single | at tp4-single |
|---|---|---|---|
| tpch left-join-reorder | 5 | 1.00 → 0.35 | 1.00 → 0.34 |
| tpch semi-join-reorder | 5 | 1.00 → 0.67 | 1.00 → 0.74 |
| tpch anti-join-reorder | 5 | 1.00 → 0.72 | 1.00 → 0.80 |
| tpch q16 | 5 | 1.00 → 0.90 | 1.00 → 0.94 |
| tpch q18 | 5 | 1.00 → 0.34 | 1.00 → 0.44 |
| tpch q20 | 5 | 1.00 → 1.00 | 1.00 → 1.00 |
| tpcds q6 | 5 | 0.17 → 0.16 | 0.19 → 0.19 |
| tpcds q10 | 5 | 0.75 → 0.75 | 0.72 → 0.72 |
| tpcds q14 | 5 | 0.44 → 0.44 | 0.43 → 0.43 |
| tpcds q33 | 5 | 0.65 → 0.65 | 0.154 → 0.155 |
| tpcds q35 | 5 | 0.70 → 0.65 | 0.69 → 0.65 |
| tpcds q40 | 5 | 0.40 → 0.22 | 0.49 → 0.26 |
| tpcds q41 | 2 | — | — (1.000 → 0.997 at the other two tp4 modes) |
| tpcds q56 | 5 | 0.36 → 0.36 | 0.14 → 0.14 |
| tpcds q58 | 5 | 0.69 → 0.15 | 0.75 → 0.18 |
| tpcds q60 | 5 | 0.64 → 0.64 | 0.18 → 0.18 |
| tpcds q69 | 5 | 0.74 → 0.73 | 0.66 → 0.66 |
| tpcds q75 | 5 | 0.438 → 0.447 | 0.430 → 0.443 |
| tpcds q77 | 5 | 0.44 → 0.44 | 0.19 → 0.19 |
| tpcds q78 | 5 | 1.00 → 0.90 | 1.00 → 0.93 |
| tpcds q80 | 5 | 0.66 → 0.33 | 0.79 → 0.40 |
| tpcds q83 | 5 | 0.73 → 0.23 | 0.80 → 0.27 |
| tpcds q93 | 5 | 0.68 → 0.68 | 0.81 → 0.81 |
| tpcds q97 | 5 | 0.74 → 0.67 | 0.70 → 0.66 |

- 19 queries are cheaper somewhere, 3 have the same ratio, and 2 are dearer by a little:
  - tpcds q75, by 0.8–3 % at four modes: an estimator defect, #239. `Estimator.equi_join` takes
    the matched shares over the two key counts, not the key domain its inner rows divide by, so
    each channel's left join to its returns is estimated far below its own preserved side
    (catalog's at tp1-single 14,372 rows against 143,810). Pricing sets with it, DPhyp moves the
    date join above the left join, where the plan had it below. With the shares over the domain
    the estimate is 28,513 and the plan's order stays in all three channels; the fix moves the
    cardinality golden and every non-inner estimate, so it waits for its own full run.
    `bug_a_left_join_is_estimated_below_its_preserved_side` records it. Fixed in #239 below.
  - tpcds q33, by 0.6–1.1 % at four modes. Its pre-run trees and inner flips are HEAD's at every
    mode. Its `RightSemi` is now in a cluster and rebuilt, and the project that picked its columns
    folds into the joins. So at the tp4 modes a different badly estimated build trips the first
    replan: store's `ss_addr_sk`, q-error 5.30, where HEAD's tripped on catalog's
    `cs_bill_addr_sk`, 6.74. The replans then keep different memory sources.
  - No ratio this task touches rises above 1.00. tpch q18 at tp1-rowgroup, HEAD's one tpch
    regression (1.01), is now 0.47.
- Totals: optimized cheaper in 80 tpch runs (55 at HEAD) and 386 tpcds runs (379), dearer in the
  same 5 (tpcds q5, under 0.06 %, at HEAD too). The median ratio is 1.00 tpch, 0.61 tpcds (0.62).
  The final ratio is within 1.4 for 18 of 20 tpch rows (17) and 67 of 79 tpcds rows (64).
- DPhyp is called 3,221 times (3,235); 808 calls reorder (719) and 45 stop at the budget, all
  tpcds q64's, as before. 590 flips (615); 726 replans (750), 91 refused (108). 555 runs call
  DPhyp (520), and 5 hit the budget.

### Corpus tier

The four files `exec-model-corpus.yml` names, at every mode against DuckDB, locally, one process at
a time: 2,020 cases, all passed, 10 h 58 min. Two shards at a time took the host under 1.5 GB of
available memory at tp1-single, so the run went serial. Wall time and peak resident memory:

| mode | answers (124) | dynamic filters (58) | reassembled (111) | optimized (111) |
|---|---|---|---|---|
| tp1-single | 604 s, 8.2 GB | 208 s, 5.6 GB | 511 s, 9.2 GB | 497 s, 5.9 GB |
| tp1-rowgroup | 772 s, 7.9 GB | 230 s, 5.0 GB | 666 s, 6.0 GB | 588 s, 5.4 GB |
| tp4-single | 3,300 s, 6.8 GB | 997 s, 4.2 GB | 3,178 s, 5.2 GB | 2,082 s, 6.0 GB |
| tp4-rowgroup | 6,208 s, 7.2 GB | 1,178 s, 3.5 GB | 5,970 s, 4.8 GB | 2,535 s, 5.1 GB |
| tp4-sized | 3,472 s, 6.8 GB | 906 s, 4.6 GB | 3,433 s, 5.0 GB | 2,149 s, 6.2 GB |

After the full run: the cheap tier, 45 files, 428 passed; `test_cardinality_corpus` 3,
`test_ordered_corpus` 2, `test_run_corpus` 1.

### Proving, second half

- Cheap tier: 45 files, 429 passed, #239's `bug_` test among them; the harness runs `bug_*`
  functions beside `test_*`. Dataset tier: `test_cardinality_corpus` 3, `test_run_corpus` 1,
  `test_ordered_corpus` 2 (every golden at every mode, 78 s). `test_disassembly_corpus` holds
  every cluster of every golden, non-inner joins included, rebuilt in its own order, to the
  engine's layout.
- `build-test.md`: the cheap-tier row 429; the statistics row 10; the engine-plans row 404 (124,
  58, 111, 111). Seven more queries hold a cluster: tpch anti-join, left-join, q13, q22, q4 and
  semi-join, and tpcds q41, whose only joins are non-inner. Python 915, total 2920.
- The README's run time and design.md's "The committed run" are this run's.

## #239

### The fix

`Estimator.equi_join` takes each side's matched share as the other side's key NDV over
`max(build NDV, probe NDV, domain, 1)`, the divisor of the inner rows; `typed` did not change. A
value of one side finds a match as often as the other's values fill the domain, so on either side
matched rows times fan-out are the inner rows. Where the domain is no wider than the larger key
count it is the old `min(1, other / own)`, so only joins over a wider domain move.

- Left and Right: inner plus the preserved side's unmatched rows. Never below the preserved side,
  since a side's rows are at least its key NDV.
- Full: both of the above.
- Semi keeps the share, anti the rest; the two sum to the side. Right semi and anti use the build's
  NDV, as the mirror.
- Mark keeps every row and reads no share. Inner reads none either, and `SetEstimates._join`, the
  inner-only clusters' path, reads only rows and columns.

Red first, over 1,000 keys with 100 on the preserved side and 143 on the other: the left join came
out at 14.4 of 100 and the anti join at 0.0 against a true 86. Both are regular tests now, and the
`bug_` test is gone. A third, the same anti join as RightAnti with the sides swapped, pins the probe
side's share that Right, RightSemi, RightAnti and Full read; reverting that share alone to
`min(1, build / probe)` turns it red and nothing else in the cheap tier.

The rows are fixed; the key NDV above an outer or anti join is not: `_matched_keys` still cuts it
to the other side's, #240.

### Cardinality golden

tpcds 636 joins: median q-error 1.29 → 1.24, 69.5 → 70.6 % within 2×. tpch keeps 77 joins, 1.07,
70.1 %; four of its lines move. The test compares the golden line by line and holds no
threshold, so the summary line is all that moves besides the joins.

| line | before | after | true |
|---|--:|--:|--:|
| tpcds q75, six left joins | 1,422.7–5,715.6 | 14,283.5–57,190.8 | 13,736–53,812 |
| tpcds q69, two anti joins | 0.0, 0.0 | 949.2, 3,325.1 | 580, 611 |
| tpcds q94, anti | 0.0 | 4.2 | 40 |
| tpcds q16, anti | 0.0 | 71.4 | 850 |

Worse, each for a reason outside the share: q75's root, q 12.33 → 123.44 (its aggregate keys —
brand, class, category, manufacturer — are correlated, so it is estimated at its input, now 99,987
against a true 93,640); q60's three right semi joins, 1.30–2.55 → 2.97–3.06 (the share is right,
0.19 against 0.20, and the probe is 2.9 times under); tpch q4, 1.09 → 1.56 (the 0.2 default for
`l_receiptdate > l_commitdate`, truly 0.63); q95's semi join, 1.33 → 1.89 (share right, input 71.4
against 135). q83 also moves, by 2 %: its root 1.14 → 1.17, its semi join 33.24 → 33.89.

### Which queries can move

Only a plan with a hash join of type Left, Right, Full, semi or anti reads a share. Over the five
`plans.txt` goldens that is 14 tpch queries (anti-join, anti-join-reorder, left-join,
left-join-fixed, left-join-reorder, q4, q13, q16, q18, q20, q21, q22, semi-join,
semi-join-reorder) and 25 tpcds (q5, q6, q8, q10, q14, q16, q33, q35, q38, q40, q41, q56, q58,
q60, q69, q75, q77, q78, q80, q83, q87, q93, q94, q95, q97). tpch q21's anti join has a residual
and is a wall, and still reads a share. Every other plan's estimates are what HEAD gives, bit for
bit. Two checks: `join_order.optimize` of all 555 clustered (query, mode) plans under both
formulas, 25 different, all in the list (tpcds q40, q69, q75, q80, q95 at every mode); and
`run.py` of tpch q3, q5 and q10 at tp1-single into a scratch `--out`, whose three files are
HEAD's byte for byte.

### The run

`run.py --query <the list>` at every mode into the committed outputs, locally, Python 3.13.0,
pandas 3.0.6, pyarrow 25.0.1: tpch 70 runs in 618 s at `--jobs 2`, tpcds 125 in 4,133 s at
`--jobs 1`, none failed. The 1,545 sections outside the list did not move.

- tpch: no plan or cost moved; only the priced sets in `.optimizer.txt`.
- tpcds: the optimized plan changed in q10, q40, q69, q75, q80 and q95 at every mode.
  `test_engine_optimized` over those six at every mode: 30 passed against DuckDB.
  `test_engine_reassembled` cannot differ: disassembly reads no estimate.

| query | tp1-single | tp1-rowgroup | tp4-single | tp4-rowgroup | tp4-sized |
|---|---|---|---|---|---|
| q75 | 0.4475 → 0.4186 | 0.3932 → 0.3767 | 0.4431 → 0.4188 | 0.3859 → 0.3720 | 0.3859 → 0.3720 |
| q80 | 0.3301 → 0.3306 | 0.3372 → 0.3302 | 0.3956 → 0.3894 | 0.3966 → 0.3921 | 0.3966 → 0.3921 |
| q69 | 0.7296 → 0.7302 | 0.6582 → 0.6588 | 0.6552 → 0.6554 | 0.6552 → 0.6554 | 0.6552 → 0.6554 |
| q95 | −1,920 bytes | −1,920 | −1,920 | −1,920 | −1,920 |

q10's and q40's plans changed at the same cost.

- q75: DPhyp keeps the plan's order in all 25 calls, and the ratio is below Task 12's and its
  predecessor's at every mode (0.4384 and 0.4299 at tp1-single and tp4-single before Task 12).
- q69 is dearer by 0.04–0.09 %. Its plan joins customer-demographics (r2), then the semi join,
  then the web and catalog anti joins. DPhyp now moves the semi join below r2 and keeps the anti
  order; HEAD's tree swapped the two anti joins. r2 cannot go last without a replan: both anti
  edges' TES hold it (`r0+r1+r2-r5+r6`), because CD-A adds the semi join's whole left side, `assoc`
  of a semi join under an anti join being false. That is CD-A's correct-but-not-complete limit.
  HEAD reached r2 last only through three replans (q-errors 8.74, 618 and 603, the last two on
  builds priced at 0 rows), whose memory source `m3` dropped the dependency, and joined r2 over
  603 rows. Now no build misses by more than 2×, nothing replans, and r2 joins 652 customers.
- q80 at tp1-single is dearer by 0.14 %, cheaper by 1.1–2.1 % at the other four. Each channel's
  left join to its returns is no longer priced far below its preserved side, so DPhyp joins it
  last, above the dimensions, where HEAD joined it right after the dates; the replans then keep
  different builds.
- Totals: cheaper and dearer runs, both medians and the 1.4 counts are unchanged. DPhyp is called
  3,249 times (3,221), 746 reorder (808), 45 stop at the budget; 584 flips (590); 710 replans
  (726), 107 refused (91).

### Proving

Cheap tier: 45 files, 431 passed. Dataset tier: `test_cardinality_corpus` 3,
`test_ordered_corpus` 2, `test_run_corpus` 1. `build-test.md`: the prototype row 431, Python 917,
total 2922. design.md's limits section was measured again over the fixed estimator: tpcds less
q64 is 599 joins, median 1.17, 74.5 % within 2×; `a != b` as one value of the larger NDV gives
1.23 against 1.24, and tpch q21 goes 2.56 → 1.10, two errors cancelling. The non-NULL-share
experiment still takes q93 from 10.7 to 1.03. Its "86 worse, 64 better" did not reproduce at HEAD
under any counting tried, so design.md now gives the re-run's: rows and both matched shares
scaled, a join counted where its q-error moves by more than 1 %, 157 worse and 175 better; q64's
middle joins worst, 16.6 → 26.4.

## Task 13

### What it does

- **Two passes.** `pipeline.filtered` is everything before the run: `named`, pass 1 on the engine's
  plan (`Builds.FILTERED`, a filter and no join, today's rule), `ordered` with pass 1's builds
  known, pass 2 on the ordered plan (`Builds.FILTERED_OR_JOINED`). No DPhyp call follows pass 2;
  replans still order again where a build misses. `run_optimized` and `test_engine_dynamic_filters`
  both call it. The rule is an enum, not a bool, at every call.
- **Names before the filters.** Task 11's note above ("a cluster inside a build is read from memory
  before naming and gets no number") no longer applies: `named` runs first, pass 1's builds hold
  no join, so the numbers are the engine plan's as before. A pass-2 build keeps its clusters' ids;
  their DPhyp calls keep their headings, and the build's line names them (`build p2 read from
  memory: 48625 rows, 6673962 bytes, holding MJ2, MJ3`). Its joins leave `.cpu.txt` with the build,
  as a replan's sources' do.
- **Lift.** `lift` returns every (scan, column) the key reaches: `GpuUnion` and `GpuInterleave` fan
  out by ordinal; a hash join of any type passes into the side the column comes from (`own_output`),
  a mark column stops. One candidate per scan, each on its own footers; the branches share the one
  probe. A branch that cannot lift stays whole and the others are pruned. No width check at a
  union: its branches have its width by construction.
- **The key lifts through non-inner joins, in this task.** The argument per type is design.md's
  "Why a non-inner join passes the key". It is what prunes q40 (through `Right`, catalog_returns
  building), q78 (store_sales under `Filter(IS NULL) ← Right`) and q80 (three channels), all in
  pass 1.
- **NULLs.** `join_types.equates_nulls` is the one rule for whether a join matches NULL to NULL:
  the plan's `null_equals_null`, or an anti or mark join whatever the plan says (#80, #59);
  `joins.py` and the candidates both read it. `summarize` records whether a build key is NULL.
  A join that equates NULLs stays a candidate; where its build holds a NULL key, that key prunes
  nothing, since the NULL matches the NULL probe keys any row group may hold. Before this, an
  anti or mark candidate whose build held a NULL lost its match when its probe's NULL rows were
  pruned: a `LeftAnti` answered a row the plan did not, a `LeftMark` flipped a mark to false.
  The corpus's one such candidate is q16's pass-2 `LeftAnti` (`cs_order_number →
  cr_order_number`), with no NULL in either column at sf1.
- **The three traps.** `apply(plan, tables, batching, run, builds, kept)`: `run(side, frames)` gets
  the frames of every source made so far, `kept`'s included; names continue after `kept`'s; a
  candidate whose build holds another candidate's join waits a round. `kept_estimates` estimates
  the kept builds in the order made, each over those before it.
- **A scan pruned in the same round.** A candidate whose build holds the scan another ready
  candidate prunes waits a round too. Probed in the same round, its build read the scan unpruned
  and then became a memory source, while the report still counted the scan narrowed. The topmost
  ready join waits for nothing, so a round is never empty.
- **TES clusters (Task 12).** A rebuilt non-inner join keeps its plan type and build side, so
  `PRUNES_PROBE` judges it as written. A pass-2 build may hold a non-inner edge (q58's semi join);
  a cluster cut by a pass-2 source is reassembled in a replan with the source opaque and its TES
  from the joins left, as `left-join-fixed`'s `m0` is. A rebuilt root's shuffle-back is an
  `GpuEmitPartitions`, which `lift` passes. The memory source copies `hashed_on`, so a hash read
  above stays met.
- **Report.** `OptimizerReport` holds two `FilterPass`es; `.optimizer.txt` prints the second as
  `dynamic filters after join order`, after the join orders and before the replans. Each probed
  line gains its bytes (`PandasBatch.byte_size`'s measure) and, where it holds clusters, their ids.
  `Fired` gains `probes_after_order` and `narrowed_after_order`; `cost_report.html` a chip `after
  join order`. Every committed `.costs.txt` lacks the two fields until the full run writes them.
- **tpch q5 gains nothing, and nothing is changed for it.** Neither the data nor a derived filter:
  `l_orderkey` is not ordered (lineitem is sorted by `l_shipdate`), so no rule prunes lineitem. At `tp4-sized` alone DPhyp flips `σ orders ⋈ customer` so that orders
  builds; pass 2 probes it (227,597 rows, 3.6 MB) against customer's two row groups and keeps both.
  The ratio is unchanged to four places: the build's bytes move from the main run to the probe.
- **Not in this task.** A row group whose key is all NULL (store_sales group 23) survives every
  pruning; dropping it for a NULL-rejecting join is a rule of its own.

### Red first

- `test_dynamic_filters.py`: the union, computed-branch and interleave tests on no candidate past
  the union; the per-type test on no candidate past a `Left` join (inner passed); the build rule's two tests on `ImportError: Builds`; the nested test on
  the outer build probed first, over the returns unpruned; the second-pass test on `apply()` taking
  no `kept`. `test_replan`: `KeyError: 'p0'` in `kept_estimates`. `test_report`: the pass-2 test on
  `ImportError: FilterPass`. The report-text and cost-report expectations moved with the format.
- Second round, all red first. The NULL-holding build test (LeftAnti, LeftMark, a null-equal
  Inner) on `LeftAnti` pruned to groups 0–1; the NULL-free one on no candidate for the null-equal
  Inner, which the first round dropped; the same-round scan test on the returns kept whole, 3 → 3.
  Honouring only the plan's flag, not anti and mark, fails the NULL-holding test.
- Mutants caught: every join lifted as inner (the per-type test); one sweep instead of rounds (the
  nested test); names restarting per call (the nested and second-pass tests); pass 2 on the narrow
  rule (`test_report`'s pass-2 test); the pass-2 block not rendered (`test_report_text`).

### Sample

`run.py` into a scratch `--out`, every mode, locally, Python 3.13, pandas and pyarrow as Task 12's:
tpch q5, q20; tpcds q5, q7, q10, q15, q40, q58, q64, q77, q78, q80. 60 runs, none failed, 39 min. q20, q7,
q15 and q77 fired before; their `.cpu.txt`, `.optimizer.txt` and `.costs.txt` sections equal the
committed ones at every mode once the new bytes and the two `fired` fields are taken out, and so do
tpch q5's at the four modes where pass 2 does not fire.

Ratio, optimized over planned, committed → this run:

| query | filters (pass 1 / pass 2) | row groups | tp1-single | tp1-rowgroup | tp4-single | tp4-rowgroup | tp4-sized |
|---|---|---|---|---|---|---|---|
| tpch q5 | 0 / 1 at tp4-sized | customer 2 → 2 | 0.9835 = | 0.9835 = | 0.9874 = | 0.9874 = | 0.9913 = |
| tpch q20 | 1 / 0 | partsupp 7 → 7 | 1.0000 = | 1.0000 = | 1.0000 = | 1.0000 = | 1.0000 = |
| tpcds q5 | 5 / 0 (union branches) | 47 → 7 | 1.0005 → 0.4478 | 1.0005 → 0.2235 | 1.0002 → 0.2234 | 1.0002 → 0.2238 | 1.0002 → 0.2234 |
| tpcds q7 | 1 / 0 | 24 → 7 | 0.7657 = | 0.5057 = | 0.7228 = | 0.4163 = | 0.4155 = |
| tpcds q10 | 3 / 1 | 42 → 5; cd 16 → 16 | 0.7516 = | 0.7212 = | 0.7181 = | 0.7181 = | 0.7489 = |
| tpcds q15 | 1 / 0 | 12 → 1 | 0.0894 = | 0.0894 = | 0.0930 = | 0.0930 = | 0.0930 = |
| tpcds q40 | 1 / 0 (through `Right`) | 12 → 2 | 0.2176 → 0.0650 | 0.2176 → 0.0650 | 0.2629 → 0.0708 | 0.2629 → 0.0708 | 0.2629 → 0.0708 |
| tpcds q58 | 0 / 3 | 42 → 4 | 0.1453 → 0.0714 | 0.1453 → 0.0275 | 0.1826 → 0.0321 | 0.1826 → 0.0321 | 0.1826 → 0.0321 |
| tpcds q64 | 0 / 6 | ss 48 → 13; cd 64 → 64 | 0.8298 → 0.6729 | 0.9084 → 0.4354 | 0.8795 → 0.7318 | 0.9238 → 0.4909 | 0.8795 → 0.4679 |
| tpcds q77 | 5 / 0 | 47 → 7 | 0.4428 = | 0.2166 = | 0.1918 = | 0.1918 = | 0.1918 = |
| tpcds q78 | 1 / 0 (through `Right`, `IS NULL`) | 24 → 7 | 0.8971 → 0.7833 | 0.8971 → 0.6554 | 0.9301 → 0.8165 | 0.9301 → 0.6887 | 0.9301 → 0.6887 |
| tpcds q80 | 3 / 0 (through `Right`) | 42 → 5 | 0.3306 → 0.1520 | 0.3302 → 0.0754 | 0.3894 → 0.0822 | 0.3921 → 0.0822 | 0.3921 → 0.0822 |

- Row groups are the same at every mode. tpcds q5: store_sales 24 → 3, store_returns 3 → 1,
  catalog_sales 12 → 1, catalog_returns 2 → 1, web_sales 6 → 1; the web returns branch, under a
  `Left` join with one row group, prunes nothing. q58: catalog_sales 12 → 1, store_sales 24 → 2,
  web_sales 6 → 1, each build the week's 7 dates, each holding its cluster (`MJ2`, `MJ3`, `MJ4`).
- q64's pass 2 nests: `p2` (holding `MJ2`, `MJ3`) is probed first, then `p4` (holding `MJ2`), which
  reads `p2` from memory. Its four customer_demographics candidates keep all 16 groups.
- q10: the pass-2 probe makes exact the build whose miss replanned at tp1-single, so that replan
  is gone; the ratio is the same to four places.
- Largest probed build: by rows, tpch q5 at tp4-sized, 227,597 rows, 3,641,552 bytes; by bytes,
  tpcds q64's `p5`, 49,247 rows, 6,814,977 bytes at tp4-rowgroup. Outside these two queries the
  largest is tpch q20's, 2,127 rows.

The two corpus files narrowed to the sample's twelve queries, at every mode, against DuckDB, one
process at a time: `test_engine_dynamic_filters` 11 at each mode but `tp4-sized`, 12 there (tpch q5
has a candidate only there), `test_engine_optimized` 12 at each; 116 cases, all passed, 43 min.

`test_engine_dynamic_filters` runs `pipeline.filtered` and takes a query where a candidate is on the
plan as written or on the order DPhyp gives it before any probe: 71 at tp1-single, 73 at
tp1-rowgroup, tp4-single and tp4-rowgroup, 75 at tp4-sized (58 before). Selecting costs an
`ordered` per clustered plan at import, about 10 s a mode.

### After the NULL and same-round fixes

The sample again, with tpcds q16 added: `run.py` at every mode, 65 runs, none failed, 40 min. Every
section of the twelve earlier queries is byte for byte what the first sample wrote, at every mode:
neither fix moved an output there. The two corpus files over the thirteen queries at every mode:
126 cases, all passed, 44 min.

tpcds q16 rises: 1.0000 → 1.0073 at the tp1 modes, 1.0081 at the tp4 modes. Its pass-2 candidate
is the `LeftAnti` on `cs_order_number`; its build holds `MJ2` (call_center, customer_address,
date_dim, catalog_sales), and catalog_returns' two row groups both stay. The probe runs `MJ2` in
DPhyp's order, customer_address first, estimated at 27,389 rows and truly 70,129 (3.37 MB of hash
join output) where the plan's order, dates first, makes 49,794 (2.39 MB). HEAD's adaptive run
stopped at that build (2,446 against 950) and replanned back to a plan priced as the planned one.
A probe is run whole by `run_plan`, so no build inside it replans: a pass-2 build that prunes
nothing can still cost more than the main run would have.

### A joined build probed only where its keys are few (removed)

This rule was tried and taken out again (next section); the evidence for it stands.

Pass 2 takes a build holding a join only where the estimator puts its key at no more than
`IN_LIMIT` (50) values; a build with a filter and no join keeps pass 1's rule. Past `IN_LIMIT` only
the keys' bounds prune, and the estimator bounds a joined build's key by its base column's range
(a filter on `d_year` does not narrow `d_date_sk`'s), so nothing says such a probe will narrow.
The estimates before each round's probes, at tp1-single:

| query | candidate | estimated key NDV | true NDV | row groups kept |
|---|---|--:|--:|---|
| q16 | `cs_order_number → cr_order_number`, joined | 174 | 607 | 2 of 2 |
| q58 | `d_date_sk → *_sold_date_sk` ×3, joined | 7 | 7 | 1/12, 2/24, 1/6 |
| q64 | `d_date_sk → ss_sold_date_sk` ×2, filter only | 363 | 365, 366 | 6/24, 7/24 |
| q64 | `*_cdemo_sk → cd_demo_sk` ×4, joined | 11,373 | 25–30 thousand | 16 of 16 |
| q10 | `c_current_cdemo_sk → cd_demo_sk`, joined | 52 | 92 | 16 of 16 |

Neither the key's estimated range nor its NDV against the probe column's tells q16 from q64's
dates: both keys' ranges are their base columns' whole range, and q16's 174 of 94,339 is the
narrower share. Only a values list prunes scattered keys, which is what the rule asks for.
`Builds.FILTERED_OR_JOINED` reads the estimates `apply` is given for each round, every build kept so
far measured.

Red first: the unit test on a joined build at `IN_LIMIT` and `IN_LIMIT + 1` estimated keys, and a
filter-only one at a million (`TypeError` on `candidates` taking no estimates), and the pipeline
test over year 5's hundred dates restricting the sales, all of them the sales' keys (a pass-2 probe
ran). The narrow toy's predicate became `dk` in [500, 520): year 5 AND `dk < 520` is estimated at 52
values under independence. Dropping the rule fails both.

The sample again, 13 queries at every mode: 65 runs, none failed, 42 min.
- q16: 1.0000 at every mode; its sections equal HEAD's byte for byte.
- q10: its pass-2 probe is gone and its sections equal HEAD's byte for byte; its ratio was the
  same either way, the probe making exact a build whose miss HEAD replans.
- q64 keeps its two date prunings, 48 → 13 row groups, and loses its four customer-demographics
  probes, which pruned nothing but made four 48-thousand-row builds exact: the run replans 9 times
  instead of 4. At tp1-single and tp1-rowgroup its stopped runs cost 948 and 734 million against 42
  million, its probes 2.4 million against 821 and 504 million. Ratio, HEAD → before the rule →
  now:

| tp1-single | tp1-rowgroup | tp4-single | tp4-rowgroup | tp4-sized |
|---|---|---|---|---|
| 0.8298 → 0.6729 → 0.7133 | 0.9084 → 0.4354 → 0.5529 | 0.8795 → 0.7318 → 0.7456 | 0.9238 → 0.4909 → 0.5262 | 0.8795 → 0.4679 → 0.4817 |

- Every other query's sections are byte for byte what the second sample wrote.
- `test_engine_dynamic_filters` selects 67 queries at four modes and 68 at tp4-sized; tpcds q16 and
  q10's pass-2 candidates no longer count.
- The two corpus files over the 13 queries at every mode: 121 cases, all passed, 44 min
  (`test_engine_dynamic_filters` 11 at each mode but tp4-sized, 12 there; tpcds q16 has no candidate
  left).

### Probes run adaptively

The NDV gate is gone: in tpcds q6, DPhyp's order makes the month's dates (date_dim ⋈ distinct
`d_month_seq`) the store_sales join's build, its key estimated at 916 values and truly 31, which
prune store_sales 24 → 2 row groups; the gate dropped it, and cost q64 more than it saved q16.
Estimated NDV gates a joined build in neither direction (q6 916 against 31, q16 174 against 607).
Pass 2 is back to a filter, a join or both.

What caused q16 was the probe running a misordered cluster with no replan, so every probe plan now
runs through `replan.run_adaptive` (`pipeline.filtered` builds the probe), as the main plan does:
- its last run is the build, priced as a probe plan; each run it stopped is priced as a stopped
  run (`OptimizedRun.stopped` holds the probes' and the main run's);
- its replans and refused replans are in `FilterPass.replans` and `.refused`, rendered under the
  pass's block as `probe pN replan K: …`, counted in `Fired.replans`, `refused` and the DPhyp counts;
- `apply` hands each probe the builds made so far, (side, frames) by name, so `run_adaptive` knows
  their estimates; `kept_estimates` and the nesting rounds need nothing else;
- a run's memory sources take names from one set shared by the probes and the main run
  (`run_adaptive(taken=…)`); with no probe replan the main run's names are HEAD's;
- a plan's root that declares a hash is read by what consumes the plan (`multijoin.clusters`), so
  a probe that replans keeps the hash its build's join reads.

Red first: the q6-shaped toy (`test_filter_passes`: a year's dates estimated at about 950 keys,
truly 100, pruning the spread 10 → 1) on no pruned scan, under the gate; a build missing inside a
probe replanning there (`FilterPass` had no `replans`); the q16-shaped toy (a fact under two
dimensions, one estimated at a row and truly ten, keys spanning the probe's ten row groups) on
71,472 against 69,552 for the plan without the second pass; shared names on `run_adaptive` taking
no `taken`; the root's hash on `hash_read_above` false. The report-text and count expectations for
probe replans were red with their rendering and counting taken out. The two gate tests are gone.
Cheap tier 46 files, 449 passed; `test_engine_dynamic_filters` selects 71, 73, 73, 73 and 75 again.

The sample at every mode, 23 queries: `run.py` 115 runs, none failed, 74 min; the two answer suites
225 cases, all passed, 60 min. Ratio, HEAD → round 2 (no gate, probes run whole, emulated) → now;
"=" is no change at four places:

| query | tp1-single | tp1-rowgroup | tp4-single | tp4-rowgroup | tp4-sized |
|---|---|---|---|---|---|
| tpcds q6 | 0.1647 → 0.0877 → 0.0877 | 0.1647 → 0.0358 → 0.0358 | 0.1922 → 0.0359 → 0.0359 | 0.1859 → 0.0300 → 0.0300 | 0.1910 → 0.0348 → 0.0348 |
| tpcds q16 | 1.0000 → 1.0073 → 1.0000 | 1.0000 → 1.0073 → 1.0000 | 1.0000 → 1.0081 → 1.0000 | 1.0000 → 1.0081 → 1.0000 | 1.0000 → 1.0081 → 1.0000 |
| tpcds q64 | 0.8298 → 0.6729 → 0.6526 | 0.9084 → 0.4354 → 0.4329 | 0.8795 → 0.7318 → 0.6955 | 0.9238 → 0.4909 → 0.4742 | 0.8795 → 0.4679 → 0.4512 |
| tpcds q75 | 0.4186 → 0.4176 → 0.4176 | 0.3767 → 0.3757 → 0.3757 | 0.4188 → 0.4177 → 0.4177 | 0.3720 → 0.3709 → 0.3709 | 0.3720 → 0.3709 → 0.3709 |
| tpcds q84 | 0.9883 → 0.9934 → 0.9934 | 0.9883 → 0.9934 → 0.9934 | 0.9918 → 0.9942 → 0.9942 | 0.9918 → 0.9942 → 0.9942 | 0.9918 → 0.9942 → 0.9942 |
| tpch q9 | = | 0.9164 → 0.8792 → 0.9089 | 0.9317 → 0.9148 → 0.9317 | = | = |

tpcds q5, q40, q58, q78, q80 and q85 move as in the full run below, the same in round 2 and now;
tpch q2, q5, q20 and tpcds q7, q10, q15, q18, q35, q69, q77 and q91 stay at HEAD's.

- tpcds q84 rises, 0.24–0.51 %, in round 2 and now alike: the second pass, not the probe's
  adaptivity. It is the one ratio above HEAD, and a limit of probing a build that prunes nothing:
  the probe materialises it and the main run reads it exactly, so it gives the main run no miss of
  its own, and the cluster around it is reordered only where another build misses. `MJ1` holds six
  relations; the candidate, `c_current_cdemo_sk → cd_demo_sk`, is its join r2+r3 × r4, and its
  build is {customer_address, customer}, which DPhyp ordered by at 139.5 rows. The probe's first
  run stops at customer_address (464 rows against 69.8); its replan has nothing to reorder, and the
  build comes out at 947 rows, all 16 customer_demographics row groups kept. The main plan reads p0
  exactly, so no build of its own misses and nothing replans. HEAD's main run stopped at the same
  customer_address build and ordered all six relations again, income_band ⋈ household_demographics
  first, which narrow the customers before customer_demographics and store_returns.
- tpch q9: the probe's replan makes a cheaper build than HEAD's but a dearer one than running it
  whole at tp1-rowgroup and tp4-single; never above HEAD.

### The full run with adaptive probes

These are the committed outputs. `run.py` in one invocation, `--jobs 1`, locally, Python 3.13.0,
pandas 3.0.6, pyarrow 25.0.1: 620 runs, none failed, 5 h 17 min, at most 7.0 GiB resident, the host's
available memory never under 3.6 GB. `cost_report.py` over them writes the committed page byte for
byte, and tpch q9 and tpcds q6, q58 and q84 run again at tp1-single and tp4-single with the code as
committed reproduce their `.cpu.txt`, `.optimizer.txt` and `.costs.txt` sections byte for byte.

Against HEAD (bytes, clusters and the two new `fired` fields stripped): 575 of 710 sections are
HEAD's byte for byte; 135 (query, mode) pairs differ, in 30 queries (23 tpcds, 7 tpch; q5, q10 and
q18 in both); 120 pairs in 27 queries change the plan, and ratios move in 70 pairs of 16 queries.
Ratio, HEAD → now, where it moved at four places:

| query | tp1-single | tp1-rowgroup | tp4-single | tp4-rowgroup | tp4-sized |
|---|---|---|---|---|---|
| tpcds q5 | 1.0005 → 0.4478 | 1.0005 → 0.2235 | 1.0002 → 0.2234 | 1.0002 → 0.2238 | 1.0002 → 0.2234 |
| tpcds q6 | 0.1647 → 0.0877 | 0.1647 → 0.0358 | 0.1922 → 0.0359 | 0.1859 → 0.0300 | 0.1910 → 0.0348 |
| tpcds q13 | = | 0.4247 → 0.4064 | 0.5783 → 0.5611 | 0.4230 → 0.4058 | 0.4230 → 0.4058 |
| tpcds q32 | 0.6066 → 0.2136 | 0.6074 → 0.2152 | = | = | = |
| tpcds q40 | 0.2176 → 0.0650 | 0.2176 → 0.0650 | 0.2629 → 0.0708 | 0.2629 → 0.0708 | 0.2629 → 0.0708 |
| tpcds q54 | 0.9652 → 0.5469 | 0.9652 → 0.5469 | 0.9694 → 0.5960 | 0.9694 → 0.5960 | 0.9694 → 0.5960 |
| tpcds q58 | 0.1453 → 0.0714 | 0.1453 → 0.0275 | 0.1826 → 0.0321 | 0.1826 → 0.0321 | 0.1826 → 0.0321 |
| tpcds q64 | 0.8298 → 0.6526 | 0.9084 → 0.4329 | 0.8795 → 0.6955 | 0.9238 → 0.4742 | 0.8795 → 0.4512 |
| tpcds q75 | 0.4186 → 0.4176 | 0.3767 → 0.3757 | 0.4188 → 0.4177 | 0.3720 → 0.3709 | 0.3720 → 0.3709 |
| tpcds q78 | 0.8971 → 0.7833 | 0.8971 → 0.6554 | 0.9301 → 0.8165 | 0.9301 → 0.6887 | 0.9301 → 0.6887 |
| tpcds q80 | 0.3306 → 0.1520 | 0.3302 → 0.0754 | 0.3894 → 0.0822 | 0.3921 → 0.0822 | 0.3921 → 0.0822 |
| tpcds q83 | 0.2338 → 0.2202 | 0.2338 → 0.2202 | 0.2744 → 0.2568 | 0.2744 → 0.2568 | 0.2744 → 0.2568 |
| tpcds q84 | 0.9883 → 0.9934 | 0.9883 → 0.9934 | 0.9918 → 0.9942 | 0.9918 → 0.9942 | 0.9918 → 0.9942 |
| tpcds q85 | 0.8326 → 0.5123 | 0.8326 → 0.5123 | 0.8765 → 0.4203 | 0.8765 → 0.4203 | 0.8765 → 0.4203 |
| tpcds q92 | 0.6218 → 0.2451 | 0.6218 → 0.2451 | = | = | 0.6114 → 0.2238 |
| tpch q9 | = | 0.9164 → 0.9089 | = | = | = |

- The one rise is tpcds q84, +0.24–0.51 % at every mode, the limit design.md's "Probe plans run
  adaptively" states: its probe prunes nothing, and the build, read exactly, gives the main run no
  miss to replan at, where HEAD's main run replanned at that build. q64's main run, by contrast,
  replans with its pass-2 builds as relations. No run is dearer than planned anywhere (HEAD: tpcds q5 at every mode).
- Plans changed with the ratio the same at four places: tpch q2, q5 (tp4-sized), q8 (one mode),
  q9 (three modes), q10, q14, q18; tpcds q10, q16, q18, q25, q35, q69, q91 — mostly pass-2 probes
  that prune nothing. tpcds q29 differs in its report and costs only.
- Largest probed build: tpch q9 at four modes, 319,404 rows, 15,331,392 bytes, keeping all of
  partsupp's 7 row groups.
- 110 (query, mode) pairs have a pass-2 candidate, in 25 queries: tpch q2, q5, q8, q9, q10, q14,
  q18; tpcds q6, q10, q13, q16, q18, q25, q29, q32, q35, q58, q64, q69, q75, q83, q84, q85, q91,
  q92. No named tpch query has a dynamic filter at any mode.
- Answers: `test_engine_dynamic_filters` and `test_engine_optimized` over the 135 changed (query,
  mode) pairs against DuckDB, one process at a time: 270 cases, all passed, 67 min; both suites
  selected every pair.
- Totals are design.md's "The committed run".

Also pinned: a probe replan's DPhyp call counted (dphyp 3); a probe's refused miss after its
union branch streamed; a probe whose root is a four-lane coalesce hashed on `k`, which replans to a
root on another column and must be shuffled back — red, a wrong answer, with
`multijoin.clusters`' root-hash rule reverted; a probe with no miss equal to its side run whole;
the q6 toy's build a partial cluster.

Proving: cheap tier 46 files, 452 passed; dataset tier green; the corpus tier's two dynamic-filter
files over every changed (query, mode), 270 cases, above; `run.py` full, above.

### A reorder after the probes, tried and dropped

A second `ordered` call after pass 2, made once where a probed build was more than 2× off the
estimate DPhyp ordered by, with every build known, was implemented and sampled at every mode over
17 queries, and taken out again. Ratio, HEAD → without it → with it:

| query | tp1-single | tp1-rowgroup | tp4-single | tp4-rowgroup | tp4-sized |
|---|---|---|---|---|---|
| tpcds q84 | 0.9883 → 0.9934 → 0.9907 | 0.9883 → 0.9934 → 0.9907 | 0.9918 → 0.9942 → 0.9955 | 0.9918 → 0.9942 → 0.9955 | 0.9918 → 0.9942 → 0.9955 |
| tpch q9 | = | 0.9164 → 0.9089 → 0.9461 | 0.9317 → 0.9317 → 0.9526 | 0.9015 → 0.9015 → 0.9221 | 0.9048 → 0.9048 → 0.9257 |
| tpcds q6 | 0.1647 → 0.0877 → 0.0827 | 0.1647 → 0.0358 → 0.0308 | 0.1922 → 0.0359 → 0.0312 | 0.1859 → 0.0300 → 0.0300 | 0.1910 → 0.0348 → 0.0300 |

- q84 took HEAD's shape, ((income_band ⋈ household_demographics) ⋈ p0) ⋈ customer_demographics ⋈
  store_returns. At tp1 the difference to HEAD fell from 136,584 to 64,385 bytes, exactly p0's
  size; at tp4 it rose by 67 KB.
- tpch q9's build was 319,404 rows against 1,200,243 estimated; the second call kept DPhyp's tree
  but oriented one more join the other way, and the ratio rose above HEAD's at four modes, by 2–4 %.
- Fourteen other queries were unchanged by it. q84 stays the one ratio above HEAD.

### The full run under the NDV gate (superseded)

The committed outputs in the working tree are this run's; they predate adaptive probes and the
gate's removal, and the next full run replaces them.

`run.py` in one invocation, both benches, every mode, `--jobs 1`, into the committed outputs, locally,
Python 3.13.0, pandas 3.0.6, pyarrow 25.0.1: 620 runs, none failed, 5 h 24 min, at most 7.4 GiB
resident (one worker); the host's available memory never fell under 3.4 GB. A first attempt, before
the joined-build rule, was stopped at 148 runs when an editor's language server took 6 GB and
available memory fell to 1.28 GB; it wrote nothing.

Against HEAD, with the bytes and clusters taken off the `read from memory` lines and the two new
`fired` fields off `.costs.txt`: 639 of 710 sections are HEAD's byte for byte. 71 (query, mode)
pairs differ, in 16 queries; 56 of them change the optimized plan. No ratio rises; none is dearer
than planned anywhere (HEAD: tpcds q5, by under 0.06 %, at every mode). Ratio, HEAD → now, where it
moved at four places:

| query | tp1-single | tp1-rowgroup | tp4-single | tp4-rowgroup | tp4-sized |
|---|---|---|---|---|---|
| tpcds q5 | 1.0005 → 0.4478 | 1.0005 → 0.2235 | 1.0002 → 0.2234 | 1.0002 → 0.2238 | 1.0002 → 0.2234 |
| tpcds q13 | = | 0.4247 → 0.4064 | 0.5783 → 0.5611 | 0.4230 → 0.4058 | 0.4230 → 0.4058 |
| tpcds q32 | 0.6066 → 0.2136 | 0.6074 → 0.2152 | = | = | = |
| tpcds q40 | 0.2176 → 0.0650 | 0.2176 → 0.0650 | 0.2629 → 0.0708 | 0.2629 → 0.0708 | 0.2629 → 0.0708 |
| tpcds q54 | 0.9652 → 0.5469 | 0.9652 → 0.5469 | 0.9694 → 0.5960 | 0.9694 → 0.5960 | 0.9694 → 0.5960 |
| tpcds q58 | 0.1453 → 0.0714 | 0.1453 → 0.0275 | 0.1826 → 0.0321 | 0.1826 → 0.0321 | 0.1826 → 0.0321 |
| tpcds q64 | 0.8298 → 0.7133 | 0.9084 → 0.5529 | 0.8795 → 0.7456 | 0.9238 → 0.5262 | 0.8795 → 0.4817 |
| tpcds q78 | 0.8971 → 0.7833 | 0.8971 → 0.6554 | 0.9301 → 0.8165 | 0.9301 → 0.6887 | 0.9301 → 0.6887 |
| tpcds q80 | 0.3306 → 0.1520 | 0.3302 → 0.0754 | 0.3894 → 0.0822 | 0.3921 → 0.0822 | 0.3921 → 0.0822 |
| tpcds q83 | 0.2338 → 0.2202 | 0.2338 → 0.2202 | 0.2744 → 0.2568 | 0.2744 → 0.2568 | 0.2744 → 0.2568 |
| tpcds q85 | 0.8326 → 0.5123 | 0.8326 → 0.5123 | 0.8765 → 0.4203 | 0.8765 → 0.4203 | 0.8765 → 0.4203 |
| tpcds q92 | 0.6218 → 0.2451 | 0.6218 → 0.2451 | = | = | 0.6114 → 0.2238 |

- Unions: q5 and q54 (catalog and web sales under one union, 12 → 2 and 6 → 2). Non-inner joins:
  q40, q78, q80. After join order: q58, q83 (the same week shape), q85 (a joined build of one
  `cd_demo_sk`, 16 → 1), q64's two date builds. q32 and q92 prune their fact a second time where
  the correlated subquery reads it again, its date build reaching the fact only after the order.
- Changed with the ratio the same to four places: tpcds q25 and q29 (a pass-2 probe on `ss_item_sk`
  over two row groups, keeping both), tpch q14 at every mode (a filtered build of 75,983 rows
  flipped to the build side, against part's two row groups) and tpch q5 at tp4-sized (227,597
  rows, customer's two row groups). q13's pass-2 build, a filter and no join (37,869 rows at the
  tp1 modes and tp4-single, 20,103 at the other two), prunes nothing; made exact, it takes one
  replan off at every mode, and the ratio moves at four.
- Largest probed build: tpch q5 at tp4-sized, 227,597 rows, 3,641,552 bytes, by rows and by bytes;
  then tpcds q13's 37,869 rows, 3,029,520 bytes, and tpch q14's 75,983 rows, 1,823,592 bytes.
- No named tpch query has a dynamic filter at any mode, before or after join order.
- Answers: `test_engine_dynamic_filters` and `test_engine_optimized` over the 71 changed (query,
  mode) pairs against DuckDB, one process at a time: 142 cases, all passed, 46 min. Every pair was
  selected by both. `test_engine_answers` and `test_engine_reassembled` run the planned and the
  reassembled plans, which no filter touches, so they were not run.
- `cost_report.py` over the regenerated `.costs.txt` writes the committed page byte for byte.
- Totals are design.md's "The committed run": 740 probes before the order narrowing 735 scans, 66
  after narrowing 45; DPhyp 3,315 calls (3,249 at HEAD), 69 at the budget, all q64's, which now
  replans 9 times at every mode (7 at HEAD).

## Task 14

### Generation

`gen_duckdb_cost.sh --gen` profiles every `<bench>-queries/*.sql`, and `--extract-only` every
committed profile, so the 21 named tpch queries have a `.duckdb_cost.txt`, a pass-1 profile and
pass-2 bounds. Its raw profiles go to a `mktemp -d` rather than fixed `/tmp` names, so concurrent
runs do not share scratch files; the import cache below is still shared. Either mode exits 1 on an
empty query or profile set, and `--gen` on any failed query. Run as written with DuckDB 1.5.4 and
pyarrow 19.0.1: tpch 43, tpcds 99, none failed, 1 min 54 s, peak RSS 0.6 GB. All 363 committed
outputs were rewritten and came out byte for byte; `--extract-only` reproduces the 21 new goldens.

The import caches a database in `<data>/.duckdb_cache/`. With the sf1 directory a symlink that
lands in the link's target; a `TESTDATA_DIR` whose sf1 directories hold one link per parquet file
keeps it local. DuckDB's memory limit can be set in a `.duckdbrc` under `HOME`; this run set 2 GB.

### The engine's report

`cost-report` looked a micro-query's `.cost.txt` section up under the registry's underscore name,
`scan_limit`, and the section is `== scan-limit`, so every named row's PeacockDB Σout was "—".
`build_dataset` now passes the hyphenated stem, as the DuckDB lookup did.
`a_micro_query_is_priced_from_its_hyphenated_section` was red on all 21 and
`every_registry_query_has_a_duckdb_cost` on the same 21 before generation. The PR comment is
56,706 of 65,536 bytes.

Three places still say the named queries have no DuckDB figure: `cost_report.py`'s legend,
`scripts/exec_model/README.md`'s `cost_report.html` entry, and the committed `cost_report.html`.
They change with the next render of the prototype's report, left to the merge with Task 13; a
render of 0205232c's outputs into a scratch directory shows all 21 rows with DuckDB and a ratio.

### Room for the optimizer

Report = the engine's figure at the last mode its cpu run is enabled at, over DuckDB's, as
`cost-report` shows it (tp4-sized for all but rollup-over-join, tp1-rowgroup). tp1 = the same at
tp1-single. Projected = `cost_report.py`'s final ratio over the prototype's committed outputs.
Forwarders cost 0, so tp4's excess is the scatter alone. n/c = not comparable: the oracle charges
DuckDB a full scan its default run avoids (#241).

| query | report | tp1 | projected | room |
|---|--:|--:|--:|---|
| left-join-reorder | 1.70 | 0.89 | 0.31 | taken: nation's one row joins customer first, then the left join; 0.35 of the plan at tp1 |
| semi-join-reorder | 1.29 | 0.72 | 0.48 | taken, the same move; 0.67 |
| anti-join-reorder | 1.24 | 0.70 | 0.50 | taken, the same move; 0.72 |
| left-join-fixed | 1.65 | 0.86 | 0.86 | not taken, see below |
| anti-join | 1.45 | 1.05 | 1.05 | exchange, not order |
| semi-join | 1.28 | 0.75 | 0.75 | exchange, not order |
| mixed-join | 1.22 | 0.79 | 0.79 | none: two large sides |
| hash-join | 1.02 | 0.71 | 0.71 | none: two large sides |
| left-join | 1.02 | 0.71 | 0.71 | none: two large sides |
| join-int | 0.76 | 0.47 | 0.47 | none |
| rollup-over-join | 0.43 | 0.43 | 0.43 | none |
| filter-project | 1.43 | 1.43 | 1.43 | none: no join |
| scan-limit | n/c | — | n/c | none for order: the limit; DuckDB's figure is a full read, #241 |
| aggregate-groupby | 0.39 | 0.39 | 0.39 | none: no join |
| shuffle-additive | 0.39 | 0.39 | 0.39 | none: no join |
| shuffle-additive-avg | 0.31 | 0.31 | 0.31 | none: no join |
| shuffle-stddev | 0.26 | 0.26 | 0.26 | none: no join |
| nested-limits | n/c | n/c | n/c | no order: a 5-row cross join; DuckDB's figure is a full scan, #241 |
| cross-join | 3.64 | 3.64 | 3.64 | none: kilobytes |
| nested-loop-join | 4.10 | 4.10 | 4.10 | none: kilobytes |
| nested-loop-left-join | 4.73 | 4.73 | 4.73 | none: kilobytes |

- The three reorder queries: DuckDB keeps the written order in all three, joining the 150 k
  customers to orders before nation's filter, so the prototype ends below DuckDB.
- left-join-fixed is the one query with room the optimizer does not take. The inner join to
  lineitem on `o_orderkey` rejects every row the left join null-extends, so the left join is an
  inner one; as written, TES keeps customer ⟕ orders together, and its 1.55 M rows are 47 MB of
  the 83 MB at tp1-single. Simplifying the outer join would let orders meet lineitem's 2,528 rows
  first. DuckDB does not do it either.
- anti-join and semi-join: at tp4 the 1.5 M-row orders side is scattered, 195 MB, to meet a
  30 k-key build; broadcasting the build would drop it. That is the exchange, not DPhyp's order.
  At tp1 anti-join's 155 MB is `SELECT *` of 1.2 M orders, joined and unloaded, as DuckDB pays it.
- hash-join, left-join and mixed-join scatter both sides at tp4, 182–213 MB, and both are large;
  at tp1 they sit at 0.71–0.79.
- filter-project pays its 58 MB output twice, filter and unload, at 24 bytes a row (`l_quantity`
  is Decimal128); DuckDB's scan emits 16 bytes a row once.
- scan-limit: the engine reads all of lineitem, 1.05 GB, for ten rows, and tp4-rowgroup stops
  after 21.5 MB. DuckDB stops after one 2,048-row vector, but the oracle charges every surviving
  row group, 1.04 GB, so the 1.01 is the oracle's, not DuckDB's (#241). A limit the scan honours
  is the room there.
- nested-limits: DuckDB's default run reads `part` by a `rowid` range (pass 2's bounds); pass 1
  turns that off and the golden charges the full 200,000-row scan, so its 0.34 flatters the
  engine (#241). Its one join is a 5-row cross join, with no order to choose.
- The aggregates are their scan alone; DuckDB counts its aggregate's input as materialized.
  cross-join and the nested-loop joins are region × nation, 5–49 KB, where fixed overhead sets the
  ratio.

## Task 15

Tasks 15 and 16 are one commit: the log and the page are two renderings of one report, and both
moved its data.

### What the log says

- A section with a DPhyp call opens with an `estimates:` line — rows by selectivity, |L| × |R|
  over the larger key NDV or the shared domain, 0.2 a residual, a non-inner join by its type;
  C_out each join's rows × the bytes per row of the columns read above it, summed; a relation's
  B/row its whole row — and any section with a call or a filter with a `names:` line: `(f)`,
  `agg(X)`, `t#2`, `p0`, `m0`. Once a section, not once a call (588 calls in tpcds tp4-single).
- A call is headed `MJ<n>: K relations, ordered before the run` (`ordered again` in a replan).
  Each `rN` is named as the page names it (`names.py`): `region#1 (f)`, `agg(MJ2)`, `p0`. It lists
  `rows N exact|est|measured`, B/row of its whole output, and the NDV of each key-edge column. A
  filter's predicate is not printed: the plan diff holds it.
- An aggregate's output column is bracketed and marked, `r5.[min(partsupp.ps_supplycost)]
  (aggregate output)`, also when a memory source holds it (`EngineNode.aggregates`); any
  non-identifier name is bracketed. A non-inner edge says which side it keeps; a side of several
  relations, `applies once r0+r1 are joined`; a residual beside keys, `then a residual on …
  keeping 0.2`; a residual alone is its own line.
- `plan's order <tree>, build first` (`previous order` in a replan), `DPhyp priced N sets, chose
  <tree>` (`: the plan's order` for the same shape), each join `R rows x W B = C B`, and `C_out X
  B; the plan's order, priced by the same estimates, Y B`.
- Filters: a candidate names its scan by alias, `p_partkey -> partsupp#1.ps_partkey: row group
  ranges sum to 1.00x the column's range, ordered (under 1.5)`; a scan by its alias, `partsupp#1: row groups 7 -> 7`, each key with its build, `ps_partkey by p0.p_partkey in
  [249, 199723]`; `build p0 = part (f), run first: 747 rows, 22504 B; the plan reads it from
  memory`. A replan: `the build of MJ2's join on cs_item_sk = i_item_sk came out 7739 rows, est
  469.3 (q-error 16.49)`, then `kept as memory sources: m3 (7739 rows) = [p0] join catalog_sales`.
- The plan diff is unchanged, byte for byte, in a file of its own: `run.py` writes
  `<mode>.plandiff.txt`, a `== <query>` section each, `plan diff` and its lines or `plan unchanged`
  (`report_text.plan_diff`). The log ends with `plan unchanged` or `plan diff: this query's section
  of <mode>.plandiff.txt`. GitHub did not display the pushed 2.57 MB tpcds tp4-single log, and the
  diffs were 2.26 MB of it.

### The data behind it

- `JoinOrder` holds `RelationEstimate`s (name, rows, `Certainty`, width, key NDVs, the memory
  sources its name shows), `EdgeReport`s, `planned` (`disassembly.baseline`) and `types`, the
  non-inner joins of both trees with the build on the left; `changed` is `oriented != planned`.
- `cardinality.certainty`: a scan's rows are counted, a memory source's measured, a node passing
  its input's rows on as known as the input, a union as its least known branch, the rest estimated.
- `disassembly.join_type_of` is the one rule for a join's type from its two sides. `report.mask`
  is the one tree mask; `join_order` and `disassembly` read it.
- `BuildMiss` carries `names.join_words` of the join and its cluster's id, and `q_error`.
  `Kept` and `Probed` carry `holds` (`names.described`) and the sources it names
  (`described_sources`). `Pruned` carries the scan's alias, and per filter a `PrunedBy`: column,
  keys, build and build key.
- A table the plan reads more than once is numbered on every scan of it, `EngineNode.alias`, in
  plan order, by `names.aliased` before `named`: one name in every cluster, build and pruned scan
  (q64's two `store_sales` reads are `#1` and `#2`). A memory source made by `names.sourced`
  keeps which of its columns are an aggregate's. Neither is in the plan text.
- `fired` is unchanged; `test_cost_report`'s count moved because the fixture's flip moved to
  `PAIR`, which it holds twice.

### Red first

- `test_names` on `ModuleNotFoundError`, later on `described_sources` missing; `test_report_text`
  on `ImportError: Certainty`, later on `shape` renamed `unordered`; `test_report` on `JoinOrder`'s
  new fields; `test_dynamic_filters` on `PrunedBy` missing; `test_cardinality`'s certainty test,
  green on arrival, then red with the memory-source case taken out. A near-integer NDV (`49.99` →
  `50`) and the residual's aggregate mark each red against the code before them. The plan-wide
  numbering's test goes red with the number given only past a second read.

### Sample

`run.py` at tp1-single and tp4-single, locally, Python 3.13, the committed pandas and pyarrow:
tpch q2, q5, q18, left-join-reorder; tpcds q5, q58, q64, q84. 16 runs, none failed, about 7 min.

- Every `.cpu.txt` and `.costs.txt` section is the committed one byte for byte, and so is every
  `.optimizer.txt` section from `plan diff` on.
- Bytes before the plan diff, committed → now: tpch q2 1,803 → 3,324, q5 816 → 1,785, q18
  1,239–1,293 → 2,448–2,509, left-join-reorder 403 → 1,173; tpcds q5 3,293 → 5,340, q58 ~8,000 →
  14,500, q64 ~25,500 → 39,000, q84 1,589 → 2,923.
- tpcds tp4-single's log was projected at about 669,000 B with the diffs out; the full run wrote
  669,960 (below).
- q2 and q58 again at tp1-single after the split: `.cpu.txt` and `.costs.txt` the committed ones,
  each `.plandiff.txt` section the committed log's text from `plan diff` on, byte for byte.

The committed q2 section at tp1-single:

    == q2
    estimates: a filtered relation's rows by its predicate's selectivity; a join's |L| x |R| / max(key NDVs, the key domain both share), x 0.2 per residual, a non-inner join by its type; C_out = each join's rows x the bytes per row of the columns read above it, summed, in bytes; a relation's B/row is its whole row
    names: (f) a filtered relation, agg(X) an aggregate over X, t#2 the plan's second read of table t, p0 a build a dynamic filter ran first and m0 one a replan kept, both read back from memory
    dynamic filters on the plan as written
      candidate p_partkey -> partsupp#1.ps_partkey: row group ranges sum to 1.00x the column's range, ordered (under 1.5)
      partsupp#1: row groups 7 -> 7 (0-6)
        ps_partkey by p0.p_partkey in [249, 199723]
      build p0 = part (f), run first: 747 rows, 22504 B; the plan reads it from memory
    MJ1: 6 relations, ordered before the run
      r0 region#1 (f): rows 1 est, 4.1 B/row; NDV r_regionkey 1
      r1 nation#1: rows 25 exact, 19.5 B/row; NDV n_nationkey 25, n_regionkey 5
      r2 supplier#1: rows 10000 exact, 165.4 B/row; NDV s_suppkey 10000, s_nationkey 25
      r3 p0: rows 747 measured, 26.2 B/row; NDV p_partkey 747
      r4 partsupp#1: rows 800000 exact, 32.4 B/row; NDV ps_partkey 200000, ps_suppkey 10000, ps_supplycost 99865
      r5 agg(MJ2): rows 160000 est, 24.2 B/row; NDV ps_partkey 160000, [min(partsupp.ps_supplycost)] 160000
      edge r3-r4: r3.p_partkey = r4.ps_partkey
      edge r2-r4: r2.s_suppkey = r4.ps_suppkey
      edge r1-r2: r1.n_nationkey = r2.s_nationkey
      edge r0-r1: r0.r_regionkey = r1.n_regionkey
      edge r3-r5: r3.p_partkey = r5.ps_partkey
      edge r4-r5: r4.ps_supplycost = r5.[min(partsupp.ps_supplycost)] (aggregate output)
      plan's order ((r0 (r1 (r2 (r3 r4)))) r5), build first
      DPhyp priced 19 sets, chose (((r0 r1) r2) ((r3 r5) r4))
      oriented (((r3 r5) r4) ((r0 r1) r2)), build first
      join r3 x r5: 747.0 rows x 42.4 B = 31654 B
      join r3+r5 x r4: 2988.0 rows x 34.4 B = 102712 B
      join r0 x r1: 5.0 rows x 15.3 B = 77 B
      join r0+r1 x r2: 2000.0 rows x 172.5 B = 345002 B
      join r3+r4+r5 x r0+r1+r2: 597.6 rows x 190.6 B = 113918 B
      C_out 593363 B; the plan's order, priced by the same estimates, 1615081 B
    MJ2: 4 relations, ordered before the run
      r0 region#2 (f): rows 1 est, 4.1 B/row; NDV r_regionkey 1
      r1 nation#2: rows 25 exact, 8.2 B/row; NDV n_nationkey 25, n_regionkey 5
      r2 supplier#2: rows 10000 exact, 12.2 B/row; NDV s_suppkey 10000, s_nationkey 25
      r3 partsupp#2: rows 800000 exact, 32.4 B/row; NDV ps_suppkey 10000
      edge r2-r3: r2.s_suppkey = r3.ps_suppkey
      edge r1-r2: r1.n_nationkey = r2.s_nationkey
      edge r0-r1: r0.r_regionkey = r1.n_regionkey
      plan's order (r0 (r1 (r2 r3))), build first
      DPhyp priced 6 sets, chose (((r0 r1) r2) r3)
      oriented (((r0 r1) r2) r3), build first
      join r0 x r1: 5.0 rows x 4.1 B = 21 B
      join r0+r1 x r2: 2000.0 rows x 8.1 B = 16250 B
      join r0+r1+r2 x r3: 160000.0 rows x 24.2 B = 3880000 B
      C_out 3896271 B; the plan's order, priced by the same estimates, 49280000 B
    dynamic filters after join order
      candidate p_partkey -> partsupp#1.ps_partkey: row group ranges sum to 1.00x the column's range, ordered (under 1.5)
      partsupp#1: row groups 7 -> 7 (0-6)
        ps_partkey by p1.p_partkey in [249, 199478]
      build p1 = [p0] join agg(MJ2), run first: 460 rows, 17538 B, holding MJ1, MJ2; the plan reads it from memory
    plan diff: this query's section of <mode>.plandiff.txt

## Task 16

### The page

- `run.py` writes `<mode>.optimizer.md` from the same `OptimizerReport` as `.optimizer.txt`
  (`report_md.section`), never from the text. A run's files are `Layout`s: the three text files
  cut at `== `, the page at `## ` with its preamble, written again on every run.
  `goldens.sections` takes the heading, and what precedes the first is no section's. A skipped or
  failed run is its line as a section, so every query has a heading; a report a renderer cannot
  write fails its query, not the run.
- Per cluster, the DPhyp calls in the order they ran: a probe plan's replans in pass 1, the call
  before the run, a probe plan's replans in pass 2, the main run's replans. A cluster some call
  left other than planned (`JoinOrder.changed`) is a table: a column for the plan's order, then
  one per call that changed it; rows the tree, what changed it (the miss, `reordered by DPhyp`,
  `DPhyp at its budget…`, each `flip: X now builds`) and C_out beside the order before it. The
  calls that left it unchanged are a line under it, `Calls that left the order unchanged: replan
  1 (MJ2's build came out 7739 rows, est 469.3); replan 2.`, a replan this cluster's own miss set
  off naming that miss, which otherwise shows only in the table of the cluster it stopped. A
  probe's column, `inside p2, replan 1`, orders the part of the cluster inside that
  build. Then the memory sources the columns' relations show, and those their own words name,
  until none is new. Any other cluster is one line, ahead of the tables.
- A tree is `names.joined` over the call's relation names and its `types`: the build bracketed,
  a join on the probe side parenthesised; inner and full joins build first; an outer, semi or
  anti join writes the side it keeps first, so the engine's `Right` reads `left join` and no tree
  holds a right join. A mark join is never in a cluster. left-join-reorder reads `[[nation (f)]
  join customer] left join orders`: the left join keeps its plan build side.
- `### <query> dynamic filters`: a row per filter on a scan either pass reached — pass, the scan
  by its alias, column, the build and key (`p0.d_date_sk`), the keys (up to ten values listed,
  past that a count and a range), row groups before → after with those kept; the builds run first;
  each probe plan's replans and refusals.
- Headings are `<query>`, `<query> MJ<n>`, `<query> dynamic filters`; GitHub's anchors are
  `#q2`, `#q2-mj1`, `#q2-dynamic-filters`, `#left-join-reorder-mj1`. `report_md.anchor` gives
  them; `cluster_heading` and `filters_heading` are the names other pages link by.

### Red first

- `test_report_md` on `ModuleNotFoundError`; `test_run`'s page test on `sections()` taking one
  argument, its failed-run test on the page missing, and its renderer test on `generate` taking no
  `worker`; the legend tests on `RelationEstimate`, then `Kept`, taking no sources; the column
  test while every call had a column.

### Sample

The same 16 runs; pages of 3.7–3.9 KB (tpch) and 15–17 KB (tpcds) for the four queries. tpch q18
at tp1-single:

    | | plan | DPhyp before the run | replan 1 |
    |---|---|---|---|
    | join tree | `([[customer] join orders] join lineitem#2) semi join [agg(lineitem#1) (f)]` | `[[customer] join (orders semi join [agg(lineitem#1) (f)])] join lineitem#2` | `[[orders semi join [m1]] join m0] join lineitem#2` |

### The full run

`run.py` in one invocation, both benches, every mode, `--jobs 1`, into the committed outputs,
locally, Python 3.13.0, pandas 3.0.6, pyarrow 25.0.1: 620 runs, none failed, 5 h 5 min, at most
9.4 GiB resident (`time -v`, the worker in tpcds tp1-single), the host's available memory never
under 2.9 GB.

- Against HEAD: every `.cpu.txt` and `.costs.txt` section byte for byte, the same sections in the
  same order; every `.plandiff.txt` section HEAD's `.optimizer.txt` from `plan diff` on, or `plan
  unchanged` where HEAD had none, and the log's pointer exactly where HEAD had a diff (tpch 20
  queries, tpcds 80, at every mode); `cost_report.py` wrote HEAD's page byte for byte. The logs
  differ in every section where a rule fired (tpch 31, tpcds 80 a mode); the pages are new.
- Sizes. `.optimizer.txt`: tpch 50.8–51.6 KB, tpcds 659.6–670.7 KB (tp4-single 669,960 B, from
  2,648,744). `.optimizer.md`: tpch 11.1–11.9 KB, tpcds 133.5–136.3 KB. `.plandiff.txt`: tpch
  116.7–203.1 KB, tpcds 1.32 MB at the tp1 modes and 2.15–2.26 MB at the tp4 ones. GitHub's blob
  view showed a 1.87 MB file and refused a 2.55 MB one, so its limit lies between: the tp4
  `.plandiff.txt` files fall in that gap, the tpcds tp4 `.cpu.txt` (1.41–1.87 MB) at or under the
  size shown. Nothing links into `.plandiff.txt`.
- Every page: a `## ` heading per query (43 tpch, 99 tpcds), no heading repeated, none with a
  character GitHub drops from an anchor; 17 tpch and 133–141 tpcds cluster tables a mode.

### The pages after the full run

Six nits on the pages and this record, after the full run:

- The `.txt` pointer names `<mode>` literally (`render` knows no mode). Left: changing it means a
  full run.
- A table's legend left out a build its `inside pN` column or its unchanged-calls line names
  (tpch q9 MJ1, tpcds q16 MJ2, tpcds q5 MJ1). The legend is now seeded with every probe column's
  build, changed or not; red first in `test_report_md` on `p0` missing, two tests.
- A bare `MJ2` as a relation (`union(web_sales#1, MJ4)` in tpcds q5) was explained nowhere. The
  preamble says it is a cluster read whole. The `.txt` `names:` line is left: a full run.
- The over-1-MB sentence above was wrong about the threshold; corrected.
- `report_md.py` and the README's log bullet rewrapped.
- An unchanged call's own miss reads `MJ1's missed build came out …`.

Only pages moved: the 182 (query, mode) sections with a probe column or an own-miss unchanged
line, and every preamble, regenerated by two `run.py --query` runs (tpch q9, q18, q21; 39 tpcds
queries; every mode, 210 runs, none failed, 2 h 41 min). Every `.cpu.txt`, `.costs.txt`,
`.optimizer.txt`, `.plandiff.txt` and `cost_report.html` stayed byte for byte.

Proving: cheap tier 48 files, 480 cases; `test_cardinality_corpus`, `test_ordered_corpus` and
`test_run_corpus` over the sf1 tables; the samples; the full run above.

## Task 17

### The links

- `--sha` on `cost_report.py`: links are `https://github.com/asymptote-tech/peacockdb/blob/<sha>/scripts/exec_model/testdata/goldens/<bench>/<file>`;
  without it `<bench>/<file>`, relative to the page. A sha is 7 to 40 hex digits, else refused.
- A ratio cell whose shown text is not `1.00` links to `<bench>/<mode>.optimizer.md` and
  `goldens.anchor(query)`; 0.998 shows as 1.00 and is plain. Dash, skipped and failed cells too.
- Rules fired is a line per mode, `<mode>: n MJ, k/t + k/t DF, r replans`, a part left out where
  its count is 0. `k/t` is per filter pass that applied a filter, the plan as written first:
  the applications that narrowed a scan out of all, one per (scan, build), so `k` is `Fired`'s
  `narrowed` and tpcds q6's `1/2` is one `store_sales` pruned by two builds. `0/1` stays: a probe
  ran and the plan reads its build from memory. A mode whose log ends `plan diff:` with every part
  0 reads `the plan changed`, and the page's legend says why: a cluster rebuilt in its own order,
  its projections folded or reordered. That is tpch q3 and tpcds q2 and q93, at every mode, DPhyp
  keeping the order each time. tpch q3: column order alone — one projection reorders, a parent's
  key index goes `@0` → `@2` — at the same cost. tpcds q2: 9 `GpuProject` lines removed and 6
  added, about 57 KB of passthrough. tpcds q93: the rebuild drops the `GpuProject` above the right
  join and folds it into the join's projection; `cuda_passthrough_bytes` falls from 124,669,288 to
  9,453,128 at tp1-single, the whole of its ratio, 0.68, and the same gives 0.81 at tp4. An oddity:
  a saving no rule made, from the cluster's disassembly. `nothing changed the plan` is left only
  where every mode's log says `plan unchanged`, `nothing fired` where every count of `Fired` is 0.
  The hover keeps every `Fired` count.
- Ranges are `#L<a>-L<b>`. A block is an unindented line and those indented under it. `n MJ`
  spans the first join-order block (`MJ…`, `replan …`) to the last, one range, so it can hold the
  filters after join order. Each DF fraction is its own pass's block. `r replans` runs from the
  first replan's head — `replan K:` or a probe's `probe pN replan K:` inside a filter block — to the
  end of the block holding the last.

### The counts come from the log

`Fired` lacks them: `reordered` counts calls cheaper than the plan's order, not distinct clusters,
and nothing counts a filter applied without narrowing. Adding them would change every `.costs.txt`,
a full run. So `cost_report.read_log` derives every count from `.optimizer.txt` in the same pass as
the ranges: per call, the `plan's order`/`previous order` tree against the `oriented` one
(`JoinOrder.changed`), keyed by the call's `MJ<n>` at any indent; per `  <alias>: row groups A -> B`
line, applied, and narrowed where B < A; each replan's head; and whether the section has `plan
diff:`. Two round trips through `report_text` tie the reader to its wording: `EVERY_RULE`, and a
report whose MJ3 changes only inside a probe's replan and whose MJ4 keeps its previous order in a
replan, reordered 2.

Over the committed files, all 620 (query, mode) runs: per pass `k` sums to `Fired.narrowed +
narrowed_after_order`, `r` equals `Fired.replans`, and n equals the `### <query> MJ<n>` tables of
that mode's page. No mode whose log says the plan changed lacks a line.

### No pandas

`cost_report.py` imported `optimizer.pipeline` for the mode names and `optimizer.report` for
`Fired`; both reach `dynamic_filters`, which imports pandas. The mode names are `plans/goldens.MODES`
now, and `test_run` holds `pipeline.MODES`' keys to them; `Fired` is `optimizer/fired.py`, imported
by `report.py`; `anchor` moved from `report_md.py` to `plans/goldens.py`. The cost-report job's
`python3` has pandas from the prototype's step, so its render proves nothing about this;
`test_the_committed_page_renders_with_pandas_pyarrow_and_numpy_blocked` sets the three to `None` in
`sys.modules` and renders the committed files at a sha. The linuxbrew Python 3.13.0, which has no
pandas, rendered the committed page.

### CI

The cost-report job's `Generate report` step, after the engine's report, runs `cost_report.py
--sha "$REPORT_SHA"` and copies the page to `report/join-optimizer/index.html`, on a PR too, where
nothing is uploaded. On master `report/` is the Pages artifact, so the page is published at
`https://asymptote-tech.github.io/peacockdb/join-optimizer/`. The site's other paths are
`index.html`, `history.tsv`, `history.html` and a `<sha>/` per report, which the next deploy
carries forward by `history.tsv`; `join-optimizer/` is rendered again on every deploy. The
engine's page lists its pages in the nav `render_html` writes (`latest · all reports`) in
`cost-report/src/main.rs`; that crate is outside this task's scope, so nothing links there yet.

### Red first

- `test_cost_report` on `ImportError: Evidence`, later `Filters`; `test_report_md` on `ImportError:
  anchor` from `goldens`.
- Each guard against a mutation of what it guards, all red: the import through `report.py`; a
  lazy `import pandas` in `render`; a 1.00 ratio linked, and a ratio linked by its exact value;
  absolute links without `--sha`; every call counted as reordered; every scan as narrowed; a
  range's first line moved; a zero MJ part shown; `_STARTED` blind to `previous order`, `_CALL`
  only at two spaces, `_LEFT` only at four; `plan diff:` never read; the `the plan changed` line
  dropped; DF only where a scan narrowed; one range for both passes; `goldens.MODES` reordered.

### Proving

- Cheap tier: 48 files, 488 cases (`test_cost_report` 10 → 18). `test_run_corpus` over the sf1
  tables.
- The committed `cost_report.html` re-rendered without `--sha`; every other file under
  `scripts/exec_model/testdata/` HEAD's sha256. Against HEAD's page every cell but Rules fired has
  the same text. Its 1,589 links: 449 ratio links whose anchors are headings of their page; 409 MJ,
  425 DF and 306 replan ranges, each within one section, starting at a block of its kind (a DF one
  at its pass's heading with no other block inside) and ending before the next unindented line.
- Rules fired, tp4-single: tpch q16 `0/1 DF, 1 replan`; tpcds q75 `6/6 + 1/2 DF, 1 replan`
  (L7805–L7829 and L7930–L7938); q58 `4 MJ, 3/3 DF, 3 replans`; q6 `1 MJ, 1/2 DF, 1 replan`.
- The `Generate report` step's new lines, run with the pandas-free Python in a copy of the tree:
  the page at `report/join-optimizer/index.html`, its links at the sha. The YAML parsed and the
  step's `run:` passed `bash -n`.
- Open: the published page's links resolve, checked after a push and a CI run.
