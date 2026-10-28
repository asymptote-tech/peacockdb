# Discord (via public indexes) and forums

## Method

All links accessed 2026-10-28.

Discord and Slack mirrors could not be read:
- Answer Overflow (www.answeroverflow.com): every path returned HTTP 429 from Cloudflare, including `/`, `/search?q=gpu`, `/api/v1/messages/search` and `/sitemap.xml`. I tried 7 times over about 2 minutes, waiting 8 s between tries, from both curl and WebFetch. `api.answeroverflow.com` is a bare Convex deployment, and its guessed REST paths return 404. Web search with `site:answeroverflow.com` and terms GPU/DuckDB/Polars/cuDF/Modal/Dagster/LanceDB found no indexed threads.
- linen.dev (`/`, `/s/`, `/s/apache-iceberg`): HTTP 429 from the Vercel Security Checkpoint, from both curl and WebFetch.
- So none of these Discord or Slack communities were readable: DuckDB, Polars, DataFusion/Arrow, RAPIDS (rapids-goai Slack), Sirius, Databend, RisingWave, GreptimeDB, LanceDB, Ibis, Daft/Eventual, Modal, dbt, Dagster, Trino, Iceberg. None of them has a separate public Discourse forum I could find. The Daft forum `discuss.daft.ai` returned 404. DNS does not resolve `discuss.pola.rs`, `forum.duckdb.org`, `forum.lancedb.com`, `forum.greptime.com`, `forum.risingwave.com` or `community.modal.com`.

Forums that were not reachable:
- `forum.clickhouse.com`, `discourse.trino.io`, `dask.discourse.group`, `discuss.dask.org`, `discuss.dagster.io`, `forum.dremio.com`, `community.kinetica.com`, `discuss.kinetica.com`, `community.sqream.com`: no DNS.
- `community.heavy.ai` (HeavyAI Discourse): DNS resolves, but TCP 443 is refused ("network unreachable"/ECONNREFUSED) from the sandbox, from outside it and from WebFetch. Treat it as dead.
- `community.cloudera.com`: 403 from CloudFront on search and on the LiQL API.
- `community.snowflake.com`: redirects to a login page.
- `community.fabric.microsoft.com` and `community.powerbi.com`: LiQL returned 403 "not authorized".
- `community.splunk.com`, `community.alteryx.com`, `community.hpe.com`: Cloudflare challenge.
- `forum.tigerdata.com`: redirects to the docs; the forum is retired.
- No Kinetica, HeavyAI or SQream user forum was reachable, so I have no user stories from those vendors.

Sources that were reached and scraped. Scripts are in `SCRATCH/scrape/discord-forums/`, raw JSON in `SCRATCH/raw/discord-forums/`.
- Discourse `search.json` (up to 2–3 pages per query, 2.5 s sleep, backoff on 429) and `/t/<id>.json` for bodies.
- **forums.developer.nvidia.com**:
  - Search queries, each with `after:2023-09-30`: cudf, libcudf, cudf parquet, cudf join, cudf sql, gpu sql, gpu database, spark rapids, rapids accelerator spark, cudf.pandas, cudf polars, polars gpu, duckdb, sirius, heavydb, kinetica, sqream, gpu dataframe, dask-cudf, gpudirect storage parquet, kvikio, nvcomp parquet, cudf out of memory, velox, clickhouse, postgres gpu, postgresql, cuspatial, groupby gpu, etl gpu, analytics gpu database, data processing gpu, time series gpu, rapids, cudf memory, pandas gpu, sql query gpu.
  - Undated queries: blazingsql, pg-strom, omnisci.
  - Second pass: `cudf|rapids|pandas|polars|duckdb|sql|database|parquet|"data science"` restricted to `category:dgx-spark-gb10` and `dgx-spark-gb10-projects`, plus dataframe, sirius duckdb, gpu analytics, theseus, presto gpu, velox cudf.
  - Slug pass, each with `after:2023-09-30`: netflow, pcap analytics, log parsing gpu, point in polygon, geospatial join, tick data, kdb, financial time series cudf, morpheus logs, gpu hash join, parquet reader, gpu sort large, apache arrow gpu, gpu dataframe windows, out of core gpu, billion rows, spark gpu.
  - Aerial pass: clickhouse, data lake, datalake query.
  - I also listed the whole "GPU-Accelerated Libraries" category (id 12) back to 2023-10: 840 topics.
- **Other Discourse forums**, with GPU/cuDF/RAPIDS/HeavyDB/Kinetica/SQream/Polars/DuckDB-GPU query sets (21 terms on general forums, 7 terms such as "gpu" or "cuda" on database forums):
  - discuss.ray.io, discourse.julialang.org, discuss.python.org, discuss.pytorch.org, discourse.holoviz.org, forum.posit.co, forums.fast.ai
  - community.dremio.com, community.grafana.com, community.influxdata.com, forum.opensearch.org, discuss.elastic.co
  - forum.knime.com, community.palantir.com, forum.anaconda.com
  - community.cratedb.com, community.questdb.com, discourse.metabase.com, discourse.getdbt.com
  - forum.mirrorship.ai (StarRocks), singlestore.com/forum, discourse.osgeo.org
  - discuss.huggingface.co, discourse.jupyter.org, discuss.streamlit.io
  - discourse.openrobotics.org, with rosbag/mcap/SQL/DuckDB/cuDF terms.
- **Khoros LiQL API** (`/api/2.0/search`, `subject|body MATCHES '<term>' AND post_time > 2023-10-01`):
  - community.databricks.com: gpu, rapids, cudf, spark-rapids, cuda, polars, photon, SQLPlugin, spark.rapids, nvidia.
  - community.sap.com: gpu, cuda, rapids.
  - community.qlik.com: gpu, cuda.

Volume:
- About 5,520 unique topics or messages were indexed, plus 840 topics from the category listing.
- About 300 candidates passed keyword triage. I read about 50 topics in full.
- 61 hits were judged relevant (table below). Most are dated 2023-10 to 2026-10; nothing older is counted.
- Rate limits: NVIDIA, Julia, Ray, python.org and Posit each returned a few 429s. All succeeded after backoff, and no source refused for good.

## Counts

| slug | request | user | complaint | vendor | other |
|---|---|---|---|---|---|
| workstation-gpu-sql (DGX Spark, Windows/WSL, notebooks, packaging) | 1 | 3 | 7 | 5 | 1 |
| spark-plugin | 0 | 1 | 2 | 4 | 0 |
| gpu-sql-migration-qualifier | 0 | 0 | 0 | 1 | 0 |
| new:ran-sim-datalake (Aerial Data Lake / AODT on ClickHouse) | 0 | 0 | 4 | 1 | 0 |
| gpu-scan-tier | 0 | 1 | 0 | 1 | 1 |
| robot-log-gpu | 1 | 1 | 0 | 2 | 2 |
| trainer-side-sql-prep | 2 | 0 | 0 | 0 | 0 |
| asof-gpu | 1 | 0 | 0 | 0 | 0 |
| new:fuzzy-join-gpu | 1 | 0 | 0 | 0 | 0 |
| llm-corpus-sql | 0 | 0 | 1 | 0 | 0 |
| gpu-bi-dashboards | 0 | 0 | 1 | 0 | 0 |
| idle-gpu-batch (Airflow ETL) | 0 | 0 | 1 | 0 | 0 |
| flow-hunt-gpu (SIEM) | 0 | 1 | 0 | 0 | 0 |
| new:gpu-graph-on-parquet | 0 | 1 | 0 | 0 | 1 |
| new:edge-jetson | 0 | 0 | 1 | 0 | 0 |
| new:duckdb-gpu (Sirius) | 0 | 0 | 0 | 1 | 0 |
| distributed-pluggable (Velox, Presto) | 0 | 0 | 0 | 2 | 0 |
| trace-analytics-gpu | 0 | 0 | 0 | 0 | 1 |
| other (generic cuDF, partner PR, packaging) | 0 | 0 | 2 | 4 | 1 |
| **total** | **6** | **8** | **19** | **21** | **7** |

## Strongest threads

First-person posts with a concrete workload come first.

| date | system | slug | kind | use case | quote | link |
|---|---|---|---|---|---|---|
| 2026-10-25 | Spark 3.5.3 + RAPIDS 25.02 on 2× DGX Spark | spark-plugin / workstation-gpu-sql | user | Nightly batch: reads ~380M rows and builds a graph dataset. The GPU phases got 1.5–2.4× slower after an OS kernel update. | "two DGX Spark units in one Apache Spark 3.5.3 standalone cluster with the RAPIDS Accelerator 25.02" | https://forums.developer.nvidia.com/t/kernel-7-0-0-1019-nvidia-makes-apache-spark-rapids-gpu-work-1-5-2-4x-slower-on-dgx-spark-6-17-0-1032-with-the-same-driver-580-178-04-is-fine/386791 |
| 2026-09-07 | NVIDIA AODT 1.5.1 + ClickHouse | new:ran-sim-datalake | complaint | RF digital twin, 42 RUs × 285 UEs, ~12M ray paths per step, 57 GB on an H100. The CPU-side insert into ClickHouse never finishes. | "Two DB worker processes ... each at 100% of one core, ~117 min CPU each ... Zero rows in cirs" | https://forums.developer.nvidia.com/t/aodt-1-5-1-insertcirs-scans-the-full-raypath-vector-per-link-cir-export-never-completes-on-a-42-ru-285-ue-scene-insertraypaths-is-fine/382625 |
| 2024-10-28 | NVIDIA Aerial Data Lake + ClickHouse | new:ran-sim-datalake | complaint | Captures uplink IQ samples from a live gNB (A100 + CX6DX). Queries are slow while the RAN runs. | "The speed of querying data from database while the gNB is running is significantly slower than doing that after" | https://forums.developer.nvidia.com/t/data-lake-24-1-collector-process-skips-collecting-ul-slots/311400 |
| 2025-05-07 | cuDF + Dask-CUDA, 8 GPUs | llm-corpus-sql | complaint | Loads 15M text records into cuDF to prep data for NeMo Curator. OOM even with spilling on. | "My objective was loading a 15Mil records dataset using cudf and then prep it using Nemo Curator." | https://forums.developer.nvidia.com/t/cudf-spill-on-demand-couldnt-find-memory-device-to-spill-spillmanager-device-memory-limit-spilled-unspilled/332484 |
| 2025-06-16 | spark-rapids on Databricks | spark-plugin | complaint | The GPU plan kicks in, but joins kill the executors and the job hangs. | "when I run the spark join like join , the executors are getting killed and the spark job is stuck" | https://community.databricks.com/t5/data-engineering/executors-getting-killed-while-scaling-spark-jobs-on-gpu-using/td-p/121862 |
| 2025-07-17 | spark-rapids on Databricks | spark-plugin (also gov-airgapped) | complaint | The cluster has no outbound internet, so the jar cannot be fetched, and SQLPlugin fails at driver start. | "for governance reasons, my cluster doesn't have the permissions to access any external URL" | https://community.databricks.com/t5/data-governance/cannot-set-spark-plugins-com-nvidia-spark-sqlplugin-config/td-p/125570 |
| 2023-10-16 | libcudf C++ | asof-gpu | request | Merge two or more Arrow/cuDF tables on timestamp, adding columns and null-filling, from C++. | "concatenation of two or more cudf::Table/arrow::Table by timestamp using libcudf c++" | https://forums.developer.nvidia.com/t/merge-arrow-cudf-datatable-by-timestamp-libcudf-c/269569 |
| 2025-11-14 | libcudf + nvCOMP, DGX cluster | gpu-scan-tier | user | Research fork "GPUFileFormat-cudf": writes TPC-H lineitem Parquet with nvCOMP CASCADED compression. | "I tried to let cudf support non-Parquet standard Compressions (e.g. CASCADED from nvComp)" | https://forums.developer.nvidia.com/t/nvcomp-cudaerrormisalignedaddress-caused-by-nvcompbatchedcascadeddecompressasync/351449 |
| 2026-04-08 | cuGraph on DGX Spark | new:gpu-graph-on-parquet | user | Knowledge graph of 500K nodes stored as Parquet and loaded into GPU memory. 3-hop traversals take 200 ms. | "The graph persists as Parquet files and loads into GPU memory in under 2 seconds." | https://forums.developer.nvidia.com/t/neuralforge-gpu-native-knowledge-intelligence-platform-built-on-dgx-spark-gb10/365954 |
| 2026-04-20 | Morpheus + Kafka + Elasticsearch on DGX Spark | flow-hunt-gpu | user | GPU SIEM pipeline. Search stays on Elasticsearch (CPU). | "I built a GPU-accelerated SIEM pipeline using NVIDIA Morpheus" | https://forums.developer.nvidia.com/t/siem-on-dgx-spark/367141 |
| 2025-11-27 | cuDF 25.10 on DGX Spark | workstation-gpu-sql | user | Fraud-detection blueprint: preprocesses the 2.2 GB TabFormer data on ARM64. | "RAPIDS cuDF 25.10 installed and working perfectly on ARM64" | https://forums.developer.nvidia.com/t/nvidias-financial-fraud-training-container-is-only-built-for-x86-64/352807 |
| 2025-11-25 | cuDF on DGX GB10 | workstation-gpu-sql | complaint | Enterprise buyer thinks cuDF does not work on GB10. Staff reply that 25.10 works. | "This is blocking GPU-accelerated dataframe work on a brand-new DGX system." | https://forums.developer.nvidia.com/t/rapids-cudf-preview-build-request-for-grace-blackwell-gb10-dgx-aarch64-kernels-missing/352629 |
| 2026-07-06 | RAPIDS on GB10 | workstation-gpu-sql | complaint | A RAPIDS job OOMs the unified-memory box, which then hangs (no ssh). | "I recently have the same issue when I do rapids task to my gb10, and seems like it has OOM" | https://forums.developer.nvidia.com/t/device-hangs-under-load/375016 |
| 2025-03-26 | RAPIDS images on JupyterHub | workstation-gpu-sql | complaint | Academic JupyterHub (rocker-org). The conda RAPIDS image is too big to build in CI. | "creates an image of over 40GB, too big to successfully build on GH Actions" | https://discourse.jupyter.org/t/minimal-image-compatible-with-nvidia-rapids/34116 |
| 2024-06-24 | RAPIDS in AI Workbench | workstation-gpu-sql | complaint | An empty RAPIDS project container is 20 GB. | "An empty rapid container image is 20GB. Is there any way to slim this down?" | https://forums.developer.nvidia.com/t/can-the-rapids-image-go-on-a-diet-20gb-image/297463 |
| 2023-10-01 | cuDF → PyTorch | trainer-side-sql-prep | request | Loads CSV straight to the GPU for training. Python 3.11 is not supported. | "loading and copy is the most time-wasting operation. I found there is cudf ... unsupported for my python version" | https://discuss.pytorch.org/t/cudf-for-python-3-11/189225 |
| 2024-05-13 | dataframe in PyTorch | trainer-side-sql-prep | request | Wants a GPU table type inside PyTorch to replace numpy+pandas. | "cudf (rapids) is also not yet available for python 3.12" | https://discuss.pytorch.org/t/dataframe-kindo-of-object-in-pytorch/202686 |
| 2024-10-07 | cudf.pandas | new:fuzzy-join-gpu | request | Fuzzy merge of two files on string similarity. Wants rapidfuzz on the GPU. | "Is there any GPU accelerated package that can be used similar to the rapidfuzz ... for fuzzy merge?" | https://forums.developer.nvidia.com/t/string-similarity-package-in-cudf-for-fuzzy-merge/308933 |
| 2025-10-07 | cuxfilter (RAPIDS 25.08) | gpu-bi-dashboards | complaint | Princeton research-computing user. The GPU dashboard tutorial fails to compile. | "Compilation failed: ... CustomInspectTool.ts" | https://forums.developer.nvidia.com/t/dashboard-not-working-with-cux-rapids-25-08/347062 |
| 2024-05-04 | Dask-CUDA + cuDF on WSL, GTX 1660 Ti laptop | workstation-gpu-sql | user | Teacher processes a 1T-element array on a 6 GB laptop GPU. Wants mixed CPU/GPU arrays. | "It shall be nice to have a hetrogeneous array defined over the CPU and the GPU" | https://forums.developer.nvidia.com/t/dask-is-another-big-winner-for-wsl/291803 |
| 2023-11-14 | cudf.pandas | workstation-gpu-sql | request | Several readers ask for native Windows cuDF, without WSL. | "Is there a roadmap for cuDf GPU acceleration in standalone Windows (local Jupyter notebook) without WSL." | https://forums.developer.nvidia.com/t/rapids-cudf-accelerates-pandas-nearly-150x-with-zero-code-changes/272201 |
| 2025-10-16 | DGX Spark vs GX10 | workstation-gpu-sql | other | A buyer believes RAPIDS does not run on ARM, and this drives the purchase. | "that would mean RAPIDS won't run (w/GPU) in them ... it was in mine." | https://forums.developer.nvidia.com/t/should-i-buy-asus-gx10-instead-nvidia-dgx-spark/347717 |
| 2024-01-31 | rosbag / MCAP | robot-log-gpu | request | Multi-robot field capture. Bags pile up unanalysed. Wants a pandas-like API. | "it would be awesome to have some pandas -like API, where one could vaguely synchronize streams" | https://discourse.openrobotics.org/t/the-rosbag-graveyard/35866 |
| 2026-08-12 | mcap-mcp-server (SQL over rosbags) | robot-log-gpu | user | Pixel Robotics (pallet AMRs) runs joins over bags and Nav2 behaviour-tree logs. CPU only. | "uses a database under the hood. This allows complex queries / joins to the rosbag for fast analysis" | https://discourse.openrobotics.org/t/analyzing-rosbags-with-llms-and-sql-queryable-nav2-behavior-trees/57339 |
| 2025-08-25 | Bagel (DuckDB over rosbags) | robot-log-gpu | other | An LLM tool turns topics into DuckDB tables. The PlotJuggler author asks for the conversion alone. | "If I just wanted the ROS2 → DuckDB table conversion only, is it possible to do that?" | https://discourse.openrobotics.org/t/dont-trust-llm-math-with-your-rosbags-bagel-has-a-solution/49737 |
| 2026-04-29 | RosBag Resurrector (Polars + DuckDB index) | robot-log-gpu | other | Pandas-like API over MCAP, with an as-of style nearest-time sync. | "bf.sync([\"/imu/data\", \"/joint_states\"], method=\"nearest\", tolerance_ms=50)" | https://discourse.openrobotics.org/t/rosbag-resurrector-pandas-like-analysis-for-mcap-ros-2-bag-files/54395 |
| 2026-09-09 | Rerun RRD rosbag2 plugin | robot-log-gpu | vendor | Bags stored as Arrow chunks, queried with DataFusion, fed to a PyTorch loader. | "Arrow gives direct paths into tools like DataFusion and out to formats like Parquet" | https://discourse.openrobotics.org/t/query-and-train-directly-from-rosbags-with-a-rosbag2-storage-plugin-for-rrd/57982 |
| 2026-08-18 | Bagel 2.0 | robot-log-gpu | vendor | DuckDB SQL over Arrow. Fleet-wide event-window reduction. | "Bagel writes your topic messages to Arrow and runs DuckDB SQL over them" | https://discourse.openrobotics.org/t/bagel-2-0-ask-your-rosbags-questions-in-plain-english-reduce-them-to-the-seconds-that-matter-open-source/57482 |
| 2024-11-27 | Airflow + RAPIDS in Docker | idle-gpu-batch | complaint | An Airflow DAG with cuDF code fails inside the container. | "When running a dag in airflow that contains cudf code the system throws and error message." | https://forums.developer.nvidia.com/t/airflow-and-rapids-integration-in-docker-not-working/314873 |
| 2024-10-04 | cuDF on Jetson AGX Orin | new:edge-jetson | complaint | `import cudf` segfaults on JetPack 6.1. It works after reflashing to 6.0. | "import cudf …got below error ... Segmentation fault (core dumped)" | https://forums.developer.nvidia.com/t/cudf-corrupted-size-vs-prev-size-aborted-core-dumped-error/308760 |
| 2025-07-14 | AODT 1.2 + ClickHouse on ESXi (4× L40S) | new:ran-sim-datalake | complaint | Digital-twin setup on vGPU. ClickHouse connection errors. | "Error: Simultaneous queries on single connection detected" | https://forums.developer.nvidia.com/t/aodt-1-2-0-error-problem-initializing-em-solver-and-simultaneous-queries-on-single-connection-detected/339015 |
| 2025-04-03 | Spark RAPIDS Parquet scan | spark-plugin | vendor | An NVIDIA engineer says the Parquet scan work helped customer jobs. | "significant improvement for multiple customer workloads on GPU as Parquet scans are key" | https://forums.developer.nvidia.com/t/accelerating-apache-parquet-scans-on-apache-spark-with-gpus/329247 |
| 2025-04-10 | Polars GPU Parquet reader | gpu-scan-tier | vendor | A reader doubts the ~100 GB/s figure. Answer: it is decoded bytes on an H100. | "It seems impossible to achieve more than 64 GB/s if the denominator is based on the read size" | https://forums.developer.nvidia.com/t/efficiently-scaling-polars-gpu-parquet-reader/329964 |
| 2025-12-15 | Sirius (DuckDB) | new:duckdb-gpu | vendor | ClickBench record. No replies. | "NVIDIA is partnering with the University of Wisconsin-Madison to bring GPU-accelerated analytics to DuckDB" | https://forums.developer.nvidia.com/t/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/354751 |
| 2026-07-08 | Presto on GB200 NVL72 | distributed-pluggable | vendor | Low-latency analytics, posted as a blog. | "Running Low-Latency Analytical Workloads with GPU-Accelerated Presto" | https://forums.developer.nvidia.com/t/running-low-latency-analytical-workloads-with-gpu-accelerated-presto-on-nvidia-gb200-nvl72/376090 |
| 2025-10-06 | Velox + cuDF | distributed-pluggable | vendor | GPU-native Velox, posted as a blog. | "Accelerating Large-Scale Data Analytics with GPU-Native Velox and NVIDIA cuDF" | https://forums.developer.nvidia.com/t/accelerating-large-scale-data-analytics-with-gpu-native-velox-and-nvidia-cudf/346956 |
| 2025-05-15 | Spark GPU qualification | gpu-sql-migration-qualifier | vendor | Predicts whether a Spark job speeds up on GPU. | "Predicting Performance on Apache Spark with GPUs" | https://forums.developer.nvidia.com/t/predicting-performance-on-apache-spark-with-gpus/333334 |
| 2026-10-13 | Spark UDF rewrite with agents | spark-plugin | vendor | AI agents rewrite UDFs so they run on the GPU. | "How to Rewrite Apache Spark UDFs for GPU Acceleration with AI Agents" | https://forums.developer.nvidia.com/t/how-to-rewrite-apache-spark-udfs-for-gpu-acceleration-with-ai-agents/385809 |
| 2025-10-28 | Palantir + NVIDIA | other | vendor | CUDA-X data science libraries inside Foundry. | "integration of NVIDIA accelerated computing, NVIDIA CUDA-X data science libraries" | https://community.palantir.com/t/new-nvidia-palantir-collaboration/5341 |

## New directions

- **new:ran-sim-datalake.** NVIDIA's own telecom stack (Aerial Data Lake, Aerial Omniverse Digital Twin) writes RF captures and ray-tracing results from the GPU into a CPU ClickHouse.
  - Users report that inserts are CPU-bound and never finish: 12M paths per step, 57 GB already resident on the GPU (aditya.chaudhuri1).
  - Queries are slow while the gNB shares the host (khanh.toan.pham).
  - Many setup threads fail on the ClickHouse connection (dz288, dbiscardi2013, shivshankar).
  - An embedded GPU-side SQL store for simulation and capture tables is a narrow, concrete niche. It sits close to telco-xdr-gpu and clickhouse.
- **new:fuzzy-join-gpu.** Requests for string-similarity joins (Levenshtein, rapidfuzz) in cuDF. One clear post. Weak.
- **new:gpu-graph-on-parquet.** A knowledge graph persisted as Parquet and loaded into the GPU for cuGraph traversal. One builder (nmaine), and one peer on Postgres AGE + pgvector (jrsphd). Weak, but it is a real single-box workload.
- **new:edge-jetson.** cuDF on Jetson Orin breaks when the JetPack version changes. Robot and edge users want dataframes on device. One post.
- **Packaging as the product.** The most repeated complaint is install friction, not speed:
  - container size: 20 GB and 40 GB images;
  - Python versions not yet supported: 3.11, 3.12;
  - no native Windows;
  - ARM64 confusion on DGX Spark;
  - conda solver failures;
  - an air-gapped Databricks cluster that cannot fetch the jar.

  A small single-binary GPU SQL engine that reads Parquet answers this directly. This supports workstation-gpu-sql and gov-airgapped.

## Leads

- **jeff1evesque** (NVIDIA forum). Runs Spark + RAPIDS on two DGX Spark units: a nightly 380M-row batch that builds a graph dataset, with timings per phase. He is a paying user of desktop GPU SQL and a natural candidate for a single-node replacement that skips the JVM.
- **aditya.chaudhuri1** (Aditya Chaudhuri). Profiles AODT down to perf and strace. Has a ClickHouse bottleneck on GPU-resident simulation data. The best lead for ran-sim-datalake.
- **khanh.toan.pham** (Khanh Toan Pham). Collects Aerial Data Lake IQ samples on an A100 + CX6DX gNB and wants faster queries while the RAN is live.
- **pham_hung2** (Pham Hung). Prepares a 15M-record text corpus on 8 GPUs for NeMo Curator and hits OOM in cuDF. Lead for llm-corpus-sql.
- **chenqi_qba** (Qi Chen, "GPUFileFormat-cudf", DGX cluster). Researches GPU-friendly Parquet encodings on TPC-H. Lead or collaborator for gpu-scan-tier.
- **stelgic** (Dom). Wants a timestamp as-of style merge of Arrow tables in C++. Lead for asof-gpu, possibly in trading.
- **rajanchaturvedi**, **CarlosAlberto** (Databricks Community). Enterprise spark-rapids attempts: one failed on join OOM, one on air-gapped install. Good for spark-plugin pain interviews.
- **cboettig** (Carl Boettiger, rocker-org, UC Berkeley). Runs GPU JupyterHub for researchers and finds RAPIDS images too big to build.
- **mse**, **adam344**, **gabriellhl**, **griffith.mark** (DGX Spark owners). Desktop GPU data science on 128 GB unified memory. They hit OOM hangs, ARM packaging confusion and container gaps.
- **jplapp** (Pixel Robotics), **mhubii** (Martin Huber), **facontidavide** (PlotJuggler), **shouheng / ArunVenkatadri** (Bagel), **segfault_survivor** (Resurrector), **Isaac_Blankenau** (Rerun). The robot-log SQL community. They already use DuckDB, Arrow and DataFusion and have never mentioned GPUs.
- **amadhivanana**. Built a GPU SIEM on DGX Spark with Morpheus. Search stays on Elasticsearch. A flow-hunt lead.
- **nmaine** (Nathan Maine). Built a cuGraph knowledge graph on Parquet on DGX Spark.

## What this source says

- I could not read any Discord or Slack at all. Answer Overflow and linen.dev both block scripted and fetched access with 429 bot checkpoints. Communities that live only in chat (DuckDB, Polars, Sirius, DataFusion, Databend, LanceDB, Daft, dbt, Trino, Iceberg, RAPIDS Slack) are absent from this file. This is the biggest gap.
- Database vendor forums show almost no demand for GPU SQL. Dremio, CrateDB, QuestDB, Metabase, dbt, StarRocks, SingleStore, Grafana, Influx, Elastic, OpenSearch, SAP and Qlik had zero first-person requests for GPU query execution. On those forums "GPU" means LLM inference.
- The NVIDIA forum is mostly vendor posts. RAPIDS staff send users to GitHub and Slack, so the forum holds packaging and install trouble, not workloads.
- The real users on the NVIDIA forum are small: a single DGX Spark box, a laptop GPU, Kaggle or Colab. Their pain is installing and fitting in memory, not speed. This supports workstation-gpu-sql with a "just works, small binary" angle.
- spark-plugin has real enterprise attempts that fail on operations: OOM on joins, air-gapped installs, kernel regressions. Only one success story was found (jeff1evesque).
- The most concrete, unexpected niche is NVIDIA's own RAN simulation and capture tools. There, GPU-resident results are funnelled into a CPU ClickHouse that cannot keep up.
- Robot-log analysis is clearly moving to SQL (DuckDB, Arrow, DataFusion, MCAP), but nobody asks for a GPU. Per-bag data is small, so GPU value there is unproven.
- Training-side data prep (CSV or Parquet straight into GPU tensors) shows up as a recurring small request, blocked by cuDF's lag in Python version support.
- Evidence overall is thin: 6 requests and 8 users in 61 relevant hits. No forum thread shows an urgent, paid-for GPU SQL need beyond the Spark-on-DGX user and the Aerial/AODT ClickHouse complaints.
