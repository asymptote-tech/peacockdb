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
- Dispatch 3: plan Tasks 6 and 7 (the harness, the record, the case list; `test_node_timing`),
  after Tasks 3–5 landed as one commit. Dispatch 2 already adapted `record.rs`,
  `corpus_benchmark.rs`, `peacock_gpu_benchmarks.rs` and `test_node_timing.rs` to the reshaped
  types (noted inline in the plan); Task 7 step 1 is already done in full.
- Dispatch 4: plan Tasks 8 and 9 (scripts and CI; captures, the Python and its tests),
  after Tasks 6–7 landed as one commit. Carries dispatch 3's two findings: `setup-glibc.sh`
  patches `rust-tests/` only, and `create_nsys_profile.sh` still exports the two retired
  variables. verda still down.

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

## Dispatch 3 — plan Tasks 6 and 7 (the harness, the record, the case list, `test_node_timing`)

### What was still owed after dispatch 2, and what it became

Task 7 was already done in full and needed no code change; only two words of emphasis
were lower-cased in it. Task 6's four files existed, so the work was the difference between
them and the spec.

- **`Capture` lives in `record.rs`**, beside `CAPTURE_ENV`, with `from_env()` exhaustive —
  an unnamed value panics naming `trace` and `metrics` rather than reading as `none`, which
  would publish a captured run's times. `corpus_benchmark::benchmark_case` reads it once at
  the top, before it plans, so a misspelling fails the case rather than the publish at the
  end of it. It decides two things and nothing else: `set_nvtx_ranges(capture != None)` after
  the warm-up, and whether `write_section` is called at all.
- **`PEACOCK_NVTX` and `PEACOCK_BENCHMARK_RESULTS_RO` are gone from the harness.** The
  read-only guard moved out of `write_section` and became an `if` at the one call site: a
  reader of the publish now sees the condition instead of finding it three functions away.
- **`RunMeta` lost `timing_mode` and `build` and gained `capture`.** Those two are constants
  in `record.rs` now (`TIMING_MODE`, `BUILD`), because the harness refuses to measure under
  anything else — `run_once`'s two asserts are what make the literals true. `BUILD` moved out
  of `corpus_benchmark.rs` for a second reason: `corpus_benchmark` is `cfg(not(rust-only))`,
  and the rust-only trailer test has to name the same string.
- **No cell in a row is empty.** `record_rows` now `expect`s the measurement instead of
  passing an `Option` down: `join_regions` refuses a call no region answered, so an absent
  one is a join that was never checked rather than a call the device missed.
- **`rows_match_the_recipes` checks the field count first.** Every column is a number or a
  name, so a row that lost a cell still parses — the cells after the gap each move one column
  left and `device_us` reads whatever `host_us` measured.
- `HEADER_NOTES` was rewritten for the 17 columns and carries no capitals for emphasis; the
  same sweep went over the four files' own comments.

### The rust-only tests, and the two that are `#[ignore]`d

`test_corpus_goldens.rs` gained `a_row_that_lost_a_cell_is_refused`,
`every_timed_case_is_enabled_on_a_device` and the trailer half of
`every_total_us_is_the_sum_of_the_time_us_beside_it`, and lost
`the_benchmark_path_reads_no_cpu_side_golden`. `test_plan_goldens.rs` needed nothing — its
port landed in the squash.

**Two `#[ignore]`s, not one.** The plan's Task 11 step 4 says "remove the `#[ignore]` Task 6
left"; there are two, and both go at the same moment for the same reason. The committed sf40
tree and `records.tsv` are the *first version's* — they came across in the squash — so they
describe a harness that no longer exists: the trailer says `build_profile=benchmarks
opt-level=3` where this one writes `build=release`, and the record's preamble has eighteen
columns with `peacock_host_us`/`cudf_host_us` where this one has seventeen with `host_us`.
The two tests are `every_committed_tree_reports_a_release_build` and
`the_records_preamble_is_what_record_header_writes`. Both were run with `--ignored` and fail
exactly there, which is their red; Task 11's re-measurement is their green. Neither `#[ignore]`
reason names the task file by name — `bp-` in a string outside `llm-wiki/` trips
`residue-gate.sh`.

The rest of the trailer is checked unignored, because the v1 data satisfies it: `device_us`
is the sum of the tree's `total_us`, `runs` holds ten entries, and `run_us` is the
second-smallest of them. That last one is beyond what the plan asked for and is the
cheapest statement of the selection rule the spec fixes.

`every_timed_case_is_enabled_on_a_device` reads both `.inc` files as text rather than through
the inventories, because `corpus_cases.inc` is expanded only by the two corpus binaries and a
rust-only build links neither. `read_cases` takes the *last* `)` on a line and asserts the
tail is `;` — the argument lists carry no parentheses of their own, and the assert is what
says so.

### Red-green

Each new assertion was reddened on its own before being believed:

- the field-count check deleted from `record.rs` → only `a_row_that_lost_a_cell_is_refused`
  failed, and on the `expect_err`.
- `q19` given `tp4_sized` in `corpus_benchmark_cases.inc` → only
  `every_timed_case_is_enabled_on_a_device` failed, naming the mode and the device column.
- `device_us` off by one, then the spread cut to nine, then `run_us` set to the minimum, each
  in the committed tree → `every_total_us_is_the_sum_of_the_time_us_beside_it` failed on
  exactly the matching assert. The file was restored from a copy each time.

### Findings for Task 8, from trying to run the binary by hand

- **`setup-glibc.sh` patches `rust-tests/` only.** `patch_rust_dir` is called once, and
  `verify_patched` walks `bin/*` and `rust-tests/*`. So `cpp/install/rust-benchmarks/` ships
  unpatched, the verifier passes, and the binary dies at load with a bare failure. The comment
  at `build-test-shadgpu.sh`'s `BENCH_STAGING` ("setup-glibc.sh patches both") is false today.
  This dispatch patched the one binary by hand with `patchelf` on the host to get its six
  assertions run; Task 8 owns the fix, and `verify_patched` is where it will be noticed if it
  is not made.
- `patchelf` is not on shad-gpu's `PATH`; it is at `/home/info/.local/bin/patchelf`.
- `create_nsys_profile.sh` still exports `PEACOCK_NVTX` and `PEACOCK_BENCHMARK_RESULTS_RO`,
  and `nsys_calls.py` still names the first in a message. Nothing reads either after this
  dispatch. Task 9 replaces both with `PEACOCK_BENCHMARK_CAPTURE=trace|metrics`.
- `--build-benchmarks` is still broken (`BENCH_PROFILE=benchmarks`). The binary was built and
  staged by hand instead: in the container, `. scripts/lib/shadgpu-env.sh` then
  `stage_cargo_test_binary peacock_gpu_benchmarks cpp/install/rust-benchmarks`, at the default
  test profile. The six assertions do not reach `run_once`, so they do not need `--release`;
  Task 8 step 6 is where the release build gets proved.

### What was proven, and how

- rust-only, whole package with both datasets present: 0 failures — `--lib` 444,
  `test_cpu_corpus` 448, `test_cpu_end_to_end` 24, `test_corpus_goldens` 23 + 2 ignored,
  `test_plan_goldens` 20, `test_module_layout` 11, `test_ci_coverage` 7,
  `test_golden_format` 26, `test_cost_model` 3, `test_null_analysis` 8,
  `test_layout_injection` 4, `test_inc2_conformance` 3, `test_cpu_executors` 1.
- shad-gpu, `--push-binaries --patch --run`: C++ 12/3/35/4/4, rust `test_gpu_abi` 4,
  `test_gpu_corpus` 8, `test_gpu_executors` 32, `test_gpu_recipe_walk` 10,
  `test_inc2_conformance` 10, `test_node_timing` 1. "GPU test run OK".
- `peacock_gpu_benchmarks --skip bench_ --test-threads=1` on shad-gpu: 6 passed, 3 filtered.
- The cuDF-shape build is warning-free, and so is the rust-only one.
- `test_node_timing` reports, at sf1 q19 tp1-single on shad-gpu: off wall 449908us, events
  wall 452298us (+0.5%) on one run and 449302us (+0.3%) on the next, 12 regions. Not the
  figure Task 10's sentence wants — that one is sf40 and Task 11 step 3 measures it — but it
  is what the instrument costs where the test runs.
- Everything above was re-run after the last edit, because a doc-comment change moves the
  compiled artifact's bytes and the shipped binaries would otherwise be one edit behind.

### Environment, on top of the earlier traps

- **The `testdata/tpch.sf1` symlink breaks `--push-binaries`, not just the working tree.**
  The fixture sweep is `git ls-files --cached --others --exclude-standard testdata`, and
  `testdata/.gitignore` matches `/tpch.sf*/` with a trailing slash, so the symlink is an
  "other" and goes into the file list. rsync then reports `cannot delete non-empty directory:
  testdata/tpch.sf1` and exits 23, and `set -e` kills the run before `--patch`. Remove the
  symlink before any push. The rust-only tiers need it, so the order is: symlink, run the CPU
  tiers, remove it, push.
- `cargo` is not on the default `PATH` in this environment; it is `~/.cargo/bin/cargo`.

## Dispatch 4 — plan Tasks 8 and 9 (scripts and CI; the captures, the Python, its tests)

### The benchmark run had no launcher

`--run-benchmarks` set `RUN_BENCH=1` and nothing read it: `remote_bench_script` was defined
and never called, in `bp-benchmarks-v1` as well as in the squash. So every phase of the
measurement existed except the one that starts it, and the flag exited 0 having done
nothing — which is why the validation the spec asks for matters more than it looks: the
shape that hides a missing launcher is a flag whose success is silence. The block added
beside the gate's mirrors it, with its own exit code rather than an OR into the gate's.

### What moved into `lib/shadgpu-env.sh`, and what moved back out

In: `die`, `BUILD_GLIBC`, `PATCHED_LD`, `SF40_DIR`, `passed_count`, `pull_one`. Out, back to
master's text: the ssh `ControlMaster` block, `resilient_rsync`'s rc=23 branch, and
`stage_cargo_test_binary`'s cargo-diagnostics forwarding — the spec's "not in this task",
which the squash had brought across. The fixture push is master's hand list again for the
same reason, and that is what retires dispatch 3's `testdata/tpch.sf1` symlink trap: with no
`git ls-files` sweep, an untracked symlink is no longer in the file list rsync is handed.

Two of those are less obvious than they look.

- **`PATCHED_LD` is remote shell text, not a path.** It is built here, where `getconf` can
  read the build host's glibc, but carries `$HOME` and `${LD_LIBRARY_PATH:-}` unexpanded for
  the remote to resolve. That is what lets one definition serve the gate, the benchmark run
  and both Nsight passes; `create_nsys_profile.sh` had hardcoded `glibc-2.35`, which is wrong
  from a 24.04 box — this one says 2.39 here.
- **`passed_count` runs on the far side.** The logs it counts are written on the host, so the
  remote heredocs insert its definition with `$(declare -f passed_count)` and call it. That is
  the only way one copy of the "N passed" sum serves a local script and a remote one.

### `--skip bench_`, and the guard over it

`peacock_gpu_benchmarks` is now in `RUST_TESTS`, in pipeline.yml's staging array, in
`gpu_runtime_targets()`, and exempt as `GpuJob` rather than `NotRun`. Both runner loops set a
per-binary `skip` from a `case` on the binary's name and pass `$skip` at the invocation.
`the_benchmark_binary_runs_without_its_cases_on_ci` reads each loop's body and asserts both
halves; it resolves `$BENCH_TARGET` out of the shell file first, so one needle reads a file
that names the target through a variable and a file that spells it out.

### The counters pass joins against the clean record

`nsys_hbm.py --record` is `testdata/calibration/records.tsv` — the clean run's — and the
metrics pass's own record stays on the host as `calibration/records-metrics.tsv`. Renamed
from `records-hbm.tsv` so the name the spec keeps out of git does not exist anywhere. The
join is still both-ways checked (`report_loss`), which now says something stronger: the
counters run made exactly the calls the clean run made. One consequence to know before
filtering: a `PCK_TEST_FILTER`ed metrics pass cannot be joined onto a full `records.tsv` —
`report_loss` refuses it, naming the rows with no captured call.

### The calibration Python

`record.py` is the one reader (`read_record` → `(run, rows)`, `read_tsv`, `require`) and the
one writer (`write_tsv`); the other three import it. `plot.py` refuses two records whose
`# run:` lines differ and requires the union of `READS`, which lists per panel what that
panel reads — so a record missing `host_us` is refused at the door instead of drawing an
empty term. `nsys_calls.py` keys regions by `(case, seq, kind, partition)` and writes the
case's four columns; a mode with no plans golden and regions that disagree about how many
times they ran are both `sys.exit(1)` now, where the second used to be a printed remark.

`scripts/calibration/tests/` holds `harness.py` (copied from `scripts/exec_model/tests/`),
`capture.py` and the three test files. `capture.py` is a fifth file the plan did not list: it
builds the synthetic sqlite export both capture readers need, and the two builders would
otherwise be one copy each of the same NVTX/CUPTI schema. `test_plot.py` needs matplotlib —
`/usr/bin/python3 <file>` on a dev box, and the new cost-report step installs it the way the
exec-model step installs pandas.

### Two bugs only a real run could find, and what they have in common

Neither the timed cases nor either capture pass had ever executed — `--run-benchmarks`
launched nothing, so Task 8's proving run was the first. Both bugs are the same shape: a
**bare** ABI call (`result_from_handle`, `slice_handle`) publishes no step of its own and is
named by the seq of the node whose output it was handed, so one seq carries two kinds and
one node carries a seq that is not its own. Two readers assumed otherwise.

- **`rows_match_the_recipes` refused every case**, one call before the record was written:
  "a row pairs node 5 with step #6, whose recipe publishes {}". Node 5 is q6's `GpuUnload`,
  whose recipes line is `result_from_handle(batch, row range)` and names no `#seq`. The rule
  now: where a node publishes nothing, its row's seq must be published by a node **below**
  it — post-order is children first, so the producer is always earlier, which is a tighter
  statement than "by some node". `a_bare_calls_row_names_the_seq_it_was_handed` in
  `test_corpus_goldens.rs` is the red-green.
- **`nsys_calls.py` refused every capture**: "kind differs: #10 is CudfProject in the plan
  and result_from_handle in the capture". `by_case[case][seq] = kind` keeps one kind per seq
  and the export's range overwrote the project's. The bare names now live in `nvtx_names.py`
  (`BARE_CALLS`, `AbiSymbol::name()`'s spellings) and are left out of that comparison.

A third came out of the same run and is arithmetic rather than naming: the capture's
"regions disagree about how many times they ran" check compared raw occurrence counts, and
a batched mode drives one seq once per batch — q6 at tp4-sized has regions at 4 occurrences
an execution beside regions at 1, which read as a run that died partway. `executions` is now
`occurrences / call indices` and refuses a remainder; `calls.tsv` gains a `regions` column
and `calls_per_exec` becomes `calls_per_region`, both of which now say what they count.

### What Task 11 has to know

- **`testdata/calibration/records-hbm.tsv` is still tracked.** The new `testdata/.gitignore`
  is deny-by-default over `calibration/`, but a rule does not untrack a file: Task 11 deletes
  it in the commit that lands the new data.
- **The committed tree and record are still v1's**, so `plot.py` now refuses `records.tsv`
  (it has `peacock_host_us`/`cudf_host_us` where the harness writes `host_us`), and the two
  `#[ignore]`d tests still fail. All three go green on the same re-measurement.
- **Check who else is on the GPU first**, and this is not a caution but a measurement: with
  another user's process holding 62 GB of the H200's 140, q6 at tp1-single came in at 553 ms
  against the committed tree's 90 ms and q19 at 1326 ms against 730 ms. The counters pass is
  worse — GPU metrics are device-wide, so the neighbour's traffic is inside our regions:
  37.7 TB of HBM against the committed file's 2.4 TB. Everything ran and every check passed;
  none of the numbers are publishable. `nvidia-smi --query-compute-apps` before the run.
- **`--pull-benchmarks` now refuses a died run** as well as a running one, and refuses a pull
  that brought neither a tree nor a record home. The recovery path for a partial run is to
  re-run it; the host keeps what it wrote either way.

### Test table recount

Recount of `build-test.md`'s "Test categories" table against the branch's sources, anchored
on real runs of the branch's binaries. Every per-target run total agrees with the source
count; no disagreement to explain.

**Rust rows whose N changed**

| Row | old N | new N | why |
|---|---|---|---|
| Executors on a device (Rust) | 31 | 32 | `a_slice_and_an_export_are_charged_to_the_node_that_produced_the_handle` added to `test_gpu_executors/accumulate.rs` (10 → 11 there) |
| Recipe plan structure (Rust) | 4 | 5 | `a_rows_node_seq_names_the_steps_its_recipes_line_prints` added to `test_plan_goldens.rs` (19 → 20 in the target) |
| Corpus goldens, self-consistency (Rust) | 20 | 26 | six benchmark/record cases added: `every_total_us_is_the_sum_of_the_time_us_beside_it`, `every_committed_tree_reports_a_release_build` (`#[ignore]`), `the_records_preamble_is_what_record_header_writes` (`#[ignore]`), `a_row_that_lost_a_cell_is_refused`, `a_bare_calls_row_names_the_seq_it_was_handed`, `every_timed_case_is_enabled_on_a_device` |
| CI wiring guard (Rust) | 6 | 8 | `the_benchmark_binary_runs_without_its_cases_on_ci` added (+1); the other +1 is pre-existing drift — master's `test_ci_coverage.rs` already held 7 cases against a printed 6 |
| Lib unit (Rust) | 435 | 444 | `driver/tests/counts.rs` 10 → 13 (the call record's lane indexing, a lane's own calls, an unmeasured entry) and `driver/tests/render.rs` 8 → 14 (the timing tree, a zero-rounding region, post-order indexing, per-seq measurements, and the two refusals) |

Two Rust rows are new on this branch and are already correct: **GPU timing method** 1
(`test_node_timing`, run 1) and **Corpus benchmarks** 9 (`peacock_gpu_benchmarks`: 6 `#[test]`
assertions + 3 `bench_` cases the `corpus_query_benchmark!`/`paste` expansion of
`corpus_benchmark_cases.inc` produces — q6 at `tp1_single` and `tp4_sized`, q19 at
`tp1_single`; the `none` arm generates no case).

Rust rows verified unchanged against their runs: `test_cpu_corpus` 448 = 447 + the registry
case, `test_gpu_corpus` 8 = 7 + the registry case, `test_gpu_abi` 4, `test_cpu_end_to_end`
24 run + 2 `#[ignore]`d = 26, `test_golden_format` 26, `test_module_layout` 11,
`test_cost_model` 3, `test_null_analysis` 8, `test_layout_injection` 4,
`test_planner_join_capability` 13, `test_planner_join_refusals` 10, `test_cpu_executors` 1,
`test_gpu_recipe_walk` 10, `test_inc2_conformance` 10, `test_gpu_batch` 3, FFI smoke 2.

**C++ rows whose N changed**

| Row | old N | new N | why |
|---|---|---|---|
| C++ CPU/FFI unit | 11 | 12 | `NodeTiming.TheAbiRefusesAModeItDoesNotName` added to `tests/cpu/test_executor.cpp` |
| cuDF GPU smoke (C++) | 5 | 3 | `NodeTiming.FloorRestoresTheSwitch` and `NodeTiming.FloorClampsSampleCount` deleted with the sampling floor |
| Plan-executor (C++) | 27 | 35 | eight `NodeRegions.*` cases on the timing regions: one region per output partition, `call_index` per seq, a fresh session's count, what a region carries, a second collect, timing off, the slice and the export, and that asking the count drains nothing |

`peacock_tpch_tests` 4 and `peacock_tpchv_tests` 4 confirmed unchanged; the manual C++ rows
(streamed 4, per-operator timings 1, multi-GPU 4/4/1) are untouched by the branch.

**Python — a row the branch did not add**

`scripts/calibration/tests/test_calls.py` (5 `def test_`), `test_hbm.py` (3) and
`test_plot.py` (3) are 11 new cases, run by the new "Calibration script tests (Python)"
step in the `cost-report` job. No row in the table names them, so the Python subtotal is
short by 11 until one is added (`capture.py` and `harness.py` are helpers and hold none).

**Arithmetic**

- Rust: 1135 + 1 (timing method) + 9 (corpus benchmarks) + 1 (executors) + 1 (recipe
  structure) + 6 (corpus goldens) + 2 (CI guard) + 9 (lib unit) = **1164**. Cross-checked by
  summing every Rust row of the corrected table: 10+10+1+32+4+3+1+9+3+13+8+10+20+26+2+8+11
  +444+26+447+26+7+4+2+37 = 1164.
- C++: 65 + 1 − 2 + 8 = **72**, i.e. 12+3+35+4+4+4+1+4+4+1.
- Python: 369 + 11 = **380**, i.e. 41+216+19+93+11.
- Grand total: 1164 + 72 + 380 = **1616**.

Header line becomes: **Grand total: 1616 test cases — Rust 1164, C++ 72, Python 380.**

**Descriptions the branch falsified**

- **cuDF GPU smoke (C++)** — "the timing floor leaves the global switch as it found it" is
  gone with the floor, and the example link `NodeTiming.FloorRestoresTheSwitch`
  (`test_cudf.cpp#L41`) names a deleted test. The row is now the GPU liveness check and the
  Spark-murmur3 kernel, nothing else.
- **C++ CPU/FFI unit** — "decimal binop typing, AST routability, lifecycle, and the
  row-range clamp rule" no longer covers the row: it also holds the ABI's refusal of a
  timing mode it does not name.
- **Plan-executor (C++)** — the sentence lists plan IR, the per-call entry points, the sqrt
  arm and a state-emitting merge; eight of its 35 cases are now about timing regions, which
  the sentence does not mention.
- **Corpus goldens, self-consistency (Rust)** — "the committed sections against their own
  arithmetic, with no dataset and no run" describes 20 of the 26: the other six read the
  committed benchmark trees and `calibration/records.tsv`, and two of those are `#[ignore]`d
  against the tree being re-measured, so N no longer equals what runs (24 do).
- **CI wiring guard (Rust)** — "Two classes the `--test` sweep cannot see are asserted line
  by line" is now three: the benchmark target's `--skip bench_` is asserted in both runner
  loops.
- **Lib unit (Rust)** — the enumeration stops before the per-node measurement record and the
  benchmark tree renderer, which is where nine of its 444 cases now are.
- **GPU timing method (Rust)**, the branch's own new row — its anchor
  `test_node_timing.rs#L83` lands on the `journalled_calls` helper; the test it names is at
  line 100.

Stale anchors that predate the branch and are not its doing: `test_plan_goldens.rs#L427`
(the test is at 672 on master too), `test_executor.cpp#L26`/`#L81` (27/82 on master),
`test_plan_executor.cpp#L255` (313 on master), `test_cudf.cpp#L84` (107 on master, 87 here).

### Review round 1, before the data

The branch was pushed at `83479e6` (PR #139 now shows the squashed branch, base `master`) while
Task 11 waits for a free device — another user's process has held the H200 since dispatch 4.
The reviewer reads `origin/master..83479e6`; the committed data files are still the first
version's and are out of that round's scope, since the data commit replaces them. The board
stays at `building` until the data lands.

## Dispatch 5 — plan Task 11 (the data): not measured, the device never freed

### What the device did

Polled `shad-gpu` every five minutes from 17:48 to 20:37 local (+03:00), then one last read
at 20:41:49 local. The neighbour never left:

```
2026-09-15T17:41:49+00:00                      # host clock is UTC; 20:41:49 local
--- compute-apps:
2074022, 62276 MiB, /home/kirill/sdg-moe-transfer-20260909/project/SDG-MoE/.venv/bin/python
--- gpu:
94 %, 62285 MiB, 143771 MiB
```

Utilization stayed 90–98 % for the whole window with one dip to 29 % at 18:49 and no
release of memory. A CI job of ours (`peacock_tpchv_tests`, pid 2092999, 31 GB) joined at
20:11 and was gone by 20:16 — worth knowing, because the gate can land beside a
measurement even when the human neighbour is absent: check for both.

Nothing was measured, so every committed data file is still v1's and the two `#[ignore]`s
in `test_corpus_goldens.rs` stay on. No file under `testdata/` was touched.

The poll loop, which does not match itself (it greps no process list; the antipattern in
build-test.md is about `pgrep -f`):

```bash
while true; do
  APPS=$(timeout 60 ssh shad-gpu 'nvidia-smi --query-compute-apps=pid,used_memory,process_name --format=csv,noheader')
  UTIL=$(timeout 60 ssh shad-gpu 'nvidia-smi --query-gpu=utilization.gpu,memory.used --format=csv,noheader')
  echo "$(date -Iseconds) apps=[$APPS] gpu=[$UTIL]" >> "$LOG"
  [ -z "$APPS" ] && [ "$(echo "$UTIL" | awk -F'[ ,]' '{print $1}')" -le 5 ] && { echo FREE >> "$LOG"; exit 0; }
  sleep 300
done
```

### What is already built, and what that saves

`--build-benchmarks` ran green at fc6a0de with no warnings, so
`cpp/install/rust-benchmarks/peacock_gpu_benchmarks` (release) and the seven binaries in
`cpp/install/rust-tests/` are staged and current for that commit. If nothing under
`peacockdb-core/`, `peacockdb-ffi/` or `cpp/` has moved since, step 1's build is already
done and the next dispatch starts at `--push-binaries`. Both stagings are in
`cpp/install/`, which the root `.gitignore` excludes.

### The command sequence Task 11 wants, in order

Re-check `nvidia-smi --query-compute-apps` immediately before and after each of the four
measured runs; a neighbour appearing mid-run voids that run's data.

```bash
# 0. the build, only if the tree moved since fc6a0de
scripts/docker-build.sh --no-image --cache-dir /build/peacock -- ./scripts/build-test-shadgpu.sh --build-benchmarks

# 1. the timed run (tens of minutes) — detached, then polled, then pulled
./scripts/build-test-shadgpu.sh --push-binaries --patch --run-benchmarks-detached
./scripts/build-test-shadgpu.sh --benchmark-status     # exits 0 only when this run finished with 0
./scripts/build-test-shadgpu.sh --pull-benchmarks

# 2. the two captures, as two invocations so the device can be re-checked between them
./scripts/create_nsys_profile.sh --trace
./scripts/create_nsys_profile.sh --metrics

# 3. the panels
/usr/bin/python3 scripts/calibration/plot.py \
    --record testdata/calibration/records.tsv --hbm testdata/calibration/hbm.tsv \
    --out-dir testdata/calibration/plots

# 4. the proving commands
CARGO_TARGET_DIR=/build/peacock/rust-only-target cargo test -p peacockdb-core \
    --features rust-only --test test_corpus_goldens --test test_plan_goldens
scripts/residue-gate.sh
```

There is no `testdata/tpch.sf1` symlink in this worktree, so the `--push-binaries` hazard
dispatch 3 hit does not apply — but check before pushing, since it is untracked and can
reappear.

### The events figure for the wiki (step 3)

Not committed. What was prepared here and deleted again at the end of this dispatch:

```bash
sed -e 's|^const SF: &str = "1";|const SF: \&str = "40";|' \
    -e 's|^const ROUNDS: usize = 7;|const ROUNDS: usize = 10;|' \
    peacockdb-core/tests/test_node_timing.rs > peacockdb-core/tests/test_node_timing_sf40.rs
scripts/docker-build.sh --no-image --cache-dir /build/peacock -- \
    bash -c '. scripts/lib/shadgpu-env.sh && stage_cargo_test_binary test_node_timing_sf40 cpp/install/rust-benchmarks --release'
```

Three decisions behind those two commands.

- **`ROUNDS = 10`, not the committed 7.** The sentence Task 10 left says "the events mode's
  second-smallest of ten", so the procedure has to run ten.
- **`--release` and `rust-benchmarks/`.** Release because the figure describes the build the
  tree was taken under; `rust-benchmarks/` because the remote gate loop globs
  `rust-tests/` and would run an extra binary there on every later gate, while the
  benchmark script runs `$BENCH_TARGET` by name and ignores its neighbours.
  `setup-glibc.sh --patch` walks both directories, so the copy is patched by the ordinary
  `--push-binaries --patch`.
- **Run it after the timed run, not before.** `plan_at` resolves sf40 through
  `testdata/tpch.sf40`, and that symlink is created on the host by `remote_bench_script`.
  Run by hand over ssh, since no flag knows this target:

```bash
ssh shad-gpu 'export PEACOCK_TESTDATA_DIR=/home/info/peacockdb/testdata
  export PEACOCK_TPCH_SF40_DIR=/home/info/peacock-datasets/testdata/tpch.sf40
  LD_LIBRARY_PATH=/home/info/peacockdb/cpp/install/lib:/usr/local/cuda-12.5/compat:$HOME/glibc-2.39/lib:$HOME/miniforge3/envs/rapids-cuda-12.2/lib \
  /home/info/peacockdb/cpp/install/rust-benchmarks/test_node_timing_sf40 --nocapture --test-threads=1'
```

The glibc directory is the *build* host's version (`getconf GNU_LIBC_VERSION` here — 2.39
from this 24.04 box, 2.35 from a 22.04 one), which is what `PATCHED_LD` in
`lib/shadgpu-env.sh` composes; take it from there rather than retyping it. Read
`off wall=` and `events wall=` off the `eprintln!` and put N into Task 10's sentence.
Delete `peacockdb-core/tests/test_node_timing_sf40.rs` afterwards: it is untracked and
unregistered, so `test_ci_coverage.rs` and `test_module_layout.rs` would both have an
opinion about it if it were left behind.

### The baseline the new data is compared against

v1's committed numbers, at `fc6a0de`, for the comparison table Task 11 owes. The
neighbour-inflated figures beside them are dispatch 4's, measured under this same
neighbour — the row to check a fresh run against is "committed", not "with a neighbour".

| case | committed `run_us` | committed `device_us` | with a neighbour |
|---|---|---|---|
| q6 tp1_single | 88 366 | 88 158 | ~553 000 |
| q6 tp4_sized | 217 758 | 217 283 | — |
| q19 tp1_single | 728 263 | 727 664 | ~1 326 000 |

`hbm.tsv` totals, Σ `hbm_bytes` per case: q6 tp1-single 1.78e11, q6 tp4-sized 2.60e11,
q19 tp1-single 1.98e12 — 2.42 TB over the three, against the 37.7 TB the counters pass read
beside the neighbour. `records.tsv` at HEAD: 70 rows for q6 tp1-single, 240 for q6
tp4-sized, 110 for q19 tp1-single.

Two shape changes to expect, neither of them a defect:

- **`calls.tsv` gains columns.** The committed file is v1's —
  `node_seq node_type partition call depth executions calls_per_exec …`, no case key. The
  new one carries `(dataset, sf, query, mode)` and `regions`/`calls_per_region`, so the
  diff is the whole file.
- **`build_profile=` disappears.** It is what the two `#[ignore]`d tests fail on today, and
  it is one of the spec's grep needles. Its four occurrences at HEAD are three lines in
  `testdata/benchmark-results/tpch.sf40/*.benchmark.txt` and one `# run:` line in
  `records.tsv`; the new harness writes `build=release` instead.

### The grep list, run at fc6a0de

Eight of the nine needles are already clean outside `llm-wiki/`. Two findings:

- `build_profile` — four hits, all in the committed data above, and all of them go when the
  data is re-measured.
- `thread_local` — one hit, `cpp/src/peacock/operators.h:17`, inside a comment about why
  ambient state must not be a thread-local. It is **master's own text**, byte-identical
  (`git diff master -- cpp/src/peacock/operators.h` is empty), so it is not this branch's
  residue and nothing here should remove it. Whoever reads the spec's "Done when" list
  should know that this needle can never reach zero while master carries that comment.

### `residue-gate.sh` at fc6a0de: the bp gates are empty, the first section is not

Run here, exit 0. The two `== bp gates` sections — the ones the spec requires empty — are
empty. The first section, `batch.?partition` outside `llm-wiki/`, prints twelve lines, and
that is not a regression: five of them are master's (`node_session.cpp`'s scan comment,
two in `flatbuffers/gpu_plan.fbs`, two under `scripts/exec_model/`), and the seven this
branch adds are prose in its own new files — "the batch-partitioned planning mode" in
`record.rs` and the record's preamble, "at one batch-partitioned mode" in
`corpus_benchmark.rs`, and the like. None is one of the six survivor spellings the gate
exists to catch. A reader who expects the whole script to print nothing will read these as
a finding; it prints them on master too.

### CI on 83479e6 is red because master moved

Run 35002080914 fails all three C++ build jobs at `instrument.rs:32`: the PR's merge commit
carries master's `rmm-pool-budget` (#144, `72fb23f`), where `peacock_install_rmm_pool` takes
a byte budget, and the branch calls it with the one argument the base had. The branch alone
compiles and its gate is green; only the merge does not. So `done` is unreachable without a
rebase, and a rebase is the human's call through the control file. Master's code changes
since the base are thirteen files, all the pool; the overlap with this branch is
`peacock_gpu.h`, `gpu_executor.cpp`, `test_cudf.cpp`, `test_plan_executor.cpp`, `lib.rs`,
plus `build-test.md`, `tickets.md` and the board — conflicts in code go to a developer, the
rest to the coordinator. The one caller to fix is `executor/instrument.rs`, which then owes
the harness a budget figure for `install_rmm_pool`.

### The rebase is authorised

The human said so in the session rather than through the control file. Order agreed: finish
review round 1's reading on `83479e6`; rebase onto master (the coordinator, conflicts in the
wiki and the board its own); one developer takes the code conflicts and the round's blocking
and important findings together, and re-proves the gate on shad-gpu; the sf40 measurement
comes after, on the rebased base. The human reads the review findings before they go to
the developer.

### Rebased onto master at 159ebfd

Thirteen commits replayed; the only conflicts were one doc comment in `peacock_gpu.h`
(master's wording kept, its emphasis lower-cased). The byte-identity set is still clean
against master. What the rebase breaks: `executor/instrument.rs` calls
`peacock_install_rmm_pool` with one argument where master's takes a byte budget first, so the
harness's `install_rmm_pool()` now owes a figure — master's gtests pass `kPoolBytes`, the
number a binary measured it needs. The pre-rebase head is tagged `pre-rebase-bp-benchmarks`.

### Review round 1 on 83479e6: 0 blocking, 7 important, 8 nits

To the developer (dispatch 6), with the rebase fallout:

1. `build-test-shadgpu.sh` `--pull-benchmarks`: the "nothing came home" refusal counts the
   local tree, which always holds the two committed files, so it cannot go red. Count what
   the rsync moved (a stamp before it, `-newer`) or ask the host.
2. `gpu_executor.cpp` `result_from_handle`: the empty-range early return opens no region while
   Rust journals the call; unreachable today only because the driver rejects the range
   elsewhere. Wrap the early return in `time_export` too; fix the Rust comment; cover the
   empty range in `TheSliceAndTheExportOpenRegionsToo`.
3. The three NVTX symbols have no test at any gate. One gtest beside `NodeRegions`: ranges on
   with timing off records no region, and a double push with one pop stays balanced.
4. `nsys_calls.py` writes `calls.tsv` before two of its refusals; move the checks above the
   write and assert in the two refusal tests that no file was left.
5. `executor/mod.rs:822` links `PartitionStat::device_us`, a type that does not exist; it is
   `Region::device_us`.

Recorded here for the signoff, no code change (items 6 and 7):

- Scope overruns beyond `GpuBatch::producer`: `driver/index.rs` gained `nodes_as_recorded`;
  `driver/accounting/tests.rs` followed `end_call(&CallStats)`; `scripts/setup-glibc.sh`
  patches both staging dirs. All three necessary, none listed in the spec's Scope.
- The spec says the two targets are not in `INTENTIONALLY_NOT_IN_CI`; they are, as
  `Exemption::GpuJob`, which is how every GPU-job target is modelled and is machine-checked
  against the staging array. The code is right and the spec's sentence is not.

Nits passed on as optional: `rows_match_the_recipes`'s doc at 11 lines; `call_us`'s doc
explaining `0` as a state `join_regions` refuses; "one case" in `nsys_calls.py`'s message
while the union spans every case; `MEASURED_RUNS` restated in `test_corpus_goldens.rs`.
Dropped: the RAII name of `ScopedNodeTimer`; the pre-existing `[ -f ] && strip` loop body.
Mine: the Corpus benchmarks section at 83 lines against 50–80; the `thread_local` grep hit
in master's own `operators.h` comment, to be named in the signoff.

- Dispatch 6: the rebase fallout and findings 1–5, one developer, gate re-proven on shad-gpu.

## Dispatch 6 — the rebase fallout and review round 1's findings 1–5

### The pool budget the harness now declares

Master's `peacock_install_rmm_pool(bytes, out_info)` takes the budget first, so the Rust
facade does too: `executor::install_rmm_pool(bytes)`, and each binary declares its own
constant beside the rest of its knobs, the way each gtest `main` declares `kPoolBytes`.
Two binaries install one.

- **`test_node_timing`: 2 GiB** (`POOL_BYTES`), and this one is measured. Swept on shad-gpu
  with `PEACOCK_RMM_POOL_BYTES` over the staged binary: 0.25 and 0.5 GiB die in the
  lineitem scan with `Maximum pool size exceeded` at `pool_memory_resource.hpp:276`, 0.75,
  1 and 2 GiB pass. Declared at 2 rather than at 0.75, because the pool cannot grow and a
  budget at the floor is one fragmentation away from that failure.
- **`peacock_gpu_benchmarks`: 69 GiB** (`BENCH_POOL_BYTES`), and this one is **taken, not
  measured** — it is `test_tpch.cpp`'s number for the same sf40 dataset read in the same
  place (measured peak 67.42 GiB there), and also the most that lets two processes share
  the 139.7 GiB H200 (#178), which matters because a gate job can land beside a
  measurement. Its own peak cannot be read from here: `peak_allocated_bytes()` is not in
  the ABI's nineteen symbols, so the only way to size it is to bisect a run — and a
  bisection step is an sf40 case, which this dispatch was told not to run. **Task 11 owes
  the real figure**: run the measurement, and if it survives 69 GiB sweep downward with
  `PEACOCK_RMM_POOL_BYTES` until it dies, then declare the floor rounded up.

Nothing else about the pool reaches this branch: `RmmPool`, the `allocator=` line and the
harness's "would measure over rmm's default resource" assert are unchanged, and a run that
cannot get its budget still fails loudly rather than quietly.

### The rest of the rebase, checked rather than assumed

`git diff pre-rebase-bp-benchmarks HEAD -- cpp peacockdb-ffi peacockdb-core scripts` is
thirteen files, all master's own. Only one caller was stale, and `cargo check -p
peacockdb-core --tests` in the container found exactly it (`instrument.rs:32`, E0061) and
nothing else after the fix. Two of master's files also gained a test, which is a
`build-test.md` correction the coordinator owns: **cuDF GPU smoke (C++) 3 → 4** and **FFI
smoke (Rust) 2 → 3**. This dispatch adds three more: **Plan-executor (C++) 35 → 38**.

### The five findings

1. **`--pull-benchmarks` counted the local tree.** `trees=` now asks the host before the
   rsync — `ssh find … | wc -l` — which is what `pull_one` already does and what the
   refusal's own message claims ("no .benchmark.txt on $REMOTE"). Counting after a pull
   answers "is there a tree here", and the committed records make that yes forever. No
   committed test: nothing runs a remote-touching script in any tier. Proven by simulating
   the two forms over one world (host empty, local tree holding the two committed files,
   record not home): the old form accepts, the new one refuses; and the new command was
   run against the real host, which answers 2 for the tree and 0 for a missing directory.
2. **The empty-range export opened no region.** `peacock_result_from_handle`'s early return
   now runs its two stores inside `session->time_export`, so the journalled call has a
   region to claim whatever the range was. Red first:
   `NodeRegions.AnExportOfNoRowsOpensOneToo` (new, through the C entry point because the
   empty range is decided there) failed with `count` 1 against 2, and passes with the fix.
   The Rust comment in `GpuExport::unload` that asserted the region said what was not yet
   true and now says what is.
3. **The three NVTX symbols had no test.** `NvtxRanges.RangesWithoutTimingRecordNoRegion`
   and `NvtxRanges.ASecondPushReplacesTheFirstRatherThanNesting`, beside the `NodeRegions`
   suite, with a `RangesOn` guard so a failed assertion cannot leave the switch on for the
   rest of the binary. Balance is not observable through NVTX, so `plan_executor.h` gained
   `harness_range_is_open()`, whose doc names the test that needs it. Both tests pass on
   write, so they were reddened by mutation instead — three mutations in one build, each
   reddening exactly its own assertion: the sink following the nvtx switch (recorded 2
   regions, line 1373), `push_harness_range` ignoring the switch (line 1385), and
   `pop_harness_range` as a no-op (line 1392). The other nine `NodeRegions` cases stayed
   green throughout.
4. **`nsys_calls.py` wrote before two refusals.** The `--plans-dir` check and the
   "regions disagree about how many times they ran" check both moved above
   `record.write_tsv`. Red first: `assert not (tmp / "calls.tsv").exists()` in
   `test_a_mode_with_no_golden_is_refused` and
   `test_regions_that_ran_different_numbers_of_times_are_refused` — 3 passed, 2 failed
   before, 5 passed after.
5. **The dead doc link.** `executor/mod.rs`'s `NodeTiming::Events` pointed at
   `PartitionStat::device_us`, a type that exists nowhere; it is `Region::device_us`.

### The optional nits, taken

All four: `rows_match_the_recipes`'s doc trimmed from 11 lines to 10; `call_us`'s doc no
longer explains `0` as a state `join_regions` refuses; `nsys_calls.py`'s refusal says the
capture's regions rather than "one case"'s; and `BENCH_MEASURED_RUNS` moved to
`common/record.rs` as `MEASURED_RUNS`, which is where `BUILD` and `TIMING_MODE` already
live for the same reason — `test_corpus_goldens.rs` now reads it instead of restating it.
The two marked dropped were left alone.

### What was proven, and how

- **rust-only, whole package** (`CARGO_TARGET_DIR=/build/peacock/rust-only-target cargo
  test -p peacockdb-core --features rust-only`, both datasets present): exit 0, no
  warnings — `--lib` 444, `test_cpu_corpus` 448, `test_cpu_end_to_end` 24 + 2 ignored,
  `test_corpus_goldens` 24 + 2 ignored, `test_golden_format` 26, `test_plan_goldens` 20,
  `test_planner_join_capability` 13, `test_module_layout` 11, `test_planner_join_refusals`
  10, `test_ci_coverage` 8, `test_null_analysis` 8, `test_layout_injection` 4,
  `test_cost_model` 3, `test_inc2_conformance` 3, `test_cpu_executors` 1. 1047 passed, 0
  failed, 4 ignored over the package.
- **cuDF-shape build**: `docker-build.sh --no-image --cache-dir /build/peacock --
  build-test-shadgpu.sh --build --build-benchmarks`, exit 0, zero warnings, eight staged
  binaries (seven in `rust-tests/`, the release `peacock_gpu_benchmarks` in
  `rust-benchmarks/`).
- **C++ CPU tier**: `ctest --test-dir cpp/build -L cpu` — 1/1, 12 cases.
- **shad-gpu gate**: `./scripts/build-test-shadgpu.sh --push-binaries --patch --run`, exit
  0, "GPU test run OK". C++ 12 / 4 / 38 / 4 / 4; rust `peacock_gpu_benchmarks` 6 (3
  filtered), `test_gpu_abi` 4, `test_gpu_corpus` 8, `test_gpu_executors` 32,
  `test_gpu_recipe_walk` 10, `test_inc2_conformance` 10, `test_node_timing` 1. Pools
  reserved, all of 78.4 GiB free beside the neighbour's 62 GB: 1.0 (gpu), 1.0 (plan), 69.0
  (tpch), 30.0 (tpchv), 2.0 (node_timing). `test_node_timing` at sf1 q19 tp1-single: off
  wall 447213us, events wall 446709us (-0.1%), 12 regions — the instrument inside the
  run-to-run spread, as the two earlier runs of this dispatch also read (+0.0%, +0.4%).
- **Python**: `test_calls.py` 5, `test_hbm.py` 3, `test_plot.py` 3, all passed.
- **`residue-gate.sh`**: exit 0, both `== bp gates` sections empty, the first section still
  the same twelve lines. `git diff origin/master` over the byte-identical set is empty.

### For the next developer

- The sf40 measurement now also has to confirm `BENCH_POOL_BYTES`. A pool that fails to
  build says so at the top of the log and the harness's own assert refuses the run; a pool
  that builds and then dies with `Maximum pool size exceeded` means 69 GiB was too small.
- The gate was run beside the same neighbour as dispatch 5 (pid 2074022, 62 GB, 92–95 %).
  Green, and no number in it is a measurement.
- `/build/peacock/cargo-target/debug/build/peacockdb-ffi-e640617edfd80d57` was poisoned
  again (`CMAKE_INSTALL_PREFIX=/usr/local`, every build dying on `file INSTALL cannot copy
  … Permission denied`). Deleting that one unit directory fixes it; grep the caches for
  `/usr/local` to find which one.
- Running a gtest binary by hand needs `PEACOCK_TESTDATA_DIR=$REMOTE_REPO/testdata` as well
  as the library path — without it every fixture-reading case dies with "Cannot open file;
  it does not exist", which looks exactly like a broken binary.
- `build-test.md`'s test table is three rows short of the tree and both halves are the
  coordinator's to apply: cuDF GPU smoke (C++) 3 → 4 and FFI smoke (Rust) 2 → 3 arrived with
  the rebase, Plan-executor (C++) 35 → 38 with this dispatch. C++ total 72 → 76, Rust
  1164 → 1165.

### Round 1 closed; the board says reviewing

Dispatch 6 landed as two commits. `build-test.md` corrected for what master's pool change
and this round added: FFI smoke 3, cuDF GPU smoke 4, Plan-executor 38, so 1621 in all. The
board moves to `reviewing` with one thing still owed: the sf40 data (plan Task 11), which
waits for an empty card and carries the harness's real pool figure — `BENCH_POOL_BYTES` is
`test_tpch.cpp`'s 69 GiB, taken and not measured. Round 2 reads the rebased head.

### Review round 2 on 2414a92: 0 blocking, 1 important, 4 nits

All five of round 1's findings confirmed closed with a reachable red each. The important
one was wiki drift master's pool change left: the VRAM-budget bullet in `build-test.md` now
names `test_node_timing` and `peacock_gpu_benchmarks`. Nit 3 taken too (`architecture.md`'s
instrumentation sentence names the gtest fixtures). Nits 1 and 2 go to the data dispatch as
optional: `harness_range_is_open()` belongs in `plan_executor_internal.h`, and
`NvtxRanges.ASecondPushReplacesTheFirstRatherThanNesting` touches no device and could sit in
the cpu tier (which moves two table counts). Nit 4, the shell header length, matches the
tree's idiom and is dropped. The board stays `reviewing`: the data commit is still ahead and
gets its own reading before the completeness pass.

Seen on shad-gpu while checking who held the card: dozens of `/tmp/peacock-gpu-executors-*.parquet`
left by `test_gpu_executors`, which writes its fixture and never removes it. Master's
behaviour, not this branch's; noted for the signoff, no ticket (nothing a user sees).

### Two independent readings of the whole diff, triaged

Two agents wrote a per-file account of `origin/master...2fbdbf2` for the human (kept outside
the repo) and listed what looked wrong. Triage:

For the data dispatch, beside Task 11, in this order:
1. `peacock_executor_collect_node_regions` with `out == NULL` and `cap > 0` reaches `memcpy`
   into null when `recorded <= cap`. Refuse it: non-zero, `last_error`, nothing drained; a
   gtest for the shape.
2. `RegionSink::producer_of` answers 0 for a handle the sink never saw, so a slice or export
   of one is silently charged to seq 0. Throw naming the handle instead; unreachable from the
   harness, which arms timing before `begin_plan`, but C++ should not guess.
3. Capitals for emphasis in new doc comments, against the plan's global constraint: "as its
   CALLER saw it" and "what a measurement IS" in `executor/mod.rs`, "lanes that DRIVE the
   node" in `driver/partitioned.rs`, "the SEQ SET" in `test_plan_goldens.rs`.
4. Two doc strings still describe the first version's host-only fallback:
   `Measured::device_us` ("Zero where a region recorded no complete pair") and
   `NodeTiming::Off` ("Every timing field stays 0"). A CUDA failure is an error now and a
   call without a region is refused.
5. `nvtx_range` with a NUL in the name pushes nothing but returns a guard that pops.
   Either refuse the name or return a guard that does not pop.
6. `create_nsys_profile.sh`: `nsys export … || true` hides an export failure behind a later
   "the trace pass left no capture". Let the export's own failure be the message.
Plus round 2's two optional nits (`harness_range_is_open()` to the internal header; the
push-replaces test to the cpu tier).

Not routed: `plot.py` states the matplotlib version in its docstring and matplotlib writes it
into every PNG's metadata, which is what the spec's sentence rests on; the release copy of
the harness is not stripped (release carries no debuginfo); `install_rmm_pool` is called four
times per case (idempotent by the C++ latch); `LimitStream` prices a slice with varlen 0, as
master does, so a string slice's `out_bytes` in the record is low — for the signoff and a
follow-up, not this task; the four spec-versus-tree deviations already listed above.

## The data landed by hand; the second rebase

The human ran plan Task 11 themselves on verda-gpu (cuDF 26.02, an H200) and committed it as
`f12e4cbf`, `9f74bcaa` and `757377a0` (2026-09-18): `build-test.sh` gained the three benchmark
flags, `create_nsys_profile.sh` takes `--host`, `nsys_hbm.py` reads the 2024.5 counter name, the
tree, `records.tsv`, `calls.tsv`, `hbm.tsv` and the panels are from that run, and #154 was
recounted. The six items triaged above "for the data dispatch" were not taken by it — checked
2026-09-22: `collect_node_regions` still reaches `memcpy` with `out == NULL, cap > 0`,
`producer_of` still answers 0, the four capitalised doc comments are still there. They go to the
rebase dispatch. The spec's Status says the task resumes at `completing` once rebased.

### Rebase 2, 2026-09-22: master moved by twelve merges, not five

The control file said `rebase`; the board said `rebase needed(completing)`. Since the first
rebase's base `159ebfd`, master took the layout chain (test-layout #145, test-support #147,
visibility #148, operator-harness #149, operator-cases #150), chain D (typed-nulls #152,
empty-build #153), chain E (#155–#157) and chain B (#158–#160, #162, #163): 79 commits, 248 files.
`peacockdb-core/tests/` now holds eight targets; every other test module moved under `src/` as
`tests`, `ffi_tests` or `gpu_tests`, the corpus harness lives in `src/test_support/` behind the
`test-support` feature, `pub` is gone from all but eight items with `test_module_layout` as the
guard, and CI stages the device rung as `peacockdb_core_gpu_lib` built `--features gpu`.

Done, in order:

- Tagged the pre-rebase head `pre-rebase-bp-benchmarks-2` (`f488e738`). A one-by-one replay
  conflicted at commit 1 of 24 on the board and would have conflicted in code at most of the
  rest, each an unbuildable intermediate, so the 24 commits were squashed to one on `159ebfd`
  (`b603a5df`, tree identical to the tag) and that one commit rebased onto `origin/master`
  (`9c781e12`).
- Git followed master's moves: the branch's edits to `tests/test_gpu_executors/*`,
  `tests/test_plan_goldens.rs`, `driver/tests/*`, `tests/test_gpu_batch.rs` landed in
  `src/executor/gpu_backend/gpu_tests/*`, `src/planner/tests/plan_goldens.rs`,
  `src/executor/ffi_tests/mod.rs` by rename detection. The C++ side, `peacockdb-ffi/src/lib.rs`,
  `shadgpu-env.sh`, the Python and the data merged clean.
- 25 files conflicted. The four wiki files are resolved and staged: the board by ownership
  (master's chains A and B, this chain's section appended); `tickets.md` with master's counter —
  master had already taken #201 for the murmur gate, so this branch's ticket (the tree does not
  name its device and stack) is **#226**, the counter is 227, and `build-test.md`'s one link to it
  must say #226; #154 keeps the branch's recount (21 sites, the same 21 on master) with master's
  note that `union.cpp`'s site went; `architecture.md`'s ABI paragraph now says twenty-one
  symbols — master's 18 (`handle_schema`, `handle_from_arrow` came) minus the floor plus the four
  this branch adds — and matches the auto-merged header. `build-test.md` took master's two-table
  structure; the branch's three C++ rows are merged into master's wording with master's counts
  left in place, the Calibration scripts row is added, `peacock_gpu_benchmarks` is in the
  cpp-build-2502 staging sentence and the `--skip bench_` sentence is back. Still owed there once
  the developer's layout is known: the two Rust rows (GPU timing method, Corpus benchmarks) in the
  gpu block's format, every count, the `tests/common/…` paths at the Corpus benchmarks section
  and the VRAM-budget bullet's `test_node_timing` entry.
- 21 code files are left with conflict markers for the developer (dispatch 7), plus two
  modify/delete: `tests/common/corpus_gpu.rs` and `tests/common/mod.rs`, deleted on master and
  modified by the branch, left in the tree as the branch's versions. The branch's six new test
  files sit in a `tests/common/` master has emptied and have to be re-homed.
- verda and verda-gpu do not resolve; shad-gpu answers with the same neighbour as dispatches
  5–6 (pid 2421888, 62 GB, 7 h) plus a CI run. The gate fits beside it; nothing measured there is
  published.

Recovery if this run dies mid-rebase: `git status` shows `rebase in progress` onto `9c781e12`
with `b603a5df` being applied; the four wiki files are staged, the code is the developer's
working tree. `git rebase --abort` returns to `b603a5df` on the old base, and the tag holds the
unsquashed history.

## Dispatch 7 — resolve the rebase's 21 code conflicts and prove the branch on `9c781e12`

The previous coordinator died (API 500) after writing the section above and before dispatching.
This run picks up at the same point: the rebase is still in progress, the four wiki files are
staged, the control file is empty, verda still does not resolve, shad-gpu answers (H200, the
same 62 GB neighbour). Dispatch 7 is the rebase alone — every conflict resolved, the branch's
new test files re-homed onto master's layout, both rungs building, the proving commands green —
and stops there. The six triaged items and round 2's two optional nits go to the same developer
as a second step once the rebase commit exists, so the rebase and the fixes are two commits and
the rebase can be read on its own.

Placement of the branch's `tests/common/{corpus_benchmark, gpu_session, record}.rs`,
`corpus_gpu.rs` and `mod.rs` is the developer's decision, measured against `test_module_layout`
and `coding-style.md`'s `test_support` rules; master moved `corpus_gpu.rs` to
`src/test_support/corpus_gpu.rs` in `a76563f3`, and the branch's 64-line cut of it re-applies
there. The developer records where the files landed; the coordinator then corrects the
`tests/common/…` paths in `build-test.md`'s Corpus benchmarks section and the test table.

### Where the harness landed

Master's rule decided it: a `tests/*.rs` binary reaches the engine only through `SURFACE`
(the CLI's items, receipt required) or through `test_support`'s bare `pub` behind the
feature, and nothing this branch adds is the CLI's. So every engine-facing item the first
version left `pub` in `executor/mod.rs`, `plan_text/mod.rs` and `gpu_backend/mod.rs` is
`pub(crate)` now — `AbiTarget`, `AbiCall`, `AbiCalls`, `Region`, `Measured`, `Measurements`,
`join_regions`, `node_measured`, `nodes_as_recorded`, `RmmPool`, `NodeTiming`, `NvtxRange`,
the five instrument functions, `collect_regions`, `CallSite`, `render_timings`,
`RunReport::abi_calls`, `CallStats::calls` — and the harness moved under `src/test_support/`:

- `tests/common/record.rs` → `src/test_support/record.rs` (every shape), with
  `src/test_support/record/tests.rs` holding
  `a_rows_node_seq_names_the_steps_its_recipes_line_prints`, which the first rebase had put
  in `planner/tests/plan_goldens.rs` — moved so `planner/` stays byte-identical to master,
  and because what it tests is `declared_steps`, the record's reading of a plan.
- `tests/common/corpus_benchmark.rs` → `src/test_support/corpus_benchmark.rs`
  (`not(rust-only)`); `BenchmarkCase`, `NOT_TIMED` and the record's consts and types
  (`COLUMNS`, `Capture`, `RunMeta`, `RECORD_PATH_ENV`, `CAPTURE_ENV`, `TIMING_MODE`,
  `BUILD`, `MEASURED_RUNS`) are declared in `test_support/mod.rs`, the file a type that
  crosses out is declared in.
- `tests/common/gpu_session.rs` → `src/test_support/gpu_session.rs`; `corpus_gpu.rs`'s
  private `Session` is gone, as the branch's 64-line cut intended.
- New: `src/test_support/node_timing.rs`, the facade `test_node_timing` measures through
  — `device_plan(dataset, sf, query, mode) -> DevicePlan` (opaque; the plan is made once),
  `set_node_timing("off" | "events")` (exhaustive, as `mode_named` is), `timed_run(&plan)
  -> TimedRun {total_us, calls, journalled, regions, host_us, device_us}` and
  `install_rmm_pool(bytes) -> String`. `calls` is the executor-call count the journal keeps
  in either mode (one `AbiCalls` entry per call that reached an executor), which is what
  master's removal of `RunReport.calls` left for the "both modes made the same calls" check.
- `tests/common/mod.rs` is not recreated; `tests/common/` holds the two `.inc` files.
- `test_support/mod.rs`'s facade for the binaries: `declared_for`, `results_file`,
  `benchmark_case`, `record_header`, `append_records`, `rows_match_the_recipes` (the
  declared set typed `BTreeSet<u32>`, since `Seq` is a wire name the guard would refuse),
  plus the node-timing four. `record_rows` and `declared_steps` name engine types and stay
  behind it.

The two binaries stay in `tests/` and reach nothing else: `peacock_gpu_benchmarks.rs` is
the macro, the include and its six assertions; `test_node_timing.rs` keeps every one of
its assertions and its three knobs. `test_ci_coverage`'s benchmark guard moved into
`test_ci_coverage/runners.rs`, where master put every runner reader it needs, and reads
the loop through master's `rust_gpu_runner_loop`. The `INTENTIONALLY_NOT_IN_CI` list is
master's two entries plus `test_node_timing` and `peacock_gpu_benchmarks`; the four
`test_gpu_*`/`test_inc2_conformance` targets are `gpu_tests` modules of the lib now.

What was dead under `rust-only` once the journal stopped being `pub` — the journal's
fillers are the device backend, its readers the harness and the driver tests — carries
`#[cfg_attr(feature = "rust-only", allow(dead_code))]`, the idiom `common.rs` and
`executor/errors.rs` already use for the GPU sink's helpers.

### The other resolutions

- `gpu_backend/mod.rs`: master hoisted the backend structs into the facade; the branch's
  `site: CallSite` field is applied to the hoisted declarations of `GpuPartitionAccumulator`,
  `GpuEmitter` and `GpuJoin`, and `GpuExport::unload` carries both master's
  `declared_precisions`/`schema_divergence` and the branch's journal entry. `accumulate.rs`
  keeps master's `State` enum with the branch's `site` and `one_call` journaling.
- `driver/index.rs`: master deleted `post_order_of_every_node` (its guard reads the index
  directly now); only `nodes_as_recorded` is added.
- Runners: `RUST_TESTS=(test_gpu_corpus test_node_timing peacock_gpu_benchmarks)` beside
  master's `RUST_LIB_STAGED`/`RUST_LIB_RUNG`; the run loops keep master's `rung_args` and
  add `$skip` (`--skip bench_` for the harness) — `\$rung \$skip` in `pipeline.yml`,
  `\$skip "\${args[@]}"` in both scripts. `build-test.sh`'s `stage_cargo_test_binary`
  takes master's lib-or-test selection so the branch's `--build-benchmarks` and the staging
  loop share one function.
- `gpu_executor.cpp`: `time_export` wraps master's precision-carrying
  `export_table_to_ipc`; `node_session.cpp` keeps master's `adopt` beside `time_export`
  and `collect_node_regions`. `adopt`ed handles have no producer, so `producer_of` answers
  0 for them — triage item 2, the second step's.

### Environment, this dispatch

The box was reset between dispatches: `/build` is gone and cannot be created (no sudo),
docker holds no image, `third_party/cudf` was uninitialised, `testdata/{tpch,tpcds}.sf1`
absent from both checkouts. So: the cache dir is `docker-build.sh`'s own fallback,
`~/.cache/peacock-build` (rust-only in `~/.cache/peacock-build/rust-only-target`); the
builder image was rebuilt (`docker-build.sh --image-only`, a 6.3 GB pull); the submodule
was re-initialised; the datasets regenerated with `generate_testdata.sh --bench tpch|tpcds
--sf 1` (synthetic embeddings, as CI does). Everything is a cold build.

### What the rebase broke, found by building

- `every_timed_case_is_enabled_on_a_device` was red on master's `corpus_cases.inc`: the
  branch's case-list reader asserted the tail after `)` is exactly `;`, and master's
  `shuffle_stddev` line ends `; // #225`. The reader now strips a `//` comment before that
  assertion, which still refuses a second invocation on the line.
- `NodeRegions.AnExportOfNoRowsOpensOneToo` (dispatch 6's gtest) called
  `peacock_result_from_handle` with the old six arguments; master's export takes the two
  precision arguments, so the call passes `nullptr, 0` as master's other sites do. Found by
  the container build, not by git — the hunk merged clean.
- `docker-build.sh` maps `github.com/google/flatbuffers.git` onto
  `$CACHE_DIR/git-mirrors/flatbuffers.git` unconditionally, so on an empty cache every
  cmake configure dies cloning it; its comment says the mapping is harmless when the mirror
  is absent, which is not what git does. Populate the mirror as the comment instructs
  (`git clone --mirror … "$CACHE_DIR/git-mirrors/flatbuffers.git"`) before the first build
  on a fresh box. Master's script, not this branch's; left as it stands.
- The two `#[ignore]`s plan Task 11 step 4 was to remove
  (`every_committed_tree_reports_a_release_build`,
  `the_records_preamble_is_what_record_header_writes`) are still there and both pass
  against the committed H200 data under `-- --ignored`; removing them is the second step's.

### What was proven on `9c781e12`, and how

- **rust-only** (`CARGO_TARGET_DIR=~/.cache/peacock-build/rust-only-target`, both sf1
  datasets regenerated): `--lib` 597 passed, 2 ignored, 0 warnings;
  `test_corpus_goldens` 24 + 2 ignored (and the 2 pass under `--ignored`);
  `test_ci_coverage` 9; `test_module_layout` 17; `test_golden_format` 26; `test_cost_model`
  3; `test_cpu_corpus` at `cpu_tpch_{q6,q19,q1,shuffle_stddev}_*` and `cpu_tpcds_q84_*`
  25 passed, goldens unchanged; `cargo build --features rust-only -p peacockdb` exit 0.
- **cuDF shape** (`docker-build.sh --no-image --cache-dir ~/.cache/peacock-build --
  build-test-shadgpu.sh --build --build-benchmarks`): exit 0, no new warnings — the one
  warning, `AsArray` unused in `src/tests/gpu_tests/aggregate_dimension_cases.rs`, is
  master's own line. Staged: `test_gpu_corpus`, `test_node_timing`, `peacock_gpu_benchmarks`,
  `peacockdb_core_gpu_lib` in `rust-tests/`, the release harness in `rust-benchmarks/`.
- **C++ CPU tier**: `ctest --test-dir cpp/build -L cpu` 1/1, `peacock_cpu_tests` 13 cases.
- **shad-gpu gate** (`--push-binaries --patch --run`, the 62 GB neighbour still there,
  every pool built from 78.4 GiB free): exit 0, "GPU test run OK". C++ `peacock_cpu_tests`
  13, `peacock_gpu_tests` 4, `peacock_plan_tests` 52 (`NodeRegions` 9, `NvtxRanges` 2 among
  them), `peacock_tpch_tests` 4, `peacock_tpchv_tests` 4; rust `peacock_gpu_benchmarks` 6 (3
  `bench_` filtered), `peacockdb_core_gpu_lib -- gpu_tests::` 535 (602 filtered),
  `test_gpu_corpus` 28, `test_node_timing` 1 — q19 sf1 tp1-single: off 446121 µs, events
  446477 µs (+0.1 %), 12 regions. No binary ran 0 tests.
- **Python**: `test_calls.py` 5, `test_hbm.py` 4, `test_plot.py` 3 (the last under
  `~/data/miniforge3/envs/rapids-26.02/bin/python3`; `/usr/bin/python3` lost matplotlib in
  the reset).
- `residue-gate.sh`: both `== bp gates` empty, exit 0. The spec's grep list finds only
  master's own `thread_local` in a `cpp/src/peacock/operators.h` comment. The byte-identical
  set (`cpp/src/operators/`, `expr.cpp`, `Cargo.toml`, `build.rs`, `.gitignore`,
  `flatbuffers/`, `plan/`, `planner/`, `wire/`) diffs empty against `origin/master`.
- `pipeline.yml` parses as YAML; all 30 rendered `run:` blocks pass `bash -n` with
  `${{ }}` substituted; `bash -n` on both scripts. No conflict marker outside `llm-wiki/`.

### Rebase 2 committed: `9ee8b1b2` on `9c781e12`, PR #139 force-pushed

Dispatch 7 resolved every conflict and proved the branch green on the new base (its own
"Dispatch 7" section above has the placement and the counts). The coordinator staged by path,
`git rm`'d the five `tests/common/*.rs` the re-homing emptied, finished the rebase as one
commit, and force-pushed; PR #139 reads one commit against master. The board returns to
`completing`. Two things to carry: the ffi rung (`--lib -- ffi_tests::`, `test_ffi`) was
compiled in the cuDF-shape build but not executed locally — the box has no CUDA runtime, and no
earlier dispatch ran it either; CI's dataset-matrix is where it runs, and a red there is a
finding for the developer. And `/build` is gone on the dev box: caches now live under
`~/.cache/peacock-build`, which supersedes the plan's global constraint naming `/build/peacock`.

Next: the same developer takes the six triaged items, round 2's two optional nits and the two
`#[ignore]`s in `test_corpus_goldens` (green against the committed data), as one commit; then
the completeness pass reads `origin/master...ENS-bp-benchmarks` as one change.

## Dispatch 7, step 2 — the six triaged items, round 2's nits, the two `#[ignore]`s

On `9ee8b1b2`, one commit's worth, nothing outside the branch's own lines.

1. `peacock_executor_collect_node_regions(ex, NULL, cap > 0, …)` is refused before the
   session is consulted — non-zero, `last_error` "collect_node_regions: a null buffer with
   capacity N — (NULL, 0) asks the count", nothing drained. `NodeRegions.ANullBufferWithACapacityIsRefused`
   sits in the cpu tier (`cpp/tests/cpu/test_executor.cpp`), since the refusal needs no
   session; reddened by mutation (the message read "no plan loaded"). One line in the
   plan-executor `AskingTheCountDrainsNothing` proves the shape drains nothing with a live
   session.
2. `RegionSink::producer_of(handle, what)` throws for the two populations it cannot name.
   A handle the sink never saw — produced before timing was turned on — throws saying so.
   An adopted handle: `adopt` notes it in the sink as `RegionSink::kAdopted` (`UINT64_MAX`,
   never a seq), and a slice or export of it under `Events` is refused naming the adoption.
   The decision: no node produced the table, so no node is charged — charging seq 0 would
   put the region on a real node while the journal named another, and the join would not
   close; the harness uploads with timing off, and the one `gpu_tests` arm that arms timing
   (`a_slice_and_an_export_are_charged_to_the_node_that_produced_the_handle`) scans, so the
   adopted refusal is unreachable from the Rust side. `slice_handle` names the producer
   before it consumes the handle, so a refusal leaves the registry as it was. Two
   plan-executor gtests: `NodeRegions.AHandleFromBeforeTimingCannotBeCharged` and
   `NodeRegions.AnAdoptedHandleCannotBeCharged`, the latter through `NodeSession::adopt` on
   a copy of a scanned table. Not reddened by mutation (a second shad-gpu cycle); before the
   change `producer_of` returned 0 without throwing, which `EXPECT_THROW` fails on.
3. Capitals: "CALLER", "IS" (`executor/mod.rs`), "DRIVE" (`driver/partitioned.rs`) lowered;
   "SEQ SET" had already gone with the move to `record/tests.rs`. A sweep of every comment
   line the branch adds found three more — "ALSO" (`build-test.sh`), "BUILD host" and
   "REMOTE shell text" (`shadgpu-env.sh`) — lowered too. `USAGE`/`WRITES`/`BENCHMARK OUTPUT`
   are the shell headers' section names (the tree's idiom), `STRING` is cuDF's type id.
4. `Measured::device_us` says every region has its pair (a failed pair is a collection
   error, a call with no region is refused by the join); `NodeTiming::Off` says no region
   is opened, so a collection answers with none.
5. `nvtx_range` panics naming the name on an interior NUL, before the C side is touched;
   `executor::ffi_tests::an_nvtx_range_name_with_a_nul_is_refused` (`#[should_panic]`),
   watched red first. The ffi rung is 4 now.
6. `create_nsys_profile.sh`: a failed `nsys export` is its own message and exit 1, carrying
   the pass's status; `bash -n` clean.
7. `harness_range_is_open()` declared in `plan_executor_internal.h`;
   `NvtxRanges.ASecondPushReplacesTheFirstRatherThanNesting` moved to the cpu tier with its
   own `RangesOn` guard. Counts for `build-test.md`: **C++ CPU/FFI unit 13 → 15** (the
   moved NVTX case and the null-buffer refusal), **Plan-executor 52 → 53** (−1 moved, +2 of
   item 2), **FFI GpuBatch surface (`ffi_tests`) 3 → 4**, **Corpus goldens 24 → 26**.
8. The two `#[ignore]`s are gone; `test_corpus_goldens` is 26 passed, 0 ignored.

Proven: rust-only `--lib` 597 (0 warnings), `test_corpus_goldens` 26, `test_module_layout`
17, `test_ci_coverage` 9; container `ffi_tests::` 4, `peacockdb-ffi --test test_ffi` 3;
`ctest -L cpu` 1/1 with `peacock_cpu_tests` 15; cuDF-shape build exit 0, five binaries
staged, no new warnings; shad-gpu gate exit 0 "GPU test run OK" — C++ 15 / 4 / 53 / 4 / 4,
rust `peacock_gpu_benchmarks` 6 (3 filtered), `peacockdb_core_gpu_lib -- gpu_tests::` 535,
`test_gpu_corpus` 28, `test_node_timing` 1; `residue-gate.sh` bp gates empty; the
byte-identical set diffs empty against `origin/master`.

### Step 2 committed as `be07c7b1`; the wiki counts

`build-test.md` recomputed on the branch: cpu 1178 (`--lib` 599 with its two ignored cases,
`test_corpus_goldens` 26), ffi 7, gpu 573 (`peacock_gpu_benchmarks` 9, `test_node_timing` 1),
C++ 94, Python 381, 2322 in all. One row was stale on master itself and is corrected here
rather than left: FFI smoke says 3 (`test_install_rmm_pool_rejects_a_zero_request` came with
master's pool change and the row still said 2). cuDF GPU smoke goes 6 → 4 because this branch
deletes the two floor tests. The step-2 ffi rung was executed in the container this time, so the gap noted
above is closed: `ffi_tests::` 4, `test_ffi` 3.

Next: the completeness pass — a reviewer for what is wrong, a fresh analyst for what is
missing, over `origin/master...ENS-bp-benchmarks`, neither seeing the other's list.

## Completeness pass — the analyst's reading

Read `origin/master...ENS-bp-benchmarks` (`9c781e12`..`9e799650`, 99 files) as one change,
against the spec's constraints and "Done when", `build-test.md`'s coverage expectations, and
`architecture.md`. What was checked and holds, then what is missing, then the page list.

### Checked and holding

- The four products are on the branch with the producers the diagram names, byte-identical to
  the human's H200 commits (`9f74bcaa` and neighbours; `pre-rebase-bp-benchmarks-2`). Both
  trees: every `total_us` is the sum beside it, `device_us` = Σ `total_us`, ten `runs`, `run_us`
  the second-smallest, `build=release`. The record: 450 rows × 17 columns, no empty cell, 45 rows
  per `run_index` 0..9, the heading is `record_header()`'s, and the chosen run is in it (q19 run
  9, q6 `tp1-single` run 5, q6 `tp4-sized` run 9 — Σ `device_us` per run equals the tree's).
  `calls.tsv` keys by all three cases (138/63/77 rows). `hbm.tsv` joins 450/450 on the
  8-tuple. Six panel directories and `index.html`, every entry backed by a file.
- The instrument: 21 ABI symbols counted in `peacock_gpu.h`; the region is the five fields; no
  sync inside a region; `varlen_content_bytes` is the one sync; NVTX names as specified.
- CI, scripts, guard: `peacock_gpu_benchmarks` and `test_node_timing` in the staging line,
  `RUST_TESTS`, the `GPUSET` heredoc, `--skip bench_` on both runner loops; `die`, `pull_one`,
  `PATCHED_LD`, `passed_count` once in the lib; `--run-benchmarks` refuses before any push,
  `--pull-benchmarks` dies on nothing home / a dead run; `PCK_TEST_FILTER` the one name. The
  spec's "not in `INTENTIONALLY_NOT_IN_CI`" describes the first version's guard: on master
  `Exemption::GpuJob` is the verified statement that a target runs from the staged artifact,
  and `test_gpu_corpus` is listed the same way — the placement is right.
- Rust-only pins present: trailer arithmetic, release build, preamble, field count, bare
  calls, timed ⊆ device-enabled. Pytest 5 + 4 + 3. Done-when greps all zero (the one
  `thread_local` is master's comment). Byte-identity set diffs empty. `== bp gates` empty.
- Counts: every row of the two tables recounted from `#[test]`/`#[tokio::test]` and
  `TEST`/`TEST_F` — cpu 1178, ffi 7, gpu 573, C++ 94, Python 381, 2322 — and every block
  header is the sum of its rows. VRAM bullet matches `POOL_BYTES` (2 GiB) and
  `BENCH_POOL_BYTES` (69 GiB). The data-flow diagram matches the scripts.
- #226 and the counter move are the human's own commit `757377a0`; the Corpus benchmarks
  section is 105 lines against the spec's 50–80 because the human's 26.02 paragraph joined it.

### Findings — 0 blocking, 5 important

1. **`llm-wiki/build-test.md:1052-1155`** — the spec's one required sentence is absent: "the
   run is measured under event timing and what that cost on sf40 and how it was measured". The
   test-table row (:434-436) promises "the effect is reported from the sf40 run instead" and
   nothing reports it; the only figure anywhere is sf1 q19 on shad-gpu beside a 62 GB
   neighbour (+0.1 %, dispatch 7). Anchor: spec "Wiki and tickets"; plan Task 11 step 3 was
   never run. Fix: `test_node_timing`'s procedure with `SF = "40"` on q19 at `tp1_single` on
   the H200 (not committed), then the sentence with N, the host and the cuDF version.
2. **`scripts/calibration/plot.py:19-22`** — states matplotlib 3.9.2 as "what the committed
   panels were rendered with"; all 17 committed PNGs embed `Matplotlib version 3.5.1`. Anchor:
   spec "The panels" — the script "states the matplotlib version its output was rendered with".
   Fix: say 3.5.1 (verda-gpu's python), and have `write_index` (:642-667) print
   `matplotlib.__version__` into `index.html` so the claim is a product rather than prose.
3. **Residue: seven new hits in `residue-gate.sh`'s `== residue` section** — "batch-partitioned"
   as prose at `src/test_support/corpus_benchmark.rs:1`, `src/test_support/mod.rs:822`,
   `src/test_support/record.rs:266` (`HEADER_NOTES`, hence `testdata/calibration/records.tsv:16`),
   `tests/common/corpus_benchmark_cases.inc:7`, `tests/peacock_gpu_benchmarks.rs:1`,
   `scripts/calibration/nsys_hbm.py:293`. `drop-mode-name` (#141) retired the phrase from every
   sentence ("a qualifier against an alternative that no longer exists"); the gate's first
   section is what catches it. Master's own five hits (`batch→partition` map,
   `ParquetBatchPartitioner`) predate the branch. Anchor: spec line 10 / Done when "the residue
   gate empty". Fix: "planning mode" / "the corpus" at the six source sites; `records.tsv:16`
   and `HEADER_NOTES` in one commit, since `the_records_preamble_is_what_record_header_writes`
   pins them together.
4. **`scripts/build-test-shadgpu.sh:393`** — the gate's remote script keeps master's literal
   `/home/info/peacock-datasets/testdata/tpch.sf40` while the branch added `SF40_DIR` to the lib
   and uses it only in the bench script (:508). Anchor: spec "the sf40 directory ... live once,
   in `lib/shadgpu-env.sh`". Fix: `export PEACOCK_TPCH_SF40_DIR=$SF40_DIR`.
5. **Two refusals the spec and `build-test.md` state with no test**: an append under a
   different `# run:` heading (`record.rs` `assert_run_conditions_match`; `build-test.md:1147`)
   and `Capture::from_env` on an unnamed value (`build-test.md:1154`). The harness binary's
   `the_record_is_written_only_when_a_path_is_named` writes one fresh file and never a second
   heading. Anchor: build-test.md's coverage expectations. Fix: two `#[should_panic(expected =
   …)]` cases beside it in `tests/peacock_gpu_benchmarks.rs`, which already owns the env var
   under `--test-threads=1`.

Wiki corrections for the coordinator, no developer needed:

- `build-test.md:789-791` lists the staged `--test` targets as `test_gpu_corpus`,
  `peacock_gpu_benchmarks` and the lib; `pipeline.yml:383` also stages `test_node_timing`.
- `build-test.md:1124-1126` explains `1` and `[]` in `time_us` but not `0`, which appears 13
  times in the committed trees — a driver call the backend answered without crossing the ABI
  (an accumulator holding a batch, a join's `set_build`); the rule lives only in
  `bench_text.rs:19-26`. One clause.
- `build-test.md:1134-1135` "Both counts are constants in `src/test_support/corpus_benchmark.rs`"
  — `MEASURED_RUNS` is `test_support/mod.rs:843`; `BENCH_WARMUP_RUNS` is `corpus_benchmark.rs:132`.

Scope deviations for the signoff, each a consequence of an in-scope change:
`scripts/setup-glibc.sh` (patches `rust-benchmarks/`, without which the harness ships
unpatched), `cpp/src/plan_executor_internal.h` (`harness_range_is_open` for the cpu gtest),
`executor/driver/index.rs` (`nodes_as_recorded`), `src/tests/gpu_tests/device.rs` (the new
`GpuBatch::new` argument).

### `architecture.md` — sentences the branch falsified

Three false, quoted with their heading:

- **Interfaces**, the instrumentation bullet (:936-937): "**Instrumentation is process-global
  and off by default; `peacock_gpu_benchmarks`, `test_node_timing` and the plan-executor
  gtests' fixtures are what turn it on.**" — also turned on by
  `executor::gpu_backend::gpu_tests::accumulate`
  (`a_slice_and_an_export_are_charged_to_the_node_that_produced_the_handle`, :234) and by the
  cpu-tier gtests `NodeTiming.*` and `RangesOn` (`cpp/tests/cpu/test_executor.cpp:153-197`).
  Say: the benchmark harness and the timing tests — `peacock_gpu_benchmarks`,
  `test_node_timing`, one `gpu_tests` case and the gtest fixtures on both tiers.
- **Node display** (:1167): "**There are two node lines, and the difference is which golden it
  is in.**" — `plan_text/bench_text.rs` renders a third, in `<mode>.benchmark.txt`, which is
  not a golden: the node line without `schema=` and without `output_rows`/`output_bytes`, with
  `time_us=[[…]] total_us=N` beneath it. Say: three node lines, the third the benchmark tree's,
  described in build-test.md's Corpus benchmarks.
- **Execution → Traits** (:538): "`GpuBatch` wraps a `u64` handle plus the session reference
  its `Drop` needs." — `GpuBatch` (`executor/mod.rs:91-97`) also carries `producer: Seq`, the
  seq of the call the handle came out of; a slice propagates it and an export is charged to it,
  and C++ keeps the same fact per handle. Add: "and the seq of the call that produced it, which
  is what a slice or an export is charged to".

Three incomplete rather than false — the coordinator's call:

- **C++ executor layout** (:1017-1019): "`node_session.cpp` (the post-order index, the handle
  registry, and the multi-partition dispatch — …)" — now also the timed regions (`RegionSink`,
  `ScopedNodeTimer`, `collect_node_regions`), the NVTX domain and the harness range.
- **The handle registry has no type** (:988-990): "It is two fields inside the private
  `NodeSession::Impl` … with allocation, lookup, consume-on-read and erase written inline at
  every site that touches them." — the registry is still those two fields, but each of the
  seven allocation sites now pairs `next_handle++` with `note_producer` into a third
  handle-keyed map in `RegionSink` (measurement-only, never erased), and `adopt` files a handle
  there as `kAdopted`. The paragraph's conclusion holds and is stronger for it.
- **The wire format → From node to seqs** (:803-804): "`execute_node` is stateless per seq —
  the only state is the handle registry" — under `Events` the session also counts calls per
  seq (`RegionSink::calls_made`, the `call_index`); state that names a call, not one that
  changes what it computes, and absent from a shipping query.

Verified unchanged and true: the ABI count and groups, the per-call entry points, `NodeStats`
(rows and varlen bytes), the byte formula in `common.rs` alone, the `varlen_content_bytes`
sync, `collect_node_regions` draining afterwards, NVTX as a separate switch, "Three additive
ABI symbols", `Backend::executors_for`, `run_with_hook`, `CallStats.scratch_bytes`, the four
accounting rules, the driver's file list, "A call can fail" (the driver still adds node and
lane; the backend now also names them).

## Completeness pass — the two lists compared, and dispatch 8

The reviewer (what is wrong) read `9c781e12..9e799650`: 0 blocking, 7 important. The analyst's
list is the section above: 0 blocking, 5 important, three wiki corrections, three
`architecture.md` sentences falsified and three incomplete. Neither saw the other's. Where they
meet: the sf40 event-timing sentence the spec requires is absent (plan Task 11 step 3 never ran);
"batch-partitioned" is back in seven lines, growing `residue-gate.sh`'s first section from
master's five hits to twelve; and the same scope deviations, to be named in the signoff
(`scripts/setup-glibc.sh`, `cpp/src/plan_executor_internal.h`, `executor/driver/index.rs`,
`executor/ffi_tests/mod.rs`, `src/tests/gpu_tests/device.rs`,
`executor/driver/accounting/tests.rs` — each a consequence of an in-scope change).

The reviewer's own four:

1. `corpus_benchmark.rs:102-105` — the trailer's `device_us` is Σ raw `Measured::device_us`,
   the tree's `total_us` is Σ `call_us()` clamped to `.max(1)` (`bench_text.rs:30-35`); the
   rust-only pin `every_total_us_is_the_sum_of_the_time_us_beside_it` asserts they agree, so a
   future collection with one sub-microsecond region turns CI red. Latent today (the record's
   smallest `device_us` is 3 µs). Fix: one rule, exposed by `bench_text` and used by the trailer.
2. `nsys_calls.py:427,436-438` and `calls.tsv` — the columns named `node_seq`/`node_type` hold
   the fb seq and kind, which the record calls `recipe_seq`/`recipe_kind`; a join on the shared
   name lands on another node. Rename in the script, its test and the committed file's header.
   The trace capture lives on verda-gpu, which does not resolve, so the header cells are renamed
   by hand — a re-run would write the same rows under the new names. Named in the signoff.
3. `tests/test_corpus_goldens.rs` is 1052 lines (704 on master), over the 1000-line limit. The
   benchmark-and-record block moves to `tests/test_corpus_goldens/benchmark.rs` as a `mod`, the
   shape `test_ci_coverage/runners.rs` has.
4. `record.rs:32-99` — `row()` is a positional list against a heading that is `COLUMNS.join`,
   and the writer runs in no CI tier (the `bench_` cases only); a cell reordered against
   `COLUMNS` would pass every pin. One rust-only test in `record/tests.rs` asserting each cell by
   `COLUMNS.position(name)`.

The analyst's own three: `plot.py:19-22` says matplotlib 3.9.2 where every committed PNG embeds
3.5.1 (say 3.5.1; print `matplotlib.__version__` into `index.html` so the claim is a product);
`build-test-shadgpu.sh:393` keeps the sf40 path as a literal beside the lib's `SF40_DIR`; and two
refusals the wiki states have no test — an append under a different `# run:` heading and
`Capture::from_env` on an unnamed value — two `#[should_panic]` cases in
`peacock_gpu_benchmarks.rs`.

Not taken, both agreed: `INTENTIONALLY_NOT_IN_CI` carrying both targets as `Exemption::GpuJob`
is master's verified "runs from the staged artifact" and what the plan asked for; the spec's
sentence describes the first version's guard. #226 moved the counter the spec said would not
move; it is the human's own commit and a real follow-up.

Dispatch 8 is all of the above that is code, plus the sf40 measurement for the sentence: the
same developer, one commit. The wiki side — the sentence itself, the three `build-test.md`
corrections, the `architecture.md` sentences — is the coordinator's, applied when the figure is
in. verda still does not resolve; shad-gpu answers with the same neighbour, so the sf40 figure
is measured beside it and the sentence says so.

## Dispatch 8 — the completeness pass's seven code items and the sf40 figure

On `ce6f447c`. Kept current as each item lands; the measurement is last.

Progress: complete — eight code items landed and proven, the sf40 figure measured, the
timing copy deleted locally and on the host.

1. `plan_text::timed_device_us` (delegating to `bench_text::timed_device_us`, Σ of every
   node's `total_of`) is the trailer's `device_us`; `run_section` calls it. The pin is in
   `driver/tests/render.rs::a_region_that_rounds_to_nothing_still_ran`: rendered
   `total_us=31` and `timed_device_us == 31`; reddened by mutation (a raw sum answers 30).
2. `nsys_calls.py` writes `recipe_seq`/`recipe_kind` (rows, `cols`, `NOTES`, docstring);
   `test_calls.py` reads them (red first, 2 failed, then 5 passed); `calls.tsv`'s heading
   edited by hand — the two column cells and the `NOTES` line — and checked equal to what
   `record.write_tsv(NOTES, cols, [])` writes now; the 262 rows are untouched. The capture
   is on verda-gpu, which does not resolve; a re-run writes the same rows under the new
   names. `nsys_hbm.py` and `plot.py` read `records.tsv`'s `node_seq`/`node_type`, which are
   the record's own and stay.
3. `tests/test_corpus_goldens.rs` is master's 704 lines plus 7 (a `#[path]` `mod benchmark`
   and one doc sentence); the benchmark-and-record block is
   `tests/test_corpus_goldens/benchmark.rs` (356 lines), every case under its old name
   (`benchmark::…`). 26 passed, 0 ignored; `test_module_layout` 17, `test_ci_coverage` 9.
4. `record/tests.rs::each_cell_sits_under_the_column_named_for_it`: q6 on `CpuBackend`, one
   armed `AbiCall` in the first `(node, lane)` that reached an executor, one `Region`,
   `record_rows`, every cell by `COLUMNS.position`. Red first with `host_us`/`device_us`
   swapped in `row()` (66 against 77).
5. `plot.py`'s docstring says 3.5.1 (verda-gpu's), and `write_index` prints
   `matplotlib.__version__` into `index.html` ("rendered with matplotlib N"); `test_plot.py`
   pins the line (red first). `index.html` is left as committed: `plot.py` has no
   index-only path, and writing the page here would stamp it with this box's 3.10.8 while
   every PNG embeds 3.5.1 — the next collection carries the line.
6. `build-test-shadgpu.sh:393` exports `$SF40_DIR`; `bash -n` clean.
7. `an_append_under_a_different_heading_is_refused` and
   `a_capture_variable_naming_no_pass_is_refused` in `peacock_gpu_benchmarks.rs`, both
   `#[should_panic(expected = …)]`, both red first with the refusals mutated away (6 passed,
   2 failed) and green restored (8 passed, 3 `bench_` filtered). Each holds its variable in
   an `EnvLoan` guard that restores on unwind — a panic that left `PEACOCK_RECORD_PATH` at a
   temp path would send every `bench_` case after it there. Harness row 9 → 11.
8. "batch-partitioned" dropped at the six source sites and at `records.tsv:16` (the heading
   matches the new `HEADER_NOTES`; the rows are untouched); `nsys_hbm.py`'s message says
   "the node-range recorder". `residue-gate.sh`'s first section is master's five.

### The sf40 event-timing figure

Taken 2026-09-22 on shad-gpu (`llm-gpu0h200`, an H200, cuDF 25.02.02 in
`rapids-cuda-12.2`) with the 62 GB neighbour (pid 2421888) on the card throughout, so the
absolute walls are inflated and only the ratio is the instrument's. Procedure: dispatch 5's
— a copy of `test_node_timing.rs` with `SF = "40"` and `ROUNDS = 10`, built `--release`
into `rust-benchmarks/`, shipped and patched by the gate's `--push-binaries --patch`, run by
hand over ssh with `PEACOCK_RMM_POOL_BYTES=$((69<<30))` (the harness's budget; the
binary's own 2 GiB does not hold sf40 q19). The pool built: 69.0 GiB reserved of 78.4 GiB
free. tpch q19 at tp1-single, 12 regions, second-smallest of ten each:

    off    wall = 1308313 µs
    events wall = 1293402 µs   (−1.1 %)   host = 1288405   device = 1293102

Events mode read faster than off by 1.1 %, inside the run-to-run spread — the sentence for
`build-test.md` says "within about 1 %" beside the neighbour rather than a saving. The copy
is deleted here and on the host (`rust-benchmarks/` holds `peacock_gpu_benchmarks` alone).

### What was proven, and how

rust-only: `--lib` 598 (+1, the cell-order pin; 0 warnings), `test_corpus_goldens` 26 across
`test_corpus_goldens.rs` and `test_corpus_goldens/benchmark.rs` (0 ignored),
`test_module_layout` 17, `test_ci_coverage` 9. Python: `test_calls.py` 5, `test_hbm.py` 4,
`test_plot.py` 3 (rapids-26.02's python, matplotlib 3.10.8). `bash -n` on
`build-test-shadgpu.sh`. cuDF-shape build exit 0, five binaries staged, 0 warnings. shad-gpu
gate exit 0, "GPU test run OK": C++ 15 / 4 / 53 / 4 / 4; rust `peacock_gpu_benchmarks` 8
(3 `bench_` filtered), `peacockdb_core_gpu_lib -- gpu_tests::` 535, `test_gpu_corpus` 28,
`test_node_timing` 1 (sf1: off 446 ms, events +0.1 %). `residue-gate.sh`: first section
master's five lines, bp gates empty. The byte-identical set diffs empty against
`origin/master`. `test_corpus_goldens/` is a new untracked directory for the coordinator to
`git add`.

## Done — 2026-09-22

CI run 35682260859 on `4379ecd4` is green on every job (both dataset-matrix legs, the 25.02
build, the GPU job on shad-gpu, cost-report, the S3 check). PR #139 targets master with ten
commits: the squashed branch rebased onto `9c781e12`, the six triaged findings, the completeness
pass's code findings, and the wiki and board commits between them. The task is `done`; the merge
is the human's. For the merge: the branch squashes cleanly, `pre-rebase-bp-benchmarks-2` holds
the unsquashed history and `bp-benchmarks-v1` the first version, both local tags/branches the
helper can drop afterwards.
