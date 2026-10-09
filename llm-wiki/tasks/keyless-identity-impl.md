# keyless-identity implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A keyless aggregate answers one row whatever arrived, and adding or removing zero-row
batches changes no row an aggregate init emits (#199). q96, q88 and q90 run at the tp4 modes; four
pbench queries over an empty input join the corpus.

**Architecture:** Every `GpuAggregate` becomes a `BatchAccumulator`. Its executor, on each backend,
holds one flag, `emitted`. A zero-row batch is released with no call. A batch with rows runs the
init (and the shortcut's finalize). At done, where nothing went out, the same calls run once more,
the first handed no input. The recipe states that as a new `CallPattern`,
`AtDoneIfNothingOut`, reusing the init's seqs. The cpu runs the init over
`RecordBatch::new_empty`. The device's `execute_aggregate`, handed no input, aggregates the
zero-row table its `aggr_input_schema` declares. With every keyless lane now owing a row, the cpu
merge's `!self.grouped` clause goes. `empty_state` states what each aggregator owes over no rows;
the tests hold both engines to it.

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
  over a zero-row table built from `aggr_input_schema`. Task 2.

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
- **`cpp/tests/gpu/test_plan_executor.cpp`.** K drops `CreateAggregateFuncNode`'s `distinct`
  argument, so Task 2's gtests use `AggregateFuncNodeBuilder`, as grouping-id's do.
- **`plan/mod.rs`, `NodeRef` and `category_of`.** join-backend (J) adds a `GpuEmpty` leaf, so the
  match gains an arm; the mock's `executors_for`, the walk's `node()` and the cpu backend test's
  `node_kind` gain it too. Only the `Aggregate` arm moves here.
- **`planner/translator/aggregate.rs`.** K splits `aggregate_sequence` into `Stage` and
  `sequence`. Nothing here edits it: the shortcut is fixed by the executor.
- **`executor/driver/`.** join-backend moves joins onto the session. This plan touches only the
  mock (`driver/tests/mock.rs`, where guard-checks already made `MockUnload` call `rows.clamp`) and
  adds a test file.
- **`wire/gpu_tests/mod.rs`.** join-backend rewrites the join walker; grouping-id adds
  `SUM_OF_QUOTIENTS` and a DISTINCT walk. `Walk::node`, `make`, `per_lane`, `phases`,
  `Session::execute`, `TWO_LANES`, `times`, `trail` keep their names.
- **`src/tests/gpu_tests/aggregate_cases.rs`.** grouping-id turns its four `bug_` pins into
  agreement cases and deletes `grouping_sets_as_exported`.
- **`peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`.** duckdb-oracle
  adds the ninth argument (`duckdb_oracle`) and `all_modes`; pbench adds its section (alphabetical,
  nine arguments); stale-cells and join-backend move the q96/q88/q90/q32 lines and tags (#152,
  #220 struck). **Build Task 6's row list from the registry as it stands.**
- **Goldens.** duckdb-oracle adds `gpu-result.txt` per dataset, written by the device run under
  `PCK_WRITE_GPU_RESULT=1`, with a rust-only guard that every enabled device cell has its section.
- **`llm-wiki/build-test.md`.** Every count moves with J and K. Recount each row this plan touches;
  the deltas in Task 7 are this task's alone.
- **pbench's `empty` table holds no row group** (`testdata/pbench.sf1/empty.parquet`: 0 rows, 0 row
  groups, checked 2026-10-08). A scan over it is refused at plan time:
  `planner/translator/scan_mapping/partition.rs`, "no surviving row groups" (#256, open). So the
  spec's four queries cannot read `empty` and reach an aggregate. Task 6 writes them over `tiny`
  under a predicate no row-group statistic can prune, `t_id + t_v < 0` (never true: `tiny` holds
  `t_id` 0–7 and `t_v = 5·t_id`). The input is still empty, tp1 still hands its one lane a
  zero-row batch, tp4 still leaves three lanes with none, and DuckDB's answers are the spec's. The
  names stay the spec's: welford-device names `empty-dispersion-aggregates`.

## Global constraints

- No facade, trait, ABI or wire change. The done call is an existing seq handed no handles.
  `recipe-payloads.txt` keeps every `sha256=` line; only its recipe lines change (Task 3).
- No production behaviour change outside the init, its recipe and the cpu merge's clause. The
  device merge, the limit (#214) and the cpu's sort and merge over zero-row batches (#205) do not
  change. The keyless Welford on the device stays refused (#216).
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
- Device builds and runs happen on the GPU host chain L's header names, as *Device cycle* below.
  Record every run (command, binaries, pass and fail counts) in
  `llm-wiki/tasks/keyless-identity-detail.md`. Out of scope, as the header says: the sf40
  binaries, `--run-benchmarks`, Nsight.
- Every foreground command carries a `timeout`. Large runs (the corpus regeneration) get a monitor
  reporting progress every 2 minutes and matching `panicked|FAILED|error\[`.
- Formatting on changed lines only: `git clang-format HEAD -- <files>` for C++;
  `rustfmt --edition 2024 --check <file>` on each touched leaf, applied only where its diff stays
  inside lines this task wrote. Never a `mod.rs` (rustfmt follows its `mod` lines).
- Files stay under 1000 lines. `wire/tests.rs` (980) and `planner/tests/plan_goldens.rs` (970)
  are near it, so the new cases there go into new files (Task 3).
- No commit leaves a test red. Commit messages at most 10 lines, ending with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Build in a workspace; never share a
  cargo target dir across worktrees.

## Device cycle

Every device step is these commands, in the foreground with their timeouts. A backgrounded chain
dies mid-build with no error. If ssh is refused at once, the sandbox is blocking it, not the host.

```bash
GPU=dmitry@89.169.109.150   # nebius-gpu, as chain L's header names it
DIR=peacockdb-L              # the header's directory there; chain J's pattern gives this name
# 1. sync the working tree, uncommitted, from the workspace root
timeout 1200 rsync -a --delete-after --exclude=.git --filter=':- .gitignore' ./ "$GPU:$DIR/"
# 2. build there
timeout 7200 ssh "$GPU" "cd ~/$DIR && . ~/peacock-env.sh && ./scripts/build-test-shadgpu.sh --build"
# 3. run what the step names, in place of RUN
timeout 3600 ssh "$GPU" "cd ~/$DIR && . ~/peacock-env.sh && \
  export LD_LIBRARY_PATH=\$PWD/cpp/install/lib:\$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib \
         PEACOCK_TESTDATA_DIR=\$PWD/testdata && RUN"
```

A fresh `~/$DIR` has no sf1 parquet: copy it from chain J's `~/peacockdb-J/testdata`, or generate
it with `testdata/generate_testdata.sh --bench tpch` and `--bench tpcds`. pbench's data is
committed. Every Rust binary takes `--test-threads=1`.

## Review focus

1. **A keyless lane that receives only zero-row batches** — tp1 over a filter that keeps nothing,
   the most common way in. Expected: no init call for either batch, one done call, the identity
   row; and zero-row batches interleaved among batches with rows change no row. Task 4:
   `a_keyless_lane_of_zero_row_batches_makes_no_init_call_and_one_done_call`,
   `zero_row_batches_among_others_change_no_row` (mock); the harness identity case over two
   zero-row batches; Task 6's pbench rows at tp1.
2. **A grouped init over nothing, under a shuffle.** Expected: it emits nothing — not a zero-row
   batch — so the per-lane merge, the scatter and the final merge above it meet a lane that sent
   nothing, and answer no rows. Task 4: the two grouped mock cases and
   `a_grouped_init_over_no_input_answers_nothing_on_both`; Task 6: `empty-grouped-count` at tp4.
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

## File structure

| file | responsibility |
|---|---|
| `peacockdb-core/src/plan/aggregates.rs`, `plan/mod.rs` | `empty_state`; `category_of` |
| `peacockdb-core/src/plan/aggregates/tests.rs`, `plan/tests/mod.rs` | their cases |
| `cpp/src/peacock/operators.h`, `cpp/src/operators/dispatch.cpp` | `was_handed_nothing` |
| `cpp/src/operators/aggregate.cpp` | no input → the zero-row table of `aggr_input_schema` |
| `cpp/tests/gpu/test_plan_executor.cpp` | `AggregateNoInput.*` |
| `peacockdb-core/src/wire/mod.rs` | `CallPattern::AtDoneIfNothingOut`; `Recipe::calls_under` |
| `peacockdb-core/src/wire/attach.rs` | the init's done calls |
| `peacockdb-core/src/wire/recipes.rs` | a call handed nothing renders without a trailing comma; a seq's payload once |
| `peacockdb-core/src/wire/tests.rs`, `wire/tests/aggregate_init.rs` (new) | the init's recipe |
| `peacockdb-core/src/planner/tests/mod.rs`, `planner/tests/recipe_patterns.rs` (new) | only an init calls at done if nothing went out |
| `peacockdb-core/src/wire/gpu_tests/mod.rs` | the walk drives an init by node kind; two walks |
| `peacockdb-core/src/executor/cpu_backend/{accumulate,backend,mod}.rs` | `State::Init`; the merge's clause goes |
| `peacockdb-core/src/executor/gpu_backend/{accumulate,backend,mod}.rs` | `State::Init`; `GpuExec::exec_over_nothing` |
| `peacockdb-core/src/executor/driver/tests/{mock,plans,mod}.rs`, `driver/tests/init.rs` (new) | the mock's init rule; the lane cases |
| `peacockdb-core/src/executor/cpu_backend/tests/{state_types,accumulate}.rs` | identity per decomposition; the merge's case |
| `peacockdb-core/src/executor/gpu_backend/gpu_tests/{mod,accumulate}.rs` | `Session::init`; the device init cases |
| `peacockdb-core/src/tests/gpu_tests/{aggregate_cases,aggregate_dimension_cases,aggregate_schema_cases}.rs` | scripts as accumulators; identity cases; the `bug_` flip |
| `peacockdb-core/src/tests/end_to_end.rs` | the outer join over an aggregate of nothing |
| `testdata/goldens/*/*.plans.txt`, `recipe-payloads.txt`, `*-mini.{cpu,cost}.txt`, `mini.result.txt`, `pbench.sf1/{duckdb,gpu}-result.txt` | regenerated |
| `testdata/pbench-queries/empty-*.sql` (new), `corpus_cases.inc`, `testdata/cost-registry.csv` | the four queries; the `199` rows |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets/` | as the spec's item 7; counts; #199's mentions |

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
- Test: `cpp/tests/gpu/test_plan_executor.cpp` (after the `AggregateMerge` cases)

**Interfaces:**
- Produces: a `CudfAggregate` seq executed with zero child handles answers one row (keyless) or
  no groups (grouped) over the zero-row table of its `aggr_input_schema`. Every other node handed
  nothing still throws `take_input: not enough input handles`. Consumed by Tasks 3 and 4.

No ABI change: `peacock_executor_execute_node` already accepts `n_children = 0`, and
`NodeSession::execute_node`'s map arm then runs one partition with an empty input vector.

- [ ] **Step 1: The failing gtests**, after `AggregateMerge.AOneColumnAggregateMergesByItsOwnRule`:

```cpp
// --- An aggregate handed no input ---------------------------------------------
//
// A lane that sent no row out makes its init's done call with no handle, and the node
// aggregates the zero-row table its aggr_input_schema declares: one row for a keyless node,
// typed as a batch would have been, and no groups for a grouped one. The plan holds a scan
// under the aggregate, as every plan the writer makes does; the scan is never called.

/// The declared input: an Int32 key, an Int64 value, a Decimal128(15, 2), a date and a
/// string. None of it is read from a file.
static flatbuffers::Offset<fb::Schema> declared_input(flatbuffers::FlatBufferBuilder& fbb) {
  std::vector<flatbuffers::Offset<fb::Field>> fields;
  fields.push_back(fb::CreateField(fbb, fbb.CreateString("k"), fb::DataType_Int32, true));
  fields.push_back(fb::CreateField(fbb, fbb.CreateString("v"), fb::DataType_Int64, true));
  fields.push_back(fb::CreateField(fbb, fbb.CreateString("amount"), fb::DataType_Decimal128,
                                   true, /*decimal_precision=*/15, /*decimal_scale=*/2));
  fields.push_back(fb::CreateField(fbb, fbb.CreateString("day"), fb::DataType_Date32, true));
  fields.push_back(fb::CreateField(fbb, fbb.CreateString("name"), fb::DataType_Utf8, true));
  return fb::CreateSchema(fbb, fbb.CreateVector(fields));
}

/// `func(column)` under `alias`; `column < 0` is `count(*)`, which names no argument.
static flatbuffers::Offset<fb::AggregateFuncNode> func_over(
    flatbuffers::FlatBufferBuilder& fbb, const char* func, int column, const char* column_name,
    const char* alias) {
  auto name = fbb.CreateString(func);
  auto out = fbb.CreateString(alias);
  flatbuffers::Offset<flatbuffers::Vector<flatbuffers::Offset<fb::Expr>>> args;
  if (column >= 0)
    args = fbb.CreateVector(std::vector<flatbuffers::Offset<fb::Expr>>{
        make_col_ref(fbb, static_cast<uint32_t>(column), column_name)});
  fb::AggregateFuncNodeBuilder node(fbb);
  node.add_name(name);
  if (column >= 0) node.add_args(args);
  node.add_alias(out);
  return node.Finish();
}

/// count(*), sum(v), sum(amount), min(day) and max(v) over `declared_input`, keyed on `k`
/// where `grouped`, as the init a done call addresses: Partial mode, seq 1 above the scan.
static std::vector<uint8_t> unfed_aggregate(flatbuffers::FlatBufferBuilder& fbb, bool grouped) {
  auto scan = nation_scan_node(fbb);
  std::vector<flatbuffers::Offset<fb::AggregateFuncNode>> funcs{
      func_over(fbb, "count", -1, nullptr, "n"),
      func_over(fbb, "sum", 1, "v", "sum(v)"),
      func_over(fbb, "sum", 2, "amount", "sum(amount)"),
      func_over(fbb, "min", 3, "day", "min(day)"),
      func_over(fbb, "max", 1, "v", "max(v)"),
  };
  auto funcs_vec = fbb.CreateVector(funcs);
  auto input = declared_input(fbb);
  flatbuffers::Offset<flatbuffers::Vector<flatbuffers::Offset<fb::Expr>>> groups;
  flatbuffers::Offset<flatbuffers::Vector<flatbuffers::Offset<flatbuffers::String>>> names;
  if (grouped) {
    groups = fbb.CreateVector(
        std::vector<flatbuffers::Offset<fb::Expr>>{make_col_ref(fbb, 0, "k")});
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
  return finish_plan(fbb, make_plan_node(fbb, fb::PlanNodeKind_CudfAggregate, agg.Union()));
}

/// Seq `seq` called with no handle at all, as an init's done call makes it.
static uint64_t call_with_nothing(peacock::NodeSession& session, uint64_t seq) {
  uint64_t out = 0;
  size_t produced = 0;
  peacock::NodeStats stats{};
  session.execute_node(seq, nullptr, nullptr, 0, &out, 1, &produced, &stats);
  EXPECT_EQ(produced, 1u);
  return out;
}

TEST(AggregateNoInput, AKeylessNodeAnswersOneRowTypedAsItsInputDeclares) {
  flatbuffers::FlatBufferBuilder fbb;
  auto buf = unfed_aggregate(fbb, /*grouped=*/false);
  peacock::NodeSession session(buf.data(), buf.size());
  const auto& result = session.table_for(call_with_nothing(session, /*seq=*/1));
  auto view = result.table->view();
  ASSERT_EQ(view.num_rows(), 1);
  ASSERT_EQ(view.num_columns(), 5);
  EXPECT_EQ(get_scalar_value<int64_t>(view.column(0), 0), 0) << "count(*) over nothing";
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
  EXPECT_EQ(result.table->view().num_rows(), 0);
  EXPECT_EQ(result.column_names.front(), "k") << "the key leads, as over any input";
}

TEST(AggregateNoInput, AnyOtherNodeHandedNothingStillRefuses) {
  flatbuffers::FlatBufferBuilder fbb;
  auto scan = nation_scan_node(fbb);
  auto predicate = make_binary_expr(fbb, make_col_ref(fbb, 0, "n_nationkey"), fb::BinaryOp_Gt,
                                    make_int64_literal(fbb, 0));
  auto filter = fb::CreateCudfFilter(fbb, predicate, scan);
  auto buf = finish_plan(fbb, make_plan_node(fbb, fb::PlanNodeKind_CudfFilter, filter.Union()));
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

  If refcounted-scatter has landed, read `result.view()` where this writes
  `result.table->view()`.

- [ ] **Step 2: Build locally; run the three on the device; the first two FAIL.**

```bash
timeout 5400 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
```

  Then the *Device cycle* with `RUN` =
  `cpp/install/bin/peacock_plan_tests --gtest_filter='AggregateNoInput.*'`.
  Expected: the keyless and grouped cases FAIL with `take_input: not enough input handles for
  node`; `AnyOtherNodeHandedNothingStillRefuses` PASSES.

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
  return {std::make_unique<cudf::table>(std::move(columns)), std::move(names)};
}
```

  (Construct the result as the other returns in the file then do — `TableResult::owning(…)` after
  refcounted-scatter.) And `execute_aggregate`'s first line becomes:

```cpp
  auto input = was_handed_nothing(in) ? zero_rows_of(agg) : take_input(in);
```

  `flatbuffers/gpu_plan.fbs` does not change: `aggr_input_schema`'s comment already says what it
  is, the input's schema.

- [ ] **Step 4: Build, the cpu ctest, and the device run.**

```bash
timeout 5400 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 900 ctest --test-dir cpp/build -L cpu --output-on-failure
git clang-format HEAD -- cpp/src/peacock/operators.h cpp/src/operators/dispatch.cpp cpp/src/operators/aggregate.cpp cpp/tests/gpu/test_plan_executor.cpp
```

  Then the *Device cycle*, `RUN` =
  `cpp/install/bin/peacock_plan_tests --gtest_filter='AggregateNoInput.*:AggregateMerge.*:PlanExecutor.Aggregate*'`.
  Expected: all PASS; record the run.

- [ ] **Step 5: Commit.**

```bash
git add cpp/src/peacock/operators.h cpp/src/operators/dispatch.cpp cpp/src/operators/aggregate.cpp cpp/tests/gpu/test_plan_executor.cpp
git commit -m "$(printf '%s\n' \
  "#199: an aggregate handed no input aggregates its declared input at zero rows" "" \
  "Keyless answers one row, grouped no groups; any other node still refuses." \
  "No ABI change: a call with no child already reaches execute_one." "" \
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
- Modify: `peacockdb-core/src/wire/gpu_tests/mod.rs` (the walk)
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

- [ ] **Step 8: The walk drives an init by its node kind.** `wire/gpu_tests/mod.rs`:
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
        // An aggregate's init, whichever category drives it: the per-batch calls for every
        // batch, and the done calls only for a lane where none of them answered a row.
        if let NodeRef::Aggregate(_) = as_node_ref(node) {
            return self.aggregate_init(node, recipe, &kids[0]);
        }
```

  - The method, beside `per_lane`:

```rust
    /// An aggregate's init: its per-batch chain per batch, each output going straight out,
    /// and its done chain — the first call handed nothing — for a lane where no output had a
    /// row. The executor's flag, kept per lane here since no executor runs.
    fn aggregate_init(&mut self, node: &dyn GpuNode, recipe: &Recipe, input: &Lanes) -> Lanes {
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
                out.push(only(self.chain(&at_done, &mut at), node.name()));
            }
            lanes.push(out);
        }
        lanes
    }
```

  - `phases`' doc gains: "`AtDoneIfNothingOut` is on the done side, which the init's walk runs
    only where nothing went out." (`partition` already puts it there.)
  - `assert_walk_matches_datafusion_with` returns `Walked`; `assert_walk_matches_datafusion`
    returns its `.calls`; `held` ignores it.
  - The two walks, beside the aggregate walks, and both added to
    `the_kinds_a_device_has_run_are_the_kinds_this_file_claims`' list:

```rust
/// nation is one row group, so at two lanes the second receives no batch.
const KEYLESS_OVER_AN_EMPTY_LANE: &str = "SELECT count(*) AS n, sum(n_nationkey) AS s, \
     min(n_regionkey) AS lo FROM nation WHERE n_regionkey >= 0";
const KEYLESS_SUM: &str = "SELECT sum(l_quantity) AS s FROM lineitem";

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
  statistics (#158).

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

- [ ] **Step 10: The cudf build and the device run.**

```bash
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 5400 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
```

  Then the *Device cycle*, `RUN` =
  `cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests:: --test-threads=1`.
  Expected: all PASS, including the two new walks; record the run.

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
  `init_declaring` takes `keyed: bool` after `input`: the key field, `group_by` and
  `group_keys = vec![0]` only where keyed, the Welford positions `[1, 2, 3]` keyed and `[0, 1, 2]`
  keyless. `init_of` passes `true`; add:

```rust
/// [`init_of`] with no group key: the init that owes a row whatever arrived.
fn keyless_init_of(func: AggFunc, ordinal: u32, name: &str, input: &DataType) -> GpuAggregate {
    init_declaring(func, ordinal, name, input, false, |state_type| state_type).0
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
```

  Import `ScalarValue` and `RecordBatch` if `super::*` does not bring them.

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

/// `empty_state`'s row for `node`'s state, its argument typed `input`.
fn empty_state_row(node: &GpuAggregate, input: &DataType) -> RecordBatch {
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
/// batch, and `row` at done.
fn at_done(zero_row_batches: usize, row: RecordBatch) -> Vec<Vec<RecordBatch>> {
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
// NULL sum over a zero count included, which divides to NULL rather than failing.
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
            let outcome = run_both(&node, Script::Accumulate(Vec::new()));
            let slots = at_done(0, row);
            each_answers(&outcome, &slots, &slots);
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
  shape for the device and for the next refactor. The harness cases (Step 4) are device cases:
  their red is `the script's shape is not the node's category`, seen on the first device run if
  the category step is held back, and not worth a cycle of its own.

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
    proven by the case with rows and by grouping-id's gtests.

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

- [ ] **Step 14: The device cycle.** `RUN` =
  `cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests:: --test-threads=1`, then
  `cpp/install/rust-tests/test_gpu_corpus --test-threads=1`, then
  `cpp/install/rust-tests/test_node_timing --test-threads=1`. Expected: all PASS — every enabled
  device cell reads the regenerated sections and agrees. Record the run.

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

- [ ] **Step 2: Run; both are red on the cpu.**

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

  `coalesce_or_nothing`'s doc: its last two sentences ("The exception is a global aggregate…")
  become "A keyless merge always meets its lanes' identity rows, since a keyless init answers one
  at done."

- [ ] **Step 4: Run.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2
timeout 10800 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
git status --short testdata
```

  Expected: PASS; `testdata` clean (no answer depended on the clause any more). Device: the
  *Device cycle* with `RUN` = `cpp/install/rust-tests/peacockdb_core_gpu_lib
  tests::gpu_tests::aggregate --test-threads=1`, the flipped case green.

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

### Task 6: Four pbench queries, and the `199` cells

**Files:**
- Create: `testdata/pbench-queries/empty-all-null-aggregates.sql`,
  `empty-count-aggregates.sql`, `empty-dispersion-aggregates.sql`, `empty-grouped-count.sql`
- Modify: `testdata/goldens/pbench.sf1/` (plans, cpu, cost, `mini.result.txt`,
  `duckdb-result.txt`, `gpu-result.txt`), `testdata/goldens/tpcds.sf1/` (the `199` rows' sections)
- Modify: `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`

**Interfaces:** consumes Tasks 4 and 5.

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

  (*Before you start*: `empty.parquet` holds no row group, and a scan over it is refused on #256.)

- [ ] **Step 2: DuckDB's answers.**

```bash
timeout 900 python3 testdata/duckdb_result.py --dataset pbench
git diff --stat testdata/goldens/pbench.sf1/duckdb-result.txt
```

  Expected: four new sections and no other line moved — one row `NULL, NULL, NULL`; one row
  `0, 0, NULL, NULL`; one row `NULL, NULL`; no rows. A `failed:` line stops the task.

- [ ] **Step 3: The plan goldens.**

```bash
UPDATE_CANONICAL=1 timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
git diff --stat testdata/goldens
```

  Expected: the pbench `.plans.txt` files gain four sections each, nothing else moves. Read
  `tp4-single.plans.txt`: `tiny`'s `GpuLoadParquet` says `partition_groups=[[[0]],[],[],[]]`, the
  filter survives above it, and no section is `refused:`. A refusal is a finding: report it.

- [ ] **Step 4: The lines and the rows, gpu off for now.** In the pbench section of
  `corpus_cases.inc`, alphabetically, with a comment above the first:

```rust
// The four empty-input aggregates (keyless-identity): `tiny` under a predicate no row-group
// statistic prunes, since a scan whose row groups all prune is refused (#256). tp1 hands the
// one lane a zero-row batch; tp4 leaves three lanes with none.
corpus_query!(pbench, 1, empty_all_null_aggregates, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, empty_count_aggregates, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, empty_dispersion_aggregates, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled); // device: #216
corpus_query!(pbench, 1, empty_grouped_count, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
```

  Rows in `cost-registry.csv`, alphabetically among pbench's: plan and cpu cells `enabled`, gpu
  `disabled`, `plan_status` `ok`, features `` / `avg` / `stddev_var` / ``, tickets `199` on all
  four for now (`216` on the dispersion row too).

- [ ] **Step 5: The cpu sections, and the `199` rows' cpu cells.** List the rows:

```bash
awk -F, '$NF ~ /(^| )199( |$)/ {print NR": "$1"/"$3}' testdata/cost-registry.csv
```

  Expected (to confirm against J's registry): tpcds q96, q88, q90, and q32 if stale-cells tagged
  it. For each whose cpu modes are off on `199`, set its `corpus_query!` cpu modes to
  `all_modes` and reword the comment above it that names #199 (q96's at "q96's `count(*)` is
  declared non-nullable…", q90's in batch 12, q88's in batch 17): the clause about #199 goes, the
  rest stays. Then:

```bash
PCK_UPDATE_SECTIONS=1 timeout 5400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2 empty_ cpu_tpcds_q96 cpu_tpcds_q88 cpu_tpcds_q90 cpu_tpcds_q32
timeout 5400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2 empty_ q96 q88 q90 q32 registry
```

  Expected: all green. A cpu cell that fails is turned off with the ticket it fails on (a new one
  where none fits). Set the registry's cpu cells to what runs.

- [ ] **Step 6: The device cells.** Turn every gpu cell of the four new rows and every gpu cell
  the `199` rows keep off on `199` alone to on in `corpus_cases.inc` (the dispersion row stays
  off on #216, untouched). Then the *Device cycle* with `RUN` =

```bash
PCK_WRITE_GPU_RESULT=1 cpp/install/rust-tests/test_gpu_corpus --test-threads=1 empty_ q96 q88 q90 q32
```

  and bring the record home:

```bash
timeout 600 rsync -a "$GPU:$DIR/testdata/goldens/" testdata/goldens/ --include='*/' --include='gpu-result.txt' --exclude='*'
git diff --stat testdata/goldens/*/gpu-result.txt
```

  A cell that fails is turned off with its ticket: one already open if the failure is it, a new
  one in `tickets/corpus-coverage.md` otherwise (at most 15 lines), with a `bug_` case where a
  hand-built node can show it. One to expect: the device's keyless `sum` reduces in its input's
  type (`aggregate.cpp`'s keyless arm, `out_t = values_col.type()`), so `sum(t_v)` over `Int32`
  may come back `INT32` where the plan declares `Int64`. If it does, file it, pin it with
  `bug_a_keyless_sum_over_an_int32_answers_an_int32_on_the_device` in `aggregate_cases.rs`, and
  leave those cells off on it. Re-run with `PCK_WRITE_GPU_RESULT=1` after the final enabling so
  `gpu-result.txt` holds exactly the enabled cells.

- [ ] **Step 7: Strike `199`; the rust-only guards.** Remove `199` from every row's tickets
  column (a row left with a disabled cell keeps that cell's ticket). Then:

```bash
timeout 5400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens --test test_cost_model
grep -n '199' peacockdb-core/tests/common/corpus_cases.inc testdata/cost-registry.csv
```

  Expected: green (the registry both ways, `every_device_cell_has_a_cpu_cell…`, the
  gpu-result coverage guard); the grep prints nothing.

- [ ] **Step 8: Commit.**

```bash
git add testdata peacockdb-core/tests/common/corpus_cases.inc llm-wiki/tickets
git commit -m "$(printf '%s\n' \
  "#199: four pbench empty-input aggregates; q96, q88 and q90 at the tp4 modes" "" \
  "The queries read tiny under an unprunable false predicate: empty.parquet" \
  "holds no row group and is refused on #256. 199 struck from every row." "" \
  "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>")"
```

---

### Task 7: The wiki, the counts

**Files:**
- Modify: `llm-wiki/architecture.md`, `llm-wiki/build-test.md`, `llm-wiki/tickets/*.md`

- [ ] **Step 1: `architecture.md`.** Short sentences; the page describes what is true.
  - *The node set*: `GpuAggregate`'s row moves to `BatchAccumulator`: "init aggregators per batch
    with rows, plus the finalize where it is also the single-node shortcut; a zero-row batch is
    released uncalled; at done, where nothing went out, the same calls over no input — one row
    keyless, nothing grouped".
  - *The aggregate sequence*: one sentence after "Shortcuts: …": a keyless aggregate answers one
    row whatever arrived, because its init answers over no input at done; `empty_state`
    (`plan/aggregates.rs`) states what each aggregator owes there.
  - *Zero-row batches change no answer*: the first bullet (#199) goes.
  - *From node to seqs*: `GpuAggregate` leaves the generic map arm's row and gets its own:
    `CudfAggregate{Partial}`, plus a `CudfProject{finalize}` for the shortcut | per batch with
    rows, the init then the finalize; at done, if nothing went out, the same seqs, the first
    handed no input.
  - *From flat buffer to cuDF call*, `CudfAggregate`'s row: `aggr_input_schema` is read when a
    call hands no input — the zero-row table the node aggregates.
  - *What the frozen surface costs*: "nothing on the surface makes a table out of nothing" gains
    its exception — an aggregate, from the input schema its node declares.
- [ ] **Step 2: Tickets.** `grep -n '#199\|t199' llm-wiki/tickets/*.md`: every sentence describing
  the gap as open is reworded (a fixed gap is documentation, fixed in this commit). #199 itself
  is archived at merge by the helper, not here. A ticket filed in Task 6 is in its milestone file
  already.
- [ ] **Step 3: `build-test.md`.** Recount every row this task touched from the code (`--list`
  per binary) and set each block header, the Rust/C++ headers and the grand total to the sums.
  This task's deltas, against whatever J and K left:

| row | delta | why |
|---|--:|---|
| Aggregate state types (`plan::aggregates::tests`) | +1 | `empty_state` |
| Plan rules, hand-built | +1 | the category |
| Recipes per join type (`wire/tests`) | +1 | the init's done call |
| a new planner row, *Recipe patterns* (`planner/tests/recipe_patterns.rs`) | +1 | only inits call if nothing went out |
| End to end | +1 | the outer join over nothing |
| Drivers over a mock backend | +6 | the init's lane cases |
| CPU backend executors | +2 | identity per decomposition; grouped nothing |
| Operator harness | +6 | identity, shortcut, decimal sum, decimal avg, grouped, interleaved |
| Executors on a device | +2 | the device init |
| Recipe walk on a device | +2 | the empty lane; no done call |
| Plan-executor (C++) | +3 | `AggregateNoInput.*` |
| Corpus, cpu | +20, the `199` cells turned on, and one DuckDB case per new line if duckdb-oracle counts them here | four queries at five modes |
| Corpus, device | the cells Task 6 enabled | |

  Rewrite the rows' prose where it names what changed: *Corpus, cpu* no longer lists q96, q88, q90
  as off on #199 and names the four pbench rows; *Drivers over a mock backend* gains "the
  aggregate init's done call only where nothing went out"; *Operator harness* says the inits run
  as accumulators and lists the identity cases; *Executors on a device* moves the aggregate from
  "the exec nodes one batch at a time" to the accumulators; *Recipe walk* names the empty lane;
  *Plan-executor* names an aggregate handed no input; *Aggregate state types* names
  `empty_state`.
- [ ] **Step 4: The full verification bar.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2
timeout 10800 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens --test test_cost_model
timeout 5400 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 900 ctest --test-dir cpp/build -L cpu --output-on-failure
```

  Then the *Device cycle* with, one after another: `cpp/install/bin/peacock_gpu_tests`,
  `cpp/install/bin/peacock_plan_tests`, `cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests::
  --test-threads=1`, `cpp/install/rust-tests/test_gpu_corpus --test-threads=1`,
  `cpp/install/rust-tests/test_node_timing --test-threads=1`,
  `cpp/install/rust-tests/peacock_gpu_benchmarks --skip bench_ --test-threads=1`. Expected: all
  green; the run recorded in the detail file.

- [ ] **Step 5: Commit.**

```bash
git add llm-wiki
git commit -m "$(printf '%s\n' \
  "#199: the init as a batch accumulator in architecture.md; counts" "" \
  "Zero-row batches change no answer loses its first break." "" \
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
