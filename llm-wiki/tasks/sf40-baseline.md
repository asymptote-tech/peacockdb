# sf40 tp1_single on the H100, and DuckDB on 128 cores, for every query the batches use

Kind: production

Query batching is measured against two baselines: peacockdb running the same queries one at a
time, and DuckDB on 128 cores. DuckDB has no batching, so its side of a batch is the sum of its
warm single-query times. This task records both. It also takes over containers' benchmark run on
an 80 GB device (task 11 of `origin/ENS-containers:llm-wiki/tasks/containers-impl.md`), with this
chain's H100 as the device.

## The query set

The candidates are the main case list's tp1_single cells
(`peacockdb-core/tests/common/corpus_benchmark_cases.inc`), in its order:

- **TPC-H:** the 44 lines carrying `tp1_single`: q1-q22 and the micro queries. TPC-H+V's vector
  probes are not in the list and are out. The batching set is q1-q22 of them.
- **TPC-DS:** the 82 lines carrying `tp1_single`. The batching set is all of them.
- A query that fails at sf40 (error, OOM or timeout) is skipped. It is written as a failed case
  with its reason, as containers' format does, and is listed in the detail file.
  A skipped query is not a ticket unless it is a wrong answer.

The one ordered list is the H100 tree's `testdata/benchmark-results/h100-80gb-hbm3/cases.csv`,
in containers-rebase's per-device format: header `dataset,sf,query,modes,status,note`, `modes`
`|`-joined and empty when skipped, `status` one of `ok`, `skipped`, `wrong`, `note` the skip
reason or the `#NN` ticket. A test, `the_h100_list_is_the_lists_measured`, writes it from the
H100 results tree under `UPDATE_CANONICAL=1`, never by hand, and holds it to the tree otherwise:

- a query with a non-failed section in a mode gets an `ok` row carrying those modes;
- a query whose sections are all failed gets a `skipped` row, no modes, its reason as the note;
- a wrong answer keeps its row as `wrong`, the ticket `#NN` as the note. The tree knows no
  tickets, so the writer carries each `wrong` mark over from the file as it stands.

query-fuser, tp4-sized and batching-report read this file. No second list is kept. The tree check
reads it by containers' rule: a device tree that holds a `cases.csv` is held to it, one without
to the whole main list. No exception names the H100.

## Done when

1. `testdata/benchmark-results/h100-80gb-hbm3/{tpch,tpcds}.sf40/tp1_single.benchmark.txt` hold every query
   above, in bench-rows' format, with pinned-loads in the build. The tree check holds
   `h100-80gb-hbm3` (the harness names the tree from the card) to its own `cases.csv`, so a tree
   carrying only tp1_single passes; that list's `ok` and `wrong` rows must be cases of the main
   list at the modes they carry.
2. DuckDB 1.5.4 timings for the same queries on `nebius-cpu128`, using `duckdb_minimal.sh`'s
   protocol: native storage, page cache warmed and its residency checked, all cores, 2nd-min of 6.
   The script is extended to the corpus query text, not forked. The results go in a per-device
   tree, `testdata/benchmark-results/duckdb-cpu128/{tpch,tpcds}.sf40/`, with DuckDB's version,
   threads and host as provenance.
3. Each peacockdb answer at sf40 matches DuckDB's answer at sf40, by the corpus comparison rules.
   Where it does not, a ticket is filed and the query's `cases.csv` row becomes `wrong` with
   note `#NN`.
4. The skips with reasons and the run commands are in the detail file.
5. **The report page** (bench-rows' `testdata/benchmark-report.html`) gains a DuckDB column: per
   (dataset, query) at sf40, the warm 2nd-min from `duckdb-cpu128/<ds>.sf40/duckdb.tsv`, or its
   failed mark, beside the devices, and the page shows the H100 tree. `duckdb.tsv` has one
   owner reader, in Rust (`test_support`), which the page uses; batching-report's Python reader is
   tested against the same committed fixture. The page is regenerated in the commit that adds
   the trees.

**Profiles.** Every case this task benchmarks is also profiled on the H100. Both Nsight passes run
(`create_nsys_profile.sh`): the trace pass's `calls.tsv` and pinned-loads' load breakdown, and the
metrics pass's `hbm.tsv`. In each case's profile, a reader sees how much of every load is reading
and copying, against decoding. The derived files are committed per device and dataset under
`testdata/calibration/h100-80gb-hbm3/`. Captures stay on the host or in the scratch area; they are too large to
commit. A case whose capture fails is listed with the reason, as a failed benchmark case is.

## Scope

| path | change |
|---|---|
| `benchmarks/duckdb_minimal.sh`, `benchmarks/tests/` (new) | the corpus queries, both datasets; its tests |
| `testdata/benchmark-results/h100-80gb-hbm3/`, `testdata/benchmark-results/duckdb-cpu128/` | results |
| `testdata/calibration/h100-80gb-hbm3/` | the profiles' derived files |
| `peacockdb-core/tests/test_corpus_goldens/benchmark.rs` | a device list's rows held to the main list; the test that writes and checks the H100's `cases.csv`; the page's DuckDB input |
| `testdata/benchmark-results/h100-80gb-hbm3/cases.csv` | the ordered query list, skips and wrong answers as rows |
| `peacockdb-core/src/test_support/` (`duckdb_results.rs`, new; `benchmark_report.rs`), `testdata/benchmark-report.html`, `testdata/fixtures/{device-cases.csv,duckdb-results.tsv}` | the DuckDB column; the formats' shared fixtures |
| `scripts/calibration/device_cases.py` (new) | the device list's one reader for scripts |
| `llm-wiki/build-test.md` | how to re-run both sides |
| `peacockdb-core/src/test_support/` (`corpus_benchmark.rs`, `corpus_gpu.rs`, `corpus_golden.rs`, `duckdb_oracle.rs`, `device_cases.rs`), `tests/test_cpu_corpus.rs`, `testdata/duckdb_result.py`, `testdata/goldens/{tpch,tpcds}.sf40/` | recording and comparing the sf40 answers (done-when 3); a per-case timeout |
| `scripts/env/` (`bench-each.sh`, `run-gpu.sh`), `scripts/calibration/` (`join_records.py`, `publish_record.sh`), `scripts/create_nsys_profile.sh` (`--cases`, `--device`) | one process per query, joined records, profiling a subset |

Component-level API: none.

## Constraints

- The H100's watcher stops one process after 4 hours. A long run goes in segments with
  `PEACOCK_BENCH_RESUME=1`, and the segments' records are joined, as containers planned.
- Both VMs are stopped when the run ends.
