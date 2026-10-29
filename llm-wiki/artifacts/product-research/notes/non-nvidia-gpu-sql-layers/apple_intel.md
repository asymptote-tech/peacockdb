# Non-NVIDIA GPU SQL layers: Apple Silicon (Metal/MLX), Intel (oneAPI/SYCL), portable layers — as of 2026-10-29

All URLs accessed 2026-10-29. HEAD SHAs were read with `git ls-remote` on 2026-10-29.

## Apple Silicon: GPU SQL/dataframe projects, results, unified memory

### Takeaway
Metal SQL work in 2026 consists of hobby or early-stage projects plus one academic paper (TUM, DaMoN '26). The only engine-level, broad-coverage project is a community DuckDB extension, `gpudb`, and its speed-ups (up to 48× at SF10 on an M4 Max) are self-reported. Independent and academic results are more modest: the paper measures up to 5× GPU over CPU, and on a base M4 DuckDB on the CPU beat hand-written MLX kernels on every TPC-H query. Unified memory removes explicit copies, but Metal buffer allocation still costs wiring and coherence passes. Framework-boundary copies (DuckDB to NumPy to MLX) dominate in practice.

### Cited Findings
**MLX TPC-H on M4 (HN Show HN 47059629)**
- The repo is sadopc/unified-db-2 (HEAD `e725f29ed34187f261a1e35a3379723113d6037c`), MIT-licensed. It compares six TPC-H queries (Q1, Q3, Q5, Q6, Q12, Q14) across DuckDB SQL, NumPy CPU kernels, and hand-written MLX GPU kernels — [HN](https://news.ycombinator.com/item?id=47059629); [PAPER.md](https://github.com/sadopc/unified-db-2/blob/main/PAPER.md) (accessed 2026-10-29)
- Hardware: base Apple M4 (10 GPU cores, 16 GB, 120 GB/s), macOS Sequoia, DuckDB 1.x — [PAPER.md](https://github.com/sadopc/unified-db-2/blob/main/PAPER.md) (accessed 2026-10-29)
- SF10 warm means in ms (DuckDB / NumPy / MLX):
  - Q1: 210 / 1,492 / 492
  - Q3: 84 / 437 / 462
  - Q5: 84 / 626 / 470
  - Q6: 26 / 122 / 56
  - Q12: 71 / 261 / 369
  - Q14: 53 / 110 / 119

  DuckDB was fastest on every TPC-H query at every SF (0.1, 1, 10). MLX beat NumPy on scan/aggregate queries (Q1, Q6) and lost on join-heavy ones (Q3, Q12) — [PAPER.md](https://github.com/sadopc/unified-db-2/blob/main/PAPER.md) (accessed 2026-10-29)
- The only MLX win over DuckDB was a custom join-free arithmetic query ("QX"), about 1.47× at SF10 (the HN post says 1.6×) — [PAPER.md](https://github.com/sadopc/unified-db-2/blob/main/PAPER.md); [HN](https://news.ycombinator.com/item?id=47059629) (accessed 2026-10-29)
- About 87% of transfer-pipeline time at SF10 is DuckDB→NumPy extraction, which is a framework-boundary cost, not the memory bus. `mx.array(numpy)` copies. PCIe overhead was modeled, not measured: about 18% mean (the HN post says 10–36%) — [PAPER.md](https://github.com/sadopc/unified-db-2/blob/main/PAPER.md) (accessed 2026-10-29)
- MLX limitations the author cites: no boolean indexing (they used an overflow-bin workaround), and float32 only (about 0.08% precision loss) — [HN](https://news.ycombinator.com/item?id=47059629) (accessed 2026-10-29)

**GPU-accelerated SQLite on Apple Silicon (r/programming 2026-01)**
- Reddit could not be fetched, because the fetch tool blocks reddit.com.
- The best candidate is sadopc/unified-db ("MetalSQLite", HEAD `dce19d2829aa91a4ed7da9e564fc345a8ad67263`): "Exploring Apple Silicon's unified memory architecture for GPU-accelerated SQLite queries" — [GitHub](https://github.com/sadopc/unified-db) (accessed 2026-10-29)
  - It is a Swift `MetalDatabase` API over SQLite storage, with Metal kernels for SUM/AVG/MIN/MAX/COUNT, bitonic sort and WHERE filter, using `storageModeShared` buffers. It is v0.1.0, and its install URL is a placeholder.
  - README numbers at 1M rows on an M4: SUM 45→8 ms (5.6×), AVG 5.3×, SORT 890→95 ms (9.4×), FILTER 6.9×. The CPU baseline is not specified (SQLite or custom) — [GitHub](https://github.com/sadopc/unified-db) (accessed 2026-10-29)

**DuckDB community extension `gpudb` (Metal + CUDA)**
- Maintainer singhpratech; repo singhpratech/duckdbgpumetaldbram, HEAD `525425ae2f6d4fd25be01c16db66b5ff4110357a` — [DuckDB community extensions](https://duckdb.org/community_extensions/extensions/gpudb) (accessed 2026-10-29)
- Operators: GROUP BY with sum, count, min, max, avg and count(DISTINCT); fused WHERE; HAVING; ORDER BY…LIMIT; inner, left, right and many-to-many joins; EXISTS, IN and scalar subqueries; CTEs and views. CPU fallback applies when the GPU is not measured to be faster — [same](https://duckdb.org/community_extensions/extensions/gpudb) (accessed 2026-10-29)
- Self-reported TPC-H results (build of 2026-09-20, N=5, row-exact against DuckDB):

  | Hardware | SF | Queries on GPU | execute() speed-up | sql() speed-up |
  |---|---|---|---|---|
  | M4 Max (Metal) | 1 | 17/22 | 1.52–15.32× | 1.37–9.49× |
  | M4 Max (Metal) | 10 | 19/22 | 1.06–48.10× | 0.92–26.39× (Q11 below parity) |
  | RTX 4090 Laptop (CUDA) | 1 | 17/22 | 1.21–31.44× | not covered |

  — [same](https://duckdb.org/community_extensions/extensions/gpudb) (accessed 2026-10-29)
- Not offloaded: window functions, FULL joins, median/stddev/quantiles, sum(DOUBLE) (order-dependent), prepared parameters, explicit transactions and correlated subqueries. Plain-SQL routing needs a Python shim, because "DuckDB's stable C API has no hook before planning" — [same](https://duckdb.org/community_extensions/extensions/gpudb) (accessed 2026-10-29)
- quackscience/duckdb-mlx ("GPU-accelerated DuckDB on Apple Silicon (Metal/MLX)", created July 2026, HEAD `196dd704dd1d0560e61ad6d7e632590d7de1f0d9`) also exists. I did not review its contents — [ecosyste.ms](https://awesome.ecosyste.ms/projects/github.com%2Fquackscience%2Fduckdb-mlx) (accessed 2026-10-29)

**Coco Alemana / DuckDB discussion #20558**
- The discussion was posted 2026-01-15 by prmelehan. It is a prototype `mlx_mean` DOUBLE aggregate via `AggregateFunction::UnaryAggregate`. The author's problem is that DuckDB's chunked, per-thread partial aggregation erodes GPU gains, and they ask how to get a stream at the aggregate point. The post has no benchmarks, the discussion is "Unanswered" with 0 replies, and no DuckDB maintainer stance is given — [GitHub discussion](https://github.com/duckdb/duckdb/discussions/20558) (accessed 2026-10-29)
- The page does not name "Coco Alemana". The link to that product rests on the coordinator's brief only.

**DataFusion/Arrow on Metal**
- `arrowmetal` 0.1.0 is Arrow compute on Apple GPUs for arrow-rs, with a Python wheel over libArrowMetalC.dylib via the Arrow C Data Interface. Its docs.rs build failed. Its README says one operation does not repay the import cost, so the design is "import once, chain, export once" — [docs.rs](https://docs.rs/crate/arrowmetal/0.1.0); [PyPI](https://pypi.org/project/arrowmetal/) (accessed 2026-10-29)
- I found no DataFusion Metal execution backend — [search result set incl. DataFusion site](https://datafusion.apache.org/blog/2024/05/07/datafusion-tlp) (accessed 2026-10-29)

**Academic: "Evaluating Apple Silicon for Data Processing" (Beischl, Morozov, Jungmair, Haussmann, Neumann; TUM; DaMoN '26)**
- Test hardware: M4 Pro (14 CPU / 20 GPU cores, 48 GB LPDDR5X-8533, $2,799) and M3 Ultra Mac Studio ($7,899) — [PDF](https://db.in.tum.de/~beischl/papers/Evaluating_Apple_Silicon_for_Data_Processing.pdf); [ACM DL](https://dl.acm.org/doi/10.1145/3789237.3809127) (accessed 2026-10-29)
- Measured bandwidth on M3 Ultra: the GPU reaches 90% of peak (738 GB/s), the CPU only 30% (253 GB/s), so the CPU-side interconnect is the limit. On M4 Pro the CPU and GPU reach similar throughput — [PDF](https://db.in.tum.de/~beischl/papers/Evaluating_Apple_Silicon_for_Data_Processing.pdf) (accessed 2026-10-29)
- Unified memory still has a cost. Shared buffers allocated via the Metal API trigger page wiring at 10–60 GB/s plus a coherence pass at 70–120 GB/s (M4 Pro). The authors avoid this by allocating one region once (24 GB) and managing it inside the DBMS. Metal also does not release memory promptly — [PDF](https://db.in.tum.de/~beischl/papers/Evaluating_Apple_Silicon_for_Data_Processing.pdf) (accessed 2026-10-29)
- Programming-model pain:
  - SIMD-group lockstep means conventional spin locks deadlock, so the authors had to design Metal-specific serial and non-blocking spin locks.
  - Coherence for device memory requires explicit opt-in.
  - Dispatch overhead is in the tens of µs (65–75 µs on M3 Ultra for one measured case).

  — [PDF](https://db.in.tum.de/~beischl/papers/Evaluating_Apple_Silicon_for_Data_Processing.pdf) (accessed 2026-10-29)
- On SSB (SF 1, 10, 50), GPU execution reaches up to 5× over their own CPU execution, and CPU+GPU co-processing can improve a further 44% in some queries. DuckDB was the baseline and was "significantly slower" than their compiled prototype — [PDF](https://db.in.tum.de/~beischl/papers/Evaluating_Apple_Silicon_for_Data_Processing.pdf) (accessed 2026-10-29)

**Hardware: bandwidth and capacity**
- M5 Max: up to 128 GB, about 614 GB/s. M4 Max: about 546 GB/s. M3 Ultra: about 819 GB/s, 512 GB max (secondary sources) — [Wikipedia M5](https://en.wikipedia.org/wiki/Apple_M5); [evetech](https://evezone.evetech.co.za/daily-drop/apple-m5-max-hits-614-gb-s-and-128gb-unified-memory-the-bandwidth-math-behind-fast-local-l) (accessed 2026-10-29)
- A search summary claims an M5 Ultra at 1.2 TB/s and 512 GB. Apple did not confirm this in any source I fetched — unverified, see Gaps.
- Base M4: 120 GB/s — [unified-db-2 PAPER.md](https://github.com/sadopc/unified-db-2/blob/main/PAPER.md). TUM quotes Apple's advertised range as about 120 to over 800 GB/s — [PDF](https://db.in.tum.de/~beischl/papers/Evaluating_Apple_Silicon_for_Data_Processing.pdf) (accessed 2026-10-29)

### Inferences
- Apple's ceiling (about 0.8 TB/s, up to 512 GB on Ultra) is roughly a quarter of an H100's HBM bandwidth, but a whole SF100+ dataset fits in memory with no PCIe. The win is capacity plus zero explicit copy, not raw bandwidth (estimate).
- Apple CPU+DuckDB is a strong baseline: CPU bandwidth on the M4 class is comparable to GPU bandwidth. A GPU layer must be a compiled or fused engine (as in TUM or gpudb) to beat it. Operator-at-a-time kernels via MLX lose.
- The market is thin. It consists of local-first analytics on Macs, there is no Apple server GPU in datacenters, and Metal is unavailable in any cloud except Mac hosting (estimate).

### Gaps
- I could not confirm the exact repo behind the Reddit r/programming post 1q6dj59, because Reddit was blocked. sadopc/unified-db is the likely match by subject and timing (estimate).
- I did not find the Polars or DuckDB official stance on Metal. Polars' GPU engine is cuDF-based, which makes it NVIDIA-only, but I have no fetched citation for that.
- I found no "cuDF for Mac" project beyond the items above.
- I did not verify M5 Pro/Max/Ultra specs on apple.com.
- gpudb benchmarks are self-reported and not independently reproduced.

## Intel: oneAPI/SYCL data analytics, hardware status

### Takeaway
Intel's GPU analytics story has mostly collapsed:
- Gazelle/OAP is archived.
- Modin no longer lists a GPU/HDK engine.
- Data Center GPU Max (Ponte Vecchio) is discontinued, Falcon Shores is cancelled, and Jaguar Shores and Crescent Island are AI-focused.

The live SYCL relational work is academic: SYCLDB (EURECOM/SYCLOPS) has on-par-with-CUDA results on NVIDIA, not Intel GPUs. The current Intel hardware is the Arc Pro B60 (24 GB, 456 GB/s, PCIe 5.0 x8).

### Cited Findings
- Data Center GPU Max 1550:
  - Specs: 128 GB HBM2e, 3,276.8 GB/s, 600 W, launched Q1'23 — [Intel ARK specs](https://www.intel.com/content/www/us/en/products/sku/232873/intel-data-center-gpu-max-1550/specifications.html) (accessed 2026-10-29)
  - Status conflict: the ARK specs page says "Launched", while the ordering page says "Retired and discontinued" — [Intel ordering](https://www.intel.com/content/www/us/en/products/sku/232873/intel-data-center-gpu-max-1550/ordering.html) (accessed 2026-10-29)
  - Intel support says the Max Series and its spares are discontinued and support is self-service — [Intel community](https://community.intel.com/t5/GPU-Compute-Software/Driver-issues-with-Intel-Datacenter-GPU-MAX-1550/m-p/1716116) (accessed 2026-10-29)
  - A tooling release (v1.3.3) deprecated Flex/Max support, per a secondary report — [whatpsu](https://fil.whatpsu.com/articles/2042-Intel-Discontinues-Ponte-Vecchio-and-Arctic-Sound-Data-Center-GPUs) (accessed 2026-10-29)
- Falcon Shores will be an internal test chip only. Jaguar Shores is the rack-scale successor and is AI-focused — [Tom's Hardware](https://www.tomshardware.com/tech-industry/artificial-intelligence/intel-cancels-falcon-shores-gpu-for-ai-workloads-jaguar-shores-to-be-successor) (accessed 2026-10-29)
- Crescent Island: Xe3P, 160 GB LPDDR5X, an inference GPU for air-cooled servers, with samples due 2H 2026 — [DCD](https://www.datacenterdynamics.com/en/news/intel-unveils-crescent-island-data-center-gpu-for-inferencing-workloads/) (accessed 2026-10-29)
- Arc Pro B60: 24 GB, up to 456 GB/s, PCIe 5.0 x8, partner-branded only — [Intel B60 datasheet 2026-03](https://www.intel.com/content/dam/www/central-libraries/us/en/documents/2026-03/datasheet-b60-gpu.pdf); [HotHardware](https://hothardware.com/news/intel-unveils-arc-pro-b60-and-b50-gpus) (accessed 2026-10-29)
- Dual-B60 48 GB cards do not pool memory — [TechRadar](https://www.techradar.com/pro/another-dual-intel-gpu-card-with-48gb-vram-launched-as-arc-pro-emerges-as-cheap-nvidia-and-amd-alternative-for-ai-crowd) (accessed 2026-10-29)
- Gazelle (OAP native Spark SQL plugin, Arrow/SIMD and CPU-only in practice): support ended February 2023, and the repo was archived 2023-09-18 (HEAD `fc828b5bec1b6a70985b7cb85c90fff192fdb45f`). Its lineage continued in Gluten/Velox on CPU — [ecosyste.ms](https://awesome.ecosyste.ms/api/v1/projects/github.com%2Foap-project%2Fgazelle_plugin) (accessed 2026-10-29)
- Modin: not archived (HEAD `7ca200b08597ed6ecfd2db2d08bd322f83c2cec1`). The README lists only the Ray, Dask and MPI/unidist engines, with no HDK/OmniSci GPU engine. I did not confirm when or why that engine was removed — [GitHub](https://github.com/modin-project/modin) (accessed 2026-10-29)
- SYCLDB (EURECOM, SYCLOPS EU project):
  - What it is: a SYCL library of relational operators, run one kernel per operator. Targets include the oneAPI Construction Kit, x86, RISC-V/RVV and NVIDIA GPUs. It is "on-par with CUDA counterparts" on NVIDIA. I found no Intel GPU results — [EURECOM](https://www.eurecom.edu/en/node/4899826); [oneapi.io talk](https://oneapi.io/event-sessions/sycldb-cross-architecture-acceleration-of-data-analytics-with-sycl-uxl-q324/) (accessed 2026-10-29)
  - Repo: SYCLOPS-Project-EU/sycldb (HEAD `0af5c549cafcddf498e0c483e20cce5e6ae704e0`, last push 2026-05-30, 0 stars) — [GitHub API](https://api.github.com/repos/SYCLOPS-Project-EU/sycldb) (accessed 2026-10-29)
  - Follow-ups: a 2026 arXiv paper on fusing and synthesizing kernels from the operator-at-a-time SYCLDB baseline, and a VLDB 2026 PhD workshop abstract that lists a SQL frontend as future work — [arXiv 2607.07632](https://arxiv.org/pdf/2607.07632); [VLDB PhD26_4](https://www.vldb.org/2026/Workshops/VLDB-Workshops-2026/PhD/PhD26_4.pdf) (accessed 2026-10-29)
- SYCLomatic (CUDA→SYCL migration tool) is still active: oneapi-src/SYCLomatic HEAD `43e270fdcaf6f9a9f57a7f87462e490d9aa1f029` — [GitHub](https://github.com/oneapi-src/SYCLomatic) (accessed 2026-10-29)
- HeavyDB is active (HEAD `72c6df9f1bce2cd5b849670b38c7e7e4592c564b`, pushed 2026-10-23). I did not verify whether it has an Intel GPU backend — [GitHub API](https://api.github.com/repos/heavyai/heavydb) (accessed 2026-10-29)

### Inferences
- There is no installed base of Intel datacenter GPUs to sell into. PVC is end-of-life and its successors are AI-only, so cloud availability of Intel GPUs for SQL is near zero (estimate).
- The B60 offers about 456 GB/s over PCIe 5.0 x8, roughly 32 GB/s each way. That is weaker than a used NVIDIA card for a PCIe-bound SQL workload (estimate).
- SYCL is viable as a source language: SYCLDB reached parity with CUDA on NVIDIA. However, nobody has shown competitive Intel GPU SQL numbers.

### Gaps
- oneDAL GPU status for relational or dataframe ops was not researched; oneDAL is ML-focused.
- Velox on Intel GPU: I found nothing.
- HeavyDB/OmniSci Intel GPU support (the earlier Intel-backed HDK/"OmniSciDB on Intel GPU" work) is unverified.
- Gaudi is irrelevant to SQL (no general relational kernels found); not researched further.

## Portable / vendor-neutral layers

### Takeaway
The only proven portable approach to SQL on any device is "SQL compiled to tensor programs" (TQP on PyTorch, PVLDB 2022). It ran the full TPC-H and was up to 10× faster than specialized CPU and GPU systems, but it was a Microsoft research prototype. SYCLDB is the main open SYCL relational library. I found no maintained Vulkan/WebGPU/Kokkos GPU dataframe library.

### Cited Findings
- TQP ("Query Processing on Tensor Computation Runtimes", He et al., PVLDB 15, 2022, Microsoft):
  - Compiles SQL to tensor programs and runs the full TPC-H.
  - Up to 10× over specialized CPU- and GPU-only systems, and up to 9× on ML+SQL queries.
  - Targets CPUs, discrete and integrated GPUs, TPUs and browsers.

  — [arXiv 2203.01877](https://arxiv.org/pdf/2203.01877) (accessed 2026-10-29)
- TQP companion demo "Share the Tensor Tea" (Spark SQL integration, PyTorch execution): VLDB 2022 best demo — [Microsoft Research](https://www.microsoft.com/en-us/research/?p=877143) (accessed 2026-10-29)
- SYCLDB: see the Intel section. It is portable across x86, RISC-V and NVIDIA via SYCL/OCK, and fusion and synthesis are ongoing research (arXiv 2607.07632) — [arXiv](https://arxiv.org/pdf/2607.07632) (accessed 2026-10-29)
- gpudb shows one codebase with both Metal and CUDA backends and a CPU fallback in a DuckDB extension. That is a practical portability pattern at the extension level — [DuckDB community extensions](https://duckdb.org/community_extensions/extensions/gpudb) (accessed 2026-10-29)

### Inferences
- For a small startup, the cheapest route to non-NVIDIA support is probably either of these (estimate):
  - a thin per-backend kernel set behind a CPU-fallback planner (the gpudb pattern), or
  - lowering to an existing tensor or compiler runtime (PyTorch/MLX/Triton), as TQP does.

  Rebuilding cuDF's breadth per vendor is not the cheap route.
- On Apple and Intel, the incumbent to beat is a CPU vectorized engine (DuckDB), not cuDF. Published GPU gains there are 1–5× (academic) to 1–48× (self-reported), which is a weak commercial wedge (estimate).

### Gaps
- Not researched within the tool budget:
  - Crystal (Shanbhag et al., SIGMOD 2020) portability
  - Voltron Theseus portability
  - ArrayFire as a relational substrate
  - Triton on AMD as a SQL route
  - Kokkos-, Vulkan- and WebGPU-based dataframe libraries (no hits surfaced)
- TQP has no public, maintained open-source repo that I verified (no SHA).
