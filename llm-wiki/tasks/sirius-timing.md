# Sirius timed without its planning, and why its GPU part is slow

Kind: production

The report page's Sirius group (sirius-bench in `llm-wiki/archive/archived-tasks.md`) does not
measure what peacockdb's columns measure. Two things are wrong with it, both in #311 (Sirius
planning time):

- **Planning is inside Sirius's total.** The runner times the wall around
  `con.execute(sql).to_arrow_table()`. Every timed run re-does DuckDB's parse, bind and optimize
  and Sirius's physical plan: about 120–140 ms of q6's ~340 ms. peacockdb plans once, outside
  its runs (`corpus_benchmark.rs`, `measure`).
- **Sirius's GPU part is slow, and nobody knows why.** q6's GPU part is ~235 ms against
  peacockdb's 58.8 ms. Short queries pay a near-constant ~200–250 ms (tpcds q42, q52, q55:
  ~320–345 ms against peacockdb's ~127 ms and DuckDB's ~70 ms). Heavy ones do not (tpcds q64:
  0.8 s against peacockdb's 4.9 s). The suspects are all in how data reaches the device:
  - the copy from Sirius's pinned cache: about 10 GB/s by a rough estimate, against
    peacockdb's 41–49 GB/s;
  - the cache's `h2d=0` counter on every run;
  - the reads outside the pin on 34 of the 84 timed rows (`O_DIRECT` from disk, most likely
    bloom filters; tpch q19 2081 per run, q10 972);
  - a parquet footer re-read whenever DuckDB binds a `read_parquet` view (making that view
    took 40 ms on lineitem).

This task finds out what the slow part is, fixes whatever lies in how the harness runs Sirius,
and reruns Sirius at sf40 on the H100 with the planning taken out. The goal is the closest
comparison of the two engines the harness can make, without changing Sirius.

## What is compared with what

Each engine keeps its own clock, and both clocks start after planning:

| | peacockdb | Sirius |
|---|---|---|
| **run** (new, the comparison) | `run_us`: the measured execution end to end, `GpuUnload` copying the root to the host included | the `[window] begin` → `[window] end` span in Sirius's log: the execution window, its result materialized on the host included, its Arrow conversion not |
| **total** (as today) | `device_us`: Σ of the tree's device time, host gaps out | the wall around the statement and its Arrow fetch, planning included |

`run_us` is already in every committed `tp1_single.benchmark.txt` section of the H100
(`benchmark_trailer.rs`), so peacockdb is not rerun.

At Sirius `8d17afe` the window opens in `PhysicalSiriusExecution::GetDataInternal`
(`src/transparent/physical_sirius_execution.cpp`) and closes after the result is materialized.
Inside it Sirius either reuses the plan it validated at prepare (`Transparent execution: reusing
finalize-validated Sirius plan`) or rebuilds it (`… rebuilding Sirius plan at execute …`). A
rebuild puts planning back inside the window.

## Done when

1. **The slow GPU part is explained, from measurements.** One finding per suspect in the
   detail file, each with its numbers, against q6 and the queries named:
   - **The copy.** An Nsight Systems capture of q6, q9 and q19 under the container's settings:
     the bytes and rate of each host-to-device copy from the pinned cache, the decode and
     kernel time, and the idle gaps between chunks. Whether every timed run copies the pinned
     bytes, from the trace and not the counters, and what `h2d=0` counts.
   - **The settings.** What `scan_manager`'s chunk and task sizes and
     `executor.pipeline.num_threads` change, on q6 and q9.
   - **The reads outside the pin.** q19 and q10 timed with the misses and without them. That
     means one of: parquet without bloom filters, the parquet on tmpfs, or something the
     measurement shows works better. What a miss reads, in bytes.
   - **The footer.** The planning span of q6 with `parquet_metadata_cache` on, and with each
     table registered other than as a `read_parquet` view, if Sirius's pin allows it.
   - **The log.** q6 with Sirius's log at `info` (the window lines need it) and at `warn`. The
     window's lines are `info`, so the timed runs log at `info`; the cost goes in the detail
     file, and on the page if it is above the runs' spread.
2. **The run configuration is decided from those findings.**
   - A departure from Sirius's defaults is kept when all of these hold:
     - it shortens Sirius's run or its total, or reads less outside the pin;
     - it lengthens neither clock beyond the runs' spread;
     - it is a setting or a data layout any Sirius user has. No Sirius code change.
   - "Changes no answer" is judged on the full rerun (Done when 5): no row `ok` today may turn
     `wrong`. If one does because of a kept departure, the departure is dropped and that
     dataset is rerun. A `failed` row that now runs is welcome.
   - A data layout other than sf40's is made on the VM from the sf40 set by a committed step
     of `run-sirius.sh prepare`. It differs from sf40 only where the departure needs it: the
     same row groups, pages, encodings and compression, checked against the parquet metadata.
     The detail file records each file's sha256. A layout that cannot be made this way is not
     kept. Serving the same files from tmpfs is not a layout change.
   - **Both engines start each run from the raw parquet bytes in pinned host memory.** If the
     capture shows Sirius keeping table data on the device between runs, a setting that makes
     every run copy from the host is kept although it makes Sirius slower: comparability comes
     first. That case gets its own finding: q6 and tpcds q64 timed both ways.
     - `bench-sirius.sh` keeps the device-resident way as a flag, off by default (the name is
       the builder's). The run's `# ` line names which way it ran. `--resume` and `--list`
       refuse a file of the other way.
     - If no setting does it, the run stays as Sirius runs it. The page says so, and a ticket
       records it.
   - `docker/sirius-config.yaml`, the runner or the data layout changes accordingly.
   - Each departure is listed in `build-test.md`'s Sirius protocol with its reason and its
     measured effect, beside the three that are there today.
   - Sirius may end up reading files laid out differently from peacockdb's (no bloom filters,
     say). Then the page says so.
3. **The runner records each timed run's window.**
   - `docker/sirius-bench.py` reads, for every timed run, the span between that run's
     `[window] begin` and `[window] end` (outcome `ok`) in Sirius's log, beside the wall it
     times today.
   - The window is that run's: the runner attributes windows to runs the way it attributes the
     warm-up's evidence, between label lines.
   - A run whose window shows a plan rebuild keeps its time, and the row's note says so.
   - A timed run with no window in the log fails its row, naming that.
   - The log's timestamps are milliseconds (`%e`), so a span has a 1 ms resolution, and
     `build-test.md` says so.
   - Offline tests over the fake Sirius log hold all of it, as `test_sirius_bench.py` holds the
     GPU evidence today.
4. **`sirius.tsv` carries both times.** Two columns follow `runs_ms`:
   - `run_ms_2nd_min`: the second-smallest window span;
   - `run_runs_ms`: every timed run's window span, comma-joined.

   `ms_2nd_min` and `runs_ms` keep the wall. If Done when 2 adds the device-resident flag, the
   `# ` line gains a key naming the way the run went. The reader, the Rust writer, the fixture
   and the runner change together.
5. **Sirius is rerun at sf40 on the H100 in the final configuration**, both datasets, every row
   of the H100's `cases.csv`, through `bench-sirius.sh`, at the same Sirius pin. Answers are
   held to DuckDB's as today, and statuses are set by
   `every_sf40_sirius_status_meets_its_answer`. A row may change status. Each change is listed in
   the detail file with its reason.
6. **The page.** Each dataset's main table:
   - **peacockdb's group** gains `run (ms)` after `total (ms)`.
   - **Sirius's group** becomes `run (ms)` and `total (ms)`, its two always-`—` columns
     (`load`, `execute`) gone.
   - **The ratio lines** are two:
     - `Σ run Sirius / Σ run peacockdb = <r> over <n> of <m> queries`, over the rows `ok` in
       both;
     - today's `Σ total Sirius / Σ total DuckDB` line, unchanged: both of those totals include
       planning.
   - The Sirius paragraph says what each of its two times holds, each departure from Sirius's
     defaults, and what is left read outside the pin.
   - The Modes, fused and h200 tables do not change.
7. **`testdata/benchmarks.csv`** gains `run_ms` after `total_ms`:
   - peacockdb: `run_us` in ms;
   - Sirius: the window;
   - DuckDB: empty.

   `total_ms` keeps what it holds today for every engine. It is regenerated by the test that
   writes the page.
8. **The wiki.**
   - `build-test.md`: Sirius's protocol, its file, the page and the counts.
   - #311 (Sirius planning time): closed into `llm-wiki/archive/archived-tickets.md`, with
     what was found. Anything found that stays unfixed gets a ticket of its own.
9. **Done is the usual rule:** `done` with the PR's CI green.

## Scope

| path | change |
|---|---|
| `docker/sirius-bench.py`, `docker/sirius-config.yaml`, other `docker/sirius*` inputs if the configuration needs them | the window per timed run; the decided configuration |
| `scripts/env/bench-sirius.sh`, `scripts/env/run-sirius.sh`, `scripts/env/run-sirius-vm.sh` | only if the configuration or the data layout needs them; a layout's step in `prepare` |
| `scripts/env/tests/` | the runner's and scripts' offline tests |
| `peacockdb-core/src/test_support/sirius_results*`, `peacockdb-core/src/test_support/benchmark_report*` | `sirius.tsv`'s two columns; the page's groups and ratio lines; `benchmarks.csv`'s `run_ms` |
| `peacockdb-core/tests/test_corpus_goldens/`, `peacockdb-core/tests/test_cpu_corpus/sirius.rs` | the page and CSV test; Sirius's statuses |
| `testdata/benchmark-results/h100-80gb-hbm3/{tpch,tpcds}.sf40/sirius.tsv`, `testdata/goldens/{tpch,tpcds}.sf40/sirius-result.txt`, `testdata/fixtures/sirius-results.tsv`, `testdata/benchmark-report.html`, `testdata/benchmarks.csv` | results and generated files |
| `llm-wiki/build-test.md`, `llm-wiki/tickets/benchmarks.md`, `llm-wiki/tickets.md`, `llm-wiki/archive/archived-tickets.md` | Done when 8 |

Component-level API: none. No engine change in peacockdb.

## Constraints

- **No Sirius change.** Same pin (`stable`, `8d17afe`), its prebuilt extension, no patch. A
  finding that needs one goes in the detail file and a ticket, not in the run.
- **The H100 for at most 3 hours of VM time in all,** `prepare` and every rerun included,
  started and stopped by the run through `scripts/nebius.sh`, each start and stop logged in the
  detail file. It is stopped after every session, whatever the outcome. An experiment pins
  only the tables its queries read. A step that would take the total past 3 hours is not
  started: the run stops and asks.
- **One change at a time** in Done when 1: each finding compares a run with and without one
  setting, other things equal. A number from a single run is marked as such.
- An image change goes through `image.sh` and `images.yml` under the input-label rules, as the
  Sirius image did.
- peacockdb's and DuckDB's results stay as they are. No test is weakened.

## Not in this task

Sirius's failing queries (its coverage); a newer Sirius; a Sirius load/execute split or peak;
Sirius on another device or mode; fused batches; any change to peacockdb's engine or its
benchmark.
