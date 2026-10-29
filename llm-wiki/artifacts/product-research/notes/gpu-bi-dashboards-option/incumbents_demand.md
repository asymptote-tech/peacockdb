# GPU BI dashboards / interactive analytics over billions of rows: incumbents and demand

All sources accessed 2026-10-29. GitHub data was pulled with `gh` (API) on 2026-10-29. "(estimate)" marks a guess.

## A1. HeavyDB after the Oct 2026 open-sourcing

### Takeaway
NVIDIA's HeavyDB 10.0.0 (2026-10-08) open-sources the former Enterprise Edition (Immerse, the GPU renderer, HeavyConnect, HeavyRF, HeavyML) under Apache-2.0 with license checks removed. Nothing I found says multi-node/distributed mode is included, and the source tree shows no leaf/aggregator code. Support is "best effort". Activity is real but comes mostly from NVIDIA staff porting CI, not from outside users.

### Cited Findings
- README: "as of October 2026, this project now includes all of the functionality of the entire HeavyAI platform… previously only available as an enterprise product. NVIDIA is now releasing the entire platform as open source." It lists Immerse (dashboards), a GPU back-end renderer (Vega-based; point/line/polygon/heatmap/raster), HeavyRF and HeavyConnect (foreign tables over CSV, Parquet, regex logs on local/S3, ODBC, raster). The README does not mention distributed or multi-node mode. — [heavydb README](https://github.com/heavyai/heavydb) (accessed 2026-10-29); the README was rewritten in commit 7dd9ec0f68 "Update README to reflect the full HeavyAI platform functionality (#899)", 2026-10-02
- Support: "HeavyAI maintainers review issues, discussions, and pull requests on a best effort basis without guaranteed response timelines." — [README, Support section](https://github.com/heavyai/heavydb#support) (accessed 2026-10-29)
- Announcement discussion #915 (jwoodford-nv, 2026-10-16) says the release "includes all functionality that was previously available in the HeavyAI Enterprise Edition: HeavyDB, Back-end Renderer, HeavyImmerse, HeavyConnect, HeavyRF, HeavyML" but "does not include the formerly available pre-trained model that was used by HeavyIQ." — [Discussion #915](https://github.com/heavyai/heavydb/discussions/915) (accessed 2026-10-29)
- Discussion #877 (anirudh-simha, 2026-09-23) asked "What features now require a license?" zNeill (2026-09-29) replied: "License systems in place in the former Enterprise Edition of HeavyAI have been removed." Map charts need your own MapBox and Google Maps API keys. — [Discussion #877](https://github.com/heavyai/heavydb/discussions/877) (accessed 2026-10-29)
- Release v10.0.0 was published 2026-10-08. The previous release, v9.0.0, was 2025-10-20. Master was bumped to 10.1 in commit 1399a06226 (2026-10-03). The latest commit on master is 72c6df9f1b (2026-10-07, "Make HeavyIQ build optional (default off) (#908)"). Repo pushed_at is 2026-10-23, with 3,104 stars and 32 open issues. — [releases](https://github.com/heavyai/heavydb/releases), [commits](https://github.com/heavyai/heavydb/commits/master) (accessed 2026-10-29)
- Activity since the release: all issues opened after 2026-05 (#867–#910) are by NVIDIA staff (simoneves, misiugodfrey), apart from #867 (pankgeorg, 2026-09-19, asking about project status and VLDB 2026 materials). Staff issues include "Port Render tests" (#902), "Port/rewrite benchmarking" (#904), "Renderer Vulkan and Shader Compiler Update" (#893), and "Move relevant build/test scripts to their own repos" (#906). — [issues list](https://github.com/heavyai/heavydb/issues?q=created%3A%3E2026-05-01) (accessed 2026-10-29)
- Distributed mode: in 2025-01 the vendor said "The distributed mode is available only with enterprise edition" (cdessanti on #842). The tree at commit 72c6df9f1b has top-level `Immerse/`, `QueryRenderer/`, `ExecuteRenderInterface/`, `GfxDriver/Render`, `HeavyIQ/` and Kafka import tests, but no directories or files named leaf/aggregator/distributed/cluster. No issue created since 2026-09-01 mentions "distributed mode" or "cluster". — [tree @72c6df9f1b](https://github.com/heavyai/heavydb/tree/72c6df9f1b), [#842](https://github.com/heavyai/heavydb/issues/842) (accessed 2026-10-29)
- Build and GPU support: dependency scripts support Ubuntu 22.04 and Rocky Linux 8.x. GPU builds need "a compatible NVIDIA driver and CUDA toolkit" (no version pinned in the README). CMakeLists.txt generates code for compute_60/70/75/80 and more, so Pascal and newer. — [README Dependencies](https://github.com/heavyai/heavydb#dependencies), [CMakeLists.txt @72c6df9f1b](https://github.com/heavyai/heavydb/blob/72c6df9f1b/CMakeLists.txt) (accessed 2026-10-29)
- Ingest: Kafka import exists (`cmake/Modules/FindRdKafka.cmake`, `dev-tools/integration-tests/kafka-import/`). HeavyConnect provides foreign tables over files, S3 and ODBC. — [tree @72c6df9f1b](https://github.com/heavyai/heavydb/tree/72c6df9f1b) (accessed 2026-10-29)
- Vendor marketing: HeavyDB "returns SQL query results in milliseconds, even on tables with billions of rows." — [docs.nvidia.com/heavyai/overview](https://docs.nvidia.com/heavyai/overview) (accessed 2026-10-29)
- Outside-user issues:
  - #842, jieguolove, 2024-12-26: "I have 5 GPU servers, each equipped with 4 NVIDIA L20 GPUs. How can I deploy heavydb in cluster mode?" Closed after the reply that distributed mode is EE-only. — [#842](https://github.com/heavyai/heavydb/issues/842) (accessed 2026-10-29)
  - #819, anirudh-here-com (HERE Technologies handle, so affiliation is an estimate), 2023-12-08: intermittent SIGSEGV crashes in v6.4 causing "outages". Root cause was `select_ipc_gpu` (GPU-memory IPC hand-off to a client) returning 0 rows. The user said "using gpu shared memory is required and cannot be discarded" and runs custom patches. The vendor reproduced it on 7.2.1 and filed an internal ticket. Closed as stale by psilvest on 2026-09-16. — [#819](https://github.com/heavyai/heavydb/issues/819) (accessed 2026-10-29)
  - #836, geo-de-tang, 2024-07-12: "Too many active sessions" after 3 connections. This was the free-edition session cap (now presumably gone with license removal; estimate). — [#836](https://github.com/heavyai/heavydb/issues/836) (accessed 2026-10-29)
  - #825, jieguolove, 2024-04-22: OSS not updated for 7+ months. fly2008begin: "Even HeavyAI doesn't respond to license application emails anymore." The vendor cited "a shortage of resources". — [#825](https://github.com/heavyai/heavydb/issues/825) (accessed 2026-10-29)

### Inferences
- The open release gives anyone a free GPU SQL engine with a dashboard UI (Immerse) and a server-side GPU renderer. That removes "GPU BI on one box" as open space for a startup. A startup's angle would be multi-node/scale-out, data larger than GPU memory, an open-lakehouse (Iceberg/Parquet) focus, and standard BI connectivity.
- Multi-node appears not to be included (estimate, inferred from the missing code and no mention). This should be checked by building it or asking in Discussions.
- Maintenance comes from a small NVIDIA team (mostly one committer, Simon Eves, in recent commits) on a best-effort basis. Outside users' bugs were closed as stale in bulk on 2026-09-16, so long-term stewardship is uncertain (estimate).

### Gaps
- Data-size limits (whether hot data must fit in GPU+CPU memory). Not verified this session; historically HeavyDB caches columns in GPU/CPU memory and pages from disk. The docs were not fetched.
- SQL coverage gaps, the COPY FROM / streaming-insert details, and the supported CUDA version were not verified.
- No outside-user issues or discussions after the release were found, apart from #877. It is too early (3 weeks) to judge uptake.

## A2. Kinetica

### Takeaway
Kinetica sells a free single-node Developer Edition, Dedicated Cloud from $1.80/hour, and Enterprise at a custom price (AWS/Azure marketplaces). GPU is now an optional, extra-cost add-on, which makes CPU the default. I could not verify the "GPU backend for DuckDB" Reddit post.

### Cited Findings
- Tiers: Developer Edition (Shared Cloud) is "Free forever single node instance". Dedicated Cloud is $1.80/hour with multi-node clusters, multi-user and "GPU Acceleration Options". Enterprise is "Contact Us", on-prem or AWS/Azure Marketplace, with PAYG or BYOL. All tiers list "unlimited storage", real-time analytics, SQL workbooks and "Reveal" dashboarding. — [kinetica.com/pricing](https://www.kinetica.com/pricing) (accessed 2026-10-29)
- The pricing FAQ says GPU clusters are for "interactive visualization of large geographic data and high-cardinality joins" and cost more "because of GPU compute nodes and additional license fees". CPU-only clusters are the alternative. — [kinetica.com/pricing](https://www.kinetica.com/pricing) (accessed 2026-10-29)
- An earlier free cloud tier was capped at 10 GB. — [TFiR](https://tfir.io/kinetica-cloud-now-free-for-up-to-10gb-of-data-with-full-kinetica-database-functionality/) (accessed 2026-10-29); this conflicts with "unlimited storage" on the current pricing page.
- Price complaint: in HN item 44340460 (2025-06-21, a ClickHouse thread), atemerev recommends Kinetica for complex loading scenarios with fast analytics "if you can afford the price". The thread does not discuss its GPU usage. — [HN 44340460](https://news.ycombinator.com/item?id=44340460) (accessed 2026-10-29)

### Inferences
- Kinetica has turned GPU into a premium option on a mostly CPU product. That suggests GPU did not justify itself as the default even for the original GPU-database vendor (estimate).

### Gaps
- The Reddit "GPU backend for DuckDB" post (~2026-01) was not found by search. The only DuckDB GPU backends found were Sirius and the `gpudb` community extension ([duckdb.org gpudb](https://duckdb.org/community_extensions/extensions/gpudb), accessed 2026-10-29).
- Not verified: the Vera CPU emphasis, tiered-storage details, Kafka ingest specifics, BI connectors (ODBC/JDBC, Tableau/Power BI) and GPU price per hour.

## A3. Microsoft Fabric Data Warehouse GPU Query Acceleration

### Takeaway
GPU Query Acceleration is a limited, opt-in preview in 5 regions. It accelerates scans, joins and aggregations in most SELECT T-SQL queries with no changes, falling back to CPU otherwise. It is billed on a separate, higher CU meter: one accelerated DW core equals 3.446 CUs. It is built on CoddSpeed/TQP (SIGMOD 2026), which reports up to 30× on TPC-H 1TB versus the CPU engine. This is the most dangerous incumbent for "GPU BI", because it is transparent to Power BI and SQL users.

### Cited Findings
- Microsoft Learn (ms.date 2026-08-11): "This feature is in limited preview", with onboarding by sign-up form "on a first-come, first-served basis". It is enabled by a workspace toggle (Workspace settings > Fabric Warehouse > Query Acceleration) and applies to all Warehouses and SQL analytics endpoints in the workspace. Toggling it "cancels any queries currently running". — [Learn: Query Acceleration](https://learn.microsoft.com/en-us/fabric/data-warehouse/query-acceleration) (accessed 2026-10-29)
- How it works: eligible operations are offloaded to "a GPU-accelerated execution engine, which acts as a co-processor alongside the CPUs in the same compute node". Joins, aggregations and scans benefit; writes do not. Queries over "typically up to 1 TB of data" are most likely to benefit. — [Learn](https://learn.microsoft.com/en-us/fabric/data-warehouse/query-acceleration) (accessed 2026-10-29)
- Eligibility gaps: limited support for `nvarchar` (use `varchar(8000)`) and for case-insensitive collations. — [Learn](https://learn.microsoft.com/en-us/fabric/data-warehouse/query-acceleration) (accessed 2026-10-29)
- Regions: East US, East US 2, South Central US, South East Asia, Germany West Central. — [Learn](https://learn.microsoft.com/en-us/fabric/data-warehouse/query-acceleration) (accessed 2026-10-29)
- Billing: a separate meter at a higher CU rate. "Once enabled, all queries are billed through the query acceleration meter." — [Learn](https://learn.microsoft.com/en-us/fabric/data-warehouse/query-acceleration) (accessed 2026-10-29). The published rate is one accelerated Data Warehouse core = 3.446 CUs, as cited by sdebruyn on 2026-08-23. — [fabric-dw-mcp-cli#595](https://github.com/sdebruyn/fabric-dw-mcp-cli/issues/595) (accessed 2026-10-29)
- Monitoring: an `is_accelerated` column in `queryinsights.exec_requests_history`, plus a "Query Acceleration" operator in SSMS plans. — [Learn](https://learn.microsoft.com/en-us/fabric/data-warehouse/query-acceleration) (accessed 2026-10-29)
- Warm cache / "cache cooldown": the Fabric blog's "Looking ahead" section describes cache cooldown "configured at SQL pool level", with direct control over how long cached data is retained for warm queries and automatic scaling of cache nodes. Build 2026 (session OD813) called the feature a limited preview. As of the 2026-08-23 re-check there was no API or T-SQL surface for cooldown, and the feature had reached "Early Access Preview" with a portal toggle. — [fabric-dw-mcp-cli#595](https://github.com/sdebruyn/fabric-dw-mcp-cli/issues/595) (accessed 2026-10-29)
- Marketing claim: "up to 7x faster" than three comparable warehouse vendors (Microsoft internal testing), covering "most SELECT T-SQL queries" with built-in fallback. — via search snippet of the [Fabric Updates blog](https://community.fabric.microsoft.com/t5/Fabric-Updates-Blog/A-new-analytics-frontier-GPU-accelerated-Fabric-Data-Warehouse/bc-p/5192701) (the page returned HTTP 403 to direct fetch; accessed 2026-10-29). Skeptical third-party take: [Towards AI](https://pub.towardsai.net/microsoft-says-its-gpu-accelerated-warehouse-is-up-to-7x-faster-heres-how-to-read-that-benchmark-5c7af07865e2) (paywalled; accessed 2026-10-29)
- CoddSpeed paper (SIGMOD 2026 Industrial Track; Interlandi, Bruno, Curino, … Ramakrishnan, Gehrke): a GPU engine derived from the Tensor Query Processor (TQP), plus data movement over NVLink/InfiniBand. It also ran on FPGAs/ASICs. It outperforms the CPU engine by "over an order of magnitude", "up to 30x on TPC-H 1TB". — [MSR CoddSpeed](https://www.microsoft.com/en-us/research/publication/coddspeed-hardware-accelerated-query-processing-in-microsoft-fabric/) (accessed 2026-10-29)

### Inferences
- Power BI users on Fabric get GPU acceleration through DirectQuery/SQL endpoints without changing tools. A startup's GPU-BI pitch to Microsoft shops is therefore weak, and the opening is non-Microsoft, on-prem or open-lakehouse users (estimate).
- The "up to 1 TB" sweet spot and the cache-node design show that Microsoft also needs warm, GPU-resident or near-GPU data for interactive speed. That is the same constraint peacockdb has.

### Gaps
- GPU model/SKU is not disclosed. GA date and the price in $ per query were not found.
- Power BI Direct Lake interaction (whether Direct Lake/VertiPaq queries use the GPU at all) was not researched in this session. Direct Lake runs in the Analysis Services engine, not the DW engine, so it is likely not accelerated (estimate).

## A4. Other GPU BI tools and CPU incumbents

### Takeaway
GPU visual-analytics libraries (cuxfilter, Graphistry) exist but have no visible broad adoption numbers. GPU SQL engines (Sirius) now lead ClickBench on cost. CPU engines (ClickHouse, StarRocks) routinely claim sub-second p99 at hundreds of millions to billions of rows, and the Superset and Grafana guides say the bottleneck is usually rows shipped to the browser, not engine compute.

### Cited Findings
- Sirius (DuckDB GPU extension) topped ClickBench (hot runs) on a GH200 at $1.5/h against CPU boxes at $7.3–9.8/h, claiming ≥7.2× cost-efficiency. Weak spots: string contains (q23), top-N (q24/q26) and huge aggregations (q27). It caches hot tables on the GPU, and spilling beyond GPU memory is listed as future work. — [NVIDIA blog 2025-12-15](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/) (accessed 2026-10-29)
- cuxfilter is NVIDIA's GPU cross-filter dashboard library; NVIDIA demos use 300M+ rows. — [NVIDIA blog](https://developer.nvidia.com/blog/build-a-fully-interactive-dashboard-in-a-few-lines-of-python), [Plotly Dash census viz](https://developer.nvidia.com/blog/making-a-plotly-dash-census-viz-powered-by-rapids) (accessed 2026-10-29). An NVIDIA page dated 2026-08-18 covers billion-row dashboard filters with GPU pandas/Polars. — [perspectives.nvidia.com](https://perspectives.nvidia.com/cudf/which-tools-are-best-for-bringing-dashboard-filter-queries-o/) (accessed 2026-10-29)
- StarRocks case study (Zepto, vendor-hosted): P99 under 500 ms on 300M-row datasets; ClickHouse "faltered on complex multi-table joins". — [besthub.dev](https://besthub.dev/articles/how-zepto-scaled-real-time-brand-analytics-with-starrocks-from-postgres-mvp-to-sub-second-queries-6f1a6041774b) (accessed 2026-10-29). StarRocks claims 1.87× ClickHouse on SSB-Flat 100 GB. — [StarRocks SSB docs](https://docs.starrocks.io/ja/docs/benchmarking/SSB_Benchmarking/) (accessed 2026-10-29)
- Superset: Preset's guide targets dashboards under 4 s on billions of rows and cites a 45 s query near the 60 s default timeout; the fix is pre-aggregation or materialization. — [Preset blog](https://preset.io/blog/the-data-engineers-guide-to-lightning-fast-apache-superset-dashboards/) (accessed 2026-10-29). Grafana: downsampling cut a 315k-row, >5 s panel to under 1 s. — [Tiger Data](https://www.tigerdata.com/blog/slow-grafana-performance-learn-how-to-fix-it-using-downsampling) (accessed 2026-10-29)

### Inferences
- For standard aggregate dashboards, CPU OLAP plus pre-aggregation already reaches sub-second. A GPU has a defensible edge only for ad-hoc, high-cardinality, join-heavy or geospatial and point-render workloads that are not pre-aggregated, and for cost per query at high concurrency (estimate).

### Gaps
- Not researched this session: Brytlyt, SQream Blue, PG-Strom GPU-Cache, Sirius `pin_table`, GQE, Druid/Pinot latency and MotherDuck numbers.
- No independent p99 benchmark at the billion-row scale was found.

## B. Demand evidence (first-person threads)

### Takeaway
First-person demand exists but is thin and comes from technical teams, not buyers. They want three things: GPU-resident data across consecutive queries, a GPU SQL endpoint behind BI tools (Superset/Thrift), and multi-node GPU deployment. No thread shows a budget or deadline. The strongest signals are architectural complaints that current tools (Spark-RAPIDS, cudf-polars) cannot keep data on the GPU between interactive queries.

### Cited Findings
- **cudf-spark discussion #9966**, LIN-Yu-Ting, 2023-11-27: "Our objective is to activate Spark Rapids (SQLPlugin) with Spark Thrift Server". The failure only occurred when Apache Superset connected via PyHive and issued `SHOW VIEWS` across multiple sessions; beeline worked. A Superset-on-GPU BI use case, first person. No budget or deadline. — [discussion #9966](https://github.com/NVIDIA/cudf-spark/discussions/9966) (accessed 2026-10-29)
- **cudf-spark #11064**, same user, 2024-06-14: "We are using Spark Rapids + Spark Thrift Server to serve SQL request" (Spark 3.3.0, RAPIDS 23.10). `SELECT DISTINCT runName` on a partitioned Delta table hit GPU OOM. revans2: "Generally we treat GPU OutOfMemory errors as bugs". The user built RAPIDS themselves to debug, which shows a sustained investment. Closed. — [#11064](https://github.com/NVIDIA/cudf-spark/issues/11064) (accessed 2026-10-29)
- **cudf-spark discussion #13435**, andygongyb, 2025-09-08: asked whether input data must be re-transferred host→device between consecutive queries. mattahrens (NVIDIA) answered: "Yes… Caching data onto GPU memory is not currently supported and input data will need to be transferred from host memory to device memory between consecutive queries even if the RAPIDS Cache Serializer is used." — [discussion #13435](https://github.com/NVIDIA/cudf-spark/discussions/13435) (accessed 2026-10-29)
- **cudf #23039**, lmeyerov (Graphistry), 2026-06-30, OPEN: building a polars/cudf-polars backend for GFQL/Cypher in PyGraphistry. "Our workload is iterative/chained — a graph already resident on the GPU, queried repeatedly (e.g. a crossfilter / multi-hop traversal loop)". `collect()` forces device→host transfers; they want device-resident results. NVIDIA (wence-, Matt711) points to partial work (#22867, #22917). — [cudf #23039](https://github.com/NVIDIA/cudf/issues/23039) (accessed 2026-10-29)
- **heavydb #842**, jieguolove, 2024-12-26: wants cluster mode on 5×4 L20 servers. The hardware is already owned, which is real intent, but the user was redirected to an EE license. **#825** (same user and fly2008begin, 2024-04) complains about the stalled OSS and unanswered license emails. — [#842](https://github.com/heavyai/heavydb/issues/842), [#825](https://github.com/heavyai/heavydb/issues/825) (accessed 2026-10-29)
- **heavydb #819**, anirudh-here-com, 2023-12: a production deployment ("outages") that depends on the GPU-IPC result hand-off ("required and cannot be discarded") and runs a custom-patched v6.4. It shows sticky production users of GPU-resident result hand-off. — [#819](https://github.com/heavyai/heavydb/issues/819) (accessed 2026-10-29)
- **HN 47109102**, lmeyerov, ~2026-02 (the page says "8 months ago"; estimate): a comment on "Show HN: Llama 3.1 70B on a single RTX 3090 via NVMe-to-GPU". It recalls helping "a customer accelerate log analytics" with dask-cudf on 8×A100 with GPUDirect Storage. It is not a BI-dashboard demand thread, only indirect evidence of paid GPU log-analytics work. — [HN 47109102](https://news.ycombinator.com/item?id=47109102) (accessed 2026-10-29)
- Against: Kinetica, the original GPU-database vendor, makes GPU an extra-cost option and CPU the default. — [kinetica.com/pricing](https://www.kinetica.com/pricing) (accessed 2026-10-29). HeavyAI's OSS stalled for lack of resources in 2024, and the whole platform was then open-sourced. Microsoft, by contrast, invested in GPU DW acceleration. — [#825](https://github.com/heavyai/heavydb/issues/825), [Learn](https://learn.microsoft.com/en-us/fabric/data-warehouse/query-acceleration) (accessed 2026-10-29)

### Inferences
- The recurring, specific pain is "keep the hot data on the GPU across interactive queries" (#13435, #23039, #819). A persistent-GPU-cache SQL server like peacockdb addresses exactly that, and Spark-RAPIDS does not.
- The second pain is "put a GPU engine behind Superset/BI via a standard endpoint" (#9966, #11064). The third is multi-node GPU without enterprise licensing (#842).
- Demand volume is low: roughly a handful of first-person threads over 3 years, from engineers rather than budget holders (estimate). With HeavyDB now fully free and Fabric bundling GPU acceleration, willingness to pay for "GPU dashboards" as such is doubtful. Differentiation would have to be scale-out, open formats and Postgres/Arrow-Flight BI connectivity (estimate).

### Gaps
- Searches for additional 2024–2026 first-person threads on Superset, Tableau, Grafana or Power BI with a GPU backend, HeavyDB/Immerse users, or "dashboard" with cudf found no new threads. Results were vendor docs and generic performance guides. Reddit was not reached.
- No thread showed a budget, deadline or company-scale procurement signal.
