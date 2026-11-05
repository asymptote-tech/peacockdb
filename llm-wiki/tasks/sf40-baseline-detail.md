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

## Task 6: `duckdb_minimal.sh` on the corpus; the H100 driver; records joined

- **Red → green**, each `python3 <file>`: `test_join_records.py` `ModuleNotFoundError: join_records`,
  then 8/8. `test_split.py` 11/12 (`cases.csv declares no corpus_query_benchmark! case`), then
  12/12. `test_publish_record.py` 1/4 (no script), then 4/4. `test_run_gpu.py` 31/33 (`unknown
  argument: --cases`), then 33/33. `test_bench_each.py` 0/11 (no script), then 11/11.
  `benchmarks/tests/test_duckdb_minimal.py` 1/7 (`unknown argument '--cases'`, `'--dataset'`),
  then 9/9 with the two below. Mutation: dropping `bench-each.sh`'s set-aside of a VM record fails
  `test_bench_each_sets_aside_a_record_a_killed_run_left` (the fake run refuses, exit 2).
- **A bug found offline, fixed test-first:** the DuckDB CLI parses a whole `-c` before running it,
  and `integer_division` is a parser setting. `duckdb -c "SET integer_division=true; SELECT 7/2"`
  answers `3.5`; with the SET in its own `-c`, `3`. So `--answers` and the corpus timed runs pass
  the settings in a `-c` before the query's (`run_once` too: the minimal set's settings are
  runtime ones, unchanged). `test_answers_take_the_oracle_s_integer_division` (tpcds q61's `/`
  named `//`) failed before the fix.
- **Offline check of Task 11's path at sf1.** Every candidate (`grep tp1_single` of the main list:
  tpch 44, tpcds 82) as an `ok` row, `--runs 1 --cases --answers --results`: 44 and 82 answered and
  timed `ok`. `answer_sections` over the two answers databases against the committed
  `goldens/<ds>.sf1/duckdb-result.txt`: 126 of 126 sections byte-identical. Before the fix tpcds
  q2 and q61 differed, in column names only.
- **The default run** (no new flag) against HEAD's script, both at sf1 into fresh dirs, numbers
  and paths normalised: identical but for the per-table size listing's order of `nation` and
  `region`, a tie under the old `ORDER BY blocks DESC` (nondeterministic before this change).
- **Departures from the plan's text:**
  - **The record has no `# case` line.** A killed run's record is set aside under its last row's
    (dataset, query, mode). A record with no row is removed on both sides. A local
    `testdata/calibration/records.tsv` at start is set aside the same way.
  - **`bench-each.sh` continues only on `run-gpu.sh` exit 0 or 1** (1 covers a failed, timed-out or
    crashed case: the container's code is mapped to 1). Any other exit (2, 3, 4, 75, 69) stops it
    with that code: 126 refusals in a row is not a run.
  - **The slug** is not read from a section's `device=` (the H200 tree holds q1 too): it is the
    device tree whose file for the case the first measured case's pull changed, kept in
    `<records>/device`. Until known nothing is skipped; the harness's own resume keeps a section
    the VM still holds.
  - **`scripts/lib/bench-cases.sh`**: `bench_lines`, `bench_datasets`, `bench_cases` moved out of
    `shadgpu-env.sh` unchanged (it sources the file), for `bench-each.sh`. `RUN_GPU_SH` and
    `NEBIUS_SH` name the two scripts for the test's fakes.
  - **`join_records.py`**: cases in the order the arguments first name them; two segments of one
    attempt holding one case are refused; the span is `79.1GiB-79.3GiB` (one token when equal).
  - **`test_publish_record.py`** (4) is beyond the plan, which names no test for the script.
  - **`create_nsys_profile.sh --cases`**: the trace pass runs the list's test names `--exact`,
    narrowed by `PCK_TEST_FILTER` when set; `test_cases_with_a_filter_narrow_the_trace_pass` beyond
    the plan. Its existing `--ssh` test now builds its tree through a shared `nsys_tree`.
  - **`duckdb_minimal.sh`**: `--answers` is refused without `--cases` and with `--skip-verify`
    (the reading of "`--cases` with `--skip-verify` and `--answers` refused"). `--results` works
    without `--cases`. `--dataset tpcds` needs `--cases` or `--build-only`. A row's `sf` is not
    held to `--sf` (the tests read a list at sf1). The answers database is removed at the start of
    the pass. TSV `status` is the timing's; a refused answer shows `ANSWER FAILED` in stdout and
    has no table. The working set's SQL reaches python through argv, not a heredoc. Corpus SQL loses
    its `--` comments and final `;` (`corpus_sql`). The preamble's values are one word each,
    whitespace as `_` (the CPU's model name); `duckdb` is `SELECT version()` without the `v`.
- **Gates (scripts):** `scripts/env/tests/test_*.py`: every file green but the two that need images
  or staged installs (`test_image_any_user.py` 1/2: no 26.02 build image; `test_run_image.py` 7/9:
  `PEACOCK_STAGED_INSTALLS` unset). `test_run_tests.py` failed once in the sweep while Docker
  reported the run images absent (`test_run_image.py` said `no image …run:cudf-25.02` in the same
  pass), then passed 8 of 8: the image state, not this change (`docker/run-tests.sh` untouched).
  `scripts/calibration/tests/test_*.py` all green but `test_plot.py` (no matplotlib on dev).
  `bash -n` on every changed script; no `shellcheck` on dev.

## Task 7: DuckDB's results read once; the page's DuckDB column

- **Red → green.** `cargo test -p peacockdb-core --features rust-only --lib -- duckdb_results
  benchmark_report`: `a_duckdb_tree_is_a_column_of_its_own` (`a column`) and
  `the_duckdb_column_names_its_provenance` failed, then 23/23 (12 + 2 page, 9 reader). The reader's
  tests compiled only with the module; mutation: accepting a failed row with a time fails
  `a_failed_row_with_a_time_is_refused`. `test_results_have_the_fixtures_shape` passed at once:
  Task 6 wrote the TSV, and the fixture follows it. Swapping two columns of the script's header
  fails it.
- **Departures from the plan's text:**
  - `workset_bytes` and `disk_read_bytes` are `Option<u64>`: a failed row has no disk read, and a
    database whose storage info DuckDB withholds has no working set (`-`). `DuckdbResults`
    derives `Debug, Clone, PartialEq` for the fixture test.
  - Beyond the plan's two reader tests: a working set read as none, an unknown status, a failed row
    with a time, an `ok` row without one, a missing preamble, the column line out of order, a query
    listed twice.
  - `a_directory_without_benchmark_files_is_no_column` wrote `duckdb/tpch.sf40/duckdb.tsv` as its
    stray file, which is now a DuckDB column; it writes `notes/tpch.sf40/notes.txt` instead.
  - A DuckDB cell links to its `duckdb.tsv` line, as a device's total links to its section.
- **The page** is unchanged (no DuckDB tree is committed): `the_benchmark_report_matches_the_trees`
  passes without `UPDATE_CANONICAL`, so nothing was regenerated.

- **Gates (Tasks 6 and 7 together):** `--lib` 984 passed, 2 ignored (986; 975 before);
  `test_corpus_goldens` 53; `test_golden_format` 41; `test_cpu_corpus` 2403; `cost-report --test
  test_source_formatting` 32; no warning. `UPDATE_CANONICAL=1 … the_benchmark_report_matches_the_trees`
  left `testdata/benchmark-report.html` byte-identical. No gpu-shape compile: nothing here is
  behind a feature. `benchmarks/tests/test_duckdb_minimal.py` (9, ~2 min) and
  `scripts/env/tests/test_bench_each.py` (11) run on dev only: no CI job runs `benchmarks/tests/`
  or `scripts/env/tests/`. `test_join_records.py` and `test_publish_record.py` run in CI's
  calibration step (stdlib, a fake aws).

### For the live runs (Tasks 8–11)

- **Task 8 Step 2**: `--dataset tpcds --build-only` prints `tables : 24 from the parquet: …`, then
  `row counts verified against the parquet` and `built : …; --build-only`; no `=== page cache`.
- **Task 9 Step 3**: before the first `bench-each.sh`, dev's `testdata/calibration/records.tsv`
  must be absent or this run's: one there is set aside as a segment under its last row's case.
  Give `--records` a scratch dir outside the tree. Each case prints `==> <ds> <q> <mode>: run-gpu.sh
  exited <rc> after <s> s`: the per-query wall times. After the first measured case,
  `<records>/device` must read `h100-80gb-hbm3`. Exit 1 means a case has no section: rerun the same
  command. Exit 75 or 69 is the VM's: the coordinator's retry, then rerun. Each case pulls the whole
  `benchmark-results/`, `calibration/` and the answers.
- **Task 9 Step 6**: `join_records.py --record <scratch>/records/*.tsv` (the `device` file is not
  `.tsv`). The joined heading's `allocator=` gives the free memory as `<min>-<max>`. `split_record.py
  --cases <the H100 cases.csv>` (a device list is read as such by its header).
- **Task 10**: `create_nsys_profile.sh --cases <scratch>/<ds>.csv --device h100-80gb-hbm3 …` runs
  the trace pass as one `--exact` list of every row's test name.
- **Task 11 Step 1**: `--answers` takes an absolute path without a quote; the database is replaced
  each run. Every `ok`/`wrong` row's `testdata/<ds>-queries/<stem>.sql` is checked before any import.
  `threads=` is DuckDB's own count. The corpus runs time the query under the oracle's two
  settings, so DuckDB's time is the answered query's.

## VM time

| VM | start (UTC) | stop (UTC) | hours | why |
|---|---|---|---|---|
| nebius-cpu128 | 2026-11-05 10:56 | 11:10 | 0.23 | Task 8: the staged install, the tpcds sf40 database |
| nebius-h100 | 2026-11-05 11:11 | 12:30 | 1.32 | Task 9: prepare, ship, the 130-case run |
| nebius-h100 | 2026-11-05 12:35 | 14:14 | 1.65 | Task 10: both Nsight passes, both datasets |
| nebius-cpu128 | 2026-11-05 14:15 | 14:40 | 0.42 | Task 11: DuckDB's corpus runs, both datasets |
| nebius-gpu | 2026-11-05 15:29 | 15:32 | 0.05 | the L40S list: synced, disk 4.9 GB free, no build (below) |

Totals on 2026-11-05: nebius-h100 2.97 h, nebius-cpu128 0.65 h. Both VMs were stopped softly at
the end of each step, and both read `STOPPED` at 14:40.

- Known red on `ENS-pinned-loads` and `ENS-bench-rows`: `scripts/env/tests/test_run_gpu.py::test_nsys_profile_uses_the_given_ssh` — pinned-loads' `1c85b136` made `create_nsys_profile.sh` call `nsys_loads.py`, which that test's fake tree lacked. Fixed here (Task 3, a stub in the fake tree); pinned-loads must carry the same fix when it resumes (or get it by the rebase order).

## Blocked (2026-11-04)

`blocked(building)`: Tasks 8–12 need the VMs: Task 8 the CPU-VM build and DuckDB's tpcds database
on nebius-cpu128, Tasks 9–10 the H100, Task 11 the 128-core DuckDB run. All wait on new-hosts'
block (the human's GitHub PAT; `new-hosts-detail.md`, "Blocked"), the obstacle the analyst
diagnosed there. Tasks 1–7 are committed (`2a6b0821`, `641a69af`, `a5458f7b`).
- GitHub's SSH from dev was flaky on 2026-11-04 from about 10:40Z (`Permission denied
  (publickey)` on most pushes and fetches, though `ssh -T git@github.com` authenticated). Pushes
  went over HTTPS with gh's credentials, no config change:
  `git -c credential.helper='!gh auth git-credential' push https://github.com/asymptote-tech/peacockdb.git <b>:<b>`.

## Rebase onto master (2026-11-04)

Rebased onto ENS-bench-rows, which is on master (the ENS-optimizer merge #165 and the
inventory-based case listing). HEAD `b047597f`.

- **`benchmark.rs` conflict (2a6b0821's replay):** this commit moves the device-list checks to
  `benchmark_lists.rs`. HEAD had ported that code to the inventory. So the port moved with it:
  `declared_files(cases: &[Case])`, its 5 callers on `&listed_cases()`, no `CASE_LIST` and no
  `macro_invocations_in` there. `Case` and `listed_cases` are `pub(crate)` in `benchmark.rs`;
  `CASE_LIST` is private again, read only by `the_benchmark_cases_follow_the_registry`.
- **`build-test.md`:** the replay kept HEAD's side of seven hunks. Restored by meaning: the sf40
  tier's paragraph before the order tier (202, master's); the calibration row with the device-list
  reader, the joined segments and the publish (70); the Images row with `run-gpu.sh`'s case timeout
  and answer switch, `create_nsys_profile.sh --cases` and `bench-each.sh` (237); the new DuckDB
  benchmark script row (9); the sf40 rules in the `duckdb-result.txt` and `gpu-result.txt` golden
  rows. Recounted from `--list`: cpu 3491 (`--lib` 984, `test_cpu_corpus` 2451,
  `test_corpus_goldens` 53, `test_cost_model` 3), the corpus cpu row 2450, grand total 7842 (Rust
  6040, C++ 277, Python 1525). The Python rows were checked against pytest's collection.
- **Re-proven offline, no VM:** `cargo test --features rust-only -p peacockdb-core` exit 0:
  `--lib` 982 passed, 2 ignored; `test_cpu_corpus` 2451; `test_corpus_goldens` 53;
  `test_cost_model` 3; `test_golden_format` 41; `test_crate_privacy` 1. No warnings, and
  `testdata/` was clean afterwards. `cargo test -p cost-report`: 49, plus 32 in
  `test_source_formatting`. `--features gpu` compiles every target against the local
  rapids-cuda-12.2 env with no warnings. No C++ changed, so no ctest.
- **Python:** `scripts/calibration/tests` 70 passed, each file also exiting 0 run alone as CI runs
  it. Locally this needs matplotlib on `PYTHONPATH`, as CI installs it. `test_duckdb_minimal.py`
  (DuckDB 1.5.4, the local sf1) and `testdata/test_duckdb_result.py` 36. `test_bench_each.py` and
  `test_run_gpu.py` 44. All of `scripts/env/tests`: 234 passed, 3 failed. The 3 need what this host
  lacks, and sf40-baseline touched none of them: `test_the_26_02_image_runs_as_any_user` (no local
  `peacockdb-build:cudf-26.02` image), and `test_every_needed_library_resolves_25_02` and `_26_02`
  (`PEACOCK_STAGED_INSTALLS` unset; the staged installs come from the CPU VM).

## Blocked (2026-11-04, after the rebase)

Still `blocked(building)` at Task 8: the VMs wait on new-hosts' new block, the two VMs' disks
(`new-hosts-detail.md`, "Blocked (2026-11-04, second)"); the PAT block is cleared. DuckDB's sf40
database on nebius-cpu128 needs the bigger disk too.

## Resume after the chain's dependencies (2026-11-05)

new-hosts, pinned-loads and bench-rows are done; this branch is rebased onto bench-rows' `9e1b6bf7`.
Three code stops were resolved (device lists onto bench-rows' check split; the case timeout and answer
switch beside pinned-loads' switches and new-hosts' `--instance-id`; the DuckDB column beside
bench-rows' report guards). `bench-each.sh` now refuses a run without `--instance-id` (exit 2).

Work items handed to this task:
- **`nsys_calls.py` refuses every query with a join** (accepted scope addition; pinned-loads' review):
  its pattern at `scripts/calibration/nsys_calls.py:124` (`execute_\w+\(#(\d+) (\w+)`) does not match
  the `join_build(#N CudfJoin …` / `join_probe` recipes master's join-backend writes, so
  `create_nsys_profile.sh --trace` refuses q3, q8 and every join query. Map `join_build`/`join_probe`
  onto `CudfJoin`, with a test over a join recipe. Pinned-loads derived q3/q8 by hand.
- **The page does not mark unpinned loads** (bench-rows' signoff): every H200 load predates the pinned
  cache; once the H100 joins the page, mark a section run with `pinned_cache=off` (or without it).
- **The H100's stale results**: `~/peacock/peacockdb-beta/testdata/benchmark-results/h100-80gb-hbm3/`
  holds q1/q6/q8 from an older binary and q3 from bench-rows' Task 5. A resumed run now re-measures
  the old-format ones (`holds_measured`), but clear the directory before the first run so nothing
  stale is pulled home.

## build-test.md after the second rebase (2026-11-05)

- **Merged by meaning** from `merge-file --diff3` (ours HEAD, base `9ab0b6e3`, theirs
  `L-pre-rebase2-sf40-baseline`), 10 hunks. Each was resolved as HEAD's text plus sf40-baseline's
  rows. That covers the calibration row (HEAD's memory-kind enum with the device-list reader, the
  joined segments and the publish) and the Images row (`run-gpu.sh`'s case timeout, answer switch
  and answers pulled, beside HEAD's run-image check, switches and `check-build-image.sh`). It adds
  `create_nsys_profile.sh --cases/--device` and `bench-each.sh` there, plus the DuckDB benchmark
  script row, the DuckDB results reader and column, `run`'s env list, `--cases` under Nsight, the
  tpcds `--build-only` line and the page's DuckDB column with HEAD's registry-order rule.
- **`bench-each.sh`'s usage** on the page and in the plan: `--instance-id <id>` is required. Without
  it, exit 2 before any call.
- **Recounted** from `--list` and pytest's collection. cpu 3501 (`--lib` 987, `test_cpu_corpus`
  2451, `test_corpus_goldens` 60, `test_cost_model` 3). The benchmark report page row is 16 (each
  side added 2), corpus goldens 60, calibration 72, Images 250. gpu 2411 re-listed (742, 1009, 659,
  1) with `cpp/build` on `LD_LIBRARY_PATH`. `cpp/install` is from 2026-10-09 and lacks
  `peacock_join_build`, so the binaries do not load against it. Grand total 7868: Rust 6050, C++
  278, Python 1540.
- **Re-proven offline.** `cargo test --features rust-only -p peacockdb-core` exit 0, no warnings,
  `testdata/` clean: `--lib` 985 passed, 2 ignored; `test_cpu_corpus` 2451; `test_corpus_goldens`
  60; `test_cost_model` 3; `test_golden_format` 41; `test_crate_privacy` 1. `cargo test -p
  cost-report`: 49, plus 32. `--features gpu --no-run` through `cargo-cudf.sh` (rapids-cuda-12.2):
  exit 0, no warnings. Python: `test_run_gpu.py` and `test_bench_each.py` 51 passed;
  `scripts/calibration/tests` 72 passed, with each file exiting 0 when run alone (matplotlib from
  `~/.cache/j13-pydeps` on `PYTHONPATH`); `test_duckdb_minimal.py` and `test_duckdb_result.py` 36
  passed (DuckDB 1.5.4, local sf1).

## Before Task 8: the two work items in code (2026-11-05)

- **`nsys_calls.py` checks a join** (`scripts/calibration/nsys_calls.py`, `nvtx_names.py`). The recipes
  declare a join once, `join_build(#N CudfJoin)`, and `recipe_seqs` now reads that form too.
  `NodeSession` names its three calls `N.<i> join_build`, `join_probe` and `join_finish`, and
  `nvtx_names.plan_kind` reads each as `CudfJoin` for the check. `calls.tsv` keeps them apart.
  Test: `test_a_join_s_three_calls_are_its_cudfjoin`, which failed first with "driven but not
  declared: [2]". Mutation: without `plan_kind` it fails with "#2 is CudfJoin in the plan and
  join_finish in the capture". On the real goldens, q3 declares `{5, 10}` CudfJoin and q8 seven
  seqs. `nsys_hbm.py` keys calls by seq and call index only, so it needed nothing.
- **The page marks unpinned loads** (`test_support/benchmark_report.rs`). A timed cell's load reads
  `<ms> unpinned` when the section's `pinned_cache` does not start with `on`, or the section has
  no such line. The intro says what the mark means. Every `h200/` row is marked (26 rows), and the
  page is regenerated. Test: `a_load_without_the_pinned_cache_is_marked_unpinned`. `TIMED` is now
  a pinned section, as one is written today. The two h200 expectations now carry the mark.
  Mutation: inverting the check fails 4.
- `cargo fmt --check` shows drift in 17 files this task never touched (HEAD's, a rustfmt
  difference); the two files changed here are clean.

## Task 8 (2026-11-05)

- **Plan drift:** `/build` does not exist on dev, and `mkdir /build` is refused. The install is
  at `/home/dmitry/build/peacock/sf40-baseline/install`, beside new-hosts' and pinned-loads'.
- `nebius-sync.sh … --no-data` → `peacockdb-peacockdb-beta`; `check-build-image.sh` → the build
  image is this tree's; `stage-install.sh` in the 25.02 build image: exit 0 in 3.5 min, no
  warnings. It staged four rust-tests and the release benchmark. The library exports
  `peacock_join_build` and `peacock_pinned_cache_prefill`. 651 MB. The worktree carries
  pinned-loads: `git log --oneline | grep -c pinned-loads` gives 21.
- **The tpcds database:** 61 GB free after it, out of 213 GB.
  `benchmarks/duckdb_minimal.sh --dataset tpcds --dir /var/lib/peacock --sf 40 --skip-download --duckdb /usr/local/bin/duckdb --build-only`:
  24 tables from the parquet, imported in 269 s, then `row counts verified against the parquet`.
  `/var/lib/peacock/tpcds.sf40.duckdb` is 12.48 GiB. The run timed nothing.
- **Candidates** (Task 9 Step 1): TPC-H 48 lines carry `tp1_single`, not 44. They are q1-q22 and
  26 micro queries, `rollup_stddev` and `global_stddev` among them. TPC-DS has 82. The list is the
  candidate set: 130 cases.

## Task 9 (2026-11-05)

- **Before the run.** Dev's `testdata/calibration/records.tsv`, an ignored leftover of 07:52, was
  moved to `/tmp/Lsurg/s3/stale/dev-records.tsv`, so no segment took it. On the H100,
  `~/peacock/peacockdb-beta/testdata/benchmark-results/h100-80gb-hbm3/` held
  `tpch.sf40/tp1_single.benchmark.txt` with q1, q3, q6 and q8 from older binaries. It was removed
  before the first run, so nothing stale was pulled home. The VM had no `calibration/records.tsv`.
- **Commands as run.** `$G prepare` exit 0 (`tpch.sf40: fetched 0, kept 8`, `tpcds.sf40: fetched 0,
  kept 24`); `$G ship --install /home/dmitry/build/peacock/sf40-baseline/install` exit 0; then
  `scripts/env/bench-each.sh $VM --cases peacockdb-core/tests/common/corpus_benchmark_cases.inc --mode tp1_single --timeout 1800 --records /tmp/Lsurg/sf40run/records`,
  detached on dev, 11:25–12:30. Exit 0: `130 cases at tp1_single, every one with its section`. No
  interruption, no rerun. `records/device` reads `h100-80gb-hbm3`.
- **Wall time per query** (`run-gpu.sh exited … after <s> s`): 130 cases, 2389 s in all; the
  longest is tpch filter_project at 133 s. Each figure includes the container start, the prefill
  and the ten rounds. tpch q1 19; tpch q2 28; tpch q3 17; tpch q4 17; tpch q5 16; tpch q6 13; tpch q7 20; tpch q8 22; tpch q9 39; tpch q10 19; tpch q11 29; tpch q12 13; tpch q13 11; tpch q14 14; tpch q15 17; tpch q16 27; tpch q17 20; tpch q18 20; tpch q19 16; tpch q20 33; tpch q21 23; tpch q22 12; tpch aggregate_groupby 12; tpch anti_join 29 (exit 1); tpch anti_join_reorder 8; tpch cross_join 7; tpch distinct_functions 19; tpch filter_project 133; tpch hash_join 21; tpch join_int 8; tpch left_join 18; tpch left_join_fixed 18; tpch left_join_reorder 16; tpch mixed_join 17; tpch nested_limits 9; tpch nested_loop_join 7; tpch nested_loop_left_join 7; tpch rollup_distinct 30; tpch rollup_grouping 11; tpch rollup_over_join 8; tpch scan_limit 13; tpch semi_join 79; tpch semi_join_reorder 9; tpch shuffle_additive 13; tpch shuffle_additive_avg 13; tpch shuffle_stddev 13; tpch rollup_stddev 16; tpch global_stddev 11; tpcds q1 9; tpcds q2 14; tpcds q3 10; tpcds q4 46; tpcds q5 22; tpcds q6 20; tpcds q7 11; tpcds q8 12; tpcds q9 18; tpcds q10 18; tpcds q11 38; tpcds q13 13; tpcds q14 41; tpcds q15 13; tpcds q16 16; tpcds q17 7 (exit 1); tpcds q18 14; tpcds q19 13; tpcds q21 7; tpcds q22 6; tpcds q23 40; tpcds q24 19; tpcds q25 6 (exit 1); tpcds q26 12; tpcds q28 15; tpcds q29 9 (exit 1); tpcds q30 12; tpcds q31 21; tpcds q32 18; tpcds q33 19; tpcds q34 16; tpcds q35 19; tpcds q37 15; tpcds q38 20; tpcds q39 11; tpcds q40 23; tpcds q41 12; tpcds q42 10; tpcds q43 10; tpcds q45 9; tpcds q46 27; tpcds q48 15; tpcds q50 20; tpcds q52 17; tpcds q54 22; tpcds q55 12; tpcds q56 21; tpcds q58 20; tpcds q59 14; tpcds q60 24; tpcds q61 16; tpcds q62 7; tpcds q64 67; tpcds q65 22; tpcds q66 18; tpcds q68 13; tpcds q69 17; tpcds q71 17; tpcds q73 19; tpcds q74 23; tpcds q75 27; tpcds q76 18; tpcds q77 18; tpcds q78 29; tpcds q79 12; tpcds q80 24; tpcds q81 14; tpcds q82 16; tpcds q83 9; tpcds q84 9; tpcds q85 10; tpcds q87 18; tpcds q88 20; tpcds q90 9; tpcds q91 7; tpcds q92 9; tpcds q93 16; tpcds q94 9; tpcds q95 13; tpcds q96 10; tpcds q97 16; tpcds q99 11.
- **Skips** (`status=failed`; each a `skipped` row in `cases.csv`; tickets per review round 1):

| dataset | query | reason |
|---|---|---|
| tpch | anti_join | `call failed: GpuUnload lane 0: the exported stream is not the sink's rows: … expected Utf8 but found LargeUtf8 at column index 8 (declared vs exported: 8 o_comment: Utf8 vs LargeUtf8)`. At sf40 the export's `o_comment` passes 2 GiB of string bytes and comes back as LargeUtf8. A production failure: [#307](../tickets/system-hardening.md#t307), filed in review round 1. |
| tpcds | q17 | `this mode refuses it: invalid plan: what the accumulators hold and cannot be holding less of (2593119555 bytes) is already the whole budget (2147483648)` |
| tpcds | q25 | the same refusal, 3146023251 bytes |
| tpcds | q29 | the same refusal, 2593120563 bytes |

  No section reads `reason=pinned cache: …`. The three tpcds refusals come from the planner, before
  any device work, at tp1_single's 2 GiB budget.
- **Cache** (Step 4). Every timed section reads `pinned_cache=on … source=default` (tpch 47, tpcds
  79). None has `prefill_files=0`. The largest `resident` is 39.1 GiB in tpch (q9) and 14.6 GiB in
  tpcds (q75). The largest `prefill_bytes` is 41947566081 (39.07 GiB) in tpch q9, `prefill_us`
  20823061, and 15625613567 (14.55 GiB) in tpcds q80, `prefill_us` 7644016. The sum of
  `prefill_us` is 249.5 s for tpch and 295.2 s for tpcds.
- **The protocol note.** peacockdb's warm numbers read files resident in pinned host memory,
  prefilled once per query process before any round. DuckDB's read through its buffer manager
  over a warm page cache. Neither counts the first read from disk. The prefill's bytes and time
  (`prefill_bytes`, `prefill_us`) are reported apart.
- **`cases.csv`** (Step 5). `the_h100_list_is_the_lists_measured` is in `benchmark_lists.rs`, with
  `H100` and `H100_MODES` beside `OTHER_ENGINES`. The rows come from the main list's `tp1_single`
  lines, ordered as the registry orders them (`registry_cases`, now `pub(crate)`). The page uses
  that order, and the list is written from it and held to it. `macro_invocations_in` no longer
  exists since the inventory port. The file is written through a new delegate,
  `test_support::write_device_cases`, staged and renamed.
  - Red: "… cases.csv does not exist beside its tree — write it with UPDATE_CANONICAL=1 …".
  - Written: 130 rows (126 `ok`, 4 `skipped`), in the list file's own order (diffed).
  - Mutation: a tampered row fails naming its line.
- **The page.** Regenerated: an h100-80gb-hbm3 column group beside h200, and 142 rows. Only the
  H200's 26 loads carry `unpinned`.
- **The record** (Step 6). `join_records.py --record <dir>/*.tsv`, as build-test.md and the plan
  give it, failed: `--record` was `append`, one path per flag. It is now `extend` with
  `nargs="+"`, so both forms work. Test: `test_one_record_flag_takes_every_segment`, which failed
  first with "unrecognized arguments". Then the run gave `126 cases from 126 segments`, with
  allocator `of 78.7GiB free`. `split_record.py --cases …/cases.csv` printed
  `h100-80gb-hbm3/tpch.sf40` and `…/tpcds.sf40` (7850 and 48220 rows). `publish_record.sh` ran
  with dev's `peacock.env` sourced, uploading `3b17277d…` (tpch) and `52d700b1…` (tpcds) and
  pinning both. `fetch_record.sh` then reported each `as pinned`.
- **Gates** (Step 7): `test_corpus_goldens` 61 passed. `test_cpu_corpus -- sf40` waits for Task 11.

## Task 10 (2026-11-05)

- **Commands as run.** Each was run from a cases file per dataset (`/tmp/Lsurg/sf40run/<ds>.csv`:
  the header and that dataset's rows):
  `PCK_BENCH_HBM_SET=gh100 PCK_BENCH_HBM_PEAK_BW=3.35e12 PCK_BENCH_HBM_FREQ=10000 scripts/create_nsys_profile.sh [--trace|--metrics] --cases <ds>.csv --device h100-80gb-hbm3 --host nebius-h100 --ssh "$H rsh" --remote-dir /home/dmitry/peacock/peacockdb-beta --remote-cudf-root /home/dmitry/peacock/cudf-25.02`.
  The install shipped in Task 9 was still on the H100's disk. The records came from Task 9's
  publish (`calibration/h100-80gb-hbm3/<ds>.sf40/records.tsv`). Each trace pass took under
  20 min, so none was split.
- **Three fixes, each test-first** (`scripts/calibration/`):
  - **`nsys_calls.recipe_seqs` missed a golden's first section.** It looked for `"\n== <q>\n"`,
    and a plans golden opens on `== aggregate-groupby` with nothing above it. The tpch trace
    pass stopped there. The fixtures all opened with a header line, so no test saw it. The fix
    puts a newline before the text. Test: `test_the_golden_s_first_section_is_found`. The tpch
    capture was home, so `calls.tsv` was re-derived from it on dev, with no second trace.
  - **The metrics pass's sampling.** At the default 20 kHz (the H200's), tpch q1 read "a sample
    reports 101% of peak". At 10 kHz q1 passed, then q2 reported one gap. At 5 kHz the same gap
    remained, so frequency was not its cause. In q2's capture on the host the gap ran 21.94–23.50 s,
    after the last NVTX range had ended at 20.19 s. That is the process's teardown, likely freeing
    its 147 GiB pinned cache, which no H200 run had. `nsys_hbm.py` now counts only gaps that
    overlap our ranges' span; a capture with no range is refused by `cases_in`, as before. Tests:
    `test_a_gap_after_the_last_range_is_no_stall`, which failed first, and
    `test_a_gap_inside_the_case_is_refused`. Mutation: without the span condition the first
    fails. Both passes ran at `PCK_BENCH_HBM_FREQ=10000`. The 101% at 20 kHz was not looked
    into further.
  - **`nsys_calls.py` and joins**, before this task (above).
- **tpcds' sf40 plans are not sf1's for 14 queries.** The tpcds trace pass over all 79 cases
  stopped at q18: "capture does not match q18's recipes". Its sf40 tree has 33 nodes with the
  joins reordered; the sf1 golden has 32. The call breakdown checks a capture against
  `goldens/<ds>.sf1/`, on the premise that "a plan's recipes are its shape and do not depend on
  how much data it reads". That does not hold at sf40, where statistics change the join order.
  Checked case by case over the 79-case capture (`/tmp/Lsurg/sf40run/diag_recipes.py`), 14
  differ: q18, q30, q31, q32, q33, q56, q60, q61, q71, q81, q83, q85, q91 and q92. The other 65
  match, and so do all 47 tpch cases.
  - **What was done.** The trace pass ran again over those 65 (`tpcds65.csv`), so tpcds'
    `calls.tsv` covers 65 cases. Its `loads.tsv` (all 79 cases) and `hbm.tsv` (all 79) come from
    the full passes; neither reads a plan golden.
  - **The 14 have no call breakdown.** They are the "case whose capture failed" of the spec, with
    this reason. Checking an sf40 capture against sf40 plans needs a plan rendered at sf40, which
    the goldens do not hold. That is the coordinator's call, not a ticket: it is calibration,
    not production behaviour.
- **Files** (Step 3): `testdata/calibration/h100-80gb-hbm3/{tpch,tpcds}.sf40/{calls,loads,hbm}.tsv`:
  - tpch: calls 5718 rows, loads 131, hbm 786 (785 calls from 47 parts; 5 calls under 10 samples).
  - tpcds: calls 26215, loads 756, hbm 4823 (4822 calls from 79 parts; 20 calls under 10 samples,
    0.73% of bytes).
  - `testdata/.gitignore` re-includes a device directory's three derived files and keeps its
    fetched `records.tsv` ignored.
  - The H200's `testdata/calibration/{calls,hbm}.tsv` were restored by `git show HEAD:…`.
  - Captures stay in scratch: `/tmp/Lsurg/sf40run/capture-tpcds.sqlite` (79 cases) and
    `testdata/calibration/capture.sqlite` (ignored; tpcds, 65 cases).
- **The load split** (Step 4, from `loads.tsv`; `host_read_us` is 0 throughout, since every load
  read pinned memory). Reading and copying against decoding:

| dataset | loads | Σ load_us | read + h2d | decompress + decode | other | unattributed |
|---|--:|--:|--:|--:|--:|--:|
| tpch | 130 | 9046567 | 1581547 (17.5%) | 5486728 (60.6%: 4105586 + 1381142) | 152256 (1.7%) | 1994566 (22.0%) |
| tpcds | 755 | 27330684 | 3158590 (11.6%) | 18437202 (67.5%: 14248162 + 4189040) | 334129 (1.2%) | 6217213 (22.7%) |

## Task 11 (2026-11-05)

- **The run** (Step 1). `nebius-sync.sh … --no-data` → `peacockdb-peacockdb-beta`. `sha256sum` of
  `cases.csv` agreed on dev and the VM (`c1b2d405…`), and so did `duckdb_minimal.sh`'s
  (`7da85a9d…`). Then, detached on the VM, per dataset:
  `benchmarks/duckdb_minimal.sh --dataset <ds> --dir /var/lib/peacock --sf 40 --skip-download --duckdb /usr/local/bin/duckdb --cases testdata/benchmark-results/h100-80gb-hbm3/cases.csv --answers /var/lib/peacock/answers-<ds>.duckdb --results /tmp/duckdb-<ds>.tsv`.
  tpch ran 14:18–14:29 and tpcds 14:29–14:38, both exit 0. Each database was 100.0 % resident
  (tpch 32.73 GiB, tpcds 12.48 GiB; "bounded dd read + absolute-Cached", vmtouch not installed).
  Every row is `warm` `yes` and `ok` (tpch 47, tpcds 79). The preamble reads
  `duckdb=1.5.4 threads=128 … cpu=AMD_EPYC-Genoa_Processor cores=128 memory_gib=503.7`. The TSVs,
  logs and answers databases were copied to `/tmp/Lsurg/sf40run/duck/`.
- **Rendering** (Step 2). `duckdb_result.py --answers` was killed (137) on tpch after
  `distinct-functions`. `filter-project`'s answer is 96008105 rows, and the renderer built the
  whole table before measuring it against the cap, beyond dev's 31 GiB. This task's script is
  fixed:
  - An answer over the cap by its row count alone is never rendered. Each line is as wide as the
    header at least, and there are rows + 4 of them, so the count settles it.
  - Such an answer is fingerprinted in batches by `fingerprint_stream`. Its exact cells go
    through `sort -z` in the C locale, and its fingerprint lines are `fingerprint_text`, which
    both writers share.
  - Tests, both failing first: `test_a_streamed_fingerprint_is_the_in_memory_one` and
    `test_an_answer_over_the_cap_by_its_row_count_is_fingerprinted_unrendered`. Mutation: with
    the count check off, the second fails.
  - tpch then rendered in 8 min 43 s, with a peak RSS of 13.5 GB: 47 sections, 9 fingerprints.
    `filter-project`'s hash `fc52384f…` is the H100's.
  - tpcds was rendered again with the fix: byte for byte the first render (79 sections, 3
    fingerprints, 2 of them now streamed).
  - The page was regenerated with the `duckdb-cpu128` column, and its intro names DuckDB 1.5.4,
    128 threads and the host.
- **Compare** (Step 3), `every_sf40_device_answer_meets_duckdb`. Nine `ok` rows failed at first:
  - **Seven were fingerprints the oracle would not compare**: tpch q20, anti_join_reorder,
    left_join_fixed and semi_join_reorder; tpcds q34, q64 and q71. At sf40 their answers pass the
    cap, but their sf1 lines do not declare `gpu: fingerprint`, and `compare_device_section`
    refuses an undeclared fingerprint. Read by hand, each device fingerprint is identical to
    DuckDB's: rows, columns and hash. The defect was this task's own `sf40_verdict`, so no
    ticket: no registry line is written for sf40, and the cap is passed there by scale. It now
    compares through `Side::Scaled` and `compare_scaled_section`: by fingerprint when the
    section holds one, as rows otherwise. The sf1 path keeps its two-way declaration guard.
    Tests: `an_sf40_section_over_the_cap_is_compared_by_its_fingerprint` and
    `an_sf40_section_of_rows_is_compared_as_rows_under_either_line`, which failed first
    (unresolved import). Mutation: always comparing rows fails the first.
  - **Two were ties under `no_sort`**: tpcds q18 (`… LIMIT 100`) and q73. Both sides hold the same
    rows: q18's 100 agree as a set at 4 dp, and q73's sorted rows have the same md5. Only rows
    tying on every `ORDER BY` key swap, counting rows from 1: q18 at 24/25 (the `AAAAAAAAAACJAAAA`
    pair) and 61/62 (the `AAAAAAAAAAHHAAAA` pair), rollup groups whose other keys are null; q73 at
    16/17 (Hamilton: Nicholas, Harry) and 22/23 (Jones: Cynthia, Robert), all with `cnt` 5. None
    is at q18's LIMIT edge. (First written 0-based, with the second pair of each missed; corrected
    in review round 1.)
    The tie rule is the oracle's: `no_sort` is `same_sequence`, row by row with no tie rule, and
    its own comment says no sf1 answer tied. Per the plan, that is a ticket against the oracle,
    never an operator one: **#306** in `tickets/testinfra.md`. Both rows are `wrong`, note
    `#306`, set by hand. `the_h100_list_is_the_lists_measured` passes, carrying the mark. The
    page now says `wrong answer #306` twice. `tickets.md` was recounted: 90 open, testinfra 4,
    next free 307.
  - The compare then passed. `cases.csv` holds 124 `ok`, 4 `skipped` and 2 `wrong`.
  - The registry did not change, so `ticket_widget.py` was not run.
- **Gates** (Step 4), every VM stopped:
  - `cargo test --features rust-only -p peacockdb-core` exit 0, no warnings: `--lib` 988 passed
    and 2 ignored, `test_corpus_goldens` 61, `test_cost_model` 3, `test_cpu_corpus` 2451,
    `test_crate_privacy` 1, `test_golden_format` 41. `cargo test -p cost-report`: 49, plus 32 in
    `test_source_formatting`.
  - Python: `scripts/calibration/tests` 77 passed, each file exiting 0 when run alone;
    `test_bench_each.py` and `test_run_gpu.py` 51.
  - `testdata/test_duckdb_result.py`, `benchmarks/tests/test_duckdb_minimal.py` and
    `scripts/test_ticket_widget.py` (run together) failed **one test once**, while the
    background cargo suite was building. Its name was not captured. Six later runs passed 49/49,
    one of them beside a running `test_cpu_corpus`. It is unidentified, not proven flaky, and not
    disabled.
  - `build-test.md` was recounted for this commit: cpu 3505 (`--lib` 990, `test_corpus_goldens`
    61). The rows are DuckDB oracle 49, DuckDB result rendering 29 and calibration 77. The grand
    total is 7879: Rust 6054, C++ 278, Python 1547.

## The coordinator's decisions (2026-11-05, after 30a6165e)

- **The 14 tpcds queries without a call breakdown are listed and left**: q18, q30, q31, q32, q33,
  q56, q60, q61, q71, q81, q83, q85, q91 and q92. At sf40 the planner orders their joins
  differently from the sf1 plan goldens that `nsys_calls.py` checks a capture against, so their
  captures cannot be checked. No sf40 plan goldens are made. build-test.md says so in
  "sf40 baseline" and in the `calls.tsv` bullet of "Benchmark data flow".
- **The one unidentified Python failure** (Task 11, Step 4) is recorded as seen once and not
  reproduced in six reruns. It is not disabled and has no ticket.

## The L40S list (2026-11-05): blocked on nebius-gpu's disk

The plan's one L40S correctness run, after the last harness change, did not happen.
`scripts/nebius.sh start` at 15:29 and `nebius-sync.sh` went fine. Then `df -h /` gave **4.9 GB
free of 96 GB** (91 GB used), and build-test.md's "Disk" says not to build below 10 GB.

- **Tried:** `~/miniforge3/bin/conda clean -a -y` freed nothing; its 13 GB `pkgs/` is linked into
  the envs.
- **What holds the disk:**
  - `~/peacockdb-peacockdb-beta` 19 GB, this tree's: `target-cudf-rapids-cuda-12.2` 13 GB, built
    today at 04:59 by this tree's earlier tasks, so warm and not stale; `cpp/install` 4.8 GB.
  - `~/peacockdb-K` 18 GB: chain K's tree, not this task's to remove.
  - `~/miniforge3` 17 GB; `/usr` 14 GB; `/opt` 4.5 GB.
  - With `incremental/` (1.9 GB) dropped as well, the disk would still be under 10 GB.
- **Removed:** nothing.
- **Stop:** the VM was stopped softly at 15:32 (`STOPPED`).
- **What it needs:** room on the shared disk, the human's or chain K's call. Or a decision to
  rebuild this tree's 13 GB target dir cold.

## Suites

Each suite this task ran, and where:

| suite | where | CI |
|---|---|---|
| `cargo test --features rust-only -p peacockdb-core` (all targets, goldens included) | dev | dataset-matrix |
| `cargo test -p cost-report` (`test_source_formatting` too) | dev | cost-report |
| `--features gpu --no-run` through `cargo-cudf.sh` (rapids-cuda-12.2); `--list` of the gpu binaries against `cpp/build` | dev | shad-gpu / nebius-gpu |
| `scripts/calibration/tests/test_*.py` (77; each file alone) | dev, matplotlib on `PYTHONPATH` | cost-report job's calibration step |
| `testdata/test_duckdb_result.py` (29; the five that render need the wheel and `tpch.sf1`) | dev | its row in build-test.md; the rendering five skip in CI |
| `benchmarks/tests/test_duckdb_minimal.py` (9; DuckDB 1.5.4 and the local sf1) | dev | **none yet**: a new directory no CI job runs; containers-only adds it to the image's script step |
| `scripts/env/tests/test_bench_each.py`, `test_run_gpu.py` (51) | dev | **none**: `scripts/env/tests` runs on dev only until then |
| `scripts/test_ticket_widget.py` | dev | as before |
| the H100 benchmark (`bench-each.sh`), both Nsight passes | nebius-h100 | none (by hand) |
| `duckdb_minimal.sh --cases` | nebius-cpu128 | none (by hand) |
| the L40S list | not run: nebius-gpu's disk, above | gpu-tests |

Coordinator decision (2026-11-05): the plan's L40S correctness run is taken by CI's `GPU Tests (nebius-gpu)`
dispatch at `completeness approved`, which runs the same L40S list from binaries built on the runner and
shipped, so it needs no build on nebius-gpu's full disk (4.9 GB free; chain K's tree and this tree's
target dir hold it). Freeing that shared disk is the human's.
- 2026-11-05: green (Task 12) at the reviewing commit; PR #198 (base ENS-bench-rows); reviewer round 1.

## Review round 1 (2026-11-05, PR #198)

- **Three stale statements reworded.** In build-test.md "the committed tree is `h200/`'s alone"
  now says the `h100-80gb-hbm3/` tree passes both checks, so the literal is what shows them
  refusing; `benchmark_checks.rs`' module comment likewise. "Only `--pull-benchmarks` writes the
  bucket" now names `publish_record.sh` too.
- **`compare_scaled_section`** compares by fingerprint when either side holds one. Before, a
  DuckDB-only fingerprint (DuckDB's longer floats near the cap) fell to `compare_sections` and
  read as a false `wrong`. Test: `an_sf40_section_of_rows_meets_a_fingerprint_on_duckdb_s_side`.
  It failed first with "the section is fingerprinted (over the cap) — declare
  duckdb_fingerprint". No committed answer was affected: the compare passed before and after.
- **Comment caps.** `benchmark_case`'s doc is down from 15 lines to 8 and `benchmark_report.rs`'s
  module doc from 11 to 8, each pointing at build-test.md.
- **#307** (system-hardening): tpch `anti_join`'s LargeUtf8 export, a production refusal. It is in
  the file's Contents and `tickets.md` (91 open; system-hardening 10; next free 308). build-test.md's
  skip line and the skip table above name it. `cases.csv` does not: a `skipped` row's note is the
  tree's failure reason, which `the_h100_list_is_the_lists_measured` writes, so a hand edit there
  fails the test.
- **#306** rewritten to the asked shape (15 lines): both tied pairs of each query, rows counted
  from 1 — q18 24/25 and 61/62, q73 16/17 and 22/23. The Task 11 entry is corrected to match.
- **The page.** A DuckDB time whose `warm` verdict is not `yes` adds `not warm: <verdict>`. Test:
  `a_duckdb_time_not_warm_says_so`, which failed first. Mutation: ignoring the verdict fails it.
  The no-op `rowspan="1"` is gone. The committed page changed by those two attributes only; every
  DuckDB row is warm `yes`.
- **Counts:** `--lib` 992 (`--list`), DuckDB oracle 50, report page 18. cpu 3507, grand total 7881:
  Rust 6056, C++ 278, Python 1547.
