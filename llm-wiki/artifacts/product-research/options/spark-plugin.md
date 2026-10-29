# spark-plugin

## Verdict

- **What:** a GPU backend for Apache Spark SQL. The best shape is a native engine behind Comet's Spark plan serializer, with a native Parquet reader and a decoded-column cache per executor. It is not a second spark-rapids.
- **Who needs it:** Spark teams with big shuffle-and-join bills (dogman123, MaxNevermind), and Spark Thrift/BI serving over billions of rows (Yu-Ting Lin). Demand is rated **strong** in demand.md, but NVIDIA sells this exact product.
- **Budget:** at the top of the budget: about 1.6× the engine on peacockdb (estimate), and Spark compatibility is the bulk of it. It fits only if the alpha is cut to read-only SQL on one workload's queries, with CPU fallback for everything else.

## Who needs it badly

- demand.md `## Ratings` row `spark-plugin`: **strong**. R/U/C 4/7/20, 13 posters. It calls this "the most painful and best-funded need, but NVIDIA sells this exact product" ([demand.md](../demand.md), accessed 2026-10-29).
- **Big shuffle/join bills.**
  - dogman123 runs EMR joins that yield 150B+ rows, and calls Glue "cost prohibitive" ([HN](https://news.ycombinator.com/item?id=43966388), 2025-05-12).
  - MaxNevermind (Maksim Konstantinov) aimed for a 50% cost cut on AWS g4dn against r6id. He missed it, and the thread has no resolution ([#11992](https://github.com/NVIDIA/cudf-spark/discussions/11992), 2025-01-21).
- **Teams evaluating the switch.**
  - u/PerfectAmbassador197 runs join-heavy PySpark on Azure Databricks, already on Photon, and wants measured speedups first ([r/dataengineering](https://www.reddit.com/r/dataengineering/comments/1p44fyv/), 2025-11-22).
  - A GKE practitioner was promised 4–5× by NVIDIA's qualification tool, and some jobs ran slower than on CPU ([HackerNoon](https://hackernoon.com/benchmarking-nvidia-rapids-on-gke-estimated-vs-actual-spark-speedups), 2026-10-20).
- **Production users with operational pain.**
  - ByteDance runs L20s on Kubernetes and hits native-memory OOM kills ([#16321](https://github.com/NVIDIA/cudf-spark/issues/16321), 2026-10-14).
  - Jeff Levesque has a cached-frame deadlock ([#16460](https://github.com/NVIDIA/cudf-spark/issues/16460), 2026-10-20), and runs a nightly 380M-row batch on 2× DGX Spark ([forum](https://forums.developer.nvidia.com/t/kernel-7-0-0-1019-nvidia-makes-apache-spark-rapids-gpu-work-1-5-2-4x-slower-on-dgx-spark-6-17-0-1032-with-the-same-driver-580-178-04-is-fine/386791), 2026-10-25).
  - On Databricks, joins kill executors ([forum](https://community.databricks.com/t5/data-engineering/executors-getting-killed-while-scaling-spark-jobs-on-gpu-using/td-p/121862), 2025-06-16).
- **Interactive serving and caching.**
  - Yu-Ting Lin serves Superset → Spark Thrift → RAPIDS over a 9.6B-row Delta table ([#11064](https://github.com/NVIDIA/cudf-spark/issues/11064), 2024-06-14). Thrift multi-session bugs block him ([#9966](https://github.com/NVIDIA/cudf-spark/discussions/9966), 2023-11-27).
  - Andy Gong wants inputs kept on the GPU between queries. NVIDIA said that is not supported ([#13435](https://github.com/NVIDIA/cudf-spark/discussions/13435), 2025-09-08).
- **IO-bound hardware.** Cloudera (melvin-koh) ran 8×H100 at 15% utilisation because HDFS fed them ([#11431](https://github.com/NVIDIA/cudf-spark/discussions/11431), 2024-09-06).
- **Caveat:** none of these posters states a budget, and most are evaluating rather than buying. All links accessed 2026-10-29.

## How it would work

- **Front end: reuse Comet's Spark side, not our own Scala.**
  - Apache DataFusion Comet (1.1.0, 2026-10-06) already rewrites Spark physical plans into a protobuf operator plan.
  - It runs that plan natively in Rust on DataFusion 55.2, with Spark-compatible expressions (`datafusion-comet-spark-expr`) ([Comet @7f73d86f](https://github.com/apache/datafusion-comet/tree/7f73d86fb841f5662f1343cf52b6e6b31eb17dfb), `native/Cargo.toml:47-52`; `native/core/src/execution/planner.rs:1631` `create_plan`, accessed 2026-10-29).
  - Comet is CPU-only, and no GPU plans were found ([releases](https://github.com/apache/datafusion-comet/releases), accessed 2026-10-29).
- **GPU stage.** One plugin step translates Comet's protobuf subtrees into peacockdb's FlatBuffers plan when every node and expression is supported. Otherwise the subtree stays on Comet's CPU path.
  - peacockdb's planner is also DataFusion, but on version 45, so an upgrade to 55 is part of the work.
- **Executor.** peacockdb's C++/cuDF session runs the stage inside each Spark task. Columns cross to Spark and to Comet as Arrow (C Data interface).
  - Spark-compatible kernels (casts, decimal, murmur3 hash, date/time) come from spark-rapids-jni (Apache-2.0) where possible. peacockdb's `partitioning.hpp` is already Spark-hash compatible ([inventory](../inventory.md)).
- **Data path, end to end:**

```
object store / HDFS / local NVMe
  → native range reads into pinned host buffers (no JVM, io_uring or kvikio/GDS)
  → [executor-local cache tier 1: compressed pages in pinned host RAM]
  → PCIe H2D
  → libcudf chunked or hybrid-scan Parquet decode
  → [cache tier 2: decoded columns in VRAM, keyed by (file, mtime, row group, column)]
  → cuDF operators
  → Arrow out → Spark/Comet shuffle → next stage
```

- **What is distinctive:** the cache survives across queries in a long-lived executor, such as a Thrift Server, a notebook session or an iterative job. spark-rapids lacks this (Q3 below).

## The human's questions

**1. Has the reader issue (Scala/JVM reader time about 10× GPU node time; SF200 Q1 22.6 s vs 1.8 s) been fixed since 25.08?**

- **No, not as of 26.10-rc.** The pipeline the human instrumented is structurally unchanged at cudf-spark `49d3be94` (2026-10-29):
  - JVM footer handling;
  - Scala `copyBlocksData` (`GpuParquetScan.scala:1841`) and `readPartFile` (`:2308`), which assemble host buffers;
  - coalescing, multithreaded and per-file reader classes (`:2745`, `:3436`, `:3994`) on `GpuMultiFileReader.scala` (1,592 lines);
  - then libcudf decode through JNI, `Table.readParquet` (`:3668`) or `ParquetChunkedReader` (`:3864`).
  - Source: [cudf-spark @49d3be94](https://github.com/NVIDIA/cudf-spark/tree/49d3be946ca9cd660c3b74b15a684a8aec92e058), accessed 2026-10-29.
- **The footer reader defaults to Java under AUTO.** The native C++ footer filter is chosen only if a query reads ≤ half the schema nodes and skips more than 10 (`GpuParquetScan.scala:298-320`). For narrow TPC-H tables that is usually Java (estimate).
- **No GDS or kvikio on the input path.** cuDF's `HybridScanReader` has Java bindings since 2026-07-07 ([cudf #22456](https://github.com/rapidsai/cudf/pull/22456), commit `2dfc8ad3`), but no production cudf-spark code calls it. Both checked by grep at `49d3be94`.
- **What changed, 25.10–26.10** ([CHANGELOG](https://github.com/NVIDIA/cudf-spark/blob/main/CHANGELOG.md), accessed 2026-10-29). Mostly Delta deletion vectors and Iceberg, plus:
  - Host-memory flow control for the cloud reader ([#13130](https://github.com/NVIDIA/cudf-spark/pull/13130), 25.12).
  - Vectored reads ([#14674](https://github.com/NVIDIA/cudf-spark/pull/14674), 26.06): Iceberg on EMR went 477 s → 390 s.
    - They first regressed NDS SF3000 q28 from 60–65 s to 421 s ([#15163](https://github.com/NVIDIA/cudf-spark/issues/15163)).
    - The fix then made local scans 24% slower, fixed in turn in 26.10 ([#15707](https://github.com/NVIDIA/cudf-spark/issues/15707), [#15709](https://github.com/NVIDIA/cudf-spark/pull/15709), commit `5bdd43ae`).
  - Faster pinned-pool start-up ([#15407](https://github.com/NVIDIA/cudf-spark/pull/15407)).
  - No changelog item targets the CPU cost of the local reader.
- **Hybrid scan was removed, not improved.** It offloaded the Parquet scan to CPU via Gluten/Velox. NVIDIA removed it in 26.10, citing "we don't see any usage of this feature" ([#15847](https://github.com/NVIDIA/cudf-spark/issues/15847); [#15849](https://github.com/NVIDIA/cudf-spark/pull/15849), commit `3580969f`).
- **A fix is now in flight.** Epic [#16073](https://github.com/NVIDIA/cudf-spark/issues/16073) was opened 2026-09-22.
  - Target: TPC-H SF30000 on 32 B200s, within 1.6× of an internal "Flux reference".
  - Its P0 items are native Parquet via `HybridScanReader` ([#16087](https://github.com/NVIDIA/cudf-spark/issues/16087): "main does not construct or orchestrate this reader") and GDS/KvikIO ([#16088](https://github.com/NVIDIA/cudf-spark/issues/16088)).
  - #16088 cites an unpublished "Wild V23" prototype that already reads through GDS.
  - Only one preparatory PR has merged, and nothing calls it yet ([#16459](https://github.com/NVIDIA/cudf-spark/pull/16459), 2026-10-23, commit `16709e2b`).
  - Expect it in 26.12 or later (estimate).
- **Outside measurements.**
  - No public issue reproduces the 10× finding, and no third party has measured reader overhead in 2025–26.
  - NVIDIA's only Spark Parquet blog (2025-04-03) measures decode kernels on data preloaded on the GPU. It says nothing about CPU, footer or IO cost ([blog](https://developer.nvidia.com/blog/accelerating-apache-parquet-scans-on-apache-spark-with-gpus/)).
- **What it means.** NVIDIA's own P0 epic is indirect confirmation that the reader is the bottleneck for scale-up TPC-H (inference). It also means the window for "we fixed the reader" as a product edge is roughly one or two NVIDIA releases.

**2. What do users say?**

- **Every headline number is vendor-published.** Examples (all accessed 2026-10-29):
  - Taboola's 20× ([NVIDIA blog](https://developer.nvidia.com/blog/gpu-integration-propels-data-center-efficiency-and-cost-savings-for-taboola/)). Its first attempt got only 3–5×, and some queries crashed until tuned.
  - NDS 5.7× speedup and 78% / 42% / 34–39% savings on Dataproc / EMR / Databricks-vs-Photon ([NVIDIA 2023 deck](https://nowlab.cse.ohio-state.edu/static/media/workshops/presentations/ties23/TIES-PEARC-23.pdf)).
  - Snap's 4× and 76% ([NVIDIA FAQ](https://perspectives.nvidia.com/cudf/faq/we-are-struggling-to-meet-our-slas-while-spark-job-runtimes-keep-climbing-as-data-volumes-grow-adopt-82857b/)).
  - No user-authored 2024–26 figure from ByteDance, Snap, Pinterest, Adobe or Cloudera was found.
  - "Up to 5×" and "4× less cost" exclude the AI Enterprise licence ([NVIDIA PDF](https://images.nvidia.com/aem-dam/Solutions/documents/public-sector-solution-overview-spark.pdf)).
- **Verification attempts mostly fail or go unresolved.**
  - #11992: the target was missed. NVIDIA replied that the node choice was poor and that the CPU node had 3× faster local disk.
  - GKE, 2026-10: some jobs ran slower than CPU. The causes were GPU node cold start, quota, pending executors, fallback (UDFs, text output, Iceberg), idle cores at one GPU per executor, and join OOM. The author's advice is to keep RAPIDS opt-in per job.
  - Discussion #8082: 4.9 min on GPU vs 4.3 min on CPU ([#8082](https://github.com/NVIDIA/spark-rapids/discussions/8082)).
- **Practitioners:**
  - threeseed: "I have run GPU accelerated Spark clusters for years… that's like 20% of what a typical job does" ([HN](https://news.ycombinator.com/item?id=43968287)).
  - winwang (ParaQuery): "Most of the issues come after the initial PoC" ([HN](https://news.ycombinator.com/item?id=43967477)).
  - andygrove (Comet): Spark compatibility "has been pretty much the bulk of the work" ([HN](https://news.ycombinator.com/item?id=43969326)).
- **Platform limits.** On Databricks the plugin uses one GPU per node, and Photon must be off ([NVIDIA docs](https://docs.nvidia.com/spark-rapids/user-guide/latest/getting-started/databricks.html); [CUDA-X docs](https://docs.nvidia.com/datascience/deployment/26.10/platforms/databricks/index.html)).
- **The baseline is moving.**
  - Comet 0.16 is 32–37% faster than Spark on TPC-DS 3 TB (AWS staff, [blog](https://datafusion.apache.org/blog/2026/06/03/comet-eks/), 2026-06-03).
  - Gluten is 9% faster than Comet overall ([Comet docs](https://datafusion.apache.org/comet/about/gluten_comparison.html)).
  - A GPU pitch now has to beat a 1.3–2× CPU-native baseline, not vanilla Spark (estimate).
  - No independent GPU-vs-CPU-native cost-per-query benchmark exists.

**3. Did spark-rapids build a RAM cache? How does it behave?**

- **Not of decoded columns, and not on the device.** NVIDIA's FAQ: "Can I cache data into GPU memory? No… It would require much larger changes to Apache Spark" ([FAQ](https://docs.nvidia.com/cudf-spark/latest/faq.html), accessed 2026-10-29). #13435 confirms the input is re-copied host→device on every query, even with the cache serializer.
- **ParquetCachedBatchSerializer** (`spark.sql.cache.serializer=com.nvidia.spark.ParquetCachedBatchSerializer`) handles `df.cache()` / InMemoryTableScan.
  - **Storage:** each batch is an on-heap `Array[Byte]` of Snappy Parquet, managed by Spark's BlockManager (memory, then disk) ([PCBS.scala:172-175](https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/sql-plugin/src/main/scala/com/nvidia/spark/rapids/parquet/ParquetCachedBatchSerializer.scala#L172-L175), `1b5576ad`).
  - **Every read:** a pageable H2D copy, Snappy decompression and a full Parquet decode on the GPU (`:404-470`).
  - **Pruning:** columns only. There is no predicate pruning until POC [#16660](https://github.com/NVIDIA/cudf-spark/issues/16660) (2026-10-29): 256 MiB per task read in about 0.2 s on the GPU vs 5.4 s on the CPU, on a local[2] run.
  - **Known problems:** a fixed 32 MB on-heap buffer per batch causes GC pressure ([#14351](https://github.com/NVIDIA/cudf-spark/issues/14351)); a fallback pipeline slowed to about 2 min ([#5975](https://github.com/NVIDIA/cudf-spark/issues/5975), open since 2022); "rethink the format" ([#7998](https://github.com/NVIDIA/cudf-spark/issues/7998), open).
  - **Config:** there is no `spark.rapids.sql.cache.*` config.
- **`spark.rapids.filecache.*`** (23.06, off by default):
  - caches raw footers and byte ranges of remote files on local disk ([doc](https://docs.nvidia.com/cudf-spark/latest/additional-functionality/filecache.html));
  - a hit still pays a disk read, H2D and a full decode;
  - its implementation is not in the public tree, no performance numbers are published, and the doc warns it can be slower on slow disks.
- **Other reuse.**
  - Broadcast hash-table reuse ([#14680](https://github.com/NVIDIA/cudf-spark/pull/14680), merged 2026-09-15, default off): 1.2–1.6× on synthetic tests, nothing significant on NDS.
  - `memory.gpu.unspill.enabled`.
  - No compatibility statement for the Databricks disk cache was found. The plugin's own GPU scan likely bypasses it (estimate).
- **How it behaves under pressure.** Cached blocks live in Spark's memory manager, outside the plugin's device→host→disk spill framework.
  - #16460 shows the clash: Spark evicts a GPU broadcast to make room for a cached block, then waits on the GPU semaphore while holding its memory lock.
  - The executor stops heartbeating and is lost after 600 s. It is reported unchanged in 26.08.1.

**What a newcomer could offer that spark-rapids, Gluten-cuDF and Comet do not**

- A reader with no JVM in it, today:
  - spark-rapids' native reader is a P0 epic with nothing wired yet;
  - Gluten-cuDF still scans on the CPU and passes 48/103 TPC-DS SF1 queries without fallback ([gluten #13122](https://github.com/apache/gluten/issues/13122), 2026-09-24; [VeloxGPU.md](https://github.com/apache/gluten/blob/381c806ec2d65ed3262b67407f2e604902f11e49/docs/backends/velox/get-started/VeloxGPU.md));
  - Comet has no GPU.
- **A decoded-column cache in VRAM and pinned RAM that survives across queries** in Thrift, notebook and iterative executors. NVIDIA says this needs "much larger changes to Spark". Comet's in-memory cache is CPU-only and experimental ([comet #5051](https://github.com/apache/datafusion-comet/pull/5051)). Gluten caches raw file bytes only ([VeloxLocalCache.md](https://github.com/apache/gluten/blob/381c806ec2d65ed3262b67407f2e604902f11e49/docs/backends/velox/get-started/VeloxLocalCache.md)).
- **One memory manager.** The device cache, spill and Spark's eviction would sit under one accountant, which avoids the #16460 class of deadlock (design claim, estimate).
- **A GPU backend for the Comet user base,** with Comet's fallback and Spark semantics for free.
- **Honest per-query routing and a "proved it ran on GPU" report.** This answers the GKE and #11992 complaints.
- **What a newcomer cannot offer:**
  - Spark-compatibility breadth;
  - managed-platform deals: Databricks, EMR, Dataproc, Cloudera and Oracle all ship NVIDIA's plugin ([nvidia.md](../nvidia.md));
  - a fix for the Databricks one-GPU, Photon-off limits.

## Data path and cache

| Link | Bandwidth | Source |
|---|--:|---|
| NIC, p5.48xlarge (8×H100) | 3,200 Gbps EFA ≈ 50 GB/s per GPU; 800 Gbps IP ≈ 12.5 GB/s per GPU | [AWS EFA](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/efa-acc-inst-types.html) |
| NIC, g6.48xlarge (8×L4) | 100 Gbps ≈ 1.6 GB/s per GPU | [AWS g6](https://aws.amazon.com/ec2/instance-types/g6/) |
| NVMe | about 7 GB/s per drive (estimate); g6.48xlarge has 8×940 GB | same |
| PCIe H2D, H200 measured | 47.3 GB/s pinned, 10.6 GB/s pageable | [inventory](../inventory.md) |
| GPU Parquet decode | 6.8–8.7 GB/s in peacockdb today; up to 125 GB/s with GPU-friendly files (A100 + GDS) | [inventory](../inventory.md); [arXiv 2602.17335](https://arxiv.org/abs/2602.17335) |
| HBM | 3.35 TB/s (H100 SXM); 4,484 GB/s measured read on H200 | [NVIDIA](https://www.nvidia.com/en-us/data-center/h100/); [inventory](../inventory.md) |

- **The thesis holds for Spark on GPUs, with one big exception.**
  - **It holds:** HBM is 70–300× the per-GPU NIC and 7× pinned PCIe.
    - The human's PoC loaded longer than it computed on 11 of 12 SF20 queries ([evidence](../evidence-in-hand.md)).
    - HDFS held 8×H100 at 15% (#11431).
    - spark-rapids adds a pageable, JVM-assembled host stage on top (PCBS on-heap bytes; Q1 22.6 s vs 2.8 s for the PoC's load plus compute).
    - A GPU that re-reads every query is capped at NIC or decode speed, roughly 1.6–12 GB/s per GPU, which a 64-core CPU engine on local NVMe can match (estimate).
  - **The exception:** Spark's paying users mostly run one-pass batch ETL (dogman123, MaxNevermind, ByteDance, Taboola). Each byte is read once per job, so a cross-query cache does nothing for them.
    - For them the fix is a fast native reader (pinned buffers, GDS, compressed transfer with GPU decompression) and fewer shuffle bytes. Not a cache.
- **The cache that is needed** is two-tier per executor:
  - pinned host RAM holding compressed pages (2–4× capacity, about 47 GB/s to the device);
  - VRAM holding decoded hot columns (HBM speed);
  - keyed by file, mtime, row group and column; evicted by the same accountant that does spill.
- **Who it pays for:** long-lived executors only. That means Thrift/BI (Lin), notebooks and iterative jobs (Andy Gong). Spark's dynamic allocation kills executors and the cache with them, so the product must pin executors (estimate).

## Build cost to alpha

Unit: peacockdb engine = 22,830 lines ([inventory](../inventory.md)). Sirius sizes are from [sirius.md](../sirius.md) `## Sirius components`. Alpha scope: read-only Spark SQL, Parquet, scan/filter/project/agg/join/sort, CPU fallback via Comet, one GPU per executor.

| Component | On peacockdb: reuse/adapt/new, size | On Sirius: reuse/adapt/new, size |
|---|---|---|
| Spark plan capture + fallback | reuse Comet (Scala 67k lines + native, external); new glue to claim GPU subtrees, about 2k | reuse Gluten's Spark→Substrait (external); new Gluten backend glue, about 4k |
| Plan translation into the engine | adapt translator to Comet protobuf or DF55 plans; DataFusion 45→55 upgrade; about 5k new | reuse Sirius Substrait FFI (2,133), but it goes through DuckDB re-planning, with wrong-answer bugs (#1963); fix, about 4k |
| Spark semantics (casts, decimal, ANSI, nulls, dates, hash) | new C++ plus spark-rapids-jni kernels, about 8k | new inside DuckDB-planned operators, about 10k (DuckDB semantics differ) |
| Operators | reuse node vocab, executor, join session (≈13k); adapt for missing nodes (window) about 2k | reuse joins, agg, sort, filter (≈19k) |
| Arrow in/out with Spark/Comet, per-task session | new, about 3k | new, about 3k |
| Native Parquet reader (pinned, chunked/hybrid, object store) | new, about 4k (today pageable `read_parquet`, local only) | reuse scan + I/O (29.5k) |
| Cross-query cache, 2 tiers | new, about 3k | adapt `pin_table` to automatic, evictable caching, about 2k |
| Memory: spill + Spark memory-manager bridge | new, about 4k, or borrow cuCascade | adapt cuCascade tiers to Spark, about 2k |
| Packaging, conf, Spark-version shims, test harness vs Spark | new, about 4k | new, about 4k |

Total new work: 1.6× engine on peacockdb, 1.3× on Sirius (estimate)

- **The hard part is Spark semantics on the GPU.** Every cast, decimal overflow, null rule and timezone must match Spark exactly, and a single mismatch is a wrong answer.
  - spark-rapids spent years on this.
  - Comet's semantics run only on the CPU, so the GPU side must re-implement each one.
- **Second hard part: a DataFusion upgrade across ten major versions** (45 → 55.2, `native/Cargo.toml:47` at Comet `7f73d86f`).
- **Sirius is cheaper on paper but weak in practice.** Spark plans would pass Gluten → Substrait → DuckDB → Sirius, which is two semantic translations with no CPU fallback inside Sirius. sirius.md calls this path the weakest part of Sirius.

## Adoption friction

- **Low in form.** It is a jar plus native libs set through `spark.plugins` and conf, inside the Spark users already run. This is the biggest advantage over every new-system option.
- **High in practice.**
  - Managed platforms already bundle NVIDIA's plugin.
  - Databricks forces Photon off and one GPU per node for any GPU plugin.
  - Air-gapped clusters cannot fetch jars ([Databricks forum](https://community.databricks.com/t5/data-governance/cannot-set-spark-plugins-com-nvidia-spark-sqlplugin-config/td-p/125570)).
  - Users must trust an unknown native lib with Spark-exact answers.
- **Riding on Comet adds a dependency** (Comet 1.x) and a second project's release cadence.
- **Proving value needs the user's own jobs.** Qualification-tool promises already failed users once (GKE, #11992).

## Platform risk

- **Highest of any option.** It goes head-on with NVIDIA's own sold product.
  - The plugin "is exclusively available with NVIDIA AI Enterprise", with support SLAs, and was renamed "NVIDIA cuDF plugin for Apache Spark" ([nvidia.md](../nvidia.md) [7][27]).
  - Every managed Spark runs it: Databricks, EMR, Dataproc, Oracle and Cloudera (nvidia.md [54]–[57], [82]).
  - Project Aether automates migration onto it ([NVIDIA blog](https://developer.nvidia.com/blog/migrate-apache-spark-workloads-to-gpus-at-scale-on-amazon-emr-with-project-aether/)).
  - NVIDIA also ships agent skills that rewrite UDFs for the GPU ([skills/](https://github.com/NVIDIA/cudf-spark/tree/main/skills)).
- **The reader edge is on NVIDIA's roadmap** as P0 (#16073, #16087, #16088). The cache edge is not: the FAQ says it needs larger Spark changes. That edge is the more durable one (estimate).
- **A second free path exists.** Gluten-cuDF on IBM's Velox fork has NVIDIA, IBM and Meta working on Velox-cuDF ([nvidia.md](../nvidia.md) [29][30]).
- **Comet could add a GPU backend itself.** No sign of that today.
- **ParaQuery shows the ceiling for a small player.** It wraps spark-rapids as a managed service, lists one employee and has no 2026 news ([YC](https://www.ycombinator.com/companies/paraquery), accessed 2026-10-29).
- **libcudf breaks every two months.** NVIDIA-staffed hosts get coordinated changes ([nvidia.md](../nvidia.md)).

## Funding story

- **What a design partner would say** (estimate):
  - Lin-type: "If Superset over our Delta table answers in under a second without Thrift crashing, we switch executors."
  - MaxNevermind-type: "Show me 50% off my bill on my job, net of licence."
  - Neither has said this yet. Both are first conversations.
- **The pitch:** "The GPU backend for the open Spark-native stack. Comet runs Spark on DataFusion; we add the GPU and a cache that keeps hot data in VRAM. No AI Enterprise licence, works where NVIDIA's plugin falls short (serving, repeated queries)."
- **What the round is raised on:**
  - a measured cost-per-query win on one partner's real Spark workload against Comet/Photon, not against vanilla Spark;
  - plus a Thrift/BI latency demo.
  - Spark spend is the largest budget in this research (demand.md), which helps the story.
- **The investor's objection** will be "NVIDIA gives this away and sells support." The answer has to be the cache and serving niche, which is narrower than "accelerate Spark."

## Evidence against

- **NVIDIA owns the slot.** It sells it, every cloud ships it, and it is now fixing the reader itself (#16073). A newcomer's reader advantage may last 2–6 months (estimate).
- **Compatibility is the product, and it is the bulk of the work.** It is the part peacockdb has least of. peacockdb has no Spark semantics, plans on DataFusion 45, has no spill, no object store and no multi-GPU ([inventory](../inventory.md)). The alpha is past budget unless it is scoped to one partner's queries.
- **The cache does not help the people with the biggest bills.** One-pass ETL reads each byte once. The cache helps BI and iterative users, of whom about two exist in the evidence (Lin, Andy Gong). demand.md rates the cache need (lean) only **medium**.
- **GPU wins are shrinking against CPU-native engines.** Comet and Gluten give 1.3–2×. Users often fail to save money even with NVIDIA's help (#11992, GKE), and operations, not the engine, dominate their failures.
- **Users are evaluators.** No poster has a budget and a deadline (demand.md `## Evidence against`).
- **The Spark ecosystem tax is real.** It means Spark 3.x/4.x shims, Databricks runtime forks, and Delta/Iceberg readers. spark-rapids carries per-version shims for this.

## Ratings

- Who needs it badly: **strong**. demand.md rates spark-plugin strong: 13 posters, painful bills and failures.
- Build cost: **weak**. About 1.6× on peacockdb (estimate), dominated by Spark semantics and a DataFusion upgrade. It sits at the top of the 1–2× budget, with high risk of overrun.
- Data path: **medium**. A native reader plus a two-tier cache fixes serving and repeated queries. One-pass ETL stays bound by NIC and decode.
- Adoption friction: **medium**. A plugin into existing Spark, but platforms bundle NVIDIA's plugin and Databricks limits GPUs.
- Platform risk: **weak**. Head-on with NVIDIA's sold product, which is fixing the reader now. Gluten-cuDF is a second free path.
- Funding story: **medium**. The biggest budgets and a clear "no licence, plus cache" pitch, but no partner yet, and the investor objection is obvious.

## Follow-up experiments

1. **Re-run the reader study on 26.10** (local Parquet, SF200, one H100), with the human's patch. This settles whether the 10× still holds, and what share the footer, the copy and the decode each take.
2. **PCBS vs re-read vs a pinned/VRAM copy, warm.** Run TPC-H Q1/Q3 on one H100 to bound what a cross-query cache saves inside Spark.
3. **Comet CPU vs spark-rapids vs peacockdb hand-off** on one MaxNevermind-style ETL job, with cost per run including the licence. Is there a GPU win left against Comet?
4. **NIC-bound run.** Measure from S3 on g6 vs p5 to see where per-GPU network bandwidth caps scan throughput.
5. **Prototype Comet protobuf → peacockdb plan for TPC-H 22.** Count unsupported expressions and Spark-semantics mismatches against Spark results. This sizes the hard part.
6. **Thrift Server serving test.** Run 10 concurrent Superset-style queries on a 1B-row table: p50/p95 latency with and without a VRAM cache.

## Sources

All accessed 2026-10-29 unless noted.

- Local inputs: [demand.md](../demand.md), [nvidia.md](../nvidia.md), [sirius.md](../sirius.md), [inventory.md](../inventory.md), [evidence-in-hand.md](../evidence-in-hand.md), [discovery.md](../discovery.md), [sources/](../sources/)
- cudf-spark code at `49d3be946ca9cd660c3b74b15a684a8aec92e058` and `1b5576adb95941f57bfd2ab22b96435d7057ecf8`: https://github.com/NVIDIA/cudf-spark/tree/49d3be946ca9cd660c3b74b15a684a8aec92e058 ; CHANGELOG https://github.com/NVIDIA/cudf-spark/blob/main/CHANGELOG.md ; releases https://github.com/NVIDIA/cudf-spark/releases
- cudf-spark issues and PRs: https://github.com/NVIDIA/cudf-spark/issues/ — 16073, 16087, 16088, 15847, 15163, 15707, 16660, 14351, 5975, 7998, 16460, 16321, 11064 ; PRs https://github.com/NVIDIA/cudf-spark/pull/ — 13130, 14674, 15407, 15709, 15849, 16459, 14680
- cudf-spark discussions: https://github.com/NVIDIA/cudf-spark/discussions/13435 , /11992 , /11431 , /9966 ; https://github.com/NVIDIA/spark-rapids/discussions/8082
- cudf-spark docs: FAQ https://docs.nvidia.com/cudf-spark/latest/faq.html ; filecache https://docs.nvidia.com/cudf-spark/latest/additional-functionality/filecache.html ; cache serializer https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/docs/additional-functionality/cache-serializer.md
- cuDF HybridScanReader Java bindings: https://github.com/rapidsai/cudf/pull/22456 (commit 2dfc8ad3)
- NVIDIA Parquet-scan blog, 2025-04-03: https://developer.nvidia.com/blog/accelerating-apache-parquet-scans-on-apache-spark-with-gpus/
- Vendor claims: Taboola https://developer.nvidia.com/blog/gpu-integration-propels-data-center-efficiency-and-cost-savings-for-taboola/ ; NDS deck https://nowlab.cse.ohio-state.edu/static/media/workshops/presentations/ties23/TIES-PEARC-23.pdf ; Snap https://perspectives.nvidia.com/cudf/faq/we-are-struggling-to-meet-our-slas-while-spark-job-runtimes-keep-climbing-as-data-volumes-grow-adopt-82857b/ ; https://images.nvidia.com/aem-dam/Solutions/documents/public-sector-solution-overview-spark.pdf
- GKE practitioner, 2026-10-20: https://hackernoon.com/benchmarking-nvidia-rapids-on-gke-estimated-vs-actual-spark-speedups
- HN threads: https://news.ycombinator.com/item?id=43966388 , 43968287 , 43967477 , 43969326 ; Reddit https://www.reddit.com/r/dataengineering/comments/1p44fyv/
- NVIDIA forum and Databricks community: https://forums.developer.nvidia.com/t/kernel-7-0-0-1019-nvidia-makes-apache-spark-rapids-gpu-work-1-5-2-4x-slower-on-dgx-spark-6-17-0-1032-with-the-same-driver-580-178-04-is-fine/386791 ; https://community.databricks.com/t5/data-engineering/executors-getting-killed-while-scaling-spark-jobs-on-gpu-using/td-p/121862 ; https://community.databricks.com/t5/data-governance/cannot-set-spark-plugins-com-nvidia-spark-sqlplugin-config/td-p/125570
- Databricks limits: https://docs.nvidia.com/spark-rapids/user-guide/latest/getting-started/databricks.html ; https://docs.nvidia.com/datascience/deployment/26.10/platforms/databricks/index.html
- Comet at `7f73d86fb841f5662f1343cf52b6e6b31eb17dfb`: https://github.com/apache/datafusion-comet ; EKS blog https://datafusion.apache.org/blog/2026/06/03/comet-eks/ ; Gluten comparison https://datafusion.apache.org/comet/about/gluten_comparison.html ; in-memory cache https://github.com/apache/datafusion-comet/pull/5051
- Gluten GPU: https://github.com/apache/gluten/issues/13122 ; https://github.com/apache/gluten/blob/381c806ec2d65ed3262b67407f2e604902f11e49/docs/backends/velox/get-started/VeloxGPU.md ; https://github.com/apache/gluten/blob/381c806ec2d65ed3262b67407f2e604902f11e49/docs/backends/velox/get-started/VeloxLocalCache.md
- ParaQuery: https://www.ycombinator.com/companies/paraquery ; https://news.ycombinator.com/item?id=43964505
- Project Aether: https://developer.nvidia.com/blog/migrate-apache-spark-workloads-to-gpus-at-scale-on-amazon-emr-with-project-aether/ ; UDF skills https://github.com/NVIDIA/cudf-spark/tree/main/skills
- Bandwidth: https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/efa-acc-inst-types.html ; https://aws.amazon.com/ec2/instance-types/g6/ ; https://www.nvidia.com/en-us/data-center/h100/ ; https://arxiv.org/abs/2602.17335
