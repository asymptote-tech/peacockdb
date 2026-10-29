# Workstation GPU SQL packaging: alternatives, static CUDA binaries, target-user OS/GPU mix

Network note: HN Algolia, Arctic Shift (Reddit) and survey.stackoverflow.co were unreachable or returned non-JSON from this host during the session; GitHub API intermittently timed out. Findings below come from GitHub API, NVIDIA docs, NVIDIA forum JSON, Steam survey page and web search. All accessed 2026-10-29.

## 1. Which GPU SQL engines ship as a single binary / pip wheel / simple installer for one machine?

### Takeaway
No GPU SQL engine ships as a small, self-contained, cross-platform binary. The easiest installs are pip wheels (Polars GPU via cudf-polars) and a DuckDB extension (Sirius, but it is a ~0.9–1.5 GB gz download per platform, Linux only). HeavyDB v10 (now NVIDIA, fully open source) ships as a Linux Docker image you build yourself, 15–20 GB. PG-Strom is RPMs for RHEL-family only. GQE has no release artifacts. The one small-footprint, Windows-capable engine found is a 13-star pre-1.0 Rust project (craton-bolt).

### Cited Findings
**HeavyDB (heavyai/heavydb)**
- Repo: "HeavyDB (formerly MapD/OmniSciDB)", 3,104 stars, last push 2026-10-23. Releases: v10.0.0 published 2026-10-08, v9.0.0 2025-10-20. Neither release has binary assets attached — [GitHub API repos/heavyai/heavydb/releases](https://github.com/heavyai/heavydb/releases) (accessed 2026-10-29)
- v10.0.0 release notes: "the first official release of HeavyAI v10.0, which is now fully open-source", including Immerse UI, the GPU Renderer, HeavyRF, HeavyConnect, and "Build tools and utilities to make the install process very easy" — [HeavyDB v10.0.0 release](https://github.com/heavyai/heavydb/releases/tag/v10.0.0) (accessed 2026-10-29)
- README points to "Build From Source", which builds "the entire HeavyAI platform which includes Immerse, WebServer, GEOS DSOs, and HeavyDB into a usable docker image". Native deps scripts target Rocky Linux 8.x and install under /usr/local/mapd-deps — [heavydb README](https://github.com/heavyai/heavydb) (accessed 2026-10-29)
- Documented install = build from source with `dev-tools/dev.sh build --docker`, producing "a single Docker image", deploy with `docker compose up -d`, move between hosts with `docker save`/`docker load`. No apt/yum, tarball, Windows or WSL path is mentioned. Build container: Ubuntu 22.04 (default) or Rocky Linux 8. Host must be Linux with NVIDIA driver and nvidia-container-toolkit; verification uses `nvcr.io/nvidia/cuda:12.9.2-runtime-ubuntu22.04`. "A full HeavyAI image is typically 15–20 GB uncompressed"; example product image 18.3 GB; build container 12.1 GB (pulled prebuilt from GHCR). The page does not say NVIDIA publishes a prebuilt product image — [NVIDIA HeavyAI docs: Build from source](https://docs.nvidia.com/heavyai/installation-and-configuration/installation/build-from-source) (accessed 2026-10-29)

**Sirius (sirius-db/sirius, DuckDB GPU extension)**
- "GPU-native composable analytics engine", 1,101 stars, Apache-2.0, pushed 2026-10-29 — [sirius-db/sirius](https://github.com/sirius-db/sirius) (accessed 2026-10-29)
- Prebuilt releases ("latest" and "stable", both 2026-10-02) ship DuckDB extension files for linux_amd64 and linux_arm64, CUDA 12 and CUDA 13. Sizes (gzipped): stable amd64-cuda12 1.54 GB, amd64-cuda13 0.92 GB, arm64-cuda12 1.53 GB, arm64-cuda13 1.03 GB; latest amd64-cuda12 1.39 GB, amd64-cuda13 0.95 GB. No Windows or macOS build — [Sirius releases](https://github.com/sirius-db/sirius/releases) (accessed 2026-10-29)
- Requirements: Linux amd64/arm64, glibc ≥ 2.28; "NVIDIA Turing or newer, with compute capability 7.5+"; CUDA 13.x (driver ≥ 580.65.06) or CUDA 12.x (driver ≥ 525.60.13); io_uring enabled at runtime (containers must allow io_uring syscalls); local Parquet on a filesystem supporting O_DIRECT. Build from source uses Pixi. Install = matching DuckDB version + download extension — [Sirius README](https://github.com/sirius-db/sirius) (accessed 2026-10-29)
- A University course project (cs764) benchmarked Sirius on TPC-H/TPC-DS and reports "7 merged upstream fixes" — [woodyycchang/cs764-sirius-project](https://github.com/woodyycchang/cs764-sirius-project) (accessed 2026-10-29)

**Polars GPU engine (cudf-polars)**
- Requirements: NVIDIA Volta or newer (compute capability 7.0+), CUDA 12, Linux or WSL2 (no native Windows). Install `pip install polars[gpu]` — [Polars docs: GPU support](https://docs.pola.rs/user-guide/gpu-support/) (accessed 2026-10-29)
- RAPIDS page install: `pip install polars[gpu] --extra-index-url=https://pypi.nvidia.com` — [RAPIDS Polars GPU engine](https://rapids.ai/polars-gpu-engine/) (accessed 2026-10-29)
- Current NVIDIA cuDF docs: wheels named cudf-polars-cu13 / cudf-polars-cu12 — [NVIDIA cuDF 26.10 docs: Polars GPU engine](https://docs.nvidia.com/cudf/26.10/cudf_polars/) (accessed 2026-10-29)
- cuDF latest release v26.10.02 (2026-10-20); 9,785 stars — [rapidsai/cudf](https://github.com/rapidsai/cudf) (accessed 2026-10-29)
- A community guide exists specifically for "Using Polars with NVIDIA GPU (CUDA) on Windows using WSL2" — [DEV Community](https://dev.to/harmanpsingh/using-polars-with-nvidia-gpu-cuda-on-windows-using-wsl2-869) (accessed 2026-10-29)

**GQE (rapidsai/gqe)**
- 62 stars, no description, last push 2026-08-11, license NOASSERTION, no GitHub releases. README is client/server ("gqe_task_manager" binary built under ./build, node manager), configured by many GQE_* env vars; uses Substrait plans and an MLIR-based JIT "that compiles relational operators into ... NVVM IR kernels"; nvCOMP compression, CUPTI monitoring — [rapidsai/gqe README](https://github.com/rapidsai/gqe) (accessed 2026-10-29)

**PG-Strom (heterodb/pg-strom)**
- 1,408 stars, pushed 2026-10-28 — [heterodb/pg-strom](https://github.com/heterodb/pg-strom) (accessed 2026-10-29)
- Install: per-PostgreSQL-major RPMs (e.g. pg_strom-PG18) from HeteroDB Software Distribution Center, or source build. Recommended OS RHEL/Rocky 8/9/10; Ubuntu has no packages (source build + .deb for heterodb-extra). No Docker mentioned. v6.1 needs PostgreSQL ≥ 15; GPU compute capability ≥ 7.5 (Turing+); CUDA ≥ 12.2u1; open NVIDIA driver recommended (GPUDirect Storage). Without a heterodb-extra license: no multi-GPU, no md-raid0 / NVMe-oF for GPUDirect SQL, no GPU GiST — [PG-Strom install docs](https://heterodb.github.io/pg-strom/install/) (accessed 2026-10-29)

**BlazingSQL** — last push 2022-09-16, last release v21.08.00 (2021-08-16); dead — [BlazingDB/blazingsql](https://github.com/BlazingDB/blazingsql) (accessed 2026-10-29)

**chDB** — CPU only (ClickHouse in-process); v4.4.0 (2026-09-11) asset is a pure `py3-none-any` wheel of 1.69 MB — [chdb-io/chdb releases](https://github.com/chdb-io/chdb/releases) (accessed 2026-10-29)

**Daft** — 5,819 stars, v0.7.26 (2026-10-06); description "High-performance data engine for AI and multimodal workloads" — no GPU SQL claim found in repo metadata — [Eventual-Inc/Daft](https://github.com/Eventual-Inc/Daft) (accessed 2026-10-29)

**craton-bolt (community, Rust)**
- "JIT-compiled GPU SQL engine", 13 stars, pushed 2026-10-27. Compiles each query to PTX at runtime, loads via CUDA driver API, "pure Rust on top of the raw CUDA driver API", no C++ shim or precompiled kernel library. Needs CC ≥ 7.0 (Volta+), CUDA 12.x toolkit, `cuda.lib`/`libcuda.so` on link path. Platform: Linux; "Windows (x86_64 MSVC): supported"; macOS not supported. README benchmarks compare to Polars 0.42 and DuckDB 1.2 on the h2o.ai table (driver 610.88, CUDA 13.3). Pre-1.0 with documented semantic gaps — [craton-co/craton-bolt](https://github.com/craton-co/craton-bolt) (accessed 2026-10-29)

**Other community repos** — "jingnanquan/duckdb-gpu" (0 stars, README is stock DuckDB); "gentialiaj411/gpu-sql" (0 stars, 2026-01) — toy/forks — [GitHub search "duckdb gpu"](https://github.com/search?q=duckdb+gpu&type=repositories) (accessed 2026-10-29)

### Inferences
- Package sizes span 1.7 MB (chDB, CPU) → ~1 GB gz (Sirius extension, which statically bundles libcudf-scale kernels) → 15–20 GB (HeavyDB image). A GPU engine under ~100 MB would be unusual.
- Sirius's CUDA 13 builds are ~35–40% smaller than CUDA 12 builds, likely from CUDA 13 dropping pre-Turing archs and/or better fatbin compression (not verified).
- GPU floors cluster at Turing (7.5: Sirius, PG-Strom) or Volta (7.0: cuDF/Polars, craton-bolt). Pascal (GTX 10) is excluded everywhere.

### Gaps
- Theseus (Voltron Data): voltrondata/theseus returns 404; no public packaging found.
- ClickHouse GPU, DataFusion GPU (Algeon, datafusion-arrowmetal), libcudf-rs: GitHub search for "datafusion cuda"/"cudf rust" timed out; not verified.
- Daft GPU execution status not checked in docs.
- Whether NVIDIA publishes a prebuilt HeavyDB v10 Docker image (old heavyai/heavyai-ee-cuda on Docker Hub) not verified.

## 2. Any GPU SQL engine with native Windows support? Any cuDF Windows port?

### Takeaway
None of the established engines runs natively on Windows: HeavyDB docs require a Linux host, cuDF/Polars GPU require Linux or WSL2, Sirius and PG-Strom are Linux only. The only native-Windows GPU SQL found is craton-bolt (tiny, pre-1.0). No community cuDF Windows port was found.

### Cited Findings
- HeavyDB install page: "Install the following on your Linux host"; no Windows/WSL mention — [NVIDIA HeavyAI docs](https://docs.nvidia.com/heavyai/installation-and-configuration/installation/build-from-source) (accessed 2026-10-29)
- Polars GPU: "Linux or Windows Subsystem for Linux 2 (WSL2)" — [Polars docs](https://docs.pola.rs/user-guide/gpu-support/) (accessed 2026-10-29)
- Sirius: Linux amd64/arm64 only, needs io_uring and O_DIRECT — [Sirius README](https://github.com/sirius-db/sirius) (accessed 2026-10-29)
- PG-Strom: Linux x86_64 only — [PG-Strom install](https://heterodb.github.io/pg-strom/install/) (accessed 2026-10-29)
- craton-bolt: "Windows (x86_64 MSVC): supported", builds with rust-lld — [craton-bolt README](https://github.com/craton-co/craton-bolt) (accessed 2026-10-29)

### Inferences
- A native-Windows GPU SQL engine is essentially an empty slot; WSL2 is the only path for RAPIDS-based tools.

### Gaps
- GitHub issue search on rapidsai/cudf for Windows requests failed (query error); a cuDF Windows tracking issue likely exists but was not retrieved.
- No first-person WSL2 failure reports collected (HN/Reddit APIs unreachable).

## 3. Can CUDA apps ship as a self-contained static binary?

### Takeaway
Mostly yes: the CUDA runtime is statically linked by default, and a fatbin can carry cubins for several sm_ archs plus PTX. But libcuda (the driver API) always comes from the user's installed NVIDIA driver and cannot be bundled on GeForce/Windows. Minor-version compatibility lets a CUDA 12.x binary run on any driver ≥ 525 (13.x: ≥ 580), with no PTX JIT on older drivers. Forward compatibility (cuda-compat) is data-center/Jetson/select RTX only, Linux only.

### Cited Findings
- "By default, nvcc links against the static CUDA runtime library"; shared needs `--cudart=shared`. Multiple PTX and cubin targets can be embedded in one binary; `-arch=compute_XY` = "PTX-only, GPU forward compatible"; `native` = "no GPU forward compatibility"; `all` "also includes the latest PTX for GPU forward compatibility" — [CUDA Programming Guide: nvcc](https://docs.nvidia.com/cuda/cuda-programming-guide/02-basics/nvcc.html) (accessed 2026-10-29)
- Best Practices Guide: static linking is "the easiest option" and "ensures that the correct version of runtime library functions are included in the application binary"; slightly larger executable. The driver ("libcuda.so on Linux systems") is a separate user-mode component; the driver is not redistributed — users install a suitable NVIDIA driver. For dynamic linking, redistribute the runtime because it "guarantees neither forward nor backward binary compatibility across versions"; use `-rpath '$ORIGIN'`. To run on future GPUs an app "must load PTX code that will be just-in-time compiled by the NVIDIA driver"; JIT results are cached on disk (env vars control location/size). With multiple -gencode, the driver picks the best cubin; else JITs PTX; else launch fails — [CUDA C++ Best Practices Guide](https://docs.nvidia.com/cuda/cuda-c-best-practices-guide/index.html) (accessed 2026-10-29)
- Minor version compatibility (CUDA 11+): binaries from any toolkit in the same major family "can run, with limited feature-set" on the minimum driver; features may return `cudaErrorCallRequiresNewerDriver`; apps that compile device code to PTX "will not work on older drivers"; requires `-arch=sm_xx`; NVRTC supports it from 11.3. Minimum drivers: CUDA 13.x ≥ 580; 12.x ≥ 525 (minor-compat range < 580); 11.x ≥ 450 — [CUDA Compatibility: minor version](https://docs.nvidia.com/deploy/cuda-compatibility/minor-version-compatibility.html) (accessed 2026-10-29)
- Forward compatibility applies only to "NVIDIA Data Center GPUs", "Select NGC Server Ready SKUs of RTX cards" and Jetson; package `cuda-compat-<major>-<minor>` provides libcuda.so, libnvidia-ptxjitcompiler.so, libnvidia-nvvm (JIT LTO) etc. into /usr/local/cuda-<ver>/compat/, requires LD_LIBRARY_PATH; only Linux install instructions — [CUDA Compatibility: forward](https://docs.nvidia.com/deploy/cuda-compatibility/forward-compatibility.html) (accessed 2026-10-29)
- craton-bolt demonstrates the driver-API-only approach (runtime PTX generation, no prebuilt kernel library), linking only libcuda/cuda.lib — [craton-bolt](https://github.com/craton-co/craton-bolt) (accessed 2026-10-29)

### Inferences
- A single binary built against CUDA 12.x with static cudart, cubins for sm_75/80/86/89/90/100/120 and compute_XX PTX would run on any GeForce with driver ≥ 525 (Turing+); building on CUDA 13 raises the floor to driver 580.
- Engines that JIT their own kernels (PTX via NVRTC or hand-generated) avoid fatbin bloat but need the user's driver to be at least as new as the PTX ISA version — conflicts with minor-version compat's "PTX won't work on older drivers".

### Gaps
- No NVIDIA-documented per-sm_ fatbin size figures found; the Sirius CUDA 12 vs 13 sizes (§1) are the only empirical datapoint.
- Exact Windows driver minima for CUDA 12/13 not split out on the page (Linux/Windows split only shown for CUDA 10.x).

## 4. OS and GPU share of the target user

### Takeaway
Steam's September 2026 data shows the RTX 50 series surging (RTX 5070 now #1 at 6.15%) and Pascal GTX 10 cards still a few percent; nearly all listed NVIDIA cards are Turing+ (CC ≥ 7.5) except GTX 10. Stack Overflow 2025 OS figures could not be retrieved; secondary sources show Windows the most common developer personal OS in 2024 (~59%).

### Cited Findings
- Steam Hardware Survey, "All video cards", Sep 2026 (May→Sep): RTX 5070 3.09→6.15% (+2.38); RTX 5060 2.58→4.40%; RTX 5060 Ti 2.06→4.06%; RTX 4060 3.89%; RTX 3060 3.66%; RTX 4060 Laptop 3.50%; RTX 3050 2.90%; RTX 4060 Ti 2.85%; RTX 5060 Laptop 2.45%; GTX 1650 2.19%; RTX 4070 2.11%; RTX 3060 Ti 2.08%; RTX 5080 1.98%; RTX 3070 1.97%; RTX 5070 Ti 1.87%; RTX 2060 1.53%; GTX 1060 1.34%; GTX 1660 SUPER 1.26%; GTX 1050 Ti 1.03%; RTX 4090 0.67%; GTX 1070 0.54%; GTX 1050 0.47% — [Steam Hardware Survey: video cards](https://store.steampowered.com/hwsurvey/videocard/) (accessed 2026-10-29)
- Earlier 2026: April RTX 3060 4.15% vs RTX 4060 4.05%, RTX 5070 2.86% — [Guru3D](https://guru3d.com/story/steam-survey-april-2026-shows-rtx-3060-still-leading-gpu-market/); June RTX 4060 Laptop 3.81% on top — [RetroHandhelds](https://retrohandhelds.gg/steam-hardware-survey-june-2026-a-laptop-gpu-lands-on-top/) (accessed 2026-10-29)
- Stack Overflow 2024 (secondary): Windows 59.2% personal / 47.6% professional, macOS 31.8%, Ubuntu 27.7%, WSL 17.1% personal — [commandlinux.com summary](https://commandlinux.com/statistics/developer-os-preference-stack-overflow-survey/) (accessed 2026-10-29); unverified against primary
- Conflicting 2026 polls: a poll put Windows at just 12% among developers — [Windows Latest 2026-09-22](https://www.windowslatest.com/2026/09/22/microsoft-keeps-rebuilding-windows-for-developers-but-a-new-poll-puts-windows-at-just-12/); a Microsoft engineer rebutted that more developers use Windows than Linux or macOS — [Windows Latest 2026-10-05](https://windowslatest.com/2026/10/05/microsoft-distinguished-engineer-fires-back-at-anti-windows-11-rage-bait-says-more-developers-use-windows-than-linux-or-macos) (accessed 2026-10-29)

### Inferences (my sums of listed Steam rows, Sep 2026 — partial, top rows only)
- RTX 50 rows listed: ≈ 21.6% of all Steam GPUs (5070, 5060, 5060 Ti, 5060 L, 5080, 5070 Ti, 5070 L).
- GTX 10 (Pascal, CC 6.x, unsupported by cuDF/Sirius/PG-Strom) rows: ≈ 5.4% (1060, 1050 Ti, 1070, 1050). GTX 16 (Turing 7.5) rows: ≈ 4.2%.
- RTX 20 rows listed (2060, 2060 S, 2070 S, 2050): ≈ 3.4%.
- Steam is gamer-weighted and Windows-dominated; RTX 50 = Blackwell sm_120 needs CUDA ≥ 12.8 and recent drivers.

### Gaps
- Stack Overflow 2025 OS stats: survey.stackoverflow.co DNS failed; primary not retrieved.
- JetBrains / Anaconda State of Data Science OS shares: not retrieved.
- Steam October 2026 not yet published at access time (page showed through September).

## 5. First-person posts 2025–2026 (workstation/laptop/DGX Spark GPU SQL/dataframes)

### Takeaway
Few retrieved. The clearest: a DGX Spark owner believed cuDF was "completely broken" on GB10 (Nov 2025) until a RAPIDS maintainer pointed out 25.10 public packages support it; and an Oct 2026 report of Spark+RAPIDS slowing 1.5–2.4× on a newer DGX Spark kernel.

### Cited Findings
- 2025-11-25, user adam344 (DGX GB10, driver 580.95.05, CUDA 13.0, Ubuntu 24.04 aarch64): "The public RAPIDS channels (24.12/nightly) do not contain GB10 kernels, so cuDF is unusable on this machine… This is blocking GPU-accelerated dataframe work on a brand-new DGX system." Reply from RAPIDS dev bdice (2025-11-26): "DGX Spark (GB10) support is already available in public RAPIDS packages with version 25.10", via `conda create -n rapids-25.10 -c rapidsai -c conda-forge -c nvidia rapids=25.10 python=3.13 'cuda-version=13.0'` (also pip and Docker) — [NVIDIA forum thread 352629](https://forums.developer.nvidia.com/t/rapids-cudf-preview-build-request-for-grace-blackwell-gb10-dgx-aarch64-kernels-missing/352629) (accessed 2026-10-29)
- 2026-10-25 thread title: "Kernel 7.0.0-1019-nvidia makes Apache Spark + RAPIDS GPU work 1.5-2.4x slower on DGX Spark; 6.17.0-1032 with the same driver 580.178.04 is fine" — [NVIDIA forum 386791](https://forums.developer.nvidia.com/t/kernel-7-0-0-1019-nvidia-makes-apache-spark-rapids-gpu-work-1-5-2-4x-slower-on-dgx-spark-6-17-0-1032-with-the-same-driver-580-178-04-is-fine/386791) (accessed 2026-10-29; body not retrieved)
- 2025-11-27: "NVIDIA's financial-fraud-training container is only built for x86_64"; 2025-11-29 "Financial Fraud Detection Blueprint on ARM64?" — DGX Spark users hitting x86-only RAPIDS containers — [forum 352807](https://forums.developer.nvidia.com/t/nvidias-financial-fraud-training-container-is-only-built-for-x86-64/352807), [forum 352949](https://forums.developer.nvidia.com/t/financial-fraud-detection-blueprint-on-arm64/352949) (accessed 2026-10-29; titles only)
- 2025-12-25: user-built "Full-Featured CUDA-X Data Science Environment on NVIDIA DGX" extending the RAPIDS playbook for ARM64 Grace Blackwell — [forum 355714](https://forums.developer.nvidia.com/t/next-iteration-full-featured-cuda-x-data-science-environment-on-nvidia-dgx-with-heavyweight-ml-libraries/355714) (accessed 2026-10-29)
- 2026-07-17: "Concern about the usefulness of a single Dgx Spark" — [forum 377281](https://forums.developer.nvidia.com/t/concern-about-the-usefulness-of-a-single-dgx-spark/377281) (accessed 2026-10-29; title only)
- craton-bolt README compares its GPU engine to Polars 0.42 and DuckDB 1.2 on h2o.ai groupby data (author-run) — [craton-bolt](https://github.com/craton-co/craton-bolt) (accessed 2026-10-29)

### Inferences
- DGX Spark forum traffic is dominated by LLM inference; dataframe/SQL threads are rare and mostly install/arch friction.

### Gaps
- HN Algolia and Arctic Shift (Reddit) APIs returned errors from this host; no HN/Reddit first-person posts collected, including CPU-vs-GPU (DuckDB/Polars vs cuDF) comparisons and WSL2/conda/image-size complaints.

## 6. DGX Spark / GB10 owners using RAPIDS for analytics in 2026

### Takeaway
NVIDIA officially supports it (RAPIDS ≥ 25.10 on GB10; DGX Spark "CUDA-X Data Science" playbook), and Sirius ships linux_arm64 CUDA 13 builds. Public 2026 user reports are thin: an Apache Spark + RAPIDS kernel-regression report (Oct 2026) is the main one found.

### Cited Findings
- NVIDIA DGX Spark playbook: "Install and use NVIDIA cuML and NVIDIA cuDF to accelerate UMAP, HDBSCAN, pandas and more with zero code changes" — [build.nvidia.com playbook](https://build.nvidia.com/playbooks/cuda-x-data-science) (accessed 2026-10-29)
- RAPIDS 25.10 public packages support GB10 — [forum 352629](https://forums.developer.nvidia.com/t/rapids-cudf-preview-build-request-for-grace-blackwell-gb10-dgx-aarch64-kernels-missing/352629) (accessed 2026-10-29)
- Sirius arm64 CUDA 13 extension 1.03 GB gz (stable) — [Sirius releases](https://github.com/sirius-db/sirius/releases) (accessed 2026-10-29)
- Spark+RAPIDS on DGX Spark kernel regression thread — [forum 386791](https://forums.developer.nvidia.com/t/kernel-7-0-0-1019-nvidia-makes-apache-spark-rapids-gpu-work-1-5-2-4x-slower-on-dgx-spark-6-17-0-1032-with-the-same-driver-580-178-04-is-fine/386791) (accessed 2026-10-29)

### Gaps
- No public 2026 benchmark of cuDF/Polars GPU/Sirius vs DuckDB CPU on DGX Spark found.
