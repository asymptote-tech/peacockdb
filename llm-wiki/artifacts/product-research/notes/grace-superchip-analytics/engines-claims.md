# Competitor Grace-superchip features and benchmark claims (as of 2026-10-29)

Method note: the session's WebSearch budget was exhausted when this researcher started, so every source is a shallow git clone (pinned SHA) or a WebFetch of a URL already known. Search-dependent items (the Spark RAPIDS GH200 blog, the 2026-07 Presto/Velox GB200 NVL72 blog, Vera CPU, academic GH200 papers, Kinetica/Voltron/SQream) could not be found or checked. They are listed under Gaps, not filled in from memory.

Clones live under /tmp/claude-1000/-media-data-peacockdb/ba2658c9-3cbe-4aa7-bcb8-c0a874d6fde0/scratchpad/ext:
- sirius-db/sirius @ b9e05ffc653c8ed031c789e5c1415ac1186e0619 (2026-10-28)
- rapidsai/gqe @ 50facc46fb99ecf4a955697e78803707335814b3 (2026-08-10)
- NVIDIA/cuCascade @ b09010ae997217a195d9a6e159f41e54e29470fc (2026-10-28)
- rapidsai/rmm @ 006f950a634908de9688cfe1881947ad4ca62b59 (2026-10-29)
- rapidsai/cudf @ 785ee522f5688f485fc0b1eabca7f8d6d2adea0e (2026-10-29)

GitHub URL form for code citations: https://github.com/<repo>/blob/<SHA>/<path>#L<n> (accessed 2026-10-29).

## Sirius: GB300/GH200 results, Grace-specific code, design intent

### Takeaway
Sirius benchmarks its current engine ("super-sirius") on a GB300 with a 72-core Grace CPU. The headline number is 8.180 s for TPC-H SF1000 hot (best-of-3, 22/22 queries) from a repro-only branch with patched libcudf. Its Grace-relevant features are generic: a NUMA-aware pinned-host tier, `pin_table(tier='host')` streaming over C2C at about 370 GB/s, and compressed pinning tuned against that link rate. Nothing in the docs says the design targets Grace, and the code has no ATS, unified-memory or C2C-specific path. The CIDR/arXiv paper evaluates the legacy engine on a GH200 but also never says it targets superchips.

### Cited Findings
- **8.180 s on GB300, TPC-H SF1000.** The repro README is titled "TPC-H SF1000 reproduction — 8.180 s on GB300". It reports a 22-query suite, best-of-3, 22/22 byte-identical, against a 15.99 s starting point (−48.8%). Measured 2026-08-01 on "GB300, 152 SMs, 256 GB HBM, 72-core Grace aarch64, driver 595.58.03, CUDA 13.2" — sirius `bench/sf1000-repro/README.md:1-7` @ b9e05ffc ([link](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/bench/sf1000-repro/README.md), accessed 2026-10-29).
  - The branch is "for reproduction only… not proposed for merge" (`README.md:9-10`).
  - It uses branch `felipeblazing/sirius@repro/sf1000-8.5s` plus a **patched libcudf** from `felipeblazing/cudf@perf/sirius-sf1000-repro` (`README.md:17-24`).
  - Ten changes produce the gain (`README.md:85-100`): `ast_jit` expression strategy, a cuDF `strings::like` backtrack skip, Bloom fast-range indexing, cuco bucket size 4, bitpack plans, and others.
  - The run needs about 470 GB of host RAM for the pinned host pool (same file, ~line 156).
- **Official TPC-H-style power/throughput run on the same stack (2026-08-11).** SF1000, 7 streams, mixed-tier pin layout: Power@Size 7,165,616, Throughput@Size 4,951,261, QphH@Size 5,956,411 — `bench/sf1000-repro/README.md:30-35` @ b9e05ffc. This is self-reported and not an audited TPC result.
- **Mainline numbers with unpatched libcudf on GB300 SF1000:**
  - GPU-pinned, late materialization: 7.391 s with the gate off, 7.003 s with it on — `docs/super-sirius/late-materialization.md:253-260` @ b9e05ffc.
  - Compressed pinning plus decode-time pushdown: 7.866 s → 6.918 s — `docs/super-sirius/compressed-pinning.md:121-123`.
  - These are GPU-pinned (data resident in HBM), so they do not exercise C2C.
- **Host-pinned profile measured on GB300.** "Tuned profile: GB300, TPC-H SF1000 host-pinned", measured 2026-07-31 on a "72-core GB300, 256 GB HBM". Suite hot time went 20.8 s → 18.1 s (−13%) with host `block_size` 64Mi, the memory prefetcher and 8 pipeline threads. The doc adds: "The H2D interconnect sustains ~350-380 GB/s regardless of stream count; past these settings the link, not scheduling, is the bound." — `docs/super-sirius/configuration.md:1007-1050` @ b9e05ffc.
- **Compressed pinning, tier trade-off (GB300, SF1000):**
  - Baseline is 20.74 s all-host uncompressed. All-host compressed is 22.20 s (+7.0%). Lineitem+orders GPU-compressed with dims GPU-raw is 16.66 s (−19.7%).
  - "Raw host pins stream at link rate (~370 GB/s on GB300 C2C)".
  - Rule of thumb: pick plans whose decompress throughput exceeds the host→GPU link (~370 GB/s on GB300).
  - Source: `docs/super-sirius/compressed-pinning.md:46-76` @ b9e05ffc.
- **`pin_table` tiers.** `tier = 'gpu'` pins columns in GPU memory; `tier = 'host'` pins them "in pinned host memory instead, for tables larger than GPU memory". Simpatico compression is available on both tiers — `docs/README.md:157-203` @ b9e05ffc.
  - Host pin batches are converted on a round-robin GPU into a `host_data_representation` on that GPU's NUMA-local host space. Peak GPU residency is about one batch — `docs/super-sirius/scan.md:343`.
- **NUMA-aware pinned memory:**
  - One pinned pool group per NUMA node; default capacity is 0.9 of each node's RAM, allocated with `cudaMallocHost` — `docs/super-sirius/configuration.md:164-169`.
  - The top-level README states a different default: "8 GiB pinned host memory per NUMA node" (`docs/README.md:222`). This conflicts with configuration.md:25 ("90% of each NUMA node's RAM").
  - `HOST` tier is "NUMA-local pinned host memory", partitioned per NUMA node via cuCascade `use_host_per_gpu()`. Topology discovery reads `/sys/class/drm/.../numa_node` — `docs/super-sirius/multi-gpu-architecture.md:32-35, 102-104, 253`.
  - `small_pinned_host_memory_resource` keeps pinned pools with NUMA affinity — `docs/super-sirius/optimizations.md:310`.
- **Platform support.** Requirements list "Linux on amd64/x86_64 or arm64/aarch64" (`docs/README.md:35`). The README's headline performance figure is AWS g7e (Sirius) vs DuckDB on m9g.16xlarge, not Grace (`docs/README.md:26-28`).
- **Design-intent statements.** A grep of docs/src for "Grace", "GH200" and "C2C" finds only the GB300 measurement contexts above. No doc says Sirius is designed for Grace.
  - `docs/sirius-for-research.md:9` claims: "As of August 2026, Sirius leads ClickBench hot-run results".
  - The ClickBench results came from the legacy engine, pinned at `f79a6f42` (`docs/README.md:31-32`).
- **CIDR 2026 / arXiv 2508.04701, "Rethinking Analytical Processing in the GPU Era" (Yogatama, …, McKinney, Yu).** v3 is dated 2025-12-20 — [arXiv abs](https://arxiv.org/abs/2508.04701) / [HTML v3](https://arxiv.org/html/2508.04701v3) (accessed 2026-10-29).
  - Evaluation hardware is a GH200: 72 Neoverse cores, 480 GB LPDDR5X, Hopper with "92 GB" HBM3 (Table 1 says 96 GB), and NVLink-C2C "900 GB/s".
  - The paper notes "the Hopper GPU can access host memory at more than 400 GB/s".
  - It caches 50% of GPU memory and reports hot runs.
  - TPC-H SF100: 8.3× speedup vs DuckDB on c8i.8xlarge at equal $1.5/h, and 38× vs ClickHouse.
  - ClickBench on a Lambda GH200 ($1.5/h): ≥7.4× better TCO than c8g.metal-48xl / c7a.metal-48xl, and 11% faster than Umbra.
  - Distributed: up to 12.5× vs Doris on 4×A100 (PCIe).
  - The buffer manager uses device or pinned host memory, never unified memory.
  - Internal inconsistencies: GH200 price is $1.5/h vs $3.2/h, and C2C is called uni- vs bidirectional.

### Inferences
- Sirius's only Grace-specific behavior is empirical tuning: block size, link-rate rules of thumb, and an L2 sweep (`configuration.md:611` default comes "from a GB300 residency sweep"). Its host tier is ordinary `cudaMallocHost` pinned memory copied H2D. It does not use ATS/coherent direct access to Grace memory. So a Grace-native engine that reads LPDDR5X in place (zero-copy) or uses the CPU cores as co-processors would be differentiated. (estimate)
- The 8.18 s figure needs patched cuDF and a non-mergeable branch. Mainline GPU-pinned numbers are about 7.0-7.4 s but assume hot data resident in 256 GB of HBM. The host-pinned (C2C-bound) mainline figure is about 18.1-20.8 s. Compare against the host-pinned figure when claiming "larger than HBM" performance. (estimate)

### Gaps
- No ClickBench result for super-sirius on GH200 was found in the repo. The GH200 ClickBench claim exists only in the paper (legacy engine). The ClickBench site entry was not fetched because search was unavailable.
- The CIDR 2026 proceedings PDF was not fetched separately. The numbers above come from arXiv v3.

## cudf-polars / cudf.pandas / RMM on Grace Hopper

### Takeaway
RAPIDS has generic unified-memory and system-allocated-memory support: managed memory with prefetch by default in cudf-polars, `rmm::mr::system_memory_resource` (HMM/ATS) and `sam_headroom_memory_resource`. RMM's docs name Grace Hopper ATS over C2C explicitly. The NVIDIA blogs found, however, benchmark on T4 or unspecified hardware and make no GH200-specific claims.

### Cited Findings
- `rmm::mr::system_memory_resource` allocates with malloc/free for SAM access. Its doc says: "Address Translation Services (ATS) is a hardware/software solution for the Grace Hopper Superchip that uses the NVLink Chip-2-Chip (C2C) interconnect to provide coherent memory". HMM is described as the x86/PCIe path — rmm `cpp/include/rmm/mr/system_memory_resource.hpp:39-55` @ 006f950a ([link](https://github.com/rapidsai/rmm/blob/006f950a634908de9688cfe1881947ad4ca62b59/cpp/include/rmm/mr/system_memory_resource.hpp#L39-L55), accessed 2026-10-29).
- `sam_headroom_memory_resource`: "System allocated memory (SAM) can be migrated to the GPU, but is never migrated back the host". It reserves GPU headroom and only conditionally sets the preferred location to the GPU — rmm `cpp/include/rmm/mr/sam_headroom_memory_resource.hpp` (header comment) @ 006f950a.
- cudf-polars default memory resource: if `cuda_managed_memory` and `cudaDevAttrConcurrentManagedAccess` are both true, it uses a managed pool (80% of memory) wrapped in `PrefetchResourceAdaptor`, with `pylibcudf.prefetch.enable()`. Otherwise it uses an async pool — cudf `python/cudf_polars/cudf_polars/callback.py` (~lines 54-110) @ 785ee522.
- cudf benchmark utilities handle Grace Hopper as a special case: `nvmlDeviceGetMemoryInfo` is "not supported" on "systems without traditional GPU memory (e.g., Grace Hopper)" (issue NVIDIA/cudf#19427) — cudf `python/cudf_polars/cudf_polars/streaming/benchmarks/utils.py:425` and `python/cudf/cudf/pandas/_benchmarks/utils.py:341` @ 785ee522.
- No other mention of Grace, GH200 or ATS was found in cudf `docs/`, `cudf_polars` or `cudf.pandas` (grep @ 785ee522).
- NVIDIA blog "RAPIDS cuDF Unified Memory Accelerates pandas up to 30x on Large Datasets" (2024-08-09): unified memory enables oversubscription. It claims "up to 30x speedups for data joins on a 16 GB memory GPU" on a T4 / Xeon Gold 6130. It does not mention GH200 — [NVIDIA blog](https://developer.nvidia.com/blog/rapids-cudf-unified-memory-accelerates-pandas-up-to-30x-on-large-datasets/) (accessed 2026-10-29).
- NVIDIA blog "How to Work with Data Exceeding VRAM in the Polars GPU Engine" (2025-06-27, Semaan and Tepera): UVM is used "by default for most data sizes". It also covers multi-GPU streaming for hundreds of GB to a few TB and PDS-H at 3 TB. It does not mention GH200 — [NVIDIA blog](https://developer.nvidia.com/blog/how-to-work-with-data-exceeding-vram-in-the-polars-gpu-engine/) (accessed 2026-10-29).

### Inferences
- The "processing larger-than-GPU-memory data on GH200" framing likely exists in some NVIDIA material, but none was located. RAPIDS ships SAM/ATS plumbing without a published GH200 benchmark claim in the sources checked. (estimate)

### Gaps
- No NVIDIA blog specifically on cudf-polars/cudf.pandas on GH200 was found (search unavailable).

## HeavyDB on Grace/ARM

### Takeaway
No evidence was found of GH200 or aarch64 support in HeavyDB.

### Cited Findings
- The heavydb README does not mention aarch64/ARM, Grace or GH200. It says: "The maintained dependency scripts support Ubuntu 22.04 and Rocky Linux 8.x." — [github.com/heavyai/heavydb](https://github.com/heavyai/heavydb) (accessed 2026-10-29).

### Gaps
- HeavyAI enterprise docs and press were not checked (search unavailable). The repo was not cloned to grep for aarch64 in CMake.

## NVIDIA's own data-processing claims (GH200 / GB200 / GQE / cuCascade / Decompression Engine)

### Takeaway
NVIDIA's marketing claims "up to 36X" for Spark RAPIDS on 16-node GH200. Its Velox/Presto blog reports TPC-H SF1000 in 99.9 s on a single GH200, or 148.9 s with Q21 run via managed memory. GQE has explicit Grace and Blackwell hooks: NUMA pools, pageable ATS/HMM memory, and hardware Decompression Engine pool usage.

### Cited Findings
- GH200 product page: GH200 "removes the need to copy memory back and forth between the CPU and GPU, accelerating data processing by up to 36X".
  - Footnote: "NDS-DS results running Apache Spark with NVIDIA RAPIDS™ Accelerator on 16-node GH200 using SF10 versus 16-node premium CPUs".
  - Also: NVLink-C2C "speeds up transfer of the preprocessed data to the Hopper GPU by 7X compared to PCIe".
  - Source: [nvidia.com GH200 page](https://www.nvidia.com/en-us/data-center/grace-hopper-superchip/) (accessed 2026-10-29). These are vendor claims with no absolute times.
- NVIDIA blog "Accelerating Large-Scale Data Analytics with GPU-Native Velox and NVIDIA cuDF" (2025-10-06; Kimball, Nagy, Makkar, Bauer, Jin): TPC-H SF1000 hot, 21 queries — [NVIDIA blog](https://developer.nvidia.com/blog/accelerating-large-scale-data-analytics-with-gpu-native-velox-and-nvidia-cudf/) (accessed 2026-10-29).
  - Presto C++ on AMD 7965WX: 1,246 s.
  - Presto on RTX PRO 6000 Blackwell: 133.8 s.
  - Presto on GH200: 99.9 s.
  - All 22 queries on GH200 take 148.9 s, with "CUDA managed memory to finish Q21".
  - The body makes no GB200 claims.
- GQE (rapidsai/gqe) @ 50facc46:
  - `system_memory_resource` for pageable memory: "On systems with Address Translation Services / Heterogeneous Memory Management the resulting pointers are also accessible from the device" — `include/gqe/memory_resource/system_memory_resource.hpp:31-38`.
  - NUMA memory resources use `mbind` and `cudaMemLocationTypeHostNuma` pools — `src/memory_resource/numa_memory_resource.cpp`, `src/memory_resource/numa_pool_memory_resource.cpp:70-71`.
  - Pool cap comment: "85% is default, because 90% typically fails on Grace with 480 GB. However, 85% is enough for TPC-H SF1k." — `src/task_manager_context.cpp:97-98`.
  - Hardware decompression: pools are created with `cudaMemPoolCreateUsageHwDecompress` when the driver and `memDecompressSupport` allow — `src/memory_resource/numa_pool_memory_resource.cpp:74-81`. There is also a `decompression_backend::de` option (`src/types.cpp:66`).
  - CUPTI comment: "Mem decompress event. Could only be observed on GB200 so far." — `include/gqe/utility/cupti_activity.hpp:62`.
  - Uses `cudaMemcpyBatchAsync` — `src/utility/cuda.cpp:175`.
  - Links: [repo @ SHA](https://github.com/rapidsai/gqe/tree/50facc46fb99ecf4a955697e78803707335814b3) (accessed 2026-10-29).
- cuCascade (NVIDIA/cuCascade @ b09010ae): tiered GPU / pinned host / disk memory with "numa aware allocators" and "Hardware Topology Discovery: Automatic detection of NUMA regions and GPU-CPU affinity" — `README.md:10-12`. Its topology doc frames NUMA in PCIe terms ("can halve PCIe transfer bandwidth", `docs/topology-and-configuration.md:32`). There are no Grace/C2C/ATS references in src/include/docs.

### Inferences
- GQE is the NVIDIA engine most explicitly built for Grace and Blackwell superchips, given its ATS pageable memory, host-NUMA pools sized for Grace's 480 GB and HW Decompression Engine use. It is the most direct technical competitor for a Grace-native design. (estimate)

### Gaps
- Not found, because search was unavailable:
  - The NVIDIA Spark RAPIDS GH200 developer blog and GTC session pages.
  - The 2026-07 Presto/Velox on GB200 NVL72 low-latency analytics blog.
  - GQE published benchmark numbers on GH200/GB200.
  - The Blackwell Decompression Engine blog.
  - NVIDIA Vera CPU analytics claims.
- GQE has no README mention of GH200/GB200 results.

## Academic GH200 database papers

### Takeaway
Not researched successfully. Search was unavailable, and no paper URLs were verified.

### Cited Findings
- The only verified GH200 analytics paper is arXiv 2508.04701 (Sirius, above) — [arXiv](https://arxiv.org/abs/2508.04701) (accessed 2026-10-29).

### Gaps
- TU Berlin/TUM/ETH/HPI GH200 papers (e.g. out-of-memory joins, CPU-GPU co-processing on Grace Hopper) were not located. A citation must not be fabricated. Re-run with search (DaMoN/VLDB/SIGMOD 2024-2026, query "Grace Hopper join").

## Grace-specific startups/products (Voltron Theseus, Kinetica, Oracle, SQream)

### Takeaway
No verified information was collected.

### Gaps
- Voltron Data Theseus, Kinetica, SQream and Oracle claims on GH200/GB200 were not checked (search unavailable).
