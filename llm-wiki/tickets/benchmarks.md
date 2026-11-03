
# Wall-time benchmarks

## Contents

- [#226 (benchmark provenance) a benchmark tree does not say which device, driver, CUDA or cuDF produced it](#t226)
- [#69 (DuckDB oracle threads) DuckDB cost oracle: multi-threaded golden generation for larger SF](#t69)

<a id="t226"></a>
### #226 (benchmark provenance) a benchmark tree does not say which device, driver, CUDA or cuDF produced it
The H200's tree and record predate provenance, and no file carries it yet. The harness now writes
`device=`, `driver=`, `cuda=`, `cudf=` and `host=` into every trailer and record heading. It files
results under `benchmark-results/<slug>/`, and a new record is keyed and pinned by its device. The
first run that writes the five is containers Task 11. The H200's tree is under `h200/` with none of
them. Its two records are still keyed `<dataset>.sf40/<sha256>.tsv` and pinned as
`<dataset>.sf40/records.tsv`.

Moving the H200 record under `h200/` is one change:
- copy each object to `s3://calibration/h200/<dataset>.sf40/<sha256>.tsv` server-side, tpch and
  tpcds, and read each back against its sha256;
- rename the two lines of `testdata/calibration/records.sha256` to `h200/<dataset>.sf40/records.tsv`;
- point `RECORD` in `scripts/exec_model/optimizer/call_cost.py` at `calibration/h200/tpch.sf40/`;
- `exec-model-corpus.yml` fetches `h200/tpch.sf40`, not `tpch.sf40`;
- `create_nsys_profile.sh` reads a record at `calibration/<device>/<dataset>.sf<sf>/`;
- `build-test-shadgpu.sh`'s header documents the per-device tree, not the flat one;
- the H200's Nsight pass runs from f3db66b1 (02f6a4d8 on ENS-optimizer), whose case lines are
  `h200/cases.inc`'s:
  `create_nsys_profile.sh --metrics` takes every case of the checkout's list;
- once the H200 is re-run with provenance, both `PREDATES_PROVENANCE` constants and
  `h200/cases.inc` go: `nsys_hbm.py` takes a record naming no device as the H200's, and the
  golden lets `h200/` lack it.

Two hazards until then:
- The GPU host's own tree still holds flat `tpch.sf40/` and `tpcds.sf40/`. No push or pull prunes
  it: both scripts' `--pull-benchmarks` rsync it additively. So the first old-path run with this
  binary brings them home beside `h200/`, and two goldens go red. A filtered run there writes an
  `h200/` holding only the cases it timed, and the pull copies it over the committed files.
- A publish adds an `h200/` pin line beside the flat one: `publish_record` replaces only a line
  of the same path. `fetch_record.sh tpch.sf40` and `call_cost.py` then go on reading the old
  record without saying so.

<a id="t69"></a>
### #69 (DuckDB oracle threads) DuckDB cost oracle: multi-threaded golden generation for larger SF
`gen_duckdb_cost.sh` pins `PRAGMA threads=1` because `operator_rows_scanned` scales with
thread count (`output_bytes`/`output_rows` are thread-invariant). Fine at sf1, too slow
at sf10/sf100. Simplest fix: parallelize across queries, keep per-query threads=1; or
drop/normalize the thread-sensitive field.
