# oracle-columns — detail

## Run log

- 2026-10-21: chain K rebased onto master d227a589 (documentation only: tasks 14–15 added, four
  reports removed); every branch kept `done`, no re-verification. Branch `ENS-oracle-columns`
  forked off `ENS-interval-days` 73f7586f.
- 2026-10-21 21:16Z: dispatched the developer, round 1. Verda not reachable (no credentials):
  CPU tests run locally. nebius-gpu started by the coordinator; sync with
  `gpu-host-sync.sh --dest peacockdb-K`.

## Developer, round 1

- **Done, per the plan's five tasks, uncommitted.** Files: `test_support/{mod.rs, duckdb_oracle.rs,
  duckdb_oracle/tests.rs, registry.rs, registry/tests.rs, corpus_golden.rs}`,
  `tests/test_cpu_corpus.rs`, `tests/common/corpus_cases.inc`, `cost-report/src/main.rs` (fixture
  only), `testdata/{cost-registry.csv, report.html, test_duckdb_result.py,
  goldens/tpcds.sf1/gpu-result.txt}`, `scripts/{ticket_widget.py, test_ticket_widget.py}`,
  `llm-wiki/{build-test.md, tickets.md, tickets/corpus-coverage.md, archive/archived-tickets.md}`.
- **Beyond the plan, small:** `compare_device_section` delegates a fingerprinted device section
  under `duckdb_fingerprint`/`duckdb_none` to `compare_sections` unchanged. Two extra oracle
  cases (a fingerprinted device section that differs fails; rows without the part compare as
  rows), two extra registry cases (a side with no section stays one word; the spelling check
  `stored_spelling_is_valid`, which `load_csv` runs on every row), and `test_split_gpu_…` in the
  widget. `stored_mismatch` says `(write it as <want>)` where the parts agree and the text does not.
- **q2 gained a `device_order_tpcds_q2` case** too (it is per line with a device cell), so the
  cpu corpus grew by 7, not 6: 2308 → 2315.
- **The `stored` column was filled by a throwaway script** (`/tmp/oc-scripts/fill_stored.py`,
  not committed) that inserts the field as text after `tickets`, so no other byte moved: diffed
  against HEAD with the field removed, only q2's row differs. 170 `result`, 24 `fingerprint`, 20
  `na`, 1 `result|result|fingerprint`.
- **q2's device recording**, nebius-gpu L40S, `~/peacockdb-K`, cuDF 25.02:
  `PCK_WRITE_GPU_RESULT=1 test_gpu_corpus --test-threads=1 tpcds_q2_` — 5 ran, 5 passed; the
  rsync brought only `tpcds.sf1/gpu-result.txt` home, five sections added, each
  `fingerprint: rows=2513`. Largest relative gap on a float triple (sum, min or max of cols 1–7),
  read with a throwaway script against DuckDB's rows and the cpu's: 2.48e-14 at every mode, both.
- **Widget test gotcha:** the gpu half's title is longer than 300 characters; read halves with a
  regex over `<span class="half" title=…>`, never a character window.
- **Links left pointing at #56's old anchor** in frozen or coordinator-owned files: the spec's
  line 5, `round-tolerance.md` line 6 and `tasks.md` lines 147 and 164. Fixed in #60's archived
  text and build-test.md.
- **Proving runs, round 1 (local CPU; verda unavailable).** `--lib` (rust-only) 893 passed, 2
  ignored; `test_cpu_corpus` 2315/2315, the five `duckdb_gpu_tpcds_q2_*` included;
  `test_corpus_goldens` 28/28; `cargo test -p cost-report` 43 + 32; Python
  `test_duckdb_cost.py` 41, `test_duckdb_result.py` 22, `test_ticket_widget.py` 10.
- **L40S list on the final tree** (second sync and build, after the recording was rsynced home):
  ALL rc=0 — `peacockdb_core_gpu_lib` 741, `test_gpu_corpus` 949 (q2 ok at all five modes),
  `peacock_gpu_benchmarks` 8 (3 `bench_` skipped), `test_node_timing` 1, `peacock_plan_tests`
  126, `peacock_join_session_tests` 56, `peacock_cpu_tests` 26, `peacock_gpu_tests` 2. Soft stop
  rc=0. Deferred as always: sf40 binaries, benchmarks, Nsight.
- 2026-10-21T21:47:45Z: developer round 1 green (CPU local, L40S list ALL rc=0, q2 at five modes). Committed, PR #190 → ENS-interval-days. Developer agent id ad97cfef0817e87fa (this run only). Reviewer round 1 dispatched.
