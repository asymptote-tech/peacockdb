# NVIDIA's stance: building a database or SQL engine itself vs. enabling others

All URLs accessed 2026-10-28. "(estimate)" marks guesses. Under "Inferences" the conclusions are mine, not what the sources say.

## 1. Public statements (GTC 2024–2026, blogs, exec keynotes)

### Takeaway
NVIDIA's official message is "we accelerate every data platform". cuDF/cuVS are libraries that partner engines embed. I found no public statement where NVIDIA says it will not ship a database. No source has NVIDIA saying "we are not a database company" either. NVIDIA's own GTC 2026 sessions and staff do present Sirius as an NVIDIA engine, and NVIDIA now owns and runs HeavyDB.

### Cited Findings
- GTC 2026 keynote (2026-03-16). Huang named data-processing partners: IBM, Dell, Google Cloud, AWS, Microsoft Azure, Oracle and CoreWeave. He described how data processing "is being accelerated for the era of AI" for those partners' customers — [NVIDIA blog, GTC 2026 live updates](https://blogs.nvidia.com/blog/gtc-2026-news/)
- The same NVIDIA blog has a section titled "NVIDIA cuDF and cuVS Adopted by World's Leading Data Platforms". It calls cuDF/cuVS "accelerated data libraries built on NVIDIA CUDA-X". It says "NVIDIA cuDF accelerates open source data processing engines such as Apache Spark, Presto, DuckDB, Polars and Velox", claiming up to 5x over CPU-only. It also says "NVIDIA cuDF and cuVS are supported by leading enterprise data platforms including EDB Postgres AI, NetApp, Snowflake, Starburst and VAST Data". The section mentions neither HeavyDB nor Sirius — [NVIDIA blog](https://blogs.nvidia.com/blog/gtc-2026-news/)
- Huang at GTC 2026: "We address every single data processing platform." Also: "The libraries is the crown jewels of our company." Also: "We're headed to Nvidia as software company." — [Constellation Research](https://www.constellationr.com/insights/news/nvidia-gtc-2026-were-software-company-too)
- In the keynote's "five-layer cake", Huang put structured data (SQL, Spark, Pandas, cloud warehouses) at the base and called it the "ground truth of business" — [Atlan recap (secondary)](https://atlan.com/know/nvidia-gtc-2026-keynote-recap/)
- Huang in the IBM release: "Together with IBM, we are bringing CUDA GPU acceleration directly into the data layer." In this release IBM owns the engine (watsonx.data Presto) and NVIDIA supplies cuDF and GPUs — [IBM newsroom, 2026-03-16](https://newsroom.ibm.com/2026-03-16-ibm-and-nvidia-announce-expanded-collaboration-at-gtc-2026-to-advance-ai-for-the-enterprise)
- NVIDIA's data-processing solutions page calls cuDF an open-source toolkit with drop-in accelerators for Spark, Presto, Polars and DuckDB. It lists platform partners AWS, Dell, Google Cloud, HPE, IBM, Oracle, Redpanda and Starburst — [nvidia.com data processing](https://www.nvidia.com/en-us/solutions/data-processing/) (seen via search snippet)
- GTC 2026 sessions on accelerated analytics, from an attendee's notes:
  - S81769 "The Era of GPU Data Processing: From SQL to Search and Back Again". Speakers: Joshua Patterson (VP Solutions Architecture, NVIDIA) and Todd Mostak (Sr. Director of Engineering, NVIDIA, ex-HEAVY.AI). Covered Sirius, Theseus, HeavyDB/OmniSci and a "SPACE MICE reference design".
  - S81870 "Achieving 8x Lower Cost Analytics with GPU-Accelerated DuckDB". Speakers: Yogatama (NVIDIA) and Yu (UW-Madison).
  - S81873 on composable building blocks such as cuCascade. Speakers: Felipe and Rodrigo Aramburu (NVIDIA).
  - S81563 on Presto GPU. Speakers: Kimball (NVIDIA) and IBM Research.
  - The notes quote the session describing Sirius as "NVIDIA's GPU-native analytics engine". They report that Hannes Mühleisen called Sirius "the blessed way of running queries on GPUs with DuckDB" and that Sirius will become a core DuckDB extension. They also say HeavyDB was "slated for open-sourcing in late Q2 2026" with Mostak "now at NVIDIA leading this".
  - Source: [jazracherif GTC 2026 notes (third-party blog, not verified against recordings)](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html)
- NVIDIA developer blog on Sirius (2025-12-15) says Sirius was "Developed by University of Wisconsin-Madison with support from NVIDIA engineers". It says Sirius "provides drop-in acceleration for DuckDB—and, in the future, other data systems". NVIDIA is building "foundational, shareable building blocks for GPU data processing" under "MICE principles". The authors are Yu and Yang (UW) and Yogatama and R. Aramburu (NVIDIA) — [NVIDIA developer blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/)

### Inferences
- The founder was told NVIDIA "denies competing". The nearest public evidence is the partner-first messaging: "we address every single data processing platform", plus partner lists that include Snowflake, Starburst, IBM and Google. That is positioning, not a commitment. I found no on-record denial.
- NVIDIA staff present Sirius in two ways. On developer.nvidia.com it is a UW project "with support from NVIDIA". At GTC it is "NVIDIA's GPU-native analytics engine". I read Sirius as NVIDIA's de facto reference engine with academic branding.

### Gaps
- No primary transcript of S81769 or S81870. The quotes above come from one attendee's blog.
- Did not check NVIDIA's 10-K for data-processing wording.
- No source found for the exact phrase "not a database company", in any form.

## 2. Commercial offerings (paid support, AI Enterprise, AI Data Platform, NIMs, hosted service)

### Takeaway
NVIDIA does sell GPU SQL acceleration commercially. The RAPIDS Accelerator for Spark is "exclusively available with NVIDIA AI Enterprise" and comes with paid support. NVIDIA does not run a hosted query service. The "AI Data Platform" is a storage reference design that partners such as Dell, NetApp, Pure and VAST build. NVIDIA does not ship it as a database.

### Cited Findings
- NGC listing: "RAPIDS Accelerator for Apache Spark is exclusively available with NVIDIA AI Enterprise". It offers "Mission critical support, bug fixes, and professional services available through NVIDIA AI Enterprise". Production branches have end-of-life dates, e.g. "Production Branch October 2025 has reached its End of Life as of July 2026" — [NGC spark-rapids PB25h2](https://catalog.ngc.nvidia.com/orgs/nvidia/-/resources/spark-rapids-pb25h2/-)
- NVIDIA docs say the Spark accelerator with AI Enterprise is BYOL. Supported platforms are Dataproc, Databricks on AWS, Azure and EMR — [NVIDIA AI Enterprise Spark docs](https://docs.nvidia.com/ai-enterprise/deployment/spark-rapids-accelerator/latest/overview.html), [Product Support Matrix](https://docs.nvidia.com/ai-enterprise/latest/product-support-matrix) (both via search snippets). The open-source spark-rapids plugin is also public on GitHub (not re-checked this session).
- AI Enterprise subscriptions include Business Standard support, with Business Critical and TAM as add-ons — [NVIDIA AI Enterprise docs](https://docs.nvidia.com/ai-enterprise/deployment/spark-rapids-accelerator/latest/overview.html)
- NVIDIA AI Data Platform, announced at GTC in March 2025: "a customizable reference design for building modern storage systems for enterprise agentic AI" — [NVIDIA blog](https://blogs.nvidia.com/blog/gtc-2026-news/). Partners: DDN, Dell, HPE, Hitachi Vantara, IBM, NetApp, Nutanix, Pure Storage, VAST and WEKA — [NVIDIA-Certified Storage blog](https://blogs.nvidia.com/blog/nvidia-certified-enterprise-storage). The 2026 follow-on is STX, a modular storage reference architecture on Vera Rubin, BlueField-4 and Spectrum-X, "a foundation for partners building a data engine" — [nvidia.com STX](https://www.nvidia.com/en-gb/data-center/ai-storage/stx/)
- Dell AI Data Platform with NVIDIA: "includes accelerated data engines". It has a Spark-based engine using cuDF and a vector DB using cuVS. Dell ships the engines — [NVIDIA blog](https://blogs.nvidia.com/blog/gtc-2026-news/)
- Text-to-SQL: no dedicated NVIDIA text-to-SQL NIM found. There is the AI-Q blueprint (open-source reference agent over enterprise data, databases among its sources) — [NVIDIA dev blog](https://developer.nvidia.com/blog/chat-with-your-enterprise-data-through-open-source-ai-q-nvidia-blueprint). There is also a NeMo Agent Toolkit Vanna text-to-SQL plugin — [PyPI nvidia-nat-vanna](https://pypi.org/project/nvidia-nat-vanna/1.7.0a20260503/)
- HeavyDB is now NVIDIA-owned (see section 4). Its docs live on docs.nvidia.com. The overview links to "Try HEAVY.AI Cloud" but shows no pricing — [docs.nvidia.com/heavyai](https://docs.nvidia.com/heavyai/overview)

### Inferences
- NVIDIA already makes money from GPU SQL acceleration as a supported enterprise software SKU (Spark RAPIDS in AI Enterprise). It does not sell its own database service. The pattern is to sell the accelerator and support, while the engine and data platform belong to Spark, Presto or the partner.
- I found no NVIDIA-hosted SQL or query service on DGX Cloud.

### Gaps
- AI Enterprise pricing per GPU was not checked (public list price was about $4,500 per GPU per year in earlier years (estimate, from memory, not verified)).
- Could not confirm whether HEAVY.AI Cloud or HeavyDB Enterprise is still sold under NVIDIA, or has been wound down into OSS.
- Did not confirm whether Sirius or Presto-GPU will enter AI Enterprise with paid support.

## 3. Job postings (2025–2026)

### Takeaway
I verified only one 2026 posting. It describes open-source library and engine-integration work: libcudf, Velox, Presto. It does not describe an NVIDIA-branded database product. The hiring volume is unknown.

### Cited Findings
- "Senior Software Engineer, C++ and CUDA – Analytics and Data Intelligence", posted 2026-08-15. Team: "Analytics and Data Intelligence (ADI) group", which builds "libcudf … the open-source CUDA C++ library".
  - Duties: "Own development for 'UcxExchange' in Velox (30%)", "Optimize multi-node performance for analytical queries with Presto GPU (30%)", and "Lead projects of interest in Presto, Velox, cuDF (40%)". Examples given: operator completeness, GPU memory oversubscription, CPU fallback.
  - The posting does not mention DuckDB, Sirius, HeavyDB, Spark, Polars or Iceberg.
  - Source: [AnitaB job mirror](https://jobs.anitab.org/companies/nvidia/jobs/90093660-senior-software-engineer-c-and-cuda-analytics-and-data-intelligence)
- Todd Mostak's NVIDIA title appears as "Senior Director of Engineering, ADI" — [Crunchbase](https://www.crunchbase.com/person/todd-mostak). It also appears as "Senior Director of Engineering, RAPIDS" — [RocketReach](https://rocketreach.co/todd-mostak-email_3284953). That would put HeavyDB in the same ADI org as cuDF (inference from titles).

### Inferences
- "Multi-node Presto GPU" plus "efficient CPU fallback" plus "operator completeness" is full-engine work. It is aimed at Presto, an engine NVIDIA does not own.

### Gaps
- Could not query nvidia.wd5.myworkdayjobs.com directly. No count of open roles mentioning DuckDB, Sirius, Polars or Iceberg. A manual Workday search on "cuDF", "Velox", "Sirius" and "HeavyDB" would close this gap.

## 4. Acquisitions, investments, academic funding; who leads Sirius

### Takeaway
NVIDIA has already acquired a GPU SQL database: HEAVY.AI/HeavyDB in 2025, a partial acqui-hire. In 2026 it is releasing the full HeavyDB platform, enterprise features included, as Apache-2.0 open source. Sirius has an academic lead (Xiangyao Yu's group at UW-Madison), but NVIDIA staff are core contributors and co-authors, and NVIDIA supplies cloud credits.

### Cited Findings
- HEAVY.AI was acquired by NVIDIA in 2025 — [Wikipedia](https://en.wikipedia.org/wiki/HEAVY.AI); [NEA portfolio](https://www.nea.com/portfolio/heavy-ai). Pavlo calls it a "sort-of acquihire of HeavyDB". He found no official closing announcement, only an M&A firm listing — [Pavlo, Databases in 2025](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html)
- Official heavyai/heavydb README: "NVIDIA is now releasing the entire platform as open source." "This includes the functionality that was previously only available as an enterprise product." As of October 2026 the project includes all HeavyAI platform functionality. Source headers read "Copyright (c) 2026, NVIDIA CORPORATION & AFFILIATES". License is Apache-2.0. Maintainers review issues "on a best effort basis without guaranteed response timelines" — [github.com/heavyai/heavydb](https://github.com/heavyai/heavydb)
  - Conflict: a search summary said the official repo's latest release was 9.0.0 (2025-10-20), and DBDB marks HeavyDB "abandoned", last commit June 2025 — [dbdb.io](https://dbdb.io/db/heavydb). Both may be stale relative to the October 2026 README.
- Sirius authors (CIDR 2026, arXiv 2508.04701):
  - NVIDIA: Yogatama, Patterson, Kimball.
  - UW-Madison: Yang, Kristensen, Sarda, Kim, Yu (last author, i.e. PI).
  - Others: Cockcroft (OrionX), McKinney (Posit), Gong (Oracle), Teng.
  - Acknowledgement: "supported (in part) by the Lambda Cloud Credits and NVIDIA Data Science Cloud Credits". It thanks "Felipe Aramburu, Amin Aramoon, Rodrigo Aramburu … for their tremendous contributions to Sirius". These are ex-BlazingSQL/Voltron people now at NVIDIA.
  - No NSF grant is listed.
  - Source: [arXiv 2508.04701v3](https://arxiv.org/html/2508.04701v3); [CIDR 2026 PDF](https://vldb.org/cidrdb/papers/2026/p12-yogatama.pdf)
- The first author, Bobbi Yogatama, works at NVIDIA and is "a core contributor to Sirius" — [NVIDIA dev blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/)
- The sirius-db/sirius repo lists "Contributors and partners: NVIDIA, University of Wisconsin-Madison, DuckDB, and VAST Data". It is Apache-2.0 and built on cuDF, cuVS and cuCascade (an NVIDIA-owned repo). About 1,666 commits. Its roadmap includes multi-node, WINDOW, ASOF JOIN, and StarRocks/Doris support "coming soon" — [GitHub sirius-db](https://github.com/sirius-db/sirius)
- cuCascade is in NVIDIA's GitHub org: a GPU/host/disk tiered memory library, Apache-2.0, "Experimental" — [github.com/NVIDIA/cuCascade](https://github.com/NVIDIA/cuCascade)
- DuckDB docs list Sirius under the `core_extensions` path: "The `sirius` extension is not yet available. It is coming soon." It describes Sirius as "a GPU-native SQL engine that runs DuckDB queries on NVIDIA GPUs" — [duckdb.org docs](https://duckdb.org/docs/2.0/core_extensions/sirius)
- NVIDIA invested in WEKA's $140M round in May 2024 (storage, not a database) — [TechCrunch](https://techcrunch.com/2026/01/02/nvidias-ai-empire-a-look-at-its-top-startup-investments/)
- I found no evidence of NVentures stakes in Voltron Data, Kinetica, SQream, MotherDuck, ClickHouse, Weaviate, Databricks or Snowflake — [Tracxn NVentures](https://tracxn.com/d/venture-capital/nventures/__72RT_CJsiqrpcbOFu1VPO7osCvnURipMIhIWwLFtajg) (partial listings, so not conclusive)
- HEAVY.AI shipped a Grace Hopper-accelerated platform in January 2025, before the acquisition — [BusinessWire](https://www.businesswire.com/news/home/20250121115004/en/HEAVY.AI-Announces-Availability-of-Analytics-Platform-Accelerated-by-NVIDIA-Grace-Hopper-Superchip)

### Inferences
- "NVIDIA backs Sirius" is accurate, and it understates NVIDIA's role. NVIDIA employs the first author and several core contributors, owns the memory-tiering library Sirius depends on, presents it at GTC as NVIDIA's engine, and got DuckDB's blessing for it as a core extension. Sirius looks like NVIDIA's way to own the single-node GPU SQL path without selling a DB SKU.
- NVIDIA's pattern with HeavyDB: buy a GPU database, then open-source all of it. It does not sell it. That fits "enable, don't compete": NVIDIA monetizes GPUs and AI Enterprise. It also means NVIDIA is putting free, complete GPU SQL engines on the market (HeavyDB, Sirius), which commoditizes the startup's layer.

### Gaps
- HEAVY.AI deal terms and closing date are unknown. No NVIDIA press release found.
- I could not see the Sirius commit-level split between NVIDIA and UW committers. Checking git shortlog for email domains would settle it (not cloned).

## 5. Voltron Data / Theseus fate and NVIDIA link

### Takeaway
Voltron Data died in 2025, and Theseus never launched broadly. Several of its key engineers are now at NVIDIA working on Sirius and cuCascade. I found no record of a formal acquisition by NVIDIA.

### Cited Findings
- Pavlo: Voltron was "supposed to be the supergroup of database companies", with "top engineers from Nvidia Rapids". It "built a GPU-accelerated database (Theseus), but failed to launch it in a timely manner". He also writes: "The death of Voltron and sort-of acquihire of HeavyDB seem to continue the trend of the inviability of GPU-accelerated databases." And: "you will hear some major GPU-accelerated database announcements by vendors in 2026." — [Pavlo retrospective, Jan 2026](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html)
- The Information (Nov 2024) reported a CEO change and layoffs of about half of roughly 120 staff after a missed revenue target — [LeadIQ citing The Information](https://leadiq.com/c/voltron-data/610c290b799f6532db40758e). Notice lists the company as "defunct '25" — [notice.co](https://notice.co/c/voltrondata)
- 2025 activity before the end: Accenture investment in February 2025 — [Accenture](https://newsroom.accenture.com/news/2025/accenture-invests-in-voltron-data-to-help-organizations-use-gpu-technology-to-simplify-large-scale-data-processing). AMD GPU preview in June 2025 — [The Register](https://www.theregister.com/2025/06/18/voltron_amd_sql/)
- Ex-Voltron people now at NVIDIA:
  - Josh Patterson, co-founder and CEO, is now VP Solutions Architecture — [TechArena](https://techarena.ai/people/josh-patterson)
  - Rodrigo Aramburu, "co-founded BlazingSQL and Voltron Data", now leads DevRel for accelerated data processing — [NVIDIA dev blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/)
  - Felipe Aramburu is a Distinguished Solutions Architect presenting cuCascade with "the Theseus lineage" — [GTC notes](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html)
- voltrondata.com did not resolve (DNS error) when fetched on 2026-10-28. This may be transient.

### Inferences
- Theseus' ideas and people went into NVIDIA's open building blocks (cuCascade, Sirius). There is no evidence that NVIDIA picked Theseus up as a product.

### Gaps
- No primary source on the exact shutdown date or on the fate of Theseus IP.

## 6. Verdict: how much should a small GPU-SQL startup fear NVIDIA shipping its own product?

### Takeaway
There is low risk that NVIDIA sells a branded, paid GPU database or SQL service in competition with its partners (estimate). There is high risk that NVIDIA gives away a complete, competitive GPU SQL engine for free. It already does: Sirius for DuckDB, Presto-GPU via Velox, Spark RAPIDS, and now all of HeavyDB under Apache-2.0. For the startup the threat is commoditization, not a competing SKU.

### Cited Findings
- NVIDIA's revenue model here is GPUs plus AI Enterprise subscriptions. The Spark accelerator is sold only through AI Enterprise — [NGC](https://catalog.ngc.nvidia.com/orgs/nvidia/-/resources/spark-rapids-pb25h2/-)
- Partners NVIDIA would upset by selling a DB include Snowflake, Starburst, IBM, Google, Oracle, Dell and VAST — [NVIDIA blog](https://blogs.nvidia.com/blog/gtc-2026-news/)
- NVIDIA does own and maintain full engines: HeavyDB (fully open) — [heavydb README](https://github.com/heavyai/heavydb). It is also the de facto co-owner of Sirius — [arXiv](https://arxiv.org/html/2508.04701v3)
- Pavlo: OLAP engines are commoditizing. Differentiation comes from UX and "the quality of the query plans their optimizers generate" — [Pavlo](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html)

### Inferences
- If the startup builds its own engine (DataFusion planner + cuDF executor), it competes head-on with free NVIDIA-staffed engines (Sirius, Presto-GPU, HeavyDB) on NVIDIA's own libraries. NVIDIA controls cuDF's roadmap and has a much larger team. A two-to-three-person team is unlikely to win on raw engine speed (estimate).
- If the startup builds on Sirius, it gets alignment with NVIDIA: NVIDIA has every incentive to keep Sirius free and improving. The risk is that NVIDIA or DuckDB Labs/MotherDuck ship the same "product" layer on top (hosted service, management, connectors).
  - NVIDIA itself has shown no interest in a hosted service.
  - MotherDuck or the clouds are the likelier competitors (estimate).
- What I would treat as safe ground (estimate): the parts NVIDIA consistently leaves to partners. That means the hosted or managed service, vertical workflows, data-platform integration (catalog, Iceberg, governance), and support and SLAs for buyers who are not on AI Enterprise.
- The "denies competing" claim fits the evidence on product SKUs: no NVIDIA DB service, and HeavyDB was open-sourced, not sold. It is not reassuring at the engine layer.

### Gaps
- No signal found on whether NVIDIA will offer paid support for Sirius or HeavyDB through AI Enterprise. If it does, NVIDIA becomes the vendor of record for GPU DuckDB/HeavyDB, which matters for a startup that plans to sell support.
- Whether Pavlo's predicted "major GPU-accelerated database announcements by vendors in 2026" have happened, and by whom, was not checked beyond the sources above.
