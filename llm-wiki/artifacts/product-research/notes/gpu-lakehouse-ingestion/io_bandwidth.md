# I/O bandwidth path: object store / NVMe -> GPU HBM (as of 2026-10-29)

All URLs accessed 2026-10-29. "(estimate)" marks my own arithmetic or judgement, not a sourced number.

## Q1. Object-store throughput per node (S3 / GCS / Azure), request limits, range-read sizing

### Takeaway
No cloud vendor publishes a per-client GB/s ceiling for object storage; the documented limits are request rates (S3: 3,500 write / 5,500 GET per second per prefix) plus, on Azure, a per-account egress cap (200 Gbps in major regions). In practice the cap per node is the instance's general-purpose NIC. One TCP flow is capped at 5 Gbps on EC2, so 25-100 Gbps takes tens to hundreds of concurrent range GETs. Published per-node results are 10-20 Gbps on 25/50 Gbps g4dn instances (KvikIO) and about 4.85 GB/s per node with Mountpoint on c6i.16xlarge.

### Cited Findings
- S3 per-prefix rates: "more than 3,500 PUT/COPY/POST/DELETE or 5,500 GET/HEAD requests per second per prefix" triggers scaling, with HTTP 503 Slow Down during gradual auto-scaling. Sustained rates above about 5,000 req/s to a small number of objects can get 503s. — [AWS S3 performance design patterns](https://docs.aws.amazon.com/AmazonS3/latest/userguide/optimizing-performance-design-patterns.html) (accessed 2026-10-29)
- On throughput, AWS gives no per-connection number on the current guideline pages. It tells clients to issue concurrent byte-range GETs "to fully use your network interface card (NIC) capacity" and to align ranges to multipart part boundaries. "Amazon S3 doesn't have any limits for the number of connections made to your bucket." — [AWS S3 performance design patterns](https://docs.aws.amazon.com/AmazonS3/latest/userguide/optimizing-performance-design-patterns.html); [AWS S3 performance guidelines](https://docs.aws.amazon.com/AmazonS3/latest/userguide/optimizing-performance-guidelines.html) (accessed 2026-10-29). Note: the older "8-16 MB range" and "~85-90 MB/s per connection" figures that circulate were NOT on the current pages I fetched (see Gaps).
- Retry guidance: for requests over 128 MB, retry the slowest 5%. For requests under 512 KB (median latency "tens of milliseconds"), retry after 2 s. — [AWS S3 design patterns](https://docs.aws.amazon.com/AmazonS3/latest/userguide/optimizing-performance-design-patterns.html) (accessed 2026-10-29)
- EC2 flow limits: single-flow (5-tuple) traffic is capped at 5 Gbps outside a cluster placement group (10 Gbps inside one; 25 Gbps with ENA Express inside the same AZ). Traffic through an internet gateway is capped at 5 Gbps (<32 vCPU) or 50% of instance bandwidth (>32 vCPU). In-region traffic "can use the full bandwidth available". Instances of 4xlarge and smaller have "up to" bandwidth with a lower baseline and burst credits lasting 5-60 min. — [EC2 instance network bandwidth](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/ec2-instance-network-bandwidth.html) (accessed 2026-10-29)
- KvikIO S3 benchmark (NVIDIA blog, 2025-02-27, Tom Augspurger): a 1 GB S3 object on g4dn.xlarge (up to 25 Gbps) read at about 10 Gbps with 64-128 threads and 16 MiB tasks, versus about 2 Gbps for boto3. Throughput was flat at about 10 Gbps across 4-128 MiB tasks: smaller tasks add HTTP overhead, larger ones reduce concurrency. Reading 360 Parquet files (about 46 GB) with Dask-cuDF on g4dn.12xlarge (up to 50 Gbps, 4 workers) gave "almost 20 Gbps" with KvikIO versus about 5 Gbps with boto3. — [NVIDIA blog: High-performance remote IO with KvikIO](https://developer.nvidia.com/blog/high-performance-remote-io-with-nvidia-kvikio) (accessed 2026-10-29)
- Mountpoint for S3 (CRT): IOR on c6i.16xlarge measured about 4.85 GB/s read and 4.23 GB/s write per node from one S3 bucket. — [AWS HPC blog, Mountpoint for HPC](https://aws.amazon.com/blogs/hpc/improve-the-speed-and-cost-of-hpc-deployment-with-mountpoint-for-amazon-s3) (search snippet, accessed 2026-10-29). Mountpoint CI benchmarks run on m5dn.24xlarge (100 Gbps), same region. — [mountpoint-s3 BENCHMARKING.md](https://github.com/awslabs/mountpoint-s3/blob/master/doc/BENCHMARKING.md) (accessed 2026-10-29). AWS only claims "terabits per second of aggregate throughput" across many instances. — [AWS Storage blog, inside story on Mountpoint](https://aws.amazon.com/blogs/storage/the-inside-story-on-mountpoint-for-amazon-s3-a-high-performance-open-source-file-client/) (accessed 2026-10-29)
- s3-connector-for-pytorch: AWS's Dec 2025 data-loading blog says the Connector "consistently achieved the highest throughput" among S3 clients tested and recommends shards of about 100 MB-1 GB. No Gbps per p4d/p5 was found. — [AWS ML blog, data loading best practices](https://aws.amazon.com/blogs/machine-learning/applying-data-loading-best-practices-for-ml-training-with-amazon-s3-clients) (accessed 2026-10-29)
- S3 Express One Zone: up to 200,000 reads and 100,000 writes per second per directory bucket by default. No per-client GB/s is published, and AWS recommends CRT-based SDKs/CLI for the highest performance. — [S3 Express performance](https://docs.aws.amazon.com/AmazonS3/latest/userguide/s3-express-performance.html) (accessed 2026-10-29)
- Azure Blob (GPv2 standard account) defaults: 200 Gbps egress, 60 Gbps ingress and 40,000 req/s per account in major regions; 50 Gbps egress, 25 Gbps ingress and 20,000 req/s elsewhere. All can be raised via support. — [Azure scalability targets, standard accounts](https://learn.microsoft.com/azure/storage/common/scalability-targets-standard-account) (accessed 2026-10-29)
- GCS initial request rates: 5,000 object reads/s on flat buckets and 40,000 on hierarchical-namespace buckets. GCSFuse docs give only relative gains (parallel downloads give "nine times faster model load"; buffered reads 2-5x for large sequential files) and auto-tune configs for A3 Ultra/Mega and A4, with no absolute GB/s. — [Cloud Storage FUSE performance](https://docs.cloud.google.com/storage/docs/cloud-storage-fuse/performance) (accessed 2026-10-29)
- Anecdotal data point: an independent issue reports kvikio `pread` from S3 at 64 threads and 16 MiB chunks reaching 6.25 Gbps (about 0.78 GB/s) on one pod, versus 0.97 Gbps for Polars object_store. It also notes that `cudf.read_parquet(storage_options=...)` used PyArrow S3, not KvikIO, in that setup. — [boettiger-lab/mcp-gpu-data-server issue #5](https://github.gg22.workers.dev/boettiger-lab/mcp-gpu-data-server/issues/5) (accessed 2026-10-29; mirror domain, low confidence)

### Inferences
- Per-stream figure (estimate): 5 Gbps is the hard EC2 per-flow cap, and real S3 single-GET throughput is commonly about 50-100 MB/s. Filling 100 Gbps (12.5 GB/s) therefore needs about 125-250 in-flight range GETs. At 8-16 MiB per GET that is about 800-1,500 GETs/s per node (estimate), comfortably under 5,500/s per prefix for one node. A 16-node cluster on one prefix could reach the per-prefix GET limit, so spread files across prefixes or warm the bucket up.
- Parquet column chunks are often well under 1 MB for narrow columns. Projected scans then become request-rate- and latency-bound rather than bandwidth-bound unless adjacent ranges are coalesced (Theseus's custom object-store datasource did this and cut SF10k runtime 75%, see Q5). Estimate.
- Azure's 200 Gbps per-account egress is not the limit for one node: the ND H100 v5 frontend is about 80 Gbps (see Q2). But 3+ such nodes reading one storage account would hit the account cap without a raise (estimate).

### Gaps
- No authoritative per-instance S3 GB/s on p5/p5en/p6 (ENA path) was found. The often-quoted "about 85-90 MB/s per connection, 8-16 MB ranges" guidance is not on the current AWS pages I fetched, so treat it as folklore unless re-sourced.
- No measured GCS or Azure Blob per-VM GB/s on GPU VMs was found.
- No authoritative S3 Express One Zone single-client GB/s figure; third-party blogs found were low quality.

## Q2. Instance NIC limits: how much of the "3200 Gbps" is usable for object storage

### Takeaway
On GPU "superpods" the headline 3,200-3,600 Gbps is GPU-to-GPU RDMA (EFA/InfiniBand/CX-7). Object-storage traffic runs over the general-purpose NICs. On GCP A3 Ultra, A4 and A4X those are documented at 400 Gbps, Azure ND H100 v5 lists about 80 Gbps frontend, and AWS doesn't split EFA from ENA on its spec tables. Mid-range GPU instances (g5/g6/g6e) have 10-100 Gbps, with g6e.48xlarge at 400 Gbps.

### Cited Findings
- AWS network spec table (accelerated computing): g5.48xlarge 100 Gbps; g6.4xlarge 10/25 Gbps (baseline/burst); g6.48xlarge 100 Gbps; g6e.xlarge 2.5/20 Gbps; g6e.48xlarge 400 Gbps (4 network cards); p4d.24xlarge 4x100 Gbps; p5.48xlarge, p5en.48xlarge and p6-b200.48xlarge 3,200 Gbps; p6-b300.48xlarge 6,400 Gbps. — [EC2 accelerated computing specs](https://docs.aws.amazon.com/ec2/latest/instancetypes/ac.html) (accessed 2026-10-29)
- P5 product page: p5.4xlarge has 100 Gbps EFA; p5/p5e/p5en.48xlarge have 3,200 Gbps EFA. EBS bandwidth is 80 Gbps (p5) or 100 Gbps (p5en). No ENA-only or S3 throughput figure is given; the "hundreds of GB/s" claim refers to FSx for Lustre. — [AWS EC2 P5](https://aws.amazon.com/ec2/instance-types/p5/) (accessed 2026-10-29)
- FSx for Lustre + EFA + GDS: "up to 12x higher throughput (up to 1200 Gbps) per client instance" on P5, versus a 100 Gbps per-client limit over traditional TCP. Docs list P5/P5e/P5en/P6-B200 as GDS clients. A Japanese AWS blog says 1,500 Gbps / 15x, which conflicts with the English sources. — [AWS What's New, Nov 2024](https://aws.amazon.com/about-aws/whats-new/2024/11/amazon-fsx-lustre-elastic-fabric-adapter-nvidia-gpudirect-storage/); [AWS News Blog](https://aws.amazon.com/blogs/aws/amazon-fsx-for-lustre-unlocks-full-network-bandwidth-and-gpu-performance/) (accessed 2026-10-29)
- GCP GPU VMs. A3 Ultra (H200) and A4 (B200): 3,600 Gbps total = 8x CX-7 at 3,200 Gbps "dedicated for only high-bandwidth GPU to GPU communication" plus 2 gVNIC at 400 Gbps for storage and other traffic. A4X (GB200): 1,600 Gbps RDMA plus 400 Gbps Titanium. A3 Mega has 1,800 Gbps and A3 High 1,000 Gbps total (split not captured). G2 (L4) has 100 Gbps. — [GCP GPU network bandwidth](https://docs.cloud.google.com/compute/docs/gpus/gpu-network-bandwidth) (accessed 2026-10-29)
- Azure ND H100 v5: 3.2 Tbps InfiniBand (400 Gb/s CX7 per GPU). Frontend Ethernet is listed as 80,000 Mbps in one localized size table, while an older English page shows 2,400 Mbps, so the sources conflict. — [Azure ND H100 v5 series](https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/gpu-accelerated/ndh100v5-series) (accessed 2026-10-29)
- Theseus cloud runs used g6.4xlarge (1x L4, 25 Gbps peak). — [Theseus paper, arXiv 2508.05029](https://arxiv.org/html/2508.05029v1) (accessed 2026-10-29)

### Inferences
- Object-store ingest ceiling per node (estimate). AWS g6/g5 small sizes: 1.25-3 GB/s. g6.48xlarge and g5.48xlarge: 12.5 GB/s. g6e.48xlarge: up to 50 GB/s on paper. GCP A3U/A4: ≤ 50 GB/s (400 Gbps gVNIC). Azure ND H100 v5: about 10 GB/s if 80 Gbps is correct. For p5, the S3 ceiling is likely well below 3,200 Gbps (EFA-only interfaces carry SRD/RDMA, and S3 is TCP over ENA). The exact ENA share is undocumented, so treat it as ≤ 100-400 Gbps.
- Compared with a single H100 doing scan and decompress at tens to hundreds of GB/s, every row above is NIC-bound by 1-2 orders of magnitude for cold scans (estimate).

### Gaps
- No AWS doc found that states how much p5/p6 bandwidth is reachable over ENA (TCP to S3) versus EFA-only interfaces.
- No measured S3 GB/s on g6e.48xlarge (400 Gbps).

## Q3. Local NVMe and GPUDirect Storage / KvikIO / cuObject

### Takeaway
GPU instances ship large local NVMe arrays: p5, p5en, p6-b200 and p6-b300 each have 8x3.8 TB, rated 4.4M random-read IOPS. AWS publishes no sequential GB/s. GDS gives a direct NVMe-to-GPU DMA path, but its gain over a pinned bounce buffer is mostly at small I/O sizes. KvikIO's remote (S3/HTTP) reader always bounces through pinned host memory via libcurl, so remote reads never use GDS. cuObject (S3-over-RDMA) exists but needs RDMA-capable S3 servers; public cloud object stores don't offer this.

### Cited Findings
- Instance store (AWS table: 4 KiB random read/write IOPS): p4d.24xlarge 8x1000 GB, 2.0M/1.6M; p5, p5en, p6-b200 and p6-b300 8x3800 GB, 4.4M/2.2M; g6.48xlarge 8x940 GB, 625k/312k; g6e.48xlarge 4x1900 GB, 625k/312.5k; g5.48xlarge 2x3800 GB, 625k/312.5k; g6.4xlarge 1x600 GB, 125k/40k; g6e.xlarge 1x250 GB, 40.6k/20k. — [EC2 accelerated computing specs, instance store](https://docs.aws.amazon.com/ec2/latest/instancetypes/ac.html) (accessed 2026-10-29)
- GDS on local NVMe: a SNIA study on 8x V100 with 8x NVMe found GDS peaks at 64 workers versus 12 for the legacy path, with gains largest at small transfer sizes and "negligible at large IOs". — [SNIA SDC EMEA 2021, Analyzing effects of GPUDirect Storage](https://www.snia.org/sites/default/files/SDCEMEA/2021/snia-analyzing-effects-of-GPU-direct-storage.pdf) (accessed 2026-10-29). NVIDIA's guide covers the gdsio/gdsbench tools but I found no H100/cloud gdsio number in it. — [GDS Benchmarking and Configuration Guide](https://docs.nvidia.com/gpudirect-storage/configuration-guide/index.html) (accessed 2026-10-29)
- GDS over network: a DGX-2 with 8x100 Gb NICs reached a sustained 94+ GB/s at 1 MB reads via NFS over RDMA (VAST). — [VAST Data GDS blog](https://www.vastdata.com/blog/nvidia-gpu-direct-for-storage-gds-the-vast-data-story) (accessed 2026-10-29)
- KvikIO code (rapidsai/kvikio @ `bb8639d71bd11d336f36a6b1dde76bb978bc36f3`, 2026-10-29):
  - `cpp/src/remote_handle.cpp` defines `BounceBufferH2D` ("Bounce buffer in pinned host memory"), which copies received bytes to the device buffer when full. The remote path makes no cuFile calls.
  - Endpoints: S3, S3_PUBLIC, S3_PRESIGNED_URL, WEBHDFS and generic HTTP (`cpp/include/kvikio/remote_handle.hpp`). There is no native GCS or Azure type; those would go through HTTP/presigned URLs (inference).
  - Defaults (`cpp/src/defaults.cpp`, `docs/source/runtime_settings.rst`): `KVIKIO_NTHREADS` 1, `KVIKIO_TASK_SIZE` 4 MiB, `KVIKIO_BOUNCE_BUFFER_SIZE` 16 MiB (one per thread), `KVIKIO_HTTP_MAX_ATTEMPTS` 3, `KVIKIO_HTTP_TIMEOUT` 60 s, retry on 429/500/502/503/504.
  - `KVIKIO_REMOTE_IO_BACKEND`: `EASY_THREADPOOL` (default; one blocking `curl_easy_perform` per thread, so concurrency = NTHREADS) or `MULTI_POLL` (libcurl multi with N reactor threads; `KVIKIO_REMOTE_IO_MAX_CONCURRENT_REQUESTS` default 256). There is also a shared DNS cache (16 threads per cache).
  - Source: [rapidsai/kvikio](https://github.com/rapidsai/kvikio/tree/bb8639d71bd11d336f36a6b1dde76bb978bc36f3) (accessed 2026-10-29)
  - Implication: the default NTHREADS=1 is far too low for S3. The NVIDIA blog found 64-128 threads optimal on g4dn ([blog](https://developer.nvidia.com/blog/high-performance-remote-io-with-nvidia-kvikio)).
- cuObject ("GPUDirect Storage for Objects") is a suite for direct transfers between GPU or system memory and S3-compatible storage over RDMA, avoiding kernel TCP and able to bypass the CPU for payload. It requires Dynamic Connection transport over InfiniBand or RoCEv2. Client v1.0.0 has release notes. — [NVIDIA cuObject docs](https://docs.nvidia.com/gpudirect-storage/cuobject); [cuObject PDF](https://docs.nvidia.com/gpudirect-storage/cuobject/pdf/cuobject.pdf); [cuObject client release notes](https://docs.nvidia.com/gpudirect-storage/cuobject/cuobject-client-release-notes/index.html) (accessed 2026-10-29)
- S3-over-RDMA ecosystem: S3 metadata and auth stay on the S3/HTTP session, while object bytes move out-of-band over RDMA ([StorageNewsletter](https://www.storagenewsletter.com/?p=304686)). Dell is integrating it into ObjectScale ([SNIA SDC25, Goldschmidt](https://www.snia.org/sites/default/files/2025-10/SNIA-SDC25-Goldschmidt-Accelerating-Object-Storage-AIML.pdf)). The open-source VersityGW `vgwrdma` gateway moves object data over RoCE/IB ([versitygw wiki](https://github.com/versity/versitygw/wiki/S3-RDMA)). No published cuObject GB/s was found. (all accessed 2026-10-29)

### Inferences
- p5 local NVMe aggregate (estimate): 4.4M IOPS × 4 KiB ≈ 18 GB/s is a lower bound from random IOPS. Sequential large reads on 8 PCIe Gen4 data-center NVMe drives typically reach about 6-7 GB/s per drive, so about 25-50 GB/s aggregate before host-side limits. Even the low end exceeds the probable S3 ENA bandwidth on the same box.
- Object stores on AWS, GCP and Azure don't expose RDMA to tenants, so cuObject and GDS-for-objects don't apply to S3/GCS/Azure Blob as of this date. For cloud S3 the path is always NIC → kernel TCP → host RAM (pinned bounce) → PCIe H2D (inference from the above).
- Bounce-buffer copy cost is negligible relative to NIC speed: PCIe Gen5 x16 ≈ 50+ GB/s versus a 12.5-50 GB/s NIC (estimate).

### Gaps
- No measured gdsio GB/s on any cloud GPU instance store. No AWS statement that GDS is supported on p5 instance-store NVMe, as opposed to FSx for Lustre.
- No published cuObject throughput numbers.

## Q4. PCIe, C2C and HBM bandwidth

### Takeaway
Host-to-GPU links (PCIe Gen5 x16 at 128 GB/s bidirectional, about 64 GB/s each way; Grace NVLink-C2C at 900 GB/s) and HBM (3.35-8 TB/s) are 1-3 orders of magnitude faster than any cloud NIC path to object storage. They don't limit a cold scan.

### Cited Findings
- H100 SXM: 3.35 TB/s HBM3, NVLink 900 GB/s, PCIe Gen5 128 GB/s. H100 NVL: 3.9 TB/s, NVLink 600 GB/s, PCIe Gen5 128 GB/s. — [NVIDIA H100 product page](https://www.nvidia.com/en-us/data-center/h100/) (accessed 2026-10-29)
- H200: 141 GB HBM3e at 4.8 TB/s. B200: about 8 TB/s (memory 180-192 GB, sources vary). GH200: sources conflict on figures (HBM side about 4-4.9 TB/s; LPDDR5X up to about 384-512 GB/s). — [Spheron H200 specs](https://www.spheron.network/blog/nvidia-h200-specs/); [getdeploying GH200](https://getdeploying.com/gpus/nvidia-gh200); [rdp.in H100 vs H200 vs B200](https://rdp.in/gpu-mart/knowledge-base/h100-vs-h200-vs-b200-which-gpu-for-your-workload/) (accessed 2026-10-29). These are third-party sources; verify against NVIDIA datasheets.
- p5/p5en GPU peer-to-peer is 900 GB/s NVSwitch. — [AWS EC2 P5](https://aws.amazon.com/ec2/instance-types/p5/) (accessed 2026-10-29)

### Inferences
- Practical H2D (estimate): pinned memory reaches about 80-90% of the per-direction link rate, i.e. about 25 GB/s on Gen4 x16 and about 50-55 GB/s on Gen5 x16. Pageable memory is typically half of that or worse because of the staging copy. Use pinned bounce buffers, as KvikIO does.
- NVLink-C2C 900 GB/s is the total bidirectional figure (about 450 GB/s per direction), widely quoted for GH200/GB200; I did not get an NVIDIA primary page for it in this session.

### Gaps
- No NVIDIA primary source fetched for the GH200 C2C or B200 HBM figures, and no measured bandwidthTest pinned/pageable numbers.

## Q5. Published GPU query engine benchmarks on remote Parquet; is the scan the bottleneck?

### Takeaway
Direct evidence is thin but consistent: the object-store read path dominates cold GPU scans. Theseus cut S3 cold-run time 4x purely by changing the S3 datasource and prefetching. KvikIO and RAPIDS ship custom S3 readers (libcurl, CRT) because default clients (boto3, PyArrow S3, Hadoop S3A) leave 4-5x on the table.

### Cited Findings
- Theseus (Voltron Data; arXiv 2508.05029, Aug 2025):
  - All results are cold starts reading Parquet from WEKA (on-prem) or AWS S3 (cloud, g6.4xlarge L4, 25 Gbps).
  - On cloud TPC-H SF10k, the Arrow S3 baseline took 3,203.7 s. A custom Object Store Datasource that "coalesces multiple reads into single requests" brought this to 812.5 s (-75%). Byte Range Pre-loading gave 651.6 s and Compute Task Pre-loading 530.9 s.
  - The paper's framing is that serializing file I/O with compute "may cancel out the benefit of adding GPUs". It reports up to 4x Photon at cost parity; Photon ran on r7gd.12xlarge (22.5 Gbps). No storage GB/s figures are given.
  - Source: [Theseus paper](https://arxiv.org/html/2508.05029v1) (accessed 2026-10-29)
- Voltron press release: TPC-H SF100k on unsorted Parquet "directly from storage" in under an hour with 5 TB of GPU memory. — [Voltron Data news, 2024-03-18](https://voltrondata.com/news/voltron-data-advances-theseus-making-it-the-first-petabyte-scale-query-engine-for-large-scale-data-processing) (accessed 2026-10-29)
- Dask-cuDF Parquet on S3: almost 20 Gbps with KvikIO versus about 5 Gbps with boto3 on a 50 Gbps g4dn.12xlarge. — [NVIDIA KvikIO remote IO blog](https://developer.nvidia.com/blog/high-performance-remote-io-with-nvidia-kvikio) (accessed 2026-10-29)
- Spark RAPIDS "PerfIO S3 Reader" bypasses Hadoop S3A using AWS SDK v2 async (Netty default, or CRT, which has "lower CPU overhead ... often delivers better throughput"). Defaults: 200 connections and 200 threads. No speedup numbers are published. — [RAPIDS PerfIO S3 docs](https://docs.nvidia.com/spark-rapids/user-guide/latest/additional-functionality/perfio-s3.html) (accessed 2026-10-29)

### Inferences
- With an L4 on 25 Gbps (about 3 GB/s) and cold S3, Theseus's 4x gain from I/O-path changes alone shows the run was I/O-bound (inference; the paper does not say "I/O bound" explicitly).
- For a GPU SQL engine on Iceberg/Parquet in S3: cold scans are NIC- or request-bound on every current cloud GPU shape. Engineering priorities are (a) many concurrent coalesced range GETs (multi-curl / CRT), (b) prefetch overlapped with compute, and (c) a local NVMe cache for warm data (estimate).

### Gaps
- No Sirius-on-remote-storage benchmark was found in this session.
- No Spark RAPIDS S3-versus-local benchmark numbers were found.
- No direct benchmark of `cudf.read_parquet` through KvikIO's S3 path with GB/s figures.

## Q6. Lakehouse scan caches: encoded or decoded?

### Takeaway
Trino's filesystem cache and Alluxio cache raw file bytes (encoded Parquet pages) in fixed-size pages on local SSD. Databricks' disk cache stores Parquet in a proprietary "fast intermediate format" on local SSD, with compression optional via config. Starburst Warp Speed adds indexes plus cache on local NVMe. Snowflake's warehouse cache sits on local SSD and is lost on suspend; its format is undocumented.

### Cited Findings
- Trino filesystem cache: "copies of the files retrieved from object or remote storage" per node via the Alluxio libraries (page-based). `fs.cache.page-size` defaults to 1 MB (64 kB-15 MB). Other settings: `fs.cache.ttl` default 7d; `node-scheduler.cache-preferred-hosts-count` default 2 (soft affinity). Recommended storage is dedicated local SSD or a RAM disk. — [Trino file system cache](https://trino.io/docs/current/object-storage/file-system-cache.html) (accessed 2026-10-29). Alluxio caching arrived in Trino 429. — [Alluxio blog](https://www.alluxio.io/blog/trino-and-alluxio-better-together) (accessed 2026-10-29)
- Databricks disk cache: "stores a copy on its local storage in a fast intermediate format". Config keys are `spark.databricks.io.cache.compression.enabled` ("should the cached data be stored in compressed format", example `false`), `maxDiskUsage` and `maxMetaDataCache`. It uses at most half the local SSD, is automatic on first read, and in DBR 14.2+ and SQL warehouses an "enhanced disk caching algorithm" replaces `CACHE SELECT`. — [Azure Databricks disk cache doc (updated 2026-10-22)](https://learn.microsoft.com/azure/databricks/optimizations/disk-cache) (accessed 2026-10-29). The 2018 launch blog credits NVMe SSDs and columnar compression. — [Databricks blog 2018](https://www.databricks.com/blog/2018/01/09/databricks-cache-boosts-apache-spark-performance.html) (accessed 2026-10-29). A third-party study guide claims "decompressed Parquet blocks"; this is unverified against official docs. — [open-exam-prep](https://open-exam-prep.com/study-guides/databricks-analyst/ch7/caching-hierarchy-dbsql) (accessed 2026-10-29)
- Starburst Warp Speed: automatically creates indexes and caches from query characteristics, stored on worker-local storage ("typically SSD NVMe"). Data not yet warmed is served from object storage. — [Starburst Galaxy Warp Speed docs](https://docs.starburst.io/starburst-galaxy/data-engineering/optimization-performance-and-quality/workload-optimization/warp-speed-enabled.html) (accessed 2026-10-29)
- Snowflake: the warehouse "local disk" cache is SSD and cleared when the warehouse suspends (default auto-suspend in Snowflake is commonly 10 min). Sources are secondary. — [Sonra deep dive](https://sonra.io/?p=15086); [Snowflake internals (Medium)](https://medium.com/snowflake/deep-dive-into-the-internals-of-snowflake-virtual-warehouses-d6d9676127d2) (accessed 2026-10-29)

### Inferences
- Encoded-byte caches (Trino/Alluxio) save only the NIC hop. Decode still happens every query, which suits a GPU because GPU decode is fast. A decoded or "intermediate" cache (Databricks) trades about 2-5x more SSD capacity for CPU savings (estimate). For a GPU engine, caching encoded/compressed Parquet pages on NVMe maximizes the effective capacity and bandwidth multiplier (estimate).

### Gaps
- Exact Databricks intermediate format, Warp Speed index format and Snowflake cache format are not publicly documented.
