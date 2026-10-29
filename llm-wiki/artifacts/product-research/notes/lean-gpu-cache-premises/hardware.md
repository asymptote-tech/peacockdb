# Hardware and published GPU-SQL numbers for single-host 4–8 GPU servers (~100 GB hot analytic SQL in a few hundred ms?)

All sources accessed 2026-10-29. "Vendor" = claim by the hardware/software maker; "independent" = third party. TPC-H SF100 ≈ 100 GB raw, so "~100 GB" maps onto SF100.

## Q1. Per-GPU memory capacity and bandwidth

### Takeaway
Every current datacenter GPU's HBM bandwidth is 3–8 TB/s, enough to scan its share of 100 GB in single-digit milliseconds. 8 GPUs hold 640 GB (8×H100) to 2.3 TB (8×B300) in HBM. That is 6–23× a 100 GB dataset, before any compression. The PCIe cards (L40S, RTX PRO 6000) have far less bandwidth and no NVLink.

### Cited Findings
- H100 SXM: 80 GB, 3.35 TB/s, NVLink 900 GB/s, PCIe Gen5 128 GB/s, up to 700 W. H100 NVL (PCIe): 94 GB, 3.9 TB/s, NVLink bridge 600 GB/s (vendor) — [NVIDIA H100](https://www.nvidia.com/en-us/data-center/h100/) (accessed 2026-10-29)
- H200 SXM and H200 NVL: 141 GB, 4.8 TB/s, NVLink 900 GB/s (the NVL uses a 2- or 4-way bridge), PCIe Gen5 128 GB/s. NVIDIA labels these specs "preliminary" (vendor) — [NVIDIA H200](https://www.nvidia.com/en-us/data-center/h200/) (accessed 2026-10-29)
- H200 SXM / B200 SXM / B300 SXM per GPU: 141 GB HBM3e at 4.80 TB/s, 180 GB HBM3e at up to 8 TB/s, and 288 GB HBM3e at up to 8 TB/s. Per 8-GPU node: 1.1 TB, 1.44 TB and 2.30 TB, with 38.4 TB/s, 64 TB/s and 64 TB/s aggregate (vendor) — [NVIDIA HGX AI Factory reference architecture, components](https://docs.nvidia.com/enterprise-reference-architectures/hgx-ai-factory/latest/components.html) (accessed 2026-10-29)
- Conflicting figures:
  - NVIDIA's HGX product page lists HGX B300 at "2.1 TB" and HGX B200 at "1.4 TB" total GPU memory — [NVIDIA HGX](https://www.nvidia.com/en-us/data-center/hgx/) (accessed 2026-10-29)
  - A third-party page cites 7.7 TB/s per B300 and a "270 GB usable" figure, which I did not verify against NVIDIA — [OpenMetal HGX B300](https://openmetal.io/resources/hardware-details/nvidia-hgx-b300-8-gpu-server/) (accessed 2026-10-29)
  - The IBM/NVIDIA Presto paper says "B200 GPUs (192 GB each)", against NVIDIA's 180 GB — [arXiv 2606.24647](https://arxiv.org/pdf/2606.24647) (accessed 2026-10-29)
- GB200 Superchip (1 Grace + 2 Blackwell): 372 GB HBM3e at 16 TB/s, so about 186 GB and 8 TB/s per GPU. Grace has up to 480 GB LPDDR5X at up to 512 GB/s. NVLink is 3.6 TB/s per superchip and 1.8 TB/s GPU-to-GPU. The GB200 NVL72 rack has 13.4 TB HBM3e at 576 TB/s and 17 TB LPDDR5X at 14 TB/s (vendor) — [NVIDIA GB200 NVL72](https://www.nvidia.com/en-us/data-center/gb200-nvl72/) (accessed 2026-10-29)
- GB300 NVL72 (72 Blackwell Ultra GPUs, 36 Grace CPUs): 20 TB of GPU memory, up to 576 TB/s, 17 TB LPDDR5X at 14 TB/s, and 130 TB/s NVLink total. That works out to about 278 GB HBM and about 472 GB LPDDR5X per Grace (my division). NVIDIA gives no per-GPU figure on this page (vendor) — [NVIDIA GB300 NVL72](https://www.nvidia.com/en-us/data-center/gb300-nvl72/) (accessed 2026-10-29)
- GH200 as rented by Sirius: Hopper GPU with 96 GB HBM3 at 3 TB/s, plus a 72-core Grace with 480 GB LPDDR5X over NVLink-C2C (the paper's Table 1 says 96 GB, its text says 92 GB). Lambda price: $1.5/h in the paper, $3.2/h in another section — [Sirius, arXiv 2508.04701v3](https://arxiv.org/html/2508.04701v3) (accessed 2026-10-29)
- NVLink-C2C bandwidth: NVIDIA's NVLink-C2C page gives no GB/s figure, only "1 TB/s of memory bandwidth" for the Grace CPU Superchip — [NVIDIA NVLink-C2C](https://www.nvidia.com/en-us/data-center/nvlink-c2c/) (accessed 2026-10-29). A third-party page cites "900 GB/s CPU to GPU" — [search snippet, petronellatech HGX B300](https://petronellatech.com/nvidia/hgx-b300/) (accessed 2026-10-29; weak source).
- GQE blog (vendor): the B200 Decompression Engine gives "up to 400 GB/s effective host-to-device throughput" in database workloads. At a 4× compression ratio, about 100 GB/s of physical C2C host-to-device bandwidth stays in use — [NVIDIA blog: Designing GPU-accelerated query engines with GQE](https://developer.nvidia.com/blog/designing-gpu-accelerated-query-engines-with-nvidia-gqe/) (accessed 2026-10-29)
- RTX PRO 6000 Blackwell Server Edition: 96 GB GDDR7 with ECC, 1597 GB/s on a 512-bit bus, PCIe Gen5, up to 600 W. No NVLink is listed (vendor) — [NVIDIA RTX PRO 6000 Server](https://www.nvidia.com/en-us/data-center/rtx-pro-6000-blackwell-server-edition/) (accessed 2026-10-29)
- L40S: 48 GB GDDR6 ECC, 864 GB/s on a 384-bit bus, PCIe 4.0 x16, 350 W, no NVLink. These come from reseller listings, not an NVIDIA datasheet; one page wrongly says GDDR6X — [B&H L40S](https://www.bhphotovideo.com/c/product/1825422-REG/nvidia_nvl40stcgpu_kit_l40s_data_center_gpu.html), [evokernel L40S](https://evokernel-specs.pages.dev/en/hardware/l40s/) (accessed 2026-10-29)
- A100:
  - 40 GB at 1550 GB/s (the Sirius paper calls it HBM3, though the A100 uses HBM2/2e) — [Sirius arXiv 2508.04701v3](https://arxiv.org/html/2508.04701v3) (accessed 2026-10-29)
  - 8×A100 80 GB = "16TB/sec aggregate memory bandwidth per server", about 2 TB/s per GPU — [arXiv 2606.24647](https://arxiv.org/pdf/2606.24647) (accessed 2026-10-29)
  - I did not fetch the NVIDIA A100 datasheet.

### Inferences
- Per-GPU scan time for its share of 100 GB (100 GB / N GPUs / HBM bandwidth), assuming the theoretical ceiling:
  - 8×H100: 12.5 GB ÷ 3.35 TB/s ≈ 3.7 ms
  - 8×B200: ≈ 1.6 ms
  - 4×L40S: 25 GB ÷ 0.864 TB/s ≈ 29 ms
  - 4×RTX PRO 6000: 25 GB ÷ 1.6 TB/s ≈ 16 ms
- Even at 20–30% of peak, raw scan bandwidth is not the bottleneck for a few-hundred-ms budget. Operator overheads, joins, kernel launches and the exchange are.
- 4×L40S has 192 GB of VRAM. A 100 GB dataset fits only with about 2× compression, or with a big chunk of the working memory given over to caching. Sirius splits GPU memory half for the cache and half for processing.

### Gaps
- No NVIDIA primary datasheet fetched for the A100, L40S or GB300 per-GPU figures (288 GB HBM3e, C2C GB/s). The NVLink-C2C GB/s figure (900 GB/s is the commonly quoted number) was not confirmed on an NVIDIA page in this session.

## Q2. NVLink/NVSwitch, PCIe H2D, and P2P on PCIe-only boxes

### Takeaway
On HGX boards each GPU gets 900 GB/s NVLink (H100/H200) or 1.8 TB/s (B200/B300), against about 64 GB/s per direction for PCIe Gen5 x16. Measured P2P over PCIe on an 8×L40S-class box is only about 20 GB/s per pair. A PCIe-only box is therefore 40–90× worse for cross-GPU shuffle.

### Cited Findings
- DGX H100/H200: "4 x 4th generation NVLinks that provide 900 GB/s GPU-to-GPU bandwidth" (vendor) — [DGX H100/H200 user guide](https://docs.nvidia.com/dgx/dgxh100-user-guide/introduction-to-dgxh100.html) (accessed 2026-10-29)
- HGX B200 and HGX B300: NVLink GPU-to-GPU 1.8 TB/s per GPU, 14.4 TB/s total (vendor) — [NVIDIA HGX](https://www.nvidia.com/en-us/data-center/hgx/); [HGX AI Factory components](https://docs.nvidia.com/enterprise-reference-architectures/hgx-ai-factory/latest/components.html) (accessed 2026-10-29)
- PCIe Gen5 x16 is listed as "128GB/s", which is bidirectional, so about 64 GB/s per direction (vendor) — [NVIDIA H100](https://www.nvidia.com/en-us/data-center/h100/) (accessed 2026-10-29)
- Measured H2D with pinned memory and H100s: 56.2 GB/s (vendor server marketing; PCIe vs SXM form factor not stated) — [MiTAC/TYAN HX FT65TB8050](https://www.mitaccomputing.com/en/industryinsight/Redefining_Server_Performance_Introducing_TYAN_Transport_HX_FT65TB8050_equipped_with_NVIDIA_H100_GPU) (accessed 2026-10-29)
- A forum report of about 4.2 GB/s H2D on an H100 PCIe is almost certainly a misconfigured link; do not use it as typical — [NVIDIA forum](https://forums.developer.nvidia.com/t/pcie-bandwidth-is-asymmetrical-between-host-to-device-and-device-to-host/318613) (accessed 2026-10-29)
- PCIe P2P on an 8-GPU L40S box (independent forum post; the GPU model is attributed from the thread title):
  - P2P write is about 22 GB/s between GPUs 0–3 and about 19.7 GB/s for pairs involving GPUs 4–7.
  - With P2P disabled it is about 22 GB/s, with some pairs at about 12.8 GB/s.
  - L40S is PCIe Gen4, so 22 GB/s is about 70% of the Gen4 x16 ceiling (my inference).
  - [NVIDIA forum: L40S PCIe Gen4 x16](https://forums.developer.nvidia.com/t/linux-enabling-persistent-pcie-gen-4-0-and-pcie-buswidth-16-for-l40s-gpu/338641) (accessed 2026-10-29)
- Without NVLink, the Presto GPU exchange falls back to RDMA through the NICs. The 22 TPC-H queries at SF1000 on 8×A100 took 269 s, against 93 s with NVLink and 828 s with the CPU-staged HTTP exchange (independent: IBM, with NVIDIA co-authors; cold runs) — [Accelerating Presto with GPUs, arXiv 2606.24647](https://arxiv.org/pdf/2606.24647) (accessed 2026-10-29)

### Inferences
- With NVLink, an all-to-all shuffle of 10 GB per GPU takes about 6–11 ms at NVLink peak. Over PCIe P2P at about 20 GB/s it takes about 0.5 s. A design for a few-hundred-ms budget on PCIe-only boxes must avoid big shuffles: replicate dimension tables, co-partition fact tables, and do local pre-aggregation.

### Gaps
- No measured PCIe Gen6 H2D numbers found.
- No measured RTX PRO 6000 P2P numbers found.
- No independent bandwidthTest figures for B200 (Gen6-capable host) found.

## Q3. Host side of a typical HGX server

### Takeaway
DGX-class hosts have 2 Xeon sockets, 2 TB of RAM (DGX B200 configurable to 4 TB), 8×400 Gb/s ConnectX-7, and 8×3.84 TB U.2 NVMe. Host RAM alone could cache 20–40× a 100 GB dataset.

### Cited Findings
- DGX H100/H200 (vendor):
  - CPUs: 2× Xeon 8480C, 56 cores each.
  - Memory: 2 TB from 32 DIMMs.
  - NICs: 8× ConnectX-7 single-port InfiniBand on 4 OSFP ports, plus 2× dual-port ConnectX-7 Ethernet.
  - Storage: 2× 1.92 TB M.2 for the OS, 8× 3.84 TB U.2 NVMe in RAID 0 for the data cache (about 30.7 TB).
  - GPUs: 640 GB (H100) or 1,128 GB (H200) total.
  - [DGX H100/H200 user guide](https://docs.nvidia.com/dgx/dgxh100-user-guide/introduction-to-dgxh100.html) (accessed 2026-10-29)
- DGX B200 (vendor):
  - GPUs: 8 Blackwell GPUs, 1,440 GB, 64 TB/s HBM3e, 14.4 TB/s NVLink.
  - CPUs: 2× Xeon Platinum 8570, 112 cores total.
  - Memory: 2 TB, configurable to 4 TB.
  - NICs: 8× ConnectX-7 at up to 400 Gb/s, plus 2× BlueField-3 at 400 Gb/s.
  - Storage: 8× 3.84 TB NVMe U.2 plus 2× 1.9 TB M.2.
  - Power: about 14.3 kW.
  - [NVIDIA DGX B200](https://www.nvidia.com/en-us/data-center/dgx-b200/) (accessed 2026-10-29)
- HGX B300 reference architecture minimums (vendor):
  - CPU: at least 48 cores per socket.
  - Memory: "Minimum of 2TB system memory" and "Minimum of 500GB/s memory bandwidth".
  - NICs: 8× ConnectX-8 SuperNICs at up to 800 Gbps each.
  - [HGX AI Factory components](https://docs.nvidia.com/enterprise-reference-architectures/hgx-ai-factory/latest/components.html) (accessed 2026-10-29)
- AWS p5.48xlarge (8×H100): 192 vCPUs and 2048 GiB of memory — [Holori p5.48xlarge](https://calculator.holori.com/aws/ec2/p5.48xlarge/us-east-1) (accessed 2026-10-29)
- Theseus's on-prem nodes were Xeon 8380 machines with 4 TiB RAM, 8×A100-80GB and 200 Gb/s IB. The IBM Presto nodes had 256 cores, 2 TB RAM and 8×A100 80 GB — [Theseus arXiv 2508.05029](https://arxiv.org/html/2508.05029v1); [arXiv 2606.24647](https://arxiv.org/pdf/2606.24647) (accessed 2026-10-29)

### Inferences
- Theoretical host DDR5 bandwidth for a 2-socket Sapphire/Emerald Rapids host is about 600–650 GB/s (8 DDR5-4800/5600 channels per socket × 38–45 GB/s). This comes from my knowledge of the platform and was not sourced this session; NVIDIA's "≥500 GB/s" minimum is consistent with it.
- Host RAM to all 8 GPUs over PCIe Gen5 is about 8 × ~55 GB/s ≈ 440 GB/s if the root complexes do not contend. Re-loading 100 GB from host RAM therefore costs about 0.25 s at best. A cold load from 8 NVMe drives (Gen4 U.2 at about 6–7 GB/s each, about 50 GB/s total, not sourced) costs about 2 s. So "a few hundred ms" requires data already resident in HBM, or a host-RAM cache with heavy compression.

### Gaps
- DGX spec pages don't list DIMM speed or channel count.
- No sourced aggregate NVMe GB/s figure for DGX was found.

## Q4. Cloud prices per hour (2025–2026)

### Takeaway
8×H100 rents for about $32/h (Lambda) to $55/h (AWS on-demand). 8×B200 rents for about $54/h (Lambda) to about $114/h (AWS, per a secondary source). No sourced 4×L40S or 8×H200 on-demand price was obtained.

### Cited Findings
- Lambda on-demand, per GPU-hour:
  - 8×H100 SXM 80 GB: $3.99 (×8 = $31.92/h)
  - 8×B200 SXM6 180 GB: $6.69 ($53.52/h)
  - 8×A100 80 GB: $2.79 ($22.32/h)
  - 8×A100 40 GB: $1.99
  - 1×GH200 96 GB: $2.29
  - No H200, B300 or L40S listed.
  - [Lambda pricing](https://lambda.ai/pricing) (accessed 2026-10-29)
- AWS p5.48xlarge (8×H100) on-demand in us-east-1: $55.04/h, i.e. $6.88 per GPU-hour, per an aggregator snapshot dated 2026-10-15 — [Holori](https://calculator.holori.com/aws/ec2/p5.48xlarge/us-east-1) (accessed 2026-10-29)
- AWS p6-b200.48xlarge: about $14.24 per GPU-hour, about $114/h per node (a vendor-competitor blog; unverified) — [Spheron blog](https://www.spheron.network/blog/aws-vs-azure-vs-gcp-gpu-pricing-2026-h100-and-b200-compared/) (accessed 2026-10-29)
- AWS raised EC2 Capacity Block prices about 15% in January 2026 and about 20% from July 1, 2026. Reported rates include P6-B200 at $12.355 per accelerator-hour and p5en at $41.61/h (Jan 2026). These are secondary sources, and Capacity Blocks are reserved capacity, not on-demand — [Introl](https://introl.com/blog/aws-gpu-price-increase-h200-january-2026); [AI Weekly](https://aiweekly.co/alerts/aws-raises-ec2-gpu-capacity-block-prices-20-from-july-1) (accessed 2026-10-29)
- Instance types used in the papers:
  - The IBM Presto paper used AWS g7e instances (RTX PRO 6000, 96 GB, no NVLink): g7e.8xlarge (1 GPU), g7e.12xlarge, and g7e.48xlarge (8 GPUs). At SF1K the GPU run cost $0.65/h × 0.9 min, against a best CPU cost metric of 1.35 (2.3× advantage) — [arXiv 2606.24647](https://arxiv.org/pdf/2606.24647) (accessed 2026-10-29)
  - The Sirius README benchmarks against "best Sirius g7e size vs DuckDB on m9g.16xlarge" — [sirius-db/sirius](https://github.com/sirius-db/sirius) (accessed 2026-10-29)

### Gaps
- CoreWeave pricing page fetch failed (DNS error).
- Nebius was not fetched.
- No sourced 4×L40S, 8×H200 or B300 on-demand prices.

## Q5. Compression ratios in GPU analytics

### Takeaway
Published GPU SQL engines use 2–4× compression in practice. GQE assumes about 4× (vendor) with Cascaded/LZ4. Lineitem in the IBM GPU-friendly format is about 400 GB at SF1000, near raw size. No published per-column nvCOMP ratio on TPC-H lineitem was found.

### Cited Findings
- GQE (vendor):
  - Compression and partition pruning were enabled for both GQE and DuckDB.
  - Pruning skips 31% of data across the 22 queries, giving a 1.43× speedup.
  - Zone-map evaluation costs about 2.2 ms per query.
  - Cascaded compression runs at about 500 GB/s on a B200, and the DE gives up to 400 GB/s effective H2D at an assumed 4× ratio.
  - [NVIDIA GQE blog](https://developer.nvidia.com/blog/designing-gpu-accelerated-query-engines-with-nvidia-gqe/) (accessed 2026-10-29)
  - Secondary coverage says GQE picks Cascaded or LZ4 per column via nvCOMP — [Quantum Zeitgeist](https://quantumzeitgeist.com/nvidia-gqe-benchmark-speedup-tpc-h/) (accessed 2026-10-29)
- The nvCOMP README claims a maximum Cascaded ratio of 80× on analytical numeric data, an "up to" figure. Its lineitem SF10 benchmarks were run on a V100, and the per-column ratios were not extracted — [nvcomp mirror](https://github.com/nv-dlasalle/nvcomp) (accessed 2026-10-29)
- A GPU SQL-on-compressed-data paper reports ratios from 30× (20M runs) down to 1.87× (320M runs) on synthetic RLE layouts built from lineitem — [arXiv 2506.10092](https://arxiv.org/pdf/2506.10092) (accessed 2026-10-29)
- IBM handcrafted format: lineitem is "roughly 400 GB" at SF1000 and 3.5 TB at SF10,000. Parquet reads were "10× slower" than the GPU-friendly format, which reached 95% of the theoretical I/O rate (22 GB/s) — [arXiv 2606.24647](https://arxiv.org/pdf/2606.24647) (accessed 2026-10-29)
- Sirius published evaluation: no compression used; lightweight compression is future work — [Sirius arXiv 2508.04701v3](https://arxiv.org/html/2508.04701v3). The current README mentions `tpch_sf1000` compression plans but no numbers — [sirius-db/sirius](https://github.com/sirius-db/sirius) (accessed 2026-10-29)
- Theseus inputs are Parquet with Zstd. Network compression cut runtime 18% over TCP; with GPUDirect RDMA, turning it off was 19% faster — [Theseus arXiv 2508.05029](https://arxiv.org/html/2508.05029v1) (accessed 2026-10-29)

### Inferences
- A 2–4× in-memory ratio makes 100 GB raw into 25–50 GB resident. That fits comfortably on 4×L40S (192 GB) or even on one B200/B300.

### Gaps
- No sourced HeavyDB dictionary-encoding ratios.
- No GQE in-memory table size at SF1000.
- No "Simpatico" Sirius compression figures found.

## Q6. Published hot GPU SQL results (TPC-H SF100–SF1000, ClickBench), including multi-GPU single node

### Takeaway
The best published hot numbers put SF1000 (22 queries) at 9.0 s on one B200 (GQE, vendor), which averages about 0.41 s per query. SF1000 on a single 8×B200 DGX takes 19.9 s with Presto-GPU. At SF100, which is about the 100 GB target, Sirius on one GH200 is hot-run competitive with ClickHouse/DuckDB at 8.3× better cost efficiency. Scaling SF1000 results down by 10×, about 100 GB on 4–8 modern GPUs, hot, should land around 1–2 s for all 22 queries, i.e. tens to low hundreds of ms per query. That supports the premise, but it is my extrapolation, not a measurement.

### Cited Findings
- **NVIDIA GQE (vendor):**
  - Result: TPC-H SF1000, all 22 queries in 9.0 s total (the 9.15 s figure in the brief was not found).
  - Hardware: one of two B200 GPUs in a GB200 NVL4, using NVLink-C2C to Grace.
  - Method: each query averaged over 5 **hot-cache** runs; GQE parameters tuned per query.
  - Baseline: DuckDB 1.4.1 on an AMD EPYC 9755 took 74.0 s on one socket and 70.6 s on two.
  - Speedup: 7.5× aggregate (note 70.6/9.0 ≈ 7.8×), up to 25.5× on one query, 20 of 22 queries faster.
  - Where the data lived (GPU HBM vs Grace memory) during the hot runs is not stated.
  - [NVIDIA GQE blog](https://developer.nvidia.com/blog/designing-gpu-accelerated-query-engines-with-nvidia-gqe/) (accessed 2026-10-29)
  - The product page says GQE can query Parquet on disk or data in CPU/GPU memory; hardware decompression needs B100/B200/B300 — [build.nvidia.com GQE](https://build.nvidia.com/nvidia/gpu-query-engine) (accessed 2026-10-29)
- **Sirius (academic, UW-Madison, with NVIDIA involvement; CIDR 2026):**
  - TPC-H SF100, all 22 queries, **hot** (data cached in GPU memory, half the memory for cache).
  - On one GH200 it is 8.3× better than DuckDB at equal rental cost and 38× faster than ClickHouse. Per-query times appear only in Fig. 4 (not extracted).
  - Distributed: 4 nodes × 1 A100-40GB with 400 Gbps IB, SF100, Q1/Q3/Q6 only. Times are presumably ms; units are not printed.

    | Query | Sirius total | compute | exchange | other | Doris | ClickHouse |
    |---|---|---|---|---|---|---|
    | Q1 | 97 | 33 | 3 | 61 | 1193 | 393 |
    | Q3 | 341 | 43 | 233 | 75 | 838 | 12785 |
    | Q6 | 84 | 36 | 1 | 47 | 199 | 294 |

  - No multi-GPU single-node results, no GB300 numbers, and no compression in the evaluation.
  - [Sirius arXiv 2508.04701v3](https://arxiv.org/html/2508.04701v3); [CIDR 2026 PDF](https://www.vldb.org/cidrdb/papers/2026/p12-yogatama.pdf) (accessed 2026-10-29)
- **Sirius ClickBench (vendor blog, Dec 15 2025):**
  - Ran on one GH200 at Lambda ($1.5/h), against c6a.metal, c8g.metal-48xl and c7a.metal-48xl.
  - Claims the lowest geometric-mean relative hot runtime and at least 7.2× cost efficiency over the top CPU systems (the paper says 7.4×, and "11% faster than Umbra").
  - Q28 ran 13× faster with JIT. Q23, Q24/26 and Q27 are bottlenecked on string contains, top-N and large aggregation respectively.
  - No per-query ms in the text.
  - [NVIDIA blog: Sirius ClickBench record](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/) (accessed 2026-10-29)
  - Current ClickBench results exist only for the legacy implementation (`legacy-dev` @ f79a6f42) — [sirius-db/sirius](https://github.com/sirius-db/sirius) (accessed 2026-10-29)
- **Presto + Velox + cuDF (IBM with NVIDIA/HeavyAI co-authors; arXiv 2606.24647):**

  | Setup | SF | Run type | Time |
  |---|---|---|---|
  | 8×A100 80 GB, single node, one worker per GPU, UcxExchange over NVLink | 1000 | **cold** (no cache, GPFS 170 GB/s, mean of 10 runs) | 93 s (22 queries) |
  | Same, NVLink disabled (RDMA via NICs) | 1000 | cold | 269 s |
  | Same, HTTP exchange | 1000 | cold | 828 s |
  | 8×B200, single DGX | 1000 | not stated | 19.9 s |
  | 1×B200 | 1000 | not stated | 64.5 s (strong scaling ×3.2) |
  | 8×B200 | 10000 | not stated | 179.6 s |
  | 24×GB200, NVL72 | 10000 | not stated | 73.2 s |
  | 24–40 GB200 | 30000 | not stated | about 4.5 min |
  | 10×RTX PRO 6000 (AWS g7e.8xlarge, no NVLink) | 3000 | hot | 108 s |
  | Same | 3000 | cold, from S3 | 190 s (1.77× hot) |

  - Q9 runs more than 20× faster with the UCX exchange.
  - Handcrafted cuDF + NVSHMEM baseline at SF10,000 on 16×A100, cold via GDS: Q1 8.2 s, Q6 5.7 s, Q9 77.0 s.
  - Presto Q1 at SF10K: 6.3 s on 8×B200, 4.7 s on 12×GB200, 2.6 s on 24×GB200.
  - [arXiv 2606.24647](https://arxiv.org/pdf/2606.24647) (accessed 2026-10-29)
- **NVIDIA/IBM Velox blog (vendor):**
  - SF1000, 21 of 22 queries, single node, **hot**: Presto CPU on AMD 7965WX took 1,246 s; Presto-GPU took 133.8 s on an RTX PRO 6000 Blackwell and 99.9 s on a GH200 (148.9 s for all 22 with managed memory for Q21).
  - The UCX/NVLink exchange on a DGX A100 gives more than 6× over HTTP (cold, multi-node).
  - [NVIDIA blog: GPU-native Velox and cuDF](https://developer.nvidia.com/blog/accelerating-large-scale-data-analytics-with-gpu-native-velox-and-nvidia-cudf) (accessed 2026-10-29)
  - A related NVIDIA post on a single DGX B200 with 8 Presto GPU workers replaces decimals with floats ([search snippet](https://developer.nvidia.com/blog/?p=106959), accessed 2026-10-29). That makes it not strictly TPC-H.
- **Theseus (Voltron Data; arXiv 2508.05029):** all runs **cold** from Parquet/Zstd on WEKA or S3, with no hot runs and no single-node multi-GPU results.
  - On-prem 8×A100-80GB nodes, TPC-H totals:

    | SF | 2 nodes (16 GPUs) | 8 nodes |
    |---|---|---|
    | 10k | 230 s | 92 s |
    | 100k | 4403 s | 1015 s |

  - Cloud (g6.4xlarge, 1×L4 each), TPC-H SF1k: 222 s on 8 nodes and 103 s on 32. Photon took 270 s on 3 nodes and 134 s on 12.
  - Claims up to 4× (4.46×) better than Photon at cost parity.
  - [Theseus arXiv 2508.05029](https://arxiv.org/html/2508.05029v1) (accessed 2026-10-29)
- **Starburst Torin (Trino + cuDF), vendor, limited GA Oct 2026:** "up to 7x" on TPC-H on Blackwell. The blog gives a geometric mean of 4.6× over the accelerated queries, a maximum of 11.4× (Q13), and 18 queries tested. No absolute times extracted — [Starburst blog](https://www.starburst.io/blog/gpu-accelerated-sql-analytics-how-starburst-and-nvidia-deliver-industry-benchmark-speedups-on-gpu-infrastructure) (accessed 2026-10-29)
- Secondary mention, not verified: a TQP paper reports all 22 TPC-H queries at 1 TB in 0.53 s on 40 H100 GPUs — [arXiv 2506.09226 "Terabyte-Scale Analytics in the Blink of an Eye"](https://arxiv.org/html/2506.09226v2) (accessed 2026-10-29; only the search snippet was seen, so verify before citing)

### Inferences
- These are linear-scaling estimates. Real scaling is sublinear: IBM found 8× data on 8× workers took 2× longer.
  - GQE: 9.0 s for SF1000 on one B200 implies roughly 0.9 s for SF100.
  - Presto: 19.9 s for SF1000 on 8×B200 implies about 2 s for SF100, but Presto carries heavy framework overhead (stages, exchange).
- Sirius' distributed SF100 per-query numbers (84–341, likely ms) show that **exchange dominates join queries** even at 400 Gb/s (Q3: 233 of 341). On a single host with NVLink, that exchange share should fall by roughly 10× (inference).
- The "few hundred ms per query over 100 GB on one multi-GPU host" premise is consistent with published hot single-GPU results (GQE, Sirius). No published system has yet demonstrated it as a **single-node, multi-GPU, hot** configuration with per-query ms.

### Gaps
- No HeavyDB or Spark RAPIDS SF100–SF1000 hot numbers were collected.
- Per-query ms for GQE, Sirius TPC-H and Sirius ClickBench exist only in figures.
- No published multi-GPU Sirius results or GB300 Sirius numbers found as of 2026-10-29.

## Q7. Measured cross-GPU shuffle bandwidth for SQL

### Takeaway
No paper found reports a measured GB/s figure for a SQL shuffle. They report only end-to-end effects: NVLink exchange 3× faster than RDMA, and 9× faster than HTTP at SF1000.

### Cited Findings
- IBM Presto: UcxExchange over NVLink took 93 s, over RDMA via the NICs 269 s, and HTTP CPU-staged 828 s (8×A100, SF1000, cold). UcxExchange is up to 20× faster for exchange-heavy queries, and the Q5 speedup stays above 10× from SF1000 to SF10000. The handcrafted baseline used NVSHMEM broadcast for joins. No GB/s figure is reported — [arXiv 2606.24647](https://arxiv.org/pdf/2606.24647) (accessed 2026-10-29)
- Theseus: no cross-GPU bandwidth measured; only nominal 200 Gb/s IB and 25 Gbps cloud figures. GPUDirect RDMA cut total time from 691 s to 652 s, and dropping network compression with RDMA cut it to 528 s — [Theseus arXiv 2508.05029](https://arxiv.org/html/2508.05029v1) (accessed 2026-10-29)
- Sirius distributed exchange times are 1–233 (ms) per query on 4×A100 with 400 Gb/s IB; data volumes are not given — [Sirius arXiv 2508.04701v3](https://arxiv.org/html/2508.04701v3) (accessed 2026-10-29)

### Gaps
- No measured NVSHMEM or UCX GB/s figure for SQL shuffle on HGX was found. Raw p2p numbers exist only for PCIe L40S (about 20 GB/s, Q2).
