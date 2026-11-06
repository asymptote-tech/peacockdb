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

## Checkpoints

| step | commit | note |
|---|---|---|
| Task 1 Step 5 | `1db1dc48` | batching-report: a generator for the query-batching page, tested on a fixture tree; test_batching_report 27, calibration tests 104, formatting 39 |

## VM time

| VM | start (local) | stop (local) | hours |
|---|---|---|---|
