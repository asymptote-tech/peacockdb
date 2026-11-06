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

## Checkpoints

| step | commit | note |
|---|---|---|
| Task 1 Step 5 | `1db1dc48` | batching-report: a generator for the query-batching page, tested on a fixture tree; test_batching_report 27, calibration tests 104, formatting 39 |

## VM time

| VM | start (local) | stop (local) | hours |
|---|---|---|---|
