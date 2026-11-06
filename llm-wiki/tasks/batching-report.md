# Report: what query batching buys on the H100 against DuckDB on 128 cores

Kind: production

The chain's output for the human is one page, `llm-wiki/reports/query-batching.md`, in the shape of
the TPC-H section of `reports/benchmark-minimal.md`: load and execute as separate columns,
DuckDB warm alongside.

## Done when

1. **A table per dataset** (TPC-H sf40, TPC-DS sf40), one row per batch size: 3, 10, 20, all.
   Columns:
   - fused load (ms) and fused execute (ms);
   - the sums of the same queries' unfused peacockdb load and execute from sf40-baseline;
   - the sum of their DuckDB warm times;
   - fused total against unfused total, and against DuckDB, as ratios.

   A batch that ran out of device memory shows that, with the cap it ran under. tp4-sized adds
   the unfused tp4_sized sums later. A batch's queries come from sf40-baseline's H100
   `cases.csv`, as query-fuser takes them (its `ok` and `wrong` rows); its `skipped` rows are
   item 4's skipped queries, and its `wrong` rows are counted and noted.
2. **The two hints per batch:** what merging the listed identical subtrees would save, and what
   overlapping execute with load would save. Each is labeled an estimate, with its model in one
   line.
3. **Load against decode:** per dataset, from the profiles, how much of the unfused and the fused
   load is reading and copying, against decoding. That says whether sharing a scan saves copies,
   decodes, or both.
4. **Provenance:** device, driver, CUDA, cuDF and host for the H100 runs, and the pinned host
   file cache they ran with (pinned-loads; bench-rows' `pinned_cache=`: on or off, its capacity,
   the largest resident bytes, and any run that fell back or re-read from disk); DuckDB version,
   threads and host for the CPU runs. Also the queries skipped at sf40 and why, and the protocol
   (runs, which minimum, and that each query process, or each fused batch, prefills the cache
   before any run: peacockdb's warm loads read pinned host memory, DuckDB's its buffer manager
   over a warm page cache).
4a. **One reader per format.** The generator reads `cases.csv` through
   `scripts/calibration/device_cases.py` (sf40-baseline), not a parser of its own. Its reader of
   `duckdb.tsv` is tested against the committed fixture `testdata/fixtures/duckdb-results.tsv`,
   whose owner reader is sf40-baseline's Rust `parse_duckdb_results`, with the same expected
   values.
5. A short reading of the numbers: where the saving comes from, where it stops growing, and what
   the hints say is worth building next. No claim the numbers do not support.

## Scope

| path | change |
|---|---|
| `llm-wiki/reports/query-batching.md` | new |
| `scripts/calibration/batching_report.py`, `scripts/calibration/tests/test_batching_report.py` | the generator the report's tables come from, so no number is typed by hand |

Component-level API: none.

## Signoff (2026-11-06)

Solved under its constraints. Done-when 1-5 and 4a are met. Every table cell was recomputed from the raw files by two reviewers. Three review rounds and a completeness pass. Shortcuts and limits, all recorded in `batching-report-detail.md`:
- The Reading is written by hand from the generated tables, and `--check` does not cover it. tp4-sized must re-read it when it adds its sums.
- A fused batch whose queries are no longer today's `cases.csv` prefix is named stale, not refused. A status change to a sf40 row then calls for an H100 rerun, without turning CI red.
- The page is no longer documentation to CI's path filters, so a page-only change runs the full pipeline.
- The DuckDB comparison is given both as measured and with the per-process floor taken off; TPC-DS first3 is DuckDB-faster only with the floor taken off.
