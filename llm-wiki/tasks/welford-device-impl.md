# welford-device implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A keyless `stddev` or `var` runs on the device, init and merge, beside any other
aggregate and over zero rows (#216); the Welford state is NULL-free on the device as it is on the
cpu, so an all-NULL group is `(0, 0.0, 0.0)` everywhere; `MERGE_M2`'s count type is chosen at
compile time from the cuDF the build links (#94); `tpch/global-stddev` joins the corpus and
pbench's `empty-dispersion-aggregates` runs on the device.

**Architecture:** All in `cpp/src/operators/aggregate.cpp`, on the one request builder
aggregate-arms leaves. `execute_aggregate` sends a keyless node that holds a Welford function to
`keyless_welford_aggregate`, which runs `grouped_aggregate` on one constant `INT32` key, drops
it, and over zero rows answers `keyless_empty_state`'s one row. `StateReadback` gains
`NullAsZero` for the init's mean and m2, and `MergedChild` zero-fills the merged moments too.
`cpp/CMakeLists.txt` passes the cuDF major and minor as `PEACOCK_CUDF_VERSION_*`;
`merge_m2_count_type(major, minor)` (`plan_executor_internal.h`, host-only so the cpu gtests
reach it) picks the type `kMergeM2CountType` is defined from. No Rust production code changes.

**Tech stack:** C++20 against cuDF 25.02 (`cpp/build`, local; nebius-gpu for the device) and
26.02 (`cpp/build26`, a local compile of the other branch); gtest; Rust harness cases
(`--features gpu`, compiled locally, run on the device); the cpu corpus (`--features rust-only`);
DuckDB 1.5.4 for the new query's oracle section.

**Spec:** [`welford-device.md`](welford-device.md). Built on
[`aggregate-arms-impl.md`](aggregate-arms-impl.md) (Task 6's *Interfaces*: `AggPhase`, `AggKind`,
`StateReadback`, `StateColumn`, `AggregateRequests`, `add_requests`, `append_state_columns`,
`merge_m2_input`, `kMergeM2CountType`, `grouped_aggregate`, `grouping_set_aggregate`;
Task 5's `column_at`) and [`keyless-identity-impl.md`](keyless-identity-impl.md)
(`empty_state`, `CallPattern::AtDoneIfNothingOut`, the init accumulators, `was_handed_nothing`,
`zero_rows_of`, the identity cases). Format models: [`guard-checks-impl.md`](guard-checks-impl.md),
[`distinct-companions-impl.md`](distinct-companions-impl.md).

**Where this plan departs from the spec, and why** (each is in the reply to the human too):

- **The count-type threshold is 25.06, not 25.10.** cuDF changed `MERGE_M2`'s count child to
  `INT64`/`FLOAT64` in [rapidsai/cudf#18546](https://github.com/rapidsai/cudf/pull/18546)
  (commit `88ec78a6ab`, 2025-04-23), which is on `branch-25.06` and not in `v25.04.00`
  (`/media/data/repo/cudf`: `git merge-base --is-ancestor`). Gated at 25.10, a 25.06 or 25.08
  build would cast to `INT32` and fail at run time. 25.02 and 26.02, the two builds there are,
  pick the same type either way.
- **The merge's moments are zero-filled too, not only the init's.** cuDF's `MERGE_M2` answers a
  NULL mean and m2 for every group whose merged counts are all 0 (`group_merge_m2.cu`,
  `is_valid = count > 0`, in 25.02 and 25.10 alike). So a per-lane merge of a group that was
  all NULL on that lane hands the cross-lane merge NULL children, and its state is not
  DataFusion's `(0, 0.0, 0.0)`. The spec's "no `MERGE_M2` meets a NULL child" holds only if the
  merge's own output is replaced as well. No answer was ever wrong — cuDF reads a count-0
  entry as `(0, 0, 0)` — but the state differs, which is what item 3 fixes.
- **The new Rust cases go in a new file, `src/tests/gpu_tests/welford_cases.rs`**, beside the
  three the spec names: `aggregate_cases.rs` is near the 1000-line cap after keyless-identity.
  The flipped pins stay where they are.

## Before you start: what will have moved

This plan was written against master at 64ced62e, where `aggregate.cpp` is the 833-line file with
the keyless `cudf::reduce` path at ~231-327, `make_reduce_agg` at ~117, the Merge/Final
`MERGE_M2` arms at ~633-701, and `cpp/CMakeLists.txt`'s `find_package(cudf)` /
`message(STATUS "Using host cudf: ${cudf_VERSION}")` at ~70-71. **None of that survives
aggregate-arms**, which rewrites the file. Write against the code as aggregate-arms-impl.md
Task 6 leaves it, and **re-locate every site by symbol, never by line**:

- **`aggregate.cpp` (aggregate-arms).** By symbol: `enum class AggKind`, `agg_kind`,
  `check_shape`, `value_of`, `enum class StateReadback`, `struct StateColumn`,
  `struct AggregateRequests`, `add_requests` (its `case AggKind::Welford:`), `as_int64_count`,
  `append_state_columns` (its `case StateReadback::MergedChild:`), `kMergeM2CountType`,
  `merge_m2_input`, `grouped_aggregate`, `grouping_set_aggregate`, `keyless_aggregate`
  (its stddev arm, `make_std_aggregation<cudf::reduce_aggregation>(func->ddof())`), and
  `execute_aggregate`'s `if (keys.empty()) return keyless_aggregate(…)`. If `is_stddev_name` /
  `is_var_name` survived aggregate-arms, they go here (Task 3). If any of these names differs,
  follow the code and say so in the detail file.
- **`aggregate.cpp` (keyless-identity).** `execute_aggregate` builds its input as
  `was_handed_nothing(in) ? zero_rows_of(agg) : take_input(in)`: a done call reaches the routed
  path as a zero-row table, so this plan adds nothing for the no-input case.
- **`TableResult` (refcounted-scatter, J).** `owning(table, names)`, `select(ordinals)` (shares
  owners), `view()`, `num_rows()`, `num_columns()`; built only through its constructors
  (aggregate-arms Task 1 makes the default constructor private).
- **`MERGE_M2`'s count type may already be gated.** verify-26.02 (J's last task) fixes what fails
  on 26.02, and `shuffle-stddev` is predicted to fail there on #94
  (`reports/join-rewrite-cell-estimate.md`, R8). Before Task 1: `git log -S MERGE_M2 --oneline --
  cpp/src/operators/aggregate.cpp` and `git grep -n 'INT32\|INT64' cpp/src/operators/aggregate.cpp`.
  If a gate exists, Task 1 keeps its site, moves its rule into `merge_m2_count_type` with the
  25.06 threshold, and adds the tests; say so in the detail file.
- **gtests.** `cpp/tests/gpu/hand_tables.hpp` (`hand::column_of`, `values_of`, `table_of`,
  `ref`, `run`) and `cpp/tests/gpu/test_aggregate_builder.cpp` (anonymous-namespace `func`,
  `aggregate`, `rows`, `one`, `by_key`, `refusal`) are aggregate-arms'; this plan adds to both.
  `test_plan_executor.cpp` is past the 1000-line cap: nothing new goes there.
- **Harness (keyless-identity).** Every `GpuAggregate` case runs `Script::Accumulate`, not
  `Script::Exec`: an init over one batch answers its state at slot 0 and an empty done slot.
  The identity cases (`DEVICE_IDENTITY`, `keyless_init`, `empty_state_row`, `at_done`) are in
  `aggregate_cases.rs`, or in `aggregate_identity_cases.rs` if keyless-identity moved them there
  for size; find them with `git grep -n DEVICE_IDENTITY peacockdb-core/src`.
- **Harness (aggregate-arms).** `welford_answered` no longer borrows the device's names; the four
  #216 pins in `aggregate_dimension_cases.rs` carry moved messages (`global_stddev_finalize`'s is
  `column_at`'s `"is past the 1 columns its input has"`), and the schema pin's divergence lost
  its second name. Re-locate them by the names in the spec's *Tests*.
- **Corpus (duckdb-oracle, J; keyless-identity).** Every `corpus_query!` has a `duckdb_oracle`
  argument before the cpu oracle; `all_modes` exists. `tpch/rollup-stddev`'s line is
  aggregate-arms'; copy its shape. pbench's `empty_dispersion_aggregates` line is
  keyless-identity's, gpu modes `none`, with a `// device: #216` comment; its registry row's
  tickets are `216`. **Build Task 5's row list from the registry as it stands**, not from this
  plan.
- **The GPU host's directory.** keyless-identity's and grouping-id's plans use `~/peacockdb-L`,
  aggregate-arms' uses `~/peacockdb-J`. Use the one `keyless-identity-detail.md` and
  `aggregate-arms-detail.md` record as holding the warm build and the sf1 data; this plan writes
  `DIR=peacockdb-L`.

## Global constraints

- **Every commit green.** Device tests a task writes are compiled locally
  (`peacock_plan_tests` built; `--features gpu --no-run`) and run in the round's one GPU cycle
  (Task 5). A `bug_` test that a task's fix turns red is flipped in that task's commit to the
  agreement case, under a name without `bug_` (`coding-style.md`, *Building around a bug*).
  `global_stddev`'s and `empty_dispersion_aggregates`' device cells stay off under `216` until
  the cycle that writes their `gpu-result.txt` sections (Task 5).
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
  the primary checkout if absent.
- **The C++ tests that need device memory run on nebius-gpu, not in `ctest -L cpu`.**
  `peacock_plan_tests` (where `test_aggregate_builder.cpp` links) is labelled `gpu`; only
  `peacock_cpu_tests` runs locally.
- **Device** on nebius-gpu, `dmitry@89.169.109.150`, cuDF 25.02
  (`~/data/miniforge3/envs/rapids-cuda-12.2`), as chain L's header says: the working tree
  rsynced uncommitted, one GPU cycle per round, every CPU build and run local, every device
  command in the foreground (a backgrounded build chain dies mid-build). If `ssh` seems dead
  while the host answers a raw socket, the sandbox is blocking the client, not the host. Record
  each run (host, tree, per-binary result) in `llm-wiki/tasks/welford-device-detail.md`.
- Formatting on changed lines only: `git clang-format HEAD -- <files>`; `rustfmt --edition 2024
  --check <leaf>` on touched leaves, never a `mod.rs`. Files under 1000 lines, functions under
  150. Commit messages at most 10 lines, ending with the `Co-Authored-By` line shown.

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
5. **The 26.02 compile branch.** The `INT64` branch is compiled only by CI's "26.02" leg (a
   25.10a image, #129) and never run. Expected: the rule picks `INT32` at 25.02 and 25.04,
   `INT64` at 25.06 and later, and each build's defines are the cuDF its headers declare.
   Task 1: `MergeM2CountType.*` in `peacock_cpu_tests`, run locally against both 25.02
   (`cpp/build`) and 26.02 (`cpp/build26`).

## File structure

| file | responsibility |
|---|---|
| `cpp/CMakeLists.txt` | `PEACOCK_CUDF_VERSION_MAJOR`/`_MINOR` on `peacock_gpu` (Task 1) |
| `cpp/src/plan_executor_internal.h` | `merge_m2_count_type` (Task 1) |
| `cpp/src/operators/aggregate.cpp` | `kMergeM2CountType` from the defines (Task 1); `NullAsZero`, the merged moments (Task 2); the keyless route and `keyless_empty_state`; the stddev reduce arm goes (Task 3) |
| `cpp/tests/cpu/test_executor.cpp` | `MergeM2CountType.*` (Task 1) |
| `cpp/tests/gpu/hand_tables.hpp` | `hand::nullable_column_of` (Task 2) |
| `cpp/tests/gpu/test_aggregate_builder.cpp` | the all-NULL, NaN and merge gtests (Task 2); the keyless gtests (Task 3) |
| `peacockdb-core/src/tests/gpu_tests/welford_cases.rs` (new), `gpu_tests/mod.rs` | the all-NULL, NaN, lane-merge cases (Task 2); the keyless cases (Task 3) |
| `peacockdb-core/src/tests/gpu_tests/aggregate_dimension_cases.rs`, `aggregate_schema_cases.rs`, `aggregate_cases.rs` (or `aggregate_identity_cases.rs`) | the #216 pins flip; `EVERY_FUNC` (Task 3) |
| `testdata/tpch-queries/global-stddev.sql` (new), `testdata/goldens/tpch.sf1/`, `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv` | the new query (Task 4); its device cells, empty-dispersion's (Task 5) |
| `testdata/goldens/{tpch,pbench}.sf1/gpu-result.txt` | the device's sections (Task 5) |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets/{corpus-coverage,complete-coverage,system-hardening}.md` | docs, counts, ticket facts (Task 5) |

---

### Task 1: `MERGE_M2`'s count type, chosen from the cuDF the build links (#94)

**Files:**
- Modify: `cpp/CMakeLists.txt` (after the cuDF block that ends with `message(STATUS "Using host
  cudf: …")`; after `target_link_libraries(peacock_gpu …)`)
- Modify: `cpp/src/plan_executor_internal.h`, `cpp/src/operators/aggregate.cpp`
  (`kMergeM2CountType`)
- Test: `cpp/tests/cpu/test_executor.cpp`

**Interfaces:**
- Produces (`plan_executor_internal.h`, namespace `peacock`; host-only, so `peacock_cpu_tests`
  reaches it):

```cpp
// The type cuDF's MERGE_M2 takes its count child in: INT32 through 25.04, and INT64 from 25.06,
// where cuDF #18546 widened it against overflow and refused INT32. A build links one cuDF, so
// aggregate.cpp picks the type at compile time from the version CMake passes (#94).
constexpr cudf::type_id merge_m2_count_type(int major, int minor) {
  return major < 25 || (major == 25 && minor < 6) ? cudf::type_id::INT32 : cudf::type_id::INT64;
}
```

- Produces (CMake, `PUBLIC` on `peacock_gpu`, so every target linking it is compiled with them):
  `PEACOCK_CUDF_VERSION_MAJOR`, `PEACOCK_CUDF_VERSION_MINOR`, decimal integers.
- Changes (aggregate-arms' ⟵ hook): `kMergeM2CountType` is
  `merge_m2_count_type(PEACOCK_CUDF_VERSION_MAJOR, PEACOCK_CUDF_VERSION_MINOR)`.

- [ ] **Step 1: The rule's tests, failing.** `test_executor.cpp`, after the includes add
  `#include <cudf/version_config.hpp>`, and at the end of the file:

```cpp
// #94: MERGE_M2 takes its count child as INT32 through cuDF 25.04 and as INT64 from 25.06
// (cuDF #18546). Both CI legs run these, so each build checks the branch it compiled.
TEST(MergeM2CountType, Int32ThroughCudf25_04AndInt64From25_06) {
  EXPECT_EQ(peacock::merge_m2_count_type(25, 2), cudf::type_id::INT32) << "25.02, the GPU hosts'";
  EXPECT_EQ(peacock::merge_m2_count_type(25, 4), cudf::type_id::INT32);
  EXPECT_EQ(peacock::merge_m2_count_type(25, 6), cudf::type_id::INT64);
  EXPECT_EQ(peacock::merge_m2_count_type(25, 10), cudf::type_id::INT64);
  EXPECT_EQ(peacock::merge_m2_count_type(26, 2), cudf::type_id::INT64) << "26.02, CI's other leg";
}

// The defines are what CMake read off cudf_VERSION (or the submodule's VERSION); the headers
// say which cuDF this file actually compiled against. A parse that read "08" as octal, or a
// stale cache, would pick the other branch here first.
TEST(MergeM2CountType, TheBuildIsToldTheCudfItCompilesAgainst) {
  EXPECT_EQ(PEACOCK_CUDF_VERSION_MAJOR, CUDF_VERSION_MAJOR);
  EXPECT_EQ(PEACOCK_CUDF_VERSION_MINOR, CUDF_VERSION_MINOR);
}
```

```bash
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
```

  Expected: FAIL to compile, `'merge_m2_count_type' is not a member of 'peacock'` (and
  `PEACOCK_CUDF_VERSION_MAJOR` undeclared).

- [ ] **Step 2: The defines.** `cpp/CMakeLists.txt`, directly after the cuDF block's `endif()`:

```cmake
# The cuDF this build compiles against, for call sites whose contract moved between releases
# (aggregate.cpp's MERGE_M2 count type, #94). Built from source it is the submodule's VERSION,
# which rapids_config.cmake read above; find_package, which sets cudf_VERSION, does not run.
if(CUDF_BUILD_FROM_SOURCE)
  set(PEACOCK_CUDF_VERSION "${RAPIDS_VERSION}")
else()
  set(PEACOCK_CUDF_VERSION "${cudf_VERSION}")
endif()
if(NOT PEACOCK_CUDF_VERSION MATCHES "^([0-9]+)\\.([0-9]+)\\.")
  message(FATAL_ERROR "cannot read major.minor from the cuDF version `${PEACOCK_CUDF_VERSION}`")
endif()
# math() reads 02 and 08 as decimal; written into the source as is, 08 is no C++ literal.
math(EXPR PEACOCK_CUDF_VERSION_MAJOR "${CMAKE_MATCH_1}")
math(EXPR PEACOCK_CUDF_VERSION_MINOR "${CMAKE_MATCH_2}")
message(STATUS "peacock: cuDF ${PEACOCK_CUDF_VERSION_MAJOR}.${PEACOCK_CUDF_VERSION_MINOR}")
```

  and after `target_link_libraries(peacock_gpu PRIVATE cudf::cudf …)`:

```cmake
# PUBLIC: a test linking the library compiles against the same cuDF and checks the choice.
target_compile_definitions(peacock_gpu PUBLIC
  PEACOCK_CUDF_VERSION_MAJOR=${PEACOCK_CUDF_VERSION_MAJOR}
  PEACOCK_CUDF_VERSION_MINOR=${PEACOCK_CUDF_VERSION_MINOR})
```

  (`peacockdb-ffi/build.rs` configures the same `cpp/CMakeLists.txt`, so the FFI's copy of the
  library gets them too.)

- [ ] **Step 3: The rule and the constant.** Add *Interfaces*' `merge_m2_count_type` to
  `plan_executor_internal.h` (after `cudf_ast_can_evaluate`; `<cudf/types.hpp>` is already
  included). In `aggregate.cpp` (`#include "plan_executor_internal.h"` if `column_at` did not
  already bring it), replace aggregate-arms' `kMergeM2CountType` and its comment with:

```cpp
#if !defined(PEACOCK_CUDF_VERSION_MAJOR) || !defined(PEACOCK_CUDF_VERSION_MINOR)
#error "cpp/CMakeLists.txt passes the cuDF version: MERGE_M2's count type depends on it (#94)"
#endif

/// MERGE_M2's count child, in the type the linked cuDF takes. merge_m2_input casts to it and
/// as_int64_count widens the merged count back to the wire's INT64, a no-op from 25.06.
constexpr cudf::type_id kMergeM2CountType =
    merge_m2_count_type(PEACOCK_CUDF_VERSION_MAJOR, PEACOCK_CUDF_VERSION_MINOR);
```

  Nothing else in `merge_m2_input` or `as_int64_count` changes.

- [ ] **Step 4: Build and run against 25.02.**

```bash
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --configure --build 2>&1 | tee /tmp/welford-build.log | tail -5
grep -m1 'peacock: cuDF' /tmp/welford-build.log
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
```

  (`--configure` re-runs cmake so the new block takes effect.) Expected: the grep prints
  `-- peacock: cuDF 25.2`; clean build; `-L cpu` PASS, both `MergeM2CountType` cases included.

- [ ] **Step 5: Compile and run the other branch against 26.02, locally.** `cpp/build26` is the
  26.02 dir (`build-test.md`); configure it the way `build-test.sh` does, build only the cpu
  gtests and the library under them:

```bash
(
  export PATH=$HOME/data/miniforge3/envs/rapids/bin:$PATH \
         CC=/usr/bin/gcc-14 CXX=/usr/bin/g++-14 \
         CUDACXX=$HOME/data/miniforge3/envs/rapids/bin/nvcc \
         LDFLAGS="-Wl,-rpath-link,$HOME/data/miniforge3/envs/rapids/lib"
  timeout 1800 cmake -S cpp -B cpp/build26 -G Ninja -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_CUDA_ARCHITECTURES="80;90" "-DCMAKE_JOB_POOLS=link_pool=1" -DCMAKE_JOB_POOL_LINK=link_pool \
    -Dcudf_ROOT=$HOME/data/miniforge3/envs/rapids
  timeout 5400 cmake --build cpp/build26 --target peacock_cpu_tests --parallel "$(nproc)"
)
LD_LIBRARY_PATH=$HOME/data/miniforge3/envs/rapids/lib timeout 600 \
  ctest --test-dir cpp/build26 -L cpu --output-on-failure -R peacock_cpu_tests
```

  Expected: the configure prints `peacock: cuDF 26.2`; `aggregate.cpp` compiles with
  `kMergeM2CountType == INT64`; `MergeM2CountType.*` PASS. If the 26.02 configure fails for a
  reason outside this task (a fresh `cpp/build26` in the workspace fetches flatbuffers), record it
  in the detail file and rely on CI's leg; do not change the build scripts here.

- [ ] **Step 6: Commit.**

```bash
git clang-format HEAD -- cpp/src/plan_executor_internal.h cpp/src/operators/aggregate.cpp cpp/tests/cpu/test_executor.cpp
git add cpp/CMakeLists.txt cpp/src/plan_executor_internal.h cpp/src/operators/aggregate.cpp cpp/tests/cpu/test_executor.cpp
git commit -m "#94: MERGE_M2's count type chosen at compile time from the linked cuDF

INT32 through 25.04, INT64 from 25.06 (cuDF #18546). CMake passes the
version; merge_m2_count_type is the rule, checked in both CI legs.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: The Welford state is NULL-free: all-NULL groups are `(0, 0.0, 0.0)`

The builder is one for the grouped, grouping-set and (after Task 3) keyless paths, so this lands
once. cuDF answers NULL in two places DataFusion answers `0.0`: `MEAN` and `M2` over a group with
no valid value, and `MERGE_M2` for a group whose merged counts are all 0. Both readbacks replace
it.

**Files:**
- Modify: `cpp/src/operators/aggregate.cpp` (`StateReadback`, `add_requests`'s Welford Partial
  arm, `append_state_columns`; a static `zero_for_null`)
- Modify: `cpp/tests/gpu/hand_tables.hpp`; Test: `cpp/tests/gpu/test_aggregate_builder.cpp`
- Create: `peacockdb-core/src/tests/gpu_tests/welford_cases.rs`; Modify: `gpu_tests/mod.rs`

**Interfaces:**
- Changes aggregate-arms' `StateReadback` (the ⟵ hook "a readback on the init's `$mean`/`$m2`"):

```cpp
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

  Red before Step 4, on the device: the init's k 2 mean and m2 are NULL, and so are the merge's
  k 1 moments in `AMergeOfOnlyEmptyStatesIsZeroNotNull` (cuDF's `is_valid = count > 0`). The
  other two pass before and after; they pin that the replacement adds nothing else.

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

  Expected: compiles. Red on the device before Step 4: the all-NULL init and the merge cases
  panic in `same_within_welford` on `cpu Some(0.0), device None`.

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

  In `add_requests`' Welford Partial arm, the mean's and m2's state columns read back
  `NullAsZero`:

```cpp
        into.columns.push_back({state(0), r, 0, StateReadback::CountToInt64});
        into.columns.push_back({state(1), r, 1, StateReadback::NullAsZero});
        into.columns.push_back({state(2), r, 2, StateReadback::NullAsZero});
```

  In `append_state_columns`, a new arm and the `MergedChild` arm:

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

  Expected: clean build, no new warnings; PASS; both files under 1000 lines. The four gtests and
  the four harness cases run in Task 5's cycle.

- [ ] **Step 6: Commit.**

```bash
git clang-format HEAD -- cpp/src/operators/aggregate.cpp cpp/tests/gpu/hand_tables.hpp cpp/tests/gpu/test_aggregate_builder.cpp
git add cpp/src/operators/aggregate.cpp cpp/tests/gpu peacockdb-core/src/tests/gpu_tests
git commit -m "Welford state NULL-free on the device: all-NULL groups are (0, 0.0, 0.0)

The init's mean and m2, and MERGE_M2's merged moments, read back with
NULL replaced by 0.0, as DataFusion's state is.

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
/// answer `no_groups`: a count 0, a Welford (0, 0.0, 0.0), anything else NULL.
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

  Red before Step 3: the init answers one finished `STD` column, the `var` merge is refused as
  `unsupported aggregate function: var`, and the zero-row case answers one column.

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
      case AggKind::Count:
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
git grep -n 'make_std_aggregation\|make_variance_aggregation\|is_stddev_name\|is_var_name\|#216' cpp/src
```

  Expected: nothing but the new route's comment.

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
  Make `empty_state_row` and `at_done` `pub(crate)`: Step 6 uses them.

- [ ] **Step 6: The keyless cases.** Append to `welford_cases.rs` (extend the imports with
  `datafusion::arrow::datatypes::DataType`; `super::aggregate_cases::{body, call, input,
  merge_over, welford_init_aggs}`; `at_done` and `empty_state_row` from wherever Step 5 found
  them (`aggregate_cases` or `aggregate_identity_cases`);
  `super::aggregate_dimension_cases::welford_init_global`; `super::script::each_answers`;
  `crate::plan::{AggCall, AggSpec, AggStateColumns, BatchLayout, Expr, GpuAggregate, NamedExpr,
  PlanAgg, Schema, finalize}`; `crate::tests::compare::Order`;
  `crate::tests::given::{Given, columns}`; `crate::tests::synthetic::{decimals, schema}`):

```rust
/// `count(i32)`, `stddev(f64)` and `sum(i64)` with no key, as
/// `SELECT count(i32), stddev(f64), sum(i64)` plans: a Welford beside what a keyless node
/// reduces when it holds none.
fn keyless_mixed_state() -> Schema {
    let mut state = columns(&[
        ("count(i32)", DataType::Int64),
        ("stddev(f64)$count", DataType::Int64),
        ("stddev(f64)$mean", DataType::Float64),
        ("stddev(f64)$m2", DataType::Float64),
        ("sum(i64)", DataType::Int64),
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
    ];
    merge_over(state.clone(), body(Vec::new(), aggs, None), state)
}

operator_case! {
    GpuAggregate,
    fn a_keyless_stddev_beside_a_sum_and_a_count_agrees() {
        let outcome = run_both(&keyless_mixed_init(), Script::Accumulate(vec![input()]));
        same_within_welford(cpu_slot(&outcome, 0), gpu_slot(&outcome, 0), false, &[2, 3]);
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_keyless_stddev_merge_beside_a_sum_and_a_count_agrees() {
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
```

  Expected: compiles; the grep prints nothing.

- [ ] **Step 7: Build and run what runs locally.**

```bash
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests::
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- executor::cpu_backend::tests::state_types
wc -l cpp/src/operators/aggregate.cpp cpp/tests/gpu/test_aggregate_builder.cpp peacockdb-core/src/tests/gpu_tests/*.rs
```

  Expected: clean, no new warnings; PASS (the cpu identity cases for `Stddev` and `Var`,
  keyless-identity's, unchanged); every file under 1000 lines. The device half runs in Task 5.

- [ ] **Step 8: Commit.**

```bash
git clang-format HEAD -- cpp/src/operators/aggregate.cpp cpp/tests/gpu/test_aggregate_builder.cpp
git add cpp/src/operators/aggregate.cpp cpp/tests/gpu peacockdb-core/src/tests/gpu_tests
git commit -m "#216: a keyless stddev or var runs through the grouped builder

One constant INT32 key, dropped; over zero rows the node answers
empty_state's row. The reduce path's stddev arm goes; the #216 pins
become agreement cases, and the identity cases cover every AggFunc.

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
// grouped request builder on one constant key (#216). Its device cells are turned on by the GPU
// cycle that writes their gpu-result sections.
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
timeout 1800 python3 testdata/duckdb_result.py --dataset tpch
git diff --stat testdata/
git diff testdata/goldens/tpch.sf1/duckdb-result.txt | grep '^[-+][^-+]' | grep -v 'global-stddev' | head
```

  Never `--only`: `duckdb_result.py` writes only the sections it ran. Expected: the five
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
git commit -m "#216: tpch/global-stddev in the corpus, device cells off until the GPU cycle

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: The device cycle, the cells, the docs

**Files:**
- Modify: `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`,
  `testdata/goldens/{tpch,pbench}.sf1/gpu-result.txt`
- Modify: `llm-wiki/architecture.md`, `llm-wiki/build-test.md`,
  `llm-wiki/tickets/corpus-coverage.md`, `tickets/complete-coverage.md`,
  `tickets/system-hardening.md`; Create or append: `llm-wiki/tasks/welford-device-detail.md`

- [ ] **Step 1: The rows, and their device cells on, uncommitted.**

```bash
awk -F, 'NR > 1 && $NF ~ /(^| )(216|94)( |$)/ {print NR": "$1"/"$3": "$NF}' testdata/cost-registry.csv
grep -n '216\|#94' peacockdb-core/tests/common/corpus_cases.inc
```

  Expected (confirm against the registry as it stands): `pbench/empty_dispersion_aggregates`
  and `tpch/global_stddev`. For each: its `corpus_query!` gpu modes become what its cpu modes
  are (`all_modes`, or the five spelled out), the `// device: #216` comment and the last
  sentence of `global_stddev`'s comment go; its registry row's five gpu cells `enabled`, `216`
  and `94` struck from its tickets. Any other row a grep lists is run the same way.

- [ ] **Step 2: The GPU cycle**, the round's one, every command in the foreground:

```bash
GPU=dmitry@89.169.109.150
DIR=peacockdb-L   # see *Before you start*: the directory the chain's earlier cycles used
timeout 1200 rsync -a --delete-after --exclude=.git --filter=':- .gitignore' ./ "$GPU:$DIR/"
timeout 10800 ssh "$GPU" "cd ~/$DIR && . ~/peacock-env.sh && ./scripts/build-test-shadgpu.sh --build"
R="cd ~/$DIR && . ~/peacock-env.sh && export LD_LIBRARY_PATH=\$PWD/cpp/install/lib:\$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib PEACOCK_TESTDATA_DIR=\$PWD/testdata &&"
timeout 1800 ssh "$GPU" "$R cpp/install/bin/peacock_gpu_tests"
timeout 3600 ssh "$GPU" "$R cpp/install/bin/peacock_plan_tests"
timeout 3600 ssh "$GPU" "$R cpp/install/bin/peacock_join_session_tests"
timeout 7200 ssh "$GPU" "$R cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests:: --test-threads=1"
timeout 1800 ssh "$GPU" "$R cpp/install/rust-tests/test_node_timing --test-threads=1"
timeout 7200 ssh "$GPU" "$R PCK_WRITE_GPU_RESULT=1 cpp/install/rust-tests/test_gpu_corpus --test-threads=1 global_stddev empty_dispersion shuffle_stddev rollup_stddev"
timeout 600 rsync -a "$GPU:$DIR/testdata/goldens/" testdata/goldens/ --include='*/' --include='gpu-result.txt' --exclude='*'
git diff --stat testdata/goldens/
```

  Add any other row Step 1 listed to the corpus filter. Not `peacock_cpu_tests` (local), not the
  sf40 pair, not `--run-benchmarks`. A fresh `~/$DIR` without the sf1 parquet takes it from
  `~/peacockdb-J/testdata`. Expected:
  - every gtest green, among them `AggregateBuilder.*` (Task 2's four, Task 3's four, and
    aggregate-arms' nine) — the reds Tasks 2 and 3 predicted are gone;
  - `gpu_tests::` green, the walk tests included: the flipped #216 pins, the two schema cases,
    `welford_cases::*`, and the identity cases over `EVERY_FUNC`;
  - the corpus run green; `gpu-result.txt` gains `global-stddev`'s five sections (tpch) and
    `empty-dispersion-aggregates`' five (pbench), and no other section moves. If the filtered
    writer dropped other sections, re-run the corpus unfiltered with `PCK_WRITE_GPU_RESULT=1`.

  Record the run in `welford-device-detail.md`. On a red: a gtest or harness case is this task's
  to fix (`superpowers:systematic-debugging`), rebuild and rerun only what failed; a corpus cell
  that fails on something else goes back off under the ticket it fails on (an open one, or a new
  one with the query, mode and message), in the line and the registry.

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
    DataFusion's own, which the device writes where cuDF answers NULL.
  - "…stay in C++ with a reason" (re-locate by "cuDF counts in INT32"): add `MERGE_M2`'s count
    child, cast to the type the linked cuDF takes (`INT32` through 25.04, `INT64` from 25.06,
    `merge_m2_count_type`) and widened back to `INT64`. Fix the paragraph's count word.
  - The `CudfAggregate.mode` row of *What the Rust side puts in the flat buffers*: the clause
    "except on the keyless path, where a `stddev` name decides all three…" goes.
- [ ] **Step 4: Tickets** (a ticket is about code; fix its facts here):
  - `corpus-coverage.md` #94: "25.10 and later" → "25.06 and later (cuDF #18546)"; the site is
    `merge_m2_input` through `kMergeM2CountType`, chosen by `merge_m2_count_type` from
    `PEACOCK_CUDF_VERSION_*`. #216's text: the keyless path routes a Welford to
    `keyless_welford_aggregate`. Both are archived at merge by the helper, as chain K's plans
    leave theirs.
  - `complete-coverage.md` #261, the paragraph starting "After [#216]": "The device runs a keyless
    Welford through the grouped request builder ([#216](corpus-coverage.md#t216)), so a keyless
    outer stage needs only the per-function phase above. The `bug_` test flips to a plan test
    and a cpu-vs-device case."
  - `system-hardening.md` (#260), the sentence naming `gpu_tpch_shuffle_stddev_tp1_single`: its
    26.02 failure is #94's count type, which `merge_m2_count_type` now gates at compile time;
    not yet run on a 26.02 device.
- [ ] **Step 5: `build-test.md` counts.** Recount each row this task touches from the code (a
  `--list` per binary) and set every header and the grand total to the sums. This task's
  deltas: C++ CPU/FFI unit +2 (`MergeM2CountType`; name the count type in the row's prose);
  Plan-executor (C++) +8 (`AggregateBuilder`: Task 2's four, Task 3's four); the operator
  harness +10 (`welford_cases.rs`: 4 + 6) and +1 (`a_global_stddev_merge_holds_its_declared_welford_state`),
  the five flipped pins keep their count but leave the `bug_` list — the sentence "Five are
  `bug_` pins: …" loses the keyless Welford's clause and its number is recounted; the cpu corpus
  +5 cells and one query (`tpch/global-stddev`), and one DuckDB comparison case if duckdb-oracle
  counts them per line; the device corpus +10 cells (`global_stddev`, `empty_dispersion_aggregates`).
- [ ] **Step 6: The full verification bar, locally.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2
timeout 10800 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_cost_model
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests:: planner::tests::plan_goldens::the_payload_golden
timeout 3600 scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
git grep -n '216\|#94' peacockdb-core/tests/common/corpus_cases.inc testdata/cost-registry.csv
git status --short testdata/goldens/recipe-payloads.txt
```

  Arm a monitor for the corpus run (progress every 2 minutes, matching
  `panicked|FAILED|error\[`). Expected: all green, the registry tests both ways and the
  `gpu-result.txt` guard included; the grep prints nothing; `recipe-payloads.txt` unchanged.
  The 26.02 build leg is CI's.

- [ ] **Step 7: Commit.**

```bash
git add testdata peacockdb-core/tests/common/corpus_cases.inc llm-wiki
git commit -m "#216, #94: global-stddev and empty-dispersion on the device; docs, counts

Device cells on where the cycle passed, gpu-result sections written,
216 struck from the registry.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

## Not changed, on purpose

- **The cpu backend.** DataFusion's variance accumulators already emit a NULL-free state and its
  merge already skips a count-0 entry (`VarianceAccumulator::merge_batch`); `merge_m2.rs` is
  untouched.
- **The keyless `cudf::reduce` path** for a node with no Welford, its decimal-sum constant-key
  arm included: the spec's Restriction.
- **`recipe-payloads.txt` and `PAYLOAD_QUERIES`.** `global-stddev` adds no fb kind or call shape;
  `shuffle-stddev` already covers the Welford payload.
- **`<cudf/version_config.hpp>`** declares `CUDF_VERSION_MAJOR`/`_MINOR` in both 25.02 and 26.02
  and would make the CMake defines unnecessary; the spec chose the defines, so they stay, and
  `MergeM2CountType.TheBuildIsToldTheCudfItCompilesAgainst` holds them to the headers.
