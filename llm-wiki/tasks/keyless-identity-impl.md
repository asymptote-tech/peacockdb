# keyless-identity implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A keyless aggregate answers one row whatever arrived, and adding or removing zero-row
batches changes no row an aggregate init emits (#199). A scan whose row groups all prune, or a
file with none, plans one lane that holds no batch instead of being refused (#282). q96, q88 and
q90 run at the tp4 modes; six pbench queries over an empty input join the corpus; pbench's
`cross-empty-build` and `outer-on-true-empty` run.

**Architecture:** Every `GpuAggregate` becomes a `BatchAccumulator`. Its executor, on each backend,
holds one flag, `emitted`. A zero-row batch is released with no call. A batch with rows runs the
init (and the shortcut's finalize). At done, where nothing went out, the same calls run once more,
the first handed no input. The recipe states that as a new `CallPattern`,
`AtDoneIfNothingOut`, reusing the init's seqs. The cpu runs the init over
`RecordBatch::new_empty`. The device's `execute_aggregate`, handed no input, aggregates the
zero-row table its `aggr_input_schema` declares. With every keyless lane now owing a row, the cpu
merge's `!self.grouped` clause goes. `empty_state` states what each aggregator owes over no rows;
the tests hold both engines to it. `source()` maps a scan with no surviving row group to
`partition_groups = [[]]`: one lane, no batch, no call, which is the arrival the init now answers.

**Tech stack:** Rust over DataFusion 45 (rust-only tier, `--features gpu` device rung); C++20 on
cuDF 25.02 (`peacock_plan_tests`, a device binary); DuckDB 1.5.4 for pbench's oracle section.

**Spec:** [`keyless-identity.md`](keyless-identity.md). Interfaces a later plan (welford-device)
consumes, defined here exactly:

- `pub(crate) fn empty_state(agg: PlanAgg, input: &DataType) -> Result<ScalarValue, PlanError>` in
  `peacockdb-core/src/plan/aggregates.rs`, beside `state_type`; and
  `PlanAgg::empty_state(self, input: &DataType) -> Result<ScalarValue, PlanError>` beside
  `PlanAgg::state_type` (`plan/mod.rs`). Task 1.
- `CallPattern::AtDoneIfNothingOut`, text `"at done, if nothing went out"`, in
  `peacockdb-core/src/wire/mod.rs`. Task 3.
- `category_of`: every `GpuAggregate` is `ExecutorCategory::BatchAccumulator`. Task 4.
- Each backend's init executor holds `emitted: bool` and releases a zero-row batch with no call,
  as `LimitStream` does. Task 4.
- The device done call: the init's seq called with no input, answered by `execute_aggregate`
  over a zero-row table built from `aggr_input_schema`. Its first line is
  `auto input = was_handed_nothing(in) ? zero_rows_of(agg) : take_input(in);`, with
  `bool was_handed_nothing(const NodeInputs*)` (`cpp/src/peacock/operators.h`) and the
  file-static `TableResult zero_rows_of(const fb::CudfAggregate*)` (`aggregate.cpp`). Task 2.
- The harness: after Task 4 every `GpuAggregate` case runs `Script::Accumulate`, never
  `Script::Exec` (`run_both` asserts the script's shape against the category). The identity
  cases in `tests/gpu_tests/aggregate_cases.rs`: `DEVICE_IDENTITY`, `keyless_init`, and the
  `pub(crate)` helpers `empty_state_row(node: &GpuAggregate, input: &DataType) -> RecordBatch`
  and `at_done(zero_row_batches: usize, row: RecordBatch) -> Vec<Vec<RecordBatch>>`. Task 4.
- A scan with no surviving row group: `GpuLoadParquet.partition_groups == [[]]`. Task 6.

## Before you start

Chain L's base is master once chains J and K have both merged. This plan was written against
8806a3c3/64ced62e, before either, and after grouping-id (chain L's first task) was planned. Find
every site below by its symbol, never by a line number. What moves:

- **`cpp/src/plan_executor.h`, `TableResult`.** refcounted-scatter (J) gives it one owner per
  column; every `return {…}` in `aggregate.cpp` becomes `TableResult::owning(…)` and readers write
  `result.view()`. Task 2's builder constructs its table the way the code around it then does.
- **`cpp/src/operators/aggregate.cpp`.** exit-copies (J) changes the exits; K (distinct-companions)
  deletes the `distinct` guard; grouping-id adds `grouping_id_column`. The first line of
  `execute_aggregate` is still `auto input = take_input(in);` — that is the line Task 2 changes.
- **`cpp/src/node_session.cpp`, `NodeSession::execute_node`.** join-session-cpp (J) adds the join
  session. Confirm the ordinary map arm still computes
  `n_out = (n_children > 0) ? child[0].size() : 1` and calls `execute_one(node, {})` when the call
  carries no child: that is what makes a call with zero handles reach `execute_aggregate` with no
  ABI change. If it no longer does, stop and report.
- **`cpp/src/expr.cpp`, `fb_to_type_id`.** repartition-keys (J) adds the fbs timestamp types. It is
  already declared in `cpp/src/peacock/expr.h`; nothing in `expr.cpp` changes here.
- **The gtests go in a new file.** `cpp/tests/gpu/test_plan_executor.cpp` is past the 1000-line
  cap (2203 lines on master), and its helpers are file-static. grouping-id (chain L's first task)
  creates the shared header `cpp/tests/gpu/hand_tables.hpp` (`namespace hand`: `column_of`,
  `values_of`, `table_of`, `ref`, `null_of`, `run`) and `cpp/tests/gpu/test_grouping_id.cpp`, and
  adds the latter to `peacock_plan_tests`' sources in `cpp/CMakeLists.txt`. Task 2 adds
  `cpp/tests/gpu/test_aggregate_no_input.cpp` beside it, using `hand::ref` and `hand::values_of`
  only; if grouping-id named them otherwise, use its names. `main` stays in
  `test_plan_executor.cpp`. K drops `CreateAggregateFuncNode`'s `distinct` argument, so the
  gtests use `AggregateFuncNodeBuilder`, as grouping-id's do.
- **Chain K's limits and empty-sorts are in the base.** limits rewrites `source()`
  (`planner/translator/nodes.rs`): a scan's pushed limit becomes a `GpuLimit` above it or joins
  the unload's interval, a limited scan's survivors are trimmed to a prefix before mapping, and
  `GpuLoadParquet` loses its `limit` field. It also moves `LimitStream` in both backends'
  `accumulate.rs` and the driver's limit count, mock included (`executor/driver/tests/`).
  empty-sorts rewrites `SortedRuns` and `CpuPartitionAccumulator` in `cpu_backend/accumulate.rs`
  around `coalesce_or_nothing` (whose doc Task 5 edits), and makes the driver answer a query whose
  sink received nothing with one zero-row batch under the sink's schema (DataFusion's empty
  answer in `test_support/corpus.rs` likewise). That is why `empty-grouped-count`, whose grouped
  init now emits nothing, compares with DuckDB exactly: its answer keeps its columns. Task 6 edits
  `source()` where it calls `partition(…)`, after limits' trim; Task 4's mock rule sits beside
  limits' rule.
- **`peacockdb-core/src/wire/gpu_tests/mod.rs` is past the cap** (1056 lines on master). Task 3
  adds only the dispatch and bookkeeping lines there; the init's walk and its two tests go in a
  new child module, `wire/gpu_tests/aggregate_init.rs`.
- **`plan/mod.rs`, `NodeRef` and `category_of`.** join-backend (J) adds a `GpuEmpty` leaf, so the
  match gains an arm; the mock's `executors_for`, the walk's `node()` and the cpu backend test's
  `node_kind` gain it too. Only the `Aggregate` arm moves here.
- **`planner/translator/aggregate.rs`.** K splits `aggregate_sequence` into `Stage` and
  `sequence`. Nothing here edits it: the shortcut is fixed by the executor.
- **`executor/driver/`.** join-backend moves joins onto the session; limits (K) changes the
  driver's limit count and its mock rule; empty-sorts (K) adds the empty answer. This plan
  touches only the mock (`driver/tests/mock.rs`, where guard-checks already made `MockUnload`
  call `rows.clamp`) and adds a test file.
- **`wire/gpu_tests/mod.rs`.** join-backend rewrites the join walker; grouping-id adds
  `SUM_OF_QUOTIENTS` and a DISTINCT walk. `Walk::node`, `make`, `per_lane`, `phases`,
  `Session::execute`, `TWO_LANES`, `times`, `trail` keep their names.
- **`src/tests/gpu_tests/aggregate_cases.rs`.** grouping-id turns its four `bug_` pins into
  agreement cases and deletes `grouping_sets_as_exported`.
- **`peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`.** duckdb-oracle
  adds the ninth argument (`duckdb_oracle`) and `all_modes`; pbench adds its section (alphabetical,
  nine arguments, with `cross-empty-build` and `outer-on-true-empty` refused at planning);
  stale-cells and join-backend move the q96/q88/q90/q32 lines and tags (#152, #220 struck).
  **Build Task 7's row list from the registry as it stands.**
- **Goldens.** duckdb-oracle adds `gpu-result.txt` per dataset, written by the device run under
  `PCK_WRITE_GPU_RESULT=1`, with a rust-only guard that every enabled device cell has its section.
- **`llm-wiki/build-test.md`.** Every count moves with J and K. Recount each row this plan touches;
  the deltas in Task 9 are this task's alone.
- **pbench's `empty` table holds no row group** (`testdata/pbench.sf1/empty.parquet`: 0 rows, 0 row
  groups, checked 2026-10-08). A scan over it, like a scan whose every row group prunes, is
  refused at plan time today: `partition()` (`planner/translator/scan_mapping/partition.rs`), "no
  surviving row groups" ([#282](../tickets/corpus-coverage.md#t282)). Task 6 makes `source()` map
  it to one lane with no batch. The validator needs no change: `validate_schemas_and_partitions`
  (`plan/source.rs`) already accepts a lane with no batch and refuses only no lane, a batch of no
  row group, or a pruned group. Nor do the readers: `CpuSource::read_next`
  (`cpu_backend/source.rs`) and `GpuSource::read_next` (`gpu_backend/source.rs`) answer `None`
  for a lane whose batch list is empty, with no call, and `largest_batch_bytes` (`plan/source.rs`)
  answers `0` over no batch. Confirm each by its symbol before Task 6.
- **`tiny` and the modes.** The first four queries of Task 7 filter `tiny` (`t_id` 0–7,
  `t_v = 5·t_id`, one row group) with `t_id + t_v < 0`, true of no row and prunable by no
  statistic. tp1 hands its one lane a zero-row batch. tp4-single cuts `tiny` into four lanes, one
  holding the batch and three none. `tiny` is under `SMALL_TABLE_BYTES`, so tp4-rowgroup and
  tp4-sized plan it one lane, and reach the zero-row batch only. The names are the spec's:
  welford-device names `empty-dispersion-aggregates`.

## Global constraints

- No facade, trait, ABI or wire change. The done call is an existing seq handed no handles.
  `recipe-payloads.txt` keeps every `sha256=` line; only its recipe lines change (Task 3).
- No production behaviour change outside the init, its recipe, the cpu merge's clause and
  `source()`'s mapping of a scan with no survivor. The device merge, the limit (#214), the
  readers, the validator and the memory estimate do not change. The keyless Welford on the
  device stays as it is (#216, welford-device).
- `empty_state` has no production caller: neither backend builds a row from it. It carries
  `#[allow(dead_code)]` with the reason at the site (`coding-style.md`, *Visibility*), never a
  `#[cfg(test)]`.
- The done call's transient is one row (keyless) or none. The accumulator's `scratch_bytes` stays
  `held + n_bytes`, which prices it at zero, as the exec node priced a zero-row batch. No
  accounting change.
- A test that asserts the old behaviour and now goes red is renamed for what is true and asserts
  it. A `bug_` test that goes red is turned into the agreement case under a name without `bug_`.
- CPU builds and runs are local (`build-test.md`): `cargo test --features rust-only` into
  `target/`; C++ in `cpp/build` against cuDF 25.02:
  `scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build`,
  then `ctest --test-dir cpp/build -L cpu`. The cudf Rust shape compiles through
  `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh`.
- Device builds and runs happen on nebius-gpu, as chain L's header says: **one sync for this
  task**, then as many back-to-back builds as its red/green pairs need, the red build first. All
  of it is Task 8, the *Device session* below. Tasks 1–7 build and run everything that runs
  locally and compile every device test (`scripts/build.sh` for the gtests,
  `scripts/cargo-cudf.sh … --no-run` for the Rust ones); a device test written in them is first
  run, red, in Task 8's first build. Record every run (command, binaries, pass and fail counts)
  in `llm-wiki/tasks/keyless-identity-detail.md`. Out of scope, as the header says: the sf40
  binaries, `--run-benchmarks`, Nsight, any H200 timing; record each as deferred there.
- **Done**, as the header says: CI green except the `gpu-tests` job ("GPU Tests (remote)", on
  shad-gpu), and Task 8's device runs passed on nebius-gpu and recorded in the detail file.
- Every foreground command carries a `timeout`. Large runs (the corpus regeneration) get a monitor
  reporting progress every 2 minutes and matching `panicked|FAILED|error\[`.
- Formatting on changed lines only: `git clang-format HEAD -- <files>` for C++;
  `rustfmt --edition 2024 --check <file>` on each touched leaf, applied only where its diff stays
  inside lines this task wrote. Never a `mod.rs` (rustfmt follows its `mod` lines).
- Files stay under 1000 lines. `wire/tests.rs` (980) and `planner/tests/plan_goldens.rs` (970)
  are near it, so the new cases there go into new files (Task 3); `wire/gpu_tests/mod.rs` and
  `test_plan_executor.cpp` are past it, so nothing but dispatch lines goes into the first and
  nothing into the second. Check `wc -l` on every file a task touched before its commit.
- No commit leaves a test red. Commit messages at most 10 lines, ending with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Build in a workspace; never share a
  cargo target dir across worktrees.

## Device session

Task 8 is the task's one sync. Every command runs in the foreground with its timeout: a
backgrounded build chain dies mid-build with no error. If ssh is refused at once, the sandbox is
blocking it, not the host.

```bash
GPU=dmitry@89.169.109.150   # nebius-gpu, as chain L's header names it
DIR=peacockdb-L              # the header's directory there
# SYNC, once per task: the working tree, uncommitted, from the workspace root
timeout 1200 rsync -a --delete-after --exclude=.git --filter=':- .gitignore' ./ "$GPU:$DIR/"
# BUILD, as often as the red/green pairs need
timeout 7200 ssh "$GPU" "cd ~/$DIR && . ~/peacock-env.sh && ./scripts/build-test-shadgpu.sh --build"
# RUN what the step names, in place of RUN
timeout 3600 ssh "$GPU" "cd ~/$DIR && . ~/peacock-env.sh && \
  export LD_LIBRARY_PATH=\$PWD/cpp/install/lib:\$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib \
         PEACOCK_TESTDATA_DIR=\$PWD/testdata && RUN"
```

The red build is made from the same sync: the fixes leave the remote tree as a patch piped over
ssh's stdin (`patch -p1 -R`), and come back the same way (`patch -p1`) for the green build. No
second rsync. If the green build fails, stop: fix locally, commit, and run the session again from
SYNC; the detail file records why a second sync was needed.

A fresh `~/$DIR` has no sf1 parquet: copy it from `~/peacockdb-J/testdata`, as the header says, or
generate it with `testdata/generate_testdata.sh --bench tpch` and `--bench tpcds`. The rsync
filter excludes it and so never deletes it. pbench's data is committed. Every Rust binary takes
`--test-threads=1`.

## Review focus

1. **A keyless lane that receives only zero-row batches** — tp1 over a filter that keeps nothing,
   the most common way in. Expected: no init call for either batch, one done call, the identity
   row; and zero-row batches interleaved among batches with rows change no row. Task 4:
   `a_keyless_lane_of_zero_row_batches_makes_no_init_call_and_one_done_call`,
   `zero_row_batches_among_others_change_no_row` (mock); the harness identity case over two
   zero-row batches; Task 7's pbench rows at tp1.
2. **A grouped init over nothing, under a shuffle.** Expected: it emits nothing — not a zero-row
   batch — so the per-lane merge, the scatter and the final merge above it meet a lane that sent
   nothing, and answer no rows. Task 4: the two grouped mock cases and
   `a_grouped_init_over_no_input_answers_nothing_on_both`; Task 7: `empty-grouped-count` at
   tp4-single, whose empty answer reaches DuckDB under its columns (chain K's empty-sorts).
3. **A decimal or a date in the input a done call declares.** The zero-row table must carry the
   field's scale; built from the type id alone, a sum's NULL comes back at scale 0 and the export
   refuses it. Expected: `Decimal128(28, 2)` NULL on both engines. Task 2:
   `AggregateNoInput.AKeylessNodeAnswersOneRowTypedAsItsInputDeclares` (scale −2, `TIMESTAMP_DAYS`);
   Task 4: `a_decimal_sum_over_no_input_answers_a_null_of_its_declared_type_on_both`.
4. **The single-node shortcut over nothing.** Its finalize meets count 0 beside NULL sums: `avg`
   divides a NULL by a zero count, on the decimal path too. Expected: NULL on both engines, never a
   divide-by-zero error. Task 4: `a_keyless_shortcut_over_no_input_answers_count_0_and_nulls_on_both`
   (with `Avg`) and `a_decimal_average_shortcut_over_no_input_answers_null_on_both`.
5. **An aggregate on the build side of an outer join, over an input that turns out empty.** Before,
   its zero-row batch reached the scatter and became the kept empty table a preserving join reads
   (#175). Now it emits nothing, and the join meets a lane with no build — the path join-backend
   answers (#212). Expected: DataFusion's answer at all five modes. Task 4:
   `an_outer_join_over_an_aggregate_of_nothing_answers_as_datafusion` (end to end).
6. **A scan of nothing (#282).** Every row group pruned, or a file with none. Expected: one lane
   with no batch at every mode — never a lane per target partition, never an empty mapping on the
   wire, never a `partition()` call — and the keyless count above it answers `0`. Task 6:
   `a_scan_no_row_group_survives_is_one_lane_with_no_batch_at_every_mode`,
   `a_count_over_a_scan_of_nothing_answers_0`; Task 7: `empty-table-count`,
   `empty-pruned-count`, and pbench's `cross-empty-build` and `outer-on-true-empty`, which now
   meet the joins they test.

## File structure

| file | responsibility |
|---|---|
| `peacockdb-core/src/plan/aggregates.rs`, `plan/mod.rs` | `empty_state`; `category_of` |
| `peacockdb-core/src/plan/aggregates/tests.rs`, `plan/tests/mod.rs` | their cases |
| `cpp/src/peacock/operators.h`, `cpp/src/operators/dispatch.cpp` | `was_handed_nothing` |
| `cpp/src/operators/aggregate.cpp` | no input → the zero-row table of `aggr_input_schema` |
| `cpp/tests/gpu/test_aggregate_no_input.cpp` (new), `cpp/CMakeLists.txt` | `AggregateNoInput.*`, in `peacock_plan_tests` |
| `peacockdb-core/src/wire/mod.rs` | `CallPattern::AtDoneIfNothingOut`; `Recipe::calls_under` |
| `peacockdb-core/src/wire/attach.rs` | the init's done calls |
| `peacockdb-core/src/wire/recipes.rs` | a call handed nothing renders without a trailing comma; a seq's payload once |
| `peacockdb-core/src/wire/tests.rs`, `wire/tests/aggregate_init.rs` (new) | the init's recipe |
| `peacockdb-core/src/planner/tests/mod.rs`, `planner/tests/recipe_patterns.rs` (new) | only an init calls at done if nothing went out |
| `peacockdb-core/src/wire/gpu_tests/mod.rs`, `wire/gpu_tests/aggregate_init.rs` (new) | the walk drives an init by node kind; the init's walk and two walk tests |
| `peacockdb-core/src/executor/cpu_backend/{accumulate,backend,mod}.rs` | `State::Init`; the merge's clause goes |
| `peacockdb-core/src/executor/gpu_backend/{accumulate,backend,mod}.rs` | `State::Init`; `GpuExec::exec_over_nothing` |
| `peacockdb-core/src/executor/driver/tests/{mock,plans,mod}.rs`, `driver/tests/init.rs` (new) | the mock's init rule; the lane cases |
| `peacockdb-core/src/executor/cpu_backend/tests/{state_types,accumulate}.rs` | identity per decomposition; the merge's case |
| `peacockdb-core/src/executor/gpu_backend/gpu_tests/{mod,accumulate}.rs` | `Session::init`; the device init cases |
| `peacockdb-core/src/tests/gpu_tests/{aggregate_cases,aggregate_dimension_cases,aggregate_schema_cases}.rs` | scripts as accumulators; identity cases; the `bug_` flip |
| `peacockdb-core/src/tests/end_to_end.rs` | the outer join over an aggregate of nothing; the count over a scan of nothing |
| `peacockdb-core/src/planner/translator/nodes.rs` (`source`) | #282: no survivor → one lane, no batch |
| `peacockdb-core/src/planner/tests/empty_scan.rs` (new), `planner/tests/mod.rs` | #282's mapping at every mode |
| `testdata/goldens/*/*.plans.txt`, `recipe-payloads.txt`, `*-mini.{cpu,cost}.txt`, `mini.result.txt`, `pbench.sf1/{duckdb,gpu}-result.txt` | regenerated |
| `testdata/pbench-queries/empty-*.sql` (new), `corpus_cases.inc`, `testdata/cost-registry.csv` | the six queries; the `199` rows; `cross-empty-build`, `outer-on-true-empty` |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets.md`, `tickets/`, `archive/archived-tickets.md` | as the spec's item 8; counts; #199 and #282 archived |

---

### Task 1: What each aggregator owes over no rows — `empty_state`

**Files:**
- Modify: `peacockdb-core/src/plan/aggregates.rs` (beside `state_type`)
- Modify: `peacockdb-core/src/plan/mod.rs` (`impl PlanAgg`, beside `state_type`)
- Test: `peacockdb-core/src/plan/aggregates/tests.rs`

**Interfaces:**
- Produces `aggregates::empty_state` and `PlanAgg::empty_state`, signatures as in the header.
  Consumed by Task 4's identity cases and by welford-device.

- [ ] **Step 1: The failing test**, at the end of `plan/aggregates/tests.rs`:

```rust
/// What an init aggregator owes over no rows is a value of the type its state column
/// declares — over every argument type `state_type` accepts, and refused where it refuses.
#[test]
fn an_empty_state_is_a_value_of_the_type_state_type_declares() {
    let inits = [
        PlanAgg::Sum,
        PlanAgg::Min,
        PlanAgg::Max,
        PlanAgg::Count,
        PlanAgg::Mean,
        PlanAgg::M2,
    ];
    for agg in inits {
        for input in [Int32, Int64, Float64, Decimal128(15, 2), Date32] {
            match (agg.state_type(&input), agg.empty_state(&input)) {
                (Ok(declared), Ok(value)) => {
                    assert_eq!(value.data_type(), declared, "{agg:?} over {input}")
                }
                (Err(_), Err(_)) => {}
                (declared, value) => {
                    panic!("{agg:?} over {input}: state_type {declared:?}, empty_state {value:?}")
                }
            }
        }
    }
    assert_eq!(PlanAgg::Count.empty_state(&Utf8).unwrap(), ScalarValue::Int64(Some(0)));
    for welford in [PlanAgg::Mean, PlanAgg::M2] {
        assert_eq!(
            welford.empty_state(&Float64).unwrap(),
            ScalarValue::Float64(Some(0.0)),
            "{welford:?}: DataFusion's variance state over no rows is (0, 0.0, 0.0)"
        );
    }
    for nullable in [PlanAgg::Sum, PlanAgg::Min, PlanAgg::Max] {
        assert!(nullable.empty_state(&Decimal128(15, 2)).unwrap().is_null(), "{nullable:?}");
    }
}
```

- [ ] **Step 2: Run it; it does not compile.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- plan::aggregates
```

  Expected: `no method named empty_state found for enum PlanAgg`.

- [ ] **Step 3: Implement.** In `plan/aggregates.rs`, `use datafusion::common::ScalarValue;` and,
  directly after `state_type`:

```rust
/// What the state column `agg` produces over no rows, given its argument's type — the value
/// a keyless init answers at done. Shaped as [`state_type`] and typed by it; no wildcard, so
/// a new aggregator says its identity here or does not compile.
///
/// `Mean` is `0.0` and not NULL because it exists only inside the Welford triple, and
/// DataFusion 45's `VarianceAccumulator::state` answers `(0, 0.0, 0.0)` over no rows. Neither
/// backend builds a row from this: each engine computes its own identity, and the identity
/// cases hold both to this table.
#[allow(dead_code)] // the identity cases' reference: no backend builds a row from it
pub(crate) fn empty_state(agg: PlanAgg, input: &DataType) -> Result<ScalarValue, PlanError> {
    Ok(match agg {
        PlanAgg::Count => ScalarValue::Int64(Some(0)),
        PlanAgg::Mean | PlanAgg::M2 => ScalarValue::Float64(Some(0.0)),
        PlanAgg::Sum | PlanAgg::Min | PlanAgg::Max => {
            let state = state_type(agg, input)?;
            ScalarValue::try_from(&state).map_err(|error| {
                PlanError::Unsupported(format!(
                    "{} over {input}: no typed NULL of {state}: {error}",
                    agg.tag()
                ))
            })?
        }
        PlanAgg::MergeM2 => {
            unreachable!("MergeM2 merges the Welford triple and declares no state")
        }
    })
}
```

  In `plan/mod.rs`, inside `impl PlanAgg`, after `state_type` (import `ScalarValue` if the file
  does not already):

```rust
    /// What this aggregator's state column holds over no rows, given its argument's type.
    #[allow(dead_code)] // the identity cases' reference: no backend builds a row from it
    pub(crate) fn empty_state(self, input: &DataType) -> Result<ScalarValue, PlanError> {
        aggregates::empty_state(self, input)
    }
```

- [ ] **Step 4: Run it, and the build for warnings.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- plan::aggregates
timeout 1800 cargo build --features rust-only -p peacockdb-core 2>&1 | grep -c '^warning' || true
```

  Expected: PASS; the warning count equal to the base's (run the same `cargo build` before Step 3
  and note the number).

- [ ] **Step 5: Commit.**

```bash
git add peacockdb-core/src/plan/aggregates.rs peacockdb-core/src/plan/mod.rs peacockdb-core/src/plan/aggregates/tests.rs
git commit -m "$(printf '%s\n' \
  "#199: empty_state, what each aggregator owes over no rows" "" \
  "Beside state_type and typed by it; no backend builds from it." "" \
  "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>")"
```

---

### Task 2: The device aggregates nothing when handed nothing

**Files:**
- Modify: `cpp/src/peacock/operators.h` (beside `take_input`)
- Modify: `cpp/src/operators/dispatch.cpp` (beside `take_input`)
- Modify: `cpp/src/operators/aggregate.cpp` (`execute_aggregate`'s first line; a static above it)
- Create: `cpp/tests/gpu/test_aggregate_no_input.cpp`; Modify: `cpp/CMakeLists.txt`
  (`peacock_plan_tests`' sources)

**Interfaces:**
- Produces: a `CudfAggregate` seq executed with zero child handles answers one row (keyless) or
  no groups (grouped) over the zero-row table of its `aggr_input_schema`. Every other node handed
  nothing still throws `take_input: not enough input handles`. Consumed by Tasks 3 and 4, and by
  welford-device and aggregate-arms, which keep the first line as written here.
- Consumes grouping-id's `cpp/tests/gpu/hand_tables.hpp` (`hand::ref`, `hand::values_of`).

No ABI change: `peacock_executor_execute_node` already accepts `n_children = 0`, and
`NodeSession::execute_node`'s map arm then runs one partition with an empty input vector.

- [ ] **Step 1: The gtests**, a new file `cpp/tests/gpu/test_aggregate_no_input.cpp`:

```cpp
/// An aggregate handed no input. A lane that sent no row out makes its init's done call with no
/// handle, and the node aggregates the zero-row table its aggr_input_schema declares: one row
/// for a keyless node, typed as a batch would have been, and no groups for a grouped one. The
/// plan holds a scan under the aggregate, as every plan the writer makes does; the scan is
/// never called, so its file is never opened.

#include "hand_tables.hpp"

#include "generated/gpu_plan_generated.h"
#include "plan_executor.h"

#include <cudf/types.hpp>
#include <flatbuffers/flatbuffers.h>
#include <gtest/gtest.h>

#include <cstdint>
#include <stdexcept>
#include <string>
#include <vector>

namespace {

namespace fb = peacock::plan;

/// The declared input: an Int32 key, an Int64 value, a Decimal128(15, 2), a date and a
/// string. None of it is read from a file.
flatbuffers::Offset<fb::Schema> declared_input(flatbuffers::FlatBufferBuilder& fbb) {
  std::vector<flatbuffers::Offset<fb::Field>> fields;
  fields.push_back(fb::CreateField(fbb, fbb.CreateString("k"), fb::DataType_Int32, true));
  fields.push_back(fb::CreateField(fbb, fbb.CreateString("v"), fb::DataType_Int64, true));
  fields.push_back(fb::CreateField(fbb, fbb.CreateString("amount"), fb::DataType_Decimal128,
                                   true, /*decimal_precision=*/15, /*decimal_scale=*/2));
  fields.push_back(fb::CreateField(fbb, fbb.CreateString("day"), fb::DataType_Date32, true));
  fields.push_back(fb::CreateField(fbb, fbb.CreateString("name"), fb::DataType_Utf8, true));
  return fb::CreateSchema(fbb, fbb.CreateVector(fields));
}

/// A scan of `declared_input`, seq 0 in post-order. Never executed.
flatbuffers::Offset<fb::PlanNode> unread_scan(flatbuffers::FlatBufferBuilder& fbb) {
  auto paths = fbb.CreateVector(
      std::vector<flatbuffers::Offset<flatbuffers::String>>{fbb.CreateString("/never/read.parquet")});
  auto schema = declared_input(fbb);
  fb::CudfScanBuilder scan(fbb);
  scan.add_file_paths(paths);
  scan.add_file_schema(schema);
  return fb::CreatePlanNode(fbb, fb::PlanNodeKind_CudfScan, scan.Finish().Union());
}

/// An Int64 literal, built field by field as the plan writer does.
flatbuffers::Offset<fb::Expr> int64_literal(flatbuffers::FlatBufferBuilder& fbb, int64_t value) {
  fb::ScalarValueBuilder sb(fbb);
  sb.add_type(fb::DataType_Int64);
  sb.add_int_val(value);
  auto scalar = sb.Finish();
  return fb::CreateExpr(fbb, fb::ExprNode_LiteralExpr, fb::CreateLiteralExpr(fbb, scalar).Union());
}

/// `func(arg)` under `alias`.
flatbuffers::Offset<fb::AggregateFuncNode> func_over(flatbuffers::FlatBufferBuilder& fbb,
                                                     const char* func,
                                                     flatbuffers::Offset<fb::Expr> arg,
                                                     const char* alias) {
  auto name = fbb.CreateString(func);
  auto args = fbb.CreateVector(std::vector<flatbuffers::Offset<fb::Expr>>{arg});
  auto out = fbb.CreateString(alias);
  fb::AggregateFuncNodeBuilder node(fbb);
  node.add_name(name);
  node.add_args(args);
  node.add_alias(out);
  return node.Finish();
}

/// count(*), sum(v), sum(amount), min(day) and max(v) over `declared_input`, keyed on `k`
/// where `grouped`, as the init a done call addresses: Partial mode, seq 1 above the scan.
/// `count(*)` is `count(1)`, as the planner writes it (`recipe-payloads.txt`).
std::vector<uint8_t> unfed_aggregate(flatbuffers::FlatBufferBuilder& fbb, bool grouped) {
  auto scan = unread_scan(fbb);
  std::vector<flatbuffers::Offset<fb::AggregateFuncNode>> funcs{
      func_over(fbb, "count", int64_literal(fbb, 1), "n"),
      func_over(fbb, "sum", hand::ref(fbb, 1, "v"), "sum(v)"),
      func_over(fbb, "sum", hand::ref(fbb, 2, "amount"), "sum(amount)"),
      func_over(fbb, "min", hand::ref(fbb, 3, "day"), "min(day)"),
      func_over(fbb, "max", hand::ref(fbb, 1, "v"), "max(v)"),
  };
  auto funcs_vec = fbb.CreateVector(funcs);
  auto input = declared_input(fbb);
  flatbuffers::Offset<flatbuffers::Vector<flatbuffers::Offset<fb::Expr>>> groups;
  flatbuffers::Offset<flatbuffers::Vector<flatbuffers::Offset<flatbuffers::String>>> names;
  if (grouped) {
    groups = fbb.CreateVector(std::vector<flatbuffers::Offset<fb::Expr>>{hand::ref(fbb, 0, "k")});
    names = fbb.CreateVector(
        std::vector<flatbuffers::Offset<flatbuffers::String>>{fbb.CreateString("k")});
  }
  fb::CudfAggregateBuilder builder(fbb);
  builder.add_mode(fb::AggregateMode_Partial);
  if (grouped) {
    builder.add_group_exprs(groups);
    builder.add_group_names(names);
  }
  builder.add_aggr_funcs(funcs_vec);
  builder.add_input(scan);
  builder.add_aggr_input_schema(input);
  auto agg = builder.Finish();
  fbb.Finish(fb::CreateGpuPlan(
      fbb, fb::CreatePlanNode(fbb, fb::PlanNodeKind_CudfAggregate, agg.Union())));
  return {fbb.GetBufferPointer(), fbb.GetBufferPointer() + fbb.GetSize()};
}

/// Seq `seq` called with no handle at all, as an init's done call makes it.
uint64_t call_with_nothing(peacock::NodeSession& session, uint64_t seq) {
  uint64_t out = 0;
  size_t produced = 0;
  peacock::NodeStats stats{};
  session.execute_node(seq, nullptr, nullptr, 0, &out, 1, &produced, &stats);
  EXPECT_EQ(produced, 1u);
  return out;
}

}  // namespace

TEST(AggregateNoInput, AKeylessNodeAnswersOneRowTypedAsItsInputDeclares) {
  flatbuffers::FlatBufferBuilder fbb;
  auto buf = unfed_aggregate(fbb, /*grouped=*/false);
  peacock::NodeSession session(buf.data(), buf.size());
  const auto& result = session.table_for(call_with_nothing(session, /*seq=*/1));
  auto view = result.view();
  ASSERT_EQ(view.num_rows(), 1);
  ASSERT_EQ(view.num_columns(), 5);
  EXPECT_EQ(hand::values_of<int64_t>(view.column(0)), std::vector<int64_t>{0})
      << "count(*) over nothing";
  EXPECT_EQ(view.column(0).null_count(), 0);
  EXPECT_EQ(view.column(1).type().id(), cudf::type_id::INT64);
  EXPECT_EQ(view.column(2).type(), (cudf::data_type{cudf::type_id::DECIMAL128, -2}))
      << "the scale is the field's, which the type id cannot carry";
  EXPECT_EQ(view.column(3).type().id(), cudf::type_id::TIMESTAMP_DAYS);
  EXPECT_EQ(view.column(4).type().id(), cudf::type_id::INT64);
  for (cudf::size_type c = 1; c < 5; ++c)
    EXPECT_EQ(view.column(c).null_count(), 1) << "column " << c << " is NULL over nothing";
  EXPECT_EQ(result.column_names,
            (std::vector<std::string>{"n", "sum(v)", "sum(amount)", "min(day)", "max(v)"}));
}

TEST(AggregateNoInput, AGroupedNodeAnswersNoGroups) {
  flatbuffers::FlatBufferBuilder fbb;
  auto buf = unfed_aggregate(fbb, /*grouped=*/true);
  peacock::NodeSession session(buf.data(), buf.size());
  const auto& result = session.table_for(call_with_nothing(session, /*seq=*/1));
  EXPECT_EQ(result.view().num_rows(), 0);
  EXPECT_EQ(result.column_names.front(), "k") << "the key leads, as over any input";
}

TEST(AggregateNoInput, AnyOtherNodeHandedNothingStillRefuses) {
  flatbuffers::FlatBufferBuilder fbb;
  auto scan = unread_scan(fbb);
  // Never evaluated: the filter's first act is take_input, which refuses.
  auto filter = fb::CreateCudfFilter(fbb, hand::ref(fbb, 0, "k"), scan);
  fbb.Finish(fb::CreateGpuPlan(
      fbb, fb::CreatePlanNode(fbb, fb::PlanNodeKind_CudfFilter, filter.Union())));
  std::vector<uint8_t> buf(fbb.GetBufferPointer(), fbb.GetBufferPointer() + fbb.GetSize());
  peacock::NodeSession session(buf.data(), buf.size());
  try {
    call_with_nothing(session, /*seq=*/1);
    FAIL() << "a filter handed no input answered";
  } catch (const std::runtime_error& e) {
    EXPECT_NE(std::string(e.what()).find("not enough input handles"), std::string::npos)
        << e.what();
  }
}
```

  `TableResult` is refcounted-scatter's (J, in the base): `view()` and `column_names`. If its
  reader is spelled otherwise, follow the code. `cpp/CMakeLists.txt`: append
  `tests/gpu/test_aggregate_no_input.cpp` to `add_executable(peacock_plan_tests …)`'s sources,
  after grouping-id's `tests/gpu/test_grouping_id.cpp` (keep every source already listed).

- [ ] **Step 2: Build locally.**

```bash
timeout 5400 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
```

  Expected: the build succeeds and `peacock_plan_tests` links with the new file. The gtests need
  the device: their red (the keyless and grouped cases FAIL with `take_input: not enough input
  handles for node`, `AnyOtherNodeHandedNothingStillRefuses` PASSES) is Task 8's red build, which
  reverts this task's `cpp/src` change.

- [ ] **Step 3: Implement.** `operators.h`, after `take_input`'s declaration:

```cpp
// Whether the call handed this node no input at all — an aggregate init's done call, made
// for a lane where no batch went out. Only execute_aggregate asks; every other operator
// resolves its child through take_input, which refuses.
bool was_handed_nothing(const NodeInputs* in);
```

  `dispatch.cpp`, after `take_input`:

```cpp
bool was_handed_nothing(const NodeInputs* in) {
  return in && in->items && in->items->empty();
}
```

  `aggregate.cpp`, a static above `execute_aggregate`:

```cpp
// The table a done call aggregates: the node's declared input at zero rows, so a keyless
// node answers its identity row through the same reduce a batch takes, and a grouped one no
// groups. A decimal takes its scale from the field, which the type id cannot carry.
static TableResult zero_rows_of(const fb::CudfAggregate* agg) {
  const auto* schema = agg->aggr_input_schema();
  if (!schema || !schema->fields())
    throw std::runtime_error(
        "CudfAggregate handed no input carries no aggr_input_schema to build one from");
  std::vector<std::unique_ptr<cudf::column>> columns;
  std::vector<std::string> names;
  for (const auto* field : *schema->fields()) {
    std::string name = field->name() ? field->name()->str() : "";
    cudf::type_id id = fb_to_type_id(field->data_type());
    if (id == cudf::type_id::EMPTY)
      throw std::runtime_error("CudfAggregate handed no input: its input column `" + name +
                               "` has no cuDF type");
    cudf::data_type type =
        id == cudf::type_id::DECIMAL128
            ? cudf::data_type{id, -static_cast<int32_t>(field->decimal_scale())}
            : cudf::data_type{id};
    columns.push_back(cudf::make_empty_column(type));
    names.push_back(std::move(name));
  }
  return TableResult::owning(std::make_unique<cudf::table>(std::move(columns)), std::move(names));
}
```

  And `execute_aggregate`'s first line becomes:

```cpp
  auto input = was_handed_nothing(in) ? zero_rows_of(agg) : take_input(in);
```

  `flatbuffers/gpu_plan.fbs` does not change: `aggr_input_schema`'s comment already says what it
  is, the input's schema.

- [ ] **Step 4: Build and the cpu ctest.**

```bash
timeout 5400 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 900 ctest --test-dir cpp/build -L cpu --output-on-failure
git clang-format HEAD -- cpp/src/peacock/operators.h cpp/src/operators/dispatch.cpp cpp/src/operators/aggregate.cpp cpp/tests/gpu/test_aggregate_no_input.cpp
wc -l cpp/tests/gpu/test_aggregate_no_input.cpp cpp/src/operators/aggregate.cpp
```

  Expected: build clean, `-L cpu` PASS, both files under 1000 lines. The device run is Task 8's
  green build.

- [ ] **Step 5: Commit.**

```bash
git add cpp/src/peacock/operators.h cpp/src/operators/dispatch.cpp cpp/src/operators/aggregate.cpp cpp/tests/gpu/test_aggregate_no_input.cpp cpp/CMakeLists.txt
git commit -m "$(printf '%s\n' \
  "#199: an aggregate handed no input aggregates its declared input at zero rows" "" \
  "Keyless answers one row, grouped no groups; any other node still refuses." \
  "No ABI change: a call with no child already reaches execute_one." \
  "The gtests are a new file; they run in the task's device session." "" \
  "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>")"
```

---

### Task 3: The pattern and the init's recipe

**Files:**
- Modify: `peacockdb-core/src/wire/mod.rs` (`CallPattern`, `CallPattern::text`, `Recipe`)
- Modify: `peacockdb-core/src/wire/attach.rs` (`aggregate`)
- Modify: `peacockdb-core/src/wire/recipes.rs` (`impl fmt::Display for Recipe`; `render_recipe_node`)
- Modify: `peacockdb-core/src/wire/tests.rs` (`mod aggregate_init;`; move `finalizing_init` and
  `an_init_that_finalizes_itself…` out)
- Create: `peacockdb-core/src/wire/tests/aggregate_init.rs`
- Create: `peacockdb-core/src/planner/tests/recipe_patterns.rs`; Modify: `planner/tests/mod.rs`
- Modify: `peacockdb-core/src/executor/gpu_backend/backend.rs` (the `Aggregate` arm)
- Modify: `peacockdb-core/src/executor/gpu_backend/gpu_tests/mod.rs` (`Session::exec`, `chaining_exec`)
- Modify: `peacockdb-core/src/wire/gpu_tests/mod.rs` (the walk's bookkeeping and dispatch);
  Create: `peacockdb-core/src/wire/gpu_tests/aggregate_init.rs` (the init's walk, two walks)
- Regenerate: `testdata/goldens/*/*.plans.txt`, `testdata/goldens/recipe-payloads.txt`

**Interfaces:**
- Produces `CallPattern::AtDoneIfNothingOut` (`text()` = `"at done, if nothing went out"`).
- Produces `#[cfg(not(feature = "rust-only"))] pub(crate) fn Recipe::calls_under(&self, when:
  CallPattern) -> Recipe`. Consumed by Task 4's `GpuAccumulator::aggregate_init`.
- Every `GpuAggregate` recipe: its per-batch calls, then the same `(seq, kind)`s under
  `AtDoneIfNothingOut`, the first with no inputs and each later one with `[PriorOutput]`.
- Consumes Task 2 (the walk's done call reaches the device).

The category stays `Exec` in this task. The GPU backend's exec arm builds from the per-batch calls
alone, so every commit drives the same calls it did; Task 4 flips the category.

- [ ] **Step 1: The failing recipe tests.** In `wire/tests.rs`, add `mod aggregate_init;` after the
  imports, and move `finalizing_init` and
  `an_init_that_finalizes_itself_carries_the_finalize_in_a_project_of_its_own` (with its doc) into
  the new file. `wire/tests/aggregate_init.rs`:

```rust
//! An aggregate's init as the recipe writes it: its calls per batch, and the same calls once
//! more at done for a lane where nothing went out, the first handed nothing.

use super::*;

/// A `sum(n)` init over `[k, n]`, keyed on `k` or keyless.
fn summing_init(keyed: bool) -> GpuAggregate {
    let group_by = if keyed { vec![Expr::column(0, "k")] } else { Vec::new() };
    let state: &[&str] = if keyed { &["k", "sum(n)"] } else { &["sum(n)"] };
    GpuAggregate::new(
        Given::input(BatchLayout::MultipleBatches, &["k", "n"]),
        AggregateBody {
            group_by,
            grouping_sets: Vec::new(),
            null_exprs: Vec::new(),
            aggs: vec![AggCall {
                func: PlanAgg::Sum,
                args: vec![Expr::column(1, "n")],
                outputs: vec![Field::new("sum(n)", DataType::Int64, true)],
            }],
            finalize: None,
        },
        columns_of(state),
        columns_of(state),
    )
}

/// The done call is the init's own seq, handed nothing: no second wire node. Grouped and
/// keyless alike — what the call answers over nothing is the engine's, not the recipe's.
#[test]
fn an_init_calls_its_own_seq_again_at_done_if_nothing_went_out() {
    for keyed in [true, false] {
        let recipe = aggregate(&summing_init(keyed), &[&columns_of(&["k", "n"])], &mut Writer::new())
            .expect("the aggregate's payloads are writable")
            .expect("an aggregate drives the ABI");
        assert_eq!(
            shape(&recipe),
            vec![
                (FbKind::Aggregate { merge: false }, CallPattern::PerBatch),
                (FbKind::Aggregate { merge: false }, CallPattern::AtDoneIfNothingOut),
            ],
            "keyed: {keyed}"
        );
        assert_eq!(recipe.calls[0].target, recipe.calls[1].target, "keyed: {keyed}");
        assert_eq!(recipe.calls[0].inputs, vec![Input::Batch]);
        assert_eq!(recipe.calls[1].inputs, Vec::<Input>::new(), "handed nothing");
        let (seq, _) = recipe.calls[0].target.expect("the init addresses a seq");
        assert_eq!(
            recipe.to_string(),
            format!(
                "per batch: execute_node(#{seq} CudfAggregate{{Partial}}, batch); \
                 at done, if nothing went out: execute_node(#{seq} CudfAggregate{{Partial}})"
            ),
            "a call handed nothing names its seq and nothing after it"
        );
    }
}
```

  And the moved shortcut test's expected shape grows to four calls:

```rust
    assert_eq!(
        shape(&recipe),
        vec![
            (FbKind::Aggregate { merge: false }, CallPattern::PerBatch),
            (FbKind::Project(ProjectRole::Finalize), CallPattern::PerBatch),
            (FbKind::Aggregate { merge: false }, CallPattern::AtDoneIfNothingOut),
            (FbKind::Project(ProjectRole::Finalize), CallPattern::AtDoneIfNothingOut),
        ],
        "the init builds state and finalizes it per batch, and once more over nothing at done"
    );
    assert_eq!(recipe.calls[2].target, recipe.calls[0].target);
    assert_eq!(recipe.calls[3].target, recipe.calls[1].target);
    assert_eq!(recipe.calls[2].inputs, Vec::<Input>::new());
    assert_eq!(recipe.calls[3].inputs, vec![Input::PriorOutput]);
```

  `wire/tests.rs` imports `aggregate` from `super::attach`; the child reaches it, `Given`,
  `columns_of`, `shape` and `summed` through `use super::*` (a child sees its parent's private
  items). Make `finalizing_init` and `summed` `pub(super)` only if the compiler asks.

- [ ] **Step 2: The structural guard.** `planner/tests/recipe_patterns.rs`, declared
  `mod recipe_patterns;` in `planner/tests/mod.rs`:

```rust
//! Which nodes call at done only if nothing went out: an aggregate's init, every one of them,
//! and nothing else. A node whose executor does not expect the pattern would make the call at
//! the wrong moment or refuse it, so the plan goldens are read for the claim in both
//! directions.

use crate::test_support::{MODES, golden_dir_for};

/// The datasets the mode goldens are written for — `plan_goldens`' own list.
const DATASETS: [(&str, &str); 3] = [("tpch", "1"), ("tpcds", "1"), ("pbench", "1")];

#[test]
fn only_an_aggregate_init_calls_at_done_if_nothing_went_out_and_every_init_does() {
    let mut inits = 0;
    for (dataset, sf) in DATASETS {
        for mode in &MODES {
            let path = golden_dir_for(dataset, sf).join(format!("{}.plans.txt", mode.name));
            let text = std::fs::read_to_string(&path).expect("a mode golden");
            // Every recipe line, not the first: a golden holds one section per query.
            for line in text.lines().map(str::trim_start) {
                let calls_if_nothing_out = line.contains("at done, if nothing went out:");
                let is_init = line.starts_with("GpuAggregate: calling_lanes=");
                assert_eq!(
                    calls_if_nothing_out,
                    is_init,
                    "{}: {line}",
                    path.display()
                );
                inits += usize::from(is_init);
            }
        }
    }
    assert!(inits > 100, "only {inits} init lines read — the goldens moved");
}
```

  If `plan_goldens.rs` names its dataset list as a shared constant by now (pbench added
  `CORPUS_DATASETS`), use it instead of `DATASETS`.

- [ ] **Step 3: Run both; they fail.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- wire::tests::aggregate_init planner::tests::recipe_patterns
```

  Expected: compile error on `CallPattern::AtDoneIfNothingOut`.

- [ ] **Step 4: The pattern.** `wire/mod.rs`, in `CallPattern` after `AtDone`:

```rust
    /// Once, when the node's input is complete, and only on a lane where no call of this
    /// node answered a row: an aggregate init's identity row, its seqs handed nothing first.
    AtDoneIfNothingOut,
```

  `CallPattern::text`: `Self::AtDoneIfNothingOut => "at done, if nothing went out",`.
  `Recipe::seqs`'s doc gains: "A seq a recipe calls under two patterns appears twice." And, in
  `impl Recipe`:

```rust
    /// The calls made under `when`, in order, as a recipe of their own: what an executor
    /// driving one phase of a node builds from. Only the GPU backend asks.
    #[cfg(not(feature = "rust-only"))]
    pub(crate) fn calls_under(&self, when: CallPattern) -> Recipe {
        Recipe::of(
            self.calls
                .iter()
                .filter(|call| call.when == when)
                .cloned()
                .collect(),
        )
    }
```

- [ ] **Step 5: The recipe.** `wire/attach.rs`, `aggregate()`, before `Ok(Some(Recipe::of(calls)))`;
  extend its doc comment with the done calls' sentence:

```rust
    // The same calls once more at done, the first handed nothing, for a lane where no batch
    // produced a row: each engine answers over no rows what it owes — the identity row
    // keyless, no groups grouped. The seqs are the init's own; no node is added.
    let at_done: Vec<Call> = calls
        .iter()
        .enumerate()
        .map(|(position, call)| {
            let (seq, kind) = call.target.expect("an aggregate's calls address seqs");
            let inputs = match position {
                0 => Vec::new(),
                _ => vec![Input::PriorOutput],
            };
            Call::seq(seq, kind, inputs, CallPattern::AtDoneIfNothingOut)
        })
        .collect();
    calls.extend(at_done);
```

- [ ] **Step 6: The renderer.** `wire/recipes.rs`, in `impl fmt::Display for Recipe`, the call
  text is built from a list, so a call handed nothing prints no trailing comma (every existing
  line renders byte for byte as before):

```rust
        for call in &self.calls {
            let mut arguments: Vec<String> = call
                .target
                .iter()
                .map(|(seq, kind)| format!("#{seq} {kind}"))
                .collect();
            arguments.extend(call.inputs.iter().map(|input| input.text().to_string()));
            let text = format!("{}({})", call.symbol.name(), arguments.join(", "));
            match groups.last_mut() {
                Some((when, calls)) if *when == call.when => calls.push(text),
                _ => groups.push((call.when, vec![text])),
            }
        }
```

  And in `render_recipe_node`'s payload loop, a seq the recipe calls twice prints its payload
  once:

```rust
        let mut printed = std::collections::BTreeSet::new();
        for call in &recipe.calls {
            let Some((seq, kind)) = call.target else {
                continue;
            };
            // The done call reuses the init's seqs: one node, one payload.
            if !printed.insert(seq) {
                continue;
            }
            // … unchanged …
```

- [ ] **Step 7: The GPU exec arm takes the per-batch calls.** `gpu_backend/backend.rs`, the
  `NodeRef::Aggregate` arm passes `&recipe.calls_under(CallPattern::PerBatch)` where it passed
  `recipe` (import `crate::wire::CallPattern`), with one line added to its comment: "Only its
  per-batch calls: an exec node is never called at done." In `gpu_backend/gpu_tests/mod.rs`,
  `Session::exec` and `Session::chaining_exec` do the same, since they build `GpuExec` from an
  aggregate's recipe too.

- [ ] **Step 8: The walk drives an init by its node kind.** `wire/gpu_tests/mod.rs` is past the
  1000-line cap, so it gains only bookkeeping and one dispatch; the init's walk and its tests go in
  a new child module.
  - `Session::execute` answers each handle beside its row count:

```rust
    /// One `execute_node`, its input handles grouped by the child slot each fills: each
    /// handle it answered, beside the rows in it.
    fn execute(&self, seq: Seq, inputs: &[Vec<u64>], out_cap: usize) -> Vec<(u64, u64)> {
        // … the call, unchanged …
        handles
            .into_iter()
            .zip(stats)
            .take(produced as usize)
            .map(|(handle, stats)| (handle, stats.rows))
            .collect()
    }
```

  - `Walk` gains `rows: std::collections::HashMap<u64, u64>` and `unfed: usize` (both
    `Default::default()` in `walk_with`); `Walked` gains `unfed: usize`, filled from `walk.unfed`.
  - `make` counts a call handed nothing and keeps the rows:

```rust
        if inputs.is_empty() {
            self.unfed += 1;
        }
        self.made.push((seq, kind));
        let answered = self.session.execute(seq, &inputs, out_cap);
        let handles: Vec<u64> = answered.iter().map(|(handle, _)| *handle).collect();
        self.rows.extend(answered);
        (self.on_call)(seq, kind, &handles, self.session);
        handles
```

  - In `node`, after the recipe is found and before the category match:

```rust
        // An aggregate's init, whichever category drives it: `aggregate_init.rs`.
        if let NodeRef::Aggregate(_) = as_node_ref(node) {
            return self.aggregate_init(node, recipe, &kids[0]);
        }
```

  - `phases`' doc gains: "`AtDoneIfNothingOut` is on the done side, which the init's walk runs
    only where nothing went out." (`partition` already puts it there.)
  - `assert_walk_matches_datafusion_with` returns `Walked`; `assert_walk_matches_datafusion`
    returns its `.calls`; `held` ignores it.
  - `mod aggregate_init;` after the imports, and the two queries below added to
    `the_kinds_a_device_has_run_are_the_kinds_this_file_claims`' list as
    `(aggregate_init::KEYLESS_OVER_AN_EMPTY_LANE, TWO_LANES)` and
    `(aggregate_init::KEYLESS_SUM, TWO_LANES)`.

  `wire/gpu_tests/aggregate_init.rs` (a child module sees `Walk`'s private fields and the
  parent's private items through `use super::*`):

```rust
//! An aggregate's init on the walk: its per-batch chain per batch, and its done chain — the
//! first call handed nothing — for a lane where no output had a row. The executor's flag,
//! kept per lane here since no executor runs.

use super::*;

impl Walk<'_> {
    /// The per-batch chain per batch, each output going straight out; the done chain where
    /// none of them answered a row, its output going out only if it has rows, as the
    /// executors answer it — so a grouped init over nothing emits nothing here too.
    pub(super) fn aggregate_init(
        &mut self,
        node: &dyn GpuNode,
        recipe: &Recipe,
        input: &Lanes,
    ) -> Lanes {
        let (per_batch, at_done) = phases(recipe);
        assert!(
            at_done
                .iter()
                .all(|call| call.when == CallPattern::AtDoneIfNothingOut),
            "{}: an init calls at done only if nothing went out",
            node.name()
        );
        let mut lanes = Vec::with_capacity(input.len());
        for lane in input {
            let mut out = Vec::with_capacity(lane.len().max(1));
            for handle in lane {
                let mut at = At {
                    batch: Some(*handle),
                    ..At::default()
                };
                out.push(only(self.chain(&per_batch, &mut at), node.name()));
            }
            if out.iter().all(|handle| self.rows[handle] == 0) {
                let mut at = At::default();
                let done = only(self.chain(&at_done, &mut at), node.name());
                // Dropped where it has no rows: the handle stays in the session's registry
                // until the session closes, which is all a dropped handle costs a walk.
                if self.rows[&done] > 0 {
                    out.push(done);
                }
            }
            lanes.push(out);
        }
        lanes
    }
}

/// nation is one row group, and a walk plans `Batching::Off`, which maps a source to
/// `target_partitions` lanes: at two lanes the second receives no batch.
pub(super) const KEYLESS_OVER_AN_EMPTY_LANE: &str = "SELECT count(*) AS n, \
     sum(n_nationkey) AS s, min(n_regionkey) AS lo FROM nation WHERE n_regionkey >= 0";
pub(super) const KEYLESS_SUM: &str = "SELECT sum(l_quantity) AS s FROM lineitem";

/// The empty lane's init makes its done call, handed nothing, and answers count 0 and
/// NULLs; the merge folds that row in beside the other lane's, and DataFusion agrees.
#[tokio::test]
async fn a_keyless_aggregate_answers_its_identity_row_on_a_lane_that_saw_no_batch() {
    let walked =
        assert_walk_matches_datafusion_with(KEYLESS_OVER_AN_EMPTY_LANE, TWO_LANES, &mut no_hook)
            .await;
    assert_eq!(
        walked.unfed,
        1,
        "one done call, on the empty lane: {}",
        trail(&walked.calls)
    );
}

#[tokio::test]
async fn a_keyless_aggregate_over_lanes_with_rows_makes_no_done_call() {
    let walked = assert_walk_matches_datafusion_with(KEYLESS_SUM, TWO_LANES, &mut no_hook).await;
    assert_eq!(walked.unfed, 0, "{}", trail(&walked.calls));
    assert_eq!(
        times(&walked.calls, FbKind::Aggregate { merge: false }),
        2,
        "one init call per lane"
    );
}
```

  The `count(*)` sits beside a `sum` and a filter so DataFusion cannot answer it from parquet
  statistics (#158). Before planning on it, print the walk's nation scan once (a scratch test,
  not committed) and confirm `partition_groups=[[[0]],[]]` at `TWO_LANES`; if nation maps
  otherwise, pick a one-row-group table that does. Make an item `pub(super)` in `mod.rs` only
  where the compiler asks.

- [ ] **Step 9: Run the recipe tests, then regenerate the plan goldens.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- wire:: planner::tests::recipe_patterns
UPDATE_CANONICAL=1 timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- planner::
git diff --stat testdata/goldens
```

  Expected: the recipe tests PASS. The regeneration rewrites every mode's `.plans.txt` (only
  `GpuAggregate:` recipe lines move) and the text of `recipe-payloads.txt`. The payload test runs
  under plain `UPDATE_CANONICAL=1`: it asserts every `sha256=` line unchanged and then rewrites
  the text (`plan_goldens.rs`, the three-state block), so **`PEACOCK_REWRITE_RECIPE_BYTES` stays
  unset** — a digest that moved is a wire change this task must not make, and the run says so.
  The test points `/tmp/peacock-plan-bytes-root` at the testdata root itself; `/tmp` must be
  writable. Then:

```bash
git diff -U0 testdata/goldens | grep '^[-+][^-+]' | grep -v 'GpuAggregate: calling_lanes' | head
git diff testdata/goldens/recipe-payloads.txt | grep '^[-+]sha256' | head
```

  Expected: both print nothing. A `GpuAggregate` line in the payload golden shows its payload
  once. The recipe guard (Step 2) now PASSES.

- [ ] **Step 10: The cudf build.**

```bash
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 5400 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
wc -l peacockdb-core/src/wire/gpu_tests/mod.rs peacockdb-core/src/wire/gpu_tests/aggregate_init.rs peacockdb-core/src/wire/tests.rs
```

  Expected: the gpu lib compiles, the two new walks in it; `mod.rs` grew by the dispatch and
  bookkeeping lines alone, the other two are under 1000. The walks run in Task 8: the first
  answers with the device's done call, so it is red in Task 8's red build.

- [ ] **Step 11: Commit.**

```bash
git add peacockdb-core/src/wire peacockdb-core/src/planner/tests peacockdb-core/src/executor/gpu_backend testdata/goldens
git commit -m "$(printf '%s\n' \
  "#199: an init's recipe calls itself once more at done if nothing went out" "" \
  "CallPattern::AtDoneIfNothingOut reuses the init's seqs, the first call handed" \
  "nothing. The walk drives it per lane; the device exec arm keeps the per-batch" \
  "calls until the init becomes an accumulator. Payload digests unchanged." "" \
  "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>")"
```

---

### Task 4: The init is a batch accumulator

**Files:**
- Modify: `peacockdb-core/src/plan/mod.rs` (`category_of`); Test: `plan/tests/mod.rs`
- Modify: `peacockdb-core/src/executor/cpu_backend/accumulate.rs` (`State::Init`, `AggregateInit`,
  `CpuAccumulator::aggregate_init`), `cpu_backend/backend.rs` (the arm; `held_bytes`),
  `cpu_backend/mod.rs` (the `CpuAccumulator` doc: five things)
- Modify: `peacockdb-core/src/executor/gpu_backend/mod.rs` (`GpuExec::exec` over `chain`;
  `exec_over_nothing`; the `GpuAccumulator` doc), `gpu_backend/accumulate.rs` (`State::Init`,
  `AggregateInit`, `GpuAccumulator::aggregate_init`), `gpu_backend/backend.rs` (the arm;
  `HeldBytes`)
- Modify: `peacockdb-core/src/executor/driver/tests/{mock,plans,mod}.rs`; Create:
  `driver/tests/init.rs`
- Modify: `peacockdb-core/src/executor/cpu_backend/tests/state_types.rs`
- Modify: `peacockdb-core/src/executor/gpu_backend/gpu_tests/{mod,accumulate}.rs`
- Modify: `peacockdb-core/src/tests/gpu_tests/{aggregate_cases,aggregate_dimension_cases,aggregate_schema_cases}.rs`
- Modify: `peacockdb-core/src/tests/end_to_end.rs`
- Regenerate: `testdata/goldens/{tpch,tpcds,pbench}.sf1/*-mini.{cpu,cost}.txt`

**Interfaces:**
- Consumes Task 1 (`empty_state`), Task 2 (the device's no-input call), Task 3 (the recipe,
  `Recipe::calls_under`).
- Produces: `category_of(GpuAggregate) == ExecutorCategory::BatchAccumulator`;
  `CpuAccumulator::aggregate_init(node: &GpuAggregate, input: &ArrowSchema, ctx: Arc<TaskContext>)
  -> Result<CpuAccumulator, PlanError>`; `GpuAccumulator::aggregate_init(site: CallSite, recipe:
  &Recipe, state: &ArrowSchema, output: &ArrowSchema) -> Result<GpuAccumulator, PlanError>`;
  `GpuExec::exec_over_nothing(&mut self) -> CallResult<GpuBatch>`; the mock's
  `AccRule::Init { keyless: bool }`. Each init executor holds `emitted: bool`.

- [ ] **Step 1: The category case.** `plan/tests/mod.rs`, after the aggregate validation cases:

```rust
/// An aggregate's init holds one flag and calls at done where nothing went out, so it is a
/// batch accumulator, as the limit — which streams and holds nothing — already is. One node,
/// one category, keyed or keyless.
#[test]
fn an_aggregate_init_is_a_batch_accumulator() {
    for group_by in [vec![Expr::column(0, "k")], Vec::new()] {
        let input = Given::input(one_lane(BatchLayout::MultipleBatches), &["k", "n"]);
        let state = columns(&["k", "n"]);
        let node = GpuAggregate::new(
            input,
            summing(group_by, vec![Expr::column(1, "n")], None),
            state.clone(),
            state,
        );
        assert_eq!(category_of(&node), ExecutorCategory::BatchAccumulator);
    }
}
```

- [ ] **Step 2: The mock's init rule and the lane cases.** `driver/tests/mock.rs`:

```rust
    /// An aggregate's init. A batch with rows answers its groups — one row where `keyless` —
    /// and a zero-row batch is released uncalled; at done, where nothing went out, one call
    /// answers the identity row a keyless init owes and nothing for a grouped one. Its calls
    /// are recorded, armed, so a lane case tells a skipped batch from an unmeasured run.
    Init { keyless: bool },
```

  as the last `AccRule` variant; `MockAcc` gains `emitted: bool` (`false` where it is built). In
  `executors_for`'s `BatchAccumulator` arm, beside the limit's rule:

```rust
                if let NodeRef::Aggregate(aggregate) = as_node_ref(node) {
                    script.accumulate = AccRule::Init {
                        keyless: aggregate.body.group_by.is_empty(),
                    };
                }
```

  The executor (import `AbiCall`, `AbiTarget` from `crate::executor`, `FbKind`, `Seq` from
  `crate::wire`):

```rust
/// The seq the mock's init calls are journalled under; nothing joins them to a region.
const INIT_SEQ: Seq = 0;

impl MockAcc {
    /// What an init call reports: the one call it made, or none.
    fn init_calls(&self, made: Option<(&MockBatch, &MockBatch)>) -> CallStats {
        let mut calls = AbiCalls::armed(true);
        if let Some((input, output)) = made {
            calls.record(AbiCall {
                seq: INIT_SEQ,
                target: AbiTarget::Node(FbKind::Aggregate { merge: false }),
                call_index: 0,
                in_rows: input.rows as u64,
                in_bytes: input.bytes as u64,
                out_rows: output.rows as u64,
                out_bytes: output.bytes as u64,
            });
        }
        CallStats {
            scratch_bytes: self.script.measured_scratch,
            calls,
        }
    }
}
```

  In `accumulate_and_fetch`'s match, before the holding arm:

```rust
            AccRule::Init { .. } if batch.rows == 0 => (Vec::new(), self.init_calls(None)),
            AccRule::Init { keyless } => {
                let out = MockBatch {
                    rows: if keyless { 1 } else { batch.rows },
                    bytes: 8,
                };
                self.emitted = true;
                let stats = self.init_calls(Some((&batch, &out)));
                (vec![out], stats)
            }
```

  In `mark_done_and_fetch`, after the failure check:

```rust
        if let AccRule::Init { keyless } = self.script.accumulate {
            if self.emitted {
                return Ok((Vec::new(), self.init_calls(None)));
            }
            let nothing = MockBatch { rows: 0, bytes: 0 };
            let identity = MockBatch {
                rows: usize::from(keyless),
                bytes: 8,
            };
            let stats = self.init_calls(Some((&nothing, &identity)));
            return Ok((if keyless { vec![identity] } else { Vec::new() }, stats));
        }
```

  `driver/tests/plans.rs` (import `AggCall`, `AggregateBody`, `GpuAggregate`, `PlanAgg`):

```rust
/// An aggregate's init: `count(k)`, grouped on `k` unless `keyless`. The driver reads its
/// category; what it emits is the mock's rule.
pub(crate) fn init(input: Box<dyn GpuNode>, keyless: bool) -> Box<dyn GpuNode> {
    let group_by = if keyless { Vec::new() } else { vec![Expr::column(0, "k")] };
    let mut fields: Vec<Field> = group_by
        .iter()
        .map(|_| Field::new("k", DataType::Int64, true))
        .collect();
    fields.push(Field::new("n", DataType::Int64, true));
    let mut state = Schema::new(Arc::new(ArrowSchema::new(fields)));
    state.group_keys = (0..group_by.len() as u32).collect();
    let body = AggregateBody {
        group_by,
        grouping_sets: Vec::new(),
        null_exprs: Vec::new(),
        aggs: vec![AggCall {
            func: PlanAgg::Count,
            args: vec![Expr::column(0, "k")],
            outputs: vec![Field::new("n", DataType::Int64, true)],
        }],
        finalize: None,
    };
    Box::new(GpuAggregate::new(input, body, state.clone(), state))
}
```

  `driver/tests/mod.rs` gains `mod init;`. `driver/tests/init.rs`:

```rust
//! An aggregate's init as the driver drives it: a batch accumulator whose lane makes its done
//! call only where nothing went out. The mock journals each init call, so a case counts the
//! calls each of a lane's backend calls made.

use super::mock::{Script, spec};
use super::plans::*;
use super::*;

/// Pre-order: the unload, the init, the source.
const INIT: usize = 1;

/// The init calls each of `lane`'s backend calls made, in order.
fn init_calls(report: &RunReport, lane: usize) -> Vec<usize> {
    report.abi_calls[INIT][lane]
        .iter()
        .map(|calls| calls.recorded().expect("the init rule is armed").len())
        .collect()
}

fn rows_out(report: &RunReport, lane: usize) -> Vec<u64> {
    report.emitted[INIT][lane].iter().map(|batch| batch.rows).collect()
}

#[test]
fn a_keyless_lane_that_saw_no_batch_makes_one_done_call_and_answers_one_row() {
    let script = Script::default().source("t", vec![vec![spec(10, 80)], vec![]]);
    let plan = unload(init(source("t", 2), true));
    let report = run(plan.as_ref(), &script);
    assert_eq!(init_calls(&report, 1), vec![1], "done, the lane's one call");
    assert_eq!(rows_out(&report, 1), vec![1]);
    assert_eq!(init_calls(&report, 0), vec![1, 0], "the other lane's batch, and no done call");
    assert_accounted(&report);
}

#[test]
fn a_keyless_lane_of_zero_row_batches_makes_no_init_call_and_one_done_call() {
    let script = Script::default().source("t", vec![vec![spec(0, 8), spec(0, 8)]]);
    let plan = unload(init(source("t", 1), true));
    let report = run(plan.as_ref(), &script);
    assert_eq!(init_calls(&report, 0), vec![0, 0, 1]);
    assert_eq!(rows_out(&report, 0), vec![1]);
    assert_accounted(&report);
}

#[test]
fn a_keyless_lane_with_rows_makes_no_done_call() {
    let script = Script::default().source("t", vec![vec![spec(10, 80), spec(4, 32)]]);
    let plan = unload(init(source("t", 1), true));
    let report = run(plan.as_ref(), &script);
    assert_eq!(init_calls(&report, 0), vec![1, 1, 0]);
    assert_eq!(rows_out(&report, 0), vec![1, 1]);
}

#[test]
fn zero_row_batches_among_others_change_no_row() {
    let plain = Script::default().source("t", vec![vec![spec(10, 80), spec(4, 32)]]);
    let padded = Script::default().source(
        "t",
        vec![vec![spec(0, 8), spec(10, 80), spec(0, 8), spec(4, 32), spec(0, 8)]],
    );
    for keyless in [true, false] {
        let plan = unload(init(source("t", 1), keyless));
        assert_eq!(
            rows_out(&run(plan.as_ref(), &padded), 0),
            rows_out(&run(plan.as_ref(), &plain), 0),
            "keyless: {keyless}"
        );
    }
}

#[test]
fn a_grouped_lane_that_saw_no_batch_makes_one_done_call_and_emits_nothing() {
    let script = Script::default().source("t", vec![vec![spec(10, 80)], vec![]]);
    let plan = unload(init(source("t", 2), false));
    let report = run(plan.as_ref(), &script);
    assert_eq!(init_calls(&report, 1), vec![1]);
    assert_eq!(rows_out(&report, 1), Vec::<u64>::new());
}

#[test]
fn a_grouped_lane_of_zero_row_batches_makes_one_done_call_and_emits_nothing() {
    let script = Script::default().source("t", vec![vec![spec(0, 8), spec(0, 8)]]);
    let plan = unload(init(source("t", 1), false));
    let report = run(plan.as_ref(), &script);
    assert_eq!(init_calls(&report, 0), vec![0, 0, 1]);
    assert_eq!(rows_out(&report, 0), Vec::<u64>::new());
}
```

  If `Driver::new` refuses the `init` plan, read the refusal and fix the builder to what it
  names (a validation rule the hand-built state misses), not the rule.

- [ ] **Step 3: The cpu identity per decomposition.** `cpu_backend/tests/state_types.rs`. Hoist
  the two local lists of `every_init_emits_the_state_its_decomposition_declares` into consts
  `EVERY_FUNC: [AggFunc; 7]` and `ARGUMENTS: [(u32, &str, DataType); 3]` and use them there.
  Split `init_declaring` in two: `init_parts(func, ordinal, name, input, keyed: bool, declare)
  -> (AggregateBody, Schema)` holds its body today, with the key field, `group_by` and
  `group_keys = vec![0]` only where `keyed`, and the Welford positions `[1, 2, 3]` keyed and
  `[0, 1, 2]` keyless; `init_declaring(func, ordinal, name, input, keyed, declare)` builds the
  `GpuAggregate` from those parts (`Given::of_columns(&INPUT)`, the state as both schemas) and
  returns it beside the state, as now. `init_of` and
  `an_init_declaring_a_narrower_decimal_than_it_produces_is_refused` pass `true`; add:

```rust
/// [`init_of`] with no group key: the init that owes a row whatever arrived.
fn keyless_init_of(func: AggFunc, ordinal: u32, name: &str, input: &DataType) -> GpuAggregate {
    init_declaring(func, ordinal, name, input, false, |state_type| state_type).0
}

/// [`keyless_init_of`] as the single-node shortcut builds it: the state finalized in the same
/// node, under the type DataFusion's `func` answers over a non-decimal argument — the first
/// state column's for sum, min, max and count, `Float64` for the rest.
fn keyless_shortcut_of(func: AggFunc, ordinal: u32, name: &str, input: &DataType) -> GpuAggregate {
    let (mut body, state) = init_parts(func, ordinal, name, input, false, |state_type| state_type);
    let output = format!("{}({name})", sql_name(func));
    let state_fields: Vec<Field> = state
        .fields
        .fields()
        .iter()
        .map(|field| field.as_ref().clone())
        .collect();
    let out_type = match func {
        AggFunc::Avg | AggFunc::Stddev | AggFunc::Var => DataType::Float64,
        _ => state_fields[0].data_type().clone(),
    };
    let expr = finalize(AggSpec { func, ddof: 1 }, &state_fields, 0, &out_type);
    body.finalize = Some(vec![NamedExpr::new(expr, &output)]);
    GpuAggregate::new(
        Given::of_columns(&INPUT),
        body,
        state,
        columns(&[(output.as_str(), out_type)]),
    )
}

fn zero_rows() -> CpuBatch {
    CpuBatch::new(RecordBatch::new_empty(columns(&INPUT).fields.clone()))
}

/// What a keyless init owes over no input, through the backend: one row, each state column
/// the value `PlanAgg::empty_state` gives — over no batch, and over two zero-row batches,
/// which it releases without running. Every decomposition, the Welford pair included: its
/// `(0, 0.0, 0.0)` is DataFusion's own variance state over no rows.
#[test]
fn every_keyless_init_answers_its_empty_state_over_no_input() {
    for func in EVERY_FUNC {
        for (ordinal, name, input) in &ARGUMENTS {
            let node = keyless_init_of(func, *ordinal, name, input);
            let (_, arg_type) = argument(func, *ordinal, name, input);
            let expected: Vec<ScalarValue> = node
                .body
                .aggs
                .iter()
                .map(|agg| agg.func.empty_state(&arg_type).expect("every init state is typed"))
                .collect();
            for zero_row_batches in [0, 2] {
                let mut init =
                    CpuAccumulator::aggregate_init(&node, &columns(&INPUT).fields, ctx())
                        .unwrap_or_else(|why| panic!("{func:?} over {input}: {why}"));
                for _ in 0..zero_row_batches {
                    let (out, _) = init.accumulate_and_fetch(zero_rows()).expect("accepted");
                    assert!(out.is_empty(), "{func:?} over {input}: a zero-row batch emits");
                }
                let (out, _) = init.mark_done_and_fetch().expect("done is accepted");
                let [row] = out.as_slice() else {
                    panic!("{func:?} over {input}: {} batches at done", out.len())
                };
                let row = row.record_batch();
                assert_eq!(row.num_rows(), 1, "{func:?} over {input}");
                for (column, want) in expected.iter().enumerate() {
                    let got = ScalarValue::try_from_array(row.column(column).as_ref(), 0)
                        .expect("a scalar at row 0");
                    assert_eq!(&got, want, "{func:?} over {input}, column {column}");
                }
            }
        }
    }
}

/// A grouped init over no input owes no groups, and emits nothing — not a zero-row batch.
#[test]
fn a_grouped_init_over_no_input_emits_nothing() {
    let (node, _) = init_of(AggFunc::Sum, 1, "i", &DataType::Int32);
    for zero_row_batches in [0, 2] {
        let mut init = CpuAccumulator::aggregate_init(&node, &columns(&INPUT).fields, ctx())
            .expect("the init builds");
        for _ in 0..zero_row_batches {
            assert!(init.accumulate_and_fetch(zero_rows()).expect("accepted").0.is_empty());
        }
        assert!(init.mark_done_and_fetch().expect("done").0.is_empty());
    }
}

/// The shortcut over no input finalizes the identity row, for every function the cpu answers
/// — the Welford pair included, whose device half is welford-device's: count 0, every other
/// output NULL (a sample or population divisor over a zero count is NULL, never an error).
/// Over no batch and over two zero-row batches. A decimal `avg` is the harness's
/// `a_decimal_average_shortcut_over_no_input_answers_null_on_both`.
#[test]
fn every_keyless_shortcut_answers_count_0_and_nulls_over_no_input() {
    for func in EVERY_FUNC {
        for (ordinal, name, input) in [(1, "i", DataType::Int32), (3, "f", DataType::Float64)] {
            let node = keyless_shortcut_of(func, ordinal, name, &input);
            let declared = node.kind().schema().expect("not a sink").fields.clone();
            let want = match func {
                AggFunc::Count => ScalarValue::Int64(Some(0)),
                _ => ScalarValue::try_from(declared.field(0).data_type()).expect("a typed NULL"),
            };
            for zero_row_batches in [0, 2] {
                let mut init = CpuAccumulator::aggregate_init(&node, &columns(&INPUT).fields, ctx())
                    .unwrap_or_else(|why| panic!("{func:?} over {input}: {why}"));
                for _ in 0..zero_row_batches {
                    assert!(init.accumulate_and_fetch(zero_rows()).expect("accepted").0.is_empty());
                }
                let (out, _) = init.mark_done_and_fetch().expect("done is accepted");
                let [row] = out.as_slice() else {
                    panic!("{func:?} over {input}: {} batches at done", out.len())
                };
                let row = row.record_batch();
                assert_eq!(row.num_rows(), 1, "{func:?} over {input}");
                assert_eq!(
                    row.column(0).data_type(),
                    declared.field(0).data_type(),
                    "{func:?} over {input}"
                );
                let got = ScalarValue::try_from_array(row.column(0).as_ref(), 0).expect("row 0");
                assert_eq!(got, want, "{func:?} over {input}");
            }
        }
    }
}
```

  Import `ScalarValue`, `RecordBatch`, `NamedExpr` and `crate::plan::finalize` if `super::*`
  does not bring them.

- [ ] **Step 4: The harness identity cases**, in `tests/gpu_tests/aggregate_cases.rs` after the
  empty-input init cases (import `AggSpec`, `decomposition`, `finalize` from `crate::plan` and
  `each_answers` from `super::script`):

```rust
/// The functions whose keyless init the device answers today. `Stddev` and `Var` join when
/// the device has a keyless Welford (#216, welford-device); their cpu half is
/// `cpu_backend/tests/state_types.rs`.
const DEVICE_IDENTITY: [AggFunc; 5] =
    [AggFunc::Sum, AggFunc::Min, AggFunc::Max, AggFunc::Count, AggFunc::Avg];

fn sql_name(func: AggFunc) -> &'static str {
    match func {
        AggFunc::Sum => "sum",
        AggFunc::Min => "min",
        AggFunc::Max => "max",
        AggFunc::Count => "count",
        AggFunc::Avg => "avg",
        AggFunc::Stddev => "stddev",
        AggFunc::Var => "var",
    }
}

/// The keyless init `decompose` writes for `func(i64)`, its state typed by `state_type` —
/// finalized where `shortcut`, as the single-node shortcut is.
fn keyless_init(func: AggFunc, shortcut: bool) -> GpuAggregate {
    let output = format!("{}(i64)", sql_name(func));
    let mut fields = Vec::new();
    let mut aggs = Vec::new();
    for (suffix, agg) in decomposition(func).state {
        let state_type = agg.state_type(&DataType::Int64).expect("typed over Int64");
        let column = format!("{output}{suffix}");
        aggs.push(call(*agg, Expr::column(3, "i64"), &column, state_type.clone()));
        fields.push((column, state_type));
    }
    let named: Vec<(&str, DataType)> =
        fields.iter().map(|(name, ty)| (name.as_str(), ty.clone())).collect();
    let mut state = columns(&named);
    if matches!(func, AggFunc::Stddev | AggFunc::Var) {
        state.agg_state = vec![AggStateColumns {
            output: output.clone(),
            func,
            ddof: 1,
            positions: vec![0, 1, 2],
        }];
    }
    if !shortcut {
        return GpuAggregate::new(given(), body(Vec::new(), aggs, None), state.clone(), state);
    }
    let out_type = match func {
        AggFunc::Avg | AggFunc::Stddev | AggFunc::Var => DataType::Float64,
        _ => fields[0].1.clone(),
    };
    let state_fields: Vec<Field> =
        state.fields.fields().iter().map(|f| f.as_ref().clone()).collect();
    let expr = finalize(AggSpec { func, ddof: 1 }, &state_fields, 0, &out_type);
    GpuAggregate::new(
        given(),
        body(Vec::new(), aggs, Some(vec![NamedExpr::new(expr, &output)])),
        state,
        columns(&[(output.as_str(), out_type)]),
    )
}

/// `empty_state`'s row for `node`'s state, its argument typed `input`. `pub(crate)`:
/// welford-device's keyless cases read it from `welford_cases.rs`.
pub(crate) fn empty_state_row(node: &GpuAggregate, input: &DataType) -> RecordBatch {
    let arrays: Vec<ArrayRef> = node
        .body
        .aggs
        .iter()
        .map(|agg| {
            agg.func
                .empty_state(input)
                .expect("every init state is typed")
                .to_array()
                .expect("one row")
        })
        .collect();
    RecordBatch::try_new(node.intermediate().fields.clone(), arrays).expect("the state's columns")
}

/// The slots a keyless init answers over `zero_row_batches` zero-row batches: nothing per
/// batch, and `row` at done. `pub(crate)`, as [`empty_state_row`].
pub(crate) fn at_done(zero_row_batches: usize, row: RecordBatch) -> Vec<Vec<RecordBatch>> {
    let mut slots = vec![Vec::new(); zero_row_batches];
    slots.push(vec![row]);
    slots
}

// A keyless init owes one row whatever arrived: over no batch and over zero-row batches,
// the row `empty_state` gives, on both engines.
operator_case! {
    GpuAggregate,
    fn a_keyless_init_over_no_input_answers_its_empty_state_on_both() {
        for func in DEVICE_IDENTITY {
            let node = keyless_init(func, false);
            let row = empty_state_row(&node, &DataType::Int64);
            for zero_row_batches in [0, 2] {
                let arrivals = (0..zero_row_batches).map(|seed| synthetic(0, seed as u64)).collect();
                let outcome = run_both(&node, Script::Accumulate(arrivals));
                let slots = at_done(zero_row_batches, row.clone());
                each_answers(&outcome, &slots, &slots);
            }
        }
    }
}

// The shortcut finalizes the identity row: count 0, every other output NULL — `avg`'s
// NULL sum over a zero count included, which divides to NULL rather than failing — over no
// batch and over two zero-row batches.
operator_case! {
    GpuAggregate,
    fn a_keyless_shortcut_over_no_input_answers_count_0_and_nulls_on_both() {
        for func in DEVICE_IDENTITY {
            let node = keyless_init(func, true);
            let output = node.kind().schema().expect("not a sink").fields.clone();
            let value = match func {
                AggFunc::Count => ScalarValue::Int64(Some(0)),
                _ => ScalarValue::try_from(output.field(0).data_type()).expect("a typed NULL"),
            };
            let row = RecordBatch::try_new(output, vec![value.to_array().expect("one row")])
                .expect("one column");
            for zero_row_batches in [0, 2] {
                let arrivals = (0..zero_row_batches).map(|seed| synthetic(0, seed as u64)).collect();
                let outcome = run_both(&node, Script::Accumulate(arrivals));
                let slots = at_done(zero_row_batches, row.clone());
                each_answers(&outcome, &slots, &slots);
            }
        }
    }
}

// The decimal sum's NULL carries the declared Decimal128(28, 2) on both: the device's done
// call builds its zero-row input from the declared Decimal128(18, 2), scale included.
operator_case! {
    GpuAggregate,
    fn a_decimal_sum_over_no_input_answers_a_null_of_its_declared_type_on_both() {
        let node = decimal_sum();
        let row = empty_state_row(&node, &DataType::Decimal128(18, 2));
        let outcome = run_both(&node, Script::Accumulate(Vec::new()));
        let slots = at_done(0, row);
        each_answers(&outcome, &slots, &slots);
    }
}

// The decimal avg's shortcut over nothing: a NULL sum over a zero count cast to Decimal128
// (22, 0), divided — NULL on both, never a divide-by-zero.
operator_case! {
    GpuAggregate,
    fn a_decimal_average_shortcut_over_no_input_answers_null_on_both() {
        let (state, output, finalize_list) = planned_decimal_average();
        let aggs = vec![
            call(PlanAgg::Sum, Expr::column(1, "dec"), "avg(dec)$sum", DataType::Decimal128(28, 2)),
            call(PlanAgg::Count, Expr::column(1, "dec"), "avg(dec)$count", DataType::Int64),
        ];
        let node = GpuAggregate::new(
            Given::of(Schema::new(decimals(0, 0).schema()), BatchLayout::MultipleBatches),
            body(Vec::new(), aggs, Some(finalize_list)),
            state,
            output.clone(),
        );
        let null = ScalarValue::try_from(output.fields.field(0).data_type()).expect("typed");
        let row = RecordBatch::try_new(output.fields.clone(), vec![null.to_array().expect("one")])
            .expect("one column");
        let outcome = run_both(&node, Script::Accumulate(vec![decimals(0, 1)]));
        let slots = at_done(1, row);
        each_answers(&outcome, &slots, &slots);
    }
}

// A grouped init over nothing owes no groups and emits nothing at all.
operator_case! {
    GpuAggregate,
    fn a_grouped_init_over_no_input_answers_nothing_on_both() {
        let node = init(true, vec![call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64)]);
        for arrivals in [Vec::new(), vec![synthetic(0, 1), synthetic(0, 2)]] {
            let calls = arrivals.len() + 1;
            let outcome = run_both(&node, Script::Accumulate(arrivals));
            let slots = vec![Vec::new(); calls];
            each_answers(&outcome, &slots, &slots);
        }
    }
}

// Zero-row batches among batches with rows: no call and no slot of their own, and no done
// call — the rows are what the batches with rows answer.
operator_case! {
    GpuAggregate,
    fn zero_row_batches_among_others_change_no_row_on_both() {
        let node = init(
            false,
            vec![
                call(PlanAgg::Count, Expr::column(2, "i32"), "count(i32)", DataType::Int64),
                call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64),
            ],
        );
        let arrivals = vec![synthetic(0, 1), input(), synthetic(0, 2), synthetic(64, 2)];
        let outcome = run_both(&node, Script::Accumulate(arrivals));
        outcome.same(Order::Any);
        let rows: Vec<usize> = outcome.cpu.as_ref().expect("the cpu answers").iter()
            .map(|slot| slot.iter().map(RecordBatch::num_rows).sum())
            .collect();
        assert_eq!(rows, vec![0, 1, 0, 1, 0], "a row per batch with rows, nothing at done");
    }
}
```

  `aggregate_cases.rs` is 755 lines; with these it stays under 1000. If it would not, move the
  cases to a new `tests/gpu_tests/aggregate_identity_cases.rs` declared in `gpu_tests/mod.rs`.

- [ ] **Step 5: The end-to-end case** (Review focus 5), in `src/tests/end_to_end.rs` after
  `a_two_key_group_by_over_many_rows_does_not_emit_a_group_twice`:

```rust
/// An aggregate on the side of a full outer join, over an input no row survives: its init
/// emits nothing, so the join meets lanes with no build batch, and still owes every nation
/// padded. A shape where the init's zero-row batch used to be what the join read.
#[tokio::test]
async fn an_outer_join_over_an_aggregate_of_nothing_answers_as_datafusion() {
    let sql = "SELECT n_name, c.cnt FROM (SELECT c_nationkey, count(*) AS cnt FROM customer \
               WHERE c_custkey + c_nationkey < 0 GROUP BY c_nationkey) c \
               FULL JOIN nation ON c.c_nationkey = n_nationkey";
    sql_answers_match_datafusion("tpch", "outer-join-over-nothing", sql, None, Coverage::ModesOnly)
        .await;
}
```

  Print its tp4-rowgroup plan once (a `planner::plan` call in a scratch test, not committed) and
  check that one side of the `GpuHashJoin` is the aggregate sequence; if DataFusion folded the
  aggregate away, change the predicate until it does not.

- [ ] **Step 6: The red runs.** Run the category and mock cases as soon as Steps 1–2 are written,
  before Step 3's cases exist — those call `aggregate_init` and stop the lib from compiling
  until Step 8, which is their red:

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- plan::tests::an_aggregate_init executor::driver::tests::init
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- tests::end_to_end::an_outer_join_over
```

  Expected: the category case FAILS (`Exec`); the mock lane cases FAIL (the exec rule answers per
  batch, nothing at done). The end-to-end case PASSES on the cpu before and after — it pins the
  shape for the device and for the next refactor. The harness cases (Step 4) and the device
  executor cases (Step 10) run on the device: their red is Task 8's red build, where the device's
  done call is reverted and each fails with `take_input: not enough input handles for node`.

- [ ] **Step 7: The category.** `plan/mod.rs`, `category_of`: `NodeRef::Aggregate(_)` leaves the
  `Exec` arm and joins `CoalesceAllBatches | AccumulateBatchesAndSort | AggregateBatches | Limit`.

- [ ] **Step 8: The cpu init.** `cpu_backend/accumulate.rs`: `State` gains `Init(AggregateInit)`;
  the module doc says five behaviours ("…, a limit that forwards, slices or drops, and an
  aggregate's init that runs per batch with rows and once more over nothing at done"); both
  dispatch matches gain the arm. Import `GpuAggregate` and `super::CpuExec`.

```rust
    /// An aggregate's init: the exec node's operators, run per batch with rows, and once over
    /// an empty batch of its input at done where nothing went out.
    pub(crate) fn aggregate_init(
        node: &GpuAggregate,
        input: &ArrowSchema,
        ctx: Arc<TaskContext>,
    ) -> Result<Self, PlanError> {
        Ok(Self::of(State::Init(AggregateInit {
            chain: CpuExec::aggregate(node, input, ctx)?,
            input: Arc::new(input.clone()),
            emitted: false,
        })))
    }
```

```rust
/// An aggregate's init. A zero-row batch is dropped uncalled, so adding one changes no row;
/// a batch with rows runs the init (and the shortcut's finalize); done runs them over an empty
/// batch where nothing went out — DataFusion's identity row keyless, no groups grouped.
pub(crate) struct AggregateInit {
    chain: CpuExec,
    input: SchemaRef,
    emitted: bool,
}

impl AggregateInit {
    fn accumulate_and_fetch(&mut self, batch: CpuBatch) -> CallResult<Vec<CpuBatch>> {
        if batch.record_batch().num_rows() == 0 {
            return Ok((Vec::new(), CallStats::default()));
        }
        let (out, stats) = self.chain.exec(batch)?;
        self.emitted |= out.record_batch().num_rows() > 0;
        Ok((vec![out], stats))
    }

    fn mark_done_and_fetch(mut self) -> CallResult<Vec<CpuBatch>> {
        if self.emitted {
            return Ok((Vec::new(), CallStats::default()));
        }
        let nothing = CpuBatch::new(RecordBatch::new_empty(self.input.clone()));
        let (out, stats) = self.chain.exec(nothing)?;
        match out.record_batch().num_rows() {
            0 => Ok((Vec::new(), stats)),
            _ => Ok((vec![out], stats)),
        }
    }
}
```

  `cpu_backend/backend.rs`: the `NodeRef::Aggregate` arm becomes
  `NodeExecutors::BatchAccumulator(CpuAccumulator::aggregate_init(aggregate, &input(0), ctx.clone())?)`;
  `held_bytes` gains `State::Init(_) => 0,` ("an init holds a flag"). `cpu_backend/mod.rs`'s
  `CpuAccumulator` doc says five things.

- [ ] **Step 9: The device init.** `gpu_backend/mod.rs`, `GpuExec::exec` becomes a call into one
  chain both entry points share:

```rust
    pub(crate) fn exec(&mut self, batch: GpuBatch) -> CallResult<GpuBatch> {
        let input = Consumed::of(&batch);
        let (_, handle) = batch.consume();
        self.chain(vec![vec![handle]], input)
    }

    /// The same calls with the first handed no input at all: an aggregate's done call, which
    /// aggregates the zero-row table its node's declared input describes.
    pub(crate) fn exec_over_nothing(&mut self) -> CallResult<GpuBatch> {
        self.chain(Vec::new(), Consumed::default())
    }

    /// The calls in order, `first` into the first and each output into the next. The input
    /// handles are consumed by the first call and every intermediate by the call after it.
    fn chain(&self, first: Vec<Vec<u64>>, mut input: Consumed) -> CallResult<GpuBatch> {
        let mut calls = AbiCalls::armed(node_timing_on());
        let mut handles = first;
        let mut last = (0u64, 0u64);
        for (position, (seq, kind)) in self.calls.iter().enumerate() {
            let (out, stats) = execute_node(self.site, *seq, *kind, &handles)?;
            let out_bytes = priced(stats, self.out_schema(position));
            calls.record(AbiCall {
                seq: *seq,
                target: AbiTarget::Node(*kind),
                call_index: 0,
                in_rows: input.rows,
                in_bytes: input.bytes,
                out_rows: stats.rows,
                out_bytes,
            });
            handles = vec![vec![out]];
            input = Consumed {
                rows: stats.rows,
                bytes: out_bytes,
            };
            last = (stats.rows, out_bytes);
        }
        let seq = self.calls.last().expect("an exec node makes at least one call").0;
        Ok((
            GpuBatch::new(self.site.executor, handles[0][0], seq, last.0 as usize, last.1 as usize),
            CallStats {
                scratch_bytes: None,
                calls,
            },
        ))
    }
```

  (Keep the existing comments of `exec` where they still describe the chain.) The `GpuAccumulator`
  doc says five things. `gpu_backend/accumulate.rs`: `State` gains `Init(AggregateInit)`, both
  dispatch matches the arm; import `super::GpuExec`.

```rust
    /// An aggregate's init: its calls per batch, and the same calls at done if nothing went
    /// out, the first handed nothing. Anything else is a recipe this executor would drive at
    /// the wrong moment.
    pub(crate) fn aggregate_init(
        site: CallSite,
        recipe: &Recipe,
        state: &ArrowSchema,
        output: &ArrowSchema,
    ) -> Result<Self, PlanError> {
        let per_batch = recipe.calls_under(CallPattern::PerBatch);
        let at_done = recipe.calls_under(CallPattern::AtDoneIfNothingOut);
        let mirrors = per_batch.calls.len() + at_done.calls.len() == recipe.calls.len()
            && per_batch.calls.len() == at_done.calls.len()
            && per_batch.calls.iter().zip(&at_done.calls).enumerate().all(
                |(position, (call, done))| {
                    let handed = match position {
                        0 => Vec::new(),
                        _ => vec![Input::PriorOutput],
                    };
                    done.target == call.target && done.inputs == handed
                },
            );
        if !mirrors {
            return Err(shape(
                "an init makes its calls per batch, and the same calls at done if nothing \
                 went out, the first handed nothing",
                recipe,
            ));
        }
        Ok(Self::of(State::Init(AggregateInit {
            chain: GpuExec::new(site, &per_batch, Some(state), output)?,
            emitted: false,
        })))
    }
```

```rust
/// An aggregate's init on a device: one flag. A zero-row batch is released uncalled, as the
/// limit releases a batch outside its interval; a batch with rows runs the chain; done runs it
/// handed nothing where nothing went out, which the device answers over the zero-row table of
/// the node's declared input.
pub(crate) struct AggregateInit {
    chain: GpuExec,
    emitted: bool,
}

impl AggregateInit {
    fn accumulate_and_fetch(&mut self, batch: GpuBatch) -> CallResult<Vec<GpuBatch>> {
        if batch.num_rows() == 0 {
            return Ok((Vec::new(), no_abi_calls()));
        }
        let (out, stats) = self.chain.exec(batch)?;
        self.emitted |= out.num_rows() > 0;
        Ok((vec![out], stats))
    }

    fn mark_done_and_fetch(mut self) -> CallResult<Vec<GpuBatch>> {
        if self.emitted {
            return Ok((Vec::new(), no_abi_calls()));
        }
        let (out, stats) = self.chain.exec_over_nothing()?;
        match out.num_rows() {
            // Dropped here, which releases the handle: no groups is nothing to emit.
            0 => Ok((Vec::new(), stats)),
            _ => Ok((vec![out], stats)),
        }
    }
}
```

  `gpu_backend/backend.rs`: the `NodeRef::Aggregate` arm becomes

```rust
            // The init: per batch with rows, and at done where nothing went out. Its first
            // call answers with the state its finalize reads, and nothing builds a batch from
            // that, so the state schema is the only thing that can price it.
            NodeRef::Aggregate(aggregate) => {
                NodeExecutors::BatchAccumulator(GpuAccumulator::aggregate_init(
                    site,
                    recipe,
                    &aggregate.intermediate().fields.as_ref().clone(),
                    &out(node),
                )?)
            }
```

  and `HeldBytes for GpuAccumulator` gains `State::Init(_) => 0,`.

- [ ] **Step 10: The device executor cases.** `gpu_backend/gpu_tests/mod.rs`, `Session` gains:

```rust
    /// The init accumulator for the aggregate at `index`: `state` is what its first call
    /// answers with, `schema` what the node declares.
    fn init(&self, index: usize, state: &ArrowSchema, schema: &ArrowSchema) -> GpuAccumulator {
        GpuAccumulator::aggregate_init(self.site(), self.recipe(index), state, schema)
            .expect("the recipe is an init's")
    }
```

  `gpu_backend/gpu_tests/accumulate.rs`, after the limit's cases (they use `Measuring`):

```rust
/// A keyless `count(v), sum(v)` over `input`, and its state.
fn keyless_count_sum(input: Box<dyn GpuNode>) -> (Box<dyn GpuNode>, ArrowSchema) {
    let state = schema_of(&[("count(v)", DataType::Int64), ("sum(v)", DataType::Int64)]);
    let value = |func, out: &str| AggCall {
        func,
        args: vec![Expr::column(1, "v")],
        outputs: vec![Field::new(out, DataType::Int64, true)],
    };
    let node = GpuAggregate::new(
        input,
        AggregateBody {
            group_by: Vec::new(),
            grouping_sets: Vec::new(),
            null_exprs: Vec::new(),
            aggs: vec![value(PlanAgg::Count, "count(v)"), value(PlanAgg::Sum, "sum(v)")],
            finalize: None,
        },
        Schema::new(Arc::new(state.clone())),
        Schema::new(Arc::new(state.clone())),
    );
    (Box::new(node), state)
}

fn calls_made(stats: &CallStats) -> Option<usize> {
    stats.calls.recorded().map(<[AbiCall]>::len)
}

/// Over a filter that keeps nothing: the zero-row batch is released with no call, and done —
/// nothing having gone out — calls the init's seq with no input, answering count 0, sum NULL.
#[test]
fn a_keyless_init_releases_a_zero_row_batch_and_answers_its_identity_row_at_done() {
    let filter = GpuFilter::new(source(), greater_than(100), None, Schema::new(Arc::new(columns())));
    let (tree, state) = keyless_count_sum(Box::new(filter));
    let session = Session::open(tree.as_ref());
    let _measuring = Measuring::on();
    let (nothing, _) = session
        .exec(1, &columns())
        .exec(session.scan(&ROW_GROUPS))
        .expect("the filter runs");
    assert_eq!(nothing.num_rows(), 0, "the filter kept nothing");
    let mut init = session.init(2, &state, &state);
    let (out, stats) = init.accumulate_and_fetch(nothing).expect("accepted");
    assert!(out.is_empty());
    assert_eq!(calls_made(&stats), Some(0), "released with no call");
    let (out, stats) = init.mark_done_and_fetch().expect("done is accepted");
    assert_eq!(calls_made(&stats), Some(1), "the init's seq, handed nothing");
    let [identity] = <[GpuBatch; 1]>::try_from(out).expect("one batch at done");
    let (row, _) = session
        .export(&state)
        .unload(identity, RowRange::WHOLE)
        .expect("the row crosses");
    assert_eq!(
        rows(&row),
        vec![vec![ScalarValue::Int64(Some(0)), ScalarValue::Int64(None)]]
    );
}

#[test]
fn a_keyless_init_that_answered_rows_makes_no_done_call() {
    let (tree, state) = keyless_count_sum(source());
    let session = Session::open(tree.as_ref());
    let _measuring = Measuring::on();
    let mut init = session.init(1, &state, &state);
    let (out, _) = init.accumulate_and_fetch(session.scan(&ROW_GROUPS)).expect("accepted");
    assert_eq!(out.len(), 1);
    let (out, stats) = init.mark_done_and_fetch().expect("done is accepted");
    assert!(out.is_empty());
    assert_eq!(calls_made(&stats), Some(0));
}
```

  Import what the file lacks (`GpuFilter`, `GpuAggregate`, `CallStats`, `AbiCall`). The existing
  cases that run an init through `session.exec` keep doing so: they test the `GpuExec` chain the
  init is built from.

- [ ] **Step 11: The harness's scripts follow the category.** In `aggregate_cases.rs`,
  `aggregate_dimension_cases.rs` and `aggregate_schema_cases.rs`, every `Script::Exec(` drives a
  `GpuAggregate`, so:

```bash
sed -i 's/Script::Exec(/Script::Accumulate(/g' \
  peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs \
  peacockdb-core/src/tests/gpu_tests/aggregate_dimension_cases.rs \
  peacockdb-core/src/tests/gpu_tests/aggregate_schema_cases.rs
grep -n 'Script::Exec' peacockdb-core/src/tests/gpu_tests/aggregate*.rs
```

  Expected: the grep prints nothing. An accumulator answers one slot per call, so each case gains
  a trailing done slot. `.same(…)`, `cpu_slot(_, 0)`, `gpu_slot(_, 0)` and the schema cases'
  flattened divergences need nothing. A helper asserting exact slots (`gpu_answered`, any
  `each_answers` with a literal slot list) gains the trailing `vec![]`. Then, by what is now true:
  - `a_grouped_aggregate_over_zero_rows_is_zero_rows_on_both` →
    `a_grouped_aggregate_over_a_zero_row_batch_answers_nothing_on_both` (both slots empty);
  - `a_global_aggregate_over_zero_rows_keeps_its_identity_row` keeps its name: the row now comes
    at done;
  - `aggregate_dimension_cases.rs`'s `a_grouped_count_star_over_zero_rows_is_zero_rows_on_both` →
    `a_grouped_count_star_over_a_zero_row_batch_answers_nothing_on_both`;
  - grouping-id's `grouping_sets_over_zero_rows_are_zero_rows_of_the_declared_id_type` loses its
    subject — a grouping-set init over a zero-row batch now emits nothing — so it becomes
    `grouping_sets_over_a_zero_row_batch_answer_nothing_on_both`. The id's declared type stays
    proven by the case with rows and by grouping-id's gtests. A grand-total row over nothing is
    #283's, not this task's.

- [ ] **Step 12: The rust-only tiers, the cudf build.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 5400 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
timeout 1800 cargo build --features rust-only -p peacockdb-core 2>&1 | grep -c '^warning' || true
```

  Expected: PASS, the new cases included; `every_node_kind_builds_the_executor_its_category_names`
  (cpu backend) passes with the aggregate's new category; the warning count unchanged. A count
  in `tests::end_to_end` or `driver::tests` that moved because an aggregate lane now records a
  `MarkDone` is read: if the new count is what the init does now, update it with a one-line why.

- [ ] **Step 13: The corpus's cpu sections.** Every section whose aggregate saw a zero-row batch or
  an empty lane changes: the init's batch lists lose the zero-row batches and gain an identity
  row per empty keyless lane; nothing else moves.

```bash
UPDATE_CANONICAL=1 timeout 10800 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
git diff --stat testdata/goldens
timeout 10800 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens --test test_cost_model
git diff testdata/goldens/*/mini.result.txt testdata/goldens/*/duckdb-result.txt | head
```

  Arm the monitor for the two corpus runs. Expected: `.cpu.txt` and `.cost.txt` move; no
  `mini.result.txt` or `duckdb-result.txt` line moves (the last command prints nothing). Read the
  `--stat` for a section that vanished (#213, two writers) and refill it with
  `PCK_UPDATE_SECTIONS=1 … -- --exact <case>`. Spot-read tpcds q96 at tp1-single: its
  `GpuAggregate`'s `batch_rows` lose the one zero (33 batches where there were 34).

- [ ] **Step 14: Sizes; the device half.**

```bash
wc -l peacockdb-core/src/tests/gpu_tests/aggregate*.rs peacockdb-core/src/executor/*/tests/*.rs \
  peacockdb-core/src/executor/gpu_backend/gpu_tests/*.rs peacockdb-core/src/executor/driver/tests/*.rs
```

  Expected: every file this task touched under 1000 lines (else move its new cases to a new file
  declared beside it). The device half of this task — the harness cases, the device executor
  cases, every enabled device cell against the regenerated sections, `test_node_timing` — runs in
  Task 8.

- [ ] **Step 15: Commit.**

```bash
git add peacockdb-core/src testdata/goldens
git commit -m "$(printf '%s\n' \
  "#199: an aggregate's init is a batch accumulator" "" \
  "Both backends release a zero-row batch uncalled and, at done where nothing" \
  "went out, run the init over nothing: the identity row keyless, no groups" \
  "grouped. The mock scripts the category; the harness drives inits as" \
  "accumulators; the corpus's cpu sections follow." "" \
  "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>")"
```

---

### Task 5: The cpu merge's clause goes

**Files:**
- Modify: `peacockdb-core/src/executor/cpu_backend/accumulate.rs` (`AggregateBatches`,
  `coalesce_or_nothing`'s doc, `CpuAccumulator::aggregate`)
- Modify: `peacockdb-core/src/executor/cpu_backend/tests/accumulate.rs`
- Modify: `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs` (the `bug_` pin)

**Interfaces:** none produced. Consumes Task 4: every keyless merge lane now receives a row.

- [ ] **Step 1: The tests say what is true now.** In `cpu_backend/tests/accumulate.rs`,
  `a_global_aggregate_that_received_nothing_still_owes_its_identity_row` becomes:

```rust
/// A merge that received nothing emits nothing, keyless included: a keyless init answers its
/// identity row at done, so the merge above it always meets a row, and no merge invents one.
#[test]
fn a_global_merge_that_received_nothing_emits_nothing() {
    // … the same node and accumulator …
    let emitted = drive(accumulator, Vec::new());
    assert!(emitted.is_empty(), "no merge invents a row for a lane that sent none");
}
```

  and the doc above `a_merge_that_received_nothing_emits_nothing` drops its sentence about "the
  shape that would owe a row". In `tests/gpu_tests/aggregate_cases.rs`, the `#199` pin becomes:

```rust
// No merge meets no arrival — a keyless init answers its identity row at done — and neither
// backend invents a row for one.
operator_case! {
    GpuAggregateBatches,
    fn a_global_merge_over_no_arrival_answers_nothing_on_both() {
        // … the same state, aggs and node …
        let outcome = run_both(&node, Script::Accumulate(Vec::new()));
        let slots = vec![Vec::new()];
        each_answers(&outcome, &slots, &slots);
    }
}
```

- [ ] **Step 2: Run the cpu case; it is red.** The harness case runs on the device (Task 8).

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- a_global_merge_that_received_nothing
```

  Expected: FAIL — the merge still answers one row (the NULL count).

- [ ] **Step 3: Remove the clause.** `AggregateBatches` loses `grouped` and its doc;
  `CpuAccumulator::aggregate` stops setting it. `mark_done_and_fetch`:

```rust
    /// Fold what is pending and emit the state, finalized where this node finishes the
    /// aggregate — nothing where nothing arrived. A keyless merge's lanes each carry their
    /// init's identity row at least, so nothing is never a keyless answer here.
    fn mark_done_and_fetch(mut self) -> CallResult<Vec<CpuBatch>> {
        if !self.pending.is_empty() {
            self.compact()?;
        }
        // … the rest unchanged …
```

  `coalesce_or_nothing`'s doc, as chain K's empty-sorts left it: the sentences naming a global
  aggregate's exception ("The exception is a global aggregate…" on master) become "A keyless merge always meets its lanes' identity rows, since a keyless init answers one
  at done."

- [ ] **Step 4: Run.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2
timeout 10800 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
git status --short testdata
```

  Expected: PASS; `testdata` clean (no answer depended on the clause any more). The flipped
  harness case runs in Task 8: red in its red build, which restores this task's clause, and
  green in its green build.

- [ ] **Step 5: Commit.**

```bash
git add peacockdb-core/src/executor/cpu_backend peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs
git commit -m "$(printf '%s\n' \
  "#199: the cpu merge no longer invents a row for a lane that sent none" "" \
  "A keyless init answers its identity row at done, so the clause is dead;" \
  "the bug_ pin becomes the agreement case." "" \
  "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>")"
```

---

### Task 6: A scan of nothing plans one lane with no batch (#282)

**Files:**
- Modify: `peacockdb-core/src/planner/translator/nodes.rs` (`source`)
- Create: `peacockdb-core/src/planner/tests/empty_scan.rs`; Modify: `planner/tests/mod.rs`
- Modify: `peacockdb-core/src/tests/end_to_end.rs`
- Regenerate: `testdata/goldens/pbench.sf1/*.plans.txt` (the two join queries' sections);
  `testdata/cost-registry.csv` (their plan cells)

**Interfaces:**
- Consumes Tasks 4 and 5: a lane that receives no batch makes its init's done call.
- Produces: `source()` maps a scan with no surviving row group to `partition_groups = [[]]`.
  `partition()` keeps refusing an empty survivor list for any other caller.

No reader, validator or estimator change (*Before you start*): the lane's `CpuSource` and
`GpuSource` answer `None` on their first `read_next` with no call, the validator accepts a lane
with no batch, and `largest_batch_bytes` answers `0`. The wire never meets an empty mapping: a
scan's row groups reach C++ only as the argument of `execute_scan_rowgroups`, and no call is made.

- [ ] **Step 1: The failing tests.** `planner/tests/empty_scan.rs`, declared `mod empty_scan;` in
  `planner/tests/mod.rs`:

```rust
//! #282: a scan no row group survives — every one pruned, or a file with none — maps to one
//! lane holding no batch, at every mode, rather than being refused; a scan with survivors maps
//! as before.

use crate::plan::{GpuLoadParquet, GpuNode, NodeRef, as_node_ref};
use crate::planner;
use crate::test_support::{MODES, Mode, data_dir_for};

/// The scans of `node`'s tree, in pre-order.
fn scans<'a>(node: &'a dyn GpuNode) -> Vec<&'a GpuLoadParquet> {
    let mut found = Vec::new();
    if let NodeRef::LoadParquet(load) = as_node_ref(node) {
        found.push(load);
    }
    for child in node.children() {
        found.extend(scans(child));
    }
    found
}

/// `sql` over tpch sf1, planned at `mode` as the corpus plans it.
async fn planned(sql: &str, mode: &Mode) -> Box<dyn GpuNode> {
    let ctx = crate::register_tables_for(
        crate::build_session_state(mode.target_partitions),
        &data_dir_for("tpch", "1"),
    )
    .await
    .expect("register the tables");
    let physical = ctx
        .sql(sql)
        .await
        .expect("DataFusion plans it")
        .create_physical_plan()
        .await
        .expect("a physical plan");
    let (tree, _memory) = planner::plan(&physical, mode.knobs())
        .unwrap_or_else(|why| panic!("{sql} at {}: {why}", mode.name));
    tree
}

/// nation's one row group holds `n_nationkey` 0–24, so `< 0` prunes it. One lane, whatever
/// the mode's target partitions: no survivor has rows to balance across lanes.
#[tokio::test]
async fn a_scan_no_row_group_survives_is_one_lane_with_no_batch_at_every_mode() {
    for mode in &MODES {
        let tree = planned("select count(*) from nation where n_nationkey < 0", mode).await;
        let found = scans(tree.as_ref());
        let [scan] = found.as_slice() else {
            panic!("{}: {} scans", mode.name, found.len())
        };
        assert!(scan.survivors.is_empty(), "{}: the row group is pruned", mode.name);
        assert_eq!(
            scan.partition_groups,
            vec![Vec::<Vec<u32>>::new()],
            "{}: one lane, no batch",
            mode.name
        );
    }
}

#[tokio::test]
async fn a_scan_with_survivors_maps_as_before() {
    for mode in &MODES {
        let tree = planned("select count(*) from nation where n_nationkey >= 0", mode).await;
        let found = scans(tree.as_ref());
        let [scan] = found.as_slice() else {
            panic!("{}: {} scans", mode.name, found.len())
        };
        let read: Vec<u32> = scan.partition_groups.iter().flatten().flatten().copied().collect();
        assert_eq!(read, vec![0], "{}: the one survivor, read once", mode.name);
    }
}
```

  `src/tests/end_to_end.rs`, after `an_outer_join_over_an_aggregate_of_nothing_answers_as_datafusion`:

```rust
/// #282: nation's one row group is pruned, so the scan is one lane with no batch, and the
/// keyless count above it answers its identity row: `0` at every mode, as DataFusion does.
#[tokio::test]
async fn a_count_over_a_scan_of_nothing_answers_0() {
    let sql = "select count(*) from nation where n_nationkey < 0";
    sql_answers_match_datafusion("tpch", "count-over-a-scan-of-nothing", sql, None, Coverage::ModesOnly)
        .await;
}
```

  Make `Mode` or `data_dir_for` reachable from `planner::tests` only if the compiler asks
  (`end_to_end.rs` already imports `data_dir_for` from `crate::test_support`).

- [ ] **Step 2: Run them; the first and the end-to-end case are red.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::empty_scan tests::end_to_end::a_count_over_a_scan_of_nothing
```

  Expected: `a_scan_no_row_group_survives…` and `a_count_over_a_scan_of_nothing_answers_0` FAIL
  with `no surviving row groups: what an empty scan means is the caller's decision…`;
  `a_scan_with_survivors_maps_as_before` PASSES. A refusal naming anything else (DataFusion
  answering from statistics, `a scan with no files`) is a finding: stop and report.

- [ ] **Step 3: Implement.** `nodes.rs`, `source()`, where it calls `partition(…)` (after limits'
  trim of a limited scan's survivors):

```rust
    // #282: no survivor — every row group pruned, or a file with none — is one lane that
    // holds no batch. Its source makes no call, so nothing above it meets a batch, and each
    // node answers what it owes over no input: a keyless init its identity row. `partition`
    // keeps refusing an empty list: the wire reads an empty mapping as the whole file, and
    // this one never reaches the wire, since no call is made to carry it.
    let partition_groups = if scan.groups.is_empty() {
        vec![Vec::new()]
    } else {
        partition(&scan.groups, lanes_for(t, &scan.groups, config.limit), batching_for_source(t))?
    };
```

  (`lanes_for`'s call moves inside the `else`; keep the arguments limits left it with.)
  `partition.rs` does not change.

- [ ] **Step 4: Run, then the plan goldens.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::empty_scan tests::end_to_end::a_count_over_a_scan_of_nothing
UPDATE_CANONICAL=1 timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
git diff --stat testdata/goldens
git diff testdata/goldens/pbench.sf1/tp1-single.plans.txt | head -80
```

  Expected: the three tests PASS. Only pbench's `.plans.txt` files move: `cross-empty-build`'s and
  `outer-on-true-empty`'s sections stop being `refused: … no surviving row groups …` and become
  plans whose `empty` scan reads `partition_groups=[[]]` at every mode. If either is refused
  for another reason (a join join-backend left refused), that section keeps its refusal and its
  row keeps its plan cells off, under the ticket the refusal names. No tpch or tpcds golden
  moves (no corpus filter prunes every row group, #282's ticket). `recipe-payloads.txt`:
  `git status --short testdata/goldens/recipe-payloads.txt` prints nothing, or only those two
  queries' new nodes, every existing `sha256=` line unchanged.

- [ ] **Step 5: The registry's plan cells, the rust-only tiers.** Set the two rows' plan cells to
  what their goldens now say (`enabled`, `plan_status` `ok`, where they plan). Their cpu and gpu
  cells stay off until Tasks 7 and 8.

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens --test test_cost_model
rustfmt --edition 2024 --check peacockdb-core/src/planner/tests/empty_scan.rs
wc -l peacockdb-core/src/planner/translator/nodes.rs peacockdb-core/src/tests/end_to_end.rs
```

  Expected: PASS, the registry both ways included; both files under 1000 lines.

- [ ] **Step 6: Commit.**

```bash
git add peacockdb-core/src/planner peacockdb-core/src/tests/end_to_end.rs testdata/goldens testdata/cost-registry.csv
git commit -m "$(printf '%s\n' \
  "#282: a scan no row group survives plans one lane with no batch" "" \
  "source() maps it to [[]] instead of refusing; partition() keeps its refusal." \
  "No call is made, so the keyless init answers its identity row: count 0." \
  "pbench's cross-empty-build and outer-on-true-empty now plan." "" \
  "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>")"
```

---

### Task 7: Six pbench queries, the `199` cells and the two join cells, on the cpu

**Files:**
- Create: `testdata/pbench-queries/empty-all-null-aggregates.sql`, `empty-count-aggregates.sql`,
  `empty-dispersion-aggregates.sql`, `empty-grouped-count.sql`, `empty-table-count.sql`,
  `empty-pruned-count.sql`
- Modify: `testdata/goldens/pbench.sf1/` (plans, cpu, cost, `mini.result.txt`,
  `duckdb-result.txt`), `testdata/goldens/tpcds.sf1/` (the `199` rows' sections)
- Modify: `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`

**Interfaces:** consumes Tasks 4, 5 and 6. Every gpu cell this task adds or touches stays off;
Task 8 turns on what the device passes.

- [ ] **Step 1: The SQL.** One line each, no comment, as pbench's files are:

```sql
SELECT sum(t_v) AS s, min(t_v) AS lo, max(t_v) AS hi FROM tiny WHERE t_id + t_v < 0
```
```sql
SELECT count(*) AS n, count(t_v) AS nv, sum(t_v) AS s, avg(t_v) AS a FROM tiny WHERE t_id + t_v < 0
```
```sql
SELECT stddev_samp(t_v) AS sd, var_pop(t_v) AS vp FROM tiny WHERE t_id + t_v < 0
```
```sql
SELECT t_k, count(*) AS n FROM tiny WHERE t_id + t_v < 0 GROUP BY t_k
```
```sql
SELECT count(*) AS n, sum(t_v) AS s FROM empty
```
```sql
SELECT count(*) AS n, min(t_v) AS lo FROM tiny WHERE t_v < 0
```

  The first four reach an init with a zero-row batch, and at tp4-single three lanes with none
  (*Before you start*); the last two scan nothing (#282): `empty` has no row group, and
  `t_v = 5·t_id` is never negative, so `t_v < 0` prunes `tiny`'s one. The `sum` beside each
  `count(*)` keeps DataFusion from answering it from statistics (#158).

- [ ] **Step 2: DuckDB's answers.** DuckDB 1.5.4's Python module, in a venv outside the repo
  (the system `python3` has no `duckdb`, and the script refuses any other version):

```bash
[ -x /tmp/duckdb-1.5.4/bin/python ] || { python3 -m venv /tmp/duckdb-1.5.4 && /tmp/duckdb-1.5.4/bin/pip install duckdb==1.5.4; }
timeout 1800 /tmp/duckdb-1.5.4/bin/python testdata/duckdb_result.py --dataset pbench
git diff --stat testdata/goldens/pbench.sf1/duckdb-result.txt
git diff testdata/goldens/pbench.sf1/duckdb-result.txt | grep '^[-+][^-+]' | head -60
```

  The script rewrites the dataset's whole file; read the diff. Expected: six new sections and no
  other line moved — one row `NULL, NULL, NULL`; one row `0, 0, NULL, NULL`; one row
  `NULL, NULL`; a header `t_k, n` and no rows; one row `0, NULL`; one row `0, NULL`. A
  `failed:` line stops the task. `cross-empty-build`'s and `outer-on-true-empty`'s sections
  were written by pbench (J) and do not move.

- [ ] **Step 3: The plan goldens.**

```bash
UPDATE_CANONICAL=1 timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
git diff --stat testdata/goldens
```

  Expected: the pbench `.plans.txt` files gain six sections each, nothing else moves. Read
  `tp4-single.plans.txt`: `tiny`'s `GpuLoadParquet` says `partition_groups=[[[0]],[],[],[]]` in
  the first four, with the filter above it, and `partition_groups=[[]]` in the last two, at
  every mode. Read `tp4-rowgroup.plans.txt`: the first four map `tiny` to one lane. No section is
  `refused:`; a refusal is a finding: report it.

- [ ] **Step 4: The lines and the rows, gpu off.** In the pbench section of `corpus_cases.inc`,
  alphabetically, with a comment above the first:

```rust
// The six empty-input aggregates (keyless-identity). The first four filter `tiny` with a
// predicate no row-group statistic prunes: tp1 hands its one lane a zero-row batch, tp4-single
// leaves three lanes with none, and tp4-rowgroup and tp4-sized plan `tiny` one lane. The last
// two scan nothing (#282): `empty` has no row group, and `t_v < 0` prunes `tiny`'s one.
// `empty_grouped_count` answers no rows under its columns (chain K's empty-sorts), so DuckDB
// compares it exactly.
corpus_query!(pbench, 1, empty_all_null_aggregates, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, empty_count_aggregates, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, empty_dispersion_aggregates, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled); // device: #216
corpus_query!(pbench, 1, empty_grouped_count, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, empty_pruned_count, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, empty_table_count, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
```

  Copy the argument shape of a neighbouring pbench line if duckdb-oracle or pbench spelled it
  otherwise. Rows in `cost-registry.csv`, alphabetically among pbench's: plan and cpu cells
  `enabled`, gpu `disabled`, `plan_status` `ok`; features `` / `avg` / `stddev_var` / `` / `` /
  ``; tickets `199` on the first four and `282` on the last two for now (gpu cells off on them),
  plus `216` on the dispersion row.

- [ ] **Step 5: The cpu cells: the new rows, the `199` rows, the two join rows.** List the rows:

```bash
awk -F, '$NF ~ /(^| )(199|282)( |$)/ {print NR": "$1"/"$3": "$NF}' testdata/cost-registry.csv
grep -n 'cross_empty_build\|outer_on_true_empty' peacockdb-core/tests/common/corpus_cases.inc
```

  Expected (to confirm against J's registry): tpcds q96, q88, q90, and q32 if stale-cells tagged
  it; the six new rows; `cross-empty-build` and `outer-on-true-empty` if pbench tagged them
  `282`. For each `199` row whose cpu modes are off on `199`, set its cpu modes to `all_modes`
  and reword the comment above it that names #199 (q96's at "q96's `count(*)` is declared
  non-nullable…", q90's in batch 12, q88's in batch 17): the clause about #199 goes, the rest
  stays. `cross-empty-build` and `outer-on-true-empty`: cpu modes `all_modes`, and their DuckDB
  oracle from `duckdb_none` to `duckdb_exact`. Then:

```bash
PCK_UPDATE_SECTIONS=1 timeout 5400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2 pbench_empty_ cross_empty_build outer_on_true_empty cpu_tpcds_q96 cpu_tpcds_q88 cpu_tpcds_q90 cpu_tpcds_q32
timeout 5400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2 empty_ cross_empty_build outer_on_true_empty q96 q88 q90 q32 registry
```

  Arm the monitor for both. Expected: green, the DuckDB comparisons of the eight pbench rows
  included. A cpu cell that fails is turned off with the ticket it fails on: `cross-empty-build`'s
  cpu cells may still meet #208 and `outer-on-true-empty`'s #160 (pbench's table); otherwise an
  open ticket the failure is, or a new one in `tickets/corpus-coverage.md` (at most 15 lines,
  numbered as the chain header says). A DuckDB comparison that fails on an empty answer's header
  means empty-sorts is not in the base: stop and report. Set the registry's plan and cpu cells to
  what runs.

- [ ] **Step 6: The rust-only guards.**

```bash
timeout 5400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens --test test_cost_model
git status --short testdata/goldens/recipe-payloads.txt
```

  Arm the monitor for the first. Expected: green (the registry both ways,
  `every_device_cell_has_a_cpu_cell…`); the payload golden untouched.

- [ ] **Step 7: Commit.**

```bash
git add testdata peacockdb-core/tests/common/corpus_cases.inc llm-wiki/tickets
git commit -m "$(printf '%s\n' \
  "#199 #282: six pbench empty-input aggregates; the 199 and join cells on the cpu" "" \
  "Four read tiny under an unprunable false predicate, two scan nothing." \
  "q96, q88, q90 run at the tp4 modes; cross-empty-build and outer-on-true-empty" \
  "run. Every gpu cell stays off until the device session." "" \
  "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>")"
```

---

### Task 8: The device session

The task's one sync (chain L's header): the red build first, then the green build, then every
device test and cell this task owes. All commands as in *Device session*.

**Files:**
- Modify: `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv` (gpu
  cells; `199` and `282` struck), `testdata/goldens/*/gpu-result.txt`,
  `llm-wiki/tasks/keyless-identity-detail.md`, `llm-wiki/tickets/` (a ticket a cell fails on)

**Interfaces:** consumes every earlier task, committed.

- [ ] **Step 1: The candidate cells, uncommitted.** A gpu cell compiles into `test_gpu_corpus`
  only where its line enables it, so the trial set is in the tree before the sync. Turn on, in
  `corpus_cases.inc`, every gpu cell whose cpu cell runs (Task 7) and that is off only on `199`
  or `282` or not yet declared:
  - the five new pbench rows other than `empty_dispersion_aggregates`, which stays off on #216
    (welford-device turns it on);
  - the `199` rows' gpu cells off on `199` alone (q96, q88, q90 at the tp4 modes; q32);
  - `cross_empty_build` and `outer_on_true_empty`, every gpu mode whose cpu cell runs, whatever
    pbench's table tagged them (#160, #208): the spec runs each cell and gives it the ticket it
    fails on.

  Write the list in the detail file.

- [ ] **Step 2: The fixes as a patch.** The red build is HEAD less two fixes: Task 2's device
  done call (`cpp/src`, which no later task touches) and Task 5's removal of the cpu merge's
  clause (`cpu_backend/accumulate.rs`, which no later task touches).

```bash
T2=$(git log -1 --format=%H --grep='^#199: an aggregate handed no input')
T5=$(git log -1 --format=%H --grep='^#199: the cpu merge no longer invents')
{ git diff "$T2^" "$T2" -- cpp/src; \
  git diff "$T5^" "$T5" -- peacockdb-core/src/executor/cpu_backend/accumulate.rs; } \
  > /tmp/keyless-identity-fixes.patch
patch -p1 -R --dry-run < /tmp/keyless-identity-fixes.patch
```

  Expected: both hashes found; the dry run reverses every hunk cleanly against the working tree.

- [ ] **Step 3: SYNC**, once.

- [ ] **Step 4: The red build.**

```bash
timeout 60 ssh "$GPU" "cd ~/$DIR && patch -p1 -R" < /tmp/keyless-identity-fixes.patch
```

  Then BUILD, and RUN, one after another:

```bash
cpp/install/bin/peacock_plan_tests --gtest_filter='AggregateNoInput.*'
cpp/install/rust-tests/peacockdb_core_gpu_lib --test-threads=1 over_no_input a_global_merge a_keyless_init_releases_a_zero_row_batch a_keyless_init_that_answered_rows a_keyless_aggregate_ zero_row_batches_among_others
```

  Expected, and recorded with the failure messages:
  - FAIL: `AggregateNoInput.AKeylessNodeAnswersOneRowTypedAsItsInputDeclares` and
    `…AGroupedNodeAnswersNoGroups` (`take_input: not enough input handles for node`); the five
    harness cases matching `over_no_input` (their device half, the same message);
    `a_keyless_init_releases_a_zero_row_batch_and_answers_its_identity_row_at_done` and the walk
    `a_keyless_aggregate_answers_its_identity_row_on_a_lane_that_saw_no_batch` (the same);
    `a_global_merge_over_no_arrival_answers_nothing_on_both` and the cpu's
    `a_global_merge_that_received_nothing_emits_nothing` (the cpu merge answers one row).
  - PASS: `AggregateNoInput.AnyOtherNodeHandedNothingStillRefuses`; the cpu's
    `every_keyless_init_answers_its_empty_state_over_no_input`,
    `every_keyless_shortcut_answers_count_0_and_nulls_over_no_input` and
    `a_grouped_init_over_no_input_emits_nothing`; `zero_row_batches_among_others_change_no_row_on_both`,
    `a_keyless_init_that_answered_rows_makes_no_done_call`,
    `a_keyless_aggregate_over_lanes_with_rows_makes_no_done_call` — none makes a done call.

  A case that passes where FAIL is expected proves nothing: stop and find why.

- [ ] **Step 5: The green build.**

```bash
timeout 60 ssh "$GPU" "cd ~/$DIR && patch -p1" < /tmp/keyless-identity-fixes.patch
```

  Then BUILD, and RUN, one after another (list `cpp/install/bin` first, and run every
  `peacock_*_tests` there, `peacock_join_session_tests` from chain J included, except the sf40
  binaries):

```bash
cpp/install/bin/peacock_plan_tests
cpp/install/bin/peacock_gpu_tests
cpp/install/bin/peacock_join_session_tests   # chain J's; whatever else `ls` lists, too
cpp/install/rust-tests/peacockdb_core_gpu_lib --test-threads=1
cpp/install/rust-tests/test_node_timing --test-threads=1
cpp/install/rust-tests/peacock_gpu_benchmarks --skip bench_ --test-threads=1
```

  Expected: all PASS — every Step 4 case included, the two walks, the device executor cases,
  every aggregate harness case as an accumulator. A failure: stop, as *Device session* says.

- [ ] **Step 6: The corpus.** RUN, with `timeout 7200` on the ssh, the candidates with their
  sections written, then the whole binary:

```bash
PCK_WRITE_GPU_RESULT=1 cpp/install/rust-tests/test_gpu_corpus --test-threads=1 pbench_empty_ cross_empty_build outer_on_true_empty q96 q88 q90 q32
cpp/install/rust-tests/test_gpu_corpus --test-threads=1
```

  The second run is the regression check for every cell already on: each init is an accumulator
  now. Bring the sections home:

```bash
timeout 600 rsync -a "$GPU:$DIR/testdata/goldens/" testdata/goldens/ --include='*/' --include='gpu-result.txt' --exclude='*'
git diff --stat testdata/goldens/*/gpu-result.txt
```

  Expected: new sections for the candidates that ran, no section of an older cell moved (the
  filtered write touches only the cells it ran).

- [ ] **Step 7: The verdicts.** A candidate that fails is turned off in `corpus_cases.inc`, its
  section deleted from `gpu-result.txt`, and given its ticket: an open one if the failure is it
  (#160, #208 for the join rows), else a new one in `tickets/corpus-coverage.md` (at most 15
  lines, numbered as the chain header says) naming the cell and the message. No `bug_` pin here:
  it could not run before the next sync. A cell already on that now fails is this task's
  regression: fix it, and run the session again (*Device session*). Then the registry: gpu cells
  as `corpus_cases.inc` says, and `199` and `282` struck from every row's tickets (a row left
  with a disabled cell keeps that cell's ticket).

```bash
timeout 5400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens --test test_cost_model
awk -F, '$NF ~ /(^| )(199|282)( |$)/' testdata/cost-registry.csv
grep -n '#199\|#282\|(199)\|(282)' peacockdb-core/tests/common/corpus_cases.inc
```

  Arm the monitor for the first. Expected: green (the registry both ways,
  `every_device_cell_has_a_cpu_cell…`, the gpu-result coverage guard); the awk and the grep
  print nothing.

- [ ] **Step 8: Commit.** The detail file holds every run: the red build's and the green build's
  commands, binaries, pass and fail counts, and the candidate verdicts.

```bash
git add testdata peacockdb-core/tests/common/corpus_cases.inc llm-wiki/tickets llm-wiki/tasks/keyless-identity-detail.md
git commit -m "$(printf '%s\n' \
  "#199 #282: the device cells the session passed, on" "" \
  "One sync: a red build without the device done call and with the merge's" \
  "clause, then the green build and the corpus. 199 and 282 struck." "" \
  "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>")"
```

---

### Task 9: The wiki, the tickets, the counts

**Files:**
- Modify: `llm-wiki/architecture.md`, `llm-wiki/build-test.md`, `llm-wiki/tickets.md`,
  `llm-wiki/tickets/*.md`, `llm-wiki/archive/archived-tickets.md`

- [ ] **Step 1: `architecture.md`.** Short sentences; the page describes what is true. Find each
  site by its text.
  - *The node set*: `GpuAggregate`'s row moves to `BatchAccumulator`: "init aggregators per batch
    with rows, plus the finalize where it is also the single-node shortcut; a zero-row batch is
    released uncalled; at done, where nothing went out, the same calls over no input — one row
    keyless, nothing grouped".
  - *The aggregate sequence*: one sentence after "Shortcuts: …": a keyless aggregate answers one
    row whatever arrived, because its init answers over no input at done; `empty_state`
    (`plan/aggregates.rs`) states what each aggregator owes there.
  - *The row-group mapping*: after "the planner always produces a plan": a scan with no
    survivor — every row group pruned, or a file with none — maps to one lane with no batch,
    `[[]]`, never an empty mapping, which the wire would read as the whole file.
  - "Hash skew needs one decision and no mechanism. A lane that receives nothing is never
    runnable" → a lane that receives nothing runs only its `MarkDone`, and only an aggregate's
    init answers anything there: its identity row, keyless. The rest of the paragraph stands.
  - *Zero rows is not zero bytes*: "an empty lane emits no batch at all, which is a different
    thing" → "an empty lane emits no batch at all — except a keyless init's, which emits its
    identity row, and the scatter lane under a join that owes its probe rows, which keeps its
    zero-row table". An empty grouped answer reaches the sink as nothing, and the driver answers
    it with one zero-row batch under the sink's schema (empty-sorts): say so in one sentence
    there, or where empty-sorts put that rule.
  - *Zero-row batches change no answer*: the first bullet (#199) goes.
  - *From node to seqs*, the call patterns: `GpuAggregate` leaves the generic map arm's row and
    gets its own: `CudfAggregate{Partial}`, plus a `CudfProject{finalize}` for the shortcut | per
    batch with rows, the init then the finalize; at done, if nothing went out, the same seqs, the
    first handed no input. If the page lists the `CallPattern` texts anywhere
    (`grep -n 'per straddling batch' llm-wiki/architecture.md`), add `at done, if nothing went
    out` there too.
  - *From flat buffer to cuDF call*, `CudfAggregate`'s row: `aggr_input_schema` is read when a
    call hands no input — the zero-row table the node aggregates.
  - *What the frozen surface costs*: "nothing on the surface makes a table out of nothing" gains
    its exception — an aggregate, from the input schema its node declares.
  - Check nothing else says otherwise:
    `grep -n 'never runnable\|no surviving\|Exec.*GpuAggregate\|GpuAggregate.*Exec\|#199\|#282' llm-wiki/architecture.md`.
- [ ] **Step 2: The tickets.** Archive #199 and #282, as the spec's scope says: move each block,
  anchor included, from `tickets/corpus-coverage.md` to the top of
  `archive/archived-tickets.md`'s `## Done`, each with a **Done.** paragraph of three to five
  lines (#199: the init a batch accumulator, the done call over no input, the merge's clause
  gone, the cells this task turned on; #282: `source()`'s one lane with no batch, the two
  queries and the two join rows). Remove both from `corpus-coverage.md`'s contents list and
  from `tickets.md`'s corpus-coverage row; decrement that row's count and the open total. Then
  `grep -rn '#199\|t199\|#282\|t282' llm-wiki/tickets llm-wiki/tickets.md`: every sentence
  describing either gap as open is reworded (#214's ticket names #199's fix, for one). A ticket
  filed in Task 7 or 8 is in its milestone file already.
- [ ] **Step 3: `build-test.md`.** Recount every row this task touched from the code (`--list`
  per binary) and set each block header, the Rust/C++ headers and the grand total to the sums.
  This task's deltas, against whatever J, K and grouping-id left:

| row | delta | why |
|---|--:|---|
| Aggregate state types (`plan::aggregates::tests`) | +1 | `empty_state` |
| Plan rules, hand-built | +1 | the category |
| Recipes per join type (`wire/tests`) | +1 | the init's done call |
| a new planner row, *Recipe patterns* (`planner/tests/recipe_patterns.rs`) | +1 | only inits call if nothing went out |
| a new planner row, *A scan of nothing* (`planner/tests/empty_scan.rs`) | +2 | one lane with no batch; survivors as before |
| End to end | +2 | the outer join over nothing; the count over a scan of nothing |
| Drivers over a mock backend | +6 | the init's lane cases |
| CPU backend executors | +3 | identity per decomposition; grouped nothing; the shortcut over nothing |
| Operator harness | +6 | identity, shortcut, decimal sum, decimal avg, grouped, interleaved |
| Executors on a device | +2 | the device init |
| Recipe walk on a device | +2 | the empty lane; no done call |
| Plan-executor (C++) | +3 | `AggregateNoInput.*`, in `test_aggregate_no_input.cpp` |
| Corpus, cpu | +30, the `199` and join cells turned on, and one DuckDB case per new line if duckdb-oracle counts them here | six queries at five modes |
| Corpus, device | the cells Task 8 enabled | |

  Rewrite the rows' prose where it names what changed: *Corpus, cpu* no longer lists q96, q88,
  q90 as off on #199 and names the six pbench rows and the two join rows; *Drivers over a mock
  backend* gains "the aggregate init's done call only where nothing went out"; *Operator
  harness* says the inits run as accumulators and lists the identity cases; *Executors on a
  device* moves the aggregate from "the exec nodes one batch at a time" to the accumulators;
  *Recipe walk* names the empty lane; *Plan-executor* names an aggregate handed no input and
  its file; *Aggregate state types* names `empty_state`.
- [ ] **Step 4: The local verification bar.** The device bar ran in Task 8 against the same
  code; this task changes only the wiki.

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2
timeout 10800 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens --test test_cost_model
timeout 5400 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 900 ctest --test-dir cpp/build -L cpu --output-on-failure
git diff --stat HEAD~1 -- ':!llm-wiki'
```

  Arm the monitor for the corpus run. Expected: all green, the registry tests included (no row
  names an archived ticket); the last command prints nothing.

- [ ] **Step 5: Commit.**

```bash
git add llm-wiki
git commit -m "$(printf '%s\n' \
  "#199 #282: architecture.md, the counts; both tickets archived" "" \
  "The init as a batch accumulator; a lane of nothing runs MarkDone; a scan of" \
  "nothing maps one lane with no batch. Zero-row batches loses its first break." "" \
  "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>")"
```

## Not changed, on purpose

- **The Python exec model** mirrors the init as an `Exec` (`scripts/exec_model/operators/nodes.py`,
  `ExecutorCategory.EXEC` with `exec_ops.PartialAggregateExec`). The spec's scope leaves it out;
  it is a model of the scheduling rules, checked against its own oracles, and nothing reads it
  against the Rust category. Recorded in the detail file as a divergence a later model task may
  close.
- **`scripts/calibration/nsys_calls.py`** reads a recipe line's seqs by
  `execute_\w+\(#(\d+) (\w+)`: a done call naming the init's seq again maps to the same entry, and
  a call handed nothing still matches.
- **The translator's shortcut** (`planner/translator/aggregate.rs`): unchanged; the executor makes
  it answer over nothing.
- **A ROLLUP or CUBE over nothing** answers no grand-total row, where DuckDB answers one
  ([#283](../tickets/complete-coverage.md#t283)). A grouping-set init is grouped, so over no rows
  it owes no groups on either engine, as DataFusion 45 answers; the spec gives the done call no
  keyless case, and neither does this plan.
