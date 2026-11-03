# batching-report Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `llm-wiki/reports/query-batching.md`: per dataset (TPC-H sf40, TPC-DS sf40) and batch
(first3/first10/first20/all), fused load and execute on the H100 against the same queries'
unfused sums and DuckDB's warm sum on 128 cores, the two hints, the load-against-decode split,
provenance, and a short reading.

**Architecture:** every number in the page comes from a small stdlib Python generator,
`scripts/calibration/batching_report.py`. It reads the committed files of sf40-baseline,
query-fuser and pinned-loads (and tp4-sized's, when they exist) and rewrites the region between
two markers in the report. The prose (the reading, item 5) sits outside the markers and is
written by hand from the generated tables. `--check` exits 1 when the page is stale, so a later
task (tp4-sized) reruns the generator instead of hand-editing numbers.

**Tech Stack:** Python 3 stdlib, run as `python3 <file>` through `scripts/calibration/tests/harness.py`
(no pytest). No Rust, no device run.

**Spec:** [`batching-report.md`](batching-report.md). Siblings consumed exactly:
[`query-fuser-impl.md`](query-fuser-impl.md) (Task 6 "The format", `BATCH_SIZES`, `batches`,
`FUSED_ALL_RUNS`, `fused_results_file`, Task 8/10 calibration layout),
[`containers-rebase-impl.md`](containers-rebase-impl.md) (Task 4: the `cases.csv` format),
[`sf40-baseline-impl.md`](sf40-baseline-impl.md) (Task 1 `device_cases.py`, the scripts' one
reader of `cases.csv`; Task 6 `duckdb.tsv` format; Task 7 its owner reader
`parse_duckdb_results` and the fixture `testdata/fixtures/duckdb-results.tsv`; Tasks 9-11 trees), [`bench-rows-impl.md`](bench-rows-impl.md) ("The format":
trailer keys), [`pinned-loads-impl.md`](pinned-loads-impl.md) (Task 1 `loads.tsv` columns),
[`tp4-sized-impl.md`](tp4-sized-impl.md) (Task 4 column names).

## Global Constraints

- Kind: prototype. The coordinator skips the reviewer and the PR; the tests are the generator's.
- Branch `ENS-batching-report` on `ENS-query-fuser`. Code base: master's engine + containers'
  scripts (`ENS-containers-master`) + every chain L task above it.
- Shape: the TPC-H section of `reports/benchmark-minimal.md` (lines 30-44): a provenance table,
  then one table per dataset, load and execute as separate columns, DuckDB warm alongside.
- Inputs, all under `testdata/`, read-only:
  - the one ordered list, `benchmark-results/h100-80gb-hbm3/cases.csv`, read through
    `device_cases.rows` (sf40-baseline Task 1; never a parser of this script's own): a dataset's
    batch set is its `ok` and `wrong` rows carrying `tp1_single`, in file order, TPC-H restricted
    to q1-q22 (`^q([1-9]|1[0-9]|2[0-2])$`, query-fuser's; the micro queries are not batched);
    "the first N" = the first N of them. A `skipped` row is a skipped query, its note the reason;
    a `wrong` row is counted and noted with its `#NN`;
  - unfused `benchmark-results/h100-80gb-hbm3/<ds>.sf40/tp1_single.benchmark.txt`;
  - fused `benchmark-results/h100-80gb-hbm3/fused/<ds>.sf40/tp1_single.fused.txt`;
  - DuckDB `benchmark-results/duckdb-cpu128/<ds>.sf40/duckdb.tsv` (preamble
    `# duckdb=… threads=… host=… cpu=… cores=… memory_gib=… db=… resident_pct=…`; columns
    `query ms_2nd_min runs_ms workset_bytes disk_read_bytes warm status`);
  - loads `calibration/h100-80gb-hbm3/<ds>.sf40/loads.tsv` (rows `mode == "tp1-single"`) and
    `calibration/h100-80gb-hbm3/fused/<ds>.sf40/<batch>.loads.tsv`;
  - optional: `benchmark-results/h100-80gb-hbm3/<ds>.sf40/tp4_sized.benchmark.txt`.
- Section markers: a body starting `skipped: ` is a marker (`SKIPPED`), one starting
  `status=failed reason=` is a failed record (`FAILED`, containers' `test_support/mod.rs:658,1011`).
- Every total is device-side: `load_us + execute_us` (= `device_us`) for both fused and unfused.
  DuckDB's is query wall time over warm native storage. The page says so once, as
  benchmark-minimal.md does.
- `peak_bytes` is always labelled "accountant's peak, a lower bound on device memory".
- ms with one decimal; ratios two decimals with `×`, written as `unfused / fused` and
  `DuckDB / fused` (above 1 = fused faster).
- coding-style.md: plain module filenames; files < 1000 lines.
- Gates: `for f in scripts/calibration/tests/test_*.py; do python3 "$f" || exit 1; done`;
  `cargo test -p cost-report --test test_source_formatting`.

## Review Focus

1. **An out-of-memory batch** — the row shows `out of memory (cap k)` with its budget, no
   fused numbers and no ratios; never a zero. → Task 1, `test_an_out_of_memory_batch_shows_its_cap`.
2. **A list of ≤ 20 counted queries** (skips cut TPC-H's 22) — `first20` is a marker; the row reads
   `same set as all`, not a second set of sums. → Task 1, `test_a_marker_batch_reads_same_set_as_all`.
3. **A query missing from a sum** — a DuckDB row with `status=failed`, or a tp4_sized `FAILED`
   section: the sum shows `(n/N)` and a footnote naming the query; an unfused tp1_single section
   missing or failed for a counted query refuses (exit 1, naming it), since sf40-baseline's
   writer makes those `skipped` rows, not `ok` ones. → Task 1, `test_a_failed_duckdb_query_is_counted_out_and_named`,
   `test_a_counted_query_without_an_unfused_section_is_refused`.
4. **A list changed after the fused run** — the fused section's `queries=[…]` must equal the
   batch's prefix of today's list; otherwise refuse, naming the batch. → Task 1,
   `test_a_fused_batch_of_other_queries_is_refused`.
5. **Provenance that differs across sections** (two hosts, two cuDF builds) — every distinct
   value is printed with its count, never the first one alone. → Task 1,
   `test_provenance_lists_every_distinct_value`.

---

### Task 1: the generator, tested on a fixture tree

**Files:**
- Create: `scripts/calibration/batching_report.py`
- Create: `scripts/calibration/tests/test_batching_report.py`
- Modify: `llm-wiki/build-test.md` — the calibration test-category row (the one naming
  `test_loads.py`, ~line 882): add `test_batching_report.py` and recount

**Interfaces:**
- Consumes: `record.read_tsv(path, *required)` (`scripts/calibration/record.py:57`);
  `device_cases.rows(path)` (sf40-baseline Task 1, same directory); the formats under Global Constraints.
- Produces (module `batching_report`):

```python
BATCH_SIZES = [("first3", 3), ("first10", 10), ("first20", 20), ("all", None)]  # query-fuser's
DATASETS = ["tpch", "tpcds"]
DEVICE = "h100-80gb-hbm3"; DUCKDB_TREE = "duckdb-cpu128"
BEGIN = "<!-- batching_report.py: generated below, do not edit -->"
END = "<!-- batching_report.py: generated above -->"

def sections(text: str) -> list[tuple[str, str]]            # `== name` → body, file order
def trailer(body: str) -> dict[str, str] | None             # None for skipped:/status=failed; dup key → ValueError
def counted(cases: list[dict], dataset: str) -> list[str]    # device_cases.rows' output → the batch set: ok/wrong rows at tp1_single, file order, tpch q1-q22 only
def notes(cases: list[dict], dataset: str) -> list[tuple[str, str]]  # (query, "skipped at sf40: <note>" | "wrong answer: #NN")
def batches(queries: list[str]) -> list[tuple[str, list[str] | None]]  # None = "same set as all"
def fused_batch(body: str) -> dict                           # blocks batch/merge/pipeline/run; outcome, queries, cap, …
def duckdb(text: str) -> tuple[dict[str, str], dict[str, dict]]  # preamble, rows by query; a failed row's `-` → None
def load_split(rows: list[dict]) -> dict[str, int]           # Σ load_us, host_read_us, h2d_us, decode (decompress_us+decode_us), other_device_us, unattributed_us
def render(testdata: Path) -> str                            # the generated region
def splice(report: str, generated: str) -> str               # replaces BEGIN..END; ValueError if either marker is missing
def main(argv) -> int                                        # --testdata <dir> --report <md> [--check]
```

`batches` follows query-fuser Task 2's `batches`: `first3` always runs; a sized batch other than
`first3` with `len(queries) <= size` is `None`. `fused_batch` splits at `--- batch ---`,
`--- merge ---`, `--- pipeline ---`, `--- tree ---`, `--- run ---`; each line is split at the
first `=` (the `model=` value keeps its spaces; `entry …` merge lines are counted, their
`saved_us=` read). A fused marker section (`skipped: same set as all…`) gives `{"marker": …}`.

`render` writes, in order:
1. **Provenance** table: H100 (`device driver cuda cudf host` from every timed trailer of the
   unfused and fused files, distinct values with counts) and DuckDB (`duckdb threads host cpu
   cores` from each preamble). Protocol line: unfused runs = `len(runs)` (distinct values), fused
   `all` = `FUSED_ALL_RUNS` (3) read from its `runs`, DuckDB = `len(runs_ms)`; "2nd-minimum".
   Skipped queries and wrong answers: every `notes` entry, `query — note`, per dataset (the TPC-H
   micro queries' too: sf40-baseline measured them).
2. Per dataset, **the table**: `batch | queries | fused load (ms) | fused execute (ms) | unfused
   load (ms) | unfused execute (ms) | DuckDB warm (ms) | unfused / fused | DuckDB / fused`, plus
   `tp4_sized load (ms) | tp4_sized execute (ms)` after the unfused pair only when
   `tp4_sized.benchmark.txt` exists (tp4-sized-impl.md Task 4). Unfused sums are the trailers'
   `load_us`/`execute_us` over the batch's queries; DuckDB is Σ `ms_2nd_min` over `status=ok`.
3. Per dataset, **hints**: `batch | cap | estimate (GiB) / budget (GiB) | accountant's peak (GiB) |
   merge saves (ms, estimate) | entries | pipelining saves (ms, estimate) | pipelined estimate (ms)`.
   Under it the two model lines: merge — "identical subtrees over one shared source run once;
   the dearest copy stays (Σ t − max t over each group, from the unfused tree's node times)";
   pipeline — the fused section's own `model=` text.
4. Per dataset, **load split**: `batch | side | Σ load | host read | H2D copy | decompress+decode
   | other device | unattributed` (ms), side `unfused` (the batch's queries' `loads.tsv` rows) and
   `fused` (`<batch>.loads.tsv`). A marker or out-of-memory batch, or one listed in
   `fused/<ds>.sf40/failed.tsv`, has no fused row; the failed one shows its `reason`. Footnote: profiled runs, per-execution medians; with
   `host_read_source=kvikio` the host read contains kvikio's bounce copies, which H2D counts too,
   so the two are not added.

- [ ] **Step 1: Write the failing tests** in `test_batching_report.py`. A helper
  `tree(tmp, **overrides)` writes a fixture testdata dir: a `cases.csv` with tpch `ok` rows
  `q1 q3 q6 q14` and `aggregate_groupby` at `tp1_single`, a row `tpch,40,q9,,skipped,timeout: the case ran past 1800 s`,
  and three tpcds `ok` rows; unfused
  sections with trailers (`load_us=1000 execute_us=500` for q1, etc., full bench-rows key order);
  a fused file with `first3` measured, `first10`/`first20` markers, `all` measured; `duckdb.tsv`;
  `loads.tsv` rows. Tests:
  - `test_counted_takes_tpch_q1_to_q22_in_file_order`: tpch → `[q1, q3, q6, q14]` (the micro
    query and the `skipped` row out; q6 as `wrong` `#7` stays in, and `notes` holds
    `(q6, "wrong answer: #7")` and `(q9, "skipped at sf40: timeout: …")`).
  - `test_duckdb_reads_the_shared_fixture`: `duckdb(read(testdata/fixtures/duckdb-results.tsv))`
    gives the values sf40-baseline's `the_shared_fixture_reads_as_expected` asserts in Rust, the
    format's owner (preamble `duckdb`, `threads`; the `ok` row's ms; the `failed` row's `None`).
  - `test_sections_and_trailer_read_bench_rows_format`: a section with the 13 trailer keys →
    `trailer(body)["load_us"] == "101308"`; a `skipped: …` and a `status=failed reason=…` body → `None`.
  - `test_batches_take_prefixes_and_mark_short_ones`: 25 names → lengths 3/10/20/25; 18 names →
    `first20` is `None`, `all` has 18; 2 names → `first3` has 2.
  - `test_the_first3_row_sums_its_three_queries`: rendered tpch row `first3` holds the fused
    `1.0`/`0.5`-style ms values and unfused sums equal to the three fixture trailers added, and
    `unfused / fused` = their quotient to two decimals.
  - `test_an_out_of_memory_batch_shows_its_cap`: fused `all` with `outcome=out-of-memory`,
    `cap=1` → the row holds `out of memory (cap 1)` and no `×` cell.
  - `test_a_marker_batch_reads_same_set_as_all`.
  - `test_a_failed_duckdb_query_is_counted_out_and_named`: q6 `status=failed` → DuckDB cell ends
    `(2/3)` and a footnote names `q6`.
  - `test_a_counted_query_without_an_unfused_section_is_refused`: drop q3's section → `main`
    returns 1, stderr names `q3` and the file.
  - `test_a_fused_batch_of_other_queries_is_refused`: fused `first3` `queries=[q1,q6,q3]` → 1,
    stderr names `first3`.
  - `test_provenance_lists_every_distinct_value`: two trailers with `cudf=25.02.01` and
    `cudf=25.02.02` → both appear, each with its count.
  - `test_tp4_columns_appear_only_with_their_file`; a tp4 `FAILED` section gives `(n/N)`.
  - `test_load_split_sums_columns_and_keeps_read_and_copy_apart`: two rows → Σ per column; the
    rendered row has separate host-read and H2D cells.
  - `test_splice_replaces_only_between_markers` (text outside kept byte-equal; a missing marker
    raises `ValueError`); `test_check_is_1_when_stale_and_0_after_writing`.
- [ ] **Step 2: Run** `python3 scripts/calibration/tests/test_batching_report.py` → FAIL
  (`ModuleNotFoundError: batching_report`).
- [ ] **Step 3: Implement** `batching_report.py` as above. Module docstring: what it reads, the
  device-side total, and that it owns every number between the markers.
- [ ] **Step 4: Run** the same file → `N passed, 0 failed`; then the gate loop over
  `scripts/calibration/tests/test_*.py` → every file `0 failed`.
- [ ] **Step 5: Docs and commit.** build-test.md row and count. Commit
  `batching-report: a generator for the query-batching page, tested on a fixture tree`.

### Task 2: the page, from the committed results

**Files:**
- Create: `llm-wiki/reports/query-batching.md`
- Create: `llm-wiki/tasks/batching-report-detail.md`

**Interfaces:**
- Consumes: Task 1's `main`; the committed trees of sf40-baseline (Tasks 9-11) and query-fuser
  (Tasks 9-10).
- Produces: the report with markers `BEGIN`/`END`, which tp4-sized Task 4 regenerates with
  `--check` after its files land.

- [ ] **Step 1: Inputs present.** On dev, in the branch:
  `ls testdata/benchmark-results/h100-80gb-hbm3/cases.csv testdata/fixtures/duckdb-results.tsv; for d in tpch tpcds; do ls testdata/benchmark-results/h100-80gb-hbm3/$d.sf40/tp1_single.benchmark.txt testdata/benchmark-results/h100-80gb-hbm3/fused/$d.sf40/tp1_single.fused.txt testdata/benchmark-results/duckdb-cpu128/$d.sf40/duckdb.tsv testdata/calibration/h100-80gb-hbm3/$d.sf40/loads.tsv; done`
  → ten paths, no error. A missing one stalls the task, naming the sibling that owns it.
- [ ] **Step 2: Skeleton.** Write the page: title "Query batching — fused vs unfused on the H100,
  and DuckDB on 128 cores"; "Reading the comparison honestly" (device-side totals vs DuckDB wall
  time; load included on both GPU sides; accountant's peak a lower bound; hints are estimates);
  then `BEGIN`/`END` on their own lines; then an empty "Reading" section.
- [ ] **Step 3: Generate.** `python3 scripts/calibration/batching_report.py --testdata testdata --report llm-wiki/reports/query-batching.md`
  → exit 0; then `… --check` → exit 0. Hand-check one cell per dataset: `first3`'s unfused load
  equals the three `load_us=` lines of `tp1_single.benchmark.txt` added (`grep -A200 '^== q1$' …`),
  ms; record the check in the detail file.
- [ ] **Step 4: The reading** (spec item 5), under "Reading", five to ten short lines from the
  tables only: where the saving comes from (load vs execute columns; the split's read/copy vs
  decode rows say whether sharing saved copies, decodes, or both); the batch size at which
  `unfused / fused` stops growing; which hint is larger and so is worth building next; OOM batches
  and their caps. No claim a cell does not show; each claim names its table and row.
- [ ] **Step 5: Gates and commit.** The gate loop; `cargo test -p cost-report --test
  test_source_formatting` → PASS. `git status --short testdata/` → empty. Detail file: the
  commands, the hand check, which inputs' commits were read (`git log -1 --format=%h -- <path>`
  per input). Commit `batching-report: fused vs unfused vs DuckDB per batch size, sf40`.

## Self-review

- Spec 1 (table per dataset, four batch sizes, fused/unfused/DuckDB, ratios, OOM with cap,
  tp4_sized later) → Task 1 render §2, Task 2 Step 3. 2 (two hints, labelled, model in one line)
  → §3. 3 (load against decode) → §4. 4 (provenance, skips from `cases.csv`, protocol) → §1; 4a (one reader per format: `device_cases.rows`,
  the shared DuckDB fixture) → Task 1 Interfaces and `test_duckdb_reads_the_shared_fixture`. 5 (reading) → Task 2
  Step 4. Scope: the page is new; the generator and its test are added scope, for numbers that
  are not hand-typed.
- Names match siblings: `first3/first10/first20/all`, `FUSED_ALL_RUNS`, `tp4_sized load (ms)`,
  `h100-80gb-hbm3`, `duckdb-cpu128`, `loads.tsv` columns, `mode == "tp1-single"` (the case text's
  mode, as `calls.tsv` writes it).
