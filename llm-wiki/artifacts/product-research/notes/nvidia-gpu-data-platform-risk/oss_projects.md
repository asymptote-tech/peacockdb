# NVIDIA (and NVIDIA-funded) open-source projects for GPU data processing / GPU SQL — status as of 2026-10-28

Method note: activity numbers come from blob-less bare clones made on 2026-10-28 (`git log --since=2026-04-01`) and the GitHub REST API (`gh api repos/...`). Author-domain counts give a **lower bound** on NVIDIA involvement: many NVIDIA staff commit under `users.noreply.github.com`, `gmail.com` or personal domains (e.g. `bradleydice.com` is NVIDIA's Bradley Dice, `entschev.com` is Peter Entschev; these are guesses from RAPIDS PR approver lists, so treat as estimate). The web-search budget ran out partway through. After that I worked from GitHub data and fetched primary pages directly. The Gaps sections list what I could not check.

## Q1. cuDF family: libcudf, libcudf_streaming, cudf-polars, cudf.pandas, dask-cudf, rapidsmpf

### Takeaway
cuDF is NVIDIA's most active data project: about 2,000 commits since April 2026, releases every two months (v26.10.02 on 2026-10-20), Apache-2.0, and now under the NVIDIA GitHub org. In 2026 NVIDIA added a streaming, multi-GPU layer on top of it: rapidsmpf, plus a new `libcudf_streaming` C++ library (June 2026). It also gave cudf-polars a streaming executor that runs on one GPU or across many GPUs and nodes. Every one of these is a reusable component. None of them is a SQL product.

### Cited Findings
- **Repo moved to NVIDIA org.** `rapidsai/cudf` now redirects (HTTP 301) to `NVIDIA/cudf`: Apache-2.0, 9,783 stars, last push 2026-10-28 — [GitHub API repos/NVIDIA/cudf](https://github.com/NVIDIA/cudf) (accessed 2026-10-28). The README calls it "NVIDIA cuDF … an Apache 2.0 licensed, GPU-accelerated DataFrame library", and names the Spark plugin "NVIDIA cuDF plugin for Apache Spark" — [NVIDIA/cudf README](https://github.com/NVIDIA/cudf/blob/main/README.md) (accessed 2026-10-28).
- cuML and cuVS also moved to the NVIDIA org (`NVIDIA/cuml`, `NVIDIA/cuvs`), as did raft (`NVIDIA/raft`). rmm, kvikio, cugraph and rapidsmpf stay under `rapidsai` — [github.com/NVIDIA/cuml](https://github.com/NVIDIA/cuml), [github.com/NVIDIA/raft](https://github.com/NVIDIA/raft), [github.com/rapidsai/rmm](https://github.com/rapidsai/rmm) (accessed 2026-10-28).
- **Releases (bimonthly cadence):** v26.04.00 (2026-04-08), v26.06.00 (2026-06-03), v26.08.00 (2026-08-05), v26.08.01 (2026-08-25), v26.10.00 (2026-10-08), v26.10.01/.02 (2026-10-20) — [NVIDIA/cudf releases](https://github.com/NVIDIA/cudf/releases) (accessed 2026-10-28).
- **Activity since 2026-04-01:** 2,065 commits by 112 distinct author emails. Commits by email domain: noreply 811, nvidia.com 637, gmail 496, bradleydice.com 87, entschev.com 18, ibm.com 6 — [git log of github.com/NVIDIA/cudf](https://github.com/NVIDIA/cudf/commits/main) (accessed 2026-10-28).
- **Packages in the monorepo:** `python/` holds `cudf`, `cudf_kafka`, `cudf_polars`, `cudf_streaming`, `custreamz`, `dask_cudf`, `libcudf`, `libcudf_streaming`, `pylibcudf`. `cpp/` holds `libcudf_kafka` and `libcudf_streaming` — [NVIDIA/cudf tree](https://github.com/NVIDIA/cudf/tree/main/python) (accessed 2026-10-28). dask-cudf now lives here; the old `rapidsai/dask-cudf` repo has been archived since 2019 — [rapidsai/dask-cudf](https://github.com/rapidsai/dask-cudf) (accessed 2026-10-28).
- **libcudf_streaming (new in 2026).** PR #22747 (merged 2026-06-04, Vyas Ramasubramani): "ports over the libcudf-specific components of librapidsmpf into a new C++ library inside this repository that depends on librapidsmpf." Python bindings followed in #22760 (2026-06-04) and tests/benchmarks in #22814 (2026-06-09). Headers: `parquet.hpp`, `partition.hpp`, `bloom_filter.hpp`, `approx_distinct_count.hpp`, `table_chunk.hpp`, `channel_metadata.hpp`. Examples and benchmarks: `example_shuffle.cpp`, `bench_shuffle.cpp` — [rapidsai/cudf PR #22747](https://github.com/rapidsai/cudf/pull/22747), [cpp/libcudf_streaming](https://github.com/NVIDIA/cudf/tree/main/cpp/libcudf_streaming) (accessed 2026-10-28).
- **Lakehouse-relevant Parquet readers in libcudf.** Under the experimental namespace: `hybrid_scan.hpp` and `hybrid_scan_multifile.hpp` (two-step filter-then-materialize scan with page-stats and dictionary-page filtering), plus `deletion_vectors.hpp`. The latter is described as "APIs for applying Delta Lake style deletion vectors when reading Parquet data": a chunked reader filtered by serialized roaring64 bitmaps — [cpp/include/cudf/io/experimental](https://github.com/NVIDIA/cudf/tree/main/cpp/include/cudf/io/experimental) (accessed 2026-10-28).
- **cudf-polars streaming executor.** "All four ways of running cudf-polars use this same streaming executor: RayEngine, DaskEngine, SPMDEngine, and the default engine="gpu"…". Streaming "lets queries scale past device memory and (on Ray, Dask, and SPMD) across multiple GPUs and multiple nodes." The in-memory engine "is the only non-streaming path" — [cudf_polars/engines.md](https://github.com/NVIDIA/cudf/blob/main/docs/cudf/source/cudf_polars/engines.md) (accessed 2026-10-28). The streaming code tree includes `actor_graph/collectives` and PDS-H/PDS-DS benchmark query sets — [python/cudf_polars/cudf_polars/streaming](https://github.com/NVIDIA/cudf/tree/main/python/cudf_polars/cudf_polars/streaming) (accessed 2026-10-28).
- cudf-polars is installed as `pip install cudf-polars-cu13` and invoked with `.collect(engine="gpu")` on Polars lazy frames. cudf.pandas is described as "A zero-code change accelerator … for existing pandas code" — [NVIDIA/cudf README](https://github.com/NVIDIA/cudf/blob/main/README.md) (accessed 2026-10-28).
- **rapidsmpf.** "Collection of multi-GPU, distributed memory algorithms. RapidsMPF provides a unified framework for asynchronous, multi-GPU pipelines using simple streaming primitives". Apache-2.0, 36 stars, v26.10.00 (2026-10-08). Since 2026-04-01: 253 commits by 32 authors (gmail 89, nvidia.com 51, noreply 42, entschev.com 40, bradleydice.com 30) — [rapidsai/rapidsmpf](https://github.com/rapidsai/rapidsmpf) (accessed 2026-10-28).
- **No cuDF SQL front end found.** No SQL parser or planner exists in the NVIDIA/cudf tree; a path search for `sql` found nothing relevant — [NVIDIA/cudf tree](https://github.com/NVIDIA/cudf) (accessed 2026-10-28). NVIDIA's SQL prototype is a separate repo, GQE (see Q5).

### Inferences
- libcudf_streaming plus rapidsmpf is the "streaming engine" building-block layer: chunked Parquet, partition/shuffle, bloom filters, approximate distinct count. A peacockdb-style C++ executor could adopt it for out-of-core and multi-GPU work instead of building its own. This helps **datafusion-plugin**, **lakehouse-iceberg**, **distributed-pluggable** and **spark-plugin**.
- cudf-polars running on Ray, Dask and SPMD across multiple GPUs and nodes is a free, NVIDIA-backed DataFrame engine for distributed GPU work. It competes for the same Python/data-science buyers that a GPU SQL engine might target (estimate). The threat to SQL-first products is moderate.
- The deletion-vector reader and hybrid_scan directly lower the cost of a GPU Delta or Iceberg reader. That lowers the barrier for **lakehouse-iceberg**, for the startup and for its competitors alike.

### Gaps
- Polars Cloud's GPU offering (managed GPU execution) could not be verified because the search budget ran out. I found only a Polars Cloud tracking issue from 2025-02-27 — [pola-rs/polars#21487](https://github.com/pola-rs/polars/issues/21487) (accessed 2026-10-28).
- I could not find an official rename date for the move of the RAPIDS repos to the NVIDIA org.

## Q2. Spark plugin (formerly spark-rapids, now "NVIDIA cuDF plugin for Apache Spark") and spark-rapids-jni

### Takeaway
The former spark-rapids is now `NVIDIA/cudf-spark`, and the "RAPIDS name will be sunset". It is very active (961 commits since April, mostly NVIDIA), released v26.08.1 on 2026-08-28, and is Apache-2.0 for code. It already ships Iceberg (with an async S3 byte-range reader), Delta Lake for Delta 2.0–4.3 and Databricks runtimes, a local-disk file cache, and a GPU cache serializer. That makes it a strong incumbent against **spark-plugin** and, in Spark shops, against **lakehouse-iceberg**.

### Cited Findings
- "Note: The NVIDIA cuDF plugin for Apache Spark was formerly known as the RAPIDS Accelerator for Apache Spark. The RAPIDS name will be sunset over time. GitHub links from `spark-rapids` will [redirect]…" — [NVIDIA/cudf-spark README](https://github.com/NVIDIA/cudf-spark) (accessed 2026-10-28). Maven artifact names (`rapids-4-spark`) stay unchanged for now — [cudf-spark download docs](https://nvidia.github.io/cudf-spark/docs/download.html) (accessed 2026-10-28, via search summary).
- **Repo:** NVIDIA/cudf-spark, 1,013 stars, latest release v26.08.1 (2026-08-28), previous v26.08.0 (2026-08-20), v26.06.1 (2026-07-23), v26.06.0 (2026-06-23), v26.04.2 (2026-05-13). The licence file reads "dual-licensed with documentation/skills under the CC-BY-4.0 AND source code under Apache-2.0" (GitHub API shows NOASSERTION because of the dual licence) — [NVIDIA/cudf-spark](https://github.com/NVIDIA/cudf-spark), [LICENSE](https://github.com/NVIDIA/cudf-spark/blob/main/LICENSE) (accessed 2026-10-28).
- **Activity since 2026-04-01:** 961 commits by 46 authors (nvidia.com 459, noreply 313, gmail 147, nyu.edu 21, apache.org 14). Recent commits (2026-10-27/28) include "Add experimental Project AST JIT … [databricks]" and "Enable non-Delta integration tests in DBR 18.3 premerge CI [databricks]" — [cudf-spark commits](https://github.com/NVIDIA/cudf-spark/commits/main) (accessed 2026-10-28).
- **spark-rapids-jni → NVIDIA/cudf-spark-jni:** Apache-2.0, v26.10.1 (2026-10-21), 849 commits since April by 30 authors (noreply 713, nvidia.com 90) — [NVIDIA/cudf-spark-jni](https://github.com/NVIDIA/cudf-spark-jni) (accessed 2026-10-28).
- **Lakehouse formats:** the repo has `iceberg/` and `iceberg-common/` modules (with a `fileio/iceberg` package), plus `delta-lake/` shims for delta-20x … delta-43x and Databricks-specific shims (`delta-spark330db`, `delta-spark350db143`, `delta-spark400db173`, `delta-spark410db183`) — [NVIDIA/cudf-spark tree](https://github.com/NVIDIA/cudf-spark/tree/main/delta-lake) (accessed 2026-10-28).
- **Iceberg S3 reader:** configs `spark.rapids.iceberg.s3.async.*` cover an "async HTTP client used by the cuDF plugin Iceberg S3 byte-range reader". There is also an "Iceberg session-level scan options" catalog wrapper — [advanced_configs.md](https://github.com/NVIDIA/cudf-spark/blob/main/docs/additional-functionality/advanced_configs.md), [iceberg-per-table-scan-options.md](https://github.com/NVIDIA/cudf-spark/blob/main/docs/additional-functionality/iceberg-per-table-scan-options.md) (accessed 2026-10-28). Delta deletion vectors are handled (`spark.rapids.sql.delta.lowShuffleMerge.deletionVector.broadcast.threshold`) — same source.
- **Caching:**
  - A local-disk file cache, `spark.rapids.filecache.*` (maxBytes, allow/block path regexps, checkStale, useChecksums; available since 24.02.0) — [advanced_configs.md](https://github.com/NVIDIA/cudf-spark/blob/main/docs/additional-functionality/advanced_configs.md) (accessed 2026-10-28).
  - A "Cache Serializer" for Spark `cache()`/`persist()` dataframes — [cache-serializer.md](https://github.com/NVIDIA/cudf-spark/blob/main/docs/additional-functionality/cache-serializer.md) (accessed 2026-10-28).
  - Neither is a VRAM-resident cache for real-time serving. The file cache stores on local disk, per its config description.
- **Shuffle:** the RAPIDS Shuffle Manager offers "MULTITHREADED" mode and a "UCX" mode for accelerated transport — same source.

### Inferences
- **spark-plugin** has a free, Apache-2.0, NVIDIA-staffed incumbent that already covers Databricks, Iceberg, Delta and multiple shuffle modes. A startup Spark plugin would compete head-on (strong threat).
- The Iceberg and Delta work is Spark/JVM-bound and does not transfer directly to a C++ engine. The underlying libcudf readers (Q1) do transfer.

### Gaps
- I did not verify the exact Iceberg feature matrix: merge-on-read position/equality deletes, v3 deletion vectors.
- Whether the file cache is open source in all builds versus only in NVIDIA's distributed jar was not verified. The configs and tests (`FileCacheIntegrationSuite`) are present in the open repo.

## Q3. Velox cuDF backend, Presto GPU (Prestissimo), Apache Gluten GPU

### Takeaway
The cuDF backend for Velox (`velox/experimental/cudf`) is a real joint NVIDIA–IBM–Meta effort. It has 212 commits since April 2026 from 49 authors. It powers GPU Presto C++, with nightly images since June 2026, and an experimental Gluten (Spark) GPU path. It is Apache-2.0 and a direct threat to **distributed-pluggable** and **spark-plugin**, and an indirect threat to **lakehouse-iceberg** because Presto and Spark are the main lakehouse query engines.

### Cited Findings
- **Velox:** `facebookincubator/velox`, Apache-2.0, 4,242 stars. The `velox/experimental/cudf` subtree has 243 files (connectors, exec, expression, functions, vector, benchmarks). Since 2026-04-01 it has had 212 commits by 49 authors (gmail 54, noreply 53, nvidia.com 44, ibm.com 20, meta.com 19, zurich.ibm.com 9). Top committers include Muhammad Haseeb, Matt Gara, Simon Eves, Liangcai Li (NVIDIA); Daniel Bauer, Chengcheng Jin, Pramod Satya, Yotam Perlitz (IBM); and Masha Basmanova (Meta) — [velox/experimental/cudf](https://github.com/facebookincubator/velox/tree/main/velox/experimental/cudf) (accessed 2026-10-28).
- **RAPIDS staging fork:** `rapidsai/velox` is "A fork of Velox with RAPIDS cuDF as a GPU-accelerated backend. This will be upstreamed to Velox." (Apache-2.0, pushed 2026-10-15). Separately, `rapidsai/velox-testing` was pushed 2026-10-16 — [rapidsai/velox](https://github.com/rapidsai/velox), [rapidsai/velox-testing](https://github.com/rapidsai/velox-testing) (accessed 2026-10-28).
- **NVIDIA/IBM blog (2025-10-06):**
  - "Velox acts as an intermediate layer, translating query plans from systems like Presto and Spark into executable GPU pipelines powered by cuDF."
  - GPU operators: TableScan, HashJoin, HashAggregation, FilterProject.
  - A UCX-based Exchange over NVLink/RoCE/InfiniBand "delivered >6x speedup when using NVLink … compared to … HTTP exchange" on an 8-GPU DGX A100.
  - TPC-H SF1000: Presto C++ on CPU 1,246 s vs. 133.8 s on RTX PRO 6000 Blackwell and 99.9 s on GH200.
  - Authors from NVIDIA and IBM.
  - [NVIDIA blog](https://developer.nvidia.com/blog/accelerating-large-scale-data-analytics-with-gpu-native-velox-and-nvidia-cudf) (accessed 2026-10-28).
- **Presto blog (2026-06-24), "GPU-Accelerated Presto C++ is Here: Nightly Images for NVIDIA GPUs":** says GPU Presto C++ nightly images are public, with some Presto-side PRs merged and others under review — [prestodb.io blog](https://prestodb.io/blog/2026/06/24/gpu-accelerated-presto-c/) (accessed 2026-10-28; direct fetch returned HTTP 403, content taken from search-result summary, so verify). Presto latest release is 0.299 (2026-08-28) — [prestodb/presto releases](https://github.com/prestodb/presto/releases) (accessed 2026-10-28).
- An IBM watsonx.data plus Nestlé proof of concept reportedly got "5x faster queries and 83% cost reduction" (vendor claim, from a search summary of NVIDIA/IBM material) — [IBM community blog](https://community.ibm.com/community/user/blogs/ali-leclerc/2025/10/07/accelerating-data-analytics-gpu-velox-nvidia) (accessed 2026-10-28; not directly fetched).
- **Apache Gluten** (renamed `apache/gluten`, Apache-2.0, v1.7.0 on 2026-08-26). Its "Velox GPU" doc says:
  - Status is "Experimental (TPC-H SF1 validated)", with "up to 8.1x speedup".
  - The switch is `spark.gluten.sql.columnar.cudf=true`, with CPU fallback.
  - "Global RMM memory manager, cannot align with Spark memory system".
  - Prebuilt image: `apache/gluten:centos-9-jdk17-cuda13.1-cudf`.
  - [VeloxGPU.md](https://github.com/apache/gluten/blob/main/docs/backends/velox/get-started/VeloxGPU.md) (accessed 2026-10-28).
- cuDF commits are landing in Gluten in October 2026, e.g. "[VL] Extend cuDF functions in Gluten with row_constructor_with_null (#13104)" and "Fix host to GPU conversion in CudfVectorStream (#13067)" — [apache/gluten commits](https://github.com/apache/gluten/commits/main) (accessed 2026-10-28).

### Inferences
- NVIDIA is putting GPU execution into shared, multi-vendor engine cores (Velox, Gluten) rather than building its own SQL product. Any Velox-based engine gets GPU support "for free" over time, which hurts **distributed-pluggable** and **spark-plugin** (estimate).
- Gluten GPU is still experimental (SF1-validated, no memory integration with Spark). cudf-spark (Q2) remains NVIDIA's production Spark path.

### Gaps
- I could not fetch the Presto blog directly (403), so the exact merge status of the Presto GPU PRs is unverified.
- I found no evidence of Meta production use of Velox GPU.

## Q4. Sirius (sirius-db/sirius), cuCascade, and who is adopting Sirius

### Takeaway
Sirius is Apache-2.0 and led by UW-Madison with heavy NVIDIA staffing: nvidia.com is the largest corporate domain, and NVIDIA blog authors are core contributors. It is very active, with 935 commits since April 2026, and has been rewritten ("Super Sirius"). Beyond DuckDB it now has experimental **StarRocks** and **Apache Doris** backends. ClickHouse Inc. engineers have opened RFCs to run ClickHouse plan subtrees on Sirius and have merged cuDF and cuCascade into ClickHouse `contrib/`. Sirius is therefore the main open-source threat to **clickhouse**, **realtime-other** and **distributed-pluggable**. It is also a usable base if the startup builds on it.

### Cited Findings
- **Repo:** sirius-db/sirius, an Organization account, created 2024-06-19, Apache-2.0, 1,101 stars, pushed 2026-10-28. There are no semver releases, only rolling `stable` (2026-10-02, "maintainer-selected") and `latest` (tracks main) tags — [sirius-db/sirius](https://github.com/sirius-db/sirius), [releases](https://github.com/sirius-db/sirius/releases) (accessed 2026-10-28).
- **Self-description:** "Sirius is a GPU-Native Composable Analytics Engine. It plugs into existing databases via the standard Substrait query format … Sirius currently supports DuckDB and Starrocks (coming soon) … Built on NVIDIA CUDA-X libraries including cuDF, cuVS, and cuCascade". It supports concurrent queries (`sirius.max_concurrent_queries`). It requires io_uring, and O_DIRECT for local Parquet. The ClickBench results came from a "legacy" implementation, archived at `legacy-dev` — [Sirius README](https://github.com/sirius-db/sirius/blob/main/README.md) (accessed 2026-10-28).
- **Activity since 2026-04-01:** 935 commits by 38 authors. By email domain: noreply 369, gmail 297, nvidia.com 160, outlook 45, msn 29, wisc.edu 26. Top committers: Matthijs Brobbel (gmail, 203), dependabot (129), Akhil Nair (nvidia.com, 57), Bobbi Winema Yogatama (54), Pradeep Garigipati (nvidia.com, 52), William Malpica (49), Mike Wendt (48), Kevin Kristensen (nvidia.com 40 + wisc.edu 20), Felipe Aramburu (26), Johan Peltenburg (19) — [sirius commits](https://github.com/sirius-db/sirius/commits/main) (accessed 2026-10-28).
- **Recent work (Sep–Oct 2026):** bump to cuDF 26.10.02, spill compression, dynamic filters, TPC-DS Q41 joins, window aggregates, DuckDB v2.0.0 pins, and the Doris build — same source.
- **NVIDIA blog (2025-12-15):**
  - Sirius is UW-Madison-led with NVIDIA support.
  - Authors: Xiangyao Yu (UW-Madison), Bobbi Yogatama ("NVIDIA, core Sirius contributor"), Yifei Yang, and Rodrigo Aramburu (NVIDIA DevRel lead for accelerated data processing).
  - "Sirius is open source with the permissive Apache License 2.0".
  - ClickBench on a GH200 at $1.5/h claimed "at least 7.2x higher cost-efficiency".
  - Roadmap: spilling, GPU-native file readers, pipeline model, multi-node multi-GPU.
  - [NVIDIA blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/) (accessed 2026-10-28).
- **CIDR 2026 paper**, "Rethinking Analytical Processing in the GPU Era": reports 8.3x cost-efficiency on TPC-H and 7.4x on ClickBench vs. DuckDB, and up to 12.5x with Apache Doris in a distributed setup — [CIDR 2026](https://vldb.org/cidrdb/2026/rethinking-analytical-processing-in-the-gpu-era.html), [arXiv 2508.04701](https://arxiv.org/html/2508.04701v3) (accessed 2026-10-28, via search summary).
- **StarRocks backend:** `experimental/starrocks` has 107 commits from 2026-05-28 to 2026-10-28 — [sirius experimental/starrocks](https://github.com/sirius-db/sirius/tree/main/experimental/starrocks) (accessed 2026-10-28).
- **Doris backend:** `experimental/doris` has 6 commits since 2026-10-06. It is "A Rust process that joins an **unmodified, official Apache Doris FE** as a backend node and runs the plan fragments it receives on the embedded Sirius engine". The path is Doris FE 4.1.4 → thrift/gRPC → `doris-plan-translator` → Substrait → engine — [experimental/doris/README.md](https://github.com/sirius-db/sirius/blob/main/experimental/doris/README.md) (accessed 2026-10-28).
- **cuCascade:** `NVIDIA/cuCascade`, "GPU Memory Reservation Library", Apache-2.0, created 2025-12-15, 63 stars. It offers "Tiered Memory Management … GPU (fastest), pinned host (medium), and disk", reservations, NUMA-aware allocators, "cuDF Integration … spilling to Host or Disk", and pluggable OOM/spill policies. Contributors since April 2026 include Sirius committers (Brobbel, Aramoon, Malpica, Aramburu, Garigipati) and NVIDIA's Kyle Edwards and Peter Entschev — [NVIDIA/cuCascade](https://github.com/NVIDIA/cuCascade) (accessed 2026-10-28).
- **Other Sirius adopters:**
  - A `polardb/duckdb-paimon` issue (2026-10-15): "Qualify the existing DuckDB v2 adaptation for Sirius co-loading" — [polardb/duckdb-paimon#137](https://github.com/polardb/duckdb-paimon/issues/137) (accessed 2026-10-28).
  - A MatrixOne subtask (2026-07-24) adds "Substrait offload" coverage — [matrixorigin/matrixone#26159](https://github.com/matrixorigin/matrixone/issues/26159) (accessed 2026-10-28).
- **ClickHouse on Sirius (see Q9).** RFC #116807, opened 2026-08-27 by a ClickHouse org member, says: "As a first step, we would like to build ClickHouse on top of Sirius" — [ClickHouse#116807](https://github.com/ClickHouse/ClickHouse/issues/116807) (accessed 2026-10-28).

### Inferences
- Sirius is effectively NVIDIA-funded. NVIDIA staff are among the top committers, NVIDIA DevRel co-authors its announcements, and its memory layer (cuCascade) sits in the NVIDIA org. Several top committers also show up in NVIDIA projects. Felipe Aramburu and William Malpica (ex-BlazingSQL/Voltron Data) commit to both Sirius and cuCascade, but their current employer is unverified (estimate).
- Sirius's Substrait plus "backend node" pattern targets the realtime OLAP engines (Doris, StarRocks, ClickHouse) directly. A startup building **clickhouse** or **realtime-other** on its own engine would be racing an NVIDIA-staffed open-source project that those vendors are already adopting (strong threat). Building *on* Sirius would make the startup a services or packaging layer competing with Sirius's own maintainers.

### Gaps
- I could not confirm current employers for Matthijs Brobbel, William Malpica, Felipe Aramburu and Mike Wendt. Commit emails are gmail/noreply.
- No governance doc or foundation was checked. Who holds the `sirius-db` org (UW-Madison vs. NVIDIA) is unknown.

## Q5. GQE — NVIDIA's own proof-of-concept GPU SQL engine (rapidsai/gqe)

### Takeaway
`rapidsai/gqe` is architecturally almost a twin of the startup's engine: DataFusion SQL → Substrait → C++ GPU executor, served over Arrow Flight SQL. It adds multi-GPU through NVSHMEM and in-memory tables compressed with nvCOMP. It is labelled a "proof-of-concept" and "blueprint" and was published as a single code drop on 2026-08-10. It is a reference design and an idea source, but also proof that NVIDIA has internally built what the startup is building.

### Cited Findings
- "`GQE` is a proof-of-concept SQL query engine for running data analytics queries on GPUs. Its purpose is to achieve speed-of-light performance and serve as a blueprint to inform your database engine design." Claimed result: "Total GQE execution time for all 22 queries at scale factor of 1000: 9.15 s" (single GPU, vs. DuckDB) — [rapidsai/gqe README](https://github.com/rapidsai/gqe) (accessed 2026-10-28).
- **Features:**
  - Node manager spawning one task manager per GPU (`--num-gpus`), with NVSHMEM bootstrap.
  - `CREATE TABLE` + `COPY … (FORMAT parquet)` for in-memory tables, and `EXTERNAL … STORED AS PARQUET` tables.
  - Env knobs: `GQE_COMPRESSION_FORMAT` (nvCOMP ans/lz4/snappy/cascaded/bitcomp/zstd…), `GQE_USE_PARTITION_PRUNING`, `GQE_JOIN_USE_HASH_MAP_CACHE`, `GQE_PARQUET_CHUNK_READ_SIZE`/`PASS_READ_SIZE` (chunked Parquet reader), and a "customized Parquet reader".
  - Same source.
- **Stack:** `rust/Cargo.toml` declares `datafusion = 52.3.0`, `datafusion-substrait = 52.3.0`, `substrait = 0.62.2`, `arrow-flight` (flight-sql-experimental) 57.3.0. Crate descriptions: "DataFusion Flight SQL example using Substrait as the intermediate plan format" and "SQL to Substrait producer for GQE". The C++ side has `include/gqe/logical/from_substrait.hpp` — [rapidsai/gqe rust/](https://github.com/rapidsai/gqe/tree/main/rust) (accessed 2026-10-28).
- **Licence:** GitHub reports "Other". The LICENSE says "This repository is covered under the Apache 2.0 License terms below. Building or using the software requires additional components licenced under other terms … CUPTI, nvCOMP, and NVSHMEM." — [rapidsai/gqe LICENSE](https://github.com/rapidsai/gqe/blob/main/LICENSE) (accessed 2026-10-28).
- **Activity:** created 2025-12-11 (private until release, estimate). The only public commit is "Initial commit" (2026-08-10, nvidia.com author); last push 2026-08-11; 62 stars — [rapidsai/gqe commits](https://github.com/rapidsai/gqe/commits/main) (accessed 2026-10-28).

### Inferences
- GQE's design (DataFusion front end, Substrait, C++ executor, in-VRAM compressed tables, multi-GPU) overlaps strongly with **datafusion-plugin** and **realtime-vram-cache**. It is not a product today: one code drop and a "PoC" label. But NVIDIA could promote it, or a well-funded team could fork it (estimate). It also shows that "DataFusion + libcudf executor" is not a defensible technical moat on its own.
- GQE's reliance on proprietary nvCOMP/NVSHMEM shows NVIDIA keeps some performance-critical pieces closed (see Q7).

### Gaps
- I found no announcement or blog post for GQE and could not tell whether NVIDIA plans further development.

## Q6. cuVS (vector search) and its integrations

### Takeaway
cuVS is Apache-2.0, very active (v26.10.00; 492 commits since April) and now in the NVIDIA org, with C/C++/Python/Rust/Go/Java bindings. It is integrated into Faiss, Milvus, Elasticsearch, OpenSearch (incl. Amazon OpenSearch Service), Solr/Lucene, Oracle AI Database 26ai, Kinetica, KDB.AI and CyborgDB. It is an excellent reusable component for **vector-sql**. But GPU vector search is already a checkbox for the major vector databases and search engines, which makes "vector search on GPU" a weak standalone differentiator.

### Cited Findings
- **Repo:** NVIDIA/cuvs, Apache-2.0, 877 stars, v26.10.00 (2026-10-08). Since 2026-04-01: 492 commits by 75 authors (nvidia.com 171, noreply 171, gmail 85, bradleydice.com 47). Language dirs: `c`, `cpp`, `python`, `rust`, `go`, `java` — [NVIDIA/cuvs](https://github.com/NVIDIA/cuvs) (accessed 2026-10-28).
- **Official integrations page:**
  - Databases: Amazon OpenSearch Service (GPU index builds on OpenSearch 3.1+ and Serverless), CyborgDB, Elasticsearch (GPU HNSW index construction), Kinetica (CAGRA indexes on vector columns, SQL vector operators), KX/KDB.AI, Milvus (GPU_CAGRA, GPU_IVF_FLAT, GPU_IVF_PQ, GPU_BRUTE_FORCE; CPU search over a GPU-built graph in 2.6.4+), OpenSearch (cuVS via Faiss, CAGRA→HNSW), Oracle AI Database 26ai (Vector Index Service builds indexes with cuVS), Solr (via `cuvs-lucene`).
  - Data platforms: Cloudian, DDN, Dell AI Data Platform (cuVS plus cuDF), MinIO, NetApp, Everpure, WEKA.
  - Libraries: Faiss, KIOXIA AiSAQ, Lucene.
  - [cuVS fern/pages/integrations.md](https://github.com/NVIDIA/cuvs/blob/main/fern/pages/integrations.md) (accessed 2026-10-28).
- **Lucene codec:** `NVIDIA/cuvs-lucene` (Apache-2.0, created 2025-05-08, last push 2026-09-17), "A Lucene codec for vector search and clustering on the GPU". The integrations page says development now lives in `java/cuvs-lucene` in the cuVS repo — [NVIDIA/cuvs-lucene](https://github.com/NVIDIA/cuvs-lucene) (accessed 2026-10-28).
- **Not on the official list:** Weaviate, DuckDB, pgvector/PostgreSQL (except via CyborgDB) — same source. Sirius's README lists cuVS among its building blocks — [Sirius README](https://github.com/sirius-db/sirius/blob/main/README.md) (accessed 2026-10-28).

### Inferences
- **vector-sql:** cuVS is a ready component, but Kinetica (GPU SQL plus CAGRA), Oracle 26ai, Milvus and Elastic already combine vector search and analytics with NVIDIA support. A startup would need a distinct angle, e.g. joins and aggregation over vector-search results in one GPU plan (estimate). If Sirius exposes cuVS through SQL, it would compete directly. I did not verify whether it does.

### Gaps
- Whether Sirius actually exposes vector-search SQL functions today was not verified; the README only lists cuVS as a building block.
- A Weaviate GPU integration was not found on the official list and could not be searched further.

## Q7. Infrastructure libraries: RMM, KvikIO, nvCOMP, cuCollections, CCCL, quent

### Takeaway
RMM, KvikIO, cuCollections and CCCL are permissively licensed and active, and are safe building blocks. KvikIO now covers S3, HTTP and WebHDFS remote reads plus GPUDirect Storage. **nvCOMP is not open source**: its GitHub repo was archived in July 2026 and binaries remain under NVIDIA's proprietary licence. That matters because libcudf, GQE and ClickHouse's GPU work use it for compressed-data paths.

### Cited Findings
- **RMM:** rapidsai/rmm, Apache-2.0, v26.10.00 (2026-10-07). 182 commits since April by 28 authors (bradleydice.com 81, noreply 36, nvidia.com 34) — [rapidsai/rmm](https://github.com/rapidsai/rmm) (accessed 2026-10-28).
- **KvikIO:** rapidsai/kvikio, Apache-2.0, v26.10.00 (2026-10-07). 124 commits since April by 27 authors. It provides "C++ and Python bindings to cuFile, which enables GPUDirect Storage (GDS)" and "works efficiently when GDS isn't available" — [rapidsai/kvikio README](https://github.com/rapidsai/kvikio) (accessed 2026-10-28). `remote_handle.hpp` defines `RemoteEndpointType` {S3, S3_PUBLIC, S3_PRESIGNED_URL, WEBHDFS, …}, `HttpEndpoint` and `S3Endpoint`; `hdfs.hpp` also exists — [kvikio remote_handle.hpp](https://github.com/rapidsai/kvikio/blob/main/cpp/include/kvikio/remote_handle.hpp) (accessed 2026-10-28).
- **nvCOMP:**
  - The README now reads "# nvCOMP (archived) This repository is **archived**. It no longer contains nvCOMP examples or benchmarks…". Examples have moved to `NVIDIA/CUDALibrarySamples/nvCOMP`, and "Binary packages and documentation are available on the nvCOMP Developer Page". GitHub reports `archived: true`, last push 2026-07-13, licence NOASSERTION — [NVIDIA/nvcomp](https://github.com/NVIDIA/nvcomp) (accessed 2026-10-28).
  - Conda packages are tagged `LicenseRef-nvCOMP-Software-License-Agreement AND LicenseRef-NVIDIA-End-User-License-Agreement` — [conda-forge libnvcomp](https://anaconda.org/conda-forge/libnvcomp) (accessed 2026-10-28, via search summary).
  - nvCOMP 5.1.0 (December 2025) is the latest on the archive page — [nvCOMP Archive](https://developer.nvidia.com/nvcomp-archive) (accessed 2026-10-28, via search summary; may be outdated).
- **cuCollections:** NVIDIA/cuCollections, Apache-2.0, 674 stars, no releases (header-only). 42 commits since April by 9 authors — [NVIDIA/cuCollections](https://github.com/NVIDIA/cuCollections) (accessed 2026-10-28).
- **CCCL:** NVIDIA/cccl, 2,558 stars, latest release python-1.3.0 (2026-10-28). The licence file covers Thrust ("Apache Licence v2.0, with some specific exceptions") and libcu++ ("Apache License v2.0 with LLVM Exceptions") — [NVIDIA/cccl LICENSE](https://github.com/NVIDIA/cccl/blob/main/LICENSE) (accessed 2026-10-28).
- **quent:** rapidsai/quent (Apache-2.0, created 2026-02-20, pushed 2026-10-28), "An experimental framework to help build application-specific performance analysis tools", with a "query engine UI" demo. Sirius integrates it (issue on "Quent/NVTX injection") — [rapidsai/quent](https://github.com/rapidsai/quent), [sirius#1873](https://github.com/sirius-db/sirius/issues/1873) (accessed 2026-10-28).

### Inferences
- **lakehouse-iceberg** can lean on KvikIO S3/HTTP plus libcudf hybrid_scan instead of writing its own object-store I/O. This removes a gap in the startup's engine ("local Parquet only").
- **realtime-vram-cache** can reuse cuCascade (tiered GPU/host/disk) plus RMM. But compressed VRAM-resident data depends on nvCOMP, which is proprietary (redistributable binary, NVIDIA licence). That is a licensing and lock-in consideration, not a blocker (estimate).

### Gaps
- I did not read the nvCOMP redistribution terms in detail.

## Q8. Other NVIDIA data projects (brief): Morpheus, NeMo Curator, cuML, cuGraph, others

### Takeaway
These are mostly adjacent rather than competitive. Morpheus has slowed (last release September 2025). NeMo Curator, cuML, cuGraph and cuOpt ship regularly. None threatens the GPU SQL directions directly.

### Cited Findings
- **Morpheus:** nv-morpheus/Morpheus, Apache-2.0. Latest release v25.02.03 (2025-09-04), last push 2026-08-11 — [nv-morpheus/Morpheus](https://github.com/nv-morpheus/Morpheus) (accessed 2026-10-28).
- **NeMo Curator:** now `NVIDIA-NeMo/Curator`, Apache-2.0, 1,831 stars, v1.4.0 (2026-10-09) — [NVIDIA-NeMo/Curator](https://github.com/NVIDIA-NeMo/Curator) (accessed 2026-10-28).
- **cuML:** NVIDIA/cuml, Apache-2.0, 5,311 stars, v26.10.00 (2026-10-08) — [NVIDIA/cuml](https://github.com/NVIDIA/cuml) (accessed 2026-10-28).
- **cuGraph:** rapidsai/cugraph, Apache-2.0, v26.10.00 (2026-10-08) — [rapidsai/cugraph](https://github.com/rapidsai/cugraph) (accessed 2026-10-28).
- **cuOpt:** NVIDIA/cuopt, Apache-2.0, v26.10.00 (2026-10-14) — [NVIDIA/cuopt](https://github.com/NVIDIA/cuopt) (accessed 2026-10-28).
- **New in 2026, NVIDIA org:**
  - `NVIDIA/kumo-engine` (Apache-2.0, created 2026-10-02): "an accelerator for context generation over structured data, designed for teams building and running predictive models" — [NVIDIA/kumo-engine](https://github.com/NVIDIA/kumo-engine) (accessed 2026-10-28).
  - `NVIDIA/structured-data-models` (2026-06-15): "Foundation Models for Structured Data" — [NVIDIA/structured-data-models](https://github.com/NVIDIA/structured-data-models) (accessed 2026-10-28).
  - `rapidsai/nvforest` (2025-11-04): GPU decision-tree inference — [rapidsai/nvforest](https://github.com/rapidsai/nvforest) (accessed 2026-10-28).

### Inferences
- NVIDIA's 2026 structured-data push leans toward AI over tables (foundation models, Kumo-style context generation) as well as SQL acceleration (estimate).

### Gaps
- None material to the SQL directions.

## Q9. Newer 2025–2026 activity: ClickHouse GPU, DuckDB, DataFusion/Comet, Iceberg, Flink, Doris/StarRocks/Databend, Theseus

### Takeaway
The biggest 2026 development: **ClickHouse Inc. is building GPU execution in-tree on cuDF and cuCascade, with Sirius offload as step one.** cuDF and cuCascade were merged into `contrib/` in September 2026, and GPU aggregation, join and MergeTree-reader PRs are open. I found no NVIDIA GPU work in DataFusion, Comet, Flink or Databend. NVIDIA's lakehouse GPU reading is centred on cudf-spark (Iceberg/Delta) and libcudf deletion vectors.

### Cited Findings
- **ClickHouse RFC #116807, "[RFC] ClickHouse GPU v1"** (2026-08-27, Konstantin Vedernikov, public ClickHouse org member):
  - "There is a project called SiriusDB … As a first step, we would like to build ClickHouse on top of Sirius."
  - Option 1: reuse Sirius through its Substrait entry point. Listed limitations: loss of PREWHERE and primary-key granule pruning, four lossy plan representations, MergeTree codec gaps.
  - Option 2: "ClickHouse internals integration", lifting Sirius components as-is: tiered memory over cuCascade, downgrade executors, the "Simpatico" GPU codec library, an io_uring/kvikio/GDS/S3 I/O layer, and custom CUDA kernels. It maps ClickHouse columns to cuDF types.
  - A ClickHouse Inc. engineer (Sergei Trifonov) commented asking for GPU memory tracking as "a first-class citizen".
  - [ClickHouse#116807](https://github.com/ClickHouse/ClickHouse/issues/116807) (accessed 2026-10-28).
- **ClickHouse RFC #124455, "GPU Offload of Query Plan Subtrees to Sirius"** (2026-10-07, Raufs Dunamalijevs, company "@ClickHouse"):
  - Offloads the largest Substrait-serializable subtree to a `SiriusStep`.
  - A separate "GPU backend process" holds a "GPU cache, keyed by (part, column, data version)" filled from the MergeTree reader.
  - "A GPU-path failure produces a clear error with no CPU fallback."
  - Cites Sirius as "reported to be 38 times faster than ClickHouse on TPC-H SF100 on a single node".
  - [ClickHouse#124455](https://github.com/ClickHouse/ClickHouse/issues/124455) (accessed 2026-10-28).
- **ClickHouse GPU PRs:**
  - Merged: "[GPU] add cudf contrib" (#117869, opened 2026-09-03), "[GPU] Add cuCascade to contrib" (#120803, 2026-09-18), "[GPU] speedup build" (#120421), "[GPU] utils to work with GPU memory and cuDF 1/N" (#122867, 2026-09-29), "[GPU] upgrade cuDF & CCCL to include streaming aggregations optimizations" (#127166, 2026-10-21), "[GPU] `GDeflate` compression codec on the CPU" (#127422, 2026-10-22).
  - Open: "[GPU] Support aggregation functions" (#119502), "[GPU] inner hash joins" (#121061), "[GPU] MergeTree compressed blocks reader" (#127604, 2026-10-23).
  - [ClickHouse PRs search "[GPU]"](https://github.com/ClickHouse/ClickHouse/pulls?q=%5BGPU%5D) (accessed 2026-10-28).
- **DuckDB:** NVIDIA's DuckDB work is Sirius (Q4). I found no separate NVIDIA DuckDB repo in the NVIDIA org listing of repos created since 2025 — [github.com/NVIDIA repositories](https://github.com/orgs/NVIDIA/repositories) (accessed 2026-10-28).
- **DataFusion / Comet:** GitHub issue searches for "cudf" in apache/datafusion and "GPU" in apache/datafusion-comet found no GPU-execution work. Comet 1.1.0 (2026-10-06) is CPU-native — [apache/datafusion issues](https://github.com/apache/datafusion/issues?q=cudf), [apache/datafusion-comet](https://github.com/apache/datafusion-comet/releases) (accessed 2026-10-28). The only NVIDIA DataFusion use found is GQE's DataFusion 52 front end (Q5).
- **Flink:** a GitHub issue search for "GPU" in apache/flink returned nothing relevant — [apache/flink issues](https://github.com/apache/flink/issues?q=GPU) (accessed 2026-10-28). The cuDF repo still carries `cudf_kafka`/`custreamz` (Python streaming over Kafka) — [NVIDIA/cudf python/](https://github.com/NVIDIA/cudf/tree/main/python) (accessed 2026-10-28).
- **StarRocks / Doris / Databend:**
  - StarRocks "Integrate with CUDA" (#45508, 2024) and Databend "Feature: Support GPU" (#15606, 2024) are both closed. No Doris GPU issue was found.
  - Sirius ships experimental StarRocks and Doris backends (Q4).
  - [StarRocks#45508](https://github.com/StarRocks/starrocks/issues/45508), [databend#15606](https://github.com/databendlabs/databend/issues/15606) (accessed 2026-10-28).
- **Iceberg:** no GPU issue in apache/iceberg. NVIDIA's Iceberg GPU reading lives in cudf-spark's `iceberg/` modules and S3 byte-range reader (Q2). Delta-style deletion vectors are in the libcudf experimental reader (Q1).

### Inferences
- The **clickhouse** direction is now largely pre-empted. ClickHouse Inc. is doing the integration itself in its own repo, on NVIDIA/Sirius components, with a GPU column cache design (strong threat). The same pattern (Sirius as a backend node) is already prototyped for Doris and StarRocks (**realtime-other**).
- **datafusion-plugin**, **streaming-flink** and **distributed-pluggable** (Databend) show no NVIDIA or upstream GPU activity. These are white space, though also unproven demand (estimate).
- **other-accelerators:** a third-party repo, `mycomputercrashsalot/cuda2rocm` (pushed 2026-10-27), ports cuCollections and cuCascade to ROCm. That hints at grassroots demand for a non-NVIDIA cuDF-like stack (weak signal) — [cuda2rocm](https://github.com/mycomputercrashsalot/cuda2rocm) (accessed 2026-10-28).

### Gaps
- Theseus (Voltron Data) 2026 status, Polars Cloud GPU, and any NVIDIA–Databricks Photon GPU work: not checked, because the web-search budget was exhausted.
- I did not check employer affiliations of ClickHouse GPU PR authors beyond the two RFC authors (both public ClickHouse org members).

## Q10. Direction-by-direction summary (help as component / threat as competitor)

### Takeaway
NVIDIA's open-source strategy puts components into other people's engines: cuDF in Velox, Gluten, ClickHouse and Sirius, plus cudf-spark for Spark. The free components help every direction. But engines that receive NVIDIA staffing (Sirius, Velox, cudf-spark) threaten **clickhouse**, **spark-plugin**, **realtime-other** and **distributed-pluggable** most. The least contested by NVIDIA open source are **datafusion-plugin**, **streaming-flink** and **other-accelerators**.

### Cited Findings (each row rests on the sources cited in Q1–Q9)
| Direction | Helps (reusable components) | Threatens (OSS competitors) |
|---|---|---|
| clickhouse | libcudf, cuCascade, kvikio | ClickHouse in-tree GPU on cuDF/cuCascade + Sirius offload (Q9) — **strong** |
| lakehouse-iceberg | libcudf hybrid_scan + deletion vectors, kvikio S3/HTTP, rapidsmpf | cudf-spark Iceberg/Delta (Q2); Velox-cuDF Presto/Gluten (Q3) — **strong in Spark/Presto shops** |
| realtime-vram-cache | cuCascade tiering, RMM, nvCOMP (proprietary), GQE design ideas | GQE in-memory compressed tables (PoC); ClickHouse RFC GPU column cache (Q9) — **medium** |
| streaming-flink | libcudf_streaming, rapidsmpf, cudf_kafka/custreamz | none found — **low** |
| datafusion-plugin | libcudf, libcudf_streaming; GQE shows DataFusion → Substrait → GPU | GQE (PoC, single code drop) — **low–medium** |
| realtime-other (Doris/StarRocks) | libcudf, cuCascade | Sirius experimental Doris and StarRocks backends (Q4) — **strong** |
| distributed-pluggable | rapidsmpf, UCX exchange ideas from Velox | Velox-cuDF (any Velox engine); Sirius multi-engine plan — **medium** |
| spark-plugin | cudf-spark-jni (Apache-2.0) | cudf-spark (Q2), Gluten-cuDF (Q3) — **very strong** |
| other-accelerators | none directly (CUDA-only stack) | none from NVIDIA; ROCm ports of cuCollections/cuCascade appear in the community — **low** |
| vector-sql | cuVS (C/C++/Rust/Go/Java/Python) | cuVS already in Milvus, Elastic, OpenSearch, Oracle 26ai, Kinetica; Sirius lists cuVS — **medium** |

### Inferences
- If the startup builds on its own engine, the most valuable NVIDIA components are libcudf_streaming/rapidsmpf (out-of-core and multi-GPU), the kvikio remote endpoints (to fix "local Parquet only"), and cuCascade (tiered memory and spill). All are Apache-2.0.
- If it builds on Sirius, it inherits the Doris, StarRocks and ClickHouse momentum. But it competes with an NVIDIA-staffed upstream that downstream vendors (ClickHouse) already integrate directly (estimate).

### Gaps
- The threat levels in the table are my own judgement (estimate), based on the activity data above.
