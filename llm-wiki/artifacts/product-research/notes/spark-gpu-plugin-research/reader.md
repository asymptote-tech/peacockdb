# cudf-spark (ex spark-rapids) GPU Parquet read path: changes from 25.08 to 26.10

All links accessed 2026-10-29. Repo: github.com/NVIDIA/cudf-spark (renamed from NVIDIA/spark-rapids; old issue numbers carry over).
Code inspected at cudf-spark `main` HEAD **49d3be946ca9cd660c3b74b15a684a8aec92e058** (2026-10-29, "Fix Unhex tests on Spark 3.4.0 (#16654)"), shallow clone at
`/tmp/claude-1000/-media-data-peacockdb/ba2658c9-3cbe-4aa7-bcb8-c0a874d6fde0/scratchpad/ext/cudf-spark`. Older history checked in a 996-commit clone (back to 2026-03-25) at `.../scratchpad/ext/NVIDIA_cudf-spark.git`.

Release dates (GitHub releases API): 25.08.0 2025-08-20; 25.10.0 2025-10-23; 25.12.0 2025-12-22; 26.02.0 2026-02-27; 26.04.0 2026-04-21; 26.06.0 2026-06-23; 26.08.0 2026-08-20 (26.08.1 2026-08-28); 26.10.0 tagged only as `v26.10.0-rc`, CHANGELOG generated 2026-10-26 — [releases](https://github.com/NVIDIA/cudf-spark/releases). No 26.10 final release yet (as of 2026-10-29).

## 1. Release-note items touching the Parquet read path, per release

### Takeaway
Across seven releases, no CHANGELOG item targets the general CPU-side reader cost behind the 10x finding: Scala footer handling, host-buffer assembly and coalescing for local files. The scan work went to Delta deletion vectors, Iceberg, cloud I/O (S3/GCS "PerfIO", vectored and tail reads), host-memory flow control and bug fixes. The two biggest structural changes are: hybrid (Gluten/Velox CPU-decode) execution was **removed** in 26.10, and a new native cuDF `HybridScanReader` integration has **started** (one preparatory PR merged, with no production caller).

### Cited Findings
**25.10** ([CHANGELOG@v25.10.0](https://github.com/NVIDIA/cudf-spark/blob/v25.10.0/CHANGELOG.md)). The Performance section has nothing on scans. The reader items are mostly Delta DV support:
- PERFILE reader DV support [#12575](https://github.com/NVIDIA/cudf-spark/issues/12575) and [#13439](https://github.com/NVIDIA/cudf-spark/pull/13439); DV in the multithreaded reader [#13491](https://github.com/NVIDIA/cudf-spark/pull/13491); "Use iceberg FileIO to access parquet files" [#13138](https://github.com/NVIDIA/cudf-spark/issues/13138).
- New "Rapids file io layer" abstraction, merged 2025-08-19 [#13252](https://github.com/NVIDIA/cudf-spark/pull/13252). It was swapped for the JNI-provided `RapidsFileIO` interface on 2025-09-18 [#13441](https://github.com/NVIDIA/cudf-spark/pull/13441). The 25.10 placement is my estimate from the merge dates.

**25.12** ([CHANGELOG@v25.12.0](https://github.com/NVIDIA/cudf-spark/blob/v25.12.0/CHANGELOG.md))
- "Resource-bounded MultiFileCloudPartitionReader", merged 2025-10-28 [#13130](https://github.com/NVIDIA/cudf-spark/pull/13130), closing [#12919](https://github.com/NVIDIA/cudf-spark/issues/12919) ("Triple buffering: traffic control by a customized thread pool", listed under Performance). It adds a `ResourceBoundedThreadExecutor` and `HostMemoryPool` that cap host memory in the MULTITHREADED (cloud) Parquet reader. New configs: `spark.rapids.sql.multiThreadedRead.memBound` and `spark.rapids.sql.multiThreadedRead.memoryLimit` (default 0 = auto). This is flow control, not a speedup claim.
- "Add op time metric to Hybrid Scan" [#13656](https://github.com/NVIDIA/cudf-spark/pull/13656); DV read-performance analysis task [#13568](https://github.com/NVIDIA/cudf-spark/issues/13568).

**26.02** ([CHANGELOG@v26.02.0](https://github.com/NVIDIA/cudf-spark/blob/v26.02.0/CHANGELOG.md)). Only fixes: "missing RmmSpark calls for the coalescing reader" [#14026](https://github.com/NVIDIA/cudf-spark/pull/14026), and combining small files in the Delta multithreaded reader [#14271](https://github.com/NVIDIA/cudf-spark/pull/14271).

**26.04** ([CHANGELOG@v26.04.0](https://github.com/NVIDIA/cudf-spark/blob/v26.04.0/CHANGELOG.md))
- The DV-aware Delta reader moved to cuDF deletion-vector APIs, covering PERFILE [#14347](https://github.com/NVIDIA/cudf-spark/pull/14347), multithreaded [#14382](https://github.com/NVIDIA/cudf-spark/pull/14382) and coalescing [#14435](https://github.com/NVIDIA/cudf-spark/pull/14435). It is on by default [#14460](https://github.com/NVIDIA/cudf-spark/pull/14460).
- "multithreaded read limit defaults to wrong size, ignores non-pinned off heap memory" [#14292](https://github.com/NVIDIA/cudf-spark/issues/14292).
- FileCache uses RapidsFileIO [#14342](https://github.com/NVIDIA/cudf-spark/pull/14342), [#14253](https://github.com/NVIDIA/cudf-spark/issues/14253).
- Removed the `next()`/`readBuffersToBatch()` recursion in MultiFileCloudPartitionReaderBase, which fixed a StackOverflowError [#14284](https://github.com/NVIDIA/cudf-spark/pull/14284), [#14194](https://github.com/NVIDIA/cudf-spark/issues/14194).
- Bug: the cuDF DV reader returns wrong results with the NATIVE footer reader on split files [#14564](https://github.com/NVIDIA/cudf-spark/issues/14564).

**26.06** ([CHANGELOG@v26.06.0](https://github.com/NVIDIA/cudf-spark/blob/v26.06.0/CHANGELOG.md))
- Iceberg-only scan work: footer cache [#14645](https://github.com/NVIDIA/cudf-spark/pull/14645) (issue [#14064](https://github.com/NVIDIA/cudf-spark/issues/14064)); small-file combining [#14593](https://github.com/NVIDIA/cudf-spark/pull/14593) / [#14063](https://github.com/NVIDIA/cudf-spark/issues/14063); planning overhead [#14068](https://github.com/NVIDIA/cudf-spark/issues/14068); PerfIO S3 for Iceberg [#14591](https://github.com/NVIDIA/cudf-spark/issues/14591).
- "optimize iceberg read" [#14674](https://github.com/NVIDIA/cudf-spark/pull/14674), merged 2026-05-14, commit 5103b620c2636ce6564032cd2255703b46438cfa. It introduced `readVectored` into `GpuParquetScan.scala`, so it is the commit that first added that string to the file. PR numbers: EMR test averaged 477 s before and 389.8 s after over 5 runs, with `spark.rapids.perfio.s3.enabled=true` (NVIDIA-internal).
- Coalescing reader correctness fix for partitioned tables [#14831](https://github.com/NVIDIA/cudf-spark/issues/14831) / [#14841](https://github.com/NVIDIA/cudf-spark/pull/14841).
- **Regression introduced in 26.06:** the Hadoop `readVectored` fallback used 128 KiB copies. On NDS SF3000 q28 (EMR on EKS, 6× g6.4xlarge), 26.04.2 took 60–65 s and 26.06.0 took 420.9 s, and "scan buffer time" rose from about 1.2–1.4k s to 17,112 s. Fixed in 26.06.1 / 26.08 by [#15164](https://github.com/NVIDIA/cudf-spark/pull/15164) / [#15242](https://github.com/NVIDIA/cudf-spark/pull/15242) (commit e301446c7a030785e941f66d4caad07c745dc8b4). After the fix, the paired full-NDS delta vs 26.04.2 had a median of −0.19% — [#15163](https://github.com/NVIDIA/cudf-spark/issues/15163).

**26.08** ([CHANGELOG.md, Release 26.08 section](https://github.com/NVIDIA/cudf-spark/blob/main/CHANGELOG.md))
- "Enable optimized S3 tail reads for Iceberg Parquet footers" [#15384](https://github.com/NVIDIA/cudf-spark/pull/15384), commit c69e9220ad91adaefb2894b2b9e2be74647ebbd3. It reads the trailer with `readTail` and the footer body with `readVectored`. No perf numbers.
- GCS Range Copier [#15039](https://github.com/NVIDIA/cudf-spark/pull/15039) and GCS PerfIO auto-enable [#15130](https://github.com/NVIDIA/cudf-spark/pull/15130); the GCS/S3 httpClient configs are marked "Unreleased" in `sql-plugin-api/.../ConfVersionInfo.scala`. Also "cuda illegal memory access error while reading parquet files" [#13723](https://github.com/NVIDIA/cudf-spark/issues/13723). The only Performance item is a GpuProjectExec change [#14868](https://github.com/NVIDIA/cudf-spark/issues/14868).

**26.10** ([CHANGELOG.md, Release 26.10, generated 2026-10-26](https://github.com/NVIDIA/cudf-spark/blob/main/CHANGELOG.md))
- **"Remove hybrid execution support"** [#15849](https://github.com/NVIDIA/cudf-spark/pull/15849), commit 3580969fb1b47c9c58a04ff4f7daa2b8f56cccef, 2026-09-04. The rationale in [#15847](https://github.com/NVIDIA/cudf-spark/issues/15847): hybrid execution offloaded the Parquet scan to CPU through Gluten/Velox, but "depends on an old version of Gluten/Velox and we don't see any usage of this feature."
- Pinned pool init: pre-touch pages concurrently [#15407](https://github.com/NVIDIA/cudf-spark/pull/15407), and default `spark.rapids.memory.pinnedPool.parallelInit.threads` changed to "all" [#15706](https://github.com/NVIDIA/cudf-spark/pull/15706) / [#15681](https://github.com/NVIDIA/cudf-spark/issues/15681). These are listed under Performance and speed up startup, not per-scan cost.
- **Local-scan regression fix:** "Default 8 MiB Hadoop vectored-read buffer regresses local Parquet scans" [#15707](https://github.com/NVIDIA/cudf-spark/issues/15707). On a local `file:` benchmark, 128 KiB took 0.398 s vs 0.493 s for 8 MiB (the 8 MiB case was 23.9% slower). Fixed by [#15709](https://github.com/NVIDIA/cudf-spark/pull/15709), commit 5bdd43aeb07827e60a457b45230a2fab0c8c15de, 2026-08-21.
- Multithreaded-reader correctness and leak fixes [#16395](https://github.com/NVIDIA/cudf-spark/issues/16395), [#16396](https://github.com/NVIDIA/cudf-spark/issues/16396), [#16467](https://github.com/NVIDIA/cudf-spark/pull/16467). Native footer reader bugs: [#16128](https://github.com/NVIDIA/cudf-spark/issues/16128) (drops empty KV metadata) and [#16120](https://github.com/NVIDIA/cudf-spark/issues/16120) (IEEE 754 total column order).
- New metric: "decoded GPU batch bytes" for GPU scans [#15584](https://github.com/NVIDIA/cudf-spark/pull/15584); bytes-read accounting fixes [#15791](https://github.com/NVIDIA/cudf-spark/issues/15791), [#10181](https://github.com/NVIDIA/cudf-spark/issues/10181), [#15803](https://github.com/NVIDIA/cudf-spark/pull/15803).

**Not found in any 25.10–26.10 CHANGELOG:**
- a GDS/kvikio read path for Parquet input;
- a default change to `spark.rapids.sql.format.parquet.reader.type` (still AUTO → COALESCING or MULTITHREADED, picked by cloud scheme);
- a default change to `...reader.footer.type` (still AUTO);
- moving the coalescing or buffer assembly to native code.

Sources: the docs in [advanced_configs.md](https://github.com/NVIDIA/cudf-spark/blob/main/docs/additional-functionality/advanced_configs.md), and `RapidsConfSqlEntries.scala` lines 460–500 @49d3be94.

### Inferences
- For local Parquet on one GPU (the user's setup), the only changes that plausibly affect wall time are:
  - the 26.06 switch to `readVectored`, which first regressed and then was fixed;
  - the 26.10 local buffer-size fix;
  - pinned-pool init speed.
  
  None of these is aimed at a 10x CPU overhead (inference).

### Gaps
- 26.10 final release notes are not out; the list comes from the auto-generated CHANGELOG on 2026-10-26.
- I did not diff the 25.08 vs HEAD code line by line for small perf tweaks that were not labeled in the CHANGELOG.

## 2. Issues and PRs on CPU-bound or slow Parquet scan; anything matching the 10x finding

### Takeaway
No public issue reproduces the user's "reader overhead ≈ 10× GPU compute" finding. The closest NVIDIA-side admission and plan is the Sept 2026 epic **#16073** ("Productize Wild cuDF-Spark scale-up capabilities"). It makes "Native Parquet and GDS" (cuDF `HybridScanReader` + GDS) a P0 item, benchmarked on TPC-H's 22 queries against a prototype ("Wild V23") and a "Flux reference". It is still open.

### Cited Findings
- **Epic [#16073](https://github.com/NVIDIA/cudf-spark/issues/16073)** (open, 2026-09-22, by binmahone):
  - Goal: "scale-up cuDF-Spark from NVIDIA/cudf-spark main on an 8-node NVL72 allocation (32 B200 GPUs) with TPC-H SF30000, targeting a complete 22-query warm runtime within 1.6x of the retained Flux reference on the same hardware class."
  - Children, both open: "Integrate HybridScanReader into cuDF-Spark Parquet scans" [#16087](https://github.com/NVIDIA/cudf-spark/issues/16087) and "Qualify GDS transport for HybridScanReader on NVL72 GPFS" [#16088](https://github.com/NVIDIA/cudf-spark/issues/16088).
- **[#16087](https://github.com/NVIDIA/cudf-spark/issues/16087)** (open, P0, estimated 2–4 weeks): "Current cuDF-Spark main does not construct or orchestrate this reader." The plugin must translate projection and filters, fetch ranges, schedule metadata/filter/payload phases, and apply residual predicates.
- **[#16088](https://github.com/NVIDIA/cudf-spark/issues/16088)** (open, P0): wants HybridScanReader byte ranges populated "directly into device buffers through a qualified GDS path" using cuFile/KvikIO. It notes "Wild V23 proves a separate ParquetChunkedReader file-path GDS implementation can run on NVL72 GPFS". That implies an unpublished NVIDIA prototype with GDS-backed reads exists (inference).
- **First piece merged:** "Add spark-filter to cudf-ast conversion for hybrid parquet reader" [#16459](https://github.com/NVIDIA/cudf-spark/pull/16459), merged 2026-10-23, commit 16709e2b14abf426477d6cb94af084f1f2c3eed0 (author pmattione-nvidia). The PR says: "No production code calls the converter yet; this is the base of the hybrid-scan PR stack and the per-file reader PR will consume it." New file: `sql-plugin/src/main/scala/com/nvidia/spark/rapids/parquet/FilterToAstConverter.scala`.
- **Reader redesign RFC** [#15846](https://github.com/NVIDIA/cudf-spark/pull/15846) (open, 2026-09-01, liurenjie1024): "unified partition-reader framework" (`docs/rfc/rapids_table_scan_v2/design.md`).
  - Cites "maintenance and extensibility limitations of the current per-file, multi-threaded, and coalescing readers".
  - Proposes Scheduler/ReadHandle/Combiner/Decoder components plus a `HybridScheduler`.
  - Separately, a draft PR "[DO NOT REVIEW] Test new scan design" [#15201](https://github.com/NVIDIA/cudf-spark/pull/15201) (open, 2026-07-06).
- **Long-standing native footer plan** [#11954](https://github.com/NVIDIA/cudf-spark/issues/11954) (open since 2025-01-10, revans2): hand footer bytes to cuDF, get an I/O plan, fetch pages lazily; "should hopefully … reduce or eliminate the need for hybrid scan features". [#15167](https://github.com/NVIDIA/cudf-spark/issues/15167) (open, 2026-06-29, revans2): port the native footer optimization to Iceberg/Delta. It "can show a massive performance improvement on jobs that have a lot of columns/row groups not actually read". It also says to do it "unless the code to switch over to the cudf hybrid read shows up instead. That does the same kind of thing, just better."
- Open PR "Avoid redundant file status lookup for Parquet footer reads" [#16065](https://github.com/NVIDIA/cudf-spark/pull/16065) (2026-09-21; external-looking author WangGuangxin): one extra `getFileStatus` RPC per file during footer filtering. No numbers.
- Open scan perf bug: "Repeated Parquet scans with a non-pushdown regex are 1.77x slower in GPU mode" [#15683](https://github.com/NVIDIA/cudf-spark/issues/15683) (2026-08-16).
- Open "Reduce GCS task tail latency from large single range requests" [#15863](https://github.com/NVIDIA/cudf-spark/issues/15863); "Investigate and improve DBR 17.3 Delta GPU performance" [#15253](https://github.com/NVIDIA/cudf-spark/issues/15253).
- Open since 2023: "readFsTime is documented as being a part of bufferTime but that doesn't seem to be correct" [#9210](https://github.com/NVIDIA/cudf-spark/issues/9210). This is relevant when interpreting the reader's bufferTime metrics.
- Searches for "SF200", "GpuMultiFileReader overhead", "time spent in reader", "TPC-H Q1 slow" and "parquet reader performance H100" found no matching issue (gh search, 2026-10-29).

### Inferences
- NVIDIA's own scale-up effort targets TPC-H on a single node and treats native Parquet and GDS as P0. That implicitly confirms the current Scala multi-file reader path is a bottleneck for scale-up TPC-H. They have not published the size of that bottleneck (inference).

### Gaps
- Contents of "Wild V23" and the "Flux reference" (prototype numbers, per-query scan share) are not public.
- I could not find the user's own Sep–Oct 2025 report as a GitHub issue.

## 3. NVIDIA blog posts (2025–2026) on Parquet scan speedups

### Takeaway
The only Spark Parquet-scan blog post is from April 2025. It covers GPU decode kernels with data preloaded on the device, so it does not address CPU, footer or I/O overhead. I found no 2026 NVIDIA blog post on Spark Parquet scan speedups.

### Cited Findings
- "Accelerating Apache Parquet Scans on Apache Spark with GPUs", 2025-04-03, by Matt Ahrens, Paul Mattione and Dave Baranec ([NVIDIA blog](https://developer.nvidia.com/blog/accelerating-apache-parquet-scans-on-apache-spark-with-gpus/)).
  - It moves cuDF from a monolithic decode kernel to microkernels; up to **117% throughput gain** for chunked list-column reads (500 KB chunks).
  - Setup: RTX A5000, 512 MB Snappy Parquet "preloaded in device buffers".
  - CPU, JVM, footer and I/O costs are not discussed.
  - The forum mirror says the optimizations were completed by 25.02 ([forum](https://forums.developer.nvidia.com/t/accelerating-apache-parquet-scans-on-apache-spark-with-gpus/329247)).
- NVIDIA cuDF FAQ page (updated 2026-08-19): a Snap case reports a 4x faster runtime and 76% cost savings. Scan specifics are not given ([perspectives.nvidia.com](https://perspectives.nvidia.com/cudf/faq/we-are-struggling-to-meet-our-slas-while-spark-job-runtimes-keep-climbing-as-data-volumes-grow-adopt-82857b/)).

### Gaps
- A web search for a 2026 Spark RAPIDS Parquet or hybrid-scan blog found nothing (2026-10-29).

## 4. Code state at HEAD 49d3be94: is decode still libcudf via JNI, and is host buffer assembly still Scala?

### Takeaway
Yes on both counts. Every production Parquet reader (PERFILE, COALESCING, MULTITHREADED) still assembles host buffers in Scala and then calls libcudf through `ai.rapids.cudf.Table.readParquet` or `ParquetChunkedReader` (JNI). Footer parsing is Java by default, with a C++ footer filter (`ParquetFooter.readAndFilter` from spark-rapids-jni) selected by heuristic. cuDF's `HybridScanReader` Java API exists upstream but is not called.

### Cited Findings
All paths are in [cudf-spark @49d3be946ca9cd660c3b74b15a684a8aec92e058](https://github.com/NVIDIA/cudf-spark/tree/49d3be946ca9cd660c3b74b15a684a8aec92e058).

**Reader classes in `sql-plugin/src/main/scala/com/nvidia/spark/rapids/parquet/GpuParquetScan.scala` (4,099 lines):**
- `MultiFileParquetPartitionReader` (COALESCING, line 2745)
- `MultiFileCloudParquetPartitionReader` (MULTITHREADED, line 3436)
- `ParquetPartitionReader` (PERFILE, line 3994)

**Host buffer assembly (Scala):**
- `copyBlocksData` (line 1841) walks each column chunk of each row group and builds `CopyRange`s into a `HostMemoryOutputStream`, with a `FileCache` channel lookup per chunk.
- `readPartFile` (line 2308) calls `copyBlocksData`.
- The shared multi-file framework is still Scala: `sql-plugin/src/main/scala/com/nvidia/spark/rapids/GpuMultiFileReader.scala` (1,592 lines).

**GPU decode (JNI → libcudf):**
- line 3668: `Table.readParquet(opts, buffers:_*)` inside an NVTX `PARQUET_DECODE` / `GPU_DECODE_TIME` range;
- or the chunked path: line 32 imports `ai.rapids.cudf.ParquetChunkedReader`, and line 3864 calls `new JniParquetChunkedReader(chunkSizeByteLimit, maxChunkedReaderMemoryUsageSizeBytes, ...)`.

**Footer:**
- `readAndFilterFooter` (line 722) calls `ParquetFooter.readAndFilter(...)` (native) at line 738 when NATIVE is selected.
- `footerReaderHeuristic` (lines 298–320): AUTO picks NATIVE only if `readNodes/dataNodes ≤ 0.5 && dataNodes − readNodes > 10`; otherwise JAVA. NATIVE is never used when field IDs are present.
- Config `spark.rapids.sql.format.parquet.reader.footer.type` defaults to AUTO (`RapidsConfSqlEntries.scala:460–472`).
- Inference: for TPC-H tables (8–16 columns) and narrow queries, AUTO often picks JAVA.

**Not wired in:**
- No production Scala/Java file in `sql-plugin*` references `HybridScanReader`, `kvikio` or `cufile`. The only cuFile hit is `GpuDeviceManager.scala`, which handles spill/GDS init, not scan input (grep @49d3be94).
- `FilterToAstConverter.scala` (from #16459) exists but has no caller.

**Upstream cuDF:**
- Java bindings "Add Java Bindings for Hybrid Scan Parquet Reader" (rapidsai/cudf #22456), commit 2dfc8ad3a2491806c94f5357cb886c6d62f5017d, 2026-07-07 ([rapidsai/cudf](https://github.com/rapidsai/cudf/blob/main/java/src/main/java/ai/rapids/cudf/HybridScanReader.java)).
- Follow-ups: ffdcafc4 (2026-07-30, relaxed page-index requirements, #23386) and d45003e0 (2026-08-29, secondary-filter API, #23800). Seen in a local rapidsai/cudf clone.

**I/O layer:**
- The Parquet reads now go through `RapidsFileIO`/`RapidsInputFile` (from JNI; #13252/#13441).
- They use `readVectored` (since #14674, 26.06) and `readTail` (Iceberg footers, #15384).
- Optional PerfIO S3 (`spark.rapids.perfio.s3.enabled`, since 24.04) and GCS (unreleased).

### Inferences
- The pipeline the user instrumented is structurally unchanged as of 2026-10-29: JVM footer → Scala block copy into host buffers → (coalesce) → H2D → libcudf decode. The flow is footer → copy → coalesce → `readParquet`. Changes since 25.08 are in the I/O primitive (vectored reads), memory flow control and DV/Iceberg features, not in moving this pipeline to native code (inference from the code and CHANGELOGs).

### Gaps
- I did not verify that the 25.08 tag's `GpuParquetScan.scala` had the same structure. The 996-commit history reaches back only to 2026-03-25, but the user's 25.08 trace names the same classes.

## 5. Verdict and independent measurements

### Takeaway
**Not fixed** for the user's case (local Parquet, single GPU, TPC-H) as of 26.10-rc. There were only incremental I/O tweaks (some of them regressions plus fixes). NVIDIA opened a P0 effort in Sept 2026 to replace the scan with cuDF's native `HybridScanReader` plus GDS. That effort is at the stage of one merged preparatory PR (2026-10-23) and an unmerged RFC, so it will most likely land in 26.12 or later (estimate). No independent (non-NVIDIA) measurement of spark-rapids Parquet read overhead in 2025–2026 was found.

### Cited Findings
- Hybrid (Gluten/Velox CPU-decode) scan was removed, not improved: [#15847](https://github.com/NVIDIA/cudf-spark/issues/15847), [#15849](https://github.com/NVIDIA/cudf-spark/pull/15849).
- Native HybridScanReader integration is open and not yet wired: [#16087](https://github.com/NVIDIA/cudf-spark/issues/16087), [#16459](https://github.com/NVIDIA/cudf-spark/pull/16459).
- The only published scan-performance numbers in 2026 are NVIDIA-internal and cloud-focused:
  - NDS SF3000 q28: 60–65 s → 420.9 s → fixed [#15163](https://github.com/NVIDIA/cudf-spark/issues/15163);
  - Iceberg EMR: 477 s → 390 s [#14674](https://github.com/NVIDIA/cudf-spark/pull/14674);
  - local scan 0.493 s vs 0.398 s depending on buffer size [#15707](https://github.com/NVIDIA/cudf-spark/issues/15707).
- Independent academic work, not Spark-RAPIDS-specific: "Do GPUs Really Need New Tabular File Formats?" (DaMoN 2026, [arXiv 2602.17335](https://arxiv.org/abs/2602.17335)).
  - It cites a RAPIDS claim that about 85% of TPC-H runtime is Parquet scan (attributed to Kimball & Natarajan 2025, in a Velox-cuDF context, i.e. secondhand and not spark-rapids).
  - It reaches up to 125 GB/s effective read bandwidth on one A100 with GDS, SF300, using GPU-friendly Parquet layouts (row-group size, encoding, compression).
  - It does not measure spark-rapids.
- The Sirius paper (CIDR 2026, [arXiv 2508.04701](https://arxiv.org/abs/2508.04701)) reports Sirius-on-DuckDB TPC-H gains. I found no head-to-head Spark RAPIDS scan comparison in the search results.

### Inferences
- NVIDIA's scale-up TPC-H target ("within 1.6x of the retained Flux reference") plus making native Parquet/GDS P0 suggests that NVIDIA sees a large gap between spark-rapids main and a hand-built pipeline. That is consistent with the user's 22.6 s vs about 2.8 s observation (inference, not a measurement).

### Gaps
- There are no third-party benchmarks of spark-rapids 25.10–26.10 Parquet scan CPU overhead.
- No NVIDIA-published numbers exist for HybridScanReader in Spark.
- The "Kimball & Natarajan 2025" 85% figure was not traced to its primary source.
