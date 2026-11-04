# Prior art: GPU and hardware acceleration on Comet and on Spark

## Summary
- Nobody has built, forked or proposed a GPU, FPGA or other hardware backend for Apache Comet. The closest thing is Auron PR #1001, a libcudf bridge on another DataFusion-based Spark engine. It covered scan, filter and project only, and was closed as stale in December 2025.
- Spark GPU acceleration today means NVIDIA's spark-rapids (mature, sold) or Gluten with Velox-cuDF (experimental; scans stay on the CPU by default). Speedata (ASIC), NeuroBlade and DualBird (FPGA) are vendor plugins whose figures nobody has checked independently. Photon is CPU-only.
- Published cost-per-query wins for GPUs are mostly against vanilla Spark, at about 1.4–5x. Against CPU-native engines (Photon, Gluten) they run from 1.1x to 2.9x, which straddles the 2x bar. No study compares a GPU with Comet.
- Every serious GPU engine reports that the scan and I/O path decides the result: reader, object-store client, decode and exchange. Compute does not. Hot-cache numbers flatter GPUs, and one-pass ETL never gets that cache.
- Evidence against: Voltron Data (Theseus) shut down, the Auron GPU work stalled, Gluten dropped Intel IAA after measuring no gain, NVIDIA dropped its CPU-scan hybrid for lack of use, and users have missed NVIDIA's promised savings.

## Method and versions
- Code facts come from local clones: Comet tag 1.2.0 at `ab46419fcc99bcd34aa4d89f7b545d5d51694a88`, spark-rapids (`NVIDIA/cudf-spark`) at `ce6349699ccb3bd527c223c66ba24bd708e67e18`, Gluten at `d64a51dfd01bbe1b8e4adb6c1f74331fb75f531f`, Auron at `cdd73ecfafa52a96a2602235930dc309edb9c019`. Web facts are cited inline; the source list is at the end.
- Comet 1.2.0 runs on DataFusion 55.2.0 and Arrow/Parquet 59.2.0 (`native/Cargo.toml:41-51` @ ab46419f). Auron runs on DataFusion 49.0.0 and Arrow 55.2.0 (`Cargo.toml:137-146` @ cdd73ecf).
- Re-checked from `spark-plugin.md` against current sources, all hold (details below): Comet is CPU-only; Gluten-cuDF passes 48 of 103 TPC-DS SF1 queries with fallback off; spark-rapids' native-reader epic is still open; spark-rapids' hybrid scan was removed; Gluten is about 9% faster than Comet; the MaxNevermind and GKE reports say what that file said; Comet's in-memory cache is experimental.

## 1. GPU or hardware acceleration on Comet
- **Nothing exists in Comet itself.**
  - Searches of apache/datafusion-comet for GPU, CUDA, cuDF, FPGA and "hardware acceleration" find no GPU issue, PR or discussion. The two GPU hits are incidental: #19 asks for a roadmap, #497 is the README change ([issues](https://github.com/apache/datafusion-comet/issues?q=GPU)) (accessed 2026-11-04).
  - The 1.2.0 README: "Comet runs on commodity hardware: no GPUs, FPGAs, or other specialized accelerators are required" (`README.md:49-51` @ ab46419f).
  - The dev@datafusion archive has 0 hits for "GPU" over 10 years; the search API works ("comet" returns 264 hits) ([lists.apache.org API](https://lists.apache.org/api/stats.lua?list=dev&domain=datafusion.apache.org&q=GPU&d=lte=10y)) (accessed 2026-11-04).
- **Seams that exist in Comet** (not GPU work, but a backend could use them):
  - **Comet Native Executors, epic [#7288](https://github.com/apache/datafusion-comet/issues/7288)** (andygrove, opened 2026-10-28; prototype #7289) runs Comet protobuf plans on JVM-free Ballista executors. The fragment seam "passes only proto bytes, a partition index, input streams and config, and returns an output stream and metrics", and can "move behind the Arrow C Stream interface" later. The narrowest documented hook for a foreign executor. All-or-nothing: no fallback to Spark (accessed 2026-11-04).
  - **Custom Rust UDFs.** Comet loads scalar UDFs from a `cdylib` through `comet-udf-sdk`. Experimental, ABI pinned per release, scalar-only (`docs/source/user-guide/latest/rust_udfs.md` @ ab46419f). A GPU cannot take whole operators this way.
  - **`ContribScan`.** An `Any`-style proto envelope for out-of-tree scans such as Delta and Lance ([#5378](https://github.com/apache/datafusion-comet/issues/5378)) (accessed 2026-11-04).
  - **Older plugin request.** Custom native operators ([#1077](https://github.com/apache/datafusion-comet/issues/1077), 2024), closed as inactive 2026-05-06 (accessed 2026-11-04).

## 2. DataFusion-level GPU efforts that bear on Comet
- **Auron PR #1001, "WIP: initialize GPU supporting with libcudf"** (richox; head `e5bc61838d8d3075376b685d0afdbac5471d55e4`). Opened 2025-05-26, closed as stale 2025-12-05 ([PR](https://github.com/apache/auron/pull/1001)) (accessed 2026-11-04).
  - One commit, +3,933 lines. It converts Auron's DataFusion `ParquetExec`, `FilterExec`, `ProjectExec` and `RenameColumnsExec` into a C++ cuDF plan (`datafusion-ext-plans/src/cudf/plan.rs`); any other node returns "cudf not supported".
  - Data reached cuDF through `cudf::io::chunked_parquet_reader` over a custom `cudf::io::datasource` whose `host_read` calls back into Rust, which reads through Auron's Hadoop-FS `InternalFileReader` (`bridge-cpp/src/plan/parquet_scan.cpp`, `src/io.rs`). Schemas cross the Arrow C ABI. Pinned to libcudf 25.6 (vcpkg).
  - Follow-on: Auron [#1504](https://github.com/apache/auron/issues/1504) split out the memory manager "to reuse memory management for GPU device memory" (closed 2025-10-24). No GPU code on Auron master at cdd73ecf (accessed 2026-11-04).
  - Why it matters: this is the design `spark-plugin.md` sketched, attempted by a team that wrote a DataFusion Spark engine, and it stopped at the first three operators.
- **libcudf-rs (gabotechs, a DataFusion committer).** A DataFusion physical-optimizer rule swapping nodes for `CuDF*Exec` variants, with `CuDFLoadExec`/`CuDFUnloadExec` at the edges. "A couple of weeks' hackathon". TPC-H Q1: 813 ms on a g4dn.xlarge (T4, about $423/month) vs 906 ms on an m5.4xlarge (about $625/month), called "1.65x cheaper". Last push 2026-09-17 ([repo @ a6739ff0](https://github.com/gabotechs/libcudf-rs)). The author gave "DataFusion on GPUs" at the NYC meetup on 2026-05-12 ([recording](https://youtu.be/P5v3y1sUMc8), listed in [discussion #20030](https://github.com/apache/datafusion/discussions/20030)); slides not readable (accessed 2026-11-04).
- **Algeon** (algeon-dev; created 2026-10-19; Apache-2.0; DataFusion 55.1, Comet's major; `Cargo.toml` @ `374eb0a5`). cuDF relational fragments plus cuGraph/cuVS/cuML table functions; unsupported operators stay on DataFusion's CPU path; Parquet via KvikIO.
  - Benchmarks: TPC-H SF100 3.5x (47.9 s CPU vs 13.9 s GPU) and TPC-DS SF100 3.32x (169.6 s vs 51.1 s; 30 of 99 queries slower on GPU), one g7e.8xlarge (32 vCPU, RTX PRO 6000), against DataFusion 55.1 on the same box. Local NVMe, KvikIO compatibility mode without GDS, cache off. No cost claims ([benchmarks](https://algeon.dev/reference/benchmarks)) (accessed 2026-11-04).
  - Limit: "Not a fit yet: working sets larger than one GPU's memory" ([README](https://github.com/algeon-dev/algeon)) (accessed 2026-11-04).
  - Upstream seam gap: Algeon asked DataFusion to let a planner identify a Parquet scan's reader factory so a GPU reader can safely replace it ([datafusion #26633](https://github.com/apache/datafusion/issues/26633)) (accessed 2026-11-04).
- **Other DataFusion threads and toys.** [Discussion #8498](https://github.com/apache/datafusion/discussions/8498) (2023, closed) sketches "granular" `ExecutionPlan` swaps vs a "coarse" single opaque node; alamb pointed to existing extension APIs. [nerdsane/datafusion-gpu](https://github.com/nerdsane/datafusion-gpu) (2025, one GPU aggregate UDAF, abandoned). `datafusion-arrowmetal` (Apple Metal; listed in [PR #26004](https://github.com/apache/datafusion/pull/26004)). A datafusion-python cuDF spike ([#936](https://github.com/apache/datafusion-python/issues/936)) (all accessed 2026-11-04).
- **LiquidCache** is a CPU pushdown cache for DataFusion over S3, not a GPU project. The VLDB 2025 paper reports up to 10x lower cache CPU time and about two orders of magnitude less network traffic ([VLDB](https://www.vldb.org/pvldb/volumes/18/paper/LiquidCache%3A%20Efficient%20Pushdown%20Caching%20for%20Cloud-Native%20Data%20Analytics)) (accessed 2026-11-04). Relevant to the NIC question, not GPU prior art.
- **Sirius and Theseus, outside DataFusion.** Sirius plugs into DuckDB via Substrait; its cost charts are TPC-H hot runs with tables pinned in GPU memory via `pin_table` ([README @ 1c486905](https://github.com/sirius-db/sirius)). Theseus came from Voltron Data, which has shut down (§6) (accessed 2026-11-04).

## 3. Spark accelerators: maturity, adoption, claims, plug-in
| Accelerator | Device | How it plugs into Spark | Maturity, adoption | Best published claim (benchmark, hardware) |
|---|---|---|---|---|
| spark-rapids (NVIDIA cuDF plugin) | NVIDIA GPU | `spark.plugins` jar; replaces physical operators; JNI to libcudf | GA for years; bundled by EMR, Dataproc, Databricks, Cloudera | TPC-DS 3 TB from S3: 8× g7.4xlarge (RTX PRO 4500) 281 s vs 1,010–1,043 s on 8× 4xlarge CPU nodes; $2.06 vs $2.93–3.18 per run (EMR 7.12 Spark, not Comet) [A1] |
| Gluten + Velox-cuDF | NVIDIA GPU | Gluten rules → Substrait → Velox; cuDF swaps operators per stage | Experimental; "TPC-H SF1 validated"; 48/103 TPC-DS SF1 with fallback off | "up to 8.1x" on some queries vs Spark Java [G1][G2] |
| Gluten + Velox (CPU) | CPU (QAT optional) | same | Apache TLP, wide use | 1.10x faster than Comet 0.16 on TPC-DS 3 TB [A3] |
| Auron (ex-Blaze, Kuaishou) | CPU; GPU attempt abandoned | Spark extension → protobuf → DataFusion 49 | Apache incubating, 8.x | TPC-DS 1 TB chart only [U1] |
| Comet (baseline) | CPU | Spark extension → protobuf → DataFusion 55 | Apache, 1.2.0 | 2.5x over Spark 4.1.3, 2.11x over Spark 4.2.0 (first 24 queries), TPC-DS SF1000 on 32×16 cores [C1] |
| Photon | CPU | Databricks-only native engine | GA, default on Databricks | 3x average, >10x max over DBR [P1]; turned off on GPU compute [P2] |
| Speedata C-200 APU | ASIC (CGRA), PCIe 5 x16, 32 GB | `com.speedata.spark.DashPlugin`; CPU fallback | Sold as 2U boxes and via HPE; $44M round 2025 | Vendor: "20X" on one query, "up to 100x"; CoddSpeed TPC-H SF100 vs one SQL Server core: Q13 259x [S1][S2][S3] |
| NeuroBlade SPU | FPGA on AWS F2 / PCIe card | Spark, Presto, Velox integration | Shipping via Dell and AWS F2 | Vendor: up to 30x; 23x TPC-H in a 2023 demo [N1] |
| DualBird | FPGA on AWS F2 | Spark plug-in (EMR, EKS) | AWS Marketplace; $25M raised | Vendor: Iceberg compaction 12–20x single-task vs vanilla Spark, 55–85% lower EC2 cost [D1] |
| Bigstream | FPGA/GPU | Spark plug-in | No news after a 2021 STAC deck (status unknown) | Vendor: 2–30x [B1] |
| Intel QAT / IAA in Gluten | Compression ASIC | Shuffle codec | QAT kept; IAA removed 2025-08-18 | QAT "up to 20%"; IAA "not yet observed performance gains" [I1] |
| DPU (DShuffle) | BlueField-3 | Research shuffle offload | Paper (ATC '25) | vs Spark 2.4.3 [X1] |
| Theseus (Voltron Data) | GPU | Own engine, not Spark | Dead | TPC-H vs Photon at cost parity: 1.12x at SF1k, 4.1x at SF30k [T1] |
| Presto + Velox-cuDF | GPU | Prestissimo worker | IBM production use starting | "up to 6x" on cost×time [V2] |
| Fabric CoddSpeed | GPU (FPGA/ASIC planned) | Fabric DW coprocessor layer | Limited preview, separate higher-rate meter | "up to 30x on TPC-H 1TB" vs Fabric CPU [M1][M2] |

- **spark-rapids.** Sold claims: Snap 4x and 76% daily savings (NVIDIA-reported) [R5]; Cloudera "up to 4x" on Spark 4.1 [R6]. Misses: MaxNevermind aimed for 50% off on g4dn.12xlarge vs r6id.24xlarge and could not get it; NVIDIA replied the node choice was poor [R3]. A GKE team was promised 4–5x by NVIDIA's qualification tool and some jobs ran slower than on CPU [R4] (accessed 2026-11-04).
- **Gluten + Velox-cuDF** is moving fast: CPU/GPU hybrid scheduling 2026-08-14 (`65dff04b0`), async GPU shuffle read 2026-07-10 (`fc756ffae`), `allowCpuFallback` 2026-07-07 (`bd2c1ce6b`), all @ d64a51df. ByteDance's Bolt backend arrived 2026-09-10 and already carries `cudf.*` configs (`backends-bolt/.../BoltConfig.scala:810-825` @ d64a51df).
- **NVIDIA's advice to engine builders** is to "add GPU execution through the Velox cuDF backend"; it does not mention DataFusion or Comet [V3] (accessed 2026-11-04).

## 4. How each accelerator feeds its device
- **spark-rapids** (code @ ce634969).
  - Reader: `spark.rapids.sql.format.parquet.reader.type` defaults to AUTO: COALESCING for local files, MULTITHREADED for cloud stores. COALESCING "coalesces all the files assigned to a task into a single host buffer before sending it down to the GPU"; MULTITHREADED "reads each file in parallel and each file is sent to the GPU separately" (`RapidsConfSqlEntries.scala:485-509`). Host buffers are built in the JVM, decoded by libcudf.
  - No GDS on input: GDS spill removed in 2023 (`a90242e6b`, #9270). The CPU-scan "hybrid execution" via Gluten was dropped: "we don't see any usage of this feature" ([#15847](https://github.com/NVIDIA/cudf-spark/issues/15847); [PR #15849](https://github.com/NVIDIA/cudf-spark/pull/15849), merged 2026-09-04). A native `HybridScanReader` + GDS path is P0 and open ([#16073](https://github.com/NVIDIA/cudf-spark/issues/16073), [#16087](https://github.com/NVIDIA/cudf-spark/issues/16087), [#16088](https://github.com/NVIDIA/cudf-spark/issues/16088), opened 2026-09-22); #16088 targets "NVL72 GPFS", not cloud object stores (accessed 2026-11-04).
  - Shuffle: `spark.rapids.shuffle.mode` defaults to MULTITHREADED (thread-pooled file shuffle, "kudo" serializer). UCX (RDMA/NVLink) is opt-in; CACHE_ONLY is for tests (`RapidsConfEntries.scala:200-210`). UCX is maintained: 1.22.0 in `03ee1ea84`, error-path fixes through `5293a5f61`.
  - Cache: `spark.rapids.filecache.*` caches remote byte ranges on local disk; implementation not in the public tree, only tests (`tests/.../filecache`) @ ce634969. No device cache of decoded columns.
- **Gluten + Velox-cuDF** (docs @ d64a51df, `docs/backends/velox/get-started/VeloxGPU.md`).
  - Reader: scan is "❌ Not supported / In Development"; `cudf.enableTableScan` defaults to false (`VeloxConfig.scala:935`), so Velox decodes on the CPU and uploads. [#13122](https://github.com/apache/gluten/issues/13122): "Table scans still run on CPU" (accessed 2026-11-04).
  - Hybrid placement: AQE's `AdjustStageExecutionMode` marks a stage GPU-bound only if every operator in it is cuDF-tagged; GPU stages get their own resource profile, so scan stages run on CPU nodes and GPU stages on GPU nodes. `onlyOffloadJoinStage` limits the GPU to join stages. GPU shared at `amountPerTask=0.1`, `concurrentGpuTasks=3`.
  - Shuffle: "In a CPU/GPU hybrid workload the shuffle read phase is typically the bottleneck"; GPU tasks wait on CPU decompression and deserialization, hence the async reader (8 threads, 2 GB prefetch). Celeborn and Uniffle modules exist.
  - Cache: a Velox SSD/memory cache of remote file bytes with soft affinity (`VeloxLocalCache.md`).
  - Memory: RMM is global and "cannot align with Spark memory system"; spill "not supported".
- **Presto + Velox-cuDF** (closest published GPU data path at scale) [V2]: `CudfHiveConnector` reads Parquet into GPU memory via KvikIO, with GDS when available. The cuDF 25.04 Parquet reader ran about 10x below the I/O rate; a custom format reached about 22 GB/s. UCX exchange cut the 22-query SF1000 total from 828 s to 93 s (269 s without NVLink). On AWS at SF3K, cold 190 s vs hot 108 s. A GTC 2026 talk summary puts table scan plus Parquet I/O at 60–70% of Presto-GPU runtime and cites an undocumented per-VM S3 bandwidth ceiling, worked around with "parallel CPU instances that write directly to GPU memory" [V4] (secondhand) (accessed 2026-11-04).
- **Auron (CPU).** Native shuffle, plus Celeborn 0.5/0.6 and Uniffle shuffle managers (`thirdparty/auron-celeborn-0.6`, `thirdparty/auron-uniffle` @ cdd73ecf). In the abandoned GPU branch, bytes went through the JVM Hadoop-FS reader into pageable host buffers, then cuDF's chunked reader (§2).
- **Photon.** No device. Its I/O edge is the Databricks disk cache: local SSD, on by default on instance types with one, at most half the SSD, no system RAM [P3]. A third-party test found Photon "became worth it only after enabling cache utilization" ([CROZ](https://croz.net/databricks-engine-comparison/)) (accessed 2026-11-04).
- **Speedata.** The chip decompresses, decodes and filters Parquet in an on-chip pipeline; vendor claims about 200 decompression engines, "roughly 150 fully busy CPU cores" [S4]. In CoddSpeed's tests it read Parquet zero-copy from host memory "at PCIe 5.0 line rate for one-pass operations", data warm in CPU memory [S2] (accessed 2026-11-04).
- **Theseus (GPU, cloud, g6.4xlarge, 25 Gbps)** — the strongest published evidence on network starvation [T1]: replacing the Arrow S3 datasource with a custom object-store datasource cut runtime from 3,203.7 s to 812.5 s (−75%); byte-range pre-loading saved another 20%, compute-task pre-loading 19%, network compression 18%, GPUDirect RDMA only 6%. Decode on the GPU from ZSTD Parquet.
- **Microsoft CoddSpeed (Fabric).** Speedata's summary: "offloading only a few operators rarely yields meaningful speedups", because transfer costs outweigh gains [S2] (vendor summary, accessed 2026-11-04).
- **Algeon** uses KvikIO in compatibility mode with 64 I/O threads; GPU decode from local NVMe; no GDS [L1].
- **Hardware Parquet decode in the datapath.** Oasis (ETH, 2026) decodes Parquet on a SmartNIC between object storage and DuckDB; best case nearly doubles throughput; its prior work measured decode at 46% of TPC-H-on-Parquet runtime [O1] (accessed 2026-11-04).

## 5. Published cost per query: GPU vs CPU
| Source | GPU side | CPU side | Cost ratio (CPU / GPU) | Caveat |
|---|---|---|---|---|
| AWS EMR G7, 2026-08-25 [A1] | 8× g7.4xlarge, $26.35/h, 281 s | 8× m9gd/m8id/m8a.4xlarge, EMR Spark 3.5.6 | 1.42–1.54x | vs Spark, not Comet. Comet runs 2–2.5x faster than Spark [C1], which would put Comet at about $1.2–1.6 per run vs the GPU's $2.06 (estimate). |
| AWS data-on-EKS, 2026-01-12 [A2] | 4× g6.2xlarge (L4), $6.27 | 8× c5d.12xlarge: Gluten $18.40, Spark $31.28 | 2.9x vs Gluten | Cluster sizes differ (32 vs 384 vCPU), months apart, inconsistent runtimes. Gluten beat RAPIDS on q93 (14.5 s vs 118.2 s). |
| Theseus paper [T1] | g6.4xlarge L4 nodes | Photon on r7gd.12xlarge | 1.12x at SF1k ($0.65 vs $0.73); 4.1x at SF30k ($16.20 vs $66.91) | Vendor paper; 2x crossed only at 30 TB. |
| Presto GPU paper [V2] | 4× g7e.12xlarge / 1× g7e.48xlarge | r6i, m7a | 2.3x at SF1K, 6.3x at SF3K | Metric is cost × time (counts speed twice); cost-only lower (not given). Warm caches. |
| libcudf-rs [L2] | g4dn.xlarge T4 | m5.4xlarge DataFusion | 1.65x | One query (Q1), hackathon. |
| Sirius [Q1] | GH200 / g7e | DuckDB on CPU | 7–9x | Hot runs, tables pinned in GPU memory. |

- **No published study puts a GPU against Comet.** The only measured case with a cold GPU side and a native CPU engine, Theseus at SF1k, shows 1.12x (all accessed 2026-11-04).

## 6. Evidence against a GPU backend
- **Died or stalled.** Pavlo on Voltron Data: "They built a GPU-accelerated database (Theseus), but failed to launch it in a timely manner", part of "the trend of the inviability of GPU-accelerated databases" alongside HeavyDB's "sort-of acquihire" ([2025 retrospective](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html)); voltrondata.com did not resolve on 2026-11-04 (accessed 2026-11-04). Auron's libcudf bridge closed stale after one commit (§2). Bigstream: no activity after 2021 [B1].
- **Hardware offloads dropped for no gain.** Gluten removed Intel IAA (`d76865ab7`, 2025-08-18) after Intel "not yet observed performance gains" [I1]. spark-rapids removed GDS spill (2023) and its CPU-scan hybrid (2026) (§4).
- **Users who missed promised savings.** MaxNevermind [R3], the GKE team [R4]. q93 is 8x slower on RAPIDS than on Gluten [A2].
- **The baseline moves.** GPU wins are quoted against vanilla Spark; Comet gives 2.5x on Spark 4.1.3 [C1]; Spark 4.2 itself got about 1.35x faster via S3A, shrinking every accelerator's ratio [C1].
- **I/O, not compute, sets the ceiling.** Presto-GPU spends 60–70% of its time in scan [V4]; Theseus' largest cloud gain was an S3 client rewrite [T1]; Gluten-cuDF names shuffle read as its bottleneck [G1]. GPU wins shrink when cold: 1.77x hot vs cold for Presto [V2]; Sirius and Algeon quote hot or local-NVMe numbers. Batched one-pass ETL from S3 is the cold case.
- **Fragment offload is weak.** Per Speedata's summary of CoddSpeed, offloading a few operators rarely pays [S2]; Velox's fallback copies "can dominate query execution time" [V2]. Comet's mixed plans would split into fragments (estimate).
- **NVIDIA backs Velox-cuDF and spark-rapids, not DataFusion** [V3]. A Comet GPU backend would compete with both, and Gluten-cuDF already has per-stage CPU/GPU placement.

## 7. What this means for the decision
- The slot is empty, but the nearest attempt (Auron) stopped where the hard work starts, and NVIDIA plus IBM are funding the Velox path.
- Two usable designs exist: Gluten's per-stage hybrid (fully-GPU stages get a GPU resource profile, scans stay on CPU nodes), and Comet #7288's fragment seam (proto bytes plus Arrow C Stream in, Arrow stream out).
- Against the 2x bar: published GPU-vs-native-CPU cost ratios run 1.1x–2.9x, none against Comet, so they straddle 2x. Batching that shares scans helps a GPU only if the scan side is fixed first (estimate).

## Follow-up experiments
1. Rerun AWS's G7 TPC-DS 3 TB study [A1] with Comet 1.2.0 on the CPU clusters, same S3 bucket and region, cost per run. Settles whether any published GPU win survives Comet.
2. Run Algeon (DataFusion 55.1 + cuDF) against plain DataFusion 55 on TPC-H sf40 on the H100 VM, and DataFusion alone on the 128-core VM, from S3 and from local disk. Gives a DataFusion-55 GPU/CPU ratio at price parity without peacockdb runs.
3. Ask the Auron maintainers (richox) why #1001 stopped: build, data path, or no speedup.
4. Run Gluten-cuDF hybrid with `cudf.enableTableScan` false then true on TPC-H sf40, to cost a CPU scan stage plus upload for a GPU stage.
5. Measure Comet's object_store reads from Nebius S3 (per-file GETs, see [comet #7459](https://github.com/apache/datafusion-comet/issues/7459)) against a tuned parallel range reader, as in Theseus' 75% gain. Sizes the reader fix both CPU and GPU sides would get.
6. Prototype a `CometFragmentExec`-style cuDF fragment (scan, filter, project, aggregate) behind the #7288 seam; count host↔device bytes per batch on TPC-H q1/q6.

## Sources
- [C1] Comet #7451 TPC-DS on Spark 4.1.3/4.2.0: https://github.com/apache/datafusion-comet/issues/7451 ; #7288: https://github.com/apache/datafusion-comet/issues/7288 ; repo @ ab46419f: https://github.com/apache/datafusion-comet (accessed 2026-11-04)
- [A1] AWS Big Data Blog, EMR on G7, 2026-08-25: https://aws.amazon.com/blogs/big-data/gpu-accelerated-apache-spark-with-amazon-emr-and-nvidia-rtx-pro-4500-on-amazon-ec2-g7-instances-runs-up-to-3-7x-faster/ (accessed 2026-11-04)
- [A2] Data on EKS, RAPIDS benchmark: https://awslabs.github.io/data-on-eks/docs/benchmarks/spark-rapids-gpu-benchmark (accessed 2026-11-04)
- [A3] Data on EKS, Gluten vs Comet: https://awslabs.github.io/data-on-eks/docs/benchmarks/spark-gluten-velox-comet-benchmark (accessed 2026-11-04)
- [G1] Gluten VeloxGPU doc: https://github.com/apache/gluten/blob/d64a51dfd01bbe1b8e4adb6c1f74331fb75f531f/docs/backends/velox/get-started/VeloxGPU.md (accessed 2026-11-04)
- [G2] Gluten #13122: https://github.com/apache/gluten/issues/13122 (accessed 2026-11-04)
- [U1] Auron: https://github.com/apache/auron ; PR #1001: https://github.com/apache/auron/pull/1001 ; benchmarks: https://auron.apache.org/documents/benchmarks (accessed 2026-11-04)
- [L1] Algeon: https://github.com/algeon-dev/algeon ; https://algeon.dev/reference/benchmarks (accessed 2026-11-04)
- [L2] libcudf-rs: https://github.com/gabotechs/libcudf-rs (accessed 2026-11-04)
- [P1] Photon, SIGMOD 2022: https://www.databricks.com/wp-content/uploads/2022/07/Photon-A-Fast-Query-Engine-for-Lakehouse-Systems.pdf (accessed 2026-11-04)
- [P2] Databricks GPU compute: https://docs.databricks.com/aws/en/compute/gpu (accessed 2026-11-04)
- [P3] Databricks disk cache: https://docs.databricks.com/gcp/en/optimizations/disk-cache (accessed 2026-11-04)
- [S1] Speedata product: https://www.speedata.io/product (accessed 2026-11-04)
- [S2] Speedata on CoddSpeed: https://www.speedata.io/post/259-faster-what-microsoft-s-award-winning-sigmod-paper-says-about-the-speedata-apu (accessed 2026-11-04)
- [S3] SiliconANGLE, Speedata $44M: https://siliconangle.com/2025/06/03/data-analytics-chip-startup-speedata-closes-44m-funding-round/ (accessed 2026-11-04)
- [S4] Speedata on Parquet: https://www.speedata.io/post/efficiently-processing-apache-parquet-file-format (accessed 2026-11-04)
- [N1] AWS F2: https://aws.amazon.com/ec2/instance-types/f2/ ; NeuroBlade and Velox: https://www.neuroblade.com/news/neuroblade-and-velox-work-to-open-source-hw-acceleration-for-presto-velox-elevating-data-analytics-performance/ (accessed 2026-11-04)
- [D1] DualBird: https://dualbird.io/resources/blog/iceberg-compaction-benchmark-faster-spark-lower-cost ; https://aws.amazon.com/marketplace/pp/prodview-hbabl27uuq5ww (accessed 2026-11-04)
- [B1] Bigstream STAC 2021: https://stacresearch.com/system/files/resource/files/GSL-Spring2021-Bigstream.pdf (accessed 2026-11-04)
- [I1] Gluten IAA/QAT: https://gluten.apache.org/docs/v1.4.0/get-started/VeloxIAA.html ; Intel: https://intel.github.io/optimization-zone/software/gluten (accessed 2026-11-04)
- [X1] DShuffle, ATC '25: https://www.usenix.org/conference/atc25/presentation/ding (accessed 2026-11-04)
- [T1] Theseus paper: https://arxiv.org/html/2508.05029v1 (accessed 2026-11-04)
- [V1] NVIDIA/IBM Velox-cuDF blog: https://developer.nvidia.com/blog/accelerating-large-scale-data-analytics-with-gpu-native-velox-and-nvidia-cudf/ (accessed 2026-11-04)
- [V2] Accelerating Presto with GPUs: https://arxiv.org/html/2606.24647v1 (accessed 2026-11-04)
- [V3] NVIDIA FAQ, GPU path without forking: https://perspectives.nvidia.com/cudf/faq/we-want-to-add-a-gpu-execution-path-to-our-existing-query-engine-but-don-t-want-to-fork-the-mainline-6627a6/ (accessed 2026-11-04)
- [V4] GTC 2026 talk summary (secondhand): https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html (accessed 2026-11-04)
- [M1] CoddSpeed: https://www.microsoft.com/en-us/research/publication/coddspeed-hardware-accelerated-query-processing-in-microsoft-fabric/ (accessed 2026-11-04)
- [M2] Fabric query acceleration: https://learn.microsoft.com/en-us/fabric/data-warehouse/query-acceleration ; https://emptysqua.re/blog/sigmod-2026/ ; https://chatforest.com/builders-log/microsoft-fabric-coddspeed-gpu-data-warehouse-build-2026/ (accessed 2026-11-04)
- [O1] Oasis: https://arxiv.org/pdf/2608.02268 (accessed 2026-11-04)
- [Q1] Sirius: https://github.com/sirius-db/sirius ; https://arxiv.org/abs/2508.04701 (accessed 2026-11-04)
- [R3] spark-rapids discussion #11992: https://github.com/NVIDIA/cudf-spark/discussions/11992 (accessed 2026-11-04)
- [R4] GKE report: https://hackernoon.com/benchmarking-nvidia-rapids-on-gke-estimated-vs-actual-spark-speedups (accessed 2026-11-04)
- [R5] Snap (NVIDIA): https://blogs.nvidia.com/blog/snap-accelerated-data-processing (accessed 2026-11-04)
- [R6] Cloudera: https://www.manilatimes.net/2026/08/20/tmt-newswire/globenewswire/cloudera-teams-with-nvidia-to-lower-cloud-compute-spend-and-accelerate-apache-spark-pipelines/2409187 (accessed 2026-11-04)
- spark-rapids code @ ce6349699ccb3bd527c223c66ba24bd708e67e18: https://github.com/NVIDIA/cudf-spark ; issues #15847, #16073, #16087, #16088 (accessed 2026-11-04)
- DataFusion threads: https://github.com/apache/datafusion/discussions/8498 ; https://github.com/apache/datafusion/discussions/20030 ; https://github.com/apache/datafusion/issues/26633 (accessed 2026-11-04)
