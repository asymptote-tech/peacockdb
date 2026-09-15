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
