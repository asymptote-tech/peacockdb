# welford-device — working record

## Standing facts

- Eighth and last of chain K (folded in from chain L), branch `ENS-welford-device` off
  `ENS-aggregate-arms`; its PR targets `ENS-aggregate-arms`. Base master `ca4dedeb`.
- Hosts, per chain K's board note: device work on nebius-gpu, tree `~/peacockdb-K`
  (`gpu-host-sync.sh --dest peacockdb-K`), the L40S list only; cpu runs local (verda not
  reachable). The final device run is the dispatched `GPU Tests (nebius-gpu)` CI job.
- Pull a recorded `gpu-result.txt` home through `gpu-host.sh rsh` before the next sync.
- Adding `global-stddev.sql` moves `testdata/test_duckdb_result.py`'s tpch pin (43 → 44). Run
  every cost-report CI step locally before reporting.
- Spec drift from the chain below: aggregate-arms left one `MERGE_M2` site, `merge_m2_input`
  (`aggregate.cpp`), with `constexpr kMergeM2CountType = INT32` not yet version-gated — item 4
  gates that constant. The keyless path is `keyless_aggregate`; its `stddev` reduction uses
  `func->ddof()`. The finalize's refusal message is now `ColumnRef stddev(…)$m2@2 is past the 1
  columns its input has`, not the spec's `ColumnRef index 2 out of range`. chain J's
  verify-26.02 has not run (`new`, on hold), so #94 is archived here.
- keyless-identity's init done call (`AtDoneIfNothingOut`) is the only zero-row keyless call.

## Dispatch log

### 2026-10-20 — `building`, developer dispatched

## Developer, 2026-10-20 — built, red then green on nebius-gpu

- **#94: no earlier gate.** `git log --grep='#94' -- cpp/src` and the `CUDF_VERSION_*` grep were
  empty; `kMergeM2CountType` was aggregate-arms' bare `INT32`. `merge_m2_count_type` and
  `kMergeM2CountType` now live in `plan_executor_internal.h`, from `<cudf/version_config.hpp>`;
  #94 closes here. No local cuDF 26.02 env (`~/data/miniforge3/envs` holds only
  `rapids-cuda-12.2`), so the `INT64` branch is compiled by CI's 26.02 leg alone.
- **Plan drift.** The registry's dispersion row carried `216` alone, not `199 216`.
  `AnEmptyStateMergedWithARealOneIsTheRealOne` was not added: its input is hand-built
  `(0, 0.0, 0.0)`, so it tests cuDF's arithmetic and no change in our code can turn it red;
  `a_group_with_values_on_one_lane_and_nulls_on_another_merges_as_the_cpu` covers the property
  through both engines' own inits. So the builder gtests are +7, not +8, and `welford_cases.rs`
  holds 9 cases, not the plan's count. `empty_state_row_over` (identity file) takes a type per
  aggregator; `empty_state_row` delegates to it.
- **A harness gap the plan missed.** `same_within_welford` with every column approximate (a
  keyless finalize of one Welford output) projected zero exact columns and `sorted` panicked
  ("Sort requires at least one column"). Both engines had answered. `compare.rs` now skips the
  exact comparison when no exact column is left.
- **Red build** (working tree: tests in, Task 1 in, no NULL fix, no keyless route), rc 0, no
  warnings. `peacock_plan_tests AggregateBuilder.*` 11 pass, 6 fail: the two NULL cases (mean and
  m2 null_count 1, k 2's mean NaN-under-NULL) and the four `AKeyless*` (one finished column;
  `unsupported aggregate function: var`). `peacockdb_core_gpu_lib` over the five aggregate
  modules: 86 pass, 15 fail, every failure the device's: 4 flipped dimension pins, 2 identity
  loops (at `Stddev`), 2 schema pins, 7 of `welford_cases` (2 NULL, 5 keyless). Each cpu half
  answered, the decimal Welford included.
- **Guards, shown red by mutation.** `ANaNAndANegativeZeroAreValuesNotNulls`,
  `a_nan_and_a_negative_zero_are_values_on_both` and
  `a_group_all_null_on_every_lane_finalizes_to_null_on_both` pass with and without the fix. A
  mutant build (`zero_for_null` also `replace_nans` to 0.0; the init's `COUNT` with
  `null_policy::INCLUDE`) turned all three red: NaN → `device: Some(0.0) Some(0.0)`, the all-NULL
  finalize on the inflated count. Mutant reversed on the VM, md5 equal to local, rebuilt.
- **Green build**, rc 0, no warnings. `AggregateBuilder.*` 17/17. Corpus write
  (`PCK_WRITE_GPU_RESULT=1 test_gpu_corpus global_stddev empty_dispersion`) 10/10;
  `gpu-result.txt` +5 sections each (tpch, pbench), nothing else moved; rsynced home before the
  next sync. L40S list (`gpu-host-test.sh run`) ALL rc=0: `peacock_cpu_tests` 26,
  `peacock_gpu_tests` 2, `peacock_join_session_tests` 54, `peacock_plan_tests` 115,
  `peacockdb_core_gpu_lib gpu_tests::` 738, `test_gpu_corpus` 904 (every enabled cell,
  shuffle-, rollup-, global-stddev and empty-dispersion at all five modes),
  `test_node_timing` 1, `peacock_gpu_benchmarks` 8 (`bench_` skipped). `ffi_tests::` and
  `the_payload_golden` from the staged lib binary: 6/6. VM soft-stopped, rc 0. Env path
  `~/data/miniforge3/envs/rapids-cuda-12.2/lib` (both it and `~/miniforge3/...` exist). Disk 41 GB
  free throughout; nothing removed.
- **Local.** `ctest -L cpu` 1/1 (`peacock_cpu_tests` 26); rust-only `--lib` 843 + 2 ignored,
  `test_cpu_corpus` 2220, `test_corpus_goldens` 28, `test_cost_model` 3, `test_ci_coverage` 11,
  `test_module_layout` 18, `cargo test -p cost-report` 41; every python step of the cost-report
  job green (venv `/tmp/aa-pyenv`), shellcheck clean; report generated;
  `--cost-diff --base ENS-aggregate-arms` (502839de): 932 compared, 0 changed, 0 regressions.
  `recipe-payloads.txt` unchanged. `test_ffi` (peacockdb-ffi) not run: no local cudf target dir
  (5.8 GB free on this disk; one is ~9 GB).
- **Deferred** (chain K's note): sf40 binaries, `--run-benchmarks`, Nsight, H200 timing; the
  26.02 compile is CI's leg.

### 2026-10-20 — green on both engines; PR opened; `reviewing`

Coordinator applied the developer's wiki list: `build-test.md` counts (5487 / 4870 / 219; cpu
3096; gpu 1654; corpus 2219 / 903; 902 cells, the 903rd case), harness and gtest prose, the GPU
host line; `architecture.md` (the keyless Welford through the builder, the NULL-free state,
`kMergeM2CountType` in "Every cast is explicit", the `CudfAggregate` cell, the mode and `ddof`
rows); #216 and #94 archived, the empty Welford section dropped, `tickets.md` 92 open
(corpus-coverage 17); #261 and #260 rewritten. Cost: no change vs base. Not run locally:
`test_ffi` (no disk for a cudf target dir) and the 26.02 compile (CI's leg). Review round 1
dispatched.
- PR #184 → ENS-aggregate-arms.

## Developer, 2026-10-20 — review round 1 fixes (local only, no GPU)

- **Comments.** `zero_rows_of` says "the same path a batch takes"; the zero-row gtest says a
  Partial's done call is handed the batch and a Merge's refuses; `TheLinkedCudfChoosesIt`
  says the `INT64` branch is built and unit-tested by CI's `cudf 26.02` leg (a 25.10a image, #129).
- **NaN guards lose their -0.0 halves**, which could not go red: `==` treats -0.0 and 0.0
  as equal, and nothing special-cases zero. Renamed `AggregateBuilder.ANaNIsAValueNotANull`
  and `welford_cases::a_nan_is_a_value_on_both`. The old names in `-impl.md` and above are
  history.
- **`tests::compare::a_keyless_lone_approximate_column_is_still_compared`**, `#[should_panic]`:
  keyless, `approximate=[0]`, cpu sd 1.0 against device sd 2.0. Red under a mutant whose
  no-exact-column branch returns before the approximate loop ("test did not panic as
  expected"). Green restored.
- **`keyless_welford_partial` is `pub(crate)`**, and `aggregate_schema_cases.rs` calls it.
  That file's `divergences_on_device` import had been unused since 0e5f4063, which removed its
  last call. It warned in the `gpu` lib build, so it is dropped.
- **Proven locally.** `cmake --build cpp/build`: rc 0, no warnings. `ctest -L cpu` 1/1
  (`peacock_cpu_tests` 26). `cargo-cudf.sh` (rapids-cuda-12.2): `--features gpu --lib
  --no-run` and `--test test_gpu_corpus --features gpu --no-run` both rc 0 with no warnings,
  after a forced recompile. `gpu_tests::` lists 738. Rust-only `--lib`: 844 passed and 2
  ignored. `tests::compare`: 12/12.
- **`build-test.md` counts that move:** Harness helpers 21 → 22, cpu `--lib` 845 → 846, cpu
  block 3096 → 3097, Rust 4870 → 4871, grand total 5487 → 5488. The renames move nothing.

### 2026-10-20 — review round 1: 0 blocking, 1 important, 7 nits; `completing`

Important: the `cudf 26.02` leg had not run at review time; it is the only build of the `INT64`
branch. It ran green on c50fce53's PR CI, and the dispatched run before `done` covers it again;
the wording "compiled by" became "built and unit-tested by CI's `cudf 26.02` leg (a 25.10a image,
#129)" in archived #94 and the gtest comment. Nits taken: two stale comments, the -0.0 halves
dropped, a `compare.rs` unit test for the no-exact-column branch (shown red), the shared helper,
`architecture.md`'s Welford prose (coordinator). Nit dropped: `aggregate.cpp:247-249` copies a
merged moment twice when it has NULLs (a merged count of 0, rare). Counts: harness helpers 22,
`--lib` 846, total 5488. Completeness pass dispatched.

### 2026-10-20 — completeness pass closed; `completeness approved`

Reviewer: 1 important — `ddof` still described as read (`architecture.md`'s `CudfAggregate` row,
the fbs comment, a wire test comment); fixed, comment-only. Analyst: 1 important — no ticket for
the 25.02 `MERGE_M2` INT32 count wrapping past 2^31, now reachable from a keyless `stddev`; filed
as #289 in corpus-coverage (93 open; filed as #287, renumbered because master had already used 287), and archived #94 points at it. No
`architecture.md` sentence left false. Signoff written. GPU host hard-stopped; dispatched CI follows.

Dispatched CI: run 39376356448, dispatched 2026-10-20T05:42:47Z on 1defb71d.

### 2026-10-20 — `done`

Dispatched run 39376356448 green on 1defb71d, `GPU Tests (nebius-gpu)` and the `cudf 26.02` leg
included; PR #184's CI green on the same head. GPU host hard-stopped. Chain K has no task left
that can progress.
