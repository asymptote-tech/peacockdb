# Vertical domains for a small GPU SQL engine (peacockdb)

Scope: which vertical domains sell, request or have tried GPU-accelerated SQL/analytics; named groups and their pain; GPU and CPU incumbents; NVIDIA platform risk; GPU-friendliness of the workload. Desk research only, 2026-10-28. About 40 searches and fetches. Most GPU performance numbers are vendor claims and are marked as such. Several requested names (Datadog Husky, Honeycomb, Axiom internals, Grafana Loki, OneTick, CARTO, GreptimeDB internals) got no specific findings. They are listed under Gaps, not filled from memory.

## Q1. Which named groups have a painful analytics problem a GPU SQL engine could solve? (by vertical)

### Takeaway
The pain is clearest and best documented in three places:
- **Log and observability cost.** Ingest-, retention- and search-priced Datadog and Splunk bills. There is a crop of DataFusion-based log lakes: OpenObserve, Parseable, IceGate.
- **Tick-data research.** kdb+ license cost. As-of joins and EOD sorts are the hot operations, and KX itself now sells GPU acceleration for them.
- **Geospatial spatial joins.** SedonaDB, which is DataFusion-based, already ships GPU spatial joins.

Telecom and adtech pain is real but sits with enterprises that buy through long sales cycles (AT&T, mobile operators via SQream and Kinetica). That is a poor fit for "1–2 users in 6 months". Robotics-log querying is an emerging niche with a DataFusion-based tool (rerun.io) but weak evidence of acute query pain. For HEP, genomics and AIS I found no SQL-shaped GPU pain beyond research prototypes and old vendor demos.

### Cited Findings

**Observability / log analytics**
- Datadog cost structure: per-GB log ingestion, separate retention charges and log-search fees on top of per-host pricing. Teams start at 15-day retention and extend it after incidents or compliance requests, "each extension multiplies the cost of the entire log volume" (vendor-adjacent source, a competitor's blog) — [OneUptime, "The Real Cost of Datadog 2026"](https://oneuptime.com/blog/post/2026-03-18-the-real-cost-of-datadog/markdown) (accessed 2026-10-28)
- Splunk's volume-based pricing gets costly at large log volumes. Common mitigations are filtering or sampling before ingest and archiving to cheap storage — [Uptrace Datadog vs Splunk](https://uptrace.dev/blog/datadog-vs-splunk.html); [Mezmo on Datadog log cost](https://www.mezmo.com/resources/optimizing-log-management-in-datadog-cut-costs-without-losing-insights) (accessed 2026-10-28)
- Hydrolix exists specifically to cut log-storage cost. It is a "streaming data lake", founded in 2018, claiming 20–50× compression (vendor claim) — [TechCrunch 2024-05-22](https://techcrunch.com/2024/05/22/hydrolix-seeks-to-make-storing-log-data-faster-and-cheaper) (accessed 2026-10-28)
- Character.AI monitors thousands of GPUs with ClickHouse/ClickStack. It reports 10× log growth, a 50% cost reduction and log search going from minutes to seconds, all on CPU ClickHouse — [ClickHouse blog](https://clickhouse.com/blog/scaling-observabilty-for-thousands-of-gpus-at-character-ai) (accessed 2026-10-28)
- The DataFusion "Known Users" list names these observability projects: **OpenObserve** (distributed observability platform), **Parseable** (columnar observability data lake for logs/metrics/traces), **IceGate** (Iceberg observability lake with OTel ingest) and **Telemetry** (structured logging) — [DataFusion known users](https://datafusion.apache.org/user-guide/introduction.html) (accessed 2026-10-28)
- A 2025 survey of log analysis says regex processing is a major cost in log analysis — [arXiv 2510.10348](https://arxiv.org/pdf/2510.10348.pdf) (accessed 2026-10-28)
- A Splunk community answer to "does Splunk support CUDA" says such searches would be I/O-limited anyway — [Splunk community](https://community.splunk.com/t5/Splunk-Search/Does-Splunk-support-CUDA/m-p/90446) (accessed 2026-10-28)

**Security analytics / SIEM / network flow**
- Zeek is positioned as a middle ground between NetFlow and full PCAP on fidelity versus volume. Hunters pivot from Zeek log events into PCAP by timestamp — [Zeek "MUC 2025 State of Zeek"](https://zeek.org/wp-content/uploads/2025/03/MUC-2025-State-of-Zeek.pdf); [APNIC threat hunting with Zeek](https://academy.apnic.net/wp-content/uploads/2022/10/APNIC-Threat-Hunting-with-Zeek.pdf) (accessed 2026-10-28)
- No public source found of GPU (cuDF) analytics over Zeek, NetFlow or PCAP. The search came back empty (see Gaps).
- RAPIDS showed cuDF/dask-cuDF parsing of Windows Event Logs (2019). cyBERT 2.0 later moved to transformer-based parsing: >1,000 raw logs/s on five V100s — [RAPIDS cyber log parsing](https://medium.com/rapids-ai/gpu-accelerated-cyber-log-parsing-with-rapids-10896f57eee9); [cyBERT 2.0](https://medium.com/rapids-ai/cybert-2-0-streaming-gpu-log-parsing-with-rapids-c94d94ae614) (accessed 2026-10-28)

**Financial tick data / quant**
- kdb+ is "commercial and pricey", with production deployments at scale "into six figures per year". ClickHouse, QuestDB, TimescaleDB and Parquet stacks are the cheaper picks, and ClickHouse is said to lack a native as-of join (secondary guide) — [techinterview.org TSDBs for quant](https://www.techinterview.org/post/3233474600/time-series-databases-quant/) (accessed 2026-10-28)
- Older evidence of the same cost pain: a TrustRadius reviewer found the license cost "a little high for some small institutions" — [TrustRadius kdb+ review](https://trustradius.com/reviews/kdb-2016-01-08-08-37-54) (accessed 2026-10-28)
- KX identified as-of joins and sorts as the operations worth putting on GPU. EOD sorts of hundreds of millions of rows went "from minutes to seconds" on A100 (vendor) — [KX blog: GPU acceleration in KDB-X](https://kx.com/?p=14992) (accessed 2026-10-28)
- Massive.com, a stock-market API, is a DataFusion user — [DataFusion known users](https://datafusion.apache.org/user-guide/introduction.html) (accessed 2026-10-28)
- No public evidence found of hedge funds using the Polars GPU engine for backtesting. The only GPU backtesting example found is NVIDIA's old gQuant cuDF demo — [ODSC gQuant](https://opendatascience.com/gquant-gpu-accelerated-examples-for-quantitative-analyst-tasks/) (accessed 2026-10-28)

**Geospatial / mobility / AIS**
- AIS volumes are "10s of billions of raw messages per year". Because of infrastructure limits, AIS data "must currently be processed in small temporal or spatial subsets" (Axiom Data Science, 2018) — [axds.co AIS big data](https://axds.co/2018/02/16/AIS-big-data-OSM/) (accessed 2026-10-28)
- HEAVY.AI (OmniSci) demo: 11.6B rows of US AIS data on 4 servers × 8 GTX 1080 Ti. Spire used OmniSci for billions of historical vessel positions in anomaly detection (vendor, about 2018–2020) — [HEAVY.AI AIS blog](https://www.heavy.ai/blog/an-interactive-deep-dive-into-11-6b-rows-of-us-shipping-data); [Spire MDA](https://spire.com/?p=6289) (accessed 2026-10-28)
- Wherobots (the commercial Apache Sedona company) sells spatial joins and zonal statistics. A Texas statewide zonal-stats job finished in 3m28s while BigQuery timed out (vendor, hardware not stated) — [Wherobots raster joins](https://wherobots.com/raster-spatial-joins-at-scale-google-earth-engine-bigquery-vs-apache-sedona-wherobots/) (accessed 2026-10-28)

**Telecom / adtech**
- AT&T: Spark RAPIDS processed a month of mobile data (2.8 trillion rows) in about 5 hours, "3.3x faster at 60 percent lower cost than any prior test". AT&T ran ETL on CPU and training on GPU and wanted a single GPU pipeline (NVIDIA blog) — [NVIDIA AT&T blog](https://blogs.nvidia.com/blog/att-data-science-rapids) (accessed 2026-10-28)
- Adobe Experience Cloud: 7× speedup and 90% cost saving in an initial Spark 3.0 GPU test (2020 press release) — [NVIDIA/Nasdaq PR](https://www.nasdaq.com/press-release/nvidia-accelerates-apache-spark-worlds-leading-data-analytics-platform-2020-05-14). PayPal claims up to 70% cloud-cost reduction with Spark RAPIDS — [GTC24 S62506](https://www.nvidia.cn/on-demand/session/gtc24-s62506) (accessed 2026-10-28)
- SQream names these mobile-operator customers: AIS, NCBA, Sinch, Mobicom, LG U+, SK Telecom (MWC 2024). Other disclosed customers: Orange, Samsung, LG, Alibaba, PubMatic (adtech). Use cases: customer mobility, device detection, location, ad optimization, network monitoring, security logs — [SQream MWC 2024 PR](https://prnewswire.com/il/news-releases/sqream-showcases-pioneering-big-data-analytics-solutions-for-the-telecom-industry-at-mwc-barcelona-2024-302065969.html) (accessed 2026-10-28)

**Robotics / edge**
- ROS 2 (from Iron) records `.mcap` by default — [Foxglove blog](https://foxglove.dev/blog/mcap-as-the-ros2-default-bag-format) (accessed 2026-10-28)
- rerun.io "visualizes and queries robotics logs and turns them into training data" on DataFusion. Catalog queries return DataFusion DataFrames, and the commercial Rerun Hub is a catalog/storage engine on object storage — [DataFusion known users](https://datafusion.apache.org/user-guide/introduction.html); [Rerun getting started](https://docs.rerun.io/dev/getting-started/) (accessed 2026-10-28)
- Bagel, an MCP server, runs DuckDB SQL over ROS bag messages for LLM-driven querying. This is weak evidence that people want SQL over robot logs — [glama.ai Bagel](https://glama.ai/mcp/servers/jjxhn2rcsw) (accessed 2026-10-28)
- No source found of a fleet-scale SQL analytics layer over MCAP (see Gaps).

**Genomics / HEP**
- Genomics GPU money goes to Parabricks (alignment and variant calling): up to 65× on germline callers. Some somatic callers cost more on GPU in the cloud — [PMC10230726](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC10230726/) (accessed 2026-10-28)
- SQream claims to speed up Hail's data-loading and prep steps. It is a vendor post with no numbers — [SQream Hail blog](https://sqream.com/blog/sqream-accelerates-hail-workflows-in-genomics-research/) (accessed 2026-10-28)
- HEP's columnar analysis (coffea on Awkward Array) is going GPU via CUDA/CCCL kernels for irregular, jagged arrays. It is not SQL — [arXiv 2607.28145](https://arxiv.org/pdf/2607.28145); [SciPy 2026 talk](https://pretalx.com/scipy-2026/speaker/ECE7HR/) (accessed 2026-10-28)

### Inferences
- The strongest "named group, painful, today" signal reachable by a 1–3 person team is **operators of self-hosted DataFusion-based log/observability lakes** (OpenObserve, Parseable, IceGate users) and the projects themselves. They already store logs as Parquet and run SQL through DataFusion, which is the same frontend peacockdb uses. The pain is that search and aggregation over retained logs are expensive. (estimate)
- The second strongest is **small quant teams or prop shops that cannot justify kdb+** and keep tick data in Parquet. Their hot loop is as-of join plus sort plus group-by on (sym, time). KX's own choice of GPU operators confirms that this loop is GPU-shaped. (estimate)
- Telecom, adtech and Fortune-500 cyber buy GPU analytics through enterprise sales (SQream, Kinetica, Spark RAPIDS with NVIDIA). They will not pick an alpha from a 1-person team within 6 months. (estimate)

### Gaps
- No first-hand Reddit or HN complaint threads were retrieved (r/sysadmin, r/quant, r/gis). The search tool surfaced vendor pages instead. A follow-up should mine those subreddits directly for named users.
- Not researched for lack of hits: Datadog Husky, Honeycomb, Axiom, Grafana Loki, SigNoz, OneTick, Deephaven internals, CARTO, DuckDB spatial, telematics/automotive fleet, gaming/product analytics, e-commerce, astronomy.
- No job posts were found that ask for GPU SQL in any vertical.

## Q2. Who are the GPU incumbents per vertical, what is their status, and what is NVIDIA's platform risk?

### Takeaway
Standalone GPU databases have mostly been absorbed or have stalled:
- HEAVY.AI was acquired by NVIDIA in 2025. Reportedly its engine is to be re-open-sourced by NVIDIA.
- Voltron Data had 50% layoffs reported in 2024, and its status is unclear.
- cuSpatial publication stopped in RAPIDS 25.06.
- Morpheus containers reached end of support in January 2026.

NVIDIA's own strategy is now to accelerate existing engines in place: Spark RAPIDS, Presto/Velox-cuDF, Sirius as an official DuckDB extension, and cudf-polars. That is the main platform risk for any general-purpose "GPU SQL on Parquet" product. Inside verticals, the live GPU players are KX (KDB-X GPU, GA April 2026, finance), Kinetica (defense, telecom, utilities), SQream (telecom, adtech) and SedonaDB/RayBooster (geo, DataFusion-based). Observability and SIEM have **no** GPU incumbent: Splunk and Elastic GPU work is monitoring or vector search only.

### Cited Findings

**Cross-vertical / NVIDIA**
- HEAVY.AI (MapD → OmniSci → HeavyDB) was acquired by NVIDIA in 2025 — [Wikipedia HEAVY.AI](https://en.wikipedia.org/wiki/HEAVY.AI) (accessed 2026-10-28). DBDB says the engine was abandoned after the deal — [dbdb.io HeavyDB](https://dbdb.io/db/heavydb/revisions/9) (accessed 2026-10-28). That conflicts with a GTC 2026 talk summary saying Todd Mostak leads it at NVIDIA and it "will be open-sourced in late Q2 2026" — [GTC 2026 summary blog](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28). HeavyAI docs now live at docs.nvidia.com/heavyai under Apache-2.0 — [NVIDIA HeavyAI docs](https://docs.nvidia.com/heavyai/tutorials/getting-started-open-source.md) (accessed 2026-10-28). The conflict is unresolved, and the GTC summary is a third-party write-up.
- Sirius: GPU execution for DuckDB on cuDF via Substrait, from UW-Madison and NVIDIA. Results cited: #1 ClickBench hot run on GH200, ~5× DuckDB on TPC-H SF1K on a GB300 DGX Station, 9× TCO against an r8i.8xlarge. It was announced as an official DuckDB core extension (per the GTC summary) — [NVIDIA blog on the Sirius ClickBench record](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/); [GitHub sirius-db/sirius](https://github.com/sirius-db/sirius); [GTC 2026 summary](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28)
- Presto GPU on Velox+cuDF: a 4× B200 node is ~30× faster than an 8-node Grace CPU cluster on TPC-H SF1K. Parquet scan is 60–70% of runtime. Nestlé cited 5× speedup and 83% cost reduction — [GTC 2026 summary](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html); [NVIDIA perspectives cuDF](https://perspectives.nvidia.com/cudf/accelerating-existing-sql-engines-in-place-for-faster-query/) (accessed 2026-10-28)
- cudf-polars: `collect(engine="gpu")` with transparent CPU fallback. Vendor claim of up to 13× (PDS-H) or 23× on DGX B200 — [RAPIDS docs cudf_polars](https://docs.rapids.ai/api/cudf/latest/cudf_polars); [NVIDIA perspectives](https://perspectives.nvidia.com/cudf/faq/we-need-to-speed-up-query-execution-across-our-data-platform-but-migrating-users-to-a-new-engine-isn-78332e/) (accessed 2026-10-28)
- Voltron Data (Theseus): AMD Instinct support was announced in June 2025 — [The Register 2025-06-18](https://www.theregister.com/2025/06/18/voltron_amd_sql/). An anonymous Blind post (2024-10-09) reports a 50% layoff — [Blind](https://www.teamblind.com/post/what-happened-to-voltron-data-bxtoxtrf). A startup directory lists the company as "Inactive" (unverified) — [startuphub.ai](https://www.startuphub.ai/startups/voltron-data) (accessed 2026-10-28)

**Security**
- NVIDIA AI Enterprise lists Morpheus container images as End of Support, January 2026. Production branch PB24h1 reached EOL in January 2025 and PB24h2 in July 2025 — [NVIDIA AI Enterprise EOL notices](https://docs.nvidia.com/ai-enterprise/lifecycle/latest/eol-notices.html); [EOL branches](https://docs.nvidia.com/ai-enterprise/lifecycle/latest/eol-branches.html) (accessed 2026-10-28)
- In February 2025, NVIDIA still pitched Morpheus with BlueField-3 DPUs for network-traffic analysis — [SiliconANGLE](https://siliconangle.com/?p=686335) (accessed 2026-10-28)
- Kinetica listed IronNet Cybersecurity and "counterterrorism and cybersecurity (multiple customers)" (old Series A release) — [Kinetica archive PR](https://archive.kinetica.com/press-releases/kinetica-raises-50-million-series-a/) (accessed 2026-10-28)

**Finance**
- KX KDB-X reached GA on 2026-04-02 as "time-series, vector and GPU compute in one runtime". KX claims 10–25× on joins, aggregations and scoring. Jensen Huang called it out at GTC 2026 — [KX press](https://kx.com/news-room/kx-delivers-gpu-accelerated-advantage-for-real-time-ai-in-capital-markets/); [BusinessWire 2026-04-02](https://www.businesswire.com/news/home/20260402375288/en/) (accessed 2026-10-28)
- KX technical detail (pre-GA blog): `.gpu.aj` as-of join, `.gpu.xasc`/`.gpu.iasc` sorts, built on CUDA and cuDF, with GPUDirect Storage loading of splayed tables. Vendor numbers:
  - as-of join, 1M rows: ~4× on L4 vs 48 CPU cores
  - as-of join, 10M rows: ~5.5× on A100
  - as-of join, 100M rows: <1 s on GPU vs >4 s on CPU
  - sort of 10M rows: ~8×
  - 5-day VaR on 5× H100: 58×

  Multi-GPU and broader types were still pending at the time — [KX blog](https://kx.com/?p=14992) (accessed 2026-10-28)
- STAC-M3: all published kdb+ results found are CPU, flash or cloud-memory systems. The latest is kdb+ 4.1 on Dell with PowerScale F710 (2026-03-13). No GPU STAC-M3 found — [STAC KX reports](https://docs.stacresearch.com/node/35566/reports); [StorageNewsletter Supermicro/Intel/Micron](https://www.storagenewsletter.com/?p=296956) (accessed 2026-10-28)
- Deephaven has `aj`/`raj` as-of joins. No GPU path found — [Deephaven cheat sheet](https://deephaven.io/enterprise/bard/docs/cheat-sheets/ultimate-cheat-sheet/) (accessed 2026-10-28)

**Geospatial**
- RAPIDS stopped publishing cuSpatial with v25.06. Development is paused, and it was removed from the metapackage and containers — [RAPIDS RSN 45](https://docs.rapids.ai/notices/rsn0045/) (accessed 2026-10-28)
- SedonaDB 0.4 (June 2026) added GPU spatial joins via RayBooster. It runs DE-9IM predicates on RT cores, with a single BVH and a memory-aware spilling scheduler. Vendor numbers:
  - up to 5.93× on heavy joins
  - 59% lower AWS cost
  - Q11: 7.51 s → 1.61 s on an RTX 3090
  - the RTX 3090 beat an H100 on Q10

  Paper accepted at VLDB 2026 (Industry). Built by Ohio State with the Sedona PMC — [Sedona blog](https://sedona.apache.org/1.9.1/blog/2026/06/26/sedonadb-04-gpu-accelerated-spatial-joins/) (accessed 2026-10-28). SedonaDB is listed as a DataFusion user — [DataFusion known users](https://datafusion.apache.org/user-guide/introduction.html) (accessed 2026-10-28)
- Kinetica's site still sells a "GPU + CPU native" distributed database. Verticals: defense and intelligence, finance, energy and utilities, telecom, logistics, insurance. Logos: Citi, Verizon, USPS, FAA, Enverus. It now leans into AI agents and vector search — [kinetica.com](https://kinetica.com/) (accessed 2026-10-28)

**Telecom / adtech**
- SQream sells SQreamDB and SQream Blue (a GPU lakehouse) with telecom as its lead vertical (see Q1). No 2026 deals found — [SQream MWC 2024](https://prnewswire.com/il/news-releases/sqream-showcases-pioneering-big-data-analytics-solutions-for-the-telecom-industry-at-mwc-barcelona-2024-302065969.html) (accessed 2026-10-28)
- Kinetica launched SQL-GPT for Telecom in December 2023 — [multiplatform.ai](https://multiplatform.ai/?p=27150) (accessed 2026-10-28)

**Observability / time-series (CPU incumbents, no GPU)**
- Splunk and Elastic have no GPU-accelerated log search. Elastic's GPU work is cuVS vector indexing. Splunk's GPU material is GPU *monitoring* — [Elastic + NVIDIA cuVS](https://opsmatters.com/events/elastic-accelerate-vector-search-enterprise-scale-ai-elastic-and-nvidia); [Splunk community blog](https://community.splunk.com/t5/Community-Blog/From-GPU-to-Application-Monitoring-Cisco-AI-Infrastructure-with/ba-p/757732) (accessed 2026-10-28). Elastic's event page markets "GPU-accelerated observability, 12× faster indexing", which is marketing with no detail — [Elastic event](https://events.elastic.co/observeatthespeedofaielasticnvidiaobservabilityworkshop) (accessed 2026-10-28)
- No GPU acceleration found in InfluxDB 3, QuestDB or GreptimeDB. All published TSBS-style benchmarks are CPU — [QuestDB vs InfluxDB3 Core](https://questdb.com/blog/influxdb3-core-benchmarks/) (accessed 2026-10-28). InfluxDB, GreptimeDB, HoraeDB and CnosDB are all DataFusion users — [DataFusion known users](https://datafusion.apache.org/user-guide/introduction.html) (accessed 2026-10-28)

### Inferences
- **Highest platform risk:** horizontal "GPU SQL over Parquet on a workstation or DGX". Sirius plus DuckDB is NVIDIA-backed, free, has the #1 ClickBench result and is an official DuckDB extension. cudf-polars covers the dataframe users. A peacockdb pitched there competes head-on with NVIDIA. (estimate)
- **Lowest NVIDIA risk:** observability log search and SIEM/threat-hunting SQL. NVIDIA's one security product (Morpheus) reached end of support in January 2026, and no vendor ships GPU log search. Absence of incumbents can also mean absence of demand, though: the Splunk community's "I/O-limited anyway" view is a real objection. (estimate)
- In finance, KX now owns "GPU as-of join" in the high end. The open niche is below KX: teams on Parquet, Polars or ClickHouse that lack kdb+. (estimate)
- In geo, SedonaDB/RayBooster already does GPU spatial joins on DataFusion, the same stack, as open-source Apache. Competing there would mean contributing to it, not building beside it. cuSpatial's death removes the library peacockdb would have used. (estimate)

### Gaps
- Whether NVIDIA actually open-sourced HeavyDB in Q2 2026 is unconfirmed: the sources conflict, and no primary NVIDIA announcement was found.
- Voltron Data's 2026 status is unconfirmed (no primary source).
- The Morpheus GitHub repository's archive status was not checked directly.
- Kinetica's and SQream's 2025–2026 customer momentum: no recent deal news found.

## Q3. Is each vertical's workload GPU-friendly, or I/O-bound?

### Takeaway
The GPU wins where the working set is reused in memory and dominated by joins, sorts, group-bys or complex predicates: as-of joins and EOD sorts, spatial joins, multi-pass threat hunts over a hot window. It loses where each query is a single cold scan over object storage or disk (needle-in-haystack log search). Even NVIDIA's own Presto numbers show Parquet scan at 60–70% of GPU runtime. Regex and string matching on GPU works in libcudf but gives modest gains on real data.

### Cited Findings
- On a PCIe coprocessor setup, the PCIe link sets the runtime floor and a good CPU can win. Earlier GPU speedups often used weak CPU baselines — [Shanbhag et al., arXiv 2003.01178](https://arxiv.org/pdf/2003.01178) (accessed 2026-10-28)
- Remaining bottlenecks are memory capacity, data movement, GPU cost and engineering effort. GPU SQL engines are "not yet mainstream" and earlier efforts stayed niche. PCIe Gen6 (128 GB/s) and NVLink-C2C (900 GB/s) are easing this — [Sirius paper, arXiv 2508.04701](https://arxiv.org/pdf/2508.04701) (accessed 2026-10-28)
- In Presto-GPU on B200, Parquet scan is 60–70% of runtime. AWS has a per-VM S3 bandwidth ceiling — [GTC 2026 summary](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28)
- libcudf has `contains_re`, `count_re` and `like` on a `regex_program` — [libcudf contains.hpp](https://docs.rapids.ai/api/libcudf/stable/strings_2contains_8hpp) (accessed 2026-10-28). HetMatch (ADMS 2026) improves on cuDF pattern matching by routing strings by length (thread-per-string below 128 B, warp-per-string above). It reports a geometric-mean speedup over cuDF of 2.5× on synthetic data and only 1.3× on 11 real datasets, which suggests stock cuDF string matching leaves performance on the table — [ADMS26 HetMatch](https://www.vldb.org/2026/Workshops/VLDB-Workshops-2026/ADMS/ADMS26_8.pdf) (accessed 2026-10-28)
- Older GPU regex research: HybridSA (OOPSLA 2024) splits multi-pattern regex between GPU and CPU — [par.nsf.gov](https://par.nsf.gov/servlets/purl/10618058). GLoP reported 20 Gbps GPU log processing (2017) — [ar5iv 1704.02278](https://ar5iv.labs.arxiv.org/html/1704.02278) (accessed 2026-10-28)
- KX's GPU gains are on as-of join and sort. GPUDirect Storage loads tables straight into GPU memory, and the CPU keeps orchestration — [KX blog](https://kx.com/?p=14992) (accessed 2026-10-28)
- Spatial join is compute-bound enough that a $1,500 RTX 3090 with RT cores beat an H100 on one query — [Sedona blog](https://sedona.apache.org/1.9.1/blog/2026/06/26/sedonadb-04-gpu-accelerated-spatial-joins/) (accessed 2026-10-28)
- Hydrolix-style log lakes win on 20–50× compression. Cost there is dominated by storage and scan volume, not compute — [TechCrunch](https://techcrunch.com/2024/05/22/hydrolix-seeks-to-make-storing-log-data-faster-and-cheaper) (accessed 2026-10-28)

### Inferences
- **Observability:** GPU-friendly only for *analytic* log queries: group-by over time buckets, top-k, high-cardinality distinct, regex extraction over a recent hot window kept in GPU or host memory. Needle search over months of cold logs is I/O-bound, and inverted indexes or bloom filters beat brute force. A GPU product here should pin a hot window (hours to days) and do heavy aggregation and regex on it. (estimate)
- **Tick data:** strongly GPU-friendly. Sorted (sym, time) data, as-of joins, window aggregates, and repeated research queries over the same days. (estimate)
- **SIEM/threat hunting:** GPU-friendly when hunts join flows to indicator lists, do IP/CIDR range joins, beaconing detection (group-by plus time-delta stats) or high-cardinality distinct counts over days of NetFlow/Zeek held in memory. (estimate)
- **Telecom CDR/xDR:** large joins and aggregates, GPU-friendly per AT&T and SQream. The buyer is the problem, not the workload. (estimate)
- **Robotics/MCAP:** dominated by decode and extraction of nested or binary messages. Mostly I/O- and parse-bound, so the GPU advantage is unclear. (estimate)
- **HEP/genomics:** jagged arrays and bespoke kernels, not SQL-shaped. Poor fit. (estimate)

### Gaps
- No independent benchmark found of GPU against CPU (ClickHouse, DataFusion) on real observability log queries, and none of cuDF regex over typical log lines against Hyperscan or RE2 on CPU.
- No public data found on hot-window sizes in OpenObserve or Parseable deployments, which would settle whether the data fits in GPU or host memory.

## Q4. Candidate product directions

### Takeaway
Two directions combine a DataFusion-native named community, documented pain, GPU-shaped compute and no NVIDIA incumbent: a GPU executor for DataFusion log/observability lakes, and a Parquet tick-data research engine built around GPU as-of join. A third, network-flow threat hunting, is GPU-shaped with an NVIDIA vacuum (Morpheus end of support), but the evidence of named users is weaker. Geo and horizontal desktop GPU SQL are crowded by SedonaDB and Sirius.

### Cited Findings
(See Q1–Q3 for every source. The table below cites the key link per row.)

Yardstick = 22,830 non-test lines. Budget is at most ~45k new lines.

| slug | one-line | evidence anyone needs it (named group, pain, link) | GPU-centred? | rough build size vs yardstick (estimate) | platform risk |
|---|---|---|---|---|---|
| `datafusion-log-gpu` | Drop-in GPU execution for DataFusion-based log lakes (OpenObserve, Parseable, IceGate): regex/LIKE filters, time-bucket group-by, top-k and distinct over a pinned hot window of Parquet logs | Named: OpenObserve, Parseable, IceGate are DataFusion users ([DF known users](https://datafusion.apache.org/user-guide/introduction.html)). Pain: Datadog/Splunk ingest, retention and search pricing ([OneUptime](https://oneuptime.com/blog/post/2026-03-18-the-real-cost-of-datadog/markdown)). Regex is a major log-analysis cost ([arXiv 2510.10348](https://arxiv.org/pdf/2510.10348.pdf)). Pain is documented, but a user asking for GPU specifically is not. | Yes for aggregation, regex and high-cardinality work on the hot window. No for cold needle search (I/O-bound, [Splunk community](https://community.splunk.com/t5/Splunk-Search/Does-Splunk-support-CUDA/m-p/90446)) | ~0.5–1.0× (11–23k): DataFusion plan bridge exists; add string/regex functions, time bucketing, hot-window cache, object-store Parquet ingest (estimate) | Low from NVIDIA (no GPU log product; Elastic GPU is vector only). Medium from ClickHouse/CPU "good enough" |
| `tick-asof-gpu` | Parquet tick-data research engine: GPU as-of join, sort, windowed aggregation and OHLC bars for quant teams without kdb+ | Named: small quant/prop teams priced out of kdb+ ("six figures per year", [techinterview](https://www.techinterview.org/post/3233474600/time-series-databases-quant/)). KX's choice of `.gpu.aj` and sort shows the hot loop ([KX blog](https://kx.com/?p=14992)). Massive.com is a DF user. No named team found that asks for this. | Yes. As-of join and sort are KX's 4–10× ops; repeated in-memory research queries | ~0.4–0.8× (9–18k): ASOF JOIN SQL syntax (DataFusion 45 lacks it; parser and planner work), GPU as-of kernel or cuDF path, time-series functions (estimate) | Medium: KX KDB-X GPU GA 2026-04 at the high end; cudf-polars for Python quants |
| `flow-hunt-gpu` | SQL threat hunting over Zeek/NetFlow/firewall logs in Parquet: IP/CIDR range joins, IOC joins, beaconing stats, distinct counts | Named only loosely: Zeek users and threat hunters ([Zeek 2025](https://zeek.org/wp-content/uploads/2025/03/MUC-2025-State-of-Zeek.pdf)). NVIDIA Morpheus containers reached end of support in January 2026 ([NVIDIA EOL](https://docs.nvidia.com/ai-enterprise/lifecycle/latest/eol-notices.html)). No public cuDF-over-Zeek user found. | Yes: joins, range joins, group-bys over in-memory days | ~0.6–1.2× (14–27k): IP/CIDR types and functions, Zeek TSV/JSON ingest to Parquet, range-join operator (estimate) | Low now that Morpheus has gone; medium from SIEM vendors' CPU lakes |
| `geo-join-gpu` | GPU spatial joins and point-in-polygon for AIS/telematics on Parquet | Named: AIS analysts forced to subset data ([axds](https://axds.co/2018/02/16/AIS-big-data-OSM/)); Wherobots/Sedona users | Yes (compute-bound) | ~1.5–2.0× (34–45k): geometry types, WKB, predicates, spatial index; cuSpatial dead ([RSN 45](https://docs.rapids.ai/notices/rsn0045/)) (estimate) | High: SedonaDB (DataFusion) ships GPU joins as of 0.4 ([Sedona](https://sedona.apache.org/1.9.1/blog/2026/06/26/sedonadb-04-gpu-accelerated-spatial-joins/)); HeavyDB possibly open-sourced by NVIDIA |
| `desk-gpu-sql` | Local GPU SQL over Parquet on DGX Spark, a workstation or a single GPU box | Generic; no named group | Yes | ~0.3× (polish) (estimate) | Very high: Sirius as an official DuckDB extension, cudf-polars ([GTC 2026 summary](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html)) |
| `telco-xdr-gpu` | GPU SQL over CDR/xDR and network probe data | AT&T 2.8T rows a month ([NVIDIA](https://blogs.nvidia.com/blog/att-data-science-rapids)); SQream operator customers | Yes | ~1.0× plus heavy enterprise integration (estimate) | High: SQream, Kinetica, Spark RAPIDS. Buyers are enterprises with long cycles |
| `robot-log-gpu` | GPU SQL over MCAP/ROS 2 fleet logs (for example as a backend to rerun-style DataFusion catalogs) | Weak: rerun.io on DataFusion ([Rerun docs](https://docs.rerun.io/dev/getting-started/)); Bagel DuckDB-over-bags ([glama](https://glama.ai/mcp/servers/jjxhn2rcsw)). No stated pain. | Unclear; parse/IO-bound (estimate) | ~0.8–1.2× (MCAP decode, nested types) (estimate) | Medium: NVIDIA Isaac/Holoscan adjacent (not researched) |

### Inferences
- Ranking by "named group plus pain plus GPU-central plus fits budget plus low NVIDIA risk": 1) `datafusion-log-gpu`, 2) `tick-asof-gpu`, 3) `flow-hunt-gpu`. The rest are crowded or lack users. (estimate)
- `datafusion-log-gpu` and `flow-hunt-gpu` share most of their engine work: string/regex functions, time bucketing and a hot-window cache. One build could serve both with different ingest. (estimate)
- Both top candidates still lack a named person who said "I need this on GPU". The next step is direct outreach: OpenObserve and Parseable maintainers and heavy users, and r/quant or quant Discords for Parquet-tick users. (estimate)

### Gaps
- No first-person demand evidence (forum post, issue or job post) for GPU in any top candidate. GitHub issues of OpenObserve, Parseable and GreptimeDB were not searched for "GPU".
- DataFusion 45's ASOF JOIN support was not checked.
- NVIDIA Holoscan, cuOpt and Isaac as platform risk for robotics/edge was not researched.
