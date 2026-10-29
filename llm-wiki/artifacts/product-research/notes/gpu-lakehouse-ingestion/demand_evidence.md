# Demand evidence: users who need faster or cheaper Iceberg/Delta/Parquet scans, and appetite for GPU acceleration

All sources accessed 2026-10-29. This was desk research of roughly 20 tool calls, so coverage is partial (see the Gaps sections).

## Q1. Public threads and complaints about lakehouse scan cost, S3 throughput, decode CPU, and small files

### Takeaway
The pain shows up repeatedly in public, but the most common complaint is S3 request count and file layout (small files, many GETs), not decode CPU. The fixes people reach for are compaction, Iceberg, and caching, not more compute. The strongest evidence that decode itself is expensive comes from academia: ETH Zürich measured decoding at 46% of DuckDB TPC-H runtime on Parquet.

### Cited Findings
- ETH Zürich (Dann & Alonso, DaMoN '26, arXiv v2 dated 2026-05-05): "Data lakes spend a significant fraction of query execution time on fetching and scanning data from remote, disaggregated storage. Data decoding alone accounts for 46% of runtime when running TPC-H directly on Parquet files." They propose a SmartNIC (not a GPU) that offloads decode and pushed-down operators. In their DuckDB tests, 16 threads on pre-filtered data outperformed all 64 cores on Parquet-resident data. DuckDB TPC-H SF30 throughput was about 2× higher on pre-filtered tables than on Parquet files. — [arXiv 2602.18775](https://arxiv.org/abs/2602.18775) (accessed 2026-10-29)
- Vantage (2024-03-21): Trino/Athena queries scanning hundreds of thousands of objects caused S3 GET request costs that in some cases exceeded S3 storage costs. Insider cut its S3 costs by 90% by moving to Iceberg, which means larger files and fewer GETs. Junglee Games cut S3 request costs by 70% with Starburst Warp Speed caching, and its reports ran 30% faster. — [Vantage blog](https://vantage.sh/blog/s3-bill-increase-athena-trino-hive-fix-iceberg-caching) (accessed 2026-10-29)
- Dune Analytics (Trino Fest 2024): cloud object storage API operations are expensive, and most queries read the same small subset of the lake. The talk evaluates Alluxio/MinIO out-of-cluster caching and intra-cluster caching; Rubix is Hive-only and unmaintained. — [Dune Trino Fest 2024 deck](https://trino.io/assets/blog/trino-fest-2024/dune.pdf) (accessed 2026-10-29; seen as a search summary, deck not fully read)
- AWS EMR docs: Trino slowdowns and S3 SlowDown errors are commonly "rooted in the presence of a large number of small files" because of split count. The recommended fixes are Iceberg compaction and higher S3 retry limits. — [AWS EMR Trino common issues](https://docs.aws.amazon.com/emr/latest/ReleaseGuide/emr-trino-common-issues.html) (accessed 2026-10-29)
- iThome (Taiwan, 2025, in Chinese): a scheduled Iceberg optimize/compaction job cut monthly Trino-related S3 cost by about 40%. The author says the measurement was not rigorous. — [ithome 10376594](https://ithelp.ithome.com.tw/articles/10376594) (accessed 2026-10-29; seen as a search summary)
- Trino (2019): S3 read amplification meant network traffic to workers was many times larger than the input data. The fix was positioned reads. This is old, but it shows that I/O inefficiency can look like a CPU or cost problem. — [Trino blog: faster S3 reads](https://trino.io/blog/2019/05/06/faster-s3-reads) (accessed 2026-10-29)
- NVIDIA tuning guidance: on wide tables (>1500 columns), Spark RAPIDS queries "wast[ed] time parsing Parquet footer data on the CPU". Raising maxPartitionBytes to 1–2 GB was recommended because the 128 MB default is too small for GPU. — [NVIDIA dev blog (via search summary)](https://developer.nvidia.com/blog/accelerating-apache-parquet-scans-on-apache-spark-with-gpus) (accessed 2026-10-29)

### Inferences
- The pain people complain about publicly is mostly "S3 bill / GETs / small files / metadata". Decode CPU cost is real (46% in the academic measurement) but is rarely named by practitioners. A GPU pitch therefore has to be framed as "fewer, cheaper nodes" rather than "faster decode" (estimate).
- Metadata and footer parsing stay on the CPU even in GPU engines (NVIDIA footer note; Bodo lists metadata latency as future work, see Q3). An accelerator that leaves the planning and metadata path untouched will hit an Amdahl ceiling on small-file tables (estimate).

### Gaps
- My searches returned no r/dataengineering threads, no Trino Slack summaries, and no trinodb/trino or apache/iceberg GitHub issues that specifically complain about Parquet decode CPU. Search engines indexed vendor content ahead of Reddit. A direct Reddit or GitHub search is still needed.
- I could not reach the Iceberg Summit 2026 (April 8–9, SF) session list or recordings. The CFP is at [sessionize](https://sessionize.com/iceberg-summit-2026).

## Q2. Companies with large Iceberg estates and cost concerns, and GPU-for-SQL case studies

### Takeaway
Large Iceberg estates are well documented: Netflix has more than 1 EB and runs Trino on about 50K vCPUs. Cost-driven moves to Trino/Iceberg are documented at Branch, New Relic, BestSecret, Insider and Wise. Named Spark RAPIDS production case studies are thin and old (Adobe, AT&T, 2020–2022). The most concrete head-to-head (Capgemini) found GPU only 6% cheaper than Photon, and Photon won the production slot.

### Cited Findings
- **Netflix** (Trino Summit 2024): 1+ EB warehouse, 3M+ Iceberg tables, 99.5%+ Iceberg adoption, largest table 36 PB, 10+ PB ingested per day. Trino runs about 500K queries per day across 20+ clusters, 1100+ worker nodes and 50K vCPUs. Metadata pain: tables with more than 100M files required a distributed metadata table scan, which gave a 2–1000× improvement. Future work lists "Trino Caching". The deck does not mention GPUs. — [Netflix Trino Summit 2024 deck](https://trino.io/assets/blog/trino-summit-2024/trino-summit-2024-netflix.pdf) (accessed 2026-10-29)
- **Branch** (Trino Fest talk, Gopi Bhagavathula): moved one business unit from a real-time query engine to Trino + Iceberg and accepted 30–60 minute ingestion lag. Reported "7-figure savings" per year. The motivation was explicitly cost: "the cost of maintaining high-performance data infrastructure can quickly spiral out of control." — [Branch deck](https://trino.io/assets/blog/trino-summit-2024/trino-summit-2024-branch.pdf) (accessed 2026-10-29)
- **Wise** (Trino Summit 2024): 3+ PB of raw data moved into an Iceberg-based lake. — [search summary of Trino Summit 2024](https://www.starburst.io/info/trinosummit) (accessed 2026-10-29; deck not read)
- **New Relic**: migrated 1,000+ datasets from Snowflake to Iceberg and reports a 35–52% reduction in annual data platform spend. This is self-reported. — [New Relic blog](https://newrelic.com/blog/observability/snowflake-to-iceberg-migration) (accessed 2026-10-29)
- **Insider**: 90% S3 cost cut from moving to Iceberg. — [Vantage blog](https://vantage.sh/blog/s3-bill-increase-athena-trino-hive-fix-iceberg-caching) (accessed 2026-10-29)
- **Adobe** (NVIDIA press release, 2020-05-14): William Yan, Sr. Director of ML, said they were "seeing significantly faster performance with NVIDIA-accelerated Spark 3.0". No cost figure was given. — [NVIDIA newsroom](https://nvidianews.nvidia.com/news/nvidia-accelerates-apache-spark-worlds-leading-data-analytics-platform) (accessed 2026-10-29)
- **AT&T** (NVIDIA blog, about 2022): tested the RAPIDS Accelerator for Spark. An engineer projected that reports taking 2–3 weeks could be done in hours. This was a projection, not a measured production result. — [NVIDIA blog](https://blogs.nvidia.com/blog/att-data-science-rapids) (accessed 2026-10-29)
- **Capgemini / unnamed retailer** (NVIDIA blog): trillions of POS records. Databricks Photon on Xeon and RAPIDS on GPU both finished in under 2 h, with Azure runtimes "statistically indistinguishable" (about 4 min 37 s). RAPIDS used 6% fewer adjusted DBUs per minute. **Photon went to production**, mainly because it was ready a few days sooner. — [NVIDIA dev blog](https://developer.nvidia.com/blog/gpus-for-etl-run-faster-less-costly-workloads-with-nvidia-rapids-accelerator-for-apache-spark-and-databricks/) (accessed 2026-10-29)
- **AWS data-on-EKS benchmark** (RAPIDS run dated 2026-01-12): TPC-DS 1TB Parquet/Snappy read from S3. Native Spark on 8× c5d.12xlarge cost $31.28 per job; Gluten/Velox on the same instances cost $18.40; RAPIDS on 4× g6.2xlarge (L4) cost **$6.27**. RAPIDS was roughly equal to native Spark on wall time (about 1.06–1.08×), while Gluten was fastest. The page credits RAPIDS' strength to "simple aggregations with large data scans" and filter-heavy queries. The page's runtime figures are internally inconsistent. — [awslabs data-on-eks](https://awslabs.github.io/data-on-eks/docs/benchmarks/spark-rapids-gpu-benchmark) (accessed 2026-10-29)

### Inferences
- The cost win for GPU in the AWS benchmark comes from a cheap L4 node (8 vCPU, $0.98/h) replacing large CPU nodes, not from speed. That suggests GPU pitches land on "$/query" and not on latency (estimate).
- Netflix-scale users build in-house and are unlikely to be design partners for a small startup. Mid-size cost-motivated Trino/Iceberg operators (Branch, Dune, Insider, Wise, BestSecret) fit the profile better (estimate).

### Gaps
- I found no public Spark RAPIDS case study for Walmart, Pinterest or Snap. Don't cite them without a primary source.
- I found no 2024–2026 named production RAPIDS customer with dollar savings on Iceberg or Delta specifically.

## Q3. Is GPU SQL limited by I/O? Evidence for and against GPU being wanted

### Takeaway
Evidence is mixed. On S3 Parquet, GPU scan is itself fast; real-world GPU Spark disappointments come from CPU fallback, partitioning and memory tuning rather than S3. Vendors keep building GPU lakehouse readers (Bodo, Microsoft Fabric, EDB, SQream, Sirius), which shows supply-side interest, but buyer pull is weakly evidenced.

### Cited Findings
- spark-rapids discussion #8082 (2023-04-12): a user reading S3 Parquet on g5.24xlarge found GPU at 4.9 min versus CPU at 4.3 min, which meant higher cost. Maintainer jlowe traced it to an unsupported `regexp_replace` Project causing row/columnar transitions over more than 2B rows ("a large portion of the slowdown problem"), plus too few shuffle partitions. "The Parquet scan seems to be a lot faster." After tuning: 3.9 min, with RMM OOM issues along the way. S3 was not named as the bottleneck. — [GitHub discussion](https://www.github.com/NVIDIA/spark-rapids/discussions/8082) (accessed 2026-10-29)
- Voltron Data (undated, competitor-authored): claims Spark-RAPIDS performance "remains constrained by Spark's inherent CPU architecture". Pitches Theseus instead. — [Carahsoft/Voltron resource](https://www.carahsoft.com/resources/25299-nvidias-spark-rapids-hits-the-wall--constrained-by-sparks-cpu-architecture) (accessed 2026-10-29). Theseus was still alive as of June 2025 and adding AMD GPU support — [The Register 2025-06-18](https://www.theregister.com/2025/06/18/voltron_amd_sql/) (accessed 2026-10-29). I found no shutdown report.
- Bodo (2026-06-04), "Building a Native GPU Reader for Apache Iceberg": built a GPU-native Iceberg source operator. Reducing metadata latency and managing VRAM spikes are listed as future work, and the post says source-operator efficiency largely determines pipeline performance. No customers named. — [Bodo blog](https://www.bodo.ai/blog/building-a-native-gpu-iceberg-reader-for-apache-iceberg) (accessed 2026-10-29; read via a [Plushcap summary](https://www.plushcap.com/companies/bodo/blog/summaries/2026/6), original not read). **This is a direct competitor.**
- Microsoft Fabric: announced a GPU-accelerated Fabric Data Warehouse in early access. A community reply says it will extend to lakehouse items; pricing is TBD. — [Fabric community blog](https://community.fabric.microsoft.com/t5/Fabric-Updates-Blog/A-new-analytics-frontier-GPU-accelerated-Fabric-Data-Warehouse/bc-p/5192701) (accessed 2026-10-29; returned 403, content from search snippet only)
- EDB PGAA docs (v1.9/1.10): offers Spark RAPIDS over Iceberg/Delta and says RAPIDS becomes the primary performance driver "at 3 TB and beyond". Storage is S3-compatible or local only. — [EDB docs](https://www.enterprisedb.com/docs/pgaa/1.10/spark/spark-rapids/) (accessed 2026-10-29)
- Sirius (CIDR 2026, Apache-2.0): GPU drop-in for DuckDB/Doris. Reports 8.3× (TPC-H) and 7.4× (ClickBench) better cost efficiency than single-node DuckDB, and up to 12.5× with Doris. — [arXiv 2508.04701](https://arxiv.org/html/2508.04701v3); [CIDR page](https://vldb.org/cidrdb/2026/rethinking-analytical-processing-in-the-gpu-era.html) (accessed 2026-10-29). HN thread (about 144 points, 17 comments): one commenter called the improvement over DuckDB "kinda marginal (44%)", which others disputed. — [HN 46441781 mirror](https://astro-hacker-news.pages.dev/46441781) (accessed 2026-10-29)
- Starburst (Trino) announced day-one support for NVIDIA's Vera **CPU** rather than GPU acceleration. — [BusinessWire 2026-03-16](https://www.businesswire.com/news/home/20260316162054/en/) (accessed 2026-10-29; seen as a search summary)

### Inferences
- The leading Trino vendor betting on an NVIDIA CPU rather than a GPU path suggests Trino-side buyers are not yet asking for GPUs (estimate).
- Several well-funded vendors (Bodo, Microsoft, Voltron, SQream, NVIDIA itself) are already building GPU lakehouse readers, so the reader alone is unlikely to be defensible (estimate).

### Gaps
- I found no RAPIDS qualification-tool report published by a user showing I/O-bound verdicts.
- I found no user thread saying GPU Spark is S3-bandwidth bound. Absence in my search is not proof it doesn't exist.

## Q4. Counter-evidence and "CPU is fast enough", plus candidate design partners

### Takeaway
Credible counter-evidence exists. Many cost cuts come from layout (compaction, Iceberg), caching, or leaving proprietary pricing, not from compute speed. Single-node CPU engines (DuckDB, DataFusion) are widely framed as sufficient for hundreds-of-GB workloads. The best design-partner profile is a mid-size, cost-motivated, self-run Trino/Iceberg shop that has already done compaction and caching and is still compute-bound.

### Cited Findings
- New Relic's savings come from leaving proprietary pricing for commodity infrastructure, not from Iceberg's design, per a third-party analysis. — [BigData Boutique](https://bigdataboutique.com/blog/does-apache-iceberg-actually-save-you-money) (accessed 2026-10-29; seen as a search summary)
- BestSecret: moved from Snowflake to Trino + Iceberg with a reported 70% cost reduction (vendor-published). — [Starburst profile](https://starburst.io/info/isa-inalcik-starburst-data-rebel) (accessed 2026-10-29)
- Single-node trend: 2026 write-ups argue teams "scale up" on one machine with DuckDB/DataFusion for hundreds of GB. Mühleisen's "data singularity" argument says laptop-class hardware outpaces most analytical workloads. — [datalakehousehub 2026-05](https://datalakehousehub.com/blog/2026-05-single-node-data-engineering); [endjin](https://endjin.com/blog/duckdb-rise-of-in-process-analytics-understanding-data-singularity) (accessed 2026-10-29)
- Capgemini chose CPU Photon over GPU because GPU saved only 6% and arrived later. — [NVIDIA dev blog](https://developer.nvidia.com/blog/gpus-for-etl-run-faster-less-costly-workloads-with-nvidia-rapids-accelerator-for-apache-spark-and-databricks/) (accessed 2026-10-29)
- Gluten/Velox (CPU) was fastest in the AWS EKS TPC-DS benchmark; GPU won only on $/job. — [awslabs](https://awslabs.github.io/data-on-eks/docs/benchmarks/spark-rapids-gpu-benchmark) (accessed 2026-10-29)

### Candidate design partners (all estimates, based on the cited public signals above)
1. **Branch** (branch.io): runs Trino + Iceberg for an internal BU explicitly to cut cost ("7-figure savings"), and has accepted latency trade-offs. They speak at Trino events (Bhagavathula), so the contact path is public. Fit: cost-per-query pitch on existing self-run Trino.
2. **Dune Analytics**: public Trino Fest talk on expensive object-store API calls and caching. Its scan-heavy public crypto datasets make it a heavy repeated-read workload where a GPU scan and cache tier could matter.
3. Alternates: **Insider** and **Wise** (Iceberg on Trino, cost-driven migrations), **New Relic** and **BestSecret** (left Snowflake for cost). Netflix is too large and in-house.

### Gaps
- I couldn't confirm whether Branch, Dune or the others are still CPU-bound after compaction and caching. That needs discovery calls.
- I found no explicit public statement from any of these companies wanting GPU acceleration.
