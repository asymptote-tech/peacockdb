# round-tolerance — run record

## Run log

- 2026-10-21: branched `ENS-round-tolerance` off `ENS-decimal-literal`. Developer dispatched.
  verda: no address, CPU tests local. GPU: nebius-gpu, tree `~/peacockdb-K`. #60's header label
  was already fixed in value-case to "(round off by one ulp)", so the plan's Step 7 note on a
  malformed label is stale.
- 2026-10-21, developer: one build on nebius-gpu `~/peacockdb-K` (cuDF 25.02,
  `~/data/miniforge3/envs/rapids-cuda-12.2/lib`); `df -h /` before it: 24 GB free of 96, card idle.
  Clean build, no warnings. **q78 on at all five device modes; q2 stays off under the Restriction**
  (below). #60 archived as won't fix (Stale); #56 stays open with the measurement.

## Red, then green (cpu)

- Red, after flipping both lines and rows and before recording:
  `cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2 tpcds_q78 tpcds_q2_ every_enabled_device_cell_has_its_gpu_result_section_and_no_other registry duckdb_tpcds_q2 device_order_tpcds_q2`
  → 33 passed, 11 failed: the ten `duckdb_gpu_tpcds_{q78,q2}_*` ("holds no section for this
  cell") and `every_enabled_device_cell_has_its_gpu_result_section_and_no_other`. Already green
  there: `duckdb_tpcds_q78` and `duckdb_tpcds_q2` under `duckdb_approx` (the looser decimal
  check the spec chose: `duckdb_approx` accepts one last-place unit on q78's decimal columns,
  which the DataFusion check and the golden still hold exactly),
  `device_order_tpcds_q78`/`_q2`, the registry checks.

## The device recording

`PCK_WRITE_GPU_RESULT=1 cpp/install/rust-tests/test_gpu_corpus --test-threads=1 tpcds_q78_ tpcds_q2_`
→ 10 passed, 914 filtered out. Each case asserts its answer against `mini.result.txt` under
`golden_approx_std` in memory before recording, so all ten device answers are within 1e-11.
`rsync` home; the five recordings of each query are byte-identical.

## Gap measurement (Review Focus 1 and 3)

Throwaway script: each q78 `gpu-result.txt` section against `mini.result.txt`, rows paired on
their cells rounded to 9 significant digits, then every cell compared as text.

- **q78**, each of the five modes: 100 rows, 1000 cells, **5 differ, all in `ratio`** (Float64,
  `round(…, 2)`). Each exactly one ulp:

  | (year, item, customer) | golden | device | relative |
  |---|---|---|--:|
  | 2000, 2971, 98888 | 2.78 | 2.7800000000000002 | 1.60e-16 |
  | 2000, 4214, 25547 | 1.36 | 1.3599999999999999 | 1.63e-16 |
  | 2000, 6487, 79301 | 2.53 | 2.5300000000000002 | **1.76e-16** (largest) |
  | 2000, 7261, 99191 | 1.64 | 1.6400000000000001 | 1.35e-16 |
  | 2000, 7751, 77977 | 1.91 | 1.9100000000000001 | 1.16e-16 |

  Every other cell, the integer columns (`ss_sold_year`, keys, quantities) and the decimal ones
  (wholesale costs, sales prices) included, matches as text. No gap above 1e-14.
- **q2**: not measurable cell by cell. Its device recording is a **fingerprint**
  (`rows=2513`, per-column sum/min/max, hash), not rows. Fingerprint against the golden's and
  DuckDB's rows: same row count, each column's sum, min and max within 2.5e-14 relative at every
  mode (Python summation order). The device's in-memory `golden_approx_std` pass bounds every cell
  by 1e-11.

## Why q2 stays off (the Restriction)

With the recording home, the same targeted run had 36 passed and 5 failed, the five `duckdb_gpu_tpcds_q2_*`:
`tpcds/q2 (the device at <mode>): duckdb_approx: the section is fingerprinted (over the cap) — declare duckdb_fingerprint`.

Root cause: the cpu's q2 section is 261,768 bytes, under `RESULT_GOLDEN_MAX_BYTES` (262,144) by
376. The device's rendering crossed it, since `rendered_or_fingerprint` wrote a fingerprint; the
likely cause is q78's: a one-ulp cell prints 18 digits and widens its column on all 2513 rows
(inferred, the fingerprint cannot show which cells). `compare_sections`
(`test_support/duckdb_oracle.rs`) refuses a fingerprint under `duckdb_approx`, and
`duckdb_fingerprint` refuses the cpu's side, where both sections hold rows. One `duckdb_oracle`
per line cannot serve both sides, so no line-only change makes q2 green. A harness change is
outside the frozen scope. So q2's line and row are back as they were, its five sections are
removed, its row tags `56` (no longer `60`), and #56 carries the measurement and the proposed
harness fix: `duckdb_approx` falls back to `compare_over_cap` within `DUCKDB_FLOAT_TOLERANCE`
when only our side is fingerprinted.

## Final runs (the tree as handed over: q78 on, q2 off)

- cpu, local: `cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2`
  → 2239 passed, 0 failed. `--lib --test test_corpus_goldens --test test_cost_model` → 844 passed
  and 2 ignored, 28, 3.
- device: re-synced (the recording build still had q2 on), rebuilt (no warnings, 23 GB free), then
  the full L40S list, `gpu-host-test.sh run ~/peacockdb-K` → `ALL rc=0`: `test_gpu_corpus` 919
  passed (`gpu_tpcds_q78_*` among them), `peacockdb_core_gpu_lib` 739, `peacock_plan_tests` 118,
  `peacock_join_session_tests` 54, `peacock_cpu_tests` 26, `peacock_gpu_tests` 2,
  `peacock_gpu_benchmarks` 8 (`--skip bench_`), `test_node_timing` 1. Soft stop, rc 0.
- Counts moved in `build-test.md`: `test_cpu_corpus` +6 (five `duckdb_gpu_tpcds_q78_*`, one
  `device_order_tpcds_q78`), `test_gpu_corpus` +5; one line `duckdb_exact` → `duckdb_approx`.
- Review round 1, after the coordinator's wording edits (`expr.cpp` comment-only, #56 retitled
  and moved under Testing, #60's rate, `corpus_cases.inc` comments): `ticket_widget.py` re-run, report
  diff against HEAD is #56's label and title only (2 lines). `test_ci_coverage` 11,
  `test_module_layout` 18, `cargo test -p cost-report` 41 (one existing `sha_links` dead-code
  warning, file not on this branch), and in `test_cpu_corpus` the 10 non-cell checks, the registry
  check, `duckdb_gpu_tpcds_q78_*`, `duckdb_tpcds_q78`/`q2`, `device_order_tpcds_q78`: all pass. The
  index (92) and corpus-coverage's Contents match the anchors in file order. No GPU (comment-only C++).

## Drift found

- `ORACLE_HOW`'s `golden_approx_std` said "on float columns". `sorted_str_approx` applies the
  tolerance to every cell that parses as a number. Now "numeric columns".
- The plan names an `ORACLE_HOW` `duckdb_approx` sentence; `ORACLE_HOW` has no DuckDB entries
  (the widget shows no DuckDB oracle), so there was nothing to edit.

## Deferred

sf40, benchmarks, Nsight: out (L40S list only).
- 2026-10-21: developer green for q78; q2 stays off under #56 (spec Restriction: device rendering crosses the 256 KiB cap → fingerprint; duckdb_approx refuses it; fix in compare_sections is outside Scope, recorded in #56). Coordinator accepts the partial outcome the spec provides for. Committed; PR opened; state reviewing.
- 2026-10-21: review round 1: 0 blocking, 3 important (round comment over 4 lines; #56 header named a solved shape; #56 too long), 6 nits. Coordinator applied all but the board heading's 'closes #56' (human's prose; named in the signoff). Developer regenerated report.html and ran the cheap tiers green. State completing.

## Completeness pass, "what is missing" (analyst, 2026-10-21)

- q2's "inferred" root cause is now re-emulated, not inferred. A throwaway script applies cuDF's
  `ip + round(fp·100)/100` to every round cell of `mini.result.txt` (N recovered from the golden).
  On q78 it reproduces the device exactly: the same five `ratio` cells, largest 1.76e-16, and a
  rendered section of 20,071 bytes (recorded: 20,072 with its newline). On q2: 301 of 17,591 round
  cells move one ulp (r4 147, the seventh column 56, r1 49, r2 49; r3, r5 and r6 none). Largest gap
  1.95e-16. The padded rendering grows from 261,767 to 367,481 bytes, over the 262,144 cap.
- So q2 is red *because of* `round`, through the cap and `compare_sections`' refusal. Neither
  Restriction clause names that case: the outcome is a Scope gap. #56's proposed fix is sound:
  `compare_over_cap` already takes `(fingerprint, rows)`.
- #56's original shape keeps device coverage across merges: q43 and q59 both
  `sum(CASE WHEN d_day_name='Sunday' …)`, on at every device mode.
- 2026-10-21: completeness: reviewer 0/0; analyst 1 important — q2's miss is a Scope gap, not the Restriction (q2 red because of round, via the harness). Coordinator accepts the partial outcome; widening Scope to land the duckdb_oracle.rs fix is the human's call. Signoff written; state completeness approved.
