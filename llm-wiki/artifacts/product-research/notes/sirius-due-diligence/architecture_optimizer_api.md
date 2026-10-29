# Sirius: architecture, optimizer and API surface

Code citations are `path:line` at SHA b9e05ffc653c8ed031c789e5c1415ac1186e0619 (HEAD commit date 2026-10-28), github.com/sirius-db/sirius. Read them at `https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/<path>#L<line>`. Web sources were accessed 2026-10-28.

## Q1. What Sirius is: architecture, DuckDB hook points, hardware, versions, repo layout

### Takeaway
Sirius ("Super Sirius" engine, namespace `sirius`) is a GPU executor that runs as a DuckDB extension.

- **What it keeps from DuckDB:** the parser, binder, optimizer and catalog.
- **What it replaces:** DuckDB's optimized `LogicalOperator` tree is translated into Sirius's own physical operators and pipelines. These run on a task-based GPU scheduler built on libcudf, RMM and cuCascade. The whole DuckDB physical plan is replaced by a single `PhysicalSiriusExecution` root operator.
- **Repo layout:**
  - `src/` is the engine library.
  - `sirius-duckdb/` is a thin DuckDB-extension wrapper plus a CUDA-selecting loader.
  - `rust/` holds early cxx bindings to a small Substrait-in, Arrow-out C++ API.
  - The "legacy" engine survives only as an archived branch.

### Cited Findings

**What Sirius is**
- AGENTS.md describes Sirius as "a GPU-native SQL engine that runs as a DuckDB extension". It routes supported SQL to the GPU "via cuDF/RMM/cuCascade" and falls back to DuckDB's CPU execution otherwise. Once loaded it "transparently intercepts normal SQL". — `AGENTS.md:3-6`
- **Live engine vs legacy:** "Super Sirius is the live engine". Its source is under `src/op/` (operators), `src/planner/` (plan builders + `sirius_physical_plan_generator.cpp`), `src/pipeline/` and `src/cuda/` (GPU kernels). "All new work targets Super Sirius." Spilling and CPU fallback are handled by the downgrade executor (`src/downgrade/`, `src/creator/`). — `AGENTS.md:64-70`
- **Legacy engine:** it is not in the tree. ClickBench results came from "the legacy Sirius implementation", archived as branch `legacy-dev` pinned at `f79a6f42`. — `docs/README.md:31-32`. CONTRIBUTING says `legacy-main` and `legacy-dev` are read-only archives. — `CONTRIBUTING.md:69-70`
- **Positioning in the docs:** docs/README calls Sirius "a GPU-Native Composable Analytics Engine" that "plugs into existing databases via the standard Substrait query format". It lists DuckDB as supported and StarRocks as "coming soon", built on "cuDF, cuVS, and cuCascade". — `docs/README.md:13`

**Execution lifecycle** — `docs/super-sirius/architecture-overview.md` (lines 128-200 of that doc)
1. DuckDB parses and optimizes.
2. `sirius_physical_plan_generator::create_plan()` converts the DuckDB logical plan into a Sirius physical operator tree.
3. `sirius_engine::initialize()` builds a `sirius_meta_pipeline`:
   - splits at TABLE_SCAN, joins, aggregates and sorts;
   - turns each scan into a unified GPU scan source with a per-table `gpu_ingestible`;
   - injects PARTITION/CONCAT/MERGE operators at pipeline boundaries;
   - wires data repositories with barrier types.
4. `task_scheduler::start_query()` starts execution. GPU pipeline executors (one per GPU) pull tasks, reserve memory and execute on CUDA streams.
5. A task creator schedules downstream work based on data availability.
6. Downgrade executors spill GPU→host under memory pressure.
7. The result comes back as a `MaterializedQueryResult` to DuckDB.

**Components and threads**
- `SiriusContext` is shared per DuckDB DatabaseInstance. It owns:
  - `sirius_memory_reservation_manager` ("GPU/Host/Disk memory management via cuCascade");
  - `task_scheduler`, `task_creator` and `sirius_scan_manager`;
  - the downgrade executors.
  — `docs/super-sirius/architecture-overview.md` (lines 97-114 of the doc)
- Thread pools — same doc (lines 128-179):
  - one GPU pipeline executor per GPU device;
  - a task creator;
  - a scan manager with io_uring/REST/kvikio I/O;
  - per-memory-space downgrade executors.
- **Operators:** about 30 `sirius_physical_*` operator classes live in `src/op/`. Examples: hash join, nested loop join, grouped/ungrouped aggregate, merge sort, top-N, partition, concat, streaming source/sink, CTE, delim join, dense count join. — `src/op/` directory listing

**Mapping from DuckDB logical operators to Sirius** — `docs/super-sirius/physical-plan-generation.md:16-37`
- GET→TABLE_SCAN, PROJECTION, FILTER
- AGGREGATE→HASH_GROUP_BY/UNGROUPED_AGGREGATE, DISTINCT
- COMPARISON_JOIN→HASH_JOIN/NESTED_LOOP_JOIN, CROSS_PRODUCT, DELIM_JOIN
- ORDER_BY, TOP_N, LIMIT
- CHUNK_GET, EXPRESSION_GET, MATERIALIZED_CTE/CTE_REF, DUMMY_SCAN, EMPTY_RESULT

**cuDF usage**
- Grouped aggregation uses cuDF `groupby()`.
- AVG is decomposed into SUM + COUNT_VALID because "cuDF doesn't support AVG directly".
- HUGEINT is downcast to BIGINT because "cuDF doesn't support int128".
- These are in `docs/super-sirius/physical-plan-generation.md:61-71`. Hash join uses `cudf::hash_join`, `cudf::filtered_join` and `cudf::mark_join`. — `docs/super-sirius/optimizations.md:106,196-198`

**How it hooks into DuckDB** — all four mechanisms are standard extension APIs; no fork of DuckDB is needed:
- **(a) Optimizer extension.** The extension registers `pre_optimize_function = sirius_pre_optimizer_hook` and `optimize_function = sirius_optimizer_hook`. — `src/sirius_extension.cpp:3676-3677`
  - The pre-hook only classifies the query (decides early whether DuckDB must keep it). — `src/transparent/sirius_optimizer_extension.cpp:80-97`
  - The post-hook copies the *already optimized* DuckDB logical plan into per-connection state. — `src/transparent/sirius_optimizer_extension.cpp:124-166`
- **(b) `ClientContextState::OnFinalizePrepare`.** It runs `sirius_physical_plan_generator::create_plan()` on the captured plan. — `src/sirius_context.cpp:1757-1952`
  - On success it stashes DuckDB's CPU physical plan as `cpu_fallback`. — `src/sirius_context.cpp:1993-1997`
  - It then replaces `prepared.physical_plan` with a new `PhysicalPlan` whose root is `sirius::transparent::PhysicalSiriusExecution`. — `src/sirius_context.cpp:1999-2021`
- **(c) Explicit table function.** `gpu_execution` still exists (`CALL gpu_execution('...')`). — `src/sirius_extension.cpp:2360`; `docs/gpu_execution.md:36,50`
  - It is "no longer required". — `docs/super-sirius/README.md:5`
  - No `gpu_processing` symbol exists in `src/` (grep of `src/` found none).
- **(d) Settings.** `gpu_execution` (on by default; `SET gpu_execution=false` disables interception) and `enable_duckdb_fallback` (default true). — `src/sirius_extension.cpp:3431,3031`; `src/sirius_context.cpp:1713-1720`; `AGENTS.md:77-84`
- **Effect on DuckDB settings and optimizer rules:**
  - Loading Sirius changes DuckDB's default `delim_join_as_cte` from true to false, because Sirius plans DuckDB v2's CTE-decorrelated shapes poorly. — `docs/super-sirius/configuration.md:737-745`
  - It also publishes a DB-global disabled-optimizer mask at load. — `src/sirius_extension.cpp:3582-3595,3691`

**Hardware and OS requirements** — `docs/README.md:34-39`
- Linux amd64/arm64 with glibc ≥ 2.28.
- NVIDIA Turing or newer (compute capability 7.5+).
- CUDA 13.x (driver ≥ 580.65.06) or CUDA 12.x (driver ≥ 525.60.13).
- `io_uring` enabled at runtime.
- Local Parquet files on a filesystem that supports O_DIRECT.
- Jetson/Tegra is not supported. — `sirius-duckdb/README.md:105`

**Build targets and pinned dependencies** — `pixi.toml`
- Compiled for CUDA archs `75-real;80-real;86-real;89-real;90a-real;100f-real;120a-real;120` on CUDA 13. — `pixi.toml:105`
- CUDA 12 builds need toolkit ≥ 12.8 for Blackwell. — `pixi.toml:117-118`
- `libcudf ==26.10.02` and `libcuvs ==26.10.00` for both CUDA 12 and 13. — `pixi.toml:100-101,113-114`
- CUDA 13 env pins `cuda-version = 13.4.*`. — `pixi.toml:99`

**DuckDB pin**
- Git submodules `duckdb` and `sirius-duckdb/duckdb` both point to duckdb/duckdb branch `v2.0-cyanoptera`, at SHA `d54cbbe3df17e8e5ce68312f483b4155836112aa`. — `.gitmodules`; `git submodule status`
- The extension "requires the exact DuckDB revision pinned in this repository". The DuckDB 1.5 Python package cannot load it. — `docs/README.md:106-108`
- The Python fixture client is pinned to `duckdb = "=2.0.0"`. — `pixi.toml:59`

**Releases**
- There are no semver releases. CMake declares `VERSION 0.0.0`. — `CMakeLists.txt:21`
- Two rolling tags exist. `latest` tracks the newest successful `main` build (commit 5350b8f, DuckDB 2.0.0). `stable` is maintainer-promoted (commit 8d17afe, DuckDB 1.5.6). Both were published 2 Oct. Each ships amd64/arm64 × CUDA 12/13 `.duckdb_extension.gz` assets. — [GitHub releases](https://github.com/sirius-db/sirius/releases) (accessed 2026-10-28)

**Repo structure: `src/` vs `sirius-duckdb/` vs `rust/`**
- **`src/`** is the engine, built as library objects (`add_library(sirius_objects OBJECT)`). Shared library is the default; a static library is optional. — `CMakeLists.txt:25-26,49`
- **`sirius-duckdb/`** is a "DuckDB extension wrapper for an installed Sirius CMake package". It only does `find_package(sirius)` and "never builds the engine itself". — `sirius-duckdb/README.md:3-4,75-78`
  - It also contains a CUDA-free `sirius` loader. The loader picks the `sirius_cuda12` or `sirius_cuda13` backend by `cuDriverGetVersion`, for DuckDB core-extension distribution. — `sirius-duckdb/README.md:88-118`
  - The wrapper "stays here until it can move to `sirius-db/sirius-duckdb`". — `sirius-duckdb/README.md:3-4`
- **`rust/`** holds `sirius-sys` (cxx bindings to `include/sirius/ffi.hpp`) and `sirius` (safe wrapper), tracking issue #835. `telemetry/*` crates are unrelated (Rust linked into the C++ extension). — `rust/README.md:1-12`

**Experimental distributed hosts**
- `experimental/doris` is a Rust process that joins an unmodified Apache Doris 4.1.4 FE as a backend. It translates `TPipelineFragmentParams` → Substrait → the Sirius engine. — `experimental/doris/README.md:1-20`
- `experimental/starrocks` (crate `sirius-starrocks-cn`) depends on `rust/crates/sirius`. — `experimental/starrocks/Cargo.toml:2,61`
- The Doris README says "the shared libSirius Rust boundary is being designed". — `experimental/doris/README.md` (Layout section, last paragraph)

**Paper**
- "Rethinking Analytical Processing in the GPU Era", Yogatama et al. (UW-Madison, NVIDIA; Wes McKinney is a co-author), CIDR 2026. It is arXiv:2508.04701, v1 6 Aug 2025, v3 20 Dec 2025. — [arXiv abs](https://arxiv.org/abs/2508.04701); [CIDR 2026 page](https://mail.vldb.org/cidrdb/2026/rethinking-analytical-processing-in-the-gpu-era.html) (both accessed 2026-10-28)
- The paper says the host layer "includes the query parser and optimizer of external data systems". It says DuckDB integration needed "zero modification to DuckDB's codebase", while Doris needed "minimal modifications … to convert its internal query plans into the Substrait format". Most physical operators use libcudf, with custom kernels for "predicate pushdown, and materialization". It describes a "graceful fallback mechanism to the host database systems in the case of an error or missing features" (§3.2.1–3.2.2). — [arXiv PDF v3](https://arxiv.org/pdf/2508.04701v3) (accessed 2026-10-28)
- **Hardware and limits in the paper:** experiments ran on a GH200 (Lambda Labs, $1.5/h) and a 4×A100 cluster. Claimed results: 8.3× (TPC-H) and 7.4× (ClickBench) cost-efficiency gain over DuckDB, and up to 12.5× speedup with Doris. The paper-era prototype was in-memory only, with out-of-core and multi-GPU listed as future work (§3.4, §4). — [arXiv PDF v3](https://arxiv.org/pdf/2508.04701v3) (accessed 2026-10-28)
- The current code now has both: tiered GPU/host/disk spilling (`docs/README.md:99`) and multi-GPU (`docs/super-sirius/multi-gpu-architecture.md`).
- NVIDIA's developer blog says Sirius set a ClickBench record as a DuckDB extension consuming Substrait. — [NVIDIA blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/) (accessed 2026-10-28; only the search snippet was read, not the full page)

### Inferences
- **Substrait is not what the DuckDB path uses.** The paper says the DuckDB path routes "the Substrait plan" to Sirius. The current transparent path does not go through Substrait: it captures DuckDB's `LogicalOperator` directly (`sirius_optimizer_extension.cpp:154-156`). Substrait is used only on the embedder/FFI path. The paper describes an older design (estimate, based on code vs paper).
- **The engine is not DuckDB-free.** Its planner consumes `duckdb::LogicalOperator`, and the FFI `Context` embeds a private DuckDB instance. "Library vs extension" means "DuckDB-as-planning-substrate embedded vs DuckDB-as-host"; DuckDB is never absent (see Q3).
- **Pace of change:** the pin to an unreleased DuckDB branch (`v2.0-cyanoptera`), the 0.0.0 version, and rolling-only tags show a fast-moving, pre-1.0 project. The ABI coupling to an exact DuckDB revision is tight.

### Gaps
- The exact git SHAs behind the `stable`/`latest` assets (only short SHAs 8d17afe and 5350b8f were seen) and the years of the release dates were not shown on the releases page.
- The NVIDIA ClickBench blog was not fetched in full.
- No talk or video was reviewed.
- Submodules (duckdb, cucascade, substrait) were not checked out. cuCascade's internals and the exact DuckDB v2.0 API used are not verified beyond Sirius's own call sites.

## Q2. Cost-based optimizer: own or DuckDB's? GPU cost model? GPU-vs-CPU decision?

### Takeaway
Sirius has **no cost-based optimizer and no GPU cost model of its own**. Join order, filter pushdown, decorrelation and cardinality estimates all come from DuckDB's optimizer. Sirius only does three things on top:
1. Disables DuckDB rewrites it cannot execute.
2. Applies local plan-time rewrites (projection folding, key materialization, dense-count-join detection, dynamic-filter wiring, decimal-width bounds).
3. Makes runtime adaptive choices (hash-join partition/BUILD_PROBE mode, broadcast of small builds, MARK-join build side).

GPU-vs-CPU is a **capability decision, not a cost decision**: any plan that `create_plan()` can translate runs on the GPU.

### Cited Findings

**DuckDB optimizer is the planner**
- The post-optimizer hook copies the optimized plan. Code comment: "OnFinalizePrepare will attempt create_plan() on this copy — that's the single source of truth for GPU support. If the plan contains unsupported operators, create_plan() throws and we fall back to CPU." — `src/transparent/sirius_optimizer_extension.cpp:137-143`
- DuckDB's cardinality estimates are explicitly copied onto the captured plan (`copy_cardinality_estimates`). — `src/transparent/sirius_optimizer_extension.cpp:99-122`
- The paper agrees: "Sirius leverages DuckDB's optimized logical plans but replaces its backend with GPUs" (§4). — [arXiv PDF v3](https://arxiv.org/pdf/2508.04701v3) (accessed 2026-10-28)
- The `gpu_execution` table-function path also runs `Optimizer optimizer(*planner.binder, context); plan = optimizer.Optimize(...)`. — `src/sirius_extension.cpp:445-446,748-749`
- The FFI path does the same after lowering Substrait. — `src/sirius_ffi.cpp:136-138`

**DuckDB optimizer rules Sirius disables**
- `unsupported_optimizers` = PARTIAL_AGGREGATE_PUSHDOWN, GROUPING_SETS, AGGREGATE_REUSE, DISTINCT_AGGREGATE_REWRITE and SCALAR_FN_PUSHDOWN ("Rewrites that produce aggregate or scan shapes unsupported by GPU execution"). — `src/planner/unsupported_optimizers.hpp:25-32`
- This mask is published DB-globally at extension load, which affects CPU queries too. — `src/sirius_extension.cpp:3572-3595`
- The FFI context additionally disables IN_CLAUSE, COMPRESSED_MATERIALIZATION, STATISTICS_PROPAGATION, COLUMN_LIFETIME and LATE_MATERIALIZATION. — `src/sirius_ffi.cpp:213-226`

**Join planning has fixed sides and no cost-based choice of physical join**
- Hash join is chosen when any equality exists; otherwise nested loop. "Left side = probe (streamed), right side = build (materialized)", i.e. Sirius takes DuckDB's build side as given. — `docs/super-sirius/physical-plan-generation.md:46-55`
- Merge/piecewise/inequality-join selection by cardinality threshold is commented out. — `src/planner/sirius_plan_comparison_join.cpp:754-782`
- Blockwise NLJ and unsupported join types throw `NotImplementedException`. — `src/planner/sirius_plan_comparison_join.cpp:802-806`

**Embedder API depends on DuckDB's optimizer too**
- `Fragment::declare_input_cardinality` exists "so the optimizer can pick a join's build side. Undeclared streams plan as 1 row." The build side in the embedder API is therefore chosen by DuckDB's optimizer from caller-supplied row counts. — `include/sirius/ffi.hpp:115-118`

**Plan-time rewrites done by Sirius** — local, not cost-based
- Projection elision and folding. — `docs/super-sirius/physical-plan-generation.md:92-102`
- Expression join-key materialization. — `docs/super-sirius/physical-plan-generation.md:57`
- Decimal SUM width selection from column statistics. — `docs/super-sirius/physical-plan-generation.md:67`
- Dense count join detection using `EstimateCardinality` of children and a histogram byte budget. — `src/planner/sirius_plan_aggregate.cpp:424-480`
- Dynamic-filter (SIP) wiring from the build side to scans. — `src/planner/sirius_plan_comparison_join.cpp:581-648`
- Late-materialization pass. — `src/planner/late_mat_plan_pass.cpp`

**Runtime adaptive decisions**
- `compute_hash_join_partition_strategy()` picks BUILD_PROBE mode when partitions ≤ GPUs and the build is below `max_build_hash_table_bytes` (default 500 MB). — `docs/super-sirius/optimizations.md:97-112`
- Small builds (`< small_table_bytes`) are broadcast to every GPU. — `docs/super-sirius/optimizations.md:108`
- MARK join switches build side when `right_rows >= mark_join_build_switch_ratio * left_rows` (default 8.0). — `docs/super-sirius/optimizations.md:192-204`
- Adaptive partition count (PR #371). — `docs/super-sirius/optimizations.md:7`
- A "Data Size Estimation" component projects bytes reaching an operator's input port at runtime. — `docs/super-sirius/README.md:34` (doc `data-size-estimation.md`)

**No GPU-vs-CPU cost decision**
- `gpu_admission.cpp` only enumerates GPU scans. It has no costing. — `src/planner/gpu_admission.cpp:26-52`
- The README states: "all DuckDB queries are automatically intercepted … Queries with unsupported operators fall back silently to CPU." — `docs/README.md:76`

**Planned future work**
- The paper lists predicate transfer and better distributed shuffling as future optimizations (§3.4). — [arXiv PDF v3](https://arxiv.org/pdf/2508.04701v3) (accessed 2026-10-28)
- A dynamic-filter "refinement design … remains future work". — `docs/super-sirius/README.md:41`

### Inferences
- **A small query cannot be kept on the CPU.** With no cost model, a tiny or latency-sensitive query is routed to the GPU whenever it is translatable, even when the CPU would be faster (estimate, since no cost gate was found). Users can only opt out per session with `SET gpu_execution=false`.
- **Plan quality is DuckDB's quality.** DuckDB's join-order optimizer is tuned for its CPU engine (estimate). Sirius compensates only at runtime (partitioning, broadcast, MARK build-side switch). For peacockdb (DataFusion planner) this is not a reason to switch: Sirius offers no reusable GPU-aware optimizer component.
- **The rule mask leaks onto CPU queries.** Disabling DuckDB rules DB-globally (`unsupported_optimizers`) means that loading Sirius changes CPU plans too.

### Gaps
- `docs/super-sirius/data-size-estimation.md` and `task-creator.md` were not read in full. Some runtime-adaptive logic there may be more cost-like than summarized.
- Whether the plan generator ever reorders join children (e.g. swaps build/probe at plan time on its own) was checked only via grep and the docs. None was found, but the plan builders were not read line by line.

## Q3. API surface: narrow fragments, library vs extension, non-DuckDB hosts, Rust

### Takeaway
There is a real, small embedder API, `include/sirius/ffi.hpp` (namespace `sirius::ffi`):
- `Context::execute_substrait(plan_bytes, ArrowArrayStream*)` for whole queries;
- a `Fragment` API for multi-fragment / distributed execution (declared input streams, hash or broadcast outputs, `relay_from`, `result_to_arrow`).

Its limits:
- The unit is always a **Substrait plan**, lowered through an **embedded private DuckDB** (binder + DuckDB optimizer) and then Sirius's planner. Single physical operators, or Sirius's own planner, cannot be called directly.
- A non-DuckDB host can link it, as the experimental Doris/StarRocks Rust backends do. It still carries DuckDB inside.
- The Rust crate today exposes only context creation and `execute_substrait`.
- There is no CPU fallback on this path.
- Today the "library" is the DuckDB extension `.so` itself; a standalone `libsirius` is a stated goal.

### Cited Findings

**The `Context` class**
- `ffi.hpp` describes itself as "Public C++ surface for embedding Sirius (`Context` and `Fragment`)". It is "Intentionally lightweight … forward-declares the heavy internal type — so consumers bind it without pulling in sirius_context.hpp (and its cudf/rmm/duckdb includes)", and "is the public C++ API installed by libsirius". — `include/sirius/ffi.hpp:17-27`
- On construction, `Context` "brings up an initialized engine … and an embedded in-process DuckDB … DuckDB is used only to lower a Substrait plan to a DuckDB `LogicalOperator` (the translation step) and to own the catalog. Execution runs directly on the Sirius engine, not through DuckDB's query pipeline." — `include/sirius/ffi.hpp:45-57`
- `execute_substrait(const std::string& plan, std::uintptr_t out_stream_addr)` writes results to a caller-owned Arrow C `ArrowArrayStream`. — `include/sirius/ffi.hpp:67-73`

**The `Fragment` class** — `include/sirius/ffi.hpp:83-200`
- Calls: `declare_input_column(stream_id, name, duckdb_type_name)`, `declare_input_sender`, `declare_input_cardinality`, `declare_output`, `declare_output_broadcast`, `declare_output_hash_key` (integer, boolean, varchar and decimal keys), `build(substrait_plan)`, `relay_from(source, …)`, `close_input`, `run()`, `result_to_arrow`, `output_row_count` and `output_types`.
- Input streams are exposed to the plan as DuckDB views named `sirius_stream_<id>`. — `include/sirius/ffi.hpp:211-214`; `docs/super-sirius/streaming-fragments.md:76-87`
- Concurrency: "build(), run() and Context::execute_substrait execute one at a time per Context". — `include/sirius/ffi.hpp:93-95`

**How a Substrait plan is lowered**
- `SubstraitToDuckDB` → `duckdb::Planner` → `duckdb::Optimizer::Optimize` (if enabled) → Sirius plan. — `src/sirius_ffi.cpp:106-138`
- The embedded DuckDB loads only CoreFunctions and Parquet extensions. — `src/sirius_ffi.cpp:186-189`
- Substrait comes from a Sirius fork of duckdb-substrait (submodule `substrait` → `sirius-db/duckdb-substrait-extension` branch `v2`). — `.gitmodules`

**No fallback on this path**
- The code calls it "this no-fallback FFI path", and disables extra DuckDB rewrites it "cannot safely execute". — `src/sirius_ffi.cpp:200-226`

**Inner C++ layer**
- A lower-level `exec::streaming_fragment` exists for C++ callers holding a live `duckdb::ClientContext`. — `docs/super-sirius/streaming-fragments.md:10-22`; `src/exec/streaming_fragment.{hpp,cpp}`

**Rust crates**
- `sirius-sys` binds `make_context`, `make_context_from_config` and `execute_substrait` (cxx). — `rust/crates/sirius-sys/src/lib.rs:29-53`
- The safe `sirius` crate: "Today it binds just enough to prove the toolchain links … More of the API surface is added in later PRs." It exposes `SiriusContext::new/from_config_file`, `execute_substrait(&[u8]) -> Vec<RecordBatch>` and `execute_substrait_result`. — `rust/crates/sirius/src/lib.rs:7-9,30-108`
- "The engine keeps process-global GPU state, so it currently supports a single live context per process." — `rust/crates/sirius/src/lib.rs:27-29`
- `Fragment` is not bound in Rust at this SHA (grep of `sirius-sys` lib.rs found only Context functions).

**Library vs extension**
- The FFI header "is the seed of the public C++ API `libsirius` will expose; today it is compiled into the DuckDB extension, which the bindings link until a dedicated `libsirius` ships". — `rust/README.md:56-61`
- Until then, `build.rs` symlinks `sirius.duckdb_extension` as `libsirius.so`. — `rust/README.md:52-54`
- The CMake shared/static library options exist. — `CMakeLists.txt:25-26`

**Non-DuckDB hosts**
- The experimental Doris backend: "A Rust process that joins an unmodified, official Apache Doris FE as a backend node and runs the plan fragments it receives on the embedded Sirius engine". It translates fragments to Substrait. — `experimental/doris/README.md:1-20`
- StarRocks CN uses `sirius = { path = "../../rust/crates/sirius" }`. — `experimental/starrocks/Cargo.toml:61`
- The paper lists DataFusion, Ibis and Gluten as future Substrait hosts (§3.4). — [arXiv PDF v3](https://arxiv.org/pdf/2508.04701v3) (accessed 2026-10-28)

**Optimizer access**
- No API exposes Sirius planning separately from execution. `Fragment::build` = lower + optimize (DuckDB) + plan (Sirius). The only optimizer input is `declare_input_cardinality`. — `include/sirius/ffi.hpp:115-118,140-146`
- Extension-side table functions are SQL-level only: `gpu_execution`, `pin_table`, `unpin`, `reset cache`, `profiler`, `query label`, ANN index. — `src/sirius_extension.cpp:1085-1640,2360`

### Inferences
- **What peacockdb could hand Sirius** (DataFusion-produced Substrait, via `datafusion-substrait`) **(estimate)**:
  - A **subtree** expressed as Substrait plus `Fragment` input streams, i.e. stream views standing in for children.
  - Possibly a single operator wrapped as a tiny Substrait plan over a stream view.
- **What that would cost (estimate):**
  - Results always cross an Arrow C stream boundary, so they are materialized host-side unless the caller relays them via `relay_from`, which works only between Sirius fragments.
  - Plans are re-lowered through the Substrait → DuckDB → optimizer path. The code comments that Substrait lowering re-binds subtrees per level; it cost 10 s of lowering vs 4.6 s of execution on TPC-H Q21 SF100 before a footer cache was added. — `src/sirius_ffi.cpp:190-196`
  - DataFusion → Substrait → DuckDB semantic fidelity (types, functions) becomes a correctness risk (estimate).
- **Limits on interleaving with peacockdb's executor:** inputs enter only through declared streams. There is no API to pass GPU-resident cuDF tables in or out (estimate: no `cudf::table` or device-pointer entry point in `ffi.hpp`). Interleaving with peacockdb's own cuDF executor without host round-trips is not supported today.
- **Maturity:** Rust bindings are proof-of-life level and allow one context per process. The `libsirius` boundary is explicitly "being designed". Building a product on this API means depending on an unstable, pre-release interface.

### Gaps
- Whether `Fragment` streams can be fed from outside Sirius (external Arrow batches pushed into an input stream) was not confirmed. `ffi.hpp` exposes only `relay_from` (Sirius→Sirius) and `close_input`. `streaming-sessions.md` was not read in full; the Doris/StarRocks shells may use another path.
- Which Substrait relations and functions the forked duckdb-substrait consumer accepts was not checked (submodule not cloned).
- Licensing (Apache-2.0 per the file headers, e.g. `include/sirius/ffi.hpp:1-15`) and any NVIDIA-specific licensing of dependencies are not evaluated here.

## Q4. Fallback on unsupported operators/types

### Takeaway
Fallback is **whole-query, to DuckDB's own CPU plan**; there is no mixed plan with GPU for some operators and CPU for others.
- **Plan-time fallback:** if `create_plan()` throws `NotImplementedException` (unsupported operator, expression, type or shape), DuckDB keeps its original physical plan.
- **Runtime fallback:** if GPU execution fails at runtime, `PhysicalSiriusExecution` replays the stashed DuckDB CPU plan on a private executor in the same transaction, subject to admission checks.
- **No fallback** for Sirius-owned S3 parquet reads, for `enable_duckdb_fallback=false`, for interrupts, and on the FFI/embedder path.

### Cited Findings

**Unsupported features** — `docs/super-sirius/physical-plan-generation.md:41-44`
- **Operators** that throw `NotImplementedException`, triggering CPU fallback: `LOGICAL_WINDOW`, `LOGICAL_UNNEST`, `LOGICAL_SAMPLE`, `LOGICAL_ANY_JOIN`, `LOGICAL_ASOF_JOIN` and `LOGICAL_RECURSIVE_CTE`.
- **Expressions** are rejected at plan construction (a `from_duckdb()` nullptr), "so the query falls back to CPU instead of reaching the GPU evaluator".
- **Nested types:** nested-typed (STRUCT/LIST/MAP) columns are accepted for scan and passthrough but rejected as operands in WHERE, GROUP BY, DISTINCT, JOIN ON and sort keys.
- **Null types:** plan nodes with `SQLNULL` output types are rejected.
- "an unsupported expression falls back even when the input is empty".
- Note: `src/op/window` exists as a directory, but the doc lists LOGICAL_WINDOW as unsupported; not reconciled.

**Plan-time fallback code**
- The catch blocks in `OnFinalizePrepare` call `record_transparent_fallback()` and return `DO_NOT_REBIND`, leaving DuckDB's CPU plan in place. — `src/sirius_context.cpp:2038-2061`
- If `enable_duckdb_fallback` is false, the error is rethrown ("GPU plan generation failed"). — `src/sirius_context.cpp:2041-2043`
- Non-SELECT statements never go to the GPU. — `src/sirius_context.cpp:1817`

**Runtime fallback code**
- DuckDB's CPU physical plan is stashed as `cpu_fallback` before replacement. — `src/sirius_context.cpp:1989-1997`
- On a GPU error, `PhysicalSiriusExecution`:
  - propagates interrupts as-is (`physical_sirius_execution.cpp:441-442`);
  - refuses replay for S3 queries (`physical_sirius_execution.cpp:444-456`);
  - surfaces the error if fallback is disabled (`physical_sirius_execution.cpp:458-467`);
  - otherwise runs `admit_cpu_replay` and `run_cpu_fallback_plan` (`physical_sirius_execution.cpp:469-519`).
- S3 parquet is read only on the GPU path, because "DuckDB's CPU reader has no S3 filesystem, so an s3:// query cannot fall back to CPU". — `src/sirius_sql_rewrite.hpp` (doc comment on `references_sirius_owned_s3_parquet`)

**Other declines**
- Queries on hidden catalogs or DuckLake-internal connections are never sent to the GPU. — `src/transparent/connection_provenance.hpp:42-47`
- An unserializable plan copy is skipped with a debug log, because "a silent skip here is indistinguishable from 'GPU ran and was slow'". — `src/transparent/sirius_optimizer_extension.cpp:157-161`

**Docs and paper**
- "Unsupported queries silently fall back to CPU." — `docs/super-sirius/README.md:5`; `docs/README.md:76`
- The paper describes "a graceful fallback mechanism to the host database systems in the case of an error or missing features in Sirius" (§3.2.2). — [arXiv PDF v3](https://arxiv.org/pdf/2508.04701v3) (accessed 2026-10-28)

**Memory pressure is not a CPU fallback**
- Memory pressure is handled by spilling GPU→host→disk (downgrade executor), "Memory spilling / CPU fallback is handled by the downgrade executor". — `AGENTS.md:69-70`; `docs/README.md:99`

**FFI path**
- No fallback: "this no-fallback FFI path". — `src/sirius_ffi.cpp:200-201`
- `Fragment::build` throws on translation or planning failure. — `include/sirius/ffi.hpp:140-146`

### Inferences
- **Whole-query granularity:** one unsupported window function anywhere in a query sends the entire query to the DuckDB CPU engine. Coverage gaps (window, UNNEST, nested-type operands, ASOF) can therefore silently remove GPU acceleration from large parts of a real workload. Fallback is "silent" by default, logged only at INFO/DEBUG.
- **A host must handle unsupported plans itself:** an embedding host (like peacockdb would be) gets an exception, not a fallback. It would need its own CPU path and its own pre-check of Sirius capability. Only the DuckDB extension path provides the fallback.
- **The runtime replay is risky:** the replay re-executes the whole query on the CPU after a partial GPU run, which doubles latency on failure (estimate). Its correctness depends on the admission and lease checks seen in `physical_sirius_execution.cpp:469-519`.

### Gaps
- No public coverage matrix (types × functions) was found. The fuzzer docs (`test/fuzz/README.md`, `docs/fuzzer/`) were not read.
- How often fallback triggers on TPC-DS or other real workloads was not found in any source.
- The state of window support (directory `src/op/window` vs doc "unsupported") was not resolved.
