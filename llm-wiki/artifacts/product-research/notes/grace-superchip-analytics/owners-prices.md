# Grace superchip owners, prices, and analytics use (as of 2026-10-29)

Method note: the session's WebSearch budget was exhausted at the start of this task, so this sheet rests on direct WebFetch of known primary pages plus cited findings already gathered in this session's other notes and the repo wiki (llm-wiki/artifacts/product-research, llm-wiki/reports/dgx-spark.md). Azure, GCP, OCI, Together, Vultr pricing pages were not fetched directly; their figures come from the GetDeploying aggregator. All access dates 2026-10-29 unless stated.

## 1. Cloud availability and prices (GH200, GB200, GB300)

### Takeaway
A single GH200 (1 GPU + 72-core Grace + 480 GB LPDDR5X) is rentable on demand for $2.29–6.50/h, but the aggregator shows nearly every offer out of stock. GB200 is sold at a minimum of 4 GPUs (one 2-superchip tray/node) at about $10.5–27/GPU-h, or 42–108 $/node-h. AWS sells only 36- or 72-GPU UltraServers through Capacity Blocks. GB300 is the one Grace Blackwell part offered as a single GPU (Verda, $12.93/h).

### Cited Findings
- Lambda: GH200 1x instance at $2.29/GPU-h, on-demand ("self-serve, first-come access"). Lambda lists no GB200 or GB300. B200 costs $6.99/h (1x) to $6.69/h (8x), and B200 1-Click Clusters cost $8.87–9.86/GPU-h with a 2-week to 1-year commitment — [Lambda pricing](https://lambda.ai/pricing) (seen 2026-10-29)
- CoreWeave: GH200 instance with 1 GPU, 96 GB, 72 vCPU, 480 GB RAM at $6.50/h. GB200 NVL72 instance with 4 GPUs (186 GB each), 144 vCPU, 960 GB RAM at $42.00/h ($10.50/GPU-h). GB300 NVL72 instance with 4 GPUs (279 GB each) is "Contact sales". "Each instance is comprised of 2 GB200 or GB300 Superchips." — [CoreWeave pricing](https://www.coreweave.com/pricing) (seen 2026-10-29)
- AWS: P6e-GB200 comes "only available in UltraServers". The u-p6e-gb200x72 (72 GPUs) is $761.904/h and the u-p6e-gb200x36 is $380.952/h, both $10.582/GPU-h, as Capacity Blocks in the US East (Dallas) Local Zone. No P6e-GB300 price is published. For comparison, P6-B200 is $14.208/GPU-h and P5 H100 is $5.970/GPU-h — [AWS Capacity Blocks pricing](https://aws.amazon.com/ec2/capacityblocks/pricing/); [AWS P6 page](https://aws.amazon.com/ec2/instance-types/p6/) (seen 2026-10-29)
- Nebius: GB200 and GB300 NVL72 are "Contact us"; GH200 is not listed; HGX B200 is $8.50/GPU-h on demand. Reserved capacity is offered for "large-scale clusters for multiple months" — [Nebius prices](https://nebius.com/prices) (seen 2026-10-29)
- Verda (ex-DataCrunch): "1x GB300 SXM6 288GB" with 32 CPUs and 225 GB RAM costs $12.93/h on demand and $6.463/h spot. GB300 NVL72 is offered "1x tray to 2+ racks" with no price. GH200 and GB200 are not listed — [Verda pricing](https://verda.com/pricing) (seen 2026-10-29). Aggregator: Verda reserved from $9.69 — [GetDeploying GB300](https://getdeploying.com/gpus/nvidia-gb300)
- Hyperstack: no GH200, GB200 or GB300 prices (only nav links to GB200/GB300 NVL72). B200 is $6.00 on demand and $5.10 reserved; B300 is $7.40 on demand — [Hyperstack pricing](https://www.hyperstack.cloud/gpu-pricing) (seen 2026-10-29)
- GH200 aggregator (updated 2026-10-29). Every offer is a single GPU: Vultr $1.99 spot, Lambda $2.29, Sesterce $2.52/$4.65, Runcrate $2.98, Denvr $3.87, CoreWeave $6.50. "No provider is shown as in stock" — [GetDeploying GH200](https://getdeploying.com/gpus/nvidia-gh200)
- GB200 aggregator (prices checked 2026-08-31..10-29):
  - CoreWeave: 4 GPUs at $10.50 on demand.
  - AWS: 72 GPUs at $10.58 reserved.
  - Azure NDsrGB200NDRv6 (4 GPUs): $11.90 (36-month reservation), $17.31 (12-month), $27.04 on demand or spot.
  - Oracle BM.GPU.GB200.4: $16.00 on demand.
  - Together, Crusoe, Nebius, CUDO, Canopy Wave, Denvr: custom pricing or waitlist.
  - Source: [GetDeploying GB200](https://getdeploying.com/gpus/nvidia-gb200)
- GB300 aggregator:
  - Oracle BM.GPU.GB300.4: $18.00/GPU-h on demand.
  - GPUaaS: from $5.00 for a 72-GPU custom deal.
  - Verda: $6.46 spot to $12.93 on demand, in 1–4 GPU configs.
  - AWS, Together, Gcore, CoreWeave, Nebius: on request.
  - Source: [GetDeploying GB300](https://getdeploying.com/gpus/nvidia-gb300)
- GCP A4X (GB200): 1,600 Gbps RDMA plus 400 Gbps Titanium networking. Per-hour price not captured: the pricing page fetch was truncated — [GCP GPU network bandwidth docs, from earlier session notes](https://cloud.google.com/compute/docs/gpus/gpu-network-bandwidth)
- The Sirius ClickBench record used a Lambda GH200 at $1.50/h against CPU boxes at $7.3–9.8/h. Lambda's price today is $2.29/h, about 50% higher — [NVIDIA blog 2025-12-15](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/); [Lambda pricing](https://lambda.ai/pricing)

### Inferences
- The smallest Grace Blackwell rental unit is a 4-GPU, 2-Grace node: about $42/h at CoreWeave, $64/h at OCI and $108/h on Azure on demand (computed from the per-GPU prices above). An analytics engine targeting GB200 should treat the node, not the superchip, as the unit (estimate).
- For a cheap trial, GH200 is the only Grace part offered at about $2–3/h, but stock is thin. A GPU SQL engine's "try it" story on Grace depends on GH200 inventory at Lambda and Vultr (estimate).
- GB200 NVL72 capacity is mostly sold as reserved racks to AI training and inference tenants. An analytics buyer would rent it only by piggybacking on AI capacity (estimate).

### Gaps
- No primary-page prices were captured for Azure ND GB300 v6, GCP A4X / A4X Max, OCI, Together or Vultr (search budget exhausted; the GCP page was truncated). The aggregator figures should be verified.
- Real stock levels are unknown: the aggregator marks most GH200 offers out of stock.

## 2. National labs and supercomputers with Grace

### Takeaway
The large Grace fleets are GH200-based: Alps has 10,752 GH200, Isambard-AI 5,448, TACC Vista 600, and JUPITER and DeltaAI have GH200 too. None of the fetched centre pages or user guides mention Spark, RAPIDS, cuDF, databases or SQL analytics. Their documented uses are simulation, weather and ML.

### Cited Findings
- CSCS Alps has 2,688 Grace-Hopper nodes × 4 = 10,752 sockets. Each socket is 72 Arm cores, 128 GB LPDDR5X and an H100 with 96 GB HBM3. The named vClusters are MeteoSwiss weather, the User Lab, and ML/AI. No analytics, database, Spark or RAPIDS mention — [CSCS Alps](https://www.cscs.ch/computers/alps)
- Isambard-AI: phase 1 has 42 nodes / 168 GH200, phase 2 has 1,320 nodes / 5,280 GH200 (4 per node). Isambard 3 has 384 Grace CPU Superchip nodes. The docs warn: "Existing code compiled for x86_64 (including Conda environments) will not work." No analytics mention — [Isambard specs](https://docs.isambard.ac.uk/specs/)
- TACC Vista: "600 Grace Hopper (GH) nodes", each with one H200-class GPU (96 GB) and a 72-core Grace, plus 256 Grace-Grace nodes (144 cores). Its ML section covers PyTorch and Transformers. The first 100k characters of the guide do not mention Spark, RAPIDS, cuDF or databases — [TACC Vista guide](https://docs.tacc.utexas.edu/hpc/vista/)
- JUPITER is described as "Europe's first exascale supercomputer". The fetched page gives no GH200 count and no analytics mention. Access is 50% EuroHPC and 50% Gauss Centre — [FZ Jülich JUPITER](https://www.fz-juelich.de/en/jsc/jupiter)
- NCSA DeltaAI is "Powered by the NVIDIA GH200 Grace Hopper Superchip" and supports "simulation and data science". No Spark, RAPIDS or database mention — [DeltaAI docs](https://docs.ncsa.illinois.edu/systems/deltaai/en/latest/)
- Related: the cudf-spark issue #16088 says "Wild V23 proves a separate ParquetChunkedReader file-path GDS implementation can run on NVL72 GPFS". That implies NVIDIA tests Spark-RAPIDS internally on GB200 NVL72 with GPFS storage — [cudf-spark #16088](https://github.com/NVIDIA/cudf-spark/issues/16088)

### Inferences
- The HPC Grace fleet is large (more than 17k GH200 across Alps, Isambard-AI and Vista alone) but allocated through grants to science and ML users. It is not a market for a commercial SQL engine; at most it offers academic users and benchmarks (estimate).

### Gaps
- Venado (LANL), Horizon (TACC) and NERSC (Doudna is Vera Rubin, per memory, unverified) were not fetched. No HPC-centre publication on Spark, RAPIDS or database use on Grace was found; with search unavailable this is "not found", not "absent".

## 3. DGX Spark (GB10) and DGX Station (GB300)

### Takeaway
DGX Spark costs $3,999 (Founders Edition, 4 TB) and $3,000–4,000 from OEMs. Seven OEMs sell variants. NVIDIA markets "Data Science" with RAPIDS on it and ships a CUDA-X data-science playbook. Real analytics users exist but are few: one Spark+RAPIDS nightly batch, fraud-blueprint cuDF users, and complaints about ARM packaging, OOM hangs and slow Sirius results. DGX Station GB300 (252 GB HBM3e + 496 GB LPDDR5X) has no public price and is sold through 7 OEMs.

### Cited Findings
- Price: "at $3,000 to $4,000 (depending on config and OEM)". The Founders Edition "has a list price of $3,999 and comes with 4TB of storage". OEM versions "may have less storage and carry a lower price" — [The Register, 2025-10-14](https://www.theregister.com/2025/10/14/dgx_spark_review/). The current price after any 2026 increase was not verified.
- Specs: GB10, 128 GB unified LPDDR5X at 273 GB/s (64 GB only through OEMs), 20-core Arm, ConnectX-7 200 Gbps, 140 W TDP. The listed OEMs are Acer Veriton GN100, ASUS Ascent GX10, Dell Pro Max with NVIDIA AI Dev, Gigabyte AI TOP ATOM, HP ZGX Nano, Lenovo and MSI. The page has a "Data Science" workload ("High-performance data science at your desk") and a FAQ entry on "NVIDIA RAPIDS libraries" — [NVIDIA DGX Spark](https://www.nvidia.com/en-us/products/workstations/dgx-spark/)
- DGX Station GB300: 252 GB HBM3e at 7.1 TB/s and 496 GB LPDDR5X at 396 GB/s. Partners are ASUS, Dell, Exxact, Gigabyte, HP ZGX Fury, MSI and Supermicro. No price is listed, and the page still has a "notify when available" form. A Data Science section says the coherent memory lets "massive data lakes to be ingested directly into memory" — [NVIDIA DGX Station](https://www.nvidia.com/en-us/products/workstations/dgx-station/)
- Playbooks: 69 in total. Analytics-adjacent ones are "Accelerate Data Science with CUDA-X Libraries", "Run cuPyNumeric Across Two DGX Sparks", "Accelerate Single-Cell RNA Data Analysis" and "Accelerate Portfolio Optimization with cuOpt". None is SQL, Spark, Polars or DuckDB — [dgx-spark-playbooks](https://github.com/NVIDIA/dgx-spark-playbooks). The CUDA-X playbook: "Install and use NVIDIA cuML and NVIDIA cuDF to accelerate UMAP, HDBSCAN, pandas and more with zero code changes" — [build.nvidia.com](https://build.nvidia.com/playbooks/cuda-x-data-science)
- Owner analytics use (NVIDIA forums):
  - Two DGX Sparks in an Apache Spark 3.5.3 cluster with RAPIDS 25.02 run a nightly batch of about 380M rows. GPU phases got 1.5–2.4× slower after the kernel 7.0 update — [forum 386791](https://forums.developer.nvidia.com/t/kernel-7-0-0-1019-nvidia-makes-apache-spark-rapids-gpu-work-1-5-2-4x-slower-on-dgx-spark-6-17-0-1032-with-the-same-driver-580-178-04-is-fine/386791) (2026-10-25)
  - "RAPIDS cuDF 25.10 installed and working perfectly on ARM64" in a fraud-detection blueprint, but the official container is x86-only — [forum 352807](https://forums.developer.nvidia.com/t/nvidias-financial-fraud-training-container-is-only-built-for-x86-64/352807) (2025-11-27)
  - A user-built "Full-Featured CUDA-X Data Science Environment on NVIDIA DGX" — [forum 355714](https://forums.developer.nvidia.com/t/next-iteration-full-featured-cuda-x-data-science-environment-on-nvidia-dgx-with-heavyweight-ml-libraries/355714) (2025-12-25)
  - A 500K-node knowledge graph is stored as Parquet and loaded into GPU memory with cuGraph — [forum 365954](https://forums.developer.nvidia.com/t/neuralforge-gpu-native-knowledge-intelligence-platform-built-on-dgx-spark-gb10/365954) (2026-04-08)
  - A GPU SIEM uses Morpheus and Kafka, but search stays on CPU Elasticsearch — [forum 367141](https://forums.developer.nvidia.com/t/siem-on-dgx-spark/367141) (2026-04-20)
- Pain points:
  - An enterprise buyer: "cuDF is unusable on this machine… blocking GPU-accelerated dataframe work on a brand-new DGX system". A RAPIDS developer replied that 25.10 supports GB10 — [forum 352629](https://forums.developer.nvidia.com/t/rapids-cudf-preview-build-request-for-grace-blackwell-gb10-dgx-aarch64-kernels-missing/352629) (2025-11-25)
  - A buyer chose between DGX Spark and GX10 believing "RAPIDS won't run" — [forum 347717](https://forums.developer.nvidia.com/t/should-i-buy-asus-gx10-instead-nvidia-dgx-spark/347717)
  - A RAPIDS job OOMs and hangs the unified-memory box, with no ssh access — [forum 375016](https://forums.developer.nvidia.com/t/device-hangs-under-load/375016) (2026-07-06)
  - GDS is not supported on DGX Spark — [forum 350328](https://forums.developer.nvidia.com/t/gds-support-for-dgx-spark/350328) (per search snippet)
  - "DGX Spark (SM121) Software Support is Severely Lacking" — [forum 357663](https://forums.developer.nvidia.com/t/dgx-spark-sm121-software-support-is-severely-lacking-official-roadmap-needed/357663) (title only)
  - On GB10, "for all 22 TPC-H experiments, Sirius was slower than the CPU implementation on DuckDB" (SF10/100) — [sirius#609](https://github.com/sirius-db/sirius/issues/609) (2026-04-10, closed 2026-06-23)
  - A cuDF Parquet over-read bug was reported on GB10 with 26.08 nightly — cuDF issue #24000 (from session notes)

### Inferences
- DGX Spark owners are mostly LLM and agent hobbyists and developers: 60+ of 69 playbooks are AI. Analytics users are a visible but small minority, and their pain is packaging and stability on ARM plus a small memory, not a missing SQL engine (estimate).
- DGX Station GB300 (about 750 GB coherent memory) is the strongest single-box Grace target for TB-scale SQL. NVIDIA itself demos Sirius TPC-H SF1K in 21 s on it ([GTC notes](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html)), so a third-party engine competes with free NVIDIA-backed Sirius there (estimate).

### Gaps
- No unit-volume figures for DGX Spark or Station. No verified 2026 price for the Station (third-party estimates exist but were not fetched). No OEM-specific prices were captured.
- No Reddit or HN threads were captured in this pass (search unavailable).

## 4. Analytics vendors and NVIDIA projects targeting Grace

### Takeaway
The only Grace-specific analytics marketing comes from NVIDIA and its partners: Presto on GB200 NVL72 (with IBM), Sirius on GH200 and GB300, and Velox/Presto on GH200. HEAVY.AI announced a GH200 platform in January 2025. No evidence was found of Snowflake or Databricks offering Grace instances, or of Kinetica, SQream or Voltron marketing Grace specifically.

### Cited Findings
- NVIDIA blog "Running Low-Latency Analytical Workloads with GPU-Accelerated Presto on NVIDIA GB200 NVL72" (2026-07-08, Kimball et al., NVIDIA RAPIDS/cuDF):
  - Setup: 18 nodes × (2 Grace + 4 B200 + 4 CX-7). The run used 8 nodes / 32 GPU workers on TPC-H-derived SF10K and SF30K Parquet with floats instead of decimals, against IBM Storage Scale (10 PB, about 4.5 TiB/s).
  - Results: tuning cut runtime 64%, with GDS giving about 30% of that.
  - No customers are named. IBM watsonx.data offers a "technical preview". The Grace CPU's role is not discussed.
  - Source: [NVIDIA blog](https://developer.nvidia.com/blog/running-low-latency-analytical-workloads-with-gpu-accelerated-presto-on-nvidia-gb200-nvl72/)
- Velox+cuDF Presto, TPC-H SF1000: 99.9 s on one GH200, 133.8 s on an RTX PRO 6000, and 1,246 s on CPU Presto C++ — [NVIDIA blog 2025-10-06](https://developer.nvidia.com/blog/accelerating-large-scale-data-analytics-with-gpu-native-velox-and-nvidia-cudf)
- Sirius: #1 on ClickBench hot runs on GH200. The README claims about 5× DuckDB on TPC-H SF1000 on a GB300 DGX Station. It was announced as a core DuckDB extension at GTC 2026 — [sirius](https://github.com/sirius-db/sirius); [NVIDIA blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/); [GTC notes](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html)
- HEAVY.AI announced availability of its analytics platform on GH200 ("up to 21x faster than CPU competitors") — [BusinessWire 2025-01-21](https://www.businesswire.com/news/home/20250121115004/en/HEAVY.AI-Announces-Availability-of-Analytics-Platform-Accelerated-by-NVIDIA-Grace-Hopper-Superchip)
- At GTC 2026, Starburst, Kinetica and Redpanda were named as validating the Vera CPU (not Grace) — [GTC notes](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html)

### Inferences
- Grace-targeted analytics is mostly NVIDIA demo work. NVIDIA funds free engines (Sirius, Velox/Presto cuDF, cudf-spark) that already run on Grace, so a paid Grace-specific SQL engine would face a free NVIDIA-backed default and few owners whose main job is analytics (estimate).
- No public signal of willingness to pay for an analytics engine on Grace hardware was found (estimate, not proof of absence).

### Gaps
- Kinetica, SQream, Voltron/Theseus, Oracle, Snowflake and Databricks Grace status was not checked (search unavailable).
- No GitHub, HN or Reddit threads from GH200 or GB200 owners running SQL or dataframes were found beyond Sirius issue #609 and the Sirius team's own Lambda GH200 runs.
