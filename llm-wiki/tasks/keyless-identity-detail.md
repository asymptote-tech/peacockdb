# keyless-identity — working record

## Standing facts

- Sixth of chain K (folded in from chain L), branch `ENS-keyless-identity` off `ENS-grouping-id`;
  its PR targets `ENS-grouping-id`. Base master `ca4dedeb`; empty-sorts (`Driver::answer`) and
  grouping-id are below it on the chain.
- Hosts, per chain K's board note: device work on nebius-gpu, tree `~/peacockdb-K`
  (`gpu-host-sync.sh --dest peacockdb-K`), the L40S list only; cpu runs local (verda not
  reachable). The final device run is the dispatched `GPU Tests (nebius-gpu)` CI job.
- Pull a recorded `gpu-result.txt` home through `gpu-host.sh rsh` before the next sync.
- grouping-id made the walk harness (`wire/gpu_tests/mod.rs`, `per_lane`) skip the at-done
  collapse on a lane with no batch, "as every device accumulator does". This task's
  `AtDoneIfNothingOut` makes that reason false: revisit it.
- Adding a query file moves pinned counts: `testdata/test_duckdb_result.py` pins tpch at 42 and
  pbench by a floor. Run the cost-report job's Python suites locally (grouping-id's lesson).

## Dispatch log

### 2026-10-19 — `building`, developer dispatched

### 2026-10-19 — developer, round 1: built, verified, device green

**What landed, by plan task.** All nine plan tasks but the wiki (the coordinator's). Deviations
from the plan, each because the code had moved:

- Task 3's interim Step 7 skipped: nothing is committed between tasks, so the device exec arm
  went straight to Task 4's accumulator. `Recipe::calls_under` is used by
  `GpuAccumulator::aggregate_init` and the executor gpu_tests' `Session::exec`/`chaining_exec`.
- `Recipe`'s `Display` (empty-sorts/joins era) elided a seq repeated by consecutive calls, for the
  join session. An init's done call reusing its seq then rendered as a bare `execute_node`. The
  elision is now for `FbKind::Join` alone; every existing golden line renders as before. The
  payload loop prints each seq once through a set.
- `source()`'s fix is in `loader()` (limits split them). `batching_for_source` is the per-source
  counter both planning passes address sources by, so the empty scan still calls it; skipping it
  made tp4-sized refuse ("the sizing pass reached 1 sources and the planning pass 0").
- grouping-id's `per_lane` empty-lane skip stays, and is right: the init is dispatched by node
  kind to `wire/gpu_tests/aggregate_init.rs` before the category match, so `per_lane` serves the
  other accumulators only. Its comment now says "every device accumulator but an aggregate's init".
- The identity harness cases went to a new `tests/gpu_tests/aggregate_identity_cases.rs`
  (`aggregate_cases.rs` would have passed 1000 lines); `given` there became `pub(crate)`.
- `wire/gpu_tests/aggregate_init.rs` items are `pub(crate)`: `test_module_layout` refuses
  `pub(super)` (`nothing_is_pub_super`), which the plan's text used.
- Removed `AsArray` from `aggregate_dimension_cases.rs`: unused on the base already (a gpu-build
  warning grouping-id left), in a file this task edits.

**Spec drift, recorded rather than built around.** The spec and plan say `tiny` "is under the
small-table size, so tp4-rowgroup and tp4-sized plan it one lane". pbench plans with the rule off
(`small_table_bytes_for`), so all three tp4 modes deal `tiny` as `[[[0]],[],[],[]]`: three empty
lanes, not one lane. The corpus comment says what the goldens show.

**Harness traps met.** Under `UPDATE_CANONICAL=1`, `planner::tests::plan_goldens` runs readers
beside writers: `the_payload_golden_covers_every_kind…` and `the_registry_matches_the_goldens…`
read a file another test in the same run rewrites, and fail in that run only. A plain run after is
the verdict. Same for `test_cpu_corpus`'s `a_hyphenated_query_resolves_its_authority…` during the
corpus regeneration (it reads `mini.result.txt` before the run writes the new sections).

**Local runs (final tree).**
- rust-only `--lib`: 835 passed, 2 ignored (837 = 820 + 17). `test_cpu_corpus` 2190 passed.
  `test_corpus_goldens` 28, `test_cost_model` 3. Build warnings 0 (rust-only and gpu lib).
- C++ `scripts/build.sh … --build`, `ctest -L cpu`: 1/1, `peacock_cpu_tests` 12 passed.
- cost-report job, step by step: `testdata/test_duckdb_cost.py` 41, `test_duckdb_result.py` 20
  (tpch pinned at 42, pbench floor > 50: now 66 sections); exec-model 213 (10 files, the three
  sf1 files excluded as the job does); calibration 12 (matplotlib in a scratch venv, as the job
  self-heals it); gpu-host 104 (6 files); shellcheck 0.11.0 clean (from the `shellcheck-py`
  wheel, none installed here); `cargo test -p cost-report` 41; `test_ci_coverage` 11;
  `test_module_layout` 18; report generation ok.
- DuckDB 1.5.4 (scratch venv): `duckdb_result.py --dataset pbench` added exactly the six
  `empty-*` sections, nothing else moved.

**Cost diff** (`cost-report --cost-diff --base ENS-grouping-id`): 875 compared, 156 changed, 149
improvements, 7 regressions, 0 over 10%, exit 0. Every regression is `cuda_aggregate_bytes` alone:
the empty lanes' inits now emitting identity rows, where the cpu merge used to invent one.
tpcds q32 tp4-rowgroup, tp4-single, tp4-sized +51 bytes each; tpch q11 tp4-rowgroup, tp4-single,
tp4-sized +51 each; tpch q22 tp4-single +52. The improvements are the zero-row batches the inits
no longer emit.

**Device session (nebius-gpu, `~/peacockdb-K`).** `gpu-result.txt` on the VM matched local before
the sync (nothing unpulled). Red build: `cpp/src` and the cpu merge clause reverted by a patch
over ssh stdin. Red results: `peacock_plan_tests --gtest_filter='AggregateNoInput.*'` 1 passed, 2
failed (`take_input: not enough input handles for node`); gpu lib filtered 8 passed, 9 failed —
the five harness identity cases, the device init case and the empty-lane walk on the same
message; both merge cases on the restored clause; every case with no done call passed.
The VM went down during the green build (CI's run ended and soft-stopped it): `start`, a second
sync (local equalled the green tree), then `touch` on the patched files, since the re-sync
restored mtimes older than the red binaries (build-test.md's antipattern). Green: the red set 3/3
and 17/17; candidates with `PCK_WRITE_GPU_RESULT=1` 75 passed (54 new cells, 21 already on);
then the whole L40S list through `gpu-host-test.sh run`: `ALL rc=0` — peacock_plan_tests 94,
peacock_join_session_tests 53, peacock_gpu_tests 2, peacock_cpu_tests 12, peacockdb_core_gpu_lib
`gpu_tests::` 723, test_gpu_corpus 889, peacock_gpu_benchmarks 8 (3 `bench_` skipped),
test_node_timing 1. `gpu-result.txt` pulled home through `gpu-host.sh rsh`: +38 pbench, +12
tpcds, +4 tpch sections, no existing line moved, `cudf=25.02`. VM soft-stopped. Since the device
run the tree changed only in comments, registry tickets and rustfmt layout.

**Candidate verdicts: all 54 on.** pbench `empty-all-null-aggregates`, `empty-count-aggregates`,
`empty-grouped-count`, `empty-pruned-count`, `empty-table-count`, `cross-empty-build`,
`outer-on-true-empty` at all five modes; `scalar-subquery-cross` at the tp4 modes; tpcds q96, q88,
q90, q32 at the tp4 modes; tpch q11 at the tp4 modes, q22 at tp4-single.
`empty-dispersion-aggregates` stays off on the device on #216. No ticket filed. `199` and `282`
struck from every registry row; the dispersion row keeps `216`.

**Deferred, per chain K's note:** the sf40 binaries (`peacock_tpch_tests`, `peacock_tpchv_tests`),
`--run-benchmarks`, Nsight captures, any H200 timing.

**Not changed, on purpose (plan's list):** the Python exec model still models the init as an
`Exec`; `architecture.md` already names the model's empty-case differences, and one more now
holds — its init answers per batch, a zero-row one included.

### 2026-10-19 — green on both engines; PR opened; `reviewing`

Coordinator applied the developer's wiki list: `build-test.md` counts and device-cell prose, two
new planner rows (a scan of nothing, recipe patterns); `architecture.md` (node set, row-group
mapping, scheduling rule and the exec-model difference, memory accounting, zero-row rule met,
`GpuAggregate`'s seqs, `CudfAggregate` with no input, the frozen surface, batch sizing's walk);
#199 and #282 archived under Done, the empty Source section dropped, `tickets.md` 97 open
(corpus-coverage 22), #214's sentence and df-upgrade's q27 note rewritten. Left open: #214 now
names no wrong row (a batch-count difference only) — whether it is stale is the human's call.
Spec drift recorded, not fixed (frozen): pbench plans `tiny` four ways at every tp4 mode, not one
lane at tp4-rowgroup and tp4-sized. Cost: 7 regressions, all `cuda_aggregate_bytes` from the
identity rows (q32 and q11 at the three tp4 modes +51, q22 tp4-single +52), none over 10%.
Review round 1 dispatched.
