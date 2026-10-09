# welford-device implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A keyless `stddev` or `var` runs on the device, init and merge, beside any other
aggregate and over zero rows (#216); the Welford state is NULL-free on the device as it is on the
cpu, so an all-NULL group is `(0, 0.0, 0.0)` at every init and every merge; `MERGE_M2`'s count
type is chosen at compile time from cuDF's own version macros (#94, unless verify-26.02 already
gated it); `tpch/global-stddev` joins the corpus and pbench's `empty-dispersion-aggregates` runs on
the device.

**Architecture:** All in `cpp/src/operators/aggregate.cpp`, on the one request builder
aggregate-arms leaves, plus one rule in `cpp/src/plan_executor_internal.h`. `execute_aggregate`
sends a keyless node that holds a Welford function to `keyless_welford_aggregate`, which runs
`grouped_aggregate` on one constant `INT32` key, drops it, and over zero rows answers
`keyless_empty_state`'s one row. The two readbacks aggregate-arms names for welford-device — the
init's `$mean`/`$m2`, and the moments a Merge reads out of `MERGE_M2` — replace NULL with `0.0`.
`merge_m2_count_type(major, minor)` and `kMergeM2CountType` move to `plan_executor_internal.h`
(host-only, so the cpu gtests reach them), the constant chosen from `CUDF_VERSION_MAJOR` /
`CUDF_VERSION_MINOR` (`<cudf/version_config.hpp>`); no CMake change. No Rust production code
changes.

**Tech stack:** C++20 against cuDF 25.02 (`cpp/build`, local; nebius-gpu for the device) and
26.02 (`cpp/build26`, a local compile of the other branch); gtest; Rust harness cases
(`--features gpu`, compiled locally, run on the device); the cpu corpus (`--features rust-only`);
DuckDB 1.5.4 for the new query's oracle section.

**Spec:** [`welford-device.md`](welford-device.md). Built on
[`aggregate-arms-impl.md`](aggregate-arms-impl.md) (Task 6's *Interfaces*: `AggKind`,
`StateReadback`, `StateColumn`, `AggregateRequests`, `add_requests`, `append_state_columns`,
`merge_m2_input`, `kMergeM2CountType`, `grouped_aggregate`, `grouping_set_aggregate`; Task 5's
`column_at`, which brings `plan_executor_internal.h` into `aggregate.cpp`) and
[`keyless-identity-impl.md`](keyless-identity-impl.md) (`empty_state`,
`CallPattern::AtDoneIfNothingOut`, the init accumulators, `was_handed_nothing`, `zero_rows_of`, the
identity cases with `empty_state_row` and `at_done`). [`verify-26.02.md`](verify-26.02.md) (chain J)
may already have gated the count type: *Before you start* says how to tell. Format models:
[`guard-checks-impl.md`](guard-checks-impl.md), [`distinct-companions-impl.md`](distinct-companions-impl.md).

## Before you start: what will have moved

This plan was written against master at 64ced62e, where `aggregate.cpp` is the 833-line file with
the keyless `cudf::reduce` path at ~231-327, `make_reduce_agg` at ~117 and the Merge/Final
`MERGE_M2` arms at ~633-701. **None of that survives aggregate-arms**, which rewrites the file.
Write against the code as aggregate-arms-impl.md Task 6 leaves it, and **re-locate every site by
symbol, never by line**:

- **`aggregate.cpp` (aggregate-arms).** By symbol: `enum class AggKind`, `agg_kind`,
  `check_shape`, `value_of`, `enum class StateReadback` (its two welford-device readbacks, below),
  `struct StateColumn`, `struct AggregateRequests`, `add_requests`, `as_int64_count`,
  `append_state_columns`, `kMergeM2CountType`, `merge_m2_input`, `grouped_aggregate`,
  `grouping_set_aggregate`, `keyless_aggregate` (its stddev arm,
  `make_std_aggregation<cudf::reduce_aggregation>(func->ddof())`), and `execute_aggregate`'s
  `if (keys.empty()) return keyless_aggregate(…)`. A `count` at Merge is refused there (the plan
  merges a count with `sum`), and a computed argument is evaluated once per node. If
  `is_stddev_name` / `is_var_name` survived aggregate-arms, they go here (Task 3). If any of these
  names differs, follow the code and say so in the detail file.
- **`aggregate.cpp` (keyless-identity).** `execute_aggregate` builds its input as
  `was_handed_nothing(in) ? zero_rows_of(agg) : take_input(in)`: a done call reaches the routed
  path as a zero-row table, so this plan adds nothing for the no-input case.
- **`TableResult` (refcounted-scatter, J).** `owning(table, names)`, `select(ordinals)` (shares
  owners), `view()`, `num_rows()`, `num_columns()`; built only through its constructors
  (aggregate-arms Task 1 makes the default constructor private).
- **`MERGE_M2`'s count type may already be gated (verify-26.02, J).** verify-26.02 runs
  `shuffle-stddev` on 26.02, where it fails on #94 (`tickets/system-hardening.md`, #260), and its
  fix rule is `CUDF_VERSION_MAJOR`/`CUDF_VERSION_MINOR` from `<cudf/version_config.hpp>`. Before
  Task 1:

```bash
git log --oneline --grep='#94' -- cpp/src
git grep -n 'CUDF_VERSION_MAJOR\|CUDF_VERSION_MINOR' -- cpp/src ':!cpp/src/gpu_executor.cpp'
git grep -n 'kMergeM2CountType\|merge_m2_count_type' -- cpp
grep -n '<a id="t94">' llm-wiki/tickets/*.md llm-wiki/archive/archived-tickets.md
```

  (`gpu_executor.cpp`'s `peacock_cudf_version()` reads the same macros for another reason.)
  **No gate** — the log and the second grep print nothing, and `kMergeM2CountType` is
  aggregate-arms' bare `INT32`: Task 1 as written; #94 closes here (Task 5 Step 4). **A gate** —
  the count type is defined from those macros, or the log shows verify-26.02's #94 fix (even if
  aggregate-arms' rewrite left a bare `INT32` behind it): Task 1 Step 0 instead. #94 is then
  archived with verify-26.02, not here, and Task 5 makes no #94 or #260 edit. Record which in the
  detail file; Task 2's commit carries it.
- **gtests.** `cpp/tests/gpu/hand_tables.hpp` (`hand::column_of`, `values_of`, `table_of`,
  `ref`, `run`) and `cpp/tests/gpu/test_aggregate_builder.cpp` (anonymous-namespace `func`,
  `aggregate`, `rows`, `one`, `by_key`, `refusal`) are aggregate-arms'; this plan adds to both.
  `test_plan_executor.cpp` is past the 1000-line cap, and keyless-identity's `AggregateNoInput.*`
  sit in their own file: nothing new goes in either.
- **Harness (keyless-identity).** Every `GpuAggregate` case runs `Script::Accumulate`, not
  `Script::Exec`: an init over one batch answers its state at slot 0 and an empty done slot.
  The identity cases (`DEVICE_IDENTITY`, `keyless_init`, and the `pub(crate)` `empty_state_row`
  and `at_done`) are in `aggregate_cases.rs`, or in `aggregate_identity_cases.rs` if
  keyless-identity moved them there for size; find them with
  `git grep -n DEVICE_IDENTITY peacockdb-core/src`.
- **Harness (aggregate-arms).** `welford_answered` no longer borrows the device's names; the four
  #216 pins in `aggregate_dimension_cases.rs` carry moved messages (`global_stddev_finalize`'s is
  `column_at`'s `"is past the 1 columns its input has"`), and the schema pin's divergence lost
  its second name. Re-locate them by the names in the spec's *Tests*.
- **Corpus (duckdb-oracle, J; keyless-identity).** Every `corpus_query!` has a `duckdb_oracle`
  argument before the cpu oracle; `all_modes` exists. `tpch/rollup-stddev`'s line is
  aggregate-arms'; copy its shape. pbench's `empty_dispersion_aggregates` line is
  keyless-identity's, gpu modes `none`, with a `// device: #216` comment; its registry row's
  tickets are `199 216`. **Build Task 5's row list from the registry as it stands**, not from this
  plan.
- **Scripts (verify-26.02, J).** If `scripts/build.sh` takes `--build-dir` (`grep -n -- --build-dir
  scripts/build.sh`), verify-26.02's 26.02 mode landed, and Task 1 Step 5 uses it.
- **The GPU host's directory.** All three upstream plans of chain L use `~/peacockdb-L`, as the
  chain header says; so does this one.

## Global constraints

- **Every commit green.** A `bug_` test that a task's fix turns red is flipped in that task's
  commit to the agreement case, under a name without `bug_` (`coding-style.md`, *Building around
  a bug*). `global_stddev`'s and `empty_dispersion_aggregates`' device cells stay off under `216`
  until Task 5's run writes their `gpu-result.txt` sections.
- **Device evidence, red first.** Tasks 2 and 3 each run their device tests on nebius-gpu before
  they commit: one sync, a red build without the fix, a green build with it (*Device cycle*). A
  predicted red that passes, or fails for another reason, stops the task until it is understood
  (`superpowers:systematic-debugging`). Task 5 syncs once for the corpus. Tasks 1 and 4 have no
  device step: on 25.02 Task 1 compiles the type the device already ran, which Tasks 2 and 3's
  merges exercise, and Task 4's device cells stay off.
- **No Rust production change, no wire change, no ABI or facade change.** `recipe-payloads.txt`
  and every existing `.plans.txt` section unchanged; the new query adds five sections.
- **Out of scope, from the spec's Restriction.** The keyless `cudf::reduce` path for every node
  with no Welford; a Welford companion beside a DISTINCT (#261); any 26.02 device run (#260,
  verify-26.02). The 26.02 branch is compiled, and its cpu gtests run, locally (Task 1).
- **Builds** as `build-test.md` documents them, each command under `timeout`: rust-only with
  `cargo test --features rust-only`; C++ in `cpp/build` with
  `scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build`
  then `ctest --test-dir cpp/build -L cpu`; the cudf Rust shape with
  `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh`. `cpp/build26` stays
  26.02. Work in a workspace, never the primary checkout; never share a cargo target dir across
  worktrees. The workspace needs the sf1 parquet under `testdata/` (gitignored): symlink it from
  the primary checkout if absent. Never pipe a build into `tail` without `set -o pipefail`: the
  status would be `tail`'s.
- **The C++ tests that need device memory run on nebius-gpu, not in `ctest -L cpu`.**
  `peacock_plan_tests` (where `test_aggregate_builder.cpp` links) is labelled `gpu`; only
  `peacock_cpu_tests` runs locally.
- **Done**, as chain L's header defines it: CI green except the `gpu-tests` job (shad-gpu), and
  this task's device tests passed on nebius-gpu, each run recorded (host, tree, per-binary
  result) in `llm-wiki/tasks/welford-device-detail.md`. The sf40 binaries, `--run-benchmarks`
  and Nsight are out, recorded there as deferred.
- Formatting on changed lines only: `git clang-format HEAD -- <files>`; `rustfmt --edition 2024
  --check <leaf>` on touched leaves, never a `mod.rs`. Files under 1000 lines, functions under
  150. Commit messages at most 10 lines, ending with the `Co-Authored-By` line shown.

## Device cycle

Chain L's header: **one sync per task** to nebius-gpu, with as many back-to-back builds as the
task's red/green pairs need, the red build first. Tasks 2 and 3 have one pair each, run after the
task's tests and fix are written and every local tier passes, and before its commit — the same
mechanism as aggregate-arms': one sync carries the working tree and the fix's reverse; the red
build is the tree with the fix reversed, the green build the tree as written. Each task names its
`FIX` (the paths whose diff is the fix; never a test the red build must run), its red set and its
green set. Every command in the foreground: a backgrounded build chain dies mid-build with no
error. If `ssh` is refused at once while the host answers a raw socket, the sandbox is blocking
the client, not the host.

```bash
GPU=dmitry@89.169.109.150   # nebius-gpu, as chain L's header names it
DIR=peacockdb-L             # the header's directory
# The 25.02 env's lib: chain J's header says ~/data/miniforge3, #260 says ~/miniforge3. Use the
# one that exists in CUDF_LIB, and record it in the detail file.
timeout 60 ssh "$GPU" 'ls -d ~/data/miniforge3/envs/rapids-cuda-12.2/lib ~/miniforge3/envs/rapids-cuda-12.2/lib'
CUDF_LIB='$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib'
B="cd ~/$DIR && . ~/peacock-env.sh && ./scripts/build-test-shadgpu.sh --build"
R="cd ~/$DIR && . ~/peacock-env.sh && export LD_LIBRARY_PATH=\$PWD/cpp/install/lib:$CUDF_LIB PEACOCK_TESTDATA_DIR=\$PWD/testdata &&"
# The fix's reverse.
timeout 60 git diff -R HEAD -- $FIX > /tmp/welford-device-red.patch
# The one sync: the working tree, uncommitted, and the reverse beside it.
timeout 1200 rsync -a --delete-after --exclude=.git --filter=':- .gitignore' ./ "$GPU:$DIR/"
timeout 300 rsync -a /tmp/welford-device-red.patch "$GPU:welford-device-red.patch"
# Red build: the tests without the fix; then the task's red set, each as `timeout … ssh "$GPU" "$R …"`.
timeout 300 ssh "$GPU" "cd ~/$DIR && patch -p1 < ~/welford-device-red.patch"
timeout 10800 ssh "$GPU" "$B"
# Green build: the fix back in; then the task's green set.
timeout 300 ssh "$GPU" "cd ~/$DIR && patch -p1 -R < ~/welford-device-red.patch"
timeout 10800 ssh "$GPU" "$B"
```

Every case the task names as red must fail, for the reason it names; a case red for another
reason, or green where the task expects red, is a finding to settle before the fix
(`superpowers:systematic-debugging`): a test that passes without its fix may assert nothing.
Every case of the green set passes. One binary per `ssh`, so one red does not hide the next. A
fresh `~/$DIR` has no sf1 parquet: copy it from chain J's `~/peacockdb-J/testdata`. Every Rust
binary takes `--test-threads=1`. Not `peacock_cpu_tests` (local), not the sf40 pair, not
`--run-benchmarks`.

## Review focus

Five input classes nothing tests today, most likely to bite, each pinned in its owning task:

1. **A keyless Welford beside a decimal sum, with rows and over none.** The routed node's sum is a
   groupby `SUM` now, not `cudf::reduce`; over zero rows its NULL must keep the field's scale,
   which the type id alone cannot carry. Expected: the cpu's answer with rows, and
   `empty_state`'s row (count 0, `(0, 0.0, 0.0)`, a `Decimal128(28, 2)` NULL) over nothing.
   Task 3: `AggregateBuilder.AKeylessWelfordOverZeroRowsAnswersItsEmptyState` and
   `a_keyless_stddev_beside_a_decimal_sum_agrees_over_rows_and_over_nothing`.
2. **One row: the sample forms NULL, the population forms 0.** The finalize's `count − ddof ≤ 0`
   branch, on a keyless node that finalizes itself. Expected: `stddev_samp` and `var_samp` NULL,
   `stddev_pop` and `var_pop` `0.0`, on both. Task 3:
   `one_row_is_null_for_the_sample_forms_and_0_for_the_population_forms_on_both`.
3. **A merge of a lane whose triple is `(0, 0.0, 0.0)` with a real one, and of only such lanes.**
   Expected: the real state unchanged; only empty states merge to `(0, 0.0, 0.0)`, not NULL.
   Task 2: `AggregateBuilder.AnEmptyStateMergedWithARealOneIsTheRealOne`,
   `AggregateBuilder.AMergeOfOnlyEmptyStatesIsZeroNotNull`, and
   `a_group_with_values_on_one_lane_and_nulls_on_another_merges_as_the_cpu`, where each engine
   merges its own init's states.
4. **`-0.0` and `NaN` values.** `replace_nulls` must not touch a NaN, and a NaN must not count as
   NULL. Expected: a group holding a NaN has count 2 and NaN moments on both; a group of one
   `-0.0` has mean `0.0` (either sign) and m2 `0.0`. Task 2:
   `AggregateBuilder.ANaNAndANegativeZeroAreValuesNotNulls` and
   `a_nan_and_a_negative_zero_are_values_on_both`.
5. **The other cuDF's branch.** The `INT64` branch is compiled only by CI's "26.02" leg (a 25.10a
   image, #129) and by Task 1's local 26.02 compile, and never run on a device. Expected: the rule
   picks `INT32` at 25.02 and 25.04, `INT64` at 25.06 and later, and the constant the aggregate
   casts to is the rule at the headers' own `CUDF_VERSION_MAJOR`/`_MINOR`. Task 1:
   `MergeM2CountType.*` in `peacock_cpu_tests`, run locally against 25.02 (`cpp/build`) and 26.02
   (`cpp/build26`).

## File structure

| file | responsibility |
|---|---|
| `cpp/src/plan_executor_internal.h` | `merge_m2_count_type`, `kMergeM2CountType` from `<cudf/version_config.hpp>` (Task 1) |
| `cpp/src/operators/aggregate.cpp` | its own `kMergeM2CountType` goes (Task 1); the two readbacks zero-fill (Task 2); the keyless route and `keyless_empty_state`; the stddev reduce arm goes (Task 3) |
| `cpp/tests/cpu/test_executor.cpp` | `MergeM2CountType.*` (Task 1) |
| `cpp/tests/gpu/hand_tables.hpp` | `hand::nullable_column_of` (Task 2) |
| `cpp/tests/gpu/test_aggregate_builder.cpp` | the all-NULL, NaN and merge gtests (Task 2); the keyless gtests (Task 3) |
| `peacockdb-core/src/tests/gpu_tests/welford_cases.rs` (new), `gpu_tests/mod.rs` | the all-NULL, NaN, lane-merge cases (Task 2); the keyless cases (Task 3) |
| `peacockdb-core/src/tests/gpu_tests/aggregate_dimension_cases.rs`, `aggregate_schema_cases.rs`, `aggregate_cases.rs` (or `aggregate_identity_cases.rs`) | the #216 pins flip; `EVERY_FUNC` (Task 3) |
| `testdata/tpch-queries/global-stddev.sql` (new), `testdata/goldens/tpch.sf1/`, `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv` | the new query (Task 4); its device cells, empty-dispersion's (Task 5) |
| `testdata/goldens/{tpch,pbench}.sf1/gpu-result.txt` | the device's sections (Task 5) |
| `llm-wiki/tasks/welford-device-detail.md` | every device run, the gate found or not, deferrals (Tasks 1-3, 5) |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets/{corpus-coverage,complete-coverage,system-hardening}.md` | docs, counts, ticket facts (Task 5) |

---

### Task 1: `MERGE_M2`'s count type, chosen from cuDF's own version macros (#94)

**Files:**
- Modify: `cpp/src/plan_executor_internal.h`, `cpp/src/operators/aggregate.cpp`
  (`kMergeM2CountType`'s definition goes)
- Test: `cpp/tests/cpu/test_executor.cpp`

**Interfaces:**
- Produces (`plan_executor_internal.h`, namespace `peacock`; host-only, so `peacock_cpu_tests`
  reaches both):

```cpp
// The type cuDF's MERGE_M2 takes its count child in: INT32 before 25.06, and INT64 from 25.06,
// where cuDF #18546 widened it against overflow and refused INT32 (#94).
constexpr cudf::type_id merge_m2_count_type(int major, int minor) {
  return major < 25 || (major == 25 && minor < 6) ? cudf::type_id::INT32 : cudf::type_id::INT64;
}

// The linked cuDF's, from its own headers: each cuDF version is its own build, so no runtime
// probe and no second source of the version. merge_m2_input casts to it; the gate goes when
// 25.02 does.
inline constexpr cudf::type_id kMergeM2CountType =
    merge_m2_count_type(CUDF_VERSION_MAJOR, CUDF_VERSION_MINOR);
```

- Changes (aggregate-arms' ⟵ hook): `aggregate.cpp`'s own `constexpr cudf::type_id
  kMergeM2CountType = cudf::type_id::INT32;` goes; `merge_m2_input` and `as_int64_count` read the
  header's.

- [ ] **Step 0: Only if verify-26.02 already gated the site** (*Before you start*). Read the gate
  and confirm three things, recording each in the detail file:
  1. the type `merge_m2_input` casts the count to is the gated one, so it covers the one site
     aggregate-arms leaves (aggregate-arms' rewrite did not put back a bare `INT32`);
  2. it is `INT32` before 25.06 and `INT64` from 25.06, from `CUDF_VERSION_MAJOR` and
     `CUDF_VERSION_MINOR`;
  3. a `peacock_cpu_tests` case pins the type this build picks under 25.02's
     `version_config.hpp`.

  All three hold: Task 1 changes no file and makes no commit; go to Task 2. Where one fails,
  make only that change with Steps 1-6: point `merge_m2_input` at the gate (1); set the boundary
  to 25.06, which is when cuDF #18546 landed (2); add Step 1's tests, moving the gate's rule into
  `plan_executor_internal.h` as *Interfaces* writes it, so the cpu gtests reach it (3). #94 stays
  verify-26.02's either way: use Step 6's second message.

- [ ] **Step 1: The rule's tests, failing.** In `test_executor.cpp`, `#include
  <cudf/version_config.hpp>` beside its other includes (an `#if` over an undeclared macro reads
  0 and would pick the wrong branch silently), and at the end of the file:

```cpp
// #94: MERGE_M2 takes its count child as INT32 before cuDF 25.06 and as INT64 from 25.06
// (cuDF #18546).
TEST(MergeM2CountType, Int32Before25_06AndInt64From25_06) {
  EXPECT_EQ(peacock::merge_m2_count_type(25, 2), cudf::type_id::INT32) << "25.02, the GPU hosts'";
  EXPECT_EQ(peacock::merge_m2_count_type(25, 4), cudf::type_id::INT32);
  EXPECT_EQ(peacock::merge_m2_count_type(25, 6), cudf::type_id::INT64);
  EXPECT_EQ(peacock::merge_m2_count_type(25, 10), cudf::type_id::INT64);
  EXPECT_EQ(peacock::merge_m2_count_type(26, 2), cudf::type_id::INT64) << "26.02";
}

// The type the aggregate casts to, as this build's cuDF headers choose it. 25.02 is the GPU
// hosts'; the other builds there are (CI's 25.10a leg, the workstation's 26.02) compile INT64.
TEST(MergeM2CountType, TheLinkedCudfChoosesIt) {
#if CUDF_VERSION_MAJOR == 25 && CUDF_VERSION_MINOR == 2
  EXPECT_EQ(peacock::kMergeM2CountType, cudf::type_id::INT32) << "cuDF 25.02";
#else
  EXPECT_EQ(peacock::kMergeM2CountType, cudf::type_id::INT64)
      << "cuDF " << CUDF_VERSION_MAJOR << "." << CUDF_VERSION_MINOR;
#endif
}
```

```bash
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
```

  Expected: FAIL to compile, `'merge_m2_count_type' is not a member of 'peacock'` and the same
  for `kMergeM2CountType`.

- [ ] **Step 2: The rule and the constant.** In `plan_executor_internal.h`, add
  `#include <cudf/version_config.hpp>` beside `<cudf/types.hpp>` and *Interfaces*' two
  declarations after `cudf_ast_can_evaluate`. `<cudf/version_config.hpp>` ships in 25.02 and
  26.02 alike (25.02's defines `25`/`2`, 26.02's `26`/`2`).

- [ ] **Step 3: The aggregate reads it.** In `aggregate.cpp` (which includes
  `plan_executor_internal.h` for `column_at`), delete aggregate-arms' `kMergeM2CountType` and its
  `⟵ welford-device` comment, and put above `merge_m2_input`:

```cpp
// MERGE_M2's count child goes in as kMergeM2CountType (plan_executor_internal.h), the type the
// linked cuDF takes; as_int64_count widens the merged count back to the wire's INT64, a no-op
// from 25.06.
```

  Nothing else in `merge_m2_input` or `as_int64_count` changes.

- [ ] **Step 4: Build and run against 25.02.**

```bash
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
```

  Expected: clean build, no new warnings; `-L cpu` PASS, both `MergeM2CountType` cases included
  (`TheLinkedCudfChoosesIt` at `INT32`).

- [ ] **Step 5: Compile and run the other branch against 26.02, locally.** If verify-26.02's
  26.02 mode landed (*Before you start*):

```bash
timeout 5400 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids --gcc-version 14 \
  --build-dir cpp/build26 --install-dir cpp/build26/install --build
```

  Otherwise configure `cpp/build26` the way `build-test.sh` does and build only the cpu gtests and
  the library under them:

```bash
(
  export PATH=$HOME/data/miniforge3/envs/rapids/bin:$PATH \
         CC=/usr/bin/gcc-14 CXX=/usr/bin/g++-14 \
         CUDACXX=$HOME/data/miniforge3/envs/rapids/bin/nvcc \
         LDFLAGS="-Wl,-rpath-link,$HOME/data/miniforge3/envs/rapids/lib"
  timeout 1800 cmake -S cpp -B cpp/build26 -G Ninja -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_CUDA_ARCHITECTURES="80;90" "-DCMAKE_JOB_POOLS=link_pool=1" -DCMAKE_JOB_POOL_LINK=link_pool \
    -Dcudf_ROOT=$HOME/data/miniforge3/envs/rapids &&
  timeout 5400 cmake --build cpp/build26 --target peacock_cpu_tests --parallel "$(nproc)"
)
```

  Then, either way:

```bash
LD_LIBRARY_PATH=$HOME/data/miniforge3/envs/rapids/lib timeout 600 \
  ctest --test-dir cpp/build26 -L cpu --output-on-failure -R peacock_cpu_tests
```

  Expected: `aggregate.cpp` compiles with `kMergeM2CountType == INT64`; `MergeM2CountType.*`
  PASS (`TheLinkedCudfChoosesIt` at `INT64`, `cuDF 26.2`). If the 26.02 configure fails for a
  reason outside this task (a fresh `cpp/build26` in the workspace fetches flatbuffers), record it
  in the detail file and rely on CI's leg; do not change the build scripts here.

- [ ] **Step 6: Commit.** `git status --short` lists only the three files.

```bash
git clang-format HEAD -- cpp/src/plan_executor_internal.h cpp/src/operators/aggregate.cpp cpp/tests/cpu/test_executor.cpp
git add cpp/src/plan_executor_internal.h cpp/src/operators/aggregate.cpp cpp/tests/cpu/test_executor.cpp
git commit -m "#94: MERGE_M2's count type chosen from cuDF's version macros

INT32 before 25.06, INT64 from 25.06 (cuDF #18546), read from
<cudf/version_config.hpp>; merge_m2_count_type is the rule, and a cpu
gtest pins the type each build's headers pick.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

  Where Step 0 found verify-26.02's gate and completed it, the message keeps #94 verify-26.02's:

```bash
git commit -m "MERGE_M2's count type: verify-26.02's gate completed

What Step 0 found missing (the site, the 25.06 boundary or the cpu
gtest) is added; the gate, and #94, are verify-26.02's.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: The Welford state is NULL-free: all-NULL groups are `(0, 0.0, 0.0)`

The builder is one for the grouped, grouping-set and (after Task 3) keyless paths, so this lands
once. cuDF answers NULL in two places DataFusion answers `0.0`: `MEAN` and `M2` over a group with
no valid value, and `MERGE_M2` for a group whose merged counts are all 0. Both readbacks replace
it.

**Files:**
- Modify: `cpp/src/operators/aggregate.cpp` (`StateReadback`, `add_requests`' Welford arm,
  `append_state_columns`' `MergedChild` arm; a static `zero_for_null`)
- Modify: `cpp/tests/gpu/hand_tables.hpp`; Test: `cpp/tests/gpu/test_aggregate_builder.cpp`
- Create: `peacockdb-core/src/tests/gpu_tests/welford_cases.rs`; Modify: `gpu_tests/mod.rs`

**Interfaces:**
- Extends aggregate-arms' two readbacks (its ⟵ hooks): the init's `$mean`/`$m2`, which
  `add_requests`' Welford `Partial` arm reads `AsIs`, and the merged mean and m2 that
  `append_state_columns`' `MergedChild` arm reads out of `MERGE_M2`'s struct (children 1 and 2):

```cpp
/// How one state column is read back out of the groupby's results.
enum class StateReadback {
  AsIs,
  CountToInt64,  // cuDF's COUNT answers INT32; every count on the wire is INT64
  NullAsZero,    // a Welford init's mean or m2: NULL over a group of no valid value, 0.0 here
  MergedChild,   // one child of MERGE_M2's struct: the count widened to INT64, a moment NullAsZero
};
```

- Produces (test-only, `hand_tables.hpp`): `hand::nullable_column_of<T>(values, valid)`.

- [ ] **Step 1: The nullable column helper.** `hand_tables.hpp`, add `#include <cudf/null_mask.hpp>`
  and, after `column_of`:

```cpp
/// `values` as a column, NULL wherever `valid` is false.
template <typename T>
std::unique_ptr<cudf::column> nullable_column_of(std::vector<T> const& values,
                                                 std::vector<bool> const& valid) {
  auto column = column_of(values);
  std::vector<cudf::bitmask_type> words((values.size() + 31) / 32, 0);
  cudf::size_type nulls = 0;
  for (size_t i = 0; i < valid.size(); ++i) {
    if (valid[i]) words[i / 32] |= cudf::bitmask_type{1} << (i % 32);
    else ++nulls;
  }
  rmm::device_buffer mask(cudf::bitmask_allocation_size_bytes(values.size()),
                          cudf::get_default_stream());
  cudaMemcpy(mask.data(), words.data(), words.size() * sizeof(cudf::bitmask_type),
             cudaMemcpyHostToDevice);
  column->set_null_mask(std::move(mask), nulls);
  return column;
}
```

- [ ] **Step 2: The gtests, failing on the device.** `test_aggregate_builder.cpp`, after
  `AFunctionWhoseStateNamesDoNotMatchItsColumnsIsRefused` (add `#include <cmath>`):

```cpp
// --- The Welford state is never NULL -------------------------------------------------------
//
// DataFusion's state for a group with no valid value is (0, 0.0, 0.0)
// (VarianceGroupsAccumulator::state). cuDF answers NULL there: MEAN and M2 over no valid
// value, and MERGE_M2 wherever every merged count is 0 (group_merge_m2.cu).

static flatbuffers::Offset<fb::AggregateFuncNode> merged_stddev(flatbuffers::FlatBufferBuilder& fbb,
                                                                uint32_t at) {
  return func(fbb, "stddev",
              {hand::ref(fbb, at, "s$count"), hand::ref(fbb, at + 1, "s$mean"),
               hand::ref(fbb, at + 2, "s$m2")},
              {"s$count", "s$mean", "s$m2"}, 1);
}

/// [k, s$count, s$mean, s$m2] from host values, one state per row.
static peacock::TableResult states(std::vector<int32_t> k, std::vector<int64_t> n,
                                   std::vector<double> mean, std::vector<double> m2) {
  std::vector<std::unique_ptr<cudf::column>> c;
  c.push_back(hand::column_of<int32_t>(k));
  c.push_back(hand::column_of<int64_t>(n));
  c.push_back(hand::column_of<double>(mean));
  c.push_back(hand::column_of<double>(m2));
  return hand::table_of(std::move(c), {"k", "s$count", "s$mean", "s$m2"});
}

TEST(AggregateBuilder, AWelfordInitOverAnAllNullGroupIsZeroNotNull) {
  // k 1 holds 10 and 20; k 2 holds only NULLs.
  std::vector<std::unique_ptr<cudf::column>> c;
  c.push_back(hand::column_of<int32_t>({1, 1, 2, 2}));
  c.push_back(hand::nullable_column_of<int64_t>({10, 20, 0, 0}, {true, true, false, false}));
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Partial, {hand::ref(fbb, 0, "k")}, {"k"},
                        {func(fbb, "stddev", {hand::ref(fbb, 1, "v")},
                              {"s$count", "s$mean", "s$m2"}, 1)});
  auto out = hand::run(fbb, node, one(hand::table_of(std::move(c), {"k", "v"})));
  for (int at : {2, 3}) EXPECT_EQ(out.view().column(at).null_count(), 0) << out.column_names.at(at);
  EXPECT_EQ((by_key<int64_t>(out, 1)), (std::map<int32_t, int64_t>{{1, 2}, {2, 0}}));
  EXPECT_EQ((by_key<double>(out, 2)), (std::map<int32_t, double>{{1, 15.0}, {2, 0.0}}));
  EXPECT_EQ((by_key<double>(out, 3)), (std::map<int32_t, double>{{1, 50.0}, {2, 0.0}}));
}

TEST(AggregateBuilder, AMergeOfOnlyEmptyStatesIsZeroNotNull) {
  // k 1: two lanes that held only NULLs for it; k 2: one real state.
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Merge, {hand::ref(fbb, 0, "k")}, {"k"},
                        {merged_stddev(fbb, 1)});
  auto out = hand::run(fbb, node, one(states({1, 1, 2}, {0, 0, 3}, {0.0, 0.0, 2.0},
                                             {0.0, 0.0, 2.0})));
  for (int at : {2, 3}) EXPECT_EQ(out.view().column(at).null_count(), 0) << out.column_names.at(at);
  EXPECT_EQ((by_key<int64_t>(out, 1)), (std::map<int32_t, int64_t>{{1, 0}, {2, 3}}));
  EXPECT_EQ((by_key<double>(out, 2)), (std::map<int32_t, double>{{1, 0.0}, {2, 2.0}}));
  EXPECT_EQ((by_key<double>(out, 3)), (std::map<int32_t, double>{{1, 0.0}, {2, 2.0}}));
}

TEST(AggregateBuilder, AnEmptyStateMergedWithARealOneIsTheRealOne) {
  // A lane that held only NULLs for k 1 beside one that held four values: the empty lane
  // must not pull the mean toward 0 or add to the count.
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Merge, {hand::ref(fbb, 0, "k")}, {"k"},
                        {merged_stddev(fbb, 1)});
  auto out = hand::run(fbb, node, one(states({1, 1}, {0, 4}, {0.0, 4.0}, {0.0, 8.0})));
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(1)), std::vector<int64_t>{4});
  EXPECT_EQ(hand::values_of<double>(out.view().column(2)), std::vector<double>{4.0});
  EXPECT_EQ(hand::values_of<double>(out.view().column(3)), std::vector<double>{8.0});
}

TEST(AggregateBuilder, ANaNAndANegativeZeroAreValuesNotNulls) {
  // k 1: 1.0 and NaN, whose moments are NaN in DataFusion's Welford too; k 2: one -0.0.
  std::vector<std::unique_ptr<cudf::column>> c;
  c.push_back(hand::column_of<int32_t>({1, 1, 2}));
  c.push_back(hand::column_of<double>({1.0, std::nan(""), -0.0}));
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Partial, {hand::ref(fbb, 0, "k")}, {"k"},
                        {func(fbb, "stddev", {hand::ref(fbb, 1, "v")},
                              {"s$count", "s$mean", "s$m2"}, 1)});
  auto out = hand::run(fbb, node, one(hand::table_of(std::move(c), {"k", "v"})));
  for (int at : {2, 3}) EXPECT_EQ(out.view().column(at).null_count(), 0) << out.column_names.at(at);
  EXPECT_EQ((by_key<int64_t>(out, 1)), (std::map<int32_t, int64_t>{{1, 2}, {2, 1}}))
      << "a NaN is counted";
  auto mean = by_key<double>(out, 2);
  auto m2 = by_key<double>(out, 3);
  EXPECT_TRUE(std::isnan(mean.at(1)) && std::isnan(m2.at(1))) << "NaN stays NaN, not 0.0";
  EXPECT_EQ(mean.at(2), 0.0) << "-0.0 or 0.0, a value";
  EXPECT_EQ(m2.at(2), 0.0);
}
```

  Red on Step 6's red build: `AWelfordInitOverAnAllNullGroupIsZeroNotNull` (k 2's mean and m2
  are NULL: cuDF's `SUM` has no valid value, `MEAN` inherits it, `M2` copies `MEAN`'s mask) and
  `AMergeOfOnlyEmptyStatesIsZeroNotNull` (k 1's merged moments are NULL: `group_merge_m2.cu`'s
  `is_valid = count > 0`). `AnEmptyStateMergedWithARealOneIsTheRealOne` and
  `ANaNAndANegativeZeroAreValuesNotNulls` pass on both builds: they pin that the replacement adds
  nothing else.

- [ ] **Step 3: The harness cases, both engines.** Create
  `peacockdb-core/src/tests/gpu_tests/welford_cases.rs`, and declare `mod welford_cases;` after
  `mod source_schema_cases;` in `gpu_tests/mod.rs`:

```rust
//! The Welford triple on both engines where `aggregate_cases.rs` does not reach: groups with no
//! valid value, a NaN, lanes that disagree on what a group holds, and (Task 3) the keyless
//! node. The builders are `aggregate_cases.rs`'s; every case is an agreement case.

use std::sync::Arc;

use datafusion::arrow::array::{Array, AsArray, Float64Array, Int32Array};
use datafusion::arrow::datatypes::{Float64Type, Int32Type, Int64Type};
use datafusion::arrow::record_batch::RecordBatch;

use super::aggregate_cases::{cpu_slot, gpu_slot, welford_init, welford_merge, welford_merge_by};
use super::aggregate_dimension_cases::dispersion_finalize;
use super::script::{Outcome, Script, run_both};
use crate::plan::{AggFunc, GpuAggregateBatches};
use crate::tests::compare::same_within_welford;
use crate::tests::synthetic::synthetic;

/// `synthetic`'s columns with `key` and `f64` laid out by the case: each group's values.
fn keyed_f64(keys: &[i32], values: &[Option<f64>]) -> RecordBatch {
    let s = synthetic(keys.len(), 1);
    let mut columns = s.columns().to_vec();
    columns[1] = Arc::new(Int32Array::from(keys.to_vec()));
    columns[4] = Arc::new(Float64Array::from(values.to_vec()));
    RecordBatch::try_new(s.schema(), columns).expect("synthetic's schema")
}

fn row_of(batch: &RecordBatch, key: i32) -> usize {
    let keys = batch.column(0).as_primitive::<Int32Type>();
    (0..batch.num_rows())
        .find(|&row| keys.is_valid(row) && keys.value(row) == key)
        .unwrap_or_else(|| panic!("no group {key}"))
}

fn float_at(batch: &RecordBatch, column: usize, row: usize) -> Option<f64> {
    let values = batch.column(column).as_primitive::<Float64Type>();
    values.is_valid(row).then(|| values.value(row))
}

/// `[key, count, mean, m2]`'s state for `key`.
fn state_at(batch: &RecordBatch, key: i32) -> (Option<i64>, Option<f64>, Option<f64>) {
    let row = row_of(batch, key);
    let counts = batch.column(1).as_primitive::<Int64Type>();
    (
        counts.is_valid(row).then(|| counts.value(row)),
        float_at(batch, 2, row),
        float_at(batch, 3, row),
    )
}

/// Two lanes' batches: key 1 holds 1.5 and 4.0 on the first and only NULLs on the second; key 2
/// holds only NULLs on both.
fn lanes() -> [RecordBatch; 2] {
    [
        keyed_f64(&[1, 1, 2, 2], &[Some(1.5), Some(4.0), None, None]),
        keyed_f64(&[1, 1, 2, 2], &[None, None, None, None]),
    ]
}

/// Each lane's init on each engine, then `merge` over that engine's own states — so the
/// device's merge meets the device's init states, never the cpu's. The done slot of each.
fn merged_by_each(merge: &GpuAggregateBatches) -> (RecordBatch, RecordBatch) {
    let inits: Vec<Outcome> = lanes()
        .into_iter()
        .map(|lane| run_both(&welford_init(), Script::Accumulate(vec![lane])))
        .collect();
    let cpu_states = inits.iter().map(|init| cpu_slot(init, 0).clone()).collect();
    let gpu_states = inits.iter().map(|init| gpu_slot(init, 0).clone()).collect();
    let on_cpu = run_both(merge, Script::Accumulate(cpu_states));
    let on_gpu = run_both(merge, Script::Accumulate(gpu_states));
    (cpu_slot(&on_cpu, 2).clone(), gpu_slot(&on_gpu, 2).clone())
}

// A group whose values are all NULL inits to DataFusion's (0, 0.0, 0.0): no NULL in the
// state, on the device as on the cpu.
operator_case! {
    GpuAggregate,
    fn an_all_null_group_inits_to_the_empty_triple_on_both() {
        let outcome = run_both(&welford_init(), Script::Accumulate(vec![lanes()[0].clone()]));
        same_within_welford(cpu_slot(&outcome, 0), gpu_slot(&outcome, 0), true, &[2, 3]);
        assert_eq!(state_at(gpu_slot(&outcome, 0), 2), (Some(0), Some(0.0), Some(0.0)));
    }
}

// Key 1 merges a real state with an empty one; key 2 merges two empty ones, which cuDF's
// MERGE_M2 answers with NULL moments unless the readback replaces them.
operator_case! {
    GpuAggregateBatches,
    fn a_group_with_values_on_one_lane_and_nulls_on_another_merges_as_the_cpu() {
        let (cpu, gpu) = merged_by_each(&welford_merge());
        same_within_welford(&cpu, &gpu, true, &[2, 3]);
        assert_eq!(state_at(&gpu, 1), (Some(2), Some(2.75), Some(3.125)));
        assert_eq!(state_at(&gpu, 2), (Some(0), Some(0.0), Some(0.0)));
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_group_all_null_on_every_lane_finalizes_to_null_on_both() {
        let finalized = welford_merge_by(
            true,
            AggFunc::Stddev,
            Some(dispersion_finalize(AggFunc::Stddev, 1)),
        );
        let (cpu, gpu) = merged_by_each(&finalized);
        same_within_welford(&cpu, &gpu, true, &[1]);
        assert_eq!(float_at(&gpu, 1, row_of(&gpu, 2)), None, "no lane holds a value for key 2");
        assert!(float_at(&gpu, 1, row_of(&gpu, 1)).is_some());
    }
}

// A NaN is a value: counted, and its moments NaN on both — which `same_within_welford`'s
// tolerance cannot compare, so the case reads the state itself. A -0.0 is a 0.
operator_case! {
    GpuAggregate,
    fn a_nan_and_a_negative_zero_are_values_on_both() {
        let batch = keyed_f64(&[1, 1, 2], &[Some(1.0), Some(f64::NAN), Some(-0.0)]);
        let outcome = run_both(&welford_init(), Script::Accumulate(vec![batch]));
        for (engine, state) in [("cpu", cpu_slot(&outcome, 0)), ("device", gpu_slot(&outcome, 0))] {
            let (count, mean, m2) = state_at(state, 1);
            assert_eq!(count, Some(2), "{engine}");
            assert!(
                mean.is_some_and(f64::is_nan) && m2.is_some_and(f64::is_nan),
                "{engine}: {mean:?} {m2:?}"
            );
            assert_eq!(state_at(state, 2), (Some(1), Some(0.0), Some(0.0)), "{engine}");
        }
    }
}
```

  `key 1`'s merged state is exact: `(1.5 + 4.0) / 2 = 2.75` and `2 · 1.25² = 3.125` are dyadic.
  If `GpuAggregate` is not in scope for `operator_case!`'s kind ident, it needs no import (the
  macro stringifies it). Remove any import the compiler calls unused.

```bash
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
```

  Expected: compiles. On Step 6's red build `an_all_null_group_inits_to_the_empty_triple_on_both`
  and `a_group_with_values_on_one_lane_and_nulls_on_another_merges_as_the_cpu` panic in
  `same_within_welford` on `cpu Some(0.0), device None`. `a_group_all_null_on_every_lane_finalizes_to_null_on_both`
  and `a_nan_and_a_negative_zero_are_values_on_both` pass on both builds — the finalize answers
  NULL for a count of 0 whatever the moments, and a NaN is no NULL — so they are pins the spec
  asks for, not reds.

- [ ] **Step 4: The readbacks.** `aggregate.cpp`: `#include <cudf/replace.hpp>`; the
  *Interfaces* `StateReadback`; above `append_state_columns`:

```cpp
// DataFusion's Welford state is never NULL: a group of no valid value is (0, 0.0, 0.0). cuDF
// answers NULL there — MEAN and M2 over no valid value, MERGE_M2 where every merged count is 0
// — so a moment's NULL is 0.0 and no MERGE_M2 downstream meets a NULL child. NaN stays NaN.
static std::unique_ptr<cudf::column> zero_for_null(std::unique_ptr<cudf::column> moment) {
  if (moment->null_count() == 0) return moment;
  return cudf::replace_nulls(moment->view(), cudf::numeric_scalar<double>(0.0));
}
```

  In `add_requests`' Welford `Partial` arm, the mean's and m2's state columns read back
  `NullAsZero`, in place of the two `AsIs` lines and their `⟵ welford-device` comment:

```cpp
        into.columns.push_back({state(0), r, 0, StateReadback::CountToInt64});
        into.columns.push_back({state(1), r, 1, StateReadback::NullAsZero});
        into.columns.push_back({state(2), r, 2, StateReadback::NullAsZero});
```

  In its `Merge` arm, the `⟵ welford-device` comment above the `MergedChild` loop becomes
  `// Children 1 and 2, the merged moments, read back NullAsZero in append_state_columns.` In
  `append_state_columns`, a new arm, and the `MergedChild` arm replaced:

```cpp
      case StateReadback::NullAsZero:
        columns.push_back(zero_for_null(std::move(result)));
        break;
      case StateReadback::MergedChild: {
        // Three columns read one struct, so by view: the count comes back as
        // kMergeM2CountType, the moments NULL wherever every merged count was 0.
        auto child = result->view().child(state.child);
        columns.push_back(state.child == 0
                              ? as_int64_count(child)
                              : zero_for_null(std::make_unique<cudf::column>(child)));
        break;
      }
```

- [ ] **Step 5: Build and run what runs locally.**

```bash
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests::
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
wc -l cpp/tests/gpu/test_aggregate_builder.cpp peacockdb-core/src/tests/gpu_tests/welford_cases.rs
```

  Expected: clean build, no new warnings; PASS; both files under 1000 lines.

- [ ] **Step 6: The device, red then green** (*Device cycle*, `FIX=cpp/src/operators/aggregate.cpp`:
  the tests and `hand_tables.hpp` stay in both builds). The red build's runs:

```bash
timeout 1800 ssh "$GPU" "$R cpp/install/bin/peacock_plan_tests --gtest_filter='AggregateBuilder.*'"
timeout 3600 ssh "$GPU" "$R cpp/install/rust-tests/peacockdb_core_gpu_lib welford_cases:: --test-threads=1"
```

  Expected red: the two gtests Step 2 names and the two harness cases Step 3 names, each on a
  NULL moment; every other `AggregateBuilder.*` and `welford_cases::` case passes. The green
  build's runs, the fix touching every grouped and grouping-set Welford:

```bash
timeout 1800 ssh "$GPU" "$R cpp/install/bin/peacock_gpu_tests"
timeout 3600 ssh "$GPU" "$R cpp/install/bin/peacock_plan_tests"
timeout 3600 ssh "$GPU" "$R cpp/install/bin/peacock_join_session_tests"
timeout 7200 ssh "$GPU" "$R cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests:: --test-threads=1"
```

  Expected: all green, the walk tests (`wire::gpu_tests::`) and the #216 `bug_` pins (still
  asserting the bug until Task 3) included. Record both runs in `welford-device-detail.md`: the
  host, the env path that held, the tree (`git rev-parse HEAD` plus "Task 2, uncommitted"), and
  each binary's pass and fail counts with the red cases' messages.

- [ ] **Step 7: Commit.**

```bash
git clang-format HEAD -- cpp/src/operators/aggregate.cpp cpp/tests/gpu/hand_tables.hpp cpp/tests/gpu/test_aggregate_builder.cpp
git add cpp/src/operators/aggregate.cpp cpp/tests/gpu peacockdb-core/src/tests/gpu_tests llm-wiki/tasks/welford-device-detail.md
git commit -m "Welford state NULL-free on the device: all-NULL groups are (0, 0.0, 0.0)

The init's mean and m2, and MERGE_M2's merged moments, read back with
NULL replaced by 0.0, as DataFusion's state is. Red, then green, on
nebius-gpu.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: The keyless Welford through the grouped builder (#216)

**Files:**
- Modify: `cpp/src/operators/aggregate.cpp` (`execute_aggregate`'s keyless branch; three new
  statics; `keyless_aggregate`'s stddev arm goes)
- Test: `cpp/tests/gpu/test_aggregate_builder.cpp`
- Modify: `peacockdb-core/src/tests/gpu_tests/aggregate_dimension_cases.rs`,
  `aggregate_schema_cases.rs`, `aggregate_cases.rs` (or `aggregate_identity_cases.rs`),
  `welford_cases.rs`

**Interfaces:**
- Consumes aggregate-arms' ⟵ hook: `grouped_aggregate` on one constant `INT32` key. Produces
  (file-local in `aggregate.cpp`):

```cpp
/// Whether any function of `agg` is a stddev or var: what sends a keyless node to the builder.
static bool holds_a_welford(const fb::CudfAggregate* agg);

/// A keyless node holding a Welford: grouped_aggregate on one constant key, dropped; over zero
/// rows, keyless_empty_state's row.
static TableResult keyless_welford_aggregate(const fb::CudfAggregate* agg, AggPhase phase,
                                             cudf::table_view input,
                                             std::vector<std::string> const& names);

/// The one row a routed node owes over no rows, typed and named as the groupby's zero-row
/// answer `no_groups`: a count 0 (Partial only: a count never reaches a Merge), a Welford
/// (0, 0.0, 0.0), anything else NULL.
static TableResult keyless_empty_state(const fb::CudfAggregate* agg,
                                       TableResult const& no_groups);
```

- [ ] **Step 1: The gtests, failing on the device.** `test_aggregate_builder.cpp`, after Task 2's
  cases (add `#include <cudf/column/column_factories.hpp>` if absent):

```cpp
// --- A keyless Welford (#216) ---------------------------------------------------------------
//
// cuDF has M2 and MERGE_M2 for groupby only: a keyless node holding a stddev or var runs whole
// through the grouped builder on one constant key, which it drops.

TEST(AggregateBuilder, AKeylessWelfordInitIsTheTripleOfAllItsRows) {
  // v: 10 20 30 40 50 — n 5, mean 30, m2 400 + 100 + 0 + 100 + 400.
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Partial, {}, {},
                        {func(fbb, "stddev", {hand::ref(fbb, 1, "v")},
                              {"s$count", "s$mean", "s$m2"}, 1)});
  auto out = hand::run(fbb, node, one(rows()));
  ASSERT_EQ(out.column_names, (std::vector<std::string>{"s$count", "s$mean", "s$m2"}))
      << "the constant key is dropped";
  ASSERT_EQ(out.num_rows(), 1);
  EXPECT_EQ(out.view().column(0).type().id(), cudf::type_id::INT64);
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(0)), std::vector<int64_t>{5});
  EXPECT_EQ(hand::values_of<double>(out.view().column(1)), std::vector<double>{30.0});
  EXPECT_EQ(hand::values_of<double>(out.view().column(2)), std::vector<double>{1000.0});
}

TEST(AggregateBuilder, AKeylessVarMergeMergesEveryArrivingState) {
  // AWelfordMergeReadsTheTripleWhereItsArgsPoint's two states with no key, under `var`, which
  // the reduce path refused by name.
  std::vector<std::unique_ptr<cudf::column>> c;
  c.push_back(hand::column_of<int64_t>({2, 4}));
  c.push_back(hand::column_of<double>({1.5, 4.0}));
  c.push_back(hand::column_of<double>({2.0, 8.0}));
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(
      fbb, fb::AggregateMode_Merge, {}, {},
      {func(fbb, "var",
            {hand::ref(fbb, 0, "s$count"), hand::ref(fbb, 1, "s$mean"), hand::ref(fbb, 2, "s$m2")},
            {"s$count", "s$mean", "s$m2"}, 1)});
  auto out = hand::run(fbb, node,
                       one(hand::table_of(std::move(c), {"s$count", "s$mean", "s$m2"})));
  ASSERT_EQ(out.num_rows(), 1);
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(0)), std::vector<int64_t>{6});
  EXPECT_NEAR(hand::values_of<double>(out.view().column(1))[0], 19.0 / 6.0, 1e-12);
  EXPECT_NEAR(hand::values_of<double>(out.view().column(2))[0], 10.0 + 6.25 * 8.0 / 6.0, 1e-12);
}

TEST(AggregateBuilder, AKeylessWelfordCarriesTheFunctionsBesideIt) {
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Partial, {}, {},
                        {func(fbb, "count", {hand::ref(fbb, 1, "v")}, {"n"}),
                         func(fbb, "stddev", {hand::ref(fbb, 1, "v")},
                              {"s$count", "s$mean", "s$m2"}, 1),
                         func(fbb, "sum", {hand::ref(fbb, 1, "v")}, {"total"})});
  auto out = hand::run(fbb, node, one(rows()));
  ASSERT_EQ(out.column_names,
            (std::vector<std::string>{"n", "s$count", "s$mean", "s$m2", "total"}));
  ASSERT_EQ(out.num_rows(), 1);
  EXPECT_EQ(out.view().column(0).type().id(), cudf::type_id::INT64);
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(0)), std::vector<int64_t>{5});
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(1)), std::vector<int64_t>{5});
  EXPECT_EQ(hand::values_of<double>(out.view().column(2)), std::vector<double>{30.0});
  EXPECT_EQ(out.view().column(4).type().id(), cudf::type_id::INT64);
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(4)), std::vector<int64_t>{150});
}

TEST(AggregateBuilder, AKeylessWelfordOverZeroRowsAnswersItsEmptyState) {
  // A done call's input. A groupby over no rows has no group, and a keyless node owes one
  // row — plan/aggregates.rs's empty_state: a `count` 0, the triple (0, 0.0, 0.0), the
  // decimal sum NULL at its scale. At Merge a count arrives as a `sum`, so it is NULL there.
  const cudf::data_type cents{cudf::type_id::DECIMAL128, -2};
  for (auto mode : {fb::AggregateMode_Partial, fb::AggregateMode_Merge}) {
    const bool merging = mode == fb::AggregateMode_Merge;
    flatbuffers::FlatBufferBuilder fbb;
    std::vector<std::unique_ptr<cudf::column>> c;
    std::vector<std::string> names;
    std::vector<flatbuffers::Offset<fb::AggregateFuncNode>> funcs;
    if (merging) {
      c.push_back(hand::column_of<int64_t>({}));
      c.push_back(hand::column_of<int64_t>({}));
      c.push_back(hand::column_of<double>({}));
      c.push_back(hand::column_of<double>({}));
      c.push_back(cudf::make_empty_column(cents));
      names = {"n", "s$count", "s$mean", "s$m2", "total"};
      funcs.push_back(func(fbb, "sum", {hand::ref(fbb, 0, "n")}, {"n"}));
      funcs.push_back(merged_stddev(fbb, 1));
      funcs.push_back(func(fbb, "sum", {hand::ref(fbb, 4, "total")}, {"total"}));
    } else {
      c.push_back(hand::column_of<double>({}));
      c.push_back(cudf::make_empty_column(cents));
      names = {"v", "amount"};
      funcs.push_back(func(fbb, "count", {hand::ref(fbb, 0, "v")}, {"n"}));
      funcs.push_back(func(fbb, "stddev", {hand::ref(fbb, 0, "v")},
                           {"s$count", "s$mean", "s$m2"}, 1));
      funcs.push_back(func(fbb, "sum", {hand::ref(fbb, 1, "amount")}, {"total"}));
    }
    auto node = aggregate(fbb, mode, {}, {}, funcs);
    auto out = hand::run(fbb, node, one(hand::table_of(std::move(c), names)));
    auto const* at = fb::EnumNameAggregateMode(mode);
    ASSERT_EQ(out.column_names,
              (std::vector<std::string>{"n", "s$count", "s$mean", "s$m2", "total"}))
        << at;
    ASSERT_EQ(out.num_rows(), 1) << at << ": a keyless node answers one row over nothing";
    EXPECT_EQ(out.view().column(0).type().id(), cudf::type_id::INT64) << at;
    EXPECT_EQ(out.view().column(0).null_count(), merging ? 1 : 0) << at;
    if (!merging) EXPECT_EQ(hand::values_of<int64_t>(out.view().column(0)), std::vector<int64_t>{0});
    for (int i = 1; i < 4; ++i) EXPECT_EQ(out.view().column(i).null_count(), 0) << at << " " << i;
    EXPECT_EQ(hand::values_of<int64_t>(out.view().column(1)), std::vector<int64_t>{0}) << at;
    EXPECT_EQ(hand::values_of<double>(out.view().column(2)), std::vector<double>{0.0}) << at;
    EXPECT_EQ(hand::values_of<double>(out.view().column(3)), std::vector<double>{0.0}) << at;
    EXPECT_EQ(out.view().column(4).type(), cents) << at << ": the scale is the sum's";
    EXPECT_EQ(out.view().column(4).null_count(), 1) << at;
  }
}
```

  Red on Step 8's red build, all four: the reduce path answers one finished `STD` column for the
  init and for the init beside a `count` and a `sum`, refuses the `var` merge as
  `unsupported aggregate function: var`, and over zero rows answers one column.

- [ ] **Step 2: The route.** `aggregate.cpp`, `#include <numeric>` (for `std::iota`), above
  `execute_aggregate`:

```cpp
static bool holds_a_welford(const fb::CudfAggregate* agg) {
  if (!agg->aggr_funcs()) return false;
  return std::any_of(agg->aggr_funcs()->begin(), agg->aggr_funcs()->end(), [](auto const* func) {
    return func->name() && agg_kind(func->name()->str()) == AggKind::Welford;
  });
}

// The row a routed node owes over no rows — plan/aggregates.rs's empty_state, which the
// identity cases hold both engines to. Typed as the groupby typed its zero-row columns, which
// keeps a decimal's scale; named as they were named, from state_names.
static TableResult keyless_empty_state(const fb::CudfAggregate* agg,
                                       TableResult const& no_groups) {
  auto one = [](cudf::scalar const& value) { return cudf::make_column_from_scalar(value, 1); };
  std::vector<std::unique_ptr<cudf::column>> columns;
  for (auto const* func : *agg->aggr_funcs()) {
    switch (agg_kind(func->name()->str())) {
      case AggKind::Count:  // Partial only: aggregate-arms refuses a count at Merge
        columns.push_back(one(cudf::numeric_scalar<int64_t>(0)));
        break;
      case AggKind::Welford:
        columns.push_back(one(cudf::numeric_scalar<int64_t>(0)));
        columns.push_back(one(cudf::numeric_scalar<double>(0.0)));
        columns.push_back(one(cudf::numeric_scalar<double>(0.0)));
        break;
      case AggKind::Sum:
      case AggKind::Min:
      case AggKind::Max: {
        // A default-constructed scalar is invalid: the NULL, at the column's type and scale.
        auto at = static_cast<cudf::size_type>(columns.size());
        columns.push_back(one(*cudf::make_default_constructed_scalar(no_groups.view().column(at).type())));
        break;
      }
    }
  }
  return TableResult::owning(std::make_unique<cudf::table>(std::move(columns)),
                             no_groups.column_names);
}

// cuDF has M2 and MERGE_M2 for groupby only, so a keyless node holding a stddev or var runs
// whole through the grouped builder, every row under one constant key, dropped after (#216).
static TableResult keyless_welford_aggregate(const fb::CudfAggregate* agg, AggPhase phase,
                                             cudf::table_view input,
                                             std::vector<std::string> const& names) {
  auto key = cudf::make_column_from_scalar(cudf::numeric_scalar<int32_t>(0), input.num_rows());
  TableResult grouped =
      grouped_aggregate(agg, phase, {key->view()}, {"__keyless"}, input, names);
  std::vector<cudf::size_type> state(static_cast<size_t>(grouped.num_columns() - 1));
  std::iota(state.begin(), state.end(), 1);
  TableResult keyless = grouped.select(state);
  return keyless.num_rows() == 0 ? keyless_empty_state(agg, keyless) : keyless;
}
```

  and in `execute_aggregate`:

```cpp
  if (keys.empty())
    return holds_a_welford(agg) ? keyless_welford_aggregate(agg, phase, tv, input.column_names)
                                : keyless_aggregate(agg, phase, tv, input.column_names);
```

  `agg_kind` throws `unsupported aggregate function: <name>` for a name it does not know, as
  the reduce path did. `keyless_empty_state`'s ordinal is `columns.size()` because the state
  columns are in function order; the `switch` has no `default`, so a new `AggKind` says its
  identity here or warns.

- [ ] **Step 3: The reduce path's stddev arm goes.** In `keyless_aggregate`, delete the arm that
  reduces with `cudf::make_std_aggregation<cudf::reduce_aggregation>(func->ddof())` and its
  comment; a Welford never reaches the reduce path now. Delete `is_stddev_name` / `is_var_name`
  if anything kept them. Reword any comment there naming #216 to say a node holding a Welford
  is `keyless_welford_aggregate`'s.

```bash
git grep -n 'make_std_aggregation\|make_variance_aggregation\|is_stddev_name\|is_var_name\|#216\|ddof()' cpp/src
```

  Expected: the new route's comment, and `ddof()` nowhere, or only where aggregate-arms' builder
  checks it: the arm was its one device reader, since the finalize is a project. Task 5 says so
  in `architecture.md`.

- [ ] **Step 4: The #216 pins flip** (`coding-style.md`: a `bug_` test the fix turns red becomes
  the agreement case). In `aggregate_dimension_cases.rs`:
  - The comment block above `welford_init_global` ("…keyless, the device has no triple to
    finalize under `stddev` and no arm at all for `var`") becomes: "Keyless, the device runs the
    triple through the grouped builder on one constant key, and both agree."
  - `bug_a_global_welford_init_answers_a_finished_stddev_on_the_device` and its `// #216`
    comment become:

```rust
operator_case! {
    GpuAggregate,
    fn a_global_welford_init_agrees() {
        let outcome = run_both(&welford_init_global(), Script::Accumulate(vec![input()]));
        same_within_welford(cpu_slot(&outcome, 0), gpu_slot(&outcome, 0), false, &[1, 2]);
    }
}
```

  - `bug_a_keyless_welford_merge_answers_the_stddev_of_its_counts_on_the_device` and its comment
    become:

```rust
operator_case! {
    GpuAggregateBatches,
    fn a_keyless_welford_merge_agrees() {
        let partial = |seed| keyless_welford_partial(AggFunc::Stddev, seed);
        let node = welford_merge_by(false, AggFunc::Stddev, None);
        let outcome = run_both(&node, Script::Accumulate(vec![partial(1), partial(2)]));
        same_within_welford(cpu_slot(&outcome, 2), gpu_slot(&outcome, 2), false, &[1, 2]);
    }
}
```

  - `bug_a_global_stddev_finalize_is_refused_on_the_device` and
    `bug_a_keyless_var_merge_is_refused_as_unsupported_on_the_device`, with their comments,
    become one helper and two cases:

```rust
/// The keyless merge finalized, the `Float64` to `WELFORD_RELATIVE` for the reason
/// `finalized_within_welford` gives.
fn global_finalized_within_welford(func: AggFunc) {
    let outcome = global_finalize_outcome(func);
    same_within_welford(cpu_slot(&outcome, 2), gpu_slot(&outcome, 2), false, &[0]);
}

operator_case! {
    GpuAggregateBatches,
    fn a_global_stddev_finalize_agrees_within_welford() {
        global_finalized_within_welford(AggFunc::Stddev);
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_global_var_finalize_agrees_within_welford() {
        global_finalized_within_welford(AggFunc::Var);
    }
}
```

  The file's top doc keeps "every case is green or a `bug_` test"; check it still reads true.
  In `aggregate_schema_cases.rs`, the `// #216` pin becomes, with a merge case beside it:

```rust
// The global Welford holds the triple the plan declares, at the init and at the merge.
operator_case! {
    GpuAggregate,
    fn a_global_stddev_holds_its_declared_welford_state() {
        assert_holds_as_declared(&welford_init_global(), Script::Accumulate(vec![input()]));
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_global_stddev_merge_holds_its_declared_welford_state() {
        let partial = |seed| {
            welford_partial(AggFunc::Stddev, seed)
                .project(&[1, 2, 3])
                .expect("the triple")
        };
        let node = welford_merge_by(false, AggFunc::Stddev, None);
        assert_holds_as_declared(&node, Script::Accumulate(vec![partial(1), partial(2)]));
    }
}
```

- [ ] **Step 5: keyless-identity's identity cases gain `Stddev` and `Var`.** Where
  `DEVICE_IDENTITY` is, replace it and its doc with:

```rust
/// Every aggregate SQL can ask for: the device answers each keyless one over no rows,
/// the Welford pair through the grouped builder on one constant key.
const EVERY_FUNC: [AggFunc; 7] = [
    AggFunc::Sum,
    AggFunc::Min,
    AggFunc::Max,
    AggFunc::Count,
    AggFunc::Avg,
    AggFunc::Stddev,
    AggFunc::Var,
];
```

  and both loops (`a_keyless_init_over_no_input_answers_its_empty_state_on_both`,
  `a_keyless_shortcut_over_no_input_answers_count_0_and_nulls_on_both`) iterate `EVERY_FUNC`.
  Both already run over no batch and over two zero-row batches, init and shortcut, as the spec
  asks.
  `empty_state_row` and `at_done` are already `pub(crate)` (keyless-identity); Step 6 uses them.

- [ ] **Step 6: The keyless cases.** Append to `welford_cases.rs` (extend the imports with
  `datafusion::arrow::array::ArrayRef`, `datafusion::arrow::datatypes::DataType`;
  `super::aggregate_cases::{body, call, input,
  merge_over, welford_init_aggs}`; `at_done` and `empty_state_row` from wherever Step 5 found
  them (`aggregate_cases` or `aggregate_identity_cases`);
  `super::aggregate_dimension_cases::welford_init_global`; `super::script::each_answers`;
  `crate::plan::{AggCall, AggSpec, AggStateColumns, BatchLayout, Expr, GpuAggregate, NamedExpr,
  PlanAgg, Schema, finalize}`; `crate::tests::compare::Order`;
  `crate::tests::given::{Given, columns}`; `crate::tests::synthetic::{decimals, schema}`):

```rust
/// `count(i32)`, `stddev(f64)`, `sum(i64)`, `min(d)` and `max(s)` with no key, as
/// `SELECT count(i32), stddev(f64), sum(i64), min(d), max(s)` plans: a Welford beside what a
/// keyless node reduces when it holds none — a `Date32` and a `Utf8` among them, which the
/// routed node takes to a groupby `MIN`/`MAX` and over nothing to a typed NULL.
fn keyless_mixed_state() -> Schema {
    let mut state = columns(&[
        ("count(i32)", DataType::Int64),
        ("stddev(f64)$count", DataType::Int64),
        ("stddev(f64)$mean", DataType::Float64),
        ("stddev(f64)$m2", DataType::Float64),
        ("sum(i64)", DataType::Int64),
        ("min(d)", DataType::Date32),
        ("max(s)", DataType::Utf8),
    ]);
    state.agg_state = vec![AggStateColumns {
        output: "stddev(f64)".to_string(),
        func: AggFunc::Stddev,
        ddof: 1,
        positions: vec![1, 2, 3],
    }];
    state
}

fn keyless_mixed_init() -> GpuAggregate {
    let mut aggs = vec![call(PlanAgg::Count, Expr::column(2, "i32"), "count(i32)", DataType::Int64)];
    aggs.extend(welford_init_aggs());
    aggs.push(call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64));
    aggs.push(call(PlanAgg::Min, Expr::column(6, "d"), "min(d)", DataType::Date32));
    aggs.push(call(PlanAgg::Max, Expr::column(5, "s"), "max(s)", DataType::Utf8));
    let state = keyless_mixed_state();
    GpuAggregate::new(
        Given::of(Schema::new(schema()), BatchLayout::MultipleBatches),
        body(Vec::new(), aggs, None),
        state.clone(),
        state,
    )
}

fn keyless_mixed_merge() -> GpuAggregateBatches {
    let state = keyless_mixed_state();
    let field = |at: usize| state.fields.field(at).clone();
    let aggs = vec![
        // A count merges by sum.
        call(PlanAgg::Sum, Expr::column(0, "count(i32)"), "count(i32)", DataType::Int64),
        AggCall {
            func: PlanAgg::MergeM2,
            args: (1..4usize).map(|at| Expr::column(at as u32, field(at).name())).collect(),
            outputs: (1..4usize).map(field).collect(),
        },
        call(PlanAgg::Sum, Expr::column(4, "sum(i64)"), "sum(i64)", DataType::Int64),
        call(PlanAgg::Min, Expr::column(5, "min(d)"), "min(d)", DataType::Date32),
        call(PlanAgg::Max, Expr::column(6, "max(s)"), "max(s)", DataType::Utf8),
    ];
    merge_over(state.clone(), body(Vec::new(), aggs, None), state)
}

/// `empty_state`'s row for the mixed init, each function at its own argument's type.
fn keyless_mixed_empty_row(node: &GpuAggregate) -> RecordBatch {
    let inputs = [
        DataType::Int32,
        DataType::Float64,
        DataType::Float64,
        DataType::Float64,
        DataType::Int64,
        DataType::Date32,
        DataType::Utf8,
    ];
    let arrays: Vec<ArrayRef> = node
        .body
        .aggs
        .iter()
        .zip(&inputs)
        .map(|(agg, input)| {
            let value = agg.func.empty_state(input).expect("every init state is typed");
            value.to_array().expect("one row")
        })
        .collect();
    RecordBatch::try_new(node.intermediate().fields.clone(), arrays).expect("the state's columns")
}

// With rows, and over nothing: count 0, the triple (0, 0.0, 0.0), and the sum, min and max
// NULL at their types — the Date32 and Utf8 NULLs default-constructed by keyless_empty_state.
operator_case! {
    GpuAggregate,
    fn a_keyless_stddev_beside_other_aggregates_agrees_over_rows_and_over_nothing() {
        let node = keyless_mixed_init();
        let outcome = run_both(&node, Script::Accumulate(vec![input()]));
        same_within_welford(cpu_slot(&outcome, 0), gpu_slot(&outcome, 0), false, &[2, 3]);
        let over_nothing = run_both(&node, Script::Accumulate(Vec::new()));
        let slots = at_done(0, keyless_mixed_empty_row(&node));
        each_answers(&over_nothing, &slots, &slots);
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_keyless_stddev_merge_beside_other_aggregates_agrees() {
        let partial = |seed| {
            let init = run_both(&keyless_mixed_init(), Script::Accumulate(vec![synthetic(64, seed)]));
            cpu_slot(&init, 0).clone()
        };
        let arrivals = vec![partial(1), partial(2), partial(3)];
        let outcome = run_both(&keyless_mixed_merge(), Script::Accumulate(arrivals));
        same_within_welford(cpu_slot(&outcome, 3), gpu_slot(&outcome, 3), false, &[2, 3]);
    }
}

/// `stddev(dec)` and `sum(dec)` with no key over `decimals`' `Decimal128(18, 2)`: the Welford
/// casts to `Float64`, the sum widens to `Decimal128(28, 2)`.
fn keyless_decimal_init() -> GpuAggregate {
    let triple = [
        ("$count", PlanAgg::Count, DataType::Int64),
        ("$mean", PlanAgg::Mean, DataType::Float64),
        ("$m2", PlanAgg::M2, DataType::Float64),
    ];
    let mut aggs: Vec<AggCall> = triple
        .iter()
        .map(|(suffix, agg, ty)| {
            call(*agg, Expr::column(1, "dec"), &format!("stddev(dec){suffix}"), ty.clone())
        })
        .collect();
    aggs.push(call(PlanAgg::Sum, Expr::column(1, "dec"), "sum(dec)", DataType::Decimal128(28, 2)));
    let mut state = columns(&[
        ("stddev(dec)$count", DataType::Int64),
        ("stddev(dec)$mean", DataType::Float64),
        ("stddev(dec)$m2", DataType::Float64),
        ("sum(dec)", DataType::Decimal128(28, 2)),
    ]);
    state.agg_state = vec![AggStateColumns {
        output: "stddev(dec)".to_string(),
        func: AggFunc::Stddev,
        ddof: 1,
        positions: vec![0, 1, 2],
    }];
    GpuAggregate::new(
        Given::of(Schema::new(decimals(0, 0).schema()), BatchLayout::MultipleBatches),
        body(Vec::new(), aggs, None),
        state.clone(),
        state,
    )
}

// The routed node's sum is a groupby SUM, not cudf::reduce's; over nothing its NULL keeps the
// declared scale, which the done call's zero-row table carries from the field.
operator_case! {
    GpuAggregate,
    fn a_keyless_stddev_beside_a_decimal_sum_agrees_over_rows_and_over_nothing() {
        let node = keyless_decimal_init();
        let outcome = run_both(&node, Script::Accumulate(vec![decimals(64, 1)]));
        same_within_welford(cpu_slot(&outcome, 0), gpu_slot(&outcome, 0), false, &[1, 2]);
        let over_nothing = run_both(&node, Script::Accumulate(Vec::new()));
        let slots = at_done(0, empty_state_row(&node, &DataType::Decimal128(18, 2)));
        each_answers(&over_nothing, &slots, &slots);
    }
}

/// A keyless `func(f64)` at `ddof` that finalizes itself, as the single-node shortcut writes
/// it, named as the planner names it.
fn keyless_dispersion_shortcut(func: AggFunc, ddof: u32) -> GpuAggregate {
    let output = match (func, ddof) {
        (AggFunc::Stddev, 1) => "stddev(f64)",
        (AggFunc::Stddev, 0) => "stddev_pop(f64)",
        (AggFunc::Var, 1) => "var(f64)",
        (AggFunc::Var, 0) => "var_pop(f64)",
        other => panic!("{other:?} is not a dispersion form"),
    };
    let triple = [
        ("$count", PlanAgg::Count, DataType::Int64),
        ("$mean", PlanAgg::Mean, DataType::Float64),
        ("$m2", PlanAgg::M2, DataType::Float64),
    ];
    let aggs: Vec<AggCall> = triple
        .iter()
        .map(|(suffix, agg, ty)| {
            call(*agg, Expr::column(4, "f64"), &format!("{output}{suffix}"), ty.clone())
        })
        .collect();
    let fields: Vec<_> = aggs.iter().map(|agg| agg.outputs[0].clone()).collect();
    let named: Vec<(&str, DataType)> = fields
        .iter()
        .map(|field| (field.name().as_str(), field.data_type().clone()))
        .collect();
    let mut state = columns(&named);
    state.agg_state = vec![AggStateColumns {
        output: output.to_string(),
        func,
        ddof,
        positions: vec![0, 1, 2],
    }];
    let expr = finalize(AggSpec { func, ddof }, &fields, 0, &DataType::Float64);
    GpuAggregate::new(
        Given::of(Schema::new(schema()), BatchLayout::MultipleBatches),
        body(Vec::new(), aggs, Some(vec![NamedExpr::new(expr, output)])),
        state,
        columns(&[(output, DataType::Float64)]),
    )
}

// One row: count 1, m2 0. The finalize's `count - ddof <= 0` makes the sample forms NULL;
// the population forms divide 0 by 1.
operator_case! {
    GpuAggregate,
    fn one_row_is_null_for_the_sample_forms_and_0_for_the_population_forms_on_both() {
        let forms = [
            (AggFunc::Stddev, 1, None),
            (AggFunc::Stddev, 0, Some(0.0)),
            (AggFunc::Var, 1, None),
            (AggFunc::Var, 0, Some(0.0)),
        ];
        for (func, ddof, want) in forms {
            let outcome = run_both(
                &keyless_dispersion_shortcut(func, ddof),
                Script::Accumulate(vec![synthetic(1, 1)]),
            );
            outcome.same(Order::Any);
            assert_eq!(float_at(cpu_slot(&outcome, 0), 0, 0), want, "{func:?} at ddof {ddof}");
        }
    }
}

// Only NULLs, no key: (0, 0.0, 0.0) on both, exactly.
operator_case! {
    GpuAggregate,
    fn a_keyless_welford_over_only_nulls_inits_to_the_empty_triple_on_both() {
        let batch = keyed_f64(&[1, 1, 2], &[None, None, None]);
        let outcome = run_both(&welford_init_global(), Script::Accumulate(vec![batch]));
        outcome.same(Order::Any);
        let state = gpu_slot(&outcome, 0);
        assert_eq!(state.column(0).as_primitive::<Int64Type>().value(0), 0);
        assert_eq!((float_at(state, 1, 0), float_at(state, 2, 0)), (Some(0.0), Some(0.0)));
    }
}
```

  `synthetic(1, 1)`'s one `f64` is valid (a column is null only at `row % 13 == 12`). If
  `decimals(64, 1)` holds a NULL `dec`, the case still holds: both skip it.

```bash
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
git grep -n 'bug_a_global\|bug_a_keyless\|DEVICE_IDENTITY' peacockdb-core/src
git grep -n '216' peacockdb-core/src cpp
```

  Expected: compiles; the first grep prints nothing. The second prints only this task's route
  comment in `aggregate.cpp` and `welford_cases.rs`' — no comment saying the keyless Welford is a
  pin or missing on the device: rewrite any such (`aggregate_cases.rs`' "The Welford init's
  global form is #216's pin, in `aggregate_dimension_cases.rs`" becomes "The Welford init's
  global form is in `aggregate_dimension_cases.rs`").

- [ ] **Step 7: Build and run what runs locally.**

```bash
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests::
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- executor::cpu_backend::tests::state_types
wc -l cpp/src/operators/aggregate.cpp cpp/tests/gpu/test_aggregate_builder.cpp peacockdb-core/src/tests/gpu_tests/*.rs
```

  Expected: clean, no new warnings; PASS (the cpu identity cases for `Stddev` and `Var`,
  keyless-identity's, unchanged); every file under 1000 lines.

- [ ] **Step 8: The device, red then green** (*Device cycle*, `FIX=cpp/src/operators/aggregate.cpp`:
  the route and the reduce arm's removal; every test compiles without them). The red build's
  runs:

```bash
timeout 1800 ssh "$GPU" "$R cpp/install/bin/peacock_plan_tests --gtest_filter='AggregateBuilder.AKeyless*'"
timeout 3600 ssh "$GPU" "$R cpp/install/rust-tests/peacockdb_core_gpu_lib welford_cases:: aggregate_dimension_cases:: aggregate_schema_cases:: aggregate_cases:: aggregate_identity_cases:: --test-threads=1"
```

  Expected red, every one on the reduce path's one column or its `var` refusal: the four
  `AKeyless*` gtests; `a_global_welford_init_agrees`, `a_keyless_welford_merge_agrees`,
  `a_global_stddev_finalize_agrees_within_welford`, `a_global_var_finalize_agrees_within_welford`,
  `a_global_stddev_holds_its_declared_welford_state`,
  `a_global_stddev_merge_holds_its_declared_welford_state`, both identity cases (at `Stddev`, the
  loop's first Welford), and the five keyless cases of Step 6. Everything else in the filter
  passes, Task 2's `welford_cases` included. The green build's runs:

```bash
timeout 1800 ssh "$GPU" "$R cpp/install/bin/peacock_gpu_tests"
timeout 3600 ssh "$GPU" "$R cpp/install/bin/peacock_plan_tests"
timeout 3600 ssh "$GPU" "$R cpp/install/bin/peacock_join_session_tests"
timeout 7200 ssh "$GPU" "$R cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests:: --test-threads=1"
timeout 1800 ssh "$GPU" "$R cpp/install/rust-tests/test_node_timing --test-threads=1"
```

  Expected: all green, the walk tests (`wire::gpu_tests::`) included. Record both runs in
  `welford-device-detail.md` as Task 2's are.

- [ ] **Step 9: Commit.**

```bash
git clang-format HEAD -- cpp/src/operators/aggregate.cpp cpp/tests/gpu/test_aggregate_builder.cpp
git add cpp/src/operators/aggregate.cpp cpp/tests/gpu peacockdb-core/src/tests/gpu_tests llm-wiki/tasks/welford-device-detail.md
git commit -m "#216: a keyless stddev or var runs through the grouped builder

One constant INT32 key, dropped; over zero rows the node answers
empty_state's row. The reduce path's stddev arm goes; the #216 pins
become agreement cases, and the identity cases cover every AggFunc.
Red, then green, on nebius-gpu.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: `tpch/global-stddev` in the corpus, device cells off

**Files:**
- Create: `testdata/tpch-queries/global-stddev.sql`
- Modify: `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`
- Modify: `testdata/goldens/tpch.sf1/*.plans.txt`, `*-mini.cpu.txt`, `*-mini.cost.txt`,
  `mini.result.txt`, `duckdb-result.txt`

- [ ] **Step 1: The query**, as the spec gives it:

```sql
-- A keyless stddev and var: the device's Welford over no keys (#216).
SELECT stddev_samp(l_quantity) AS sd, var_samp(l_quantity) AS v FROM lineitem;
```

- [ ] **Step 2: The corpus line and the registry row.** After `rollup_stddev`'s line, in its
  shape (its `duckdb_oracle` argument, its oracle and golden):

```
// global-stddev is the keyless Welford, which no other query has: the device runs it through the
// grouped request builder on one constant key (#216). Its device cells stay off until a device
// run writes their gpu-result sections.
corpus_query!(tpch, 1, global_stddev, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, duckdb_approx, data_fusion_approximate, golden_approx_std, schema_validation_enabled);
```

  (Use `rollup_stddev`'s duckdb argument if it is not `duckdb_approx`.) The registry row, beside
  `rollup_stddev`'s, plan and cpu cells on, gpu off under `216` (open until the merge archives
  it):

```
tpch,1,global_stddev,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,disabled,disabled,disabled,disabled,disabled,ok,stddev_var,216
```

- [ ] **Step 3: Write the sections.**

```bash
UPDATE_CANONICAL=1 timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
UPDATE_CANONICAL=1 timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- global_stddev
[ -x /tmp/duckdb-1.5.4/bin/python ] || { python3 -m venv /tmp/duckdb-1.5.4 && /tmp/duckdb-1.5.4/bin/pip install duckdb==1.5.4; }
timeout 1800 /tmp/duckdb-1.5.4/bin/python testdata/duckdb_result.py --dataset tpch
git diff --stat testdata/
git diff testdata/goldens/tpch.sf1/duckdb-result.txt | grep '^[-+][^-+]' | grep -v 'global-stddev' | head
```

  The system `python3` has no `duckdb`, and the script refuses any version but 1.5.4: hence the
  venv, outside the repo (grouping-id's recipe). Never `--only`: `duckdb_result.py` writes only
  the sections it ran. Expected: the five
  `.plans.txt`, five `.cpu.txt`, five `.cost.txt` and `mini.result.txt` gain a `global-stddev`
  section each and nothing else moves; `duckdb-result.txt` gains one section of one row and the
  last command prints nothing. `recipe-payloads.txt` unchanged (the query is not in
  `PAYLOAD_QUERIES`: it adds no fb kind or call shape). Read `tp4-single.plans.txt`'s section:
  one `GpuAggregate` init per lane, the merges, a finalize project, no `refused:`. A cpu mode
  that fails on an open ticket comes off the line and its registry cell takes the ticket; an
  unknown failure is filed in `tickets/corpus-coverage.md` (at most 15 lines).

- [ ] **Step 4: Verify without writing.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- global_stddev registry
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_cost_model
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --test test_gpu_corpus --features gpu --no-run
```

  Expected: PASS, the registry tests both ways and duckdb-oracle's `gpu-result.txt` guard (no
  device cell of `global_stddev` is on yet).

- [ ] **Step 5: Commit.**

```bash
git add testdata peacockdb-core/tests/common/corpus_cases.inc
git commit -m "#216: tpch/global-stddev in the corpus, device cells off until their run

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: The device corpus, the cells, the docs

**Files:**
- Modify: `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`,
  `testdata/goldens/{tpch,pbench}.sf1/gpu-result.txt`
- Modify: `llm-wiki/architecture.md`, `llm-wiki/build-test.md`,
  `llm-wiki/tickets/corpus-coverage.md`, `tickets/complete-coverage.md`, and
  `tickets/system-hardening.md` only as Step 4 says; append: `llm-wiki/tasks/welford-device-detail.md`

- [ ] **Step 1: The rows, and their device cells on, uncommitted.**

```bash
awk -F, 'NR > 1 && $NF ~ /(^| )(216|94)( |$)/ {print NR": "$1"/"$3": "$NF}' testdata/cost-registry.csv
grep -n '216\|#94' peacockdb-core/tests/common/corpus_cases.inc
```

  Expected (confirm against the registry as it stands): `pbench/empty_dispersion_aggregates`
  and `tpch/global_stddev`. For each: its `corpus_query!` gpu modes become what its cpu modes
  are (`all_modes`, or the five spelled out), the `// device: #216` comment and the last
  sentence of `global_stddev`'s comment go; its registry row's five gpu cells `enabled`, `216`
  and `94` struck from its tickets, and any archived number beside them (keyless-identity wrote
  the dispersion row's as `199 216`) struck too, since no cell of it is off. Any other row a grep
  lists is run the same way.

- [ ] **Step 2: The device corpus**, one sync and one build (*Device cycle* without the patch:
  the code is committed, only the cells move):

```bash
timeout 1200 rsync -a --delete-after --exclude=.git --filter=':- .gitignore' ./ "$GPU:$DIR/"
timeout 10800 ssh "$GPU" "$B"
timeout 7200 ssh "$GPU" "$R PCK_WRITE_GPU_RESULT=1 cpp/install/rust-tests/test_gpu_corpus --test-threads=1 global_stddev empty_dispersion"
timeout 7200 ssh "$GPU" "$R cpp/install/rust-tests/test_gpu_corpus --test-threads=1 shuffle_stddev rollup_stddev"
timeout 600 rsync -a "$GPU:$DIR/testdata/goldens/" testdata/goldens/ --include='*/' --include='gpu-result.txt' --exclude='*'
git diff --stat testdata/goldens/
```

  Any other row Step 1 listed joins the first corpus filter. Only that run writes: the writer
  merges per (query, mode) section, so `gpu-result.txt` gains `global-stddev`'s five sections
  (tpch) and `empty-dispersion-aggregates`' five (pbench), and no other section moves; the second
  run, the spec's `shuffle-stddev` and `rollup-stddev`, writes nothing, since a float cell's
  digits move run to run. Expected: both runs green. A cell that fails goes back off under the
  ticket it fails on (an open one, or a new one with the query, mode and message, at the next
  free number), in the line and the registry. Record the run in `welford-device-detail.md`.

- [ ] **Step 3: `architecture.md`.** Short sentences; each a current fact.
  - The aggregate sequence (re-locate by "on the device except the keyless Welford path"): the
    exception goes — a node with a finalize list emits finalized columns on both engines. Add:
    a keyless node holding a `stddev` or `var` runs on the device through the grouped request
    builder on one constant key, since cuDF has `M2` and `MERGE_M2` for groupby only; over zero
    rows it answers `empty_state`'s row.
  - "Every aggregate merges as state and finalizes in a project" (re-locate by "The device's
    keyless Welford path is the one exception"): that sentence goes.
  - The state types paragraph (re-locate by "the Welford moments are `Float64`"): the Welford
    state is never NULL on either engine; a group with no valid value is `(0, 0.0, 0.0)`,
    DataFusion's own, which the device writes where cuDF answers NULL, at the init and after
    every `MERGE_M2`.
  - "…stay in C++ with a reason" (re-locate by "cuDF counts in INT32"), unless verify-26.02
    already says it: add `MERGE_M2`'s count child, cast to the type the linked cuDF takes
    (`INT32` before 25.06, `INT64` from 25.06, chosen by `kMergeM2CountType` from
    `<cudf/version_config.hpp>`) and widened back to `INT64`. Fix the paragraph's count word.
  - The `CudfAggregate.mode` row of *What the Rust side puts in the flat buffers*: the clause
    "except on the keyless path, where a `stddev` name decides all three…" goes.
  - The `AggregateFuncNode.ddof` row (aggregate-arms wrote it as the keyless stddev's divisor):
    carried for the plan; no device code reads it, since the finalize is a project — or, if Task 3
    Step 3's grep found the builder checking it, that it is checked and nothing else.
  - The `CudfAggregate` row of the node table (re-locate by "`cudf::reduce`"): `cudf::reduce`
    for a keyless node with no `stddev` or `var`; a keyless node holding one runs the groupby on
    one constant key.
- [ ] **Step 4: Tickets** (a ticket is about code; fix its facts here). #216 and, where this task
  closes it, #94 are archived at merge by the helper, as chain K's plans leave theirs.
  - `corpus-coverage.md` #216: the keyless path routes a node holding a Welford to
    `keyless_welford_aggregate`.
  - `corpus-coverage.md` #94, **only if Task 1 found no verify-26.02 gate**: the site is
    `merge_m2_input`, the one `MERGE_M2` site, with no Final arm; its "Fix proposed" becomes the
    fix as built — `kMergeM2CountType` (`plan_executor_internal.h`) is
    `merge_m2_count_type(CUDF_VERSION_MAJOR, CUDF_VERSION_MINOR)` from
    `<cudf/version_config.hpp>`, `INT32` before 25.06 and `INT64` from 25.06; no CMake define, no
    runtime probe. With a gate, #94 is verify-26.02's and this task does not touch it.
  - `complete-coverage.md` #261, the paragraph starting "After [#216]": "The device runs a keyless
    Welford through the grouped request builder ([#216](corpus-coverage.md#t216)), so a keyless
    outer stage needs only the per-function phase above. The `bug_` test flips to a plan test
    and a cpu-vs-device case."
  - `system-hardening.md` #260, **only if Task 1 found no gate and #260 is still open there**
    (`grep -n '<a id="t260">' llm-wiki/tickets/system-hardening.md`): the sentence naming
    `gpu_tpch_shuffle_stddev_tp1_single` says its 26.02 failure is #94's count type, which
    `kMergeM2CountType` now gates at compile time, not yet run on a 26.02 device. Otherwise no
    edit: verify-26.02 settled it.
- [ ] **Step 5: `build-test.md` counts.** Recount each row this task touches from the code (a
  `--list` per binary) and set every header and the grand total to the sums. This task's
  deltas: C++ CPU/FFI unit +2 where Task 1 added `MergeM2CountType` (name the count type in the
  row's prose); Plan-executor (C++) +8 (`AggregateBuilder`: Task 2's four, Task 3's four);
  Operator harness +9 (`welford_cases.rs`: Task 2's four, Task 3's five; the five flipped pins
  keep their count but leave the `bug_` list — the sentence naming the `bug_` pins loses the
  keyless Welford's clause and its number is recounted); Operator harness, what the device holds
  +1 (`a_global_stddev_merge_holds_its_declared_welford_state`); the cpu corpus +5 cells and one
  query (`tpch/global-stddev`), and one DuckDB comparison case if duckdb-oracle counts them per
  line; the device corpus +10 cells (`global_stddev`, `empty_dispersion_aggregates`).
- [ ] **Step 6: The full verification bar, locally.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2
timeout 10800 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_cost_model
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests:: planner::tests::plan_goldens::the_payload_golden
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
awk -F, 'NR > 1 && $NF ~ /(^| )(216|94)( |$)/ {print NR": "$1"/"$3": "$NF}' testdata/cost-registry.csv
grep -n '216\|#94' peacockdb-core/tests/common/corpus_cases.inc
git status --short testdata/goldens/recipe-payloads.txt
```

  Arm a monitor for the corpus run (progress every 2 minutes, matching
  `panicked|FAILED|error\[`). Expected: all green, the registry tests both ways and the
  `gpu-result.txt` guard included; the awk and the grep print nothing but a cell Step 2 sent back
  off under `216` or `94`; `recipe-payloads.txt` unchanged. The 26.02 build leg is CI's.

- [ ] **Step 7: Commit.**

```bash
git add testdata peacockdb-core/tests/common/corpus_cases.inc llm-wiki
git commit -m "#216, #94: global-stddev and empty-dispersion on the device; docs, counts

Device cells on where the run passed, gpu-result sections written,
216 struck from the registry.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

  Where Task 1 found verify-26.02's gate, #94 is not this task's: the first line reads
  `#216: global-stddev and empty-dispersion on the device; docs, counts`.

## Not changed, on purpose

- **The cpu backend.** DataFusion's variance accumulators already emit a NULL-free state and its
  merge already skips a count-0 entry (`VarianceAccumulator::merge_batch`); `merge_m2.rs` is
  untouched.
- **The keyless `cudf::reduce` path** for a node with no Welford, its decimal-sum constant-key
  arm included: the spec's Restriction.
- **`recipe-payloads.txt` and `PAYLOAD_QUERIES`.** `global-stddev` adds no fb kind or call shape;
  `shuffle-stddev` already covers the Welford payload.
- **`cpp/CMakeLists.txt`.** The count type reads cuDF's own `CUDF_VERSION_MAJOR`/`_MINOR`, which
  `<cudf/version_config.hpp>` declares in 25.02 and 26.02 alike; a CMake define would be a second
  source of the same fact.
