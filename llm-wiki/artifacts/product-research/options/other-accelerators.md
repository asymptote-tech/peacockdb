# other-accelerators — GPU SQL where CUDA does not run (AMD, Apple, Intel, AI chips)

## Verdict

- **What it is:** peacockdb's executor, rebuilt on a cuDF-like layer for non-NVIDIA chips. For AMD that layer already exists: hipDF is a ROCm port of the same cuDF 25.10 that peacockdb pins. Apple Metal would need a new kernel layer. Intel datacenter GPUs and AI chips (Cerebras, Groq, TPU) are dead ends for this.
- **Who needs it:** demand.md rates this **weak**: 12 requests from 5 posters, and none of them attaches a workload or a budget [D]. On Apple, DuckDB on the CPU beats GPU kernels [7].
- **Budget:** an AMD-only port fits easily, at about 0.5× (estimate). Adding Metal brings it to about 1.5×. Code is not the problem here. Demand is, and so is the build and tuning risk that stalled the only public attempt [4][5].

## Who needs it badly

- **Rating: weak**, taken from demand.md `## Ratings`, row `other-accelerators`: "Many asks, but no workload or budget attached. On Apple, the CPU engine wins." R/U/C 12/3/2 · 5 posters [D].
- **Jon, CEO of Hot Aisle** (`jon-hotaisle`, an AMD MI300X neocloud), asked Sirius for AMD support so it could use MI300's 192 GB+ of memory. [sirius #53](https://github.com/sirius-db/sirius/issues/53) was opened 2025-06-28 and is still open; it has one skeptical reply and no workload (accessed 2026-10-29). He is a seller of AMD hours, not a user with a query.
- **`mycomputercrashsalot`** spent July–October 2026 trying to port Sirius to ROCm ([PR #1153](https://github.com/sirius-db/sirius/pull/1153), closed 2026-09-28; [sirius-rocm](https://github.com/mycomputercrashsalot/sirius-rocm) @ a24f2c0f, accessed 2026-10-29). He is a builder, not a buyer. He states no workload.
- **Coco Alemana's founder** (`prmelehan` / u/Impressive_Run8512) writes MLX/Metal aggregates for a DuckDB-based desktop tool. Threads: [duckdb #20558](https://github.com/duckdb/duckdb/discussions/20558) and [r/DuckDB](https://www.reddit.com/r/DuckDB/comments/1qdrwxi/) (both 2026-01-15). He says of Sirius: "doesn't seem like they're doing anything." The discussion is a prototype `mlx_mean` with no benchmarks and 0 replies (accessed 2026-10-29).
- **u/Wackome** ([r/mac](https://www.reddit.com/r/mac/comments/1hdxh51/), 2024-12-14) is a policy analyst who gets 70× from cuDF on an A5000 laptop and wants the same on an M4. This is the only first-person Apple *user* with a measured pain [D].
- **u/Individual-Divide817** ([r/apachespark](https://www.reddit.com/r/apachespark/comments/1dzlu3b/), 2024-07-10) wants Spark on the GPU of an M2 Ultra.
- **Builders who measured, with negative or mixed results:**
  - On an M4, "DuckDB's optimized SQL engine beats hand-written GPU kernels on every standard TPC-H query" ([HN 47059629](https://news.ycombinator.com/item?id=47059629), 2026-02-18).
  - u/sado361 built a Metal SQLite "to test a theory about the PCIe bottleneck" ([r/programming](https://www.reddit.com/r/programming/comments/1q6dj59/), 2026-01-07).
- **Cerebras, Groq, TPU:** I found no request at all. An HN search for "SQL TPU" finds no relevant story ([HN search](https://hn.algolia.com/api/v1/search?query=SQL%20TPU&tags=story), accessed 2026-10-29) [A].
- **Best first conversations:**
  - **Hot Aisle, and TensorWave by the same logic.** Ask whether their customers run any non-AI data work on idle MI300X hours.
  - **Coco Alemana**, for Apple.

  Neither is a design partner yet.

## How it would work

- **AMD (the only realistic target).** Keep the Rust planner, the FlatBuffers wire format, the driver and the FFI unchanged. Rebuild `cpp/` against `libhipdf` instead of `libcudf`.
  - hipDF keeps `namespace cudf`, the `<cudf/...>` headers, `rmm::` (via hipMM) and the `cudf::cudf` CMake target [1].
  - Port the five custom kernels in `cpp/src/spark_hash_partition.cu` (288 lines) with HIPIFY, checking for wave64.
  - Port about 30 CUDA runtime calls (events, `cudaMemcpy*`, `cudaMemGetInfo`) to `hip*`.
- **Data path on AMD:** Parquet on NVMe or object store → host RAM → hipDF `read_parquet`. Decompression runs through hipCOMP: Snappy and LZ4 natively; ZSTD and GZIP decompression is experimental [2]. Then hipDF operators run on HBM (192–288 GB per GPU), and results go out as Arrow. There is no GPUDirect Storage or KvikIO [1].
- **Apple:** add a new `MetalExecutor` behind the same FFI. Parquet is decoded on the CPU (arrow-rs, already in the tree via DataFusion) into one pre-wired unified-memory region, so nothing is copied. Metal kernels then cover filter, project, hash aggregate, hash join, sort and top-N. Everything else falls back to the existing CPU backend (2,086 lines) [I]. This is the pattern of the `gpudb` DuckDB extension [6].
- **Intel and Cerebras/Groq/TPU:** not built (see below).

## The human's questions

### What exists today

- **AMD — hipDF / ROCm-DS.** This is AMD's own port of cuDF 25.10 [1].
  - **Status:** GA ("production status") since ROCm-DS 25.10. The current release is hipDF 3.0.0 in ROCm-DS 26.03 on ROCm 7.2.3.
  - **Licence and API:** Apache-2.0. C++ and Python APIs; no Java [1].
  - **GPUs:** MI355X, MI325X/MI300X and MI250/MI210. Radeon RDNA 3/4 is experimental [3].
  - **C++ library:** builds with `./build.sh libcudf`. AMD's PyPI also ships an `amd-libhipdf` wheel; I did not inspect its contents [1].
  - **Not supported:** zstd *compression*, GDS/KvikIO, per-thread default streams, rocTX, Polars, and -O0 debug builds [1].
  - **Dependencies:**
    - hipMM replaces RMM.
    - hipCollections replaces cuco ([ROCm/hipCollections](https://github.com/ROCm/hipCollections) @ 401e21a7).
    - hipCOMP, an nvCOMP 2.2 fork, replaces nvCOMP [2].
  - **Activity:** the last commit is f59924fd (2026-07-29, a repo move). There have been no functional commits since July, and no branch tracks cuDF 26.x. The lag behind RAPIDS is about 2–3 releases [1].
  - **ROCm-DS also has:** hipRAFT, hipVS, hipGRAPH (early access), dask-hip, hip-ucxx and rocOpt [3]. None of these is a SQL engine.
- **AMD — Sirius on ROCm.**
  - [PR #1153](https://github.com/sirius-db/sirius/pull/1153) used 77 shim headers on top of hipDF. It compiled on gfx942 and never ran a query; a claimed runtime pass was retracted.
  - Sirius pins cuDF 26.06, so two Parquet APIs had to be backported by a patch script ([#1158](https://github.com/sirius-db/sirius/issues/1158)).
  - Maintainers moved it out of core, saying it could return "once it's benchmarked". It continues in [sirius-rocm](https://github.com/mycomputercrashsalot/sirius-rocm) and [cuda2rocm](https://github.com/mycomputercrashsalot/cuda2rocm) @ 0b869506. Both are blocked on "rented boxes dying mid-build" [4][5].
- **AMD — other routes.**
  - I found no Spark-RAPIDS path: hipDF has no JNI [1].
  - HeavyDB has no HIP code (heavyai/heavydb @ 72c6df9f) [H].
  - SCALE (CUDA compiled for AMD) shows thrust failing on every target in its own validation table, which has no gfx942 column ([scale-validation](https://github.com/spectral-compute/scale-validation) @ a77f241b).
  - ZLUDA is a hobby project aimed at consumer GPUs ([ZLUDA](https://github.com/vosen/ZLUDA/releases), [Tom's Hardware](https://www.tomshardware.com/pc-components/gpu-drivers/cuda-emulator-for-amd-gpus-zluda-loses-funding-with-v6-release-embattled-project-goes-back-to-hobby-status-but-now-includes-32-bit-physx-support)) (all accessed 2026-10-29).
  - **I found no published GPU-SQL or dataframe benchmark on MI300X** [H].
- **Apple Metal.**
  - **`gpudb`**, a DuckDB community extension with both Metal and CUDA backends (singhpratech/duckdbgpumetaldbram @ 525425ae):
    - On an M4 Max at SF10, 19 of 22 TPC-H queries run on the GPU.
    - It reports 1.06–48× via `execute()` and 0.92–26× via `sql()`. These figures are self-reported and nobody has reproduced them.
    - Plain SQL needs a Python shim, because "DuckDB's stable C API has no hook before planning" [6].
  - **TUM, DaMoN '26:**
    - The GPU reaches up to 5× over the CPU on SSB.
    - On an M3 Ultra the GPU reads 738 GB/s against the CPU's 253 GB/s.
    - Metal buffers pay page wiring at 10–60 GB/s and a coherence pass at 70–120 GB/s.
    - SIMD lockstep breaks ordinary spin locks [8].
  - **MLX TPC-H on a base M4** (sadopc/unified-db-2 @ e725f29e): DuckDB wins every TPC-H query. At SF10, Q1 takes 210 ms in DuckDB against 492 ms in MLX [7].
  - `arrowmetal` 0.1.0 exists for arrow-rs, but its docs build fails ([docs.rs](https://docs.rs/crate/arrowmetal/0.1.0)).
  - I found no DataFusion Metal backend [H].
- **Intel.**
  - The Data Center GPU Max 1550 (128 GB HBM2e, 3.28 TB/s) is discontinued ([Intel ordering](https://www.intel.com/content/www/us/en/products/sku/232873/intel-data-center-gpu-max-1550/ordering.html)).
  - Falcon Shores is cancelled ([Tom's Hardware](https://www.tomshardware.com/tech-industry/artificial-intelligence/intel-cancels-falcon-shores-gpu-for-ai-workloads-jaguar-shores-to-be-successor)). The successors are AI or inference parts.
  - Gazelle/OAP was archived in 2023. Modin lists no GPU engine.
  - SYCLDB (EURECOM, SYCLOPS-Project-EU/sycldb @ 0af5c549) matches CUDA on NVIDIA, with no Intel GPU results ([EURECOM](https://www.eurecom.edu/en/node/4899826)) (all accessed 2026-10-29) [H].
- **SQL on Cerebras, Groq or TPU.**
  - The only result is Microsoft's TQP (PVLDB 2022). It ran TPC-H Q6 at SF1 on a TPU in 25 ms, against 1 ms on a V100, and notes "PyTorch/XLA does not support all the necessary tensor operations" ([arXiv 2203.01877](https://ar5iv.labs.arxiv.org/html/2203.01877)).
  - TCUDB used NVIDIA Tensor Cores, not TPUs.
  - On Graphcore IPU there is only BFS and PageRank work.
  - For Cerebras, Groq, SambaNova, Tenstorrent and Trainium I found no SQL or analytics work [A].

### How big is the cuDF surface peacockdb uses, and what would porting cost

- **Measured at master f36a68ec, read only, over `cpp/src` and `cpp/include` (6,699 raw lines; 4,586 yardstick lines [I]):**
  - 1,010 `cudf::` references, 210 distinct `cudf::` names.
  - About 75 distinct algorithm entry points. The most used are binary_operation (17), cast (17), make_column_from_scalar (13), apply_boolean_mask (9), slice (7) and ast `compute_column` (7).
  - The rest:
    - **Joins:** groupby, hash_join, distinct_hash_join, conditional_{inner,left,left_semi}_join, mixed_left_semi_join and cross_join.
    - **Sort and windows:** sorted_order, merge and grouped_rolling_window.
    - **Shuffling and selection:** hash_partition, partition, gather, concatenate, distinct, drop_nulls, replace_nulls and copy_if_else.
    - **Strings and dates:** strings::{like, slice_strings, concatenate, to_upper, to_lower} and datetime::{extract_datetime_component, add_calendrical_months}.
    - **I/O and conversion:** io::read_parquet (+metadata), from_arrow/to_arrow_host and dictionary::decode.
  - It includes 56 distinct `<cudf/...>` headers.
- **Outside cuDF:**
  - 5 `__global__` kernels (Spark-compatible hash plus pmod) in one 288-line `.cu`.
  - About 30 CUDA runtime calls.
  - RMM pool and statistics resources.
  - One `thrust::fill`.
  - No NCCL, KvikIO or nvCOMP calls.
- **Version match:** peacockdb's `third_party/cudf` is at 43505bb9, `VERSION` 25.10.00 (v25.10.00a-440). hipDF is "API-compatible with RAPIDS cuDF 25.10" [1]. This is the single best fact for this option: **peacockdb sits on exactly the cuDF version AMD ported.** Sirius, on 26.06, does not [5].
- **What the layer underneath is worth:** libcudf's own `cpp/src` is about 166k non-blank lines, plus about 77k in `cpp/include` (raw `grep`, block comments included; estimate). That is roughly **7× + 3× the whole peacockdb yardstick**. Rebuilding even the 75 entry points from scratch for a new vendor is the expensive path.
- **Porting cost by target:**
  - **AMD via hipDF:** about 1–2k changed lines, ≈ 0.05–0.1× (estimate). Recompile, HIPIFY the .cu, swap `cuda*` for `hip*`, and fix 25.10a→25.10 API drift. Unverified per entry point: there is no public matrix of which libhipdf tests pass on gfx942 [H].
  - **Apple Metal from scratch:** a TPC-H/TPC-DS-subset kernel set (filter, project/AST, hash aggregate, hash join, sort/top-N, gather, partition, basic strings) plus a Metal memory manager, at about 15–25k lines ≈ 0.7–1.1× (estimate). For scale, gpudb covers 19/22 TPC-H queries with no window functions, FULL joins or median [6].
  - **Intel SYCL:** about the same size as Metal, with no hardware base to sell into. Not worth it.
  - **Cerebras/Groq/TPU:** no cuDF-like substrate exists. The only route is TQP-style SQL compiled to tensor ops. It is a research project, not an alpha.

### Do wafer-scale or dataflow chips fit relational operators at all

- **Scan, filter, projection and dense aggregation fit.** These are streaming tensor ops. TQP shows they can be expressed [A].
- **Hash joins, high-cardinality group-by, variable-length strings and data-dependent output sizes fit badly:**
  - **TPU/XLA** need static shapes. Any op whose output size depends on the data needs a fixed `size=` and pads or truncates ([JAX docs](https://docs.jax.dev/en/latest/_autosummary/jax.numpy.unique.html)).
  - **Groq** fixes its schedule at compile time and has about 230 MB of SRAM per chip ([groq.com](https://groq.com/lpu-architecture)).
  - **Cerebras WSE-3** spreads 44 GB of SRAM as 48 KB per core and talks by mesh message passing ([cerebras.ai](https://www.cerebras.ai/chip)). A hash join would have to become a radix shuffle across the fabric, which no toolchain supports.
  - TQP itself turned joins and group-by into sort-based algorithms, repeated its hash join once per collision, padded strings into character matrices, and dropped decimals [A].
- **TPU v7** (192 GiB HBM at 7.38 TB/s, [Google](https://docs.cloud.google.com/tpu/docs/tpu7x)) is the only AI chip with GPU-class memory, but it is reachable only through XLA.
- **Verdict:** no. These chips win by compiling a known dataflow graph ahead of time. SQL's hot paths are data-dependent by nature. Plan Cerebras and friends out of this option (all accessed 2026-10-29).

### Who is asking

- **The count:** 12 requests, 5 posters, R/U/C 12/3/2 [D].
- **Mostly AMD and Apple.** AMD asks come from a neocloud CEO and a volunteer porter. Apple asks come from desktop-tool builders and one analyst.
- **Common to all:** no poster names a dataset, a query or a deadline. Nobody says they would pay.
- **The digests agree:**
  - "nobody attaches a workload to them" [HN digest].
  - "the posters are mostly vendors or hobbyists" [issue-trackers digest].

## Data path and cache

| Link | Figure | Source |
|---|---|---|
| MI300X / MI325X / MI355X HBM | 192 GB at 5.3 TB/s / 256 GB at 6 TB/s / 288 GB at 8 TB/s | [SemiAnalysis](https://inferencex.semianalysis.com/chips/mi325x-vs-mi300x), [TechRadar](https://www.techradar.com/pro/amd-releases-details-of-288gb-mi355x-accelerator-80-percent-faster-than-mi325x-8tb-s-memory-bandwidth) |
| H200 device read, measured | 4,484 GB/s | [I] |
| PCIe Gen5 x16 host→GPU (H200, measured) | 47.3 GB/s pinned, 10.6 GB/s pageable | [I]; MI300X should be similar (estimate) |
| GPU Parquet load (H200, measured) | 6.8–8.7 GB/s; decode plus Snappy is 80% of kernel time on q11v | [I] |
| 400 GbE NIC | ~50 GB/s | (estimate) |
| NVMe Gen4 / Gen5, per drive | ~7 / ~14 GB/s | (estimate) |
| Apple M4 Max / M3 Ultra unified memory | ~546 GB/s / ~819 GB/s (738 measured on the GPU), up to 512 GB | [8], [Wikipedia](https://en.wikipedia.org/wiki/Apple_M5) |
| Metal buffer wiring / coherence pass | 10–60 / 70–120 GB/s | [8] |
| Intel Arc Pro B60 | 24 GB at 456 GB/s, PCIe 5.0 x8 | [Intel](https://www.intel.com/content/dam/www/central-libraries/us/en/documents/2026-03/datasheet-b60-gpu.pdf) |

(all accessed 2026-10-29)

- **The thesis holds on AMD, with the same arithmetic as on NVIDIA.**
  - HBM is about 100× faster than PCIe and about 600× faster than Parquet decode.
  - Without a VRAM or pinned-RAM cache, the load eats the GPU's lead, as in the human's SF20 study (load > compute on 11 of 12 queries [E]).
  - **AMD is slightly worse on the data path:** hipDF has no GDS or KvikIO, so SSD data bounces through host RAM. ZSTD decompression is experimental [1][2].
  - **AMD is better on the cache:** 192–288 GB of HBM per GPU, 1.5–2.4 TB per 8-GPU box. The hot set fits in VRAM more often than on an 80 GB H100. That is Hot Aisle's whole argument [4].
- **On Apple the thesis changes shape.** There is no PCIe, but memory bandwidth is 5–10× below HBM, and the CPU reads the same memory at 250–550 GB/s. The GPU's edge over a good CPU engine is therefore ≤5× at best [8], and zero for operator-at-a-time kernels [7].
  - The bottlenecks become the internal SSD (~6 GB/s, estimate), CPU-side Parquet decode and Metal buffer wiring.
  - The cache needed is one pre-wired, DBMS-managed unified-memory arena holding decoded columns [8].
- **Cache needed:** the lean option's VRAM plus pinned-RAM column cache, ported to hipDF. Nothing new is specific to AMD.

## Build cost to alpha

| Component | On peacockdb: reuse/adapt/new, size | On Sirius: reuse/adapt/new, size |
|---|---|---|
| Planner, node vocabulary, wire, driver, Rust FFI | reuse, 0 | reuse (DuckDB plans), 0 |
| C++ executor → libhipdf (recompile, API drift 25.10a→25.10, CMake) | adapt, ~0.05× | adapt: shims plus backport of 26.06 APIs to 25.10, ~0.2× [5] |
| Custom kernels and CUDA runtime calls | adapt: 288-line .cu plus ~30 calls, wave64 check, ~0.01× | adapt: decode kernels (4.0k), dynamic filters, Simpatico compression (17.6k) and cuCascade (24.6k), ~0.4× [S] |
| Build/CI on rented MI300X, packaging (ROCm 7.2.3, GCC 14) | new, ~0.03× code, weeks of time | same, plus the build that has not finished in 3 months [5] |
| Performance tuning (hash join on hipCollections, wave64, Parquet via hipCOMP) | new, ~0.1–0.2× (estimate; no public MI300X numbers exist) | same, ~0.2× |
| Front door a user can run (Flight SQL or DuckDB/DataFusion host) | new, ~0.2× (estimate; shared with any option) | reuse (DuckDB extension), 0 |
| Optional Metal backend (kernels, unified-memory arena, CPU fallback) | new, ~0.7–1.1× (estimate) | new, ~1×; Sirius has no backend seam |
| Intel SYCL / Cerebras / TPU | not built | not built |

Total new work: 0.5× engine on peacockdb, n/a on Sirius (estimate)

- **peacockdb, AMD only:** about 0.4–0.5×. With Metal added: about 1.5×, still within budget.
- **Sirius is n/a as a base.** A one-time ROCm port costs about 0.8× (estimate). But it is a hard fork of a 154k-line tree that changes by about 50–60k lines a week, and sirius.md already rules that out [S]. Its maintainers have declined to carry ROCm in core until someone benchmarks it [4].
- **The hard part is not code.** It is:
  - getting libhipdf built and green on gfx942 (the one public attempt stalled here);
  - finding where hipCollections joins and wave64 kernels lose to H100;
  - living on a fork that has had no functional commits since July 2026 and trails cuDF by about a year.

## Adoption friction

- **AMD:** the user must already own or rent MI300-class GPUs. Those sit mostly in AI neoclouds and hyperscalers (Azure ND MI300X), not in data teams [H]. A data team without AMD gains nothing. A team with H100s has cuDF, Sirius and Spark-RAPIDS already.
- **Apple:** works on every Mac, so friction is low, but the user must prefer a new engine over DuckDB. DuckDB is faster on most TPC-H queries on an M4 [7]. gpudb already ships as `INSTALL gpudb FROM community` [6].
- **No incumbent plugin path on AMD:** Spark-RAPIDS needs JNI, which hipDF lacks [1]. A product would be a new engine, or a DuckDB extension like Sirius.

## Platform risk

- **NVIDIA cannot compete on AMD silicon.** nvidia.md rates this risk weak: "The real rival is AMD's own hipDF" [N]. The CUDA EULA ban on translation layers bites binary shims, not source ports of Apache-2.0 cuDF ([Tom's Hardware](https://www.tomshardware.com/pc-components/gpus/nvidia-bans-using-translation-layers-for-cuda-software-to-run-on-other-chips-new-restriction-apparently-targets-zluda-and-some-chinese-gpu-makers), accessed 2026-10-28) [N].
- **AMD is both the dependency and the competitor.**
  - If hipDF stalls (no functional commits since July 2026), the product stalls with it.
  - If AMD ports Sirius or a SQL layer itself (it has ported cuOpt, cuVS and cuGraph [3]), the product is redundant.
- **Sirius is a second risk.** If sirius-rocm finishes and is benchmarked, Sirius maintainers said they may host it under sirius-db [4].
- **Apple:** a hobby field (gpudb, duckdb-mlx, unified-db). Apple ships MLX and no database. The risk is DuckDB itself getting faster, not a GPU competitor.

## Funding story

- **What a design partner would say today:** nothing. None exists.
  - The likeliest "partner" is a neocloud (Hot Aisle, TensorWave). It might say: "we have MI300X hours idle between AI jobs; a SQL engine that uses them sells more hours."
  - That is a channel deal, not a user with a painful problem (estimate).
- **What a round could be raised on:** "the only GPU SQL engine on AMD, at 2.4× the HBM of an H100 for the same hourly price" (MI300X ~$1.7–2/hr at neoclouds against an H100 median of ~$2.9–3.3, aggregator figures: [amcompute](https://www.amcompute.com/gpu-cloud-prices/mi300x), [gpus.io](https://gpus.io/gpus/h100), accessed 2026-10-29).
  - That is a market-size and hardware-arbitrage argument, which the spec discounts.
  - An AMD ecosystem grant or partnership is plausible but unverified (estimate).
- **Apple:** no fundable story. It is a desktop convenience, and DuckDB on the CPU is the bar.

## Evidence against

- **Nobody with a workload asks.** There are 12 requests and 0 datasets, queries or budgets [D]. The most concrete AMD ask comes from a company that sells AMD hours [4].
- **On Apple, the CPU engine wins.** DuckDB beat MLX on every TPC-H query on an M4 [7]. Academic gains peak at 5× over a CPU baseline that is not DuckDB [8].
- **The cheap part is cheap because AMD did the expensive part.** That leaves the product with no moat. AMD, Sirius-on-ROCm, or anyone who recompiles a libcudf engine against hipDF reaches the same point.
- **The one public port stalled on infrastructure.** It spent three months, never ran a query, and was closed [4][5].
- **hipDF is a moving dependency trailing cuDF by about a year, with a quiet repo since July 2026** [1]. Pinning peacockdb to 25.10 forever forgoes NVIDIA's later Parquet and join work.
- **Intel and the AI chips are out entirely.** Intel has no datacenter GPU on sale. The AI chips have no relational substrate and a structural mismatch with hash joins [A][H].
- **For the decision:** this is a *portability feature*, not a product. It is worth about 0.1× once a real user on AMD hardware appears. It does not win a first user.

## Ratings

- **Who needs it badly:** weak — demand.md's rating. Twelve asks, no workload, no budget [D].
- **Build cost:** strong — AMD-only is about 0.5× because peacockdb pins the exact cuDF version hipDF ports. Metal adds about 1×. The AI chips are out of reach.
- **Data path:** medium — the same PCIe and decode wall as NVIDIA, minus GDS, plus 2.4× more HBM for a cache. Apple has no PCIe but low bandwidth and a strong CPU rival.
- **Adoption friction:** weak — few data teams hold AMD GPUs, there is no Spark plugin path, and Mac users already have DuckDB.
- **Platform risk:** medium — NVIDIA cannot follow, but AMD (hipDF's owner) or Sirius-on-ROCm can ship the same thing, and hipDF's pace is the product's pace.
- **Funding story:** weak — a hardware-arbitrage and channel story, with no named user in pain.

## Follow-up experiments

- Build peacockdb's `cpp/` against `amd-libhipdf` 3.0.0 on one rented MI300X for one day. Count the compile errors per entry point and run the C++ test suite. This settles the 0.05× adapt estimate.
- Run TPC-H SF40 end to end on MI300X against the H200 numbers in inventory.md (q6 48.2 ms, q19 321.9 ms). This settles whether hipCollections joins and wave64 lose.
- Measure hipDF `read_parquet` GB/s with Snappy and with ZSTD, against 6.8–8.7 GB/s on H200. This settles whether the AMD data path is worse.
- Check whether the `amd-libhipdf` wheel contains `libcudf.so` plus headers. If it does, the build-infrastructure risk largely disappears.
- On an M4 Max, run gpudb against DuckDB on TPC-H SF10 independently. This settles whether Metal gains survive outside self-reports.
- Ask Hot Aisle and TensorWave (one email each) whether any customer has asked for SQL or dataframes on MI300X. This is demand evidence, not a measurement, and it is the cheapest test of this option.

## Sources

- [D] demand.md `## Ratings` row other-accelerators and `## What people ask for`; [E] evidence-in-hand.md; [I] inventory.md `## Yardstick` and measured numbers; [S] sirius.md `## Sirius components`; [N] nvidia.md row other-accelerators and refs [47][72][74]; digests sources/hn-stackexchange.md, sources/issue-trackers.md, sources/reddit.md (all in llm-wiki/artifacts/product-research/, accessed 2026-10-29).
- [H] Desk research notes for AMD, Apple and Intel (`../notes/non-nvidia-gpu-sql-layers/amd.md`, `apple_intel.md`; claims carried here with their own links).
- [A] Desk research notes for AI accelerators (`../notes/non-nvidia-gpu-sql-layers/ai_accelerators.md`; claims carried here with their own links).
- Code counts: peacockdb master f36a68ec `cpp/src` and `cpp/include`, plus `third_party/cudf` @ 43505bb975f46ce4e140fdaf55192611d6e231ac (local, 2026-10-29).
1. hipDF repo and docs @ f59924fde8165ef59588abe65b5965336e28f99a: https://github.com/AMD-Ecosystem/hipDF ; https://github.com/AMD-Ecosystem/hipDF/blob/f59924fde8165ef59588abe65b5965336e28f99a/docs/what-is-hipdf.rst ; https://pypi.amd.com/rocm-7.2.3/simple/ (accessed 2026-10-29)
2. hipCOMP-core @ 22cc762f54fba7cdfca74a4c50c00f2aac4ace7a: https://github.com/ROCm/hipCOMP-core (accessed 2026-10-29)
3. ROCm-DS 26.03 and latest release notes: https://rocm.docs.amd.com/projects/rocm-ds/en/docs-26.03/about/release-notes.html ; https://rocm.docs.amd.com/projects/rocm-ds/en/latest/about/release-notes.html (accessed 2026-10-29)
4. Sirius #53, PR #1153 (head ce937de9ae3a90dbec60537288b7d19fb7493e78): https://github.com/sirius-db/sirius/issues/53 ; https://github.com/sirius-db/sirius/pull/1153 (accessed 2026-10-29)
5. Sirius #1158; sirius-rocm @ a24f2c0f1aa4d692d8b50b64426e6928eaee6a84; cuda2rocm @ 0b86950669129046d69c33e38a7ea7c96cdfee62: https://github.com/sirius-db/sirius/issues/1158 ; https://github.com/mycomputercrashsalot/sirius-rocm ; https://github.com/mycomputercrashsalot/cuda2rocm (accessed 2026-10-29)
6. gpudb DuckDB community extension (singhpratech/duckdbgpumetaldbram @ 525425ae2f6d4fd25be01c16db66b5ff4110357a): https://duckdb.org/community_extensions/extensions/gpudb (accessed 2026-10-29)
7. MLX TPC-H on M4, sadopc/unified-db-2 @ e725f29ed34187f261a1e35a3379723113d6037c: https://github.com/sadopc/unified-db-2/blob/main/PAPER.md ; https://news.ycombinator.com/item?id=47059629 (accessed 2026-10-29)
8. Beischl et al., "Evaluating Apple Silicon for Data Processing", DaMoN '26: https://db.in.tum.de/~beischl/papers/Evaluating_Apple_Silicon_for_Data_Processing.pdf (accessed 2026-10-29)
