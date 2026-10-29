# workstation-gpu-sql (+ dataframe-gpu)

## Verdict

- **What it is:** a SQL engine for one machine with one GPU, querying local Parquet. It ships as one small Linux wheel or binary with libcudf linked in, and its results load straight into Polars, pandas or R.
- **Who needs it:** pandas/Polars users and researchers on desk GPUs and DGX Spark. Their pain is installing RAPIDS, not query speed. Their demand is real ("strong" in demand.md), but it is aimed at dataframe APIs. Within months, DuckDB's own `INSTALL sirius` will serve the SQL part of it.
- **Budget:** fits on peacockdb at about 0.8× the engine (estimate). The win rests almost entirely on packaging and on desk-GPU speed, and NVIDIA and DuckDB can close both. The funding story is weak.

## Who needs it badly

- **demand.md rating: strong.** It counts R/U/C 9/21/55 and about 29 posters. Its own caveat: "the need is for dataframe APIs and easy installs, not SQL, and Sirius is entering DuckDB core" ([demand.md](../demand.md) `## Ratings`, accessed 2026-10-29).
- **Install pain is the biggest complaint group.** demand.md counts about 33 threads ([demand.md](../demand.md)). The named posters:
  - **u/tecedu's team** runs data jobs on in-house GPU boxes. "Willing to pay someone to port over cudf to work with windows" ([r/Python](https://www.reddit.com/r/Python/comments/1fj0kfi/comment/lnrwn9g/), 2024-09; [r/gadgets](https://www.reddit.com/r/gadgets/comments/1gywo4t/comment/lyyif4w/), 2024-11; accessed 2026-10-28).
  - **Readers of NVIDIA's cudf.pandas launch post** ask for native Windows without WSL ([forum](https://forums.developer.nvidia.com/t/rapids-cudf-accelerates-pandas-nearly-150x-with-zero-code-changes/272201), 2023-11, accessed 2026-10-28).
  - **AI Workbench user:** an empty RAPIDS container is 20 GB ([forum](https://forums.developer.nvidia.com/t/can-the-rapids-image-go-on-a-diet-20gb-image/297463), 2024-06, accessed 2026-10-28).
  - **rocker-org JupyterHub (Carl Boettiger's group):** the RAPIDS image is "over 40GB, too big to … build on GH Actions" ([Jupyter](https://discourse.jupyter.org/t/minimal-image-compatible-with-nvidia-rapids/34116), 2025-03, accessed 2026-10-28).
  - **Python lag:** users found no cuDF for Python 3.11 ([PyTorch forum](https://discuss.pytorch.org/t/cudf-for-python-3-11/189225), 2023-10) or 3.12 ([PyTorch forum](https://discuss.pytorch.org/t/dataframe-kindo-of-object-in-pytorch/202686), 2024-05) (accessed 2026-10-28).
  - **DGX Spark ARM confusion:**
    - An enterprise buyer believed cuDF did not run on GB10 ([forum](https://forums.developer.nvidia.com/t/rapids-cudf-preview-build-request-for-grace-blackwell-gb10-dgx-aarch64-kernels-missing/352629), 2025-11).
    - Another buyer chose hardware on the belief that RAPIDS does not run on ARM ([forum](https://forums.developer.nvidia.com/t/should-i-buy-asus-gx10-instead-nvidia-dgx-spark/347717), 2025-10).
    - An NVIDIA blueprint container is x86-only ([forum](https://forums.developer.nvidia.com/t/nvidias-financial-fraud-training-container-is-only-built-for-x86-64/352807), 2025-11) (all accessed 2026-10-29).
  - **Old hardware locked out:** "Pity it requires Volta 7 … for fiddling around on at home" ([HN 44410362](https://news.ycombinator.com/item?id=44410362), 2025-06, accessed 2026-10-28).
- **Happy users on one box, with dataframes:**
  - xs83: 500 GB on a 6 GB GTX 1060 ([HN 40560169](https://news.ycombinator.com/item?id=40560169), 2024-06).
  - u/Wackome: 70× on an A5000 laptop ([r/mac](https://www.reddit.com/r/mac/comments/1hdxh51/), 2024-12).
  - u/thegratefulshread: about 50 GB on an RTX 3080 Ti ([r/macbookpro](https://www.reddit.com/r/macbookpro/comments/18cczma/comment/kcaebgz/), 2023-12).
  - fmoralesh: 1.2 TB of logs on a V100 ([r/dataengineering](https://www.reddit.com/r/dataengineering/comments/1kw9ku2/comment/mugm06f/), 2025-05).
  - Accessed 2026-10-28.
- **Fit and speed complaints:**
  - A 6 GB laptop runs out of memory on a 1 GB CSV ([r/CUDA](https://www.reddit.com/r/CUDA/comments/1iena2p/), 2025-01).
  - "Could not get cuDF to work with larger than memory" data ([r/datascience](https://www.reddit.com/r/datascience/comments/1fj0nmt/comment/lnl3cw9/), 2024-09).
  - "Bought a Nvidia GPU … performance is awful" on small frames ([SO 78626099](https://stackoverflow.com/q/78626099), 2024-06).
  - A small Polars join took 0.1 s on CPU and 7 s on GPU ([polars #20696](https://github.com/pola-rs/polars/issues/20696), 2025-01).
  - Accessed 2026-10-28.
- **SQL-specific desk-GPU pain:**
  - Sirius was slower than DuckDB on all 22 TPC-H queries on GB10, RTX 5090 and A6000 ([sirius #609](https://github.com/sirius-db/sirius/issues/609), 2026-04).
  - Silent CPU fallback ([sirius #2174](https://github.com/sirius-db/sirius/issues/2174), 2026-10).
  - u/No_Mongoose6172 outgrew one DuckDB node and wants R + CUDA on a few workstations with no Docker ([r/dataengineering](https://www.reddit.com/r/dataengineering/comments/1o6bnir/), 2025-10).
  - Accessed 2026-10-29.
- **Best first conversations** (from [demand.md](../demand.md)):
  - Carl Boettiger (UC Berkeley): GPU JupyterHub images, and a public GPU-vs-DuckDB record ([#227](https://github.com/boettiger-lab/mcp-data-server/issues/227)).
  - u/tecedu.
  - Jeff Levesque: Spark + RAPIDS on 2× DGX Spark ([forum](https://forums.developer.nvidia.com/t/kernel-7-0-0-1019-nvidia-makes-apache-spark-rapids-gpu-work-1-5-2-4x-slower-on-dgx-spark-6-17-0-1032-with-the-same-driver-580-178-04-is-fine/386791)).
  - u/fmoralesh.
  - DGX Spark owners on the NVIDIA forum.
  - Accessed 2026-10-29.
- **Caveat:** no poster names a budget. None asks for SQL in particular. Only tecedu offers to pay, and that offer is for Windows.

## How it would work

- **Front doors:**
  - a CLI: `peacock query 'SELECT …' --data ./parquet/`;
  - a Python module: `peacock.sql(q)` returns Arrow, so `pl.from_arrow` and pandas cost nothing;
  - later, an ADBC/Flight SQL endpoint for R and BI tools.
- **No dataframe API of its own.** Polars and pandas users call SQL and get frames back.
- **Planner:** DataFusion 45, translated into peacockdb's node set and FlatBuffers recipe, as today ([inventory.md](../inventory.md) "Planner", peacockdb `f25c250d`).
- **Executor:** the C++/cuDF session behind the 25-symbol C ABI. It is linked **statically** into one `.so` together with libcudf, RMM, kvikio and the static cudart. Only `libcuda.so` comes from the user's driver.
- **Data path, end to end:**
  1. NVMe → page cache or O_DIRECT.
  2. Host buffer, pinned once.
  3. H2D over PCIe, or zero-copy on GB10.
  4. `chunked_parquet_reader` decodes on the GPU.
  5. cuDF operators run, streaming batches per lane.
  6. Arrow C stream back to the host.
- **Hot columns** stay in a pinned host-RAM cache (tier 1) or in VRAM (tier 2) across queries.
- **Whole-query CPU fallback** goes through the existing DataFusion CPU backend, and every query reports where it ran. That is the answer to the silent-fallback complaints.
- **A size gate** sends small queries (under about 1 GB scanned, estimate) to the CPU. That answers polars #20696 and SO 78626099.
- **The Sirius variant:** ship a curated DuckDB + Sirius bundle (pinned DuckDB revision, right-sized backend, defaults for small VRAM). DuckDB keeps the planner, and Sirius keeps the executor.

## The human's questions

**1. Who would install this instead of DuckDB/Polars on CPU, or cudf.pandas/cudf-polars, and why?**
- **Compared with DuckDB or Polars on CPU:** only people with a ≥16 GB NVIDIA GPU, 10–200 GB of local Parquet, and join- or group-by-heavy queries that take seconds. Below that size the CPU wins. A desk CPU is cheap, and DuckDB 2.0 scales to 128+ vCPUs ([HN 50181617](https://news.ycombinator.com/item?id=50181617), accessed 2026-10-29).
  - The concrete people: research groups on GPU JupyterHubs (Boettiger), DGX Spark owners who bought the GPU for AI and have Parquet beside it (Levesque), and teams that already run GPU boxes for data (tecedu, fmoralesh).
- **Compared with cudf-polars:** people whose work is SQL (analysts, R users, dbt-style scripts), and people blocked by cudf-polars today:
  - **Install size.** `polars[gpu]` downloads 2.69 GB on CUDA 12 and 1.73 GB on CUDA 13. About 1.5 GB of the CUDA 12 figure is cuBLAS/cuSOLVER/cuFFT pulled in through `cupy[ctk]` ([PyPI cudf-polars-cu12](https://pypi.org/pypi/cudf-polars-cu12/json), [cupy-cuda12x](https://pypi.org/pypi/cupy-cuda12x/json), accessed 2026-10-29).
  - **It does not install against current Polars.** cudf-polars 26.10 pins `polars<1.45`, while PyPI Polars is 2.1.0. Polars 2 support lands on 26.12 ([cudf #24513](https://github.com/NVIDIA/cudf/issues/24513), [PyPI polars](https://pypi.org/pypi/polars/json), accessed 2026-10-29). This is temporary.
  - **SQL gaps on the GPU:**
    - `NOT IN (subquery)` is unsupported ([cudf #24761](https://github.com/NVIDIA/cudf/issues/24761)).
    - Inequality joins return **wrong results** ([cudf #24693](https://github.com/NVIDIA/cudf/issues/24693)).
    - A Polars 2.1 pattern crashes instead of falling back ([cudf #24843](https://github.com/NVIDIA/cudf/issues/24843)).
    - Accessed 2026-10-29.
  - **Silent CPU fallback** is the default, because `raise_on_fail` is False ([config.py, release/26.10](https://github.com/NVIDIA/cudf/blob/release/26.10/python/cudf_polars/cudf_polars/utils/config.py), accessed 2026-10-29).
- **Compared with cudf.pandas:** almost no one. Its users want zero code changes in pandas, and SQL is a code change.
- **Windows users (tecedu):** not in the alpha. No libcudf-based engine runs natively on Windows: not cuDF, Sirius, HeavyDB or PG-Strom ([RAPIDS platform support](https://docs.nvidia.com/datascience/platform-support/); [HeavyDB build docs](https://docs.nvidia.com/heavyai/installation-and-configuration/installation/build-from-source), accessed 2026-10-29). Porting libcudf is out of budget (estimate). WSL2 works for Sirius on an RTX 4060, with a manual memory setting ([sirius #1873](https://github.com/sirius-db/sirius/issues/1873), accessed 2026-10-29).
- **Bottom line:** the group that would switch is small. They would switch for "it installed in one minute, ran on my GPU, and told me so", not for raw speed.

**2. Does a SQL engine or a dataframe API win this user?**
- **The dataframe API wins the user.** demand.md: "The happiest users do not want SQL. They use pandas/Polars or hand-written CUDA." The posters above all use cuDF, dask_cudf or Polars.
- **SQL wins the build.** peacockdb is a SQL engine. A Polars-compatible GPU engine means rebuilding cudf-polars, which NVIDIA staffs and which already passes 88.8% of Polars tests without fallback (cudf-polars notes; [cudf-polars docs](https://docs.nvidia.com/cudf/latest/cudf_polars/), accessed 2026-10-29).
- **The compromise:** SQL in, Arrow/Polars out, plus `pl.SQLContext`-style registration of LazyFrames as tables (estimate: about 0.3k lines). This reaches Polars users who already write `pl.sql`, but not the rest.
- **Strongest version:** "the GPU SQL engine that is honest about where it ran and fits in a small wheel". Not "GPU Polars".

**3. What is the smallest packaging that removes the install pain?**
- **One manylinux_2_28 wheel per CPU arch (x86_64, aarch64).** It holds a single `.so` with libcudf, RMM, kvikio, nvCOMP and cudart linked statically, so there is no CuPy, no math libraries, no conda and no toolkit.
  - Sirius already proves the static link: its distribution build bundles cuDF/cuVS/cuCascade, and "only NVIDIA driver and standard Linux libraries remain external" ([sirius-duckdb/README.md @ b9e05ff](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/sirius-duckdb/README.md), accessed 2026-10-29).
  - libcudf supports `BUILD_SHARED_LIBS=OFF`, and cudf-java ships it that way ([cudf #23419](https://github.com/NVIDIA/cudf/issues/23419); regression [#23575](https://github.com/NVIDIA/cudf/issues/23575), accessed 2026-10-29).
- **Build on CUDA 13 only, which needs driver ≥ 580.**
  - SASS for sm_75 (RTX 20/T4), sm_86 (RTX 30/A6000), sm_89 (RTX 40/L4) and sm_120 (RTX 50; GB10 runs the sm_120 code), plus compute_120 PTX for newer parts.
  - This drops Volta and Pascal. Pascal (GTX 10) is still 5.4% of Steam GPUs, but RAPIDS and Sirius already exclude it ([Steam survey](https://store.steampowered.com/hwsurvey/videocard/); [RAPIDS install](https://docs.nvidia.com/datascience/install/), accessed 2026-10-29).
  - CUDA 13 and the 580 driver floor follow minor-version compatibility. Forward-compat packages do not cover GeForce ([NVIDIA minor-version compat](https://docs.nvidia.com/deploy/cuda-compatibility/minor-version-compatibility.html); [forward compat](https://docs.nvidia.com/deploy/cuda-compatibility/forward-compatibility.html), accessed 2026-10-29).
- **Size estimate:**
  - libcudf-cu13's `libcudf.so` is 369.7 MB for six SASS targets, of which the fatbin is 272.8 MB (cudf-polars notes; [PyPI libcudf-cu13 26.10.2](https://pypi.org/pypi/libcudf-cu13/26.10.2/json), accessed 2026-10-29).
  - peacockdb uses a fraction of libcudf's kernels, but a static link keeps whatever the linker cannot drop.
  - Estimate: a **250–400 MB uncompressed, 120–200 MB compressed** wheel for four archs. That is about 10× smaller than `polars[gpu]`, 5–8× smaller than a Sirius backend (0.92–1.54 GB gzipped) ([sirius releases](https://github.com/sirius-db/sirius/releases), accessed 2026-10-29), and about 25× smaller than `rapidsai/base` (4.1–6.0 GB compressed) ([Docker Hub](https://hub.docker.com/v2/repositories/rapidsai/base/tags), accessed 2026-10-29).
  - Split per arch (`peacock-sm89`) and each download falls to about 80–120 MB (estimate).
- **Python:** one abi3 wheel covers 3.10–3.14, through PyO3 + Arrow C data. That ends the Python-version lag complaint.
- **A static CLI binary** comes from the same link. It is Linux only: glibc ≥ 2.28 and a driver ≥ 580.
- **Windows:** WSL2 only, documented. A native Windows build is not possible on libcudf. It would need a non-cuDF executor such as craton-bolt's driver-API PTX approach ([craton-bolt](https://github.com/craton-co/craton-bolt), accessed 2026-10-29). That is a different product, over 2× the budget (estimate).
- **Kernel requirements:** avoid them. Sirius needs io_uring and O_DIRECT ([docs/README.md @ b9e05ff](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md)); use plain reads plus pinned buffers.

**4. How does it compete with Sirius becoming a DuckDB core extension, and with cudf-polars?**
- **Status of [duckdb #28022](https://github.com/duckdb/duckdb/pull/28022)** (accessed 2026-10-29):
  - It is **open, with no reviews**. +170/−18 lines, labelled "backport v2.0-cyanoptera".
  - It adds only a CUDA-free `sirius` **loader** to core, on Linux glibc amd64/arm64. The loader reads the driver's CUDA version and runs `INSTALL AND LOAD sirius_cuda12|sirius_cuda13 FROM community`.
  - The GPU backends ship as **community** extensions. The draft community entry builds `cuda_archs "80-real"` only, and is marked "DO NOT MERGE" ([community-extensions #3447](https://github.com/duckdb/community-extensions/pull/3447), accessed 2026-10-29).
  - The CI images (CUDA 12.9/13.4) are merged ([duckdb-ci #86](https://github.com/duckdb/duckdb-ci/pull/86); [extension-ci-tools #458](https://github.com/duckdb/extension-ci-tools/pull/458), accessed 2026-10-29).
  - The docs page says "coming soon" ([duckdb-web sirius.md](https://github.com/duckdb/duckdb-web/blob/main/docs/2.0/core_extensions/sirius.md), accessed 2026-10-29).
- **What it means:** in DuckDB 2.0, `INSTALL sirius; LOAD sirius;` inside the DuckDB that users already run is likely weeks to months away (estimate). Its first-load download is about 1 GB. **This removes most of the install pain for SQL users, from the incumbent, at zero adoption cost.**
- **Where a separate product could still win today (each is thin):**
  - **Desk-GPU speed.** #609 found Sirius slower than DuckDB on GB10, RTX 5090 and A6000. That was never re-measured. The RMM bug behind part of it is fixed in cuDF 26.06+, and the issue was closed by NVIDIA ([#609](https://github.com/sirius-db/sirius/issues/609), accessed 2026-10-29). peacockdb has no desk-GPU numbers either. **Unknown.**
  - **Honest fallback.** #2174 is open, and the fix, PR #2175, is unmerged ([#2174](https://github.com/sirius-db/sirius/issues/2174), accessed 2026-10-29). A small fix.
  - **Download size:** about 150 MB against about 1 GB (estimate). It matters on JupyterHub images and CI, but less once the backend is cached.
  - **Defaults:** Sirius claims 95% of VRAM and 90% of RAM pinned by default ([sirius.md](../sirius.md)). That is hostile on a shared desktop, and a config change fixes it.
  - **Coverage:** peacockdb runs TPC-DS 82/99 on device against Sirius's ~72 ([sirius.md](../sirius.md)). DuckDB falls back to the CPU anyway, so the user still gets an answer.
- **cudf-polars:** NVIDIA staffs it. Polars 2 support and the SQL bugs above are on the 26.12 line, with an open story to run all Polars TPC-H SQL ([cudf #24756](https://github.com/NVIDIA/cudf/issues/24756), accessed 2026-10-29). Its weakness, install size, comes from RAPIDS's packaging (CuPy math libraries). NVIDIA can fix that, and the CUDA 13 wheels are already 35% smaller.
- **Verdict on competition:** both incumbents own the user's existing tool. peacockdb's edges are a smaller binary and fallback you can see. Upstream can copy both in one release (estimate).

## Data path and cache

- **Bandwidth on a workstation** (vendor specs, estimate unless cited):
  - NVMe Gen4: about 7 GB/s; Gen5: about 12–14 GB/s.
  - PCIe 4.0 x16 H2D: about 25 GB/s pinned. PCIe 5.0: about 50 GB/s.
  - H200 measured: pageable 10.6 GB/s, pinned 47.3 GB/s ([inventory.md](../inventory.md) "Benchmark evidence").
  - GDDR: about 1.0 TB/s on an RTX 4090, about 1.8 TB/s on an RTX 5090. H200 HBM was measured at 4,484 GB/s (inventory.md).
- **Decode:** cuDF Parquet materialization ran at 6.8–8.7 GB/s on H200. Page decode plus Snappy was 80% of kernel time on q11v (inventory.md). A consumer GPU is likely slower (estimate).
  - On GB10, uncompressed Parquet read at 7.42 GB/s against Snappy's 5.02 GB/s ([dgx-spark.md](../../../reports/dgx-spark.md) §10, peacockdb `c7c8568e`).
- **GB10 (DGX Spark):**
  - one memory pool;
  - `cudaMemcpy` about 59 GB/s;
  - kernels read mapped host memory at 205–219 GB/s, against 239 GB/s for device memory;
  - page cache delivers 23.6 GB/s;
  - the chunked reader loads q6 in 61 ms, against 340 ms for `read_parquet` (dgx-spark.md §1–2).
- **The thesis holds for a desk box, cold.**
  - The chain is NVMe 7 GB/s → decode about 5–8 GB/s → compute at 1+ TB/s. A cold scan runs at disk speed.
  - DuckDB on a 16-core desktop reads Parquet at a few GB/s too (estimate). So on cold queries the GPU wins only where compute per byte is high (big joins, high-cardinality group-bys).
  - The human's SF20 PoC spent more time loading than computing on 11 of 12 queries ([evidence-in-hand.md](../evidence-in-hand.md)).
- **The cache this product needs:**
  1. **OS page cache plus a pinned host staging pool.** This is the cheap win: 4.4× over pageable on discrete GPUs (#149). On GB10 pinning must be off (dgx-spark.md §1).
  2. **A decoded-column cache in pinned host RAM.** A workstation has 64–256 GB of RAM against 8–32 GB of VRAM. Re-reading it over PCIe 4.0 costs about 25 GB/s with no decode. That is 3–4× better than the cold path, and enough for repeated interactive queries (estimate).
  3. **A VRAM pin for the hottest columns only.** VRAM is too small to hold the data set. It is also shared with the user's display and any local LLM.
- **Without tier 2**, repeated queries pay the decode every time, and a tuned DuckDB stays close. **With it**, the case looks like Sirius's `pin_table`, which already exists ([sirius.md](../sirius.md) "Cache in RAM and VRAM").
- **Out of core:** desk VRAM (8–32 GB) is below the data size (10–200 GB). So streaming or spill decides whether queries run at all, not only how fast. peacockdb has no spill today (inventory.md "Spilling").

## Build cost to alpha

| Component | On peacockdb: reuse/adapt/new, size | On Sirius: reuse/adapt/new, size |
|---|---|---|
| SQL frontend + planner | reuse DataFusion 45 + translator, 0 | reuse DuckDB, 0 |
| Node set, wire, driver, cuDF operators | reuse, 0 | reuse Sirius engine, 0 |
| Product entry: CLI, PyO3 module, Arrow out, table registration over dirs/globs | new, 1.0–1.5k | adapt: Python wheel of the pinned DuckDB revision, 0.3–0.5k |
| Whole-query CPU fallback + "ran on GPU/CPU" report | adapt the existing CPU backend, 0.3–0.8k | adapt: surface fallback, fix #2174, 0.2–0.4k |
| Small-query CPU gate | new, 0.3–0.6k | new in a planner hook, 0.3–0.6k |
| Chunked reader, pinned buffers, RMM pool, memory limit (#232, #149, #148) | adapt, 0.5–1.5k | reuse, 0 |
| Host + VRAM column cache | new, 2–4k | reuse `pin_table`, plus auto-pin policy 0.5–1k |
| Out-of-core: partitioned join/agg/sort with host spill (cuCascade) | new, 5–10k | reuse (young), 0 |
| Window functions (15 TPC-DS queries) | new, 1.5–3k | upstream gap (#1745), 1–2k |
| Static link, fat binary sm_75/86/89/120, manylinux wheels, CUDA 13 CI | new, 0.5–1k + build infra | adapt: Sirius already links statically; re-slice per arch, 0.2–0.5k + infra |
| Desk-GPU tuning (small VRAM, shared display) | new, 0.5–1k | adapt defaults, 0.3–0.6k |
| Total | ≈ 12–23k | ≈ 3–6k |

Total new work: 0.8× engine on peacockdb, 0.2× on Sirius (estimate)

- **The hard part on peacockdb is out-of-core execution on 8–32 GB of VRAM.** A desk user's data does not fit, and peacockdb fails any query that overflows (inventory.md, #142). Adopting cuCascade (sirius.md follow-up 4) is the unproven step.
- **The second hard part is the static libcudf build.** The upstream static build has an open regression ([cudf #23575](https://github.com/NVIDIA/cudf/issues/23575)). Getting it under 200 MB and keeping it green on every RAPIDS bump is ongoing CI work, not code lines.
- **On Sirius the lines are few, but the product is a repackaging of an extension.** DuckDB and NVIDIA are packaging that same extension right now (#28022, community-extensions #3447). The work is cheap and leaves nothing to own.

## Adoption friction

- **On peacockdb:** a new tool, even if small. Users must move their SQL from DuckDB or Polars into a third engine with DataFusion's dialect. `pip install` of one wheel is the best case, against the 2.7 GB of `polars[gpu]`.
- **On Sirius:** zero friction once DuckDB ships the loader. That is exactly why a third-party Sirius bundle has no place to stand.
- **Trust:** demand.md counts five silent-fallback threads. A per-query "GPU/CPU, and why" line is cheap and is a real differentiator today.
- **Platform reach:**
  - Linux plus WSL2 only.
  - The Steam survey shows RTX 50 at about 21.6% of GPUs, so the sm_120 target matters ([Steam](https://store.steampowered.com/hwsurvey/videocard/), accessed 2026-10-29).
  - About 59% of developers use Windows personally (2024 SO survey via [commandlinux](https://commandlinux.com/statistics/developer-os-preference-stack-overflow-survey/), secondary, unverified, accessed 2026-10-29). WSL2 adds friction for most of them.

## Platform risk

- **Very high.** demand.md and discovery.md flag it: "Sirius is joining DuckDB core."
- **NVIDIA:**
  - staffs Sirius (≈92% of human commits), cudf-polars, cudf.pandas and RAPIDS packaging ([sirius.md](../sirius.md); [nvidia.md](../nvidia.md));
  - publishes GQE, a DataFusion → libcudf twin of peacockdb ([nvidia.md](../nvidia.md));
  - ships pip wheels every two months, and RAPIDS 26.12 drops Python 3.11 ([RAPIDS notices](https://docs.nvidia.com/datascience/notices/), accessed 2026-10-29).
- **libcudf is a moving base.** A static build pins one RAPIDS version. Bumps break APIs, with no stability policy (nvidia.md). nvCOMP is closed source on the hot path (nvidia.md).
- **The packaging edge decays:**
  - CUDA 13 wheels are already 35% smaller than CUDA 12.
  - Sirius's static extension shows NVIDIA can ship single-file builds.
  - A conda/rattler split is planned ([sirius #1734](https://github.com/sirius-db/sirius/issues/1734), accessed 2026-10-29).
  - If RAPIDS drops the `cupy[ctk]` math libraries from the cudf-polars dependency chain, its footprint halves (estimate).

## Funding story

- **What a design partner would say:**
  - Boettiger: "Smaller images would help my JupyterHub, but I'll use DuckDB + Sirius when it lands."
  - tecedu: "Does it run on Windows?" The answer is no.
  - A DGX Spark owner: "cudf-polars already works on 25.10+."
  - None of them is a budget holder (demand.md "Buyers are missing").
- **What the round would be raised on:** "the easiest GPU analytics for a desk GPU and DGX Spark: one wheel, honest fallback, fast on consumer cards", backed by a measured desk-GPU win over DuckDB and Sirius.
  - Without that number, the pitch is a packaging improvement on free NVIDIA software, which investors will see as a feature.
  - A paid tier has no clear buyer. Local tools are free, and the history (BlazingSQL users did not pay) is against it ([HN 40079813](https://news.ycombinator.com/item?id=40079813), accessed 2026-10-28).
- **Best use:** as a distribution channel, the free local entry point, for a later paid product (lean or gpu-bi-dashboards). Not as the company.

## Evidence against

- **The incumbent ships the SQL version first.** DuckDB's loader plus community GPU backends gives DuckDB users GPU SQL in the tool they already run. NVIDIA maintains the backends (#28022, community-extensions #3447).
- **The loud pain is not about SQL.** It is about pandas/Polars installs and Windows. A Linux-only SQL engine fixes neither.
- **Windows, the one thing someone offered to pay for, is impossible on libcudf** within budget.
- **GPU loses to a tuned CPU at desk sizes:**
  - Sirius was slower than DuckDB on RTX 5090, A6000 and GB10 (#609).
  - Boettiger's GPU path lost by about 4× (demand.md).
  - Small Polars joins are 70× slower on GPU (#20696).
  - A desk box is NVMe-bound on cold reads.
- **VRAM is small and shared** (display, local LLMs). So the product lives or dies on spilling, which peacockdb lacks entirely and Sirius has only had for about 10 months.
- **The packaging moat is a build flag.** Sirius already links statically, and CUDA 13 already cut the RAPIDS wheels by 35%.
- **cudf-polars has the happy users,** a 23× multi-GPU story ([pola.rs](https://pola.rs/posts/gpu-streaming-backend), accessed 2026-10-29) and NVIDIA staff. The happiest group in the corpus would have to switch from their dataframe API to SQL.
- **No one in the corpus names a budget or a deadline.**

## Ratings

- **Who needs it badly: strong.** demand.md rates it strong: about 29 posters, and tecedu "willing to pay". The need is for easy dataframe installs, and SQL is secondary.
- **Build cost: medium.** About 0.8× on peacockdb fits the budget. Out-of-core on small VRAM and a static libcudf build are both unproven.
- **Data path: medium.** A cold desk box is NVMe/decode-bound. A pinned host-RAM column cache recovers 3–4× (estimate). VRAM is too small to hold the data.
- **Adoption friction: weak.** A third engine against `INSTALL sirius` in DuckDB and `pip install polars[gpu]`. Linux/WSL2 only.
- **Platform risk: weak (high risk).** NVIDIA and DuckDB are shipping this exact packaging now (#28022). cudf-polars is NVIDIA's.
- **Funding story: weak.** A packaging edge on free NVIDIA software, with no budget-holding user. It works as a funnel, not a company.

## Follow-up experiments

1. **Desk-GPU head-to-head.** peacockdb, DuckDB + Sirius `latest` and DuckDB CPU on an RTX 4090/5090 (24–32 GB) and on GB10. TPC-H SF10/SF50, cold and warm. This settles whether a desk-GPU speed edge exists at all (#609 never gave numbers).
2. **Static-link size.** Build peacockdb's executor against static libcudf 26.10 for sm_75/86/89/120 and measure the `.so` and wheel size. This tests the 120–200 MB claim against Sirius's ~1 GB.
3. **Host-cache payoff on PCIe 4.0.** Decoded columns in pinned RAM against a cold NVMe read, per query. This tests the 3–4× estimate.
4. **Data 2–5× VRAM** on a 16 GB card. Sirius with spill against cudf-polars streaming. This shows whether out-of-core works on desk VRAM, which decides the build cost.
5. **cudf-polars SQL coverage** on the Polars TPC-H/TPC-DS SQL set, against peacockdb's 82/99. This tells whether a coverage edge exists once 26.12 lands.
6. **Talk to Boettiger and tecedu.** Would a ~150 MB Linux wheel with GPU SQL change their setup, or would they wait for DuckDB's `INSTALL sirius`? Not a measurement, but it decides the option.

## Sources

Local (peacockdb repo; accessed 2026-10-29): [demand.md](../demand.md), [discovery.md](../discovery.md), [inventory.md](../inventory.md) (master `f25c250d`), [sirius.md](../sirius.md), [nvidia.md](../nvidia.md), [evidence-in-hand.md](../evidence-in-hand.md), [sources/](../sources/), `llm-wiki/reports/dgx-spark.md` (peacockdb `c7c8568e`).

DuckDB / Sirius (accessed 2026-10-29):
- https://github.com/duckdb/duckdb/pull/28022 , https://github.com/duckdb/duckdb/pull/28022/files
- https://github.com/duckdb/community-extensions/pull/3447 , https://github.com/duckdb/duckdb-ci/pull/86 , https://github.com/duckdb/extension-ci-tools/pull/458
- https://github.com/duckdb/duckdb-web/blob/main/docs/2.0/core_extensions/sirius.md
- https://github.com/sirius-db/sirius/blob/bb247c9dbb67739d125fc65149085bf0701d4852/sirius-duckdb/loader/src/sirius_loader.cpp (loader, SHA bb247c9)
- https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/sirius-duckdb/README.md , https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md (SHA b9e05ff)
- https://github.com/sirius-db/sirius/releases , https://github.com/sirius-db/sirius/pull/1973
- https://github.com/sirius-db/sirius/issues/609 , /2174 , /1873 , /1728 , /1734 , https://github.com/sirius-db/sirius/pull/2175 , https://github.com/sirius-db/sirius/pull/2398

RAPIDS / cudf-polars (accessed 2026-10-29):
- https://pypi.org/pypi/cudf-polars-cu12/json , https://pypi.org/pypi/cudf-polars-cu13/json , https://pypi.org/pypi/libcudf-cu13/26.10.2/json , https://pypi.org/pypi/libcudf-cu12/26.10.2/json , https://pypi.org/pypi/cupy-cuda12x/json , https://pypi.org/pypi/polars/json
- https://hub.docker.com/v2/repositories/rapidsai/base/tags
- https://docs.nvidia.com/datascience/install/ , https://docs.nvidia.com/datascience/platform-support/ , https://docs.nvidia.com/datascience/notices/
- https://docs.nvidia.com/cudf/latest/cudf_polars/ , https://docs.pola.rs/user-guide/gpu-support/ , https://pola.rs/posts/gpu-streaming-backend
- https://github.com/NVIDIA/cudf/blob/release/26.10/python/cudf_polars/cudf_polars/utils/config.py
- https://github.com/NVIDIA/cudf/issues/24513 , /24693 , /24756 , /24761 , /24843 , /23419 , /23575

CUDA, platforms, alternatives (accessed 2026-10-29):
- https://docs.nvidia.com/deploy/cuda-compatibility/minor-version-compatibility.html , https://docs.nvidia.com/deploy/cuda-compatibility/forward-compatibility.html , https://docs.nvidia.com/cuda/cuda-c-best-practices-guide/index.html
- https://docs.nvidia.com/heavyai/installation-and-configuration/installation/build-from-source , https://github.com/heavyai/heavydb/releases/tag/v10.0.0 , https://heterodb.github.io/pg-strom/install/ , https://github.com/rapidsai/gqe , https://github.com/craton-co/craton-bolt
- https://store.steampowered.com/hwsurvey/videocard/ , https://commandlinux.com/statistics/developer-os-preference-stack-overflow-survey/

Community threads (accessed 2026-10-28 unless marked):
- https://www.reddit.com/r/Python/comments/1fj0kfi/comment/lnrwn9g/ , https://www.reddit.com/r/gadgets/comments/1gywo4t/comment/lyyif4w/ , https://www.reddit.com/r/mac/comments/1hdxh51/ , https://www.reddit.com/r/macbookpro/comments/18cczma/comment/kcaebgz/ , https://www.reddit.com/r/CUDA/comments/1iena2p/ , https://www.reddit.com/r/datascience/comments/1fj0nmt/comment/lnl3cw9/ , https://www.reddit.com/r/dataengineering/comments/1kw9ku2/comment/mugm06f/ , https://www.reddit.com/r/dataengineering/comments/1o6bnir/ (2026-10-29)
- https://news.ycombinator.com/item?id=40560169 , https://news.ycombinator.com/item?id=44410362 , https://news.ycombinator.com/item?id=40079813 , https://news.ycombinator.com/item?id=50181617 (2026-10-29) , https://stackoverflow.com/q/78626099
- https://forums.developer.nvidia.com/t/can-the-rapids-image-go-on-a-diet-20gb-image/297463 , https://forums.developer.nvidia.com/t/rapids-cudf-accelerates-pandas-nearly-150x-with-zero-code-changes/272201 , https://forums.developer.nvidia.com/t/rapids-cudf-preview-build-request-for-grace-blackwell-gb10-dgx-aarch64-kernels-missing/352629 (2026-10-29) , https://forums.developer.nvidia.com/t/should-i-buy-asus-gx10-instead-nvidia-dgx-spark/347717 (2026-10-29) , https://forums.developer.nvidia.com/t/nvidias-financial-fraud-training-container-is-only-built-for-x86-64/352807 (2026-10-29) , https://forums.developer.nvidia.com/t/kernel-7-0-0-1019-nvidia-makes-apache-spark-rapids-gpu-work-1-5-2-4x-slower-on-dgx-spark-6-17-0-1032-with-the-same-driver-580-178-04-is-fine/386791 (2026-10-29)
- https://discourse.jupyter.org/t/minimal-image-compatible-with-nvidia-rapids/34116 , https://discuss.pytorch.org/t/cudf-for-python-3-11/189225 , https://discuss.pytorch.org/t/dataframe-kindo-of-object-in-pytorch/202686
- https://github.com/pola-rs/polars/issues/20696 , https://github.com/boettiger-lab/mcp-data-server/issues/227 (2026-10-29)
