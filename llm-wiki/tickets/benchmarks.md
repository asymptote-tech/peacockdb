
# Wall-time benchmarks

## Contents

- [#311 (Sirius planning time) Sirius totals include ~130 ms of planning per run, and its GPU part of q6 is 4× peacockdb's](#t311)
- [#226 (benchmark provenance) a benchmark tree does not say which device, driver, CUDA or cuDF produced it](#t226)
- [#69 (DuckDB oracle threads) DuckDB cost oracle: multi-threaded golden generation for larger SF](#t69)

<a id="t311"></a>
### #311 (Sirius planning time) Sirius totals include ~130 ms of planning per run, and its GPU part of q6 is 4× peacockdb's
Found on sirius-bench's first sf40 measurement (2026-11-10, H100, Sirius `stable` 8d17afe,
tpch q6, one Sirius process, tables pinned `tier='parquet'`). q6 reports 339.4 ms (runs
336.5–393.0) against peacockdb's 58.8 ms tp1_single total and DuckDB's 252 ms. Sirius's own log
(`testdata/calibration/sirius/tpch.sf40/log/`, info level) splits every timed run the same way:

| part of each run (~360 ms) | time | log lines |
|---|---|---|
| planning: DuckDB parse and bind, then Sirius's physical plan | ~120–140 ms, on every run | `QueryBegin: … SQL:` → `Sirius physical plan generated successfully` |
| GPU execution: scan from the pinned cache, decode, filter, aggregate | ~230–245 ms | `[window] begin` → `[window] end` |

1. **The two totals measure different things.** peacockdb's total is `device_us`, device-side load
   plus execute; planning is outside it. Sirius's total is the harness's wall time around the
   statement and its Arrow fetch, so about a third of it is planning, re-done on every run (no
   plan is reused across statements). Part of it is likely DuckDB binding the `read_parquet` view,
   which reads lineitem's footer: creating that view alone took 40 ms. Unverified.
2. **The GPU part is still about 4× peacockdb's.** Each run reads 894 chunks from Sirius's pinned
   cache (`prefetching_cache` `reads=894 hits=1118` per run, `h2d=0` throughout, which is
   undocumented). If q6's four lineitem columns are a few GB compressed, 235 ms is about 10 GB/s,
   against the 41–49 GB/s peacockdb's pinned loads copy at (`pinned-loads-detail.md`, "Copy
   rate"). Small chunks, Sirius's 4 pipeline threads and per-chunk scheduling are suspects, none
   measured. The readahead manager logs `fell_behind=1` on the warm-up.
3. Logging is not the cause: q6 was 341.0 ms with Sirius's log off and 339.4 ms with it on.

To do:
- **Take planning out of Sirius's total.** Report the `[window] begin` → `[window] end` span of
  each timed run (the log has it for every query) as Sirius's total, keeping the wall time beside
  it in `sirius.tsv` for reference. Re-render the page, the ratio line and `benchmarks.csv`, and
  say in build-test.md what Sirius's total now holds and how it compares with peacockdb's
  `device_us` and DuckDB's CLI wall time.
- **Investigate the slow GPU part.** One nsys capture of q6 (and of one join-heavy query, q9)
  under the same container settings: the copy sizes and rate from the pinned cache, decode and
  kernel time, idle gaps between chunks. Measure what `scan_manager` chunk size, task size and
  `executor.pipeline.num_threads` change. Confirm, from the trace rather than the counters, that
  every timed run copies the pinned bytes host to device, since `h2d=0` says otherwise.
- **Check where planning goes**, if planning stays in any reported number: DuckDB's bind of the
  `read_parquet` view against Sirius's plan generation, and whether a prepared statement or a
  table in place of the view removes the footer read.

<a id="t226"></a>
### #226 (benchmark provenance) a benchmark tree does not say which device, driver, CUDA or cuDF produced it
The H200's tree and record predate provenance, and no file carries it yet. The harness now writes
`device=`, `driver=`, `cuda=`, `cudf=` and `host=` into every trailer and record heading. It files
results under `benchmark-results/<slug>/`, and a new record is keyed and pinned by its device. The
first run that writes the five is sf40-baseline's, on the H100. The H200's tree is under `h200/`
with none of them. Its two records are still keyed `<dataset>.sf40/<sha256>.tsv` and pinned as
`<dataset>.sf40/records.tsv`.

Moving the H200 record under `h200/` is one change:
- copy each object to `s3://calibration/h200/<dataset>.sf40/<sha256>.tsv` server-side, tpch and
  tpcds, and read each back against its sha256;
- rename the two lines of `testdata/calibration/records.sha256` to `h200/<dataset>.sf40/records.tsv`;
- `create_nsys_profile.sh` reads a record at `calibration/<device>/<dataset>.sf<sf>/`;
- `build-test-shadgpu.sh`'s header documents the per-device tree, not the flat one;
- the H200's Nsight pass runs from a5c852e4 on master, whose case lines are
  `h200/cases.csv`'s rows:
  `create_nsys_profile.sh --metrics` takes every case of the checkout's list;
- once the H200 is re-run with provenance, both `PREDATES_PROVENANCE` constants and
  `h200/cases.csv` go: `nsys_hbm.py` takes a record naming no device as the H200's, and the
  golden lets `h200/` lack it.

Two hazards until then:
- The GPU host's own tree still holds flat `tpch.sf40/` and `tpcds.sf40/`. No push or pull prunes
  it: both scripts' `--pull-benchmarks` rsync it additively. So the first old-path run with this
  binary brings them home beside `h200/`, and two goldens go red. A filtered run there writes an
  `h200/` holding only the cases it timed, and the pull copies it over the committed files.
- A publish adds an `h200/` pin line beside the flat one: `publish_record` replaces only a line
  of the same path. `fetch_record.sh tpch.sf40` then goes on reading the old record without
  saying so.

<a id="t69"></a>
### #69 (DuckDB oracle threads) DuckDB cost oracle: multi-threaded golden generation for larger SF
`gen_duckdb_cost.sh` pins `PRAGMA threads=1` because `operator_rows_scanned` scales with
thread count (`output_bytes`/`output_rows` are thread-invariant). Fine at sf1, too slow
at sf10/sf100. Simplest fix: parallelize across queries, keep per-query threads=1; or
drop/normalize the thread-sensitive field.
