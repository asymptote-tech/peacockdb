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
- PR #182 → ENS-grouping-id.

### 2026-10-20 — review round 1: 0 blocking, 3 important, 11 nits

1. Important: `gpu_backend/accumulate.rs:176-216` — `impl GpuAccumulator { aggregate_init }`
   inserted between `addressed`'s doc and `addressed`. Developer.
2. Important: `wire/gpu_tests/aggregate_init.rs:56-70` — the empty-lane walk cannot tell
   `(0, NULL, NULL)` from NULLs or no row. Developer: assert the done call's one row, and walk
   `count(*)` over a `[[]]` scan where the identity row is the answer.
3. Important: the 7 cost regressions are outside the board note's pre-acceptance. Coordinator's
   decision: accepted — each is an identity row the spec requires (q32 and q11 at tp4 +51, q22
   tp4-single +52, all `cuda_aggregate_bytes`), and master's gate passes them (none over 10%).
   Named here and in PR #182 for the human.
Nits, coordinator (markdown): `build-test.md` stray column on the walk row; `architecture.md`'s
exec-model differences recounted, the `[[]]` sentence (nothing reads `[]` as the whole file: the
validator refuses it), batch sizing names the limit and the init; "now"/"no longer" in #214 and
df-upgrade. Nits, developer: zero-row handles in the walk, a `mirrors` refusal case,
`aggregate.cpp:184` no-input gated on Partial, stale comments (`cpu_backend/accumulate.rs:1-2`,
`gpu_backend/accumulate.rs:4`, `gpu_backend/gpu_tests/mod.rs:222-224`, `gpu_tests/exec.rs:1-2`,
`end_to_end.rs:570`, `nodes.rs:569`, `memory_estimation.rs:214-216`), the keyless shortcut recipe
pin, two over-long lines. For the human: `empty_state` is production code only tests call
(`#[allow(dead_code)]`), as the spec placed it; coding-style would make it `#[cfg(test)]`. #214
names no wrong row now and may be stale. nebius-gpu started 00:01Z; developer resumed.

### 2026-10-20 — developer, review round 1 addressed (on 74a24ba4)

- **Item 1:** `GpuAccumulator::aggregate_init` moved into the first `impl GpuAccumulator`, after
  `aggregate`; `addressed` has its doc back.
- **Item 2:** the walk records `done_rows` (each done chain's answered rows) in place of a count
  of input-less calls. The empty-lane walk asserts `[1]`; a new walk,
  `a_keyless_count_over_a_scan_of_nothing_answers_its_identity_row` (`select count(*) … where
  n_nationkey < 0`, one lane `[[]]`), joins `PROVEN`'s query list. Shown red on the device twice,
  each a deliberately broken `aggregate.cpp` shipped as a whole file over ssh, then restored:
  break B (a done call answering the input's zero-row table) failed both walks (`done_rows` `[0]`
  against `[1]`; "the walk exported no rows"); break A (a count over nothing answering NULL) left
  the empty-lane walk green — the merge's sum of 25 and NULL is 25 — and failed the
  scan-of-nothing walk ("first difference at sorted row 0").
- **Item 5:** the walk skips a zero-row handle at the init, as both executors do; scan handles now
  record their rows too (`Session::scan` answers rows beside the handle).
- **Item 6:** `an_init_whose_recipe_lacks_its_done_calls_is_refused` (device). Red with the
  `mirrors` refusal disabled (red build 2), green restored.
- **Item 7:** the no-input path is gated on `AggregateMode_Partial`.
  `AggregateNoInput.AMergeHandedNothingStillRefuses` red without the gate (red build 1), green with.
- **Item 9:** the seven stale comments reworded. **Item 11:**
  `a_keyless_init_that_finalizes_itself_calls_the_pair_again_at_done` pins the keyless four-call
  recipe (red with the done calls dropped from `attach.rs`, restored); `backend.rs`'s doc and the
  corpus batch-12 comment rewrapped. `empty_state`'s `#[allow(dead_code)]` left as placed.

**Runs.** Local: `--lib` 836 passed, 2 ignored; `test_cpu_corpus` 2190; `test_corpus_goldens` 28;
`test_cost_model` 3; `test_ci_coverage` 11; `test_module_layout` 18; `test_golden_format` 43;
`cost-report` 41; build warnings 0 (rust-only and gpu lib); `ctest -L cpu` 1/1. Cost diff
unchanged: 7 regressions, 0 over 10%. Device: one sync (VM gpu-result matched local), three
builds — red 1, red 2, green — each source swapped by a whole file over ssh and touched. Green
targeted: `AggregateNoInput.*` 4/4, the init and walk set 19/19.
Green L40S list (`gpu-host-test.sh run`): `ALL rc=0` — peacock_plan_tests 95, peacock_join_session_tests
53, peacock_gpu_tests 2, peacock_cpu_tests 12, peacockdb_core_gpu_lib `gpu_tests::` 725,
test_gpu_corpus 889, peacock_gpu_benchmarks 8 (3 `bench_` skipped), test_node_timing 1. No
`gpu-result.txt` recorded this round (no cell changed). VM soft-stopped 00:34Z.

### 2026-10-20 — round 1 closed; `completing`

Developer fixed both code importants and every developer nit, each new test shown red on a broken
build; L40S list green (plan 95, gpu lib 725, corpus 889). Coordinator: `build-test.md` counts
(5387 / 4805 / 184; cpu 3059, `--lib` 838; gpu 1626; recipes 23; walk 28; executors 42;
plan-executor 95) and the walk and executor prose. Completeness pass dispatched.

### 2026-10-20 — completeness pass closed; `completeness approved`

Reviewer: 3 important — `limit`'s doc reassigned to `aggregate_init` (round 1's fix moved the
defect; the coordinator moved the doc back above `limit`, comment-only), `end_to_end.rs:653`'s
stale #282 refusal (reworded), #214 (left to the human). Analyst: the same doc defect,
`plan_goldens.rs:962` (no pbench refusal is left uncited; sentence cut), and four
`architecture.md` sentences, fixed: the row-group mapping's one owner (`[[]]` is the loader's),
node display (verbatim what the loader stored), memory accounting (the done call priced at zero,
named as the exception), and the zero-row rule (a keyless merge is the one node that does not
meet it, unreachable). Cost-report job green on HEAD; nothing pins a moved count. Signoff written.
GPU host hard-stopped; dispatched CI follows.

Dispatched CI: run 39350996530, dispatched 2026-10-20T01:00:35Z on 6c602488.

### 2026-10-20 — `done`

Dispatched run 39350996530 green on 6c602488, `GPU Tests (nebius-gpu)` included; PR #182's CI
green on the same head. GPU host hard-stopped. Next: aggregate-arms.
