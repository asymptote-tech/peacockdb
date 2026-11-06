# batching-report — detail

Branch `ENS-batching-report`, forked from `ENS-query-fuser` at `413366c4` on 2026-11-06. PR targets
`ENS-query-fuser`. Plan: [`batching-report-impl.md`](batching-report-impl.md).

## Progress

- 2026-11-06: branched, board `building`. Verda unavailable (`VERDA_CLIENT_ID` unset); tests run
  locally. No VM needed. Developer dispatched at Task 1 Step 1.
- Inputs from query-fuser: the fused format and the merge/pipeline hints are documented in
  `query-fuser-detail.md` ("The fused format"). Its completeness pass notes that part of the fused
  load saving is deduplication inside one query (`shared=` shows `q14+q14+q14`), which a batch of
  one would also show; the merge hint counts cross-query groups only (since `7b1db00e`).
- 2026-11-06, Task 1 (developer), Steps 1-5, to the checkpoint (not committed). All local.
  - Red: `test_batching_report.py` failed with `ModuleNotFoundError: batching_report`. Green:
    27 passed. Two first-run failures were the test's own (a sum written as 650 for 600; a row
    helper that wanted one row per batch where the split has two); one was the generator's:
    it read the unfused trailers before checking the fused sections against the list, so a
    grown list named a new query rather than the batch. The fused check now comes first.
  - **Departures from the plan's sketch, the committed files and query-fuser winning:**
    - The merge model line is `merge.rs`'s after the completeness pass: groups above a shared
      load with members from two or more queries, Σ t − max t, t the queries' own unfused node
      times. The plan's line predates the cross-query rule.
    - `failed.tsv` is headerless `batch<TAB>pass<TAB>reason` (`create_nsys_profile.sh`); a
      listed batch's fused split row shows `profile failed: <pass>: <reason>`, as the plan says.
    - A fused `<batch>.loads.tsv` carries the batch name in `query`. DuckDB's preamble also
      holds `floor_ms` (the owner reads it; the fixture test asserts it).
    - The DuckDB reader keeps the owner's refusals (seven fields, `ok`/`failed`, a failed row
      all `-`, a query twice), tested; it does not import Rust's struct names.
  - **Decisions the plan left open:** `DuckDB / fused` is `—` when the DuckDB sum is `(n/N)`
    (a ratio over part of a batch reads as the whole). The estimate cell adds `(over budget)`
    when `over_budget=true`. An out-of-memory row reads `out of memory (cap k); budget X GiB`,
    the unfused and DuckDB sums kept, no ratios. A failed fused section reads `failed: <reason>`.
    tp4_sized sections join the provenance when the file exists. A counted query with no
    `tp1-single` row in `loads.tsv` is refused, so the unfused split never undercounts silently.
  - **Added to the cache row** (spec 4, "any run that fell back or re-read from disk"): the
    sections whose `pinned_cache` shows `fallbacks`, `evictions` or `reloads` above 0, named
    with them. bench-rows says a timed section cannot show any; the row says `none` today.
  - Fixture: `spec()` builds the tree as data and `tree(tmp, s)` writes it, rather than the
    plan's `tree(tmp, **overrides)`: a test edits one section's values in place.
  - Tests beyond the plan's list: the DuckDB refusals, `fused_batch`'s blocks, a failed fused
    batch, a failed unfused section and a missing profile row refused, a fused marker where the
    list has a batch, a missing fused section, the wrong-answer note, the hints row, the
    protocol and DuckDB rows, the cache counters.
  - A dry run of `render` over the committed tree (stdout only) builds without a refusal; tpch
    first3 fused 585.5 + 335.3 ms against query-fuser-detail's 591 + 336 (its ±1.5 %).
  - Gates: the loop over `scripts/calibration/tests/test_*.py`, every file `0 failed`.
    `test_plot.py` needs matplotlib, which this host's python lacks and CI installs
    (`pipeline.yml:705`); it ran with `pip install --target /tmp/br-mpl matplotlib` on
    `PYTHONPATH`, 7 passed. build-test.md: the calibration row 77 → 104, the header 8144 →
    8171 (Python 1559 → 1586); the tables' N columns sum to 8171.
  - Wiki drift: the plan cites `record.read_tsv` at `record.py:57`; it is at `:128`.
- 2026-11-06, Task 2 (developer), Steps 1-5, to the checkpoint (not committed). All local, no VM.
  - **Step 1:** the plan's ten input paths and the fixture all present.
  - **A column added before the reading** (generator change, test first: red on the missing
    column, then 28 passed): the hints tables' `folded scans (a query's own)`. Per shared load,
    its scans less one, from `shared=`; in brackets those that repeat a scan the same query
    already makes, which a batch of one folds too. It is what lets the reading say that part of
    the load saving is deduplication inside one query from a table rather than from
    query-fuser-detail. It counts loads, not time: no table splits the saving's ms between the
    two kinds, and the reading says so.
  - **Steps 2-3:** skeleton, then `python3 scripts/calibration/batching_report.py --testdata
    testdata --report llm-wiki/reports/query-batching.md` → 0, and the same with `--check` → 0.
    Hand check, `load_us=`/`execute_us=` of `tp1_single.benchmark.txt` added: tpch first3 load
    155178 + 311162 + 200359 = 666699 µs (page 666.7), execute 334053 (334.1); tpcds first3
    load 163674 + 134360 + 108894 = 406928 (406.9), execute 307972 (308.0). DuckDB tpch first3
    694 + 272 + 507 = 1473 (1473.0).
  - **Step 4, the reading:** seven lines, each naming its table and row. Every cited number was
    re-read from the generated tables by a script: execute fused/unfused 0.99-1.044 in all 8
    batches; fused load lower in all 8; host read 0.0 on every split row; decode falls 1321.6 ms
    (tpch all) and 6296.5 ms (tpcds all), H2D 355.2 and 350.4; caps 3,1,1,1 and 3,2,2,1 (6 of 8
    at 1 or 2); pipelining above merge in all 8; no out-of-memory batch.
  - **Inputs read** (`git log -1 --format=%h -- <path>`): `cases.csv`, both
    `tp1_single.benchmark.txt` and both unfused `loads.tsv` `30a6165e`; both `duckdb.tsv` and the
    fixture `148f4a4a`; both `tp1_single.fused.txt` `e96e52d4` (the completeness rerun); both
    `calibration/h100-80gb-hbm3/fused/<ds>.sf40/` `ec6935dd` (round 2's profiles, which
    query-fuser-detail says stand for the completeness binary).
  - build-test.md: the calibration row 104 → 105, the header 8171 → 8172 (Python 1587), the
    tables sum to 8172; "Fused batches" names the page and its regenerate command.
  - Gates: the calibration loop, every file `0 failed` (`test_plot.py` with matplotlib from
    `/tmp/br-mpl`, as in Task 1); `test_source_formatting` 39 passed; `--check` 0.
    `git status --short testdata/` shows only `testdata/calibration/hbm-parts/`, query-fuser's
    untracked leftover, not this task's and not to be staged.
- 2026-11-06: plan complete. PR #201 against `ENS-query-fuser`; board `reviewing`; reviewer round 1 dispatched.
- 2026-11-06, review round 1 (developer): I1-I5 and N1-N7, test first (12 new or changed tests
  red on the missing behaviour, then 37 passed). Not committed. All local, no VM.
  - **I1:** `floor_ms` joins the DuckDB provenance row; the note under each table says what one
    DuckDB time holds and the floor's ms; a column `DuckDB / fused, floors out` takes
    `n × floor_ms` off the DuckDB sum (`—` when the sum is `(n/N)`). Both ratios are kept: the
    as-shown one is what benchmark-minimal.md compares. A `duckdb.tsv` without `floor_ms` is
    refused. tpcds first3: 1.00× as shown, 0.82× floors out; the reading says DuckDB is faster
    there with the floors out.
  - **I2:** the intro uses build-test.md's wording (one shared load per file, plus a load per
    re-read). The hints tables gain `loads (re-reads)`: Σ `reads=` and `rereads` (tpch all
    46 (38), tpcds all 426 (402)), and the reading cites them.
  - **I3:** `pipeline.yml`, dataset-matrix, a step `Query-batching page is current (Python)`
    right after the one ending `generate_pbench.sh --check`, on the 25.02 leg only (python over
    committed text). Checked by parsing the YAML (the step sits between "Generate SF-1 test
    data" and the exec-model step) and `bash -n` on its rendered `run:`; run as written it exited
    1 on the page before regeneration and 0 after. Not proved in CI. A push of only `.md` and
    `llm-wiki/` files skips the pipeline (`changes`), so a hand edit of the page alone is caught
    at the next push that runs it; build-test.md says so.
  - **I4:** a measured batch's fused split comes from `<batch>.loads.tsv` whenever it exists;
    `failed.tsv`'s reason shows only when it does not; neither is still refused.
  - **I5, N1, N4:** the reading's DuckDB line rewritten (DuckDB's sum over fused's, both
    datasets, as shown and floors out); TPC-H "no plateau within its 22 queries"; the
    pipelining model line and the reading say it has no memory term.
  - **N2:** the split footnote says the parts are separate medians that can overlap, so they
    may add up past Σ load. **N3:** a counted query's DuckDB row with `warm` other than `yes` is
    named under the table (none today). **N5:** the largest prefill names every section of that
    size and the range of their times (tpch's four fused batches, 20.4-460.4 s). **N6:** a line
    under each table says what `all` holds of the list's lines (tpch 22 of 48: 1 skipped, 25
    micro queries; tpcds 79 of 82: 3 skipped). **N7:** the test asserts the host and the floor,
    renamed `test_provenance_holds_the_protocol_and_the_duckdb_run`.
  - build-test.md: the calibration row 105 → 114, the header 8172 → 8181 (Python 1596), the
    tables sum to 8181; dataset-matrix's paragraph and "Fused batches" name the CI step.

## Review round 1 (2026-11-06)

0 blocking, 5 important, 8 nits; every generated number recomputed and right. All to the developer except N8 (checkpoint row, coordinator).

- **I1.** DuckDB's per-process floor (30 ms tpch, 36 ms tpcds) is neither named nor subtracted; each batch sums N floors. tpcds first3 "level (1.00x)" is 0.82x floors out. Fix: `floor_ms` in `DUCKDB_KEYS`, say what one DuckDB time holds, give the ratio floors out (or both), test.
- **I2.** The intro's "each file loaded once for every scan of it" is false (tpch all 46 loads of 8 files; tpcds all 426 of 24). Use build-test.md's wording; optionally `rereads` beside folded scans.
- **I3.** Nothing in CI runs `--check`. **Decided:** add a `pipeline.yml` step beside `generate_pbench.sh --check`, and update build-test.md.
- **I4.** A batch in `failed.tsv` hides a valid `<batch>.loads.tsv`. Show the split whenever the file exists; the reason only when it does not; tests for both.
- **I5.** `query-batching.md:169` inverts the ratio ("fused is 1.60x-2.48x DuckDB's").
- Nits N1-N7 for the developer: N1 no plateau within 22 tpch queries; N2 split parts exceed load by 1-4 % (overlap, separate medians); N3 name non-`yes` DuckDB `warm` rows; N4 the pipelining model has no memory term; N5 prefill `max` keeps the cold first of four ties; N6 tpcds "all" is the 82 tp1_single lines; N7 test name says host, asserts threads.

## Review round 2 (2026-11-06)

0 blocking, 1 important, 1 nit. Round 1 confirmed fixed; floors-out ratios and `loads (re-reads)` recomputed and right.

- **I1.** The new `--check` step asserts on `llm-wiki/reports/query-batching.md`, which the pipeline still treats as docs-only (`pipeline.yml:3-6`, both `paths-ignore` lists, the `changes` job's grep). A page-only change never runs the check, and the next unrelated code PR goes red. **Decided:** take the page off the docs-only list in every place that keeps it, as the workflow header's own rule says; check that the trigger form really honours the exception; update the header and build-test.md.
- **N1.** A `<batch>.loads.tsv` left from an earlier run now wins silently over a `failed.tsv` entry. Put the reason in the split table's footnote when both exist; test.

## Checkpoints

| step | commit | note |
|---|---|---|
| Task 1 Step 5 | `1db1dc48` | batching-report: a generator for the query-batching page, tested on a fixture tree; test_batching_report 27, calibration tests 104, formatting 39 |
| Task 2 Step 5 | `672fe067` | batching-report: fused vs unfused vs DuckDB per batch size, sf40; test_batching_report 28, formatting 39 |
| Review round 1 fixes | `7f7b51aa` | batching-report: review round 1: DuckDB's floor named and taken off, the page checked in CI; I1-I5, N1-N7; test_batching_report 37, calibration 114, formatting 39, --check 0 |

## VM time

| VM | start (local) | stop (local) | hours |
|---|---|---|---|
