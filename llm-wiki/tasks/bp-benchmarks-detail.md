# bp-benchmarks — detail

## Where the task starts

`ENS-bp-benchmarks` is one commit on master: PR #139's 26 commits over `c42b601`, squashed
and re-homed into the component layout by the helper, with no build run on the result. The
old head is kept as `bp-benchmarks-v1` locally. What the re-homing did: `instrument.rs`,
`driver/measurements.rs` and `plan_text/bench_text.rs` sit under `executor/` and `plan_text/`
as private modules, with `Measured`, `Region`, `Measurements`, `AbiCall`, `AbiCalls`,
`RmmPool`, `NodeTiming`, `NvtxRange` and the free functions declared on the `executor`
facade; the harness uses master's mode names (`tp1_single`, `bench_…`); the tickets the spec
drops were left at master's copy. Expect the first build to fail on imports and on
`test_module_layout` — that is Task 3 and Task 5 of the plan, not a regression to bisect.
The spec was written from a whole-branch reading of the first version; every decision in
it removes something the branch carries. The plan is written from the final state.

## What the branch has that the spec keeps

The timing model (CUDA events, no sync inside a region), the ABI-call journal and its
`call_index` join, `Measured`/`Region`/`Measurements`, `render_timings`, the harness with its
six assertions, `record.rs`, `test_node_timing`, the shell phases, the three Python scripts,
the data-flow diagram and the Corpus Benchmarks section. Port these from
`git show origin/ENS-bp-benchmarks:<path>` as the plan directs.

## What the spec removes or replaces

The host_setup/host_submit split and `mark_device_start` in `operators/*.cpp`, `expr.cpp`
and its two thread-locals; `logical_size_from_table`, `call_outcome`, `PEACOCK_LOG_LOGICAL_BYTES`
and the `rows`/`logical_bytes` region fields; `[profile.benchmarks]` and `build.rs`'s
profile probe; the 20 % ceiling in `test_node_timing`; `PEACOCK_NVTX` and
`PEACOCK_BENCHMARK_RESULTS_RO` (one `PEACOCK_BENCHMARK_CAPTURE`); `records-hbm.tsv` in git;
`calls.tsv` keyed without the case; the `date_part` cast and tickets #197/#198 (master holds
#191 and #65); ssh multiplexing, the rsync rc=23 exit and the `git ls-files` fixture sweep;
the `*.*.benchmark.txt` detector; `Exemption::NotRun` for the harness.

## Known defects of the branch a reviewer would raise

`create_nsys_profile.sh` calls a `die` nothing defines; `build-test.md` documents a tree
format the code never wrote; `architecture.md`'s ABI sentence still counts sixteen symbols
and names `measure_timing_floor_us`; the test table lists a deleted gtest; the harness is
compiled by no CI step.
