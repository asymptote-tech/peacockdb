# grace-superchip-analytics — a GPU SQL engine built for Grace superchips (GH200, GB200/GB300, GB10)

## Verdict

- **What it is:** an engine that keeps hot columns in the Grace CPU's large LPDDR5X memory (480 GB per Grace) and has the GPU read them over NVLink-C2C (~375 GB/s measured on GH200), with HBM as the top tier. On GB10 there is one shared pool, so nothing is staged at all.
- **Who needs it:** demand.md rates it **thin** (row added after this file was written: no first-person poster). Nobody was found who asks for SQL on Grace. Grace machines are bought and rented for AI. The few analytics owners found run Spark or RAPIDS on DGX Spark and complain about packaging and stability [D][F1–F4].
- **Budget:** it fits easily: ~0.8× on peacockdb, ~0.3× on Sirius (estimate). **But it is a deployment target, not a product.** It makes the cache in **lean** and **gpu-bi-dashboards** cheaper and bigger. On DGX Spark it serves **workstation-gpu-sql**.

## Who needs it badly

- **Rating: none in demand.md.** `## Ratings` has no grace-superchip row [D]. The nearest rows are:
  - **workstation-gpu-sql: strong**, partly from DGX Spark owners. demand.md's reason: "it is for dataframe APIs and easy installs, not SQL" [D];
  - **lean: medium**: "users ask for it softly" [D].
- **Evidence added here (not a re-rating):**
  - **Jeff Levesque** (`jeff1evesque`) runs a nightly 380M-row Spark + RAPIDS batch on two DGX Sparks. A kernel update made the GPU phases 1.5–2.4× slower ([forum](https://forums.developer.nvidia.com/t/kernel-7-0-0-1019-nvidia-makes-apache-spark-rapids-gpu-work-1-5-2-4x-slower-on-dgx-spark-6-17-0-1032-with-the-same-driver-580-178-04-is-fine/386791), 2026-10) [F1]. This is the only Grace owner found running SQL-shaped analytics for real.
  - An enterprise GB10 buyer thought cuDF did not run on aarch64: "This is blocking GPU-accelerated dataframe work on a brand-new DGX system" ([forum 352629](https://forums.developer.nvidia.com/t/rapids-cudf-preview-build-request-for-grace-blackwell-gb10-dgx-aarch64-kernels-missing/352629), 2025-11) [F2].
  - A RAPIDS job ran out of memory on GB10 and hung the box with no ssh ([forum 375016](https://forums.developer.nvidia.com/t/device-hangs-under-load/375016), 2026-07) [F3].
  - Sirius ran slower than CPU DuckDB on all 22 TPC-H queries on GB10 ([sirius #609](https://github.com/sirius-db/sirius/issues/609), 2026-04) [F4].
- **National labs have the hardware, but publish no analytics use.** Alps (CSCS) has 10,752 GH200 [O1], Isambard-AI 5,448 [O2] and TACC Vista 600 GH200 nodes [O3]. No centre page mentions SQL, Spark or databases (search was cut short; gap).
- **The only Grace-specific analytics marketing is NVIDIA's and its partners'.** That covers Presto on GB200 NVL72 with IBM, Sirius, Velox/Presto on GH200, and HEAVY.AI's GH200 platform (2025-01) [N3][N4][O6]. No customer is named in any of them.
- **No willingness to pay was found.** No thread mentions a budget for an analytics engine on Grace (search was budget-limited; this is "not found", not "does not exist").

## How it would work

- **Tiers:** decoded columns live in Grace memory, pinned (`cudaMallocHost`) or system-allocated with 64K pages. They are NUMA-local to their GPU. Hot, small or join-build data is promoted to HBM. The Parquet files stay the source of truth.
- **Scan:** the GPU either streams batches from Grace memory with async copies (~375 GB/s) or reads them in place through coherent access (ATS). No pageable bounce buffer is used.
- **Join:** the build side stays in HBM and the probe side streams from Grace memory. A build that does not fit is partitioned, Grace-style.
- **Aggregate:** streamed in ~8M-row batches, which also avoids the cuDF 26.02 groupby cliff (dgx-spark.md §6) [R].
- **GB10 variant:** one 128 GB pool. Read in place, never copy, never pin (§1) [R]. Keep CPU work off the memory bus while the GPU scans (§11) [R].
- **End-to-end data path:** Parquet on NVMe or a network store → chunked cuDF reader → decoded columns in Grace memory (fill once) → C2C → GPU kernels → Arrow out. Every query after the first starts at the C2C step.

## The human's questions

**1. What the link really delivers for scans, joins and aggregations.**
- **GH200, copies:** H2D 375 GB/s and D2H 297 GB/s, against a 450 GB/s per-direction spec (KTH) [P1]. nvbandwidth gives ~377–380 GB/s [P4].
- **GH200, GPU kernels reading Grace memory:**
  - 350 GB/s (78% of spec) [P2]. Alps reports ~93% of 450, about 418 GB/s (derived from a percentage) [P3].
  - malloc'd memory with 4K pages, read in place: 219 GiB/s. The same memory migrated to HBM with 64K pages reads at 2,112 GiB/s; `cudaMalloc` memory at 2,390 [P5].
  - **Pathological case:** 0.22 GiB/s after cache eviction [P5].
- **Against HBM:** GH200 HBM measures 3.4 TB/s [P1], so Grace memory is ~8–10× slower than HBM. Against PCIe it is ~8× faster: the H200's pinned PCIe H2D is 47.3 GB/s [I].
- **Grace LPDDR5X itself:**
  - measured 486 GB/s (KTH) or 362–370 GB/s (HPI) [P1][P2];
  - the spec is 500–546 GB/s [P6].
  - **CPU and GPU share it.**
- **GB300:** Sirius measures ~350–380 GB/s host → GPU "regardless of stream count" (`docs/super-sirius/configuration.md:1007-1050` @ b9e05ffc) [S1].
- **GB200:** no measured number was found. One Grace (≤512 GB/s, 480 GB) feeds two Blackwells [O4]. So if both GPUs stream at once, each gets ≤~250 GB/s (estimate).
- **GB10 (peacockdb's own measurements):** `cudaMemcpy` is capped at ~59 GB/s, while a kernel reads mapped host memory at 205–219 GB/s against 233–239 for device memory (dgx-spark.md §1) [R]. Staging costs ~4× what reading in place costs.
- **Coherent access vs copies vs migration:**
  - Access-counter migration exists only on coherent systems, from CUDA 12.4 [P7]. It kicks in after 256 accesses and pays off only after ~5 passes [P1].
  - mmap'd files never migrate [P7].
  - `cudaMemcpy` from malloc'd memory takes a slow host-driven path, so it needs `cudaMallocHost` [P3].
  - 64K pages allocate 15.9× faster, but compute ran 1.1–2.1× faster with 4K pages in KTH's apps [P1].
  - GPU TLB misses on small pages make random access costly [P7].
- **By operator (estimate from the above):**
  - **Scans and aggregations** stream, so they get the full ~375 GB/s. A 100 GB scan costs ~0.27 s from Grace memory against ~30 ms from HBM.
  - **Hash-join probes into Grace memory** are random access: TLB misses, plus ~800 ns latency in the reverse direction (CPU → HBM) [P2][P8]. No measured GPU random-gather rate into Grace memory was found (gap). Builds must stay in HBM.
- **The one end-to-end measurement:** Sirius on GB300, TPC-H SF1000 hot. With everything pinned in Grace memory: 20.8 s, or 18.1 s tuned. With everything pinned in HBM: 7.0–7.4 s [S1]. **The Grace tier costs ~2.6× against HBM, but it holds data that HBM cannot.**

**2. What an engine must do differently.**
- **Never stage through pageable buffers.** Use pinned or registered Grace memory, or read in place. Branch on `pageableMemoryAccess`: on GB10 pinning hurts, falling from 59.4 to 48.3 GB/s at 32 GiB (§1) [R].
- **Keep a CPU-resident decoded cache and read it in place.** Store it uncompressed: Sirius found host-tier compression 7% *slower* over C2C, while compressing data held in HBM was 19.7% faster (`compressed-pinning.md:46-76`) [S1].
- **Make memory NUMA-aware.** On quad-GH200 nodes, Grace memory is NUMA nodes 0–3 and placement is first-touch. Cross-superchip traffic reaches ≤60% of theoretical [P3].
  - GQE pools per host-NUMA node and caps them at 85%, because "90% typically fails on Grace with 480 GB" (`src/task_manager_context.cpp:97-98` @ 50facc46) [S3].
- **Share work between CPU and GPU only with care:**
  - On GB10, CPU memcpy traffic takes up to 37% of the GPU's bandwidth (§11) [R].
  - On GH200 the CPU reads HBM at only ~130 GB/s with ~807 ns latency [P2].
  - So the CPU should work on Grace-resident data, not on GPU-resident data. Over-scheduling the CPU slows the GPU.
- **Use a pool allocator and the chunked reader.** Pooling is worth 2.5× and chunked reading 5.6× on GB10 (§2, §3) [R]. peacockdb does neither today (#148, #232) [I].
- **Use managed memory for recovery only.** It oversubscribes to 120% on GB10, but first touch runs at ~3 GB/s (§13) [R]. It stops migrating at ~130% oversubscription on GH200 [P1].

**3. Who has these machines, and would they pay.**
- **GH200, one GPU, on demand:**
  - Lambda $2.29/h [O5];
  - CoreWeave $6.50/h (1 GPU, 72 cores, 480 GB) [O4];
  - Vultr $1.99 spot, Sesterce $2.52, Denvr $3.87. The aggregator showed every offer out of stock [O7].
- **GB200: the smallest unit is a 4-GPU node with 2 superchips:**
  - CoreWeave $42/node-h ($10.50/GPU-h) [O4];
  - OCI $16/GPU-h;
  - Azure $27.04/GPU-h on demand, $11.90 on a 3-year reservation [O8];
  - AWS P6e-GB200 only as 36- or 72-GPU UltraServer Capacity Blocks, at $10.58/GPU-h [O9].
- **GB300:** Verda sells one GPU at $12.93/h ($6.46 spot) [O10]. CoreWeave, Nebius and AWS are contact-sales.
- **Desk:**
  - DGX Spark is $3,999, with OEM versions at $3,000–4,000 from seven makers [O11][O12]. No unit sales are published.
  - NVIDIA's 69 DGX Spark playbooks include cuDF-for-pandas but nothing for SQL, Spark, Polars or DuckDB [O13].
  - DGX Station GB300 (252 GB HBM + 496 GB LPDDR5X) has no public price [O14].
- **Price against a CPU box:**
  - A GH200 at $2.29/h costs 1.7–5× a 64-core CPU box ($0.45–1.35/h, evidence-in-hand) [E].
  - Sirius's 8.3× cost edge over DuckDB assumed GH200 at $1.50/h [S2]. At today's Lambda price that falls to ~5.4× (estimate, linear), and only on hot data.
- **Would they pay?** No evidence. The people who own the big machines run AI and have NVIDIA's free engines. Desk owners hit packaging pain and use free RAPIDS [F2][O13].

**4. What Sirius, cudf-polars, HeavyDB and NVIDIA already offer on Grace.**
- **Sirius:**
  - TPC-H SF1000 in 8.18 s on GB300 (2026-08-01, best-of-3). It needs patched libcudf from a fork, on a branch "not proposed for merge" (`bench/sf1000-repro/README.md:1-7` @ b9e05ffc) [S1].
  - Mainline gets 6.92–7.39 s with data in HBM and 18.1–20.8 s with data in Grace memory [S1].
  - Its host tier is ordinary `cudaMallocHost`, one pool per NUMA node via cuCascade. **There is no ATS or in-place path**, and no doc says it targets Grace [S1].
  - The CIDR paper on GH200 reports 8.3× (TPC-H SF100) and 7.4× (ClickBench) cost efficiency against DuckDB, from the legacy engine [S2].
- **cudf-polars and RAPIDS:**
  - cudf-polars defaults to a managed pool with prefetch (`python/cudf_polars/cudf_polars/callback.py` @ 785ee522) [S4].
  - RMM's `system_memory_resource` names "ATS … for the Grace Hopper Superchip" (`cpp/include/rmm/mr/system_memory_resource.hpp:39-55` @ 006f950a) [S5].
  - No published GH200 claim for Polars was found.
- **HeavyDB:**
  - HEAVY.AI announced its platform on GH200 in January 2025, "up to 21x faster than CPU competitors" [O6].
  - NVIDIA reports one GH200 at 5× the speed of 8×A100 and 16× cheaper at SF100, with data offloaded to Grace memory (vendor claim) [N1].
  - The open-source README lists only Ubuntu 22.04 and Rocky 8, not aarch64 [S6].
- **NVIDIA:**
  - Spark RAPIDS on 16 GH200 ran NDS 10 TB in 6 min, against 42 min on 16 x86 nodes [N1]. The GH200 page claims "up to 36X" [N2].
  - Presto (Velox + cuDF) ran TPC-H SF1000 in 99.9 s on one GH200, against 1,246 s on CPU Presto [N3].
  - Presto ran on 8 GB200 NVL72 nodes (32 GPUs) at SF10K/30K, with no customer named [N4].
  - **GQE is the most Grace-native engine in existence** [S3]:
    - ATS pageable memory;
    - host-NUMA pools sized for 480 GB;
    - the Blackwell hardware Decompression Engine ("only observed on GB200 so far", `include/gqe/utility/cupti_activity.hpp:62`).

**5. Product on its own, or a deployment target?**
- **A deployment target.** Every Grace-specific technique here is an engine setting, a memory tier or an allocator branch, and nobody buys those alone.
- **It strengthens lean most.** lean's weakness is VRAM capacity. A Grace tier gives 480 GB per GPU at 375 GB/s, and Sirius shows the cost of that tier: ~2.6× against HBM [S1].
- **Next, gpu-bi-dashboards:** dashboard hot sets of 100–400 GB fit in Grace memory at ~0.3–1 s per full scan (estimate).
- **DGX Spark belongs to workstation-gpu-sql.** There the edge is "one pool, no copies, no OOM hang", not bandwidth.

## Data path and cache

| Stage | Bandwidth | Source |
|---|--:|---|
| NIC, 400 Gb (GB200 tray CX-7) | 50 GB/s line rate (estimate) | [N4] |
| NVMe Gen4 / Gen5, one drive | ~7 / ~14 GB/s (estimate) | — |
| Parquet decode, engine on H200 | 6.8–8.7 GB/s | [I] |
| cuDF read, GB10, Snappy / uncompressed | 5.0 / 7.4 GB/s | §10 [R] |
| Chunked reader, GB10, q6 from page cache | ~25 GB/s (1.5 GB in 61 ms) | §2 [R] |
| PCIe Gen5 H2D, H200, pageable / pinned | 10.6 / 47.3 GB/s | [I] |
| C2C, GPU reads Grace memory (GH200 / GB300) | 350–418 / 350–380 GB/s | [P1–P3][S1] |
| GB10 unified, kernel read in place | 205–219 GB/s | §1 [R] |
| HBM, GH200 measured / H200 spec | 3.4 / 4.8 TB/s | [P1][I] |

- **The thesis holds, and Grace sharpens it.**
  - Cold, the engine is decode- or IO-bound at 5–25 GB/s. That is 15–75× below C2C and 140–700× below HBM.
  - Grace does nothing for the cold path. Its value is that **the RAM cache moves within ~10× of HBM**, against ~70–100× over PCIe.
  - So a decoded Grace-memory cache is mandatory, not optional. Without it, a GB200 is a $10/GPU-h Parquet decoder.
- **What cache to build:**
  - decoded columns, uncompressed, in NUMA-local Grace memory;
  - HBM for join builds and the hottest columns, lightly compressed;
  - fill through the chunked reader.
- **Capacity:**
  - GH200: 96 GB HBM + 480 GB Grace;
  - GB200: 2 × 186 GB HBM sharing 480 GB [O4];
  - GB10: 128 GB in all;
  - DGX Station GB300: 252 + 496 GB [O14].
- **The GB200 caveat:** two GPUs share one Grace's ~500 GB/s, so per-GPU streaming halves (estimate). With NVL72, cross-tray traffic goes over NVLink, not C2C. That is a different design problem.

## Build cost to alpha

Sizes are in yardstick lines; the unit is 22,830 [I]. The alpha targets one GH200, or one GB200 superchip (2 GPUs), plus GB10. Every figure is an estimate.

| Component | On peacockdb: reuse/adapt/new, size | On Sirius: reuse/adapt/new, size |
|---|---|---|
| Grace-memory column cache (fill, NUMA placement, LRU, invalidation) | new: 2–4k | adapt `pin_table` tier='host' into an automatic cache: 1–2k [S1] |
| Read in place vs copy (ATS path, `pageableMemoryAccess` branch, 64K pages) | adapt scan and GpuBatch: 0.5–1k | adapt: Sirius has no ATS path today: 0.5–1k [S1] |
| HBM-build / Grace-probe join; partitioned build and aggregate when over HBM | new: 3–5k (no spill today [I]) | reuse (Grace-style partitioned operators) [SM] |
| Allocator pools, per-NUMA, 85% cap | adapt: 0.5k (#148) | reuse (cuCascade) |
| Cold load: chunked reader, multi-file tables | adapt: 0.5–1k (#232) | reuse |
| CPU/GPU placement aware of bus contention | new: 1–3k | new: 1–3k |
| Two GPUs per Grace (GB200), worker per GPU | new: 3–6k (test-binary code exists [I]) | adapt: 1–2k (single-node multi-GPU exists [SM]) |
| Front door (CLI/server) and aarch64 packaging | new: 1–2k | reuse the DuckDB extension (arm64 supported); 0.3k |
| Tests on Grace hosts | adapt the corpus harness: ~1k | adapt: 0.5k |
| **Total** | **~12.5–23.5k** | **~4.3–8.8k** |

Total new work: 0.8× engine on peacockdb, 0.3× on Sirius (estimate)

- **The hard part is the join.** Probes into Grace memory are random access, and that is unmeasured (gap). If a GPU gathering from Grace memory is slow, every large build must be partitioned to fit HBM. That is the 3–5k spilling work peacockdb lacks entirely.
- **On GB10, the hard part is CPU contention** (§11). The placement rule needs measuring, not guessing.
- **On Sirius, the hard part is upstream churn.** Sirius already has a NUMA host tier and GB300 numbers [S1]. The marginal work is small, so NVIDIA can do it first.

## Adoption friction

- **Rare hardware.** The buyer must already own or rent a Grace machine. Rentals: GH200 from ~$2.29/h, often out of stock [O5][O7]; GB200 only as 4-GPU nodes or racks [O4][O9].
- **A new system to adopt** on peacockdb, unless shipped as a plugin into lean's or gpu-bi-dashboards' host. Sirius ships as a DuckDB extension on arm64, so the friction there is low [S1].
- **Desk:** DGX Spark owners struggle with aarch64 packaging and container gaps [F2][O13]. A single small binary that runs would answer that pain, but it is the workstation-gpu-sql pitch.

## Platform risk

- **The highest of any option (estimate).** NVIDIA sells the hardware, writes the drivers, and already has three Grace analytics stories:
  - GQE, built for ATS, host-NUMA pools and the Decompression Engine [S3];
  - Sirius on GB300 [S1];
  - Presto on GB200 NVL72 with IBM [N4].
- nvidia.md: the risk is "commoditization by free, NVIDIA-staffed engines" [NV]. GQE is "DataFusion + libcudf", peacockdb's own design, and it is already Grace-tuned [NV][S3].
- **Grace tuning is know-how, not a moat.** Each technique is a documented CUDA feature (ATS, `HOST_NUMA` pools, memory advice) [P7][P9]. NVIDIA's tuning guides publish it [P6].
- **Drivers and kernels shift performance under you.** A kernel update cost 1.5–2.4× on DGX Spark [F1].

## Funding story

- **What a design partner would say (estimate):** "We have GB200s for training. If they idle at night, could they run our SQL?" That is a utilisation story. The buyer is an AI-infrastructure team, and they already get Spark RAPIDS with NVIDIA AI Enterprise [NV].
- **The round could be raised on** the "GPU memory wall is solved by Grace" narrative: 480 GB per GPU at 375 GB/s, so terabyte hot sets on one tray. But that is NVIDIA's own marketing line [N2]. No named user or customer backs it, and investors would ask why NVIDIA's free GQE or Sirius does not win.
- **It adds to another option's story:** "lean, and on Grace it scales to 480 GB per GPU". It does not carry a story alone.

## Evidence against

- **No demand.** No demand.md row exists, and no first-person ask for SQL on Grace was found. The only analytics user found runs Spark RAPIDS on two DGX Sparks [D][F1].
- **The machines are AI machines.** GB200 rents only as 4-GPU nodes or racks, at $10–27/GPU-h [O4][O8][O9]. An analytics buyer would rarely pick it over a $0.45–1.35/h CPU box [E].
- **The Grace tier is 2.6× slower than HBM** on Sirius's SF1000 [S1]. Below ~150 GB of hot data an H200 holds it all in HBM. Above that, a 64-core box also has the RAM, and evidence-in-hand shows Voltron at only ~1.3× a CPU box once scaled (estimate) [E].
- **GB10 is a different, weaker machine.** It has 239 GB/s total and is 8× slower than an H200 at dense FP. Sirius lost every TPC-H query to CPU DuckDB there [R][F4].
- **NVIDIA got there first.** GQE has the Grace-specific code [S3], Sirius has the published Grace numbers [S1], and Presto has the GB200 NVL72 showcase [N4].
- **Unmeasured core risk:** random hash probes into Grace memory (gap). If they are slow, the "join over 480 GB" story collapses into partitioning, which every engine already does.

## Ratings

- **Who needs it badly: thin.** demand.md rates it thin (0 first-person posters). The nearest rows are workstation-gpu-sql (strong, but dataframes and installs) and lean (medium). Here, only 4 DGX Spark posts and vendor marketing were found [D][F1–F4].
- **Build cost: strong.** ~0.8× on peacockdb and ~0.3× on Sirius, well inside the budget.
- **Data path: strong.** C2C is the best host link that exists (~375 GB/s, 8× PCIe). The cold path is still decode-bound, so a decoded cache is mandatory.
- **Adoption friction: medium.** It needs rare, AI-allocated hardware. As a DuckDB extension (Sirius) or a plugin it is easy, but as a new peacockdb system it is not.
- **Platform risk: weak.** NVIDIA's GQE and Sirius already target Grace, and NVIDIA controls the hardware, drivers and showcase.
- **Funding story: weak.** NVIDIA's own narrative, no named user, and no buyer whose main job is analytics on Grace.

## Follow-up experiments

1. **Random gather into Grace memory.** Hash-probe throughput on GH200 (~$2.29/h on Lambda): build in HBM against build in Grace memory (pinned vs ATS malloc, 4K vs 64K pages). This settles the join story.
2. **Read in place vs async copy for peacockdb's scan on GH200**, sweeping batch size. Does in-place access match the ~375 GB/s copy rate for streaming?
3. **Two GPUs on one Grace (GB200).** Per-GPU C2C rate when both stream at once. This tests the ≤250 GB/s estimate.
4. **Sirius host pin vs GPU pin on GH200 at SF300–1000,** set against the cost of a 64-core DuckDB box per query. Is $/query on Grace actually lower?
5. **GB10 placement.** Run the CPU on Grace-resident filters while the GPU aggregates, and measure the net gain or loss under the §11 contention.
6. **Cold fill rate on Grace.** Chunked reader from NVMe into Grace memory: how long a 400 GB cache takes to warm.

## Sources

- [D] demand.md, `## Ratings` and `## First conversations` (local, llm-wiki/artifacts/product-research/demand.md) (accessed 2026-10-29)
- [I] inventory.md at master f25c250d, incl. yardstick and benchmark evidence (local) (accessed 2026-10-29)
- [E] evidence-in-hand.md (local) (accessed 2026-10-29)
- [R] llm-wiki/reports/dgx-spark.md, peacockdb's GB10 measurements, §1–§13 (local) (accessed 2026-10-29)
- [SM] sirius.md, `## Spilling to host`, `## peacockdb vs Sirius` (local) (accessed 2026-10-29)
- [NV] nvidia.md, verdict and GQE sections (local) (accessed 2026-10-29)
- [S1] Sirius @ b9e05ffc653c8ed031c789e5c1415ac1186e0619: `bench/sf1000-repro/README.md:1-7,30-35`, `docs/super-sirius/configuration.md:1007-1050`, `docs/super-sirius/late-materialization.md:253-260`, `docs/super-sirius/compressed-pinning.md:46-76,121`, `docs/README.md:157-203` — https://github.com/sirius-db/sirius/tree/b9e05ffc653c8ed031c789e5c1415ac1186e0619 (accessed 2026-10-29)
- [S2] Yogatama et al., CIDR 2026, arXiv 2508.04701v3 — https://arxiv.org/html/2508.04701v3 (accessed 2026-10-29)
- [S3] rapidsai/gqe @ 50facc46fb99ecf4a955697e78803707335814b3: `include/gqe/memory_resource/system_memory_resource.hpp:31-38`, `src/memory_resource/numa_pool_memory_resource.cpp:70-81`, `src/task_manager_context.cpp:97-98`, `include/gqe/utility/cupti_activity.hpp:62` — https://github.com/rapidsai/gqe/tree/50facc46fb99ecf4a955697e78803707335814b3 (accessed 2026-10-29)
- [S4] rapidsai/cudf @ 785ee522f5688f485fc0b1eabca7f8d6d2adea0e, `python/cudf_polars/cudf_polars/callback.py` — https://github.com/rapidsai/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/python/cudf_polars/cudf_polars/callback.py (accessed 2026-10-29)
- [S5] rapidsai/rmm @ 006f950a634908de9688cfe1881947ad4ca62b59, `cpp/include/rmm/mr/system_memory_resource.hpp:39-55` — https://github.com/rapidsai/rmm/blob/006f950a634908de9688cfe1881947ad4ca62b59/cpp/include/rmm/mr/system_memory_resource.hpp#L39-L55 (accessed 2026-10-29)
- [S6] heavyai/heavydb README — https://github.com/heavyai/heavydb (accessed 2026-10-29)
- [P1] KTH, GH200 unified memory study, arXiv 2407.07850 — https://arxiv.org/html/2407.07850v1 (accessed 2026-10-29)
- [P2] Werner et al. (HPI, Rabl group), Grace Hopper characterisation, HCDS 2025 — https://hpi.de/oldsite/fileadmin/user_upload/fachgebiete/rabl/publications/2025/hcds25-Werner-GraceHopper.pdf (accessed 2026-10-29)
- [P3] Alps quad-GH200 node study, arXiv 2408.11556 — https://arxiv.org/html/2408.11556v2 (accessed 2026-10-29)
- [P4] dnhkng, GH200 benchmarking — https://dnhkng.github.io/posts/gh200-benchmarking/ (accessed 2026-10-29)
- [P5] Verda (DataCrunch), data movement in GH200 — https://verda.com/blog/data-movement-in-the-nvidia-gh200-grace-hopper-superchip (accessed 2026-10-29)
- [P6] NVIDIA Grace performance tuning guide — https://docs.nvidia.com/grace-performance-tuning-guide.pdf (accessed 2026-10-29)
- [P7] CUDA programming guide, unified memory — https://docs.nvidia.com/cuda/cuda-programming-guide/04-special-topics/unified-memory.html (accessed 2026-10-29)
- [P8] Chips and Cheese, "Grace Hopper, Nvidia's halfway APU" — https://chipsandcheese.com/p/grace-hopper-nvidias-halfway-apu (accessed 2026-10-29)
- [P9] CUDA programming guide, extended GPU memory — https://docs.nvidia.com/cuda/cuda-programming-guide/04-special-topics/extended-gpu-memory.html (accessed 2026-10-29)
- [N1] NVIDIA blog, GH200 for Apache Spark (incl. HEAVY.AI GH200 vs 8×A100) — https://developer.nvidia.com/blog/nvidia-gh200-superchip-delivers-breakthrough-energy-efficiency-and-node-consolidation-for-apache-spark/ (accessed 2026-10-29)
- [N2] NVIDIA GH200 product page — https://www.nvidia.com/en-us/data-center/grace-hopper-superchip/ (accessed 2026-10-29)
- [N3] NVIDIA blog, GPU-native Velox and cuDF, 2025-10-06 — https://developer.nvidia.com/blog/accelerating-large-scale-data-analytics-with-gpu-native-velox-and-nvidia-cudf/ (accessed 2026-10-29)
- [N4] NVIDIA blog, Presto on GB200 NVL72, 2026-07-08 — https://developer.nvidia.com/blog/running-low-latency-analytical-workloads-with-gpu-accelerated-presto-on-nvidia-gb200-nvl72/ (accessed 2026-10-29)
- [O1] CSCS Alps — https://www.cscs.ch/computers/alps (accessed 2026-10-29)
- [O2] Isambard-AI specs — https://docs.isambard.ac.uk/specs/ (accessed 2026-10-29)
- [O3] TACC Vista — https://docs.tacc.utexas.edu/hpc/vista/ (accessed 2026-10-29)
- [O4] CoreWeave pricing (GH200, GB200 NVL72 instances); NVIDIA GB200 NVL72 page — https://www.coreweave.com/pricing , https://www.nvidia.com/en-us/data-center/gb200-nvl72/ (accessed 2026-10-29)
- [O5] Lambda pricing — https://lambda.ai/pricing (accessed 2026-10-29)
- [O6] HEAVY.AI on GH200, BusinessWire 2025-01-21 — https://www.businesswire.com/news/home/20250121115004/en/HEAVY.AI-Announces-Availability-of-Analytics-Platform-Accelerated-by-NVIDIA-Grace-Hopper-Superchip (accessed 2026-10-29)
- [O7] getdeploying, GH200 offers — https://getdeploying.com/gpus/nvidia-gh200 (accessed 2026-10-29)
- [O8] getdeploying, GB200 / GB300 offers — https://getdeploying.com/gpus/nvidia-gb200 , https://getdeploying.com/gpus/nvidia-gb300 (accessed 2026-10-29)
- [O9] AWS EC2 Capacity Blocks pricing — https://aws.amazon.com/ec2/capacityblocks/pricing/ (accessed 2026-10-29)
- [O10] Verda pricing — https://verda.com/pricing (accessed 2026-10-29)
- [O11] The Register, DGX Spark review, 2025-10-14 — https://www.theregister.com/2025/10/14/dgx_spark_review/ (accessed 2026-10-29)
- [O12] NVIDIA DGX Spark — https://www.nvidia.com/en-us/products/workstations/dgx-spark/ (accessed 2026-10-29)
- [O13] NVIDIA dgx-spark-playbooks — https://github.com/NVIDIA/dgx-spark-playbooks (accessed 2026-10-29)
- [O14] NVIDIA DGX Station — https://www.nvidia.com/en-us/products/workstations/dgx-station/ (accessed 2026-10-29)
- [F1] jeff1evesque, Spark RAPIDS on 2× DGX Spark — https://forums.developer.nvidia.com/t/kernel-7-0-0-1019-nvidia-makes-apache-spark-rapids-gpu-work-1-5-2-4x-slower-on-dgx-spark-6-17-0-1032-with-the-same-driver-580-178-04-is-fine/386791 (accessed 2026-10-29)
- [F2] cuDF on GB10 aarch64 request — https://forums.developer.nvidia.com/t/rapids-cudf-preview-build-request-for-grace-blackwell-gb10-dgx-aarch64-kernels-missing/352629 (accessed 2026-10-29)
- [F3] GB10 hangs under load — https://forums.developer.nvidia.com/t/device-hangs-under-load/375016 (accessed 2026-10-29)
- [F4] Sirius slower than DuckDB on GB10 — https://github.com/sirius-db/sirius/issues/609 (accessed 2026-10-29)
