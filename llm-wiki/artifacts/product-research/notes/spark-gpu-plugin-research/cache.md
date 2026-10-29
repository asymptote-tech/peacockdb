# Cross-query caching in NVIDIA spark-rapids (NVIDIA/cudf-spark) and competitors

All links accessed 2026-10-29. Code claims refer to NVIDIA/cudf-spark master at
`1b5576adb95941f57bfd2ab22b96435d7057ecf8` (committed 2026-10-28), read from a shallow clone at
`.../scratchpad/ext/NVIDIA_cudf-spark.git`. Abbreviation: `C=https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8`.
Other clones: apache/gluten `381c806ec2d65ed3262b67407f2e604902f11e49`, facebookincubator/velox `37931b32a83a080a1ebb797afa2dc00b817af120`, sirius-db/sirius `b9e05ffc653c8ed031c789e5c1415ac1186e0619`.

## Q1. ParquetCachedBatchSerializer (PCBS): what it stores, how it reads, limits, bugs, numbers

### Takeaway
PCBS stores each cached batch as a Snappy-compressed Parquet byte array on the **JVM heap / Spark BlockManager (host, then disk per StorageLevel)**, never on the device. With the plugin on, encoding runs on the GPU (cudf Parquet writer) and every read copies those bytes to the GPU and runs the cudf Parquet decoder again, on every query. NVIDIA confirmed (2025-09) that you cannot cache in GPU memory and that input is re-transferred H2D between queries. Column pruning works on read. Predicate (batch) pruning does not yet exist; a POC was filed on 2026-10-29.

### Cited Findings
- Enable with `spark.sql.cache.serializer=com.nvidia.spark.ParquetCachedBatchSerializer`, a static conf (needs Spark 3.1.1 or later). It shipped in plugin 0.4+. It works whether or not `spark.rapids.sql.enabled` is set: on the GPU when the plugin is on, otherwise "the cached objects will still be compressed on the CPU". With the GPU path, `spark.sql.inMemoryColumnarStorage.enableVectorizedReader` is ignored. On the GPU, MapType and BinaryType are unsupported and fall back to CPU caching. — [cache-serializer.md @1b5576a](https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/docs/additional-functionality/cache-serializer.md)
- Storage object: `case class ParquetCachedBatch(numRows: Int, buffer: Array[Byte]) extends CachedBatch { sizeInBytes = buffer.length }`. This is an on-heap byte array that Spark's MemoryStore/DiskStore manages like any cached block. — [ParquetCachedBatchSerializer.scala L172-175](https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/sql-plugin/src/main/scala/com/nvidia/spark/rapids/parquet/ParquetCachedBatchSerializer.scala#L172-L175)
- Write path (GPU): a CPU batch is first copied to the GPU (`putOnGpuIfNeeded`). `Table.writeParquetChunked` then writes Parquet into a host buffer consumer. Compression is `SNAPPY` if Spark's `useCompression` (`spark.sql.inMemoryColumnarStorage.compressed`) is set, else `NONE`. — [same file L247-300, L379-402](https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/sql-plugin/src/main/scala/com/nvidia/spark/rapids/parquet/ParquetCachedBatchSerializer.scala#L379-L402)
- Read path: `gpuConvertCachedBatchToColumnarBatch` "decodes the CachedBatch leaving it on the GPU". For each batch it takes the GPU semaphore and calls `Table.readParquet(parquetOptions.includeColumn(selected…), parquetCB.buffer, 0, sizeInBytes)`. That is a full H2D copy plus a GPU Parquet decode of the selected columns on every scan. When no columns are selected (count-only), it returns row-count batches without decoding. — [same file L404-470](https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/sql-plugin/src/main/scala/com/nvidia/spark/rapids/parquet/ParquetCachedBatchSerializer.scala#L404-L470)
- FAQ: "the cache is GPU accelerated but still stored on the host memory". "Can I cache data into GPU memory? No, that isn't currently supported. It would require much larger changes to Apache Spark to be able to support this." — [cudf-spark FAQ](https://docs.nvidia.com/cudf-spark/latest/faq.html) (page "last updated Oct 23, 2026")
- Discussion #13435, "Cache input data in GPU memory between consecutive queries" (2025-09-08). Matt Ahrens (NVIDIA) answered on 2025-09-09: "Caching data onto GPU memory is not currently supported and input data will need to be transferred from host memory to device memory between consecutive queries even if the RAPIDS Cache Serializer is used." — [discussion #13435](https://github.com/NVIDIA/cudf-spark/discussions/13435)
- #16660 (opened 2026-10-29, open, by revans2): "PCBS currently does not support predicate pruning". The POC reuses Parquet footer stats via Spark ParquetFilters/RowGroupFilter. Measured numbers (256 MiB/task, 32 numeric cols, medians, Spark 3.5.5, local[2]):
  - GPU-written/GPU-read full scan: 0.214 s.
  - CPU-written/CPU-read full scan: 5.368 s, so GPU read of the cache is about 25× faster than CPU read.
  - With pruning, a no-match predicate drops GPU→GPU from 0.195 s to 0.103 s and CPU→CPU from 4.194 s to 0.057 s.
  - 2048 MiB/task GPU→GPU full read: 1.014 s.
  - "CPU-written PCBS uses uncompressed Parquet pages… GPU-written PCBS uses Snappy."
  - The issue also references PR #16633 (small-record batching, bundled GPU reads) and a "companion CPU/GPU cache issue" on compressed-batch experiments.
  — [issue #16660](https://github.com/NVIDIA/cudf-spark/issues/16660)
- #7998 (2023-04-03, open), revans2: "Go back and do cache serialization differently". Parquet was chosen in 2020 to avoid writing custom kernels. The issue proposes matching Spark's native in-memory encodings (RLE, dictionary, IntDelta…) so the CPU path is never slower. — [issue #7998](https://github.com/NVIDIA/cudf-spark/issues/7998)
- #14351 (2026-03-02, open): `convertRowIteratorToCachedBatch` allocates a fixed 32 MB on-heap `ByteArrayOutputStream` per batch, which causes GC pressure. "both consumption paths (GPU via Table.readParquet, CPU via OffHeapColumnVector) copy it off-heap immediately". — [issue #14351](https://github.com/NVIDIA/cudf-spark/issues/14351)
- #5975 (2022-07-08, open): with PCBS on, a VectorUDT (CPU-fallback) pipeline runs "almost 2 min" vs "a few seconds" without it. A related ArrayIndexOutOfBounds was also reported. — [issue #5975](https://github.com/NVIDIA/cudf-spark/issues/5975)
- #4580 (2022-01, closed 2022-05): PCBS on a 12-partition array<double> frame took 37.0 s to cache+count vs 15.4 s with 1 partition. — [issue #4580](https://github.com/NVIDIA/cudf-spark/issues/4580)
- No `spark.rapids.sql.cache.*` configs exist. Grepping `advanced_configs.md` @1b5576a for "cache" finds only filecache, `shuffle.mode=CACHE_ONLY`, the concurrent-writer cache and `sql.join.hashTable.reuse`. — [advanced_configs.md @1b5576a](https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/docs/additional-functionality/advanced_configs.md)

### Inferences
- Every query against a PCBS cache pays three costs: (a) a pageable H2D copy of the compressed bytes, which is on-heap and so cannot DMA directly, (b) a Snappy decompress, and (c) a Parquet decode on the GPU. The #16660 numbers (256 MiB/task in ~0.2 s ≈ 1.2 GB/s per task, 4 tasks on 1 GPU) suggest this is cheap relative to CPU but far from HBM speed (estimate; the uncompressed vs compressed size is not stated).
- In this design the "cache" is a compressed host copy. That supports the thesis: NVIDIA chose not to keep decoded columns on device across queries.

### Gaps
- No NVIDIA-published PCBS throughput (GB/s) on a datacenter GPU. The only numbers found are from the #16660 POC on local[2].
- I did not find what the "companion" packed-cache issue number is; the issue body leaves it as a placeholder.

## Q2. spark.rapids.filecache.* (local-disk cache of remote files)

### Takeaway
The file cache arrived in 23.06 (PR #8212, merged 2023-05-04) and is off by default. It caches **raw encoded bytes**: Parquet/ORC footers and the column-chunk byte ranges a GPU scan reads, stored on the executor's Spark local dirs. It is neither decoded nor compressed differently, so each hit still pays a local-disk read, H2D and GPU decode. The implementation is not in the public repo; only the call sites, metrics and tests are.

### Cited Findings
- PR #8212 "Add file caching" merged 2023-05-04 (merge `1c14ea22375c31150d6382c425e4ac2ea3ac0f2a`). The PR says it is meant "to help accelerate repeated GPU reads to data on remote filesystems", is disabled by default, and uses Spark local dirs "up to half of the available disk space". Only GPU Parquet reads were covered at first. ORC support followed in PR #8389 (merged 2023-05-24). — [PR #8212](https://github.com/NVIDIA/cudf-spark/pull/8212), [PR #8389](https://github.com/NVIDIA/cudf-spark/pull/8389)
- Configs, with the version each appeared in:
  - `filecache.enabled` (false, 23.06.0).
  - `filecache.maxBytes` (default half the free local disk, 23.06.0).
  - `filecache.checkStale` (true, 23.06.0): "if an individual input file will not be overwritten… performance may be improved by setting this to false".
  - `filecache.allowPathRegexp` / `blockPathRegexp` (23.08.1).
  - `filecache.useChecksums` (false, 24.02.0).
  - `filecache.minimumFreeSpace.bytes`, which auto-evicts below a threshold and may "not drop caches in time" when apps share a node.
  — [configs.md](https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/docs/configs.md), [advanced_configs.md](https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/docs/additional-functionality/advanced_configs.md)
- User doc: it caches "portions of remote files being accessed onto the local filesystem of executors". It is used only by GPU-accelerated Parquet and ORC scans. It works best when "the local disks are significantly faster than the distributed filesystem". On slow local disks it "can cause applications to run slower rather than faster". Lakehouse formats write immutable files, so `checkStale=false` is safe for them. — [cudf-spark filecache doc](https://docs.nvidia.com/cudf-spark/latest/additional-functionality/filecache.html)
- What is cached: the metrics are `filecacheFooterHits/Misses(+Size)`, `filecacheDataRangeHits/Misses(+Size)`, `filecacheFooterReadTime` and `filecacheDataRangeReadTime`. `GpuParquetScan` calls `FileCache.get.getDataRangeChannel(...)` and `FileCache.get.startDataRangeCache(...)`. So the units are footers plus byte ranges, and the tests check "partial hit" after "Read a cluster of columns, skipping ranges". — [GpuMetrics.scala L134-143](https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/sql-plugin/src/main/scala/com/nvidia/spark/rapids/GpuMetrics.scala#L134-L143), [GpuParquetScan.scala L1856, L2250-2301](https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/sql-plugin/src/main/scala/com/nvidia/spark/rapids/parquet/GpuParquetScan.scala#L2250-L2301), [FileCacheIntegrationSuite.scala](https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/tests/src/test/scala/org/apache/spark/sql/rapids/filecache/FileCacheIntegrationSuite.scala)
- Closed source: no `com.nvidia.spark.rapids.filecache.FileCache` implementation appears under any `src/main` in the public tree @1b5576a; only `FileCacheConf`, the call sites and tests do (my `git ls-tree` check). Issue #8260 mentions "configs propagated from getPrivateConfigs". — [issue #8260](https://github.com/NVIDIA/cudf-spark/issues/8260)
- Open and recent work:
  - #11775 (2024-11, open): the CPU Parquet decompression path does not use the filecache; GpuParquetScan has `// TODO: Add FileCache support …/issues/11775` at L1972.
  - #13307 (2025-08, open): add `CachedFileIO` into `RapidsFileIO`.
  - #14253 (closed 2026-02): FileCache should use RapidsFileIO for remote access.
  - #14705 (closed 2026-04): FileCache metrics were missing for Iceberg, so Iceberg scans are covered.
  — [#11775](https://github.com/NVIDIA/cudf-spark/issues/11775), [#13307](https://github.com/NVIDIA/cudf-spark/issues/13307), [#14253](https://github.com/NVIDIA/cudf-spark/issues/14253), [#14705](https://github.com/NVIDIA/cudf-spark/issues/14705)

### Inferences
- The filecache removes the NIC/object-store bottleneck, but not the local-SSD → host → PCIe → decode chain. That is the same class of cache as Databricks disk cache and Velox SSD cache, and unlike Sirius `pin_table`.

### Gaps
- I found no published performance numbers for the filecache (docs, PR and issues give none).

## Q3. Any device/pinned-host cache of decoded columns across queries? Broadcast reuse, Databricks disk cache, Iceberg

### Takeaway
There is no cross-query cache of decoded columns on device or in pinned host memory, and I found no roadmap issue for one; NVIDIA's stated position is that it needs Spark changes. The closest things are within a query or across probes:
- broadcast hash-table reuse (`spark.rapids.sql.join.hashTable.reuse`, merged 2026-09-15, off by default, unreleased);
- `memory.gpu.unspill.enabled` for frequently re-used spilled buffers such as broadcasts.

### Cited Findings
- FAQ "Can I cache data into GPU memory? No" (see Q1) — [FAQ](https://docs.nvidia.com/cudf-spark/latest/faq.html). #13435 answer (see Q1) — [discussion #13435](https://github.com/NVIDIA/cudf-spark/discussions/13435)
- PR #14680 "Cache and reuse build-side hash tables during broadcast hash joins", merged 2026-09-15 (merge `6dde13d8a9fffa066cbdd85412bdf8dc5d002b7f`), fixes #12327/#10413. It adds a "shared recomputable handle in the spill framework, for hash tables that can be dropped when spilled and recomputed".
  - A6000 synthetic results: 1.23× (1 join, 64 partitions) and 1.59× (4 joins sharing an exchange).
  - NDS: no significant gain (power run 319.2 s on vs 320.4 s off).
  — [PR #14680](https://github.com/NVIDIA/cudf-spark/pull/14680)
- Config `spark.rapids.sql.join.hashTable.reuse`, default false, version "Unreleased". A follow-up bug, #16189 "Broadcast hash join can deadlock rebuilding a reused hash table", was filed and closed in 2026-10. — [advanced_configs.md @1b5576a](https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/docs/additional-functionality/advanced_configs.md), [#16189](https://github.com/NVIDIA/cudf-spark/issues/16189)
- `spark.rapids.memory.gpu.unspill.enabled` (false, since 21.06): "Unspilling may be useful for GPU buffers that are needed frequently, for example, broadcast variables". — [advanced_configs.md](https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/docs/additional-functionality/advanced_configs.md)
- Databricks disk cache:
  - Stores "a copy on its local storage in a fast intermediate format" and applies to Parquet (incl. Delta) on S3/ABFS etc.
  - Automatic on supported worker types, using up to half of local SSD.
  - Configs: `spark.databricks.io.cache.maxDiskUsage`, `maxMetaDataCache`, `compression.enabled`.
  - Databricks recommends it over the Spark cache.
  — [Databricks disk cache docs](https://docs.databricks.com/aws/en/optimizations/disk-cache)
- I found no cudf-spark doc or issue on Databricks disk cache compatibility; searches for "databricks io cache", "io.cache.enabled" and "Delta cache Databricks" in the cudf-spark issues returned nothing relevant. The FAQ does not mention it. — [FAQ](https://docs.nvidia.com/cudf-spark/latest/faq.html)
- Iceberg reads go through the filecache, per the #14705 metrics fix and #14800 "cache-aware 1.10.x shim". — [#14705](https://github.com/NVIDIA/cudf-spark/issues/14705), [#14800](https://github.com/NVIDIA/cudf-spark/issues/14800)

### Inferences
- (estimate) The plugin replaces the Parquet scan with its own GPU scan (`GpuParquetScan`), so Databricks' disk cache, which lives in Databricks' reader, is probably bypassed. That would explain why NVIDIA built its own filecache in 2023. I have not verified this.

### Gaps
- No roadmap issue for a GPU-resident cross-query table cache was found.
- Databricks disk cache interplay is undocumented.

## Q4. Spill framework, host memory limits, pinned pool, and cached-block interaction

### Takeaway
GPU objects are wrapped in spillable handles that cascade device → host → disk. The host tier is bounded by `memory.host.offHeapLimit.*` (formerly `spillStorageSize`), and the pinned pool defaults to 0. PCBS cached blocks live outside this framework, in Spark's BlockManager. Spark evicting blocks from it to make room for cached blocks can deadlock the executor (#16460).

### Cited Findings
- SpillFramework header: it "tracks device/host/disk object lifecycle". "A cascade of spills can occur device -> host -> disk, given that host allocations can fail, or could not fit in the SpillableHostStore's limit". An object is spillable only when not currently materialized/aliased. — [SpillFramework.scala L52-117](https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/sql-plugin/src/main/scala/com/nvidia/spark/rapids/spill/SpillFramework.scala#L52-L117)
- Configs:
  - `spark.rapids.memory.pinnedPool.size` defaults to 0 ("Use 0 to disable the pool").
  - `spark.rapids.memory.host.spillStorageSize` (-1 = pinned+pageable pool size) is deprecated in favor of `memory.host.offHeapLimit.enabled/size`.
  - `memory.gpu.allocFraction` 1.0, pool `ASYNC`, `memory.gpu.reserve` 640 MiB.
  — [configs.md](https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/docs/configs.md), [advanced_configs.md](https://github.com/NVIDIA/cudf-spark/blob/1b5576adb95941f57bfd2ab22b96435d7057ecf8/docs/additional-functionality/advanced_configs.md)
- FAQ: spilled buffers are not covered by `spark.io.encryption.enabled`. — [FAQ](https://docs.nvidia.com/cudf-spark/latest/faq.html)
- #16460 (2026-10-20, open): an "Executor deadlock when the MemoryStore evicts a GPU broadcast", in this sequence:
  1. A task storing a cached RDD block holds the `UnifiedMemoryManager` monitor.
  2. `evictBlocksToFreeSpace` picks a broadcast block (`SerializeConcatHostBuffersDeserializeBatch`).
  3. Serializing that block calls `SpillableColumnarBatchFromBufferImpl.getColumnarBatch()` → `GpuSemaphore.acquireIfNecessary`, which parks while still holding the lock.
  4. The other 31 task threads and the heartbeater block on the same monitor. The driver drops the executor after `spark.network.timeout` (600 s), and each occurrence costs about 10 minutes.

  Seen on 25.02.0; per the reporter the path is unchanged in v26.08.1 and main. It links to the unchecked item in #15230.
  — [issue #16460](https://github.com/NVIDIA/cudf-spark/issues/16460)
- The `CACHE_ONLY` shuffle mode ("short-circuit cached shuffle (for testing purposes)") is slated for removal in #16339 (2026-10-14). — [#16339](https://github.com/NVIDIA/cudf-spark/issues/16339)

### Inferences
- Two memory managers coexist: Spark's for cached blocks and broadcasts on heap, and RAPIDS' for device, off-heap host and disk. This is the root of #16460, and it is a structural cost of bolting GPU execution onto Spark's cache.

### Gaps
- I did not trace the `offHeapLimit` default size or the pinned-pool sizing guidance from the tuning guide.

## Q5. Competitors' caching (brief)

### Takeaway
- Gluten+Velox caches raw file data in memory plus SSD (AsyncDataCache/SsdCache); in its cuDF mode, scan is not yet on the GPU.
- Comet merged an experimental native in-memory cache in 2026-08.
- Databricks/Photon relies on the disk cache.
- Sirius is the only one with an explicit GPU-resident decoded-column cache (`pin_table`, GPU or pinned-host tier).

### Cited Findings
- Gluten Velox local cache:
  - "Velox asynchronously caches data on local disk when reading from remote storage". Configs: `velox.cacheEnabled` (false), `memCacheSize` 128MB, `ssdCachePath`, `ssdCacheSize`, `ssdCacheShards`, `ssdCacheIOThreads`.
  - `loadQuantum` "must be at most 8MB when Velox cache is enabled". Soft affinity scheduling is recommended.
  - "Gluten cannot reuse older caches for now". `ssdODirect` is broken (Velox #10597).
  — [Gluten VeloxLocalCache.md @381c806](https://github.com/apache/gluten/blob/381c806ec2d65ed3262b67407f2e604902f11e49/docs/backends/velox/get-started/VeloxLocalCache.md)
- Velox: "transparent file cache to accelerate table scan through the hot data reuse and prefetch… integrated with the memory system to achieve dynamic memory sharing between file cache and query memory" (AsyncDataCache). Velox also documents "Broadcast Build Caching" to avoid per-task hash-table rebuilds. — [velox memory.rst @37931b3](https://github.com/facebookincubator/velox/blob/37931b32a83a080a1ebb797afa2dc00b817af120/velox/docs/develop/memory.rst), [hash-table-caching.rst](https://github.com/facebookincubator/velox/blob/37931b32a83a080a1ebb797afa2dc00b817af120/velox/docs/develop/hash-table-caching.rst)
- Gluten cuDF backend: Velox plan nodes are replaced by GPU operators via `spark.gluten.sql.columnar.cudf=true`. The operator table lists Scan as "❌ Not supported / In Development". By default only fully cuDF-offloaded stages go to the GPU. — [Gluten VeloxGPU.md @381c806](https://github.com/apache/gluten/blob/381c806ec2d65ed3262b67407f2e604902f11e49/docs/backends/velox/get-started/VeloxGPU.md)
- DataFusion Comet:
  - PR #5051 "experimental native support for in-memory cache, disabled by default" (`spark.comet.exec.inMemoryCache.enabled`, `CometInMemoryTableScanExec`) merged 2026-08-28. The search summary reports about 1.5× for repeated full scans on a 5M-row table and about 1.1× for selective filters (not verified on the PR page).
  - The competing serializer-based PR #4569 (Arrow IPC CachedBatchSerializer) was closed unmerged.
  — [PR #5051](https://github.com/apache/datafusion-comet/pull/5051), [PR #4569](https://github.com/apache/datafusion-comet/pull/4569)
- Databricks disk cache: see Q3. — [Databricks docs](https://docs.databricks.com/aws/en/optimizations/disk-cache)
- Sirius `pin_table`:
  - It "materializes a table's columns into memory once, and subsequent queries over that source are served straight from the pinned copy, skipping file I/O and decode entirely". Matching against later queries is automatic.
  - `tier='gpu'` pins in GPU memory; `tier='host'` pins in pinned host memory "for tables larger than GPU memory".
  - Optional Simpatico compression is available (`pin_table_compression`).
  - UPDATE/MERGE are rejected while pinned, and CHECKPOINT invalidates the pin.
  — [sirius docs/README.md @b9e05ff L148-200](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md)

### Inferences
- Of all the systems surveyed, only Sirius caches *decoded* columns on device. The Spark-based engines (RAPIDS, Gluten, Comet, Databricks) cache encoded bytes (disk/RAM) or an engine-specific compressed format in host memory.

### Gaps
- Photon-specific caching beyond the disk cache: not researched.
- Velox's cuDF scan roadmap: not researched.

## Q6. Bandwidth numbers for the thesis

### Takeaway
Compare a per-node NIC of 10-100 Gbps (≈1.25-12.5 GB/s) on g-class instances and 3200 Gbps (≈400 GB/s) on p5 with H100 HBM at 3.35 TB/s. Even PCIe Gen5 (128 GB/s bidirectional, about 64 GB/s per direction) is roughly 25-50× below HBM, so any engine that re-reads input every query is bounded by NIC, SSD or PCIe, not by the GPU.

### Cited Findings
- p5.48xlarge / p5e.48xlarge: 32 network cards, "total network bandwidth capacity of 3,200 Gbps", of which up to 800 Gbps can carry IP traffic; EFA and IP share it. — [AWS EFA docs](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/efa-acc-inst-types.html); p5en 3200 Gbps EFAv3 — [AWS blog](https://aws.amazon.com/blogs/aws/new-amazon-ec2-p5en-instances-with-nvidia-h200-tensor-core-gpus-and-efav3-networking)
- g6 network and local NVMe by size:

  | Instance size | Network | Local NVMe |
  |---|---|---|
  | g6.xlarge | up to 10 Gbps | 1×250 GB |
  | g6.4xlarge | up to 25 Gbps | — |
  | g6.12xlarge | 40 Gbps | 4×940 GB |
  | g6.24xlarge | 50 Gbps | 4×940 GB |
  | g6.48xlarge | 100 Gbps | 8×940 GB |

  — [AWS g6 page](https://aws.amazon.com/ec2/instance-types/g6/)
- H100 SXM: 3.35 TB/s HBM, NVLink 900 GB/s, PCIe Gen5 128 GB/s. H100 NVL: 3.9 TB/s. — [NVIDIA H100 page](https://www.nvidia.com/en-us/data-center/h100/)
- S3 guidance does not give a per-instance throughput cap. It advises parallel byte-range GETs to "fully use your network interface card (NIC) capacity", and gives 5,500 GET/HEAD req/s per prefix. — [S3 design patterns](https://docs.aws.amazon.com/AmazonS3/latest/userguide/optimizing-performance-design-patterns.html), [S3 guidelines](https://docs.aws.amazon.com/AmazonS3/latest/userguide/optimizing-performance-guidelines.html)
- GPU Parquet scan: "effective read bandwidth up to 125 GB/s" with GPU-aware Parquet settings (Luo, Chen, Binnig, "Do GPUs Really Need New Tabular File Formats?", arXiv 2602.17335, v1 2026-02-19). The search snippet says about 3 GB/s with DuckDB-default files and that 125 GB/s used 4 SSDs (not verified in the PDF). — [arXiv 2602.17335](https://arxiv.org/abs/2602.17335)
- NVIDIA Polars GPU Parquet reader: a forum chart peaks near 100 GB/s on H100. Throughput there is computed as decoded size over read time, and a user disputed it. — [NVIDIA forum](https://forums.developer.nvidia.com/t/efficiently-scaling-polars-gpu-parquet-reader/329964)

### Inferences
- (estimate) Practical PCIe Gen4 x16 is ~25 GB/s and Gen5 x16 ~50 GB/s per direction. These figures come from common knowledge, not a source fetched here.
- (estimate) A single PCIe Gen4 NVMe drive does ~7 GB/s sequential reads. g6.48xlarge with 8 drives would be ~50 GB/s if they were that class, but AWS does not publish per-drive throughput on the g6 page.
- At 100 Gbps (12.5 GB/s), a g6.48xlarge NIC is roughly 10× below one GPU's PCIe Gen4 link and over 200× below HBM, so the input path dominates unless decoded data stays resident.

### Gaps
- No authoritative libcudf Parquet decode GB/s on H100 was found; the cudf benchmark docs were not located.
- No sourced S3 per-instance throughput figure: the current AWS pages don't state one.
- Sourced NVMe and PCIe practical numbers are not included.
