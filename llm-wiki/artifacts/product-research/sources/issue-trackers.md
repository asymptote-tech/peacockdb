# Source: issue trackers (GitHub issues and discussions, Apache JIRA)

## Method

- All links accessed 2026-10-28. Date range: 2023-10-01 to 2026-10-28. Older threads appear only when marked "(older)".
- **GitHub issue search (REST `search/issues`).** I ran four queries in each of 52 general repos, with `created:>=2023-10-01`: `GPU`, `CUDA`, `cudf OR rapids`, and `"spark-rapids" OR sirius OR heavydb OR omnisci OR kinetica OR sqream OR blazingsql`.
  - The repos are the requested list plus apache/spark, apache/gluten, apache/paimon, apache/hudi, apache/kylin, apache/sedona, apache/arrow-rs, apache/datafusion-python, apache/datafusion-ballista, ibis-project/ibis, duckdb/duckdb-python, lance-format/lance, vortex-data/vortex, spiceai/spiceai, influxdata/influxdb, VictoriaMetrics/VictoriaMetrics, modin-project/modin, dask/dask, dask/dask-expr, narwhals-dev/narwhals and uber/h3.
- **GPU projects.** Eight queries per repo:
  - `slow OR slower OR performance`
  - `parquet OR reader OR IO`
  - `cache OR caching OR memory`
  - `CPU faster OR "slower than"`
  - `fallback OR "not supported"`
  - `production OR "use case" OR deploy`
  - `request OR feature OR support`
  - an unfiltered query, three pages deep
- The GPU projects searched: NVIDIA/cudf-spark (the new name of NVIDIA/spark-rapids; there is no separate repo), NVIDIA/cudf (the new name of rapidsai/cudf), NVIDIA/cudf-spark-tools, NVIDIA/cuml-spark, sirius-db/sirius, heavyai/heavydb, rapidsai/gqe, rapidsai/dask-cuda, rapidsai/cuspatial and heterodb/pg-strom.
- NVIDIA/cudf got three extra queries: `polars`, `duckdb OR sql`, and `"cudf.pandas" OR "cudf-polars"`.
- **Label queries.**
  - cudf-spark: `label:question`, `label:"? - Needs Triage"`, `label:performance`.
  - cudf: `label:question`, `label:External`, `label:cudf-polars`, `label:cudf.pandas`, `label:cuIO label:Performance`.
- **GitHub Discussions (GraphQL `search(type: DISCUSSION)`).** The same terms, without the date filter.
- **Global (no repo) queries**, issues and discussions:
  - `"GPU acceleration" SQL`, `"GPU database"`, `"GPU SQL"`, `"GPU" "query engine"`, `cudf polars`
  - `"GPU join"`, `"GPU parquet"`, `sirius duckdb GPU`, `"spark-rapids" slower`
  - `heavydb`, `kinetica`, `sqream`, `blazingsql`, `"pg-strom"`
  - `"GPU" clickhouse`, `"GPU" datafusion`, `"GPU" iceberg`, `"GPU" "time series"`, `"GPU" logs query`, `"GPU" OLAP`
- **Full issue listings (REST `repos/*/issues`, with `author_association`)**, used to separate outside users from maintainers:
  - sirius (591 issues since 2023-10; 16 by outside users)
  - heavydb (53; 42 outside)
  - gqe (2; both outside)
  - cudf-spark (3,398; 77 outside)
  - cudf-spark-tools (848; 15 outside)
  - cudf, filtered by the labels above (676; 559 non-member)
- **Apache JIRA REST.** JQL `project = X AND text ~ "<term>"` for SPARK, FLINK, ARROW, HIVE, IMPALA, KYLIN, CALCITE, DRILL and IGNITE, with terms GPU, CUDA and RAPIDS. This gave 787 unique tickets. ARROW moved to GitHub in 2022, so its JIRA hits are all older. HIVE, IMPALA and KYLIN have no GPU-SQL demand after 2023.
- **Volume.**
  - 303 issue-search calls and 362 discussion-search calls.
  - 8,469 unique GitHub items fetched; 6,452 mention a GPU term.
  - 111 threads read in full, with comments and +1 counts. Many more were triaged from title and body excerpt.
- **Judged relevant: 100 individual threads (102 hits), plus bulk families counted separately:**
  - Velox cuDF work, about 70 issues
  - Presto cuDF PRs, about 30
  - Lance GPU indexing, about 40
  - Milvus GPU-index bugs, about 20
  - Gluten GPU status notes, 16
  - PG-Strom partner crash reports, about 12
  - CPU-vs-GPU fuzzing bug reports in Polars and HeavyDB, about 65
- **What failed.**
  - 84 search calls returned HTTP 422 because of renamed or missing repos (rapidsai/cudf, NVIDIA/spark-rapids-tools, NVIDIA/spark-rapids-ml, voltrondata/theseus). I re-ran them under the new names; voltrondata/theseus does not exist publicly.
  - One GraphQL call hit a transient error and was retried.
  - No secondary rate limits.
  - Global queries were mostly noise: GitHub matches the words separately. I kept only hits where both the title and the body were about GPU plus SQL or dataframes.
- **Not reachable.** Kinetica and SQream have no public trackers with user traffic. Theseus is private. BlazingSQL has been dead since 2021.

## Counts

Individual judged threads only. The bulk families listed in Method are excluded.

| slug | request | user | complaint | vendor | other | total |
|---|---|---|---|---|---|---|
| spark-plugin | 1 | 2 | 12 | 3 | 1 | 19 |
| gpu-bi-dashboards | 3 | 1 | 2 | 2 | 0 | 8 |
| workstation-gpu-sql | 1 | 0 | 5 | 1 | 1 | 8 |
| lakehouse-iceberg | 3 | 0 | 1 | 3 | 0 | 7 |
| datafusion-plugin | 2 | 1 | 1 | 1 | 1 | 6 |
| clickhouse | 2 | 0 | 0 | 3 | 0 | 5 |
| gpu-scan-tier | 0 | 1 | 2 | 0 | 2 | 5 |
| new:dataframe-gpu | 2 | 0 | 3 | 0 | 0 | 5 |
| other-accelerators | 3 | 1 | 0 | 0 | 1 | 5 |
| postgres-gpu-analytics | 0 | 1 | 3 | 0 | 0 | 4 |
| vector-sql | 2 | 0 | 1 | 0 | 1 | 4 |
| asof-gpu | 2 | 0 | 0 | 1 | 0 | 3 |
| geo-join-gpu | 1 | 1 | 1 | 0 | 0 | 3 |
| gpu-sql-migration-qualifier | 0 | 3 | 0 | 0 | 0 | 3 |
| new:gpu-resident-results | 2 | 1 | 0 | 0 | 0 | 3 |
| realtime-other | 1 | 0 | 0 | 2 | 0 | 3 |
| distributed-pluggable | 1 | 0 | 0 | 1 | 0 | 2 |
| new:htap-gpu | 0 | 2 | 0 | 0 | 0 | 2 |
| realtime-vram-cache | 1 | 0 | 1 | 0 | 0 | 2 |
| trainer-side-sql-prep | 1 | 1 | 0 | 0 | 0 | 2 |
| new:inference-in-db | 1 | 0 | 0 | 0 | 0 | 1 |
| new:tsdb-gpu | 1 | 0 | 0 | 0 | 0 | 1 |
| streaming-flink | 0 | 0 | 0 | 1 | 0 | 1 |

- Totals: complaint 32, request 30, vendor 18, user 15, other 7.
- Bulk families:
  - vendor: distributed-pluggable (Velox and Presto, about 100); vector-sql (Lance, about 40); spark-plugin (Gluten, 16)
  - complaint: vector-sql (Milvus, about 20)
  - user and complaint: postgres-gpu-analytics (PG-Strom partner reports, about 12)
  - other: CPU-vs-GPU fuzzing reports (about 65)

## Strongest threads

First-person posts with a concrete workload come first, then vendor moves. Each quote is at most 25 words.

| date | system | slug | kind | use case | quote | link |
|---|---|---|---|---|---|---|
| 2026-03-24 | Polars/cuDF vs DuckDB | geo-join-gpu | complaint | UC Berkeley: H3 joins over hive-partitioned parquet on S3/MinIO; DuckDB 5.2 s vs GPU 23.5 s; GPU engine silently fell back to CPU | "GPU runs must hard-disable CPU fallback or they silently measure CPU." | https://github.com/boettiger-lab/mcp-data-server/issues/227 |
| 2025-09-08 | spark-rapids | realtime-vram-cache | request | repeated queries over the same input | "the input data always need to be transferred from the host memory to the device memory again between consecutive queries?" (answer: yes, unsupported) | https://github.com/NVIDIA/cudf-spark/discussions/13435 |
| 2024-06-14 | spark-rapids | gpu-bi-dashboards | user | Spark Thrift Server + RAPIDS serving SQL (Superset) over a 9.6B-row Delta table, Azure T4s; GPU OOM on DISTINCT | "We are using Spark Rapids + Spark Thrift Server to serve SQL request" | https://github.com/NVIDIA/cudf-spark/issues/11064 |
| 2023-11-27 | spark-rapids | gpu-bi-dashboards | complaint | Superset → Thrift Server → RAPIDS breaks with multiple sessions | "Our objective is to activate Spark Rapids (SQLPlugin) with Spark Thrift Server." | https://github.com/NVIDIA/cudf-spark/discussions/9966 |
| 2025-01-21 | spark-rapids | spark-plugin | complaint | ETL on AWS g4dn vs r6id; 24 GB gzip parquet, 40M rows; target was half the cost | "Achieve 50% compute cost reduction of basic ETL pipeline" (not achieved) | https://github.com/NVIDIA/cudf-spark/discussions/11992 |
| 2025-01-14 | spark-rapids | spark-plugin | complaint | same user: CPU pegged even with all operators on GPU | "I see utilization 80-90% for the most of the time time of the query." | https://github.com/NVIDIA/cudf-spark/discussions/11963 |
| 2024-09-06 | spark-rapids | spark-plugin | complaint | Cloudera: NDS on HGX 8×H100 reading remote HDFS; GPUs at 15% | "the bottleneck is my HDFS/network and it is limiting the speed of the GPU acceleration." | https://github.com/NVIDIA/cudf-spark/discussions/11431 |
| 2026-04-10 | Sirius | workstation-gpu-sql | complaint | TPC-H SF10/100 on GB10, RTX 5090, A6000 vs DuckDB | "for all 22 TPC-H experiments, Sirius was slower than the CPU implementation on DuckDB." | https://github.com/sirius-db/sirius/issues/609 |
| 2026-04-10 | Sirius | workstation-gpu-sql | complaint | A100 SF100; 0.83× until an RMM bug was fixed, then about 10× | "average speedup of only 0.83× (i.e., Sirius is ~20% slower on average)." | https://github.com/sirius-db/sirius/issues/609 |
| 2026-10-14 | Sirius | workstation-gpu-sql | complaint | DuckDB vs Sirius on EMR g7.4xlarge, TPC-DS/TPC-H SF100 | "after moving to `latest` the answers were right but every query quietly ran on the CPU." | https://github.com/sirius-db/sirius/issues/2174 |
| 2026-05-05 | libcudf-rs (DataFusion) | datafusion-plugin | user | Datadog engineer profiling cuDF-backed DataFusion on TPC-H | "bottlenecked by runtime orchestration and host/device transfer overhead rather than raw GPU kernel time." | https://github.com/gabotechs/libcudf-rs/issues/49 |
| 2026-05-05 | Arrow/DataFusion | datafusion-plugin | complaint | own cuda-oxide kernels, RTX 5080 vs 9950X3D | "the data CPU<>GPU data transfers dominate compute time, and a strong CPU beats the GPU most of the time." | https://github.com/gabotechs/libcudf-rs/issues/49 |
| 2026-06-04 | Beacon (DataFusion) | datafusion-plugin | request | scientific data server (Zarr/NetCDF/Parquet) eyeing libcudf-rs | "Investigate integrating libcudf-rs as an optional GPU execution backend for Beacon queries." | https://github.com/maris-development/beacon/issues/231 |
| 2026-06-30 | cudf-polars | new:gpu-resident-results | request | Graphistry: Cypher/GFQL graph queries on polars-gpu | "there's no supported way to keep the result of a query on the GPU." | https://github.com/NVIDIA/cudf/issues/23039 |
| 2023-12-08 | HeavyDB | new:gpu-resident-results | user | production HeavyDB 6.4 with custom patches; results handed over in GPU memory | "using gpu shared memory is required and cannot be discarded." | https://github.com/heavyai/heavydb/issues/819 |
| 2026-01-15 | DuckDB | other-accelerators | user | Coco Alemana founder writing MLX/Metal aggregates inside a DuckDB extension | "I'm trying to make some use-case specific aggregates, joins and filter functions run on the GPU." | https://github.com/duckdb/duckdb/discussions/20558 |
| 2023-08-23 | DuckDB | workstation-gpu-sql | request | (older, +3) local 25–50 GB, big distinct joins and group-bys | "difficulties in handling workloads that require joining or grouping large amounts of distinct values." | https://github.com/duckdb/duckdb/discussions/8675 |
| 2026-09-11 | Sirius | lakehouse-iceberg | request | Qbeast field CTO: large append-only Iceberg tables on S3 | "large, append-only Iceberg tables in mind, where performance tends to come from early file/unit skipping." | https://github.com/sirius-db/sirius/issues/1760 |
| 2026-10-20 | spark-rapids | spark-plugin | complaint | production job with cached frames + GPU broadcast hangs | "An executor deadlocks when Spark's MemoryStore evicts a GPU broadcast block to make room for a cached block." | https://github.com/NVIDIA/cudf-spark/issues/16460 |
| 2026-10-14 | spark-rapids | spark-plugin | user | ByteDance, Spark on Kubernetes, 4 L20 nodes; native-memory OOM kills | "Kubernetes, four L20 GPU worker nodes" | https://github.com/NVIDIA/cudf-spark/issues/16321 |
| 2025-01-13 | Polars | new:dataframe-gpu | complaint | small join 0.1 s CPU vs 7 s GPU; transfer dominates | "For now we will stick to CPU." | https://github.com/pola-rs/polars/issues/20696 |
| 2023-11-27 | cudf.pandas | new:dataframe-gpu | complaint | Sportmaster: 280M-row groupby on V100 silently runs on CPU | "It's not clear how cudf decides to use CPU or GPU in calculations." | https://github.com/NVIDIA/cudf/issues/14500 |
| 2025-11-25 | cudf-polars | asof-gpu | request | time-series asof joins | "would significantly improve the usability of time-series workflows on GPU." | https://github.com/NVIDIA/cudf/issues/20727 |
| 2025-10-19 | cudf-polars | asof-gpu | request | G-Research: fixed-size rolling windows | "there's currently no support for fixed size rolling windows." | https://github.com/NVIDIA/cudf/issues/20307 |
| 2024-06-12 | cuDF/kvikio | gpu-scan-tier | user | Fermilab HEP: GPU-direct ROOT file reading, remote access needed | "cuDF's cuFile and nvcomp based file i/o is extremely powerful (recently benchmarked this ourselves)." | https://github.com/NVIDIA/cudf/issues/16004 |
| 2025-07-25 | cuDF | trainer-side-sql-prep | user | embeddings in parquet → cuML, forced through CPU | "This breaks the GPU-native pipeline and introduces unnecessary CPU-GPU data transfer" | https://github.com/NVIDIA/cudf/issues/19497 |
| 2025-12-15 | spark-rapids-tools | gpu-sql-migration-qualifier | user | trains the speedup predictor on own event logs | "Approximately 90% of the apps had an acceleration ratio greater than 1x, while only 10% had an acceleration ratio less than 1." | https://github.com/NVIDIA/cudf-spark-tools/issues/1995 |
| 2024-06-25 | PG-Strom | postgres-gpu-analytics | user | Apache Fineract (core banking) on PG16 with GPU-Cache | "I am now testing and understanding the PG Strom and its technical benefits." | https://github.com/heterodb/pg-strom/issues/796 |
| 2024-12-26 | HeavyDB | gpu-bi-dashboards | request | 5 servers × 4 L20; cluster mode is enterprise-only | "I have 5 GPU servers, each equipped with 4 NVIDIA L20 GPUs. How can I deploy heavydb in cluster mode?" | https://github.com/heavyai/heavydb/issues/842 |
| 2024-04-22 | HeavyDB | gpu-bi-dashboards | complaint | OSS edition stalled | "Even HeavyAI doesn't respond to license application emails anymore." | https://github.com/heavyai/heavydb/issues/825 |
| 2025-06-28 | Sirius | other-accelerators | request | Hot Aisle CEO: MI300 for more GPU memory (+2) | "AMD GPUs such as the MI300 series have 192GB+, it seems like supporting that hardware platform would be a good idea." | https://github.com/sirius-db/sirius/issues/53 |
| 2025-11-17 | Sirius | realtime-vram-cache | complaint | GPU-pinned tables go stale after UPDATE | "Updating value -> query still returns old value (stale cache)." | https://github.com/sirius-db/sirius/issues/106 |
| 2026-03-09 | Arc (DuckDB TSDB) | new:tsdb-gpu | request | time-series DB weighs Sirius, blocked by CPU-only fleet | "Most Arc deployments are CPU-only VMs/containers." | https://github.com/Basekick-Labs/arc/issues/260 |
| 2026-08-27 | ClickHouse | clickhouse | vendor | ClickHouse builds GPU execution on Sirius; 11+ GPU PRs (cuDF contrib, joins, aggregations, MergeTree GPU reader) | "As a first step, we would like to build ClickHouse on top of Sirius." | https://github.com/ClickHouse/ClickHouse/issues/116807 |
| 2026-10-06 | Sirius + Doris | realtime-other | vendor | Doris PMC chair's Sirius backend for an unmodified Doris FE, L40S | "SF100 took 154.4 s with Doris's own BE and 40.6 s with this backend." | https://github.com/sirius-db/sirius/issues/2025 |
| 2026-10-28 | DuckDB | workstation-gpu-sql | vendor | Sirius added as a DuckDB core extension | "add the Sirius loader and CUDA 12/13 backends to DuckDB's core extension build" | https://github.com/duckdb/duckdb/pull/28022 |
| 2026-07-24 | MatrixOne | distributed-pluggable | vendor | Substrait offload of MySQL-protocol queries to a Sirius sidecar | "Introduce a production-grade MatrixOne-to-Sirius sidecar path" | https://github.com/matrixorigin/matrixone/issues/26154 |
| 2026-09-17 | Paimon | lakehouse-iceberg | vendor | Paimon PMC wants Paimon tables in Sirius's GPU scan | "Paimon tables cannot be read on the GPU." | https://github.com/apache/paimon/issues/9923 |
| 2026-06-20 | Microsoft Fabric | gpu-bi-dashboards | vendor | Fabric Warehouse "GPU Query Acceleration" preview with a warm cache | "GPU Query Acceleration reached Early Access Preview." | https://github.com/sdebruyn/fabric-dw-mcp-cli/issues/595 |
| 2024-05-22 | Databend | distributed-pluggable | request | generic GPU ask, declined | "Databend is predominantly IO (S3)-bound rather than CPU-bound" | https://github.com/databendlabs/databend/issues/15606 |

## New directions

- **new:gpu-resident-results.** Keep query results on the GPU and hand them to the next GPU consumer (graph engine, ML, notebook) without a device-to-host copy.
  - Evidence: Graphistry, for Cypher/GFQL on polars-gpu (https://github.com/NVIDIA/cudf/issues/23039).
  - Evidence: a HeavyDB production user who relies on `select_ipc_gpu` and carries custom patches (https://github.com/heavyai/heavydb/issues/819).
  - Evidence: pgmpy asking Narwhals for GPU array export (https://github.com/narwhals-dev/narwhals/issues/4041).
  - This is close to trainer-side-sql-prep, but the consumer is a graph or analytics engine, not a trainer.
- **new:dataframe-gpu.** cudf.pandas and cudf-polars complaints: silent CPU fallback, slow small or wide frames, whole-plan fallback on one unsupported operator. NVIDIA owns this space. The pain is real, but it is not a SQL product.
  - https://github.com/pola-rs/polars/issues/29991
  - https://github.com/pola-rs/polars/issues/20696
  - https://github.com/NVIDIA/cudf/issues/14500
- **new:tsdb-gpu.** A DuckDB-based time-series DB considers Sirius, but its fleet is CPU-only (https://github.com/Basekick-Labs/arc/issues/260). This is one thread only.
- **new:htap-gpu.** MakeInfinite Labs, which leads an HTAP platform, files feature and bug issues against NVIDIA's GQE. That suggests a startup embedding a GPU query engine (https://github.com/rapidsai/gqe/issues/2).
- **new:inference-in-db.** Model inference on GPUs from inside the DB: ClickHouse (https://github.com/ClickHouse/ClickHouse/issues/58646) and seven Flink JIRA tickets for a GPU inference sidecar (FLINK-39625). This is GPU for UDF inference, not SQL acceleration.
- **Silent CPU fallback** recurs across Sirius, cudf-polars, cudf.pandas and Polars: users cannot tell whether the GPU ran. A "proof it ran on GPU" mode, or a hard-fail mode, is a small product idea. It fits gpu-sql-migration-qualifier.
  - https://github.com/sirius-db/sirius/issues/2174
  - https://github.com/boettiger-lab/mcp-data-server/issues/227
  - https://github.com/pola-rs/polars/issues/28842

## Leads

- **Carl Boettiger (`cboettig`, UC Berkeley).** H3 geospatial joins over S3 parquet in an MCP data server, already benchmarked. GPU lost to DuckDB 23.5 s vs 5.2 s; IO and fallback were the blockers. Concrete workload, public numbers, open to GPU. https://github.com/boettiger-lab/mcp-data-server/issues/227
- **Yu-Ting Lin (`LIN-Yu-Ting`).** Serves Superset SQL through Spark Thrift Server + RAPIDS over a 9.6B-row Delta table. An interactive BI-on-GPU need, hurt by Spark's session and OOM problems. https://github.com/NVIDIA/cudf-spark/issues/11064
- **Gene Bordegaray (`gene-bordegaray`, Datadog) and Gabriel (`gabotechs`, Distributed DataFusion author).** Building and profiling cuDF-backed DataFusion. These are the closest peers to peacockdb; partner or competitor. https://github.com/gabotechs/libcudf-rs/issues/49
- **Robin Kooyman (`robinskil`, MARIS Beacon).** DataFusion-based scientific data server; wants an optional GPU backend. https://github.com/maris-development/beacon/issues/231
- **Ryan (`prmelehan`, Coco Alemana founder).** Writes his own GPU aggregates for DuckDB, on Apple MLX. https://github.com/duckdb/duckdb/discussions/20558
- **Louie Meyerov (`lmeyerov`, Graphistry).** Wants GPU-resident query results for graph and Cypher queries. Already ships polars-gpu. https://github.com/NVIDIA/cudf/issues/23039
- **Andy Gong (`andygongyb`).** Asked to cache inputs in GPU memory across consecutive Spark queries. https://github.com/NVIDIA/cudf-spark/discussions/13435
- **Maksim Konstantinov (`MaxNevermind`).** Ran a cost-cut benchmark on AWS and missed the 50% target. Knows his ETL shape and costs. https://github.com/NVIDIA/cudf-spark/discussions/11992
- **Aaron Spiegel (`spiegela`, Qbeast) and Chen Junjie (`chenjunjiedada`, Tencent).** Large append-only Iceberg tables on S3; asking for file skipping and late materialization on GPU. https://github.com/sirius-db/sirius/issues/1760
- **Karthik Prabhakar (`knkarthik01`).** DuckDB vs Sirius on EMR at SF100; files fixes himself. https://github.com/sirius-db/sirius/issues/2174
- **Víctor Romero (`IOhacker`, Fintecheando).** Apache Fineract on PostgreSQL with PG-Strom GPU-Cache. https://github.com/heterodb/pg-strom/issues/796
- **`jieguolove`.** Owns 5×4 L20 servers and wants an open distributed GPU DB; HeavyDB's cluster mode is paid. https://github.com/heavyai/heavydb/issues/842
- **Melvin Koh (`melvin-koh`, Cloudera).** Benchmarks RAPIDS on HGX for Cloudera; found HDFS/network to be the limit. https://github.com/NVIDIA/cudf-spark/discussions/11431
- **Jeff Levesque (`jeff1evesque`).** Production Spark-RAPIDS with cached frames. https://github.com/NVIDIA/cudf-spark/issues/16460
- **Vendors moving into the slots:**
  - ClickHouse (`scanhex12`, `rienath`)
  - Doris (`morningman`, `dataroaring`)
  - MatrixOne (`aunjgr`)
  - Paimon (`SteNicholas`)
  - PolarDB (`realAaronWu`)
  - Presto/IBM (`majetideepak`)
  - Microsoft Fabric

## What this source says

- The loudest change since 2025 is vendor-side. ClickHouse, Doris, MatrixOne, Paimon, PolarDB and DuckDB core are all wiring in Sirius through Substrait, and Presto/Velox and Gluten are wiring in cuDF. The clickhouse, realtime-other and distributed-pluggable slots are being filled by the engines themselves.
- First-person complaints share one theme. GPU SQL loses to a good CPU engine on small or medium data, and IO, transfer and orchestration dominate: S3 or HDFS reads, host-to-device copies, CPU-side decode, runtime API overhead.
  - Users hit this in spark-rapids, Sirius, cudf-polars, and DataFusion+cuDF.
- Silent CPU fallback is a repeated trust problem. Several benchmarks measured the CPU without knowing it.
- Demand to keep data or results resident on the GPU between queries is explicit but thin (3–4 threads). spark-rapids says it is unsupported; Sirius has a pin cache with staleness bugs.
- BI-serving on GPU has one strong first-person user (Thrift Server + Superset) and stale HeavyDB users. Microsoft Fabric is shipping GPU query acceleration with a warm cache, which validates the shape but adds a big competitor.
- DataFusion-plus-GPU has two active builders (Datadog, Gabriel) and one downstream asker (Beacon). It is a small but real peer group, close to peacockdb's architecture.
- Requests for AMD/Apple support recur (Sirius, pandas, DuckDB/MLX), but the posters are mostly vendors or hobbyists.
- Requests for time-series asof joins and rolling windows exist in cudf-polars (quant-flavoured) but have no SQL-engine counterpart yet.
- Evidence is thin for:
  - telco-xdr-gpu, robot-log-gpu, trace-analytics-gpu, flow-hunt-gpu, llm-corpus-sql, gov-airgapped, storage-oem-engine, idle-gpu-batch and ab-metrics-gpu: zero issue-tracker hits.
  - observability and log stores (OpenObserve, Parseable, Quickwit, Greptime): zero GPU-query hits.
  - streaming engines (Flink, RisingWave, Materialize): zero GPU-query hits.
- Issue trackers under-represent buyers. Most first-person posters are evaluators, students or engineers at large companies, not people with budget.
