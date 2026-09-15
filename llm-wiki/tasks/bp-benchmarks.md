# bp-benchmarks — the corpus benchmark, rebuilt on the component layout

Kind: production

A benchmark run over the corpus at sf40 under CUDA-event timing: one timing tree per
(dataset, mode), one record row per cuDF call, two Nsight captures read down to what a call
splits into and what it moved, and the panels drawn from all of it. PR #139 built this on
`c42b601`, before `drop-mode-name` (#141) and `module-layout` (#143) landed; it is rebuilt on
today's master under the decisions below, every one of which removes something the first
version carried. Nothing outside `llm-wiki/` says `bp` — `scripts/residue-gate.sh` stays empty.

## The four products

**The tree**, `testdata/benchmark-results/<dataset>.sf<sf>/<mode>.benchmark.txt`, one file per
(dataset, mode), a `== <query>` section each. A section is the plan tree with one line per node
under it: `time_us=[[…],[…]] total_us=N` — lanes outermost, one entry per call the lane made,
each entry the call's device microseconds summed over its output partitions, `1` where the
clock rounded a region to zero — every call opens one. Then a `--- run ---`
trailer: `run_us` (the chosen execution end to end, after planning), `device_us` (Σ of the
tree's `total_us`), `runs=[…]` (every measured execution, in order), `build=release`,
`allocator=` (what `install_rmm_pool` reported). The reported execution is the second-smallest
by `run_us` of ten, after one discarded warm-up; both counts are constants in the harness.
The case list is `tests/common/corpus_benchmark_cases.inc`:
`corpus_query_benchmark!(tpch, 40, q6, tp1_single | tp4_sized)` and q19 at `tp1_single`. A
(query, mode) timed here must be enabled on the device in `corpus_cases.inc`, and a rust-only
test says so.

**The record**, `testdata/calibration/records.tsv`, one row per cuDF call, every measured
execution, seventeen columns: `dataset sf query mode node_seq node_type lane recipe_seq
recipe_kind call_index run_index in_rows in_bytes out_rows out_bytes host_us device_us`.
`node_seq` is post-order, `lane` the driving lane, `call_index` the number C++ counts to per
seq. The `#` heading carries what is constant across a run — `timing_mode=events`,
`build=release`, `allocator=…`, `capture=none|trace|metrics` — and an append under a
different heading is refused. No cell is empty: `out_rows` comes from `NodeStats`, `out_bytes`
from `logical_size_from_schema` with the schema the executor holds — the two calls whose output
no batch is built from (an aggregate before its finalize, the concat before a merge) are priced
with the node's `intermediate()` — and a middle call's `in_*` is the call before it. The
harness checks each execution's rows against the plan (`rows_match_the_recipes`) before it
writes them.

**The captures**, `scripts/create_nsys_profile.sh --trace | --metrics` (neither means both),
against the binaries `build-test-shadgpu.sh` pushed. The trace pass feeds `nsys_calls.py`,
which writes `testdata/calibration/calls.tsv`: one row per (case, node_seq, call, depth), the
case being `(dataset, sf, query, mode)` read off the harness's NVTX range — the first version
keyed without it and merged three cases into one row set. The metrics pass feeds
`nsys_hbm.py`, which joins `hbm_bytes` onto `records.tsv`'s tuple and writes
`testdata/calibration/hbm.tsv`; `--peak-bw` is required and comes from the script's knob. The
metrics pass's own record stays on the host beside its capture — it is `records.tsv` with
other microseconds, and nothing reads it but the join. Neither `.sqlite` export is committed.

**The panels**, `scripts/calibration/plot.py --record … --hbm … --out-dir
testdata/calibration/plots`: `load/`, `compute/`, `spread/`, `query/` (two terms, host and
device, side by side and never summed), `icicle/`, `hbm/`, and `index.html`. It requires every
column it reads, refuses records whose `# run:` lines differ, and states the matplotlib
version its output was rendered with — the PNGs embed it, so a re-render on another version
rewrites every file with unchanged data.

## The instrument

`NodeTiming::{Off, Events}`, process-global, `Off` by default. Under `Events` every per-call
entry point — `execute_node`, `execute_scan_rowgroups`, `slice_handle`, `result_from_handle` —
opens one region per output partition: the host clock around the whole call, a CUDA event
pair recorded on the default stream at region open and close, no synchronisation inside. The
region carries `seq, partition, call_index, host_us, device_us` and nothing else. The first
version split the host side at the operator's first device touch; across every call kind in
the sf40 record that prologue measured 0–2 µs, the clock's own resolution, and the split cost
twenty marks in `operators/*.cpp`, one in `expr.cpp` and two thread-locals. It is not carried:
operators and `expr.cpp` stay as master has them.

A CUDA failure while measuring is an error — `last_error` and a non-zero return from the
collection — never a region reported with zeros. `peacock_set_node_timing` refuses a mode it
does not name. `peacock_executor_collect_node_regions(executor, out, cap, out_count)` with
`out == NULL, cap == 0` answers the count and drains nothing; a `cap` below the count fails and
drains nothing; otherwise it copies and releases the events. Rust asks, allocates, drains.
`PeacockNodeStats` loses `time_us`, `peacock_measure_timing_floor_us` goes, and the byte
formula lives in Rust alone — C++ prices nothing.

NVTX is its own switch: `peacock_set_nvtx_ranges`, and a push/pop pair for the harness to name
the case. Node ranges are `<seq>.<call_index> <kind>` with `p<k>` inside, in a `peacockdb`
domain.

## The driver, the record and the harness

The GPU backend journals each ABI call it makes — seq, kind, what it was handed, what came
back — only while `node_timing_on()`; the driver stamps `call_index` and files the journal by
node and driving lane in `RunReport::abi_calls`. `join_regions` matches journal to regions on
`(seq, call_index)` and refuses a mismatch in either direction: a region no call claims, or a
call no region answered. `render_timings` in `plan_text` writes the node lines above;
`Measured`, `Region`, `Measurements`, `join_regions`, `node_measured` and `nodes_as_recorded`
are the driver's. Gone from the first version: `driving_lanes`, `Measured::host_us`,
`Measurements::entry`, the `measure: bool` plumbing (a handover is always priced), the
`file_stem_of` prefix strip. `CallSite {executor, node, lane}` stays because an error naming
its node and lane is worth the two fields; its doc says that and nothing more.

The harness reads two variables. `PEACOCK_RECORD_PATH` names the record file; unset, no record.
`PEACOCK_BENCHMARK_CAPTURE=trace|metrics` turns NVTX on, writes `capture=` into the heading and
leaves the tree alone — a captured run is never the published one; unset means `capture=none`;
any other value panics naming the two. It refuses a debug build (`cfg!(debug_assertions)`) and
a run without the pool. `--build-benchmarks` is `cargo test --release --no-run` into
`cpp/install/rust-benchmarks/`; no custom profile and no `build.rs` change.

## CI, tests, scripts

`peacock_gpu_benchmarks` is built in `cpp-build-2502`, staged with the GPU targets and run on
`gpu-tests` with `--skip bench_`: its own six assertions run on every job, the `bench_` cases
only under `--run-benchmarks`. It is in `RUST_TESTS`, in `gpu_runtime_targets()`, in the lists
the coverage guard compares, and not in `INTENTIONALLY_NOT_IN_CI`. `test_node_timing` stays as
the instrument's structural test — `Off` records nothing, regions equal recorded calls ×
partitions, Σ `device_us` > 0 and ≤ the wall, both modes make the same calls — with no
percentage ceiling: a bound loose enough never to flake on a shared host proves nothing, and
the effect is reported from an sf40 run instead. Rust-only tests pin what CI can reach without
a device: the trailer's arithmetic on the committed tree, the timed set ⊆ the device-enabled
set, a row's field count, and the record's preamble against `record_header()`. The source-text
tripwire over the harness files goes.

`scripts/calibration/tests/` holds pytest cases over a synthetic two-case capture, run by the
cost-report job beside the exec-model tests; the first case is two cases staying two. One
record reader serves the three scripts. In shell: `die`, `pull_one`, the patched
`LD_LIBRARY_PATH`, the sf40 directory and the "N passed" counter live once, in
`lib/shadgpu-env.sh`; `--run-benchmarks` refuses before any push when the binary is not
staged; `--pull-benchmarks` fails when nothing came home or the run died; `PCK_TEST_FILTER` is
the one filter name; the detector for a naming scheme master never had is not written.

## Wiki and tickets

`build-test.md`: the Corpus Benchmarks section describes the tree and the record as written
above, in 50–80 lines, one sentence saying the run is measured under event timing and what
that cost on sf40 and how it was measured; the data-flow diagram under Golden files; the test
table gains its rows and loses the deleted floor test, with the counts recomputed; no capitals
for emphasis. `architecture.md`, under Interfaces: the ABI count and list, the instrumentation
bullet, `NodeStats` without a time, the byte formula in Rust alone. Tickets: master's #191
owns the `date_part` width, so the first version's #197 and its cast do not come; the
`__grouping_id` width finding becomes a paragraph on #65, not a number; the counter does not
move. `refcounted-tables.md` loses only its `nodes_at_or_below_floor` sentences.

## Not in this task

The `date_part` cast in `expr.cpp` (#191, the schema chain's); ssh multiplexing, the rsync
rc=23 exit and cargo diagnostics forwarding in `shadgpu-env.sh`; replacing the hand-kept
fixture list with a `git ls-files` sweep; per-call declared schemas on the wire
(`declared-schemas.md`); `bug_` tests for #65 and #191 (the operator-cases chain's); the JIT
survey of sirius.

## Scope

Code expected to change, on master's layout:

- `cpp/include/peacock_gpu.h`; `cpp/src/{gpu_executor.cpp, node_session.cpp, plan_executor.h}`;
  `cpp/tests/cpu/test_executor.cpp`, `cpp/tests/gpu/{test_cudf.cpp, test_plan_executor.cpp}`.
- `peacockdb-ffi/src/lib.rs`.
- `peacockdb-core/src/executor/`: `mod.rs`, a new `instrument.rs`, `driver/mod.rs`, a new
  `driver/measurements.rs`, `driver/{partitioned, single_partition, mock, accounting}.rs`,
  `driver/tests/{counts, render}.rs`, `gpu_backend/{mod, backend, accumulate, emit, join,
  source}.rs`, `cpu_backend/mod.rs`. `peacockdb-core/src/plan_text/`: `mod.rs`, a new
  `bench_text.rs`.
- `peacockdb-core/tests/`: new `peacock_gpu_benchmarks.rs` and `test_node_timing.rs`;
  `test_corpus_goldens.rs`, `test_plan_goldens.rs`, `test_ci_coverage.rs`,
  `test_gpu_executors.rs` and its directory, `test_module_layout.rs` (register entries);
  `common/`: new `corpus_benchmark.rs`, `corpus_benchmark_cases.inc`, `record.rs`,
  `gpu_session.rs`; `corpus_gpu.rs`, `mod.rs`.
- `scripts/`: `build-test-shadgpu.sh`, `build-test.sh`, `lib/shadgpu-env.sh`, new
  `create_nsys_profile.sh`, new `calibration/{nsys_calls, nsys_hbm, nvtx_names, plot,
  record}.py` and `calibration/tests/`.
- `.github/workflows/pipeline.yml`; `testdata/.gitignore`; `testdata/benchmark-results/**`,
  `testdata/calibration/**`.
- `llm-wiki/{build-test.md, architecture.md, tickets.md, tasks/refcounted-tables.md}`.
- Nothing in `cpp/src/operators/`, `cpp/src/expr.cpp`, `Cargo.toml`, `peacockdb-core/build.rs`,
  `plan/`, `planner/`, `wire/`, the wire format, or `.gitignore` at the root.

Component-level API expected to change:

- The C ABI, sixteen symbols to nineteen: `peacock_measure_timing_floor_us` and
  `PeacockNodeStats::time_us` go; `peacock_set_nvtx_ranges`, `peacock_nvtx_push_range`,
  `peacock_nvtx_pop_range`, `peacock_executor_collect_node_regions`, `PeacockNodeRegion` and
  `PEACOCK_NODE_TIMING_{OFF, EVENTS}` come; `peacock_set_node_timing(int mode)` returns `int`.
  `NodeSession::collect_node_regions`, the timing mode enum and the NVTX switch in
  `plan_executor.h`.
- `executor`'s facade: `Measured`, `Region`, `Measurements`, `join_regions`, `node_measured`,
  `nodes_as_recorded`, `AbiCall`, `AbiCalls`, `CallStats::calls`, `RunReport::abi_calls`,
  `CallSite`, `collect_regions`, and `instrument::{NodeTiming, RmmPool, install_rmm_pool,
  set_node_timing, node_timing_on, set_nvtx_ranges, nvtx_range}` — declared where the
  Visibility rules put them, with any test-crate reach registered in `PUB_MODULES`.
  `CallStats` stops being `Copy`. `plan_text::render_timings`.
- Test targets: `peacock_gpu_benchmarks` and `test_node_timing`, both GPU-job targets.

## Done when

The gate is green on shad-gpu with both targets in it and the residue gate empty; the three
sf40 cases produce the two trees, the record, both captures, `calls.tsv`, `hbm.tsv` and every
panel from the commands `build-test.md` gives, each committed file with the producer the diagram
names; `grep` finds no `mark_device_start`, `thread_local`, `logical_size_from_table`,
`PEACOCK_LOG_LOGICAL_BYTES`, `PEACOCK_NVTX`, `PEACOCK_BENCHMARK_RESULTS_RO`, `build_profile`,
`EVENTS_GROSS_LIMIT` or `NotRun("GPU host only` in the tree; `cpp/src/operators/`, `expr.cpp`,
`Cargo.toml` and `build.rs` are byte-identical to master; the four `architecture.md` sentences
are true; and the wiki counts add up.
