# bp-benchmarks — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:subagent-driven-development`
> (recommended) or `superpowers:executing-plans`, task by task. Steps use `- [ ]` checkboxes.

**Goal:** the corpus benchmark of [`bp-benchmarks.md`](bp-benchmarks.md) — tree, record,
captures, panels — on master's component layout, with the first version's instrument cut down
to what measured something.

**Architecture:** C++ opens one timed region per output partition at every per-call ABI entry
point and hands the regions back through one new ABI symbol; the Rust GPU backend journals
each ABI call it makes and prices what went in and came out; the driver joins the two on
`(seq, call_index)`; a test binary drives the corpus at sf40 and writes the tree and the
record; shell and Python turn two Nsight captures into two TSVs and the panels.

**Tech stack:** C++17/cuDF/CUDA events/NVTX3, Rust 2024 (`inventory`, `paste`, `tokio`),
bash, Python 3 (`sqlite3`, `matplotlib`), GitHub Actions, shad-gpu.

**Spec:** `llm-wiki/tasks/bp-benchmarks.md`. **Reference:** the first version is
`origin/ENS-bp-benchmarks` (merge base `c42b601`); every "port from" below means
`git show origin/ENS-bp-benchmarks:<path>`, then apply the deltas the step names. Never
`git checkout` a file from it whole: the paths, the names and half the content have moved.

## Global constraints

- Read `llm-wiki/coding-style.md` before the first edit. Comment caps: 4 lines in a body,
  10 above a declaration or at a file top. No capitals for emphasis anywhere. No `pub use`,
  no `pub mod` outside `lib.rs` and the registered exceptions, API in `mod.rs`.
- No `bp` in any identifier, label, file name or test name outside `llm-wiki/`:
  `scripts/residue-gate.sh` prints nothing under `== bp gates`.
- Byte-identical to master: `cpp/src/operators/**`, `cpp/src/expr.cpp`, `Cargo.toml`,
  `peacockdb-core/build.rs`, `.gitignore`, `flatbuffers/`, `peacockdb-core/src/{plan,planner,wire}/`.
- Builds go to `/build/peacock`, never `./target`: cuDF shape through
  `scripts/docker-build.sh --no-image --cache-dir /build/peacock -- ./scripts/build-test-shadgpu.sh --build`;
  rust-only through `CARGO_TARGET_DIR=/build/peacock/rust-only-target cargo test -p peacockdb-core --features rust-only --test <target>`;
  C++ CPU tier through `ctest --test-dir cpp/build26 -L cpu` after the build above.
- Every foreground build/test/ssh command under `timeout`.
- GPU targets run on shad-gpu only: `./scripts/build-test-shadgpu.sh --push-binaries --patch --run`,
  `PCK_TEST_FILTER=<substring>` to narrow.
- Commit per task, message ≤ 10 lines; no data files until Task 11.

---

### Task 1: the C++ instrument — regions, modes, NVTX, collection

**Files:**
- Modify: `cpp/src/plan_executor.h` (the `NodeStats`, `set_node_timing`, `NodeRegion`,
  `NodeSession::collect_node_regions` declarations)
- Modify: `cpp/src/node_session.cpp` (the "Per-node timing" section and every `ScopedNodeTimer`
  site in `execute_node`, `execute_scan_rowgroups`, `slice_handle`, `materialize`/`table_for` path
  that `peacock_result_from_handle` calls)
- Modify: `cpp/tests/gpu/test_plan_executor.cpp` (new `NodeRegions` suite),
  `cpp/tests/gpu/test_cudf.cpp` (delete `NodeTiming.FloorRestoresTheSwitch`,
  `NodeTiming.FloorClampsSampleCount`), `cpp/tests/cpu/test_executor.cpp` (`NodeTiming.*`)

**Interfaces — produces:**

```cpp
// plan_executor.h
struct NodeStats { uint64_t rows = 0; uint64_t varlen_content_bytes = 0; };   // time_us gone
enum class NodeTiming : int { Off = 0, Events = 1 };
void set_node_timing(NodeTiming mode);
NodeTiming node_timing();
bool node_timing_enabled();
void set_nvtx_ranges(bool on);
bool nvtx_ranges();
void push_harness_range(const char* name);
void pop_harness_range();
struct NodeRegion {
  uint64_t seq = 0, partition = 0, call_index = 0;
  uint64_t host_us = 0;    // steady_clock across the whole call, this partition's region
  uint64_t device_us = 0;  // cudaEventElapsedTime between the region's two events
};
// NodeSession
std::vector<NodeRegion> collect_node_regions();   // drains; throws on any CUDA error
size_t recorded_regions() const;                  // count without draining
```

- [ ] **Step 1: port the timing section, then cut it.** Port `node_session.cpp`'s
  `RegionSink`, `RegionSlot`, `ScopedNodeTimer`, `OptionalRange`, `harness_range` and the
  NVTX domain from the reference. Delete: `logical_size_from_table`, `call_outcome`,
  `record_outcome`, `CallOutcome`, `mark_device_start`, `t_open_region`, `t_open_timer`,
  the `PEACOCK_LOG_LOGICAL_BYTES` block, `NodeRegion::{rows, logical_bytes,
  host_setup_us, host_submit_us}`. `ScopedNodeTimer`'s constructor records the start event
  right after `t0_`; `stop()` records the stop event and sets `out.host_us = us_since(t0_, now)`.
  A failed `cudaEventCreateWithFlags` or `cudaEventRecord` throws
  `std::runtime_error("node timing: " + cudaGetErrorString(err))` — no host-only fallback.
- [ ] **Step 2: every entry point opens a region.** `execute_node` (all four arms, as the
  reference has them), `execute_scan_rowgroups`, `slice_handle`, and the export path behind
  `peacock_result_from_handle` (find the function `table_for`'s caller in `gpu_executor.cpp`
  exports through; open the region around the IPC export in `NodeSession`, partition 0,
  `call_index` from the sink like the others). Each site: `ScopedNodeTimer timer(sink, seq,
  p, call_index); … timer.stop();` and `out_stats[p] = NodeStats{rows, varlen}` from the
  table view as master does today.
- [ ] **Step 3: collection.** `collect_node_regions()`: for every slot
  `cudaEventSynchronize(stop)` then `cudaEventElapsedTime`; any failure throws with the CUDA
  string, after destroying every event; success destroys them and clears the deque.
  `recorded_regions()` returns `sink ? sink->slots.size() : 0`.
- [ ] **Step 4: gtests, red first.** In `test_plan_executor.cpp`, a `NodeRegions` suite over
  `tpch.minimal` using the existing hand-built plan helpers:

```cpp
TEST(NodeRegions, OffRecordsNothing) {
  peacock::set_node_timing(peacock::NodeTiming::Off);
  /* begin plan, execute one node, materialize */
  EXPECT_EQ(session.recorded_regions(), 0u);
  EXPECT_TRUE(session.collect_node_regions().empty());
}
TEST(NodeRegions, EveryCallOpensOnePerPartition) {
  peacock::set_node_timing(peacock::NodeTiming::Events);
  /* execute a scan (1 partition) and a hash scatter to 4 */
  auto regions = session.collect_node_regions();
  ASSERT_EQ(regions.size(), 1u + 4u);
  EXPECT_EQ(regions[0].call_index, 0u);
  for (auto& r : regions) EXPECT_GT(r.host_us, 0u);
  peacock::set_node_timing(peacock::NodeTiming::Off);
}
TEST(NodeRegions, ASecondCallOfTheSameSeqCountsUp) { /* two execute_node on one seq → call_index 0, 1 */ }
TEST(NodeRegions, CollectingTwiceReportsNothingTheSecondTime) { /* collect, collect → empty */ }
TEST(NodeRegions, TheExportOpensARegionToo) { /* result_from_handle at the root → one more region */ }
```

  In `test_executor.cpp`, `NodeTiming.SwitchRoundTrips` uses the enum and `node_timing()`.
  Delete the two floor tests in `test_cudf.cpp`.
- [ ] **Step 5: build and run.** `ctest --test-dir cpp/build26 -L cpu` locally; the GPU suite
  on shad-gpu: `./scripts/build-test-shadgpu.sh --push-binaries --patch --run` with
  `PCK_TEST_FILTER=NodeRegions` — expect the five green.
- [ ] **Step 6: commit** — `regions: one per output partition at every entry point`.

### Task 2: the ABI and the FFI declarations

**Files:**
- Modify: `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp`, `peacockdb-ffi/src/lib.rs`

**Interfaces — produces:**

```c
typedef struct PeacockNodeStats { uint64_t rows; uint64_t varlen_content_bytes; } PeacockNodeStats;
enum { PEACOCK_NODE_TIMING_OFF = 0, PEACOCK_NODE_TIMING_EVENTS = 1 };
int  peacock_set_node_timing(int mode);            /* non-zero for a mode not named above */
void peacock_set_nvtx_ranges(int on);
void peacock_nvtx_push_range(const char* name);
void peacock_nvtx_pop_range(void);
typedef struct PeacockNodeRegion {
  uint64_t seq, partition, call_index, host_us, device_us;
} PeacockNodeRegion;
/* out == NULL && cap == 0: count only, nothing drained. cap < recorded: non-zero, nothing
   drained. Otherwise copies, releases the events, returns 0. */
int peacock_executor_collect_node_regions(peacock_executor_t*, PeacockNodeRegion* out,
                                          uint64_t cap, uint64_t* out_count);
```

- [ ] **Step 1: header.** Port the reference's additions; delete `peacock_measure_timing_floor_us`
  and `PeacockNodeStats::time_us`; the region struct is the five fields above; one doc block
  per declaration, ≤ 10 lines, no capitals for emphasis.
- [ ] **Step 2: `gpu_executor.cpp`.** `peacock_set_node_timing`: `switch` over the two named
  values, `default: return 1`. `peacock_executor_collect_node_regions`: `if (!out && cap == 0)
  { *out_count = session->recorded_regions(); return 0; }`; `if (session->recorded_regions() >
  cap) { *out_count = …; last_error = "collect_node_regions: buffer holds N of M"; return 1; }`;
  else collect, `memcpy`, return 0, `catch` → `last_error`, return 1. Keep the `PCK_SAME_OFFSET`
  asserts for the five fields and `sizeof` equality.
- [ ] **Step 3: `lib.rs`.** One doc block above `PeacockNodeRegion` (below the attributes);
  the two constants; the four externs; delete `peacock_measure_timing_floor_us`; the
  `peacock_set_node_timing` extern returns `i32`.
- [ ] **Step 4: prove.** The cuDF-shape build links (`--build`); `ctest -L cpu`; on shad-gpu
  `PCK_TEST_FILTER=NodeRegions` still green.
- [ ] **Step 5: commit** — `abi: regions come back through collect_node_regions`.

### Task 3: `executor::instrument`, `CallSite`, `collect_regions`, `Session`

**Files:**
- Create: `peacockdb-core/src/executor/instrument.rs`
- Modify: `peacockdb-core/src/executor/mod.rs`, `peacockdb-core/src/executor/gpu_backend/mod.rs`
- Create: `peacockdb-core/tests/common/gpu_session.rs`; Modify: `tests/common/corpus_gpu.rs`,
  `tests/common/mod.rs`, `tests/test_gpu_executors.rs` and `tests/test_gpu_executors/*.rs`
  (constructor call sites)

**Interfaces — produces:**

```rust
// executor/mod.rs (cfg(not(feature = "rust-only")) on every item of this block)
pub enum NodeTiming { Off, Events }
pub enum RmmPool { Pool { integrated: bool, free_bytes: u64, initial_bytes: u64, maximum_bytes: u64 }, Unavailable }
impl std::fmt::Display for RmmPool { /* the allocator= line, as the reference */ }
pub fn install_rmm_pool() -> RmmPool;
pub fn set_node_timing(mode: NodeTiming);      // panics if the C side refuses the mode
pub fn node_timing_on() -> bool;
pub fn set_nvtx_ranges(on: bool);
pub fn nvtx_range(name: &str) -> NvtxRange;    // #[must_use], pops on drop
pub struct NvtxRange(());
// executor/gpu_backend/mod.rs
#[derive(Clone, Copy)] pub struct CallSite { pub executor: *mut PeacockExecutor, pub node: usize, pub lane: usize }
pub fn recorded_regions(executor: *mut PeacockExecutor) -> Result<usize, BackendError>;
pub fn collect_regions(executor: *mut PeacockExecutor) -> Result<Vec<Region>, BackendError>; // asks the count, allocates, drains
```

- [ ] **Step 1: `instrument.rs`** as a private `mod instrument;` of `executor`, `pub(crate)`
  functions and the `MEASURING` atomic, ported from the reference's
  `src/batch_partitioned/instrument.rs` with `NodeTiming`/`RmmPool` declared in
  `executor/mod.rs` and the free functions there delegating (`pub fn set_node_timing(mode) {
  instrument::set_node_timing(mode) }`). `set_node_timing` asserts the FFI returned 0.
- [ ] **Step 2: `CallSite`** in `gpu_backend/mod.rs`, doc: "the session a call goes through
  and the node and lane it belongs to, so a failure names them". Thread it through
  `GpuExec`, `GpuExport`, `GpuSource`, `GpuEmitter`, `GpuJoin`, `GpuAccumulator` constructors
  and `execute_node{,_many}` as the reference does; the error text names node and lane.
- [ ] **Step 3: `collect_regions`** — `recorded_regions` calls the ABI with `(null, 0)`;
  `collect_regions` allocates exactly that many and drains. `Region` is Task 5's type; until
  then declare it here as the plain struct Task 5 moves.
- [ ] **Step 4: `tests/common/gpu_session.rs`** — lift `Session` from `corpus_gpu.rs` (the
  reference's file, minus `region_cap`): `open`, `context`, `regions(&self, what) ->
  Vec<Region>` calling `collect_regions`, `Drop` ends the plan. `corpus_gpu.rs` imports it.
- [ ] **Step 5: `test_gpu_executors`** call sites take `CallSite { executor, node: 0, lane: 0 }`
  (a `site()` helper on the test's session, as the reference).
- [ ] **Step 6: prove.** rust-only `--test test_cpu_executors` (unchanged) and the cuDF build
  of `test_gpu_executors`; shad-gpu `PCK_TEST_FILTER=test_gpu_executors` green.
- [ ] **Step 7: commit** — `instrument: the switches and the pool, behind the executor facade`.

### Task 4: the ABI-call journal

**Files:**
- Modify: `executor/mod.rs` (`AbiCall`, `AbiCalls`, `CallStats`, `RunReport::abi_calls`),
  `executor/driver/{partitioned,single_partition,mock,accounting}.rs`,
  `executor/driver/accounting/tests.rs`, `executor/cpu_backend/mod.rs`,
  `executor/gpu_backend/{mod,accumulate,emit,join,source}.rs`
- Test: `executor/driver/tests/counts.rs`

**Interfaces — produces:**

```rust
pub struct AbiCall { pub seq: Seq, pub kind: FbKind, pub call_index: u64,
                     pub in_rows: u64, pub in_bytes: u64, pub out_rows: u64, pub out_bytes: u64 }
pub struct AbiCalls(Option<Box<Vec<AbiCall>>>);      // None = nobody was measuring
impl AbiCalls { pub fn armed(measuring: bool) -> Self; pub fn record(&mut self, call: AbiCall);
                pub fn recorded(&self) -> Option<&[AbiCall]>; pub fn recorded_mut(&mut self) -> Option<&mut [AbiCall]>; }
pub struct CallStats { pub scratch_bytes: Option<usize>, pub calls: AbiCalls }   // Clone, not Copy
// RunReport gains: pub abi_calls: Vec<Vec<Vec<AbiCalls>>>   // node → driving lane → calls in order
```

- [ ] **Step 1: red.** In `driver/tests/counts.rs`, port
  `a_lane_records_its_backend_calls_and_not_the_drivers_own` and
  `a_backend_that_names_no_seq_leaves_every_entry_unmeasured` from the reference; not
  `the_call_record_is_indexed_by_the_driving_lanes_the_report_names` — assert instead that
  `report.abi_calls[node].len()` equals the index's `ready_lanes` for a scatter and a
  cross-lane merge (port the second test's plan, compare against `PlanIndex`). Run rust-only
  `--lib driver::tests::counts` — compile error is the red.
- [ ] **Step 2: types** in `executor/mod.rs` as above; `accounting.rs` takes `&CallStats`.
  `CpuBackend` and the mock fill `calls: AbiCalls::default()`.
- [ ] **Step 3: the driver** stamps `call_index` per seq in `record_calls` (port from the
  reference's `partitioned.rs`) and `LaneOutcome::calls: Option<AbiCalls>` in
  `single_partition.rs` — port those hunks whole. Delete `driving_lanes`.
- [ ] **Step 4: the GPU backend journals and prices every call.** Port the reference's
  `calls.record(...)` sites; replace the `measure: bool` / `is_armed().then(..)` plumbing
  with unconditional pricing: `Consumed::of(&batch)` before `consume()`, always. Fill
  `out_rows`/`out_bytes` from the call's own `PeacockNodeStats` and the schema the executor
  holds: `logical_size_from_schema(schema, rows, varlen)` — the same call `produced()` makes.
  Two sites hold the wrong schema for a middle call and get the right one:
  `GpuExec` for an aggregate with a finalize takes `intermediate` beside `schema`
  (`backend.rs` passes `aggregate.intermediate()` for the `Aggregate` arm, `None` otherwise);
  `AggregateBatches::compact` prices the concat with `self.held` (the state schema). For a
  scatter, `out_*` sums the N partitions' stats.
- [ ] **Step 5: green.** rust-only `--lib` (driver tests), then the cuDF build; shad-gpu
  `test_gpu_executors` + `test_gpu_corpus` green.
- [ ] **Step 6: commit** — `journal: what each ABI call was handed and answered with`.

### Task 5: measurements and the timing tree

**Files:**
- Create: `executor/driver/measurements.rs`, `plan_text/bench_text.rs`
- Modify: `executor/mod.rs`, `executor/driver/mod.rs`, `plan_text/mod.rs`
- Test: `executor/driver/tests/render.rs`

**Interfaces — produces:**

```rust
// executor/mod.rs
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Measured { pub host_us: u64, pub device_us: u64, pub out_rows: u64, pub out_bytes: u64, pub regions: usize }
impl std::ops::AddAssign for Measured { /* field for field */ }
#[derive(Debug, Clone, Copy)]
pub struct Region { pub seq: Seq, pub partition: usize, pub call_index: u64, pub host_us: u64, pub device_us: u64 }
pub struct Measurements { /* private: per_entry, per_call */ }
impl Measurements { pub fn lanes(&self, node: usize) -> &[Vec<Option<Measured>>]; pub fn nodes(&self) -> usize;
                    pub fn call(&self, seq: Seq, call_index: u64) -> Option<Measured>; }
pub fn join_regions(report: &RunReport, regions: &[Region]) -> Result<Measurements, String>; // refuses either direction
pub fn node_measured(times: &Measurements, node: usize) -> Option<Measured>;
pub fn nodes_as_recorded(root: &dyn GpuNode) -> Result<Vec<(&'static str, usize)>, PlanError>;
// plan_text/mod.rs
pub fn render_timings(root: &dyn GpuNode, times: &Measurements) -> String;
```

  `Measured::out_rows/out_bytes` are copied from the `AbiCall` (Rust's price), never from
  the region. `join_regions` returns `Err` naming the first region no call claimed **or** the
  first call no region answered — every call opens one now, so an absence is a failed run.
- [ ] **Step 1: red.** In `driver/tests/render.rs` port the reference's tests over the mock
  (`render_timings` over a report with hand-built regions), plus:
  `a_call_without_a_region_is_refused` and `a_region_without_a_call_is_refused`, both asserting
  `join_regions(..).is_err()` with the message naming `(seq, call_index)`.
- [ ] **Step 2: implement** — port `measurements.rs` and `bench_text.rs`, drop
  `Measured::host_us()`, `Measurements::entry`, `per_entry` stays private. `bench_text.rs`
  keeps `call_us`: `Some(t) if t.regions > 0 => t.device_us.max(1)`, `_ => 0`.
- [ ] **Step 3: green** rust-only `--lib`; **commit** — `measurements: the two halves meet on (seq, call_index)`.

### Task 6: the harness, the record, the case list

**Files:**
- Create: `tests/common/record.rs`, `tests/common/corpus_benchmark.rs`,
  `tests/common/corpus_benchmark_cases.inc`, `tests/peacock_gpu_benchmarks.rs`
- Modify: `tests/common/mod.rs`, `tests/test_corpus_goldens.rs`, `tests/test_plan_goldens.rs`

**Interfaces — produces:**

```rust
// tests/common/record.rs (compiled in every shape)
pub const RECORD_PATH_ENV: &str = "PEACOCK_RECORD_PATH";
pub const CAPTURE_ENV: &str = "PEACOCK_BENCHMARK_CAPTURE";
pub enum Capture { None, Trace, Metrics }        // from_env(): unset → None, other → panic naming the two
pub const COLUMNS: &[&str] = &["dataset","sf","query","mode","node_seq","node_type","lane","recipe_seq",
  "recipe_kind","call_index","run_index","in_rows","in_bytes","out_rows","out_bytes","host_us","device_us"];
pub struct RunMeta<'a> { pub dataset: &'a str, pub sf: &'a str, pub query: &'a str, pub mode: &'a str,
                         pub allocator: &'a str, pub capture: Capture }
pub fn record_rows(nodes: &[(&str, usize)], report: &RunReport, measured: &Measurements, meta: &RunMeta, run_index: usize) -> Vec<String>;
pub fn record_header(meta: &RunMeta) -> String;      // HEADER_NOTES + "# run: timing_mode=events" + build=release + allocator= + capture=
pub fn declared_steps(recipes: &RecipePlan) -> BTreeMap<usize, BTreeSet<Seq>>;
pub fn rows_match_the_recipes(rows: &[String], declared: &BTreeMap<usize, BTreeSet<Seq>>) -> Result<(), String>; // also: every row has COLUMNS.len() fields
pub fn append_records(rows: &[String], meta: &RunMeta);
// tests/common/corpus_benchmark.rs (cfg(not(feature = "rust-only")))
pub struct BenchmarkCase { pub dataset: &'static str, pub sf: &'static str, pub query: &'static str, pub mode: &'static str }
pub const NOT_TIMED: &str = "none";
pub fn declared_for(dataset: &str, sf: &str, mode: &str) -> Vec<(String, Option<String>)>;
pub fn results_file(dataset: &str, sf: &str, mode: &str) -> PathBuf;   // <mode>.benchmark.txt, no prefix strip
pub async fn benchmark_case(dataset: &str, sf: &str, query: &str, mode: &str);
```

- [ ] **Step 1: `record.rs`** — port; 17 columns; no `Option` in a row: `in_bytes` of a
  middle call is the call before it, taken from the journal; `capture=` in the heading;
  `rows_match_the_recipes` first checks `row.split('\t').count() == COLUMNS.len()`. Rewrite
  `HEADER_NOTES` for the new columns, no capitals.
- [ ] **Step 2: `corpus_benchmark.rs`** — port; `mode_named` from `common/mode.rs`; no
  `file_stem_of`; `Capture::from_env()` decides `set_nvtx_ranges(true)` and skipping
  `write_section`; the trailer is `run_us`, `device_us`, `runs=[…]`, `build=release`,
  `allocator=`; `assert!(!cfg!(debug_assertions))` and the pool assert stay in `run_once`;
  `measured_of` panics on `join_regions`'s `Err`. Case names expand to
  `bench_<dataset>_sf<sf>_<query>_<mode>`.
- [ ] **Step 3: cases** — `corpus_query_benchmark!(tpch, 40, q6, tp1_single | tp4_sized);`
  and `corpus_query_benchmark!(tpch, 40, q19, tp1_single);` with a ≤ 10-line file comment.
- [ ] **Step 4: `peacock_gpu_benchmarks.rs`** — port the macro and the six assertions
  (`every_declared_mode_names_the_queries_its_file_will_hold`,
  `the_sections_of_a_file_are_ordered_numerically`, `a_modes_results_go_to_one_file_per_dataset_and_mode`,
  `a_filtered_run_keeps_the_sections_it_did_not_produce`, `the_record_is_written_only_when_a_path_is_named`
  — its fake row has 17 fields — and `the_record_is_checked_against_what_the_plan_declares`);
  every `bp_` in a name or a label goes.
- [ ] **Step 5: rust-only tests.** In `test_corpus_goldens.rs`: port
  `every_total_us_is_the_sum_of_the_time_us_beside_it` without the empty-tree escape and
  extend it to the trailer (`device_us == Σ total_us`, `runs` has ten entries, `build=release`);
  add `the_records_preamble_is_what_record_header_writes` (the committed `records.tsv`'s `#`
  lines minus `# run:` equal `record_header`'s); add `every_timed_case_is_enabled_on_a_device`
  reading both `.inc` files as text (`corpus_query_benchmark!(d, sf, q, m1 | m2)` ⊆ the gpu
  column of `corpus_query!(d, q, …)`). Delete `the_benchmark_path_reads_no_cpu_side_golden`.
  In `test_plan_goldens.rs`: port `a_rows_node_seq_names_the_steps_its_recipes_line_prints`.
  These skip when `testdata/benchmark-results/` or `records.tsv` is absent only until Task 11
  commits them; after that the absence is a failure — write it that way now with a comment.
- [ ] **Step 6: prove.** rust-only `--test test_corpus_goldens --test test_plan_goldens`
  (the preamble test red until Task 11 — mark it `#[ignore]` with the Task 11 note, remove
  the ignore there); cuDF build of `peacock_gpu_benchmarks`; on shad-gpu run it with
  `--skip bench_` by hand (the script flag comes in Task 8): six green.
- [ ] **Step 7: commit** — `harness: the corpus timed at sf40, one file per mode`.

### Task 7: `test_node_timing`

**Files:** Create `tests/test_node_timing.rs`.

- [ ] **Step 1** port the reference; delete `EVENTS_GROSS_LIMIT`, check 2 and check 3c; the
  region count check compares `regions.len()` against Σ over the journal of (calls × their
  partitions) — at `tp1_single` that is `report.abi_calls` flattened. Keep `eprintln!` of
  both walls. Runs q19 at sf1, `tp1_single`, `ROUNDS = 7`.
- [ ] **Step 2** shad-gpu `PCK_TEST_FILTER=events_are` green; **commit** —
  `node timing: the instrument's structural test`.

### Task 8: scripts and CI

**Files:**
- Modify: `scripts/lib/shadgpu-env.sh`, `scripts/build-test-shadgpu.sh`, `scripts/build-test.sh`,
  `.github/workflows/pipeline.yml`, `peacockdb-core/tests/test_ci_coverage.rs`,
  `peacockdb-core/tests/test_module_layout.rs`

- [ ] **Step 1: `shadgpu-env.sh`** gains, once: `die()`, `pull_one <rel> <what>` (returns 1 on
  a missing file), `PATCHED_LD` (the glibc-2.35 loader path), `SF40_DIR`, and
  `passed_count <log>` (the "N passed" sum). No ControlMaster, no rc=23 branch, no cargo
  diagnostics forwarding — master's `resilient_rsync` and `stage_cargo_test_binary` stay.
- [ ] **Step 2: `build-test-shadgpu.sh`** — port the benchmark phases (`--build-benchmarks`,
  `--run-benchmarks`, `--run-benchmarks-detached`, `--benchmark-status`, `--pull-benchmarks`)
  and the validation lines. Deltas: `--build-benchmarks` = `stage_cargo_test_binary
  "$BENCH_TARGET" "$BENCH_STAGING" --release`; validation refuses `--run-benchmarks` when
  `$BENCH_STAGING/$BENCH_TARGET` is not executable locally, before any push; the remote
  gate loop passes `--skip bench_` when `$tname = peacock_gpu_benchmarks`; the remote bench
  script exports `PEACOCK_RECORD_PATH` only (no `PEACOCK_NVTX`, no `_RESULTS_RO`);
  `--pull-benchmarks` dies when the run died or when no `.benchmark.txt` and no record came
  home, and has no `*.*.benchmark.txt` block; the fixture push is master's hand list, unchanged.
  `RUST_TESTS` gains `test_node_timing` and `peacock_gpu_benchmarks`. Header comment ≤ 10
  lines, usage block as the reference minus the stale filter example.
- [ ] **Step 3: `build-test.sh`** — GPUSET heredoc gains `peacockdb-core:test_node_timing`
  and `peacockdb-core:peacock_gpu_benchmarks`.
- [ ] **Step 4: `pipeline.yml`** — the staging `for t in …` line gains the two targets (one
  line, the guard reads it with one `find`); the gpu-tests run loop passes `--skip bench_` to
  `peacock_gpu_benchmarks` the same way; the cost-report job gains a step after the exec-model
  one: `for f in scripts/calibration/tests/test_*.py; do python3 "$f" || status=1; done`
  with the empty-glob guard copied from the exec-model step. Verify with `python3 -c
  'import yaml,sys; yaml.safe_load(open(".github/workflows/pipeline.yml"))'` and `bash -n`
  on the rendered `run:` blocks.
- [ ] **Step 5: `test_ci_coverage.rs`** — `INTENTIONALLY_NOT_IN_CI` gains
  `("test_node_timing", Exemption::GpuJob)` and `("peacock_gpu_benchmarks", Exemption::GpuJob)`;
  a new case `the_benchmark_binary_runs_without_its_cases_on_ci` asserting both runner loops
  carry `--skip bench_` for that target. **`test_module_layout.rs`**: register in
  `PUB_MODULES` whatever the new test crates force (`executor/gpu_backend` is already
  there; the harness reaches only the `executor` facade and `plan_text` otherwise — if a run
  of `--test test_module_layout` names more, register it with `forced_by`).
- [ ] **Step 6: prove.** rust-only `--test test_ci_coverage --test test_module_layout`;
  `bash -n` both scripts; a full `./scripts/build-test-shadgpu.sh --push-binaries --patch --run`
  green with the two new binaries in its output; then `--build-benchmarks --push-binaries
  --patch --run-benchmarks --pull-benchmarks` producing two trees and a record locally
  (not committed yet).
- [ ] **Step 7: commit** — `ci: the harness runs its own assertions on every job`.

### Task 9: captures, the Python, its tests

**Files:**
- Create: `scripts/create_nsys_profile.sh`, `scripts/calibration/{record,nvtx_names,nsys_calls,nsys_hbm,plot}.py`,
  `scripts/calibration/tests/{harness.py,test_calls.py,test_hbm.py,test_plot.py}`
- Modify: `testdata/.gitignore`

- [ ] **Step 1: `record.py`** — one reader: `read_record(path) -> (run: dict, rows: list[dict])`
  raising on a missing column or a second `# run:` block that differs; `nsys_calls.py`,
  `nsys_hbm.py` and `plot.py` import it (plain module names, no leading underscore).
- [ ] **Step 2: `nsys_calls.py`** — port; the region identity is `(case, node_seq, call, depth)`
  where `case` is the enclosing harness range's text; the output gains `dataset sf query mode`
  columns; the "regions disagree" check and the "mode with no golden" path `sys.exit(1)`;
  one `--plans-dir`; docstring ≤ 40 lines naming `PEACOCK_BENCHMARK_CAPTURE=trace`.
- [ ] **Step 3: `nsys_hbm.py`** — port; `--peak-bw` required; reads the clean record through
  `record.py`; `nvtx_names.py` is the one place the domain id is looked up.
- [ ] **Step 4: `plot.py`** — port; `require()` lists every column each panel reads
  (`in_bytes`, `host_us`, `device_us`, `out_bytes`, `out_rows` …); the `query/` panel draws
  `host_us` and `device_us`; refuses two records with different `# run:` lines; docstring
  states the matplotlib version.
- [ ] **Step 5: tests.** `scripts/calibration/tests/harness.py` copied from
  `scripts/exec_model/tests/harness.py`; `test_calls.py` builds a two-case sqlite capture in a
  temp dir (the NVTX and CUPTI tables `nsys_calls.py` queries, minimal rows) and asserts the
  output has one row set per case; `test_hbm.py` joins a three-row record with a capture and
  asserts `hbm_bytes` lands on the right tuple and a missing `--peak-bw` exits non-zero;
  `test_plot.py` runs `plot.py` on a ten-row record into a temp dir and asserts the six
  directories and `index.html` exist. Run each with `python3 <file>`.
- [ ] **Step 6: `create_nsys_profile.sh`** — port; `--trace | --metrics`, both by default;
  exports `PEACOCK_BENCHMARK_CAPTURE=trace|metrics` and `PEACOCK_RECORD_PATH`; the metrics
  record stays on the host; passes `--peak-bw "$HBM_PEAK_BW"` (a knob beside `HBM_SET`);
  uses `die`, `pull_one`, `PATCHED_LD`, `passed_count` from `shadgpu-env.sh`; `PCK_TEST_FILTER`
  is its filter. `bash -n`, then a real `--trace` on shad-gpu after Task 8's push.
- [ ] **Step 7: `testdata/.gitignore`** — the deny-by-default block with a ≤ 10-line comment;
  re-include `records.tsv`, `calls.tsv`, `hbm.tsv`, `plots/`, `plots/**`; not `records-hbm.tsv`.
- [ ] **Step 8: commit** — `captures: two passes, two tsvs, one reader`.

### Task 10: wiki and tickets

**Files:** `llm-wiki/build-test.md`, `llm-wiki/architecture.md`, `llm-wiki/tickets.md`,
`llm-wiki/tasks/refcounted-tables.md`.

- [ ] **Step 1: `build-test.md`.** Test table: a `Corpus benchmarks (Rust)` row (six
  assertions on shad-gpu, the cases manual), a `GPU timing method (Rust)` row, the `cuDF GPU
  smoke` row without the floor sentence and its test, `Plan-executor` +5, counts recomputed
  from `--list` (write the arithmetic in the commit message). `### Benchmark data flow` under
  Golden files, 40–60 lines, from the reference with `--metrics`, `capture=`, no
  `records-hbm.tsv` in git, `calls.tsv` keyed by case. `### Corpus benchmarks` under
  Benchmarks, 50–80 lines: the six commands, the tree format as `bench_text.rs` writes it
  (`time_us=[[…]] total_us=`, the `--- run ---` fields), the record's columns by name, the
  two variables, the second-smallest rule, and one sentence: "The run is measured under
  CUDA-event timing; on tpch q19 at sf40 the events mode's second-smallest of ten was within
  N % of the unmeasured run, measured by `test_node_timing`'s procedure by hand" — N from
  Task 11's run. No capitals for emphasis.
- [ ] **Step 2: `architecture.md`, Interfaces.** "The ABI is nineteen symbols in five groups:
  … instrumentation (`install_rmm_pool`, `set_node_timing`, `set_nvtx_ranges`,
  `nvtx_push_range`, `nvtx_pop_range`, `executor_collect_node_regions`)"; the instrumentation
  bullet: process-global, off by default, turned on by `peacock_gpu_benchmarks` and
  `test_node_timing`; events at region open and close on the default stream, no sync
  inside, the one sync a STRING output pays in either mode (`varlen_content_bytes`); the
  `NodeStats` sentence without a time; the byte-formula sentence stays true and is left alone.
- [ ] **Step 3: tickets.** `#65` gains one paragraph: `__grouping_id` is built `INT32` where
  DataFusion sizes it to the group count (`UInt8` ≤ 8 …), a width defect beside the encoding
  one. No `#197`, no `#198`, counter untouched. `refcounted-tables.md`: the two
  `nodes_at_or_below_floor` sentences become the reference's wording.
- [ ] **Step 4: commit** — `wiki: the benchmark tree and record as written`.

### Task 11: the data

- [ ] **Step 1** `scripts/docker-build.sh --no-image --cache-dir /build/peacock -- ./scripts/build-test-shadgpu.sh --build-benchmarks`;
  `./scripts/build-test-shadgpu.sh --push-binaries --patch --run-benchmarks-detached`; poll
  `--benchmark-status`; `--pull-benchmarks`.
- [ ] **Step 2** `./scripts/create_nsys_profile.sh` (both passes); then
  `python3 scripts/calibration/plot.py --record testdata/calibration/records.tsv --hbm testdata/calibration/hbm.tsv --out-dir testdata/calibration/plots`.
- [ ] **Step 3** the events figure for the wiki: on shad-gpu run `test_node_timing`'s
  procedure by hand on q19 at sf40 (a copy of the test with `SF = "40"`, not committed), read
  the two walls from its `eprintln!`, put N into the sentence Task 10 left.
- [ ] **Step 4** remove the `#[ignore]` Task 6 left; rust-only `--test test_corpus_goldens`
  green against the committed files; `scripts/residue-gate.sh` empty.
- [ ] **Step 5: commit** — `sf40: the trees, the record, both tsvs and the panels`.

---

## Self-review

- Spec coverage: tree (T5, T6), record (T4, T6), captures (T9), panels (T9), instrument
  (T1–T3), CI (T8), rust-only tests (T6), structural timing test (T7), wiki and tickets
  (T10), data (T11), residue and byte-identity constraints (global, T11 step 4).
- Type consistency: `Region {seq, partition, call_index, host_us, device_us}` in T2 (C),
  T3/T5 (Rust); `Measured {host_us, device_us, out_rows, out_bytes, regions}` in T5, read by
  T6's `record_rows`; `AbiCall` carries `out_rows/out_bytes` (T4) and `record_rows` copies
  them; `Capture` and `CAPTURE_ENV` in T6, exported by T9's script; `recorded_regions` (T1
  C++, T2 ABI as `(NULL, 0)`, T3 Rust).
- Open at plan time, decided by the developer with the detail file: how `test_module_layout`
  wants the harness's reach registered (T8 step 5), and which NVTX/CUPTI tables the synthetic
  capture of T9's tests must populate (read them off `nsys_calls.py`'s queries).
