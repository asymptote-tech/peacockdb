# Exotic architectures: what feeds a GPU faster than a 40 Gbit/s NIC

## Summary
- What feeds a GPU faster than a 40 Gbit/s NIC (5 GB/s) falls into two kinds. **Sources** hold the data. **Caches** only hold data that already crossed a source once.
- Rentable sources reach 12.5–16 GB/s per GPU: AWS p5.4xlarge at 100 Gbps, and Nebius's shared filesystem at "up to" 15 GiB/s per client. Neither is measured by us. Faster sources (GPUDirect Storage at 22–28 GB/s per GPU, S3 over RDMA at an estimated 46) are on-prem.
- Caches are fast: GH200 C2C 375 GB/s, GB10 205–219 GB/s (in-repo), NVL72 NVLink 900 GB/s, peer RAM over InfiniBand about 24 GB/s. They help batched ETL that re-reads data, not the one-pass floor.
- CXL memory, Kove:SDM and the research far-memory systems do not beat host RAM over PCIe for a GPU. They are lab-only, unrentable or unpriced. They add capacity, not speed.
- Spark and Comet use none of these fabrics today: Comet shuffles over Spark's TCP block transfer and reads through object_store or HDFS. The plugin would need its own cuFile or RDMA reader and its own shuffle transport.

## How to read this file
- The tier table at the end feeds the cost model. `GB_per_s_to_GPU` is per GPU. `capacity_GB` is what one GPU can reach; `1000000` stands for "unbounded" (object store or parallel filesystem), a placeholder.
- `price_per_hour` is USD per GPU-hour for the GPU side: the GPU node, plus any memory hardware bought per host. On-prem hardware is spread over 3 years at 24/7, 26,280 h (estimate).
- Storage charged by capacity (S3, VAST, WEKA, Nebius SFS) is not folded in, matching the spec's rule for S3 charges; those rates are in the text.
- Tags like (K1) resolve in `## Sources`; every web source there was accessed 2026-11-04. Code claims give the commit SHA inline. In-repo measurements are cited as such.

## 1. RDMA to a memory server
**How it works.** A remote server's DRAM is registered with an RDMA NIC. The GPU node's NIC registers HBM (GPUDirect RDMA via nvidia-peermem or dma-buf) and issues RDMA READs; data lands in HBM without touching the local CPU. HGX-class nodes have one 400G NIC per GPU (N2, N9).
- **Bandwidth.** The NIC is the ceiling: 400 Gbit/s = 50 GB/s per GPU. Mooncake's Transfer Engine reaches 190 GB/s over 8×400G RoCE, about 24 GB/s per NIC (F5) (accessed 2026-11-04). One ConnectX-7 at 400 GbE carried 43.4 GiB/s (46.6 GB/s) of NFS-over-RDMA into GPUs, 96% of line rate (S1) (accessed 2026-11-04). spark-rapids' docs show GPUDirect RDMA *slower* than staging through host (4.6 vs 6.1 GB/s) on machines without PCIe switches (N11) (accessed 2026-11-04).
- **Latency.** A few microseconds per RDMA READ (estimate): fine for streaming column chunks, poor for random probes.
- **Capacity.** Whatever the memory servers hold. Peer GPU nodes have 1.6–2 TiB: Nebius 8×H100 1600 GiB (N1), Azure ND H100 v5 1900 GiB (N9), AWS p5.48xlarge 2 TiB (N6) (accessed 2026-11-04).
- **Kove:SDM.** Pools server memory and lends it to other hosts over InfiniBand; RoCE promised for Q1 2025 (K1). A Viking-built "Memory Tower" went to pre-order November 2024 (K2). Only vendor-run Redis/Valkey benchmarks on OCI, relative latency only, no bandwidth (K3). IEEE Spectrum names a customer (Swift), no price, no GPU support (K4) (accessed 2026-11-04). No source says a GPU can DMA from Kove memory; if it appears as host RAM, the GPU path is host RAM over PCIe, filled at Kove's link rate (estimate).
- **Research far memory.** AIFM (OSDI '20), application-level far-memory runtime (F1). Infiniswap (NSDI '17), remote paging over 56 Gb/s ConnectX-3; 8–25× slowdown with half the working set remote (F2). LegoOS (OSDI '18), disaggregated OS, ConnectX-3 only (F3). Carbink (OSDI '22) adds erasure-coded fault tolerance (F4). All CPU-side research prototypes; none targets GPUs (accessed 2026-11-04). The GPU-side equivalent that ships is LLM KV-cache pooling: Mooncake Store pools cluster DRAM for Kimi, SGLang, vLLM, LMCache (F5).
- **Spark/Comet today:** no. Spark has no remote-memory tier. OSU's RDMA-Spark stopped at 0.9.5 on Spark 2.1 (2018) (F6) (accessed 2026-11-04).
- **What the plugin needs:** a column-chunk directory keyed by file, row group, column; a UCX or verbs client with GPU memory registered; RDMA READ into HBM below Comet's scan. Mooncake's Transfer Engine (Apache-2.0, EFA and RDMA) is a reusable transport (F5). It is a cache: the first read still comes over the source tier.

## 2. CXL memory pools and switches
**How it works.** A CXL Type-3 device puts DRAM behind a PCIe Gen5 link; Linux shows it as a CPU-less NUMA node or a DevDAX device. A switch lets several hosts share one box.
- **Can a GPU read it directly? Not today.** H100 and Blackwell are not CXL devices; the GPU reaches CXL memory as host memory, GPU → PCIe → CPU root complex → CXL device (X2). A real-hardware study measured +500 ns from the GPU vs +120 ns from the CPU (A10, CXL 1.1); GPU bandwidth equal to DRAM because PCIe was the limit (X2) (accessed 2026-11-04).
- **Best GPU measurement.** CCCL (2026) pooled six Micron CZ120 cards (128 GB, Gen5 x8) behind a CXL switch, mapped via DevDAX + `cudaHostRegister`, copied with `cudaMemcpyAsync` from an H100 (X1): about 20 GB/s, capped by the x8 card and the GPU's one copy engine per direction. CPU latency 658 ns vs 214 ns local DRAM. The pool is "not available in the market"; 12-node results are emulated (accessed 2026-11-04).
- **Vendors** (accessed 2026-11-04):
  - Samsung CMM-D / CMM-B: CMM-B launched at 2 TB (8× E3.S CMM-D), 60 GB/s, 596 ns (X3); a later showcase claims up to 24 TB and 8 hosts (X4). No price.
  - Astera Labs Leo: CXL 2.0 x16 controller, 2 DDR5-5600 channels, up to 2 TB (X6). In Azure M-series VMs as a preview since 2025-11-18 (X5): CPU-only VMs, no GPU SKU. Gen5 x16 ≈ 64 GB/s per direction (estimate, arithmetic).
  - Liqid: CXL 2.0 composable memory, 1–20 TB per EX-5410C chassis, 100 TB over 5 chassis, shared by up to 32 servers (X7). No bandwidth, no price.
  - UnifabriX MAX: CXL memory appliance aimed at AI, claims up to 256 TB, no clear per-device bandwidth (X8). No price.
  - Panmnesia: PCIe 6.4 / CXL 3.2 fabric switch as samples and pilot systems (CES 2026); the April 2026 "mass production" release was corrected to "sample chip" (X9). Its GPU-side CXL root port (CXL-Opt, 2024) reports two-digit-ns round trips, on a custom GPU prototype, not an NVIDIA part (X10).
- **Spark/Comet today:** transparently, CPU side only: a JVM or Comet sees more NUMA memory. Nothing GPU-specific.
- **What the plugin needs:** host buffers on the CXL NUMA node (or DevDAX mmap), `cudaHostRegister`, copy in. A host-RAM tier with more capacity and less bandwidth.

## 3. Grace superchips
**How it works.** NVLink-C2C joins a Grace CPU's LPDDR5X to the GPU coherently; the GPU can copy from it or read it in place (ATS). NVL72 adds a rack-wide NVLink fabric between GPUs.
- **GH200.** 375 GB/s H2D and 297 GB/s D2H copies vs 450 GB/s spec; HBM 3.4 TB/s; LPDDR5X 486 GB/s STREAM (G1). 480 GB LPDDR5X + 96 GB HBM; Lambda's 1-GPU instance shows 64 vCPU, 432 GB (G5). Prices: Vultr $1.99, Lambda $2.29, Denvr $3.87, CoreWeave $6.50 per hour (CoreWeave: 72 vCPU, 480 GB, 7.68 TB local) (G5, G6) (accessed 2026-11-04). These are the Grace option file's leads (R3), re-checked. **Cheaper per hour than Nebius's $4.50 H100 (N3).** Stock is often short (R3).
- **GB200.** One Grace (480 GB, up to 512 GB/s) feeds two Blackwells (186 GB HBM each) (G2). Per-GPU C2C with both streaming is unmeasured: at most about 256 GB/s each (estimate). CoreWeave sells a 4-GPU instance at $42/h, $10.50/GPU-h (G6) (accessed 2026-11-04).
- **GB200 NVL72 rack-wide memory.** 13.4 TB HBM3E and 17 TB LPDDR5X per rack; NVLink 1.8 TB/s per GPU both directions, about 900 GB/s each way (G2). Peer HBM is reachable via fabric handles and IMEX (G9). **No source shows a GPU reading another tray's Grace LPDDR5X**; HOST_NUMA allocations exported as fabric handles are undocumented (G9) (accessed 2026-11-04). Treat the "17 TB pool" as unverified for this use. NVIDIA's own Presto-on-NVL72 run reads through GDS from IBM Storage Scale (about 4.5 TiB/s aggregate), not Grace memory, and exchanges over UCX (G8) (accessed 2026-11-04).
- **GB10 (DGX Spark), in-repo measurements** (`llm-wiki/reports/dgx-spark.md` §1, §11 @ c7c8568e) (R1): one 121.7 GiB LPDDR5X pool; a kernel reads mapped host memory at 205–219 GB/s vs 233–239 GB/s device memory; `cudaMemcpy` capped at about 59 GB/s; CPU memcpy traffic takes up to 37% of the GPU's bandwidth. NVIDIA lists 273 GB/s and one ConnectX-7 at 200 Gbps (G3); list price $3,999 (G4) (accessed 2026-11-04).
- **Spark/Comet today: yes, CPU side.** Comet publishes Linux arm64 jars (`docs/source/user-guide/latest/installation.md:28-34` @ ab46419fcc99bcd34aa4d89f7b545d5d51694a88) (C1). spark-rapids ran NDS on 16 GH200 nodes in 2024 (6 min vs 42 min on 16 x86 nodes), not saying where the data lived or what fabric was used (G7) (accessed 2026-11-04).
- **What the plugin needs:** aarch64 builds of peacockdb's executor (it already runs on GB10, R1); a NUMA-local decoded column cache in Grace memory, read in place or by async copy; join builds kept in HBM. The Grace file sizes this at 12.5–23.5k lines on peacockdb (estimate) (R3).

## 4. RDMA storage through GPUDirect Storage (VAST, WEKA, DDN)
**How it works.** cuFile DMAs file data from NVMe, or from an RDMA filesystem client, straight into HBM, skipping the CPU bounce buffer (S7).
- **Supported targets:** local NVMe, NVMe-oF, DDN EXAScaler, VAST (NFS), WekaFS, Lustre, NFS. RDMA filesystems need MLNX_OFED; files must use O_DIRECT (S7) (accessed 2026-11-04).
- **Published throughput** (vendor unless noted): VAST 162 GiB/s (174 GB/s) to one DGX A100 over 8×200 Gb IB, about 21.7 GB/s per GPU (S2, S3); third-party: 26.4 GiB/s to one L40S and 43.4 GiB/s to four over one 400 GbE ConnectX-7 (S1). WEKA 73 GB/s to one DGX-2 (2021) (S4). DDN >85 GB/s read to one DGX B200 over two links (AI400X2-Turbo) (S5); 162 GiB/s per DGX A100 claimed with GDS (S6). Local NVMe for scale: 24.3 GiB/s into one L40S from 4× Gen5 drives RAID0 (S1) (accessed 2026-11-04); that is the L40S's PCIe Gen4 x16 link limit, not the drives' (data-path.md, Rates for the cost model).
- **S3 over RDMA.** NVIDIA cuObject client ships with CUDA 13.1.1+; server library r1.16 (2026-01) (S8). Cloudian, MinIO, Scality, IBM support it. IBM reports about 300 GB/s aggregate S3 GETs on Storage Scale over IB (S9) (accessed 2026-11-04). No per-client figure published.
- **Price.** WEKA AWS Marketplace flash license $1,000/TB-year ≈ $0.083/GB-month before hardware (S10). VAST and DDN publish none. CoreWeave Distributed File Storage (VAST-based) $0.07/GiB-month (G6). CoreWeave object store with LOTA measured 2.3 GiB/s per H200, *below* a 40 Gbit NIC (S11) (accessed 2026-11-04). Nebius shared filesystem "tested up to" 15 GiB/s read per client at $0.08/GiB-month (S12, N3); GDS support and the 1-GPU VM's rate not stated.
- **Spark/Comet today: not via GDS.** Comet reads through Rust object_store and opendal/HDFS (`native/core/Cargo.toml:74-133` @ ab46419f) (C1). spark-rapids removed GDS spill in 23.x (`docs/archives/CHANGELOG_23.02_to_23.12.md:445`, #9270 @ ce6349699ccb3bd527c223c66ba24bd708e67e18); a grep at that SHA finds no cuFile or KvikIO on the scan path (C2), confirming spark-plugin.md (R4). A POSIX mount of VAST or WEKA works today without GDS as `file://` paths.
- **What the plugin needs:** a native GPU Parquet reader calling cuFile (KvikIO) on GDS mounts, or cuObject for S3 over RDMA, replacing Comet's scan for files on those mounts. The only exotic tier that speeds up one-pass ETL.

## 5. Cloud GPU nodes with InfiniBand or EFA, and Spark on the fabric
- **Nebius.** H100 $4.50/GPU-h on both presets (N3). Only `8gpu-128vcpu-1600gb` can join an InfiniBand GPU cluster; `1gpu-16vcpu-200gb` cannot (N1). Fabrics give 400 Gbps per GPU, 3.2 Tbps per VM (N2). No IPoIB documented. **The fabric is free per GPU, but only in 8-GPU units, and links GPU nodes to each other, not to storage.**
- **AWS.** p5.48xlarge: 3,200 Gbps EFA but at most 800 Gbps IP, about 12.5 GB/s per GPU for S3/TCP (N5); $55.04/h, $6.88/GPU-h (N7). p5.4xlarge (1 H100, 256 GiB, one 3.84 TB NVMe): 100 Gbps EFA, no GPUDirect RDMA (N6) (accessed 2026-11-04). EFA traffic goes only via libfabric (NCCL, MPI, NIXL), is not routable, cannot cross AZs; plain TCP uses ENA (N4).
- **Azure ND H100 v5.** 8×400 Gb IB with GPUDirect RDMA, but only 80 Gbps Ethernet per VM, 10 Gbit per GPU (N9). Third-party price $98.32/h, $12.29/GPU-h (N10) (accessed 2026-11-04).
- **CoreWeave.** HGX H100 8-GPU $49.24/h, $6.16/GPU-h; the pricing page does not state InfiniBand (G6) (accessed 2026-11-04).
- **Can Spark's shuffle and scans use the fabric?**
  - Comet: no. Its shuffle reader wraps Spark's `ShuffleBlockFetcherIterator` over `blockManager.blockStoreClient`, Netty over TCP (`spark/src/main/scala/org/apache/spark/sql/comet/execution/shuffle/CometBlockStoreShuffleReader.scala:59-62` @ ab46419f) (C1). It could ride IPoIB where offered (none of the four document it). Gluten @ d64a51df and Auron @ cdd73ecf: grep for rdma/ucx finds nothing (C3).
  - spark-rapids UCX shuffle exists and is maintained, but is not the default: `spark.rapids.shuffle.mode` defaults to MULTITHREADED (`sql-plugin/src/main/scala/com/nvidia/spark/rapids/RapidsConfEntries.scala:200-210` @ ce634969); UCX pinned 1.22.0-1 (`pom.xml:1114`). Device bounce buffers use `cudaMalloc` because "CUDA async allocator is not compatible with GPUDirectRDMA" (`shuffle-plugin/src/main/scala/com/nvidia/spark/rapids/shuffle/ucx/UCXShuffleTransport.scala:135`) (C2). Docs list IB, RoCE, NVLink, TCP with `UCX_TLS=cuda_copy,cuda_ipc,rc,tcp`; EFA not mentioned (N11) (accessed 2026-11-04). UCX added an EFA SRD transport in v1.19.0 (2025-08) (`src/uct/ib/efa/srd` @ feb231ce228d160e1ecaae6ffbce2f3141cb2ac6) (C4), so UCX shuffle on EFA is plausible but untested (estimate). The only published gain is 1.23× (Alluxio/NVIDIA slides, cited in N11's ecosystem): small.
  - SparkRDMA (Mellanox): archived 2022-12-20, last push 2019-05-13, Spark ≤2.4 (N12). Successor SparkUCX last pushed 2024-06-05 (N13).
  - Spark on EFA: no shuffle or scan path exists; Spark speaks TCP and EFA does not carry it (N4).
- **What the plugin needs:** its own shuffle transport (UCX, or Mooncake's Transfer Engine) behind a custom ShuffleManager replacing CometShuffleManager; placement on 8-GPU nodes, which breaks the 1-GPU price-parity layout. Scans gain nothing from the fabric unless storage sits on it (VAST/WEKA on IB, on-prem).

## Evidence against
- **Lab-only:** CCCL's CXL pool (X1); Panmnesia (samples, ISCA paper) (X9, X10); AIFM, Infiniswap, LegoOS, Carbink (F1–F4); GPU reads of remote Grace memory in NVL72 (G9).
- **Unrentable for a GPU:** CXL memory anywhere (Azure's preview has no GPU, X5); Kove (no listing); InfiniBand on any 1-GPU VM (N1, N9); GB200 below 4 GPUs (G6).
- **Unpriced:** Kove, Samsung CMM-B, Liqid, UnifabriX, Panmnesia, VAST, DDN, GB300.
- **Vendor-only numbers:** every Kove, VAST, WEKA, DDN and cuObject figure. The only independent GDS numbers are S1 and the in-repo GB10 set (R1).
- **Caches do not help the floor.** One-pass ETL reads each byte once, so only source tiers matter. Section 4's tiers are on-prem; rentable sources above 5 GB/s (p5 at 100 Gbit, Nebius SFS) are unmeasured by us.
- **Faster tiers cost more per GPU-hour** on AWS, Azure and CoreWeave ($6.16–14.24 vs $4.50), raising the bar at the human's parity. Exception: GH200 at $1.99–2.29.
- **Slower than it sounds:** CoreWeave's object store measured 2.47 GB/s per GPU (S11); Azure ND's IP path gives 1.25 GB/s per GPU (N9).

## Tier table
```tsv
tier	GB_per_s_to_GPU	capacity_GB	price_per_hour	cloud|on-prem	source
nic40_s3	5	1000000	4.50	cloud	40 Gbit/s NIC per spec llm-wiki/tasks/comet-research.md; Nebius H100 $4.50/GPU-h https://nebius.com/prices (accessed 2026-11-04); capacity unbounded, 1 PB placeholder (estimate)
nic100_s3_p5_4xlarge	12.5	1000000	6.88	cloud	line rate, p5.4xlarge 100 Gbps 1xH100 https://aws.amazon.com/ec2/instance-types/p5/ (accessed 2026-11-04); $6.88/h https://aws-pricing.com/p5.4xlarge.html (accessed 2026-11-04); S3 rate unmeasured (estimate)
nic200_s3_p6_b200	25	1000000	14.24	cloud	200 Gbps ENA per GPU https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/efa-acc-inst-types.html (accessed 2026-11-04); $113.93/8 https://aws-pricing.com/p6-b200.48xlarge.html (accessed 2026-11-04); B200 not H100; S3 rate unmeasured (estimate)
nic400_s3	50	1000000	4.60	on-prem	line rate of one 400 GbE ConnectX-7; GPU at Nebius list plus NIC share (estimate)
local_nvme_x4_gds	26.1	30720	4.73	on-prem	1-GPU gdsio 24.3 GiB/s from 4x Gen5 NVMe RAID0 into one L40S https://gds-report.pages.dev/ (accessed 2026-11-04): the L40S's PCIe Gen4 x16 link limit, not the drives', so a lower bound for an H100 on Gen5; 4x7.68 TB and drive cost over 3 y (estimate)
local_nvme_x1_p5_4xlarge	3.9	3840	6.88	cloud	one 3.84 TB NVMe https://aws.amazon.com/ec2/instance-types/p5/ (accessed 2026-11-04); per-drive rate not published: 550,000 IOPS scaled at i4i's 2.8 GB/s per 400,000 IOPS, data-path.md section 4 (estimate)
host_ram_pcie5	47.3	196	4.50	cloud	pinned H2D measured on H200, in-repo llm-wiki/artifacts/product-research/inventory.md @ 8db9f5f3; 196 GB H100 VM llm-wiki/tasks/new-hosts.md; https://nebius.com/prices (accessed 2026-11-04)
peer_ram_ib400_nebius	24	1718	4.50	cloud	GPUDirect RDMA from a peer node's RAM; 190 GB/s over 8x400G https://github.com/kvcache-ai/Mooncake (accessed 2026-11-04); only the 8-GPU preset joins InfiniBand https://docs.nebius.com/compute/virtual-machines/types (accessed 2026-11-04); needs 16+ GPUs (estimate)
peer_ram_ib400_azure_nd	24	2040	12.29	cloud	rate as Mooncake (estimate); 1900 GiB, 8x400G IB https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/gpu-accelerated/ndh100v5-series (accessed 2026-11-04); $98.32/8 https://pcr.cloud-mercato.com/providers/azure/flavors/standard_nd96isr_h100_v5/pricing (accessed 2026-11-04)
peer_ram_efa_p5_48xlarge	24	2199	6.88	cloud	3200 Gbps EFA, 2 TiB https://aws.amazon.com/ec2/instance-types/p5/ (accessed 2026-11-04); $55.04/8 https://aws-pricing.com/p5.48xlarge.html (accessed 2026-11-04); EFA via libfabric only; rate (estimate)
memserver_rdma_onprem	46	1024	6.21	on-prem	one 400G ConnectX-7 by analogy with 43.4 GiB/s NFSoRDMA https://gds-report.pages.dev/ (accessed 2026-11-04); 1 TB DRAM server over 3 y (estimate)
kove_sdm	25	4096	6.21	on-prem	no GPU path, bandwidth or price published https://www.storagenewsletter.com/2025/09/12/software-defined-memory-kove-announces-benchmarks/ (accessed 2026-11-04); all three numbers (estimate)
cxl_pool_switch	20	2048	7.20	on-prem	H100 cudaMemcpy to CZ120 x8 behind a CXL switch about 20 GB/s https://arxiv.org/html/2602.22457v1 (accessed 2026-11-04); CMM-B 2 TB https://www.anandtech.com/show/21333 (accessed 2026-11-04); unpriced, not rentable (estimate)
cxl_expander_local	40	2048	6.50	on-prem	Leo x16 2 TB https://www.asteralabs.com/products/leo-cxl-smart-memory-controllers (accessed 2026-11-04); GPU via host PCIe, unmeasured; rate and price (estimate)
gh200_c2c	375	480	2.29	cloud	H2D 375 GB/s https://arxiv.org/html/2407.07850v1 (accessed 2026-11-04); Lambda $2.29 https://www.amcompute.com/gpu-cloud-prices/gh200 (accessed 2026-11-04); a cache, filled over the NIC
gb200_c2c_per_gpu	256	240	10.50	cloud	512 GB/s and 480 GB per Grace, shared by 2 GPUs https://www.nvidia.com/en-us/data-center/gb200-nvl72/ (accessed 2026-11-04); $42 per 4 GPUs https://www.coreweave.com/pricing (accessed 2026-11-04); split (estimate)
gb200_nvl72_peer_hbm	900	13400	10.50	cloud	NVLink 1.8 TB/s per GPU both ways, 13.4 TB HBM per rack https://www.nvidia.com/en-us/data-center/gb200-nvl72/ (accessed 2026-11-04); https://www.coreweave.com/pricing (accessed 2026-11-04); remote Grace access unverified
gb10_unified	205	128	0.15	on-prem	kernel reads mapped host memory at 205-219 GB/s, in-repo llm-wiki/reports/dgx-spark.md section 1 @ c7c8568e; $3,999 over 3 y https://www.theregister.com/2025/10/14/dgx_spark_review/ (accessed 2026-11-04) (estimate)
gds_vast_one_nic	28.3	1000000	4.50	on-prem	26.4 GiB/s (28.3 GB/s) into one L40S from VAST over one 400 GbE ConnectX-7; 43.4 GiB/s is four GPUs together, not one (Task 6 audit finding 9) https://gds-report.pages.dev/ (accessed 2026-11-04); storage price not public
gds_parallel_fs_per_gpu	21.7	1000000	4.50	on-prem	162 GiB/s per DGX A100 over 8x200G, divided by 8 https://kb.vastdata.com/documentation/docs/nfs-features-for-enhanced-performance (accessed 2026-11-04); WEKA $1000/TB-y https://aws.amazon.com/marketplace/pp/prodview-2mfqnh6p4yurs (accessed 2026-11-04)
s3_rdma_cuobject	46	1000000	4.50	on-prem	cuObject in CUDA 13.1.1+ https://docs.nvidia.com/gpudirect-storage/cuobject (accessed 2026-11-04); per-client rate unpublished, one 400G NIC (estimate)
nebius_shared_fs	16.1	112589990	4.50	cloud	tested up to 15 GiB/s per client, 100 PiB max https://docs.nebius.com/compute/storage/types (accessed 2026-11-04); $0.08/GiB-month extra; rate from the 1-GPU VM unknown (estimate)
coreweave_caios_lota	2.47	1000000	6.16	cloud	2.3 GiB/s per H200 with LOTA https://coreweave.com/blog/benchmark-results-coreweave-ai-object-storage-delivers-2-gb-s-per-gpu-throughput-across-any-number-of-gpus (accessed 2026-11-04); $49.24/8 https://www.coreweave.com/pricing (accessed 2026-11-04)
```

## Follow-up experiments
1. **Nebius shared filesystem from the 1-GPU H100 VM** (fio, then KvikIO into HBM): is it a rentable source above 5 GB/s on the human's own platform? Cost: minutes of H100 time plus SFS capacity.
2. **GH200 on Lambda ($2.29/h):** measure S3 ingest first, then Comet on 64 Grace cores with a Grace-memory cache. Settles whether the cheapest exotic tier also has a usable source path.
3. **Two Nebius 8×H100 nodes:** `ib_read_bw --use_cuda` from the peer node's RAM into HBM, to test the 24 GB/s per GPU estimate; check whether IPoIB exists for Spark's TCP shuffle.
4. **AWS p5.4xlarge:** S3 throughput to one H100 at 100 Gbps, and GDS from its one NVMe drive. Fills two estimate rows.
5. **spark-rapids UCX shuffle on EFA** (UCX ≥1.19, SRD) on p5.48xlarge: does `UCX_TLS` with srd work, and what does it beat MULTITHREADED by?
6. **CXL:** no action until a cloud offers CXL on a GPU SKU. Re-check the Azure CXL preview and Panmnesia GA quarterly.

## Sources
- R1 llm-wiki/reports/dgx-spark.md @ c7c8568e5ab46b1e9f4c5c73be43d1ee3d5c5ab9 (in-repo GB10 measurements, §1 and §11) (accessed 2026-11-04)
- R2 llm-wiki/artifacts/product-research/inventory.md @ 8db9f5f36867f32f6309392f110d74910d021fbe (in-repo H200 PCIe rates) (accessed 2026-11-04)
- R3 llm-wiki/artifacts/product-research/options/grace-superchip-analytics.md (lead, re-checked here) (accessed 2026-11-04)
- R4 llm-wiki/artifacts/product-research/options/spark-plugin.md (lead) (accessed 2026-11-04)
- C1 apache/datafusion-comet @ ab46419fcc99bcd34aa4d89f7b545d5d51694a88 (tag 1.2.0): CometBlockStoreShuffleReader.scala:59-62, installation.md:28-34, native/core/Cargo.toml:74-133 — https://github.com/apache/datafusion-comet/tree/ab46419fcc99bcd34aa4d89f7b545d5d51694a88 (accessed 2026-11-04)
- C2 NVIDIA/cudf-spark (spark-rapids) @ ce6349699ccb3bd527c223c66ba24bd708e67e18: RapidsConfEntries.scala:200-210, UCXShuffleTransport.scala:135, pom.xml:1114, docs/archives/CHANGELOG_23.02_to_23.12.md:445 — https://github.com/NVIDIA/cudf-spark/tree/ce6349699ccb3bd527c223c66ba24bd708e67e18 (accessed 2026-11-04)
- C3 apache/gluten @ d64a51dfd01bbe1b8e4adb6c1f74331fb75f531f; apache/auron @ cdd73ecfafa52a96a2602235930dc309edb9c019 (grep, no RDMA/UCX) (accessed 2026-11-04)
- C4 openucx/ucx v1.19.0 release notes and src/uct/ib/efa/srd @ feb231ce228d160e1ecaae6ffbce2f3141cb2ac6 — https://github.com/openucx/ucx/releases/tag/v1.19.0 (accessed 2026-11-04)
- K1 Kove:SDM hardware launch — https://prnewswire.com/news-releases/kove-unveils-hardware-memory-solution-embedded-with-worlds-first-patented-software-defined-memory-allowing-enterprises-data-center-providers-and-hyperscalers-to-achieve-unprecedented-data-center-performance-302304036.html (accessed 2026-11-04)
- K2 Kove Memory Tower 39i — https://www.storagenewsletter.com/2024/11/21/kovesdm-memory-tower-39i-with-software-defined-memory/ (accessed 2026-11-04)
- K3 Kove Redis/Valkey benchmarks on OCI — https://www.storagenewsletter.com/2025/09/12/software-defined-memory-kove-announces-benchmarks/ (accessed 2026-11-04)
- K4 IEEE Spectrum on Kove, 2025-05-06 — https://spectrum.ieee.org/computer-memory-ai (accessed 2026-11-04)
- F1 AIFM, OSDI '20 — https://www.usenix.org/conference/osdi20/presentation/ruan (accessed 2026-11-04)
- F2 Infiniswap, NSDI '17 — https://www.usenix.org/system/files/conference/nsdi17/nsdi17-gu.pdf (accessed 2026-11-04)
- F3 LegoOS — https://github.com/wuklab/legoos (accessed 2026-11-04)
- F4 Carbink, OSDI '22 — https://www.usenix.org/conference/osdi22/presentation/zhou-yang (accessed 2026-11-04)
- F5 Mooncake (Transfer Engine, Store) — https://github.com/kvcache-ai/Mooncake (accessed 2026-11-04)
- F6 OSU HiBD RDMA-Spark — https://hibd.cse.ohio-state.edu/ (accessed 2026-11-04)
- X1 CCCL, CXL pooling with H100, arXiv 2602.22457 — https://arxiv.org/html/2602.22457v1 (accessed 2026-11-04)
- X2 Wang et al., real-world CXL, arXiv 2405.14209 — https://arxiv.org/html/2405.14209 (accessed 2026-11-04)
- X3 AnandTech, Samsung CMM-B — https://www.anandtech.com/show/21333 (accessed 2026-11-04)
- X4 Samsung CMM-B white paper — https://download.semiconductor.samsung.com/resources/white-paper/CMM-B_whitepaper-V2.pdf (accessed 2026-11-04)
- X5 Astera Labs Leo on Azure M-series, 2025-11-18 — https://www.asteralabs.com/news/astera-labs-leo-cxl-smart-memory-controllers-on-microsoft-azure-m-series-virtual-machines-overcome-the-memory-wall/ (accessed 2026-11-04)
- X6 Astera Labs Leo — https://www.asteralabs.com/products/leo-cxl-smart-memory-controllers (accessed 2026-11-04)
- X7 Liqid EX-5410C spec; Blocks & Files 2025-07-17 — https://cdi.liqid.com/hubfs/Liqid-CXL%20EX5410C_071425.pdf , https://www.blocksandfiles.com/ai-ml/2025/07/17/liqid-unveils-composable-gpu-servers-with-cxl-20-memory-pooling/100989 (accessed 2026-11-04)
- X8 UnifabriX, Blocks & Files 2025-01-16 — https://blocksandfiles.com/2025/01/16/unifabrix-tears-down-the-ai-memory-wall/ (accessed 2026-11-04)
- X9 Panmnesia switch samples, 2026 — https://www.businesswire.com/news/home/20260415984752/en/Panmnesia-to-Mass-Produce-PCIe-6.4-CXL-3.2-Fusion-Fabric-Switch , https://www.storagenewsletter.com/2026/01/12/ces-2026-panmnesia-showcased-pcie-6-4-cxl-3-2-fabric-switch-and-pilot-system/ (accessed 2026-11-04)
- X10 Tom's Hardware, Panmnesia CXL-Opt — https://www.tomshardware.com/pc-components/gpus/gpus-get-a-boost-from-pcie-attached-memory-that-boosts-capacity-and-delivers-double-digit-nanosecond-latency-ssds-can-also-be-used-to-expand-gpu-memory-capacity-via-panmnesias-cxl-ip (accessed 2026-11-04)
- G1 KTH GH200 study, arXiv 2407.07850 — https://arxiv.org/html/2407.07850v1 (accessed 2026-11-04)
- G2 NVIDIA GB200 NVL72 — https://www.nvidia.com/en-us/data-center/gb200-nvl72/ (accessed 2026-11-04)
- G3 NVIDIA DGX Spark — https://www.nvidia.com/en-us/products/workstations/dgx-spark/ (accessed 2026-11-04)
- G4 The Register, DGX Spark review — https://www.theregister.com/2025/10/14/dgx_spark_review/ (accessed 2026-11-04)
- G5 GH200 cloud prices, 2026-10 — https://www.amcompute.com/gpu-cloud-prices/gh200 ; Lambda config https://gpus.io/de/gpus/gh200 (accessed 2026-11-04)
- G6 CoreWeave pricing — https://www.coreweave.com/pricing (accessed 2026-11-04)
- G7 NVIDIA blog, GH200 for Apache Spark, 2024-08-20 — https://developer.nvidia.com/blog/nvidia-gh200-superchip-delivers-breakthrough-energy-efficiency-and-node-consolidation-for-apache-spark/ (accessed 2026-11-04)
- G8 NVIDIA blog, Presto on GB200 NVL72 — https://developer.nvidia.com/blog/running-low-latency-analytical-workloads-with-gpu-accelerated-presto-on-nvidia-gb200-nvl72/ (accessed 2026-11-04)
- G9 CUDA virtual memory management, fabric handles — https://docs.nvidia.com/cuda/archive/13.3.0/cuda-programming-guide/04-special-topics/virtual-memory-management.html (accessed 2026-11-04)
- S1 GDS benchmark report (L40S, ConnectX-7, VAST) — https://gds-report.pages.dev/ (accessed 2026-11-04)
- S2 VAST NFS features — https://kb.vastdata.com/documentation/docs/nfs-features-for-enhanced-performance (accessed 2026-11-04)
- S3 Blocks & Files, VAST and GDS, 2021 — https://blocksandfiles.com/2021/06/28/gpu-data-cramming-gds-storage-hitting-the-streets (accessed 2026-11-04)
- S4 WEKA with GDS at Microsoft Research — https://www.weka.io/company/weka-newsroom/press-releases/weka-announces-marked-results-with-nvidia-magnum-io-gpudirect-storage-in-microsoft-research-lab/ (accessed 2026-11-04)
- S5 DDN with DGX B200 — https://ddn.com/blog/ddns-ai-storage-optimized-for-nvidia-dgx-b200-platform-handling-ai-at-any-scale-with-nvidia-blackwell (accessed 2026-11-04)
- S6 DDN A3I with DGX A100 brief — https://c-3389-20140109-www-ddn-com.i.icims.com/wp-content/uploads/download-manager-files/1656328403wpdm_DDN-A3I-with-DGX-A100-Performance-Brief.pdf (accessed 2026-11-04)
- S7 NVIDIA GPUDirect Storage overview — https://docs.nvidia.com/gpudirect-storage/overview-guide/index.html (accessed 2026-11-04)
- S8 NVIDIA cuObject — https://docs.nvidia.com/gpudirect-storage/cuobject (accessed 2026-11-04)
- S9 IBM Storage Scale S3 over RDMA, 2026-06-16 — https://community.ibm.com/community/user/blogs/ted-hoover/2026/06/16/ibm-storage-scale-announces-support-for-s3-over-rd (accessed 2026-11-04)
- S10 WEKA NeuralMesh on AWS Marketplace — https://aws.amazon.com/marketplace/pp/prodview-2mfqnh6p4yurs (accessed 2026-11-04)
- S11 CoreWeave AI Object Storage benchmark, 2026-10-22 — https://coreweave.com/blog/benchmark-results-coreweave-ai-object-storage-delivers-2-gb-s-per-gpu-throughput-across-any-number-of-gpus (accessed 2026-11-04)
- S12 Nebius storage types — https://docs.nebius.com/compute/storage/types (accessed 2026-11-04)
- N1 Nebius VM types — https://docs.nebius.com/compute/virtual-machines/types (accessed 2026-11-04)
- N2 Nebius GPU clusters — https://docs.nebius.com/compute/clusters/gpu (accessed 2026-11-04)
- N3 Nebius prices — https://nebius.com/prices (accessed 2026-11-04)
- N4 AWS EFA — https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/efa.html (accessed 2026-11-04)
- N5 AWS EFA bandwidth by instance — https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/efa-acc-inst-types.html (accessed 2026-11-04)
- N6 AWS P5 instances — https://aws.amazon.com/ec2/instance-types/p5/ (accessed 2026-11-04)
- N7 p5 prices — https://aws-pricing.com/p5.48xlarge.html , https://aws-pricing.com/p5.4xlarge.html , https://aws.amazon.com/about-aws/whats-new/2025/06/pricing-usage-model-ec2-instances-nvidia-gpus (accessed 2026-11-04)
- N8 p6-b200 price — https://aws-pricing.com/p6-b200.48xlarge.html (accessed 2026-11-04)
- N9 Azure ND H100 v5 — https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/gpu-accelerated/ndh100v5-series (accessed 2026-11-04)
- N10 Azure ND H100 v5 price (third party) — https://pcr.cloud-mercato.com/providers/azure/flavors/standard_nd96isr_h100_v5/pricing (accessed 2026-11-04)
- N11 spark-rapids RAPIDS Shuffle Manager docs — https://docs.nvidia.com/spark-rapids/user-guide/latest/additional-functionality/rapids-shuffle.html (accessed 2026-11-04)
- N12 Mellanox/SparkRDMA (archived) — https://github.com/Mellanox/SparkRDMA (accessed 2026-11-04)
- N13 NVIDIA/sparkucx — https://github.com/NVIDIA/sparkucx (accessed 2026-11-04)
