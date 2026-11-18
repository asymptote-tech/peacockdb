# sirius-timing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sirius timed by its execution window beside peacockdb's `run_us`, the slow GPU part
explained from measurements, the harness fixed where it is at fault, one sf40 rerun on the H100.

**Architecture:** The runner (`docker/sirius-bench.py`, in the Sirius image) reads each timed
run's `[window]` span from Sirius's log and writes it beside the wall in `sirius.tsv`. The Rust
reader and the report render both clocks on the page and in `benchmarks.csv`. Two H100 sessions:
the first measures (Nsight, one setting at a time), the second reruns in the decided
configuration.

**Tech Stack:** Python 3 (runner, pytest over a fake DuckDB), Rust (`peacockdb-core`
`test_support`), bash (`scripts/env/`), Nsight Systems, Sirius `8d17afe` on the H100.

**Spec:** `llm-wiki/tasks/sirius-timing.md`

## Global Constraints

- Sirius pin unchanged: `stable`, `8d17afe`, its prebuilt extension. No Sirius patch.
- H100: at most 3 hours of VM time in all, `prepare` and every rerun included. Stopped after
  every session. A step that would pass 3 hours is not started.
- One change at a time in the experiments; a single-run number is marked as such.
- peacockdb's and DuckDB's results stay as they are. No test is weakened.
- The image changes only through `image.sh` and `images.yml`; the runner and the config are image
  inputs, so each change to them is published before an H100 session uses it:
  `gh workflow run images.yml --ref ENS-sirius-timing -f kind=sirius -f cudf=26.08`.
- `llm-wiki/coding-style.md` holds for every change: files under 1000 lines, functions under 150,
  comment limits, commit messages of at most 10 lines.
- All Rust and Python runs go through `scripts/build-test.sh` (`build-test.md`).

## Review Focus

1. A timed run's windows interleaved with another query's late-flushed lines: only the windows
   between this query's `timed` and `done` label lines count (test in Task 2).
2. A window that opens before midnight and closes after, across two day files: the span is taken
   from full timestamps, not the time of day (test in Task 2).
3. A window that ends with an outcome other than `ok` (`unwind`, `cleanup_failed`): the row is
   `failed`, never timed (test in Task 2).
4. `--resume` or `--list` over a `sirius.tsv` of the old five-column format: not kept, and
   `--list` refuses it as it refuses another Sirius commit (test in Task 2).
5. A peacockdb section without `run_us`, or a Sirius row timed before windows were recorded: a
   `—` cell, an empty CSV field, and the row out of the run ratio (test in Task 3).

---

### Task 1: `sirius.tsv` carries the run columns

**Files:**
- Modify: `peacockdb-core/src/test_support/sirius_results.rs`, `peacockdb-core/src/test_support/mod.rs` (`SiriusRow`)
- Modify: `testdata/fixtures/sirius-results.tsv`, both committed `sirius.tsv`
- Modify: `docker/sirius-bench.py` (`COLUMNS`, `ok_row`, `failed_row`)
- Test: `peacockdb-core/src/test_support/sirius_results/tests.rs`, `scripts/env/tests/test_sirius_bench.py`

**Interfaces:**
- Produces: `SiriusRow { …, run_ms_2nd_min: Option<f64>, run_runs_ms: Vec<f64>, … }`; the column
  line `query\tms_2nd_min\truns_ms\trun_ms_2nd_min\trun_runs_ms\tstatus\tnote`.
- Produces (runner): `ok_row(stem, walls, windows=None, note="") -> str`, writing `-` `-` for
  the run fields while `windows` is `None`; `failed_row(stem, why) -> str`.

Rules the reader holds a row to:
- `failed`, `skipped`: all four time fields `-`.
- `ok`, `wrong`, `unverified`: the wall fields set. The run fields are either both set, with as
  many runs as `runs_ms`, or both `-`: a row timed before windows were recorded.

- [x] **Step 1: Write the failing tests.** In `sirius_results/tests.rs`:
  - `the_fixture_reads_its_run_columns`: the fixture's `q1` has `run_ms_2nd_min == Some(301.0)`
    and `run_runs_ms` `305.0,301.0,299.0,303.0,308.0,302.0,306.0,304.0,307.0,309.0`.
  - `a_timed_row_without_run_times_reads_as_none`: `run_ms_2nd_min == None` and an empty
    `run_runs_ms`.
  - `a_run_count_unlike_the_wall_count_is_refused`: the panic names `run_runs_ms`.
  - `a_failed_row_with_a_run_time_is_refused`.
  - `write` round-trips the fixture byte for byte (the existing round-trip test, now over seven
    columns).

  In `test_sirius_bench.py`, `test_an_ok_row_has_seven_fields`: a scripted query's row is
  seven tab-separated fields, its run fields `-` `-`.
- [x] **Step 2: Run them; they fail** on the five-field rule.
- [x] **Step 3: Implement.**
  - The reader and writer take seven fields.
  - The fixture's `q1` gains the run times above.
  - Both committed `sirius.tsv` files get the new column line, and `-` `-` in the run fields of
    every row; nothing else in them changes.
  - The runner's `COLUMNS` and row writers follow.
- [x] **Step 4: Run them; they pass.** Also `every_sf40_sirius_status_meets_its_answer` and
  `the_benchmark_report_matches_the_trees`, unchanged and green (the page does not read the run
  columns yet).
- [x] **Step 5: Commit.**

### Task 2: the runner records each timed run's window

**Files:**
- Modify: `docker/sirius-bench.py`
- Test: `scripts/env/tests/test_sirius_bench.py` (the fake DuckDB writes window lines)

**Interfaces:**
- Consumes: Task 1's `ok_row`.
- Produces:
  - `between_labels(log_dir, sizes, start, end) -> str | None`: the log between two label
    lines. `warm_up_log` becomes a call of it.
  - `window_spans(log) -> tuple[list[float], int]`: each window's span in ms, in log order, and
    how many show a plan rebuild. Raises `ValueError(reason)` when a window is unpaired or ended
    other than `ok`.
  - The runner flag `--config <yaml>`: the Sirius config for this run, by default the image's.

How a query runs, and what Sirius logs at `8d17afe` (`src/sirius_context.cpp:551`,
`src/transparent/physical_sirius_execution.cpp`):
- The runner's statements, in order: label `<stem>`, the warm-up, label `<stem> timed`, the
  timed runs, then a new label `<stem> done`.
- The window lines read
  `[<date> <time>.<ms>] [info] [file:line] [window] begin|end instance=… connection=… window=<id> query=<n> outcome=<o>`.
  Begin and end are paired by `window=`.
- A rebuild is the line `Transparent execution: rebuilding Sirius plan at execute` between a
  pair.
- The row:
  - exactly `runs` paired windows, every one ending `outcome=ok` → `ok`, with the walls and the
    spans;
  - any rebuild → note `plan rebuilt in <k> of <runs> runs`;
  - another count → `failed: <n> execution windows logged for <runs> timed runs`;
  - an outcome other than `ok` → `failed: a window ended <outcome>`;
  - no `done` line in `LOG_WAIT_S` → `failed: no log of its timed runs in 10 s`.

- [x] **Step 1: Teach the fake to log windows.**
  - Each scripted execution writes `[window] begin` and `[window] end` with a fresh window id.
  - The timestamps come from a log clock the fake advances by the query's scripted `window_ms`
    (default: the run's latency).
  - Script keys: `window_outcome`, `rebuild` (runs that log a rebuild), `windows_missing` (runs
    that log none), `midnight` (the window crosses into the next day's file).
- [x] **Step 2: Write the failing tests.**
  - `test_records_each_timed_run_s_window_and_the_second_smallest`: `--runs 3`, walls
    `420, 400, 410`, windows `305, 300, 302` → the row
    `q1\t410.0\t420.0,400.0,410.0\t302.0\t305.0,300.0,302.0\tok\t-`.
  - `test_a_window_ended_unwind_fails_the_row`.
  - `test_a_missing_window_fails_the_row_naming_the_counts`.
  - `test_a_rebuilt_plan_keeps_the_time_and_says_so`.
  - `test_a_window_across_midnight_spans_both_files` (Review Focus 2).
  - `test_another_query_s_late_lines_do_not_count` (Review Focus 1).
  - `test_resume_and_list_refuse_an_old_column_line` (Review Focus 4): `--list` exit 2 naming
    the column line, `--resume` keeps no row.
  - `test_config_flag_reaches_sirius_config_file`.
- [x] **Step 3: Run them; they fail.**
- [x] **Step 4: Implement** `between_labels`, `window_spans`, the `done` label, the row rules
  above, the column-line check in `read_results`'s callers, and `--config`. The timestamps are
  parsed with `datetime.strptime(…, "%Y-%m-%d %H:%M:%S.%f")`.
- [x] **Step 5: Run the whole file and the image's self-test test; they pass.** Mutations to
  try:
  - drop the `outcome` check;
  - count windows outside the labels;
  - take the time of day only.

  Each must fail a test.
- [x] **Step 6: Commit.** The image's self-test runs where it is published (Task 4 Step 1).

### Task 3: the page and `benchmarks.csv` show both clocks

**Files:**
- Modify: `peacockdb-core/src/test_support/benchmark_report.rs` (main table header and `Shown`),
  `benchmark_report/decided.rs`, `benchmark_report/sirius.rs`, `benchmark_report/csv.rs`
- Create: `peacockdb-core/src/test_support/benchmark_report/run_tests.rs` (`#[cfg(test)]`
  module; `tests.rs` is over 1000 lines already and does not grow)
- Modify: `testdata/benchmark-report.html`, `testdata/benchmarks.csv` (regenerated)

**Interfaces:**
- Consumes: Task 1's `SiriusRow` run fields.
- Produces:
  - `Decided::Timed { …, run_us: Option<u64> }`, read from the section's `run_us`;
  - `run_ratio_line(queries, sirius, peacockdb_runs) -> Option<String>`, where `peacockdb_runs`
    maps a query to the Sirius device's `MAIN_MODE` `run_us` for rows timed and not `wrong`.

The page:
- **The main table.**
  - peacockdb's group is five columns: `load`, `execute`, `total`, `run (ms)`, `peak`.
    `run (ms)` is `run_us` in ms, linked as `total` is, or `—`.
  - An untimed peacockdb cell spans five.
  - The Modes tables keep their columns.
- **Sirius's group** is two columns:
  - `run (ms)`: `run_ms_2nd_min` with the `wrong` or `unverified` mark, or `—`;
  - `total (ms)`: the wall, linked, unmarked.
  - A `failed` or `skipped` row is one cell spanning two.
- **The ratio lines, in this order:**
  - `Σ run Sirius / Σ run peacockdb = <r> over <n> of <m> queries`: rows where Sirius is `ok`
    with a run and peacockdb is timed, not `wrong`, with `run_us`;
  - the existing `Σ total Sirius / Σ total DuckDB` line.
- **The Sirius paragraph** says what `run` and `total` hold, as the spec's table does. Task 6
  adds the departures and what is read outside the pin.
- **The CSV header** is
  `dataset,sf,query,engine,device,mode,load_ms,exec_ms,total_ms,run_ms,peak_gib,status`.
  - peacockdb: `run_us` in ms, or empty;
  - Sirius: `run_ms_2nd_min`, or empty;
  - DuckDB: empty.

- [x] **Step 1: Write the failing tests** in `run_tests.rs`:
  - `the_main_table_shows_peacockdb_s_run_after_its_total`;
  - `a_sirius_group_shows_run_then_total`;
  - `a_failed_sirius_row_spans_both_cells`;
  - `the_run_ratio_counts_only_rows_both_ran`, which pins the line's text;
  - `a_section_without_run_us_shows_a_dash_and_stays_out_of_the_ratio` (Review Focus 5);
  - `a_sirius_row_without_run_times_shows_a_dash_and_stays_out_of_the_ratio` (Review Focus 5);
  - `the_csv_has_run_ms_after_total_ms`;
  - `the_modes_tables_do_not_change`.

  Build each over the existing test trees in `tests.rs`, with `run_us` added where needed.
- [x] **Step 2: Run them; they fail.**
- [x] **Step 3: Implement.** If `benchmark_report.rs` would pass 1000 lines, move the main table
  (`dataset_table` and its helpers) into `benchmark_report/main_table.rs`.
- [x] **Step 4: Regenerate** with `UPDATE_CANONICAL=1 … the_benchmark_report_matches_the_trees`.
  Then run `test_corpus_goldens`, `test_cpu_corpus`'s `sirius` tests and the lib's
  `benchmark_report` and `sirius_results` tests without the variable: all green. Today's
  `sirius.tsv` has no run times, so the run ratio reads `— over 0 of <m>`.
- [x] **Step 5: Commit.**

### Task 4: publish the image, then the measuring session on the H100

**Files:**
- Modify: `llm-wiki/tasks/sirius-timing-detail.md` (create on first write)

Each experiment runs the runner in the Sirius container as `vm_sirius` (`run-sirius-vm.sh`)
starts it, over a data directory holding only the tables its queries read: symlinks to the VM's
sf40 files, made by hand. `run-sirius.sh run` takes neither `--data` nor `--out`, so the
experiment's `docker run` is written out. It passes the experiment's config by `--config` and
its own `--out`, both under the shipped `testdata/calibration/sirius/experiments/`, which git
ignores: an experiment never writes into `testdata/benchmark-results/`, whose `# ` line does not
name the config. Every command that is not a committed script goes into the detail file
verbatim, with its time.

- [x] **Step 1: Publish** the image of Tasks 1–2 (`images.yml`, Global Constraints). Wait for
  green, and note the inputs tag in the detail file.
- [x] **Step 2: Start the H100** (`build-test.md`, "Sirius on the H100", for the instance and
  profile). Then `run-gpu.sh … prepare` and `run-sirius.sh … prepare`. Log each start, stop and
  running total in the detail file.
- [x] **Step 3: The first real check of the windows.**
  - Run tpch q6 alone, the image's config, `--runs 10`.
  - Expected: `ok`, ten windows, no rebuild note, the wall near 339 ms.
  - Check that no label `CALL` opens a window of its own. If one does, stop: Task 2's count
    rule is wrong.
- [x] **Step 4: Nsight.**
  - Capture q6, q9 and q19 inside the Sirius container under its three options. nsys is the
    VM's (`run-gpu-vm.sh`'s `vm_nsight` installs it), mounted read-only into the container.
  - Record per query: the bytes and rate of each host-to-device copy from the pinned cache, the
    decode and kernel time, and the idle gaps between chunks.
  - Record whether every timed run copies the pinned bytes, and what `h2d=0` counts.
- [x] **Step 5: The settings.** q6 and q9: `scan_manager` chunk size, task size and
  `executor.pipeline.num_threads`, one at a time, two values each beside the default.
- [x] **Step 6: The reads outside the pin.**
  - q19 and q10: as today, then from the same files on tmpfs, then from parquet without bloom
    filters (made on the VM, the metadata checked as Task 5 states).
  - Record the bytes a miss reads.
- [x] **Step 7: The footer.** q6's planning span (wall minus window) as today, with
  `parquet_metadata_cache` on, and with tables registered other than as `read_parquet` views,
  if `pin_table` accepts it.
- [x] **Step 8: The log.** q6's wall at `info` and at `warn` (`SET sirius_log_level`). At `warn`
  Sirius logs neither the GPU evidence nor the windows, so the runner fails the row: both walls
  come from a short timing script in the container, written into the detail file.
- [x] **Step 9: Device residency.** Only if Step 4 shows timed runs not copying from the host:
  find the setting that makes every run copy, then time q6 and tpcds q64 both ways.
- [x] **Step 10: Stop the H100.** Write each finding into the detail file under "Findings",
  numbers and commands with it.
- [x] **Step 11: Decide the configuration.**
  - Apply the spec's keep rule (Done when 2) to every finding.
  - Under "Configuration" in the detail file, list each kept departure with its reason and its
    measured effect, and the departures rejected.
  - If nothing is kept and residency needs nothing, Task 5 is skipped, and Task 6 runs the image
    as it is.
- [x] **Step 12: Commit the detail file.**

### Task 5: the decided departures in the harness

Skipped: no departure is kept (`sirius-timing-detail.md`, "Configuration").

**Files:** only those a kept departure needs:
- `docker/sirius-config.yaml` (settings);
- `docker/sirius-bench.py` (statements; the residency flag);
- `scripts/env/run-sirius.sh`, `scripts/env/run-sirius-vm.sh` (a layout's `prepare` step,
  tmpfs);
- `scripts/env/bench-sirius.sh` (the residency flag);
- tests: `scripts/env/tests/test_{sirius_bench,run_sirius,bench_sirius}.py`;
- `sirius_results.rs` and the fixture, if the `# ` line gains a key.

**Interfaces:**
- Produces, if residency is kept:
  - the `bench-sirius.sh` flag `--device-resident`, passed to the runner as the same flag;
  - the `# ` line's eighth key, `device_resident=yes|no`, last.
  - `--resume` and `--list` refuse a file whose value differs (exit 2, naming both values).
- Produces, if a layout is kept: `run-sirius.sh … prepare` makes it on the VM from the sf40 set.
  - Same row groups, pages, encodings and compression: compared over `parquet_metadata()`. Only
    the departure's own metadata may differ.
  - Refused on any other difference, exit 3.
  - The sha256 of each file goes into `prepare`'s output.

- [x] **Step 1: Write the failing tests** for each kept departure:
  - the config's settings in `test_the_config_file_is_*` (the self-test's expected set grows);
  - the flag's pass-through and refusal in `test_bench_sirius.py` and `test_sirius_bench.py`;
  - the layout step against a fake VM in `test_run_sirius.py`, a metadata difference refused.
- [x] **Step 2: Run them; they fail.**
- [x] **Step 3: Implement.**
- [x] **Step 4: Run them; they pass.** Rebuild the image locally; self-test green.
- [x] **Step 5: Commit, then publish the image** (Global Constraints) and wait for green.

### Task 6: the rerun at sf40

**Files:**
- Modify: both `sirius.tsv`, both `sirius-result.txt`, `testdata/benchmark-report.html`,
  `testdata/benchmarks.csv`, the Sirius paragraph's text in `benchmark_report/sirius.rs`
- Modify: `llm-wiki/tasks/sirius-timing-detail.md`

- [x] **Step 1: Check the budget.** Only if the detail file's running total plus ~60 minutes
  stays within 3 hours: start the H100, `prepare` both scripts. Make the layout if one is kept.
- [x] **Step 2: Run** `bench-sirius.sh … --dataset all`, no `--resume`, under the kept
  configuration (the residency flag off). Then stop the H100.
- [x] **Step 3: Set the statuses.**
  - Run `UPDATE_CANONICAL=1 … every_sf40_sirius_status_meets_its_answer`.
  - List each status change against today's files in the detail file, with its reason.
  - A row `ok` today that is now `wrong` because of a kept departure: drop that departure,
    re-publish, rerun that dataset (back to Step 1's budget check), and record it.
- [x] **Step 4: Finish the page.**
  - The Sirius paragraph gains the departures and what is still read outside the pin.
  - If Sirius reads files laid out unlike peacockdb's, the paragraph says so.
  - If residency could not be turned off, the paragraph says so too.
  - Regenerate the page and the CSV. All the Task 3 tests stay green.
- [x] **Step 5: Commit** the results, the page, the CSV and the detail file.

### Task 7: the wiki and the tickets

**Files:**
- Modify: `llm-wiki/build-test.md` ("Sirius on the H100": the image's settings, the protocol's
  two clocks and the window's 1 ms resolution, each departure, the `sirius.tsv` columns, the
  flag, a layout step; "The page"; the test counts)
- Modify: `llm-wiki/tickets/benchmarks.md`, `llm-wiki/tickets.md`,
  `llm-wiki/archive/archived-tickets.md`

- [x] **Step 1: Close #311 (Sirius planning time).** Move it to the archive with a "**Done.**"
  paragraph: what the window is, the run ratio, the findings in one line each.
- [x] **Step 2: File a ticket for each finding left unfixed** (one needing a Sirius change,
  residency that no setting turns off). Use the next free number in `tickets.md`.
- [x] **Step 3: Update the counts.** Take them from runs on this tree: the lib's
  `benchmark_report` and `sirius_results`, `test_cpu_corpus`, `test_corpus_goldens`,
  `scripts/env/tests`. Then `cost-report --test test_source_formatting` stays green.
- [x] **Step 4: Commit; push; open the PR against master.**
