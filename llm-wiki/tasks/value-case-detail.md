# value-case — run record

## Run log

- 2026-10-21: branched `ENS-value-case` off `ENS-welford-device` (chain rebased onto master
  97f91a02 the same day, docs only, states kept). Developer dispatched. verda: no address
  (`list_verda_instances.sh` has no credentials here), so CPU tests run locally. GPU: nebius-gpu,
  tree `~/peacockdb-K`.

## Developer, round 1 (2026-10-21)

**What changed.** `build_column_case` builds the comparand once (`std::optional<EvaluatedColumn>`)
and a lambda `condition` turns each WHEN into `binary_operation(comparand, when, EQUAL, BOOL8)`,
the scalar overload for a `LiteralExpr` WHEN (`build_scalar`), the column one otherwise. The
search form goes through the same lambda unchanged. The fold is untouched.

**Subtle.**
- The gtest is two `TEST`s, not one: `CaseValueForm.TheFirstMatchWinsAndNullNeverMatches` (ELSE)
  and `CaseValueForm.WithoutElseAnUnmatchedRowIsNull`. So `peacock_plan_tests` is +2 and C++ 221.
  The comparand is itself a search CASE (`WHEN r_regionkey = 4 THEN NULL::Int32 ELSE r_regionkey`),
  so the value form is exercised over a made, nullable column, not a borrowed one.
- A new `make_int32_literal` helper: the comparand is Int32, and the plan wants Int32 WHENs.
- `testdata/report.html` was stale before this task: last regenerated at f30e1ca7, while five
  chain-K commits after it changed the registry. The regeneration here therefore moves far more
  than q39's row (TPC-H 35/39 → 43/44 and so on). That is the report catching up, not this task.
- `tasks.md`'s task-9 heading and the frozen spec link `#57` at `tickets/corpus-coverage.md#t57`,
  which no longer exists; the archive now holds `t57`. Both are the coordinator's to repoint.

**Local CPU evidence.**
- `cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2`: 844 passed,
  2 ignored, 0 failed.
- `--test test_cpu_corpus -- device_order_tpcds_q39 duckdb_gpu_tpcds_q39
  every_enabled_device_cell`: `device_order_tpcds_q39` passes; the five `duckdb_gpu_tpcds_q39_*`
  and the coverage guard fail on "holds no section for this cell", as expected before the device
  records q39.
- `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh test -p peacockdb-core
  --lib --features gpu --no-run`: rc 0, no warnings.

**GPU.** Red patch `/tmp/value-case-red.patch` (`git diff -R -- cpp/src/expr.cpp`). VM disk
before building: 24 GB free of 96. Card idle. cuDF lib: `~/data/miniforge3/envs/rapids-cuda-12.2/lib`.

**GPU run, red then green (nebius-gpu, `~/peacockdb-K`).**
- Red build (patch applied, rc 0): `peacock_plan_tests --gtest_filter='CaseValueForm.*'` ran 2,
  failed 2 on `[in CudfProject] value-form CASE not supported in column path`;
  `peacockdb_core_gpu_lib --test-threads=1 gpu_tests::exec_cases::a_value_case` ran 2, failed 2 on
  the same refusal.
- Green build (patch reversed, rc 0, no warnings): the same four pass; `--gtest_filter=
  'Case*:Literals.*:PlanExecutor.ProjectSqrt*'` 13/13; `PCK_WRITE_GPU_RESULT=1 test_gpu_corpus
  --test-threads=1 tpcds_q39` 5/5 (904 filtered, so 909 in the binary).
- The rsync home moved only `tpcds.sf1/gpu-result.txt`: +1240 lines, five `== q39 mode=…`
  sections, nothing removed.
- `gpu-host-test.sh run` (the L40S list): `ALL rc=0`. peacock_cpu_tests 26, peacock_gpu_tests 2,
  peacock_join_session_tests 54, peacock_plan_tests 117, peacock_gpu_benchmarks 8 (3 `bench_`
  skipped), peacockdb_core_gpu_lib 739, test_gpu_corpus 909, test_node_timing 1.
- Then `git clang-format`'s diff applied by `patch` (it refuses unstaged files, and staging is
  the coordinator's). Whitespace only; re-synced, rebuilt (rc 0, no warnings), and re-ran
  peacock_plan_tests 117/117, `gpu_tests::exec_cases::` 59/59, q39 5/5. Soft stop, rc 0.
- Deferred, per the dispatch: sf40, benchmarks.

**Local CPU, after the rsync.** `test_cpu_corpus --test-threads=2`: 2227 passed, 0 failed.
`test_module_layout` 18, `test_ci_coverage` 11, `test_corpus_goldens` 28, `test_cost_model` 3,
`cargo test -p cost-report` 41: all pass. No plan golden moved; no cpu golden moved.
- 2026-10-21: developer green (see above); committed, PR #185 → ENS-welford-device; state reviewing. Reviewer round 1 dispatched.

## Developer, review round 1 (2026-10-21)

- **report.html regenerated after the q39 sections landed** (`python3 scripts/ticket_widget.py`).
  The first regeneration ran before the rsync brought `gpu-result.txt` home, so its anchors were
  stale. Against HEAD, 50 rows change, each classified by a script over the old and new pages:
  q39's gpu ✔ gains `#L11569`; 46 later TPC-DS rows shift their anchors by exactly +1240, q40
  through q99; and #60's new label "(round off by one ulp)" appears in three rows: the by-ticket
  row, q2 and q78. Nothing else moves. Lesson: run the widget after the rsync.
- **`make_int32_literal` moved** beside `make_int64_literal`, body unchanged. Proof is the
  compile: `cmake --build cpp/build --target peacock_plan_tests` rc 0, no warnings, and
  `--gtest_list_tests` lists 117 tests, both `CaseValueForm` cases among them. No GPU rerun, since
  a function moved within one translation unit changes no behaviour.
- 2026-10-21: review round 1: 0 blocking, 1 important (report.html stale vs q39's gpu-result lines; regenerated), 3 nits fixed (helper moved; #289 order and #60 label by the coordinator). round-tolerance-impl.md:101 calls #60's label malformed — fixed here, so that note is stale. State completing; completeness reviewer + analyst dispatched.
