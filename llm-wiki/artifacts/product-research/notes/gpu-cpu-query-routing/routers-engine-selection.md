# Multi-engine query routers and automatic engine selection (for "gpu-cpu-router")

Research caveat: the session-wide WebSearch budget ran out after 2 searches. The rest of these notes come from direct fetches of known URLs, GitHub API/raw files and a shallow clone of trino-gateway (`ext/trino-gateway`, HEAD `cfd7fc270fb3b1e1cacf83dc406154498521e117`). As a result, several questions (Starburst GPU, Tobiko/SQLMesh, e6data, Dremio, Snowflake/BigQuery adaptive compute, most academic accuracy numbers) are listed under Gaps, not answered.

## Q1. Trino Gateway / Presto Gateway / Starburst: do any routing rules use cost or EXPLAIN estimates?

### Takeaway
No. Trino Gateway routes on syntactic request properties (user, headers, query type, referenced tables) or on cluster load (running/queued counts). Any cost-aware logic would have to live in a user-written EXTERNAL routing service. Nothing in the gateway itself calls EXPLAIN.

### Cited Findings
- There are two rule types. FILE is a multi-doc YAML of MVEL condition/action rules, re-read every minute by default (`rulesRefreshPeriod`). EXTERNAL POSTs the request headers and HTTP details to a configured URL, which returns a routing group and optional `externalHeaders` — [Trino Gateway routing rules](https://trinodb.github.io/trino-gateway/routing-rules/) (accessed 2026-10-29).
- MVEL rules can use `request` (HttpServletRequest), `state`, `result`, `trinoRequestUser` and `trinoQueryProperties`. The last two need `requestAnalyzerConfig.analyzeRequest`. `trinoQueryProperties` exposes `getQueryType()`, `getResourceGroupQueryType()` (e.g. SELECT, DATA_DEFINITION) and `getTables()` (fully qualified names). The docs describe only syntactic analysis of the request body, with no cost or EXPLAIN — [routing-rules doc](https://trinodb.github.io/trino-gateway/routing-rules/); [docs/routing-rules.md on main](https://github.com/trinodb/trino-gateway/blob/main/docs/routing-rules.md) (accessed 2026-10-29).
- In code (commit `cfd7fc27`), `ExternalRoutingGroupSelector.createRequestBody` forwards the parsed `TrinoQueryProperties` and `TrinoRequestUser` (when analysis is on) plus the URI and headers. The selector set is `FileBasedRoutingGroupSelector`, `ExternalRoutingGroupSelector`, `StochasticRoutingManager` and `QueryCountBasedRouter`. `QueryCountBasedRouter` reads cluster `runningQueryCount`, `queuedQueryCount` and per-user `userQueuedCount` from `ClusterStats{Http,Jdbc,Jmx}Monitor`. No class calls EXPLAIN — [trino-gateway router package @cfd7fc27](https://github.com/trinodb/trino-gateway/tree/cfd7fc270fb3b1e1cacf83dc406154498521e117/gateway-ha/src/main/java/io/trino/gateway/ha/router) (accessed 2026-10-29; grepped locally).
- The latest release is tag `22`, published 2026-10-20. The repo has 268 stars — [GitHub API releases/latest](https://api.github.com/repos/trinodb/trino-gateway/releases/latest); [repo](https://github.com/trinodb/trino-gateway) (accessed 2026-10-29).
- The OSDS 2024 deck lists three routing approaches: stochastic even distribution, load-based routing by cluster query count, and file rules on query/user. Blue/green upgrades are a headline use — [OSDS 2024 deck](https://trinodb.github.io/trino-gateway/assets/misc/trino-gateway-at-osds-2024.pdf) (accessed 2026-10-29).
- Lineage: Trino Gateway evolved from lyft/presto-gateway. Bloomberg contributed "the main bulk" of the new features (authN/authZ) in July 2023 — [Trino blog 2023-09-28](https://trino.io/blog/2023/09/28/trino-gateway) (accessed 2026-10-29).

### Inferences
- A gpu-cpu-router could ship as a Trino Gateway EXTERNAL routing service: it receives the parsed tables and query type, runs EXPLAIN (TYPE IO / estimates) itself, and returns a routing group such as "gpu" or "cpu". That is a low-friction integration path with no fork needed (estimate).
- Gateways today route at whole-query granularity only. No open gateway splits a plan.

### Gaps
- Starburst Warp Speed and any Starburst/NVIDIA "Trino on GPU / cuDF" routing: search budget ran out, so this is unverified.
- Presto (prestodb) router: not checked.

## Q2. Commercial engine-selection products (small → DuckDB, big → Spark/Trino; any GPU target?)

### Takeaway
Engine selection exists in three forms. Some products route per query between a single-node engine and a distributed one (Ilum, Palantir guidance, Watershed in-house). Some fall back per operator based on support, not cost (Databricks Photon, Sirius, spark-rapids default). MotherDuck places operators across local and remote sites. None of the routers I found targets a GPU engine. The GPU engines themselves fall back to CPU based on support, not on predicted benefit.

### Cited Findings
- **Ilum.** The docs describe a router that observes incoming queries and selects the engine best suited to each workload, with explicit user override. DuckDB is recommended for small-to-medium single-node work and Spark for large ETL/ML. Another Ilum page says users pick the engine per query, so in practice routing may be manual — [Ilum execution engines](https://ilum.cloud/docs/features/engines/) (accessed 2026-10-29; seen via search snippet, page not fetched).
- **Palantir Foundry.** The guidance is to move to Spark only when scale demands it, typically above 50 GB. DuckDB's optimal range is given as 1-50 GB and about 1-200M rows. The engine is a user choice, not automatic — [Foundry compute engines](https://palantir.com/docs/foundry/transforms-python/compute-engines/) (accessed 2026-10-29; snippet).
- **Watershed (in-house).** Product teams estimate each request's resource needs to route it to machines. Simple queries (counts) are always "small", while complex generated SQL needed observability data to set thresholds — [Watershed blog](https://watershed.com/blog/how-watershed-uses-duckdb-to-reliably-compute-customer-footprints) (accessed 2026-10-29; snippet).
- **MotherDuck benchmark.** On 100 GB TPC-DS, DuckDB ran out of memory on a few of the heaviest queries. Over half the queries finished in under 15 s, often less than Spark's startup time — [MotherDuck video page](https://motherduck.com/videos/why-use-duckdb-in-your-data-pipelines-ft-niels-claeys/) (accessed 2026-10-29; snippet).
- **MotherDuck Dual Execution.** The docs say it "automatically routes the various stages of queries execution to the most opportune locations". Local data runs in local DuckDB and cloud/S3 data runs in the cloud. For mixed joins, "MotherDuck finds the best way". The docs give no cost model — [MotherDuck architecture doc](https://motherduck.com/docs/concepts/architecture-and-capabilities/) (accessed 2026-10-29).
- **MotherDuck paper (CIDR 2024).** A client extension adds a "remote-local optimizer" rule that runs after DuckDB's optimizer. It collects constraints (leaf locations, shared hash tables), marks each operator local or remote, inserts order-aware "bridge" upload/download operators, and "looks for the plan with smallest estimated data transfer cost". The objective is transfer volume, not CPU time. The paper notes egress costs (an SF100 example with 90 GB transferred, about $8) — [Atwal et al., CIDR 2024](https://www.cidrdb.org/cidr2024/papers/p46-atwal.pdf) (accessed 2026-10-29).
- **Databricks Photon.** "Photon takes over at the execution layer". Unsupported operations fall back to Spark per operation, so a query can run partly on each. This is support-based, with no cost model — [Databricks Photon docs](https://docs.databricks.com/aws/en/compute/photon) (accessed 2026-10-29).
- **Sirius (DuckDB GPU).** "Queries with unsupported operators fall back silently to CPU". Execution is out-of-core across GPU, host and disk tiers. GPU use is a user setting (`SET gpu_execution = false`, `SET GLOBAL ...`) with no cost-based CPU/GPU choice — [sirius-db/sirius README](https://github.com/sirius-db/sirius) @ `0b183b832e2fc0d7b35faf1a997ab650a543555f` (accessed 2026-10-29).
- **Greybeam.** The site title is "Snowflake Cost Optimization Powered by DuckDB". The routing mechanism, pricing and any GPU support were not visible on the page — [greybeam.ai](https://www.greybeam.ai/) (accessed 2026-10-29).
- **Bauplan.** The homepage describes a serverless data platform ("Execution is managed by the platform") and a blog on moving from DuckDB to DataFusion. It says nothing about per-query engine selection or GPUs — [bauplanlabs.com](https://www.bauplanlabs.com/) (accessed 2026-10-29).

### Inferences
- The "small → DuckDB, big → Spark" pattern is common as guidance and as in-house code, but I found no packaged product with a GPU engine as a routing target. That looks like white space (estimate; limited by search budget).
- Thresholds in the wild are crude (about 50 GB of data, or "counts are small"). Even simple EXPLAIN-based size and shape features would be an improvement (estimate).

### Gaps
- Fivetran/Tobiko SQLMesh engine selection, Definite, e6data, Dremio reflections/engines routing, Snowflake/BigQuery adaptive compute, Tower, and Ibis- or DataFusion-based routers: not verified (search budget exhausted).

## Q3. Cost-based / learned engine and CPU-GPU placement (academic plus spark-rapids)

### Takeaway
The best-documented CPU-vs-GPU placement in production code is spark-rapids' experimental `CostBasedOptimizer`. It is internal and off by default, and it uses per-row cost constants and row-count estimates (default 1,000,000 rows when unknown). NVIDIA's offline QualX XGBoost model predicts GPU speedup but publishes no accuracy figure. Wayang does cross-platform cost-based placement, including splitting a job, but has no GPU platform.

### Cited Findings
- **spark-rapids CBO configs.** `spark.rapids.sql.optimizer.enabled` is `.internal()` and defaults to false: a "cost-based optimizer that will attempt to avoid transitions to GPU for operations that will not result in improved performance over CPU". Related settings: `optimizer.defaultRowCount` = 1000000 when stats are missing, `optimizer.cpu.exec.default` = 0.0002 s per-row CPU operator cost, and `optimizer.explain` = ALL|NONE — [RapidsConf.scala @ v26.08.1](https://github.com/NVIDIA/spark-rapids/blob/v26.08.1/sql-plugin/src/main/scala/com/nvidia/spark/rapids/RapidsConf.scala) (accessed 2026-10-29).
- **spark-rapids CBO logic.** `CostBasedOptimizer` is described as an "Experimental cost-based optimizer". It walks the plan computing CPU and GPU cost per operator and its children, adds row-to-columnar transition costs (`GpuRowToColumnarExec` cost × rowCount plus `MemoryCostHelper.estimateGpuMemory`), and calls `costPreventsRunningOnGpu()` when GPU plus transition cost exceeds CPU cost — [CostBasedOptimizer.scala @ v26.08.1](https://github.com/NVIDIA/spark-rapids/blob/v26.08.1/sql-plugin/src/main/scala/com/nvidia/spark/rapids/CostBasedOptimizer.scala) (accessed 2026-10-29). The current public advanced-configs doc lists only `sql.optimizer.joinReorder.enabled`, so the CBO is hidden from users — [advanced_configs.md](https://github.com/NVIDIA/spark-rapids/blob/main/docs/additional-functionality/advanced_configs.md) (accessed 2026-10-29).
- **spark-rapids release.** The latest release is v26.08.1, published 2026-08-28 — [releases/latest](https://api.github.com/repos/NVIDIA/spark-rapids/releases/latest) (accessed 2026-10-29).
- **QualX (RAPIDS Qualification tool).** It is "an XGBoost model trained on paired CPU and GPU runs of representative Spark workloads" (per-platform, NDS benchmarks). It predicts per-SQL and per-app GPU duration and speedup from CPU event logs, and users can supply a custom model. No accuracy is stated. The tool "does not guarantee" that top-ranked queries accelerate most, and event-log truncation can cause "optimistic assumptions regarding GPU compatibility" — [Qualification overview](https://docs.nvidia.com/spark-rapids/user-guide/latest/qualification/overview.html) (accessed 2026-10-29).
- **Apache Wayang.** You register several engines (local Java, Spark, Flink, Postgres, …) and "let the cost-based optimizer pick the best one for each step, even splitting a single job across engines". On small data it "will almost always pick the local engine (Spark's startup overhead isn't worth it)". No GPU platform is listed — [apache/wayang README](https://github.com/apache/wayang) (accessed 2026-10-29; repo pushed 2026-10-28).

### Inferences
- The spark-rapids CBO staying internal and off for years suggests static per-row constants were not reliable enough to ship as a default. That is weak evidence that naive cost models for CPU-vs-GPU placement mispredict (estimate).
- QualX is the closest existing "should this query go to GPU?" predictor. It is offline and app-level, not an online per-query router.

### Gaps
- I could not fetch numbers for the following, because of the search budget and VLDB/ACM fetch failures (EAI_AGAIN, 403): HyPE/CoGaDB (Breß et al.; e.g. SIGMOD 2016 "Robust Query Processing in Co-Processor-accelerated Databases", doi 10.1145/2882903.2882936), Musketeer (EuroSys 2015), IReS, BigDAWG, Rheem/Wayang ML cost models, and Microsoft learned cost models (e.g. Cleo, SIGMOD 2020). Their prediction accuracy is unverified.

## Q4. Do teams pay for routing or gateways? Adoption evidence

### Takeaway
Adoption of open-source gateways is broad among large Trino users. I found no evidence of a standalone paid market for routers. Routing value is captured inside platforms (Starburst, MotherDuck, Databricks) or as cost-optimization services (Greybeam).

### Cited Findings
- **Trino Gateway users page.** It lists BestSecret, Bloomberg, Expedia, Inter&Co, LY, Lyft, Naver, Pinterest, Shopify, Slack, Starburst and Zomato. Inter&Co uses it to route user profiles and apps to clusters "optimized for specific workloads", separating low-latency from analytical queries — [docs/users.md](https://github.com/trinodb/trino-gateway/blob/main/docs/users.md) (accessed 2026-10-29).
- **Trino Summit 2024 deck.** Lists users or migrators from Presto Gateway forks, including LinkedIn and Wise — [Trino Summit 2024 Trino Gateway deck](https://trino.io/assets/blog/trino-summit-2024/trino-summit-2024-trino-gateway.pdf) (accessed 2026-10-29; snippet).

### Inferences
- The workload-separation use case (Inter&Co) is the closest analogue to CPU/GPU cluster routing. Buyers already split traffic by profile, not by estimated cost (estimate).
- Monetization would more likely come from savings framing ("cost optimization", Greybeam-style) than from selling a gateway (estimate).

### Gaps
- No pricing found for Greybeam, Bauplan or Starburst gateway features.
- No HN discussion was gathered.
