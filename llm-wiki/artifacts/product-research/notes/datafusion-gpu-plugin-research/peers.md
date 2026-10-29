# GPU-for-DataFusion peers: how each plugs into DataFusion

All sources accessed 2026-10-29. Code claims are pinned to these commit SHAs (shallow clones):
- rapidsai/gqe `50facc46fb99ecf4a955697e78803707335814b3` (the only commit in the repo)
- gabotechs/libcudf-rs `a6739ff058723e2d9d80b9abc7ee3d787bc07fd3` (HEAD, 2026-09-17, 181 commits)
- algeon-dev/algeon `73dadb70b076d6fa33b11e244c31fbdc1542b966` (HEAD, about 1,175 commits)
- apache/datafusion-comet `7f73d86fb841f5662f1343cf52b6e6b31eb17dfb` (HEAD on 2026-10-29)
- singhpratech/ArrowMetal `1f2111540e68edfdf1adf87df8821ca9cd1958c0`

Method note: the web search budget for this session ran out before I could run any web search. All findings come from the GitHub API (`gh api`), GitHub issue search, crates.io, and shallow clones. HN/Reddit launch posts, blogs, and non-GitHub projects (Voltron/Theseus, LanceDB GPU) were therefore **not** checked. See the Gaps sections.

## 1. NVIDIA GQE (rapidsai/gqe): DataFusion integration, scope, status

### Takeaway
GQE is a standalone C++/cuDF **server** (a node manager plus one task manager per GPU, using NVSHMEM across GPUs) that speaks Arrow Flight. DataFusion appears only on the **client side**. A Rust CLI uses DataFusion 52.3 to parse and optimize SQL into a *LogicalPlan*, encodes it as Substrait with `datafusion-substrait`, and sends it over Flight. A C++ `from_substrait` consumer then builds GQE's own logical plan, optimizer, and physical plan. Nobody can embed it as an ExecutionPlan inside a DataFusion app. It is a research/blueprint PoC ("speed-of-light"), published as one squashed commit with no activity since 2026-08-11.

### Cited Findings
- The README calls it "a proof-of-concept SQL query engine for running data analytics queries on GPUs. Its purpose is to achieve speed-of-light performance and serve as a blueprint to inform your database engine design." It claims "Total GQE execution time for all 22 queries at scale factor of 1000: 9.15 s" (single GPU, speedup chart vs DuckDB). The footnote says results are TPC-H-derived and not comparable to published TPC-H results. The README names no GPU model for the 9.15 s figure. — [gqe README @50facc4](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/README.md)
- **Deployment:** `gqe_node_manager --num-gpus N --task-manager-binary gqe_task_manager` spawns one task manager per GPU. The port opens "only after all task managers have started and NVSHMEM bootstrap has finished". Clients connect with `gqe-cli --server-url http://127.0.0.1:50051`. — [README](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/README.md)
- The C++ node manager's service "inherits `arrow::flight::FlightServerBase` rather than `FlightSqlServerBase`". So it is a custom Flight server that accepts Substrait/physical plans, not a full Flight SQL server. — [include/gqe/node_manager/service.hpp L40-47](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/include/gqe/node_manager/service.hpp)
- **DataFusion version:** `datafusion = "52.3.0"`, `datafusion-substrait = "52.3.0"`, `arrow-flight = "57.3.0"` (flight-sql-experimental), `substrait = "0.62.2"`. — [rust/gqe-cli/Cargo.toml](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/rust/gqe-cli/Cargo.toml); [rust/producer/Cargo.toml](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/rust/producer/Cargo.toml)
- **SQL → plan path:** the client calls `to_substrait_plan(&plan, &ctx.state())` from `datafusion_substrait::logical_plan::producer`, encodes the result, and sends it with `execute_substrait_plan` over Flight (`CommandStatementSubstraitPlan`). The client also accepts a serialized `gqe.proto.PhysicalRelation` (`--physical-plan`). DDL and `COPY ... FROM` go through custom Substrait extensions (`substrait_ext/ddl.rs`, `write.rs`). — [rust/gqe-cli/src/client.rs L51, L368-374](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/rust/gqe-cli/src/client.rs); [rust/gqe-cli/src/main.rs](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/rust/gqe-cli/src/main.rs)
- A separate Rust `gqe-server` binary, described as "DataFusion Flight SQL example using Substrait as the intermediate plan format", wraps a DataFusion `SessionContext` (`from_substrait_plan`). It is a DataFusion CPU reference server, not the GPU path. — [rust/gqe-cli/src/server.rs](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/rust/gqe-cli/src/server.rs)
- There is also an offline "Substrait producer" (Rust, DataFusion SQL parser and optimizer). It writes `.bin` Substrait plans that the C++ `tpc` benchmark binary executes. — [docs/substrait_producer/README.md](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/docs/substrait_producer/README.md)
- **C++ consumer:** `gqe::substrait_parser::parse_relation` accepts only Substrait `read, project, join, fetch, filter, sort, aggregate, set`. Anything else throws "Unsupported relation type". Window functions are parsed as expressions with frame bounds. Subquery expressions throw "has not been implemented". Types: bool, i8–i64, fp32/64, string/varchar, date, decimal. — [src/logical/from_substrait.cpp L770-800](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/src/logical/from_substrait.cpp); [include/gqe/logical/from_substrait.hpp](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/include/gqe/logical/from_substrait.hpp)
- GQE has its own logical relations (aggregate, fetch, filter, join, project, read, set, sort, user_defined, window, write), its own optimizer (`optimizer/rules`, estimator, statistics), and its own physical relations including `shuffle`. Its executor has custom kernels: mark_join, unique_key_inner_join, a shift-and LIKE kernel, partition/partition_merge, and window. — [include/gqe/logical](https://github.com/rapidsai/gqe/tree/50facc46fb99ecf4a955697e78803707335814b3/include/gqe/logical), [include/gqe/physical](https://github.com/rapidsai/gqe/tree/50facc46fb99ecf4a955697e78803707335814b3/include/gqe/physical), [include/gqe/executor](https://github.com/rapidsai/gqe/tree/50facc46fb99ecf4a955697e78803707335814b3/include/gqe/executor)
- **Storage:** `CREATE TABLE` + `COPY ... FROM '<dir>' (FORMAT parquet)` loads into GPU memory. "Tables live in GPU memory until the node manager exits." `CREATE EXTERNAL TABLE ... STORED AS PARQUET` reads from disk per query. Storage kinds: system, numa, numa_pinned, pinned, device, managed memory, and parquet_file. In-memory tables support zone maps and partition pruning, plus **nvCOMP compression** chosen per column: `GQE_COMPRESSION_FORMAT` ∈ {none, ans, lz4, snappy, cascaded, bitpacking, gdeflate, deflate, zstd, gzip, bitcomp}, an optional secondary format, ratio thresholds, and a decompression backend `auto|de|sm`. The Parquet reader is chunked (`GQE_PARQUET_CHUNK_READ_SIZE` 1 GiB, `PASS_READ_SIZE` 4 GiB). An experimental "customized Parquet reader" is described as "more performant but limited in functionality". — [README configuration table](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/README.md); [include/gqe/storage](https://github.com/rapidsai/gqe/tree/50facc46fb99ecf4a955697e78803707335814b3/include/gqe/storage)
- Other experimental work: an MLIR-based query compiler (relational operators → NVVM IR JIT) and CUPTI SM-utilization monitoring. — [README "Experimental features"](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/README.md)
- **Multi-GPU:** NVSHMEM symmetric heap (`nvshmem_malloc`), shuffle joins (`GQE_NUM_SHUFFLE_PARTITIONS`). "Symmetric memory region is not growable" in multi-GPU mode. — [include/gqe/communicator.hpp](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/include/gqe/communicator.hpp); [README](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/README.md)
- **License:** the repo is Apache-2.0 (GitHub reports `NOASSERTION` because of the extra text). LICENSE says "Building or using the software requires additional components licenced under other terms … CUPTI, nvCOMP, and NVSHMEM". `licenses/` contains the cuCollections, cuDF, DataFusion, nvCOMP (NVIDIA SDK license agreement), NVSHMEM, Spark, and Substrait licenses. — [LICENSE](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/LICENSE); [licenses/LICENSE.nvcomp.txt](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/licenses/LICENSE.nvcomp.txt)
- The README lists 22 contributors (NVIDIA staff, e.g. Clemens Lutz, Viktor Rosenfeld, James Xia). — [README Contributors](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/README.md)
- **Commits and activity:** the repo was created 2025-12-11. It has exactly **1 commit**, `50facc4` "Initial commit" by James Xia, dated 2026-08-10, and `pushed_at` is 2026-08-11. The only branch is `main`. There are no commits after 2026-08-11. 62 stars. — [GitHub API repos/rapidsai/gqe](https://api.github.com/repos/rapidsai/gqe); [commit 50facc4](https://github.com/rapidsai/gqe/commit/50facc46fb99ecf4a955697e78803707335814b3)
- In PR #1, an NVIDIA maintainer said: "To maintain consistency between our internal and public repos, I will make this change internally and propagate here". The PR was closed unmerged. A sibling repo `rapidsai/gqe-bench` exists (created 2026-02-20, last pushed 2026-08-11). — [gqe PR #1](https://github.com/rapidsai/gqe/pull/1); [API repos/rapidsai/gqe-bench](https://api.github.com/repos/rapidsai/gqe-bench)
- **Issue #2 / PR #3 / issue #4** are all by Rishitesh Mishra, whose GitHub profile says "Director of Engineering at MakeInfinite Labs, leading development of a HTAP database platform":
  - #2 (open, 2026-07-11) proposes ILIKE in the expression evaluator: lowercase with `cudf::strings::to_lower`, then reuse the shift-and or `cudf::strings::like` paths. It was verified on an L4 "end-to-end through gqe_node_manager + gqe-cli".
  - PR #3 (closed 2026-07-17, **not merged**) implemented it.
  - #4 (open, 2026-07-14) is a correctness bug: the default shift-and LIKE kernel returns wrong rows for patterns not anchored by `%`, e.g. `LIKE 'w_dget'` matches `gizmo`. Three root-cause defects in `like.cu` are listed.
  - None of the three has a maintainer reply.
  — [gqe #2](https://github.com/rapidsai/gqe/issues/2); [gqe PR #3](https://github.com/rapidsai/gqe/pull/3); [gqe #4](https://github.com/rapidsai/gqe/issues/4); [rishitesh profile](https://github.com/rishitesh)

### Inferences
- GQE is the "DataFusion as front end only" pattern: LogicalPlan → Substrait → foreign engine. peacockdb instead consumes the DataFusion **physical** plan. GQE needs no DataFusion ExecutionPlan integration, and gets no CPU fallback. An unsupported Substrait relation fails the whole query.
- The repo history was squashed, and PR #1 says changes are made internally and propagated. Combined with an unmerged external PR, this means the public repo is a code drop, not a community project (estimate). The issue timestamps (May–July 2026) predate the single commit (Aug 2026), which suggests earlier public history was replaced by a force-push (estimate).
- An external DB startup (MakeInfinite Labs) is evaluating GQE as an engine and finding correctness bugs in the default LIKE path. That is a signal GQE gets used as a kernel/engine donor despite its PoC status.
- The nvCOMP license (an NVIDIA SDK agreement, not OSS) is a redistribution consideration for anyone embedding GQE's storage layer (estimate; read the full agreement before relying on this).

### Gaps
- The GPU model and system for the 9.15 s SF1000 claim are not in the README. The `rapidsai/gqe-bench` repo was not inspected.
- No launch blog, HN, or GTC talk was found, because the web search budget was exhausted.
- I did not verify whether `CommandStatementSubstraitPlan` is the only query entry point on the C++ side (the server derives from FlightServerBase and handles the commands itself).

## 2. libcudf-rs (gabotechs/libcudf-rs)

### Takeaway
libcudf-rs is a Rust workspace with three parts: cxx-based libcudf bindings (`libcudf-sys`), a safe API (`libcudf-rs`), and `libcudf-datafusion`. The last one is a **physical-optimizer-rule plug-in**: it rewrites DataFusion's ExecutionPlan, replacing `FilterExec`/`ProjectionExec`/`SortExec`/`HashJoinExec`/`AggregateExec` with `CuDF*Exec` nodes and inserting `CuDFLoadExec`/`CuDFUnloadExec` at the CPU↔GPU boundaries. GPU data travels through DataFusion as ordinary `RecordBatch`es whose columns are a custom `unsafe impl Array for CuDFColumnView`. It targets DataFusion 55 and cuDF 26.08. It started as a hackathon project and is now an active multi-company effort (Datadog). It is the closest architectural analogue to "peacockdb as a DataFusion plugin".

### Cited Findings
- The README says "Rust bindings for libcudf … using the cxx library". It continues: "For SQL execution, this project uses Apache DataFusion with a physical optimizer rule that replaces vanilla DataFusion nodes with GPU variants". Example rewritten plan: `CuDFUnloadExec → CuDFSortExec → CuDFProjectionExec → CuDFAggregateExec mode=Single → CuDFProjectionExec → CuDFFilterExec → CuDFLoadExec → DataSourceExec`. — [README @a6739ff](https://github.com/gabotechs/libcudf-rs/blob/a6739ff058723e2d9d80b9abc7ee3d787bc07fd3/README.md)
- The README's cost claim: m5.4xlarge (16 vCPU, about $625/mo) runs TPC-H Q1 in 906 ms, while g4dn.xlarge (NVIDIA T4, about $423/mo) runs it in 813 ms, so "executing on GPU is 1.65x cheaper". It is described as "the result of a couple of weeks' hackathon … capable of running the full TPCH suite on GPU". — [README](https://github.com/gabotechs/libcudf-rs/blob/a6739ff058723e2d9d80b9abc7ee3d787bc07fd3/README.md)
- **Plug-in mechanics:** `SessionStateBuilderExt::with_cudf_planner()` inserts a `CuDFConfig` config extension, **forces `target_partitions = 1`**, and registers three `PhysicalOptimizerRule`s: `HostToCuDFRule`, `ParquetAggregateStreamsRule`, and `RescaleLeafsRule(target_partitions)`. — [libcudf-datafusion/src/planner/session_state_builder_ext.rs](https://github.com/gabotechs/libcudf-rs/blob/a6739ff058723e2d9d80b9abc7ee3d787bc07fd3/libcudf-datafusion/src/planner/session_state_builder_ext.rs)
- `HostToCuDFRule` downcasts to `FilterExec`, `ProjectionExec`, `SortExec`, `HashJoinExec`, `AggregateExec`, and `CoalescePartitionsExec`. — [planner/host_to_cudf.rs L48-102](https://github.com/gabotechs/libcudf-rs/blob/a6739ff058723e2d9d80b9abc7ee3d787bc07fd3/libcudf-datafusion/src/planner/host_to_cudf.rs)
- Physical nodes: aggregate, coalesce, cudf_load, cudf_unload, filter, hash_join, parquet_scan, projection, sort. Expressions: binary, cast, column, literal, plus an AST path. — [libcudf-datafusion/src/physical](https://github.com/gabotechs/libcudf-rs/tree/a6739ff058723e2d9d80b9abc7ee3d787bc07fd3/libcudf-datafusion/src/physical); [src/expr](https://github.com/gabotechs/libcudf-rs/tree/a6739ff058723e2d9d80b9abc7ee3d787bc07fd3/libcudf-datafusion/src/expr)
- `CuDFConfig` defaults: `batch_size = 512_000` rows (small upstream batches are coalesced before upload), `aggregate_chunk_target_bytes = 256 MiB`, `parquet_scan = false` (an experimental cuDF direct Parquet scan, off by default), and `parquet_scan_streams = 1`. — [planner/config.rs](https://github.com/gabotechs/libcudf-rs/blob/a6739ff058723e2d9d80b9abc7ee3d787bc07fd3/libcudf-datafusion/src/planner/config.rs)
- **GPU-resident batches between operators:** `unsafe impl Array for CuDFColumnView` and `unsafe impl Array for CuDFScalar`. `to_data()` panics in debug builds "so implicit GPU->CPU copies are easily caught". — [src/column_view.rs L292](https://github.com/gabotechs/libcudf-rs/blob/a6739ff058723e2d9d80b9abc7ee3d787bc07fd3/src/column_view.rs); [src/scalar.rs L253](https://github.com/gabotechs/libcudf-rs/blob/a6739ff058723e2d9d80b9abc7ee3d787bc07fd3/src/scalar.rs)
- **Versions:** `datafusion = "55"` (upgraded in PR #83, 2026-09-14), `arrow = "59"` (lockfile `arrow-array 59.3.0`), cuDF 26.08.01 (PR #84). — [libcudf-datafusion/Cargo.toml](https://github.com/gabotechs/libcudf-rs/blob/a6739ff058723e2d9d80b9abc7ee3d787bc07fd3/libcudf-datafusion/Cargo.toml); [PR #83](https://github.com/gabotechs/libcudf-rs/pull/83); [PR #84](https://github.com/gabotechs/libcudf-rs/pull/84)
- **Maturity and people:** the repo was created 2025-11-24. It has 181 commits, 20 stars, Apache-2.0, and was last pushed 2026-09-17. Contributors by API: gabotechs 124, notfilippo (Filippo, Datadog) 13, jayshrivastava 10. Gene Bordegaray (profile: "Datadog") authored about 40 PRs (#16–#81). His commits do not appear in the contributors API, probably because of unlinked commit emails (estimate). gabotechs' profile: "Author of Distributed DataFusion … Apache DataFusion committer". — [API repos/gabotechs/libcudf-rs](https://api.github.com/repos/gabotechs/libcudf-rs); [contributors](https://api.github.com/repos/gabotechs/libcudf-rs/contributors); [issue/PR list](https://github.com/gabotechs/libcudf-rs/pulls?q=is%3Apr); [gene-bordegaray](https://github.com/gene-bordegaray); [gabotechs](https://github.com/gabotechs); [notfilippo](https://github.com/notfilippo)
- **Issue #49 "EPIC: Reduce cuDF DataFusion runtime and transfer overhead"** (Gene Bordegaray, 2026-05-05, open, 1 comment), in full:
  - Summary: "libcudf-datafusion is primarily bottlenecked by runtime orchestration and host/device transfer overhead rather than raw GPU kernel time."
  - Pooling (RMM) cut device alloc/free from **550,984 calls / 3.54 s to 16 calls / 9 ms** across the compared query set.
  - The profiles still showed **about 6.78 s of CUDA runtime API time vs 0.54 s of kernel activity**.
  - q19 default scan output was **736 lineitem batches for 6M rows** (6M rows implies SF1). Raising the batch size to 262,144 rows gave 24 batches and cut **q19 GPU runtime from about 1362 ms to about 525 ms**.
  - Plans "bounce between GPU and CPU". In q19, a large filter runs on GPU, then the plan unloads to CPU for `HashJoinExec` and the global aggregate, then reloads to the GPU for the final projection.
  - `cudaHostAlloc` is a hotspot, but is mostly per-profile pinned-pool setup.
  - Action items: (1) a GPU-oriented batch size (compare default/65,536/262,144); (2) thread an explicit cuDF/RMM stream through Arrow interop, because upload today pins the batch, calls `from_arrow_host`, then synchronizes the default stream per batch; keep pinned batches alive until event completion; (3) reduce CPU/GPU ping-pong (residual-filter hash joins, GPU global aggregates, no reload of tiny CPU outputs); (4) plan-boundary and transfer diagnostics (Load/Unload counts, batch counts).
  - The issue does not name a GPU model. The companion issue #50 by the same author on the same day uses a **Tesla T4 15 GiB**.
  — [libcudf-rs #49](https://github.com/gabotechs/libcudf-rs/issues/49)
- **#49 comment by nevi-me** (2026-07-23), from their own cuda-oxide + DataFusion attempt on an RTX 5080 + Ryzen 9950X3D:
  - "On consumer hardware, the data CPU<>GPU data transfers dominate compute time, and a strong CPU beats the GPU most of the time."
  - Measured PCIe5 H2D: about 16 GB/s pageable vs about 46 GB/s pinned. Kernel launch floor of about 7 µs. The CPU/GPU crossover is 64K–1M rows; DataFusion's 8192-row default is "~100x too small".
  - Per-batch pinned staging came out net 0.8x. A "row-preserving GPU island feeding a CPU consumer lost at every scale" (about 0.55x for standalone filters). Only scalar-output global aggregates beat the CPU from host-resident data.
  - q6 at SF20: CPU-decode-then-upload took 1217 ms (0.13x vs a 154 ms multi-core CPU). A GPU-native Parquet read (cuDF 26.06) took 131 ms (1.18x). "The gap is architectural rather than orchestration."
  - They proposed a cost gate at island creation and "cpu / gated / forced" triad benchmarking.
  — [libcudf-rs #49 comment](https://github.com/gabotechs/libcudf-rs/issues/49#issuecomment)
- **#50** (2026-05-05, closed): on a Tesla T4 15 GiB at TPC-H SF10 with 4 partitions, the 4 GiB RMM pool cap caused OOMs on multi-join queries (q5/q8/q9 implied). With a 12 GiB cap, the full SF10 run totalled **97,779 ms across all 22 queries**. Suspected cause: the GPU hash join "materializes whole join sides by collecting all input batches and concatenating them". — [libcudf-rs #50](https://github.com/gabotechs/libcudf-rs/issues/50)
- **#75** (open): `cudf::detail::gather` stream syncs via `column_device_view::create` for strings. A maintainer comment from felipeblazing (a BlazingSQL co-founder, per their handle; estimate) warns that CPU profilers misattribute sync time. An nsys profile of TPC-H Q1 SF10 on a T4 shows GPU work of 658.8 ms per Q1: hash groupby 225.7 ms (34%), H2D transfer 116.2 ms (17.6%), Parquet decode kernels 105.3 ms (16%), gather/casts 87.8 ms, D2D copies 44.0 ms. — [libcudf-rs #75](https://github.com/gabotechs/libcudf-rs/issues/75)
- **#82** (open, 2026-08-04, 6 comments): "Blocking work runs directly on tokio async worker threads". Each FFI call into cuDF mixes CPU work, async launches, and blocking stream syncs inside `poll_next()`. The proposal is `spawn_blocking` and later a "stream + tokio task per batch" model, because `target_partitions=1` leaves the GPU unfed. — [libcudf-rs #82](https://github.com/gabotechs/libcudf-rs/issues/82)
- **Direction (2026-09):**
  - PR #93 "attach execution streams to record batch data".
  - PR #92 "support cuda streams in aggregations" adds `ParquetAggregateStreamsRule`, which splits a Single aggregate over the cuDF Parquet scan into 8 Partial pipelines on separate CUDA streams plus `CuDFCoalescePartitionsExec` and a Final aggregate.
  - PR #90 nonblocking stream; #89 avoid long polls; #86 CAST.
  - #85 (open) semi/anti joins.
  - May 2026: #72 experimental cuDF direct Parquet scans, #68 coalesce host batches before upload, #60 pinned MR pool, #58 streaming hash joins with reusable build state, #66 cache cuDF literal scalars.
  - No PR mentions caching tables across queries; #66 is scalar-literal caching only.
  — [PR #92](https://github.com/gabotechs/libcudf-rs/pull/92); [PR #93](https://github.com/gabotechs/libcudf-rs/pull/93); [issue list](https://github.com/gabotechs/libcudf-rs/issues?q=)
- **Upstream friction:** arrow-rs PR #9092 "Seal Array trait" (merged 2026-01-07). gabotechs objected that he used `dyn Array` "for implementing a GPU-based dyn Array baked by cudf in libcudf-rs … As DataFusion moves RecordBatches between nodes … that was my means of transportation", and asked whether DataFusion could expose "a customizable trait for transporting data". tustvold replied that passing it into any arrow-rs kernel "will cause it at best panic". scovich: "let's figure out a better API for backing arrays with GPU memory as a follow on". At tag 59.3.0, `Array` is `pub unsafe trait Array` (not sealed), and libcudf-rs on 59.3 still implements it. — [arrow-rs PR #9092](https://github.com/apache/arrow-rs/pull/9092); [arrow-array/src/array/mod.rs @59.3.0 L100](https://github.com/apache/arrow-rs/blob/59.3.0/arrow-array/src/array/mod.rs)
- gabotechs' earlier experiment, `gabotechs/datafusion-gpu` (2024-12 to 2025-05, no license), compared CubeCL and cudarc hand-written CUDA kernels for a single SUM UDAF inside DataFusion. — [gabotechs/datafusion-gpu README](https://github.com/gabotechs/datafusion-gpu)

### Inferences
- libcudf-rs is the "Comet-style inside DataFusion" pattern: per-node replacement within one DataFusion ExecutionPlan, with CPU islands where nodes are unsupported. peacockdb's whole-plan recipe is the opposite trade-off. It has no ping-pong, but it gives all-or-nothing coverage.
- Its main lessons align with peacockdb's design choices: the cost is orchestration and transfer rather than kernels, GPU-native Parquet decode is needed, and CPU↔GPU ping-pong is costly. They are moving toward GPU-native scans and multi-stream pipelines.
- Using `RecordBatch` with a fake `Array` impl to carry GPU columns is fragile against arrow-rs. The "unsafe" marker and #9092 show upstream considers external `Array` impls unsupported. No upstream device-aware batch type exists yet (see section 3).
- All public numbers are on a T4 at SF1–SF10, far below GQE's and Algeon's scales.

### Gaps
- The text of #49 contains no complete per-query TPC-H table. Only q19 (1362 → 525 ms) and the aggregate profile totals are given. #50 gives the 97.8 s SF10 total on a T4.
- I did not read the full 6-comment thread of #82 beyond the first two posts.
- The URL for nevi-me's comment is the issue URL; the comment anchor ID was not retrieved.

## 3. Algeon, datafusion #26633 / PR #26004, and upstream DataFusion GPU discussion

### Takeaway
Algeon (algeon-dev/algeon, Apache-2.0, created 2026-10-19 publicly but with about 1,175 commits) is the most complete peer. It is a Rust DataFusion **55.1** physical-optimizer-rule backend that lowers supported physical subtrees into its own native IR, executed with cuDF on forked RAPIDS libraries. It has a GPU Parquet reader via KvikIO (local and S3), cuGraph/cuVS/cuML table functions, an embeddable `algeon-datafusion` crate, and a Flight SQL server. Its upstream asks are small API hooks: issue #26633 / PR #26635 make `ParquetFileReaderFactory` downcastable so a rule can safely swap in its GPU reader. **PR #26004 is not Algeon.** It is a one-line docs PR adding *datafusion-arrowmetal* to DataFusion's integrations list, merged 2026-10-09. I found no upstream DataFusion design discussion of device-aware ExecutionPlans or non-CPU batches. The closest are arrow-rs issue #10752 (C Device Data Interface FFI, open) and the #9092 thread.

### Cited Findings
- README: "a GPU-accelerated query engine built on Apache DataFusion. It brings NVIDIA's cuGraph, cuVS and cuML algorithms into SQL as table functions, alongside cuDF relational execution". "Algeon's DataFusion physical optimizer rule replaces supported plan candidates with in-process GPU execution, returning results as standard Arrow batches. Unselected candidates execute on the DataFusion CPU path". "GPU execution reads Parquet from local disk or S3-compatible object storage through KvikIO". "Not a fit yet: working sets larger than one GPU's memory, sources other than Parquet". It is marked "experimental". — [algeon README @73dadb7](https://github.com/algeon-dev/algeon/blob/73dadb70b076d6fa33b11e244c31fbdc1542b966/README.md)
- **Versions and deps:** `datafusion = "55.1"`, `arrow = "59.3"`, plus vendored or forked `algeon-cudf`/`algeon-cudf-sys` 0.1.3 and cugraph/cuml/cuvs components. — [Cargo.toml](https://github.com/algeon-dev/algeon/blob/73dadb70b076d6fa33b11e244c31fbdc1542b966/Cargo.toml); [components/](https://github.com/algeon-dev/algeon/tree/73dadb70b076d6fa33b11e244c31fbdc1542b966/components)
- **Embedding API:** add the crate `algeon-datafusion = "0.1.1"`, then build `AlgeonGpuBackend::builder().devices([...]).build()?` and call `backend.install_on(SessionStateBuilder::new_with_default_features())`. "Only one live backend may exist in a process." It installs "backend-bound native rules and coverage". — [website/docs/integration/datafusion-session.mdx](https://github.com/algeon-dev/algeon/blob/73dadb70b076d6fa33b11e244c31fbdc1542b966/website/docs/integration/datafusion-session.mdx)
- **Lowering:** "Algeon's adapter lowers supported physical subtrees into native IR, replacing them with GPU execution wrappers" while preserving ordering and partition contracts. — [website/docs/design/native-lowering.mdx](https://github.com/algeon-dev/algeon/blob/73dadb70b076d6fa33b11e244c31fbdc1542b966/website/docs/design/native-lowering.mdx)
- **Execution:**
  - A compiled schedule runs per native fragment on a GPU execution pool, with CUDA stream "lanes".
  - Runtime dynamic filters are derived from join builds.
  - An admission control ledger enforces a per-query memory cap.
  - "Within supported GPU plan fragments, intermediate relations stay in device memory".
  — [design/gpu-execution.mdx](https://github.com/algeon-dev/algeon/blob/73dadb70b076d6fa33b11e244c31fbdc1542b966/website/docs/design/gpu-execution.mdx); [README](https://github.com/algeon-dev/algeon/blob/73dadb70b076d6fa33b11e244c31fbdc1542b966/README.md)
- **Benchmarks:** TPC-H SF100 on AWS g7e.8xlarge (Xeon 8559C 32 vCPU, 256 GiB, **NVIDIA RTX PRO 6000 Blackwell Server Edition 96 GB**), DataFusion 55.1, source revision d4b48c5. Parquet comes from local NVMe via the page cache, KvikIO compat mode, no GDS, and "The GPU keeps no data between queries (`cache_cap_bytes = 0`)".
  - TPC-H: all 22 queries on GPU, parity matched. CPU total 47,911 ms vs **GPU total 13,872 ms (3.45x)**. GPU faster on all 22; peak operator memory about 29 GB.
  - TPC-DS SF100: all 99 on GPU, 99/99 parity matched. CPU 169,559 ms vs GPU 51,097 ms (my sum). GPU faster on 69 of 99.
  — [website/docs/reference/benchmarks/index.mdx L263-299](https://github.com/algeon-dev/algeon/blob/73dadb70b076d6fa33b11e244c31fbdc1542b966/website/docs/reference/benchmarks/index.mdx); [website/src/data/tpchSf100Benchmark.ts](https://github.com/algeon-dev/algeon/blob/73dadb70b076d6fa33b11e244c31fbdc1542b966/website/src/data/tpchSf100Benchmark.ts); [tpcdsSf100Benchmark.ts](https://github.com/algeon-dev/algeon/blob/73dadb70b076d6fa33b11e244c31fbdc1542b966/website/src/data/tpcdsSf100Benchmark.ts)
- **Roadmap:**
  - Near term: plan-sized admission grants and packaged releases.
  - Mid term: Iceberg (delete files etc.), "Build against NVIDIA's own repositories instead of the current forks", and Flight SQL auth/audit.
  - Longer term: "Spilling to disk through cuCascade", GPUDirect Storage, **"Multi-GPU execution. Development runs on one GPU today"**, and non-Parquet GPU scans.
  — [website/docs/reference/roadmap.md](https://github.com/algeon-dev/algeon/blob/73dadb70b076d6fa33b11e244c31fbdc1542b966/website/docs/reference/roadmap.md)
- **datafusion #26633** (open, 2026-10-23, by morristai, 0 comments) asks for an `Any` supertrait plus `is`/`downcast_ref` on `dyn ParquetFileReaderFactory`. Its stated reason: "Algeon replaces DataFusion Parquet scans with its own GPU reader when that reader can reproduce what the scan reads. A scan with DefaultParquetFileReaderFactory or CachedParquetFileReaderFactory qualifies. A caller-provided factory does not … Today we tell them apart by checking that format!("{factory:?}") starts with …". Companion PR #26635 is open. The same author also filed #26632 / PR #26634 (nested-loop-join filter pushdown evaluating CASE branches on every row). — [datafusion #26633](https://github.com/apache/datafusion/issues/26633); [PR #26635](https://github.com/apache/datafusion/pull/26635); [#26632](https://github.com/apache/datafusion/issues/26632)
- **datafusion PR #26004** (singhpratech, merged 2026-10-09 by alamb) is a one-line addition of `datafusion-arrowmetal` to `docs/source/user-guide/introduction.md` "Integrations", described as "extends DataFusion through a PhysicalOptimizerRule … run on the Apple silicon GPU through Metal and hand back the RecordBatches DataFusion expects". — [datafusion PR #26004](https://github.com/apache/datafusion/pull/26004)
- **Upstream "GPU / device-aware" discussion found:**
  - arrow-rs #10752 (open, 2026-08-19): "arrow-rs cannot receive or hand out an Arrow array whose buffers are not in host memory". It proposes `FFI_ArrowDeviceArray`/`FFI_ArrowDeviceArrayStream`. "This came up as a blocker moving a GPU-resident batch between two Rust libraries". Its PR #10754 was closed unmerged.
  - The arrow-rs #9092 thread (section 2).
  - datafusion-python #936 (open, 2024-10-28, timsaucer): "Spike: evaluate if cuDF can be used with datafusion-python".
  - GitHub issue search for GPU/cuda/cudf/device in apache/datafusion returned no design issue for device-aware ExecutionPlan or non-CPU batches.
  — [arrow-rs #10752](https://github.com/apache/arrow-rs/issues/10752); [arrow-rs PR #10754](https://github.com/apache/arrow-rs/pull/10754); [datafusion-python #936](https://github.com/apache/datafusion-python/issues/936); [GitHub search](https://github.com/apache/datafusion/issues?q=gpu)

### Inferences
- Algeon is the strongest direct competitor to a "peacockdb as DataFusion plugin" product. It has the same building blocks: Rust + DataFusion physical plan → own IR → cuDF, plus a GPU Parquet reader and single-GPU scope. It is further along on coverage (TPC-DS 99/99), mixed CPU/GPU execution, S3/Iceberg, admission control, and a Flight SQL server. Its ML/graph/vector UDTFs are an extra differentiator. Its listed gaps (spilling, multi-GPU, cache across queries, `cache_cap_bytes=0` in benchmarks) overlap with peacockdb's own.
- The upstream pattern is "physical optimizer rule + small extensibility PRs" (downcastable traits). That is what peacockdb would need too. It is not a new device-aware ExecutionPlan API, and none is being discussed in DataFusion (as far as GitHub search shows).

### Gaps
- Algeon's authorship and organization are unclear (morristai profile: "I make AI smarter in data infra"). The "Introducing Algeon" blog post was not read (no web access).
- The Algeon TPC-H per-query table and the cost of its native IR versus DataFusion ExecutionPlan granularity were not examined in depth.
- Upstream discussion outside GitHub issues (DataFusion Discord, mailing list, community-call notes) was not searched.

## 4. datafusion-arrowmetal

### Takeaway
`datafusion-arrowmetal` (crates.io 0.5.0, 2026-10-08, Apache-2.0, by singhpratech) is a DataFusion 55.1 `PhysicalOptimizerRule` that offloads a narrow, measured set of operators to the Apple-silicon GPU via Metal through the C library `libArrowMetalC.dylib`. Those operators are full sorts and certain group-bys over ≥50M rows. It relies on unified memory, so no PCIe copy is involved. Its relevance to an NVIDIA-PCIe engine is mainly as a plugin-shape precedent: an opt-in rule with a per-node TAKEN/LEFT report.

### Cited Findings
- The crates.io description: "A DataFusion 55.1 physical optimizer rule that runs full sorts, and count(*), DISTINCT and integer MIN/MAX group-bys over tables of 50M rows or more in the measured group-count ranges, on Apple silicon GPUs through ArrowMetal; hash joins, top-k and filters are available and off by default". Versions 0.4.0/0.4.1 (2026-10-02) and 0.5.0 (2026-10-08); 34 downloads. — [crates.io datafusion-arrowmetal](https://crates.io/crates/datafusion-arrowmetal)
- The ArrowMetal README says "Arrow columns in unified memory, copy-free where the producer's buffers are page aligned" and that it integrates with DataFusion, Polars, and DuckDB. Claims on an Apple M4 Max at 50M rows: DataFusion `ORDER BY` int64 22.7x faster; full sorts 6.9x–28.8x; group-by series 2.31x–4.27x. Operators "Left to DataFusion": top-k 0.14x–0.41x and filters 0.31x–0.79x. `rule.report()` gives each node TAKEN or LEFT and why. It supports Arrow C Data, C Device, and C Stream interop. — [ArrowMetal README @1f21115](https://github.com/singhpratech/ArrowMetal/blob/1f2111540e68edfdf1adf87df8821ca9cd1958c0/README.md)
- The repo was created 2026-09-06, last pushed 2026-10-28, and has 13 stars. It is listed on DataFusion's integrations page via PR #26004. — [API repos/singhpratech/ArrowMetal](https://api.github.com/repos/singhpratech/ArrowMetal); [datafusion PR #26004](https://github.com/apache/datafusion/pull/26004)

### Inferences
- The measured slowdowns on filters and top-k, even with zero-copy unified memory, support nevi-me's finding that only certain operator shapes pay off on a GPU. A cost-gated, per-operator rule seems to be the norm for "plugin" designs.

### Gaps
- None material for this scope.

## 5. Apache DataFusion Comet as the "Comet-style native exec" analogue; cuDF for Comet?

### Takeaway
Comet plugs into Spark through Spark's own plan-rewrite rules. `CometExecRule extends Rule[SparkPlan]` walks the Spark physical plan. `QueryPlanSerde` serializes supported operators and expressions into protobuf (`operator.proto`). The JVM hands the bytes to Rust via JNI (`Native.createPlan` / `executePlan`), and `planner.rs` rebuilds a DataFusion ExecutionPlan. Unsupported operators stay in Spark. I found no cuDF/GPU backend for Comet. Comet's own FAQ positions it as CPU-only and points to NVIDIA's Spark plugin for GPUs.

### Cited Findings
- `case class CometExecRule(session: SparkSession, ...) extends Rule[SparkPlan]` uses `QueryPlanSerde` helpers to build native operators. — [spark/src/main/scala/org/apache/comet/rules/CometExecRule.scala L227-228 @7f73d86](https://github.com/apache/datafusion-comet/blob/7f73d86fb841f5662f1343cf52b6e6b31eb17dfb/spark/src/main/scala/org/apache/comet/rules/CometExecRule.scala); [QueryPlanSerde.scala](https://github.com/apache/datafusion-comet/blob/7f73d86fb841f5662f1343cf52b6e6b31eb17dfb/spark/src/main/scala/org/apache/comet/serde/QueryPlanSerde.scala)
- The protobuf operator schema lives at `native/proto/src/proto/operator.proto` (about 57 KB). The Rust planner matches `OpStruct::Filter`, `Limit`, `Sort`, `NativeScan`, etc. — [operator.proto](https://github.com/apache/datafusion-comet/blob/7f73d86fb841f5662f1343cf52b6e6b31eb17dfb/native/proto/src/proto/operator.proto); [native/core/src/execution/planner.rs](https://github.com/apache/datafusion-comet/blob/7f73d86fb841f5662f1343cf52b6e6b31eb17dfb/native/core/src/execution/planner.rs)
- The JNI entry points are `Java_org_apache_comet_Native_createPlan` (L606) and `Java_org_apache_comet_Native_executePlan` (L1458). — [native/core/src/execution/jni_api.rs](https://github.com/apache/datafusion-comet/blob/7f73d86fb841f5662f1343cf52b6e6b31eb17dfb/native/core/src/execution/jni_api.rs)
- Releases: 1.0.0 on 2026-08-07 and 1.1.0 on 2026-10-06; 1,306 stars. — [Comet releases](https://github.com/apache/datafusion-comet/releases)
- Comet's FAQ refresh (PR #6783, 2026-10-08) says: "The NVIDIA cuDF plugin for Apache Spark (https://github.com/NVIDIA/cudf-spark), formerly the RAPIDS Accelerator, runs queries on NVIDIA GPUs. Comet runs on the CPUs your cluster already has." — [datafusion-comet PR #6783](https://github.com/apache/datafusion-comet/pull/6783)
- GitHub issue search in apache/datafusion-comet for "gpu" and "cudf" found no proposal for a cuDF backend. The only hits are roadmap or FAQ mentions. — [search](https://github.com/apache/datafusion-comet/issues?q=gpu)

### Inferences
- Comet is structurally the same as peacockdb's recipe: host-engine physical plan → serialized IR (protobuf vs FlatBuffers) → native executor across an FFI (JNI vs C ABI), with per-operator fallback to the host engine. The difference is that Comet keeps fallback inside the plan through columnar transitions (Arrow FFI), while peacockdb today is whole-plan. A "Comet for DataFusion" would be libcudf-rs or Algeon's in-process rule. peacockdb's existing serde boundary makes a Comet-style "native island" design natural.
- NVIDIA renamed RAPIDS Accelerator to "cuDF plugin for Apache Spark" (`NVIDIA/cudf-spark`), per Comet's FAQ. I did not verify this independently.

### Gaps
- I could not confirm, through web or HN search, whether anyone has prototyped Comet + cuDF outside the apache org.

## 6. MARIS Beacon issue #231

### Takeaway
Beacon (an AGPL-3.0 Rust/DataFusion climate data-lake engine) opened #231 to prototype `libcudf-rs` as an optional GPU backend. As of 2026-10-29 it has no replies. This is evidence of downstream DataFusion-app demand for a drop-in GPU plugin, specifically naming the physical-optimizer-rule approach.

### Cited Findings
- #231 "Explore optional GPU acceleration via libcudf-rs" (robinskil, 2026-06-04, open, **0 comments**). It notes that libcudf-datafusion "integrates with Apache DataFusion by applying physical optimizer rules that replace eligible DataFusion execution nodes with cuDF-backed GPU variants". Target workloads: large Parquet/Arrow IPC/CSV scans, projection- and filter-heavy queries, aggregation, sort/group-by, and repeated analytical queries. Goals: optional and feature-flagged, CPU path unchanged and used as fallback, and measure performance and correctness. — [beacon #231](https://github.com/maris-development/beacon/issues/231)
- Beacon: "A high-performance climate data lake query engine supporting subsetting for zarr, netcdf, parquet, arrow ipc, csv, delta and iceberg". AGPL-3.0, about 692 commits, active (last commit `a612fce`, 2026-10-29). — [API repos/maris-development/beacon](https://api.github.com/repos/maris-development/beacon); [commit a612fce](https://github.com/maris-development/beacon/commit/a612fcec6c8493902c7b8148e51c4316d34ac6db)

### Inferences
- What a DataFusion app wants: an opt-in rule, CPU fallback, and formats beyond Parquet (Zarr/NetCDF via custom TableProviders). A Parquet-only, whole-plan GPU engine would not cover Beacon's custom sources unless it accepts Arrow batches from arbitrary DataFusion scans.

### Gaps
- No follow-up commits or PRs on this were found. I did not search the Beacon PRs.

## 7. Other GPU + DataFusion projects (2024–2026)

### Takeaway
Besides GQE, libcudf-rs, Algeon, and ArrowMetal, GitHub search shows only small or early efforts: gabotechs/datafusion-gpu (CubeCL/cudarc SUM UDAF experiment), two other libcudf Rust binding crates (mbrobbel/cudf-rs, 0xDaizz/cudf-rs), and vyncint/oxidelake (DataFusion/Ballista with "optional CUDA/Metal acceleration and per-batch CPU fallback", created October 2026). nevi-me reported a private cuda-oxide + DataFusion attempt. Voltron Data (Theseus), LanceDB GPU, and Ballista GPU could not be checked because the web search budget was exhausted.

### Cited Findings
- `gabotechs/datafusion-gpu` (2024-12-04 to 2025-05-30, no license): "showcase the capabilities of wiring up Apache Datafusion with GPU execution runtimes". It compares CubeCL ("Bad performance … got my laptop bricked several times") with cudarc and hand-written CUDA for one aggregation. — [gabotechs/datafusion-gpu](https://github.com/gabotechs/datafusion-gpu)
- `mbrobbel/cudf-rs` "Rust bindings for cuDF" (8 stars, last push 2026-01-28). `0xDaizz/cudf-rs` "Unofficial Rust bindings for NVIDIA libcudf" (1 star, 2026-04-11). — [mbrobbel/cudf-rs](https://github.com/mbrobbel/cudf-rs); [0xDaizz/cudf-rs](https://github.com/0xDaizz/cudf-rs)
- `vyncint/oxidelake` (1 star, pushed 2026-10-27): "Arrow-native SQL engine in Rust: embedded DataFusion or distributed Ballista, with optional CUDA/Metal acceleration and per-batch CPU fallback." — [vyncint/oxidelake](https://github.com/vyncint/oxidelake)
- `ryrobes/rvbbit-sql` (7 stars) is a Postgres-based engine that "learns to route queries across native, DuckDB, DataFusion, and GPU". It is tangential. — [ryrobes/rvbbit-sql](https://github.com/ryrobes/rvbbit-sql)
- nevi-me (in libcudf-rs #49): "I started an implementation recently using cuda-oxide for the arrow::compute kernels + datafusion". It is not public as far as I could find. — [libcudf-rs #49](https://github.com/gabotechs/libcudf-rs/issues/49)

### Inferences
- The field of serious "GPU plugin for DataFusion" efforts comes down to libcudf-rs (Datadog-backed, per-node rule, DF 55) and Algeon (subtree lowering to its own IR, DF 55.1, broadest coverage). GQE is a separate kind of thing: a standalone NVIDIA reference engine with DataFusion as the SQL front end only. All three use cuDF. None does spilling or cross-query caching, and only GQE does multi-GPU.
- Version note: every active peer is on DataFusion 52–55 (GQE 52.3; libcudf-rs 55; Algeon 55.1; ArrowMetal 55.1). peacockdb's DataFusion 45 is about 10 majors behind, which would matter for any plugin story.

### Gaps
- Voltron Data Theseus, LanceDB GPU, Ballista GPU, Sirius (DuckDB, not DataFusion), and any HN/Reddit launch threads were **not checked** because the web search quota was exhausted. These should be revisited with web search.
- Projects without "datafusion" + "gpu/cuda/cudf" in their repo name or description would have been missed by GitHub repo search.
