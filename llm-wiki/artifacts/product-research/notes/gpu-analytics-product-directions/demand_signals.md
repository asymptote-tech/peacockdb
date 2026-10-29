# Demand signals and post-mortems for GPU-accelerated SQL/analytics

All links accessed 2026-10-28. Research note: the session's shared WebSearch budget ran out partway through, so Reddit, Stack Overflow, Discord, Benn Stancil and Data Engineering Weekly were not searched directly. Those gaps are listed below. Vendor numbers are vendor claims unless noted.

## 1. Where practitioners ask for GPU SQL/analytics, and what they want

### Takeaway
Public demand is thin and centred on **cost of heavy batch ETL**: shuffle-heavy Spark/Glue/BigQuery jobs and large daily metric pipelines. Nobody in what I found asks for interactive dashboards or laptop use. On HN, GPU SQL launches get a moderate response (about 135–145 points), and the comments are mostly sceptical: data movement, PCIe/IO-bound pipelines, OLTP, hardware cost, and Spark compatibility.

### Cited Findings
- Launch HN: ParaQuery (YC X25), "GPU Accelerated Spark/SQL", got 135 points and 81 comments (2025-05-12). Sirius ("A GPU-native SQL engine") got 145 points and 19 comments (2025-06-28). An HN Algolia search for "GPU SQL" stories with more than 30 points since 2024 found no other GPU SQL launches. — [HN Algolia query](https://hn.algolia.com/api/v1/search?query=GPU%20SQL&tags=story&numericFilters=created_at_i%3E1704067200,points%3E30) (accessed 2026-10-28)
- The GPU database stories with more than 100 points on HN are almost all from 2012–2017: MapD open source (263 points), Alenka (364 points), BlazingDB TechCrunch (121 points), Brytlyt (118 points). — [HN Algolia query](https://hn.algolia.com/api/v1/search?query=GPU%20database&tags=story&numericFilters=points%3E100) (accessed 2026-10-28)
- **Explicit demand quote.** dogman123 on the ParaQuery launch: "Currently struggling to complete jobs with massive amounts of shuffle with Spark on EMR … We use Glue currently, but it has become cost prohibitive." — [HN 43964505](https://news.ycombinator.com/item?id=43964505) (accessed 2026-10-28)
- ParaQuery's launch targets teams with large ETL workloads and high BigQuery bills. It claims one customer saved more than 60% on their BigQuery bill with a 2x speedup, and a 44-minute BigQuery job ran in 5.5 minutes. The founder reported "a 450TB shuffle" on one large query. The product is built on Spark-RAPIDS. — [HN 43964505](https://news.ycombinator.com/item?id=43964505) (accessed 2026-10-28)
- **Sceptics on the same thread:**
  - threeseed: "you will be able to accelerate the query part of the pipeline but that's like 20% of what a typical job does", and "Enterprises have almost entirely moved away from self-hosting software."
  - andygrove: "Spark compatibility is notoriously difficult with Spark accelerators built with non-JVM languages."
  - Boxxed: "I'm surprised the GPU is a win when the data is coming from GCS."
  - mritchie712 cited data that most BigQuery queries process under 100 MB.
  - Source for all four: [HN 43964505](https://news.ycombinator.com/item?id=43964505) (accessed 2026-10-28)
- **Sirius thread.** bob1029: "when it comes to the nasty OLTP use cases the CPU will absolutely dominate" and "How does the information get to & from the GPU in the first place?". RachelF: "Pity it requires Volta 7 which is rather high end for fiddling around on at home." antonmks: "TPCH SF=100 would fit nicely into GPU memory" (wants larger-scale tests). Joel_Mckay raised GPU reliability concerns. — [HN 44404876 via Algolia](https://hn.algolia.com/api/v1/items/44404876) (accessed 2026-10-28)
- A mirror of the Sirius thread shows commenters arguing about whether "44% faster than DuckDB" is marginal against a promised 10x. Others asked about running Sirius on non-NVIDIA hardware via ZLUDA or SCALE, and wanted Vulkan support. — [HN mirror](https://astro-hacker-news.pages.dev/46441781) (accessed 2026-10-28; this is a secondary mirror, not the live page)
- **DuckDB.** I found no official GPU feature request in the DuckDB project. GPU work there is third-party: Sirius (Substrait-based extension, UW-Madison with NVIDIA), the `gpudb` community extension (GPU sum/min/max on CUDA and Metal), and duckdb-mlx (Apple Silicon). — [Sirius repo](https://github.com/sirius-db/sirius); [gpudb extension](https://duckdb.org/community_extensions/extensions/gpudb); [duckdb-mlx](https://awesome.ecosyste.ms/projects/github.com%2Fquackscience%2Fduckdb-mlx) (accessed 2026-10-28)
- **Polars GPU engine.** Out-of-memory errors are the visible pain. RAPIDS 24.12 added chunked Parquet reads and CUDA unified memory "to process workloads that can fit in combined GPU+CPU memory but would previously cause GPU out-of-memory errors". Managed memory "imposes a performance cost", and ZSTD decompression can still run out of memory during IO. The docs pitch the engine for datasets of "hundreds of gigabytes or more". — [NVIDIA RAPIDS 24.12 blog](https://developer.nvidia.com/blog/rapids-24-12-introduces-cudf-on-pypi-cuda-unified-memory-for-polars-and-faster-gnns); [Polars GPU docs](https://docs.pola.rs/user-guide/gpu-support/) (accessed 2026-10-28)
- The longstanding objection goes back to 2013. The top Slashdot answer to "Why don't open source databases use GPUs?" was that database applications "bottleneck on I/O, not processor". — [Slashdot 2013](https://developers.slashdot.org/story/13/12/25/1327216/why-dont-open-source-databases-use-gpus) (accessed 2026-10-28)

### Inferences
- The one recurring, named pain is **expensive shuffle-heavy batch ETL**: EMR/Glue/BigQuery bills. That is a distributed problem, not a single-node one.
- The HN audience treats GPU SQL as a research curiosity. A launch draws about 140 points and mostly sceptical comments, so HN is not where the first user will come from (estimate).
- Wanting GPU SQL on non-NVIDIA hardware (ZLUDA, SCALE, Vulkan, Metal) shows up only as hobbyist curiosity, not as buyer pain.

### Gaps
- Reddit r/dataengineering, Stack Overflow, and Discord/Slack quotes were not collected because the search budget ran out and no Reddit thread surfaced in the earlier searches.
- No ClickHouse GitHub issue requesting GPU was found (not searched directly).

## 2. Post-mortems and pivots

### Takeaway
2025 was a bad year for GPU databases. In his 2025 review, Andy Pavlo calls Voltron Data dead and describes HeavyDB as "sort-of acquihired" by NVIDIA. He frames both as continuing "the trend of the inviability of GPU-accelerated databases", says Kinetica lives off government contracts, and says SQream "still appears to be kicking". The GPU data processing startups that did exit were bought for the storage layer or the AI-data story, not for SQL.

### Cited Findings
- **Pavlo, 2025 retrospective (Jan 2026):**
  - Voltron "was supposed to be the supergroup of database companies", combining "the Peruvian GPU wizards from BlazingSQL" and "top engineers from Nvidia Rapids". "They built a GPU-accelerated database (Theseus), but failed to launch it in a timely manner."
  - "The death of Voltron and sort-of acquihire of HeavyDB seem to continue the trend of the inviability of GPU-accelerated databases"; "Kinetica has been milking government contracts for years"; "Sqream still appears to be kicking it"; "nobody has been able to make a significant dent in the dominance of CPU-powered DBMSs".
  - Prediction: "you will hear some major GPU-accelerated database announcements by vendors in 2026".
  - Also listed: MyScaleDB (vector SQL) closed in May 2025, and Oxla was acquired by Redpanda.
  - Source: [Pavlo, Databases in 2025](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html) (accessed 2026-10-28)
- **Voltron Data before its death:**
  - About 14 enterprise customers including two large government agencies, with early adoption in financial services. One retailer reportedly went from 1,400 CPU machines to 14 GPU servers. — [VentureBeat, Accenture partnership](https://venturebeat.com/ai/voltron-data-just-partnered-with-accenture-to-solve-one-of-ais-biggest-headaches) (accessed 2026-10-28)
  - Accenture Ventures invested in February 2025 (amount undisclosed). — [FinSMEs](https://www.finsmes.com/2025/02/voltron-data-receives-investment-from-accenture-ventures.html) (accessed 2026-10-28)
  - Carahsoft became its government reseller in April 2025. — [Yahoo/GlobeNewswire](https://finance.yahoo.com/news/voltron-data-carahsoft-partner-bring-160000202.html) (accessed 2026-10-28)
  - HPE embedded Theseus in Ezmeral. — [VMblog](https://vmblog.com/bylines/voltron-data-launches-theseus-to-unlock-the-power-of-the-largest-data-sets-for-ai/) (accessed 2026-10-28)
  - Voltron acquired Claypot for real-time features. — [VentureBeat](https://venturebeat.com/data-infrastructure/exclusive-voltron-data-acquires-claypot-to-unlock-real-time-ai-with-modular-data-systems) (accessed 2026-10-28)
  - Total raised was about $110M across two rounds. — [Bitscale aggregator](https://bitscale.ai/directory/voltron-data) (accessed 2026-10-28; aggregator, low confidence)
  - An aggregator cites The Information on a November 2024 CEO change and 50% layoffs. — [CB Insights compare page](https://www.cbinsights.com/compare/datapelago-vs-voltron-data) (accessed 2026-10-28; secondhand, unverified)
- **HEAVY.AI (MapD → OmniSci → HeavyDB):** acquired by NVIDIA in 2025. — [Wikipedia](https://en.wikipedia.org/wiki/HEAVY.AI); [PitchBook](https://pitchbook.com/profiles/company/98344-09) (accessed 2026-10-28). dbdb.io says the project was abandoned after the acquisition. — [dbdb.io](https://dbdb.io/db/heavydb/revisions/9) (accessed 2026-10-28; single source)
- **SQream:** the last round I found is a $45M Series C in September 2023, bringing the total to $135M. It is pitched at GPU data preparation ("SQream Blue … data preparation lakehouse"), and the CEO tied the round to generative-AI demand. — [Globes](https://en.globes.co.il/en/article-israeli-data-analytics-co-sqream-raises-45m-1001457706); [SQream blog](https://sqream.com/blog/sqream-raises-45m-to-expand-gpu-analytics-machine-learning/) (accessed 2026-10-28). I found no 2025–2026 news.
- **Kinetica:** its origin is a 2009 build for US Army INSCOM and the NSA. It raised a $50M Series A in 2017. By September 2024 its most recent visible product push was SQL-GPT (natural language to SQL, aimed at telecom). — [Kinetica news](https://kinetica.com/news); [RTInsights](https://www.rtinsights.com/kinetica-raises-50-million-in-venture-capital/) (accessed 2026-10-28). I found no 2025–2026 news.
- **DataPelago:** came out of stealth in October 2024 with $47M (Eclipse, Taiwania, Qualcomm Ventures). Its "universal data processing engine" sits between the data lake and the query engine (Spark, Trino) and runs on CPU, GPU or FPGA. — [SiliconANGLE](https://siliconangle.com/2024/10/01/datapelago-raises-47m-optimize-hardware-analytical-workloads/) (accessed 2026-10-28)
  - **NetApp acquired DataPelago in July 2026** to run "GPU-accelerated data processing at storage layer". DataPelago's Nucleus engine "uses CPUs and GPUs to process data where it resides". It continues as a NetApp subsidiary, and NetApp claims up to 80% lower cost and up to 10x speed. The price was not disclosed. — [NetApp press release PDF](https://s21.q4cdn.com/371534297/files/doc_news/NetApp-Acquires-DataPelago-Making-Data-AI-Ready-at-the-Infrastructure-Layer-2026.pdf); [Techzine](https://www.techzine.eu/news/infrastructure/142971/netapp-acquires-datapelago-for-gpu-accelerated-data-processing/); [Orrick](https://www.orrick.com/en/News/2026/07/NetApp-Acquires-DataPelago) (accessed 2026-10-28)
- **Technical reasons, from the research literature:** the PCIe transfer bottleneck "has to either be reduced or concealed". Existing GPU databases "suffer from limited GPU memory capacity and PCIe bandwidth, thereby failing to scale to large datasets". — [Breß et al. 2014 survey PDF](https://www.se.cs.uni-saarland.de/publications/docs/BHS+14.pdf); [UWaterloo seminar listing](https://cs.uwaterloo.ca/events/day/2024-3-12) (accessed 2026-10-28)

### Inferences
- **Failure pattern.** Voltron (about $110M raised) and HeavyDB tried to sell a standalone GPU database. The exits that happened (DataPelago to NetApp, HeavyDB to NVIDIA) were acqui-hires or storage-layer plays, which suggests the buyer is an **infrastructure vendor**, not an analytics team.
- **NVIDIA is now the main player.** It owns HeavyDB, backs Sirius, ships spark-rapids, cudf-polars and cudf.pandas, and markets all of them. Any open GPU SQL engine competes with free NVIDIA software.
- Kinetica and Voltron's government sales (Carahsoft, federal agencies) show that air-gapped government buyers do pay for GPU analytics.

### Gaps
- I found no primary news source (press release or TechCrunch) for Voltron's shutdown date. Pavlo is the only citation.
- Brytlyt and Zilliz were not researched (search budget).
- No Pavlo 2023 or 2024 quotes on GPUs were collected.

## 3. YC and seed-stage startups, 2023–2026

### Takeaway
Few startups are funded specifically for GPU SQL. ParaQuery (YC X25, a managed spark-rapids service against BigQuery and Databricks) is the only YC example found. The well-funded neighbours are **multimodal AI-data engines** (Eventual/Daft and LanceDB, about $30M each in 2025), not GPU SQL.

### Cited Findings
- **ParaQuery (YC X25):** "fully-managed GPU-accelerated Spark solution providing double the performance at half the cost compared to solutions like Databricks and BigQuery". It built on the Spark-RAPIDS plugin for more than a year before launch. A third-party aggregator lists a $500K seed. — [YC profile](https://www.ycombinator.com/companies/paraquery); [Launch HN](https://news.ycombinator.com/item?id=43964505); [startupintros](https://startupintros.com/orgs/paraquery) (accessed 2026-10-28). I found no 2026 news on traction.
- **Eventual (Daft):** $20M Series A (Felicis; M12 and Citi participating) plus an earlier seed, in June 2025. Daft is used in production at AWS, Essential AI and Together. Eventual sees "the same pattern playing out at AI labs training foundation models". — [SiliconANGLE](https://siliconangle.com/2025/06/24/eventual-launches-30m-funding-streamline-multimodal-data-processing/); [Daft blog](https://www.daft.ai/blog/eventual-raises-30m-to-build-the-future-of-data); [TechCrunch](https://www.techcrunch.com/2025/06/24/how-a-data-processing-problem-at-lyft-became-the-basis-for-eventual/) (accessed 2026-10-28). The seed size conflicts across sources: $10M (SiliconANGLE) vs $7.5M (TechCrunch).
- **LanceDB:** $30M Series A led by Theory Ventures, with CRV, YC and Databricks Ventures participating. It positions itself as a "multimodal lakehouse". — [LanceDB blog](https://lancedb.com/blog/series-a-funding) (accessed 2026-10-28)
- **DataPelago:** $47M, then acquired by NetApp (see section 2).
- **Sirius:** open source (Apache 2.0), led by UW-Madison with NVIDIA support. The December 2025 NVIDIA post describes no commercial offering or customers. Co-author Rodrigo Aramburu previously co-founded BlazingSQL and Voltron Data. The roadmap covers spilling beyond GPU memory, multi-node multi-GPU, and GPU-native file readers. — [NVIDIA blog, Sirius ClickBench](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/) (accessed 2026-10-28)
- A search for 2026 seed rounds in GPU SQL analytics found none. The adjacent rounds were GPU infrastructure (hosted.ai, $19M seed) and an AI-native BI company (Golden Analytics, $14M seed extension). — [hosted.ai](https://www.hosted.ai/blogs/hostedai-closes-19m-seed-round-to-transform-gpu-infrastructure-economics); [GeekWire](https://www.geekwire.com/2026/golden-analytics-lands-14m-seed-extension-and-opens-ai-platform-to-public-beta/) (accessed 2026-10-28)
- Other recent YC launches on HN are about GPU **capacity**, not GPU data processing: Expanse (P26) says HPC/GPU clusters run at "30-40% effective utilization", and Stoa Markets (S26) is a GPU marketplace. — [Expanse Launch HN](https://news.ycombinator.com/item?id=48356312); [HN Algolia query](https://hn.algolia.com/api/v1/search?query=GPU%20SQL&tags=story&numericFilters=created_at_i%3E1704067200,points%3E30) (accessed 2026-10-28)

### Inferences
- Investors in 2025–2026 funded the "data for AI/multimodal" framing, not "faster SQL on GPU". A YC-sized round for a GPU SQL engine would probably need the AI-data or cost-of-ETL framing (estimate).
- ParaQuery has the closest pitch to the cost-ETL direction and has already taken the "managed spark-rapids" slot. Its lack of visible traction a year later is weak evidence that the slot is hard (estimate).

### Gaps
- e6data and Theseus-adjacent startups were not checked. As far as I know e6data is a CPU engine (estimate, not verified).
- The YC directory was not scanned exhaustively (search budget).

## 4. Cost-driven demand and GPU economics

### Takeaway
The strongest public paying-user evidence is **Snap**: A/B-testing metric pipelines on Spark plus cuDF over about 10 PB/day in a 3-hour window, at 76% lower daily cost. It is NVIDIA-marketed and runs on stock spark-rapids. H100 rental prices fell in 2025 but **rose about 40% from October 2025 to March 2026**, which weakens a "cheap idle GPUs" thesis in 2026.

### Cited Findings
- **Snap** (NVIDIA blog, 2026-03-17):
  - The pipeline runs thousands of experiments a month, "over 10 petabytes of data in a three-hour window each morning", across nearly 6,000 metrics.
  - It uses about 2,100 concurrent NVIDIA L4 GPUs on GCP G2, against a projected 5,500.
  - Results: 4x runtime speedup on the same machine count and 76% daily cost saving versus CPU-only. Snap says GPU pipelines "flatten the scaling curve" of its experimentation roadmap.
  - Source: [NVIDIA blog, Snap](https://blogs.nvidia.com/blog/snap-accelerated-data-processing/) (accessed 2026-10-28)
  - A podcast description adds that Snap reuses idle inference GPUs overnight. The blog itself does not say this. — [Podbean, NVIDIA AI Podcast](https://www.podbean.com/ew/dir-ytehw-2db39e2c) (accessed 2026-10-28; unverified)
- **GTC 2026** (secondary summary, March 2026):
  - cuDF now accelerates Spark, Presto, DuckDB, Polars and Velox, plus Databricks, Snowflake, Starburst and EDB Postgres. Other integrators include IBM watsonx.data, Dell AI Data Platform, Google Dataproc and Oracle AI Database 26ai.
  - Customer claims: IBM watsonx.data with Nestlé saw 5x faster workloads and 83% lower cost.
  - cuVS is integrated into Milvus, Weaviate, Lucene, pgvector and AlloyDB.
  - Source: [NextBigFuture](https://www.nextbigfuture.com/2026/03/nvidia-structured-data-is-the-ground-truth-of-ai-120-billion-structure-data-ecosystem.html) (accessed 2026-10-28; secondary, verify against NVIDIA)
- **Snowflake:** cuDF and cuML ship in the default GPU environment of Snowflake ML. — [Snowflake blog](https://www.snowflake.com/en/blog/nvidia-gpu-acceleration/) (accessed 2026-10-28)
- **Databricks:** the RAPIDS Accelerator is supported, but "on a multi-GPU node, RAPIDS Accelerator can use only one GPU" because of a Databricks limitation. — [NVIDIA docs, Databricks](https://docs.nvidia.com/spark-rapids/user-guide/latest/getting-started/databricks.html) (accessed 2026-10-28)
- **Cloudera** announced GPU-accelerated Spark 4.1 (up to 4x) in Cloudera Data Engineering on 2026-08-20. — [shashi.co](https://www.shashi.co/2026/08/cloudera-speeds-up-spark-41-fourfold.html) (accessed 2026-10-28; secondary)
- **H100 prices:**
  - One-year contract rates went from $1.70 (October 2025) to $2.35 (March 2026), about +40%, citing SemiAnalysis.
  - The on-demand median was about $3.39/hr across 53 providers in September 2026, up about 11% year on year.
  - Hyperscaler single-GPU rates are about $6.9/hr (AWS, Azure).
  - Source: [Akash, H100 price 2026](https://akash.network/the-bid/h100-rental-price-2026-cost-per-hour/) (accessed 2026-10-28)
  - AWS Capacity Blocks went from $5.191 to $5.970 per H100-hour, and Runpod SXM from $3.29 to $3.99. — [IntuitionLabs](https://intuitionlabs.ai/articles/h100-rental-prices-cloud-comparison) (accessed 2026-10-28)
  - AWS cut H100 prices by about 44% in June 2025. — same IntuitionLabs page (accessed 2026-10-28)
- **Sirius ClickBench economics:** a Lambda GH200 at $1.5/hr beat CPU instances at $7.3–9.8/hr (c6a.metal, c8g.metal-48xl, c7a.metal-48xl), for "at least 7.2x higher" cost efficiency. — [NVIDIA blog, Sirius](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/) (accessed 2026-10-28). The CIDR 2026 paper reports 8.3x (TPC-H) and 7.4x (ClickBench) cost efficiency. — [CIDR 2026](https://vldb.org/cidrdb/2026/rethinking-analytical-processing-in-the-gpu-era.html) (accessed 2026-10-28)
- The Expanse founders measured 59% wasted compute across 122k jobs on a national HPC cluster. — [Expanse Launch HN](https://hn.algolia.com/api/v1/items/48356312) (accessed 2026-10-28)
- The Expanse thread contained no comments about running ETL or analytics on idle GPUs. — [Expanse Launch HN](https://hn.algolia.com/api/v1/items/48356312) (accessed 2026-10-28)

### Inferences
- Paying cost-driven demand exists at **very large scale** (Snap at 10 PB/day, Nestlé through IBM). It is served by NVIDIA plus a platform vendor, so a single-node engine does not fit that buyer.
- **Pricing caveat.** GPU cost-efficiency claims (7–8x) were measured at $1.5/hr GH200 rates. With 2026 rental prices rising and capacity sold out, the "cheap idle GPU" economics favour on-prem owners of already-paid-for GPUs, such as AI labs, inference fleets and government (estimate).
- **Shared-capacity buyer.** The "inference GPUs idle at night" story (Snap, if the podcast is right) suggests a buyer who already owns GPUs and wants a batch SQL engine that runs in off-peak windows. That buyer exists but is large-company (estimate).

### Gaps
- No public Snowflake/Databricks/BigQuery bill complaint thread was found where a team tried GPU, apart from dogman123's Glue/EMR comment on ParaQuery (search budget).
- No direct SemiAnalysis article was read; the figures come via Akash.
- No AI lab (other than via Eventual/Daft customers) was found publicly using GPU SQL for data curation.

## 5. Directions implied that are not on the named list, and candidate list

### Takeaway
The evidence suggests three directions that are not on the named list:
1. Sell or OEM a GPU data engine to **storage and infrastructure vendors** (NetApp/DataPelago, HPE/Theseus, Dell/cuDF).
2. **Air-gapped government and on-prem** GPU analytics (Kinetica, Voltron/Carahsoft).
3. **Experimentation and metrics pipelines** on GPU (Snap).

Every named direction faces NVIDIA's free software or a platform vendor already integrating cuDF.

### Cited Findings
Evidence on named directions, one line each:
- **Spark plugin:** spark-rapids is already the engine behind ParaQuery, Snap and Cloudera. NVIDIA claims ">9x without requiring a change to your queries". — [ParaQuery HN](https://news.ycombinator.com/item?id=43964505); [NVIDIA Snap blog](https://blogs.nvidia.com/blog/snap-accelerated-data-processing/); [NVIDIA Spark ML blog](https://developer.nvidia.com/blog/accelerate-apache-spark-ml-on-nvidia-gpus-with-zero-code-change/) (accessed 2026-10-28)
- **Iceberg/lakehouse GPU accelerator:** DataPelago, the closest example (between the lake and Spark/Trino), was bought by NetApp in July 2026. No Iceberg-specific demand quote was found. — [Techzine](https://www.techzine.eu/news/infrastructure/142971/netapp-acquires-datapelago-for-gpu-accelerated-data-processing/) (accessed 2026-10-28)
- **DataFusion plugin:** the only signal is andygrove's warning that non-JVM Spark accelerators struggle with compatibility. — [HN 43964505](https://news.ycombinator.com/item?id=43964505) (accessed 2026-10-28)
- **DuckDB GPU extension (close to "DataFusion plugin"):** Sirius, backed by NVIDIA, holds the ClickBench record. — [NVIDIA blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/) (accessed 2026-10-28)
- **Real-time ingestion/VRAM cache:** Voltron bought Claypot for real-time and still died. — [VentureBeat](https://venturebeat.com/data-infrastructure/exclusive-voltron-data-acquires-claypot-to-unlock-real-time-ai-with-modular-data-systems); [Pavlo](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html) (accessed 2026-10-28)
- **Databend-like distributed:** Theseus, a distributed GPU query engine, "failed to launch … in a timely manner". — [Pavlo](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html) (accessed 2026-10-28)
- **Vector+SQL:** cuVS is already inside Milvus, Weaviate, pgvector, AlloyDB and Lucene. MyScaleDB (vector SQL) shut down in May 2025. — [NextBigFuture](https://www.nextbigfuture.com/2026/03/nvidia-structured-data-is-the-ground-truth-of-ai-120-billion-structure-data-ecosystem.html); [Pavlo](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html) (accessed 2026-10-28)
- **cuDF for AMD/Cerebras:** the only signal is HN commenters asking about ZLUDA, SCALE or Vulkan for Sirius. No buyer evidence. — [HN mirror](https://astro-hacker-news.pages.dev/46441781) (accessed 2026-10-28)
- **ClickHouse GPU plugin, Flink streaming, VeloDB-like real-time:** no evidence found either way.

### Inferences
Candidate product directions follow. The yardstick is the current engine, about 22.8k non-test lines; new code should be at most 1–2x that. All sizes are estimates.

| slug | one-line | evidence anyone needs it | GPU-centred? | build size vs yardstick | platform risk |
|---|---|---|---|---|---|
| storage-oem-engine | Embeddable single-node GPU SQL/ETL engine sold to storage or AI-data-platform vendors to run "where data resides" | NetApp bought DataPelago for exactly this ([Techzine](https://www.techzine.eu/news/infrastructure/142971/netapp-acquires-datapelago-for-gpu-accelerated-data-processing/)); HPE embedded Theseus ([VMblog](https://vmblog.com/bylines/voltron-data-launches-theseus-to-unlock-the-power-of-the-largest-data-sets-for-ai/)); Dell AI Data Platform uses cuDF ([NextBigFuture](https://www.nextbigfuture.com/2026/03/nvidia-structured-data-is-the-ground-truth-of-ai-120-billion-structure-data-ecosystem.html)) | Yes | About 0.5–1x (estimate): S3/object-store readers, an embedding API, spilling | High: NVIDIA ships free cuDF integrations to the same vendors, and remaining vendors (Pure, VAST, Weka) may build in-house |
| gov-airgapped-analytics | On-prem, air-gapped GPU SQL appliance for agencies and defence on fixed GPU boxes | Kinetica lives on government contracts ([Pavlo](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html)); Voltron sold to two federal agencies at petabyte scale and via Carahsoft ([VentureBeat](https://venturebeat.com/data-infrastructure/exclusive-voltron-data-acquires-claypot-to-unlock-real-time-ai-with-modular-data-systems); [Yahoo](https://finance.yahoo.com/news/voltron-data-carahsoft-partner-bring-160000202.html)) | Yes | About 0.5x engine, plus large non-code work: compliance, sales cycle (estimate) | Low from NVIDIA; high from sales-cycle length versus a 6-month runway |
| ab-metrics-gpu | GPU engine for experimentation and metric pipelines (wide group-bys, joins over event logs) for companies outgrowing warehouse-native experimentation costs | Snap: 10 PB/day, 6,000 metrics, 3-hour SLA, 76% saving ([NVIDIA](https://blogs.nvidia.com/blog/snap-accelerated-data-processing/)) | Yes | About 0.5–1x: metric DSL to SQL, incremental runs (estimate) | High: Snap did it with stock spark-rapids; mid-size buyers are not proven |
| managed-gpu-etl | Hosted drop-in for expensive BigQuery/Glue/EMR batch jobs | dogman123: Glue "cost prohibitive", shuffle jobs fail ([HN](https://news.ycombinator.com/item?id=43964505)); ParaQuery claims a customer saved 60% | Yes | 1–2x+: needs distributed shuffle, so beyond single-node (estimate) | High: ParaQuery, NVIDIA, Cloudera, Databricks |
| idle-gpu-batch | Batch SQL that runs in off-peak windows on GPUs an org already owns (inference or training fleets), scheduled around jobs | Snap reportedly reuses idle inference GPUs ([Podbean](https://www.podbean.com/ew/dir-ytehw-2db39e2c), unverified); clusters run at 30–40% effective use ([Expanse](https://news.ycombinator.com/item?id=48356312)) | Yes | About 0.5x: SLURM/K8s integration, preemption, spilling (estimate) | Medium: no clear incumbent, but demand quotes are indirect |
| ai-data-curation-sql | GPU SQL over Parquet/Lance for training-data filtering, dedup and joins at AI labs | Eventual/Daft ($30M) is used by Essential AI and Together, and says the pattern recurs at labs ([Daft blog](https://www.daft.ai/blog/eventual-raises-30m-to-build-the-future-of-data)); LanceDB raised $30M ([LanceDB](https://lancedb.com/blog/series-a-funding)) | Partly: the work is mostly multimodal and model-bound, not SQL | 1–2x: multimodal types, Lance reader, UDFs (estimate) | High: Daft, LanceDB, NVIDIA NeMo Curator |
| duckdb-gpu-ext | GPU extension for DuckDB | Sirius ClickBench record, 145-point HN thread ([NVIDIA](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/)) | Yes | About 0.5x via Substrait (estimate) | Very high: Sirius is NVIDIA-backed and open source |

- The weakest-covered but most interesting fit for a single-node engine plus a 6-month alpha is **gov-airgapped-analytics** or **idle-gpu-batch**. In both, the buyer already owns GPUs, so 2026 rental price rises don't matter (estimate).

### Gaps
- No named first-customer group was found asking for a *single-node* GPU SQL engine specifically. All paying evidence is either large distributed jobs (Snap, BigQuery-ETL) or infrastructure-vendor acquisitions.
- No evidence was found on ClickHouse GPU, Flink GPU streaming, VeloDB-like real-time, or cuDF for AMD/Cerebras buyers.
