# datafusion-plugin (+datafusion-log-gpu) — a GPU execution plugin for DataFusion-based systems, log stores first

## Verdict

- **What it is:** a Rust crate that a DataFusion host compiles in. A `PhysicalOptimizerRule` cuts out the largest subtrees peacockdb can run, swaps each for one `PeacockGpuExec` node that runs the subtree on cuDF, and hands Arrow back. The first niche is log stores (OpenObserve, Parseable, GreptimeDB, InfluxDB 3): aggregations and `LIKE`/regex over a VRAM-pinned window of the last few hours.
- **Who needs it:** demand.md rates it **weak**. There is one user ask (Beacon, 0 replies) and four builders, who are peers, not buyers. No log-store user or vendor has asked for a GPU [D][LN].
- **Budget:** it fits (about 0.55× on peacockdb, about 0.6× on Sirius). But two DataFusion GPU plugins already exist: Algeon and Datadog-backed libcudf-rs. Both are on DataFusion 55 and further along. GQE is NVIDIA's own DataFusion → libcudf engine [PE][N].

## Who needs it badly

- **Rating: weak**, from demand.md `## Ratings`, row `datafusion-plugin (+datafusion-log-gpu)` [D]. demand.md's reason: "Peers, not buyers. Algeon, datafusion-arrowmetal and GQE fill the slot already."
- **Robin Kooyman (`robinskil`, MARIS Beacon)** is the only downstream asker. He wants to "Investigate integrating libcudf-rs as an optional GPU execution backend": feature-flagged, with the CPU as fallback. Workloads named: large Parquet/Arrow IPC/CSV scans, filters, aggregation, sort ([beacon #231](https://github.com/maris-development/beacon/issues/231), 2026-06-04, 0 comments, accessed 2026-10-29).
  - Beacon is an AGPL climate data lake over Zarr, NetCDF and Parquet, with custom `TableProvider`s. It is on DataFusion 54.1 and already depends on `datafusion-ffi ^54.1` ([Cargo.toml L76-79 @a612fcec](https://github.com/maris-development/beacon/blob/a612fcec6c8493902c7b8148e51c4316d34ac6db/Cargo.toml#L76), accessed 2026-10-29) [VS].
- **Gene Bordegaray (Datadog) and Gabriel (`gabotechs`, Distributed DataFusion author)** build and profile libcudf-rs. They found the GPU is "bottlenecked by runtime orchestration and host/device transfer overhead rather than raw GPU kernel time" ([libcudf-rs #49](https://github.com/gabotechs/libcudf-rs/issues/49), 2026-05-05, accessed 2026-10-29). They are builders: partners or competitors [D][PE].
- **Morris (`morristai`, Algeon)** builds a DataFusion + cuDF engine. His upstream ask is to make `ParquetFileReaderFactory` downcastable so a GPU rule can swap the reader ([datafusion #26633](https://github.com/apache/datafusion/issues/26633), 2026-10-23, accessed 2026-10-29). He too is a peer.
- **Log users with real pain do not run DataFusion:**
  - `fmoralesh` has 1.2 TB of logs; his old DB crashed at 25 GB, and he now uses cuDF on a V100 ([r/dataengineering](https://www.reddit.com/r/dataengineering/comments/1kw9ku2/comment/mugm06f/), 2025-05-27, accessed 2026-10-29).
  - A Graphistry customer ran SSD arrays → GDS → 8×A100 for log analytics ([HN 47109102](https://news.ycombinator.com/item?id=47109102), 2026-02-22, accessed 2026-10-29).
  - Neither is a DataFusion log lake [D].
- **Log-store communities ask for nothing GPU-related.** `gh search issues GPU` found zero relevant hits in openobserve, parseable, greptimedb, quickwit and influxdb (2026-10-29) [LN]. Vendor forums (Grafana, Influx, Elastic, OpenSearch) had zero first-person GPU query asks ([sources/discord-forums.md](../sources/discord-forums.md)).
- **Their performance complaints are not about scan compute:**
  - metadata and file lists ([openobserve #8864](https://github.com/openobserve/openobserve/issues/8864));
  - tiny files ([#14846](https://github.com/openobserve/openobserve/issues/14846));
  - the labels API ([greptimedb #6980](https://github.com/GreptimeTeam/greptimedb/issues/6980));
  - Flight under concurrency ([greptimedb #7939](https://github.com/GreptimeTeam/greptimedb/issues/7939));
  - ORDER BY ([parseable #430](https://github.com/parseablehq/parseable/issues/430)).
  - All accessed 2026-10-29 [LN]. A GPU fixes none of these.
- **Best first conversations:**
  1. Kooyman (Beacon): the only one asking.
  2. Parseable, which has a public hook for plugin optimizer rules (below).
  3. Bordegaray/gabotechs: to learn whether Datadog wants a partner or already has its answer.

## How it would work

- **Shape:** a crate `peacockdb-datafusion` with `install_on(SessionStateBuilder)`, the pattern Algeon uses [PE]. It adds:
  - a `PhysicalOptimizerRule` that walks the host's physical plan bottom-up and marks nodes the translator accepts;
  - a replacement of each maximal accepted subtree with `PeacockGpuExec`, an `ExecutionPlan` that holds the FlatBuffers recipe and streams Arrow `RecordBatch`es out;
  - nothing for unaccepted nodes, which stay on DataFusion's CPU. The host's own executor is the fallback, which peacockdb lacks today [I].
- **What changes in peacockdb:** today the translator takes the whole plan and errors on unknown node kinds [I]. It must become "translate this subtree, with these leaves". Each leaf is one of:
  - **a Parquet scan peacockdb can read itself** (a `DataSourceExec` with a default reader). The GPU reads the files: the fast path;
  - **any other host `ExecutionPlan`** (OpenObserve's Tantivy-pruned scan, Beacon's Zarr reader). A new source node uploads its Arrow batches through pinned buffers: the slow path;
  - **a hot-window cache hit:** columns already resident in VRAM.
- **Hot window (the log niche):** a VRAM cache keyed by (file or row group, column), filled as new Parquet files flush from the host's WAL/memtable. It evicts by event time (oldest first), not LRU. Only filter/group columns are kept decoded. Message bodies stay dictionary/zstd-compressed and are decoded on demand (estimate).
- **Data path, end to end:**
  - ingest → host WAL/memtable in host RAM → Parquet flushed to local disk or S3;
  - an async copy of each new file → pinned host → GPU decode → VRAM window;
  - query → host planner → rule → `PeacockGpuExec` scans the window in HBM → aggregates → small Arrow result → host.
  - Queries outside the window stay on the host's CPU path, or go through the slow upload path when cost-gated.
- **Cost gate:** offload only when the subtree's input sits in VRAM, or is GPU-readable Parquet above a row threshold. libcudf-rs #49's commenter measured a 64K–1M-row crossover, and "GPU islands" feeding CPU consumers came out at about 0.55× [PE].

## The human's questions

**"Data Fusion plugin, to work with other data fusion systems."**

**1. How exactly would it plug in?** Four ways exist. Only one fits peacockdb.

| Mechanism | Who uses it | Fit for peacockdb |
|---|---|---|
| `PhysicalOptimizerRule` that replaces subtrees with a custom `ExecutionPlan` | libcudf-rs (three rules, per-node swap of Filter/Projection/Sort/HashJoin/Aggregate) ([session_state_builder_ext.rs @a6739ff](https://github.com/gabotechs/libcudf-rs/blob/a6739ff058723e2d9d80b9abc7ee3d787bc07fd3/libcudf-datafusion/src/planner/session_state_builder_ext.rs)); Algeon (subtree → own IR, [native-lowering.mdx @73dadb7](https://github.com/algeon-dev/algeon/blob/73dadb70b076d6fa33b11e244c31fbdc1542b966/website/docs/design/native-lowering.mdx)); datafusion-arrowmetal ([crates.io](https://crates.io/crates/datafusion-arrowmetal)) | **Best.** peacockdb already translates DataFusion physical plans node by node [I]. Whole subtrees avoid libcudf-rs's GPU↔CPU ping-pong (in q19 the hash join and global aggregate bounce back to the CPU [PE]). |
| Custom `TableProvider` | Beacon, OpenObserve, Parseable (their own scans) [VS] | **Only for the cache.** A provider returning GPU-computed batches can push down filters, but not aggregates or joins. It is useful for exposing the hot window as a table, not as the engine hook. |
| Substrait hand-off | GQE: DataFusion LogicalPlan → `to_substrait_plan` → Flight → C++ `from_substrait` ([client.rs @50facc4](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/rust/gqe-cli/src/client.rs)); Sirius (DuckDB dialect) [S] | **Worse.** It is logical, so the host's physical choices (pruned files, Tantivy row selections) are lost. DF45→Sirius has silent wrong answers (LIMIT→0, #1963) [S]. Useful only as a portable cross-version format. |
| Comet-style serialized native plan | Comet: `CometExecRule` → protobuf `operator.proto` → JNI `createPlan` → Rust `planner.rs` ([CometExecRule.scala @7f73d86](https://github.com/apache/datafusion-comet/blob/7f73d86fb841f5662f1343cf52b6e6b31eb17dfb/spark/src/main/scala/org/apache/comet/rules/CometExecRule.scala)) | **peacockdb already is this**, inside the rule: DataFusion plan → FlatBuffers recipe → C ABI → cuDF. Comet's FAQ says it is CPU-only and sends GPU users to cudf-spark ([comet PR #6783](https://github.com/apache/datafusion-comet/pull/6783)). |

- So "plugin" means **a Rust crate compiled into the host, against the host's exact DataFusion**. None of the targets loads plugins at runtime [VS]:
  - **Parseable** has a public hook: `ADDITIONAL_PHYSICAL_OPTIMIZER_RULES`, "registered by enterprise/plugins" ([src/query/mod.rs L96 @bbbb79ec](https://github.com/parseablehq/parseable/blob/bbbb79ec52ecd023e5654f9fc5cf6c2f27b85898/src/query/mod.rs#L96)). Integration is a few lines plus a rebuild.
  - **OpenObserve** builds its rule list and its own QueryPlanner in `exec.rs`. A patch is needed ([exec.rs L202 @f8827251](https://github.com/openobserve/openobserve/blob/f88272514512336c2f24d5e8d7b00735094ae439/src/search/src/datafusion/exec.rs#L202)), and its enterprise code is private.
  - **GreptimeDB** has compile-time plugin hooks ([src/plugins @d33c6641](https://github.com/GreptimeTeam/greptimedb/blob/d33c66418379463c5e7c928f3a132517d2a1fa9b/src/plugins/src/lib.rs)).
  - All accessed 2026-10-29.
- **`datafusion-ffi` does not rescue this.** It has `FFI_ExecutionPlan`, `FFI_PhysicalOptimizerRule` (54+) and `FFI_QueryPlanner` (55.1+). But host plans cross it as `Foreign*` handles that cannot be downcast to `AggregateExec` or `HashJoinExec`, the 56 guide says so, and peacockdb's translator lives on downcasts. The boundary broke in 49, 50, 52, 54 and 55, and [#17374 "Stabilize FFI Boundary"](https://github.com/apache/datafusion/issues/17374) is open (accessed 2026-10-29) [VS].

**2. What does version skew cost?** peacockdb is on DataFusion 45 (`peacockdb-core/Cargo.toml:21` at master `9d61cac8`). The latest is 55.2.0, released 2026-10-07 ([crates.io](https://crates.io/api/v1/crates/datafusion/versions), accessed 2026-10-29) [VS].

| Target | DataFusion at HEAD (Cargo.lock) | Fork? | SHA |
|---|---|---|---|
| OpenObserve | 55.1.0 | no | f8827251 |
| Parseable | 55.0.0 | no | bbbb79ec |
| GreptimeDB | 55.1.0 | yes, [24 commits ahead](https://github.com/apache/datafusion/compare/55.1.0...GreptimeTeam:datafusion:cc5534492d5408831395609cdafb6520c7d514b7) | d33c6641 |
| InfluxDB 3 | 52.5.0 | yes, [12 ahead](https://github.com/apache/datafusion/compare/52.5.0...influxdata:arrow-datafusion:e9e2e6d36bf63e39f746f82ee9b33c47feb65361) | 185688a5 |
| HoraeDB | 43.0.0, repo archived | no | 8f9698cd |
| Quickwit (Datadog) | none: DataFusion added in [#6276](https://github.com/quickwit-oss/quickwit/pull/6276) (2026-04), removed in [#6809](https://github.com/quickwit-oss/quickwit/pull/6809) (2026-09) | — | 30938ed9 |
| MARIS Beacon | 54.1.0 | no | a612fcec |
| Comet / Spice AI / Lance | 55.2.0 (Spice: fork, 454 ahead) | — | 7f73d86f / e1460121 / 06315516 |
| GQE / libcudf-rs / Algeon | 52.5 / 55 / 55.1 | no | 50facc46 / a6739ff0 / 73dadb70 |

- **Cadence:** 11 majors (45→55) shipped between 2025-02-07 and 2026-08-18, one every 6–10 weeks. The upgrade guides grew from 310 lines (46) to 1,534 (55) ([upgrading/ @14aa185d](https://github.com/apache/datafusion/tree/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading), accessed 2026-10-29) [VS].
- **Breaks that hit a plan translator:**
  - 46–48: `ParquetExec` removed for `DataSourceExec` + `FileSource`. This is exactly what `scan_mapping/` matches.
  - 49–52: `SchemaAdapter` removed.
  - 53: `ExecutionPlan::statistics` removed; `properties()` returns `&Arc<PlanProperties>`.
  - 54: `as_any` removed from `ExecutionPlan`/`PhysicalExpr`/`TableProvider`, so the downcast idiom changes.
  - 55: `apply_expressions` required, `replace_children`, a new statistics API, and planner traits moved to `datafusion-session` [VS].
- **Cost, one-off:** port 45→55 in the translator (2,952 lines), the CPU backend (2,086) and the goldens. Estimate 1.5–3k lines touched, plus golden churn. This also may fix q27/q72, which do not plan on DF45 (#23) [I] (estimate).
- **Cost, ongoing:** one build per host major and per fork rev. To cover OpenObserve + Parseable + Greptime + Influx you need 55.0, 55.1, the Greptime fork and the 52.5 Influx fork. Each new major is roughly 200–600 lines of churn every 6–10 weeks (estimate). Pick one host, and ride its pin.

**3. Against NVIDIA's GQE** ([rapidsai/gqe @50facc46](https://github.com/rapidsai/gqe/tree/50facc46fb99ecf4a955697e78803707335814b3), accessed 2026-10-29) [PE][N]:
- **GQE is not a plugin.** It is a standalone C++/cuDF server: a node manager plus a task manager per GPU, NVSHMEM between GPUs, and an Arrow Flight service. DataFusion 52.3 is used only client-side, to turn SQL into Substrait. No DataFusion app can embed it.
- **Coverage:** `from_substrait` accepts read, project, join, fetch, filter, sort, aggregate and set. Subqueries are unimplemented, and there is no CPU fallback.
- **What GQE has that peacockdb lacks:** nvCOMP-compressed in-GPU tables loaded by `COPY`, zone maps, a chunked Parquet reader, and multi-GPU.
- **Claim:** TPC-H SF1000 in 9.15 s on one GPU (model not named).
- **Status:** one squashed commit (2026-08-10), no activity since 2026-08-11. A LIKE bug ([gqe #4](https://github.com/rapidsai/gqe/issues/4)) has no reply.
- **Licence:** Apache-2.0, but it needs nvCOMP (NVIDIA SDK licence), NVSHMEM and CUPTI.
- **Verdict:** GQE is a blueprint and a ceiling. It proves "DataFusion + libcudf" is no moat [N]. peacockdb's edge over it is the embeddable, physical-plan seam. That is exactly what libcudf-rs and Algeon already have.

**4. Against the Datadog engineer's libcudf-rs** ([@a6739ff](https://github.com/gabotechs/libcudf-rs/tree/a6739ff058723e2d9d80b9abc7ee3d787bc07fd3), accessed 2026-10-29) [PE]:
- **Design:** cxx bindings plus `libcudf-datafusion` on DataFusion 55 and cuDF 26.08. 181 commits; about 40 PRs by Bordegaray and 13 commits by `notfilippo` (both Datadog).
- **How it plugs in:** `with_cudf_planner()` forces `target_partitions=1` and registers three rules that swap single nodes for `CuDF*Exec`, with `CuDFLoadExec`/`CuDFUnloadExec` at the boundaries.
- **GPU columns travel inside ordinary RecordBatches** via `unsafe impl Array for CuDFColumnView`. arrow-rs considers that unsupported ([arrow-rs #9092](https://github.com/apache/arrow-rs/pull/9092), accessed 2026-10-29).
- **What #49 measured:**
  - 6.78 s of CUDA runtime API time against 0.54 s of kernel time.
  - RMM pooling cut 550,984 alloc/free calls (3.54 s) to 16 calls (9 ms).
  - q19 at SF1: 736 batches became 24, and 1,362 ms became 525 ms.
  - On a T4, all 22 TPC-H SF10 queries took 97.8 s in total ([#50](https://github.com/gabotechs/libcudf-rs/issues/50)).
- **The #49 commenter (RTX 5080, PCIe5):**
  - pageable H2D about 16 GB/s, pinned about 46 GB/s;
  - TPC-H q6 at SF20: 0.13× the CPU with CPU decode then upload, but 1.18× with GPU-native Parquet. "The gap is architectural rather than orchestration."
- **Against peacockdb:**
  - peacockdb already has what #49 asks for: big budgeted batches, whole subtrees on the device, a GPU Parquet scan, and a stateful join session [I].
  - libcudf-rs has what peacockdb lacks: DF55, a host CPU fallback, and Datadog engineers.
  - Neither caches across queries.
- **Algeon is the stronger rival.** It runs DataFusion 55.1 with subtree lowering, KvikIO Parquet (local and S3), admission control and an embeddable crate.
  - TPC-H SF100: 22/22 on the GPU, 13.9 s against 47.9 s on the CPU (3.45×).
  - TPC-DS SF100: 99/99, 51.1 s against 169.6 s.
  - Hardware: one RTX PRO 6000 96 GB, no cross-query cache ([benchmarks @73dadb7](https://github.com/algeon-dev/algeon/blob/73dadb70b076d6fa33b11e244c31fbdc1542b966/website/docs/reference/benchmarks/index.mdx), accessed 2026-10-29) [PE].
  - peacockdb is at 82/99 TPC-DS, with only 3 engine timings [I].

**5. The log niche: sizes, query shapes, and whether the window fits.**
- **Size:** OpenObserve's own billion-log benchmark used 1B Kubernetes log rows, 2,199 GB of raw NDJSON and about 4 h 13 m of event time, which is about 12.5 TB/day. In Parquet + zstd with indexes it took 673.5 GB: **3.26×**, not the 10–30× of folklore ([OpenObserve blog](https://openobserve.ai/blog/openobserve-vs-clickhouse-one-billion-logs-benchmark.md), 2026-08-22, vendor, accessed 2026-10-29) [LN].
- **Does it fit?** Usable VRAM is about 60 GB on an H100 and about 110 GB on an H200 (estimate; decoded ≈ raw/1.5, unverified) [LN]:

```
ingest/day   H100 compressed @3.26×   H100 decoded   H200 compressed   H200 decoded
100 GB       ~47 h                    ~22 h          ~86 h             ~40 h
1 TB         ~4.7 h                   ~2.2 h         ~8.6 h            ~4 h
10 TB        ~28 min                  ~13 min        ~52 min           ~24 min
```

  - Yes for the last few hours at ≤1 TB/day, which covers dashboard and alert windows. It holds minutes at 10 TB/day. Seven- to thirty-day searches never fit.
- **Query shapes:**
  - **Full-text search is already indexed.** OpenObserve uses a Tantivy inverted index, on by default. Greptime has a bloom (default) or Tantivy fulltext index ([column_schema.rs L786 @d33c6641](https://github.com/GreptimeTeam/greptimedb/blob/d33c66418379463c5e7c928f3a132517d2a1fa9b/src/datatypes/src/schema/column_schema.rs#L786)). Quickwit is Tantivy.
  - **Indexed lookups are already fast.** In the benchmark over 1B rows, token search, trace_id and span_id lookups took **24–27 ms**. A GPU adds nothing there.
  - **Aggregations are the slow cases:** top-N namespaces 1.2–1.5 s, filtered histogram 0.6 s [LN].
  - **So the GPU's work is aggregations, plus the predicates the index cannot serve.** OpenObserve's docs say `LIKE '%x%'`, regex and `body = 'x'` fall back to a DataFusion scan ([Tantivy docs](https://openobserve.ai/docs/user-guide/advanced/query-tuning/tantivy-index/), accessed 2026-10-29).
- **String speed:** libcudf's stock strings API reached about 40 GB/s on an A6000; hand-written kernels reached 320–340 GB/s ([NVIDIA blog](https://developer.nvidia.com/blog/mastering-string-transformations-in-rapids-libcudf/), accessed 2026-10-29). At 40 GB/s, a brute-force `LIKE` over a 60 GB window takes about 1.5 s (estimate). There is no sourced figure for cuDF regex or for a CPU baseline [LN].

## Data path and cache

- **Bandwidths:**
  - H200, measured by the human: pageable H2D 10.6 GB/s, pinned 47.3 GB/s, device read 4,484 GB/s [I].
  - The #49 commenter's PCIe5 RTX 5080: 16 GB/s pageable, 46 GB/s pinned [PE].
  - S3, per instance: up to about 12.5 GB/s (100 Gb/s NIC, spec value, unverified) [LN].
  - Local NVMe: a few GB/s per drive (estimate).
  - Parquet decode on the GPU: 6.8–8.7 GB/s (H200, `read_parquet`). The chunked reader is about 5.5× faster on GB10 (61 ms against 340 ms for q6) [I].
- **Thesis test: holds, and the DataFusion evidence is the sharpest of any option.**
  - libcudf-rs, uncached: runtime API time is 12× kernel time [PE].
  - Upload-from-CPU-decode: q6 at 0.13× the CPU, against 1.18× with GPU-native reads [PE].
  - Arrowmetal, even with zero-copy unified memory, leaves filters and top-k on the CPU (0.14×–0.79×) ([crates.io](https://crates.io/crates/datafusion-arrowmetal), accessed 2026-10-29).
  - peacockdb's own load is about 20× execute on H200 [I], and the human's PoC loaded longer than it computed on 11 of 12 SF20 queries [E].
- **Why the log case is worse than TPC-H:**
  - Hosts often hand the plugin Arrow from their own scans (Tantivy-pruned, custom formats), so the batches arrive host-side and must cross PCIe.
  - Their CPU caches already exceed VRAM:
    - OpenObserve's memory cache defaults to 50% of RAM in cluster mode, with a disk cache up to 500 GB ([config.rs @2f3d893b](https://github.com/openobserve/openobserve/blob/2f3d893b56015f4598f8860f9d2e586666639266/src/config/src/config.rs#L3390));
    - InfluxDB 3 keeps a Parquet file cache of 20% of RAM with a recency window ([serve_all.txt @185688a5](https://github.com/influxdata/influxdb/blob/185688a5a813016c0d26ec10abbad2ca770670ed/influxdb3/src/help/serve_all.txt));
    - Parseable has a disk hot tier of at least 10 GiB per stream ([hottier.rs @bbbb79ec](https://github.com/parseablehq/parseable/blob/bbbb79ec52ecd023e5654f9fc5cf6c2f27b85898/src/hottier.rs#L244)).
    - All accessed 2026-10-29 [LN].
  - So the GPU does not save IO. It only saves compute on data already in RAM.
- **Cache needed:**
  1. A VRAM hot window filled asynchronously at flush time, never on the query path.
  2. Pinned host staging for the memtable tail (minutes of data not yet flushed).
  3. A cost gate that keeps cold, out-of-window queries on the CPU.
- **With the window, a full scan is HBM-bound:** 60 GB in about 15–20 ms ideal; about 1.5 s for `LIKE` at stock libcudf string speed (estimate).
- **Without the window,** reloading 60 GB over pinned PCIe takes about 1.3 s, and GPU decode at 7–9 GB/s about 7–9 s. That loses to the host's RAM-cached CPU path (estimate).

## Build cost to alpha

Sizes are in yardstick lines; the unit is 22,830 [I]. Every figure is an estimate. The alpha has one host (Parseable or OpenObserve), one GPU, a hot window, and aggregations plus LIKE/regex.

| Component | On peacockdb: reuse/adapt/new, size | On Sirius: reuse/adapt/new, size |
|---|---|---|
| DataFusion 45 → host's 55.x | adapt translator, CPU backend, goldens: 1.5–3k | n/a: Sirius has no DataFusion side; the emitter below targets 55 directly |
| Plan hand-off | reuse node set, wire, C ABI, driver [I] | new: DF55 physical → DuckDB-dialect Substrait emitter with fixes (LIMIT→0, DISTINCT), 3–7k [S]; DuckDB re-optimizes the plan |
| Subtree rule + `PeacockGpuExec` (partial plans, leaves, cost gate) | new: 1.5–2.5k | new: 1.5–2.5k, plus Rust bindings for `Fragment` (unbound today, one context per process) 0.5–1k [S] |
| Arrow-upload source node (host scans → GPU, pinned) | new: 0.4–0.8k (Rust + C++) | adapt: `Fragment` declared input streams [S]: 0.3k |
| Scan: chunked reader, pinned buffers (#232, #149), host object store / file list | adapt: 0.8–1.5k | reuse cuDF Parquet over kvikIO/S3 [S]; adapt to host file lists: 0.5k |
| Hot-window cache (event-time eviction, async fill at flush, invalidation on compaction) | new: 2–4k | adapt `pin_table` into time-evicted auto-fill: 1.5–3k [S] |
| Log operators: LIKE/regex, `date_trunc`/histogram, approx percentile, top-k, JSON field extract | new: 1–2k | adapt: about 1k; window functions are not wired (#1745) [S] |
| Shared GPU session across concurrent queries, RMM pool (#148) | new: 1–1.5k | adapt: one query at a time per `Context` [S]: 1–1.5k |
| Host integration patch + build per DataFusion pin | new: 0.3–0.6k | new: 0.3–0.6k |
| Differential tests against the host's CPU path | adapt corpus harness: 0.5–1k | new: 1k |
| **Total** | **~9–17k** | **~10–18k** |

Total new work: 0.55× engine on peacockdb, 0.6× on Sirius (estimate)

- **The hard part: winning on the host's own scan.**
  - Each log store has its own scan: OpenObserve's file list plus Tantivy row selection, Greptime's mito regions, Influx's gen1 files.
  - Taking over that scan is host-specific work. Accepting its Arrow output instead puts PCIe back on the query path, which loses (0.13× above).
  - Getting window fill and invalidation right against the host's compaction is the core of the work.
- **The second hard part: DataFusion churn.** A 10-major port now, then a major every 6–10 weeks, per host and per fork.
- **On Sirius,** the Substrait road throws away the host's physical choices, and DuckDB re-plans. It also brings silent wrong answers [S]. Sirius is plausible, but worse than peacockdb here, because peacockdb's seam is already DataFusion's physical plan.

## Adoption friction

- **For a host vendor:** low to medium. It is one crate and a few lines (Parseable's hook), with CPU fallback for free.
- **For end users of a log store:** high. They cannot add it themselves: it has to be compiled in at the host's exact DataFusion pin, and no host loads `.so` plugins [VS]. The adopter is therefore the vendor (OpenObserve Enterprise, Parseable, Greptime), not an ops team.
- **Hardware:** log stores run on CPU fleets, sized by RAM and disk. Asking for an H100 per query node is a new line in the bill, for a speed-up on top-N panels that already take 0.6–1.5 s [LN].
- **Beacon** is the easy case: it already uses `datafusion-ffi` and asks for an optional backend. But it is a scientific server with Zarr/NetCDF sources, so every scan is an upload (slow path) [PE].

## Platform risk

- **NVIDIA: medium** (nvidia.md, `## Per direction`, datafusion-plugin). There is no NVIDIA or upstream DataFusion GPU product, but GQE "proves the design is no moat and could be promoted or forked at any time" [N].
- **Incumbents shipping the same thing: high.**
  - **Algeon** is the same product: DF 55.1, subtree lowering, 99/99 TPC-DS, embeddable, Apache-2.0 [PE].
  - **libcudf-rs** has Datadog engineers on it.
  - Two near-identical open plugins exist, both already on the hosts' DataFusion version.
- **Host projects:** a log vendor that wants a GPU path is most likely to adopt libcudf-rs or Algeon, or do it in-house. Datadog already put DataFusion into Quickwit and removed it within five months [VS].
- **Upstream:** there is no device-aware batch type. arrow-rs #10752 (C Device Data Interface FFI) is open, and its PR was closed unmerged [PE]. Any plugin rides unsupported tricks or copies at every boundary.

## Funding story

- **A design partner would say** (estimate): "We'd try it behind a flag if it makes our top-N dashboards 5× faster without a new box per node. Our searches are indexed and already fast." That is a feature request, not a pain.
- **What the round would be raised on:** "the GPU backend for the DataFusion ecosystem", a plumbing layer for some 20 Rust databases. Investors will ask why not Algeon, why not libcudf-rs, and why not GQE. There is no named buyer, and no log vendor has asked [D][LN].
- **Strongest possible version:** one log vendor (Parseable has the hook; OpenObserve publishes benchmarks) co-builds a GPU tier for an enterprise SKU. Nothing suggests one wants it.

## Evidence against

- **No buyer.** One user ask, with 0 replies, against four builders. Zero GPU issues in five log-store trackers [D][LN].
- **The slot is filled twice over:** Algeon (99/99 TPC-DS, 3.45× TPC-H SF100) and libcudf-rs (Datadog), both on DF55. peacockdb starts on DF45 with 82/99 [PE][I].
- **NVIDIA wrote the reference design** (GQE) and gives it away [N].
- **The log niche is index-first.** The common queries (token, trace_id) return in about 25 ms on a CPU. What is left is 0.6–1.5 s aggregations, where a tuned CPU and bigger RAM caches are the cheap fix [LN].
- **Logs compress poorly** (3.26×), so VRAM holds hours at 1 TB/day and minutes at 10 TB/day [LN].
- **Every DataFusion GPU measurement says transfers dominate** unless reads are GPU-native and data stays resident. Hosts hand over Arrow from CPU scans [PE].
- **"Plugin" is a fiction at the ABI level.** It needs a build per host pin and per fork, every 6–10 weeks [VS].
- **For it:**
  - peacockdb's seam (a DataFusion physical plan → whole-subtree recipe) is the right architecture, and better than libcudf-rs's per-node swap.
  - Nobody (GQE excepted) caches across queries, and the hot window is the one differentiator [PE].
  - It is also the cheapest option here to reach an alpha.

## Ratings

- **Who needs it badly — weak:** demand.md's row. Peers, not buyers; log users don't run DataFusion [D].
- **Build cost — strong:** about 0.55× on peacockdb fits easily. The seam already exists; the hard part is the host's scan and the DF upgrade.
- **Data path — medium:** it works only for hot, resident windows (hours at ≤1 TB/day). Uncached, transfers lose to CPU [PE][LN].
- **Adoption friction — medium:** one crate for a vendor, but impossible for end users, and it needs GPUs in CPU fleets.
- **Platform risk — weak:** Algeon and libcudf-rs ship the same plugin on DF55; NVIDIA's GQE is the blueprint [PE][N].
- **Funding story — weak:** "GPU plumbing for DataFusion" has no named buyer, and two free rivals.

## Follow-up experiments

1. **DF45 → 55 spike** on the translator only: count lines touched and goldens changed. This settles the 1.5–3k estimate (2–3 days).
2. **OpenObserve's billion-log benchmark** (top-N, filtered histogram) on a GPU-resident 1–4 h window against OpenObserve on the same box. If the speed-up is under 5×, drop the log niche.
3. **cuDF `like`/`contains_re` GB/s on real log bodies** (H200, GB10), against the CPU's DataFusion `LIKE`/regex on 32–64 cores.
4. **Upload path against native read** for a host-supplied Arrow scan: reproduce the 0.13× vs 1.18× on peacockdb with pinned buffers.
5. **Run Algeon** on the same TPC-H SF100 box against peacockdb's engine path. If Algeon is equal or ahead, a peacockdb plugin has no technical edge left.
6. **Ask Kooyman, Parseable and Bordegaray** for working-set sizes, query mix and whether they would run GPUs at all.

## Sources

- [D] demand.md, `## Ratings`, `## First conversations`, `## Evidence against` (local; accessed 2026-10-29)
- [E] evidence-in-hand.md (local; accessed 2026-10-29); [I] inventory.md at master f25c250d; peacockdb DataFusion pin `peacockdb-core/Cargo.toml:21` at master 9d61cac8 (local; accessed 2026-10-29)
- [S] sirius.md, `## Substrait`, `## API surface`, `## Sirius components` (local; Sirius @b9e05ffc; accessed 2026-10-29); [N] nvidia.md, `## Open-source projects`, `## Per direction` (local; accessed 2026-10-29)
- [VS] research notes, version skew: `../notes/datafusion-gpu-plugin-research/version_skew.md` (accessed 2026-10-29)
- [PE] research notes, peers: same folder, `peers.md` (accessed 2026-10-29); [LN] research notes, log niche: same folder, `log_niche.md` (accessed 2026-10-29)
- sources/issue-trackers.md, sources/reddit.md, sources/hn-stackexchange.md, sources/discord-forums.md (local; accessed 2026-10-29)
- GQE: https://github.com/rapidsai/gqe/tree/50facc46fb99ecf4a955697e78803707335814b3, https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/rust/gqe-cli/Cargo.toml#L35, https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/src/logical/from_substrait.cpp, https://github.com/rapidsai/gqe/issues/2, https://github.com/rapidsai/gqe/issues/4 (accessed 2026-10-29)
- libcudf-rs: https://github.com/gabotechs/libcudf-rs/tree/a6739ff058723e2d9d80b9abc7ee3d787bc07fd3, https://github.com/gabotechs/libcudf-rs/issues/49, https://github.com/gabotechs/libcudf-rs/issues/50, https://github.com/gabotechs/libcudf-rs/issues/82, https://github.com/gabotechs/libcudf-rs/pull/92, https://github.com/gabotechs/libcudf-rs/pull/93 (accessed 2026-10-29)
- Algeon: https://github.com/algeon-dev/algeon/tree/73dadb70b076d6fa33b11e244c31fbdc1542b966, https://github.com/algeon-dev/algeon/blob/73dadb70b076d6fa33b11e244c31fbdc1542b966/website/docs/reference/benchmarks/index.mdx, https://github.com/algeon-dev/algeon/blob/73dadb70b076d6fa33b11e244c31fbdc1542b966/website/docs/reference/roadmap.md, https://github.com/apache/datafusion/issues/26633, https://github.com/apache/datafusion/pull/26635 (accessed 2026-10-29)
- arrowmetal and upstream: https://crates.io/crates/datafusion-arrowmetal, https://github.com/apache/datafusion/pull/26004, https://github.com/apache/arrow-rs/pull/9092, https://github.com/apache/arrow-rs/issues/10752, https://github.com/apache/datafusion-python/issues/936 (accessed 2026-10-29)
- Comet: https://github.com/apache/datafusion-comet/blob/7f73d86fb841f5662f1343cf52b6e6b31eb17dfb/spark/src/main/scala/org/apache/comet/rules/CometExecRule.scala, https://github.com/apache/datafusion-comet/blob/7f73d86fb841f5662f1343cf52b6e6b31eb17dfb/native/proto/src/proto/operator.proto, https://github.com/apache/datafusion-comet/pull/6783 (accessed 2026-10-29)
- DataFusion releases, upgrade guides and FFI: https://crates.io/api/v1/crates/datafusion/versions, https://github.com/apache/datafusion/tree/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading, https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/datafusion/ffi/README.md, https://github.com/apache/datafusion/issues/17374, https://github.com/apache/datafusion/commit/af7904f635 (accessed 2026-10-29)
- Host pins: https://github.com/openobserve/openobserve/blob/f88272514512336c2f24d5e8d7b00735094ae439/Cargo.toml#L438, https://github.com/parseablehq/parseable/blob/bbbb79ec52ecd023e5654f9fc5cf6c2f27b85898/Cargo.toml#L22, https://github.com/GreptimeTeam/greptimedb/blob/d33c66418379463c5e7c928f3a132517d2a1fa9b/Cargo.toml#L134, https://github.com/influxdata/influxdb/blob/185688a5a813016c0d26ec10abbad2ca770670ed/Cargo.toml#L167, https://github.com/apache/horaedb/blob/8f9698cdf83d02e12b93581464114d4e7cef483a/Cargo.toml#L53, https://github.com/quickwit-oss/quickwit/blob/30938ed9ce48e09a802ec9a316e0c1e8ab46aea7/quickwit/Cargo.toml, https://github.com/maris-development/beacon/blob/a612fcec6c8493902c7b8148e51c4316d34ac6db/Cargo.toml#L76, https://github.com/spiceai/spiceai/blob/e146012130aaf6fbf71ef9563078834a08f207d3/Cargo.toml#L254, https://github.com/lancedb/lance/blob/0631551696d4c89f3e6a23c09f9ff0be0b534fcd/Cargo.toml#L138, https://www.datadoghq.com/blog/datadog-acquires-quickwit/ (accessed 2026-10-29)
- Host hooks: https://github.com/parseablehq/parseable/blob/bbbb79ec52ecd023e5654f9fc5cf6c2f27b85898/src/query/mod.rs#L96, https://github.com/openobserve/openobserve/blob/f88272514512336c2f24d5e8d7b00735094ae439/src/search/src/datafusion/exec.rs#L202, https://github.com/GreptimeTeam/greptimedb/blob/d33c66418379463c5e7c928f3a132517d2a1fa9b/src/plugins/src/lib.rs (accessed 2026-10-29)
- Log niche: https://openobserve.ai/blog/openobserve-vs-clickhouse-one-billion-logs-benchmark.md, https://openobserve.ai/docs/user-guide/advanced/query-tuning/tantivy-index/, https://github.com/openobserve/openobserve/blob/2f3d893b56015f4598f8860f9d2e586666639266/src/config/src/config.rs#L3390, https://github.com/GreptimeTeam/greptimedb/blob/d33c66418379463c5e7c928f3a132517d2a1fa9b/config/datanode.example.toml, https://github.com/influxdata/influxdb/blob/185688a5a813016c0d26ec10abbad2ca770670ed/influxdb3/src/help/serve_all.txt, https://github.com/parseablehq/parseable/blob/bbbb79ec52ecd023e5654f9fc5cf6c2f27b85898/src/hottier.rs#L244, https://www.datadoghq.com/blog/engineering/husky-deep-dive/, https://developer.nvidia.com/blog/mastering-string-transformations-in-rapids-libcudf/ (accessed 2026-10-29)
- Log users: https://www.reddit.com/r/dataengineering/comments/1kw9ku2/comment/mugm06f/, https://news.ycombinator.com/item?id=47109102, https://github.com/maris-development/beacon/issues/231 (accessed 2026-10-29)
- Bandwidth spec values (unverified this session): https://www.nvidia.com/en-us/data-center/h100/, https://www.nvidia.com/en-us/data-center/h200/ (accessed 2026-10-29)
