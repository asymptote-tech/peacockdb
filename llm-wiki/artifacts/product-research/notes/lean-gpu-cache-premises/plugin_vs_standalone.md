# GPU SQL delivery model: plugin/extension inside an existing engine vs standalone GPU database

All URLs accessed 2026-10-29. Desk research only, with about 15 tool calls, so several sub-questions are left as gaps (listed at the end of each section). Vendor-published numbers are marked as such.

## Q1. Plugin model: adoption evidence

### Takeaway
Every GPU SQL effort that found production users or got into hyperscaler and vendor distributions in 2024–2026 is an accelerator inside an existing engine: Spark RAPIDS in Spark, cuDF in Velox/Presto at IBM, and Sirius in DuckDB, with ClickHouse and Doris looking at it. But the named production evidence is thin and mostly vendor-published. Sirius is ~2 years old and ~1.1k stars, and is only now being proposed as a DuckDB core extension (PR opened 2026-10-28).

### Cited Findings
**Sirius (DuckDB extension)**
- GitHub repo sirius-db/sirius: created 2024-06-19, 1,101 stars, 145 forks, last push 2026-10-29 (GitHub API) — [GitHub API](https://api.github.com/repos/sirius-db/sirius)
- Releases are unsigned binaries outside DuckDB's signed repo. Users must allow unsigned extensions and match the exact DuckDB version. Linux plus NVIDIA Turing or newer only — [Sirius release notes](https://github.com/sirius-db/sirius/releases/tag/latest)
- DuckDB PR #28022 "Add Sirius as a core extension": opened 2026-10-28, open, 0 comments. It adds "the lightweight Sirius loader to DuckDB's existing core extension group on supported Linux targets", while `sirius_cuda12` and `sirius_cuda13` "will be distributed as community extensions" (related: duckdb/community-extensions#3447) — [duckdb/duckdb#28022](https://github.com/duckdb/duckdb/pull/28022)
- Built by UW-Madison with NVIDIA engineers. CIDR 2026 paper claims 8.3× (TPC-H) and 7.4× (ClickBench) better cost-efficiency as a drop-in DuckDB accelerator, and up to 12.5× speedup inside Apache Doris (author-reported) — [CIDR 2026 paper](https://vldb.org/cidrdb/papers/2026/p12-yogatama.pdf); [arXiv 2508.04701](https://arxiv.org/abs/2508.04701)
- NVIDIA blog: ClickBench record on GH200, "at least 7.2× higher cost-efficiency" than top CPU systems (vendor) — [NVIDIA developer blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/)
- Sirius site lists DuckDB and Doris integration. README says DuckDB supported, StarRocks "coming soon" — [sirius-db.com](https://www.sirius-db.com/); [GitHub](https://github.com/sirius-db/sirius)
- No download counts or named production users found.

**ClickHouse GPU RFCs (both build on Sirius)**
- #116807 "[RFC] ClickHouse GPU v1", opened 2026-08-27, open, 4 comments: "As a first step, we would like to build ClickHouse on top of Sirius." It lists the lossy path QueryPlan → Substrait → DuckDB logical plan → Sirius plan, and notes that PREWHERE, primary-key granule pruning, projections and skip indexes are lost — [ClickHouse#116807](https://github.com/ClickHouse/ClickHouse/issues/116807)
- #124455 "[RFC] GPU Offload of Query Plan Subtrees to Sirius", opened 2026-10-07, open, 0 comments. It calls this "the cheapest path to working GPU execution", with a GPU cache keyed by (part, column, data version) in a separate GPU backend process. It cites Sirius-in-DuckDB as reportedly 38× faster than ClickHouse on TPC-H SF100 — [ClickHouse#124455](https://github.com/ClickHouse/ClickHouse/issues/124455)
- An older ClickHouse "GPU support" issue also exists — [ClickHouse#63392](https://github.com/ClickHouse/ClickHouse/issues/63392)

**Spark RAPIDS / cudf-spark**
- Repo renamed NVIDIA/spark-rapids → NVIDIA/cudf-spark: 1,013 stars, last push 2026-10-29 — [GitHub API](https://api.github.com/repos/NVIDIA/cudf-spark)
- v26.04.2 supports Databricks 17.3 ML LTS, GCP Dataproc 2.1–2.3 and Dataproc Serverless — [cudf-spark download page](https://nvidia.github.io/cudf-spark/docs/download.html)
- Sold and supported through NVIDIA AI Enterprise, whose deployment guide names Amazon EMR among supported platforms (page undated) — [NVIDIA AI Enterprise guide](https://docs.nvidia.com/ai-enterprise/deployment/spark-rapids-accelerator/latest/overview.html)
- AT&T (NVIDIA blog): 2.8 trillion rows (a month of mobile data) processed in 5 hours, "3.3x faster at 60% lower cost" than prior tests. On CPU, 7 days of data took over 48 hours — [NVIDIA blog: AT&T](https://blogs.nvidia.com/blog/att-data-science-rapids)
- Adobe (NVIDIA announcement): one GPU node beat a 16-node CPU cluster by 33% at 70% lower compute cost. The IRS reported "over 20x speed improvements at half the cost" — [NVIDIA newsroom](https://nvidianews.nvidia.com/news/nvidia-accelerates-apache-spark-worlds-leading-data-analytics-platform); [NVIDIA blog: energy efficiency](https://blogs.nvidia.com/blog/spark-rapids-energy-efficiency)
- These are vendor-published, about 2020–2022 era, and mostly trials. Current production use is not confirmed.

**PG-Strom (PostgreSQL extension, HeteroDB)**
- Repo heterodb/pg-strom: created 2014-04-09, 1,408 stars, 176 forks, last push 2026-10-28 — [GitHub API](https://api.github.com/repos/heterodb/pg-strom)
- HeteroDB, Inc. was founded in Shinagawa, Tokyo in 2017. It sells GPU/SSD-accelerated DB software and consulting, founders hold 100% of the stock, and KaiGai Kohei has led PG-Strom since 2012. The software is open source (PostgreSQL License), and target uses are IoT/M2M logs and mobile location data — [ASCII.jp](https://ascii.jp/elem/000/004/061/4061011/); [PG-Strom install docs](https://heterodb.github.io/pg-strom/install)
- No named customers found. HeteroDB looks like a small, founder-owned niche company (inference from ownership and the lack of press).

**Velox-cuDF in IBM watsonx.data Presto**
- At GTC 2026 (March), IBM and NVIDIA announced GPU acceleration of the Presto engine in watsonx.data through Presto C++ + Velox + cuDF. Presto still plans, while scans, filters, joins and aggregations run on GPU, with CPU fallback for unsupported operations — [IBM newsroom 2026-03-16](https://newsroom.ibm.com/2026-03-16-ibm-and-nvidia-announce-expanded-collaboration-at-gtc-2026-to-advance-ai-for-the-enterprise); [NVIDIA cuDF perspectives](https://perspectives.nvidia.com/cudf/accelerating-existing-sql-engines-in-place-for-faster-query/)
- Nestlé Order-to-Cash data mart (186 countries, 44 tables, TBs): refresh went from 15 min on CPU to 3 min, with "83% cost savings" and "30X price-performance" (proof-of-concept figures, vendor-published) — [IBM Think](https://www.ibm.com/think/insights/rethinking-query-economics-for-ai-scale); [IT Jungle 2026-03-23](https://www.itjungle.com/2026/03/23/ibm-taps-nvidia-gpus-for-ai-turbocharged-data-mart/)
- Velox repo: 4,244 stars. NVIDIA/cudf: 9,785 stars — [GitHub API velox](https://api.github.com/repos/facebookincubator/velox); [GitHub API cudf](https://api.github.com/repos/NVIDIA/cudf)

### Inferences
- NVIDIA's own strategy since 2024–25 is "accelerate existing engines in place". It sells cuDF and cudf-spark through AI Enterprise, absorbed HeavyDB staff (see Q2), and backs Sirius. Its product page is literally titled "Accelerating existing SQL engines in place". This is the strongest signal of which model the main GPU vendor thinks wins.
- A plugin inherits the host's distribution, so it never has to get users to migrate. Its adoption, though, is still measured in trials and PoCs rather than disclosed revenue.

### Gaps
- Walmart and ByteDance as Spark RAPIDS production users: not verified in this pass.
- Starburst Trino-cuDF, KX kdb-x GPU, Microsoft Fabric GPU warehouse, Apache Doris production use of Sirius: no sources found (Fabric was not in the NVIDIA docs).
- NVIDIA AI Enterprise revenue attributable to Spark RAPIDS: not public, as far as I found.

## Q2. Standalone model: funding, customers, outcomes

### Takeaway
More than $300M of VC went into standalone GPU databases (MapD/OmniSci/HEAVY.AI ~$92M, Voltron ~$110M, SQream ~$111–135M, Kinetica ≥$63M as of 2017). The outcomes: one shutdown (Voltron, 2025), one quiet sale to NVIDIA (HEAVY.AI, 2025), one closure (BlazingSQL, 2021), and two niche survivors (Kinetica on government work, SQream). None is known to have broken out commercially.

### Cited Findings
**HeavyDB / OmniSci / MapD**
- Founded September 2013 as MapD by Todd Mostak and Thomas Graham. Renamed OmniSci in 2018 and HEAVY.AI in March 2022 — [Dealroom](https://dealroom.co/companies/heavy-ai/); [Wikipedia](https://en.wikipedia.org/wiki/HEAVY.AI)
- $55M Series C in October 2018 led by Tiger Global (In-Q-Tel, NEA, Vanedge, NVIDIA, Verizon Ventures participated), bringing the total to about $92M. Verizon was an investor; I found no source saying it was a customer — [PR Newswire](https://prnewswire.com/news-releases/omnisci-formerly-mapd-secures-55-million-in-new-funding-300723404.html); [Global Venturing](https://globalventuring.com/omnisci-maps-out-55m-series-c)
- January 2025: still launching products (analytics platform on Grace Hopper) — [BusinessWire 2025-01-21](https://www.businesswire.com/news/home/20250121115004/en/HEAVY.AI-Announces-Availability-of-Analytics-Platform-Accelerated-by-NVIDIA-Grace-Hopper-Superchip)
- Acquired by NVIDIA in 2025 (PitchBook M&A entry). Pavlo: "I couldn't find an official announcement of their closing, aside from an M&A firm listing the successful deal", and he calls it a "sort-of acquihire" — [PitchBook](https://pitchbook.com/profiles/company/98344-09); [Pavlo 2025 retrospective](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html)
- Docs now hosted at docs.nvidia.com/heavyai, and HeavyDB is still Apache-2.0 on GitHub. The heavyai/heavydb repo has 3,104 stars and its last push was 2026-10-23, which contradicts DBDB.io's "abandoned, last commit 2025-06-17" — [NVIDIA HeavyDB docs](https://docs.nvidia.com/heavyai/overview); [GitHub API](https://api.github.com/repos/heavyai/heavydb); [DBDB.io](https://dbdb.io/db/heavydb)
- A third-party aggregator says Mostak is now a Senior Director of Engineering for RAPIDS at NVIDIA, working on SQL-on-GPU (unverified) — [getprog.ai](https://www.getprog.ai/profile/953210)
- Ingest model: HeavyDB is its own store (load into HeavyDB tables). A VLDB 2025 paper found HeavyDB ran only 9 of 22 TPC-H queries fully on GPU without CPU fallback, and names PCIe bandwidth and GPU memory capacity as the main limits — [VLDB 2025, Li et al.](https://www.vldb.org/pvldb/vol18/p4518-li.pdf)

**Kinetica (formerly GPUdb)**
- Built at first for US Army INSCOM threat tracking. $50M Series A in June 2017 (Canvas, Meritech, Citi Ventures, Ray Lane), bringing the total to $63M. Customers then included GSK, PG&E, USPS, IronNet and "one of the world's largest retailers" — [Kinetica press release](https://www.kinetica.com/press-releases/kinetica-raises-50-million-series-a/); [VentureBeat](https://venturebeat.com/business/kinetica-raises-50-million-in-series-a-financing)
- Pavlo (Jan 2026): "Kinetica has been milking government contracts for years" — [Pavlo 2025 retrospective](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html)
- A 7wData profile claims Kinetica raised a "$400M Series D led by Dragoneer" and bought Langfuse in January 2026. This is very likely a mix-up with ClickHouse, which (from memory, unverified here) announced exactly that Series D and Langfuse purchase in January 2026. Do not use it — [7wData](https://7wdata.be/company/kinetica/)
- The profile also lists a native SQL-GPT (natural language to SQL) module — [7wData](https://7wdata.be/company/kinetica/)

**SQream**
- $45M Series C announced 2023-09-18, led by World Trade Ventures. Reported totals conflict: past $111M (Blocks & Files), over $118M (AIPressRoom), $135M (VentureBeat). I found no separate 2024 round; the "2024 $45M" in the brief seems to be this September 2023 round — [VentureBeat](https://venturebeat.com/business/sqream-nabs-45m-to-accelerate-data-analytics-with-gpus); [Blocks & Files](https://blocksandfiles.com/2023/09/12/sqream-screeches-to-111m-funding-mark-in-big-gpu-data-processing-play/)
- Named customers: Samsung, LiveAction, Sinch, Orange, AIS, LG — [VentureBeat](https://venturebeat.com/enterprise-analytics/sqream-nabs-45m-to-accelerate-data-analytics-with-gpus)
- Pivot to SQream Blue, a managed "data preparation lakehouse" that queries data in place: "all data remains at the customer's low-cost cloud storage… eliminating the need for data duplication." This moves away from the ingest-into-SQreamDB model — [SQream Blue page](https://sqream.com/product/blue/)
- Pavlo: "Sqream still appears to be kicking it" — [Pavlo 2025 retrospective](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html)

**BlazingSQL**
- Closed-source at first, open-sourced in August 2019. The company closed in 2021 and the team joined Voltron Data — [DBDB.io](https://dbdb.io/db/blazingsql); [BigDATAwire 2021-08-10](https://bigdatawire.com/2021/08/10/voltron-data-takes-flight-to-unify-arrow-community)
- Ex-CEO Rodrigo Aramburu (HN user roaramburu, 2024-04-18): "I was the CEO of BlazingSQL, and we were fully OSS with an open-core model… The number of Fortune 500 customers that were deploying us at scale but not paying us in money, feedback, or testimonials was honestly heartbreaking." — [HN comment 40079813](https://news.ycombinator.com/item?id=40079813)
- Delivery: a Python/RAPIDS SQL engine over files and dataframes (no proprietary store), so ingest was not the barrier. Monetisation was.

**Voltron Data / Theseus**
- Theseus is a distributed GPU query engine introduced in late 2023. It is a composable engine built on Arrow standards, sold to large enterprises (and to the public sector via Carahsoft from April 2025). AMD GPU support was in preview in June 2025 — [Intellyx](https://intellyx.com/2023/12/15/voltron-data-modular-standards-based-data-processing-engine-offering-massive-scale/); [The Register 2025-06-18](https://www.theregister.com/2025/06/18/voltron_amd_sql/); [Carahsoft](https://www.carahsoft.com/news/voltron-data-and-carahsoft-partner-to-bring-cutting-edge-data-infrastructure-solutions-to-the-us-government-2025); [Theseus paper](https://arxiv.org/pdf/2508.05029)
- Accenture invested in 2025 — [Accenture newsroom](https://newsroom.accenture.com/news/2025/accenture-invests-in-voltron-data-to-help-organizations-use-gpu-technology-to-simplify-large-scale-data-processing)
- The Information: switched CEOs and laid off about 50% of ~120 staff after missing revenue targets (date not visible, roughly late 2024 by index age) — [The Information](https://www.theinformation.com/briefings/ai-startup-voltron-data-switches-ceos-lays-off-staff)
- Pavlo (Jan 2026), listed under "Deaths": "the supergroup… $110m in VC money… They built a GPU-accelerated database (Theseus), but failed to launch it in a timely manner." — [Pavlo 2025 retrospective](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html)
- See also Wes McKinney's "Voltron Data Update: Transitions" — [wesmckinney.com](https://wesmckinney.com/blog/voltron-data-transitions/)

### Inferences
- The standalone vendors that survived did so in niches with special buyers (government, telco) and tended to drift toward query-in-place (SQream Blue) or AI features (Kinetica SQL-GPT). Estimate: none has disclosed revenue; HEAVY.AI's ~$92M raise ended in a deal small enough that no announcement was made.
- BlazingSQL shows that removing the ingest barrier (open source, query over files) does not make money by itself. Usage without payment is a separate failure mode from ingest friction.

### Gaps
- HEAVY.AI and Kinetica revenue, and the HEAVY.AI deal price: not found.
- Kinetica's total funding after 2017 and its current state: not found (the aggregator claim is suspect).
- Brytlyt (PostgreSQL-based) current state: no results found.

## Q3. Insider and analyst explanations of success or failure

### Takeaway
The dominant analyst view (Pavlo, January 2026) is that standalone GPU databases have proven commercially unviable and stay niche against CPU DBMSs. Insiders point to non-paying open-source users (BlazingSQL) and slow launch (Voltron). Technical papers point to PCIe bandwidth and GPU memory limits.

### Cited Findings
- Pavlo: "The death of Voltron and sort-of acquihire of HeavyDB seem to continue the trend of the inviability of GPU-accelerated databases… These companies are still niche, and nobody has been able to make a significant dent in the dominance of CPU-powered DBMSs." He also predicted: "you will hear some major GPU-accelerated database announcements by vendors in 2026." — [Pavlo 2025 retrospective](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html)
- 2026 bore that out with announcements from incumbent vendors that put the GPU inside an existing engine: IBM watsonx.data Presto (March), ClickHouse RFCs (August and October), and the DuckDB core-extension PR (October) — sources in Q1.
- BlazingSQL CEO: Fortune 500 firms deployed at scale without paying — [HN 40079813](https://news.ycombinator.com/item?id=40079813)
- Voltron: missed revenue targets and cut 50% of staff — [The Information](https://www.theinformation.com/briefings/ai-startup-voltron-data-switches-ceos-lays-off-staff)
- Technical: PCIe and GPU memory are "the two main limitations", and HeavyDB fell back to CPU on 13 of 22 TPC-H queries — [VLDB 2025](https://www.vldb.org/pvldb/vol18/p4518-li.pdf)

### Inferences
- The causes run together: ingest friction, a narrow workload fit (data must fit in GPU memory or be cached there), the open-source monetisation problem, and competition from fast-improving CPU engines (DuckDB, ClickHouse). A plugin removes only the first, and it rents the host's distribution.

### Gaps
- No Todd Mostak post-mortem interview was found. Pavlo's 2021–2024 retrospectives were not checked for GPU passages.

## Q4. Counter-evidence

### Takeaway
Plugins also stumble. Sirius was slower than CPU DuckDB on desk and workstation GPUs until a cuDF/RMM bug was fixed, and its benchmarks assume data preloaded on the GPU. Standalone CPU engines that require ingest (Snowflake, ClickHouse) became very large, so ingest alone does not explain the standalone GPU failures.

### Cited Findings
- Sirius issue #609 (opened 2026-04-10, closed 2026-06-23, 13 comments): on an NVIDIA GB10 (DGX Spark-class), "for all 22 TPC-H experiments, Sirius was slower than the CPU implementation on DuckDB", and slower still on an RTX 5090 and an A6000 — [sirius#609](https://github.com/sirius-db/sirius/issues/609)
  - On an A100 40GB at SF100, the first result was a 0.83× average speedup (slower). With the official script it reached a 1.45× geometric-mean speedup (18/22 queries faster).
  - The root cause turned out to be a cuDF v26.04 `get_current_device_resource_ref()` bug that bypassed RMM's memory pool, plus a CPU baseline run without warm-up. After the fix, the GPU gain rose to about 10× (as reported by the issue opener's peer), and RMM 26.06 fixed it upstream.
  - Another user: the official results "maybe exclude CPU-GPU data transfer time; all tables should be preloaded and parsed on the GPU beforehand." So the plugin's headline numbers still depend on a GPU-resident data cache.
  - Same source for all sub-points — [sirius#609 comments](https://api.github.com/repos/sirius-db/sirius/issues/609/comments)
- ClickHouse RFC #116807 notes that a plugin via Substrait loses host optimizations (PREWHERE, PK pruning, skip indexes) and takes four lossy plan hops. Integration costs are real — [ClickHouse#116807](https://github.com/ClickHouse/ClickHouse/issues/116807)

### Inferences
- Plugin advantages (no migration, the host's distribution, CPU fallback) come with costs. Dependence on the host's version cadence (Sirius must match the exact DuckDB build), lost host-specific optimizations, and pass-through of upstream library bugs.
- Successful standalone CPU engines won on broad workload fit and cloud distribution, not by avoiding ingest. GPU standalone engines lacked a workload where a 10× gain justified migration. (Inference; no sources gathered here on Snowflake or ClickHouse adoption.)

### Gaps
- Databricks GPU Photon never shipping, and Gluten-GPU stalling: not researched (out of tool budget).
- No Snowflake, ClickHouse or DuckDB adoption figures were gathered in this pass.
