
# Wall-time benchmarks

## Contents

- [#317 (Sirius metadata copy) Sirius copies a parquet file's whole row-group metadata in every scan task, so most of its run is host CPU](#t317)
- [#226 (benchmark provenance) a benchmark tree does not say which device, driver, CUDA or cuDF produced it](#t226)
- [#69 (DuckDB oracle threads) DuckDB cost oracle: multi-threaded golden generation for larger SF](#t69)

<a id="t317"></a>
### #317 (Sirius metadata copy) Sirius copies a parquet file's whole row-group metadata in every scan task, so most of its run is host CPU
Each scan task builds a `hybrid_scan_reader` from the file's `FileMetaData`, and that constructor
copies the whole `vector<RowGroup>`. Meanwhile the GPU idles for most of the execution window.

From Nsight captures at sf40 on the H100 (Sirius `8d17afe`, tables on tmpfs):
- q6, a 222 ms run: kernels and copies take about 21 ms; the first ~148 ms show no GPU work.
  The captures are in the calibration bucket (`sirius-timing-detail.md`, "How the experiments ran").
- In each capture's last timed run the copy is on the stack of 288 of q6's 580 CPU samples, 586
  of q19's 2632 and 976 of q9's 3185, on scan_manager threads in 248, 472 and 654 of them.
- No setting removes it. Measured: `executor.pipeline.num_threads`,
  `executor.scan_manager.num_threads`, `operator_params.scan_task_batch_size`, the log level and
  `parquet_metadata_cache` (`llm-wiki/tasks/sirius-timing-detail.md`, findings 2, 3, 5 and 6).

Where, in Sirius `8d17afe`: the footer is parsed once per file and shared as a
`shared_ptr<FileMetaData const>` (`src/scan_manager/sirius_scan_manager.cpp:1404`), but every
`hybrid_scan_reader` built from it copies it. Two scan-path callers build one:
- `column_chunk_ranges` (`src/op/scan/parquet_materialize.cpp:38`), once per slice of every
  split, for one call of `all_column_chunks_byte_ranges`
  (`parquet_batch_coalescer::emit_current`, `src/op/scan/parquet_gpu_ingestible.cpp:399`);
- `parquet_gpu_ingestible::build_file_scan_info` (`parquet_gpu_ingestible.cpp:874`), for its
  row-group filtering.

Over each whole capture, of the samples whose stack unwinds above the constructor,
`column_chunk_ranges` is the caller in 60 of q6's 100, 171 of q9's 232 and 134 of q19's 190, and
`build_file_scan_info` in the rest.

The fix is in Sirius's scan path, or in a libcudf reader that takes the metadata without a copy.
Closed by a Sirius commit whose scan copies no row-group metadata per task, shown by capturing q6
and q9 again as finding 2 did: the copy gone from their windows, and their runs on this page rerun
at that commit.

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
  it: `build-test-shadgpu.sh --pull-benchmarks` rsyncs it additively. So the first old-path run with this
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
