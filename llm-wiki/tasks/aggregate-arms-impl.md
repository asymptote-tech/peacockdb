# aggregate-arms implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The device aggregate reads every input through the `ColumnRef` its `args` carry and
names every column from the wire's `state_names` (#225); one request builder serves the grouped
and grouping-set paths, so a Welford under a grouping set is the triple the plan declares (#280);
the Final, Single and `avg` arms go, and a `count` at `Merge` is refused; every C++ `ColumnRef`
reader checks the reference's bounds and name, and every `TableResult` is built through a checked
constructor (#164); a union branch named otherwise than its union is projected; the scan's declared
names are checked against the file at plan time; `tpch/rollup-stddev` joins the corpus.

**Architecture:** Two fields appended to `AggregateFuncNode` (`state_names`, `ddof`), filled by
`state_funcs` and written by the recipe writer; two deprecated (`alias`, and
`CudfAggregate.mergeable_agg_state`). In `aggregate.cpp`, `agg_phase` maps `Partial` and `Merge`
and refuses the rest; one `add_requests` turns a function node into cuDF requests plus the state
columns they fill, built once per node, and `grouped_aggregate` / `grouping_set_aggregate` run them
(every grouping set reuses the one set of requests, so a computed argument is evaluated once); the
keyless path keeps `cudf::reduce`. One helper pair, `column_ordinal` / `column_at`
(`cpp/src/column_refs.cpp`), resolves a `ColumnRef` everywhere in `cpp/src`, threaded through
`evaluate_column`, `build_column` and `cudf_ast_can_evaluate`. `cast_branch` projects a union
branch whose names differ from the union's. `survivor_metadata` refuses a scan whose declared field
differs from the file's at a projected ordinal.

**Tech stack:** C++20 against cuDF 25.02 (`cpp/build`, local; nebius-gpu for the device), gtest;
FlatBuffers schema; Rust (`--features rust-only` for the cpu tier, `scripts/cargo-cudf.sh` for the
FFI rung and the device build).

**Spec:** [`aggregate-arms.md`](aggregate-arms.md). Format models:
[`guard-checks-impl.md`](guard-checks-impl.md), [`distinct-companions-impl.md`](distinct-companions-impl.md).
Consumed by [`welford-device.md`](welford-device.md): it builds on the interfaces Task 6 states
and Task 5's `column_at`.

## Before you start: what will have moved

This plan was written against master at 8806a3c3 and revised at 0753f8c9, against the spec as
amended on 2026-10-08. Chain L's base is master after chains J and K merge, and after chain L's
grouping-id and keyless-identity. Every line number below is master's; **re-locate by symbol**,
never by line. What moves, by the plan that moves it:

- **refcounted-scatter (J).** `TableResult` (`cpp/src/plan_executor.h`) is
  `{owners, columns, column_names}` with `view()`, `num_rows()`, `num_columns()` and four
  constructors in `cpp/src/table_result.cpp`: `owning(table, names)`, `slice`, `select`, `with`.
  `owning` already refuses a name count that differs from the column count, and a table of no
  columns. Every `X.table->view()` is `X.view()`. Task 1 builds on that shape; it does not redo it.
- **exit-copies (J).** `expr.h` declares `EvaluatedColumn evaluate_column(expr, table)`, which
  borrows a bare `ColumnRef`'s column and owns everything else, and `build_column(expr, table)` is
  `evaluate_column(...).take()`. The `ColumnRef` reader with a bounds check is now
  `evaluate_column`'s first arm. `project.cpp`'s bare-`ColumnRef` arm shares `input.owners.at(idx)`;
  `window.cpp` starts from `TableResult out = input`; `sort.cpp` has no `ColumnRef` special case
  (`evaluate_column` borrows it); `aggregate.cpp`'s `computed_args` is a
  `std::vector<EvaluatedColumn>` and it releases the groupby's key columns rather than copying them.
- **join-session-cpp and join-backend (J).** `cpp/src/operators/join.cpp` is deleted; joins run in
  `cpp/src/operators/join_session.cpp` (`CudfJoin`). Its key reads are where it fills
  `bkeys`/`pkeys` from `CudfJoin.keys`; its residual is evaluated per side with `build_column` /
  `cudf_ast_can_evaluate` over tables in filter-schema order. `JoinFilterColMap` is a span
  (`col_map_of`). Its gtests are `cpp/tests/gpu/test_join_session.cpp`, binary
  `peacock_join_session_tests`; their harness builds filter `ColumnRef`s with no name.
- **repartition-keys (J).** The repartition arm in `node_session.cpp` still reads `hash_exprs()` as
  `ColumnRef` ordinals; the grouping id is out of the shuffle. Re-locate it by `hash_exprs`.
- **duckdb-oracle (J).** Every `corpus_query!` line has a `duckdb_oracle` argument before the cpu
  oracle (`duckdb_exact | duckdb_approx | …`) and may use `all_modes`. The device corpus writes
  `testdata/goldens/<dataset>.sf1/gpu-result.txt` under `PCK_WRITE_GPU_RESULT=1`, and a rust-only
  guard holds every enabled device cell to a section there. `duckdb_result.py` rewrites a whole
  dataset; read its diff.
- **distinct-companions (K).** `AggregateFuncNode.distinct` is `(deprecated)`; the distinct guard
  in `aggregate.cpp` and the `/*distinct=*/false` arguments in the gtests are gone. Its outer DISTINCT
  stage is an init (`Partial`) whose functions include merge rules — `sum`, `min`, `max` over state
  columns — which the builder treats as any `sum`, `min`, `max`. A Welford companion stays refused
  (#261), so no `Partial` node ever carries a three-argument `stddev`.
- **guard-checks (K).** `peacock_cpu_tests` has `PEACOCK_TESTDATA_DIR`; build-test.md's counts were
  reconciled.
- **grouping-id (L).** `static std::unique_ptr<cudf::column> grouping_id_column(const
  flatbuffers::Vector<uint8_t>* mask, cudf::size_type nkeys, cudf::size_type rows)` in
  `aggregate.cpp` builds the id; the grouping-set arm calls it, and so does Task 6's
  `grouping_set_aggregate`. Its gtests, `GroupingId.*`, are in a new file,
  `cpp/tests/gpu/test_grouping_id.cpp`, linked into `peacock_plan_tests` (`test_plan_executor.cpp`
  is past the 1000-line cap), and grouping-id **creates `cpp/tests/gpu/hand_tables.hpp`**, the
  shared header of `hand::` helpers (`column_of`, `values_of`, `table_of`, `ref`, `null_of`,
  `run`) this plan's gtest files include; Task 1 adds whichever of them it lacks. If the file is
  named otherwise, `git grep -l 'TEST(GroupingId' cpp/tests` finds it. Its `grouping_sets_plan`
  helper builds a `count` function node with `add_alias`, as master's did with no `args`; Tasks 3,
  4 and 6 update it. The `bug_`
  grouping-set pins are agreement cases and `grouping_sets_as_exported` is gone, so a rollup's id
  compares exactly between the engines.
- **keyless-identity (L).** `execute_aggregate` starts
  `auto input = was_handed_nothing(in) ? zero_rows_of(agg) : take_input(in);` — handed no input, it
  builds a zero-row table from `aggr_input_schema` and runs as over any input. Keep that line
  exactly; `agg_phase` goes above it (Task 4). `GpuAggregate` is a `BatchAccumulator` with an
  `AtDoneIfNothingOut` call, so recipes and the payload golden carry it, and **every harness case
  driving a `GpuAggregate` runs `Script::Accumulate`, never `Script::Exec`** (`run_both` asserts
  the script's shape against the node's category); an init over one batch answers its state at
  slot 0 and an empty done slot, so `cpu_slot(…, 0)` reads it as before. Its
  `AggregateNoInput.*` gtests are in their own new file, `cpp/tests/gpu/test_aggregate_no_input.cpp`
  (`git grep -l 'TEST(AggregateNoInput' cpp/tests` if named otherwise), on grouping-id's
  `hand_tables.hpp`; their `func_over(fbb, func, arg, alias)` writes one argument and an
  `add_alias`, and `unfed_aggregate` writes `count(*)` as the planner does, `count(1)`
  (`int64_literal(fbb, 1)`). Tasks 3 and 4 update `func_over`; Task 6 checks every `count` there
  carries its argument. Its recipe tests are in
  `peacockdb-core/src/wire/tests/aggregate_init.rs` (`use super::*;`), where Task 2's wire test
  goes: `wire/tests.rs` is at 980 lines.

## Global constraints

- **Every commit green, device included.** The name check would refuse every Welford merge on the
  device until the state columns carry their declared names, so the names (Tasks 2-3) land before
  anything resolves a reference by name (Tasks 6 and 8), and the union's branches carry the union's
  names (Task 7) before every reader checks them (Task 8). Each task proves locally what can be
  proved locally, compiles the device tests (`--features gpu --no-run`, `peacock_plan_tests`
  built), and a task with device tests runs them in its own *Device cycle* (below) before its
  commit: Tasks 1, 3, 4, 6, 8 and 10. `rollup_stddev`'s device cells are turned on in Task 10 with
  the run that writes their `gpu-result.txt` sections.
- **Wire.** `AggregateFuncNode` appends `state_names: [string]` and `ddof: int8`;
  `AggregateFuncNode.alias` and `CudfAggregate.mergeable_agg_state` become `(deprecated)`. No slot
  moves, as chain K deprecated `distinct`. No facade, trait or C ABI symbol changes;
  `cudf_ast_can_evaluate`, `evaluate_column` and `build_column` are internal C++ and gain a names
  parameter.
- **No answer moves.** Every plan golden unchanged except `tpch/rollup-stddev`'s new sections;
  `recipe-payloads.txt` moves in Task 2 (the new fields) and Task 4 (the deprecated two), and in no
  other task. Every corpus cell keeps its state but `tpch/shuffle-stddev`'s schema validation
  (Task 10).
- **A `count` never reaches a `Merge`.** The plan merges a count's state with `sum`
  (`recipe-payloads.txt`: `count(*): sum(count(*)@1)`), and the cpu's `merge_aggregates` refuses a
  `Count` at merge (`cpu_backend/mod.rs`). From Task 6 the device refuses it too, grouped,
  grouping-set and keyless alike; no hand-built plan merges a count by `count`.
- **Out of scope, from the spec's Restriction.** The keyless Welford (#216), the `MERGE_M2` count
  type (#94) and all-NULL Welford groups are welford-device's: the keyless `stddev` reduce arm stays,
  and the one `MERGE_M2` site keeps casting to `INT32` through a named constant. Dead code in
  `window.cpp`, `limit.cpp`, `union.cpp` stays, beyond the edits a changed signature forces.
- **The 1000-line cap** (`coding-style.md`). `wire/tests.rs` (980 lines) takes nothing new: Task 2's
  case goes in `wire/tests/aggregate_init.rs`. `expr.cpp` (934 on master) takes no new function:
  the resolver is `cpp/src/column_refs.cpp` (Task 5), and Task 8 checks its length after threading
  the names. `test_plan_executor.cpp` takes no new test: the new gtests go in
  `test_column_refs.cpp` and `test_aggregate_builder.cpp`.
- **A `bug_` test that goes red** is flipped in the same commit to the agreement case, under a name
  without `bug_` (`coding-style.md`, *Building around a bug*).
- **Builds** as `build-test.md` documents them, each command under `timeout`: rust-only with
  `cargo test --features rust-only`; C++ in `cpp/build` with
  `scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build`;
  the FFI rung and the device build of the Rust tests with
  `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh`. Work in a workspace,
  never the primary checkout; never share a cargo target dir across worktrees. The workspace needs
  the sf1 parquet under `testdata/` (gitignored): symlink it from the primary checkout if absent.
  DuckDB's sections are written with DuckDB 1.5.4's Python module in a venv outside the repo
  (`python3` here has no `duckdb`, and `duckdb_result.py` refuses any other version):
  `[ -x /tmp/duckdb-1.5.4/bin/python ] || { python3 -m venv /tmp/duckdb-1.5.4 && /tmp/duckdb-1.5.4/bin/pip install duckdb==1.5.4; }`.
- **Device** on nebius-gpu, `dmitry@89.169.109.150`, cuDF 25.02, as chain L's header says: the
  working tree synced uncommitted into `~/peacockdb-L` (its sf1 data copied from
  `~/peacockdb-J/testdata`), **one sync per task with as many back-to-back builds as its red/green
  pairs need, the red build first**, every CPU build and run local. Device commands run in the
  foreground (a backgrounded build chain dies mid-build). If `ssh` seems dead while the host
  answers a raw socket, the sandbox is blocking the client, not the host.
- **Out of every task, as the header says:** the sf40 binaries, `--run-benchmarks`, Nsight
  captures and any H200 timing, each recorded in `aggregate-arms-detail.md` as deferred. **Done**
  is CI green except the `gpu-tests` job ("GPU Tests (remote)", shad-gpu), with every task's
  device runs passed on nebius-gpu and recorded in the detail file.
- New tickets take the next free number from master's `tickets.md` as it stands after J's merge.
- Commit messages at most 10 lines, ending with the `Co-Authored-By` line shown in each task.

## Device cycle

Every task with device tests runs this once, after its tests and its fix are written and every
local tier passes, and before its commit. One sync carries the whole working tree and the fix's
reverse; the red build is the tree with the fix reversed, the green build the tree as written.
Each task names its `FIX` (the paths whose diff is the fix, plus any test file that cannot compile
without it; never a test the red build must run), its red set and its green set, each set a list
of `ssh` commands written with `$H` and `$R` below. Run everything in the foreground.

```bash
H=dmitry@89.169.109.150
R='cd ~/peacockdb-L && . ~/peacock-env.sh && export LD_LIBRARY_PATH=$PWD/cpp/install/lib:$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib PEACOCK_TESTDATA_DIR=$PWD/testdata &&'
B='cd ~/peacockdb-L && . ~/peacock-env.sh && ./scripts/build-test-shadgpu.sh --build'
# The fix's reverse (git add -N any new file among FIX first, so the diff carries it).
git diff -R HEAD -- $FIX > /tmp/aggregate-arms-red.patch
# The one sync.
timeout 900 rsync -a --delete-after --exclude=.git --filter=':- .gitignore' ./ $H:peacockdb-L/
timeout 300 rsync -a /tmp/aggregate-arms-red.patch $H:aggregate-arms-red.patch
# Red build: the tests without the fix; then the task's red set.
timeout 300 ssh $H 'cd ~/peacockdb-L && patch -p1 < ~/aggregate-arms-red.patch'
timeout 10800 ssh $H "$B"
# Green build: the fix back in; then the task's green set.
timeout 300 ssh $H 'cd ~/peacockdb-L && patch -p1 -R < ~/aggregate-arms-red.patch'
timeout 10800 ssh $H "$B"
```

A task with no red build (it says why) skips the patch and the red lines. Every case the task
names as red must fail, for the reason it names; a case red for another reason, or green where
the task expects red, is a finding to settle before the fix — a test that passes without its fix
may assert nothing. Every case of the green set passes. One binary per `ssh`, so one red does not
hide the next. If the loader cannot find `libcudf`, the 25.02 env on that host is
`~/miniforge3/envs/rapids-cuda-12.2` (#260 records that path): use whichever exists and say so in
the detail file. Record each build (host, tree, command, pass and fail counts per binary) in
`llm-wiki/tasks/aggregate-arms-detail.md`.

## Review focus

Six input classes nothing tests today, most likely to bite, each pinned in its owning task:

1. **A merge whose state columns are not the next columns after the keys, or not in state
   order.** A cursor reads the wrong column and nothing fails; q17 interleaves count, avg and
   stddev state. Expected: each function reads where its `args` point. Task 6:
   `AggregateBuilder.AWelfordMergeReadsTheTripleWhereItsArgsPoint` (input `[k, m2, count, mean]`)
   and `AggregateBuilder.AMergeReadsEachStateColumnWhereItsArgPoints`.
2. **A function after a Welford under a grouping set.** #280's shift: with a one-column Welford,
   the `sum` after it read a neighbour. Expected: the triple, then the `sum`'s right value, on both
   engines. Task 6: `AggregateBuilder.AGroupingSetBuildsTheWelfordTripleAndWhatFollowsIt` and the
   Rust `a_rollup_with_a_stddev_a_var_and_a_sum_agrees` / `a_rollup_welford_merge_agrees`.
3. **Zero rows through the builder.** After keyless-identity a grouped init's done call hands a
   zero-row table, and a merge can meet a zero-row arrival; `MERGE_M2` then packs a zero-row
   struct. Expected: no groups, every state column typed as declared. Task 6:
   `AggregateBuilder.AWelfordOverZeroRowsIsNoGroupsTypedAsDeclared` (both phases).
4. **A `ColumnRef` resolved against a table the operator assembled**, not a handle: the join
   residual's filter-schema table (its names come from the two sides through `filter_columns`) and
   the grouping-set path's per-set keys. Expected: a residual reading the right names runs, and one
   naming another column is refused. Task 8: `ColumnRefs.AJoinResidualResolvesThroughItsSidesNames`
   in `test_join_session.cpp`.
5. **A union branch whose names differ from the union's and whose types match.** A forwarder hands
   the node above each branch's batches as they are. Expected: the branch is renamed by a project
   of bare references, and a branch already under the union's names is left alone. Task 7:
   `a_union_branch_named_otherwise_is_renamed_by_a_project`.
6. **A scan whose declared schema renames a column it does not read**, or reads under a
   projection. Expected: a rename at an unprojected ordinal plans; one at a projected ordinal is
   refused naming the ordinal and both names. Task 9:
   `a_rename_of_a_column_the_scan_does_not_read_plans` and the two refusals.

## File structure

| file | responsibility |
|---|---|
| `cpp/src/plan_executor.h`, `cpp/src/table_result.cpp` | `TableResult::of`; the default constructor private (Task 1) |
| `flatbuffers/gpu_plan.fbs` | `state_names`, `ddof` appended (Task 2); `alias`, `mergeable_agg_state` deprecated (Task 4) |
| `peacockdb-core/src/plan/mod.rs`, `plan/aggregate.rs`, `plan/tests/aggregate.rs` | `StateFunc.state_names`, `.ddof`; `.welford` removed (Task 4) |
| `peacockdb-core/src/wire/aggregate_writer.rs`, `wire/fb_text.rs`, `wire/tests/aggregate_init.rs` | the fields written, printed, read back |
| `cpp/src/operators/aggregate.cpp` | names (Task 3), phases and dead arms (Task 4), the builder (Task 6), `column_at` (Tasks 6 and 8) |
| `cpp/src/plan_executor_internal.h`, `cpp/src/column_refs.cpp` (new), `cpp/CMakeLists.txt` | `column_ordinal`, `column_at` (Task 5) |
| `cpp/src/expr.cpp`, `cpp/src/peacock/expr.h` | names threaded (Task 8) |
| `cpp/src/operators/{project,sort,filter,window,join_session}.cpp`, `cpp/src/node_session.cpp` | every other `ColumnRef` reader (Task 8) |
| `cpp/tests/gpu/hand_tables.hpp` (grouping-id's; extended), `test_column_refs.cpp` (new), `test_aggregate_builder.cpp` (new), `test_plan_executor.cpp`, grouping-id's and keyless-identity's gtest files, `test_join_session.cpp`, `cpp/tests/cpu/test_executor.cpp`, `cpp/CMakeLists.txt` | the gtests |
| `peacockdb-core/src/planner/translator/nodes.rs` (`cast_branch`), `planner/translator/schema_tests.rs` | a union branch named otherwise projected (Task 7) |
| `peacockdb-core/src/planner/translator/scan_mapping/parquet_meta.rs`, `parquet_meta/tests.rs` | the scan's name check (Task 9) |
| `peacockdb-core/src/tests/gpu_tests/aggregate_{cases,dimension_cases,schema_cases}.rs` | #225's pins flip; the rollup Welford cases |
| `testdata/tpch-queries/rollup-stddev.sql`, `testdata/goldens/`, `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv` | the corpus (Task 10) |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets/corpus-coverage.md` | docs, counts, tickets citing moved code (Task 11) |
| `llm-wiki/tasks/aggregate-arms-detail.md` | every device run (Tasks 1, 3, 4, 6, 8, 10) |

---

### Task 1: Every `TableResult` is built through a checked constructor (#164)

#164's constructor is chain J's `TableResult::owning(table, names)`, which already refuses a name
count unequal to the column count, and a table of no columns. What #164 still needs: the sites that
assemble a `TableResult` from shared parts (a project sharing its input's owners, a join's emit)
cannot hand `owning` a table without copying, so they get its parts form, `of`, with the same
check; nothing builds a `TableResult` around the constructors; every name list is indexed with
`.at()`.

**Files:**
- Modify: `cpp/src/plan_executor.h` (`TableResult`), `cpp/src/table_result.cpp`
- Modify: every site the compiler names once the default constructor is private; every
  `names[...]` read the grep below lists
- Modify: `cpp/tests/gpu/hand_tables.hpp` (grouping-id's: the helpers below it lacks)
- Create: `cpp/tests/gpu/test_column_refs.cpp`
- Test: `cpp/tests/cpu/test_executor.cpp`
- Modify: `cpp/CMakeLists.txt` (`peacock_plan_tests`' sources)

**Interfaces:**
- Produces (Task 8 and every later C++ site build through these):

```cpp
// plan_executor.h, inside struct TableResult, after `with`:
  /// owning's check over parts: the constructor for a table assembled from columns other
  /// handles own. Refuses parts of different lengths, and no columns: every reader indexes
  /// owners, columns and names by one ordinal.
  static TableResult of(std::vector<std::shared_ptr<cudf::column const>> owners,
                        std::vector<cudf::column_view> columns,
                        std::vector<std::string> column_names);

 private:
  // Only the constructors above make an empty one to fill, so no site can build around them.
  TableResult() = default;
```

- Consumes and extends (test-only, `cpp/tests/gpu/hand_tables.hpp`, which grouping-id created;
  used by Tasks 6 and 8 and by welford-device). This plan's gtests use `hand::column_of`,
  `values_of`, `table_of`, `ref`, `null_of` and `run`. Add to the header, inside its `namespace
  hand`, each of these it does not already define with this meaning (one that exists under the
  same name and signature is used as it is; one that exists with another signature keeps its
  name, and this plan's calls are written to it), and the includes they need:

```cpp
// Includes the helpers below need, beside the header's own:
#include "generated/gpu_plan_generated.h"
#include "peacock/operators.h"
#include "plan_executor.h"
#include <cudf/column/column.hpp>
#include <cudf/table/table.hpp>
#include <cudf/utilities/default_stream.hpp>
#include <cudf/utilities/type_dispatcher.hpp>
#include <rmm/device_buffer.hpp>
#include <cuda_runtime.h>
#include <flatbuffers/flatbuffers.h>

namespace fb = peacock::plan;  // inside namespace hand, if the header has no such alias

template <typename T>
std::unique_ptr<cudf::column> column_of(std::vector<T> const& values) {
  rmm::device_buffer data(values.data(), values.size() * sizeof(T), cudf::get_default_stream());
  return std::make_unique<cudf::column>(cudf::data_type{cudf::type_to_id<T>()},
                                        static_cast<cudf::size_type>(values.size()),
                                        std::move(data), rmm::device_buffer{}, 0);
}

template <typename T>
std::vector<T> values_of(cudf::column_view const& column) {
  std::vector<T> host(column.size());
  cudaMemcpy(host.data(), column.data<T>(), host.size() * sizeof(T), cudaMemcpyDeviceToHost);
  return host;
}

inline peacock::TableResult table_of(std::vector<std::unique_ptr<cudf::column>> columns,
                                     std::vector<std::string> names) {
  return peacock::TableResult::owning(std::make_unique<cudf::table>(std::move(columns)),
                                      std::move(names));
}

/// A ColumnRef; `name` null writes none, which is what a hand-built plan used to do.
inline flatbuffers::Offset<fb::Expr> ref(flatbuffers::FlatBufferBuilder& fbb, uint32_t index,
                                         const char* name) {
  auto text = name ? fbb.CreateString(name) : flatbuffers::Offset<flatbuffers::String>{};
  return fb::CreateExpr(fbb, fb::ExprNode_ColumnRef, fb::CreateColumnRef(fbb, index, text).Union());
}

inline flatbuffers::Offset<fb::Expr> null_of(flatbuffers::FlatBufferBuilder& fbb, fb::DataType type) {
  fb::ScalarValueBuilder sb(fbb);
  sb.add_type(type);
  sb.add_is_null(true);
  auto value = sb.Finish();
  return fb::CreateExpr(fbb, fb::ExprNode_LiteralExpr, fb::CreateLiteralExpr(fbb, value).Union());
}

/// `node` as the root of `fbb`'s plan, run once over `inputs`, as NodeSession hands them.
inline peacock::TableResult run(flatbuffers::FlatBufferBuilder& fbb,
                                flatbuffers::Offset<fb::PlanNode> node,
                                std::vector<peacock::TableResult> inputs) {
  fbb.Finish(fb::CreateGpuPlan(fbb, node));
  return peacock::execute_one(fb::GetGpuPlan(fbb.GetBufferPointer())->root(), std::move(inputs));
}
```

- [ ] **Step 1: The refusals, as gtests.** `of`'s refusals need no device memory (a
  `column_view` of no rows over no buffer is `typed_col`'s, and an owner may be null when nothing
  reads it), so they run in the cpu tier. In `cpp/tests/cpu/test_executor.cpp` (add
  `#include "plan_executor.h"` if absent), after the anonymous namespace:

```cpp
// #164: a handle's owners, columns and names are one list read by one ordinal, so a
// TableResult assembled from parts is refused unless the three agree, and refused empty.
TEST(TableResultInvariant, PartsOfDifferentLengthsAreRefused) {
  auto int64 = typed_col(cudf::data_type{cudf::type_id::INT64});
  try {
    peacock::TableResult::of({nullptr, nullptr}, {int64, int64}, {"a"});
    FAIL() << "two columns took one name";
  } catch (std::runtime_error const& e) {
    EXPECT_STREQ(e.what(), "TableResult: 2 columns, 2 owners and 1 names");
  }
}

TEST(TableResultInvariant, NoColumnsAreRefused) {
  EXPECT_THROW(peacock::TableResult::of({}, {}, {}), std::runtime_error);
}
```

  `owning`'s refusal needs a table on the device, so it is a device gtest, a pin of J's check in
  #164's terms. `cpp/tests/gpu/test_column_refs.cpp`:

```cpp
/// #164: a handle's columns and names are one list read by one ordinal, so every TableResult is
/// built through a constructor that refuses a mismatch, and every ColumnRef reader checks the
/// reference's bounds and name (Task 8 adds those cases here).

#include "hand_tables.hpp"

#include <gtest/gtest.h>

#include <stdexcept>
#include <string>

TEST(TableResultInvariant, ThreeColumnsAndTwoNamesAreRefused) {
  std::vector<std::unique_ptr<cudf::column>> columns;
  for (int i = 0; i < 3; ++i) columns.push_back(hand::column_of<int64_t>({1, 2}));
  try {
    hand::table_of(std::move(columns), {"a", "b"});
    FAIL() << "a table of three columns took two names";
  } catch (std::runtime_error const& e) {
    std::string what = e.what();
    EXPECT_NE(what.find('3'), std::string::npos) << what;
    EXPECT_NE(what.find('2'), std::string::npos) << what;
  }
}
```

  The pin asserts `owning`'s two counts, whatever refcounted-scatter's sentence around them.

- [ ] **Step 2: Wire the files.** `cpp/CMakeLists.txt`: append `tests/gpu/test_column_refs.cpp` to
  `peacock_plan_tests`' sources, keeping every source chain J, grouping-id and keyless-identity
  added (`main` stays in `test_plan_executor.cpp`). Build: it fails — `TableResult::of` does not
  exist.

```bash
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
```

  Expected: FAIL, `'of' is not a member of 'peacock::TableResult'`.

- [ ] **Step 3: `of`, and the private default.** Add the *Interfaces* block to `TableResult`, and
  to `table_result.cpp`:

```cpp
TableResult TableResult::of(std::vector<std::shared_ptr<cudf::column const>> owners,
                            std::vector<cudf::column_view> columns,
                            std::vector<std::string> column_names) {
  if (columns.empty())
    throw std::runtime_error(
        "TableResult: a table of no columns reads as no rows; the plan's __rowmarker__ "
        "placeholder exists so that none is ever made");
  if (owners.size() != columns.size() || column_names.size() != columns.size())
    throw std::runtime_error("TableResult: " + std::to_string(columns.size()) + " columns, " +
                             std::to_string(owners.size()) + " owners and " +
                             std::to_string(column_names.size()) + " names");
  TableResult out;
  out.owners = std::move(owners);
  out.columns = std::move(columns);
  out.column_names = std::move(column_names);
  return out;
}
```

- [ ] **Step 4: Every site through the constructors.** Build; the compiler names every
  `TableResult x;` outside the class. Each becomes `owning(...)` when it wraps a fresh table, or
  `of(...)` when it assembles parts (exit-copies' project builds owners, columns and names in a
  loop: collect three local vectors and `return TableResult::of(...)`). Then find what mutates a
  built one:

```bash
git grep -nE '\.(owners|columns|column_names)\s*(\.push_back|\.emplace_back|=[^=])' cpp/src ':!cpp/src/table_result.cpp'
```

  Each hit becomes a constructor call: an appended computed column is `out = out.with(std::move(col),
  name)` (exit-copies' window), a concatenation of two sides is `of` over the concatenated parts
  (the join session's emit). Expected: the grep prints nothing afterwards.

- [ ] **Step 5: Names read with `.at()`.**

```bash
git grep -nE '(names|column_names)\[' cpp/src
```

  Every hit becomes `.at(...)`, except a read inside a loop bounded by the same vector's `size()`.
  On master the list was `filter.cpp:42`, `join.cpp:203,212,260,271,338,343,377,526`,
  `window.cpp:47`, `aggregate.cpp:169,760` and `gpu_executor.cpp:88,92`; J replaced most of them
  with `select` (which bounds-checks) and deleted `join.cpp`, and Task 6 rewrites `aggregate.cpp`.

- [ ] **Step 6: Build and run what runs locally.**

```bash
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
```

  Expected: the build is clean (no new warnings) and `peacock_plan_tests` links; `-L cpu` PASS,
  the two `TableResultInvariant` cpu cases among them; the gpu lib compiles.

- [ ] **Step 7: The device cycle**, green build only: the one device case pins `owning`'s refusal,
  which J landed, and `of` cannot run before it exists (its red was Step 2's). The run is what
  shows every construction site Step 4 moved still answers on the device. Green set:

```bash
timeout 3600 ssh $H "$R cpp/install/bin/peacock_plan_tests"
timeout 1800 ssh $H "$R cpp/install/bin/peacock_gpu_tests"
timeout 1800 ssh $H "$R cpp/install/bin/peacock_join_session_tests"
timeout 7200 ssh $H "$R cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests:: --test-threads=1"
```

  Expected: all green, `TableResultInvariant.ThreeColumnsAndTwoNamesAreRefused` among them; the
  run recorded in `aggregate-arms-detail.md`.

- [ ] **Step 8: Commit.**

```bash
git add cpp/src cpp/tests/gpu/hand_tables.hpp cpp/tests/gpu/test_column_refs.cpp cpp/tests/cpu/test_executor.cpp cpp/CMakeLists.txt llm-wiki/tasks/aggregate-arms-detail.md
git commit -m "#164: every TableResult built through a checked constructor; names read with .at()

owning (J) wraps a table; of applies its check to shared parts.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `state_names` and `ddof` on the wire

**Files:**
- Modify: `flatbuffers/gpu_plan.fbs` (`table AggregateFuncNode`)
- Modify: `peacockdb-core/src/plan/mod.rs` (`struct StateFunc`), `plan/aggregate.rs` (`state_funcs`)
- Modify: `peacockdb-core/src/wire/aggregate_writer.rs` (`state_funcs`, `named_func`),
  `wire/fb_text.rs` (the `CudfAggregate` arm)
- Test: `peacockdb-core/src/plan/tests/aggregate.rs`, `peacockdb-core/src/wire/tests/aggregate_init.rs`
  (keyless-identity's; `wire/tests.rs` is at the 1000-line cap)
- Regenerate: `testdata/goldens/recipe-payloads.txt`

**Interfaces:**
- Produces, the wire (appended after `out_decimal_scale`; nothing reads them until Task 3):

```
  /// The state columns this function emits, in order: one for a sum, min, max or count, three
  /// (`<out>$count`, `<out>$mean`, `<out>$m2`) for a stddev or var. The executor names every
  /// column it emits from here.
  state_names: [string];
  /// A stddev's or var's divisor offset: 1 for the sample forms, 0 for the population ones.
  /// 0, and unread, for every other function.
  ddof: int8;
```

- Produces, the Rust struct carrying them from `state_funcs` to the writer (`plan/mod.rs`):

```rust
pub(crate) struct StateFunc<'a> {
    pub(crate) name: &'static str,
    pub(crate) call: &'a AggCall,
    /// The SQL output name: the cpu backend's DataFusion alias.
    pub(crate) alias: String,
    /// Every state column this function emits, as the plan declares them: its calls' outputs,
    /// three for a folded Welford triple, one otherwise. The device names its columns from these.
    pub(crate) state_names: Vec<String>,
    /// `AggStateColumns::ddof` for a Welford triple, 0 for every other function.
    pub(crate) ddof: u32,
    pub(crate) welford: bool,
}
```

- [ ] **Step 1: The rule's cases, failing.** In `plan/tests/aggregate.rs`, extend
  `a_welford_triple_is_one_aggregate_under_the_name_sql_wrote` after its `assert_eq!`:

```rust
    assert_eq!(
        funcs[0].state_names,
        ["stddev(v)$count", "stddev(v)$mean", "stddev(v)$m2"],
        "the triple's three columns, each under the name the plan declares"
    );
    assert_eq!(funcs[0].ddof, 1, "the sample form");
```

  and add after it:

```rust
/// A plain aggregator is one column under its call's output name, and has no ddof.
#[test]
fn a_plain_aggregate_has_one_state_name_and_no_ddof() {
    let body = AggregateBody {
        group_by: vec![Expr::column(0, "k")],
        grouping_sets: Vec::new(),
        null_exprs: Vec::new(),
        aggs: vec![AggCall {
            func: PlanAgg::Sum,
            args: vec![Expr::column(1, "v")],
            outputs: vec![Field::new("sum(v)", DataType::Int64, true)],
        }],
        finalize: None,
    };
    let funcs = state_funcs(&body, &columns(&["k", "sum(v)"])).expect("nameable");
    assert_eq!(funcs[0].state_names, ["sum(v)"]);
    assert_eq!(funcs[0].ddof, 0);
}

/// A merge's `merge_m2` is one call with three outputs; its names are those three.
#[test]
fn a_welford_merge_names_the_three_columns_it_merges() {
    let names = ["stddev(v)$count", "stddev(v)$mean", "stddev(v)$m2"];
    let body = AggregateBody {
        group_by: vec![Expr::column(0, "k")],
        grouping_sets: Vec::new(),
        null_exprs: Vec::new(),
        aggs: vec![AggCall {
            func: PlanAgg::MergeM2,
            args: (0..3).map(|i| Expr::column(1 + i, names[i as usize])).collect(),
            outputs: names
                .iter()
                .map(|n| Field::new(*n, DataType::Float64, true))
                .collect(),
        }],
        finalize: None,
    };
    let state = welford_state(&["k", names[0], names[1], names[2]], 1, "stddev(v)");
    let funcs = state_funcs(&body, &state).expect("nameable");
    assert_eq!(funcs[0].name, "stddev");
    assert_eq!(funcs[0].state_names, names);
    assert_eq!(funcs[0].ddof, 1);
}
```

  `columns` is the helper the file's finalize tests already use; if it is named otherwise there,
  use that one.

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- plan::tests::aggregate
```

  Expected: FAIL to compile, `no field state_names on StateFunc`.

- [ ] **Step 2: The fields and the rule.** Append the two fields to the fbs as *Interfaces* shows.
  Add `state_names` and `ddof` to `StateFunc` (keep `welford` until Task 4). In `state_funcs`
  (`plan/aggregate.rs`), the `Some(owner)` arm:

```rust
            Some(owner) => {
                if funcs.iter().any(|f| f.alias == owner.output) {
                    continue;
                }
                // Every aggregator the triple folds, in state order: the init's three calls, or the
                // merge's one merge_m2 with three outputs.
                let state_names = body
                    .aggs
                    .iter()
                    .enumerate()
                    .filter(|(at, _)| {
                        welford
                            .get(*at)
                            .copied()
                            .flatten()
                            .is_some_and(|other| other.output == owner.output)
                    })
                    .flat_map(|(_, owned)| owned.outputs.iter().map(|field| field.name().clone()))
                    .collect();
                funcs.push(StateFunc {
                    name: sql_name(owner.func, owner.ddof),
                    call,
                    alias: owner.output.clone(),
                    state_names,
                    ddof: owner.ddof,
                    welford: true,
                });
            }
```

  and the `None` arm:

```rust
            None => {
                let state_names: Vec<String> =
                    call.outputs.iter().map(|field| field.name().clone()).collect();
                funcs.push(StateFunc {
                    name: agg_name(call.func)?,
                    call,
                    alias: state_names.first().cloned().unwrap_or_default(),
                    state_names,
                    ddof: 0,
                    welford: false,
                });
            }
```

  Update `StateFunc`'s doc comment: the order is still load-bearing for the cpu backend, which
  builds one DataFusion aggregate per entry; the device reads each function's inputs through its
  `args` from Task 6.

- [ ] **Step 3: Run the rule's cases.** Same command. Expected: PASS.

- [ ] **Step 4: The writer, failing first.** In `wire/tests/aggregate_init.rs`, at the end (the
  child reaches `aggregate`, `Writer`, `Given`, `columns_of` and `node_at` through `use super::*`;
  after keyless-identity an init's recipe carries a second call at done, and `recipe.seqs()[0]` is
  still the init's seq):

```rust
/// Every column the device emits is named on the wire, and a Welford's ddof rides beside its
/// three names: a sample stddev must not reach the device reading as a population one.
#[test]
fn an_init_writes_each_functions_state_names_and_a_welfords_ddof() {
    let triple = |out: &str| -> Vec<AggCall> {
        [(PlanAgg::Count, "$count"), (PlanAgg::Mean, "$mean"), (PlanAgg::M2, "$m2")]
            .into_iter()
            .map(|(func, suffix)| AggCall {
                func,
                args: vec![Expr::column(1, "n")],
                outputs: vec![Field::new(format!("{out}{suffix}"), DataType::Int64, true)],
            })
            .collect()
    };
    let mut aggs = triple("stddev(n)");
    aggs.extend(triple("var_pop(n)"));
    aggs.push(AggCall {
        func: PlanAgg::Count,
        args: vec![Expr::column(1, "n")],
        outputs: vec![Field::new("count(n)", DataType::Int64, true)],
    });
    let names = [
        "k", "stddev(n)$count", "stddev(n)$mean", "stddev(n)$m2",
        "var_pop(n)$count", "var_pop(n)$mean", "var_pop(n)$m2", "count(n)",
    ];
    let state = Schema {
        group_keys: vec![0],
        agg_state: vec![
            AggStateColumns { output: "stddev(n)".into(), func: AggFunc::Stddev, ddof: 1, positions: vec![1, 2, 3] },
            AggStateColumns { output: "var_pop(n)".into(), func: AggFunc::Var, ddof: 0, positions: vec![4, 5, 6] },
        ],
        ..columns_of(&names)
    };
    let body = AggregateBody {
        group_by: vec![Expr::column(0, "k")],
        grouping_sets: Vec::new(),
        null_exprs: Vec::new(),
        aggs,
        finalize: None,
    };
    let node = GpuAggregate::new(
        Given::input(BatchLayout::MultipleBatches, &["k", "n"]),
        body,
        state.clone(),
        state,
    );
    let mut writer = Writer::new();
    let recipe = aggregate(&node, &[&columns_of(&["k", "n"])], &mut writer)
        .expect("the aggregate's payloads are writable")
        .expect("an aggregate drives the ABI");
    let (bytes, _) = writer.finish().expect("one root");
    let plan = flatbuffers::root::<fb::GpuPlan>(&bytes).expect("the buffer verifies");
    let written = node_at(&plan, recipe.seqs()[0])
        .expect("the walk names every kind")
        .and_then(|node| node.node_as_cudf_aggregate())
        .expect("the first seq is the init");
    let funcs: Vec<(String, Vec<String>, i8)> = written
        .aggr_funcs()
        .expect("functions")
        .iter()
        .map(|f| {
            (
                f.name().unwrap_or_default().to_string(),
                f.state_names()
                    .map(|n| n.iter().map(str::to_string).collect())
                    .unwrap_or_default(),
                f.ddof(),
            )
        })
        .collect();
    let owned = |n: &[&str]| n.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        funcs,
        vec![
            ("stddev".to_string(), owned(&names[1..4]), 1),
            ("var_pop".to_string(), owned(&names[4..7]), 0),
            ("count".to_string(), owned(&names[7..8]), 0),
        ]
    );
}
```

  Add `use crate::plan::{AggFunc, AggStateColumns};` to the file if `super::*` does not bring
  them (and `fb`, `flatbuffers` likewise, from `wire/tests.rs`' own imports).

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- wire::tests::aggregate_init::an_init_writes
```

  Expected: FAIL — every `state_names` is empty and every `ddof` 0, since the writer does not
  write them yet.

- [ ] **Step 5: Write them.** In `aggregate_writer.rs`, `state_funcs` calls
  `named_func(b, func)?`, and `named_func` becomes:

```rust
fn named_func<'a>(
    b: &mut FlatBufferBuilder<'a>,
    func: &StateFunc,
) -> Result<WIPOffset<fb::AggregateFuncNode<'a>>, PlanError> {
    let mut args = Vec::with_capacity(func.call.args.len());
    for arg in &func.call.args {
        args.push(write_expr(b, arg)?);
    }
    let args = b.create_vector(&args);
    let name = b.create_string(func.name);
    let alias = b.create_string(&func.alias);
    let names: Vec<WIPOffset<&str>> =
        func.state_names.iter().map(|n| b.create_string(n)).collect();
    let state_names = b.create_vector(&names);
    let ddof = i8::try_from(func.ddof).map_err(|_| {
        PlanError::Invalid(format!("{}: ddof {} does not fit the wire's int8", func.name, func.ddof))
    })?;
    Ok(fb::AggregateFuncNode::create(
        b,
        &fb::AggregateFuncNodeArgs {
            name: Some(name),
            args: Some(args),
            alias: Some(alias),
            out_decimal_precision: 0,
            out_decimal_scale: 0,
            state_names: Some(state_names),
            ddof,
        },
    ))
}
```

  (`use crate::plan::StateFunc;`.) Keep the doc comment above it; its "read by `aggregate.cpp`"
  sentence is Task 4's to change.

- [ ] **Step 6: Print them.** In `fb_text.rs`'s aggregate arm, the function line keys on the state
  names and shows a ddof only where one is stored (the file's rule: a field at its default is
  absent):

```rust
                for func in funcs.iter() {
                    let args = func
                        .args()
                        .map(|args| {
                            let written: Vec<String> = args.iter().map(|a| expr_text(&a)).collect();
                            written.join(", ")
                        })
                        .unwrap_or_default();
                    let named = func
                        .state_names()
                        .map(|names| names.iter().collect::<Vec<_>>().join(", "))
                        .unwrap_or_default();
                    let ddof = match func.ddof() {
                        0 => String::new(),
                        d => format!(" ddof={d}"),
                    };
                    field(&named, format!("{}({args}){ddof}", func.name().unwrap_or("?")));
                }
```

- [ ] **Step 7: Run, and regenerate the payloads.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- wire::tests plan::tests::aggregate
UPDATE_CANONICAL=1 PEACOCK_REWRITE_RECIPE_BYTES=1 timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens::the_payload_golden_carries_what_each_call_hands_the_executor
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
git diff --stat testdata/
```

  The payload test points `/tmp/peacock-plan-bytes-root` at this workspace's `testdata` itself
  (`point_canonical_root`), so the embedded paths and digests are machine-independent; do not run
  two workspaces' payload tests at once. Expected: PASS; `git diff --stat testdata/` lists
  `recipe-payloads.txt` alone, every aggregate's `sha256=` and function lines changed — e.g.
  `stddev(lineitem.l_quantity)$count, stddev(lineitem.l_quantity)$mean, stddev(lineitem.l_quantity)$m2: stddev(l_quantity@0) ddof=1`.
  No `.plans.txt` moves.

- [ ] **Step 8: The C++ still builds** (the generated header gains two accessors nothing reads):

```bash
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests:: planner::tests::plan_goldens::the_payload_golden
```

  Expected: clean build; PASS.

- [ ] **Step 9: Commit.**

```bash
git add flatbuffers/gpu_plan.fbs peacockdb-core/src testdata/goldens/recipe-payloads.txt
git commit -m "#225: AggregateFuncNode carries state_names and ddof; the writer fills them

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: The device names every state column from `state_names` (#225)

**Files:**
- Modify: `cpp/src/operators/aggregate.cpp` (every name push; `make_agg`; `stddev_ddof` goes)
- Modify: `cpp/tests/gpu/test_plan_executor.cpp`, grouping-id's `test_grouping_id.cpp` and
  keyless-identity's `test_aggregate_no_input.cpp` (every hand-built `AggregateFuncNode`)
- Modify: `peacockdb-core/src/tests/gpu_tests/aggregate_schema_cases.rs` (#225's pins; #216's
  schema pin's message), `aggregate_cases.rs` (`welford_answered`)

- [ ] **Step 1: Flip #225's pins** (red on the device until Step 3). In `aggregate_schema_cases.rs`,
  delete `WELFORD_RENAMED` and its two `// #225` comments; the two cases become (a `GpuAggregate`
  runs `Script::Accumulate` since keyless-identity):

```rust
operator_case! {
    GpuAggregate,
    fn stddev_holds_its_three_declared_state_names() {
        assert_holds_as_declared(&welford_init(), Script::Accumulate(vec![input()]));
    }
}

operator_case! {
    GpuAggregateBatches,
    fn stddev_merge_holds_its_three_declared_state_names() {
        let partial = |seed| welford_partial(AggFunc::Stddev, seed);
        assert_holds_as_declared(&welford_merge(), Script::Accumulate(vec![partial(1), partial(2), partial(3)]));
    }
}
```

  `bug_a_global_stddev_holds_one_finished_float64_where_the_plan_declares_the_welford_state` (#216,
  welford-device's, in the same file) keeps its name and its `Script::Accumulate`; the one column
  is now named from `state_names`, so its divergence loses the second name:
  `"3 columns declared, 1 held; 0 stddev(f64)$count: Int64 vs FLOAT64"`.
  In `aggregate_cases.rs`, `welford_answered` stops borrowing the device's names — the doc
  sentence about "a relabelling unobservable past the sink" goes with them:

```rust
/// The device's Welford state against the cpu's at slot `at`, the count exact and `Int64` on
/// both, names as declared. The mean and m2 are checked to `WELFORD_RELATIVE`, since a Welford
/// update is order-dependent and a mean is not dyadic, and the harness's exact comparison has
/// no tolerance by design.
fn welford_answered(outcome: &Outcome, at: usize) {
    same_within_welford(cpu_slot(outcome, at), gpu_slot(outcome, at), true, &[2, 3]);
}
```

```bash
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
```

  Expected: compiles (unused imports, if any, removed).

- [ ] **Step 2: One name helper, every push through it.** In `aggregate.cpp`, above `make_agg`:

```cpp
// The name the plan gives the `i`th column `func` emits. Every state column is named from the
// wire (#225): the three Welford columns are three names, never one alias three times.
static std::string state_name(const fb::AggregateFuncNode* func, flatbuffers::uoffset_t i) {
  const auto* names = func->state_names();
  if (!names || i >= names->size())
    throw std::runtime_error("CudfAggregate: `" +
                             (func->name() ? func->name()->str() : std::string("?")) +
                             "` emits a column " + std::to_string(i) + " its " +
                             std::to_string(names ? names->size() : 0) +
                             " state_names do not name");
  return names->Get(i)->str();
}
```

  Then, by symbol:
  - the keyless loop's `out_names.push_back(func->alias() ? … : name)` → `state_name(func, 0)`;
  - the grouping-set loop's `gs_agg_names.push_back(...)` → `state_name(func, 0)` (it emits one
    column per function until Task 6);
  - the grouped loop: delete `std::string alias = …`; each arm's `builds.push_back({alias, …})`
    names its k-th emitted column `state_name(func, k)` — the Welford init's three builds 0, 1, 2;
    the Welford merge's `ob.name = state_name(func, child)`; every one-column arm 0; the `avg`
    arms 0 and 1 (they go in Task 4).
- [ ] **Step 3: `ddof` from the wire.** `make_agg(name, is_final)` gains a third parameter
  `cudf::size_type ddof` used where it called `stddev_ddof(func_name)`; its three callers pass
  `func->ddof()`. The keyless `is_std` arm's `make_std_aggregation<cudf::reduce_aggregation>(…)`
  takes `func->ddof()`, and so does the grouped Final arm's `ob.ddof`. Delete `stddev_ddof` and its
  comment block.
- [ ] **Step 4: The hand-built function nodes carry their names.** Every
  `fb::CreateAggregateFuncNode` / `fb::AggregateFuncNodeBuilder` in `cpp/tests/` (find them with
  `git grep -nE 'CreateAggregateFuncNode|AggregateFuncNodeBuilder' cpp/tests`) writes
  `state_names`:
  - `nation_aggregate`: `{"state"}` for `sum`, `count`, `avg`'s two `{"state$sum", "state$count"}`,
    `stddev`'s three `{"state$count", "state$mean", "state$m2"}` and `ddof` 1;
  - `AggregateCount`: `{"count(*)"}`; `AggregateGroupBy`: `{"nation_count"}`;
  - grouping-id's `grouping_sets_plan` (`test_grouping_id.cpp`):
    `auto count_states = fbb.CreateVectorOfStrings({"n"});` before the builder opens, and
    `count.add_state_names(count_states);`;
  - keyless-identity's `func_over` (`test_aggregate_no_input.cpp`): its node's one name,
    `auto states = fbb.CreateVectorOfStrings({alias});` before the builder opens, and
    `node.add_state_names(states);` — `AKeylessNodeAnswersOneRowTypedAsItsInputDeclares` asserts
    the names `{"n", "sum(v)", "sum(amount)", "min(day)", "max(v)"}`, which are those aliases.
  Positional `CreateAggregateFuncNode` calls gain the two trailing arguments
  (`fbb.CreateVectorOfStrings(names)`, `ddof`).
- [ ] **Step 5: Build and run what runs locally.**

```bash
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests::
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
git grep -n 'stddev_ddof\|->alias()' cpp/src/operators/aggregate.cpp
```

  Expected: clean; PASS; the grep prints nothing.

- [ ] **Step 6: The device cycle**, `FIX=cpp/src/operators/aggregate.cpp`. The red build is Steps 1
  and 4's tests over the names from `alias`. Red set:

```bash
timeout 3600 ssh $H "$R cpp/install/rust-tests/peacockdb_core_gpu_lib --test-threads=1 aggregate_schema_cases:: aggregate_cases::a_welford"
```

  Expected red, each for the name: `stddev_holds_its_three_declared_state_names` and
  `stddev_merge_holds_its_three_declared_state_names` (three columns under one name),
  `bug_a_global_stddev_holds_one_finished_float64_where_the_plan_declares_the_welford_state` (its
  divergence still names `stddev(f64)`), and `a_welford_init_exports_its_count_as_int64` and
  `a_welford_merge_exports_its_count_as_int64` (`welford_answered` now holds the names). Every other
  case in the set passes. Green set:

```bash
timeout 3600 ssh $H "$R cpp/install/bin/peacock_plan_tests"
timeout 7200 ssh $H "$R cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests:: --test-threads=1"
```

  Expected: all green — `AggregateMerge.*`, `PlanExecutor.Aggregate*`, `GroupingId.*` and
  `AggregateNoInput.*` among the gtests, the five red cases above among the Rust ones.

- [ ] **Step 7: Commit.**

```bash
git add cpp/src/operators/aggregate.cpp cpp/tests peacockdb-core/src/tests/gpu_tests llm-wiki/tasks/aggregate-arms-detail.md
git commit -m "#225: the device names every state column from state_names; ddof from the wire

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Only `Partial` and `Merge` reach the aggregate; the dead arms and two fields go

**Why it is safe.** The writer maps `Phase::Init` to `Partial` and `Phase::Merge` to `Merge` in an
exhaustive match (`aggregate_writer.rs:77-80`), so no other mode is written. No `avg` reaches the
wire: `PlanAgg` has no `Avg` (`plan/mod.rs:362`), `agg_name` names only `PlanAgg`s, and `sql_name`'s
`Avg` arm is reachable only through a Welford owner, whose `func` is `Stddev` or `Var`
(`welford_owners`). `mergeable_agg_state` is set whenever a function folds a triple
(`folded |= func.welford`, `aggregate_writer.rs:143`), so the non-mergeable stddev path never runs.
The payload golden, a cover of every kind and call shape, prints only `mode: Partial` and
`mode: Merge` (Step 6 checks it). `agg_phase`'s refusal is the test.

**Files:**
- Modify: `cpp/src/operators/aggregate.cpp`
- Modify: `flatbuffers/gpu_plan.fbs`, `peacockdb-core/src/plan/mod.rs`, `plan/aggregate.rs`,
  `wire/aggregate_writer.rs`, `wire/fb_text.rs`
- Modify: `cpp/tests/cpu/test_executor.cpp`, `cpp/tests/gpu/test_plan_executor.cpp`,
  grouping-id's `test_grouping_id.cpp`, keyless-identity's `test_aggregate_no_input.cpp`
- Regenerate: `testdata/goldens/recipe-payloads.txt`

**Interfaces:**
- Produces (file-local in `aggregate.cpp`; Task 6 and welford-device use them):

```cpp
enum class AggPhase { Partial, Merge };
/// Throws naming the mode for anything but Partial or Merge, which is all the planner writes.
static AggPhase agg_phase(fb::AggregateMode mode);
```

- [ ] **Step 1: The refusal, failing.** In `cpp/tests/cpu/test_executor.cpp`, add
  `#include "peacock/operators.h"` and:

```cpp
// The planner writes Partial and Merge only; an aggregate finalizes in a project. A Final or
// Single mode is a hand-built plan the arms that served it no longer exist for, so it is refused
// by name before any input is read — which is why this runs with no device.
TEST(AggregatePhase, FinalAndSingleAreRefusedNamingTheMode) {
  for (auto mode : {fb::AggregateMode_Final, fb::AggregateMode_FinalPartitioned,
                    fb::AggregateMode_Single, fb::AggregateMode_SinglePartitioned}) {
    flatbuffers::FlatBufferBuilder b;
    // An input schema, so keyless-identity's no-input arm has something to build from if the
    // mode is not refused first (the red run of Step 1 takes that arm).
    auto field = fb::CreateField(b, b.CreateString("v"), fb::DataType_Int64, true);
    auto schema = fb::CreateSchema(b, b.CreateVector(std::vector<flatbuffers::Offset<fb::Field>>{field}));
    fb::CudfAggregateBuilder agg(b);
    agg.add_mode(mode);
    agg.add_aggr_input_schema(schema);
    auto node = fb::CreatePlanNode(b, fb::PlanNodeKind_CudfAggregate, agg.Finish().Union());
    b.Finish(fb::CreateGpuPlan(b, node));
    try {
      peacock::execute_one(fb::GetGpuPlan(b.GetBufferPointer())->root(), {});
      ADD_FAILURE() << fb::EnumNameAggregateMode(mode) << " was not refused";
    } catch (std::runtime_error const& e) {
      EXPECT_NE(std::string(e.what()).find(std::string("mode ") + fb::EnumNameAggregateMode(mode)),
                std::string::npos)
          << e.what();
    }
  }
}
```

```bash
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure -R peacock_cpu_tests
```

  Expected: FAIL — `Final` reaches the old arms (or `take_input`'s "no inputs" message), and the
  message does not name the mode.

- [ ] **Step 2: `agg_phase` first.** Replace `AggPhase` and `agg_phase` (and the comment above
  them) with:

```cpp
// What a node does with its rows: Partial builds state from values, Merge merges state into
// state. The planner writes nothing else; every aggregate finalizes in a project (#25).
enum class AggPhase { Partial, Merge };

static AggPhase agg_phase(fb::AggregateMode mode) {
  switch (mode) {
    case fb::AggregateMode_Partial:
      return AggPhase::Partial;
    case fb::AggregateMode_Merge:
      return AggPhase::Merge;
    default:
      throw std::runtime_error(std::string("CudfAggregate: mode ") +
                               fb::EnumNameAggregateMode(mode) +
                               " is not one the planner writes — an aggregate builds state "
                               "(Partial) or merges it (Merge), and finalizes in a project");
  }
}
```

  and make `const AggPhase phase = agg_phase(agg->mode());` the first statement of
  `execute_aggregate`, above keyless-identity's input acquisition. Delete the later
  `AggPhase phase = agg_phase(...)` and `bool is_final`.

- [ ] **Step 3: The dead arms go.** With `phase` two-valued and every Welford mergeable:
  - `make_agg(name, is_final, ddof)` → `make_agg(name, phase, ddof)`: `count` is `COUNT` at
    `Partial`, `SUM` at `Merge` (an arm no plan reaches, refused in Task 6 with the builder that
    replaces this function); `sum`, `min`, `max`; `var`/`stddev` names `VARIANCE`/`STD` with
    `ddof`, used by the grouping-set path alone until Task 6 (#280); the `avg` branch and its
    comment go;
  - `make_reduce_agg`'s `avg` branch goes; `is_avg_name` goes;
  - `get_values_col`: its Final branch and its `avg` decimal cast go; it and `arg_col` become one
    `arg_col(func)` (args[0] as a `ColumnRef` read or a `build_column`, the dummy column 0 where
    there are no args — Task 6 refuses that);
  - the keyless loop: the Final AVG/STDDEV guard, `is_avg`, the `count` `is_final` branch (a
    `Merge` count is now `cudf::reduce` SUM to `INT64` until Task 6 refuses it, the `Partial`
    count `size − null_count`),
    the `avg` half of the decimal condition (`is_sum` alone), and the `is_std` arm's `is_final`
    MEAN go; the `is_std` arm keeps `make_std_aggregation(func->ddof())` (#216, welford-device's);
  - the grouped section: `OutBuild` keeps `name`, `req`, `res`, `count_cast`, `struct_child`;
    `has_stddev_or_var_final` and its guard, `mergeable` and `stddev_stride`, the width recovery
    (`avg_state_2col`, `reads_state`, the residual check), and every `avg` / `Final` arm go. The
    loop keeps three arms:

```cpp
  size_t in_off = key_indices.size();  // the merge's state cursor; Task 6 reads args instead
  if (agg->aggr_funcs()) {
    for (flatbuffers::uoffset_t i = 0; i < agg->aggr_funcs()->size(); ++i) {
      auto* func = agg->aggr_funcs()->Get(i);
      std::string name = func->name() ? func->name()->str() : "count";
      bool welford = is_stddev_name(name) || is_var_name(name);
      int r = static_cast<int>(requests.size());
      if (welford && phase == AggPhase::Partial) {
        // (today's mergeable Partial arm, unchanged but for its condition)
      } else if (welford) {
        // (today's mergeable Merge arm, unchanged but for its condition; in_off += 3; continue)
      } else {
        cudf::groupby::aggregation_request req;
        req.values = phase == AggPhase::Merge ? tv.column(static_cast<cudf::size_type>(in_off))
                                              : arg_col(func);
        req.aggregations.push_back(make_agg(name, phase, func->ddof()));
        bool counted = (name == "count" || name == "COUNT") && phase == AggPhase::Partial;
        builds.push_back({state_name(func, 0), r, 0, counted});
        requests.push_back(std::move(req));
      }
      if (phase == AggPhase::Merge) in_off += 1;
    }
  }
```

    and the assembly loop keeps its `struct_child` and `count_cast` branches; `std_finalize`,
    `avg_div` and their arithmetic go.
- [ ] **Step 4: The two fields deprecated.** In `gpu_plan.fbs`: `alias: string (deprecated);` with
  `/// Never set: every column a function emits is named by state_names.`, and
  `mergeable_agg_state: bool = false (deprecated);` with `/// Never set: every Welford state is the
  mergeable triple.` Reword `out_decimal_precision`/`out_decimal_scale`'s comment: on the wire,
  written as zero and read by nothing, since no `avg` reaches a device. Then:
  - `aggregate_writer.rs`: `named_func` drops `alias`; `state_funcs` returns the `Vec` alone and
    `aggregate` drops `mergeable_agg_state`; `named_func`'s doc comment says the decimal pair is read
    by nothing (the `:216 and :711` reference goes);
  - `StateFunc.welford` goes (field, the two literals, its doc);
  - `fb_text.rs`: the `mergeable_agg_state` line goes;
  - `aggregate.cpp`: the generated accessors are gone, so the build lists any reader left.
- [ ] **Step 5: The gtests, in the plan's shapes.** `test_plan_executor.cpp`:
  - `nation_aggregate` loses its `mergeable` parameter and `add_mergeable_agg_state`, and so do
    its callers; no function node sets an alias;
  - `partial_and_merged(func, mergeable)` becomes `partial_and_merged(func, merge)`: the partials
    run `func`, the merge runs `merge`, the function the plan merges `func`'s state with. Its doc
    comment: "One partial of `func`, and the same partial unioned with itself and merged by
    `merge` — `sum` for a `count`, which never reaches a Merge, and the function itself
    otherwise." `WelfordStateComesBackAsStateAndNotAsAValue` calls
    `partial_and_merged("stddev", "stddev")`;
  - `AggregateMerge.AOneColumnAggregateMergesByItsOwnRule` merges a count as the plan does:

```cpp
TEST(AggregateMerge, AOneColumnAggregateMergesByItsOwnRule) {
  // A sum's state merges by sum, and so does a count's: the plan writes a count's merge as
  // `sum` (`count(*): sum(count(*)@1)`), so no count ever reaches a Merge.
  for (auto [func, merge] : {std::pair{"sum", "sum"}, std::pair{"count", "sum"}}) {
    auto [partial, merged] = partial_and_merged(func, merge);
    ASSERT_EQ(partial.size(), 5u) << func;
    for (auto& [key, state] : partial) {
      ASSERT_EQ(state.size(), 1u) << func;
      EXPECT_DOUBLE_EQ(merged.at(key)[0], state[0] * 2) << func << " group " << key;
    }
  }
}
```

  - `AggregateMerge.AnAvgsSumAndCountBothSurviveTheMerge` is deleted (no `avg` reaches a device;
    decomposition's `sum` and `count` are `AOneColumnAggregateMergesByItsOwnRule`'s);
  - `AggregateCount` and `AggregateGroupBy` run `AggregateMode_Partial`, their `count` over
    `make_col_ref(fbb, 0, "r_regionkey")` and `make_col_ref(fbb, 0, "r_name")` respectively;
  - grouping-id's `grouping_sets_plan`: `count.add_alias(...)` and its `count_alias` string go;
  - keyless-identity's `func_over`: `node.add_alias(out)` and its `out` string go (the generated
    builder has no `add_alias` once the field is deprecated, so the compiler names every one left).
- [ ] **Step 6: Build, run, regenerate.**

```bash
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
UPDATE_CANONICAL=1 PEACOCK_REWRITE_RECIPE_BYTES=1 timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens::the_payload_golden_carries_what_each_call_hands_the_executor
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests:: planner::tests::plan_goldens::the_payload_golden
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
grep -E '^ +mode: ' testdata/goldens/recipe-payloads.txt | sort | uniq -c
git grep -nE 'is_avg_name|is_final|mergeable|has_stddev_or_var_final|avg_state_2col' cpp/src
git diff --stat testdata/
```

  Expected: `FinalAndSingleAreRefusedNamingTheMode` PASS; every tier PASS; the `mode:` count shows
  `Partial` and `Merge` only; the grep prints nothing; `recipe-payloads.txt` alone moved, its
  `mergeable_agg_state: true` lines gone and every aggregate digest changed.

- [ ] **Step 7: The device cycle**, green build only: the arms this task deletes are the ones no
  plan reaches, which is the claim the green run checks, and its one new refusal ran red in the
  cpu tier at Step 1. Green set:

```bash
timeout 3600 ssh $H "$R cpp/install/bin/peacock_plan_tests"
timeout 1800 ssh $H "$R cpp/install/bin/peacock_gpu_tests"
timeout 7200 ssh $H "$R cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests:: --test-threads=1"
```

  Expected: all green, `AggregateMerge.*` (three cases), `PlanExecutor.AggregateCount` and
  `AggregateGroupBy` at `Partial`, `GroupingId.*` and `AggregateNoInput.*` among them.

- [ ] **Step 8: Commit.**

```bash
git add cpp flatbuffers/gpu_plan.fbs peacockdb-core/src testdata/goldens/recipe-payloads.txt llm-wiki/tasks/aggregate-arms-detail.md
git commit -m "aggregate: Partial and Merge only; Final, Single and avg arms go

alias and mergeable_agg_state deprecated, no slot moved: the writer sends
only Partial/Merge, never avg, and every Welford as the mergeable triple.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: One resolver for a `ColumnRef` (#164)

**Files:**
- Modify: `cpp/src/plan_executor_internal.h`, `cpp/CMakeLists.txt` (`peacock_gpu`'s sources)
- Create: `cpp/src/column_refs.cpp` (`expr.cpp` is near the 1000-line cap and Task 8 adds to it)
- Test: `cpp/tests/cpu/test_executor.cpp`

**Interfaces:**
- Produces (`plan_executor_internal.h`, defined in `column_refs.cpp`; host-only, so the CPU tests
  reach them as they reach `cudf_ast_can_evaluate`; every reader in Tasks 6 and 8 uses them, and
  welford-device consumes `column_at`):

```cpp
// The ordinal a ColumnRef names in a table whose columns are `names`, checked: an index past
// the width throws naming the index and the width, a reference with no name throws, and a name
// other than names[index] throws naming both. `names` is the table's, one per column.
cudf::size_type column_ordinal(const fb::ColumnRef* ref, std::vector<std::string> const& names);

// table.column(column_ordinal(ref, names)): the column, borrowed.
cudf::column_view column_at(cudf::table_view const& table,
                            std::vector<std::string> const& names, const fb::ColumnRef* ref);
```

- [ ] **Step 1: The three refusals, failing.** In `test_executor.cpp`'s anonymous namespace:

```cpp
// A standalone ColumnRef table; `name` null writes none.
const fb::ColumnRef* finished_ref(flatbuffers::FlatBufferBuilder& b, uint32_t index,
                                  const char* name) {
  auto text = name ? b.CreateString(name) : flatbuffers::Offset<flatbuffers::String>{};
  b.Finish(fb::CreateColumnRef(b, index, text));
  return flatbuffers::GetRoot<fb::ColumnRef>(b.GetBufferPointer());
}
```

  and after `AstRouting.CudfAstCanEvaluate`:

```cpp
// #164: a ColumnRef is resolved by index AND name, as the planner's own check does
// (expr_physical.rs); each wrong reference has its own message.
TEST(ColumnRefs, TheNamedColumnResolves) {
  flatbuffers::FlatBufferBuilder b;
  EXPECT_EQ(peacock::column_ordinal(finished_ref(b, 1, "m"), {"n", "m"}), 1);
}

TEST(ColumnRefs, AnIndexPastTheWidthIsRefusedNamingBoth) {
  flatbuffers::FlatBufferBuilder b;
  try {
    peacock::column_ordinal(finished_ref(b, 2, "n"), {"n", "m"});
    FAIL() << "an index past the width resolved";
  } catch (std::runtime_error const& e) {
    EXPECT_STREQ(e.what(), "ColumnRef n@2 is past the 2 columns its input has");
  }
}

TEST(ColumnRefs, AReferenceWithNoNameIsRefused) {
  flatbuffers::FlatBufferBuilder b;
  try {
    peacock::column_ordinal(finished_ref(b, 0, nullptr), {"n", "m"});
    FAIL() << "an unnamed reference resolved";
  } catch (std::runtime_error const& e) {
    EXPECT_STREQ(e.what(), "ColumnRef @0 carries no name, so nothing checks it reads the column "
                           "the plan meant");
  }
}

TEST(ColumnRefs, ANameOtherThanTheColumnsIsRefusedNamingBoth) {
  flatbuffers::FlatBufferBuilder b;
  try {
    peacock::column_ordinal(finished_ref(b, 0, "m"), {"n", "m"});
    FAIL() << "a reference named for another column resolved";
  } catch (std::runtime_error const& e) {
    EXPECT_STREQ(e.what(), "ColumnRef m@0 reads n at that position");
  }
}

TEST(ColumnRefs, ColumnAtBorrowsTheColumnTheRefNames) {
  flatbuffers::FlatBufferBuilder b;
  std::vector<cudf::column_view> cols{typed_col(cudf::data_type{cudf::type_id::INT32}),
                                      typed_col(cudf::data_type{cudf::type_id::INT64})};
  auto got = peacock::column_at(cudf::table_view{cols}, {"n", "m"}, finished_ref(b, 1, "m"));
  EXPECT_EQ(got.type().id(), cudf::type_id::INT64);
}
```

  Build: FAIL, `column_ordinal` is not declared.

- [ ] **Step 2: The resolver.** Declare the *Interfaces* pair in `plan_executor_internal.h`
  (after `cudf_ast_can_evaluate`). Create `cpp/src/column_refs.cpp` and add `src/column_refs.cpp`
  to `peacock_gpu`'s sources beside `src/expr.cpp`:

```cpp
// A ColumnRef resolved against the table it reads (#164): by index AND name, as the planner's
// own check does (expr_physical.rs). Every ColumnRef reader in cpp/src comes through here.

#include "plan_executor_internal.h"

#include <stdexcept>
#include <string>

namespace peacock {

cudf::size_type column_ordinal(const fb::ColumnRef* ref, std::vector<std::string> const& names) {
  const auto index = ref->index();
  const std::string name = ref->name() ? ref->name()->str() : std::string();
  if (index >= names.size())
    throw std::runtime_error("ColumnRef " + name + "@" + std::to_string(index) + " is past the " +
                             std::to_string(names.size()) + " columns its input has");
  if (!ref->name())
    throw std::runtime_error("ColumnRef @" + std::to_string(index) +
                             " carries no name, so nothing checks it reads the column the plan "
                             "meant");
  if (name != names[index])
    throw std::runtime_error("ColumnRef " + name + "@" + std::to_string(index) + " reads " +
                             names[index] + " at that position");
  return static_cast<cudf::size_type>(index);
}

cudf::column_view column_at(cudf::table_view const& table,
                            std::vector<std::string> const& names, const fb::ColumnRef* ref) {
  return table.column(column_ordinal(ref, names));
}

}  // namespace peacock
```

  If `plan_executor_internal.h` names the flat-buffer namespace other than `fb` at file scope, use
  the alias `expr.cpp` uses.

- [ ] **Step 3: Run.**

```bash
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
```

  Expected: the five `ColumnRefs.*` PASS; nothing else moves (no reader calls the pair yet).
  No device cycle: the pair is host code, its tests are the cpu tier's, and nothing on the device
  calls it until Task 6.

- [ ] **Step 4: Commit.**

```bash
git add cpp/src/plan_executor_internal.h cpp/src/column_refs.cpp cpp/CMakeLists.txt cpp/tests/cpu/test_executor.cpp
git commit -m "#164: column_ordinal/column_at resolve a ColumnRef by index and name

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: One request builder; inputs read through `args` (#280)

**Files:**
- Modify: `cpp/src/operators/aggregate.cpp` (the grouped and grouping-set paths rewritten; the
  keyless path's argument read)
- Create: `cpp/tests/gpu/test_aggregate_builder.cpp`; Modify: `cpp/CMakeLists.txt`
- Modify: `cpp/tests/gpu/test_plan_executor.cpp` (`nation_aggregate`'s merge args), grouping-id's
  `test_grouping_id.cpp` (`grouping_sets_plan`'s `count` args, if it has none)
- Modify: `peacockdb-core/src/tests/gpu_tests/aggregate_dimension_cases.rs`,
  `aggregate_schema_cases.rs`

**Interfaces:**
- Produces (file-local in `aggregate.cpp`). welford-device consumes four extension points, marked
  ⟵ below: a keyless Welford node routed through `grouped_aggregate` on one constant `INT32` key;
  **two readbacks** that replace a NULL moment with `0.0` — the init's `$mean` and `$m2` (today
  `AsIs`, from `add_requests`' Welford `Partial` arm), and the moments a Merge reads out of
  `MERGE_M2` (today `MergedChild` children 1 and 2); the count type at the one `MERGE_M2` site.

```cpp
/// What a function node runs, by the name state_funcs gives it.
enum class AggKind { Sum, Min, Max, Count, Welford };
static AggKind agg_kind(std::string const& name);  // throws "unsupported aggregate function: <name>"

/// How one state column is read back out of the groupby's results. ⟵ welford-device extends two
/// readbacks to replace a NULL moment with 0.0 (all-NULL groups): the Welford init's `$mean` and
/// `$m2`, read AsIs here, and the merged mean and m2 MergedChild reads out of MERGE_M2's struct.
enum class StateReadback {
  AsIs,
  CountToInt64,  // cuDF's COUNT answers INT32; every count on the wire is INT64
  MergedChild,   // one child of MERGE_M2's struct: the count widened to INT64, a moment as is
};

struct StateColumn {
  std::string name;  // from state_names
  size_t request;    // into AggregateRequests::requests
  size_t result;     // into that request's aggregations
  StateReadback readback;
  int child = -1;    // MergedChild only
};

/// A node's cuDF requests and the state columns they fill, in state order, plus every column a
/// request's view points into.
struct AggregateRequests {
  std::vector<cudf::groupby::aggregation_request> requests;
  std::vector<StateColumn> columns;
  std::vector<std::unique_ptr<cudf::column>> owned;
};

/// Appends `func`'s requests and state columns to `into`, evaluating each computed argument (and
/// a Welford's FLOAT64 cast, a merge's MERGE_M2 struct) into `into.owned` once. Every input is the
/// column its `args` name in `input` (whose columns are `names`), never one located by counting;
/// a function whose args or state_names have the wrong length for its kind and phase, or a
/// `count` at Merge, is refused naming it.
static void add_requests(const fb::AggregateFuncNode* func, AggPhase phase,
                         cudf::table_view input, std::vector<std::string> const& names,
                         AggregateRequests& into);

/// The state columns `built` declared, read out of `results`, appended in order.
static void append_state_columns(AggregateRequests& built,
                                 std::vector<cudf::groupby::aggregation_result>& results,
                                 std::vector<std::unique_ptr<cudf::column>>& columns,
                                 std::vector<std::string>& names);

/// ⟵ welford-device makes this a compile-time choice of the cuDF version (#94).
constexpr cudf::type_id kMergeM2CountType = cudf::type_id::INT32;

/// MERGE_M2's input: a Welford state's three columns as one struct, its count in
/// kMergeM2CountType. The one place a MERGE_M2 input is built.
static std::unique_ptr<cudf::column> merge_m2_input(cudf::column_view count,
                                                    cudf::column_view mean,
                                                    cudf::column_view m2);

/// The groupby over `keys`: the key columns under `key_names`, then every function's state
/// columns.  ⟵ welford-device runs a keyless Welford node through this on one constant INT32
/// key column and drops that column.
static TableResult grouped_aggregate(const fb::CudfAggregate* agg, AggPhase phase,
                                     std::vector<cudf::column_view> const& keys,
                                     std::vector<std::string> key_names, cudf::table_view input,
                                     std::vector<std::string> const& names);

/// One groupby per grouping set over the one set of requests the node's functions build (so a
/// computed argument is evaluated once per node, not once per set), NULL placeholders for the
/// masked keys, grouping_id_column after the keys, concatenated.
static TableResult grouping_set_aggregate(const fb::CudfAggregate* agg, AggPhase phase,
                                          std::vector<cudf::column_view> const& keys,
                                          std::vector<std::string> const& key_names,
                                          cudf::table_view input,
                                          std::vector<std::string> const& names);
```

  The builder's table, which is the spec's:

  | kind | `Partial` | `Merge` | args / state_names |
  |---|---|---|---|
  | `sum`, `min`, `max` | that aggregation over `args[0]` | the same | 1 / 1 |
  | `count` | `COUNT` over `args[0]`, `CountToInt64` | refused: the plan merges a count with `sum` | 1 / 1 |
  | `stddev`, `stddev_pop`, `var`, `var_pop` | `COUNT`, `MEAN`, `M2` over `args[0]` cast to `FLOAT64` | `MERGE_M2` over `merge_m2_input(args[0], args[1], args[2])`, three `MergedChild` | 1 or 3 / 3 |

- [ ] **Step 1: The builder's gtests, failing.** `cpp/tests/gpu/test_aggregate_builder.cpp`
  (add it to `peacock_plan_tests`' sources beside `test_column_refs.cpp`):

```cpp
/// The aggregate's one request builder, per function and phase: what each function emits and
/// under which names, read where its args point, and the shapes it refuses.

#include "hand_tables.hpp"

#include <gtest/gtest.h>

#include <algorithm>
#include <map>
#include <stdexcept>
#include <string>
#include <vector>

namespace {
namespace fb = peacock::plan;
using Offsets = std::vector<flatbuffers::Offset<fb::Expr>>;

flatbuffers::Offset<fb::AggregateFuncNode> func(flatbuffers::FlatBufferBuilder& fbb,
                                                const char* name, Offsets args,
                                                std::vector<std::string> const& state,
                                                int8_t ddof = 0) {
  auto n = fbb.CreateString(name);
  auto a = fbb.CreateVector(args);
  auto s = fbb.CreateVectorOfStrings(state);
  fb::AggregateFuncNodeBuilder b(fbb);
  b.add_name(n);
  b.add_args(a);
  b.add_state_names(s);
  b.add_ddof(ddof);
  return b.Finish();
}

/// A CudfAggregate grouped on `groups`; `sets` non-empty makes it a grouping-set node, with an
/// INT32 NULL placeholder per key.
flatbuffers::Offset<fb::PlanNode> aggregate(
    flatbuffers::FlatBufferBuilder& fbb, fb::AggregateMode mode, Offsets groups,
    std::vector<std::string> const& group_names,
    std::vector<flatbuffers::Offset<fb::AggregateFuncNode>> funcs,
    std::vector<std::vector<uint8_t>> const& sets = {}) {
  auto g = fbb.CreateVector(groups);
  auto gn = fbb.CreateVectorOfStrings(group_names);
  auto f = fbb.CreateVector(funcs);
  Offsets nulls;
  std::vector<flatbuffers::Offset<fb::GroupingSetMask>> masks;
  for (size_t i = 0; !sets.empty() && i < groups.size(); ++i)
    nulls.push_back(hand::null_of(fbb, fb::DataType_Int32));
  for (auto const& set : sets) masks.push_back(fb::CreateGroupingSetMask(fbb, fbb.CreateVector(set)));
  auto n = fbb.CreateVector(nulls);
  auto nn = fbb.CreateVectorOfStrings(sets.empty() ? std::vector<std::string>{} : group_names);
  auto m = fbb.CreateVector(masks);
  fb::CudfAggregateBuilder b(fbb);
  b.add_mode(mode);
  b.add_group_exprs(g);
  b.add_group_names(gn);
  b.add_aggr_funcs(f);
  b.add_null_exprs(n);
  b.add_null_names(nn);
  b.add_grouping_sets(m);
  return fb::CreatePlanNode(fbb, fb::PlanNodeKind_CudfAggregate, b.Finish().Union());
}

/// k: 1 1 2 2 2; v: 10 20 30 40 50.
peacock::TableResult rows() {
  std::vector<std::unique_ptr<cudf::column>> c;
  c.push_back(hand::column_of<int32_t>({1, 1, 2, 2, 2}));
  c.push_back(hand::column_of<int64_t>({10, 20, 30, 40, 50}));
  return hand::table_of(std::move(c), {"k", "v"});
}

std::vector<peacock::TableResult> one(peacock::TableResult t) {
  std::vector<peacock::TableResult> in;
  in.push_back(std::move(t));
  return in;
}

/// Column `at` of a result grouped on an INT32 key, by key: cuDF's groupby orders nothing.
template <typename T>
std::map<int32_t, T> by_key(peacock::TableResult const& r, int at) {
  auto keys = hand::values_of<int32_t>(r.view().column(0));
  auto values = hand::values_of<T>(r.view().column(at));
  std::map<int32_t, T> out;
  for (size_t i = 0; i < keys.size(); ++i) out[keys[i]] = values[i];
  return out;
}

std::string refusal(flatbuffers::FlatBufferBuilder& fbb, flatbuffers::Offset<fb::PlanNode> node,
                    peacock::TableResult input) {
  try {
    hand::run(fbb, node, one(std::move(input)));
  } catch (std::runtime_error const& e) {
    return e.what();
  }
  return "(not refused)";
}

}  // namespace

TEST(AggregateBuilder, PlainFunctionsAreNamedFromTheirStateNames) {
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Partial, {hand::ref(fbb, 0, "k")}, {"k"},
                        {func(fbb, "sum", {hand::ref(fbb, 1, "v")}, {"sum(v)"}),
                         func(fbb, "min", {hand::ref(fbb, 1, "v")}, {"min(v)"}),
                         func(fbb, "max", {hand::ref(fbb, 1, "v")}, {"max(v)"}),
                         func(fbb, "count", {hand::ref(fbb, 1, "v")}, {"count(v)"})});
  auto out = hand::run(fbb, node, one(rows()));
  EXPECT_EQ(out.column_names,
            (std::vector<std::string>{"k", "sum(v)", "min(v)", "max(v)", "count(v)"}));
  EXPECT_EQ(out.view().column(4).type().id(), cudf::type_id::INT64) << "every count is INT64";
  EXPECT_EQ((by_key<int64_t>(out, 1)), (std::map<int32_t, int64_t>{{1, 30}, {2, 120}}));
  EXPECT_EQ((by_key<int64_t>(out, 2)), (std::map<int32_t, int64_t>{{1, 10}, {2, 30}}));
  EXPECT_EQ((by_key<int64_t>(out, 3)), (std::map<int32_t, int64_t>{{1, 20}, {2, 50}}));
  EXPECT_EQ((by_key<int64_t>(out, 4)), (std::map<int32_t, int64_t>{{1, 2}, {2, 3}}));
}

TEST(AggregateBuilder, AWelfordInitEmitsItsTripleUnderItsThreeNames) {
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Partial, {hand::ref(fbb, 0, "k")}, {"k"},
                        {func(fbb, "stddev", {hand::ref(fbb, 1, "v")},
                              {"s$count", "s$mean", "s$m2"}, 1)});
  auto out = hand::run(fbb, node, one(rows()));
  ASSERT_EQ(out.column_names, (std::vector<std::string>{"k", "s$count", "s$mean", "s$m2"}));
  EXPECT_EQ(out.view().column(1).type().id(), cudf::type_id::INT64);
  EXPECT_EQ((by_key<int64_t>(out, 1)), (std::map<int32_t, int64_t>{{1, 2}, {2, 3}}));
  EXPECT_EQ((by_key<double>(out, 2)), (std::map<int32_t, double>{{1, 15.0}, {2, 40.0}}));
  EXPECT_EQ((by_key<double>(out, 3)), (std::map<int32_t, double>{{1, 50.0}, {2, 200.0}}));
}

TEST(AggregateBuilder, AMergeReadsEachStateColumnWhereItsArgPoints) {
  // [k, a, b], merged as sum(b) then max(a): the first function's state is not the column
  // after the keys, so a cursor would hand sum the a's (3, 3) and max the b's (200, 300).
  std::vector<std::unique_ptr<cudf::column>> c;
  c.push_back(hand::column_of<int32_t>({1, 1, 2}));
  c.push_back(hand::column_of<int64_t>({1, 2, 3}));
  c.push_back(hand::column_of<int64_t>({100, 200, 300}));
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Merge, {hand::ref(fbb, 0, "k")}, {"k"},
                        {func(fbb, "sum", {hand::ref(fbb, 2, "b")}, {"b"}),
                         func(fbb, "max", {hand::ref(fbb, 1, "a")}, {"a"})});
  auto out = hand::run(fbb, node, one(hand::table_of(std::move(c), {"k", "a", "b"})));
  EXPECT_EQ(out.column_names, (std::vector<std::string>{"k", "b", "a"}));
  EXPECT_EQ((by_key<int64_t>(out, 1)), (std::map<int32_t, int64_t>{{1, 300}, {2, 300}}));
  EXPECT_EQ((by_key<int64_t>(out, 2)), (std::map<int32_t, int64_t>{{1, 2}, {2, 3}}));
}

TEST(AggregateBuilder, ACountAtMergeIsRefused) {
  // The plan merges a count's state with sum (`count(*): sum(count(*)@1)`), and the cpu's
  // merge refuses a count: one at Merge is a hand-built plan, refused by name, keyed or not.
  for (bool keyed : {true, false}) {
    flatbuffers::FlatBufferBuilder fbb;
    auto count = func(fbb, "count", {hand::ref(fbb, 1, "v")}, {"n"});
    auto node = keyed ? aggregate(fbb, fb::AggregateMode_Merge, {hand::ref(fbb, 0, "k")}, {"k"},
                                  {count})
                      : aggregate(fbb, fb::AggregateMode_Merge, {}, {}, {count});
    auto why = refusal(fbb, node, rows());
    EXPECT_NE(why.find("`count` at Merge"), std::string::npos) << "keyed " << keyed << ": " << why;
  }
}

TEST(AggregateBuilder, AWelfordMergeReadsTheTripleWhereItsArgsPoint) {
  // The state arrives as [k, m2, count, mean]; a merge that counted columns would read m2 as
  // the count. Chan's merge of (2, 1.5, 2) and (4, 4, 8): n 6, mean 19/6,
  // m2 = 2 + 8 + 2.5² · 2·4/6.
  std::vector<std::unique_ptr<cudf::column>> c;
  c.push_back(hand::column_of<int32_t>({1, 1}));
  c.push_back(hand::column_of<double>({2.0, 8.0}));
  c.push_back(hand::column_of<int64_t>({2, 4}));
  c.push_back(hand::column_of<double>({1.5, 4.0}));
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(
      fbb, fb::AggregateMode_Merge, {hand::ref(fbb, 0, "k")}, {"k"},
      {func(fbb, "stddev",
            {hand::ref(fbb, 2, "s$count"), hand::ref(fbb, 3, "s$mean"), hand::ref(fbb, 1, "s$m2")},
            {"s$count", "s$mean", "s$m2"}, 1)});
  auto out = hand::run(fbb, node,
                       one(hand::table_of(std::move(c), {"k", "s$m2", "s$count", "s$mean"})));
  ASSERT_EQ(out.column_names, (std::vector<std::string>{"k", "s$count", "s$mean", "s$m2"}));
  EXPECT_EQ(out.view().column(1).type().id(), cudf::type_id::INT64) << "widened back";
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(1)), std::vector<int64_t>{6});
  EXPECT_NEAR(hand::values_of<double>(out.view().column(2))[0], 19.0 / 6.0, 1e-12);
  EXPECT_NEAR(hand::values_of<double>(out.view().column(3))[0], 10.0 + 6.25 * 8.0 / 6.0, 1e-12);
}

TEST(AggregateBuilder, AGroupingSetBuildsTheWelfordTripleAndWhatFollowsIt) {
  // #280: ROLLUP(k) with a stddev and a sum after it. The grand total is the set with k masked.
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Partial, {hand::ref(fbb, 0, "k")}, {"k"},
                        {func(fbb, "stddev", {hand::ref(fbb, 1, "v")},
                              {"s$count", "s$mean", "s$m2"}, 1),
                         func(fbb, "sum", {hand::ref(fbb, 1, "v")}, {"sum(v)"})},
                        {{0}, {1}});
  auto out = hand::run(fbb, node, one(rows()));
  ASSERT_EQ(out.column_names, (std::vector<std::string>{"k", "__grouping_id", "s$count",
                                                        "s$mean", "s$m2", "sum(v)"}));
  ASSERT_EQ(out.num_rows(), 3) << "two keys and the grand total";
  ASSERT_EQ(out.view().column(1).type().id(), cudf::type_id::UINT8) << "one key: a UInt8 id";
  auto ids = hand::values_of<uint8_t>(out.view().column(1));
  auto counts = hand::values_of<int64_t>(out.view().column(2));
  auto sums = hand::values_of<int64_t>(out.view().column(5));
  ASSERT_EQ(std::count(ids.begin(), ids.end(), uint8_t{0}), 2) << "the set keyed on k: two groups";
  ASSERT_EQ(std::count(ids.begin(), ids.end(), uint8_t{1}), 1) << "the grand total: one row";
  for (size_t row = 0; row < ids.size(); ++row) {
    if (ids[row] != 1) continue;
    EXPECT_EQ(counts[row], 5) << "the grand total counts every row";
    EXPECT_EQ(sums[row], 150) << "and the sum after the triple is its own, not a neighbour";
  }
}

TEST(AggregateBuilder, AWelfordOverZeroRowsIsNoGroupsTypedAsDeclared) {
  for (auto mode : {fb::AggregateMode_Partial, fb::AggregateMode_Merge}) {
    bool merging = mode == fb::AggregateMode_Merge;
    std::vector<std::unique_ptr<cudf::column>> c;
    c.push_back(hand::column_of<int32_t>({}));
    c.push_back(hand::column_of<int64_t>({}));
    c.push_back(hand::column_of<double>({}));
    c.push_back(hand::column_of<double>({}));
    flatbuffers::FlatBufferBuilder fbb;
    Offsets args = merging ? Offsets{hand::ref(fbb, 1, "s$count"), hand::ref(fbb, 2, "s$mean"),
                                     hand::ref(fbb, 3, "s$m2")}
                           : Offsets{hand::ref(fbb, 1, "s$count")};
    auto node = aggregate(fbb, mode, {hand::ref(fbb, 0, "k")}, {"k"},
                          {func(fbb, "stddev", args, {"s$count", "s$mean", "s$m2"}, 1)});
    auto out = hand::run(
        fbb, node, one(hand::table_of(std::move(c), {"k", "s$count", "s$mean", "s$m2"})));
    EXPECT_EQ(out.num_rows(), 0) << fb::EnumNameAggregateMode(mode);
    ASSERT_EQ(out.num_columns(), 4) << fb::EnumNameAggregateMode(mode);
    EXPECT_EQ(out.view().column(1).type().id(), cudf::type_id::INT64);
    EXPECT_EQ(out.view().column(2).type().id(), cudf::type_id::FLOAT64);
    EXPECT_EQ(out.view().column(3).type().id(), cudf::type_id::FLOAT64);
  }
}

TEST(AggregateBuilder, AFunctionGivenTheWrongNumberOfArgsIsRefusedByName) {
  struct Case { const char* name; fb::AggregateMode mode; int args; int states; };
  for (auto const& bad : {Case{"sum", fb::AggregateMode_Partial, 2, 1},
                          Case{"count", fb::AggregateMode_Partial, 0, 1},
                          Case{"stddev", fb::AggregateMode_Merge, 1, 3},
                          Case{"var_pop", fb::AggregateMode_Partial, 3, 3}}) {
    flatbuffers::FlatBufferBuilder fbb;
    Offsets args;
    for (int i = 0; i < bad.args; ++i) args.push_back(hand::ref(fbb, 1, "v"));
    std::vector<std::string> state(bad.states, "s");
    auto node = aggregate(fbb, bad.mode, {hand::ref(fbb, 0, "k")}, {"k"},
                          {func(fbb, bad.name, args, state)});
    auto why = refusal(fbb, node, rows());
    EXPECT_NE(why.find(std::string("`") + bad.name + "`"), std::string::npos) << why;
    EXPECT_NE(why.find("args"), std::string::npos) << why;
  }
}

TEST(AggregateBuilder, AFunctionWhoseStateNamesDoNotMatchItsColumnsIsRefused) {
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Partial, {hand::ref(fbb, 0, "k")}, {"k"},
                        {func(fbb, "stddev", {hand::ref(fbb, 1, "v")}, {"s"}, 1)});
  auto why = refusal(fbb, node, rows());
  EXPECT_NE(why.find("`stddev` emits 3 state columns and names 1"), std::string::npos) << why;
}

TEST(AggregateBuilder, AKeylessNodeNamesItsColumnsFromItsStateNames) {
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Partial, {}, {},
                        {func(fbb, "count", {hand::ref(fbb, 1, "v")}, {"n"}),
                         func(fbb, "sum", {hand::ref(fbb, 1, "v")}, {"total"})});
  auto out = hand::run(fbb, node, one(rows()));
  EXPECT_EQ(out.column_names, (std::vector<std::string>{"n", "total"}));
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(0)), std::vector<int64_t>{5});
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(1)), std::vector<int64_t>{150});
}
```

  Ten cases. They compile now and run in Step 6's device cycle. Red there before Step 3:
  `AMergeReadsEachStateColumnWhereItsArgPoints` and `AWelfordMergeReadsTheTripleWhereItsArgsPoint`
  (the cursor reads the columns after the keys in order), `ACountAtMergeIsRefused` (a count merges
  by `SUM`, grouped and keyless), `AGroupingSetBuildsTheWelfordTripleAndWhatFollowsIt` (one `STD`
  column, so four names where six are declared), `AFunctionGivenTheWrongNumberOfArgsIsRefusedByName`
  (nothing checks an arity) and `AFunctionWhoseStateNamesDoNotMatchItsColumnsIsRefused` (Task 3's
  `state_name` refuses with its own message). Green before Step 3 too, as pins of what the builder
  keeps: `PlainFunctionsAreNamedFromTheirStateNames`, `AWelfordInitEmitsItsTripleUnderItsThreeNames`,
  `AWelfordOverZeroRowsIsNoGroupsTypedAsDeclared` (its merge input is in state order) and
  `AKeylessNodeNamesItsColumnsFromItsStateNames`.

- [ ] **Step 2: The rollup Welford, both engines.** Every `GpuAggregate` here runs
  `Script::Accumulate` (keyless-identity made the init a batch accumulator; its state is slot 0).
  In `aggregate_dimension_cases.rs`:

```rust
/// One key under ROLLUP, then a `stddev(f64)`, a `var_pop(f64)` and a `sum(i64)` after them:
/// the `sum` after the triples is the column #280 shifted. Both triples are over `f64`, which
/// `welford_init` shows has no all-NULL group in `input()` (all-NULL groups are welford-device's).
fn rollup_welford_state() -> Schema {
    let triple = |out: &str| {
        [("$count", DataType::Int64), ("$mean", DataType::Float64), ("$m2", DataType::Float64)]
            .map(|(suffix, ty)| (format!("{out}{suffix}"), ty))
    };
    let mut named: Vec<(String, DataType)> = vec![
        ("key".to_string(), DataType::Int32),
        ("__grouping_id".to_string(), DataType::UInt8),
    ];
    named.extend(triple("stddev(f64)"));
    named.extend(triple("var_pop(f64)"));
    named.push(("sum(i64)".to_string(), DataType::Int64));
    let fields: Vec<(&str, DataType)> = named.iter().map(|(n, t)| (n.as_str(), t.clone())).collect();
    Schema {
        group_keys: vec![0, 1],
        agg_state: vec![
            AggStateColumns { output: "stddev(f64)".into(), func: AggFunc::Stddev, ddof: 1, positions: vec![2, 3, 4] },
            AggStateColumns { output: "var_pop(f64)".into(), func: AggFunc::Var, ddof: 0, positions: vec![5, 6, 7] },
        ],
        ..columns(&fields)
    }
}

pub(crate) fn rollup_welford_init() -> GpuAggregate {
    let triple = |out: &str| -> Vec<AggCall> {
        [(PlanAgg::Count, "$count", DataType::Int64), (PlanAgg::Mean, "$mean", DataType::Float64), (PlanAgg::M2, "$m2", DataType::Float64)]
            .into_iter()
            .map(|(func, suffix, ty)| call(func, Expr::column(4, "f64"), &format!("{out}{suffix}"), ty))
            .collect()
    };
    let mut aggs = triple("stddev(f64)");
    aggs.extend(triple("var_pop(f64)"));
    aggs.push(call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64));
    let state = rollup_welford_state();
    let body = AggregateBody {
        group_by: vec![Expr::column(1, "key")],
        grouping_sets: vec![vec![false], vec![true]],
        null_exprs: vec![Expr::Literal(ScalarValue::Int32(None))],
        aggs,
        finalize: None,
    };
    GpuAggregate::new(
        Given::of(Schema::new(schema()), BatchLayout::MultipleBatches),
        body,
        state.clone(),
        state,
    )
}

fn rollup_welford_merge() -> GpuAggregateBatches {
    let state = rollup_welford_state();
    let field = |at: u32| state.fields.field(at as usize).clone();
    let merged = |at: u32| AggCall {
        func: PlanAgg::MergeM2,
        args: (at..at + 3).map(|i| Expr::column(i, field(i).name())).collect(),
        outputs: (at..at + 3).map(field).collect(),
    };
    let aggs = vec![
        merged(2),
        merged(5),
        call(PlanAgg::Sum, Expr::column(8, "sum(i64)"), "sum(i64)", DataType::Int64),
    ];
    let group_by = vec![Expr::column(0, "key"), Expr::column(1, "__grouping_id")];
    merge_over(state.clone(), body(group_by, aggs, None), state)
}

/// Rows by `__grouping_id`, then `key`, NULLs first: a rollup repeats a key across its sets, so
/// both sides are paired by position once in this order.
fn by_set_then_key(batch: &RecordBatch) -> RecordBatch {
    let order = lexsort_to_indices(
        &[
            SortColumn { values: batch.column(1).clone(), options: None },
            SortColumn { values: batch.column(0).clone(), options: None },
        ],
        None,
    )
    .expect("sortable keys");
    take_record_batch(batch, &order).expect("the rows in that order")
}

const ROLLUP_WELFORD_MOMENTS: [usize; 4] = [3, 4, 6, 7];

operator_case! {
    GpuAggregate,
    fn a_rollup_with_a_stddev_a_var_and_a_sum_agrees() {
        let outcome = run_both(&rollup_welford_init(), Script::Accumulate(vec![input()]));
        same_within_welford(
            &by_set_then_key(cpu_slot(&outcome, 0)),
            &by_set_then_key(gpu_slot(&outcome, 0)),
            false,
            &ROLLUP_WELFORD_MOMENTS,
        );
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_rollup_welford_merge_agrees() {
        let partial = |seed| {
            let init = run_both(&rollup_welford_init(), Script::Accumulate(vec![synthetic(64, seed)]));
            cpu_slot(&init, 0).clone()
        };
        let outcome = run_both(&rollup_welford_merge(), Script::Accumulate(vec![partial(1), partial(2)]));
        same_within_welford(
            &by_set_then_key(cpu_slot(&outcome, 2)),
            &by_set_then_key(gpu_slot(&outcome, 2)),
            false,
            &ROLLUP_WELFORD_MOMENTS,
        );
    }
}
```

  (`use datafusion::arrow::compute::{SortColumn, lexsort_to_indices, take_record_batch};` and the
  `crate::plan`, `given::columns`, `synthetic` names the file lacks; `call`, `body`, `merge_over`,
  `cpu_slot`, `gpu_slot` are `aggregate_cases.rs`'s.) In `aggregate_schema_cases.rs`:

```rust
operator_case! {
    GpuAggregate,
    fn a_rollup_with_a_stddev_a_var_and_a_sum_declares_the_state_the_device_holds() {
        assert_holds_as_declared(&rollup_welford_init(), Script::Accumulate(vec![input()]));
    }
}
```

```bash
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
git grep -n 'Script::Exec' peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs peacockdb-core/src/tests/gpu_tests/aggregate_dimension_cases.rs peacockdb-core/src/tests/gpu_tests/aggregate_schema_cases.rs
```

  Expected: compiles; the grep prints nothing (keyless-identity swept the three files, and nothing
  here writes `Script::Exec` back). Red on the device before Step 3:
  `a_rollup_with_a_stddev_a_var_and_a_sum_agrees` and
  `a_rollup_with_a_stddev_a_var_and_a_sum_declares_the_state_the_device_holds` (the grouping-set
  path answers `[key, id, STD, VARIANCE, sum]`). `a_rollup_welford_merge_agrees` is a pin, green
  before and after: its node is a plain grouped merge, its partials are the cpu's, and the state
  arrives in order, which the cursor read right.

- [ ] **Step 3: The builder.** In `aggregate.cpp`, delete `make_agg`, `OutBuild`, `arg_col`, the
  grouped section and the grouping-set block, and write the *Interfaces* declarations with these
  bodies (file-local types in an anonymous namespace, functions `static`, as the file's idiom):

```cpp
static AggKind agg_kind(std::string const& name) {
  if (name == "sum") return AggKind::Sum;
  if (name == "min") return AggKind::Min;
  if (name == "max") return AggKind::Max;
  if (name == "count") return AggKind::Count;
  if (name == "stddev" || name == "stddev_pop" || name == "var" || name == "var_pop")
    return AggKind::Welford;
  throw std::runtime_error("unsupported aggregate function: " + name);
}

// What `func` reads and emits, checked against its kind: a node's state is never located by
// counting, so a function with an arity of its own would read or name the wrong columns.
static void check_shape(const fb::AggregateFuncNode* func, std::string const& name, AggKind kind,
                        AggPhase phase) {
  if (kind == AggKind::Count && phase == AggPhase::Merge)
    throw std::runtime_error("CudfAggregate: `count` at Merge: the plan merges a count's state "
                             "with `sum`, so no count reaches a Merge");
  const bool welford = kind == AggKind::Welford;
  const size_t want_args = welford && phase == AggPhase::Merge ? 3 : 1;
  const size_t args = func->args() ? func->args()->size() : 0;
  if (args != want_args)
    throw std::runtime_error("CudfAggregate: `" + name + "` at " +
                             (phase == AggPhase::Merge ? "Merge" : "Partial") + " reads " +
                             std::to_string(want_args) + " args and was given " +
                             std::to_string(args));
  const size_t want_states = welford ? 3 : 1;
  const size_t states = func->state_names() ? func->state_names()->size() : 0;
  if (states != want_states)
    throw std::runtime_error("CudfAggregate: `" + name + "` emits " +
                             std::to_string(want_states) + " state columns and names " +
                             std::to_string(states));
}

// The column `arg` names in `input`, borrowed, or the column it computes over `input`, kept in
// `owned` for as long as the requests view it.
static cudf::column_view value_of(const fb::Expr* arg, cudf::table_view input,
                                  std::vector<std::string> const& names,
                                  std::vector<std::unique_ptr<cudf::column>>& owned) {
  if (arg->node_type() == fb::ExprNode_ColumnRef)
    return column_at(input, names, arg->node_as_ColumnRef());
  owned.push_back(build_column(arg, input));
  return owned.back()->view();
}

static std::unique_ptr<cudf::column> merge_m2_input(cudf::column_view count,
                                                    cudf::column_view mean,
                                                    cudf::column_view m2) {
  // A struct owns its children, so the moments are copied in; small beside the input.
  std::vector<std::unique_ptr<cudf::column>> members;
  members.push_back(cudf::cast(count, cudf::data_type{kMergeM2CountType}));
  members.push_back(std::make_unique<cudf::column>(mean));
  members.push_back(std::make_unique<cudf::column>(m2));
  return cudf::make_structs_column(count.size(), std::move(members), 0, rmm::device_buffer{});
}

static void add_requests(const fb::AggregateFuncNode* func, AggPhase phase,
                         cudf::table_view input, std::vector<std::string> const& names,
                         AggregateRequests& into) {
  const std::string name = func->name() ? func->name()->str() : std::string();
  const AggKind kind = agg_kind(name);
  check_shape(func, name, kind, phase);
  auto state = [&](flatbuffers::uoffset_t i) { return func->state_names()->Get(i)->str(); };
  auto arg = [&](flatbuffers::uoffset_t i) {
    return value_of(func->args()->Get(i), input, names, into.owned);
  };
  const size_t r = into.requests.size();
  cudf::groupby::aggregation_request req;
  switch (kind) {
    case AggKind::Sum:
      req.values = arg(0);
      req.aggregations.push_back(cudf::make_sum_aggregation<cudf::groupby_aggregation>());
      into.columns.push_back({state(0), r, 0, StateReadback::AsIs});
      break;
    case AggKind::Min:
      req.values = arg(0);
      req.aggregations.push_back(cudf::make_min_aggregation<cudf::groupby_aggregation>());
      into.columns.push_back({state(0), r, 0, StateReadback::AsIs});
      break;
    case AggKind::Max:
      req.values = arg(0);
      req.aggregations.push_back(cudf::make_max_aggregation<cudf::groupby_aggregation>());
      into.columns.push_back({state(0), r, 0, StateReadback::AsIs});
      break;
    case AggKind::Count:
      // Partial only: check_shape refused a count at Merge, whose state the plan merges by sum.
      req.values = arg(0);
      req.aggregations.push_back(cudf::make_count_aggregation<cudf::groupby_aggregation>());
      into.columns.push_back({state(0), r, 0, StateReadback::CountToInt64});
      break;
    case AggKind::Welford:
      if (phase == AggPhase::Partial) {
        // DataFusion's stddev and var are float-valued: the moments of the value as FLOAT64.
        into.owned.push_back(cudf::cast(arg(0), cudf::data_type{cudf::type_id::FLOAT64}));
        req.values = into.owned.back()->view();
        req.aggregations.push_back(cudf::make_count_aggregation<cudf::groupby_aggregation>());
        req.aggregations.push_back(cudf::make_mean_aggregation<cudf::groupby_aggregation>());
        req.aggregations.push_back(cudf::make_m2_aggregation<cudf::groupby_aggregation>());
        into.columns.push_back({state(0), r, 0, StateReadback::CountToInt64});
        // ⟵ welford-device: the init's mean and m2, NULL for an all-NULL group, read back as 0.0.
        into.columns.push_back({state(1), r, 1, StateReadback::AsIs});
        into.columns.push_back({state(2), r, 2, StateReadback::AsIs});
      } else {
        into.owned.push_back(merge_m2_input(arg(0), arg(1), arg(2)));
        req.values = into.owned.back()->view();
        req.aggregations.push_back(cudf::make_merge_m2_aggregation<cudf::groupby_aggregation>());
        // ⟵ welford-device: children 1 and 2, the merged mean and m2, NULL where every merged
        // count is 0, read back as 0.0 (in append_state_columns' MergedChild arm).
        for (int child = 0; child < 3; ++child)
          into.columns.push_back({state(child), r, 0, StateReadback::MergedChild, child});
      }
      break;
  }
  into.requests.push_back(std::move(req));
}

static std::unique_ptr<cudf::column> as_int64_count(cudf::column_view count) {
  return count.type().id() == cudf::type_id::INT32
             ? cudf::cast(count, cudf::data_type{cudf::type_id::INT64})
             : std::make_unique<cudf::column>(count);
}

static void append_state_columns(AggregateRequests& built,
                                 std::vector<cudf::groupby::aggregation_result>& results,
                                 std::vector<std::unique_ptr<cudf::column>>& columns,
                                 std::vector<std::string>& names) {
  for (auto const& state : built.columns) {
    auto& result = results.at(state.request).results.at(state.result);
    switch (state.readback) {
      case StateReadback::AsIs:
        columns.push_back(std::move(result));
        break;
      case StateReadback::CountToInt64:
        columns.push_back(result->type().id() == cudf::type_id::INT32
                              ? cudf::cast(result->view(), cudf::data_type{cudf::type_id::INT64})
                              : std::move(result));
        break;
      case StateReadback::MergedChild:
        // Three columns read one struct, so by view; its count comes back as kMergeM2CountType,
        // and as_int64_count copies a moment as it is.
        columns.push_back(as_int64_count(result->view().child(state.child)));
        break;
    }
    names.push_back(state.name);
  }
}

static TableResult grouped_aggregate(const fb::CudfAggregate* agg, AggPhase phase,
                                     std::vector<cudf::column_view> const& keys,
                                     std::vector<std::string> key_names, cudf::table_view input,
                                     std::vector<std::string> const& names) {
  AggregateRequests built;
  if (agg->aggr_funcs())
    for (auto const* func : *agg->aggr_funcs()) add_requests(func, phase, input, names, built);
  // INCLUDE: SQL puts NULL keys in a group of their own; cuDF's default drops those rows
  // (tpcds q15's NULL ca_zip).
  cudf::groupby::groupby gb{cudf::table_view{keys}, cudf::null_policy::INCLUDE};
  auto [group_keys, results] = gb.aggregate(built.requests);
  auto columns = group_keys->release();
  append_state_columns(built, results, columns, key_names);
  return TableResult::owning(std::make_unique<cudf::table>(std::move(columns)),
                             std::move(key_names));
}

static TableResult grouping_set_aggregate(const fb::CudfAggregate* agg, AggPhase phase,
                                          std::vector<cudf::column_view> const& keys,
                                          std::vector<std::string> const& key_names,
                                          cudf::table_view input,
                                          std::vector<std::string> const& names) {
  auto* sets = agg->grouping_sets();
  const auto nkeys = static_cast<cudf::size_type>(keys.size());
  if (agg->null_exprs()->size() != static_cast<flatbuffers::uoffset_t>(nkeys))
    throw std::runtime_error("grouping sets: null_exprs length != group_exprs");
  // A masked position contributes an all-NULL key of its type, concatenable with the real one.
  std::vector<std::unique_ptr<cudf::column>> placeholders;
  for (cudf::size_type i = 0; i < nkeys; ++i)
    placeholders.push_back(build_column(agg->null_exprs()->Get(i), input));
  // groupby::aggregate reads its requests const, so the node's one set of requests serves every
  // grouping set: each computed argument, Welford cast and MERGE_M2 struct is built once per node,
  // not once per set (a CUBE over n keys has 2^n sets).
  AggregateRequests built;
  if (agg->aggr_funcs())
    for (auto const* func : *agg->aggr_funcs()) add_requests(func, phase, input, names, built);
  std::vector<std::unique_ptr<cudf::table>> set_tables;
  std::vector<std::string> out_names;
  for (auto const* set : *sets) {
    auto* mask = set->values();
    if (!mask || mask->size() != static_cast<flatbuffers::uoffset_t>(nkeys))
      throw std::runtime_error("grouping set mask length != group_exprs length");
    std::vector<cudf::column_view> set_keys;
    for (cudf::size_type i = 0; i < nkeys; ++i)
      set_keys.push_back(mask->Get(i) ? placeholders[i]->view() : keys[i]);
    cudf::groupby::groupby gb{cudf::table_view{set_keys}, cudf::null_policy::INCLUDE};
    auto [group_keys, results] = gb.aggregate(built.requests);
    const auto rows = group_keys->num_rows();
    auto columns = group_keys->release();
    columns.push_back(grouping_id_column(mask, nkeys, rows));
    out_names = key_names;
    out_names.push_back("__grouping_id");
    append_state_columns(built, results, columns, out_names);
    set_tables.push_back(std::make_unique<cudf::table>(std::move(columns)));
  }
  std::vector<cudf::table_view> views;
  for (auto const& t : set_tables) views.push_back(t->view());
  return TableResult::owning(cudf::concatenate(views), std::move(out_names));
}
```

  `grouping_id_column(mask, nkeys, rows)` takes the set's `const flatbuffers::Vector<uint8_t>*`,
  as grouping-id landed it. `append_state_columns` moves each set's own results and only reads
  `built.columns`, so calling it once per set over the one `built` is sound. The keyless loop
  reads each argument with `check_shape(func, name, agg_kind(name), phase)` then
  `value_of(func->args()->Get(0), tv, input.column_names, owned)` in place of `arg_col`, and its
  `count` `Merge` arm (the `cudf::reduce` SUM Task 4 left) goes: `check_shape` refuses a count at
  Merge first. Its `var` still falls to `make_reduce_agg`'s "unsupported aggregate function: var",
  which `bug_a_keyless_var_merge_is_refused_as_unsupported_on_the_device` pins; its decimal `sum`
  uses `cudf::make_sum_aggregation<cudf::groupby_aggregation>()` directly. `execute_aggregate`
  becomes:

```cpp
TableResult execute_aggregate(const fb::CudfAggregate* agg, NodeInputs* in) {
  const AggPhase phase = agg_phase(agg->mode());
  // keyless-identity's no-input arm, exactly as it landed: a done call handed nothing
  // aggregates the zero-row table of aggr_input_schema.
  auto input = was_handed_nothing(in) ? zero_rows_of(agg) : take_input(in);
  auto tv = input.view();
  std::vector<cudf::column_view> keys;
  std::vector<std::string> key_names;
  if (agg->group_exprs()) {
    for (flatbuffers::uoffset_t i = 0; i < agg->group_exprs()->size(); ++i) {
      auto* expr = agg->group_exprs()->Get(i);
      if (expr->node_type() != fb::ExprNode_ColumnRef)
        throw std::runtime_error("CudfAggregate: only ColumnRef group exprs supported");
      auto* ref = expr->node_as_ColumnRef();
      keys.push_back(column_at(tv, input.column_names, ref));
      key_names.push_back(agg->group_names() && i < agg->group_names()->size()
                              ? agg->group_names()->Get(i)->str()
                              : ref->name()->str());
    }
  }
  if (keys.empty()) return keyless_aggregate(agg, phase, tv, input.column_names);
  // Non-empty null_exprs, not grouping_sets, is the discriminator: a plain GROUP BY still writes
  // one all-false mask and no null_exprs.
  if (agg->null_exprs() && agg->null_exprs()->size() > 0 && agg->grouping_sets() &&
      agg->grouping_sets()->size() > 0)
    return grouping_set_aggregate(agg, phase, keys, key_names, tv, input.column_names);
  return grouped_aggregate(agg, phase, keys, std::move(key_names), tv, input.column_names);
}
```

  with the keyless loop moved into `static TableResult keyless_aggregate(agg, phase, input, names)`.
  Delete `is_stddev_name`/`is_var_name` if the keyless arm's own name test (`name == "stddev" ||
  name == "stddev_pop"`) is all that is left. Keep the file under 1000 lines and each function
  under 150.

- [ ] **Step 4: The hand-built merges read by reference.** In `test_plan_executor.cpp`,
  `nation_aggregate`'s merge writes one `ColumnRef` per state column: `stddev` reads
  `{make_col_ref(fbb, 1, "state$count"), make_col_ref(fbb, 2, "state$mean"),
  make_col_ref(fbb, 3, "state$m2")}`; a one-column merge (`sum`, which also merges a count's
  state since Task 4) reads `make_col_ref(fbb, 1, "state")`; the partial reads
  `make_col_ref(fbb, 0, "n_nationkey")` as before. Delete its comment that a merge "reads its state
  columns positionally". The arity check refuses a function node with no `args`, so every
  hand-built `count` carries one, as the planner writes `count(*)` — `count(1)`
  (`recipe-payloads.txt`: `count(*): count(1)`):

```bash
git grep -nE '"count"' cpp/tests/gpu
```

  - grouping-id's `grouping_sets_plan` (`test_grouping_id.cpp`): if its `count` has no `args`, it
    gains `{make_col_ref(fbb, 0, "n_nationkey")}` (or `hand::ref(fbb, 0, "n_nationkey")`, whichever
    the file builds references with), built before the builder opens, and
    `count.add_args(count_args);` — the ids it asserts do not depend on what is counted;
  - keyless-identity's `unfed_aggregate` (`test_aggregate_no_input.cpp`) already writes
    `func_over(fbb, "count", int64_literal(fbb, 1), "n")`; if the landed file still builds
    `count(*)` with no argument, it takes that call. The expectations stand: `count(1)` over the
    zero-row table counts 0 rows, keyless, and a grouped node has no groups.

  Every other hit already passes one argument (`nation_aggregate`, `AggregateCount`,
  `AggregateGroupBy` since Task 4, the builder file's).
- [ ] **Step 5: Build and run what runs locally.**

```bash
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests::
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
git grep -nE 'in_off|make_agg|OutBuild|struct_child' cpp/src/operators/aggregate.cpp
wc -l cpp/src/operators/aggregate.cpp
```

  Expected: clean; PASS; the grep prints nothing; under 1000 lines. `git status --short testdata/`
  is clean: no wire change here.

- [ ] **Step 6: The device cycle**, `FIX=cpp/src/operators/aggregate.cpp`: the red build is the
  ten gtests, the three rollup cases and Step 4's test edits over the aggregate Task 4 left (Task 5
  did not touch it). Red set:

```bash
timeout 3600 ssh $H "$R cpp/install/bin/peacock_plan_tests --gtest_filter='AggregateBuilder.*'"
timeout 3600 ssh $H "$R cpp/install/rust-tests/peacockdb_core_gpu_lib --test-threads=1 a_rollup_with_a_stddev a_rollup_welford_merge"
```

  Expected: the six `AggregateBuilder` cases and two rollup cases Steps 1 and 2 name red, each for
  the reason named there; the four builder pins and `a_rollup_welford_merge_agrees` green. Green
  set (the builder is every grouped aggregate's path, so the whole device corpus runs, written
  nowhere):

```bash
timeout 3600 ssh $H "$R cpp/install/bin/peacock_plan_tests"
timeout 1800 ssh $H "$R cpp/install/bin/peacock_gpu_tests"
timeout 7200 ssh $H "$R cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests:: --test-threads=1"
timeout 14400 ssh $H "$R cpp/install/rust-tests/test_gpu_corpus --test-threads=1"
```

  Expected: all green — the ten `AggregateBuilder`, `AggregateMerge.*`, `GroupingId.*` and
  `AggregateNoInput.*` among the gtests; the three rollup cases among the Rust ones; every enabled
  corpus cell as before.

- [ ] **Step 7: Commit.**

```bash
git add cpp peacockdb-core/src/tests/gpu_tests llm-wiki/tasks/aggregate-arms-detail.md
git commit -m "#280: one request builder for the grouped and grouping-set paths

Every input read through args by column_at, once per node; a Welford
under a grouping set is the declared triple; a count at Merge is refused.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: A union branch named otherwise is projected to the union's names

The device's schema validator never runs at a forwarder: a union hands the node above each
branch's batches as they are, and that node's references are named from the union's schema. So a
branch whose names differ from the union's and whose types match would meet Task 8's name check
under its own names. `cast_branch` already projects a branch whose types differ; it now projects
one whose names differ, with bare references renamed. SQL never reaches the shape — DataFusion's
union coercion aliases each branch to the union's names in a projection
(`a_union_forwards_its_branches_into_one_lane_numbering`'s second branch is that `Project`), and no
plan golden has a union whose branch names differ — so no golden moves, and the test builds the
union by hand. Today such a tree is refused at planning by the union's own validator
(`check_branch_schemas` holds names as well as types); after this task it plans.

**Files:**
- Modify: `peacockdb-core/src/planner/translator/nodes.rs` (`cast_branch`, `branches`' doc)
- Test: `peacockdb-core/src/planner/translator/schema_tests.rs`

- [ ] **Step 1: The case, failing.** In `schema_tests.rs`, after
  `a_union_branch_is_cast_to_the_declared_output_by_a_project` (add
  `use datafusion::physical_plan::ExecutionPlan;` and
  `use datafusion::physical_plan::union::UnionExec;`):

```rust
#[tokio::test]
async fn a_union_branch_named_otherwise_is_renamed_by_a_project() {
    // SQL never reaches this shape — DataFusion aliases a union's branches to its names itself —
    // so the union is built by hand: the second branch holds `n_regionkey` where the union
    // declares `n_nationkey`, at the same type. A union forwards its branches' batches as they
    // are, so the node above would meet the branch's name under a reference to the union's.
    let union: Arc<dyn ExecutionPlan> = Arc::new(UnionExec::new(vec![
        physical_plan_for("SELECT n_nationkey FROM nation", 1).await,
        physical_plan_for("SELECT n_regionkey FROM nation", 1).await,
    ]));
    let tree = Translator::new(1, Batching::Off)
        .translate(&union)
        .expect("translate the plan");
    let union = tree.children()[0];
    let declared = types_of(union);
    assert_eq!(declared, vec![("n_nationkey".to_string(), DataType::Int32)]);
    let branches = union.children();
    assert!(
        !matches!(as_node_ref(branches[0]), NodeRef::Project(_)),
        "a branch already under the union's names and types is left as it is"
    );
    let NodeRef::Project(project) = as_node_ref(branches[1]) else {
        panic!("the branch named otherwise is renamed by a project");
    };
    assert_eq!(
        project.exprs[0].expr,
        Expr::column(0, "n_regionkey"),
        "a bare reference: nothing to cast where the types agree"
    );
    assert_eq!(types_of(branches[1]), declared, "the project emits the union's names");
    union
        .validate_schemas_and_partitions()
        .expect("every branch now declares the union's names");
}
```

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::translator::schema_tests::a_union_branch
```

  Expected: `a_union_branch_named_otherwise_is_renamed_by_a_project` FAILS with "the branch named
  otherwise is renamed by a project" (the branch is the scan itself); the type case still passes.

- [ ] **Step 2: Project on a name as on a type.** In `cast_branch` (`nodes.rs`), `differs` becomes:

```rust
    // A type that differs needs a cast, and a name that differs a rename: a union forwards its
    // branches' batches as they are, and the node above reads them by the union's names.
    let differs = schema
        .fields()
        .iter()
        .zip(declared.fields().iter())
        .any(|(field, out)| field.data_type() != out.data_type() || field.name() != out.name());
```

  The rest of the function already writes a bare `Expr::column` where the type agrees, named
  `out.name()`. `branches`' doc comment gains: "A branch named otherwise than the union is projected
  too, its references renamed: nothing re-checks a forwarder's names on the device."

- [ ] **Step 3: Run, and every plan as before.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::translator planner::tests::plan_goldens
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
git status --short testdata/
```

  Expected: PASS; `testdata/` clean — no corpus union has a branch named otherwise, so no plan
  golden gains a project. No device cycle: the change is the planner's, and its effect on the
  device is Task 8's (and its whole-corpus run's) to show.

- [ ] **Step 4: Commit.**

```bash
git add peacockdb-core/src/planner/translator/nodes.rs peacockdb-core/src/planner/translator/schema_tests.rs
git commit -m "planner: a union branch named otherwise is projected to the union's names

A forwarder hands the node above each branch's batches as they are, so the
names it reads must be the union's before every ColumnRef is checked (#164).

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Every other `ColumnRef` reader resolves through `column_at` (#164)

**Files:**
- Modify: `cpp/src/plan_executor_internal.h`, `cpp/src/peacock/expr.h`, `cpp/src/expr.cpp`
- Modify: `cpp/src/operators/{filter,project,sort,window,aggregate,join_session}.cpp`,
  `cpp/src/node_session.cpp`
- Modify: `cpp/tests/cpu/test_executor.cpp`, `cpp/tests/gpu/test_plan_executor.cpp` (and every
  gtest file calling `make_col_ref`), `cpp/tests/gpu/test_join_session.cpp`,
  `cpp/tests/gpu/test_column_refs.cpp`
- Modify: `peacockdb-core/src/tests/gpu_tests/aggregate_dimension_cases.rs`
- Modify, only if the device corpus refuses a cell: `peacockdb-core/tests/common/corpus_cases.inc`,
  `testdata/cost-registry.csv`, `llm-wiki/tickets/`

**Interfaces:**
- Produces (the names are the table's, one per column; `cudf_ast_can_evaluate` resolves every
  `ColumnRef` of an expression it answers `true` for, which is what lets `build_expr`'s AST arm read
  the index unchecked):

```cpp
bool cudf_ast_can_evaluate(const fb::Expr* expr, cudf::table_view const& table,
                           std::vector<std::string> const& names);
EvaluatedColumn evaluate_column(const fb::Expr* expr, cudf::table_view const& table,
                                std::vector<std::string> const& names);
std::unique_ptr<cudf::column> build_column(const fb::Expr* expr, cudf::table_view const& table,
                                           std::vector<std::string> const& names);
```

- [ ] **Step 1: The routes, failing.** In `test_column_refs.cpp`:

```cpp
namespace {
namespace fb = peacock::plan;

/// [n, m], both INT64.
peacock::TableResult nm() {
  std::vector<std::unique_ptr<cudf::column>> c;
  c.push_back(hand::column_of<int64_t>({3, 1, 2}));
  c.push_back(hand::column_of<int64_t>({30, 10, 20}));
  return hand::table_of(std::move(c), {"n", "m"});
}

struct BadRef { uint32_t index; const char* name; const char* says; };
const BadRef kBadRefs[] = {
    {2, "n", "ColumnRef n@2 is past the 2 columns its input has"},
    {0, nullptr, "ColumnRef @0 carries no name"},
    {0, "m", "ColumnRef m@0 reads n at that position"},
};

/// A sort of nm() on `key`.
std::string sort_refusal(flatbuffers::FlatBufferBuilder& fbb, flatbuffers::Offset<fb::Expr> key) {
  auto spec = fb::CreateSortExprNode(fbb, key, /*asc=*/true, /*nulls_first=*/false);
  auto specs = fbb.CreateVector(std::vector<flatbuffers::Offset<fb::SortExprNode>>{spec});
  fb::CudfSortBuilder b(fbb);
  b.add_exprs(specs);
  b.add_fetch(-1);
  auto node = fb::CreatePlanNode(fbb, fb::PlanNodeKind_CudfSort, b.Finish().Union());
  std::vector<peacock::TableResult> in;
  in.push_back(nm());
  try {
    hand::run(fbb, node, std::move(in));
  } catch (std::runtime_error const& e) {
    return e.what();
  }
  return "(not refused)";
}
}  // namespace

TEST(ColumnRefs, ASortKeyRefusesEachBadReference) {
  for (auto const& bad : kBadRefs) {
    flatbuffers::FlatBufferBuilder fbb;
    auto why = sort_refusal(fbb, hand::ref(fbb, bad.index, bad.name));
    EXPECT_NE(why.find(bad.says), std::string::npos) << why;
  }
}

TEST(ColumnRefs, AComputedKeyRefusesEachBadReferenceInsideIt) {
  // CAST(ref AS FLOAT64): not a bare ColumnRef, so the column path evaluates it, and the
  // reference inside resolves through the same check.
  for (auto const& bad : kBadRefs) {
    flatbuffers::FlatBufferBuilder fbb;
    auto inner = hand::ref(fbb, bad.index, bad.name);
    auto cast = fb::CreateCastExprNode(fbb, inner, fb::DataType_Float64);
    auto key = fb::CreateExpr(fbb, fb::ExprNode_CastExprNode, cast.Union());
    auto why = sort_refusal(fbb, key);
    EXPECT_NE(why.find(bad.says), std::string::npos) << why;
  }
}

TEST(ColumnRefs, AWellNamedSortKeyStillSorts) {
  flatbuffers::FlatBufferBuilder fbb;
  auto why = sort_refusal(fbb, hand::ref(fbb, 1, "m"));
  EXPECT_EQ(why, "(not refused)");
}
```

  (If `CreateCastExprNode` takes the decimal fields positionally, pass them as `0`.) In
  `test_executor.cpp`, after the `ColumnRefs` cases:

```cpp
TEST(ColumnRefs, TheAstRouteResolvesAReferenceUnderAnOperator) {
  // make_binary writes l@0 < r@1; the names here say r is at 0.
  flatbuffers::FlatBufferBuilder b;
  auto buf = make_binary(b, fb::BinaryOp_Lt);
  auto* expr = flatbuffers::GetRoot<fb::Expr>(buf.data());
  std::vector<cudf::column_view> cols{typed_col(cudf::data_type{cudf::type_id::INT32}),
                                      typed_col(cudf::data_type{cudf::type_id::INT32})};
  EXPECT_THROW(peacock::cudf_ast_can_evaluate(expr, cudf::table_view{cols}, {"r", "l"}),
               std::runtime_error);
  EXPECT_TRUE(peacock::cudf_ast_can_evaluate(expr, cudf::table_view{cols}, {"l", "r"}));
}
```

  In `test_join_session.cpp`, using its own harness as join-session-cpp wrote it (its spec struct
  with `.filter_columns = {{side, index}, …}` and its runner): an Inner join with a residual
  `lv < rv` over left `[lk, lv]` and right `[rk, rv]`, `filter_columns = {{Left, 1}, {Right, 1}}`,
  the residual's references named `lv` and `rv`, answers the rows where `lv < rv`; the same with the
  first reference named `rv` is refused with `"ColumnRef rv@0 reads lv at that position"`. Name it
  `ColumnRefs.AJoinResidualResolvesThroughItsSidesNames`.

  Build: FAIL, `cudf_ast_can_evaluate` takes two arguments.

- [ ] **Step 2: Thread the names through `expr.cpp`.** The three declarations above take
  `std::vector<std::string> const& names` after the table; so do the file-local
  `infer_expr_type`, `make_column`, `build_column_binary`, `build_column_scalar_fn` and
  `build_column_case`, each passing it on. `eval_ast_subtree` does not: it runs only after
  `cudf_ast_can_evaluate` answered `true` for the same expression and table. Then:
  - `evaluate_column`'s `ColumnRef` arm: `return {nullptr, column_at(table, names, expr->node_as_ColumnRef())};`
  - `infer_expr_type`'s `ColumnRef` arm:
    `return column_at(table, names, expr->node_as_ColumnRef()).type().id();` — the `EMPTY` answer
    for an out-of-range index goes; update the function's comment (it throws on a bad reference;
    `EMPTY` is left for shapes it cannot type);
  - `cudf_ast_can_evaluate` gains, before `default`:

```cpp
    case fb::ExprNode_ColumnRef:
      // Resolved here so every reference of an expression answered true has been checked:
      // build_expr's AST arm reads the index alone.
      column_ordinal(expr->node_as_ColumnRef(), names);
      return true;
```

- [ ] **Step 3: Every caller passes the names it reads against.**
  - `filter.cpp`: `cudf_ast_can_evaluate(pred, tv, input.column_names)`, `evaluate_column(pred, tv, input.column_names)`;
  - `project.cpp`: the bare-`ColumnRef` arm `auto idx = column_ordinal(expr->node_as_ColumnRef(), input.column_names);`
    before it shares `input.owners.at(idx)`; the computed arm passes `input.column_names`; the
    empty-projection placeholder arm reads no reference and stays;
  - `sort.cpp`: `evaluate_column(expr, tv, input.column_names)`;
  - `window.cpp`: every `evaluate_column` / `build_column` passes `input.column_names`;
  - `aggregate.cpp`: `value_of`'s and the placeholders' `build_column` pass `names`;
  - `node_session.cpp`: the sort-preserving merge's keys
    `key_cols.push_back(column_ordinal(expr->node_as_ColumnRef(), owned[0].column_names));` and the
    repartition's hash keys `column_ordinal(e->node_as_ColumnRef(), <its input's names>)`
    (re-locate both by `CudfSortPreservingMerge` and `hash_exprs`);
  - `join_session.cpp`: the build and probe keys `column_ordinal(k->left()->node_as_ColumnRef(), B.column_names)`
    and `column_ordinal(k->right()->node_as_ColumnRef(), P.column_names)` where it fills
    `bkeys`/`pkeys`; every table it evaluates the residual over in filter-schema order gets the
    names in the same order, from one helper in that file:

```cpp
// The names of a filter-schema table: entry i of `columns` names its side and ordinal, and the
// residual's ColumnRef(i) carries that column's name (the validator holds it to it,
// plan/join.rs check_filter_columns).
static std::vector<std::string> filter_schema_names(JoinFilterColMap columns,
                                                    std::vector<std::string> const& build,
                                                    std::vector<std::string> const& probe) {
  std::vector<std::string> names;
  for (auto const& c : columns)
    names.push_back(c.side() == fb::JoinSide_Right ? probe.at(c.index()) : build.at(c.index()));
  return names;
}
```

    (Which side is `Left` in the session's terms is join-session-cpp's convention; follow it.)
    `build_expr`'s join arm keeps its bounds check against the map and reads no names.
  - Tests: `test_executor.cpp`'s `AstRouting.CudfAstCanEvaluate` passes `{"l", "r"}` and `{"c"}`.
    In `test_plan_executor.cpp`, remove `make_col_ref`'s `= nullptr` default; each call the compiler
    then names, in that file or any gtest file that shares the helper, takes the name of the column
    at that ordinal of its input (on master: the join keys
    `n_regionkey` / `r_regionkey`, the sorts after nation's projection `n_name`, the project of
    region `r_name` / `r_regionkey`, the sqrt and CASE cases `r_regionkey`, the date_part early
    `n_nationkey` and its `d`). `test_join_session.cpp`'s harness writes its filter references with
    the side column's name.
- [ ] **Step 4: One device pin's message moves.** The finalize above the keyless stddev merge now
  meets a reference past its one column at `column_at`:
  `bug_a_global_stddev_finalize_is_refused_on_the_device` asserts
  `outcome.gpu_refuses().contains("is past the 1 columns its input has")`.
- [ ] **Step 5: Build and run what runs locally.**

```bash
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests::
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
git grep -nE 'node_as_ColumnRef\(\)->index\(\)' cpp/src
```

  Expected: clean; PASS (`TheAstRouteResolvesAReferenceUnderAnOperator` included); the grep lists
  only `column_ordinal` itself and `build_expr`'s two arms (the AST arm, guarded as Step 2 says,
  and the join-map arm).

- [ ] **Step 6: `expr.cpp` under the cap.**

```bash
wc -l cpp/src/expr.cpp
```

  Expected: at most 1000. If the names threaded through it took it past 1000, move
  `build_column_scalar_fn` (its largest function, ~145 lines on master) into a new
  `cpp/src/expr_scalar_fn.cpp` in `namespace peacock`, declared non-static in
  `plan_executor_internal.h` beside `cudf_ast_can_evaluate`, with the includes it uses, and add
  `src/expr_scalar_fn.cpp` to `peacock_gpu`'s sources; rebuild and rerun Step 5.

- [ ] **Step 7: The device cycle**, `FIX="cpp/src cpp/tests/cpu/test_executor.cpp"` (the cpu
  case calls the three-argument `cudf_ast_can_evaluate`, so it leaves the red build with the fix;
  `git add -N cpp/src/expr_scalar_fn.cpp` first if Step 6 made it). The red build is the
  `ColumnRefs` gtests, the join session's residual case and Step 4's message over the readers as
  Task 6 left them. Red set:

```bash
timeout 3600 ssh $H "$R cpp/install/bin/peacock_plan_tests --gtest_filter='ColumnRefs.*'"
timeout 1800 ssh $H "$R cpp/install/bin/peacock_join_session_tests --gtest_filter='ColumnRefs.*'"
timeout 3600 ssh $H "$R cpp/install/rust-tests/peacockdb_core_gpu_lib --test-threads=1 bug_a_global_stddev_finalize"
```

  Expected red: `ASortKeyRefusesEachBadReference` and `AComputedKeyRefusesEachBadReferenceInsideIt`
  (no reader checks a name, so the unnamed and misnamed keys sort, and the index past the width
  fails with another message), `AJoinResidualResolvesThroughItsSidesNames` (its misnamed residual
  runs) and `bug_a_global_stddev_finalize_is_refused_on_the_device` (the old out-of-range message).
  `AWellNamedSortKeyStillSorts` green. Green set — the whole device bar, since the check touches
  every node (Tasks 9 and 10 change no plan the device runs but `rollup-stddev`'s and
  `shuffle-stddev`'s, which Task 10 runs):

```bash
timeout 3600 ssh $H "$R cpp/install/bin/peacock_plan_tests"
timeout 1800 ssh $H "$R cpp/install/bin/peacock_gpu_tests"
timeout 1800 ssh $H "$R cpp/install/bin/peacock_join_session_tests"
timeout 7200 ssh $H "$R cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests:: --test-threads=1"
timeout 1800 ssh $H "$R cpp/install/rust-tests/test_node_timing --test-threads=1"
timeout 3600 ssh $H "$R cpp/install/rust-tests/peacock_gpu_benchmarks --skip bench_ --test-threads=1"
timeout 14400 ssh $H "$R cpp/install/rust-tests/test_gpu_corpus --test-threads=1"
```

  Expected: all green — every gtest (`ColumnRefs.*`, `TableResultInvariant.*`, `AggregateBuilder.*`,
  `AggregateMerge.*`, `GroupingId.*`, `AggregateNoInput.*` and the join session's residual case
  among them), `gpu_tests::` with the #216 pins at their moved messages, and every enabled corpus
  cell, `shuffle_stddev` included. On a red: a corpus cell the name check refuses is a wrong
  reference found — ticket it (query, mode, message) at the next free number and take the cell
  off under it, in its line and its registry row (the spec's rule), then rerun that cell's test
  to see it skipped; a harness case the check refuses because its uploaded batch is named
  otherwise than the node declares is a test fixture to fix, never a check to weaken. Rebuild
  and rerun only what failed.

- [ ] **Step 8: Commit.**

```bash
git add cpp peacockdb-core/src/tests/gpu_tests llm-wiki/tasks/aggregate-arms-detail.md
# and, only if Step 7 took a cell off: peacockdb-core/tests/common/corpus_cases.inc testdata/cost-registry.csv llm-wiki/tickets
git commit -m "#164: every ColumnRef reader in cpp/src checks bounds and name via column_at

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: The scan's names, checked at plan time

**Files:**
- Modify: `peacockdb-core/src/planner/translator/scan_mapping/parquet_meta.rs` (`survivor_metadata`)
- Test: `peacockdb-core/src/planner/translator/scan_mapping/parquet_meta/tests.rs`

- [ ] **Step 1: The cases, failing.** In `parquet_meta/tests.rs` (add
  `use datafusion::arrow::datatypes::Schema as ArrowSchema;`):

```rust
/// nation registered under its own schema with `rename` applied to every field name, then
/// `sql` planned over it as `declared`.
async fn metadata_over(
    rename: impl Fn(&str) -> String,
    sql: &str,
) -> Result<ScanMetadata, PlanError> {
    let path = minimal().join("nation.parquet");
    let path = path.to_str().unwrap();
    let own = SessionContext::new();
    own.register_parquet("nation", path, ParquetReadOptions::default())
        .await
        .expect("the file's own schema");
    let file = own.table("nation").await.expect("registered").schema().as_arrow().clone();
    let declared = ArrowSchema::new(
        file.fields()
            .iter()
            .map(|f| f.as_ref().clone().with_name(rename(f.name())))
            .collect::<Vec<_>>(),
    );
    let ctx = SessionContext::new();
    ctx.register_parquet("declared", path, ParquetReadOptions::default().schema(&declared))
        .await
        .expect("registered under the declared schema");
    let plan = ctx
        .sql(sql)
        .await
        .expect("the query parses")
        .create_physical_plan()
        .await
        .expect("the query plans");
    let scan = find_scan(&plan).expect("a scan");
    survivor_metadata(scan.as_any().downcast_ref::<ParquetExec>().unwrap())
}

#[tokio::test]
async fn a_declared_schema_that_renames_a_read_column_is_refused_naming_both() {
    // The device reads n_name by the declared name and finds none; the cpu reads ordinal 1.
    let refused = metadata_over(
        |n| if n == "n_name" { "nation_name".into() } else { n.into() },
        "SELECT * FROM declared",
    )
    .await
    .expect_err("a renamed column is read by different names on the two engines");
    let message = refused.to_string();
    assert!(matches!(refused, PlanError::Unsupported(_)), "{message}");
    assert!(
        message.contains("ordinal 1") && message.contains("`nation_name`") && message.contains("`n_name`"),
        "{message}"
    );
}

#[tokio::test]
async fn a_declared_schema_that_swaps_two_columns_of_one_type_is_refused() {
    // Same type, so nothing downstream would notice: the device would read one key as the other.
    let refused = metadata_over(
        |n| match n {
            "n_nationkey" => "n_regionkey".into(),
            "n_regionkey" => "n_nationkey".into(),
            other => other.into(),
        },
        "SELECT * FROM declared",
    )
    .await
    .expect_err("a swap reads each key as the other on the device");
    let message = refused.to_string();
    assert!(
        message.contains("ordinal 0") && message.contains("`n_regionkey`") && message.contains("`n_nationkey`"),
        "{message}"
    );
}

#[tokio::test]
async fn a_rename_of_a_column_the_scan_does_not_read_plans() {
    // Only the projected ordinals are read, and n_comment is not one of them here.
    metadata_over(
        |n| if n == "n_comment" { "remark".into() } else { n.into() },
        "SELECT n_name FROM declared",
    )
    .await
    .expect("an unread column's name is nobody's to check");
}
```

  If nation's two keys are not of one type in tpch.minimal, swap two columns that are; the case is
  a same-type swap.

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::translator::scan_mapping::parquet_meta
```

  Expected: the two refusals FAIL (`expect_err` on `Ok`); the unread rename PASSES.

- [ ] **Step 2: The check.** In `survivor_metadata`, after `projected` is computed and before the
  row-group loop (`use datafusion::physical_plan::ExecutionPlan;` for `parquet.schema()`):

```rust
    // The device reads a scan's columns by the declared names (the writer sends the output
    // schema and no projection) and the cpu by the projection's file ordinals; the two read the
    // same column only while each declared name is the file's at that ordinal.
    let file_fields = reader
        .metadata()
        .file_metadata()
        .schema_descr()
        .root_schema()
        .get_fields();
    let declared = parquet.schema();
    for (position, ordinal) in projected.iter().enumerate() {
        let held = file_fields.get(*ordinal).map(|field| field.name()).ok_or_else(|| {
            PlanError::Invalid(format!("{path}: the file has no column at ordinal {ordinal}"))
        })?;
        let wanted = declared.fields().get(position).map(|field| field.name()).ok_or_else(|| {
            PlanError::Invalid(format!("{path}: the scan declares no column {position}"))
        })?;
        if held != wanted {
            return Err(PlanError::Unsupported(format!(
                "{path}: the scan declares column {position} as `{wanted}` and the file holds \
                 `{held}` at ordinal {ordinal} — the device reads a column by its name and the \
                 cpu by its ordinal, so the two would read different columns"
            )));
        }
    }
```

  Add one sentence to the module doc: it also refuses a declared schema that renames or reorders
  the file's columns.

- [ ] **Step 3: Run, and every corpus scan as before.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::translator::scan_mapping planner::tests::plan_goldens
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus
git status --short testdata/
```

  Expected: PASS; `testdata/` clean. A corpus scan refused here is a registration whose declared
  names differ from its file (pbench's `empty` is the one registered under another table's
  schema): stop and report it, do not weaken the check.

- [ ] **Step 4: Commit.**

```bash
git add peacockdb-core/src/planner/translator/scan_mapping
git commit -m "the planner refuses a scan whose declared names differ from the file's

The device reads by name and the cpu by ordinal; the check makes the two
reads the same column by construction.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: `tpch/rollup-stddev`, and `tpch/shuffle-stddev` validated

**Files:**
- Create: `testdata/tpch-queries/rollup-stddev.sql`
- Modify: `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`
- Modify: `testdata/goldens/tpch.sf1/*.plans.txt`, the cpu tier's `*-mini.cpu.txt`,
  `*-mini.cost.txt`, `mini.result.txt`, `duckdb-result.txt`, `gpu-result.txt`

- [ ] **Step 1: The query**, as the spec gives it:

```sql
-- A stddev and a var under a rollup: the grouping-set path builds the Welford triple (#280).
select l_returnflag, l_linestatus, stddev_samp(l_quantity), var_pop(l_quantity)
from lineitem
group by rollup (l_returnflag, l_linestatus);
```

- [ ] **Step 2: The corpus lines, device cells off for now.** After `shuffle_stddev`'s line, in
  the shape the file's lines have (duckdb-oracle's argument before the cpu oracle; copy
  `shuffle_stddev`'s):

```
// rollup-stddev puts a Welford under a grouping set, which neither benchmark has; the grouping-set
// path builds the triple through the one request builder (#280).
corpus_query!(tpch, 1, rollup_stddev, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, duckdb_approx, data_fusion_approximate, golden_approx_std, schema_validation_enabled);
```

  The device cells stay off through Step 4, under `280`: duckdb-oracle's guard holds every enabled
  device cell to a `gpu-result.txt` section, which only a device run writes (Step 5). `280` is
  open until the merge archives it. `shuffle_stddev`'s line ends `schema_validation_enabled);`
  with its `// #225` gone, and the comment block above it loses any sentence about #225. Registry,
  after `shuffle_stddev`'s row:

```
tpch,1,rollup_stddev,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,disabled,disabled,disabled,disabled,disabled,ok,rollup stddev_var,280
```

  (`225` is in no registry row: shuffle-stddev's tags are `183`. Its cells do not change.)

- [ ] **Step 3: Write the cpu and DuckDB sections.**

```bash
UPDATE_CANONICAL=1 timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
UPDATE_CANONICAL=1 timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- rollup_stddev
[ -x /tmp/duckdb-1.5.4/bin/python ] || { python3 -m venv /tmp/duckdb-1.5.4 && /tmp/duckdb-1.5.4/bin/pip install duckdb==1.5.4; }
timeout 1800 /tmp/duckdb-1.5.4/bin/python testdata/duckdb_result.py --dataset tpch
git diff --stat testdata/
```

  Expected: the five `.plans.txt`, five `.cpu.txt`/`.cost.txt` and `mini.result.txt` gain a
  `rollup-stddev` section each and nothing else moves; `duckdb-result.txt` gains one section (read
  the diff: the script rewrites the whole file, and any other moved line is reverted with
  `git checkout -p`). `recipe-payloads.txt` unchanged (the query is not in the payload set). A cpu
  mode that fails on an open ticket comes off the line and the registry cell takes the ticket; an
  unknown failure is filed at the next free number.

- [ ] **Step 4: Verify without writing.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- rollup_stddev shuffle_stddev registry
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_cost_model
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
```

  Expected: PASS, the registry tests both ways and the `gpu-result.txt` guard included (no device
  cell of `rollup_stddev` is on yet).

- [ ] **Step 5: The device cells on, and the device cycle.** In the working tree, uncommitted:
  `rollup_stddev`'s line's gpu modes become `tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup |
  tp4_sized`; its registry row's five gpu cells `enabled` and its tickets empty. Compile the
  device corpus locally first:

```bash
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --test test_gpu_corpus --features gpu --no-run
```

  Then the *Device cycle*, green build only: the cells test Task 6's fix, which ran red first
  there, and no tree in this branch holds the query without the fix. Green set, the write
  filtered to the new query so no other section's float digits move:

```bash
timeout 3600 ssh $H "$R PCK_WRITE_GPU_RESULT=1 cpp/install/rust-tests/test_gpu_corpus --test-threads=1 rollup_stddev"
timeout 3600 ssh $H "$R cpp/install/rust-tests/test_gpu_corpus --test-threads=1 rollup_stddev shuffle_stddev"
timeout 600 rsync -a $H:peacockdb-L/testdata/goldens/tpch.sf1/gpu-result.txt testdata/goldens/tpch.sf1/
git diff --stat testdata/goldens/
```

  Expected: the five `rollup_stddev` cells and the five `shuffle_stddev` cells green, the latter
  now schema-validated; `gpu-result.txt` gains `rollup-stddev`'s five sections and nothing else
  moves (duckdb-oracle merges per `(query, mode)` section). A `rollup_stddev` cell that fails goes
  back off under its ticket, in the line and the registry, its section not committed. Record the
  run in `aggregate-arms-detail.md`.

- [ ] **Step 6: The guard, with the cells on.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- rollup_stddev registry
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
```

  Expected: PASS — every enabled device cell of `rollup_stddev` has its section; the registry
  agrees with the line both ways.

- [ ] **Step 7: Commit.**

```bash
git add testdata peacockdb-core/tests/common/corpus_cases.inc llm-wiki/tasks/aggregate-arms-detail.md
git commit -m "#280: tpch/rollup-stddev in the corpus; shuffle-stddev schema-validated (#225)

rollup-stddev's device sections written by a run filtered to it.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: The docs, the counts, the tickets

**Files:**
- Modify: `llm-wiki/architecture.md`, `llm-wiki/build-test.md`, `llm-wiki/tickets/corpus-coverage.md`

No device cycle: Tasks 8 and 10 ran the spec's device bar on this tree — every `peacock_*_tests`
gtest binary (`peacock_plan_tests`, `peacock_gpu_tests`, `peacock_join_session_tests`),
`gpu_tests::`, the walk tests and the whole `test_gpu_corpus` at Task 8, whose name check touches
every node; Tasks 9 and 10 change no plan the device runs but `rollup-stddev`'s (new) and
`shuffle-stddev`'s (validated), both run in Task 10; and this task changes only docs. Point the
detail file's device-bar line at those two runs.

- [ ] **Step 1: `architecture.md`.** Short sentences; each a current fact.
  - The aggregate sequence: the keyless Welford sentence stays (#216); add that the device names
    every state column from the function's `state_names`, and that a `count` merges by `sum`, so a
    `count` at `Merge` is refused on both engines.
  - The fbs table's `CudfAggregate` row: `mode` (Partial/Merge; the rest refused), `group_exprs`,
    `aggr_funcs` (each with `args`, `state_names`, `ddof`), `grouping_sets`, `aggr_input_schema`;
    `aggregate.cpp` — one request builder (`add_requests`) behind `grouped_aggregate` and
    `grouping_set_aggregate`, inputs read through `args`, built once per node and shared by every
    grouping set; `cudf::reduce` for a keyless node.
  - *What the Rust side puts in the flat buffers*: rows for `AggregateFuncNode.state_names` and
    `.ddof` (`aggregate_writer.rs`, from `state_funcs` → the names every state column carries; the
    divisor of a stddev or var, read by the keyless `stddev` arm); `CudfAggregate.mode`'s row says
    Final is refused.
  - The ordinal table: `ColumnRef.index` is read by `column_at` / `column_ordinal`
    (`column_refs.cpp`), which every reader uses; *What guards it* is rewritten: a reference is
    checked for its index and its name, a `TableResult` is built only through its constructors
    (`owning`, and `of` for shared parts), a union branch named otherwise is projected to the
    union's names (`cast_branch`), and a scan's declared names are checked against the file at
    plan time. The "parallel array with no invariant" paragraph and the Final-stage arity sentence
    go. Recount the `->index()` and `.column(…)` figures or drop them.
  - The union row: a branch whose types or names differ from the union's gets a projecting
    `GpuProject`.
  - The scan row: the device reads by declared name; `survivor_metadata` refuses a declared schema
    that renames or reorders a read column.
- [ ] **Step 2: Tickets citing moved code** (a ticket is about code; fix its facts here):
  - #216: `mergeable`, `is_stddev_name` and the `ColumnRef index 2 out of range (cols=1)` message
    are gone; the keyless arm tests the `stddev` names and reads `ddof` from the wire, and the
    finalize's refusal is `column_at`'s "is past the 1 columns its input has".
  - #94: the site is `merge_m2_input` and its constant `kMergeM2CountType`; there is no Final arm.
  - #164, #225 and #280 are archived at merge by the helper, as chain K's plans leave theirs.
- [ ] **Step 3: `build-test.md` counts.** Every count moved with J, K and L: recount each row this
  plan touches from the code (`--list` for the Rust tiers, `--gtest_list_tests` for the gtest
  binaries) and set every header and the grand total to the sums. This plan's deltas, to check
  the recount against:
  - C++ CPU/FFI unit (`peacock_cpu_tests`) +9: `TableResultInvariant` 2 (Task 1),
    `AggregatePhase` 1 (Task 4), `ColumnRefs` 6 (Task 5's five, and Task 8's
    `TheAstRouteResolvesAReferenceUnderAnOperator`) — 2 + 1 + 6 = 9;
  - Plan-executor (`peacock_plan_tests`) +14 − 1 = +13: `TableResultInvariant` 1 (Task 1),
    `AggregateBuilder` 10 (Task 6), `ColumnRefs` 3 (Task 8) — 1 + 10 + 3 = 14 — all in two new files
    linked into the binary (say so in the row); − `AnAvgsSumAndCountBothSurviveTheMerge` (Task 4);
  - the join session (`peacock_join_session_tests`) +1, in its own row
    (`ColumnRefs.AJoinResidualResolvesThroughItsSidesNames`);
  - plan types +2 (`plan::tests::aggregate`); wire recipes +1 (`wire::tests::aggregate_init`);
    planner translator +1 (`a_union_branch_named_otherwise_is_renamed_by_a_project`); scan mapping
    +3;
  - gpu aggregate cases +3 (the three rollup Welford cases; the two #225 flips are renames);
  - the cpu corpus +5 cells and one query (`tpch/rollup-stddev`); the device corpus +5 cells
    (fewer by any Task 10 left off); the device corpus row's sentence about `shuffle-stddev` saying
    `disabled` against #225 goes.
- [ ] **Step 4: The full verification bar, locally.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib
timeout 7200 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_cost_model
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests:: planner::tests::plan_goldens::the_payload_golden
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
```

  Expected: all green, the registry tests both ways and duckdb-oracle's `gpu-result.txt` guard
  included. The 26.02 build leg is CI's.

- [ ] **Step 5: Commit.**

```bash
git add llm-wiki/architecture.md llm-wiki/build-test.md llm-wiki/tickets/corpus-coverage.md llm-wiki/tasks/aggregate-arms-detail.md
git commit -m "aggregate-arms: architecture, tickets and counts

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```
