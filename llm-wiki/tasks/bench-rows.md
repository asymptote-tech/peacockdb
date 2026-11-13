# Benchmark results carry each node's batch rows and bytes, and the load apart from the execute

Kind: production

A `.benchmark.txt` section is the plan tree with `time_us` and `total_us` under each node. The
`.cpu.txt` goldens print each node's `batch_rows` and `batch_bytes`; the benchmark does not, though
the harness already holds the full `RunReport` and reads only `report.emitted.len()`
(`test_support/corpus_benchmark.rs`). Two later tasks need those numbers from the sf40 run:

- query-fuser reads each query's live bytes to decide how many fused queries may run at once;
- batching-report splits load from execute, the way `reports/benchmark-minimal.md` does.

Load today is only the `GpuLoadParquet` node's time, inside `run_us`.

Containers already closes #226 (benchmark provenance) and writes the per-device tree,
`testdata/benchmark-results/<device>/<dataset>.sf<sf>/<mode>.benchmark.txt`. This task builds on
both and does not redo them.

## Done when

1. Each node in a `.benchmark.txt` section carries `batch_rows` and `batch_bytes`, by lane then by
   batch, in the same notation as `.cpu.txt` (`architecture.md` "Node display"), taken from the
   reported run (the 2nd-smallest).
2. The `--- run ---` trailer adds `load_us` (the sum of the `GpuLoadParquet` nodes' `total_us`) and
   `execute_us` (`device_us − load_us`), plus `peak_bytes`: the most bytes the run's memory
   accountant held at once. It is the number query-fuser's hint reads. If the accountant cannot
   give a true peak without a change in the driver, the builder says so and gives the closest
   bound it can, named as a bound. `load_us` stays the device-side load: the benchmark prefills
   pinned-loads' host file cache before its first case, so every round's load reads pinned
   memory. The trailer also carries `pinned_cache`, the cache's state at the end of the case
   (pinned-loads' `PinnedCache`, e.g. `on cap=136.9GiB resident=12.3GiB … source=default`, or
   `off source=env`), followed by `prefill_files=<n> prefill_bytes=<n> prefill_us=<n>`: the
   prefill's host IO, reported apart from the device load. A round that filled, reloaded, evicted
   or fell back fails its case instead of being timed.
3. The parsers and checks that read the tree accept the new fields, and the existing H200 files
   still parse. Re-measuring them is not required.
4. `build-test.md` documents the format.
5. **A benchmark report page**, `testdata/benchmark-report.html`, committed. For every device
   tree under `testdata/benchmark-results/`, one row per (dataset, query, mode), the devices side
   by side: load, execute, total and `peak_bytes` (labelled a lower bound), or the case's failed
   or skipped mark with its reason (from the device's `cases.csv` where it has one), and a link
   to the section. A Rust test in `test_corpus_goldens`, `the_benchmark_report_matches_the_trees`,
   writes it through item 3's reader, `parse_benchmark_section`, so the format has one reader: it
   checks the committed page equals the render, and rewrites it under `UPDATE_CANONICAL=1`. The
   output is deterministic: no timestamp, no machine path. Its look is `testdata/report.html`'s
   (`scripts/ticket_widget.py`). Later tasks extend it: sf40-baseline a DuckDB column,
   tp4-sized its mode's rows, query-fuser a fused-batches table. `build-test.md` says that a
   commit that changes a results tree regenerates the page, and that the test enforces it.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/test_support/corpus_benchmark.rs`, `benchmark_trailer.rs`, `plan_text/bench_text.rs` | the fields |
| `peacockdb-core/src/plan_text/node_text.rs`, `plan_text/run_text.rs` | the batch-list renderer, shared rather than copied |
| `peacockdb-core/src/executor/` (accounting) | only if a peak needs recording |
| `peacockdb-core/tests/test_corpus_goldens/benchmark.rs`, `scripts/calibration/` | readers of the format |
| `peacockdb-core/src/test_support/benchmark_report.rs` (new), `peacockdb-core/tests/test_corpus_goldens/benchmark.rs` | the report page's renderer and its test |
| `testdata/benchmark-report.html` (new) | the page |
| `llm-wiki/build-test.md` | the format; the page and its regeneration rule |
| `llm-wiki/architecture.md` "Node display" | the benchmark line's new fields |

Component-level API: none.

## Signoff (2026-11-05)

Solved under its constraints: per-node `batch_rows`/`batch_bytes`; `load_us`, `execute_us`,
`peak_bytes`, the pinned cache and the prefill in the trailer; one reader and a committed page of every
device tree; the format read back from one H100 case (tpch sf40 q3); a resumed run re-measures any
section not in the current format. Shortcuts: the page does not mark unpinned loads (every H200 load
predates the cache; handed to sf40-baseline); writer and reader are tied by one real H100 section,
not a rust-only round trip; `peak_bytes` is a lower bound only as far as the join session's
per-row model holds; the page was read as text, never opened in a browser.
