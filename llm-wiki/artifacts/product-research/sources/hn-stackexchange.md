# Community evidence: Hacker News and Stack Exchange

## Method

- All links accessed 2026-10-28.
- Window: 2023-10-01 to 2026-10-28. A few older items are kept and marked [older].
- Scripts and raw responses are in the session scratchpad. Scripts are under `scrape/hn-se/` and raw responses under `raw/hn-se/`. The judged hit list is `scrape/hn-se/hits.tsv`.

### Hacker News (Algolia API)

- Story search, run once with no date filter and once with `created_at_i>1696118400`. 43 queries, 50 hits each:
  GPU database, GPU SQL, cuDF, RAPIDS, spark-rapids, Spark RAPIDS, HeavyDB, OmniSci, MapD, Kinetica, SQream, BlazingSQL, Voltron Data, Theseus, Sirius GPU, PG-Strom, Brytlyt, cudf polars, Polars GPU, GPU dataframe, GPU analytics, GPU DuckDB, DuckDB GPU, GPU query engine, GPU accelerated SQL, GPU join, GPU parquet, GPU accelerated database, GPU ClickHouse, GPU pandas, GPU OLAP, GPU data processing, GPU ETL, HeavyAI, Velox GPU, DataFusion GPU, GPU Postgres, cuDF pandas, GPU data science, GPU accelerated analytics, gpu dataframes, Crystal GPU, TQP tensor query.
- Comment pass 1 used `search_by_date`, tags=comment, since 2023-10, 2 pages of 100. 34 queries:
  GPU database, GPU SQL, cuDF, RAPIDS, spark-rapids, HeavyDB, OmniSci, Kinetica, SQream, BlazingSQL, Voltron Data, Theseus, Sirius, PG-Strom, Brytlyt, cudf polars, Polars GPU, GPU dataframe, GPU duckdb, duckdb on GPU, GPU query, GPU joins, GPU accelerated SQL, databases on GPU, GPU analytics, cudf.pandas, GPU parquet, GPU for analytics, GPU OLAP, query engine GPU, GPU for ETL, Spark on GPU, GPU groupby, GPU aggregation.
- Comment pass 2 used relevance `search`, since 2023-10, up to 4 pages of 100, plus a story search. 54 queries:
  cudf, rapids gpu, spark rapids, gpu database(s), gpu sql, heavydb, omnisci, heavy.ai, mapd, kinetica, sqream, blazingsql, voltron data, theseus gpu, sirius gpu, sirius duckdb, pg-strom, brytlyt, polars gpu, gpu dataframe(s), duckdb gpu, gpu joins, gpu query engine, gpu analytics, cudf.pandas, dask-cudf, gpu parquet, gpu clickhouse, clickhouse gpu, gpu spark, gpu etl, gpu olap, fabric sql gpu, starburst gpu, gqe nvidia, gpu accelerated database, gpu query, datafusion gpu, velox gpu, gpu postgres, gpu geospatial, gpu spatial join, gpu log analytics, gpu time series, gpu warehouse, idle gpus data, gpu vram analytics, gpu bi dashboard, gpu cybersecurity logs, gpu dedup, gpu data processing, gpu data curation.
- Full comment trees: 46 fetched through `/items/<id>`. These cover the launch threads for ParaQuery, Sirius (Jun 2025 and Dec 2025), SedonaDB 0.4, cuDF, Polars GPU, cudf-polars, Theseus, the Voltron distributed runtime, 1BRC-in-CUDA, and the Apple MLX TPC-H thread. Older threads: AresDB 2019/2020, BlazingSQL 2018/2019, MapD open-source 2017, PG-Strom 2015/2017, "Postgres on the GPU" 2013, Alenka 2017, BlazingDB 2017, OmniSci MacBook 2020, plus 11 comment sub-threads.
- Volume: 1,853 stories and 9,964 comments indexed. A regex filter then kept about 350 comments mentioning a GPU-data system, and all of them were read.
- Result: 89 HN hits judged relevant, 88 of them in the window.
- Nothing was refused and no rate limit was hit.

### Stack Exchange (API 2.3, unauthenticated, filter=withbody, pagesize=100)

- Tag check: cudf (175 questions), rapids (212), kinetica (14), blazingsql (3), cuml (27). These tags do not exist: spark-rapids, dask-cudf, heavydb, omnisci, mapd, sqream, pg-strom, brytlyt.
- Tag queries on stackoverflow: tagged cudf (2 pages), rapids (3), kinetica, blazingsql, cuml.
- `search/advanced` q=gpu on stackoverflow, crossed with tags: apache-spark, pyspark, sql, duckdb, python-polars, pandas, postgresql, clickhouse, dask, parquet, apache-arrow, datafusion. Also q=gpu with tagged=dataframe since 2023-10.
- Free-text queries on stackoverflow: "spark-rapids", "spark rapids", "rapids accelerator", heavydb, omnisci, mapd, sqream, "pg-strom", brytlyt, "cudf.pandas", "cudf-polars", "gpu database", "gpu sql", "gpu join", "sql on gpu", "gpu dataframe", "polars gpu", "duckdb gpu", "dask_cudf", "dask-cudf", and cudf (since 2023-10).
- dba: q=gpu, q=cuda.
- datascience: cudf, rapids, "gpu pandas", "gpu dataframe", "gpu sql", "gpu spark", "gpu database".
- softwareengineering: "gpu database", "gpu sql", "gpu query", cudf.
- gis: gpu, cuda, cuspatial, heavydb, omnisci.
- One answers batch for 10 key questions.
- Volume: 1,217 questions fetched. 517 match GPU keywords, and 79 of those fall in the window. 44 were judged relevant: 40 in the window and 4 older.
- 64 API calls in total. **Quota left at the end: 236.**
- Not reachable or empty: no dba, softwareengineering or gis question in the window is about GPU SQL. Only 8 GPU-keyword questions exist on any site for 2025–2026. Stack Overflow traffic has collapsed, so it says almost nothing about current demand.

## Counts

All 133 judged hits (HN 89, SE 44). `new:pydf-gpu` holds pandas/Polars/dask cuDF API users. `new:generic-gpu-sql` holds general GPU-SQL engine launches and category-level skepticism. Most `workstation-gpu-sql` complaints are Windows/WSL2 install failures from local pandas users, not SQL users.

| slug | request | user | complaint | vendor | other | total |
|---|---|---|---|---|---|---|
| new:pydf-gpu | 0 | 10 | 14 | 4 | 1 | 29 |
| workstation-gpu-sql | 4 | 3 | 12 | 0 | 2 | 21 |
| new:generic-gpu-sql | 0 | 0 | 7 | 5 | 5 | 17 |
| spark-plugin | 1 | 2 | 4 | 3 | 2 | 12 |
| other-accelerators | 5 | 2 | 0 | 2 | 0 | 9 |
| distributed-pluggable | 0 | 0 | 2 | 3 | 1 | 6 |
| gpu-bi-dashboards | 1 | 3 | 0 | 2 | 0 | 6 |
| postgres-gpu-analytics | 2 | 0 | 1 | 1 | 1 | 5 |
| trainer-side-sql-prep | 1 | 1 | 0 | 1 | 1 | 4 |
| clickhouse | 0 | 0 | 4 | 0 | 0 | 4 |
| idle-gpu-batch | 0 | 0 | 2 | 0 | 2 | 4 |
| new:graph-query-gpu | 0 | 2 | 1 | 1 | 0 | 4 |
| datafusion-log-gpu | 0 | 3 | 0 | 0 | 0 | 3 |
| geo-join-gpu | 0 | 1 | 0 | 2 | 0 | 3 |
| gpu-sql-migration-qualifier | 0 | 0 | 1 | 0 | 0 | 1 |
| flow-hunt-gpu | 0 | 1 | 0 | 0 | 0 | 1 |
| realtime-other | 0 | 0 | 1 | 0 | 0 | 1 |
| vector-sql | 0 | 0 | 0 | 1 | 0 | 1 |
| new:agent-query-load | 0 | 0 | 0 | 1 | 0 | 1 |
| new:llm-in-sql-batch | 0 | 0 | 0 | 1 | 0 | 1 |
| **total** | 14 | 28 | 49 | 27 | 15 | 133 |

Totals by kind and source: HN has 8 request, 15 user, 24 complaint, 27 vendor and 15 other. SE has 6 request, 13 user, 25 complaint, 0 vendor and 0 other.

These slugs have zero hits: lakehouse-iceberg, realtime-vram-cache, streaming-flink, datafusion-plugin, storage-oem-engine, gov-airgapped, ab-metrics-gpu, llm-corpus-sql, asof-gpu, gpu-scan-tier, telco-xdr-gpu, robot-log-gpu, trace-analytics-gpu.

## Strongest threads

First-person, concrete-workload posts come first.

| date | system | slug | kind | use case | quote | link |
|---|---|---|---|---|---|---|
| 2025-05-12 | spark-rapids | spark-plugin | request | Spark on EMR/Glue, joins yielding 150B+ rows, shuffle-bound; Glue too costly | "Currently struggling to complete jobs with massive amounts of shuffle with Spark on EMR (large joins yielding 150+ billion rows)." | https://news.ycombinator.com/item?id=43966388 |
| 2026-01-03 | Sirius | gpu-sql-migration-qualifier | complaint | Mixed job sizes; GPU idle; needs a router that sends each job to GPU or CPU | "you need a scheduling layer that estimates the size of the job and routes it to the appropriate hardware" | https://news.ycombinator.com/item?id=46480338 |
| 2025-05-12 | spark-rapids | spark-plugin | user | Ran GPU Spark for enterprises for years; query is ~20% of job time | "I have run GPU accelerated Spark clusters for years for enterprise companies… that's like 20% of what a typical job does" | https://news.ycombinator.com/item?id=43968287 |
| 2024-06-03 | dask_cudf | workstation-gpu-sql | user | Data engineer: 500GB conversion on a 6GB GTX 1060 | "faster than a 20 node m3.xlarge Cluster. What you can do on even consumer GPU's is mind blowing." | https://news.ycombinator.com/item?id=40560169 |
| 2026-02-22 | dask-cudf + GPUDirect Storage | datafusion-log-gpu | user | Graphistry customer log analytics: SSD arrays to GDS to 8x A100 | "We were helping a customer accelerate log analytics by proving out our stack" | https://news.ycombinator.com/item?id=47109102 |
| 2024-04-27 | RAPIDS + GDS | datafusion-log-gpu | user | Event/log processing; ~100GB/s SSD feeding 1–2 GPUs | "using GPU RAPIDS to scale event/log processing is to get GPU direct storage going" | https://news.ycombinator.com/item?id=40177439 |
| 2026-02-24 | dask_cudf + cuML | flow-hunt-gpu | user | HPC netflow attack parquet; encode IPv4 to train a VAE anomaly model on 2 GPUs | "I have 2 columns with IPV4 ip values… to train a variational autoencoder" | https://stackoverflow.com/q/79895441 |
| 2023-11-14 | cuDF | datafusion-log-gpu | user | 1.4GB syslog CSV (timestamp, program, host, message) | "I am reading in one single file (1.4GB) using CuDF" | https://stackoverflow.com/q/77479371 |
| 2023-10-16 | PG-Strom | postgres-gpu-analytics | request | Postgres + pg_strom on an NVIDIA Xavier edge box; Docker build fails | "so that I can use GPU based query acceleration in engine, I have a NVidia Xavier device" | https://stackoverflow.com/q/77300203 |
| 2024-02-08 | cuDF | postgres-gpu-analytics | request | Pull SQL Server/Postgres tables straight into GPU memory (711 views) | "Is there anyway to read data from Microsoft sql directly into cudf (GPU's RAM)?" | https://stackoverflow.com/q/77960181 |
| 2023-07-17 | spark-rapids | spark-plugin | complaint | GPU Spark slower: one executor per GPU, 6 GPUs = 6 executors [older] | "after enabling Spark3 with GPUs we are seeing downtrend in spark job performance" | https://stackoverflow.com/q/76702634 |
| 2024-07-17 | cuDF | workstation-gpu-sql | complaint | T4 server vs 32-core CPU; row-wise transforms slower on GPU | "I could find the CPU computing faster than GPU(using cudf)" | https://stackoverflow.com/q/78760156 |
| 2024-06-15 | cuDF | workstation-gpu-sql | complaint | Bought a GPU for dataframe code; small frames slower (1,015 views) | "I bought a Nvidia GPU and try to implement it using the cudf library. But performance is awful." | https://stackoverflow.com/q/78626099 |
| 2024-06-03 | cuDF | new:pydf-gpu | complaint | Wants 100GB+ in memory without batching | "Limited by VRAM is a huge constraint for me." | https://news.ycombinator.com/item?id=40559986 |
| 2024-06-03 | cuDF | new:pydf-gpu | complaint | High Performance Python co-author: setup breaks on upgrades | "getting a working setup can be a PITA (and then if you upgrade libraries...)" | https://news.ycombinator.com/item?id=40560475 |
| 2025-11-20 | Vortex to cuDF | trainer-side-sql-prep | request | Fast path from columnar files to GPU for XGBoost GPU inference | "The cuDF interop in the roadmap will be huge for my workloads." | https://news.ycombinator.com/item?id=45988948 |
| 2026-07-31 | cuDF/cuGraph | new:graph-query-gpu | user | Billion-edge traversals on one node in seconds vs Databricks graphframes | "billion-edge graph traversals & scans in single node in-memory in seconds with regular dataframe (cudf) libraries" | https://news.ycombinator.com/item?id=49128260 |
| 2026-02-17 | cuDF | new:graph-query-gpu | complaint | Eager per-step CPU/GPU control cost hurts small queries | "Major constant overheads for smaller workloads that seem avoidable." | https://news.ycombinator.com/item?id=47055009 |
| 2026-02-18 | MLX (Apple M4) | other-accelerators | user | TPC-H on M4: GPU wins only on arithmetic-heavy, join-light queries | "DuckDB's optimized SQL engine beats hand-written GPU kernels on every standard TPC-H query." | https://news.ycombinator.com/item?id=47059629 |
| 2026-03-27 | hash join | other-accelerators | request | Hash joins are memory-bandwidth bound; wants high-bandwidth servers | "i wish aws and the other cloud providers would offer arm servers with apple m-series levels of memory bandwidth per core" | https://news.ycombinator.com/item?id=47543091 |
| 2025-06-29 | Sirius | workstation-gpu-sql | request | Wants to try GPU SQL at home | "Pity it requires Volta 7 which is rather high end for fiddling around on at home." | https://news.ycombinator.com/item?id=44410362 |
| 2024-06-07 | cuDF | workstation-gpu-sql | user | Perf engineer running cuDF experiments on an eGPU RTX 4090 | "Just random LLM use (no training) and some cuDF experiments so far." | https://news.ycombinator.com/item?id=40608538 |
| 2024-10-01 | cupy/cudf | new:pydf-gpu | complaint | Vote-ring bot clustering; set intersection missing on GPU | "cupy and cudf haven't implemented intersection functions yet for the GPU" | https://news.ycombinator.com/item?id=41714220 |
| 2024-01-25 | dask_cudf | new:pydf-gpu | user | 8GB CSV larger than GPU and RAM; time filter and rewrite | "Reading all file at once doesn't fit neither into GPU memory nor into my RAM." | https://stackoverflow.com/q/77879888 |
| 2024-09-17 | cudf-polars | new:pydf-gpu | complaint | Categorical dtype missing on GPU; common in ML | "The main one that is a really bummer is the categorical datatypes." | https://news.ycombinator.com/item?id=41568756 |
| 2025-05-12 | ParaQuery | spark-plugin | vendor | Customer: >1PB ETL off BigQuery, 450TB shuffle | "they're saving over 60% off of their BigQuery bill by using ParaQuery, while also being 2x faster" | https://news.ycombinator.com/item?id=43964505 |
| 2025-05-13 | spark-rapids/Comet | spark-plugin | other | Ex spark-rapids contributor on what dominates the work | "Spark compatibility has been pretty much the bulk of the work in my experience so far." | https://news.ycombinator.com/item?id=43969326 |
| 2024-04-18 | BlazingSQL | distributed-pluggable | vendor | Ex-BlazingSQL CEO on open-source GPU SQL economics | "The number of Fortune 500 customers that were deploying us at scale but not paying us… was honestly heartbreaking." | https://news.ycombinator.com/item?id=40079813 |
| 2024-03-30 | GPU DB startup | new:generic-gpu-sql | complaint | Friend at GPU DB startup: driver instability; company dropped GPU | "his former company now offers solutions without GPU acceleration that simply work, but they also lost their USP." | https://news.ycombinator.com/item?id=39872559 |
| 2025-06-29 | GPU DB | new:generic-gpu-sql | complaint | Ops: GPU failure took down host power | "for database reliability no GPU should be allowed in those racks" | https://news.ycombinator.com/item?id=44409786 |
| 2026-01-03 | GPU DBs | clickhouse | complaint | ClickHouse CTO on GPU DB fit | "GPU databases can run a small subset of production workloads in a narrow combination of conditions." | https://news.ycombinator.com/item?id=46482420 |
| 2026-01-03 | Sirius | new:generic-gpu-sql | complaint | Price/perf compares Lambda vs AWS GPU prices | "~4.5x of the price performance benefits is not real" | https://news.ycombinator.com/item?id=46484962 |
| 2026-01-03 | Sirius | new:generic-gpu-sql | other | ClickBench gain over DuckDB is 44% | "improvement over DuckDb is kinda marginal (44%)" | https://news.ycombinator.com/item?id=46482003 |
| 2023-04-22 | PG-Strom | postgres-gpu-analytics | complaint | pg-strom crashes PG 14/15 when pgAdmin lists schemas [older] | "trying to expand the "Schemas" section in pgAdmin4 crashes the whole database" | https://dba.stackexchange.com/q/326286 |
| 2025-06-21 | Kinetica | realtime-other | complaint | Kinetica named for complex fast analytics, but costly | "there's also Kinetica... if you can afford the price." | https://news.ycombinator.com/item?id=44340460 |
| 2026-01-03 | Sirius | trainer-side-sql-prep | other | Prepare data in GPU memory before AI | "need something like SiriusDB to manipulate and prepare data in-place before the AI algorithms get to work" | https://news.ycombinator.com/item?id=46479821 |
| 2026-06-03 | HeavyDB/Fabric | new:agent-query-load | vendor | HeavyAI founder: Microsoft, IBM, Starburst on GPU; agents add query load | "The widespread use of agents to query data will be a part of this" | https://news.ycombinator.com/item?id=48378299 |
| 2026-09-26 | Quail | new:llm-in-sql-batch | vendor | Query planner fused with LLM inference for batch classification | ">1B tokens/minute/GPU by combining query planner and inference engine" | https://news.ycombinator.com/item?id=49861274 |
| 2026-06-26 | SedonaDB 0.4 | geo-join-gpu | vendor | GPU spatial joins in a Rust DB; RT cores on gaming card | "SedonaDB 0.4: GPU-accelerated spatial joins" | https://news.ycombinator.com/item?id=48690130 |
| 2026-07-23 | used P100 | idle-gpu-batch | complaint | Cheap old GPUs exist but old CUDA caps break modern stacks | "You can get used 16GB P100s on AliExpress for ~$100… It hasn't proven revolutionary." | https://news.ycombinator.com/item?id=49015576 |

Older notable threads, not counted:
- AresDB 2020. A former Uber engineer says the project "was openly mocked" internally: https://news.ycombinator.com/item?id=24463676
- jandrewrogers argues the economics fail because "the last 10% is poor enough that it largely offsets the benefit": https://news.ycombinator.com/item?id=24464584
- A MemSQL user says they got better results "for much cheaper" than BlazingDB, 2017: https://news.ycombinator.com/item?id=13989179

## New directions

- **new:pydf-gpu (pandas/Polars on GPU).** This is the largest pool of first-person GPU-data activity (29 hits).
  - The pains are VRAM ceilings, install breakage, small-data slowdowns, API gaps (categoricals, lists, interpolate, UDFs) and multi-GPU confusion.
  - NVIDIA owns this API (cudf.pandas, cudf-polars), so it is evidence of who has GPUs and Parquet, not a product gap for peacockdb.
  - Links: https://news.ycombinator.com/item?id=40559986 and https://stackoverflow.com/q/78626099
- **new:graph-query-gpu (Cypher/graph traversal as dataframe joins on GPU).** Graphistry (GFQL) sells GPU graph investigation to banks, governments and cyber teams.
  - Billion-edge traversals run on one GPU using cudf joins.
  - It is the one sustained, revenue-backed GPU-dataframe user on HN.
  - A GPU join engine could back such a layer.
  - Links: https://news.ycombinator.com/item?id=49128260 and https://news.ycombinator.com/item?id=38922992
- **Router / qualifier variant of gpu-sql-migration-qualifier.** The most concrete objection is utilization: "paying for the GPU whether you're fully using it or not."
  - The asked-for fix is a layer that estimates job size and routes each job to GPU or CPU.
  - Spark users report that only part of a job accelerates, and that executor-per-GPU limits make jobs slower.
  - Links: https://news.ycombinator.com/item?id=46480338 and https://stackoverflow.com/q/76702634
- **new:llm-in-sql-batch.** A query planner fused with LLM inference for batch classification over tables. There is one vendor post with no community demand, so the evidence is thin: https://news.ycombinator.com/item?id=49861274
- **new:agent-query-load.** One vendor claims agents will issue many low-latency queries. There is no user evidence: https://news.ycombinator.com/item?id=48378299
- **Edge or Jetson Postgres GPU.** One request for pg_strom on a Xavier box. Thin: https://stackoverflow.com/q/77300203
- **new:generic-gpu-sql is not a direction. It is crowding.** 2026 brought Sirius, Torin, Craton Bolt, GQE, Fabric SQL on GPU, IBM/Starburst (per tmostak), ronfriedhaber's engine and wsql. Most 2026 launches drew 0–4 comments.

## Leads

- **dogman123 (HN).** Spark on EMR with joins yielding 150B+ rows, shuffle-bound, and a Glue bill they call cost-prohibitive. This is a concrete, painful workload today: https://news.ycombinator.com/item?id=43966388
- **necubi (HN).** Runs mixed scheduled and ad-hoc jobs. Named the routing need and the niche "we run the same 100TB job every day": https://news.ycombinator.com/item?id=46480338
- **threeseed (HN).** Has run GPU Spark for enterprises for years. A good source on where the remaining ~80% of job time goes: https://news.ycombinator.com/item?id=43968287
- **xs83 (HN).** Long-term RAPIDS user as a data engineer, working on consumer GPUs and DGX Spark. The workstation-gpu-sql profile: https://news.ycombinator.com/item?id=40559068
- **lmeyerov / Graphistry (HN).** Has customers with log analytics on SSD arrays to 8x A100, and graph queries on cudf. Complains about cudf eager overhead on small queries. A possible partner or design partner: https://news.ycombinator.com/item?id=47109102
- **Vasile Olteanu (SO).** Netflow attack dataset on an HPC GPFS project, 2 GPUs, dask_cudf + cuML. This fits flow-hunt-gpu: https://stackoverflow.com/q/79895441
- **fabio.geraci (SO).** Syslog exports processed with cudf.pandas: https://stackoverflow.com/q/77479371
- **Mostafa Bouzari (SO).** Wants SQL Server/Postgres tables pulled straight into GPU memory. Also runs cuML on financial fundamentals: https://stackoverflow.com/q/77960181
- **Zain Ul Abidin (SO).** Wants GPU-accelerated Postgres on an NVIDIA Xavier edge device: https://stackoverflow.com/q/77300203
- **kipukun (HN).** Needs a columnar-file-to-GPU path for XGBoost GPU inference: https://news.ycombinator.com/item?id=45988948
- **R_M and HakanA (SO).** Single-T4 or new-GPU buyers whose dataframe code ran slower. Their posts show the mismatch between expectations and small data: https://stackoverflow.com/q/78760156
- **Peers to ask, not customers:**
  - winwang (ParaQuery; his customers are BigQuery spenders).
  - andygrove (Comet, ex spark-rapids).
  - antonmks (Alenka author).
  - _zoltan_ (works on GPU query processing; VeloxCon panel).
  - IanOzsvald (High Performance Python co-author).
  - tanelpoder (perf engineer experimenting with cuDF).

## What this source says

- GPU-SQL launch posts get attention: Sirius 145 points, ParaQuery 135, SedonaDB 42. But comments skew skeptical, and few people say they would use the product for a named workload.
- The one concrete, painful first-person need is a large Spark shuffle and join job with a cloud bill that hurts (dogman123, and ParaQuery's BigQuery customer). That market is crowded (spark-rapids, ParaQuery, Gluten/Velox, Comet), and Spark-compatibility work dominates.
- The recurring objections are:
  - GPU utilization and cost for mixed job sizes.
  - VRAM limits.
  - Storage and IO dominating total time.
  - Driver and hardware reliability.
  - Benchmark skepticism (pricing tricks, a 44% ClickBench gain).
  
  A router that sends only the jobs that fit to the GPU answers the first objection directly.
- Happy GPU-data users are single-node Python dataframe users on cheap GPUs (xs83, Graphistry, trainers). That supports workstation-gpu-sql or single-node more than distributed. They use pandas/Polars APIs, not SQL.
- Niche signals with first-person evidence:
  - Log, syslog and netflow security analytics (datafusion-log-gpu, flow-hunt-gpu).
  - Geospatial joins (SedonaDB, HeavyDB; vendor-heavy).
  - Moving data from Postgres or SQL Server into the GPU.
- Requests for non-NVIDIA backends (Mac, Vulkan, AMD) recur, but nobody attaches a workload to them.
- Stack Exchange shows the pain is installation (WSL2, Windows, version pins) and small-data slowness. No in-window SE question is about a GPU SQL engine (HeavyDB, Kinetica, SQream, Sirius, PG-Strom besides one), and SO activity is near zero for 2025–26.
- Evidence is thin or absent for telco XDR, robot logs, trace analytics, as-of joins, A/B metrics, air-gapped government, Iceberg/lakehouse, streaming and VRAM caches. Neither source mentions them.
