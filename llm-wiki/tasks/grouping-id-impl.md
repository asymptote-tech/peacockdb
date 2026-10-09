# grouping-id implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The device's grouping-set id is DataFusion 45's in value and width (#65); a summed
quotient's merge (#55) and the DISTINCT lowering (#262) are proven on a device; `tpch/rollup-grouping`
joins the corpus; every device cell the three tickets hold is run, then enabled or ticketed.

**Architecture:** One static `grouping_id_column(mask, nkeys, rows)` in `aggregate.cpp` folds a
set's mask into a `uint64_t`, first key highest, and builds the column at DataFusion's width. The
exec model folds the same way. The rest is proof: gtests in a new `test_grouping_id.cpp` over
hand-built tables from a new shared `hand_tables.hpp`, the four `bug_` pins turned into agreement
cases, a contract row both engines answer, walk tests against DataFusion, and the corpus run.

**Tech stack:** C++20 on cuDF 25.02 (`peacock_plan_tests`, a device binary); Rust: the rust-only
cpu tier and the `--features gpu` device rung; the Python exec model; DuckDB 1.5.4 for the oracle
section.

**Spec:** [`grouping-id.md`](grouping-id.md).

## Before you start

Chain L's base is master once chains J and K have both merged. This plan was written against
8806a3c3, before either, and revised against 0753f8c9. Find every site below by its symbol, never
by a line number. What moves:

- **`cpp/src/operators/aggregate.cpp`.** K (distinct-companions, its Task 5) deletes the
  `distinct` guard at the top of `execute_aggregate`. refcounted-scatter makes every `return {…}`
  a `TableResult::owning(…)`, the grouping-set arm's return included. exit-copies (its Task 4)
  releases groupby's keys in the loop that builds `cols`, right beside the `gid_s` lines. So read
  `gk->num_rows()` before anything releases `gk`. The arm is still the block under
  `// ---- ROLLUP / CUBE / GROUPING SETS ----`, holding `int32_t gid` and `numeric_scalar<int32_t> gid_s`.
- **`cpp/tests/gpu/test_plan_executor.cpp`** (2203 lines, past the 1000-line cap) takes nothing
  from this plan. K drops its `/*distinct=*/false` arguments; refcounted-scatter rewrites
  `.table->view()` to `.view()`; join-backend moves its join cases onto the session. `main`, which
  installs the RMM pool, stays there and serves every source of `peacock_plan_tests`.
- **`TableResult`** (refcounted-scatter, J) is `{owners, columns, column_names}` with `view()`
  and the constructor `owning(std::unique_ptr<cudf::table>, std::vector<std::string>)`, which
  refuses a name count unequal to the column count. `hand_tables.hpp` builds on both.
- **`cpp/CMakeLists.txt`.** join-session-cpp adds the `peacock_join_session_tests` binary
  (`tests/gpu/test_join_session.cpp`). `peacock_plan_tests`' source list is
  `tests/gpu/test_plan_executor.cpp` alone on master; append to it as it stands.
- **`peacockdb-core/src/wire/gpu_tests/mod.rs`.** join-backend rewrites the join walker, `driven`
  and `PROVEN`. Nothing here depends on them. The aggregate walk tests, `held`, `after`, `times`,
  `trail`, `TWO_LANES` and `ONE_LANE` keep their names. A `mod.rs` has no length cap.
- **`peacockdb-core/tests/common/corpus_cases.inc`.** duckdb-oracle gives every line a ninth
  argument, `duckdb_oracle`, before `cpu_oracle`, and adds the `all_modes` sugar. pbench adds a
  third dataset, with `pbench/rollup-small-keys`. K adds `distinct_functions` with gpu oracle
  `golden_exact`; Task 5 makes it `golden_approx_std`. Copy the shape of a neighbouring line.
- **`testdata/cost-registry.csv`.** repartition-keys closes #189: the tp4 cpu cells of the rollup
  rows turn on, and their gpu tp4 cells take `65`, `pbench/rollup-small-keys`' among them
  (`repartition-keys-impl.md`, its corpus task). join-backend closes #152 and #220 and tags
  `rollup_over_join` with `65`. K adds `tpch/rollup-distinct` (`65 189`, then `262`),
  `tpch/distinct-functions` (`262`) and tpcds q28 (`262` and `152`). **Build Task 5's row list
  from the registry as it stands, not from the spec's table.**
- **The goldens.** duckdb-oracle adds `gpu-result.txt` per dataset, written by the device corpus
  run under `PCK_WRITE_GPU_RESULT=1`. It adds a rust-only guard that every enabled device cell
  has its section and no other, and a `duckdb_gpu_<dataset>_<query>_<mode>` comparison per
  section. Turning a device cell on means writing its section.
- **`testdata/duckdb_result.py`** rewrites a dataset's whole `duckdb-result.txt` on every run, and
  `--only` writes only the queries it names (`generate`, the `target.write_text` at its end). Run
  it over the whole dataset and read the diff. Check whether duckdb-oracle has changed this.
- **`llm-wiki/build-test.md`.** Every count moves with J and K. Recount each row this plan touches
  from the code. The deltas below are this task's alone.
- **Ticket numbers.** `tickets.md` on 0753f8c9 leaves 264–279 to chain J's branches and has K's
  280 on. A new ticket here takes the next free number from master's `tickets.md` as it stands
  after J's merge, which reconciles the two; read its counter line, never a number from this plan.
- **After this task:** keyless-identity's `AggregateNoInput.*` gtests and aggregate-arms'
  `test_column_refs.cpp` and `test_aggregate_builder.cpp` go in their own files, appended to
  `peacock_plan_tests`' sources, and include the `hand_tables.hpp` Task 2 creates; aggregate-arms
  no longer creates it. aggregate-arms rewrites how the grouping-set arm builds its requests (#280)
  and keeps calling `grouping_id_column`. Its arity check wants one argument per function, which
  `grouping_sets_node`'s `count(k)` already has; its alias and `state_names` work updates that
  node in `test_grouping_id.cpp`. The suite is `GroupingId`. Do not reach into their scope.

## Global constraints

- The id is DataFusion 45's (`group_id_array`, `datafusion-physical-plan-45.0.0/src/aggregates/mod.rs`):
  `fold(0u64, |acc, is_null| (acc << 1) | is_null)` over the keys in order, first key highest.
  The width is `UInt8` up to 8 keys, `UInt16` up to 16, `UInt32` up to 32, `UInt64` beyond, and
  past 64 keys DataFusion answers `not_impl_err`.
- No wire change and no payload change: `recipe-payloads.txt` stays byte-identical. After every
  regeneration, `git status --short testdata/goldens/recipe-payloads.txt` prints nothing.
- No facade, trait or ABI change.
- `GROUPING()` over a subset or a reordering of the keys stays refused on the device (#230).
  DataFusion 55's duplicate ordinal stays #228.
- A device cell that fails on anything but the id, the quotient or the DISTINCT lowering's device
  path gets a ticket, not a fix here.
- A `bug_` test that goes red is deleted in the same change that fixed it, or here, turned into
  the agreement case under a name without `bug_` (`coding-style.md`, *Building around a bug*).
- **The 1000-line cap** (`coding-style.md`). `test_plan_executor.cpp` takes no new test: the
  `GroupingId.*` gtests go in a new `cpp/tests/gpu/test_grouping_id.cpp`, and Task 3's conditional
  gtest in a new `test_summed_quotient.cpp`, each added to `peacock_plan_tests`' sources.
- CPU builds and runs are local, as `build-test.md` describes them. Rust: `cargo test --features
  rust-only` into `target/`. C++: `cpp/build` against cuDF 25.02, `scripts/build.sh --cudf_ROOT
  ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build`, then
  `ctest --test-dir cpp/build -L cpu`. The cudf Rust shape goes through `scripts/cargo-cudf.sh`.
  The workspace needs the sf1 parquet under `testdata/` (gitignored): symlink it from the primary
  checkout if absent.
- **Device** on nebius-gpu, `dmitry@89.169.109.150`, cuDF 25.02, in `~/peacockdb-L`, as chain L's
  header says: **one sync per task, with as many back-to-back builds as its red/green pairs need,
  the red build first.** Three tasks have device tests: Task 2 (a red build and a green build),
  Task 3 (one build, plus a pair if #55 is live) and Task 5 (the trial build and the final build).
  Each syncs once, as the *Device cycle* below, after its local tiers pass and before its commit;
  a file written after the sync travels by name, never in a second sync. Record every build and
  run (command, binaries, pass and fail counts per binary) in `llm-wiki/tasks/grouping-id-detail.md`,
  committed with the task that ran it.
- **Out of every task, as the header says:** the sf40 binaries (`peacock_tpch_tests`,
  `peacock_tpchv_tests`), `--run-benchmarks`, Nsight captures and any H200 timing, each recorded in
  the detail file as deferred. **Done** is CI green except the `gpu-tests` job ("GPU Tests
  (remote)", shad-gpu), with every task's device runs passed on nebius-gpu and recorded in the
  detail file.
- Formatting applies to changed lines only. For C++, `git clang-format HEAD -- <files>`, and
  `clang-format -i` on a new file, all of whose lines are this task's. For Rust,
  `rustfmt --edition 2024 --check <file>`, applied only where its diff stays inside the lines this
  task wrote.
- No commit leaves a test red. Commit messages are at most 10 lines and end with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Build in a workspace, and never
  share a cargo target dir across worktrees.

## Device cycle

A task with device tests runs this once, after its tests and its fix are written and every local
tier passes, and before its commit. The one sync carries the whole working tree, uncommitted. The
red build is the tree with the task's `FIX` (the paths whose diff is the fix, never a test file)
reversed by a patch; the green build is the tree as written. Run everything in the foreground: a
backgrounded build chain dies mid-build with no error. If `ssh` seems dead while the host answers
a raw socket, the sandbox is blocking the client, not the host.

```bash
H=dmitry@89.169.109.150
R='cd ~/peacockdb-L && . ~/peacock-env.sh && export LD_LIBRARY_PATH=$PWD/cpp/install/lib:$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib PEACOCK_TESTDATA_DIR=$PWD/testdata &&'
B='cd ~/peacockdb-L && . ~/peacock-env.sh && ./scripts/build-test-shadgpu.sh --build'
# The fix's reverse, for the red build.
git diff -R HEAD -- $FIX > /tmp/grouping-id-red.patch
# The task's one sync, from the workspace root.
timeout 1200 rsync -a --delete-after --exclude=.git --filter=':- .gitignore' ./ $H:peacockdb-L/
timeout 300 rsync -a /tmp/grouping-id-red.patch $H:grouping-id-red.patch
# Red build: the tests without the fix.
timeout 300 ssh $H 'cd ~/peacockdb-L && patch -p1 < ~/grouping-id-red.patch'
timeout 7200 ssh $H "$B"
timeout 3600 ssh $H "$R $CMD"     # once per command of the red set
# Green build: the fix back in.
timeout 300 ssh $H 'cd ~/peacockdb-L && patch -p1 -R < ~/grouping-id-red.patch'
timeout 7200 ssh $H "$B"
timeout 3600 ssh $H "$R $CMD"     # once per command of the green set
```

A task with no red build (it says why) skips the patch and the red lines. A file changed after the
sync, for a further red/green pair or a changed declaration, travels by name from the workspace
root, then the next build runs back to back:

```bash
timeout 300 rsync -aR $PATHS $H:peacockdb-L/   # PATHS: each file changed since the sync
```

A fresh `~/peacockdb-L` has no sf1 parquet. After the first sync and before its first build, copy
it from chain J's tree;
it is gitignored, so later syncs leave it alone. pbench's data is committed.

```bash
timeout 1800 ssh $H 'cd ~/peacockdb-L/testdata && for d in tpch.sf1 tpcds.sf1; do [ -d $d ] || cp -a ~/peacockdb-J/testdata/$d .; done'
```

Every case a task names as red must fail, for the reason it names. A case red for another reason,
or green where the task expects red, is a finding to settle before the fix: a test that passes
without its fix may assert nothing. Every case of the green set passes. One binary per `ssh`, so
one red does not hide the next. Every Rust binary takes `--test-threads=1`. If the loader cannot
find `libcudf`, the 25.02 env on that host may be `~/miniforge3/envs/rapids-cuda-12.2` (#260
records that path): use whichever exists and say so in the detail file.

## Review focus

1. **A mask that is not a rollup's, at a width boundary.** If the first key is masked alone, a
   fold run from the wrong end answers 1, where DataFusion answers 2ⁿ⁻¹. At 33 and 64 keys a 32-bit
   shift overflows. Expected: 128 at 8 keys (`UInt8`), 256 at 9 (`UInt16`), 65536 at 17 (`UInt32`),
   2³² at 33 and 2⁶³ at 64 (`UInt64`), and the all-masked set at all-ones. Task 2's `GroupingId.*`
   gtests.
2. **More than 64 keys.** Expected: refused with a message naming 65, never a wrapped id. Task 2's
   `GroupingId.SixtyFiveKeysAreRefusedByCount`.
3. **Zero rows under grouping sets.** The id column is built from a scalar, and at zero rows it
   must still hold the declared type. Expected: zero rows, `UInt8`, on both engines. Task 2's
   `grouping_sets_over_zero_rows_are_zero_rows_of_the_declared_id_type` compares exactly, types
   included.
4. **A natural NULL key beside a placeholder NULL.** `synthetic` puts NULLs in `key` and `b`.
   Only the id keeps a natural-NULL group of the full set apart from a masked set's group.
   Expected: the cpu's rows exactly. Task 2's `grouping_sets_carry_datafusions_id_in_its_type`.
5. **A DISTINCT under an eight-key rollup.** The inner stage carries nine keys, so its id is
   `UInt16`, and a project narrows it to DataFusion's `UInt8`. Before the fix the device doubled
   that id, and at eight keys the cast overflowed (#65's note from distinct-companions). Expected:
   both stages' inits hold `UInt16`, and the answer is DataFusion's. Task 2's
   `a_distinct_under_an_eight_key_rollup_narrows_a_uint16_id_to_datafusions`.

## File structure

| file | responsibility |
|---|---|
| `scripts/exec_model/operators/aggregates.py`, `tests/test_operators.py`, `tests/test_end_to_end.py` | the model's id |
| `cpp/src/operators/aggregate.cpp` | `grouping_id_column`; the arm calls it |
| `cpp/tests/gpu/hand_tables.hpp` (new) | hand-built device tables and plan pieces, shared by chain L's gtests |
| `cpp/tests/gpu/test_grouping_id.cpp` (new), `cpp/CMakeLists.txt` | `GroupingId.*`, in `peacock_plan_tests` |
| `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs`, `aggregate_schema_cases.rs` | three `bug_` pins become agreement cases; `grouping_sets_as_exported` goes |
| `peacockdb-core/src/tests/executor_cases.rs`, `executor/cpu_backend/tests/contract.rs`, `executor/gpu_backend/gpu_tests/contract.rs` | the rollup contract row, on both engines |
| `peacockdb-core/src/wire/gpu_tests/mod.rs` | the fourth pin; the `GROUPING()`, narrowed-id, quotient and DISTINCT walks |
| `peacockdb-core/src/planner/translator/tests.rs` | one comment that names #65 |
| `testdata/tpch-queries/rollup-grouping.sql` (new), `testdata/goldens/tpch.sf1/*` | the new query |
| `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`, `testdata/goldens/*/gpu-result.txt` | the cells |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets.md`, `tickets/`, `archive/archived-tickets.md`, `tasks/grouping-id-detail.md` | the id described; counts; #65, #55, #262; the device record |

---

### Task 1: The exec model folds the id as DataFusion does (#65)

**Files:**
- Modify: `scripts/exec_model/operators/aggregates.py` (`grouping_set_id`, `rollup_masks`)
- Modify: `scripts/exec_model/tests/test_operators.py` (`test_a_rollup_expands_into_one_batch_carrying_every_set`,
  `test_a_masked_key_is_null_rather_than_absent`, one new test)
- Modify: `scripts/exec_model/tests/test_end_to_end.py` (`test_a_rollup_carries_the_grouping_id_through_the_whole_sequence`)
- Modify: `llm-wiki/build-test.md`

**Interfaces:** none produced. `grouping_set_id(mask) -> int` keeps its signature.

- [ ] **Step 1: The pins and a test the rollups cannot satisfy by accident.** In
  `test_operators.py`:

```python
    assert set(out[A.GROUPING_ID]) == {0, 1, 3}          # DataFusion's: the first key highest
```

  In `test_a_masked_key_is_null_rather_than_absent`, the set that masks `b` alone is now id 1:

```python
    masked = out[out[A.GROUPING_ID] == 1]
```

  After `test_a_rollup_expands_into_one_batch_carrying_every_set`:

```python
def test_the_grouping_id_puts_the_first_key_highest():
    # A rollup's masks agree with a bitmask numbered from the other end on (F,F) and (T,T);
    # a mask no rollup makes is what tells the two apart.
    assert A.grouping_set_id((False, True)) == 1
    assert A.grouping_set_id((True, False, False)) == 4
    assert A.grouping_set_id((False, False, True)) == 1
    assert A.grouping_set_id((True,) * 9) == 511
```

  In `test_end_to_end.py`:

```python
    assert set(got[A.GROUPING_ID]) == {0, 1, 3}
```

- [ ] **Step 2: Run, red.**

```bash
timeout 600 python3 scripts/exec_model/tests/test_operators.py
timeout 600 python3 scripts/exec_model/tests/test_end_to_end.py
```

  Expected: both exit non-zero. `test_a_rollup_expands_into_one_batch_carrying_every_set`,
  `test_a_masked_key_is_null_rather_than_absent` and `test_the_grouping_id_puts_the_first_key_highest`
  fail in the first file, and `test_a_rollup_carries_the_grouping_id_through_the_whole_sequence`
  in the second.

- [ ] **Step 3: The fold.** Replace `grouping_set_id` and the last sentence of `rollup_masks`'
  docstring:

```python
def grouping_set_id(mask) -> int:
    """The id tagging a set's rows: DataFusion's `__grouping_id`.

    The mask folds in key order, the first key highest, as `group_id_array` does
    (datafusion-physical-plan, aggregates/mod.rs). `GROUPING(k1, …, kn)` over the full key
    list reads this value, so it is the answer and not just a tag that keeps the sets apart.
    """
    gid = 0
    for masked in mask:
        gid = (gid << 1) | int(bool(masked))
    return gid
```

```python
    `GroupingSetMask` convention. For two keys that is (F,F), (F,T), (T,T), so the ids are
    0, 1 and 3.
```

  The id column stays `int64`. The model holds no width, and its id never reaches a sink.

- [ ] **Step 4: Run every exec-model file.** The CI loop, then the three that CI runs elsewhere:

```bash
status=0
for f in scripts/exec_model/tests/test_*.py; do
  case "$f" in */test_tpch_corpus.py|*/test_tpcds.py) continue ;; esac
  echo "== $f"; timeout 1800 python3 "$f" || status=1
done; echo "status=$status"
timeout 3600 python3 scripts/exec_model/tests/test_tpch_corpus.py
DUCKDB=$(command -v duckdb || echo ~/.duckdb/cli/latest/duckdb) timeout 3600 python3 scripts/exec_model/tests/test_tpcds.py
```

  Expected: `status=0`, and both corpus files exit 0. Their rollups (q5, q14, q18, q22, q77, q80)
  drop the id in their final project, so no answer moves.

- [ ] **Step 5: Counts.** In `build-test.md`: *Exec-model prototype (Python)* +1 (216 at
  0753f8c9); the header's Python +1 and the grand total +1.

- [ ] **Step 6: Commit.**

```bash
git add scripts/exec_model/operators/aggregates.py scripts/exec_model/tests/test_operators.py \
        scripts/exec_model/tests/test_end_to_end.py llm-wiki/build-test.md
git commit -F - <<'EOF'
#65: the exec model's grouping id is DataFusion's

The mask folds in key order, the first key highest: a two-key rollup is 0, 1, 3.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 2: The device's grouping-set id is DataFusion's (#65)

**Files:**
- Modify: `cpp/src/operators/aggregate.cpp` (a static above `execute_aggregate`; the grouping-set arm)
- Create: `cpp/tests/gpu/hand_tables.hpp`, `cpp/tests/gpu/test_grouping_id.cpp`
- Modify: `cpp/CMakeLists.txt` (`peacock_plan_tests`' sources)
- Modify: `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs`, `aggregate_schema_cases.rs`
- Modify: `peacockdb-core/src/tests/executor_cases.rs`, `peacockdb-core/src/executor/cpu_backend/tests/contract.rs`,
  `peacockdb-core/src/executor/gpu_backend/gpu_tests/contract.rs`
- Modify: `peacockdb-core/src/wire/gpu_tests/mod.rs`
- Modify: `peacockdb-core/src/planner/translator/tests.rs` (the comment in
  `grouping_sets_expand_at_the_init_and_group_on_the_id_above_it`)
- Modify: `llm-wiki/architecture.md` (*Grouping sets*), `llm-wiki/build-test.md`, `llm-wiki/tickets/corpus-coverage.md` (#65)
- Create: `llm-wiki/tasks/grouping-id-detail.md`

**Interfaces:**
- Produces (file-static, `aggregate.cpp`; aggregate-arms' `grouping_set_aggregate` calls it):

```cpp
static std::unique_ptr<cudf::column> grouping_id_column(const flatbuffers::Vector<uint8_t>* mask,
                                                        cudf::size_type nkeys,
                                                        cudf::size_type rows);
```

- Produces (test-only, `cpp/tests/gpu/hand_tables.hpp`; keyless-identity's and aggregate-arms'
  gtest files include it): namespace `hand` with `column_of<T>(values)`, `values_of<T>(column)`,
  `table_of(columns, names)`, `ref(fbb, index, name)`, `null_of(fbb, type)` and
  `run(fbb, node, inputs)`, exactly as Step 1 writes them.
- Produces (test-only, `src/tests/executor_cases.rs`): `Shape::SumOverRollup`, `ROLLUP_STATE`,
  `rollup_bodies()`, `rollup_rows(&RecordBatch)`.

- [ ] **Step 1: The shared header.** `cpp/tests/gpu/hand_tables.hpp`:

```cpp
#pragma once
// Hand-built inputs for operator gtests: device columns from host values and back, and the
// flat-buffer pieces an operator reads. An operator runs through execute_one with these as its
// resident inputs, as NodeSession hands them.

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

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

namespace hand {
namespace fb = peacock::plan;

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

/// `node` as the root of `fbb`'s plan, run once over `inputs`.
inline peacock::TableResult run(flatbuffers::FlatBufferBuilder& fbb,
                                flatbuffers::Offset<fb::PlanNode> node,
                                std::vector<peacock::TableResult> inputs) {
  fbb.Finish(fb::CreateGpuPlan(fbb, node));
  return peacock::execute_one(fb::GetGpuPlan(fbb.GetBufferPointer())->root(), std::move(inputs));
}

}  // namespace hand
```

  A node run this way has no `input` child: `execute_one` hands it the tables as resident inputs,
  and the operator takes them with `take_input`, as `NodeSession` does.

- [ ] **Step 2: The gtests.** `cpp/tests/gpu/test_grouping_id.cpp`:

```cpp
/// #65: the grouping-set id is DataFusion's `__grouping_id`. The masks fold in key order, the
/// first key highest, at the narrowest unsigned width with a bit per key; past 64 keys refused.
/// Every key is the one hand-built column `k` again, so a set's groups are k's two values or,
/// fully masked, one row; only the id is read.

#include "hand_tables.hpp"

#include <gtest/gtest.h>

#include <cstdint>
#include <set>
#include <stdexcept>
#include <string>
#include <vector>

namespace {
namespace fb = peacock::plan;

/// The resident input: k = 1 1 2 2 2.
std::vector<peacock::TableResult> the_keys() {
  std::vector<std::unique_ptr<cudf::column>> columns;
  columns.push_back(hand::column_of<int32_t>({1, 1, 2, 2, 2}));
  std::vector<peacock::TableResult> inputs;
  inputs.push_back(hand::table_of(std::move(columns), {"k"}));
  return inputs;
}

/// A Partial grouped on `nkeys` copies of `k` under `masks`, with count(k).
flatbuffers::Offset<fb::PlanNode> grouping_sets_node(flatbuffers::FlatBufferBuilder& fbb,
                                                     int nkeys,
                                                     const std::vector<std::vector<bool>>& masks) {
  std::vector<flatbuffers::Offset<fb::Expr>> keys, nulls;
  std::vector<flatbuffers::Offset<flatbuffers::String>> names;
  for (int i = 0; i < nkeys; ++i) {
    keys.push_back(hand::ref(fbb, 0, "k"));
    nulls.push_back(hand::null_of(fbb, fb::DataType_Int32));
    names.push_back(fbb.CreateString("k" + std::to_string(i)));
  }
  std::vector<flatbuffers::Offset<fb::GroupingSetMask>> sets;
  for (const auto& mask : masks) {
    std::vector<uint8_t> bits(mask.begin(), mask.end());
    auto values = fbb.CreateVector(bits);
    sets.push_back(fb::CreateGroupingSetMask(fbb, values));
  }
  auto count_name = fbb.CreateString("count");
  auto count_alias = fbb.CreateString("n");
  auto count_arg = hand::ref(fbb, 0, "k");
  auto count_args = fbb.CreateVector(std::vector<flatbuffers::Offset<fb::Expr>>{count_arg});
  fb::AggregateFuncNodeBuilder count(fbb);
  count.add_name(count_name);
  count.add_args(count_args);
  count.add_alias(count_alias);
  auto count_func = count.Finish();
  auto funcs =
      fbb.CreateVector(std::vector<flatbuffers::Offset<fb::AggregateFuncNode>>{count_func});
  auto key_vec = fbb.CreateVector(keys);
  auto name_vec = fbb.CreateVector(names);
  auto null_vec = fbb.CreateVector(nulls);
  auto null_names = fbb.CreateVector(names);
  auto set_vec = fbb.CreateVector(sets);
  fb::CudfAggregateBuilder agg(fbb);
  agg.add_mode(fb::AggregateMode_Partial);
  agg.add_group_exprs(key_vec);
  agg.add_group_names(name_vec);
  agg.add_aggr_funcs(funcs);
  agg.add_null_exprs(null_vec);
  agg.add_null_names(null_names);
  agg.add_grouping_sets(set_vec);
  return fb::CreatePlanNode(fbb, fb::PlanNodeKind_CudfAggregate, agg.Finish().Union());
}

std::vector<bool> none_masked(int nkeys) { return std::vector<bool>(nkeys, false); }
std::vector<bool> all_masked(int nkeys) { return std::vector<bool>(nkeys, true); }

/// `nkeys` keys with only `position` masked.
std::vector<bool> masked_at(int nkeys, int position) {
  auto mask = none_masked(nkeys);
  mask[position] = true;
  return mask;
}

/// The id column a grouping-set Partial answers: its type, and the value each set carries.
struct IdColumn {
  cudf::type_id type;
  std::set<uint64_t> values;
};

IdColumn id_column(int nkeys, const std::vector<std::vector<bool>>& masks) {
  flatbuffers::FlatBufferBuilder fbb;
  auto result = hand::run(fbb, grouping_sets_node(fbb, nkeys, masks), the_keys());
  EXPECT_EQ(result.column_names.at(nkeys), "__grouping_id");
  auto id = result.view().column(nkeys);
  IdColumn out{id.type().id(), {}};
  auto widen = [&](auto zero) {
    auto host = hand::values_of<decltype(zero)>(id);
    out.values.insert(host.begin(), host.end());
  };
  switch (out.type) {
    case cudf::type_id::UINT8: widen(uint8_t{}); break;
    case cudf::type_id::UINT16: widen(uint16_t{}); break;
    case cudf::type_id::UINT32: widen(uint32_t{}); break;
    case cudf::type_id::UINT64: widen(uint64_t{}); break;
    default:
      ADD_FAILURE() << "the id is not an unsigned integer: type id " << static_cast<int>(out.type);
  }
  return out;
}

}  // namespace

TEST(GroupingId, OneKeyIsUInt8) {
  auto ids = id_column(1, {none_masked(1), all_masked(1)});
  EXPECT_EQ(ids.type, cudf::type_id::UINT8);
  EXPECT_EQ(ids.values, (std::set<uint64_t>{0, 1}));
}

TEST(GroupingId, ATwoKeyRollupFoldsTheFirstKeyHighest) {
  // ROLLUP(a, b) is (a, b), (a), (): 0, 1, 3. Bit i for masked key i gave the middle set 2.
  auto ids = id_column(2, {none_masked(2), masked_at(2, 1), all_masked(2)});
  EXPECT_EQ(ids.type, cudf::type_id::UINT8);
  EXPECT_EQ(ids.values, (std::set<uint64_t>{0, 1, 3}));
}

TEST(GroupingId, EightKeysStayUInt8) {
  auto ids = id_column(8, {masked_at(8, 0), masked_at(8, 7), all_masked(8)});
  EXPECT_EQ(ids.type, cudf::type_id::UINT8);
  EXPECT_EQ(ids.values, (std::set<uint64_t>{1, 128, 255}));
}

TEST(GroupingId, NineKeysAreUInt16) {
  auto ids = id_column(9, {masked_at(9, 0), all_masked(9)});
  EXPECT_EQ(ids.type, cudf::type_id::UINT16);
  EXPECT_EQ(ids.values, (std::set<uint64_t>{256, 511}));
}

TEST(GroupingId, SeventeenKeysAreUInt32) {
  auto ids = id_column(17, {masked_at(17, 0), all_masked(17)});
  EXPECT_EQ(ids.type, cudf::type_id::UINT32);
  EXPECT_EQ(ids.values, (std::set<uint64_t>{65536, 131071}));
}

TEST(GroupingId, ThirtyThreeKeysAreUInt64) {
  auto ids = id_column(33, {masked_at(33, 0), all_masked(33)});
  EXPECT_EQ(ids.type, cudf::type_id::UINT64);
  EXPECT_EQ(ids.values, (std::set<uint64_t>{uint64_t{1} << 32, (uint64_t{1} << 33) - 1}));
}

TEST(GroupingId, SixtyFourKeysFillTheWord) {
  auto ids = id_column(64, {masked_at(64, 0), all_masked(64)});
  EXPECT_EQ(ids.type, cudf::type_id::UINT64);
  EXPECT_EQ(ids.values, (std::set<uint64_t>{uint64_t{1} << 63, UINT64_MAX}));
}

TEST(GroupingId, SixtyFiveKeysAreRefusedByCount) {
  // DataFusion refuses past 64 keys; an id that wrapped would answer the wrong sets.
  flatbuffers::FlatBufferBuilder fbb;
  auto node = grouping_sets_node(fbb, 65, {none_masked(65), all_masked(65)});
  try {
    hand::run(fbb, node, the_keys());
    FAIL() << "a grouping set over 65 keys answered";
  } catch (const std::runtime_error& e) {
    EXPECT_NE(std::string(e.what()).find("65 keys"), std::string::npos) << e.what();
  }
}
```

- [ ] **Step 3: Wire the file.** In `cpp/CMakeLists.txt`, append `tests/gpu/test_grouping_id.cpp`
  to `add_executable(peacock_plan_tests …)`'s sources, keeping every source already there. On
  master that line becomes:

```cmake
add_executable(peacock_plan_tests tests/gpu/test_plan_executor.cpp tests/gpu/test_grouping_id.cpp)
```

  The target's include directories (`include`, `src`, the generated dir's parent) already reach
  `peacock/operators.h`, `plan_executor.h` and `generated/gpu_plan_generated.h`.

- [ ] **Step 4: The four pins become agreement cases.** In `aggregate_cases.rs`, delete
  `grouping_sets_as_exported` and its doc comment. Replace the two `#65` cases, comments
  included:

```rust
// Three sets, both keys, `key` alone, neither — over `synthetic`, whose NULL keys meet the
// placeholders, so only the id keeps a natural-NULL group apart from a masked one.
operator_case! {
    GpuAggregate,
    fn grouping_sets_carry_datafusions_id_in_its_type() {
        let node = grouping_sets(vec![
            vec![false, false],
            vec![false, true],
            vec![true, true],
        ]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
    }
}
```

```rust
// The id column is built from a scalar; at zero rows it still holds the declared type.
operator_case! {
    GpuAggregate,
    fn grouping_sets_over_zero_rows_are_zero_rows_of_the_declared_id_type() {
        let node = grouping_sets(vec![vec![false, false], vec![true, true]]);
        run_both(&node, Script::Exec(vec![synthetic(0, 1)])).same(Order::Any);
    }
}
```

  Drop the imports the compiler then names unused (`Int32Array`, `UInt8Type`, `AsArray` if
  nothing else uses it). Delete `gpu_answered` if no caller is left: an unused `pub(crate)` there
  is a new warning. A chain J task (repartition-keys' #243 pins) may have added a caller.

  In `aggregate_schema_cases.rs`, replace the `#65` case and its comment:

```rust
operator_case! {
    GpuAggregate,
    fn grouping_sets_hold_the_uint8_grouping_id_the_plan_declares() {
        let node = grouping_sets(vec![vec![false, false], vec![false, true], vec![true, true]]);
        assert_holds_as_declared(&node, Script::Exec(vec![input()]));
    }
}
```

  In `wire/gpu_tests/mod.rs`, replace `bug_a_rollup_partial_holds_an_int32_grouping_id_where_the_plan_says_uint8`
  and its `#65` comment:

```rust
#[tokio::test]
async fn a_rollup_partial_holds_the_uint8_grouping_id_the_plan_declares() {
    let held = held(ROLLUP, ONE_LANE).await;
    let expected = DeviceSchema(vec![
        column("l_returnflag", TypeId::String),
        column("l_linestatus", TypeId::String),
        column("__grouping_id", TypeId::UInt8),
        decimal("sum(lineitem.l_quantity)", 2),
    ]);
    assert_eq!(after(&held, PARTIAL), vec![&expected]);
}
```

- [ ] **Step 5: `GROUPING()` and a narrowed id on a walk.** In `wire/gpu_tests/mod.rs`, after
  `const ROLLUP`:

```rust
const ROLLUP_GROUPING: &str = "SELECT n_regionkey, n_nationkey, \
     grouping(n_regionkey, n_nationkey) AS g, count(*) FROM nation \
     GROUP BY ROLLUP (n_regionkey, n_nationkey)";
const DISTINCT_UNDER_A_WIDE_ROLLUP: &str = "SELECT l_orderkey, l_partkey, l_linenumber, \
     l_returnflag, l_linestatus, l_shipinstruct, l_shipmode, l_comment, \
     count(DISTINCT l_suppkey), count(*) FROM lineitem WHERE l_orderkey <= 7 \
     GROUP BY ROLLUP (l_orderkey, l_partkey, l_linenumber, l_returnflag, l_linestatus, \
     l_shipinstruct, l_shipmode, l_comment)";
```

  The wide rollup's keys are integers and strings only, the types the device already groups
  under grouping sets, so a red there is the id or the lowering and not a key type.

  After `a_rollup_answers_with_every_grouping_set`:

```rust
/// `GROUPING()` over every key plans as the grouping id cast to Int32, so the id's value is
/// the answer here rather than a tag that only keeps the sets apart.
#[tokio::test]
async fn a_grouping_over_every_key_answers_datafusions_id() {
    assert_walk_matches_datafusion(ROLLUP_GROUPING, TWO_LANES).await;
}
```

  After `a_rollup_partial_holds_the_uint8_grouping_id_the_plan_declares`:

```rust
/// Eight keys and the DISTINCT argument make nine, so both stages carry a `UInt16` id where
/// DataFusion's over the eight is `UInt8`, and a project narrows it above the outer stage.
/// The argument is never masked, so its bit is 0 and the narrowed value is DataFusion's.
#[tokio::test]
async fn a_distinct_under_an_eight_key_rollup_narrows_a_uint16_id_to_datafusions() {
    let held = held(DISTINCT_UNDER_A_WIDE_ROLLUP, ONE_LANE).await;
    let ids: Vec<TypeId> = after(&held, PARTIAL)
        .iter()
        .map(|schema| {
            schema
                .0
                .iter()
                .find(|(name, _)| name == "__grouping_id")
                .map(|(_, held)| held.id)
                .expect("each stage's init carries the id")
        })
        .collect();
    assert_eq!(ids, vec![TypeId::UInt16, TypeId::UInt16]);
}
```

  Add `(ROLLUP_GROUPING, TWO_LANES),` and `(DISTINCT_UNDER_A_WIDE_ROLLUP, ONE_LANE),` to the list
  in `the_kinds_a_device_has_run_are_the_kinds_this_file_claims`.

  Nation's lanes at `TWO_LANES`: nation has one row group, but a walk's
  `BatchSizing::OneBatchPerLane` is `Batching::Off`, for which `lanes_for`
  (`planner/translator/nodes.rs`) plans `target_partitions` lanes whatever the table's size, so
  `SMALL_TABLE_BYTES` never applies. Nation's scan therefore has two lanes, and the second holds no
  batch. The init makes no call on that lane, and the repartition above it hands both merge lanes
  a handle, so the rollup walks as `SUM_BY_FLAG` does. If the walk nonetheless fails on the empty
  lane, that is not the id: diagnose it with `superpowers:systematic-debugging` and settle it with
  the reviewer. The spec asks for `TWO_LANES`, so do not drop to `ONE_LANE`.

- [ ] **Step 6: The contract row.** In `src/tests/executor_cases.rs`, add the imports
  `datafusion::arrow::array::RecordBatch`, `datafusion::arrow::datatypes::{DataType, Field}`,
  `datafusion::common::ScalarValue` and `crate::plan::{AggCall, AggregateBody, Expr, PlanAgg}`.
  Add a variant to `Shape`, after `SumByKeyAndGroupingId`:

```rust
    /// `sum(v) GROUP BY ROLLUP(k, v)`: an init expanding three sets and a merge folding them.
    /// The grouping id is part of the answer, in DataFusion's value and in its type.
    SumOverRollup,
```

  `Case::expect`'s doc gains: "or `k|v|id:type|sum` where the shape is the rollup". After `CASES`:

```rust
/// The rollup's state: the keys, the id the init appends, the sum.
pub(crate) const ROLLUP_STATE: [(&str, DataType); 4] = [
    ("k", DataType::Utf8),
    ("v", DataType::Int64),
    ("__grouping_id", DataType::UInt8),
    ("sum(v)", DataType::Int64),
];

/// The init over `INPUT`'s `(k, v)` and the merge over `ROLLUP_STATE`, as the planner writes a
/// two-key rollup: a mask entry is true where its key is masked.
pub(crate) fn rollup_bodies() -> (AggregateBody, AggregateBody) {
    let sum = |column: u32, name: &str| AggCall {
        func: PlanAgg::Sum,
        args: vec![Expr::column(column, name)],
        outputs: vec![Field::new("sum(v)", DataType::Int64, true)],
    };
    let init = AggregateBody {
        group_by: vec![Expr::column(0, "k"), Expr::column(1, "v")],
        grouping_sets: vec![vec![false, false], vec![false, true], vec![true, true]],
        null_exprs: vec![
            Expr::Literal(ScalarValue::Utf8(None)),
            Expr::Literal(ScalarValue::Int64(None)),
        ],
        aggs: vec![sum(1, "v")],
        finalize: None,
    };
    let merge = AggregateBody {
        group_by: vec![
            Expr::column(0, "k"),
            Expr::column(1, "v"),
            Expr::column(2, "__grouping_id"),
        ],
        grouping_sets: Vec::new(),
        null_exprs: Vec::new(),
        aggs: vec![sum(3, "sum(v)")],
        finalize: None,
    };
    (init, merge)
}

/// `k|v|id:type|sum(v)` per row. The id's type is in the cell: a device that numbers the sets
/// alike in another width is refused at the sink, and the value alone would not say so.
pub(crate) fn rollup_rows(batch: &RecordBatch) -> Vec<String> {
    let id_type = batch.column(2).data_type().to_string();
    (0..batch.num_rows())
        .map(|row| {
            (0..batch.num_columns())
                .map(|column| {
                    let cell = match ScalarValue::try_from_array(batch.column(column), row)
                        .expect("a value at every position")
                    {
                        ScalarValue::Utf8(Some(text)) => text,
                        other => other.to_string(),
                    };
                    if column == 2 { format!("{cell}:{id_type}") } else { cell }
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}
```

  The row, at the end of `CASES`. NULL renders as `NULL`, which sorts before lowercase:

```rust
    Case {
        name: "a rollup's grouping id is DataFusion's, in value and in type",
        shape: Shape::SumOverRollup,
        expect: &[
            "NULL|NULL|3:UInt8|21",
            "a|2|0:UInt8|2",
            "a|4|0:UInt8|4",
            "a|6|0:UInt8|6",
            "a|NULL|1:UInt8|12",
            "b|1|0:UInt8|1",
            "b|3|0:UInt8|3",
            "b|5|0:UInt8|5",
            "b|NULL|1:UInt8|9",
        ],
    },
```

  CPU half (`cpu_backend/tests/contract.rs`): split `merged` so that the batches can be read with
  their types.

```rust
fn merged(
    input: Schema,
    state: Schema,
    init: AggregateBody,
    merge: AggregateBody,
    output: Schema,
) -> Vec<String> {
    rendered(&merged_batches(input, state, init, merge, output))
}

/// The two nodes the planner stacks: an init per batch and a merge at done.
fn merged_batches(
    input: Schema,
    state: Schema,
    init: AggregateBody,
    merge: AggregateBody,
    output: Schema,
) -> Vec<CpuBatch> {
    let init_node = GpuAggregate::new(
        Given::of(input.clone(), BatchLayout::MultipleBatches),
        init,
        state.clone(),
        state.clone(),
    );
    let mut partial =
        CpuExec::aggregate(&init_node, &input.fields, ctx()).expect("the init builds");
    let merge_node = GpuAggregateBatches::new(
        Given::of(state.clone(), BatchLayout::MultipleBatches),
        merge,
        state.clone(),
        output,
    );
    let mut accumulator = CpuAccumulator::aggregate(&merge_node, &state.fields, ctx(), 1 << 20)
        .expect("the merge builds");
    for batch in batches() {
        let (partials, _) = partial.exec(batch).expect("the init runs");
        accumulator
            .accumulate_and_fetch(partials)
            .expect("the arrival is accepted");
    }
    let (emitted, _) = accumulator.mark_done_and_fetch().expect("done is accepted");
    emitted
}
```

  This is `merged`'s present body, ending in `emitted` where it rendered it. Import
  `ROLLUP_STATE`, `rollup_bodies` and `rollup_rows` beside `CASES`. The arm:

```rust
        Shape::SumOverRollup => {
            let state = columns(&ROLLUP_STATE);
            let (init, merge) = rollup_bodies();
            merged_batches(input, state.clone(), init, merge, state)
                .iter()
                .flat_map(|batch| rollup_rows(batch.record_batch()))
                .collect()
        }
```

  Device half (`gpu_backend/gpu_tests/contract.rs`): the same split of `merged_over`.

```rust
fn merged_over(
    source: Box<dyn GpuNode>,
    between: &[ArrowSchema],
    state: Schema,
    init: AggregateBody,
    merge: AggregateBody,
    output: Schema,
) -> Vec<String> {
    merged_over_batches(source, between, state, init, merge, output)
        .iter()
        .flat_map(rendered)
        .collect()
}
```

  `merged_over_batches` takes `merged_over`'s present body and doc comment, with
  `-> Vec<CpuBatch>`. Its tail after `mark_done_and_fetch` becomes:

```rust
    emitted
        .into_iter()
        .map(|batch| {
            session
                .export(&out)
                .unload(batch, RowRange::WHOLE)
                .expect("the rows cross the boundary")
                .0
        })
        .collect()
```

  Import the three items beside `CASES`. The arm:

```rust
        Shape::SumOverRollup => {
            let state = Schema::new(Arc::new(schema_of(&ROLLUP_STATE)));
            let (init, merge) = rollup_bodies();
            merged_over_batches(source_per_row_group(), &[], state.clone(), init, merge, state)
                .iter()
                .flat_map(|batch| rollup_rows(batch.record_batch()))
                .collect()
        }
```

- [ ] **Step 7: Local, before the fix.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- executor::cpu_backend::tests::contract
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib
timeout 5400 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 5400 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
```

  Expected: the contract passes on the cpu, which was already right, new row included. The whole
  `--lib` passes. `peacock_plan_tests`, with `test_grouping_id.cpp` linked in, and the gpu lib
  compile. A `ScalarValue` that renders NULL other than as `NULL` shows here as a cpu failure: fix
  the expectation from the cpu's actual rows, then check them against DataFusion's convention by
  hand. The device tests' red is seen in Step 10's red build.

- [ ] **Step 8: The fold.** In `aggregate.cpp`, above `execute_aggregate`:

```cpp
// DataFusion's `__grouping_id` (`group_id_array`, datafusion-physical-plan aggregates/mod.rs):
// the mask folds in key order, the first key highest, held at the narrowest unsigned width
// with a bit per key (`Aggregate::grouping_id_type`). GROUPING() over every key reads it, so
// the value is an answer. DataFusion refuses past 64 keys, and so does this.
static std::unique_ptr<cudf::column> grouping_id_column(const flatbuffers::Vector<uint8_t>* mask,
                                                        cudf::size_type nkeys,
                                                        cudf::size_type rows) {
  if (nkeys > 64)
    throw std::runtime_error("grouping sets over " + std::to_string(nkeys) +
                             " keys: DataFusion's grouping id holds at most 64");
  uint64_t gid = 0;
  for (cudf::size_type i = 0; i < nkeys; ++i) gid = (gid << 1) | (mask->Get(i) ? 1u : 0u);
  auto column = [&](auto width) {
    using T = decltype(width);
    cudf::numeric_scalar<T> id(static_cast<T>(gid), true);
    return cudf::make_column_from_scalar(id, rows);
  };
  if (nkeys <= 8) return column(uint8_t{});
  if (nkeys <= 16) return column(uint16_t{});
  if (nkeys <= 32) return column(uint32_t{});
  return column(uint64_t{});
}
```

  Add `#include <cstdint>`. In the grouping-set arm, the key loop loses the bit and its comment
  block (from "gid only has to be DISTINCT" through `int32_t gid = 0;`):

```cpp
      std::vector<cudf::column_view> set_keys;
      set_keys.reserve(nkeys);
      for (cudf::size_type i = 0; i < nkeys; ++i)
        set_keys.push_back(mask->Get(i) ? null_placeholders[i]->view() : key_cols[i]);
```

  The two `gid_s` lines become the one below, placed before anything releases `gk`:

```cpp
      cols.push_back(grouping_id_column(mask, nkeys, gk->num_rows()));
```

  In the arm's header comment, "tag rows with a distinct id" becomes "tag each set's rows with
  DataFusion's grouping id".

- [ ] **Step 9: Format and the local bar.**

```bash
git clang-format HEAD -- cpp/src/operators/aggregate.cpp
clang-format -i cpp/tests/gpu/hand_tables.hpp cpp/tests/gpu/test_grouping_id.cpp
rustfmt --edition 2024 --check peacockdb-core/src/tests/executor_cases.rs \
  peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs peacockdb-core/src/tests/gpu_tests/aggregate_schema_cases.rs \
  peacockdb-core/src/executor/cpu_backend/tests/contract.rs peacockdb-core/src/executor/gpu_backend/gpu_tests/contract.rs
timeout 5400 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 1800 ctest --test-dir cpp/build -L cpu --output-on-failure
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 5400 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
```

  Expected: green, with no new warning in either build. Check `wire/gpu_tests/mod.rs` by eye: it is
  a `mod.rs`, so rustfmt would reach past it.

- [ ] **Step 10: The device cycle.** `FIX=cpp/src/operators/aggregate.cpp`. One sync, a red build,
  a green build, as *Device cycle* says.

  Red set, one `ssh` each:

```bash
cpp/install/bin/peacock_plan_tests --gtest_filter='GroupingId.*'
cpp/install/rust-tests/peacockdb_core_gpu_lib --test-threads=1 \
  tests::gpu_tests::aggregate_cases::grouping_sets tests::gpu_tests::aggregate_schema_cases::grouping_sets \
  wire::gpu_tests:: executor::gpu_backend::gpu_tests::contract
```

  Expected red: all eight `GroupingId` cases FAIL. Seven fail with "the id is not an unsigned
  integer: type id 3" (INT32), and `SixtyFiveKeysAreRefusedByCount` fails with "answered". Seven
  Rust tests FAIL: both harness cases ("cpu and gpu differ"), the schema case (`2 __grouping_id:
  UInt8 vs INT32`), the rollup partial (`Int32` at position 2), the `GROUPING()` walk (`g` 2 where
  the oracle has 1), the narrowed-id walk (`Int32` where `UInt16` is expected, or the narrowing
  project's cast refusing the doubled id) and the contract (the rollup row). Every other walk
  passes.

  Green set, one `ssh` each:

```bash
cpp/install/bin/peacock_plan_tests
cpp/install/bin/peacock_gpu_tests
cpp/install/bin/peacock_join_session_tests
cpp/install/rust-tests/peacockdb_core_gpu_lib --test-threads=1 gpu_tests::
```

  Expected green: every case passes, the eight `GroupingId` ones and the seven Rust cases above
  among them. `peacock_plan_tests` runs whole, so K's `test_plan_executor.cpp` cases, built and
  never run until now, run here (#262's gtest proof). A K case red here whose cause is the DISTINCT
  lowering's device path is fixed here, red first, as its own pair of back-to-back builds, the test
  and then the fix carried by name; any other cause is a finding to settle with the reviewer
  before the commit. Create `llm-wiki/tasks/grouping-id-detail.md` with a *Device runs*
  table (build, tree, command, pass and fail counts per binary), and fill in both builds.

- [ ] **Step 11: The docs.**
  - `architecture.md`, *Grouping sets*. The paragraph that begins "The gid is a real column":
    replace the dash clause with "at the width DataFusion declares: `UInt8` up to 8 keys,
    `UInt16` to 16, `UInt32` to 32, `UInt64` to 64, and past 64 keys both engines refuse". Keep
    K's sentence about the inner DISTINCT stage if it is there. In the paragraph that begins "A
    masked column is a typed NULL", replace from "The ids are the bitmask" to its end with: "The
    id is DataFusion's on both engines. The mask folds in key order, the first key highest, so a
    two-key rollup is 0, 1, 3. The merge needs only one distinct id per set; `GROUPING()` over
    every key, which plans as a cast of the id, is why the value matters too."
  - `planner/translator/tests.rs`: the comment's second sentence becomes "The id's value and
    width are execution-side and not pinned here."
  - `tickets/corpus-coverage.md`, #65: one line above **Corpus queries**, "**Fixed by
    grouping-id** (`grouping_id_column`, `aggregate.cpp`). Its cells run in grouping-id's Task 5,
    which archives it." The ticket stays at 15 lines or fewer.
  - `build-test.md`. *Plan-executor (C++)* +8 (56 at 0753f8c9): its prose gains "the grouping id's
    value and width from one key to sixty-four, and the refusal past them", and where the row
    names its source files, `test_grouping_id.cpp` joins them. The C++ header +8. *Recipe walk on a
    device* +2 (21 at 0753f8c9): its last clause becomes "and the rollup's partial holding the
    `UInt8` id the plan declares", and `GROUPING()` over every key and the narrowed `UInt16` id
    under an eight-key DISTINCT rollup are named among the walks. `--lib -- gpu_tests::`, the gpu
    block and the Rust header each +2. *Operator harness, what the device holds*: recount its
    `bug_` pins and drop the #65 clause. *Executor contract, both engines*: one row more, "a
    rollup's grouping id in value and type". The grand total +10.
  - `rg -n '#65\b|t65\b' --glob '!llm-wiki/archive/**' --glob '!llm-wiki/tasks/**'`. Every hit
    left in code says something true.

- [ ] **Step 12: Commit.**

```bash
git add cpp/src/operators/aggregate.cpp cpp/tests/gpu/hand_tables.hpp cpp/tests/gpu/test_grouping_id.cpp \
        cpp/CMakeLists.txt peacockdb-core/src/tests peacockdb-core/src/executor/cpu_backend/tests/contract.rs \
        peacockdb-core/src/executor/gpu_backend/gpu_tests/contract.rs peacockdb-core/src/wire/gpu_tests/mod.rs \
        peacockdb-core/src/planner/translator/tests.rs llm-wiki/architecture.md llm-wiki/build-test.md \
        llm-wiki/tickets/corpus-coverage.md llm-wiki/tasks/grouping-id-detail.md
git status --short
git commit -F - <<'EOF'
#65: the device's grouping-set id is DataFusion's, in value and width

grouping_id_column folds the mask first key highest into UInt8/16/32/64 and
refuses past 64 keys; GroupingId gtests in test_grouping_id.cpp over the new
hand_tables.hpp. The four bug_ pins are agreement cases; a contract row, a
GROUPING() walk and a narrowed-id walk hold both engines to it.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

  If a K case needed a fix in Step 10, add its files, and the body gains one line naming that
  case and its fix.

---

### Task 3: A summed quotient and the DISTINCT lowering on a device (#55, #262)

**Files:**
- Modify: `peacockdb-core/src/wire/gpu_tests/mod.rs`
- Create and modify, only if #55 is live: `cpp/tests/gpu/test_summed_quotient.cpp`,
  `cpp/CMakeLists.txt`, `cpp/src/operators/aggregate.cpp`
- Modify: `llm-wiki/build-test.md`, `llm-wiki/tickets/corpus-coverage.md` (#55), `llm-wiki/tasks/grouping-id-detail.md`

**Interfaces:** none.

- [ ] **Step 1: The two queries**, beside the other consts:

```rust
const SUM_OF_QUOTIENTS: &str = "SELECT sum(per_flag) FROM (SELECT l_returnflag, \
     sum(l_extendedprice / l_linenumber) AS per_flag FROM lineitem GROUP BY l_returnflag)";
const DISTINCT_BESIDE_COMPANIONS: &str = "SELECT l_returnflag, count(DISTINCT l_suppkey), \
     avg(l_quantity), count(*) FROM lineitem GROUP BY l_returnflag";
```

- [ ] **Step 2: The tests.** After `each_lane_merges_its_own_state_before_the_cross_lane_merge_folds_them`:

```rust
/// q66's shape (#55). DataFusion casts the divisor of `sum(decimal / int)` in the partial
/// alone; a merge that evaluated the argument again over state would find no such column, or
/// divide twice. The inner sum merges on each lane and across them; the outer folds its two
/// partials in one merge.
#[tokio::test]
async fn a_summed_quotient_divides_once_in_the_partial_and_merges_by_reference() {
    let calls = assert_walk_matches_datafusion(SUM_OF_QUOTIENTS, TWO_LANES).await;
    assert_eq!(times(&calls, PARTIAL), 4, "an init per lane for each sum: {}", trail(&calls));
    assert_eq!(
        times(&calls, MERGE),
        5,
        "the inner sum's four merges and the outer's one: {}",
        trail(&calls)
    );
    assert_eq!(
        times(&calls, FINALIZE),
        3,
        "two inner finalizes, one outer: {}",
        trail(&calls)
    );
}

/// The DISTINCT lowering (#262). An inner stage grouping on the argument and the key runs
/// the companions' inits. The outer stage's init counts the deduplicated argument and runs
/// `avg`'s and `count`'s merge rules over the inner's state. DataFusion 45 answers these
/// shapes right; `avg` and `stddev` DISTINCT it does not.
#[tokio::test]
async fn a_distinct_count_beside_its_companions_runs_both_stages_on_a_device() {
    let calls = assert_walk_matches_datafusion(DISTINCT_BESIDE_COMPANIONS, TWO_LANES).await;
    assert_eq!(
        times(&calls, PARTIAL),
        4,
        "two stages, each its init on both lanes: {}",
        trail(&calls)
    );
    assert_eq!(
        times(&calls, FINALIZE),
        2,
        "the outer stage finalizes once per lane: {}",
        trail(&calls)
    );
}
```

  Add `(SUM_OF_QUOTIENTS, TWO_LANES)` and `(DISTINCT_BESIDE_COMPANIONS, TWO_LANES)` to the list in
  `the_kinds_a_device_has_run_are_the_kinds_this_file_claims`.

  The counts come from the plans' shapes at two lanes. The inner grouped sum is
  `SUM_BY_FLAG`'s (2 partials, 4 merges, 2 finalizes). The outer global sum over two
  single-batch lanes is q6's top without the lane merge: 2 partials, then 1 merge and its
  finalize on one lane. They are pinned from the trail, as the neighbours' are. If the trail
  differs, render the plan at `TWO_LANES` with `plan_text::render_plan` and
  `wire::render_plan_recipes` from a throwaway rust-only test (never committed), and pin what the
  recipes say. A trail that disagrees with its own recipes is a finding.

- [ ] **Step 3: Compile locally, then the device cycle.**

```bash
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 5400 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib
```

  No red build: the two walks prove behaviour the spec believes right, so green on their first run
  is the proof, and a red is the defect itself (Step 4 or Step 5 then makes its own pair). One sync
  and one build, as *Device cycle* says. Green set, one `ssh` each:

```bash
cpp/install/bin/peacock_plan_tests
cpp/install/bin/peacock_gpu_tests
cpp/install/bin/peacock_join_session_tests
cpp/install/rust-tests/peacockdb_core_gpu_lib --test-threads=1 wire::gpu_tests::
```

  Expected: green. Record the build in the detail file.
  - `a_summed_quotient…` green: #55 is stale.
  - `a_summed_quotient…` red: Step 4.
  - `a_distinct_count…` red: Step 5.

- [ ] **Step 4 (only if `a_summed_quotient…` is red): the defect is live.** Use
  `superpowers:systematic-debugging`. The trail names the call, and the error names its phase.
  If it is a `CudfAggregate{Merge}`, the merge evaluated the argument. The fix is in
  `execute_aggregate`: a merge reads each aggregate's state column by position, past the keys, as
  the Final branch of `get_values_col` already does, and never evaluates `func->args()`. The gtest
  comes first, in a new `cpp/tests/gpu/test_summed_quotient.cpp` appended to `peacock_plan_tests`'
  sources (`test_plan_executor.cpp` is past the cap), built from `hand_tables.hpp`: a
  `CudfScan` of `tpch.minimal`'s customer (path from `PEACOCK_TESTDATA_DIR`, as
  `test_plan_executor.cpp`'s `parquet_path` builds it) run alone through `hand::run` with no input;
  a Partial `sum(c_acctbal / CAST(c_nationkey AS Decimal128(10, 0)))` grouped on `c_nationkey` run
  over it, twice; a `CudfUnion` run over the two partials; and a Merge run over the union, whose
  sums must be twice the partial's, read with `hand::values_of<__int128_t>` per key. Carry the
  gtest file and `CMakeLists.txt` by name, build, run
  `cpp/install/bin/peacock_plan_tests --gtest_filter='SummedQuotient.*'`: red. Carry the fix, build
  back to back, run it and Step 3's green set: green.

- [ ] **Step 5 (only if `a_distinct_count…` is red): read the trail.** A failure in the lowering's
  device path (an outer init's merge rule, the `__distinct_arg` key) is fixed here, red first: a
  harness case in `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs` that reproduces it
  without the walk, carried by name and built red, then the fix carried and built green, back to
  back. Anything else gets a ticket (*Before you start*, ticket numbers), and the test becomes a
  `bug_` pin under it, with the number in a comment above.

- [ ] **Step 6: Docs.**
  - `tickets/corpus-coverage.md`, #55, one line above **Corpus queries**: "**Proven stale by
    grouping-id's** `a_summed_quotient_divides_once_in_the_partial_and_merges_by_reference`. q66's
    cells run in grouping-id's Task 5, which archives it." If Step 4 ran, the line reads
    "**Fixed by grouping-id**: a merge reads its state by position and never evaluates the
    argument (`execute_aggregate`); `a_summed_quotient_divides_once_in_the_partial_and_merges_by_reference`
    and `SummedQuotient.*` hold it. q66's cells run in grouping-id's Task 5, which archives it."
  - `build-test.md`: *Recipe walk on a device* +2. Its prose names the summed quotient and the
    two-stage DISTINCT beside its companions. `--lib -- gpu_tests::`, the gpu block, the Rust
    header and the grand total each +2. If Step 4 ran, *Plan-executor (C++)* +1, with the C++
    header and the total.

- [ ] **Step 7: Commit.**

```bash
git add peacockdb-core/src/wire/gpu_tests/mod.rs llm-wiki/build-test.md llm-wiki/tickets/corpus-coverage.md \
        llm-wiki/tasks/grouping-id-detail.md
git status --short
git commit -F - <<'EOF'
#55, #262: a summed quotient and the DISTINCT lowering walked on a device

The quotient divides once in the partial and merges by reference; a two-stage
DISTINCT beside avg and count matches DataFusion at two lanes.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

  If Step 4 ran, also add `cpp/src/operators/aggregate.cpp cpp/tests/gpu/test_summed_quotient.cpp
  cpp/CMakeLists.txt`, and the message's first line is "#55, #262: a merge reads its state by
  position; the DISTINCT lowering walked on a device". If Step 5 fixed code, add its files and
  name the fix in the body.

---

### Task 4: `tpch/rollup-grouping` on the cpu corpus

**Files:**
- Create: `testdata/tpch-queries/rollup-grouping.sql`
- Modify: `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`
- Modify: `testdata/goldens/tpch.sf1/{tp1-single,tp1-rowgroup,tp4-single,tp4-rowgroup,tp4-sized}.plans.txt`,
  the five `*-mini.cpu.txt` and `*-mini.cost.txt`, `mini.result.txt`, `duckdb-result.txt`
- Modify: `llm-wiki/build-test.md`

**Interfaces:** none.

- [ ] **Step 1: The declaration, red.** In `corpus_cases.inc`, after the `rollup_over_join` line,
  in the line shape of the file when this task builds:

```
// rollup-grouping is the one query reading the grouping id's value: GROUPING() over every key
// plans as the id cast to Int32. Its device cells run in grouping-id's last task.
corpus_query!(tpch, 1, rollup_grouping, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
```

  In `cost-registry.csv`, in alphabetical place among the tpch rows (after `rollup_distinct`,
  before `rollup_over_join`):

```
tpch,1,rollup_grouping,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,disabled,disabled,disabled,disabled,disabled,ok,rollup,65
```

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- rollup_grouping
```

  Expected: FAIL. The cases find no query file.

- [ ] **Step 2: The query**, `testdata/tpch-queries/rollup-grouping.sql`, as the spec gives it:

```sql
-- GROUPING() over the full key list reads the grouping-set id itself, so its value and width
-- are the answer, not just a tag that keeps the sets apart (#65).
select n_regionkey, n_nationkey, grouping(n_regionkey, n_nationkey) as g, count(*)
from nation
group by rollup (n_regionkey, n_nationkey);
```

- [ ] **Step 3: The plan goldens.**

```bash
UPDATE_CANONICAL=1 timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
git diff --stat testdata/goldens/
git status --short testdata/goldens/recipe-payloads.txt
```

  Expected: the five `tpch.sf1/*.plans.txt` files change, each gaining one `== rollup-grouping`
  section and nothing else (read the diff). Where a tp4 section shuffles, it hashes the keys and
  not `__grouping_id` (repartition-keys' fix for #189). `recipe-payloads.txt` is untouched. Then
  the verify run:

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
```

  Expected: PASS, the registry-against-goldens meta case and the payload cover among them.
  `GROUPING()` adds no fb kind or call shape: its cast rides a plain project.

- [ ] **Step 4: The cpu sections.** Merge only, never prune, on a filtered run:

```bash
PCK_UPDATE_SECTIONS=1 timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- rollup_grouping
git diff --stat testdata/goldens/tpch.sf1/
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- rollup_grouping
```

  Expected: the five `*-mini.cpu.txt`, the five `*-mini.cost.txt` and `mini.result.txt` gain a
  `rollup-grouping` section each, and no other section moves. The result is 31 rows: `g` is 0 on
  the 25 full rows, 1 on the five regions' subtotals, 3 on the grand total. Then
  `cpu_tpch_rollup_grouping_*` at all five modes pass against DataFusion. If the
  `duckdb_tpch_rollup_grouping` case is listed, it fails until step 5.

- [ ] **Step 5: DuckDB's section.** DuckDB 1.5.4's Python module, in a venv outside the repo
  (`python3` here has no `duckdb`, and `duckdb_result.py` refuses any other version):

```bash
[ -x /tmp/duckdb-1.5.4/bin/python ] || { python3 -m venv /tmp/duckdb-1.5.4 && /tmp/duckdb-1.5.4/bin/pip install duckdb==1.5.4; }
timeout 1800 /tmp/duckdb-1.5.4/bin/python testdata/duckdb_result.py --dataset tpch
git diff --stat testdata/goldens/tpch.sf1/duckdb-result.txt
git diff testdata/goldens/tpch.sf1/duckdb-result.txt | head -60
```

  Expected: one new `== rollup-grouping` section of 31 rows, with `g` in {0, 1, 3}. DuckDB's
  `grouping()` puts the first argument highest too. No other section moves. If one does, stop: the
  oracle's environment differs, and the diff is not this task's to commit.

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- rollup_grouping registry
```

  Expected: PASS, `duckdb_tpch_rollup_grouping` under `duckdb_exact` and the registry both ways
  among them. If the two answers differ only by column names, that is by position and not a
  divergence. A real difference is a `duckdb_divergent(<ticket>, <positions>)` with a new ticket,
  numbered as *Before you start* says.

- [ ] **Step 6: The rest of the cpu bar.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_cost_model
```

  Expected: PASS.

- [ ] **Step 7: Counts.** `build-test.md`: `test_cpu_corpus` +5 cpu cells and +1 DuckDB case, its
  table row and the cpu block header with them; the Rust header and the grand total by the same
  six. The *Corpus, cpu* prose's query count +1 and its cell count +5.

- [ ] **Step 8: Commit.**

```bash
git add testdata/tpch-queries/rollup-grouping.sql peacockdb-core/tests/common/corpus_cases.inc \
        testdata/cost-registry.csv testdata/goldens/tpch.sf1 llm-wiki/build-test.md
git status --short
git commit -F - <<'EOF'
#65: tpch/rollup-grouping on the cpu corpus

GROUPING() over every key reads the grouping id's value: 31 rows, g in {0, 1, 3},
against DataFusion and DuckDB at all five modes. Its device cells are off on 65.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 5: The device cells

**Files:**
- Modify: `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`
- Modify: `testdata/goldens/{tpch,tpcds,pbench}.sf1/gpu-result.txt`
- Modify: `llm-wiki/build-test.md`, `llm-wiki/tickets.md`, `llm-wiki/tickets/*.md`,
  `llm-wiki/archive/archived-tickets.md`, `llm-wiki/tasks/grouping-id-detail.md`

**Interfaces:** none.

One sync, two builds back to back: the trial build runs every candidate cell, and the final build
runs the whole device bar over the declaration as it is committed. No red build: the trial is the
experiment, and a cell's verdict decides its state.

- [ ] **Step 1: The rows.** From the registry as it stands, plus `rollup_over_join` whatever its
  tags:

```bash
awk -F, 'NR > 1 && ($NF ~ /(^| )(65|55|262)( |$)/ || $3 == "rollup_over_join")' testdata/cost-registry.csv
```

  Expected, give or take what J and K left: tpch `rollup_over_join`, `rollup_grouping`,
  `rollup_distinct`, `distinct_functions`; tpcds q5, q14, q18, q22, q28, q66, q77, q80 and q67;
  pbench `rollup_small_keys`. For each row, the candidate cells are the gpu modes still off
  where the cpu mode of the same name is enabled. q67's cpu is `na` (window functions, #32 and
  #143), so it has none and stays as it is. Write the candidate list into the detail file.

- [ ] **Step 2: The trial declaration.** Each candidate row's `gpu_modes` in `corpus_cases.inc`
  becomes every mode its cpu runs at. In `distinct_functions`' line, the gpu oracle `golden_exact`
  becomes `golden_approx_std`, and the line ends with this comment:

```
 // golden_approx_std: its stddev(DISTINCT)'s last digits move with the lanes, as shuffle-stddev's do
```

  Its `stddev(DISTINCT l_quantity)` is a Welford merge whose association order changes with the
  lanes, which is why `shuffle_stddev` is approximate. `rollup_distinct` and q28 stay
  `golden_exact`. The registry stays as it is, so the device's registry case fails on the trial
  build, and it is not run there. Compile locally:

```bash
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 5400 scripts/cargo-cudf.sh test -p peacockdb-core --test test_gpu_corpus --no-run
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- distinct_functions
```

  Expected: both green; the cpu cases do not read the gpu oracle.

- [ ] **Step 3: The sync and the trial build.** One sync and one build, as *Device cycle* says, with
  no red build. Then this one command (the trailing `_` stops `q5` matching `q50`), each cell
  writing its answer before asserting:

```bash
PCK_WRITE_GPU_RESULT=1 cpp/install/rust-tests/test_gpu_corpus --test-threads=1 \
  gpu_tpch_rollup_ gpu_tpch_distinct_functions_ \
  gpu_tpcds_q5_ gpu_tpcds_q14_ gpu_tpcds_q18_ gpu_tpcds_q22_ gpu_tpcds_q28_ \
  gpu_tpcds_q66_ gpu_tpcds_q77_ gpu_tpcds_q80_ gpu_pbench_rollup_small_keys_ 2>&1 | tee /tmp/cells.log; \
grep -E '^test .* (ok|FAILED)$' /tmp/cells.log
```

  Then bring the record home:

```bash
for d in tpch tpcds pbench; do
  timeout 600 rsync -a "$H:peacockdb-L/testdata/goldens/$d.sf1/gpu-result.txt" "testdata/goldens/$d.sf1/"
done
```

- [ ] **Step 4: Each cell's verdict**, into the detail file as a table (cell, pass or the first
  line of its failure, ticket):
  - **Passes:** it stays enabled.
  - **Fails on the id** (a `__grouping_id` width or value): fixed here, red first, as Task 2's
    cases are: the case that reproduces it carried by name and built red, then the fix carried
    and built green, back to back. Then the cell again on the green build.
  - **Fails on the quotient** (q66, a merge over the quotient's state) or **on the DISTINCT
    lowering's device path** (q28, `rollup_distinct`, `distinct_functions`: an outer init's
    merge rule, `__distinct_arg`, the narrowing project): fixed here the same way, red first in a
    harness case or a walk. Then the cell again.
  - **Differs only in a float's last digits under an exact oracle:** `distinct_functions` is the
    one row whose oracle Step 2 loosened, and only its `stddev(DISTINCT …)` column may differ so.
    The same miss on another row is a finding to settle with the reviewer, never a loosened oracle:
    the spec keeps `rollup_distinct` and q28 exact.
  - **Fails on anything else:** the open ticket it fails on, if one says so in its words.
    Otherwise a new ticket in the milestone file it blocks, numbered as *Before you start* says,
    with the query, the mode and the first line of the error, at most 15 lines. The cell goes
    off, and the row's tags gain the number.

  A cell that passes against the cpu golden can still differ from DuckDB. That is the
  `duckdb_gpu_*` case in Step 6, and it is ticketed the same way, with one exception: if
  `duckdb_gpu_tpch_distinct_functions_*` misses only in the `stddev(DISTINCT …)` column, within
  `1e-11` relative, under `duckdb_exact`, the line's DuckDB oracle becomes `duckdb_approx` (whose
  float rule is that bound) and its trailing comment ends "; duckdb_approx for the same reason".

- [ ] **Step 5: Declaration, registry and record agree.**
  - `corpus_cases.inc`: each row's `gpu_modes` are the cells that passed. Update the comments
    above these rows that name #65, #55, #262, or a ticket J closed, to say what holds now.
  - `cost-registry.csv`: the gpu cells as run. On each row, strike `65`, `55` and `262` if every
    off gpu cell there is explained by another ticket on the row (or none is off). Add the new
    tickets.
  - `gpu-result.txt`: the trial run wrote a section for every candidate, failures included.
    Delete the section of each cell that stays off, from its `== <query> mode=<mode>` line to the
    next `==`. The coverage guard in Step 6 checks the rest, both ways.

- [ ] **Step 6: The cpu bar.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_cost_model
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib
```

  Expected: PASS. That covers the registry against the cpu corpus both ways, the
  `gpu-result.txt` coverage guard, and a `duckdb_gpu_<dataset>_<query>_<mode>` case per enabled
  cell. A `duckdb_gpu_*` red is ticketed as in Step 4: the cell goes off and its section goes.
  Then this step runs again.

- [ ] **Step 7: The final build, back to back.** Carry the changed declaration, the registry and
  any file a Step 4 fix touched, by name, not in a second sync:

```bash
timeout 300 rsync -aR peacockdb-core/tests/common/corpus_cases.inc testdata/cost-registry.csv $H:peacockdb-L/
```

  Build (`$B`), then run each of these, one `ssh` each:

```bash
cpp/install/bin/peacock_gpu_tests
cpp/install/bin/peacock_plan_tests
cpp/install/bin/peacock_join_session_tests
cpp/install/rust-tests/peacockdb_core_gpu_lib --test-threads=1 gpu_tests::
cpp/install/rust-tests/test_gpu_corpus --test-threads=1
cpp/install/rust-tests/test_node_timing --test-threads=1
cpp/install/rust-tests/peacock_gpu_benchmarks --test-threads=1 --skip bench_
```

  Expected: every binary green. `test_gpu_corpus` runs whole, every enabled cell plus its
  registry case, which now matches. No `PCK_WRITE_GPU_RESULT` this time, so no file moves. Record
  both builds and their counts in the detail file, and the sf40 binaries, `--run-benchmarks` and
  Nsight as deferred.

- [ ] **Step 8: The tickets.** Move #65 and #262 to `archive/archived-tickets.md` under *Done*, and
  #55 under *Stale or obsolete*, or under *Done* if Task 3's Step 4 fixed it. Each keeps its text,
  plus one paragraph: closed by grouping-id, by what, the cells it turned on, and those left off
  under which tickets. Each ends "awaiting merge". In `tickets.md`, take the three numbers out of
  the corpus-coverage row, add the new tickets, and set the open count and the next free number.
  Run `rg -n '#(65|55|262)\b|t(65|55|262)\b' --glob '!llm-wiki/archive/**' --glob '!llm-wiki/tasks/**'`.
  Every link to the three now points at `archive/archived-tickets.md#tNN`.

- [ ] **Step 9: Counts and prose.** `build-test.md`: `test_gpu_corpus` gains one case per cell
  enabled here; its row's prose names them, along with the gpu block and the Rust header.
  `test_cpu_corpus` gains one `duckdb_gpu_*` case per cell enabled, the cpu block with them. The
  grand total by twice the cells enabled. Remove any mention of the three tickets as open.

- [ ] **Step 10: Commit.**

```bash
git add peacockdb-core/tests/common/corpus_cases.inc testdata/cost-registry.csv \
        testdata/goldens/tpch.sf1/gpu-result.txt testdata/goldens/tpcds.sf1/gpu-result.txt \
        testdata/goldens/pbench.sf1/gpu-result.txt llm-wiki
git status --short
git commit -F - <<'EOF'
#65, #55, #262: their device cells run; the three archived

Every candidate cell run on nebius-gpu against the cpu golden and DuckDB; the
passing ones enabled, the rest off under the tickets their registry rows name.
distinct-functions' gpu oracle is golden_approx_std, as shuffle-stddev's.
Tags 65, 55 and 262 struck from the rows no off cell needs them on.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

  If a Step 4 fix touched code, add its files, and the body gains one line naming the fix and the
  ticket it was under (the id, the quotient or the DISTINCT lowering).
