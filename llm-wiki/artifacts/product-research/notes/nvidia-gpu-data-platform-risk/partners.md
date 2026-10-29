# NVIDIA-blessed GPU SQL / data-processing partners, and where NVIDIA leaves room for third parties

All URLs accessed 2026-10-28 unless noted. "(estimate)" marks my own inference. "(vendor claim)" marks numbers that come only from NVIDIA or the partner.

## Q1. Big platform partnerships 2024–2026: what each announced, and whether it shipped

### Takeaway
NVIDIA's big-platform play is "cuDF inside the engine the customer already runs": the Spark plugin on every managed Spark (Databricks, EMR, Dataproc, Oracle AIDP, Cloudera), and Velox+cuDF under IBM's Presto C++. Snowflake got cuDF/cuML only in its ML containers, not in its SQL engine. No GPU path for BigQuery, Fabric, or Databricks Photon is confirmed. Oracle's deal is about vector-index building (cuVS) plus Spark, not GPU SQL in Oracle Database.

### Cited Findings
- **GTC 2026 keynote (Mar 16, 2026):** Huang "walked through the field of data processing" and said NVIDIA worked with IBM, Dell, Google Cloud, AWS, Microsoft Azure, Oracle and CoreWeave. The same post names Databricks and Snowflake as data platforms supporting the ecosystem. It also claims "Apache Spark accelerated by NVIDIA cuDF provides up to 5x faster query performance" and 10x better TCO on 10 TB (vendor claim). Storage partners for the AI Data Platform with RTX PRO 4500: Cloudian, DDN, Dell, Everpure, Hammerspace, Hitachi Vantara, IBM, NetApp, Nutanix, VAST Data, WEKA — [NVIDIA GTC 2026 live blog](https://blogs.nvidia.com/blog/gtc-2026-news/) (accessed 2026-10-28)
- A keynote summary says the keynote listed cuDF dataframes as used in "Snowflake, Databricks, Amazon EMR, Azure Fabric, Google Cloud BigQuery". That describes where the technology is used, not specific product announcements. This is secondary and unverified against the slide — [TechRadar GTC 2026 live](https://www.techradar.com/pro/live/nvidia-gtc-2026-live-coverage-all-the-news-and-updates-as-it-happens) (accessed 2026-10-28)
- **IBM (shipped in watsonx.data; open-source pre-release):**
  - IBM announced an expanded collaboration at GTC on 2026-03-16 — [IBM newsroom](https://newsroom.ibm.com/2026-03-16-ibm-and-nvidia-announce-expanded-collaboration-at-gtc-2026-to-advance-ai-for-the-enterprise) (accessed 2026-10-28)
  - The collaboration links cuDF to IBM's Presto query engine. Nestlé ran the same workload 5x faster at 83% lower cost on GPU-accelerated watsonx.data (vendor claim) — [CIO Dive](https://www.ciodive.com/news/ibm-nvidia-gtc-partnership-ai-data-management-gpus/814877/) (accessed 2026-10-28)
  - The mechanism is cuDF inside Velox, so Presto SQL is unchanged. GH200 runs TPC-H-like SF1000 in about 100 s versus about 1,246 s for CPU Presto C++ — [NVIDIA dev blog: GPU-native Velox and cuDF](https://developer.nvidia.com/blog/accelerating-large-scale-data-analytics-with-gpu-native-velox-and-nvidia-cudf) (accessed 2026-10-28)
  - Also described by IBM — [IBM community blog, 2025-10-07](https://community.ibm.com/community/user/blogs/ali-leclerc/2025/10/07/accelerating-data-analytics-gpu-velox-nvidia) (accessed 2026-10-28)
  - Open-source GPU Presto C++ nightly images were published in June 2026 and are explicitly pre-release — [prestodb.io blog 2026-06-24](https://prestodb.io/blog/2026/06/24/gpu-accelerated-presto-c/) (accessed 2026-10-28)
  - A GTC 2026 session (S81563, NVIDIA's Greg Kimball with IBM Research) reported Presto on 4×B200 about 30x faster than an 8-node Grace CPU cluster on TPC-H SF1K — [third-party GTC notes](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28; secondary source)
- **Snowflake (shipped, ML only):** On 2025-11-18, cuML and cuDF were made available in Snowflake ML through the Container Runtime in Notebooks and ML Jobs, to accelerate scikit-learn, pandas, UMAP and HDBSCAN. Benchmarks: 5x for Random Forest and 200x for HDBSCAN on A10 (vendor claim). The release has quotes from Kleinerman (Snowflake) and Pat Lee (NVIDIA). It does not cover Snowflake's SQL engine — [Snowflake press release](https://www.snowflake.com/en/news/press-releases/snowflake-supercharges-machine-learning-for-enterprises-with-native-integration-of-nvidia-cuda-x-libraries/) (accessed 2026-10-28)
- **Databricks (2024 Photon promise; status unconfirmed):**
  - In June 2024 (older), Databricks said it planned "native support for NVIDIA-accelerated computing in … Photon" — [Databricks press release](https://www.databricks.com/company/newsroom/press-releases/databricks-and-nvidia-strengthen-partnership-accelerate-enterprise) (accessed 2026-10-28)
  - NVIDIA's current Databricks guide still says to uncheck "Use Photon Acceleration" to launch a GPU node, and cuDF-Spark can use only one GPU per node because of a Databricks limitation — [NVIDIA docs, Databricks](https://docs.nvidia.com/cudf-spark/26.06/getting-started/databricks.html) (accessed 2026-10-28)
  - At Data+AI Summit 2026 there was a session on AI agents (Project Aether) converting Spark UDFs for the GPU — [Databricks DAIS session](https://www.databricks.com/dataaisummit/session/coding-ai-agents-supercharge-your-spark-udfs-gpu-acceleration) (accessed 2026-10-28)
- **AWS (shipped; Spark plugin on EMR):**
  - In December 2025 NVIDIA published Project Aether, a set of microservices that migrates CPU Spark jobs on EMR to the cuDF plugin. Access is by application — [NVIDIA dev blog](https://developer.nvidia.com/blog/migrate-apache-spark-workloads-to-gpus-at-scale-on-amazon-emr-with-project-aether/) (accessed 2026-10-28)
  - NVIDIA's data-processing page claims 3x faster Spark on EMR and OpenSearch billion-scale vector indexes built in under an hour — [NVIDIA data processing solutions](https://www.nvidia.com/en-us/solutions/data-processing/) (accessed 2026-10-28)
- **Google Cloud (shipped for Spark; nothing for BigQuery):**
  - Managed Service for Apache Spark (formerly Dataproc and Serverless for Spark) supports A100 and L4 GPUs, with GPUs on the premium tier — [Google Cloud docs](https://cloud.google.com/dataproc-serverless/docs/guides/gpus-serverless) (accessed 2026-10-28)
  - NVIDIA documents the cuDF plugin there — [NVIDIA docs, Dataproc Serverless](https://docs.nvidia.com/spark-rapids/user-guide/latest/getting-started/google-cloud-serverless.html) (accessed 2026-10-28)
  - NVIDIA's page says "Google Cloud and NVIDIA deliver GPU-accelerated analytics on G4 VMs" — [NVIDIA data processing solutions](https://www.nvidia.com/en-us/solutions/data-processing/) (accessed 2026-10-28)
  - I found no BigQuery GPU announcement.
- **Oracle (shipped; Spark plus vector indexing, not GPU SQL):**
  - At Oracle AI World (October 2025), Oracle AI Data Platform got a built-in NVIDIA GPU option and the RAPIDS Accelerator for Spark plugin — [NVIDIA blog: NVIDIA and Oracle](https://blogs.nvidia.com/blog/nvidia-oracle-accelerate-enterprise-ai-data-processing/) (accessed 2026-10-28)
  - Oracle's docs cover installing the cuDF and cuML Spark plugins — [Oracle AIDP docs](https://docs.oracle.com/en/cloud/paas/ai-data-platform/aidug/manage-compute.html) (accessed 2026-10-28)
  - At GTC 2026, Oracle AI Database 26ai customers "can now offload vector index creation to NVIDIA accelerated computing" through the Oracle Private AI Services Container with cuVS — [NVIDIA GTC 2026 live blog](https://blogs.nvidia.com/blog/gtc-2026-news/) (accessed 2026-10-28)
- **Microsoft (no Fabric GPU SQL found):**
  - The only Fabric material is community requests for GPU support; one older post says Fabric has none — [Fabric community](https://community.fabric.microsoft.com/t5/Data-Science/MS-Fabric-GPU-Cuda-support/m-p/3447729) (accessed 2026-10-28)
  - The NVIDIA–Microsoft news is about Foundry, where cuDF and other CUDA-X libraries are exposed as agent "skills" — [NVIDIA blog, Microsoft Build](https://blogs.nvidia.com/blog/microsoft-build-windows-local-cloud-devices/) (accessed 2026-10-28)
- **Cloudera (shipped):**
  - Native GPU acceleration for Spark 4.1 through cuDF, "up to 4x faster" with no code changes (vendor claim) — [datacentre.solutions](https://www.datacentre.solutions/news/23622-cloudera-adds-native-nvidia-gpu-acceleration-to-apache-spark-for-faster-ai-data-prep) (accessed 2026-10-28)
  - Listed on NVIDIA's page as "Powered by NVIDIA cuDF" — [NVIDIA data processing solutions](https://www.nvidia.com/en-us/solutions/data-processing/) (accessed 2026-10-28)
- **Dell / HPE / VAST / DDN:**
  - Dell AI Data Platform claims "up to 12x speedup for vector indexing and 3X for data processing with cuDF and cuVS" — [NVIDIA data processing solutions](https://www.nvidia.com/en-us/solutions/data-processing/) (accessed 2026-10-28)
  - NVIDIA is building an AI Data Platform with Dell and NTT Data — [TechRadar GTC 2026](https://www.techradar.com/pro/live/nvidia-gtc-2026-live-coverage-all-the-news-and-updates-as-it-happens) (accessed 2026-10-28)
  - HPE appears only with the Vera CPU in the ProLiant DL394 Gen12 — [NVIDIA data processing solutions](https://www.nvidia.com/en-us/solutions/data-processing/) (accessed 2026-10-28)
  - VAST Data is listed as a Sirius "Contributor and Partner" — [Sirius GitHub](https://github.com/sirius-db/sirius) (accessed 2026-10-28)
  - DDN reports 22x faster Milvus index builds with CAGRA — [NVIDIA cuVS blog](https://developer.nvidia.com/blog/optimizing-vector-search-for-indexing-and-real-time-retrieval-with-nvidia-cuvs/) (accessed 2026-10-28)
- **Meta / Velox:** Velox's cuDF backend (`velox/experimental/cudf`) runs Velox plans on libcudf, swapping operators one-to-one with CPU fallback. It covers more relational operators than the alternative Wave backend — [Velox GPU docs](https://facebookincubator.github.io/velox/develop/gpu-acceleration.html) (accessed 2026-10-28)
- **Customer reference:** Snap reports 4x faster Spark runtimes and 76% cost savings (vendor claim) — [NVIDIA cuDF perspectives](https://perspectives.nvidia.com/cudf/accelerating-existing-sql-engines-in-place-for-faster-query/) (accessed 2026-10-28; the page carries an AI-generated-content disclaimer)

### Inferences
- The Spark plugin is the "default" GPU SQL product that NVIDIA itself owns and ships everywhere, with hyperscalers as distribution (estimate). Any startup "Spark GPU plugin" competes directly with NVIDIA's own product.
- The hyperscalers' proprietary SQL engines (BigQuery, Snowflake SQL, Photon, Fabric) are not visibly GPU-accelerated. Either they are not convinced of the TCO, or the work is unannounced (estimate).

### Gaps
- Whether Databricks Photon GPU support ever shipped: I found no 2025–2026 confirmation.
- I could not see the GTC 2026 keynote slide listing data-processing logos. Only text summaries were available.
- No AWS or Google first-party press release for 2026 was found. Mentions come through NVIDIA pages.
- I could not open the date and primary release for Cloudera's Spark 4.1 GPU announcement.

## Q2. GPU database startups and vendors: status, and NVIDIA endorsement

### Takeaway
The standalone GPU databases have mostly died or been absorbed. Voltron Data (Theseus) shut down in 2025. NVIDIA acquihired HEAVY.AI in 2025, and its founder now leads SQL-on-GPU at NVIDIA RAPIDS. The live NVIDIA-promoted projects are open-source accelerators for existing engines: Sirius for DuckDB, Velox/Presto, Polars GPU, and Starburst's Trino. The surviving commercial GPU DBs (Kinetica, SQream, Brytlyt) have Inception or partner badges but receive little 2025–2026 NVIDIA promotion.

### Cited Findings
- **HEAVY.AI (MapD/OmniSci):**
  - Acquired by NVIDIA in 2025 — [Wikipedia](https://en.wikipedia.org/wiki/HEAVY.AI) (accessed 2026-10-28)
  - Pavlo calls it an acquihire and found no official announcement — [Pavlo, Databases in 2025](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html) (accessed 2026-10-28)
  - Todd Mostak's profile lists a role leading SQL-on-GPU work for RAPIDS at NVIDIA — [GetProg profile](https://www.getprog.ai/profile/953210) (accessed 2026-10-28; low-quality source)
  - NVIDIA now hosts the HeavyDB docs — [docs.nvidia.com/heavyai](https://docs.nvidia.com/heavyai/overview) (accessed 2026-10-28)
  - At GTC 2026 (S81769), Mostak, now at NVIDIA, said HeavyDB would be open-sourced in late Q2 2026 — [third-party GTC notes](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28; single secondary source)
  - In January 2025 (pre-acquisition), it announced availability on GH200 with claims of up to 21x faster than CPU competitors — [BusinessWire](https://www.businesswire.com/news/home/20250121115004/en/HEAVY.AI-Announces-Availability-of-Analytics-Platform-Accelerated-by-NVIDIA-Grace-Hopper-Superchip) (accessed 2026-10-28)
- **Voltron Data (Theseus):**
  - Listed under 2025 "Deaths": it had $110M in VC and "built a GPU-accelerated database … but failed to launch it in a timely manner". Pavlo adds that Voltron and the HeavyDB acquihire "continue the trend of the inviability of GPU-accelerated databases," and predicts "major GPU-accelerated database announcements by vendors in 2026" — [Pavlo, Databases in 2025](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html) (accessed 2026-10-28)
  - Theseus appeared in GTC 2026 talks (S81769, S81873) as the research system behind NVIDIA's cuCascade, with TPC-H at 100 TB on two DGX A100s. The speakers' earlier project was BlazingSQL — [third-party GTC notes](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28)
  - Accenture Ventures invested in February 2025, before the shutdown — [FinSMEs](https://www.finsmes.com/2025/02/voltron-data-receives-investment-from-accenture-ventures.html) (accessed 2026-10-28)
- **Sirius (UW-Madison with NVIDIA engineers):**
  - Open-source (Apache-2.0) GPU-native engine built as a DuckDB extension on Substrait, cuDF, cuVS and cuCascade. Contributors and partners: NVIDIA, UW-Madison, DuckDB, VAST Data. DuckDB is supported today and StarRocks is "coming soon." No multi-node support yet — [Sirius GitHub](https://github.com/sirius-db/sirius) (accessed 2026-10-28)
  - NVIDIA blog (December 2025) on its ClickBench record. It is "developed by University of Wisconsin-Madison with support from NVIDIA engineers" — [NVIDIA dev blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/) (accessed 2026-10-28)
  - Paper (CIDR 2026): 8.3x and 7.4x better cost efficiency on TPC-H and ClickBench with DuckDB, and up to 12.5x speedup with Apache Doris — [arXiv 2508.04701](https://arxiv.org/html/2508.04701v3) (accessed 2026-10-28)
  - GTC 2026 (S81870, NVIDIA's Bobbi Yogatama with UW's Xiangyao Yu): #1 and #2 ClickBench hot runs, and TPC-H SF1K in 21 s on a GB300 DGX Station. DuckDB's Hannes Mühleisen reportedly called it "the blessed way of running queries on GPUs with DuckDB" and announced it as a core extension — [third-party GTC notes](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28; I could not confirm the "core extension" status from DuckDB primary sources; the Sirius README still describes a separately loaded extension binary)
- **Polars:** The GPU engine is powered by cuDF and remains in Open Beta. It uses `engine="gpu"` with transparent CPU fallback, and claims up to 13x on H100 for PDS-H (vendor claim) — [NVIDIA cuDF docs: Polars GPU engine](https://docs.nvidia.com/cudf/latest/cudf_polars/) (accessed 2026-10-28); [NVIDIA dev blog: open beta](https://developer.nvidia.com/blog/polars-gpu-engine-powered-by-rapids-cudf-now-available-in-open-beta/) (accessed 2026-10-28)
- **Starburst (Trino):**
  - On 2026-06-01, Starburst said it is integrating cuDF at the physical-operator level of its proprietary Trino distribution (not in open-source Trino). Completed operators: Parquet scan, filter, aggregation, TopN, join. GPU exchange is still in progress. Results so far: TPC-H geomean 4.6x and ClickBench 3.4x on L4 and GB202 (vendor claim). NVIDIA contributed cuDF expertise, memory management and Blackwell guidance. Status is active development, not GA — [Starburst blog](https://www.starburst.io/blog/gpu-accelerated-sql-analytics-how-starburst-and-nvidia-deliver-industry-benchmark-speedups-on-gpu-infrastructure/) (accessed 2026-10-28)
  - NVIDIA lists Starburst at up to 3x on Vera and up to 7x on RTX PRO 6000 — [NVIDIA data processing solutions](https://www.nvidia.com/en-us/solutions/data-processing/) (accessed 2026-10-28)
- **Transwarp:** A GTC 2026 talk cited 150 GB TPC-DS on Sirius/DuckDB running 26x faster than CPU DuckDB — [third-party GTC notes](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28)
- **Kinetica:**
  - Has an archived NVIDIA partner page (DGX, RAPIDS integration) — [Kinetica archive](https://archive.kinetica.com/partner/nvidia/) (accessed 2026-10-28; older)
  - At GTC 2026, Kinetica was named only as a validator of Vera CPU gains, not GPU — [third-party GTC notes](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28)
  - Listed among cuVS integrations — [NVIDIA cuVS blog](https://developer.nvidia.com/blog/optimizing-vector-search-for-indexing-and-real-time-retrieval-with-nvidia-cuvs/) (accessed 2026-10-28)
- **SQream:**
  - Was "one of the newest partners of NVIDIA Inception" at GTC 2024 (older) — [SQream GTC 2024 page](https://sqream.com/event/gtc-nvidia-event-2024-sqream-sponsorship/) (accessed 2026-10-28)
  - Its partners page claims an NVIDIA partnership — [SQream partners](https://sqream.com/about/partners/) (accessed 2026-10-28)
- **Brytlyt:** Became an NVIDIA Inception Premier Partner in January 2023 (older) — [PR Newswire](https://prnewswire.co.uk/news-releases/brytlyt-becomes-nvidia-inception-premier-partner-301734551.html) (accessed 2026-10-28)
- **ClickHouse:**
  - A ClickHouse-member RFC (opened 2026-08-27) proposes "As a first step … build ClickHouse on top of Sirius" via Substrait, with an alternative of in-engine cuDF/cuCascade operators. It is open with no milestone — [ClickHouse issue #116807](https://github.com/ClickHouse/ClickHouse/issues/116807) (accessed 2026-10-28)
  - A PR dated 2026-10-21 bumps cuDF to 26.12 for streaming groupby — [ClickHouse PR #127166](https://github.com/ClickHouse/ClickHouse/pull/127166) (accessed 2026-10-28)
  - NVIDIA's page lists ClickHouse only for a Vera CPU blog post, "Running ClickHouse on NVIDIA Vera" — [NVIDIA data processing solutions](https://www.nvidia.com/en-us/solutions/data-processing/) (accessed 2026-10-28)
  - I found no joint GPU announcement.
- **Apache Gluten (Spark plus Velox):**
  - The cuDF backend is enabled and experimental. A recent issue reports TPC-DS SF1 failures with CPU fallback disabled, and scan stays on CPU by default — [Gluten issue #13122](https://github.com/apache/gluten/issues/13122) (accessed 2026-10-28)
  - NVIDIA recommends Velox+cuDF as the way to add GPU without forking, naming Presto C++ and Spark via Gluten — [NVIDIA cuDF FAQ](https://perspectives.nvidia.com/cudf/faq/we-want-to-add-a-gpu-execution-path-to-our-existing-query-engine-but-don-t-want-to-fork-the-mainline-6627a6/) (accessed 2026-10-28)
- **Vector databases (cuVS):** cuVS integrations include FAISS, Milvus, Lucene, Elasticsearch, OpenSearch, Weaviate, Kinetica, AlloyDB and Vertex AI. Weaviate reports 8x faster index builds and OpenSearch 3.0 up to 9.4x (vendor claims) — [NVIDIA cuVS blog](https://developer.nvidia.com/blog/optimizing-vector-search-for-indexing-and-real-time-retrieval-with-nvidia-cuvs/) (accessed 2026-10-28); [cuVS integrations docs](https://docs.nvidia.com/cuvs/getting-started/integrations) (accessed 2026-10-28)
- **No evidence found** for GPU efforts at StarRocks itself (only the Sirius "coming soon"), VeloDB, Databend, LanceDB-cuVS, DataFusion Comet GPU, or MotherDuck. PG-Strom and BlazingSQL were not researched in depth; BlazingSQL is referenced only as the Theseus team's predecessor.

### Inferences
- NVIDIA's revealed preference is to absorb promising GPU-database IP (HEAVY.AI talent, the Theseus ideas behind cuCascade) into open libraries, rather than fund standalone GPU database companies (estimate).
- Sirius is effectively NVIDIA's reference GPU SQL engine, co-built with NVIDIA engineers and promoted on NVIDIA blogs and GTC stages. A startup building on Sirius is building on NVIDIA's favored path, but with no moat in the engine itself (estimate).

### Gaps
- Primary-source confirmation of the HEAVY.AI deal terms and the HeavyDB open-source relaunch.
- An official DuckDB Labs statement on Sirius as a "core extension." MotherDuck's GPU stance was not found.
- Current Inception membership lists: NVIDIA does not publish them. I relied on vendor press releases.

## Q3. NVIDIA announcements naming GPU SQL partners in 2025–2026 GTC keynotes

### Takeaway
At GTC 2026, NVIDIA framed data processing as a keynote pillar and named platform partners (IBM, Dell, Google Cloud, AWS, Azure, Oracle, CoreWeave), plus Databricks and Snowflake as ecosystem platforms. The technical GPU SQL sessions featured Presto/Velox with IBM, Sirius/DuckDB, Starburst/Trino, and Theseus research. Standalone GPU database vendors were absent from those GPU sessions.

### Cited Findings
- Keynote partner list and data-processing segment — [NVIDIA GTC 2026 live blog](https://blogs.nvidia.com/blog/gtc-2026-news/) (accessed 2026-10-28)
- Session-level content:
  - S81563: Presto GPU with IBM
  - S81870 and S81873: Sirius, cuCascade, Theseus
  - S81769: cuDF for Spark, Trino and DuckDB; Starburst; HEAVY.AI; and cuVS with Milvus, Elastic and OpenSearch
  - Polars, ClickHouse, Doris, StarRocks, SQream, Databricks and Snowflake were not mentioned in these sessions
  - Source: [third-party GTC notes](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28)
- NVIDIA's standing data-processing page lists the partners as:
  - AWS, Google Cloud, Cloudera, Dell, HPE, Oracle, IBM, Starburst, Redpanda and ClickHouse
  - the open-source engines Spark, Presto, Polars, DuckDB, Velox, OpenSearch, Elastic and Milvus
  - Source: [NVIDIA data processing solutions](https://www.nvidia.com/en-us/solutions/data-processing/) (accessed 2026-10-28)

### Inferences
- The partner tiers are hyperscalers and enterprise platforms first, then open-source engines, then ISVs that integrate cuDF into their own engine (Starburst). Startups appear only as research or open-source contributors (estimate).

### Gaps
- No slide image or official logo list for the keynote's data-processing segment. GTC 2025 keynote data-processing partners were not checked.

## Q4. The pattern: cuDF inside existing engines, not new standalone GPU databases

### Takeaway
The evidence strongly supports this pattern. NVIDIA's own messaging is "accelerate the engine you already run, no code changes," and every flagship integration is an existing engine (Spark, Presto/Velox, Polars, pandas, DuckDB via Sirius, Trino via Starburst, the vector databases via cuVS). NVIDIA has acquired a standalone GPU database (HEAVY.AI) rather than backing it.

### Cited Findings
- NVIDIA's position is to accelerate "the engine itself on GPUs rather than switching other engines", naming Spark, Presto via Velox, Polars and DuckDB. The page is AI-generated content per its disclaimer — [NVIDIA cuDF perspectives](https://perspectives.nvidia.com/cudf/accelerating-existing-sql-engines-in-place-for-faster-query/) (accessed 2026-10-28)
- NVIDIA recommends the Velox cuDF backend so engines gain GPU execution "without forking," keeping their planner and API — [NVIDIA cuDF FAQ](https://perspectives.nvidia.com/cudf/faq/we-want-to-add-a-gpu-execution-path-to-our-existing-query-engine-but-don-t-want-to-fork-the-mainline-6627a6/) (accessed 2026-10-28)
- NVIDIA tells customers to use "their existing data stack," with "drop-in accelerators" — [NVIDIA data processing solutions](https://www.nvidia.com/en-us/solutions/data-processing/) (accessed 2026-10-28)
- GTC 2026 speakers repeated "no rewrite required." cuDF is positioned for "accelerating large-scale systems like databases and lakehouses," and "cuDF itself is not intended to be a multi-GPU library" — [third-party GTC notes](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28)
- Pavlo: GPU databases look "inviable" as standalone businesses (Voltron dead, HeavyDB acquihired) — [Pavlo, Databases in 2025](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html) (accessed 2026-10-28)
- Even AMD copies the library approach: ROCm-DS is a fork of RAPIDS, and hipDF is API-compatible with cuDF but early access and Python-only. I found no AMD SQL or Spark accelerator — [AMD ROCm-DS docs](https://rocm.docs.amd.com/projects/rocm-ds/en/latest/index.html) (accessed 2026-10-28); [hipDF 25.05 docs](https://rocm.docs.amd.com/projects/hipDF/en/docs-25.05/index.html) (accessed 2026-10-28)

### Inferences
- NVIDIA's moat is the libraries (cuDF, cuVS, cuCascade, RMM, UCX). Engines are distribution. It wants every engine to depend on CUDA-X, not one winner (estimate).
- Two kinds of third party fit NVIDIA's plan (estimate):
  - those that bring an installed base of an existing engine (IBM, Starburst, Cloudera, ClickHouse)
  - those that build open, reusable components (Sirius)

  A new proprietary standalone GPU database fits neither.
- Gap NVIDIA explicitly leaves: cuDF is single-GPU. Distributed execution, exchange, spilling and multi-node orchestration are left to engines. NVIDIA is partly filling this with cuCascade, RAPIDS MPF and UCXExchange in Presto (estimate, based on the GTC notes above).

### Gaps
- No explicit NVIDIA statement saying "we will not build a database." The pattern is inferred from actions and messaging.

## Q5. Candidate directions: does an NVIDIA-blessed partner already occupy each?

### Takeaway
Spark, lakehouse/Trino, Presto/Velox, DuckDB and vector search are all occupied by NVIDIA itself or a blessed partner. ClickHouse is being done in-house by ClickHouse on Sirius. The open spaces are:
- streaming SQL (Flink): only Alibaba–NVIDIA research
- the Doris/StarRocks/VeloDB family: only a Sirius paper result and a "coming soon"
- DataFusion
- Databend
- a non-NVIDIA (AMD) cuDF-equivalent SQL layer
- a real-time VRAM-resident cache product

### Cited Findings and per-direction verdict
- **spark-plugin — occupied by NVIDIA itself.**
  - NVIDIA's cuDF plugin for Spark runs on Databricks, EMR, Dataproc, Oracle AIDP and Cloudera; Project Aether handles migration — [NVIDIA Databricks docs](https://docs.nvidia.com/cudf-spark/26.06/getting-started/databricks.html) (accessed 2026-10-28); [Project Aether blog](https://developer.nvidia.com/blog/migrate-apache-spark-workloads-to-gpus-at-scale-on-amazon-emr-with-project-aether/) (accessed 2026-10-28)
  - Gluten+Velox+cuDF is a second path — [Gluten issue #13122](https://github.com/apache/gluten/issues/13122) (accessed 2026-10-28)
- **lakehouse-iceberg — occupied:**
  - Starburst/Trino cuDF integration (proprietary distribution, in development) — [Starburst blog](https://www.starburst.io/blog/gpu-accelerated-sql-analytics-how-starburst-and-nvidia-deliver-industry-benchmark-speedups-on-gpu-infrastructure/) (accessed 2026-10-28)
  - IBM watsonx.data Presto+Velox+cuDF — [IBM newsroom](https://newsroom.ibm.com/2026-03-16-ibm-and-nvidia-announce-expanded-collaboration-at-gtc-2026-to-advance-ai-for-the-enterprise) (accessed 2026-10-28)
  - Open-source Trino has no GPU path (per the Starburst blog). Iceberg-specific GPU work was not found.
- **clickhouse — occupied by ClickHouse Inc. itself**, building on Sirius and cuDF in-tree (RFC August 2026, cuDF bump October 2026). There is no NVIDIA GPU announcement yet; NVIDIA lists ClickHouse only for Vera CPU — [ClickHouse issue #116807](https://github.com/ClickHouse/ClickHouse/issues/116807) (accessed 2026-10-28); [ClickHouse PR #127166](https://github.com/ClickHouse/ClickHouse/pull/127166) (accessed 2026-10-28)
  - A third-party ClickHouse GPU product would compete with upstream (estimate).
- **realtime-other (Doris/VeloDB/StarRocks) — not occupied by a vendor; Sirius is reaching in.**
  - The Sirius paper shows up to 12.5x with Apache Doris — [arXiv 2508.04701](https://arxiv.org/html/2508.04701v3) (accessed 2026-10-28)
  - The Sirius README lists StarRocks as "coming soon" — [Sirius GitHub](https://github.com/sirius-db/sirius) (accessed 2026-10-28)
  - I found no VeloDB, CelerData or StarRocks first-party GPU announcement.
- **datafusion-plugin — no blessed occupant found.** I found no NVIDIA–DataFusion or Comet GPU announcement. Not found ≠ absent; it was searched only indirectly.
- **distributed-pluggable (Databend) — no occupant found.** Theseus (distributed GPU SQL) is dead as a company, though its ideas live in NVIDIA's cuCascade — [Pavlo](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html) (accessed 2026-10-28); [third-party GTC notes](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28)
  - Sirius has no multi-node support yet — [Sirius GitHub](https://github.com/sirius-db/sirius) (accessed 2026-10-28)
- **streaming-flink — weakly occupied.**
  - Alibaba Cloud and NVIDIA discussed a collaboration on multimodal stream processing in Flink at Flink Forward Asia 2026, with no cuDF or benchmarks named — [Alizila](https://www.alizila.com/alibaba-cloud-pushes-open-source-apache-flink-toward-agentic-streaming-for-ai/) (accessed 2026-10-28; page returned 403, content from search snippet only)
  - NVIDIA's streaming partner Redpanda is promoted for the Vera CPU (6x lower p99), not GPU — [NVIDIA data processing solutions](https://www.nvidia.com/en-us/solutions/data-processing/) (accessed 2026-10-28)
- **realtime-vram-cache — partially occupied.**
  - Sirius `pin_table` keeps tables in GPU memory for hot runs — [Sirius GitHub](https://github.com/sirius-db/sirius) (accessed 2026-10-28)
  - Kinetica and HeavyDB (now NVIDIA) historically owned "GPU in-memory real-time analytics" — [Kinetica archive](https://archive.kinetica.com/partner/nvidia/) (accessed 2026-10-28); [Wikipedia HEAVY.AI](https://en.wikipedia.org/wiki/HEAVY.AI) (accessed 2026-10-28)
  - A HeavyDB open-source relaunch by NVIDIA, if real, would directly occupy this space — [third-party GTC notes](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28)
- **vector-sql — heavily occupied** via cuVS in Milvus, Weaviate, Elastic, OpenSearch, Lucene, FAISS, Kinetica, AlloyDB and Oracle 26ai — [NVIDIA cuVS blog](https://developer.nvidia.com/blog/optimizing-vector-search-for-indexing-and-real-time-retrieval-with-nvidia-cuvs/) (accessed 2026-10-28); [NVIDIA GTC 2026 live blog](https://blogs.nvidia.com/blog/gtc-2026-news/) (accessed 2026-10-28)
- **other-accelerators (AMD, Cerebras) — not occupied by NVIDIA by definition, and thin elsewhere.** AMD's hipDF is an early-access, Python-only cuDF fork. I found no SQL engine on it — [AMD ROCm-DS docs](https://rocm.docs.amd.com/projects/rocm-ds/en/latest/index.html) (accessed 2026-10-28). Cerebras SQL work was not found.

### Inferences
- Where NVIDIA leaves room (estimate):
  - engines with a sizeable installed base whose vendor has not yet moved: Doris/VeloDB, StarRocks, Databend, DataFusion-based systems, Flink SQL
  - operational layers NVIDIA does not want to own: distributed scheduling, multi-tenant serving, managed service, cost-aware CPU/GPU routing
  - hardware-neutral layers
- Risks:
  - Sirius is already reaching into the Doris/StarRocks space with NVIDIA engineers.
  - NVIDIA could absorb any successful open component, as it did with Theseus ideas and HEAVY.AI.

### Gaps
- No first-party checks for VeloDB/Doris, StarRocks/CelerData, Databend, DataFusion/Comet, LanceDB, MotherDuck or PG-Strom/HeteroDB within the tool budget. A follow-up should search each by name with "GPU."
- No Cerebras data-processing evidence.
