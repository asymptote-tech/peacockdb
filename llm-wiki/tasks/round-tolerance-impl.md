# round-tolerance implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** tpcds q78 and q2 run on the device, compared within 1e-11 relative. The engine keeps
`cudf::round`. #60 is archived as won't fix and #56 as proven.

**Architecture:** No engine change.
- **q78's and q2's lines change oracle:** `gpu_oracle` `golden_exact` → `golden_approx_std`, and
  `duckdb_oracle` `duckdb_exact` → `duckdb_approx`. Their device cells turn on.
- **A comment** at `round` in `cpp/src/expr.cpp` names the one-ulp gap.

**Tech stack:** the corpus, the registry, nebius-gpu.

**Spec:** [`round-tolerance.md`](round-tolerance.md). Task 11 of chain K, after decimal-literal.

## Global Constraints

- No engine change. Only q78's and q2's lines change oracle. The cpu's `data_fusion_exact`
  stays.
- If q2 is red for a reason other than `round`, #56 (q2 CASE in partial sum) stays open with the
  measurement, and q2's cells stay off under it.
- **A gap bigger than one ulp is not this task's.** A half-way value can round to a different
  last digit, which is 0.01 rather than an ulp, and 1e-11 would not accept it. If Step 5 finds
  one, the affected query's cells stay off, #60 stays open with the measurement, and the
  coordinator hears of it. q2 has never been compared cell by cell.
- GPU: nebius-gpu, `~/peacockdb-K` (`value-case-impl.md`, *Device cycle*: there is no fix to
  reverse, so one build).

## Review Focus

1. **A device cell that differs by more than an ulp.** 1e-11 would hide a real defect up to that
   size. The detail file records each differing cell's relative gap. A gap above 1e-14 is a
   finding, not a pass.
2. **The cpu's DuckDB check loosens too.** `duckdb_approx` judges the cpu's section as well. Its
   `duckdb_tpcds_q78` and `duckdb_tpcds_q2` cases must stay green, and the detail file says the
   cpu side lost nothing it had.
3. **`golden_approx_std` on non-float numerics.** It applies 1e-11 relative to every numeric
   cell, decimals and integers included. q78 and q2 have integer and decimal columns, and their
   device answers must match those exactly. The detail file confirms that from the comparison's
   output.

---

### Task 1: q78 and q2 on the device

**Files:**
- Modify: `cpp/src/expr.cpp` (the comment at `round`, ~717)
- Modify: `peacockdb-core/src/test_support/device_answer.rs` (`STDDEV_TOLERANCE`'s and
  `GoldenApproxStddev`'s docs) and `scripts/ticket_widget.py` (`ORACLE_HOW`'s `golden_approx_std`
  and `duckdb_approx` sentences). They now also serve `round`'s one-ulp gap.
- Modify: `peacockdb-core/tests/common/corpus_cases.inc` (`tpcds, 1, q78` and `tpcds, 1, q2`)
- Modify: `testdata/cost-registry.csv` (both rows)
- Regenerate: `testdata/goldens/tpcds.sf1/gpu-result.txt`, `testdata/report.html`
- Modify: `llm-wiki/tickets/corpus-coverage.md` → `archive/archived-tickets.md` (#60, #56),
  `tickets.md`, `build-test.md`

- [ ] **Step 1: The comment** at `round`:

  > `cudf::round(·, p, HALF_UP)` rounds in two steps (`modf`, then the fraction), DataFusion's
  > `(x·10^p).round()/10^p` in one, so for `p > 0` about one value in twenty at or above 1.0 is
  > one ulp apart (#60, won't fix: a bit-identical kernel costs three passes or a custom kernel).
  > The corpus compares those queries within 1e-11 relative (`golden_approx_std`,
  > `duckdb_approx`).

- [ ] **Step 2: The lines and rows.**
  - q78 and q2 become `all_modes, all_modes, duckdb_approx, data_fusion_exact,
    golden_approx_std, schema_validation_enabled`.
  - Their registry rows: five `gpu_*` cells `enabled`; `gpu_oracle` `golden_approx_std`; `60`
    struck from both rows' `tickets` and `56` from q2's.
  - Then `python3 scripts/ticket_widget.py`.
- [ ] **Step 3: Run locally** `--test test_cpu_corpus` (rust-only), except what needs the
  recording: the `duckdb_gpu_tpcds_q78_*` and `duckdb_gpu_tpcds_q2_*` cases and
  `every_enabled_device_cell_has_its_gpu_result_section_and_no_other`. `duckdb_tpcds_q78`,
  `duckdb_tpcds_q2`, `device_order_tpcds_q78` and `device_order_tpcds_q2` must pass. Expected:
  PASS.
- [ ] **Step 3a: The tolerance's docs.** `STDDEV_TOLERANCE`'s doc says it covers the Welford
  merge's reassociation and `round`'s two-step rounding (#60). `GoldenApproxStddev`'s doc and
  `ORACLE_HOW`'s sentences say the same, and the report is regenerated.
- [ ] **Step 4: The device.** One build, then:
  - `$G ssh "$R PCK_WRITE_GPU_RESULT=1 cpp/install/rust-tests/test_gpu_corpus --test-threads=1 tpcds_q78_ tpcds_q2_"`;
  - the `rsync`.

  It ran 10 tests and all 10 pass. `git diff --stat testdata/goldens/` shows only
  `tpcds.sf1/gpu-result.txt`, with ten sections added. The query lines and rows from Step 2 are
  in the tree before the sync, since `test_gpu_corpus` compiles a device case only for a line
  that enables it. Never `--pull-results`: it pulls from shad-gpu. A failing cell goes back off
  under a ticket, and its section is removed.
- [ ] **Step 5: Measure for the record.** Compare each new q78 and q2 `gpu-result.txt` section
  with `mini.result.txt`, cell by cell, with a throwaway script. Record in the detail file:
  - the number of differing cells, by column;
  - the largest relative gap;
  - that every integer and decimal cell matched exactly.

  This script is the evidence for Review Focus 3, since the comparison prints nothing on
  success. Expected: q78's five `ratio` cells at about 1.6e-16, and q2's gaps under 1e-15.
- [ ] **Step 6: Run locally** `--test test_cpu_corpus` in full (the `duckdb_gpu_*` cases read the
  recording). Expected: PASS.
- [ ] **Step 7: Archive.**
  - #60 (round() off by one ulp) under Stale, as won't fix. Its text says why, with Step 5's
    numbers. Its header label is malformed today ("(round() `round(x…"); the archive's header
    style has no label anyway.
  - #56 (q2 CASE in partial sum) under Done: "proven by tpcds q2 on the device at all five
    modes".
  - Update the index and `build-test.md`'s counts.
- [ ] **Step 8: Final device run:** the dispatched `GPU Tests (nebius-gpu)` CI job, green.
- [ ] **Step 9: Commit** `corpus: tpcds q78 and q2 on the device within 1e-11; #60 won't fix, #56 proven`
