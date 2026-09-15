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

## Run log

Facts the coordinator established before the first dispatch:

- The reference for every "port from" in the plan is `bp-benchmarks-v1` (local branch,
  `bda00fc`), the same commit `origin/ENS-bp-benchmarks` holds until the first push. The
  first push replaces the remote with the squashed branch, so read the reference as
  `git show bp-benchmarks-v1:<path>`, never `origin/ENS-bp-benchmarks:<path>`.
- PR #139 is open, base `master`, head `ENS-bp-benchmarks`, and still shows the 26 old
  commits; the push at `reviewing` is a force-push that retargets its head to the squashed
  branch. The base stays `master`: this is the chain's first task.
- verda does not resolve; CPU tiers run locally. shad-gpu answers (H200, idle).
- The plan says "commit per task". The developer never commits; it stops at the boundary
  the dispatch names and the coordinator commits with the plan's message.
- Dispatch 1: plan Tasks 1 and 2 (the C++ instrument, the ABI and FFI declarations).
- Dispatch 2: plan Tasks 3, 4 and 5 (instrument facade, the ABI-call journal, measurements
  and the tree), after Tasks 1–2 landed as `7dad6a7` and `69b3051`. verda still down.

## Dispatch 1 — plan Tasks 1 and 2 (the C++ instrument, the ABI and the FFI)

### The seq a slice and an export are charged to, and what Task 4 owes it

The spec has four entry points open regions, but two of them take no seq: `wire/attach.rs`
builds `Call::bare(SliceHandle, …)` for `GpuLimit` and `Call::bare(ResultFromHandle, …)` for
`GpuUnload`, and neither node writes a wire node at all — `Call::target` is `None` and
`RecipePlan` has no seq for them. So C++ cannot be handed one, and the ABI signatures do not
change (the spec's symbol list does not grow them a parameter).

The rule implemented instead: **a region's seq is the node whose output the call handled.**
`execute_node` and `execute_scan_rowgroups` name it; the slice and the export take the seq of
the node that produced the handle, which `RegionSink::produced_by` records at every
`registry.emplace` while measuring. `slice_handle` propagates the producer onto its output, so
an export downstream of a limit still names the node that computed the rows. `call_index`
keeps counting per seq across all four entry points: a root node with one `execute_node` and
one export gets 0 and 1.

**What Task 4 must do for `join_regions` to close.** Every region has to be claimed by a
journalled `AbiCall` on `(seq, call_index)`, so `GpuExport::unload` and `LimitStream`'s slice
must journal a call, and they need the producing seq to do it. The cheap route: carry it on
`GpuBatch` — `GpuBatch::new` is called immediately after the call that produced it, where the
seq is in hand (`gpu_backend/mod.rs`, `calls.record(*seq, …)` is two lines away). Two things
are still undecided and are Task 4's to settle:

- `AbiCall::kind` is an `FbKind`, and neither bare symbol has one. Either widen the field or
  give the journal a kind for the two.
- C++ counts `call_index` in real call order. The driver stamps it per seq in `record_calls`,
  over the journal. Those two orders must agree for a batched limit, where the child's call
  for batch 2 falls between the slice of batch 1 and the slice of batch 2.

If Task 4 or 5 finds a better rule, the C++ side to change is `RegionSink::produced_by`,
`NodeSession::slice_handle` and `NodeSession::time_export` — nothing else depends on it.

### The export region needed a seam

The IPC export lives in `gpu_executor.cpp` (arrow/IPC), the region machinery in
`node_session.cpp`. `NodeSession::time_export(handle, body)` is the seam: the FFI passes the
export as a `std::function`, the session opens the region around it. `body` runs exactly once
whatever the mode.

### Decisions inside the instrument

- A CUDA failure throws (`"node timing: <call>: <cuda string>"`) at region open, at region
  close and at collection. No host-only degradation: the reference dropped to host-only, and a
  device time that was never measured reads as a fast node.
- `collect_node_regions` destroys every event before it throws, so a failed collection does
  not strand the rest of the run's events.
- The per-partition NVTX range follows `set_nvtx_ranges` alone, not the sink. The reference
  guarded it on the sink as well, which made the "ranges without timing" case emit node ranges
  and no `p<k>` ranges inside them.
- `RegionSink::produced_by` is never pruned. Handles are monotonic and the sink dies with the
  session, so a stale entry cannot alias; a measured run holds one `u64` pair per handle.

### What was proven, and how

- C++ CPU tier: `scripts/docker-build.sh --no-image --cache-dir /build/peacock -- ctest
  --test-dir cpp/build -L cpu` — 12 cases, 0 failures (master has 11; `NodeTiming.
  TheAbiRefusesAModeItDoesNotName` is new).
- shad-gpu, every gtest binary: cpu 12, gpu 3, plan 35, tpch 4, tpchv 4 — all pass.
- `cargo build -p peacockdb-ffi` inside the container: clean, no warnings.
- Red-green was watched for the three behaviours that are new rather than ported: the ABI's
  refusal of an unnamed mode, the export region, and the scatter's per-partition regions. Each
  mutation reddened exactly its own case and left the other seven green.

### Traps this dispatch walked into

- **The worktree had no `third_party/cudf`.** `git submodule status` showed `-43505bb…`; every
  cmake configure died on `rapids_config.cmake`. `git submodule update --init third_party/cudf`
  fixes it and costs one clone. A fresh chain worktree will need it again.
- **`cpp/build26` is the native build dir, not the container's.** Inside the container
  `/build/peacock/cpp-build` is bind-mounted at `cpp/build`, and the binaries there link cuDF
  from the container's conda prefix, so they do not run on the host at all.
- **`PCK_TEST_FILTER` does not reach the gtests** — `build-test-shadgpu.sh` forwards it to the
  rust binaries only. Filtering a C++ suite takes `--gtest_filter` on the binary.
- **`--run` fails while the rust core does not compile**: the gate reports "no rust test
  binaries found in cpp/install/rust-tests — every rust GPU test vanished" and exits 1. Until
  Task 3 lands, use `--push-binaries --patch` and run the gtest binaries over ssh with
  `LD_LIBRARY_PATH=$REMOTE_REPO/cpp/install/lib:/usr/local/cuda-12.5/compat:/home/info/glibc-2.39/lib:$HOME/miniforge3/envs/rapids-cuda-12.2/lib`
  per command.

### State left for the coordinator

No new or deleted files — eight modified, all tracked. `cpp/src/operators/**`,
`cpp/src/expr.cpp`, `Cargo.toml`, `peacockdb-core/build.rs` and `.gitignore` were restored to
master byte for byte; `Cargo.toml`'s `[profile.benchmarks]` and `build.rs`'s
`PEACOCK_BUILD_PROFILE`/`PEACOCK_BUILD_OPT_LEVEL` are therefore gone, which Task 6 must not
put back — `assert!(!cfg!(debug_assertions))` is what the spec puts in their place.

## Dispatch 2 — plan Tasks 3, 4 and 5 (the facade, the journal, the measurements)

### The two open points, settled

**`AbiCall::kind` for a bare call.** Neither widened. `FbKind` cannot grow members: it maps
onto `fb::PlanNodeKind` through `wire_kind()`, which has nothing for a slice or an export,
and `wire/` has to stay byte-identical to master. So the journal carries its own:

    pub enum AbiTarget { Node(FbKind), Bare(AbiSymbol) }

`AbiSymbol` already exists in `wire/`, is already `pub`, and already names exactly the four
entry points — so this is the split `Call::target` makes on the wire, under the same word,
and the field is `AbiCall::target` for that reason. `Display` prints the `FbKind` text or
`AbiSymbol::name()`, which is what keeps `records.tsv`'s `recipe_kind` cell non-empty for the
two bare symbols.

**C++'s call order versus the driver's per-seq stamping.** They agree by construction, and the
construction is: the driver is the only thing that makes ABI calls, it is sequential, and
`record_calls` runs immediately after each backend call returns, over a journal already in the
order that call made its own. So the sequence (record_calls invocations × journal order) *is*
the chronological order C++ counted in. A batched limit interleaves child call and slice the
same way on both sides for the same reason — C++ is not running a second schedule, it is
watching ours.

That is not provable inside `join_regions`, so it is checked instead: the join refuses a call
no region answered and a region no call claimed. A *count* disagreement is caught there; a
*permutation* (right keys, wrong calls) is not, which is why the construction above has to
hold. It is pinned end to end by `test_node_timing` (regions == journalled calls on q19, real
device) and by the new
`test_gpu_executors::accumulate::a_slice_and_an_export_are_charged_to_the_node_that_produced_the_handle`.

The C++ side needed no change: `RegionSink::produced_by`, `slice_handle` and `time_export`
already implement the rule, so the gtests from dispatch 1 stand as written.

### What carries the producing seq on this side

`GpuBatch` gains `producer: Seq`, set at every place a handle becomes a batch and propagated
by the slice onto its output — the exact mirror of C++'s `produced_by[handle] = seq` at every
`registry.emplace`. `GpuBatch::new` therefore takes it, which is why `test_gpu_batch.rs` and
`test_gpu_abi.rs` are in the diff although the spec's Scope does not list them.

Alternatives rejected: a new ABI symbol to ask C++ (the spec freezes nineteen), and deriving
the seq from the plan in `GpuAccumulator::limit`/`GpuExport` (a handle can come from further
down than the child node, so the derivation would be a second rule that can drift).

### Other decisions in this dispatch

- **`collect_regions` is on the `executor` facade**, not reached through `pub mod gpu_backend`.
  That is what `tests/common/gpu_session.rs` now imports, and it is why `PUB_MODULES` needed no
  new entry — plan Task 8 step 5's open question, answered: nothing new is forced.
  `test_module_layout` also wanted `no_abi_calls`/`Consumed` at `pub(crate)` rather than
  `pub(super)`; done.
- **`GpuExec` takes the state schema.** A chain of calls prices every call but the last by the
  aggregate's `intermediate()`, since nothing builds a batch from a middle call's output.
  `GpuExec::new` refuses a multi-call recipe with no state schema, which is what caught the one
  test site that needed it (`one_chaining_node` in `test_gpu_executors`).
- **Pricing is unconditional.** `AbiCalls::is_armed` and the `measure: bool` threading are gone;
  `Consumed::of` is two field reads, and `logical_size_from_schema` is computed once per call
  and shared between the journal entry and the batch (`produced`/`priced` in `gpu_backend`).
- **`hand_over` and `one_call`** live once in `gpu_backend/mod.rs`; the two copies of
  `hand_over` in `accumulate.rs` and `join.rs` are gone, and six accumulator call sites are one
  line each.
- **`Measured` is `{host_us, device_us, out_rows, out_bytes, regions}`**; `out_rows`/`out_bytes`
  are copied from the `AbiCall` by the join, never from the region — C++ prices nothing now.
- **`RunReport::driving_lanes` is gone.** The test that read it now compares `abi_calls[node].len()`
  against `PlanIndex::build(...).nodes[node].ready_lanes`, which is where the driver fills it
  from, so the two cannot disagree.

### Work from later tasks that landed early, because the tree had to build

Dispatch 2 was asked to stop at Task 5, and did for new work. But `--run` on shad-gpu refuses
to start while any rust test binary fails to compile, and four files from Tasks 6 and 7 read
the types Tasks 4 and 5 reshaped. What was done to them is minimal and is noted inline in the
plan under the step that owns it: `record.rs` (17 columns, `target`, `in_bytes`), 
`corpus_benchmark.rs` (`BUILD_PROFILE` → `BUILD`, `measured_of` on the `Result`), 
`peacock_gpu_benchmarks.rs` (one field rename) and `test_node_timing.rs` (the plan's Task 7
step 1 in full). Nothing else in those files was touched.

`scripts/build-test-shadgpu.sh` still carries `BENCH_PROFILE=benchmarks` and a comment naming
`[profile.benchmarks]`, which no longer exists in `Cargo.toml` — plan Task 8 step 2 replaces
that with `--release`. `--build-benchmarks` is broken until it does; `--build` is unaffected.

### Environment, on top of dispatch 1's traps

- **`testdata/tpcds.sf1` is absent from a fresh worktree** and every tpcds case in
  `test_cpu_corpus`, `test_cpu_end_to_end` and `test_plan_goldens` fails with
  `register the tables: IoError(NotFound)` until it exists. `testdata/generate_testdata.sh
  --bench tpcds --sf 1` needs duckdb-cli **1.5.4 exactly** (it checks, and says so); the
  release zip from GitHub works and costs one download. `tpch.sf1` can be symlinked from the
  primary checkout — but `testdata/.gitignore` matches `/tpch.sf*/` with a trailing slash, so
  a *symlink* shows up as an untracked file while a directory does not. Remove it before
  handing the tree back.
- **A poisoned `peacockdb-ffi` OUT_DIR cache.** Running cargo inside the container *without*
  sourcing `scripts/lib/shadgpu-env.sh` first leaves a `CMakeCache.txt` with
  `CMAKE_INSTALL_PREFIX=/usr/local`, and every later build through that unit dies on
  `file INSTALL cannot copy ... Permission denied`. Fix: delete the offending
  `/build/peacock/cargo-target/debug/build/peacockdb-ffi-*/` (find it by grepping the caches
  for `/usr/local`). Always `. scripts/lib/shadgpu-env.sh` before a bare cargo in there.
- `peacock_gpu_benchmarks` is not in `RUST_TESTS`, so `--build` does not compile it. To check
  it builds: `docker-build.sh ... -- bash -c '. scripts/lib/shadgpu-env.sh && cargo test
  --no-run -p peacockdb-core --test peacock_gpu_benchmarks'`.
- Most files under `peacockdb-core/` are not rustfmt-clean at `d9fd723` — the re-homing did not
  format them. Formatting one whole would bury the change, so this dispatch formatted only the
  one file that was clean before it (`gpu_backend/source.rs`) and hand-matched rustfmt's shape
  on the lines it added everywhere else.

### What was proven, and how

- rust-only, whole package, with both datasets present: 0 failures anywhere —
  `--lib` 444, `test_cpu_corpus` 448, `test_cpu_end_to_end` 24, `test_corpus_goldens` 22,
  `test_plan_goldens` 20, `test_module_layout` 11, `test_ci_coverage` 7, `test_golden_format` 26.
- shad-gpu, `--push-binaries --patch --run`: C++ 12/3/35/4/4, rust `test_gpu_abi` 4,
  `test_gpu_corpus` 8, `test_gpu_executors` 32, `test_gpu_recipe_walk` 10,
  `test_inc2_conformance` 10, `test_node_timing` 1. Exit 0.
- The cuDF-shape build is warning-free.
- Red-green watched on four behaviours: `join_regions` refusing a call with no region and
  refusing a region with no call (each mutation reddened only its own case), and the slice
  test against both a broken producer propagation and a slice that journals nothing.
- `git diff origin/master --stat` over the byte-identical set is empty; `residue-gate.sh`
  prints nothing under `== bp gates`.
