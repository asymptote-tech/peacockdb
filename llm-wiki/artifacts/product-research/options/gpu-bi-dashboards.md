# gpu-bi-dashboards (+ realtime-vram-cache, gpu-resident-results)

Bracketed numbers point to Sources. Web sources were accessed 2026-10-29. The local wiki pages were accessed 2026-10-28/29.
Code claims carry a SHA. "(estimate)" marks a guess.

## Verdict

- **What it is:** a GPU SQL server. It keeps hot tables resident in VRAM and pinned RAM, appends live data into that cache, answers BI tools over Flight SQL, and can hand results to GPU consumers without leaving the device.
- **Who needs it:** a handful of named engineers, rated medium in demand.md [1]: Lin (Superset over 9.6B rows), Graphistry (results kept on the GPU), andygongyb (a cross-query cache). None shows a budget. HeavyDB is now free with its dashboard UI, and Fabric ships the same idea [15][20].
- **Budget:** it fits. About 0.9× the engine on peacockdb, or about 0.35× as a thin layer on DuckDB+Sirius (estimate). The hard part is serving concurrent queries over a shared cache, not the operators.

## Who needs it badly

- **Rating:** demand.md rates it **medium**: "Three or more first-person threads exist, but urgency is unclear." That is R/U/C 7/11/5 from about 9 posters [1].
- **Yu-Ting Lin (`LIN-Yu-Ting`).** He serves Superset through Spark Thrift Server + RAPIDS over a 9.6B-row Delta table [7][8].
  - **2023-11:** Superset's concurrent sessions hit a plugin classloading race. The fix was a static classpath plus "pre-warming" the server with one beeline session [8].
  - **2024-06:** `SELECT DISTINCT runName` (a dashboard filter dropdown) ran out of GPU memory. It was fixed upstream in #11155 [7].
  - Neither thread shows a budget or a deadline.
- **Louie Meyerov, Graphistry (`lmeyerov`), 2026-06-30, still open.** He wants cudf-polars `collect()` results kept on the device to drive "a crossfilter / multi-hop traversal loop" [9].
  - In his words, the lack of D→D chaining "eliminates utility for many of our non-trivial use cases" [9].
  - His customers run log analytics on 8×A100 [47]. Graphistry is a revenue business [1].
- **andygongyb, 2025-09.** He asked whether inputs must be re-copied host→device between queries. NVIDIA replied: "Caching data onto GPU memory is not currently supported" in Spark RAPIDS [13].
- **anirudh-here-com, 2023-12.** He runs production HeavyDB 6.4 with custom patches. His crash came from `select_ipc_gpu`, the GPU-memory result hand-off, and he said "using gpu shared memory is required and cannot be discarded". The issue was closed as stale on 2026-09-16 [10].
- **jieguolove, 2024-12.** He owns 5 servers × 4 L20 and asked for HeavyDB cluster mode. The reply was that it is enterprise-only [11]. In 2024-04 he complained that the open-source edition had stalled [12].
- **Sirius users:** pinned tables returned stale values after UPDATE (#106). It was closed as "legacy" with no fix [14].
- **No new 2024–26 first-person dashboard-on-GPU threads** turned up on Superset, Grafana, Tableau or Power BI forums. Vendor forums show no GPU-SQL demand at all [6].

## How it would work

- **One process per GPU box,** with four parts:
  - **Front door:** an Arrow Flight SQL server in Rust (SQL text and prepared statements), plus a `DoPut` path for ingest.
  - **Catalog:** DataFusion `TableProvider`s over cached tables, with per-chunk zone maps for pruning.
  - **Cache manager:** immutable chunks of (table version, row group, column) in two tiers, VRAM and pinned host RAM. A pin count blocks eviction while a query uses a chunk. Eviction is LRU or by priority between watermarks, as in HeavyDB and Kinetica [15][18].
  - **Executor:** today's driver and cuDF operators, with scans reading cache chunks instead of `read_parquet` (`cpp/src/operators/scan.cpp:99` @f36a68ec) [48].
- **The data path, end to end:**
  1. Parquet (local or S3) is loaded once by the chunked reader through pinned buffers.
  2. It is encoded (dictionary strings, bit-packed or FOR numerics) into VRAM chunks.
  3. Each dashboard query plans in DataFusion and scans resident chunks at HBM speed.
  4. The result is copied D→H (kilobytes) and returned over Flight SQL.
- **Live data:**
  1. Micro-batches arrive through Flight `DoPut` or Kafka into a pinned-host delta buffer.
  2. The delta is sealed into a GPU chunk every N rows or T seconds.
  3. Queries read the sealed chunks plus the delta, up to a published row count (HeavyDB's `shadowNumTuples` fence) [15].
  4. Sealed chunks are flushed to Parquet for restart.
- **GPU consumers:** an in-process Python/Rust API returns `ArrowDeviceArrayStream` with a CUDA sync event, so nothing is copied to the host [34]. Results can also be kept as cache entries, so a later query can name them. This is the D→D chaining Graphistry asks for [9].
- **Out of alpha scope:** UPDATE/DELETE, multi-node, spilling of operator state, and Postgres wire.

## The human's questions

**1. Real-time ingestion plus fast analytics: what would it take to cache data partly in VRAM?**

- **The design every engine uses.** Writes land in host RAM. The GPU copy is updated in batches and read in chunks.
  - **HeavyDB:** 32M-row fragments with 2 MiB pages and three tiers (GPU, CPU, disk). Its Kafka importer loads 10k-row batches and commits after each load [15].
  - **PG-Strom GPU Cache:** a trigger writes a REDO log in host RAM. It is applied on the GPU every 5 s, at 25% of a 160 MB buffer, and always at query start [26].
  - **Kinetica:** five tiers. VRAM evicts between 90% and 50% watermarks, with priorities 1–10 [18].
  - **GQE:** appendable in-memory tables, each column optionally nvCOMP-compressed [25].
  - **Sirius `pin_table`:** explicit pins, no eviction. UPDATE is rejected and CHECKPOINT drops the pin [3].
- **What peacockdb must build** (it has no cache, server or concurrency today [2]):
  - **(a) A chunk cache keyed by data version.** Invalidation becomes a key miss, so the Sirius #106 stale-read bug cannot happen [14].
  - **(b) An append delta in pinned RAM** with a visibility fence and periodic sealing. Start from PG-Strom's 5 s / 25% defaults (estimate).
  - **(c) Two RMM pools,** one for the resident cache and one for query working memory. The accountant must count both. Today `gpu_memory_limit` is ignored (#148) [2].
  - **(d) Eviction with pin counts and watermarks.** Sirius saw ~84 GB of round-trip traffic per iteration when intermediates were evicted under pressure [24].
  - **(e) Many concurrent small queries against one GPU,** behind an admission queue. GB10 gave only 1.83× throughput at 4 copies [2].
  - **(f) Parquet flush and reload on restart.**
- **Ingest is not the bottleneck.** 1M rows/s × 100 B is 0.1 GB/s, against ~10–47 GB/s H2D on H200 [2] (estimate). The limits are VRAM capacity and how fast data can be encoded on the way in.
- **Capacity.** At ~2.8× (Sirius's 264 → 93 GB [24]) the hot set is L20/L40S 48 → ~130 GB, H100 80 → ~220 GB, H200 141 → ~390 GB. Keep 30–50% headroom for operators (estimate) [31][32].
- **Size:** about 6–9k lines on peacockdb (see the cost table).

**2. Compressed segments (nvCOMP pages, decompressed per query) vs encoded columns (dictionary/RLE/bit-pack, operated on directly).**

- **Capacity multipliers.**
  - Lightweight encodings (bitpack, FOR/delta, dictionary) give about 2.5–3× on TPC-H-like data. Sirius's 7 hottest lineitem columns went 264 → ~93 GB, i.e. 2.84× [24].
  - General codecs on a B200 microbenchmark compressed lz4 1.0×, snappy 1.9×, zstd and gzip 2.0×, bitcomp 3.0×. That data is synthetic [28].
  - Parquet pages (dictionary + RLE + snappy) usually land in the same 2–4× range (estimate; no primary TPC-H source found).
  - The capacity gain is roughly a wash. The real choice is decode cost.
- **Per-query decode cost on the GPU.**
  - Decode speeds:
    - nvCOMP output on B200's hardware engine: snappy 117 GB/s, zstd 155, lz4 173, cascaded 213, bitcomp 462, ANS 539 [28].
    - Sirius's lightweight plans on GB300 decode at 580–1,700 GB/s; dictionary strings at about 250 GB/s [24].
    - peacockdb's own Parquet load materializes only 6.8–8.7 GB/s on H200. Page decode plus snappy is 80% of kernel time on q11v [2].
    - GPU-tuned Parquet reaches about 125 GB/s effective on an A100 [29].
  - For an 80 GB decoded touch set (2B rows × 5 columns), per query: raw in H200 HBM ~17–35 ms; bit-pack ~50–140 ms; ANS ~150 ms; zstd ~0.5 s; snappy ~0.7 s; tuned Parquet ~0.64 s; today's reader ~10 s (estimate, from [2][24][28][29]).
  - **So storing Parquet or nvCOMP-general pages and decoding per query breaks a sub-second budget at billions of rows.**
- **Encoded columns win twice: decode is cheap, and filters skip work.**
  - Sirius filters during decode:
    - range predicates on bit-packed data never unpack rejected rows;
    - equality and IN predicates on dictionary codes are 2.1–2.6× faster at every selectivity.
  - The GB300 SF1000 suite went 7.87 → 6.92 s [24]. The SIGMOD 2022 tile-based GPU-FOR work decodes inside query kernels for the same reason [30].
  - Sirius's split works well: raw or bit-packed on the GPU tier, high-ratio codecs only on the host tier. On a PCIe link, host-tier compression should pay, unlike on GB300's 370 GB/s C2C link (estimate) [24].
- **Which cuDF operators work on encoded data** (cudf @785ee522, vendored @43505bb9) [27]:
  - **DICTIONARY columns work with:** groupby keys and many aggregations (20 test files), join (key sets aligned first with `match_dictionaries`), gather/copy, concatenate, merge, search, and JIT transforms.
  - **They do not work with `binary_operation`,** which throws `data_type_error` on them. Filters must decode, or compare in key space and gather by index.
  - **Untested:** sort and stream compaction (distinct) have no dictionary tests.
  - **No run-end or bit-packed column type exists.** RLE and bit-packing must be decoded by a custom fused kernel at scan.
  - **The Parquet reader can emit strings as DICTIONARY32.** Setting an AST filter disables that fast path.
  - **The experimental `hybrid_scan` reader filters, then materializes,** with page pruning [27].
- **Which peacockdb operators would change** [27][48]: scan (cache read plus fused unpack+filter); filter (`expr.cpp` needs key-space compares); groupby and join (align dictionary keys first); hash repartition already *decodes* DICTIONARY32 before hashing (`cpp/src/spark_hash_partition.cu:171` @f36a68ec); sort and DISTINCT (decode first); export (decode, or emit an Arrow dictionary).
  - **Recommendation:** dictionary strings plus bit-packed numerics, decoded in the scan kernel. Late materialization of payload columns comes after the filter mask (estimate).

**3. How BI tools would connect.**

- **Arrow Flight SQL is the cheapest front door.**
  - DataFusion's reference server core is 1,172 lines (`service.rs` @45eb1456) [35]. A peacockdb version would be about 1–2.5k lines and 1–3 weeks (estimate).
  - No crate release targets DataFusion 45, so expect a backport or a DataFusion upgrade [35].
  - It reaches Superset (master has a `datafusion` engine spec over `flightsql-dbapi`) [36], DBeaver, DataGrip and Tableau via the Flight SQL JDBC driver [37], Grafana via the FlightSQL plugin [46], and Python via ADBC.
  - It does not reach Power BI, Looker or Metabase. Each ships one connector per vendor, and Power BI is moving to per-vendor ADBC connectors [38].
- **Postgres wire reaches every tool, but catalog emulation is the cost.**
  - `pgwire` gives the protocol [40]. `datafusion-postgres` adds about 25k lines, mostly pg_catalog and function emulation, and still marks Power BI and DataGrip as unsupported [39].
  - It would take 3–6 weeks for the protocol plus 1–3 months of catalog work (estimate). Kinetica took this path: simple queries are fully supported, extended queries "largely" [19].
- **Plugin into DuckDB or ClickHouse.** These inherit the host's catalog, auth and BI drivers.
  - DuckDB is Sirius's slot now. PR #28022 adds a Sirius loader to core [41].
  - ClickHouse's RFCs plan a separate GPU process that takes Substrait and keeps a (part, column, version) cache. That is the same design as this option, but upstream [42].
  - A plugin also loses the cache's freedom: the host owns ingest and versions. Lin's Thrift story shows the host's fragility becomes yours [8].
- **Recommendation:** Flight SQL first, plus the in-process device API for GPU consumers. Add Postgres wire only if a design partner's tool demands it.

**4. Comparison with HeavyDB, Kinetica and Fabric.**

| | HeavyDB 10 (NVIDIA) | Kinetica | Fabric DW GPU acceleration | This option |
|---|---|---|---|---|
| Price | Free, Apache-2.0 since 2026-10-08, incl. Immerse UI, GPU renderer, HeavyConnect [15][16] | Free single-node dev edition; cloud $1.80/h; GPU is an extra-licence add-on [17] | Limited preview; separate meter, 1 accelerated core = 3.446 CU [20][21] | Open source (estimate) |
| Cache | GPU/CPU/disk buffer pools, LRU per slab, pin counts [15] | 5 tiers, watermarks 90/50, priority 1–10 [18] | Warm cache nodes; "cache cooldown" still roadmap [21] | VRAM + pinned RAM chunks, encoded |
| Ingest | Kafka micro-batch, COPY, inserts [15] | Real-time ingest (not checked) | Writes not accelerated [20] | Flight DoPut / Kafka delta |
| Scale-out | Distributed mode was EE-only; none found in open tree (estimate) [11][15] | Multi-node in paid tiers [17] | Managed | Single node (alpha) |
| BI | Thrift, JDBC, ODBC, own UI; no Superset spec [15] | HTTP, JDBC/ODBC, Postgres wire [19] | Power BI native (DirectQuery), TDS [23] | Flight SQL; device hand-off |
| Data on GPU for consumers | `sql_execute_gdf` returns CUDA IPC handles [15] | no evidence | no | In-process ArrowDevice + cache ids |
| Health | "Best effort"; since 2026-05 only NVIDIA staff file issues [15] | GPU demoted to option [17]; "if you can afford the price" [43] | Microsoft-backed; CoddSpeed up to 30× vs CPU at TPC-H 1TB [22] | Unbuilt |

- **Where it could differ:** open Parquet/Iceberg inputs; device hand-off as a first-class API (HeavyDB's Thrift side path crashed [10]); standard Flight SQL; an open multi-GPU roadmap, which jieguolove wanted [11].
- **What it cannot claim:** it is not the first free GPU DB, and it does not have a UI. HeavyDB already has both.

## Data path and cache

- **Bandwidth, from fast to slow:**

  | Path | Bandwidth | Source |
  |---|---|---|
  | H200 HBM | 4.8 TB/s spec, 4,484 GB/s measured | [31][2] |
  | L20 / L40S GDDR6 | 864 GB/s | [32] |
  | GH200 / GB300 NVLink-C2C H2D | ~370–380 GB/s measured | [33][24] |
  | H200 PCIe Gen5 H2D, pinned | 47.3 GB/s (own measurement) | [2] |
  | H200 PCIe Gen5 H2D, pageable | 10.6 GB/s (own measurement) | [2] |
  | PCIe Gen4 x16 | ~25 GB/s | (estimate) |
  | 4×NVMe + GDS, GPU-tuned Parquet | ~125 GB/s effective | [29] |
  | GDS on stock Parquet | 30–50% above the non-GDS path on average | [50] |
  | Single NVMe; 100 / 400 GbE | Gen4 ~7, Gen5 ~14; 12.5 / 50 GB/s | (estimate) |
  | peacockdb's `read_parquet` | 6.8–8.7 GB/s materialized | [2] |
- **The thesis holds.** IO-fed, the GPU runs at the speed of its link, not its HBM.
  - In peacockdb, the load is about 20× the execute on H200 sf40. q6 is 19 ms of execute against a 398–741 ms load [2].
  - The human's PoC loaded longer than it computed on 11 of 12 SF20 queries [5].
  - The 80 GB example at PCIe Gen5 pinned rate takes about 1.3 s, and from 400 GbE about 1.6 s, before any compute (estimate).
  - A 96-core CPU box reading its own DRAM at ~300–460 GB/s matches or beats that (estimate).
- **The cache this option needs:** the hot touch set in VRAM, lightweight-encoded, plus a pinned host tier for warm columns.
  - The pinned tier pays off only behind C2C, or with compression on PCIe [24].
  - Cold data goes through the chunked reader (61 vs 340 ms on GB10 for q6, #232) [2] or `hybrid_scan` page pruning [27].
- **Fabric also needs it.** Its "up to 1 TB" sweet spot and warm-cache nodes show the same constraint [20][21].

## Build cost to alpha

| Component | On peacockdb: reuse/adapt/new, size | On Sirius: reuse/adapt/new, size |
|---|---|---|
| SQL frontend, planner, operators | reuse (DataFusion 45, 19 node kinds, cuDF operators) [2] | reuse (DuckDB + Sirius operators) [3] |
| GPU entry point outside tests | adapt, 0.3k | reuse (DuckDB extension) |
| Flight SQL server (+ DataFusion upgrade or crate backport) | new, 1.5–2.5k + adapt 0.5–1k | new, 1.5–2.5k (DuckDB has no network server) (estimate) |
| Concurrent queries: sessions, admission queue, shared RMM pools, accounting of resident bytes | new, 2–3k | adapt, 0.5–1k (Context runs one at a time per context [3]) |
| Resident cache: chunk keys, VRAM + pinned tiers, pin counts, eviction | new, 2–4k (sirius.md's "minimal pin cache" range) [3] | reuse `pin_table` + adapt eviction, 0.5–1k (pins never evict) [3] |
| Append ingest: delta buffer, sealing, visibility, Parquet flush | new, 2–3k | new, 2–4k against pins that reject writes [3] |
| Encoded columns: dictionary through filter/groupby/join, fused bit-pack decode+filter kernels | new, 2–4k | reuse (Simpatico + filter-on-decode) [24] |
| Scan from cache; chunked reader + pinned buffers for cold loads | adapt, 1–1.5k [3] | reuse |
| Result and plan cache | new, 0.5–1k | new, 0.5–1k |
| GPU-resident results: ArrowDevice export, Python binding, result-as-table | new, 1–1.5k | new, 1–2k (no device-table in/out in FFI [3]) |
| Window functions for dashboard SQL | new, 1–2k (#143) [2] | upstream gap (not wired, #1745) [3] |
| Spill of operator state | out of alpha (fail cleanly) | reuse (cuCascade) [3] |

Total new work: 0.9× engine on peacockdb, 0.35× on Sirius (estimate)

- **Arithmetic.** peacockdb's midpoints sum to about 20k against a 22,830-line yardstick. Sirius's sum to about 8k, from its 0.3–0.4× range. Both are estimates.
- **The Sirius base is plausible here, unlike for DataFusion-planned options.** The product would be a DuckDB+Sirius server and would drop peacockdb [3].
  - The cost is control. You build on ≈1.1M lines of churn in 6 months, about 92% from NVIDIA, with no versions [3].
  - The pin code is exactly what NVIDIA is changing.
  - Writes into pins are blocked by design today [3].
- **The hard part:** concurrency over a shared, mutable cache.
  - Many sessions, an append stream and eviction must coexist without stale reads, deadlocks or OOM.
  - Sirius already deadlocks on under-reserved pins [3]. Spark RAPIDS failed Lin on exactly this kind of multi-session load [7][8].
  - The second hard part is keeping dictionaries aligned across chunks and appends, so joins and groupbys on codes stay correct.

## Adoption friction

- **It is a new server to adopt,** not a plugin.
  - Users must load data into it and point BI at a Flight SQL URL. Superset, DBeaver, Tableau and Grafana can do that; Power BI, Looker and Metabase cannot without a connector [36][37][38].
  - Lin would have to leave Spark Thrift and Delta. Graphistry would need a Python API, not SQL over the network [8][9].
- **Fabric users get acceleration with a workspace toggle and no tool change** [20]. HeavyDB users get Immerse for free [15].
- **What eases adoption:** Parquet in, Arrow out; a single binary; an "every query ran on the GPU" log, which answers the silent-fallback distrust [1].

## Platform risk

- **Strong risk.** nvidia.md rates realtime-vram-cache as strong platform risk. NVIDIA owns and open-sourced HeavyDB, GQE has nvCOMP in-memory tables, and Sirius pins tables in VRAM [4].
  - The one opening it names is that HeavyDB is maintained only "best effort" [4][15].
- **More players own pieces of this space:** Fabric ships GPU acceleration with a warm cache to Power BI shops [20][22]; ClickHouse plans a GPU cache keyed by part, column and version [42]; Sirius topped ClickBench on cost, hot runs [44].
- **The compression path leans on closed code.** nvCOMP is now binaries under an NVIDIA licence [4]. The bit-pack and dictionary path can avoid it.
- **Device hand-off is the least-occupied piece.** cudf-polars has only partial progress on it (#22867, #22917) [9]. NVIDIA could close it in cudf-polars at any time (estimate).

## Funding story

- **A design partner might say** (estimate): Graphistry: "If I can run SQL or Polars-like filters over GPU-resident tables and get device frames back for GFQL, I will use it in my product." Lin: "If Superset can hit a GPU engine without Spark's session bugs, I will move my 9.6B-row dashboards."
- **The round would be raised on** "an open, Parquet-native GPU analytics server: the HeavyDB successor for the lakehouse, with a live cache and device hand-off for GPU apps." The long-term story is CUDA analytics as the serving layer for agent and visual-analytics query load [47] (estimate).
- **The weak point:** MapD/HeavyAI raised on this exact pitch and ended up absorbed and given away. Kinetica made GPU an extra-cost option [4][17]. Investors will ask why a third attempt works when the incumbent is now free.

## Evidence against

- **The incumbent is free and complete.** HeavyDB 10 brings Immerse, the GPU renderer and HeavyConnect under Apache-2.0 [15][16]. A startup without a UI starts behind it on the core dashboard use case.
- **The original GPU-DB vendors retreated.** Kinetica charges extra for GPU and defaults to CPU [17]. HeavyAI's open-source edition stalled "for lack of resources" before NVIDIA gave it away [12].
- **CPU OLAP already meets the target with pre-aggregation.** StarRocks reports P99 under 500 ms on 300M rows, and Preset's Superset guide fixes slow dashboards by materializing [45]. A GPU wins only on ad-hoc, high-cardinality, join-heavy or geo queries (estimate).
- **Demand is soft.** About 9 posters in three years, no budgets, and nobody states a latency target [1]. Lin's pain was Spark's bugs, which NVIDIA fixed [7].
- **The best lead does not want a SQL server.** Graphistry wants a Polars/cudf in-process API, which NVIDIA is building [9].
- **Fabric's billing,** which applies the accelerated meter to every query, and its preview status show even Microsoft prices GPU as a premium [20].
- **VRAM caps the hot set.** 48–141 GB per GPU (×2.8 encoded) holds billions of rows of a few columns, not a whole warehouse. Above that, the thesis says the advantage drains away.
- **Single-node concurrency is unproven.** GB10 gave 1.83× throughput at 4 copies [2]. Dashboards fire 5–30 queries at once (estimate).

## Ratings

- Who needs it badly: **medium.** This is demand.md's rating [1]. The named threads exist (Lin, Graphistry, andygongyb), but none shows a budget.
- Build cost: **medium.** About 0.9× on peacockdb fits the budget, but concurrency over a shared, appendable cache is new and hard.
- Data path: **strong.** The option exists to fix the data path. With resident encoded data, queries run at HBM speed and ingest is far below link rate. The limit is VRAM size.
- Adoption friction: **weak.** It is a new server. Power BI, Looker and Metabase are not reachable, while Fabric and HeavyDB need no new protocol.
- Platform risk: **weak.** NVIDIA's free HeavyDB, Sirius pins, GQE, Fabric and ClickHouse each occupy part of the space [4][20][42].
- Funding story: **weak.** It repeats the MapD/Kinetica pitch, the incumbent is now free, and no partner has a budget.

## Follow-up experiments

1. **Resident dashboard queries.** On SF100–SF1000 TPC-H/ClickBench data, resident in VRAM on an L40S and on an H200, measure p50/p95 per tile with 20 concurrent clients. Compare with ClickHouse and DuckDB on the same box.
2. **Decode cost per query on H200.** Time bit-pack and dictionary filter-on-decode against nvCOMP ANS, zstd and snappy, and against the chunked Parquet reader. Confirm the 80 GB arithmetic.
3. **Dictionary columns through peacockdb's operators.** Measure correctness and speed for filter via key space, groupby, join after `match_dictionaries`, and sort.
4. **Concurrency on one GPU.** Sweep 1–32 small queries with a shared RMM pool and an admission queue. Find where throughput flattens (GB10 flattened at 1.83×).
5. **Append freshness.** Run 1M rows/s into the delta with a dashboard query running. Measure seal latency, query p95 and VRAM growth.
6. **HeavyDB 10 baseline.** Run the same dashboard suite on HeavyDB 10. Check whether cluster mode builds from the open tree.
7. **Ask the leads.** Ask Graphistry whether an in-process SQL API with device output fits GFQL. Ask Lin whether he still runs Superset on Spark Thrift.

## Sources

1. demand.md, `## Ratings` and `## First conversations` (local, `llm-wiki/artifacts/product-research/demand.md`) (accessed 2026-10-29)
2. inventory.md, sections "Cross-cutting capabilities" and "Benchmark evidence" (local, master f25c250d) (accessed 2026-10-28)
3. sirius.md, sections "Cache in RAM and VRAM", "API surface", "Activity and governance" and "Sirius components" (local; Sirius @b9e05ffc653c8ed031c789e5c1415ac1186e0619) (accessed 2026-10-28)
4. nvidia.md, "Open-source projects" and "Per direction: realtime-vram-cache" (local) (accessed 2026-10-28)
5. evidence-in-hand.md, "TPC-H SF20: where the time goes" (local) (accessed 2026-10-28)
6. discovery.md and sources/discord-forums.md (local) (accessed 2026-10-28)
7. https://github.com/NVIDIA/cudf-spark/issues/11064 (accessed 2026-10-29)
8. https://github.com/NVIDIA/cudf-spark/discussions/9966 (accessed 2026-10-29)
9. https://github.com/NVIDIA/cudf/issues/23039 (accessed 2026-10-29)
10. https://github.com/heavyai/heavydb/issues/819, with comments via `gh api` (accessed 2026-10-29)
11. https://github.com/heavyai/heavydb/issues/842 (accessed 2026-10-29)
12. https://github.com/heavyai/heavydb/issues/825 (accessed 2026-10-29)
13. https://github.com/NVIDIA/cudf-spark/discussions/13435 (accessed 2026-10-29)
14. https://github.com/sirius-db/sirius/issues/106 (accessed 2026-10-29)
15. heavyai/heavydb @72c6df9f1bce2cd5b849670b38c7e7e4592c564b: README; `DataMgr/MemoryLevel.h:10`; `Fragmenter/FragmentDefaultValues.h:8-12`; `DataMgr/BufferMgr/BufferMgr.cpp:370-400`; `Fragmenter/InsertOrderFragmenter.cpp:1225-1250`; `ImportExport/KafkaImporter.cpp:410-418`; `heavy.thrift:168-176,550-552`; `Tests/ArrowIpcIntegrationTest.cpp:182-184` — https://github.com/heavyai/heavydb/tree/72c6df9f1bce2cd5b849670b38c7e7e4592c564b (accessed 2026-10-29)
16. https://github.com/heavyai/heavydb/discussions/915, https://github.com/heavyai/heavydb/discussions/877 (accessed 2026-10-29)
17. https://www.kinetica.com/pricing (accessed 2026-10-29)
18. https://docs.kinetica.com/7.2/rm/concepts/ (accessed 2026-10-29)
19. https://docs.kinetica.com/7.2/connectors/psql_wire_protocol/ (accessed 2026-10-29)
20. https://learn.microsoft.com/en-us/fabric/data-warehouse/query-acceleration (accessed 2026-10-29); blog https://community.fabric.microsoft.com/t5/Fabric-Updates-Blog/A-new-analytics-frontier-GPU-accelerated-Fabric-Data-Warehouse/bc-p/5192701 (HTTP 403; via search snippet) (accessed 2026-10-29)
21. https://github.com/sdebruyn/fabric-dw-mcp-cli/issues/595 (accessed 2026-10-29)
22. https://www.microsoft.com/en-us/research/publication/coddspeed-hardware-accelerated-query-processing-in-microsoft-fabric/ (accessed 2026-10-29)
23. https://learn.microsoft.com/en-us/fabric/fundamentals/direct-lake-overview (accessed 2026-10-29)
24. https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/compressed-pinning.md (lines 1-175) (accessed 2026-10-29)
25. https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/README.md (lines 148-190); `include/gqe/storage/in_memory.hpp:125-190` (accessed 2026-10-29)
26. https://heterodb.github.io/pg-strom/gpucache/ (accessed 2026-10-29)
27. rapidsai/cudf @785ee522f5688f485fc0b1eabca7f8d6d2adea0e: `cpp/include/cudf/binaryop.hpp:170`, `cpp/include/cudf/io/parquet.hpp:344-356`, `cpp/include/cudf/io/experimental/hybrid_scan.hpp`, `cpp/include/cudf/dictionary/`, `cpp/include/cudf/transform.hpp:247-250`, DEVELOPER_GUIDE "Dictionary columns" — https://github.com/rapidsai/cudf/tree/785ee522f5688f485fc0b1eabca7f8d6d2adea0e ; test counts from peacockdb `third_party/cudf` @43505bb975f46ce4e140fdaf55192611d6e231ac (accessed 2026-10-29)
28. https://arxiv.org/html/2512.02189v2 (Tables I, XIII) (accessed 2026-10-29)
29. https://arxiv.org/html/2602.17335 (accessed 2026-10-29)
30. https://dspace.mit.edu/handle/1721.1/146390 (Shanbhag et al., SIGMOD 2022) (accessed 2026-10-29)
31. https://www.nvidia.com/en-us/data-center/h200/ (accessed 2026-10-29)
32. https://www.nvidia.com/en-us/data-center/l40s/, https://techpowerup.com/gpu-specs/l20.c4206 (accessed 2026-10-29)
33. https://dnhkng.github.io/posts/gh200-benchmarking/, https://verda.com/blog/data-movement-in-the-nvidia-gh200-grace-hopper-superchip (accessed 2026-10-29)
34. https://arrow.apache.org/docs/format/CDeviceDataInterface.html (accessed 2026-10-29)
35. https://github.com/datafusion-contrib/datafusion-flight-sql-server/tree/45eb14560cc2d4034987bda7aeafb7f9b3272795 ; https://crates.io/api/v1/crates/datafusion-flight-sql-server/0.4.7/dependencies (accessed 2026-10-29)
36. https://github.com/apache/superset/blob/7ad1d7ff50e193bc37a33cc4ae55bdd307b1fd2e/superset/db_engine_specs/datafusion.py (accessed 2026-10-29)
37. https://docs.dremio.com/current/client-applications/ (accessed 2026-10-29)
38. https://learn.microsoft.com/power-query/transition-to-adbc (accessed 2026-10-29)
39. https://github.com/datafusion-contrib/datafusion-postgres/tree/87f338367305d65d85dcb9c67715eda7de79b418 (accessed 2026-10-29)
40. https://docs.rs/pgwire/latest/pgwire/ (accessed 2026-10-29)
41. https://github.com/duckdb/duckdb/pull/28022 (accessed 2026-10-29)
42. https://github.com/ClickHouse/ClickHouse/issues/116807, https://github.com/ClickHouse/ClickHouse/issues/124455 (accessed 2026-10-29)
43. https://news.ycombinator.com/item?id=44340460 (accessed 2026-10-29)
44. https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/ (accessed 2026-10-29)
45. https://preset.io/blog/the-data-engineers-guide-to-lightning-fast-apache-superset-dashboards/ ; StarRocks case https://besthub.dev/articles/how-zepto-scaled-real-time-brand-analytics-with-starrocks-from-postgres-mvp-to-sub-second-queries-6f1a6041774b (accessed 2026-10-29)
46. https://grafana.com/grafana/plugins/influxdata-flightsql-datasource/ (accessed 2026-10-29)
47. https://news.ycombinator.com/item?id=47109102 ; https://news.ycombinator.com/item?id=48378299 (accessed 2026-10-29)
48. peacockdb @f36a68eca3704bb4add89d0292060ed545660948: `cpp/src/operators/scan.cpp:99`, `cpp/src/spark_hash_partition.cu:162-174` (local) (accessed 2026-10-29)
49. https://developer.nvidia.com/nvcomp (accessed 2026-10-29)
50. https://developer.nvidia.com/blog/boosting-data-ingest-throughput-with-gpudirect-storage-and-rapids-cudf (accessed 2026-10-29)
