# Integration surfaces and hardware options for a small GPU SQL engine

All links accessed 2026-10-28 unless stated otherwise. "(estimate)" marks my own judgement. Source quality: many 2026 facts come from search-engine summaries of vendor pages and one secondary GTC write-up (jazracherif.github.io). They are labelled as such. Primary pages I fetched directly: DataFusion discussion #8498, NVIDIA GQE blueprint page, IT Jungle on IBM/Nestlé, the GTC 2026 write-up, and the nextbigfuture GTC summary.

## datafusion-plugin: DataFusion-based production systems, their GPU asks, and existing DataFusion GPU efforts

### Takeaway
I found no DataFusion-based production system (InfluxDB 3, GreptimeDB, Spice.ai, LanceDB, etc.) publicly asking for GPU execution. DataFusion GPU work is limited to individual prototypes: libcudf-rs (gabotechs, Jan 2026) and ArrowMetal (Apple Metal, Oct 2026). A maintainer has endorsed the "custom ExecutionPlan + rewrite rule" shape in principle. NVIDIA's open-source GQE reference engine lists DataFusion as a dependency and accepts Substrait, so it is the closest well-funded competitor on this surface.

### Cited Findings
- apache/datafusion Discussion #8498 "How DataFusion could support other compute engines (libcudf, velox)". Opened 2023-12-11 by spencerwilson, who proposed a granular per-operator design or a coarse `LibCudfExec` node. alamb (accepted answer, 2023-12-12) pointed to custom ExecutionPlans / PhysicalOptimizerRules / QueryPlanner. He called a GPU ExecutionPlan plus a rewrite rule "a pretty neat idea", wanted a PoC, and said the coarse approach may suit libcudf better because it keeps data on the GPU — [GitHub discussion #8498](https://github.com/apache/datafusion/discussions/8498) (accessed 2026-10-28)
- Same thread: gabotechs (2026-01-05) is prototyping GPU execution for DataFusion via **libcudf-rs** (Rust bindings to libcudf), "results so far are encouraging". brancz (Polar Signals) asked for a license on 2026-03-19, it was licensed 2026-03-24, and brancz planned to evaluate it around end of April 2026 — [GitHub discussion #8498](https://github.com/apache/datafusion/discussions/8498) (accessed 2026-10-28)
- Same thread: singhpratech (2026-10-03) published **ArrowMetal** / `datafusion-arrowmetal`. A PhysicalOptimizerRule swaps SortExec/AggregateExec for Metal GPU nodes. Self-reported results on M4 Max vs DataFusion 55.1: ORDER BY 6.9–28.8× faster, group-by 2.31–4.27× faster, top-k (0.14–0.41×) and filters (0.31–0.79×) slower. The crate must pin `datafusion = "=55.1.0"` because the physical-plan API changes every release, and the author asked for a stable API subset — [GitHub discussion #8498](https://github.com/apache/datafusion/discussions/8498) (accessed 2026-10-28); crate docs [docs.rs datafusion-arrowmetal 0.5.0](https://docs.rs/datafusion-arrowmetal/0.5.0) (accessed 2026-10-28) (per search summary it also covers filters and hash joins)
- datafusion-python issue #936 (opened 2024-10-28): "Spike: evaluate if cuDF can be used with datafusion-python". Separately, an older PR #205 by jdye64 added cuDF as a physical execution engine in arrow-datafusion-python. I could not confirm whether either is still active — [datafusion-python #936](https://github.com/apache/datafusion-python/issues/936); [lists.apache.org PR #205 thread](https://lists.apache.org/api/plain?thread=122p9zry98l3gvjr57kom120xp2o4tq6) (accessed 2026-10-28)
- **NVIDIA GPU Query Engine (GQE)**: an open-source (Apache-2.0) "reference architecture designed, developed and benchmarked by NVIDIA". It queries Parquet on disk/host/GPU memory, scales multi-GPU, uses the Blackwell hardware decompression engine (B100/B200/B300), and is Substrait-compatible ("test your own plans or even a different SQL front-end"). It lists DataFusion among third-party software. Repo github.com/rapidsai/gqe; page last updated 2026-08-03 — [build.nvidia.com GQE blueprint](https://build.nvidia.com/nvidia/gpu-query-engine) (accessed 2026-10-28)
- Spice.ai uses DataFusion extension points (20+ table providers, federation/acceleration pushdown rules, UDFs). Its "acceleration" materialises data into DuckDB/Arrow/SQLite, not GPU memory — [Spice.ai blog](https://spice.ai/blog/how-we-use-apache-datafusion-at-spice-ai) (accessed 2026-10-28)
- InfluxDB 3 is built on DataFusion (chosen for Rust, Arrow, extensibility), with no GPU mention — [InfluxData blog](https://www.influxdata.com/blog/7-datafusion-projects-influxdb/) (accessed 2026-10-28)
- GreptimeDB's only GPU hit is a user story about storing GPU *metrics*, not GPU query execution — [Greptime blog](https://greptime.com/blogs/tensor-fusion-greptimedb-user-story) (accessed 2026-10-28)
- A third-party 2026 guide says datafusion-python 46.x has no GPU acceleration and points GPU users to cuDF or Theseus — [pythondatabench.com](https://pythondatabench.com/article/datafusion-python-arrow-query-engine-2026) (accessed 2026-10-28) (secondary source)
- Apache DataFusion Comet: a search summary called it "partially GPU-accelerated". I could not verify this. Comet is a native-Rust (CPU) Spark accelerator, so treat the claim as suspect. Reported Comet 0.15.0 (Apr 2026) ~2× TPC-H SF1000 speedup — [Monthly Python Data Engineering, Apr 2026](https://pythondataeng.substack.com/p/monthly-python-data-engineering-april-4e5) (accessed 2026-10-28) (secondary)
- The same roundup lists DataFusion 53.0.0 in ~Apr 2026, and ArrowMetal pins 55.1 by Oct 2026 — [substack](https://pythondataeng.substack.com/p/monthly-python-data-engineering-april-4e5); [discussion #8498](https://github.com/apache/datafusion/discussions/8498) (accessed 2026-10-28)

### Inferences
- peacockdb already translates DataFusion physical plans into its own node tree. A "DataFusion GPU offload crate" (a PhysicalOptimizerRule that replaces a subtree with one coarse `PeacockGpuExec`) is therefore the shape alamb endorsed, and is probably the cheapest integration to build (estimate).
- peacockdb is on DataFusion 45. Upstream is ~55 by Oct 2026, i.e. about 10 major releases behind. Any plugin product must first catch up and then track a churning physical-plan API, which ArrowMetal's author calls the main pain. This is a recurring maintenance cost for a 1-person team (estimate).
- No named DataFusion-based company has a public GPU ask. The adopter would have to be a DataFusion *library user* (e.g. Polar Signals/brancz evaluating libcudf-rs) rather than a product team. Demand evidence is thin: one or two individuals, no company.
- GQE (NVIDIA, Substrait, DataFusion-adjacent, multi-GPU, Apache-2.0) and libcudf-rs overlap heavily with peacockdb. Platform risk is high: NVIDIA could ship a DataFusion GPU frontend itself.

### Gaps
- I could not check GitHub issue trackers of GreptimeDB, LanceDB, Seafowl, Arroyo, OpenObserve, Parseable, CnosDB, HoraeDB, Synnada, Cube, or Ballista for GPU requests within budget. Absence of evidence ≠ evidence of absence.
- Whether GQE uses DataFusion as its SQL frontend (the page only lists it as third-party software).
- ParadeDB pg_analytics status (possibly deprecated in favour of pg_search/DuckDB): unverified.

## other-accelerators: AMD, Intel, Apple, AI chips, analytics ASIC startups

### Takeaway
AMD has a real cuDF fork (hipDF, production status since ROCm-DS 25.10) restricted to Instinct datacenter GPUs. Voltron Data reportedly targets it for Theseus. No open GPU *SQL engine* on hipDF surfaced. Apple Silicon has several hobby-scale GPU SQL projects (ArrowMetal, gpudb, duckdb-mlx). The analytics-ASIC startups (Speedata, NeuroBlade, DataPelago) all target Spark ETL in datacenters, not small users. I found no evidence of analytics work on Cerebras/Groq/Tenstorrent or a 2026 Intel analytics GPU stack.

### Cited Findings
- ROCm-DS is AMD's open-source data-science toolkit, "a fork of NVIDIA's RAPIDS project, extended for AMD GPUs". hipDF is a pandas-like GPU DataFrame on Arrow — [ROCm-DS docs](https://rocm.docs.amd.com/projects/rocm-ds/en/latest/index.html) (accessed 2026-10-28)
- ROCm-DS 25.10 moved hipDF and hipMM to production status (hipDF 2.0.0) and added hipRAFT and hipVS. hipGRAPH stayed early access — [ROCm-DS 25.10 release notes](https://rocm.docs.amd.com/projects/rocm-ds/en/latest/about/release-notes.html) (accessed 2026-10-28)
- The 26.03 docs list hipDF, hipGRAPH, hipMM, hipRAFT, hipVS, dask-hip, and hip-ucxx — [AMD Data Science 26.03 docs](https://rocm.docs.amd.com/projects/rocm-ds/en/docs-26.03/contribute/contributing.html) (accessed 2026-10-28)
- hipDF requires ROCm 7.2.3 and Ubuntu 24.04+. Officially supported archs are only gfx90a, gfx942, and gfx950 (MI200/MI300/MI350-class Instinct), with no consumer Radeon. RAPIDS and ROCm-DS packages must not be mixed — [GitHub ROCm-DS/hipdf](https://github.com/ROCm-DS/hipdf) (accessed 2026-10-28)
- Voltron Data is adding AMD Instinct support to Theseus via hipDF, "production support expected later in the year". The date of this item is uncertain — [daily.dev summary](https://daily.dev/posts/voltron-data-embraces-amd-for-gpu-accelerated-sql-nohqd0jtn) (accessed 2026-10-28) (secondary, undated)
- Voltron Data's GitHub org now shows "Nomura Research Institute America (Formerly Voltron Org)", which suggests Voltron was absorbed/acquired. Unverified beyond the org name — [github.com/voltrondata](https://github.com/voltrondata) (accessed 2026-10-28)
- Apple Silicon:
  - ArrowMetal (Arrow compute on Metal, DataFusion crate) — [PyPI arrowmetal](https://pypi.org/project/arrowmetal/) (accessed 2026-10-28)
  - DuckDB community extension `gpudb` (gpu_sum/min/max on CUDA or Metal, hybrid Metal GROUP BY in 0.1.3; self-reported M4 Max numbers) — [duckdb.org community extension gpudb](https://duckdb.org/community_extensions/extensions/gpudb) (accessed 2026-10-28)
  - `duckdb-mlx` (MLX-based, osx_arm64 only, status page 2026-07-03 says transparent GPU acceleration live) — [ecosyste.ms duckdb-mlx](https://awesome.ecosyste.ms/projects/github.com%2Fquackscience%2Fduckdb-mlx) (accessed 2026-10-28)
- TUM paper "Evaluating Apple Silicon for Data Processing" (DaMoN 2026, per search summary): GPU and CPU-GPU co-processing on Apple Silicon reach up to 5× over CPU-only — [TUM PDF](https://db.in.tum.de/~beischl/papers/Evaluating_Apple_Silicon_for_Data_Processing.pdf) (accessed 2026-10-28)
- Speedata (Israel) APU: an ASIC for Spark SQL/ETL. Raised a $44M Series B in Jun 2025 — [BigDATAwire](https://www.hpcwire.com/bigdatawire/this-just-in/speedata-raises-44m-series-b-to-launch-analytics-processing-chip-for-big-data-workloads/). Feb 2026 deal: Nebul is the first cloud to offer the APU, in its European sovereign cloud — [DCD](https://www.datacenterdynamics.com/en/news/neocloud-nebul-to-offer-speedatas-analytics-processing-units/). Speedata cites a Microsoft SIGMOD 2026 paper benchmarking its C-200 inside Microsoft Fabric at up to 259× on TPC-H. That is vendor characterisation — [Speedata blog](https://www.speedata.io/post/259-faster-what-microsoft-s-award-winning-sigmod-paper-says-about-the-speedata-apu) (all accessed 2026-10-28)
- DataPelago launched an Accelerator for Spark in Aug 2025: native execution + CPU vectorisation + GPU, on its "Nucleus" engine. Vendor claims up to 10× and 80% cost savings, and a customer case with 3–4× speedup and 60–70% cost cut on PB-scale ETL — [BigDATAwire launch](https://www.hpcwire.com/bigdatawire/this-just-in/datapelago-launches-worlds-1st-accelerator-for-apache-spark-that-leverages-both-cpus-and-gpus/); [BigDATAwire CEO interview](https://www.hpcwire.com/bigdatawire/2025/08/05/bigdatawire-exclusive-interview-datapelago-ceo-on-launching-the-spark-accelerator/); [AWS APN blog](https://aws.amazon.com/blogs/apn/run-apache-spark-up-to-10x-faster-with-datapelago-on-amazon-eks/) (accessed 2026-10-28)
- NeuroBlade SPU: a PCIe SQL processing unit integrating with Spark, Presto, and ClickHouse. Claims range 10×–100× (sources disagree) — [Tech Field Day 2024](https://techfieldday.com/2024/when-cpus-gpus-are-not-enough-neuroblades-spu); [TechRadar](https://www.techradar.com/pro/are-hyper-specialized-processing-units-the-future-of-computing-meet-the-company-who-wants-to-be-the-nvidia-of-data-queries) (accessed 2026-10-28). No 2026 news found.
- NVIDIA is pushing its Vera CPU for analytics (1.2 TB/s memory BW, claimed 2.5–3× analytics lift, validated by Starburst, Kinetica, and Redpanda) — [GTC 2026 write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28) (secondary)

### Inferences
- Who wants cuDF-equivalent on non-NVIDIA hardware: (1) AMD Instinct owners (clouds/HPC sites that bought MI300 for AI and have idle capacity), signalled by AMD funding hipDF and Voltron's AMD port; (2) Mac users who want local acceleration, signalled by 3+ independent hobby projects in 2026. Neither group has a public, named "we need a GPU SQL engine" ask (estimate).
- A hipDF port of peacockdb's C++ executor *might* be mostly mechanical because hipDF is a RAPIDS fork. However, hipDF's API lags cuDF, and only datacenter Instinct GPUs are supported, so a weekend team would need cloud MI300 access to test (estimate).
- An Apple Metal backend means abandoning cuDF and writing kernels. That likely exceeds the 1–2× yardstick budget for TPC-H/DS coverage (estimate).
- The analytics ASICs (Speedata, NeuroBlade, DataPelago) sell to Spark-scale enterprises. They are not a surface for a small engine, but they confirm the "cut Spark/warehouse bill" pain.

### Gaps
- Intel (oneAPI data analytics / Gaudi), Cerebras, Groq, and Tenstorrent: I found no 2025–2026 analytics/SQL efforts within budget. Unknown, not confirmed absent.
- No AMD newsroom 2026 announcement on GPU SQL found.
- Whether hipDF exposes libcudf-equivalent C++ APIs (libhipdf) at parity with cuDF 26.x: not checked.
- Fungible and Pliops: not researched (Fungible was acquired by Microsoft in 2023 per my prior knowledge, unverified here).

## DuckDB extensions beyond Sirius, and Sirius status

### Takeaway
Sirius is becoming the default GPU path for DuckDB. NVIDIA blogs back it, it holds ClickBench #1/#2, and Hannes Mühleisen reportedly announced it will become a core DuckDB extension. That makes "GPU for DuckDB" a closed surface for a small competitor. The other DuckDB GPU extensions are small (gpudb, duckdb-mlx).

### Cited Findings
- Sirius ships rolling "latest" and maintainer-promoted "stable" snapshots, not versioned releases. Extensions are pinned to an exact DuckDB version (install example: v1.5.6). It is unsigned (not in DuckDB's signed repo). Binaries cover Linux amd64/arm64 with CUDA 12/13. Unsupported operators fall back silently to CPU — [Sirius release "latest"](https://github.com/sirius-db/sirius/releases/tag/latest); [Sirius release "stable"](https://github.com/sirius-db/sirius/releases/tag/stable) (accessed 2026-10-28)
- GTC 2026 claims for Sirius: ClickBench hot-run #1/#2 on GH200 and H100; all 22 TPC-H SF1K queries in 21 s on a GB300 DGX Station; TPC-H Q9 on 1 TB Parquet 2.5 s vs 16 s CPU DuckDB on a 384-core box; 7× TCO on ClickBench. Sirius never mixes CPU/GPU within a query. "DuckDB creator Hannes Mühleisen announced Sirius will become a core DuckDB extension." Transwarp: TPC-DS 150 GB 26× faster than CPU DuckDB — [GTC 2026 write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28) (secondary; core-extension claim not verified on duckdb.org)
- Sirius CIDR 2026 paper: it accepts Substrait plans, builds on libcudf/RMM/NCCL, reports 8.3× / 7.4× cost efficiency on TPC-H / ClickBench with DuckDB, and up to 12.5× speedup integrated with Apache Doris — [CIDR 2026](https://vldb.org/cidrdb/2026/rethinking-analytical-processing-in-the-gpu-era.html); [arXiv 2508.04701](https://arxiv.org/html/2508.04701v3) (accessed 2026-10-28)
- NVIDIA's cuDF page lists Sirius as the DuckDB drop-in option alongside the Polars GPU engine and cudf.pandas — [NVIDIA cuDF](https://developer.nvidia.com/cudf) (accessed 2026-10-28)
- NVIDIA cuCascade (memory tiering/spilling GPU↔host↔disk, multi-GPU routing) started 2026-01-01, is public at NVIDIA/cuCascade, and Sirius integrates it — [GTC 2026 write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28)
- DuckDB v2.0.0 reportedly released 2026-10-21 (Wikipedia snippet, unverified). Sirius pins to 1.5.6 — [Wikipedia DuckDB](https://en.wikipedia.org/wiki/DuckDB) (accessed 2026-10-28)

### Inferences
- Building on Sirius (allowed by the brief) is the low-friction way to stay GPU-centred while avoiding a head-on fight. peacockdb could contribute or package a niche product *on top of* Sirius instead of competing as a DuckDB GPU extension (estimate).
- Sirius's version pinning, unsigned distribution, and silent fallback are friction points a first user hits. Packaging/hosting Sirius for a niche is a possible product (estimate).

### Gaps
- MotherDuck GPU plans: none found.
- DuckDB community "GPU please" issue counts: not measured.
- Whether DuckDB 2.0 made Sirius a signed core extension: not verified.

## Substrait / Arrow Flight SQL / ADBC as a neutral "GPU query server"

### Takeaway
Substrait is already the de facto plan-input for NVIDIA-aligned GPU engines (Sirius and GQE both accept it), so a "Substrait GPU server" duplicates what NVIDIA open-sources. I found no Flight SQL GPU server.

### Cited Findings
- Sirius accepts Substrait plans. With DuckDB, the Substrait plan is routed to Sirius for GPU execution. With Apache Doris, it gives up to 12.5× — [arXiv 2508.04701](https://arxiv.org/html/2508.04701v3) (accessed 2026-10-28)
- GQE is "Substrait compatible" so users can bring "a different SQL front-end" — [GQE blueprint](https://build.nvidia.com/nvidia/gpu-query-engine) (accessed 2026-10-28)
- NVIDIA publishes guidance on "add GPU acceleration without forking our query engine", positioning cuDF as in-place acceleration for existing engines — [NVIDIA perspectives FAQ](https://perspectives.nvidia.com/cudf/faq/we-want-to-add-a-gpu-execution-path-to-our-existing-query-engine-but-don-t-want-to-fork-the-mainline-6627a6/); [NVIDIA perspectives: GPU-accelerated SQL engines](https://perspectives.nvidia.com/cudf/accelerating-existing-sql-engines-in-place-for-faster-query/) (accessed 2026-10-28)

### Inferences
- A Flight SQL + ADBC endpoint is cheap (estimate 2–5k lines with arrow-flight in Rust). It lets any BI tool or Python client reach peacockdb, so it is a delivery mechanism rather than a product.
- Substrait consumer support in peacockdb (DataFusion has substrait consumer crates) would make it pluggable into the same hosts as Sirius/GQE, but with no differentiation (estimate).

### Gaps
- No search evidence of any third party asking for a neutral GPU Substrait server.

## BI acceleration (dashboards, semantic layers, GPU BI)

### Takeaway
The dashboard-latency pain is real, but the market solves it with caching/pre-aggregation (Cube, Sisense ElastiCubes, warehouse accelerators), not GPUs. GPU BI incumbent HeavyDB appears to have moved under NVIDIA, with an open-source re-release planned for 2026.

### Cited Findings
- Cube's answer to latency is its built-in caching engine / pre-aggregations, with no GPU — [SourceForge Cube mirror](https://www.sourceforge.net/projects/cube.mirror) (accessed 2026-10-28)
- Sisense "acceleration" builds aggregation tables in an ElastiCube. The semantic layer itself "does not accelerate the query itself" — [Sisense docs](https://docs.sisense.com/main/SisenseLinux/acceleration-benefits.htm) (accessed 2026-10-28)
- HeavyDB docs are now hosted at docs.nvidia.com/heavyai — [NVIDIA HeavyAI docs](https://docs.nvidia.com/heavyai/tutorials/getting-started-open-source.md) (accessed 2026-10-28). GTC 2026 write-up: Heavy AI/OmniSci "planned open-source release in late Q2 2026 … Todd Mostak now leads this work at NVIDIA" (LLVM codegen, Vulkan rendering, geospatial) — [GTC 2026 write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28) (secondary; NVIDIA–Heavy relationship not confirmed by a primary announcement)
- Trino at Airbnb: 2% of queries consume 92% of cluster resources (GTC 2026) — [GTC 2026 write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28)

### Inferences
- The Airbnb stat suggests a "heavy-query offload" product: send only the expensive tail of queries to a GPU box. That fits a small engine better than serving all dashboards (estimate).
- Because HeavyDB is under NVIDIA, a small GPU-BI backend faces an NVIDIA-backed open-source competitor with geospatial and rendering features (platform risk high).

### Gaps
- Kinetica, Graphistry, Tableau/Power BI GPU efforts: not found within budget.

## Embedded / workstation (DGX Spark, DGX Station, RTX, laptops, Jetson)

### Takeaway
NVIDIA's own workstation-scale analytics story is Polars GPU engine + Sirius. Sirius showcases a GB300 DGX Station (TPC-H SF1K in 21 s). I found no DGX Spark-specific SQL tooling or Jetson analytics product. "Local GPU SQL on my workstation" is plausible but owned by NVIDIA-backed projects.

### Cited Findings
- The Polars GPU engine (cuDF-backed) gives up to 13× vs Polars CPU on H100 (PDS-H) and falls back to CPU when needed — [NVIDIA cuDF](https://developer.nvidia.com/cudf); [RAPIDS 24.12 blog](https://developer.nvidia.com/blog/rapids-24-12-introduces-cudf-on-pypi-cuda-unified-memory-for-polars-and-faster-gnns) (accessed 2026-10-28)
- Compute Paris 2026 talk: scaling Polars GPU from laptop to workstation to multi-GPU — [pretalx](https://pretalx.com/compute-paris-2026/talk/9N8K3B/) (accessed 2026-10-28)
- Sirius on GB300 DGX Station: 22 TPC-H SF1K queries in 21 s. Velox async cache got hot runs to ~20 s on an RTX 6000 — [GTC 2026 write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28)

### Inferences
- DGX Spark (128 GB unified memory) could make "laptop-sized GPU SQL over 100 GB Parquet" feasible without spilling. However, nobody I found is asking for a non-DuckDB/Polars SQL engine there (estimate).

### Gaps
- DGX Spark data-science docs and Jetson analytics: no sources found.

## Postgres extension (PG-Strom, pg_duckdb, pg_mooncake, ParadeDB)

### Takeaway
PG-Strom (HeteroDB) is a mature, open-source (PostgreSQL License) GPU extension at v6.1 supporting PG 14–18. I found no 2026 news showing growing demand. Postgres analytics momentum is CPU/DuckDB-based (pg_duckdb, pg_mooncake), with no visible GPU ask.

### Cited Findings
- pg_strom v6.1 is packaged for PostgreSQL 14–18. It needs an NVIDIA GPU of CC 7.5+ and CUDA 12.2+, and is loaded via shared_preload_libraries. It does GPU scan/join/agg/sort, GPUDirect SQL, Arrow_Fdw, and GPU cache — [Pigsty pg_strom](https://pigsty.io/ext/e/pg_strom/); [PG-Strom docs](https://heterodb.github.io/pg-strom); [github.com/pg-strom/devel](https://github.com/pg-strom/devel) (accessed 2026-10-28)
- Dated HeteroDB claims (2018–19): ~100 TB per node, ~40 GB/s query throughput — [Canonical blog](https://ubuntu.com/blog/lets-play-sharded-big-data-postgresql) (accessed 2026-10-28)

### Inferences
- PG-Strom already occupies "GPU inside Postgres". Without a named user group complaining that PG-Strom fails them, this surface looks low-value (estimate).

### Gaps
- pg_duckdb / pg_mooncake / ParadeDB GPU asks: not searched within budget.

## Trino/Presto with GPU worker (Velox cuDF, IBM watsonx.data)

### Takeaway
IBM + NVIDIA have shipped GPU Presto C++ (Velox + cuDF). It has nightly images (Jun 2026), a VLDB 2026 paper, and a production customer (Nestlé: 15 min → 3 min, 83% cheaper). This surface is owned by IBM/NVIDIA/Meta and closed to a small entrant.

### Cited Findings
- Nestlé order-to-cash data mart (44 tables) runs on watsonx.data Presto + Velox + cuDF. Refresh went from ~15 min to 3 min, overall cost 83% lower, which IBM calls a 30× price-performance gain. Announced at GTC 2026 by Sam Werner (IBM); no GA date given — [IT Jungle 2026-03-23](https://www.itjungle.com/2026/03/23/ibm-taps-nvidia-gpus-for-ai-turbocharged-data-mart/) (accessed 2026-10-28)
- Presto blog 2026-06-24: "GPU-Accelerated Presto C++ is Here: Nightly Images for NVIDIA GPUs" (page returned 403 to fetch; title/date from search) — [prestodb.io](https://prestodb.io/blog/2026/06/24/gpu-accelerated-presto-c/) (accessed 2026-10-28)
- IBM Research paper "Accelerating Presto with GPUs" (VLDB 2026): one GPU per Velox worker, UcxExchange between GPU workers, up to 6× cost/perf vs CPU Presto. cuDF lacks streaming groupby, which they worked around by concatenation — [arXiv 2606.24647](https://arxiv.org/html/2606.24647v1) (accessed 2026-10-28)
- GTC 2026: 4× B200 ran TPC-H SF1K ~30× faster than an 8-node Grace CPU cluster, at ~10× lower TCO. Parquet scan was 60–70% of runtime — [GTC 2026 write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28)
- Velox's site labels cuDF mode experimental — [velox-lib.io](https://velox-lib.io/) (accessed 2026-10-28)

### Inferences
- A finding that matters for peacockdb: Parquet scan dominates GPU query time (60–70%). Single-node engines win on I/O (GDS, decompression, caching) more than on operators (estimate).

### Gaps
- Trino GPU worker: none found.

## Snowflake / Databricks-adjacent (GPU containers, external accelerators on Iceberg/Delta)

### Takeaway
Snowflake and Databricks expose GPUs for ML (cuDF/cuML in Snowflake ML, RAPIDS on Databricks classic/serverless GPU), not for their SQL engines. Third-party Spark accelerators (DataPelago, Speedata, Spark-RAPIDS) chase the "cut the bill" pain. I found no small product reading Iceberg/Delta on GPU to offload warehouse queries.

### Cited Findings
- Snowflake ML's default GPU environment integrates cuML and cuDF (pandas, scikit-learn, UMAP, HDBSCAN acceleration) — [Snowflake blog](https://www.snowflake.com/en/blog/nvidia-gpu-acceleration/) (accessed 2026-10-28)
- RAPIDS docs cover cuDF/cuML on Databricks classic and serverless GPU compute (pins 26.08). Spark RAPIDS 26.06 jar for Databricks falls back to CPU for unsupported ops — [RAPIDS on Databricks](https://docs.rapids.ai/deployment/stable/platforms/databricks/); [cuDF for Apache Spark on Databricks](https://docs.nvidia.com/spark-rapids/user-guide/latest/getting-started/databricks.html) (accessed 2026-10-28)
- Secondary GTC 2026 summary: adopters include "Apache Spark, Databricks, Snowflake, Presto, DuckDB, Polars, Velox", and IBM watsonx.data, Dell AI Data Platform (3× faster), Google Cloud Dataproc, and Oracle AI Database 26ai. Spark + cuDF: up to 5× faster, 10× better TCO on 10 TB. Snap on GKE: 76% lower daily cost on 10 PB — [nextbigfuture](https://www.nextbigfuture.com/2026/03/nvidia-structured-data-is-the-ground-truth-of-ai-120-billion-structure-data-ecosystem.html) (accessed 2026-10-28) (secondary, unsourced numbers)
- Databricks Data+AI Summit sessions on LLM-rewriting Spark UDFs for GPU (cuAether), with UDFs noted as an adoption blocker — [DAIS session](https://www.databricks.com/dataaisummit/session/cuaether-assistant-using-llms-rewrite-udfs-gpu-acceleration) (accessed 2026-10-28); "UDF support still incomplete and a blocker" — [GTC 2026 write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html)

### Inferences
- A "bring-your-own-GPU over your Iceberg tables" offload for the expensive tail of warehouse queries matches documented pain: Airbnb's 2%/92% skew, Nestlé's 83% cost cut, DataPelago customers' 60–70% savings. No small open product owns it yet, but Sirius + DuckDB-iceberg could cover it cheaply (estimate).

### Gaps
- Snowpark Container Services GPU SKUs and whether anyone runs a SQL engine there: not verified.
- Databricks GPU-serverless SQL: no evidence found.

---

### (a) One-line verdicts
- **datafusion-plugin**: technically the cheapest fit, since peacockdb already consumes DataFusion physical plans and the rewrite-rule + custom ExecutionPlan shape is maintainer-endorsed. But no DataFusion-based company has a public GPU ask, only individual prototypes (libcudf-rs, ArrowMetal). NVIDIA's Substrait/DataFusion-adjacent GQE competes, and API churn (DF 45 → 55) is costly — [discussion #8498](https://github.com/apache/datafusion/discussions/8498); [GQE](https://build.nvidia.com/nvidia/gpu-query-engine) (accessed 2026-10-28).
- **other-accelerators**: AMD hipDF is a production RAPIDS fork limited to Instinct gfx90a/942/950. Apple has only hobby-scale GPU SQL (ArrowMetal, gpudb, duckdb-mlx). Analytics ASICs target enterprise Spark. I found no named small user group demanding non-NVIDIA GPU SQL — [hipDF](https://github.com/ROCm-DS/hipdf); [ROCm-DS 25.10 notes](https://rocm.docs.amd.com/projects/rocm-ds/en/latest/about/release-notes.html); [gpudb](https://duckdb.org/community_extensions/extensions/gpudb) (accessed 2026-10-28).

### (b) New candidate directions

| slug | one-line | evidence anyone needs it | GPU-centred? | build vs yardstick (estimate) | platform risk |
|---|---|---|---|---|---|
| df-gpu-offload-crate | Publish peacockdb as a DataFusion PhysicalOptimizerRule that swaps a supported subtree for one coarse `PeacockGpuExec` (Arrow in/out) | alamb invited a PoC; brancz (Polar Signals) evaluating libcudf-rs; ArrowMetal author asks for this — [#8498](https://github.com/apache/datafusion/discussions/8498). No company ask. | yes | 0.3–0.6× (DF 45→55 upgrade + Arrow C-data bridge + fallback) | high: DF API churn every release; NVIDIA GQE / libcudf-rs could take the slot |
| hipdf-port | Port the C++ executor from cuDF to hipDF so AMD Instinct (MI300/MI350) owners get GPU SQL | AMD funds hipDF (production 25.10); Voltron targeted AMD for Theseus — [hipDF](https://github.com/ROCm-DS/hipdf), [daily.dev](https://daily.dev/posts/voltron-data-embraces-amd-for-gpu-accelerated-sql-nohqd0jtn). No named end user. | yes | 0.2–0.5× if hipDF API ≈ cuDF; much more if gaps | medium-high: hipDF lags cuDF, datacenter-only hardware, hard for weekend contributors to test |
| iceberg-tail-offload | Single-node GPU box that reads Iceberg/Delta and runs only the heaviest warehouse/Trino queries (routed via Flight SQL) to cut bills | Airbnb: 2% of Trino queries = 92% of resources; Nestlé 83% cost cut; DataPelago 60–70% — [GTC write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html), [IT Jungle](https://www.itjungle.com/2026/03/23/ibm-taps-nvidia-gpus-for-ai-turbocharged-data-mart/), [BigDATAwire](https://www.hpcwire.com/bigdatawire/2025/08/05/bigdatawire-exclusive-interview-datapelago-ceo-on-launching-the-spark-accelerator/) | yes | 0.4–0.8× (iceberg-rust scan, deletes, Flight SQL, router) | high: Sirius+DuckDB-iceberg, Spark-RAPIDS, GPU Presto, DataPelago all adjacent |
| flight-sql-gpu-endpoint | Arrow Flight SQL / ADBC server in front of peacockdb so BI tools and Python clients connect without embedding | No direct ask found; enabling layer for other directions | yes (wrapper) | 0.1–0.2× | low (open protocols) but no differentiation |
| sirius-packaged-niche | Build the niche product on Sirius (signed builds, version-pin handling, spill/ops UX) instead of peacockdb's executor | Sirius pain points: unsigned, exact-DuckDB-version pin, silent CPU fallback — [Sirius release](https://github.com/sirius-db/sirius/releases/tag/latest) | yes | 0.3–0.7× | high: NVIDIA/DuckDB Labs may ship this themselves (core extension announced, per GTC write-up) |
| metal-local-sql | GPU SQL on Apple Silicon unified memory for local-first analysts | 3+ hobby projects in 2026 (ArrowMetal, gpudb, duckdb-mlx); TUM up to 5× — [#8498](https://github.com/apache/datafusion/discussions/8498), [gpudb](https://duckdb.org/community_extensions/extensions/gpudb), [TUM](https://db.in.tum.de/~beischl/papers/Evaluating_Apple_Silicon_for_Data_Processing.pdf) | yes (non-NVIDIA) | 1.5–3× (no cuDF; own kernels) — likely over budget | medium: Apple API stable, but crowded hobby space and no paying user |
| pg-gpu-ext | GPU analytics inside Postgres | PG-Strom v6.1 already exists; no new demand found — [PG-Strom](https://heterodb.github.io/pg-strom) | yes | 0.8–1.5× | high: PG-Strom incumbent, Postgres analytics momentum is DuckDB-based |

Ruled out as surfaces (owned by large players): Presto/Velox GPU worker (IBM+NVIDIA+Meta), DuckDB GPU extension (Sirius, NVIDIA-backed, reportedly becoming core), GPU BI backend (HeavyDB now under NVIDIA per GTC write-up), Spark acceleration (Spark-RAPIDS, DataPelago, Speedata, NeuroBlade).
