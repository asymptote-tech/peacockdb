# sf40-baseline implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the H100's sf40 tp1_single tree for the main case list's tp1_single cells (TPC-H 44:
q1-q22 and the micro queries; TPC-DS 82) in bench-rows format with pinned-loads in the build,
both Nsight passes' derived files per dataset, DuckDB 1.5.4's warm 2nd-min times for the same
queries on nebius-cpu128, each H100 answer held to DuckDB's at sf40, the H100 tree's
`cases.csv`, written from that tree, as the one ordered query list, and bench-rows' report page
showing the H100 tree with a DuckDB column.

**Architecture:** one benchmark process per query (`scripts/env/bench-each.sh` over `run-gpu.sh run
bench`, always `PEACOCK_BENCH_RESUME=1`), so no process nears the 4-hour cap and a crashed case
cannot poison the next one's CUDA context. Each segment's record is pulled and set aside;
`join_records.py` joins them and `split_record.py` cuts the result for publishing. The harness
records the chosen run's answer under `PCK_WRITE_GPU_RESULT` into `goldens/<ds>.sf40/gpu-result.txt`;
DuckDB's sf40 answers come from `duckdb_minimal.sh`'s threads=1 pass, stored as tables in an
answers database and rendered by `duckdb_result.py`. A Rust test compares the two under the
corpus's own `duckdb_oracle` rules. The H100 tree passes the tree check by containers' rule: a
device tree that holds a `cases.csv` is held to it (containers-rebase Task 4); no exception names
the H100. That `cases.csv` is also the ordered list every consumer reads (query-fuser, tp4-sized,
batching-report): `the_h100_list_is_the_lists_measured` writes it from the tree under
`UPDATE_CANONICAL=1` and checks it otherwise. No other list file exists. Each format has one
owner reader in Rust, which scripts' readers are tested against through a shared fixture: the
device list (`parse_device_cases`) and DuckDB's results (`parse_duckdb_results`, new here, which
also gives bench-rows' page its DuckDB column).

**Tech stack:** Rust (`peacockdb-core` test_support and test binaries), bash, Python 3 stdlib
(+ duckdb 1.5.4 wheel on dev for rendering), DuckDB CLI 1.5.4, Nsight Systems, the containers
run/build images.

**Spec:** [`sf40-baseline.md`](sf40-baseline.md). Base: `ENS-bench-rows` (chain L: containers-rebase
→ new-hosts → pinned-loads → bench-rows). Branch: `ENS-sf40-baseline`. Siblings consumed exactly:
[`new-hosts-impl.md`](new-hosts-impl.md) (`run-gpu.sh` over `scripts/nebius.sh`, `fetch-sf40.sh`,
`install-duckdb.sh`, `stage-install.sh`, `create_nsys_profile.sh --ssh`),
[`bench-rows-impl.md`](bench-rows-impl.md) (`parse_benchmark_section`, `BenchSection`,
`PREDATES_ROWS`, `rows_problem`, `render_benchmark_report`, `ReportCell`, `the_benchmark_report_matches_the_trees`),
[`pinned-loads-impl.md`](pinned-loads-impl.md) (`nsys_loads.py`, `loads.tsv`),
[`containers-rebase-impl.md`](containers-rebase-impl.md) (case list from master's registry,
`PREDATES_PROVENANCE`, `DEVICE_LIST`, `device_list_files`, the compiled-in `CASE_LIST` and
`macro_invocations_in`, and Task 4's `DeviceCase`,
`CaseStatus`, `device_cases_path`, `parse_device_cases`, `render_device_cases`). Line numbers are `origin/ENS-containers`'s for
harness files and master's for the rest; find each by the function named beside it.

## Global Constraints


- **VM time is minimal** (the human's rule, 2026-11-03). A VM starts right before its first
  remote command and stops right after its last `pull`. It is never left up across a wait: a local
  build, CI, a review, the human. Binaries are built and staged first, and the H100 starts only
  once they are ready to ship. No step leaves a VM running for a later task. Each start and stop
  goes into the detail file's `## VM time` table, in the host's local time, with the hours per VM
  for the task. The watchers (1 h idle, 4 h per process, the hourly GitHub check) are the backstop,
  not the plan. Order: the CPU-VM build first (Task 8), the CPU VM stopped; the H100 starts for
  the run and stops after it; local cargo work (the `cases.csv` test, the page) runs with every VM
  stopped; the H100 starts again for the profiles and stops after them; the CPU VM starts again
  for DuckDB. Never a cargo edit, build or test with a VM up.
- **Developers never commit, push, or dispatch a workflow** (`prompts.md`). A step that says
  *Commit `<msg>`* means: report a checkpoint `<msg>` and stop; the coordinator stages, commits and pushes, then
  dispatches the next step. Only the plan's last step is green. A step marked *(coordinator)* is the coordinator's; the developer stops before it
  and says what it needs. Files "kept for a later commit" stay
  uncommitted in the chain workspace; the developer never runs `git add`.
- **What runs on nebius-cpu128 comes from the chain workspace as it stands, uncommitted, never a
  `git pull`** (new-hosts' "CPU-VM build", in new-hosts' Global Constraints):
  `D=$(scripts/nebius-sync.sh --instance-id computeinstance-e01t2mm4265y8x88ey --profile eu-west1 --no-data)`
  after `$C start`, then every command in `~/$D`. That carries this task's own scripts
  (`duckdb_minimal.sh`'s new flags) and the H100's `cases.csv` before either is committed. A file a
  VM step reads is checked first: `sha256sum` on dev and over `$C ssh` agree.
- Host roles (`build-test.md`, "Nebius host", host roles): the H100 runs the sf40 benchmark and
  Nsight through `run-gpu.sh`; the CPU VM runs DuckDB and stages the H100's install. The L40S
  runs its correctness list once, bracketed as `prompts.md` says, after the last harness change
  (this task edits `corpus_benchmark.rs` and `corpus_gpu.rs`, which that list runs): the
  Definition of done's "GPU tests green on nebius-gpu's L40S list". No sf40 there.
- Visibility (`coding-style.md`, `cost-report/tests/test_source_formatting/visibility.rs`): no
  `pub(super)`, no `pub use`. Shared items inside a component are `pub(crate)`; what a test binary
  reaches is declared in `test_support/mod.rs` (types there, functions as one-expression `pub fn`
  delegates).
- Candidates: every line of the main case list `peacockdb-core/tests/common/corpus_benchmark_cases.inc`
  carrying `tp1_single`, in file order: TPC-H 44 (q1-q22 and the micro queries), TPC-DS 82. The
  batching set (query-fuser) is TPC-H q1-q22 and every TPC-DS line.
- A query failing at sf40 (error, OOM, timeout) is skipped: a failed section in the tree (the harness's `FAILED`,
  `status=failed reason=<one line>`, `test_support/mod.rs:1011`) with its reason, a `skipped`
  row (no modes, the reason as its note) in the H100 `cases.csv`, a line in the detail file. Not a ticket unless a wrong answer.
- The H100 list, `testdata/benchmark-results/h100-80gb-hbm3/cases.csv`, header
  `dataset,sf,query,modes,status,note`, containers-rebase Task 4's format. It is read only
  through `parse_device_cases` (Rust, the format's owner) and `scripts/calibration/device_cases.py`
  (every script: `bench-each.sh --within`, `duckdb_minimal.sh --cases`, `create_nsys_profile.sh
  --cases`, `split_record.py --cases`); both are tested against `testdata/fixtures/device-cases.csv`.
  A reader takes `ok` and `wrong` rows as cases; a `skipped` row is no case. The main list
  `corpus_benchmark_cases.inc` stays in macro form and is read as containers-rebase left it:
  compiled in (`CASE_LIST`), never by a path into `tests/` (`no_core_test_reads_the_checkout`).
- DuckDB's results, `duckdb-cpu128/<ds>.sf40/duckdb.tsv`: written by `duckdb_minimal.sh --results`,
  read by `test_support::duckdb_results::parse_duckdb_results`, the format's owner; its fixture
  `testdata/fixtures/duckdb-results.tsv` is what batching-report's Python reader is tested against.
- bench-rows' page `testdata/benchmark-report.html` is regenerated in every commit that changes
  a results tree (`the_benchmark_report_matches_the_trees` fails otherwise).
- Device tree name `h100-80gb-hbm3`; DuckDB tree `duckdb-cpu128`.
- DuckDB protocol (unchanged from `duckdb_minimal.sh`): native storage, warm page cache with
  residency checked, all cores (no `--threads`), 2nd-min of 6. Pinned `DUCKDB_VERSION=1.5.4`.
- **The pinned host file cache** (pinned-loads; `build-test.md` beside `PEACOCK_RMM_POOL_BYTES`)
  is on by default on the H100: `min(host RAM, the cgroup limit) × 3/4`, about 147 GB (the run container
  sets no limit). It lives per process, and under `bench-each.sh`'s one process per query each
  process **prefills that query's files once**, before its case (pinned-loads Task 4b: the process
  plans the case its `PCK_TEST_FILTER` selects, prefills `plan_files` of it, then runs the warm-up
  and the measured rounds, all of which read the pinned buffers). The prefill is the only read
  from disk; it runs inside the case (inside `measure`, the future `measure_into` records), so it
  counts against `PEACOCK_BENCH_CASE_TIMEOUT_S` and a refusal is always written. A query
  whose files exceed the limit is a failed section `status=failed reason=prefill: needs <needed> >
  limit <limit>`, skipped as any failure is (its `cases.csv` row `skipped` with that reason); a
  round that filled or fell back is `status=failed reason=pinned cache: … during the rounds`, which
  is a defect to explain, not a skip. The baseline never sets `PEACOCK_PINNED_HOST_CACHE_BYTES`.
  Every timed section's trailer records the cache and the prefill (bench-rows' one definition:
  `pinned_cache=`, `prefill_files=`, `prefill_bytes=`, `prefill_us=`): Task 9 Step 4 checks each
  timed section for `pinned_cache=on` and `prefill_files` > 0, and names any other in the detail
  file with its value. **The protocol note** (detail file and build-test.md): peacockdb's warm
  numbers read files resident in pinned host memory, prefilled once per query process before any
  round; DuckDB's read its buffer manager over a warm page cache; neither counts the first read
  from disk, and the prefill's bytes and time (`prefill_bytes`, `prefill_us`) are reported apart.
- The H100's watcher stops one process at 4 h; segments resume with `PEACOCK_BENCH_RESUME=1`.
  On the CPU VM only `duckdb`/`aws` processes and terminals count as use (new-hosts Task 3): long
  work there runs as the `duckdb` CLI, never as `python3`.
- H100 HBM: `PCK_BENCH_HBM_SET=gh100`, `PCK_BENCH_HBM_PEAK_BW=3.35e12`.
- Hosts: `VM="--instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu"` (nebius-h100), `G="scripts/env/run-gpu.sh $VM"`,
  `H="scripts/nebius.sh $VM"`, `C="scripts/nebius.sh --instance-id computeinstance-e01t2mm4265y8x88ey --profile eu-west1"`.
  Every ssh from the Bash tool needs the sandbox off. Both VMs stopped when the run ends.
- Never open `~/.nebius/*` or any `*.pem`. Captures (`*.nsys-rep`, `*.sqlite`) are never committed.
- Gates at every commit: `cargo test -p peacockdb-core --features rust-only --lib`, `--test
  test_corpus_goldens`, `--test test_cpu_corpus`, `--test test_golden_format`;
  `cargo test -p cost-report --test test_source_formatting`; every changed Python suite
  (`benchmarks/tests/test_duckdb_minimal.py` included).

## Review Focus

1. **A hung sf40 query** (a nested-loop plan at 40× the rows) holds the H100 until the watcher's
   cap and leaves no section. → Task 3, `a_case_past_its_timeout_is_recorded_failed`; Task 6,
   `test_bench_each_skips_a_failed_case_on_rerun`.
2. **Writing one sf40 answer drops the others**: `merged_cells` keeps only cells the registry
   enables at the file's sf, and the registry has no sf40 rows. → Task 3,
   `an_sf40_answer_keeps_the_other_sf40_answers`.
3. **A segment killed between its rows and its section**: the case is measured again, so its rows
   are in two records, and the killed run's record still sits on the host, where `run-tests.sh`
   refuses the rerun. → Task 6, `test_bench_each_sets_aside_a_record_a_killed_run_left`,
   `test_join_takes_the_later_segment_of_a_case`, `test_join_orders_segments_by_attempt_not_argv`.
4. **The DuckDB tree read as a device tree** — held to the whole case list, red. → Task 2,
   `the_duckdb_tree_is_no_device_tree`.
5. **A wrong-answer mark that outlives its fix** — a `wrong` row must still disagree with
   DuckDB, an `ok` row must agree. → Task 4, `a_stale_wrong_mark_is_refused`.
6. **A wrong-answer mark lost when the list is rewritten from the tree** — the tree knows no
   tickets, so the writer carries each `wrong` row's note over from the file as it stands. → Task 1,
   `a_rewrite_keeps_a_wrong_answer_mark`.
7. **The scripts' reader of `cases.csv` drifting from Rust's** (a comma in a reason, a `wrong`
   row read as no case). → Task 1, `the_shared_fixture_reads_as_expected` in both languages,
   over one committed fixture.

---

### Task 1: the H100 list's rows, written from the tree; the scripts' reader

**Files:**
- Modify: `peacockdb-core/src/test_support/device_cases.rs`, `…/device_cases/tests.rs`
  (containers-rebase Task 4's; the writer joins the reader)
- Create: `testdata/fixtures/device-cases.csv` (and its row in `testdata/fixtures/README.md`)
- Create: `scripts/calibration/device_cases.py`, `scripts/calibration/tests/test_device_cases.py`

**Interfaces:**
- Consumes: containers-rebase's `DeviceCase`, `CaseStatus`, `DEVICE_CASES_HEADER`,
  `parse_device_cases`, `render_device_cases`, `device_cases_path`.
- Produces the H100 file `testdata/benchmark-results/h100-80gb-hbm3/cases.csv` (Task 9 Step 5's
  test writes it; query-fuser, tp4-sized and batching-report read it):

```
dataset,sf,query,modes,status,note
tpch,40,q1,tp1_single,ok,
tpch,40,q9,,skipped,timeout: the case ran past 1800 s
tpch,40,q13,tp1_single,wrong,#NNN
```
  The reason is the text after `reason=` in the tree's failed section. A row's modes follow
  `H100_MODES`' order (Task 2).
- Produces in `test_support` (beside the reader):

```rust
pub enum Measured { Timed, Failed(String) }               // a section; Failed carries its reason
pub fn device_lines(
    candidates: &[(String, String, Vec<String>)],  // (ds, query, main-list modes), main-list order
    modes: &[&str],                                // H100_MODES
    files: &dyn Fn(&str, &str) -> Option<BTreeMap<String, Measured>>, // (ds, mode) → sections; None: no file
    wrong: &BTreeMap<(String, String), String>,    // (ds, query) → `#NN`, the file's `wrong` rows as it stands
) -> Result<Vec<DeviceCase>, String>;              // sf "40"
```
  `device_lines`, per candidate: the modes considered are those of `modes` its main-list line
  carries whose `<ds>.sf40/<mode>.benchmark.txt` exists (`tp1_single` always). A considered mode
  without a section is an `Err` naming dataset, query and mode, except a later mode of a query
  whose `tp1_single` failed (tp4-sized does not run it). Modes with a `Timed` section → an `ok`
  row with those modes, or `wrong` with the candidate's mark as its note; none → `skipped` with
  the first failed mode's reason. A `wrong` mark on a query that is not measured is an `Err` naming it.
- Produces `scripts/calibration/device_cases.py` (stdlib), the scripts' one reader:

```python
HEADER = "dataset,sf,query,modes,status,note"
def rows(path) -> list[dict]   # dataset, sf, query, modes (list), status, note; parse_device_cases' rules,
                               # SystemExit naming the line on a row they refuse
# CLI: device_cases.py <csv> [--dataset <ds>] [--mode <mode>]
#   one line per `ok`/`wrong` row, file order: dataset<TAB>sf<TAB>query<TAB>modes (`|`-joined);
#   --mode keeps rows whose modes carry it. Exit 1 naming the line on a refused row.
```
- The fixture `testdata/fixtures/device-cases.csv`: an `ok` row with two modes, a `wrong` row
  (`#7`), a `skipped` row whose reason holds a comma, a tpcds row.

- [ ] **Step 1: failing tests.** `device_cases/tests.rs`: `a_timed_section_makes_an_ok_row`;
  `a_query_failed_in_every_mode_is_a_skipped_row` (the reason kept);
  `a_mode_failed_beside_a_timed_one_drops_from_the_row` (tp1 timed, tp4 failed → `[tp1_single]`);
  `a_mode_without_its_file_is_not_considered`; `a_missing_section_is_refused` (`Err` naming
  query and mode); `a_rewrite_keeps_a_wrong_answer_mark` (a `wrong` mark in → a `wrong` row
  with that note out); `a_wrong_mark_on_a_skipped_query_is_refused`;
  `the_shared_fixture_reads_as_expected` (`parse_device_cases` of the fixture equals a literal
  `Vec<DeviceCase>`). `test_device_cases.py`: `test_the_shared_fixture_reads_as_rust_reads_it`
  (the same values as that literal, field by field); `test_cli_prints_ok_and_wrong_rows` (the
  skipped row absent, file order); `test_cli_mode_and_dataset_narrow`;
  `test_a_refused_row_exits_1_naming_it` (a `wrong` row with note `7`).
- [ ] **Step 2:** `cargo test -p peacockdb-core --features rust-only --lib -- device_cases` and
  `python3 scripts/calibration/tests/test_device_cases.py` → FAIL.
- [ ] **Step 3: implement.** **Step 4:** same commands → PASS.
- [ ] **Step 5: commit** `sf40-baseline: the H100 list's rows written from the tree; one reader for scripts`.
  The test over the committed file (`the_h100_list_is_the_lists_measured`, Task 9 Step 5) lands
  with the tree, so no commit holds a test over files it lacks.

### Task 2: a device list's rows are main-list cases; the DuckDB tree is not a device

**Files:**
- Modify: `peacockdb-core/tests/test_corpus_goldens/benchmark.rs` — `missing_case_problem` (:568),
  `device_directories` (:605), constants beside `PREDATES_PROVENANCE` (:625)

**Interfaces:**
- Consumes: containers-rebase's `DEVICE_LIST`, `device_list_files`; the rule "a device tree that
  holds a `cases.csv` is held to it; one without is held to the whole main list".
- Produces (tp4-sized adds `"tp4_sized"` to `H100_MODES`):

```rust
/// The modes the H100's `cases.csv` is written at, in row order: sf40-baseline (tp1_single).
const H100_MODES: &[&str] = &["tp1_single"];
/// Other engines' result trees under benchmark-results/, read by their own tests.
const OTHER_ENGINES: &[&str] = &["duckdb-cpu128"];
/// A device list's `ok` and `wrong` rows are cases of the main list, at modes the main list
/// carries for them. A tree in `PREDATES_PROVENANCE` was measured from an older list: exempt.
fn device_list_problem(device: &str, own: &Files, list: &Files) -> Result<(), String>;
```
  `missing_case_problem` calls it for every tree that holds a `DEVICE_LIST`, before holding the
  tree to that list. `device_directories` leaves out `OTHER_ENGINES`.

- [ ] **Step 1: failing tests:** `a_device_list_beyond_the_case_list_is_refused` (a synthesized
  `h100-80gb-hbm3` tree from the h200 tpch tp1_single file, its `cases.csv` naming a query the
  main list lacks → `Err` naming it and the device); `a_device_list_at_a_mode_the_case_list_lacks_is_refused`
  (a row carrying a mode the main list's line for it does not → `Err` naming the mode);
  `the_duckdb_tree_is_no_device_tree` (a temp root with `h200/` copied and
  `duckdb-cpu128/tpch.sf40/duckdb.tsv` → `device_directories` names only `h200`).
  containers-rebase's `a_tree_with_its_own_list_is_held_to_it` and
  `a_new_device_tree_is_held_to_every_declared_case` keep passing unchanged.
- [ ] **Step 2:** `cargo test -p peacockdb-core --features rust-only --test test_corpus_goldens -- device_list duckdb_tree` → FAIL.
- [ ] **Step 3: implement**; `rows_problem` (bench-rows) is unchanged: the h100 tree carries the fields.
- [ ] **Step 4:** whole `test_corpus_goldens` → PASS (`h200/` exempt). **Step 5: commit**
  `sf40-baseline: a device list holds main-list cases; other engines' trees are no device`.

### Task 3: the harness records the sf40 answer and times a case out

**Files:**
- Modify: `peacockdb-core/src/test_support/corpus_benchmark.rs` (`benchmark_case` :110, `measure` :175)
- Modify: `peacockdb-core/src/test_support/corpus_gpu.rs` (`record_gpu_result` :88 → `pub(crate)`)
- Modify: `peacockdb-core/src/test_support/corpus_golden.rs` (`merged_cells` :316)
- Modify: `peacockdb-core/src/test_support/mod.rs` (`CASE_TIMEOUT_ENV`)
- Modify: `scripts/env/run-gpu.sh` (`run`: `-e PEACOCK_BENCH_CASE_TIMEOUT_S -e PCK_WRITE_GPU_RESULT`;
  `pull`: also `testdata/goldens/*/gpu-result*.txt`), `scripts/env/tests/test_run_gpu.py`
- Test: `peacockdb-core/src/test_support/corpus_golden/tests.rs` (or the module's existing tests file),
  `peacockdb-core/src/test_support/benchmark_section` tests

**Interfaces:**
- Consumes: `measure_into`, `merge_section`, `FAILED`, `fields(provenance)` (`benchmark_section.rs`);
  `corpus::line_row_order(duckdb_oracle, gpu_oracle)`; `load_csv()`.
- Produces: `pub const CASE_TIMEOUT_ENV: &str = "PEACOCK_BENCH_CASE_TIMEOUT_S";` unset = no limit.
  On expiry a watchdog thread writes the case's section `status=failed reason=timeout: the case ran past <N> s`
  (`FAILED` + the reason) plus the five provenance keys through `merge_section` and calls `std::process::exit(1)`.
  `fn timeout_record(seconds: u64, provenance: &Provenance) -> String` (the body; unit-tested).
- Produces: under `gpu_recording()` ≠ `No` and `Capture::None`, a measured case writes the chosen
  run's `report.batches` through `record_gpu_result(dataset, sf, query, mode, order, &batches)`,
  order from the sf1 registry row's `duckdb_oracle`/`gpu_oracle`. Into
  `goldens/<ds>.sf40/gpu-result.txt`, section `== <query> mode=tp1-single`.
- Produces: `fn declared_sf(sf: &str) -> &str` in `corpus_golden.rs`: `"40"` → `"1"` (the sf the
  benchmark's cells are declared at, `BENCHMARK_SF`), any other → itself; `merged_cells` filters
  registry rows by it.
- Produces: `run-gpu.sh … run` passes both env names when set; `pull` brings `testdata/goldens/<ds>/gpu-result*.txt`.

- [ ] **Step 1: failing tests:** `an_sf40_answer_keeps_the_other_sf40_answers` (`merged_cells` on a
  text holding `q1 mode=tp1-single`, adding `q2` at sf `"40"` → both sections, q1 first);
  `an_sf1_answer_still_drops_a_disabled_cell` (today's behaviour at sf1); `the_timeout_record_is_a_failed_record`
  (`timeout_record(1800, &p)` starts `status=failed reason=timeout: the case ran past 1800 s` and ends with the
  five keys, so `device_tree_problem`'s failed-record branch accepts it — assert by running the
  same check bench-rows' reader applies: `parse_benchmark_section(body) == None`).
  In `test_run_gpu.py`: `test_run_passes_the_case_timeout_and_the_answer_switch`,
  `test_pull_brings_the_gpu_answers`.
- [ ] **Step 2:** `cargo test -p peacockdb-core --features rust-only --lib -- corpus_golden benchmark_section`
  and `python3 scripts/env/tests/test_run_gpu.py` → FAIL.
- [ ] **Step 3: implement.** `measure` returns the chosen run's batches beside `(body, rows)`; the
  recording happens only on `CaseOutcome::Measured`, after the section is written. The watchdog is
  started in `benchmark_case` after the provenance is read and before `measure`; it holds the
  file, `declared`, query and provenance by value.
- [ ] **Step 4: Run** Step 2's commands → PASS; `cargo test --no-run -p peacockdb-core --features gpu
  --tests` compiles in the 25.02 build image (containers-rebase Task 3 Step 7's command).
- [ ] **Step 5: commit** `sf40-baseline: a benchmark case records its sf40 answer and fails past its timeout`.

### Task 4: the sf40 answers held to DuckDB's

**Files:**
- Modify: `peacockdb-core/src/test_support/duckdb_oracle.rs` (`judge` :515, `duckdb_gpu_case` :473)
- Modify: `peacockdb-core/src/test_support/mod.rs` (`pub async fn sf40_answer_verdict`)
- Modify: `peacockdb-core/tests/test_cpu_corpus.rs`

**Interfaces:**
- Produces: `pub(crate) async fn device_verdict(dataset, sf, kinds_sf, query, mode, oracle) -> Result<(), String>`
  — `judge`'s comparison without the panic, reading `gpu-result.txt` and `duckdb-result.txt` at
  `sf` and the cell kinds by `output_kinds(dataset, kinds_sf, query)` (sf1 data: the schema does not
  depend on scale; CI holds no sf40). `judge`/`duckdb_gpu_case` call it with `kinds_sf == sf`.
- Produces: `pub async fn sf40_answer_verdict(dataset: &str, query: &str) -> Result<(), String>`
  (mode `tp1_single`, oracle = the sf1 row's `duckdb_oracle`).

- [ ] **Step 1: failing tests** in `test_cpu_corpus.rs`:
  `every_sf40_device_answer_meets_duckdb` — for each row of
  `parse_device_cases(&std::fs::read_to_string(device_cases_path("h100-80gb-hbm3"))…)`: `ok` → `Ok`; `wrong` → `Err`;
  `skipped` → no section expected; all failures collected and reported together. When the tree
  `testdata/benchmark-results/h100-80gb-hbm3/` exists and its `cases.csv` does not, it fails naming
  the file; only with neither does it pass with nothing read (before Task 9). That judgment is a
  pure `fn list_problem(tree_exists: bool, list_exists: bool) -> Option<String>`, tested directly:
  `a_tree_without_its_list_is_refused`.
  `a_stale_wrong_mark_is_refused` — a row `tpch,40,q6,tp1_single,wrong,#NN` over a section pair
  that agrees → the check's message contains `"meets DuckDB"` and `"ok"` (the row judgment is a
  pure `fn row_problem(row: &DeviceCase, verdict: Option<Result<(), String>>) -> Option<String>`,
  tested directly). `an_ok_row_without_an_answer_is_refused` (no gpu-result section →
  message names `PCK_WRITE_GPU_RESULT`).
- [ ] **Step 2:** `cargo test -p peacockdb-core --features rust-only --test test_cpu_corpus -- sf40 stale ok_row`
  → FAIL (no `device_verdict`). Before Task 9 neither the H100 tree nor its `cases.csv` exists:
  the test reads no file and passes.
- [ ] **Step 3: implement.** **Step 4:** whole `test_cpu_corpus` → PASS (the sf1 `duckdb_gpu_*` cases
  unchanged). **Step 5: commit** `sf40-baseline: the H100's sf40 answers held to DuckDB's`.

### Task 5: DuckDB's sf40 answers, rendered

**Files:**
- Modify: `testdata/duckdb_result.py` (`generate` :249, `main` :295)
- Modify: `testdata/test_duckdb_result.py`

**Interfaces:**
- Produces: `duckdb_result.py --dataset <ds> --sf 40 --answers <answers.duckdb>`: one section per
  table of the answers database (tables named by query stem, Task 6), in `query_order`, rendered by
  `render`/`fingerprint` exactly as today (`SELECT * FROM "<stem>" ORDER BY rowid` keeps DuckDB's
  row order for `no_sort` queries); a stem with no table → `failed: no answer recorded`. Writes
  `goldens/<ds>.sf40/duckdb-result.txt`. `--answers` without `--sf`, or `--sf 1` with `--answers`,
  exits 2 naming both. With neither, today's sf1 behaviour, unchanged.

- [ ] **Step 1: failing tests** (they need the duckdb wheel, as `generate` does; follow the file's
  existing pattern for that): `test_answers_render_as_the_parquet_path_does` (an answers db built
  from `tpch.sf1` for q1,q3 → sections byte-equal to `--only q1,q3` over sf1);
  `test_a_missing_answer_is_failed`; `test_answers_need_a_scale`.
- [ ] **Step 2:** `python3 testdata/test_duckdb_result.py` → FAIL. **Step 3: implement.** **Step 4:** PASS.
- [ ] **Step 5: commit** `sf40-baseline: duckdb_result.py renders DuckDB's answers from an answers database`.

### Task 6: `duckdb_minimal.sh` on the corpus; the H100 driver; records joined

**Files:**
- Modify: `benchmarks/duckdb_minimal.sh` (args :52-88, paths :95-99, query set :456-502, verify
  :550-575, report :619-649)
- Create: `benchmarks/tests/test_duckdb_minimal.py`
- Create: `scripts/env/bench-each.sh`, `scripts/env/tests/test_bench_each.py`
- Create: `scripts/calibration/join_records.py`, `scripts/calibration/tests/test_join_records.py`
- Create: `scripts/calibration/publish_record.sh`
- Modify: `scripts/create_nsys_profile.sh` (`--cases`, `--device`), `scripts/calibration/tests/`
  (its option test, beside new-hosts' `test_nsys_profile_uses_the_given_ssh`)

**Interfaces:**
- Produces `duckdb_minimal.sh` flags (defaults keep today's run byte for byte):
  `--dataset tpch|tpcds` (default `tpch`; bucket `<ds>-sf<N>`, db `<dir>/<ds>.sf<N>.duckdb`,
  tables = the parquet files' stems for tpcds, today's eight for tpch);
  `--cases <file>` (a device `cases.csv`, the H100's in Task 11, read through
  `device_cases.py <file> --dataset <ds>`: the corpus query text `testdata/<ds>-queries/<q>.sql`
  for every `ok` and `wrong` row of `<ds>`, in file order (`<q>` the ident with `_` → `-`, as
  `benchmark_case` names it: `aggregate_groupby` → `aggregate-groupby.sql`), `skipped` rows not
  run; without it, today's q1/q3/q6/q8 + vector set);
  `--answers <file.duckdb>` (needs `--cases`: the threads=1 pass runs each query as
  `ATTACH '<file>' AS answers; SET default_null_order='nulls_last_on_asc_first_on_desc'; SET integer_division=true; CREATE OR REPLACE TABLE answers."<q>" AS <sql>`;
  a failure is recorded `failed`, the query still timed);
  `--results <file.tsv>`: the timing table as TSV, preamble
  `# duckdb=<version> threads=<n> host=<hostname> cpu=<model> cores=<nproc> memory_gib=<n> db=<path> resident_pct=<p>`,
  columns `query ms_2nd_min runs_ms workset_bytes disk_read_bytes warm status`
  (`query` the list's ident, `q1`, `aggregate_groupby`; `runs_ms` comma-joined, no spaces;
  `status` = `ok` | `failed`, a failed row's `ms_2nd_min` and `runs_ms` `-`). Task 7's reader
  owns this format. `threads` is DuckDB's `current_setting('threads')`.
  `--cases` with `--skip-verify` and `--answers` refused (exit 2). Golden verify stays for the
  minimal set only; a corpus run has no sf40 goldens and says so once.
  `--build-only`: build `<ds>`'s database (import and the row-count check against the parquet),
  then exit 0 without warming or timing any query; refused with `--cases`, `--answers` or
  `--results` (exit 2).
- Produces `bench-each.sh [--instance-id <id>] [--profile <name>] --cases <file> --mode <mode> [--within <csv>] --timeout <s> --records <dir> [--dir <i>]`;
  `<vm options>` below are its `--instance-id`, `--profile` and `--dir`, passed on as given to
  `run-gpu.sh`; `<nebius options>` are `--instance-id` and `--profile` alone (`nebius.sh` has
  no `--dir` and exits 64 on it):
  per `corpus_query_benchmark!` line of `--cases` (the main list) whose modes carry `<mode>`, in
  file order, and with `--within` only a query that has an `ok` or `wrong` row there, read
  through `device_cases.py` (tp4-sized: `--within` the H100 `cases.csv`, so a query skipped at
  tp1_single is not run): skip it if the local `testdata/benchmark-results/<slug>/<ds>.sf40/<mode>.benchmark.txt`
  holds its section (timed with this card, or `status=failed`); else
  `PCK_TEST_FILTER=bench_<ds>_sf40_<q>_<mode> PEACOCK_BENCH_RESUME=1 PEACOCK_BENCH_CASE_TIMEOUT_S=<s> PCK_WRITE_GPU_RESULT=1 run-gpu.sh <vm options> run bench`,
  then `run-gpu.sh <vm options> pull`, move `testdata/calibration/records.tsv` to
  `<dir>/<ds>-<q>-<mode>.<attempt>.tsv` and remove the host's copy (`run-tests.sh bench` refuses a
  record holding rows). `<attempt>` is 1 + the highest attempt already in `<dir>` for that case,
  so a rerun never overwrites an earlier segment. **Before each case's run**, a non-empty host
  record (`nebius.sh <nebius options> ssh 'test -s ~/peacock/<i>/testdata/calibration/records.tsv'`),
  which a killed run (a preemption, the watcher) left behind, is pulled and set aside the same
  way under the case it belongs to (its last `# case` line), then removed on the host. A run
  exit ≠ 0 continues to the next query (the section says why). Exits 1 if any section is missing
  at the end.
  `<slug>` is read from the first pulled section's `device=`.
- Produces `join_records.py --record <seg>… --out <file>`: every segment `capture=none`, headings
  equal except `# run: allocator=`, whose lines must agree on every token but the free-memory one;
  the joined line gives that token as `<min>-<max>` (a token already `<a>-<b>`, a published joined
  record that tp4-sized joins again, counts as both ends). Segments are ordered by the attempt
  number in their name (`<ds>-<q>-<mode>.<attempt>.tsv`; a name without one, a published joined
  record, counts as attempt 0), never by argv or glob order; a case's rows come from its highest
  attempt holding it. Refuses (exit 1, nothing written) a heading that differs otherwise, naming the line.
- Produces `publish_record.sh --from <split dir> <device>/<ds>.sf<sf>…` on dev: uploads each
  `records.tsv` to `s3://calibration/<name>/<sha>.tsv` with `CALIBRATION_S3`, re-reads its sha,
  and rewrites its line in `testdata/calibration/records.sha256` (the code of
  `build-test-shadgpu.sh` `publish_record` :685-727, minus the remote checks, run locally).
- Produces `create_nsys_profile.sh --cases <csv>` (a device `cases.csv`, its `ok` and `wrong`
  rows at their modes, read through `device_cases.py`; without it the main list, as `BENCH_CASES`
  reads it today) and `--device <slug>` (records read from
  `calibration/<slug>/<ds>.sf<sf>/records.tsv`, where `fetch_record.sh <slug>/<ds>` puts them).
- Produces `split_record.py --cases <file>`: a file whose first line is `device_cases.HEADER` is
  read through `device_cases.rows` (`ok` and `wrong` rows at their modes); any other is the main
  list, read by `declared` as today.

- [ ] **Step 1: failing tests.** `test_duckdb_minimal.py` (real `duckdb` CLI 1.5.4 on PATH, the sf1
  parquet, `--sf 1 --skip-download --dir <tmp with symlinked tpch.sf1/tpcds.sf1>`):
  `test_corpus_cases_time_every_line_in_order` (a `cases.csv` with `q6` `ok`, `q1` `wrong` `#7`
  and `q9` `skipped` → results rows `q6`,`q1`, `status ok`); `test_answers_hold_one_table_per_query` (answers db tables `q1`,`q6`; `q1` row count
  = sf1's 4); `test_results_name_version_threads_host`; `test_tpcds_tables_from_the_parquet`
  (`--dataset tpcds` imports every `tpcds.sf1/*.parquet` stem); `test_answers_without_cases_is_2`;
  `test_build_only_runs_no_query` (`--dataset tpcds --build-only` → the database exists, the row
  counts are checked, no query timed; with `--cases` → 2);
  `test_default_run_is_unchanged` (no new flag → stdout's `=== results ===` table holds q6,q1,q3,q8).
  `test_bench_each.py` (fake `run-gpu.sh` on PATH writing sections and a record):
  `test_bench_each_runs_one_process_per_query`; `test_bench_each_resumes_and_sets_the_timeout`
  (env of each call); `test_bench_each_skips_a_failed_case_on_rerun`;
  `test_bench_each_moves_each_record_aside` (names `<ds>-<q>-<mode>.1.tsv`; a rerun of the case
  writes `.2.tsv`, `.1.tsv` unchanged); `test_bench_each_sets_aside_a_record_a_killed_run_left`
  (the fake host holds a non-empty record before the first case → it is pulled to that case's
  next attempt, the host copy removed, then the run goes ahead); `test_bench_each_takes_the_mode_s_lines_only`
  (lines without `<mode>` are not run); `test_bench_each_within_narrows_the_queries` (a `--within`
  CSV whose `skipped` row's query is not run, whose `wrong` row's is).
  `test_join_records.py`: `test_join_takes_the_later_segment_of_a_case`;
  `test_join_orders_segments_by_attempt_not_argv` (`q1.2.tsv` given before `q1.1.tsv`, and
  `q1.10.tsv` beside `q1.9.tsv` → the higher attempt's rows win);
  `test_join_spans_the_free_memory`; `test_join_takes_a_joined_record`; `test_join_refuses_another_device`; `test_join_refuses_a_capture`.
  Options test: `test_cases_and_device_reach_the_passes` (fake ssh: the trace filter holds exactly
  the `ok` and `wrong` rows' test names, the `skipped` row's absent; `--record` paths under
  `calibration/h100-80gb-hbm3/`). `test_split.py`: `test_split_record_reads_a_device_list`
  (a `cases.csv` → its `ok`/`wrong` cases; the main-list form unchanged).
- [ ] **Step 2:** run each file with `python3 <file>` → FAIL.
- [ ] **Step 3: implement.** The duckdb timing loop is untouched; the corpus set only feeds `emit`.
  Vector-param resolution runs only for the minimal set.
- [ ] **Step 4:** each file → `0 failed`; also every `scripts/env/tests/test_*.py` and
  `scripts/calibration/tests/test_*.py` that needs no image.
- [ ] **Step 5: commit** `sf40-baseline: duckdb_minimal.sh times the corpus; one process per H100 case; segments' records joined`.

### Task 7: DuckDB's results read once; the page's DuckDB column

**Files:**
- Create: `peacockdb-core/src/test_support/duckdb_results.rs`, `…/duckdb_results/tests.rs`
  (`mod duckdb_results;` in `test_support/mod.rs`; no `pub use`: `DuckdbStatus`, `DuckdbRow` and
  `DuckdbResults` are declared in `mod.rs`, `parse_duckdb_results` is a one-expression `pub fn`
  there delegating to the module's `pub(crate)` one)
- Create: `testdata/fixtures/duckdb-results.tsv` (its row in `testdata/fixtures/README.md`: the
  format's shared fixture; batching-report's Python reader is tested against it too)
- Modify: `peacockdb-core/src/test_support/benchmark_report.rs`, `…/benchmark_report/tests.rs`
  (bench-rows Task 4)
- Modify: `benchmarks/tests/test_duckdb_minimal.py`

**Interfaces:**
- Consumes: Task 6's `--results` format; bench-rows' `render_benchmark_report`, `ReportCell`.
- Produces (the format's owner reader):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuckdbStatus { Ok, Failed }
#[derive(Debug, Clone, PartialEq)]
pub struct DuckdbRow {
    pub query: String,
    pub ms_2nd_min: Option<f64>,   // None: failed (`-`)
    pub runs_ms: Vec<f64>,         // empty: failed
    pub workset_bytes: u64,
    pub disk_read_bytes: u64,
    pub warm: String,              // `yes` | `partial` | `NO`, as written
    pub status: DuckdbStatus,
}
pub struct DuckdbResults { pub preamble: Vec<(String, String)>, pub rows: Vec<DuckdbRow> } // the `# ` line's key=value tokens, in order
pub fn parse_duckdb_results(text: &str) -> DuckdbResults;   // panics naming the line: a missing preamble or header, a short row, an unknown status
```
- Produces in the page: every directory under `benchmark-results/` holding a
  `<ds>.sf<sf>/duckdb.tsv` is a DuckDB column, after the devices, header `<tree>`, sub-header
  `warm (ms)`: on every row of a (dataset, sf, query) the file holds, `ms_2nd_min` with one
  decimal, or `failed` (class `off`); otherwise `—` (class `na`). DuckDB has no mode, so the
  value stands on each mode's row. The intro names each DuckDB file's `duckdb`, `threads` and
  `host` from its preamble, and says DuckDB's time is query wall time over warm native storage.

- [ ] **Step 1: failing tests.** `duckdb_results/tests.rs`: `the_shared_fixture_reads_as_expected`
  (the fixture: preamble with `duckdb=1.5.4 threads=128`, an `ok` row and a `failed` row → a
  literal `DuckdbResults`); `a_short_row_is_refused`. `benchmark_report/tests.rs`:
  `a_duckdb_tree_is_a_column_of_its_own` (one `ok` row's ms, a `failed` row's mark, a query it
  lacks `—`; the DuckDB tree is not a device column); `the_duckdb_column_names_its_provenance`.
  `test_duckdb_minimal.py`: `test_results_have_the_fixtures_shape` (the written TSV's preamble
  keys and column header line equal the fixture's).
- [ ] **Step 2:** `cargo test -p peacockdb-core --features rust-only --lib -- duckdb_results benchmark_report`
  and `python3 benchmarks/tests/test_duckdb_minimal.py` → FAIL.
- [ ] **Step 3: implement.** **Step 4:** same commands → PASS; `the_benchmark_report_matches_the_trees`
  still PASS (no DuckDB tree is committed yet, so the page is unchanged).
- [ ] **Step 5: commit** `sf40-baseline: DuckDB's results read once; the page's DuckDB column`.

### Task 8: the build, and DuckDB's tpcds database on nebius-cpu128

**Files:** none; detail file.

- [ ] **Step 1: the staged install, on the CPU VM** (new-hosts' CPU-VM build, Global
  Constraints): `$C start`; `D=$(scripts/nebius-sync.sh --instance-id computeinstance-e01t2mm4265y8x88ey --profile eu-west1 --no-data)`;
  the `stage-install.sh` command in the 25.02 build image in `~/$D` → exit 0; rsync
  `cpp/install/` to dev `/build/peacock/sf40-baseline/install` with `-e "$C rsh"`. Record that
  the worktree carries pinned-loads (`git log --oneline | grep pinned-loads` on dev) — Done-when 1.
- [ ] **Step 2: the tpcds database.** `$C ssh 'df -h /var/lib/peacock'` ≥ 30 GB free, else stall.
  `$C ssh "cd ~/$D && benchmarks/duckdb_minimal.sh --dataset tpcds --dir /var/lib/peacock --sf 40 --skip-download --duckdb /usr/local/bin/duckdb --build-only"`
  → `row counts verified against the parquet`, `/var/lib/peacock/tpcds.sf40.duckdb` exists.
  `$C stop` → `STOPPED` (record the stop).

### Task 9: the H100 run, the tree, its `cases.csv`

**Files:**
- Create: `testdata/benchmark-results/h100-80gb-hbm3/{tpch,tpcds}.sf40/tp1_single.benchmark.txt`,
  `testdata/benchmark-results/h100-80gb-hbm3/cases.csv`,
  `testdata/goldens/{tpch,tpcds}.sf40/gpu-result.txt`; Modify: `testdata/calibration/records.sha256`
- Modify: `benchmark.rs` (`the_h100_list_is_the_lists_measured`); `testdata/benchmark-report.html` (regenerated)

- [ ] **Step 1: candidates.** `grep -v '^//' peacockdb-core/tests/common/corpus_benchmark_cases.inc | grep tp1_single | grep -c '(tpch,'`
  → 44, and `'(tpcds,'` → 82; both counts into the detail file. Other counts: record them and use
  what the list holds (the list is the candidate set, not these numbers).
- [ ] **Step 2: start and ship.** `$H start`; `$G prepare` (exit 0; `fetched 0` twice) ;
  `$G ship --install /build/peacock/sf40-baseline/install` → exit 0.
- [ ] **Step 3: run, tpch then tpcds** (foreground on dev; each call is one query's process):
  `scripts/env/bench-each.sh $VM --cases peacockdb-core/tests/common/corpus_benchmark_cases.inc --mode tp1_single --timeout 1800 --records <scratch>/records`
  (both datasets, tpch first, the file's order) → exits 0. Rerun after any interruption (VM stop,
  ssh loss): it resumes, setting aside any record a killed run left. Expected: every candidate has
  a section; record per-query wall time and the total in the detail file. Then `$H stop` →
  `STOPPED`: Steps 4-8 are local.
- [ ] **Step 4: skips, and the cache.** Each section matching `^status=failed`: query and reason
  (the text after `reason=`) into the detail file's skip table; a `reason=pinned cache: …` one
  is a defect (Global Constraints), explained in the detail file before Step 5, not a skip. Then
  the cache: `f=testdata/benchmark-results/h100-80gb-hbm3/*.sf40/tp1_single.benchmark.txt;
  grep -h '^pinned_cache=' $f | grep -v '^pinned_cache=on '; grep -h '^prefill_files=0$' $f`
  → nothing, else each section's query and value into the detail file's "Cache" section. That
  section also gives, per dataset, the largest `resident`, the largest `prefill_bytes` with its
  `prefill_us`, and the sum of `prefill_us` over the dataset's queries.
- [ ] **Step 5: `cases.csv`.** Add to `benchmark.rs` `the_h100_list_is_the_lists_measured`:
  candidates = the main list's lines carrying `tp1_single` (`macro_invocations_in(CASE_LIST, "corpus_query_benchmark!")`,
  file order); rows = Task 1's `device_lines(candidates, H100_MODES, the tree's sections, the
  `wrong` rows' notes of the committed file)`; text = `render_device_cases`. Under
  `UPDATE_CANONICAL=1` it writes `device_cases_path("h100-80gb-hbm3")` (staged and renamed, as
  `the_benchmark_cases_follow_the_registry` does); without, the file must equal the text, else a
  panic naming the first differing line and the command. No h100 tree and no file → passes; a
  tree and no file, or an `Err` from `device_lines`, fails naming it. Run with `UPDATE_CANONICAL=1`,
  then without → PASS. Expected today: every row `ok` at `tp1_single` but one `skipped` row per
  Step 4 entry, 44 + 82 rows under the header. Then the page:
  `UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens -- the_benchmark_report_matches_the_trees`
  → the H100 column beside the H200's.
- [ ] **Step 6: record.** `python3 scripts/calibration/join_records.py --record <scratch>/records/*.tsv --out <scratch>/joined.tsv`
  → exit 0; `python3 scripts/calibration/split_record.py --record <scratch>/joined.tsv --cases testdata/benchmark-results/h100-80gb-hbm3/cases.csv --out-dir <scratch>/split`
  → prints `h100-80gb-hbm3/tpch.sf40` and `…/tpcds.sf40`; `scripts/calibration/publish_record.sh --from <scratch>/split h100-80gb-hbm3/tpch.sf40 h100-80gb-hbm3/tpcds.sf40`
  → two new lines in `records.sha256`; `scripts/calibration/fetch_record.sh h100-80gb-hbm3/tpch.sf40` → `checked`.
- [ ] **Step 7: gates.** `cargo test -p peacockdb-core --features rust-only --test test_corpus_goldens`
  → PASS, including `every_declared_case_has_a_section_in_the_tree`,
  `every_section_outside_h200_carries_batches_and_the_load`, `every_total_us_is_the_sum_of_the_time_us_beside_it`
  and `the_benchmark_report_matches_the_trees` over the new tree. `test_cpu_corpus -- sf40` fails until DuckDB's answers land (Task 11): do not
  report a commit before then.
- [ ] **Step 8:** the files above stay uncommitted in the worktree for Task 11's commit (no `git add`).

### Task 10: both Nsight passes on the H100, per dataset

**Files:** Create `testdata/calibration/h100-80gb-hbm3/{tpch,tpcds}.sf40/{calls,loads,hbm}.tsv`.

- [ ] **Step 1: cases per dataset.** `f=testdata/benchmark-results/h100-80gb-hbm3/cases.csv; { head -1 $f; grep '^tpch,' $f; } > <scratch>/tpch.csv` (and tpcds).
- [ ] **Step 2: both passes**, per `<ds>`. `$H start` first (the shipped install is still on the
  H100's disk; `$G ship --install /build/peacock/sf40-baseline/install` again if `status` says the
  VM was rebuilt). With new-hosts' H100 arguments
  (`--ssh "$H rsh" --host nebius-h100 --remote-dir /home/dmitry/peacock/<i> --remote-cudf-root /home/dmitry/peacock/cudf-25.02`):
  `scripts/calibration/fetch_record.sh h100-80gb-hbm3/<ds>.sf40`, then
  `PCK_BENCH_HBM_SET=gh100 PCK_BENCH_HBM_PEAK_BW=3.35e12 scripts/create_nsys_profile.sh --cases <scratch>/<ds>.csv --device h100-80gb-hbm3 <H100 arguments>`
  → `the trace pass ran N tests` (N = the file's `ok` and `wrong` rows), `the call breakdown: … rows`,
  `the load breakdown: … rows` (pinned-loads), every counters case joined, `hbm.tsv` home.
  A trace pass longer than 3.5 h is split by halving the cases file and joining the two `calls.tsv`/
  `loads.tsv` bodies under one preamble (record it). After both datasets: `$H stop` → `STOPPED`.
- [ ] **Step 3:** move `testdata/calibration/{calls,loads,hbm}.tsv` to `…/h100-80gb-hbm3/<ds>.sf40/`;
  `for f in calls hbm; do git show HEAD:testdata/calibration/$f.tsv > testdata/calibration/$f.tsv; done`
  (the H200's, restored by a read, no git write).
  A case whose capture failed: its reason in the detail file. Captures stay on the host.
- [ ] **Step 4: the load split, read.** In the detail file, per dataset: Σ `load_us`, Σ
  `host_read_us + h2d_us` against Σ `decompress_us + decode_us`, from `loads.tsv` — the spec's
  "reading and copying against decoding". The H100 is already stopped (Step 2).

### Task 11: DuckDB on 128 cores; the answers compared; tickets

**Files:** Create `testdata/benchmark-results/duckdb-cpu128/{tpch,tpcds}.sf40/duckdb.tsv`,
`testdata/goldens/{tpch,tpcds}.sf40/duckdb-result.txt`; Modify the H100 `cases.csv` (`wrong`
rows), `testdata/benchmark-report.html` (the DuckDB column); tickets.

- [ ] **Step 1: timing and answers, per dataset** on the CPU VM. `$C start`;
  `D=$(scripts/nebius-sync.sh --instance-id computeinstance-e01t2mm4265y8x88ey --profile eu-west1 --no-data)`
  (the worktree, uncommitted: Task 9's `cases.csv` and Task 6's script with it); then
  `sha256sum testdata/benchmark-results/h100-80gb-hbm3/cases.csv` on dev equals
  `$C ssh "sha256sum ~/$D/testdata/benchmark-results/h100-80gb-hbm3/cases.csv"`, else stop. Then
  `$C ssh "cd ~/$D && benchmarks/duckdb_minimal.sh --dataset <ds> --dir /var/lib/peacock --sf 40 --skip-download --duckdb /usr/local/bin/duckdb --cases testdata/benchmark-results/h100-80gb-hbm3/cases.csv --answers /var/lib/peacock/answers-<ds>.duckdb --results /tmp/duckdb-<ds>.tsv"`
  → exit 0; `resident` 100.0 %, every `warm` `yes` (a `partial`/`NO` row is named in the detail
  file, not hidden); `threads=128` in the preamble. Copy both TSVs and answers dbs to dev
  (`rsync -e "$C rsh"`). `$C stop`.
- [ ] **Step 2:** `python3 testdata/duckdb_result.py --dataset <ds> --sf 40 --answers <scratch>/answers-<ds>.duckdb`
  for both; the TSVs to `duckdb-cpu128/<ds>.sf40/duckdb.tsv`. Each parses:
  `the_benchmark_report_matches_the_trees` under `UPDATE_CANONICAL=1` reads them through
  `parse_duckdb_results` and writes the page with its DuckDB column.
- [ ] **Step 3: compare.** `cargo test -p peacockdb-core --features rust-only --test test_cpu_corpus -- every_sf40_device_answer_meets_duckdb`.
  Each failing `ok` row: confirm by reading both sections. **Ties under LIMIT first:** where the
  query ends `ORDER BY … LIMIT n` and the first difference lies among rows tied on the sort keys
  at the limit's edge, the engines may each be right. Name the oracle's tie rule for that query
  (its `duckdb_oracle`/`gpu_oracle` in the registry and what `duckdb_oracle.rs`'s comparison does
  with ties) in the detail file. If the oracle should have accepted the tie, that is a ticket
  against the oracle in `tickets/testinfra.md`, and the row stays `ok` only once the oracle is
  fixed; it is never an operator ticket. Otherwise a ticket in the milestone file
  that owns the operator (Shared rules' header form, label naming the wrong answer, "sf40, H100,
  tp1_single", both sections' first difference), and its row's status set to `wrong`, note
  `#NN`, by hand. `the_h100_list_is_the_lists_measured` (no `UPDATE_CANONICAL`) → PASS: the writer
  carries the mark. Regenerate the page (`UPDATE_CANONICAL=1 … the_benchmark_report_matches_the_trees`:
  its cells now say `wrong answer #NN`). Rerun the compare → PASS.
  `python3 scripts/ticket_widget.py` only if the registry changed (it should not).
- [ ] **Step 4: gates** (Global Constraints list) → all PASS. Commit Tasks 9-11's files:
  `sf40-baseline: H100 tp1_single and DuckDB on 128 cores at sf40, the H100 case list, the profiles`
  (`testdata/benchmark-report.html` in the same commit). If a ticket was filed, `tickets.md`'s
  per-file count, its total open count (`77 open tickets`, `:22`) and the next free number
  (`currently <N>`, `:15`) are recounted.

### Task 12: `build-test.md` and the detail file

**Files:** Modify `llm-wiki/build-test.md`; Create `llm-wiki/tasks/sf40-baseline-detail.md`.

- [ ] **Step 1: `build-test.md`**, a section "sf40 baseline": the H100 `cases.csv` as the one
  ordered list (written from the tree by `the_h100_list_is_the_lists_measured`; `skipped` and
  `wrong` rows; `device_cases.py` the scripts' reader); the protocol note (the pinned host cache
  prefilled once by each query's process, before any round, against DuckDB's warm buffer
  manager; `prefill_*` reported apart); a device list's rows held to the main
  list; `duckdb.tsv`'s format and its owner reader; the page's DuckDB column; re-running the H100 side (Task 8 Step 1, Task 9 Steps 2-6,
  Task 10 commands, verbatim); the DuckDB side (Task 8 Step 2, Task 11 Steps 1-2); the sf40 answer
  check; `bench-each.sh`, `join_records.py`, `publish_record.sh`, the new `duckdb_minimal.sh`,
  `create_nsys_profile.sh` and `run-gpu.sh` options; "Benchmark data flow" gains the gpu-result and
  duckdb-result arrows at sf40; test counts recounted.
- [ ] **Step 2: detail file:** the run commands as run, wall times, the skip table (query, reason),
  wrong answers and tickets, warm verdicts, the load split, the cache (Task 9 Step 4) and the
  protocol note (Global Constraints), VM ids and their stops, and `## Suites`:
  each suite this task ran and where. `benchmarks/tests/test_duckdb_minimal.py` is a new
  directory no CI job runs yet (containers-only adds it to the image's script step); the new
  `scripts/env/tests` suites likewise run on dev only until then. Say so there.
- [ ] **Step 3:** `cargo test -p cost-report --test test_source_formatting` → PASS. Commit
  `sf40-baseline: build-test.md documents both sides of the sf40 baseline`.

## Self-review

- Spec 1 → Tasks 2, 3, 8-9 (pinned-loads in the build: Task 8 Step 1). Spec 2 → Tasks 6, 8, 11.
  Spec 3 → Tasks 3-5, 11. Spec 4 → Tasks 9, 12. Spec 5 (the page's DuckDB column, one owner
  reader, the shared fixture) → Tasks 7, 9 Step 5, 11. The one list (`cases.csv`: `ok`,
  `skipped`, `wrong` rows) → Tasks 1, 2, 9 Step 5, 11 Step 3. Profiles → Tasks 6, 10. Scope rows
  → Tasks 1-2, 6-7, 9-12. Constraints: segments → Task 6 `bench-each.sh`; VMs stopped → Tasks 8, 10, 11.
- Names across tasks: `DeviceCase`/`CaseStatus` (containers-rebase), `Measured`,
  `device_cases_path`, `parse_device_cases`, `render_device_cases`, `device_lines`,
  `device_cases.py` (`rows`, `HEADER`), `the_h100_list_is_the_lists_measured`, `H100_MODES`,
  `device_list_problem`, `OTHER_ENGINES`, `DuckdbResults`/`DuckdbRow`/`parse_duckdb_results`, `CASE_TIMEOUT_ENV`, `declared_sf`, `device_verdict`,
  `sf40_answer_verdict`, `bench-each.sh`, `join_records.py`, `publish_record.sh`.
