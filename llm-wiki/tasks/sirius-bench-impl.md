# sirius-bench Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Time SiriusDB `stable` on the H100 from pinned parquet bytes, show its total and a
Σ Sirius / Σ DuckDB line on the report page, and write `testdata/benchmarks.csv` of every result.

**Architecture:** Three layers. On the VM, a Python runner inside a new `peacockdb-sirius` image
times each query in its own process and stores its answer. On the laptop, `bench-sirius.sh`
drives the VM through `run-gpu.sh`. In Rust, a `sirius.tsv` reader feeds the page, the CSV, and
a test that judges Sirius's answers against DuckDB's and owns each row's status.

**Tech Stack:** Rust (`peacockdb-core` `test_support`, `--features rust-only`), bash
(`scripts/env/`), Python 3 (the runner, the offline script tests, `duckdb==1.5.6` in the image),
Docker, GitHub Actions (`images.yml`).

**Spec:** `llm-wiki/tasks/sirius-bench.md`. Read it first; this plan argues from it.

## Global Constraints

- Base: `ENS-containers-only` after Task 1's rebase. Branch: `ENS-sirius-bench`.
- Sirius `stable` = commit `8d17afe`; Python `duckdb==1.5.6`; the CUDA 13 prebuilt
  `.duckdb_extension` for linux-amd64 (cudf 26.08.01 linked in statically), mirrored and pinned
  by mirror URL and sha256 in `docker/pins.env`. No libcudf runtime, no conda, no Sirius source
  build, no pixi, no Sirius change. Can't load or run → stop and ask.
- One new image: `ghcr.io/asymptote-tech/peacockdb-sirius:cudf-26.08`, inputs tag
  `:cudf-26.08-in-<hash>`, commit tag `:cudf-26.08-<commit>`. Existing images unchanged.
- Nothing lands on master first. The branch dispatches `images.yml` with `--ref ENS-sirius-bench`.
- Sirius settings: `pin_table(..., tier='parquet')`;
  `sirius.executor.scan_manager.cache.mode: sirius`;
  `sirius.executor.scan_manager.cache.eviction_threshold_fraction: 1.0`;
  `sirius.telemetry.enable_quent: false`; `SET enable_duckdb_fallback = false`; everything else
  Sirius's default, memory included (no 69 GiB cap).
- Sirius's container: `--security-opt seccomp=<docker/sirius-seccomp.json>` (Docker's default plus
  `io_uring_setup`, `io_uring_enter`, `io_uring_register`, `mbind`, `set_mempolicy`,
  `get_mempolicy`), `--ulimit memlock=-1:-1`, no `--privileged`.
- Protocol: every row of the H100's `cases.csv` at sf40, `skipped` rows included (their notes
  are peacockdb's refusals, not Sirius's); one Sirius process per
  dataset, every table pinned once before the first query, untimed; per query one warm-up, then
  `MEASURED_RUNS` (10) runs; the second-smallest reported; wall time around the statement and an
  Arrow fetch. Only a total: no load, execute or peak for Sirius. Queries named by stem
  (`aggregate-groupby`).
- Files: `testdata/benchmark-results/h100-80gb-hbm3/<ds>.sf40/sirius.tsv`,
  `testdata/goldens/<ds>.sf40/sirius-result.txt`, `testdata/benchmarks.csv`.
- CSV columns, in order: `dataset,sf,query,engine,device,mode,load_ms,exec_ms,total_ms,peak_gib,status`.
- Rebased branches (Task 1): no build, no review, `done` without waiting for CI. This task:
  `done` with its PR's CI green.
- `run-gpu.sh` and `bench-sirius.sh` never start or stop a VM.

## Review Focus

1. **A query whose answer pass fails after its timed runs succeeded:** the row is `failed` with
   the answer error, never `ok` with no answer. Pinned in Task 7.
2. **A Sirius run that hangs or crashes** (a spill loop, an io_uring stall, a segfault):
   `--timeout` (per query, pin excluded) kills the process, the row is `failed: timeout after
   <s> s` or `failed: crashed (<signal>)`, and a fresh process re-pins and resumes at the next
   query. Pinned in Task 7.
3. **A dataset with no `sirius.tsv`** (tpcds measured, tpch not yet): the page shows `—` in the
   Sirius cells, no ratio line for that dataset, and the CSV has no Sirius rows for it; nothing
   panics. Pinned in Task 4.
4. **`--resume` across a changed Sirius pin or another host:** rows from another commit or host
   are rerun, not kept. Pinned in Task 7.
5. **A Sirius-only status change** (UPDATE_CANONICAL flips a row to `wrong`): the page, the
   ratio and the CSV all follow in the same regeneration, and the ratio's `n` drops. Pinned in
   Task 5.

---

### Task 1: Rebase the chain onto the report commits

Run by the coordinator under the rebase protocol before this task is dispatched: the board marks
batching-report, tp4-sized and containers-only `rebase needed(done)`, and chain L's note "Report
rebase, 2026-11-09" sets the verification below. The coordinator does every git step; a
developer is dispatched only for a conflict in code. No review round. It builds nothing but the
report tests.

**Files:** none authored; `testdata/benchmark-report.html` and `llm-wiki/build-test.md` resolved
per branch.

- [ ] **Step 1: Confirm the two report commits are on their branches**

Run: `git fetch origin && git merge-base --is-ancestor fc4decfb origin/ENS-query-fuser && git merge-base --is-ancestor a647a223 origin/ENS-tp4-sized && echo ok`
Expected: `ok`.

- [ ] **Step 2: Rebase each descendant onto its rebased parent, lowest first**

```bash
git checkout -B ENS-batching-report origin/ENS-batching-report
git rebase --onto origin/ENS-query-fuser <query-fuser tip batching-report was built on>
git checkout -B ENS-tp4-sized origin/ENS-tp4-sized
git rebase --onto ENS-batching-report origin/ENS-batching-report
git checkout -B ENS-containers-only origin/ENS-containers-only
git rebase --onto ENS-tp4-sized origin/ENS-tp4-sized
```

The old parent tip for batching-report is `git merge-base origin/ENS-batching-report
origin/ENS-query-fuser`. For tp4-sized, the old parent is batching-report before its rebase,
which is why the remote refs are used before they are force-pushed.

On a conflict in `testdata/benchmark-report.html`, take either side and regenerate (Step 3). In
code and wiki, resolve to the report commits' intent. `ENS-tp4-sized` already holds `a647a223`,
so its rebase replays it on the new base.

- [ ] **Step 3: On each rebased branch, regenerate the page and run the report tests only**

```bash
UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens -- the_benchmark_report_matches_the_trees
cargo test --features rust-only -p peacockdb-core --lib -- prototypes::fuser::report test_support::benchmark_report
cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens -- the_benchmark_report_matches_the_trees
```

Expected: all pass. If the page changed, commit it as `<task>: report page regenerated on the
rebased base`. No other build, no image, no device step.

- [ ] **Step 4: Force-push each with lease, lowest first**

Run: `git push --force-with-lease origin ENS-batching-report ENS-tp4-sized ENS-containers-only`
Expected: three `forced update` lines. Do not wait for CI.

- [ ] **Step 5: Board**

In `llm-wiki/tasks/tasks.md`, batching-report, tp4-sized and containers-only stay or return to
`done`. Create `ENS-sirius-bench` from the rebased `ENS-containers-only`.

---

### Task 2: The `sirius.tsv` format and its reader

**Files:**
- Create: `peacockdb-core/src/test_support/sirius_results.rs`, `.../sirius_results/tests.rs`
- Create: `testdata/fixtures/sirius-results.tsv` (shared with the runner's tests, Task 7)
- Modify: `peacockdb-core/src/test_support/mod.rs` (types and the `pub fn` below)

**Interfaces:**
- Produces, in `test_support`:
  - `pub struct SiriusResults { pub preamble: Vec<(String, String)>, pub rows: Vec<SiriusRow> }`
  - `pub struct SiriusRow { pub query: String, pub ms_2nd_min: Option<f64>, pub runs_ms: Vec<f64>, pub status: SiriusStatus, pub note: String }`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum SiriusStatus { Ok, Wrong, Unverified, Failed, Skipped }`
  - `pub fn parse_sirius_results(text: &str) -> SiriusResults`
  - `pub fn write_sirius_results(results: &SiriusResults) -> String` (exact inverse; Task 3 uses it)
- The format: line 1 `# ` then space-separated `key=value`, keys in this order:
  `sirius duckdb libcudf cuda driver device host`, spaces inside a value written as `_`. Line 2
  `query\tms_2nd_min\truns_ms\tstatus\tnote`. Then one row per query: `ms_2nd_min` one decimal
  or `-`; `runs_ms` comma-joined one-decimal values or `-`; `status` `ok|wrong|unverified|failed|skipped`;
  `note` free text without tabs, `-` when empty.

- [ ] **Step 1: Write the failing tests** in `sirius_results/tests.rs`

  - `reads_the_fixture`: parsing `testdata/fixtures/sirius-results.tsv` gives the preamble's
    seven keys in order, and rows `q1` ok 412.0 with ten runs, `q2` failed with note
    `NotImplemented: INTERSECT` and no time, `q3` skipped with its note.
  - `writes_what_it_reads`: `write_sirius_results(&parse_sirius_results(fixture)) == fixture`.
  - `refuses_by_line`: a missing `# ` line, a wrong column line, six fields, a query twice, an
    `ok`, `wrong` or `unverified` row without a time, a `failed` row with one, an unknown status:
    each panics naming its line number.

- [ ] **Step 2: Run, expect FAIL** — `cargo test --features rust-only -p peacockdb-core --lib -- test_support::sirius_results` → unresolved names.

- [ ] **Step 3: Implement** the reader and writer, shaped like `duckdb_results.rs` (one
  `refused(number, line, why) -> !`).

- [ ] **Step 4: Run, expect PASS.**

- [ ] **Step 5: Commit** — `sirius-bench: sirius.tsv, its reader and writer`.

---

### Task 3: Sirius's answers against DuckDB's, and the statuses they set

**Files:**
- Modify: `peacockdb-core/src/test_support/duckdb_oracle.rs` (generalize `sf40_verdict`)
- Modify: `peacockdb-core/src/test_support/mod.rs`
- Modify: `peacockdb-core/tests/test_cpu_corpus.rs` (beside `every_sf40_device_answer_meets_duckdb`)
- Modify: `peacockdb-core/src/test_support/corpus_golden.rs` (`sirius_result_golden`)

**Interfaces:**
- Consumes: Task 2's types.
- Produces:
  - `pub fn sirius_result_golden(dataset: &str, sf: &str) -> PathBuf` → `testdata/goldens/<ds>.sf<sf>/sirius-result.txt`.
  - `pub async fn sf40_sirius_verdict(dataset: &str, query: &str) -> AnswerVerdict`: the section
    for `query` in `sirius-result.txt` against DuckDB's sf40 section, under the same oracle and
    tolerance as `sf40_answer_verdict`. `sirius-result.txt` is `testdata/duckdb_result.py
    --answers`'s format (the runner renders it, Task 7), so its sections are read the way
    `duckdb-result.txt`'s are; the comparison side is `Side::Scaled("tp1_single")`.
  - `pub fn sirius_status_for(recorded: SiriusStatus, verdict: Option<AnswerVerdict>) -> Result<SiriusStatus, String>`:
    `Failed`/`Skipped` stay; a timed row is `Ok` on `Meets`, `Wrong` on `Differs`, `Unverified`
    when DuckDB has no stored sf40 answer for the query (no section in `duckdb-result.txt`: the
    list's `skipped` rows); any other `Unanswered` (no Sirius section, DuckDB `failed:`) is `Err`
    naming how to record it. `sf40_sirius_verdict` must tell the two apart: it returns a new
    `AnswerVerdict::NoReference` variant for a missing DuckDB section, which
    `sf40_answer_verdict` never returns.

- [ ] **Step 1: Failing tests**
  - In `test_support` unit tests: `sirius_status_for` over the seven combinations above.
  - In `test_cpu_corpus.rs`: `every_sf40_sirius_status_meets_its_answer` (tokio). For each
    dataset with a `sirius.tsv`: each `ok` or `wrong` row's status must equal
    `sirius_status_for(row.status, Some(sf40_sirius_verdict(..).await))`. Under
    `UPDATE_CANONICAL=1` it rewrites the file (statuses only) with `write_sirius_results` and passes. No
    `sirius.tsv` anywhere: passes, reading nothing.
  - `a_sirius_row_that_meets_duckdb_marked_wrong_is_refused`: over a doctored temp copy, the
    message names the query and says to rerun with `UPDATE_CANONICAL=1`.

- [ ] **Step 2: Run, expect FAIL.** `cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- sirius`

- [ ] **Step 3: Implement.** `sf40_verdict` gains a private source parameter (the peacockdb
  section or the Sirius one); the public `sf40_answer_verdict` keeps its signature.

- [ ] **Step 4: Run, expect PASS.** Also `every_sf40_device_answer_meets_duckdb` unchanged.

- [ ] **Step 5: Commit** — `sirius-bench: Sirius's sf40 answers held to DuckDB's, the status test`.

---

### Task 4: The page: the Sirius group and the ratio line

**Files:**
- Modify: `peacockdb-core/src/test_support/benchmark_report.rs`, `.../benchmark_report/tests.rs`

**Interfaces:**
- Consumes: `parse_sirius_results`. Sirius files are found at
  `<root>/<device>/<ds>.sf<sf>/sirius.tsv` for any device not in `MODES_ONLY`.
- Produces: `fn sirius_cells(place: &Place, results: Option<&SiriusResults>, rel: &str) -> String`
  (three `<td>`s) and `fn ratio_line(...) -> Option<String>`.

Main table header: after the device groups, `<th colspan="3">sirius · h100-80gb-hbm3</th>`, with
sub-headers `load (ms)`, `execute (ms)`, `total (ms)`; DuckDB stays last. Load and execute are
`<td class="na">—</td>` always. Total: `ms_2nd_min` one decimal linked to its line (row index
+ 3), or `failed: <note>` class `off`, or `skipped: <note>` class `skip`; a `wrong` row is the
time plus ` <span class="missing">wrong answer</span>`; an `unverified` row is the time plus
` <span class="source" title="no DuckDB answer to check it against">unverified</span>`. No row: `—`. The Modes tables don't
change.

Ratio line, a `<p>` straight after the `<h2>`:
`Σ total Sirius / Σ total DuckDB = <r> over <n> of <m> queries`, `<r>` two decimals, `<n>` the
main table's queries where Sirius's row and DuckDB's row are both `ok`, `<m>` the main table's
rows. No `sirius.tsv` for the dataset: no line.

- [ ] **Step 1: Failing tests**
  - `a_sirius_file_adds_a_group_before_duckdb`: main row `q3` reads
    `["q3", "1.0", "0.5", "1.5", "2.00", "—", "—", "300.0", "412.0"]`, and the total links to
    `sirius.tsv#L3`.
  - `a_failed_sirius_row_shows_its_reason`: `failed: NotImplemented: INTERSECT`, class `off`.
  - `the_ratio_counts_rows_both_ok`: Sirius q3 300 ok, q6 100 wrong, q9 failed, q11 50
    unverified (no DuckDB row); DuckDB q3 400, q6 200, q9 ok → `= 0.75 over 1 of 4 queries`, and
    q11's total reads `50.0 unverified`.
  - `no_sirius_file_no_line_and_dashes` (Review Focus 3).

- [ ] **Step 2: FAIL.** `cargo test --features rust-only -p peacockdb-core --lib -- test_support::benchmark_report`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: PASS**, then regenerate the page (`UPDATE_CANONICAL=1 … the_benchmark_report_matches_the_trees`); no `sirius.tsv` is committed yet, so the page changes only by the header group.
- [ ] **Step 5: Commit** — `sirius-bench: the page's Sirius group and ratio line`.

---

### Task 5: `testdata/benchmarks.csv`

**Files:**
- Modify: `peacockdb-core/src/test_support/benchmark_report.rs` (+ tests), `mod.rs`
- Modify: `peacockdb-core/tests/test_corpus_goldens/benchmark.rs`
- Create: `testdata/benchmarks.csv`

**Interfaces:**
- Produces: `pub fn render_benchmarks_csv(root: &Path, order: &[(String, String)]) -> String`,
  `pub fn benchmarks_csv_path() -> PathBuf`, `pub fn write_benchmarks_csv(text: &str)` (staged and
  renamed, as `write_benchmark_report`).

Rows: peacockdb, every device and mode it has a section for; then Sirius per `sirius.tsv` row;
then DuckDB per `duckdb.tsv` row. Order: dataset, sf, then the page's query order, then engine
(`peacockdb`, `sirius`, `duckdb`), device by name, mode in `MODES` order. Numbers: ms one decimal,
`peak_gib` two decimals, empty where absent. Status as the page decides it (the device list's
`skipped`/`wrong` over the section; Sirius's file's; DuckDB `ok`/`failed`). A `failed` or
`skipped` row has empty times; a `wrong` or `unverified` row keeps them (spec item 8). Sirius `mode` is `tp1_single`.

- [ ] **Step 1: Failing tests** over one temp tree (h100 tp1_single + tp4_sized, h200 tp1_single,
  a `sirius.tsv`, a `duckdb.tsv`): exact expected CSV text, header line first; a DuckDB row with
  `mode,load_ms,exec_ms,peak_gib` empty; a Sirius row with `load_ms,exec_ms,peak_gib` empty; and
  `a_status_flip_moves_page_ratio_and_csv_together` (Review Focus 5).
- [ ] **Step 2: FAIL.**
- [ ] **Step 3: Implement**, sharing `shown_of` with the page so a status is decided once.
  `the_benchmark_report_matches_the_trees` also renders the CSV: under `UPDATE_CANONICAL` it
  writes both; otherwise it compares both byte for byte, naming the first line that moved.
- [ ] **Step 4: PASS**; write the file with `UPDATE_CANONICAL=1`.
- [ ] **Step 5: Commit** — `sirius-bench: testdata/benchmarks.csv, every result, beside the page`.

---

### Task 6: The `peacockdb-sirius` image

**Files:**
- Create: `docker/sirius.Dockerfile`, `docker/sirius.Dockerfile.dockerignore`,
  `docker/sirius-config.yaml`, `docker/sirius-seccomp.json`, `docker/sirius-venv.lock`
- Modify: `docker/pins.env` (`SIRIUS_COMMIT=8d17afe…`, `SIRIUS_EXTENSION_URL` (the mirror),
  `SIRIUS_EXTENSION_SHA256`, `SIRIUS_EXTENSION_BYTES=921613272`, `SIRIUS_DUCKDB=1.5.6`,
  `SIRIUS_LIBCUDF=26.08.01`, `DRIVER_MINIMUM_SIRIUS=580.65.06`), `scripts/env/image.sh`,
  `scripts/lib/image-inputs.sh`, `.github/workflows/images.yml`
- Test: `scripts/env/tests/test_image.py`, `test_images_workflow.py`, `test_pins.py`

`image.sh`'s call shape is `image.sh build <kind> <cudf>` / `image.sh push <kind> <cudf>`
(image.sh:4, :31); the new kind is `sirius`, valid with cuDF `26.08` only, and no other kind
takes `26.08`. `images.yml` gains `sirius` under `kind` and `26.08` under `cudf`, with matrix
excludes so `all` builds `sirius` at `26.08` alone.

Mirror first: upload the `stable` CUDA 13 `.gz` (sha256 `01e4797c…`, 921,613,272 bytes; take the
full hash from the analyst's evidence `scratchpad/vmcheck*.out` or recompute) to the calibration
bucket beside the other pinned downloads (`scripts/lib/calibration-bucket.sh`), and pin that URL.

`docker/sirius-config.yaml` holds exactly: `sirius.telemetry.enable_quent: false`,
`sirius.executor.scan_manager.cache.mode: sirius`,
`sirius.executor.scan_manager.cache.eviction_threshold_fraction: 1.0`.
`docker/sirius-seccomp.json` is Docker's default profile with the six calls of Global
Constraints added; its header comment names the exact source file and commit it was taken from
(moby's `profiles/seccomp/default.json` moved; find the current one for Docker 29.9).
The image: a slim base with glibc ≥2.28 and libstdc++ ≥ GLIBCXX_3.4.22, Python 3, `duckdb==1.5.6`
from `docker/sirius-venv.lock`, the extension at `/opt/sirius/sirius.duckdb_extension`, `/opt/sirius/PIN`
(the pins as `key=value`), `docker/sirius-config.yaml`, and `docker/sirius-bench.py`. No
libcudf, no conda, no peacockdb binary.

- [ ] **Step 1: Failing tests.** `image.sh build sirius 26.08` tags `peacockdb-sirius:cudf-26.08`,
  the inputs tag and the commit tag; `image.sh build sirius 25.02` and `image.sh build run 26.08`
  are refused; the `peacockdb.inputs` label names every `docker/` file the Dockerfile reads and the
  Sirius pins; a wrong `SIRIUS_EXTENSION_SHA256` or byte count fails the build; `images.yml` offers
  `sirius`/`26.08` and its Test step for `sirius` runs `sirius-bench.py --self-test`.
- [ ] **Step 2: FAIL.** `python3 -m pytest scripts/env/tests/test_image.py scripts/env/tests/test_images_workflow.py scripts/env/tests/test_pins.py -q`
- [ ] **Step 3: Implement.** `--self-test` (no GPU): the extension's sha256 against `PIN`, its
  footer reads `v1.5.6 linux_amd64`, `ldd` lists no library beyond libcuda.so.1,
  libnvidia-ml.so.1, libstdc++, libgcc, libc, libm, libdl, libpthread, librt, and the config
  parses. It does not LOAD.
- [ ] **Step 4: PASS**; `scripts/env/image.sh build sirius 26.08` locally; the self-test passes
  in it; record the image's size (expect 2–2.5 GB).
- [ ] **Step 5: Commit, push, dispatch** —
  `gh workflow run images.yml --ref ENS-sirius-bench -f kind=sirius -f cudf=26.08`; record the
  pushed inputs tag in the detail file.

---

### Task 7: The runner inside the image, `docker/sirius-bench.py`

**Files:**
- Create: `docker/sirius-bench.py`, `scripts/env/tests/test_sirius_bench.py`

**Interfaces:**
- CLI: `sirius-bench.py --dataset tpch|tpcds --sf 40 --data <dir> --queries <stem>,… --runs <n>
  --timeout <s> --out <dir> [--resume] [--self-test]`.
- Writes `<out>/benchmark-results/h100-80gb-hbm3/<ds>.sf<sf>/sirius.tsv` (Task 2's format,
  rewritten after every query) and the answers database
  `<out>/calibration/sirius/<ds>.sf<sf>/answers.duckdb` (gitignored), one table `"<stem>"` per
  answered query. It renders nothing.

A parent process supervises one worker per dataset (`multiprocessing`, spawn) over a pipe:
1. The worker connects (`config={'allow_unsigned_extensions': 'true'}`), loads the extension with
   `SIRIUS_CONFIG_FILE=/opt/sirius/sirius-config.yaml`, runs `SET enable_duckdb_fallback = false`
   and the session settings `duckdb_minimal.sh`'s answers pass uses (copied by name, with a
   comment pointing at the script line), creates a view per table over
   `<data>/<table>.parquet` (one file per table on the H100), and pins every table with
   `CALL pin_table('<path>', name='<table>', tier='parquet')`. It reports `pinned` with the time
   taken. The pin is outside every timeout.
2. Per query (SQL from `testdata/<ds>-queries/<stem>.sql`): one warm-up, `--runs` timed runs,
   `time.perf_counter_ns()` around `execute(sql).fetch_arrow_table()`; the last run's Arrow
   table is written to the answers database as `"<stem>"` through a plain DuckDB connection
   (`CREATE TABLE … AS SELECT * FROM arrow_tbl`, which runs on the CPU over the Arrow table and
   stores Sirius's answer, not DuckDB's). It sends the row, then the next query.
3. The parent writes each row as it arrives: `ok` with the second-smallest; `failed: <first
   line of the error>` for an error in any step, the answer write included; on `--timeout` (per
   query, from the warm-up's start) or a worker death, it kills the worker, writes `failed:
   timeout after <s> s` or `failed: crashed (<signal or exit code>)`, starts a fresh worker
   (re-pinning) and resumes at the next query. The runner writes no `skipped` row: the status
   stays in the format for the reader, and nothing in this task produces it.
   A worker exits only after the parent acknowledged its last row (the exit-time panic seen on
   the H100 must not lose a row).
4. `--resume` keeps a row only where the file's `sirius` and `host` preamble values equal this
   run's. The preamble comes from `/opt/sirius/PIN` (`sirius`, `duckdb`, `libcudf`),
   `nvidia-smi` (`cuda`, `driver`, `device`) and the hostname.

- [ ] **Step 1: Failing tests** in `test_sirius_bench.py`, over a fake `duckdb` module on
  `sys.path` (scripted latencies, errors, hangs, crashes, Arrow results), no GPU:
  `pins_every_table_once_per_dataset`, `times_ten_runs_and_reports_the_second_smallest`,
  `the_answer_is_the_sirius_selects_arrow_result` (no `CREATE TABLE … AS <query>` reaches the
  Sirius connection), `an_error_is_a_failed_row_with_its_first_line`,
  `an_answer_write_error_after_good_runs_is_failed` (Review Focus 1),
  `a_hang_is_killed_repinned_and_the_next_query_runs` and
  `a_crash_is_repinned_and_the_next_query_runs` (Review Focus 2),
  `the_pin_is_outside_the_timeout`, `resume_keeps_rows_of_this_commit_and_host_only` (Review
  Focus 4), `queries_are_named_by_stem`, `the_file_parses_as_the_rust_fixture_does` (against
  `testdata/fixtures/sirius-results.tsv`'s shape), `bad_input_is_refused_before_any_query`.
- [ ] **Step 2: FAIL.** `python3 -m pytest scripts/env/tests/test_sirius_bench.py -q`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: PASS.**
- [ ] **Step 5: Commit** — `sirius-bench: the runner inside the Sirius image`.

---

### Task 8: `run-gpu.sh`'s Sirius verb, `bench-sirius.sh`, and rendering at home

**Files:**
- Modify: `scripts/env/run-gpu.sh` (`prepare --sirius`; a `sirius` verb; `KIT` gains
  `docker/sirius-seccomp.json`), `scripts/env/tests/test_run_gpu.py`
- Modify: `testdata/duckdb_result.py` (`--out <file>`; default unchanged)
- Create: `scripts/env/bench-sirius.sh`, `scripts/env/tests/test_bench_sirius.py`

**Interfaces:**
- `run-gpu.sh … prepare --sirius`: also ships the seccomp profile, pulls the Sirius image by this
  tree's inputs tag (or `PEACOCK_SIRIUS_IMAGE`), checks its label and the driver against
  `DRIVER_MINIMUM_SIRIUS` (exit 3), and exits 2 if Docker's root lacks the image's size plus
  10 GB, naming free and needed GB.
- `run-gpu.sh … sirius [--detach] -- <runner args>`: needs no staged peacockdb install (unlike
  `ship`/`run`); it rsyncs only `testdata/<ds>-queries/` and `testdata/benchmark-results/
  h100-80gb-hbm3/` to the VM, then `docker run --gpus all --security-opt seccomp=<shipped
  profile> --ulimit memlock=-1:-1` the Sirius image as container `peacock-<i>` with
  `python3 /opt/sirius/sirius-bench.py <runner args>`, the sf40 data mounted read-only at its own
  path and the shipped testdata at `--out`. `status` works on it unchanged; `pull` already brings
  `benchmark-results/` and `calibration/` (and so the answers database) home.
- `bench-sirius.sh`: the spec's item 4 flags (`--instance-id`, `--profile`, `--dataset
  tpch|tpcds|all`, `--sf` 40, `--cases` (default `testdata/benchmark-results/h100-80gb-hbm3/
  cases.csv`), `--query` repeatable, `--runs` 10, `--timeout` 900, `--resume`, `--dry-run`,
  `--help`). It reads the list itself (a small reader of `cases.csv` taking every row, `skipped` included,
  since `device_cases.py` drops skipped rows),
  converts names to stems, then per dataset `sirius -- …`, then `pull`, then at home
  `python3 testdata/duckdb_result.py --dataset <ds> --sf <sf> --answers testdata/calibration/
  sirius/<ds>.sf<sf>/answers.duckdb --out testdata/goldens/<ds>.sf<sf>/sirius-result.txt` under the
  pinned DuckDB 1.5.4. `--dry-run` prints those commands and calls nothing.

- [ ] **Step 1: Failing tests** over a fake `nebius.sh`/`run-gpu.sh`, in `test_bench_each.py`'s
  style: each flag reaches the runner as specified; `--query q6` narrows to one; `--dataset all`
  runs tpch then tpcds; `aggregate_groupby` reaches the runner as `aggregate-groupby`; a `skipped`
  list row is run like any other; no `--instance-id`, an unknown dataset, a query not in the list,
  `--runs 0`, all refused before any call (exit 2); `--dry-run` calls nothing; `run-gpu.sh`'s
  3/4/75/69 exits pass through; the render runs after `pull` with `--out`. For `run-gpu.sh`:
  the `docker run` line carries the seccomp profile, `--ulimit memlock=-1:-1`, the inputs tag,
  the data read-only, and no `--privileged`; `sirius` runs without a staged install;
  `prepare --sirius` on a full disk exits 2. For `duckdb_result.py`: `--out` writes there and
  the default path is unchanged.
- [ ] **Step 2: FAIL.** `python3 -m pytest scripts/env/tests/test_run_gpu.py scripts/env/tests/test_bench_sirius.py -q`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: PASS**, and `test_run_gpu.py`, `test_bench_each.py` and `test_duckdb_minimal.py`
  otherwise unchanged.
- [ ] **Step 5: Commit** — `sirius-bench: bench-sirius.sh, run-gpu.sh's Sirius verb, the render at home`.

---

### Task 9: The H100 run

**Files:** `testdata/benchmark-results/h100-80gb-hbm3/{tpch,tpcds}.sf40/sirius.tsv`,
`testdata/goldens/{tpch,tpcds}.sf40/sirius-result.txt`, `testdata/benchmark-report.html`,
`testdata/benchmarks.csv`.

- [ ] **Step 1: Prepare.** Start the H100 with `nebius.sh` (the chain's VM rules). If Docker's
  root is short, remove the stale `:cudf-25.02` tags the analyst found (about 15 GB). Then
  `scripts/env/run-gpu.sh --instance-id <id> prepare --sirius`. Expected exit 0; record free GB.
- [ ] **Step 2: One query first.** `scripts/env/bench-sirius.sh --instance-id <id> --dataset tpch --query q6`.
  Expected: the pin of all eight tpch tables (about 7 min at ~100 MB/s), then a `sirius.tsv` row
  `ok` with ten runs, the answers table, and after the render a `sirius-result.txt` section. In
  the log: no CPU-fallback line, no `fixed buffers disabled`, no `mbind: Operation not
  permitted`. Any of those → fix the container, not the protocol.
- [ ] **Step 3: Both datasets.** `scripts/env/bench-sirius.sh --instance-id <id> --dataset all --resume`.
  Expected: one row per list row, peacockdb's skipped queries included (those end `unverified`
  in Step 4 if timed); tpcds q8, q14, q38, q87 (INTERSECT/EXCEPT) expected `failed`,
  and possibly other DISTINCT queries. Stop the VM.
- [ ] **Step 4: Statuses, page, CSV.**
  `UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- every_sf40_sirius_status_meets_its_answer`,
  then the page test with `UPDATE_CANONICAL=1`, then both without it. Expected: PASS.
- [ ] **Step 5: Commit** — `sirius-bench: Sirius stable on the H100, tpch and tpcds sf40`,
  recording in the detail file each dataset's ratio line, the `failed` rows and their reasons,
  and the VM time used.

---

### Task 10: The wiki and CI

**Files:** `llm-wiki/build-test.md`; the CI wiring guard if a new test file needs naming.

- [ ] **Step 1:** In `build-test.md`: a "Sirius" section (the image, `prepare --sirius`,
  `bench-sirius.sh` with its flags, the protocol, the seccomp change and why, `sirius.tsv`'s
  format); "The page" gains the Sirius group and the ratio line; the generated-files table gains
  `sirius.tsv`, `sirius-result.txt` and `benchmarks.csv`; the test tables gain Tasks 2–8's tests
  with their counts; the tier totals are recomputed from a run.
- [ ] **Step 2:** `cargo test -p cost-report --test test_source_formatting` passes (the CI wiring
  guard sees any new test target).
- [ ] **Step 3:** Push; open the PR on `ENS-containers-only`; CI green. That is `done`.
