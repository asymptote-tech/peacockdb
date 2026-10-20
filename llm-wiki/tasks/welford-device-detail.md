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
