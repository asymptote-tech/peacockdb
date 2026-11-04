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


## Task 3: the harness records the sf40 answer and times a case out

- **Red → green.** `cargo test -p peacockdb-core --features rust-only --lib -- corpus_golden
  benchmark_section` failed to compile (`E0432 unresolved imports super::case_timeout_asked,
  super::timeout_record, super::watch_case` and `super::declared_sf`), then 17/17 (9 before, 8 new).
  `python3 scripts/env/tests/test_run_gpu.py`: the two new tests failed (`envs ==
  ['PEACOCK_BENCH_RESUME=1']`; no `gpu-result.txt` came home), then 31/31. Mutation: `merged_cells`
  filtering at `sf` again fails `an_sf40_answer_keeps_the_other_sf40_answers`.
- **The watch** (`benchmark_section.rs`, `watch_case`): a thread waits on a channel for
  `PEACOCK_BENCH_CASE_TIMEOUT_S`. The case drops the returned `CaseWatch` once `measure_into`
  returns, which ends the thread with nothing written. Without the drop, a run of several cases
  in one process would have the first case's watch fire during a later one and overwrite the
  first section. On expiry the thread merges `timeout_record` (the same body as a panicked case's,
  `failed_record`) and calls `expire`, which `benchmark_case` makes `std::process::exit(1)`.
  Started only for a plain run (`Capture::None`): a captured run writes no section. It covers
  the prefill, which runs inside `measure`.
- **Departures from the plan's text:**
  - The plan names one timeout test; Review Focus 1 names `a_case_past_its_timeout_is_recorded_failed`.
    Both exist, plus `a_case_within_its_timeout_records_nothing` and three over the variable's
    parse (`case_timeout_asked`: unset or empty → none; `30m` and `0` refused naming it). The
    watch takes its expiry as a closure so the test hears it instead of exiting.
  - `Provenance` derives `Clone`: the watch holds the five by value.
  - The answer's row order (`answer_order`, `corpus_benchmark.rs`) reads the registry row at
    `declared_sf(sf)`; recording happens in `benchmark_case`'s `Measured` arm, after the section.
    `measure` returns `(body, (rows, answer))`; the closure `measure_into` runs before the section
    appends the rows and keeps the answer.
  - `test_nsys_profile_uses_the_given_ssh` (test_run_gpu.py) failed at HEAD too: pinned-loads'
    `1c85b136` made `create_nsys_profile.sh` run `nsys_loads.py`, and the test's fake tree stubbed
    only `nsys_calls.py`. Fixed in the test (a `nsys_loads.py` stub; `loads.tsv` asserted), since
    Task 6's `--cases` test sits beside it.
- **Unrun (needs a device):** a case timing out on a card; a measured case's answer written into
  `goldens/<ds>.sf40/gpu-result.txt` (Task 9 runs both under `bench-each.sh`).
- **Gates:** `--lib` 973 passed, 2 ignored (975; 967 before); `test_corpus_goldens` 53;
  `cost-report --test test_source_formatting` 32; `test_run_gpu.py` 31/31. Gpu shape: `cargo test
  --no-run -p peacockdb-core --features gpu --lib --tests` in the 25.02 build image
  (`CARGO_TARGET_DIR=target-img-25.02`), exit 0, no warning (cold 13 min 21 s, then 17 s over the
  formatted sources). `shellcheck` is not installed on dev; `bash -n scripts/env/run-gpu.sh` passes.

## Task 4: the sf40 answers held to DuckDB's

- **Red → green.** `cargo test -p peacockdb-core --features rust-only --test test_cpu_corpus --
  sf40 stale ok_row tree_without` failed to compile (`E0432 unresolved imports
  test_support::AnswerVerdict, test_support::sf40_answer_verdict`), then 4/4. Mutation: `sf40_verdict`
  reading the registry at sf40 instead of `declared_sf` fails `an_ok_row_without_an_answer_is_refused`
  (no sf40 row). Whole `test_cpu_corpus` 2403 (2399 before), the sf1 `duckdb_gpu_*` cases unchanged.
- **Departures from the plan's text:**
  - **The verdict is `AnswerVerdict { Meets, Differs(String), Unanswered(String) }`, not
    `Result<(), String>`** (declared in `test_support/mod.rs`). With a `Result`, a `wrong` row
    passes on any `Err`, and a missing `gpu-result` section, a file another cuDF recorded or
    DuckDB's `failed:` would keep a stale `wrong` mark green: Review Focus 5. `row_problem` holds
    a `wrong` row to `Differs` alone; `Unanswered` fails an `ok` and a `wrong` row alike.
  - `judge` became `verdict` (returns the verdict, takes `kinds_sf`) and `held` (the sf1 cases'
    panic, text unchanged: `<ds>/<q> (the cpu|the device at <mode>): …`). `device_verdict` holds
    `duckdb_gpu_case`'s reading of the file. Its messages lost their `<ds>/<q> at <mode>:` prefix,
    which `held` now adds in its own form.
  - `sf40_answer_verdict` takes the list's ident (`aggregate_groupby`) and stems it.
    `sf40_verdict` (`duckdb_oracle.rs`) reads the registry at `declared_sf("40")`, which is no
    longer rust-only dead code.
  - `an_ok_row_without_an_answer_is_refused` asks for tpcds q12, whose `gpu_tp1_single` is `na`:
    no sf40 recording ever holds its section, so the test stays true once Task 9's files land.
  - `list_problem`: a list without its tree cannot happen (the list lives in the tree's directory)
    and passes.
- **Unrun until Tasks 9 and 11:** a `Meets` and a `Differs` verdict at sf40 over real files, and
  the kinds planned at sf1 for an sf40 comparison. The sf1 cases cover the comparison itself.
- **Gates:** `--lib` 973 passed, 2 ignored; `test_corpus_goldens` 53; `test_golden_format` 41;
  `test_cpu_corpus` 2403; `cost-report --test test_source_formatting` 32. Gpu shape in the 25.02
  image: exit 0, no warning (2 min 28 s incremental).

## Task 5: DuckDB's sf40 answers, rendered

- **Red → green.** `python3 testdata/test_duckdb_result.py`: 27 run, 1 failure and 4 errors
  (`AttributeError: … no attribute 'answer_sections'`; `generate() got an unexpected keyword
  argument 'answers'`; `unrecognized arguments: --answers`), then 27/27. Regenerating every sf1
  golden through the refactored path (`python3 testdata/duckdb_result.py`, 40 s) left
  `testdata/goldens/{tpch,tpcds,pbench}.sf1/duckdb-result.txt` byte-identical (`git status` clean).
- **Shape.** `generate` splits into `parquet_sections` (sf1, unchanged) and `answer_sections`, both
  over one `sections(dataset, stems, answer)` and one `session()` (threads=1, the null order,
  integer division). `answer_sections` attaches the database read-only and selects
  `SELECT * FROM answers."<stem>" ORDER BY rowid`. Without `--only` its stems are the database's
  tables in `query_order`. With `--only`, they are the named query files, and one with no table is
  `failed: no answer recorded`. A database with no table exits, since an empty golden reads as
  "DuckDB answers nothing" everywhere.
- **Departures from the plan's text:**
  - Table names are the query file stems (`aggregate-groupby`), which Task 6's `--answers` must
    create (`answers."<q>"`, `<q>` the ident with `_` → `-`, as the plan says).
  - Beyond the plan's three tests: `test_an_answers_database_holding_no_answer_is_refused`,
    `test_answers_are_written_beside_the_sf40_goldens` (`generate(…, root=<tmp>)`).
  - The flag rules, each `parser.error` (exit 2) before `import duckdb`: `--answers` needs `--sf`,
    refuses `--sf 1`, and needs exactly one `--dataset`; `--sf` other than 1 needs `--answers`.
  - **CI runs the four rendering tests as skips.** The `testdata/test_*.py` step has no duckdb
    wheel and no parquet, so `Answers`' rendering tests skip there (`unittest.skipUnless`).
    `test_answers_need_a_scale` runs in CI. Shown with `python3 -S`: 27 run, 4 skipped. They run
    on dev and in the build image (`/opt/venv` has duckdb 1.5.4).
- **Gates:** `testdata/test_duckdb_result.py` 27/27 on dev.

## VM time

| VM | start (UTC) | stop (UTC) | hours | why |
|---|---|---|---|---|
- Known red on `ENS-pinned-loads` and `ENS-bench-rows`: `scripts/env/tests/test_run_gpu.py::test_nsys_profile_uses_the_given_ssh` — pinned-loads' `1c85b136` made `create_nsys_profile.sh` call `nsys_loads.py`, which that test's fake tree lacked. Fixed here (Task 3, a stub in the fake tree); pinned-loads must carry the same fix when it resumes (or get it by the rebase order).
