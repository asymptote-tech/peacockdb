# The data path: NIC, local SSD, RAM cache

## Summary
- On cloud VMs the wire sets the speed. One H100 would need roughly 50-80 GB/s of compressed Parquet to stay busy on TPC-H scan and join shapes (estimate). Single-GPU VMs get 2.5-12.5 GB/s (20-100 Gbit/s), so the GPU idles 75-95% of the time unless the data is already in host RAM or VRAM (estimate).
- At the human's price parity, small CPU nodes buy as much or more NIC as the GPU node, except on AWS p5.4xlarge: about 0.5-1.9× (vs baseline or burst) on AWS, 3× on Azure, 18× on GCP. On single-GPU shapes local NVMe is no faster than the NIC, or unpublished. So one-pass ETL from S3 should not clear 2× on the data path (estimate).
- The GPU's data-path edge exists only where bytes are reused: pinned host RAM (47 GB/s measured on H200) and VRAM. Batched ETL gets that, but a CPU engine that batched the same way would cut its wire bytes just as much.
- Nebius sells local NVMe only on its 8-GPU B300 VM. GPUDirect Storage runs in "compatibility mode" (a copy through host memory) on cloud VMs. RAM per GPU is about 200-320 GB on every cloud and on a DGX H100, so on-prem does not fix RAM; only custom or bare-metal builds do.
- Layout to target first: local NVMe plus host RAM as a cache in front of the lake (Databricks, Fabric, Meta, Uber run it). Local SSD as the primary store is real only historically (Meta's Raptor, replaced 2022). Co-located HDFS is large on-prem but its biggest users are moving to object storage.

## 1. The NIC: what VMs get from object storage
**Per stream and per instance (published).**
- A single S3 request delivers about 25-95 MiB/s, median 55-60 MiB/s, on 16 MiB objects. About 200-250 requests in flight fill 100 Gbit/s; the median measured aggregate on AWS was about 75 of 100 Gbit/s; best request size 8-16 MiB. Durner, Leis, Neumann, VLDB 2023 [S1], figures read through a secondary summary [S2] (accessed 2026-11-04).
- AWS gives no per-connection figure; it says to use many connections and byte-range GETs [S3] (accessed 2026-11-04).
- AWS limits single-flow traffic to 5 Gbit/s outside a cluster placement group; instances under 32 vCPUs are limited to 5 Gbit/s through an internet gateway [S4] (accessed 2026-11-04). A 16-vCPU p5.4xlarge reaching S3 without a VPC endpoint would hit that (estimate).
- AWS "up to" NICs have a lower baseline: m7i.4xlarge 6.25 Gbit/s baseline, 12.5 burst, burst lasting 5-60 minutes [S4][S5] (accessed 2026-11-04). Batched ETL windows are longer, so the baseline counts (estimate).
- Mountpoint for S3 measured 4.85 GB/s read per c6i.16xlarge node [S6] (accessed 2026-11-04).
- Nebius Object Storage, Standard class: "limited to 20 GBps per tenant in each region"; Enhanced Throughput is unthrottled [S7] (accessed 2026-11-04). All of a tenant's nodes share the Standard limit, the H100 and all eight small nodes alike.
- Nebius lists no NIC rate for its 1-GPU H100 preset or CPU presets. The H100 platform lists a "ConnectX-6 100 Gbps network adapter" and 8×400 Gbit/s InfiniBand [S8] (accessed 2026-11-04). In the cost model both Nebius NIC rates are assumptions until experiment 1 measures them.

**When the GPU stalls, by query shape (modelled).** Uses peacockdb's measured H200 sf40 execute times (load excluded) from `llm-wiki/reports/benchmark-minimal.md` @ `de17e8bc` (measured), and q6's 1.5 GB of compressed columns (lakehouse-iceberg.md, a local lead). H100 has about 70% of H200's HBM bandwidth, so its required rates are about 30% lower (estimate).

| Query, sf40 | Compressed bytes | H200 execute | Input rate to keep GPU busy | GPU busy at 12.5 GB/s (100 Gbit) | at 3.1 GB/s (25 Gbit) |
|---|--:|--:|--:|--:|--:|
| q6 scan/filter | 1.5 GB | 19.2 ms | ~78 GB/s | ~16% | ~4% |
| q3 3-way join | ~2 GB (estimate) | 40.1 ms | ~50 GB/s | ~25% | ~6% |
| q8 7-table join | ~2.5 GB (estimate) | 47.7 ms | ~52 GB/s | ~24% | ~6% |
| q1 group-by agg | ~3 GB (estimate) | 239.9 ms | ~12.5 GB/s | ~100% | ~25% |

- Rule for the model: the GPU stalls whenever the NIC is below bytes ÷ execute time. On single-GPU cloud shapes this holds for every TPC-H shape except heavy aggregation. q1's 240 ms includes a libcudf group-by cliff that streaming removes (inventory.md item 9); once fixed, q1 joins the stalled group (estimate).
- Cloudera ran 8×H100 at about 15% utilisation reading HDFS from a separate cluster; the poster blamed HDFS and the network [S9] (accessed 2026-11-04).
- **Today the reader, not the NIC, limits peacockdb.** q6's load is 398 ms for 1.5 GB, about 3.8 GB/s, below a 40 Gbit/s NIC. Parquet decode materializes only 6.8-8.7 GB/s on H200 (inventory.md items 5-6, measured). A NIC-bound story needs the reader fixed first.
- **The CPU baseline may be NIC-bound too.** CPU Spark on 8×r8g.4xlarge ran TPC-DS 31% (3 TB) to 47% (10 TB) faster on S3 Express One Zone than on S3 Standard [S10] (accessed 2026-11-04). If Comet on 128 cores is also wire-bound, cost per query is set by NIC bandwidth per dollar, not the engine (estimate). Experiment 2 tests this.

## 2. Cloud instances: cores, RAM, NIC, local NVMe, price
On-demand Linux $/h, US regions (Nebius: eu-north1). n/p = rate not published; n/f = not fetched. Sources [S5][S8][S11]-[S27] (accessed 2026-11-04). vCPU = hyper-threads unless noted.

| Shape | vCPU | RAM | GPU | NIC Gbit/s | Local NVMe | Rated read | GDS | $/h |
|---|--:|--:|---|---|---|---|---|--:|
| AWS p5.48xlarge | 192 | 2 TiB | 8×H100 | 3,200 EFA | 8×3.84 TB | n/p | FSx Lustre GDS ≤1,200 Gbit/s per client; local: no AWS statement | 55.04 |
| AWS p5.4xlarge | 16 (8 cores) | 256 GiB | 1×H100 | 100 | 1×3.84 TB | n/p | as above; no GPUDirect RDMA | 6.88 |
| AWS g6e.4xlarge | 16 (8 cores) | 128 GiB | 1×L40S | 20 | 1×600 GB | n/p | no statement | 3.004 |
| AWS g6e.48xlarge | 192 | 1,536 GiB | 8×L40S | 400 | 4×1.9 TB | n/p | no statement | n/f |
| AWS m7i.2xlarge | 8 | 32 GiB | – | 3.125 base / 12.5 burst | – | – | – | 0.403 |
| AWS m7i.4xlarge | 16 | 64 GiB | – | 6.25 / 12.5 | – | – | – | 0.806 |
| AWS c7i.4xlarge | 16 | 32 GiB | – | up to 12.5 | – | – | – | 0.714 |
| AWS r7i.4xlarge | 16 | 128 GiB | – | up to 12.5 | – | – | – | 1.058 |
| AWS m7a.32xlarge | 128 (cores) | 512 GiB | – | 50 | – | – | – | 7.419 |
| AWS i4i.4xlarge | 16 | 128 GiB | – | 9.375 / 25 | 1×3.75 TB | 2.8 GB/s | – | 1.373 |
| AWS i4i.32xlarge | 128 | 1,024 GiB | – | 75 | 8×3.75 TB | 22.4 GB/s | – | 10.982 |
| AWS i7ie.2xlarge | 8 | 64 GiB | – | 8.333 / 25 | 2×2.5 TB | n/p (216,666 IOPS) | – | 1.04 |
| AWS i7ie.48xlarge | 192 | 1,536 GiB | – | 200 | 16×7.5 TB | n/p (5.2 M IOPS) | – | n/f |
| AWS im4gn.4xlarge | 16 (Graviton2) | 64 GiB | – | 25 | 1×7.5 TB | n/p (250k IOPS) | – | 1.455 |
| GCP a3-highgpu-8g | 208 | 1,872 GB | 8×H100 | 1,000 | 16×375 GiB | 6,240 MiB/s | no statement | 88.49 |
| GCP a3-highgpu-1g | 26 | 234 GB | 1×H100 | 25 | 2×375 GiB | 1,320 MiB/s | no statement | 11.06 |
| GCP c3-standard-8-lssd | 8 | 32 GB | – | 23 | 2×375 GiB | 1,320 MiB/s | – | 0.485 + SSD |
| GCP c3-standard-22-lssd | 22 | 88 GB | – | 23 | 4×375 GiB | 2,650 MiB/s | – | 1.273 + SSD |
| GCP c3-standard-176-lssd | 176 | 704 GB | – | 100 (200 Tier_1) | 32×375 GiB | 12,480 MiB/s | – | n/f |
| GCP n2-standard-16 | 16 | 64 GB | – | 32 | – | – | – | 0.777 |
| Azure ND96isr H100 v5 | 96 | 1,900 GiB | 8×H100 | 80 Ethernet + 8×400 IB | 8 disks, 28 TiB | n/p | no statement | 98.32 |
| Azure NC40ads H100 v5 | 40 | 320 GiB | 1×H100 NVL | 40 | 1×3,576 GiB | n/p | no statement | 6.98 |
| Azure L8s/L16s/L80s v3 | 8/16/80 | 64/128/640 GiB | – | 12.5/12.5/32 | 1/2/10×1.92 TB | 2/4/20 GB/s | – | n/f |
| Azure L16as v4 | 16 | 128 GiB | – | 12.5 | 4×960 GB | 6 GB/s | – | 1.376 |
| Azure L96as v4 | 96 | 768 GiB | – | 40 | 12×1.92 TB | 36 GB/s | – | n/f |
| Azure D16s v5 / E16s v5 | 16 | 64 / 128 GiB | – | 12.5 / n/f | – | – | – | 0.768 / 1.008 |
| Nebius H100 1gpu-16vcpu-200gb | 16 | 200 GiB | 1×H100 | not stated (assumption) | none | – | – | 4.50 |
| Nebius H100 8gpu-128vcpu-1600gb | 128 | 1,600 GiB | 8×H100 | 8×400 IB | none | – | – | 36.00 (8×4.50) |
| Nebius L40S 1gpu-16vcpu-64gb | 16 | 64 GiB | 1×L40S | not stated | none | – | – | 1.75 (computed) |
| Nebius B300 8gpu-192vcpu-2768gb | 192 | 2,768 GiB | 8×B300 | 8 GPUs' IB | 6×3.84 TB | 6.8 GB/s per disk | not stated | n/f |
| Nebius cpu-d3 8/16/128 vCPU | 8/16/128 | 32/64/512 GiB | – | not stated (assumption) | none | – | – | 0.264 / 0.528 / 4.224 (computed) |

- AWS publishes sequential read rates only for i4i (128 KB blocks, 2022 launch blog [S17]); for p5, g6e, i7ie, im4gn only random IOPS [S16] (accessed 2026-11-04). No independent fio results for these drives were found.
- **NIC per dollar at the human's parity (computed from the table):**

| Cloud | GPU node | Same money in small CPU nodes | Summed NIC, GPU : CPU |
|---|---|---|---|
| AWS | p5.4xlarge, 100 Gbit/s, $6.88 | 8.5× m7i.4xlarge: 53 base / 106 burst Gbit/s | 1.9 : 1 (base), 0.94 : 1 (burst) |
| AWS | same | 5× i4i.4xlarge: 47 Gbit/s + 14 GB/s NVMe | 2.1 : 1 on NIC; NVMe only on the CPU side |
| GCP | a3-highgpu-1g, 25 Gbit/s, $11.06 | 14× n2-standard-16: 455 Gbit/s | 0.05 : 1 |
| GCP | same | 8.7× c3-standard-22-lssd: 200 Gbit/s + 22.5 GiB/s SSD | 0.13 : 1 |
| Azure | NC40ads H100 v5, 40 Gbit/s, $6.98 | 9× D16s v5: 114 Gbit/s | 0.35 : 1 |
| Azure | same | 5× L16as v4: 63 Gbit/s + 30 GB/s NVMe | 0.63 : 1 |
| Nebius | H100 1-GPU, $4.50 | 8.5× cpu-d3 16 vCPU | unknown; experiment 1 |

- The one 128-core box differs: on AWS, p5.4xlarge has 2.2× the NIC per dollar of m7a.32xlarge (100 vs 50 Gbit/s at a similar price). The single-box comparison flatters the GPU; the small-node layout does not (estimate).

## 3. Getting more out of the wire
- **Compressed transfer with GPU decompression.** Parquet already crosses the wire compressed for both CPU and GPU, so the only GPU question is whether it decodes fast enough. B200's hardware decompression engine outputs 117 GB/s Snappy and 155 GB/s zstd; H100 has software decompression only [S28] (accessed 2026-11-04). NVIDIA rates zstd GPU decompression "Moderate", Snappy and LZ4 "High" [S29] (accessed 2026-11-04). peacockdb decodes at 6.8-8.7 GB/s today, with page decode plus Snappy 80% of kernel time on q11v (inventory.md item 6, measured).
  - Compression pays only when the scan is wire-bound. Theseus cut runtime 18% compressing its TCP exchange, then gained 19% more by turning it off on GPUDirect RDMA [S30] (accessed 2026-11-04). With four SSDs feeding one A100 over GDS, the GPU became compute-bound and dropping needless compression helped [S31] (accessed 2026-11-04). On NVMe, zstd's CPU decode cost outweighs its I/O saving [S32] (accessed 2026-11-04).
  - GPU-friendly Parquet settings reached 125 GB/s effective read on one A100 from local SSDs via GDS [S31] (accessed 2026-11-04): a file-layout result on local disk, not a network result.
- **Column and row-group pruning before the wire.** Range GETs fetch only projected column chunks [S3] (accessed 2026-11-04). Measured: q6 at sf40 reads 298 of about 1,954 row groups after pruning (inventory.md item 3). Comet's cache scan decompresses only projected columns (`docs/source/user-guide/latest/in-memory-cache.md:85-91` @ ab46419f). Server-side filtering (S3 Select) made PushdownDB 6.7× faster and 30% cheaper [S33] (accessed 2026-11-04), but S3 Select is closed to new customers [S34] (accessed 2026-11-04). Pruning cuts bytes equally for CPU and GPU (estimate).
- **Local NVMe as a cache** (mostly CPU engines). Fabric's intelligent cache: "up to 60%" faster repeated reads [S35]. Meta's RaptorX: almost 2× better P90 than Raptor at near-100% hit rate [S36]. Uber's Presto cache: hit rate about 65% → over 90% with 500 GB per node [S37]. S3 Express One Zone: CPU Spark 31-47% faster and 36% cheaper per run [S10]. Theseus byte-range pre-loading from S3: about 20% faster [S30] (all accessed 2026-11-04).
- **Shuffle volume.** LinkedIn shuffles 15-18 PB/day; push-based shuffle cut runtime nearly 30% [S38]. GPU Presto: UCX exchange 93 s vs 828 s HTTP (TPC-H SF1000, 8×A100) [S39] (accessed 2026-11-04). Comet's native shuffle compresses with lz4 by default (`spark/src/main/scala/org/apache/comet/CometConf.scala:937-947` @ ab46419f).
  - **The GPU node's one wire advantage:** one H100 node shuffles inside the box, while eight small CPU nodes shuffle over their NICs. That favours the GPU on join-heavy shapes (estimate). Experiment 2's 8×16 layout shuffles over loopback, so it misses this.

## 4. Local SSD: throughput, the GDS model, capacity and loss
- **NVMe vs NIC, same instance, rated:** AWS i4i.4xlarge 2.8 vs 1.17 GB/s baseline NIC, 2.4×; i4i.32xlarge 22.4 vs 9.4, 2.4×. Azure L16as v4 6 vs 1.56, 3.8×; L8s v3 2 vs 1.56, 1.3×. GCP c3-standard-8-lssd 1.4 vs 2.9, 0.5×; c3-standard-176-lssd about 1×; a3-highgpu-1g 1.4 vs 3.1, 0.45×. AWS p5.4xlarge: one drive, rate unpublished, vs 12.5 GB/s; one Gen4 drive is about 6-7 GB/s, about 0.5× (estimate). Sources: §2 table.
  - **Storage-optimized CPU shapes get 1.3-3.8× the NIC from local NVMe; single-GPU shapes 0.5× or less.** Only 8-GPU shapes have many drives (e.g. 8×3.84 TB on p5.48xlarge).
- **GDS on cloud VMs.** NVIDIA: "cuFile APIs can be used in Cloud-service-providers environments in compatibility mode"; with IOMMU on, GDS is "not guaranteed to work" [S40] (accessed 2026-11-04). Compatibility mode falls back to POSIX reads through host memory [S41] (accessed 2026-11-04). The real cloud GDS path is AWS FSx for Lustre on P5-class: up to 1,200 Gbit/s per client [S12] (accessed 2026-11-04). No provider documents GDS from local instance NVMe. On bare metal the direct P2P path works, e.g. NVMe P2PDMA on ext4/XFS since GDS 1.13 [S40] (accessed 2026-11-04).
- **Model: tables staged on local NVMe, read to the GPU.** Rate = min(NVMe, PCIe ~47 GB/s pinned, decode); decode is 6.8-8.7 GB/s today. The host bounce costs CPU and RAM bandwidth but does not cap the rate while NVMe stays below PCIe (estimate). On p5.4xlarge, half from NVMe and half from S3 gives about 19 GB/s (estimate) vs 12.5 from S3 alone, 1.5× at most. A 128-vCPU CPU box like i4i.32xlarge gets 22.4 GB/s from NVMe alone.
- **Capacity and loss on stop.** AWS cryptographically erases instance store on stop, hibernate or terminate; it persists on reboot [S42]. GCP discards local SSD on stop; preserve-on-stop is in preview [S20]. Azure Lsv3 loses NVMe data on stop/deallocate [S23]. Nebius B300 local disks are erased on stop or host failure [S43] (all accessed 2026-11-04).
  - Refill, p5.4xlarge: 3.84 TB at 12.5 GB/s is about 5 minutes, about 7 at the 75% S3 efficiency in [S1]: roughly $0.6-0.8 of VM time plus about 229k GETs at $0.0004/1,000 ≈ $0.09 [S44] (accessed 2026-11-04) (estimate). A stop between nightly windows means one refill per window: small next to a multi-hour batch, but it adds to the first job's latency (estimate).

## 5. Local SSD as a product option: who runs which layout
- **(a) Local SSD as the primary store.** Meta's Presto Raptor kept tables on workers' flash, backed up externally; Meta replaced it with RaptorX (a cache over the warehouse), citing low utilisation, poor tail latency, ops overhead and security policy [S36] (Presto blog 2022-01-28, accessed 2026-11-04). A bare-metal host markets single-server Spark on local NVMe for 1-20 TB [S45] (vendor, accessed 2026-11-04). No Spark or Comet production deployment with local SSD as the system of record was found: rare.
- **(b) HDFS co-located on compute nodes' disks.** AWS EMR: HDFS on core nodes is "ephemeral storage which is reclaimed when the cluster ends", "best used for caching the results produced by intermediate job-flow steps" [S46]. Uber runs about 1.5 EB of on-prem HDFS and is moving its lake to GCS (2024-07-30, 2025-11-13) [S47][S48]. LinkedIn's largest HDFS cluster held about 500 PB in 2021 [S49]. Cloudera's HGX case read a separate HDFS cluster [S9] (2024-09-06) (all accessed 2026-11-04). Comet's HDFS support is "experimental and is not covered by continuous integration" (`docs/source/user-guide/latest/datasources.md:159` @ ab46419f). Real on-prem, shrinking at the largest users.
- **(c) Local NVMe as a cache in front of the lake.** Databricks disk cache: default on SSD workers, up to half the local SSD, evicted on cluster restart (page updated 2026-10-22) [S50]. Fabric intelligent cache: default on, 50% of node SSD [S35]. Meta RaptorX and Uber Presto local cache [S36][S37] (all accessed 2026-11-04). Gluten Velox cache: off by default, 128 MB defaults, soft affinity, "cannot reuse older caches" (`docs/backends/velox/get-started/VeloxLocalCache.md` @ d64a51df). spark-rapids file cache: off by default, "doesn't perform well if the executor node's local disks are relatively slow", no published speedups [S51] (accessed 2026-11-04); implementation not in the public tree, only imports and tests (`sql-plugin/.../Plugin.scala:35`; `tests/.../filecache/` @ ce634969); no cuFile or kvikio in `sql-plugin` @ ce634969. spark-rapids staff blamed a missed 50% saving partly on disk: "r6id.24xlarge has 3x better local disk io performance than g4dn.12xlarge" [S52] (2025-01-22, accessed 2026-11-04). Mainstream: managed platforms run it by default.
- **Target first: (c).** It is what managed platforms run; it keeps the lake as system of record; it fits batched ETL, where many jobs read the same tables in one window. The GPU version should cache in pinned host RAM first and NVMe second: on single-GPU shapes NVMe is not faster than the NIC, so it only saves requests and refetches (estimate). It needs soft-affinity scheduling (Gluten, RaptorX) so the same executor sees the same files across the window.

## 6. A RAM cache inside Comet and Spark
- **Spark** (apache/spark master, accessed 2026-11-04): `CacheManager`: "Data is cached using byte buffers stored in an InMemoryRelation", default `MEMORY_AND_DISK` [S53]. It lives in `SharedState`, "state shared across sessions", so a cache lasts one Spark application [S54]; separate scheduled spark-submit jobs share nothing (inference). Under dynamic allocation, "executors containing cached data are never removed" by default (`cachedExecutorIdleTimeout`); Kubernetes has no external shuffle service (Spark 4.3.0 docs) [S55] (accessed 2026-11-04).
- **Comet 1.2.0 (ab46419f):** `spark.comet.exec.inMemoryCache.enabled` is "experimental and disabled by default" (`in-memory-cache.md:28`). Each cached batch is one Arrow IPC batch, zstd level 1 by default, in JVM heap chunks managed by Spark's BlockManager, spilling via `spark.serializer` (`CometConf.scala:416-487`; `in-memory-cache.md:76-91, 223-229`). It prunes batches on stored min/max; the scan is JVM-side (`understanding-comet-plans.md:301`). For a GPU, each query would copy heap → host → device and decompress zstd: the same shape as spark-rapids' PCBS problem (spark-plugin.md lead).
- **Gluten** has a columnar table cache on by default: `spark.gluten.sql.columnar.tableCache` (`GlutenConfig.scala:1124-1128` @ d64a51df).
- **So a GPU cache must live outside BlockManager**: in the native executor (pinned host RAM plus VRAM), with one accountant and spill. To serve several scheduled jobs it needs one long-lived application (Thrift or Connect server) or a node-level daemon like Databricks' disk cache (design claim, estimate).
- **Only viable on-prem?** RAM per GPU: about 200 GB Nebius H100, 256 GiB p5.4xlarge, 234 GB a3-highgpu-1g, 320 GiB NC40ads (§2). A DGX H100 has 2 TB for 8 GPUs, also 256 GB per GPU [S56] (secondary, accessed 2026-11-04). Stock on-prem servers do not have more RAM per GPU; only custom builds (1-2 TB per GPU, many NVMe drives, bare-metal GDS) change the picture. In the cloud the binding limit is NIC per dollar on single-GPU shapes, not RAM.
- **Data-size breakpoints, Nebius H100 VM:** under 80 GB decoded hot columns: VRAM. Up to about 150 GB compressed: host RAM, after OS, JVM and staging (estimate). Above that every pass goes over the NIC (no local NVMe). sf40 fits in host RAM; real ETL tables often do not.

## 7. Evidence against a GPU backend, on the data path
- On GCP and Azure, small CPU nodes buy 3-18× the GPU node's NIC at equal money. On AWS the GPU node has at most 1.9×, about 0.9× against burst (§2).
- Storage-optimized CPU shapes get 1.3-3.8× their NIC from local NVMe; single-GPU shapes about 0.5× or unpublished (§4).
- Batching cuts wire bytes per query for any engine. The batching win belongs to the fuser, not the GPU, unless the CPU engine cannot batch (estimate).
- Theseus, a GPU engine built around data movement, was only 12.3% faster than Photon at cost parity at SF1k on g6.4xlarge (25 Gbit/s); it led by 4.46× only at SF30k [S30] (accessed 2026-11-04).
- peacockdb's reader (3.8 GB/s compressed input on q6) is slower than a 40 Gbit/s NIC today. NVIDIA's own reader rework is P0 (spark-plugin.md lead).

## Follow-up experiments
1. Experiment 1 as planned, plus: Nebius NIC rate for 1-GPU H100 and cpu-d3 16 vCPU (iperf3 between VMs); S3 at 1, 16, 64 and 256 in-flight 16 MiB range GETs; Standard vs Enhanced bucket class.
2. The CPU's wire ceiling: Comet q6 and q1 from S3 vs local disk on the 128-core VM (experiment 2, priority 1-2). If S3 and local times are close, the CPU is not wire-bound and the GPU's per-dollar NIC gap matters less.
3. Bytes per query: log compressed bytes read per TPC-H and TPC-DS query at sf40, to replace the (estimate) cells in §1.
4. One p5.4xlarge hour on AWS (needs approval): fio on the instance store; S3 throughput via a VPC endpoint vs NAT/internet gateway. Settles the 5 Gbit/s question and the 0.5× NVMe estimate.
5. A pinned-RAM cache prototype measured as GPU busy time: replay a batch of 10 queries from pinned RAM vs S3, to bound the batched-ETL gain.

## Sources
- [S1] Durner, Leis, Neumann, "Exploiting Cloud Object Storage for High-Performance Analytics", PVLDB 16(11) 2023: https://vldb.org/pvldb/vol16/p2769-durner.pdf (accessed 2026-11-04)
- [S2] Greptime summary of [S1]: https://greptime.com/blogs/2024-01-31-paper-sharing (accessed 2026-11-04)
- [S3] AWS S3 performance design patterns: https://docs.aws.amazon.com/AmazonS3/latest/userguide/optimizing-performance-design-patterns.html (accessed 2026-11-04)
- [S4] AWS EC2 instance network bandwidth: https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/ec2-instance-network-bandwidth.html (accessed 2026-11-04)
- [S5] AWS general-purpose specs: https://docs.aws.amazon.com/ec2/latest/instancetypes/gp.html (accessed 2026-11-04)
- [S6] AWS HPC blog, Mountpoint for S3: https://aws.amazon.com/blogs/hpc/improve-the-speed-and-cost-of-hpc-deployment-with-mountpoint-for-amazon-s3 (accessed 2026-11-04)
- [S7] Nebius Object Storage classes: https://docs.nebius.com/object-storage/storage-classes (accessed 2026-11-04)
- [S8] Nebius VM types: https://docs.nebius.com/compute/virtual-machines/types.md (accessed 2026-11-04)
- [S9] spark-rapids discussion #11431: https://github.com/NVIDIA/spark-rapids/discussions/11431 (accessed 2026-11-04)
- [S10] AWS, Spark on S3 Express One Zone, 2026-09-03: https://aws.amazon.com/blogs/storage/run-spark-31-faster-and-optimize-compute-costs-with-amazon-s3-express-one-zone-on-amazon-emr/ (accessed 2026-11-04)
- [S11] AWS P5: https://aws.amazon.com/ec2/instance-types/p5/ ; prices https://instances.vantage.sh/aws/ec2/p5.4xlarge , https://instances.vantage.sh/aws/ec2/p5.48xlarge (accessed 2026-11-04)
- [S12] FSx for Lustre performance, EFA and GDS: https://docs.aws.amazon.com/fsx/latest/LustreGuide/performance.html ; https://aws.amazon.com/about-aws/whats-new/2024/11/amazon-fsx-lustre-elastic-fabric-adapter-nvidia-gpudirect-storage/ (accessed 2026-11-04)
- [S13] AWS G6e: https://aws.amazon.com/ec2/instance-types/g6e/ ; https://instances.vantage.sh/aws/ec2/g6e.4xlarge (accessed 2026-11-04)
- [S14] AWS accelerated specs: https://docs.aws.amazon.com/ec2/latest/instancetypes/ac.html (accessed 2026-11-04)
- [S15] Vantage prices: https://instances.vantage.sh/aws/ec2/m7i.2xlarge , /m7i.4xlarge , /c7i.4xlarge , /r7i.4xlarge , /m7a.32xlarge , /i4i.4xlarge , /i4i.32xlarge , /i7ie.2xlarge , /im4gn.4xlarge (accessed 2026-11-04)
- [S16] AWS storage-optimized specs: https://docs.aws.amazon.com/ec2/latest/instancetypes/so.html (accessed 2026-11-04)
- [S17] AWS I4i launch blog, 2022-04-27: https://aws.amazon.com/blogs/aws/new-storage-optimized-amazon-ec2-instances-i4i-powered-by-intel-xeon-scalable-ice-lake-processors/ (accessed 2026-11-04)
- [S18] GCP accelerator machines: https://docs.cloud.google.com/compute/docs/accelerator-optimized-machines ; GPU bandwidth https://docs.cloud.google.com/compute/docs/gpus/gpu-network-bandwidth (accessed 2026-11-04)
- [S19] GCP general-purpose (C3): https://docs.cloud.google.com/compute/docs/general-purpose-machines (accessed 2026-11-04)
- [S20] GCP Local SSD: https://docs.cloud.google.com/compute/docs/disks/local-ssd (accessed 2026-11-04)
- [S21] GCP prices: https://gcloud-compute.com/a3-highgpu-1g.html , /c3-standard-8-lssd.html , /c3-standard-22-lssd.html , /n2-standard-16.html ; a3-highgpu-8g https://calculator.holori.com/gcp/vm/a3-highgpu-8g (accessed 2026-11-04)
- [S22] Azure ND H100 v5: https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/gpu-accelerated/ndh100v5-series ; NCads H100 v5: https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/gpu-accelerated/ncadsh100v5-series (accessed 2026-11-04)
- [S23] Azure Lsv3: https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/storage-optimized/lsv3-series (accessed 2026-11-04)
- [S24] Azure Lasv4: https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/storage-optimized/lasv4-series (accessed 2026-11-04)
- [S25] Azure Dsv5: https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/general-purpose/dsv5-series (accessed 2026-11-04)
- [S26] Azure prices: https://cloudprice.net/vm/Standard_ND96isr_H100_v5 , https://www.azurespeed.com/AzureVmPricing/Standard_NC40ads_H100_v5 , https://cloudprice.net/vm/Standard_L16as_v4 , https://www.devzero.io/instances/azure/Standard_D16s_v5 , https://cloudprice.net/vm/Standard_E16s_v5 (accessed 2026-11-04)
- [S27] Nebius compute pricing: https://docs.nebius.com/compute/resources/pricing ; https://nebius.com/prices (accessed 2026-11-04)
- [S28] Jarmusch, Chandrasekaran, Blackwell microbenchmarks, Table I: https://arxiv.org/html/2512.02189 (accessed 2026-11-04)
- [S29] nvCOMP algorithm selection: https://docs.nvidia.com/cuda/nvcomp/algorithm_selection.html (accessed 2026-11-04)
- [S30] Theseus, arXiv 2508.05029: https://arxiv.org/html/2508.05029v1 (accessed 2026-11-04)
- [S31] Luo, Chen, Binnig, "Do GPUs Really Need New Tabular File Formats?": https://arxiv.org/html/2602.17335 (accessed 2026-11-04)
- [S32] Zeng et al., "An Empirical Evaluation of Columnar Storage Formats", PVLDB 17(2): https://arxiv.org/pdf/2304.05028 (accessed 2026-11-04)
- [S33] PushdownDB: https://arxiv.org/abs/2002.05837 (accessed 2026-11-04)
- [S34] AWS S3 Select (closed to new customers): https://docs.aws.amazon.com/AmazonS3/latest/userguide/selecting-content-from-objects.html (accessed 2026-11-04)
- [S35] Microsoft Fabric intelligent cache: https://learn.microsoft.com/en-us/fabric/data-engineering/intelligent-cache (accessed 2026-11-04)
- [S36] Meta, Raptor to RaptorX (Presto blog 2022-01-28, republished): https://hackernoon.com/avoiding-data-silos-in-presto-in-meta-from-raptor-to-raptorx (accessed 2026-11-04)
- [S37] Uber/Alluxio Presto local cache: https://www.alluxio.io/blog/speed-up-ubers-presto-with-alluxio-part-1 (accessed 2026-11-04)
- [S38] LinkedIn push-based shuffle: https://engineering.linkedin.com/blog/2021/push-based-shuffle-in-apache-spark (accessed 2026-11-04)
- [S39] "Accelerating Presto with GPUs", arXiv 2606.24647: https://arxiv.org/html/2606.24647 (accessed 2026-11-04)
- [S40] GPUDirect Storage release notes v1.19.1: https://docs.nvidia.com/gpudirect-storage/release-notes/index.html (accessed 2026-11-04)
- [S41] GDS O_DIRECT and compat mode guide: https://docs.nvidia.com/gpudirect-storage/o-direct-guide/index.html (accessed 2026-11-04)
- [S42] AWS instance store data persistence: https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instance-store-lifetime.html (accessed 2026-11-04)
- [S43] Nebius local disks: https://docs.nebius.com/compute/storage/local-disks ; disk types https://docs.nebius.com/compute/storage/types (accessed 2026-11-04)
- [S44] AWS S3 pricing: https://aws.amazon.com/s3/pricing/ (accessed 2026-11-04)
- [S45] InMotion Hosting, bare-metal Spark (vendor): https://www.inmotionhosting.com/blog/big-data-bare-metal-hadoop-spark/ (accessed 2026-11-04)
- [S46] AWS EMR file systems: https://docs.aws.amazon.com/emr/latest/ManagementGuide/emr-plan-file-systems.html (accessed 2026-11-04)
- [S47] Uber, securing Hadoop on GCP, 2024-07-30: https://www.uber.com/blog/securing-hadoop-on-gcp (accessed 2026-11-04)
- [S48] Uber, I/O observability, 2025-11-13: https://www.uber.com/blog/i-o-observability-for-ubers-massive-petabyte-scale-data-lake/ (accessed 2026-11-04)
- [S49] LinkedIn, the exabyte club, 2021: https://engineering.linkedin.com/blog/2021/the-exabyte-club--linkedin-s-journey-of-scaling-the-hadoop-distr (accessed 2026-11-04)
- [S50] Databricks disk cache: https://docs.databricks.com/aws/en/optimizations/disk-cache (accessed 2026-11-04)
- [S51] spark-rapids file cache: https://docs.nvidia.com/cudf-spark/latest/additional-functionality/filecache.html (accessed 2026-11-04)
- [S52] spark-rapids discussion #11992: https://github.com/NVIDIA/spark-rapids/discussions/11992 (accessed 2026-11-04)
- [S53] Spark CacheManager.scala (master): https://github.com/apache/spark/blob/master/sql/core/src/main/scala/org/apache/spark/sql/execution/CacheManager.scala (accessed 2026-11-04)
- [S54] Spark SharedState.scala (master): https://github.com/apache/spark/blob/master/sql/core/src/main/scala/org/apache/spark/sql/internal/SharedState.scala (accessed 2026-11-04)
- [S55] Spark 4.3.0 job scheduling, dynamic allocation: https://spark.apache.org/docs/latest/job-scheduling.html (accessed 2026-11-04)
- [S56] DGX H100 specs (secondary): https://www.weka.io/learn/glossary/gpu/nvidia-dgx-h100 ; NVIDIA guide https://docs.nvidia.com/dgx/dgxh100-user-guide/introduction-to-dgxh100.html (accessed 2026-11-04)
- Code: Comet `ab46419fcc99bcd34aa4d89f7b545d5d51694a88` (tag 1.2.0); spark-rapids `ce6349699ccb3bd527c223c66ba24bd708e67e18`; Gluten `d64a51dfd01bbe1b8e4adb6c1f74331fb75f531f`. Local: `llm-wiki/reports/benchmark-minimal.md` (commit `de17e8bc`), `llm-wiki/artifacts/product-research/inventory.md`, `llm-wiki/artifacts/product-research/options/lakehouse-iceberg.md`, `llm-wiki/artifacts/product-research/options/spark-plugin.md`.
