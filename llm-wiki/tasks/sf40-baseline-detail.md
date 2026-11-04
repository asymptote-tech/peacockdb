# sf40-baseline — detail

## Run log

- 2026-11-04: `ENS-sf40-baseline` branched from `ENS-bench-rows`, while new-hosts, pinned-loads and
  bench-rows are `blocked(building)` on the H100 path (the human's GitHub PAT; see
  `new-hosts-detail.md`, "Blocked"). Tasks 1–7 are offline and run now; Task 8 (the CPU-VM build and
  DuckDB's tpcds database), Tasks 9–11 (the H100 and the 128-core runs) and Task 12 wait.

## Task 1: the H100 list's rows, written from the tree; the scripts' reader

- **Red → green.** `cargo test -p peacockdb-core --features rust-only --lib -- device_cases` failed
  to compile (`E0432 unresolved imports crate::test_support::Measured, device_lines`), then 24/24
  (11 before, 13 new). `python3 scripts/calibration/tests/test_device_cases.py` failed
  (`ModuleNotFoundError: No module named 'device_cases'`), then 6/6. Mutations: honouring no
  tp1-failed exemption fails `a_later_mode_of_a_query_failed_at_tp1_single_owes_no_section`;
  dropping the `wrong` mark fails `a_rewrite_keeps_a_wrong_answer_mark`; `device_cases.py`
  splitting at every comma refuses the shared fixture's `skipped` row (exit 1).
- **What `device_lines` decides** (`test_support/device_cases.rs`, `lines`):
  - Considered modes, in `modes`' order: those the candidate's line carries whose file exists.
    `tp1_single` is considered even without its file, so a tree that never ran the dataset is an
    `Err`, not an empty row.
  - Sections are matched by the query with `_` → `-` (`aggregate_groupby` ↔ `== aggregate-groupby`),
    as `declared_files` and the page match them. The `files` closure returns sections as the file
    spells them; Task 9's closure leaves out `SKIPPED` markers, which are no measurement.
  - Each `(dataset, mode)` file is asked for once, before any lookup.
  - A `wrong` mark on a skipped query, and one on no candidate, are both `Err`: the spec's "a mark
    on a query not measured" covers both, and the second would otherwise vanish on a rewrite.
  - A candidate carrying none of `modes` is an `Err`; no caller passes one (candidates are the
    `tp1_single` lines).
- **Departures from the plan's text:**
  - Beyond the plan's eight Rust tests: `a_query_is_found_under_its_section_name`,
    `a_mode_the_line_does_not_carry_is_not_considered`, `a_missing_tp1_single_file_is_refused`,
    `a_later_mode_of_a_query_failed_at_tp1_single_owes_no_section`,
    `a_wrong_mark_on_no_candidate_is_refused`. Beyond the plan's four Python tests:
    `test_rows_refuse_what_rust_refuses`, `test_a_note_keeps_its_commas`.
  - `parse_device_cases` panics rather than returning `Result` (containers-rebase's shape, kept);
    `device_cases.rows` matches it with `SystemExit` naming the file, line number and text.
  - `device_cases.py --mode` takes only the five `MODES` idents (argparse `choices`), so a typo
    fails before it narrows to nothing. Its `MODES` tuple is a copy of Rust's `MODES` idents.
  - The fixture's skipped reason is `timeout: the case ran past 1800 s, and was killed` (a comma).
- **Gates:** `--lib` 965 passed, 2 ignored (967; 954 before); `test_corpus_goldens` 50;
  `cost-report --test test_source_formatting` 32; `scripts/calibration/tests/test_*.py` all green
  but `test_plot.py`, which needs matplotlib, absent on dev (CI installs it; untouched here).
  No warning.

## Task 2: a device list's rows are main-list cases; the DuckDB tree is not a device

- **Red → green.** `cargo test -p peacockdb-core --features rust-only --test test_corpus_goldens --
  device_list duckdb_tree`: three FAILED. The two list tests returned `Ok(())` (panicked at their
  `expect_err`), and `device_directories` named `duckdb-cpu128` beside `h200`. Then 4/4 (with
  `every_device_list_parses`), and the binary 53/53.
- **Departures from the plan's text:**
  - **`H100_MODES` is not added here.** Its first reader is Task 9's
    `the_h100_list_is_the_lists_measured`; a const with no reader warns `dead_code` in the test
    binary. Task 9 adds it beside `OTHER_ENGINES`.
  - **`benchmark.rs` split.** With the three tests it reached 1071 lines. The device-list block
    (`Files`, `declared_files`, `DEVICE_LIST`, `device_list_files`, `missing_case_problem`,
    `device_list_problem`, `OTHER_ENGINES`, `device_directories` and their eight tests, three new)
    moved verbatim to `tests/test_corpus_goldens/benchmark_lists.rs` (342 lines); `benchmark.rs`
    is 742. `CASE_LIST`, `PREDATES_PROVENANCE`, `synthesized_tree` and `modes` became
    `pub(crate)` for it. Task 9's test goes in `benchmark_lists.rs`, beside the list's other checks.
  - `a_device_list_at_a_mode_the_case_list_lacks_is_refused` holds the tree to a one-line main
    list (q1 at `tp1_single`): every line of the committed list carries all five modes, so no row
    of it lacks one. `a_device_list_beyond_the_case_list_is_refused` uses the committed list, the
    synthesized file's `== q1` renamed `== q99`, so the section exists and only the new check
    refuses it.
  - `the_duckdb_tree_is_no_device_tree` also asserts `missing_case_problem` over that root is
    `Ok`: the end that matters is that the DuckDB tree is held to nothing.
  - The page (`benchmark_report::devices`) needed no change: a directory with no mode file is no
    column already.
- **Gates (Tasks 1 and 2 together):** `--lib` 965 passed, 2 ignored (967); `test_corpus_goldens` 53
  (50 before); `test_golden_format` 41; `test_cpu_corpus` 2399; `cost-report --test
  test_source_formatting` 32; calibration Python as in Task 1. No warning. No gpu-shape compile:
  nothing here is behind a feature, and `test_support` has no device code in this change.


## VM time

| VM | start (UTC) | stop (UTC) | hours | why |
|---|---|---|---|---|
