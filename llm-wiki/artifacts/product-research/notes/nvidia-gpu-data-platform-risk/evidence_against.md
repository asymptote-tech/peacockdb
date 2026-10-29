# NVIDIA as a direct or future competitor to a small GPU SQL startup: the adversarial case

All URLs accessed 2026-10-28. "(estimate)" marks a guess. Items under "Cited Findings" are what the sources say. Items under "Inferences" are my reading of them. GitHub facts come from `gh api` calls made on 2026-10-28.

## Q1. Does NVIDIA move up the stack against its ecosystem partners? (precedents)

### Takeaway
Yes, repeatedly, and the pattern is consistent. NVIDIA presents each move as "enabling partners", then buys or builds the layer above its libraries. Partners get caught in the overlap (Lepton vs CoreWeave and Nebius), and acquired startups often stop operating on their own (CentML). NVIDIA sometimes pulls back when hyperscalers push back (DGX Cloud). A small startup has no leverage of that kind.

### Cited Findings
- **Gretel (Mar 2025).** The reported price was above Gretel's last $320M valuation. About 80 staff were folded into NVIDIA's cloud and generative-AI services. — [CB Insights / search summary](https://www.cbinsights.com/research/nvidia-next-big-bet-physical-ai/), [AIwire](https://www.hpcwire.com/aiwire/2025/04/01/nvidias-strategic-acquisitions-signal-push-toward-full-stack-ai-control/) (accessed 2026-10-28)
- AIwire framed NVIDIA's 2025 acquisitions as a "push toward full-stack AI control". — [AIwire, 2025-04-01](https://www.hpcwire.com/aiwire/2025/04/01/nvidias-strategic-acquisitions-signal-push-toward-full-stack-ai-control/) (accessed 2026-10-28)
- **CentML (Jun 2025).** Co-founders and at least 15 engineers joined NVIDIA, and CentML ceased operations. — search summary of 2025 acquisition coverage, [TechCrunch snippet](https://techcrunch.com/snippet/2983711/nvidia-reportedly-buys-an-ai-startup/) (accessed 2026-10-28)
- **Run:ai** was bought for about $700M in 2024. — [AIwire](https://www.hpcwire.com/aiwire/2025/04/01/nvidias-strategic-acquisitions-signal-push-toward-full-stack-ai-control/) (accessed 2026-10-28)
- **Other 2025 deals:**
  - Solver, an AI coding-agent startup, acquired Sep 2025. — [Tech Startups](https://techstartups.com/2025/09/03/nvidia-acquires-ai-coding-startup-solver-amid-growing-ai-investment-spree/) (accessed 2026-10-28)
  - SchedMD (Slurm), acquired Dec 2025 with a pledge to keep Slurm open source and vendor-neutral. — [TechCrunch 2025-12-15](https://techcrunch.com/2025/12/15/nvidia-bulks-up-open-source-offerings-with-an-acquisition-and-new-open-ai-models/) (accessed 2026-10-28)
  - Enfabrica: over $900M to hire its CEO and license its technology. — [CNBC 2025-09-18](https://www.cnbc.com/2025/09/18/nvidia-spent-over-900-million-on-enfabrica-ceo-ai-startup-technology.html) (accessed 2026-10-28)
  - Groq: assets bought for about $20B. — [CNBC 2025-12-24](https://www.cnbc.com/2025/12/24/nvidia-buying-ai-chip-startup-groq-for-about-20-billion-biggest-deal.html) (accessed 2026-10-28)
- **Lepton AI / DGX Cloud Lepton.**
  - Some Lepton features overlapped with CoreWeave, a key NVIDIA cloud partner. — search summary of The Information report (accessed 2026-10-28)
  - DGX Cloud Lepton puts NVIDIA "in competition with companies like CoreWeave and Nebius, which are also participating in the service". Partners worried it reduces the need to use independent clouds. — [TipRanks](https://www.tipranks.com/news/nvidias-nvda-new-cloud-market-push-puts-partners-on-edge) (accessed 2026-10-28)
  - NVIDIA says it doesn't compete with its partners. — same source and [Techstrong](https://techstrong.ai/features/nvidia-in-the-ai-cloud-cooperation-and-competition/) (accessed 2026-10-28)
- **DGX Cloud.** NVIDIA "steps back from DGX Cloud — stops trying to compete with AWS and Azure". DGX Cloud is now mainly for internal use, per a 2025 report from The Information. — [Tom's Hardware](https://www.tomshardware.com/tech-industry/nvidia-steps-back-from-dgx-cloud), [Motley Fool](https://www.fool.com/investing/2025/09/19/nvidia-amazon-microsoft-google-coreweave) (accessed 2026-10-28)
- **NIM.** NIM ships pre-built inference containers built on Triton and TensorRT-LLM. Comparison sites pit it directly against inference startups such as FriendliAI and Together AI (low-quality AI-generated comparison pages, weak evidence). — [rfp.wiki](https://www.rfp.wiki/artificial-intelligence/cloud-ai-developer-services/nvidia-nim-microservices/friendliai) (accessed 2026-10-28)
- **Jensen Huang** has said NVIDIA is "a computing platform company" that "doesn't build clouds" (Lex Fridman podcast). — [Lex Fridman transcript](https://lexfridman.com/jensen-huang-transcript/) (accessed 2026-10-28)

### Inferences
- The "we don't compete" line has been said before and later contradicted by product moves (DGX Cloud, Lepton). That retreat happened because AWS, Azure and CoreWeave are huge customers. A 3-person startup buys no GPUs at scale and gives NVIDIA no reason to hold back.
- NVIDIA's acqui-hire pattern targets the software layer just above its own libraries (scheduling, inference optimisation, synthetic data, Slurm). A GPU SQL engine on libcudf sits in exactly that position.

### Gaps
- Lepton AI's price and closing date were not confirmed. Only "advanced talks" reporting and the May 2025 DGX Cloud Lepton launch were found.
- I found no primary source for what happened to OctoAI, Deci or Brev partners after their acquisitions. I did not search these separately.
- I found no NVIDIA quote on the record that says "we do not compete in GPU SQL". The founder's claim could not be traced to a source.

## Q2. Acquisitions and acqui-hires in data, analytics and databases (2023–2026)

### Takeaway
This is the strongest finding. **NVIDIA acquired HEAVY.AI (MapD/OmniSci) in 2025.** As of October 2026 it is open-sourcing the whole HEAVY.AI platform, including former enterprise features, under Apache-2.0. The work is done with nvidia.com commits, and the docs now live on docs.nvidia.com. Separately, the BlazingSQL and Voltron Data GPU-SQL engineers now work at NVIDIA and are top committers to Sirius. NVIDIA now owns a complete, free GPU SQL database and employs most of the experienced GPU-SQL engine builders.

### Cited Findings
- **The HEAVY.AI deal.**
  - HEAVY.AI (formerly MapD/OmniSci; founded 2013 by Todd Mostak) was "Acquired by Nvidia" in 2025. Wikipedia cites [Harvest MP transactions](https://www.harvestmp.com/transactions) and [NEA portfolio](https://www.nea.com/portfolio/heavy-ai). No price or date is given. — [Wikipedia: HEAVY.AI](https://en.wikipedia.org/wiki/HEAVY.AI) (accessed 2026-10-28)
  - HEAVY.AI documentation is now hosted at docs.nvidia.com/heavyai. HeavyDB remains Apache-2.0. — [docs.nvidia.com/heavyai/overview](https://docs.nvidia.com/heavyai/overview) (accessed 2026-10-28)
- **The October 2026 open-sourcing.**
  - The canonical repo heavyai/heavydb README says: "as of October 2026, this project now includes all of the functionality of the entire HeavyAI platform. This includes the functionality that was previously only available as an enterprise product. NVIDIA is now releasing the entire platform as open source." That covers the Immerse UI, the GPU renderer, HeavyRF, and HeavyConnect (Parquet, CSV and S3 foreign tables). — [github.com/heavyai/heavydb](https://github.com/heavyai/heavydb) (accessed 2026-10-28)
  - The latest commits (2026-10-03 to 2026-10-07) are all from nvidia.com addresses, including "Bump master to 10.1". The repo was last pushed 2026-10-23 and has 3,068 stars. — `gh api repos/heavyai/heavydb/commits` (accessed 2026-10-28)
- **Talks and staff at GTC 2026.**
  - Todd Mostak appears as "Sr. Director of Engineering", NVIDIA, co-presenting S81769 with Joshua Patterson (NVIDIA VP, Solutions Architecture). — [GTC 2026 analytics write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (secondary blog, accessed 2026-10-28)
  - The same write-up says HEAVY.AI/OmniSci was slated for open-source release in late Q2 2026. The actual release appears to have slipped to October 2026 per the README. — same source; [heavyai/heavydb](https://github.com/heavyai/heavydb) (accessed 2026-10-28)
- **Voltron Data (still independent, but weakening).**
  - In Nov 2024 Voltron Data switched CEOs and laid off about 50% of roughly 120 staff after missing revenue targets. — [The Information](https://www.theinformation.com/briefings/ai-startup-voltron-data-switches-ceos-lays-off-staff) (accessed 2026-10-28)
  - Accenture invested in Feb 2025. — [Accenture newsroom](https://newsroom.accenture.com/news/2025/accenture-invests-in-voltron-data-to-help-organizations-use-gpu-technology-to-simplify-large-scale-data-processing) (accessed 2026-10-28)
  - Voltron added AMD Instinct support to its Theseus engine in Jun 2025. — [The Register 2025-06-18](https://www.theregister.com/2025/06/18/voltron_amd_sql/) (accessed 2026-10-28)
  - Headcount was 58 in Sep 2025 per Tracxn; PitchBook says 100 (the two conflict). — [Tracxn](https://tracxn.com/d/companies/voltron-data/__wvF7PoiDC_jlFFR_L26kK2r6zeDYf7BX_lxNDcx7Mng), [PitchBook](https://pitchbook.com/profiles/company/471272-77) (accessed 2026-10-28)
- **Where former BlazingSQL and Voltron Data engineers work now (self-reported GitHub profiles):**
  - William Malpica (BlazingSQL co-founder, then Voltron): company field "Nvidia".
  - Alexander Ocsa (BlazingSQL, then Voltron): bio "Senior Solutions Architect, Data Processing at NVIDIA".
  - Matthijs Brobbel (Voltron/Arrow): "@NVIDIA".
  - Rodrigo Aramburu: GitHub still says "Voltron Data", but LinkedIn reads "Accelerated Data Processing at NVIDIA" and he co-authored NVIDIA's Sirius blog post.
  - Felipe Aramburu (BlazingSQL founder) is listed as an NVIDIA "Distinguished Solutions Architect" at GTC 2026 and created the cuCascade fork alongside NVIDIA/cuCascade.
  - Sources: `gh api users/...`, [LinkedIn roaramburu](https://www.linkedin.com/in/roaramburu/), [GTC write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28)
- GTC 2026 described Theseus (Voltron's engine) as "a research engine behind cuCascade". cuCascade, a GPU memory-tier and spill library used by Sirius, is now published under github.com/NVIDIA (created 2025-12-15). — [GTC write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html); [github.com/NVIDIA/cuCascade](https://github.com/NVIDIA/cuCascade) (accessed 2026-10-28)

### Inferences
- A deal between NVIDIA and Voltron Data was not found. The pattern does look like an acqui-hire or talent migration from Voltron to NVIDIA in 2025 (estimate, not confirmed by any news source): key engineers moved, and the Theseus memory-tier design came out as an NVIDIA repo.
- "NVIDIA doesn't compete in GPU SQL" was arguably already false in October 2026. NVIDIA owns HeavyDB, a full GPU SQL database with UI, and is giving it away. A free, NVIDIA-backed GPU SQL database caps what a startup can charge for a "GPU SQL server".
- Giving it away is the commoditise-the-complement play (sell GPUs, make the software free). It hurts a startup selling licences just as much as direct competition would.

### Gaps
- No HEAVY.AI price, closing date or official press release was found. No NVIDIA newsroom item was found either.
- I found no news report confirming a Voltron Data acqui-hire. This is inferred only from staff profiles.

## Q3. Is spark-rapids already a product? Does NVIDIA co-own the lakehouse SQL GPU path (Velox/Presto/IBM)? Is cudf-polars an embryonic engine?

### Takeaway
Yes on all three.
- **Spark.** The Spark plugin is a supported, sold NVIDIA product: it is bundled with NVIDIA AI Enterprise, has its own migration service (Project Aether), and was renamed in 2026 to **NVIDIA "cuDF for Apache Spark"**.
- **Presto/Velox.** NVIDIA and IBM jointly own the GPU path through Presto and Velox, which is in production at Nestlé via watsonx.data.
- **Polars.** cudf-polars now has a streaming, multi-GPU backend (RapidsMPF with Ray, Dask or SPMD). That is a distributed engine in all but name.

### Cited Findings
- **Spark plugin.**
  - The repo NVIDIA/spark-rapids now resolves to NVIDIA/cudf-spark, described as "NVIDIA cuDF for Apache Spark plugin". Homepage nvidia.github.io/cudf-spark. Releases v26.06.1 (2026-07-23), v26.08.0 (2026-08-20), v26.08.1 (2026-08-28). — `gh api repos/NVIDIA/spark-rapids` (accessed 2026-10-28)
  - NVIDIA docs say the RAPIDS Accelerator for Apache Spark "is exclusively available with NVIDIA AI Enterprise" and is supported on Dataproc, Databricks and EMR. This is the search-result summary of the docs page; the direct fetch of the overview page did not show the sentence. — [NVIDIA AI Enterprise Spark RAPIDS docs](https://docs.nvidia.com/ai-enterprise/deployment/spark-rapids-accelerator/latest/overview.html) (accessed 2026-10-28)
- **Project Aether (GTC 2025).**
  - It is a set of microservices that automate migrating and tuning CPU Spark jobs to GPUs. The QualX XGBoost model predicts speedup, and the steps are predict, optimize, validate, migrate. Large-scale migrations require applying for access.
  - Vendor claim: a year of engineer work for 100 jobs drops to about 4 days per job.
  - Sources: [NVIDIA blog](https://blogs.nvidia.com/blog/project-aether-accelerates-apache-spark), [NVIDIA dev blog on EMR](https://developer.nvidia.com/blog/migrate-apache-spark-workloads-to-gpus-at-scale-on-amazon-emr-with-project-aether/), [BigDataWire 2025-03-21](https://bigdatawire.com/2025/03/21/nvidia-pushes-boundaries-of-apache-spark-with-rapids-and-project-aether) (accessed 2026-10-28)
- **Velox/Presto with IBM.**
  - The Presto C++, Velox and cuDF stack in IBM watsonx.data is in production. IBM reports Nestlé ran 5x faster at 83% lower cost (IBM figures).
  - The IBM–NVIDIA partnership was expanded at GTC 2026.
  - Sources: [IT Jungle 2026-03-23](https://www.itjungle.com/2026/03/23/ibm-taps-nvidia-gpus-for-ai-turbocharged-data-mart/), [IBM PR](https://www.prnewswire.com/news-releases/ibm-announces-expanded-collaboration-with-nvidia-to-advance-ai-for-the-enterprise-302714975.html), [NVIDIA dev blog](https://developer.nvidia.com/blog/accelerating-large-scale-data-analytics-with-gpu-native-velox-and-nvidia-cudf) (accessed 2026-10-28)
  - NVIDIA staff present this work at GTC (Greg Kimball, cuDF manager, with IBM Research). NVIDIA built UCXExchange shuffle, which cut one TPC-H SF1K run from 690 s to 60 s. — [GTC write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28)
  - There is an arXiv paper titled "Accelerating Presto with GPUs" (2026). — [arXiv 2606.24647](https://arxiv.org/html/2606.24647v1) (accessed 2026-10-28)
  - The rapidsai/velox-testing repo was created 2025-07-22. — `gh api` (accessed 2026-10-28)
- **NVIDIA's own positioning.** The data-processing solutions page lists cuDF as having "drop-in accelerators for Apache Spark, Presto, Polars, and DuckDB". It cites Starburst/Trino (up to 7x), Cloudera, Snap, AWS EMR and Dell AI Data Platform. — [nvidia.com/solutions/data-processing](https://www.nvidia.com/en-us/solutions/data-processing/) (accessed 2026-10-28)
- **cudf-polars.**
  - The Polars GPU engine runs on a streaming backend built on RapidsMPF, recommended for any nontrivial workload.
  - The 26.10 API lists RayEngine, DaskEngine and SPMD multi-GPU modes.
  - Sources: [cudf_polars streaming docs](https://docs.rapids.ai/api/cudf/latest/cudf_polars/streaming_execution), [API 26.10](https://docs.nvidia.com/cudf/26.10/cudf_polars/api/index.html) (accessed 2026-10-28)
  - Polars Inc. itself is hiring for a GPU streaming backend. — [pola.rs](https://pola.rs/posts/gpu-streaming-backend) (accessed 2026-10-28)
- **Multi-GPU design.** At GTC 2026 an NVIDIA speaker said "cuDF itself is not intended to be a multi-GPU library". Multi-GPU support comes through cuCascade and RapidsMPF instead. — [GTC write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28)

### Inferences
- The lakehouse GPU SQL path is co-owned by NVIDIA in practice.
  - Spark: NVIDIA's own plugin, sold through AI Enterprise.
  - Presto/Velox: NVIDIA and IBM.
  - Trino: Starburst with NVIDIA.
  - Distributed shuffle: NVIDIA's RapidsMPF and UCX.
- A startup's Iceberg GPU accelerator would compete with NVIDIA's showcase reference stacks, sold to the same buyers.
- RapidsMPF, cudf-polars streaming and cuCascade together amount to an NVIDIA-owned distributed GPU query runtime. The "engine" part is thin on top. A DataFusion or Rust planner on libcudf would sit where NVIDIA is already building parts.

### Gaps
- No public Project Aether GA or pricing was found.
- I could not quote the "exclusively available with NVIDIA AI Enterprise" sentence from a direct page fetch; it appears only in the search-result summary.

## Q4. Does NVIDIA's work on DuckDB/Sirius mean NVIDIA-aligned people are taking the DuckDB GPU niche?

### Takeaway
Yes. Sirius is in practice an NVIDIA engine with a UW-Madison origin.
- The top committer and many others are NVIDIA staff, and 54 of the last 200 commits (about 27%) use nvidia.com addresses. Many NVIDIA staff commit from personal or noreply addresses, so that share is a lower bound.
- NVIDIA's own GTC session calls it "NVIDIA's GPU-native analytics engine".
- DuckDB's creator reportedly blessed it as the core DuckDB GPU extension.

"NVIDIA backs Sirius" therefore means NVIDIA is building Sirius. A startup building on Sirius would be building on a product NVIDIA controls the roadmap of.

### Cited Findings
- **Repo and origin.**
  - sirius-db/sirius: Apache-2.0, created 2024-06-19, 1,101 stars, pushed 2026-10-28. The README lists "NVIDIA, University of Wisconsin-Madison, DuckDB, and VAST Data" as contributors and partners. — [github.com/sirius-db/sirius](https://github.com/sirius-db/sirius) (accessed 2026-10-28)
  - NVIDIA's Dec 2025 blog says Sirius was developed by UW-Madison "with support from NVIDIA engineers". It is built on cuDF and RMM and uses Substrait. — [NVIDIA dev blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/) (accessed 2026-10-28)
- **Top contributors by commits, with self-reported affiliation:**
  - bwyogatama (Bobbi Yogatama, NVIDIA Sr. Systems SW Engineer per GTC): 310
  - mbrobbel (@NVIDIA): 236
  - Yifei-yang7 (UW-Madison): 128
  - kevkrist ("Solutions Architect at NVIDIA"): 93
  - felipeblazing (Felipe Aramburu, NVIDIA per GTC): 70
  - wmalpica (Nvidia): 67
  - 9prady9 (@nvidia): 60
  - mike-wendt (RAPIDS ops lead, estimate): 53
  - aminaramoon (Nvidia): 53
  - dhruv9vats (@NVIDIA), madsbk (NVIDIA), jrbourbeau ("Senior Solutions Architect @NVIDIA"), aocsa (NVIDIA), wence- (Lawrence Mitchell, cudf-polars dev, estimate)
  - Source: `gh api repos/sirius-db/sirius/contributors` and `users/*` (accessed 2026-10-28)
- **Commit email domains.** The last 200 commits break down as nvidia.com 25 + 29, gmail 22 + 41, and github noreply 39 + 19. The rest use other personal domains, plus one selectdb.com commit (SelectDB is the company behind Apache Doris). — `gh api repos/sirius-db/sirius/commits` (accessed 2026-10-28)
- **The CIDR 2026 paper** "Rethinking Analytical Processing in the GPU Era" (first author Yogatama) reports Sirius at 8.3x and 7.4x better cost efficiency than DuckDB on TPC-H and ClickBench. — [CIDR 2026 PDF](https://vldb.org/cidrdb/papers/2026/p12-yogatama.pdf), [arXiv 2508.04701](https://arxiv.org/pdf/2508.04701) (accessed 2026-10-28)
- **GTC 2026.**
  - The cuCascade session called Sirius "NVIDIA's GPU-native analytics engine".
  - Hannes Mühleisen announced on stage that Sirius will become a core DuckDB extension, "the blessed way of running queries on GPUs with DuckDB".
  - Reported: 22 TPC-H SF1K queries in about 21 s on one GB300 node, and #1 and #2 on ClickBench hot runs.
  - Source: [GTC write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (secondary; no DuckDB primary source found). (accessed 2026-10-28)

### Inferences
- The DuckDB GPU niche is closed to an independent: the incumbent is NVIDIA-staffed and blessed by DuckDB Labs.
- Building a product on Sirius exposes the startup to NVIDIA's roadmap, refactors and priorities (for example cuCascade, which was built in about 3 months, per the write-up). The startup would hold no moat against NVIDIA shipping the same feature upstream.
- The selectdb.com commit hints that the Doris/VeloDB camp is also looking at Sirius (weak signal, one commit).

### Gaps
- No primary DuckDB Labs announcement of core-extension status was found.
- The affiliation of several top committers (Joost Hoozemans, Johan Peltenburg, Akhil Nair) is unknown. Hoozemans and Peltenburg are ex-Voltron/Arrow names (estimate).

## Q5. Does NVIDIA crowd vector-sql (cuVS, NeMo Retriever, AI Data Platform)?

### Takeaway
Yes. cuVS is already integrated into nearly every major vector engine (Milvus, Elastic, OpenSearch, FAISS, Oracle, Weaviate). NVIDIA's "AI Data Platform" reference design, built with Dell and others, bundles cuDF and cuVS. A vector-plus-SQL startup would be repackaging NVIDIA libraries that every incumbent already ships.

### Cited Findings
- **GTC 2026 announcements.**
  - cuDF and cuVS "are being adopted by data platforms across industries". cuVS accelerates FAISS, Amazon OpenSearch Service and Milvus. Oracle's Private AI Services Container supports cuVS. — [NVIDIA GTC 2026 live blog](https://blogs.nvidia.com/blog/gtc-2026-news/) (accessed 2026-10-28)
  - Keynote: Dell "create[d] the Dell AI Data Platform that integrates cuDF and cuVS". — same, and [NextBigFuture recap](https://www.nextbigfuture.com/2026/03/nvidia-structured-data-is-the-ground-truth-of-ai-120-billion-structure-data-ecosystem.html) (accessed 2026-10-28)
  - Huang reportedly called cuDF the "RTX for 3D graphics" of structured data. GTC 2026 adoption list: IBM watsonx.data, Dell, Google Cloud Dataproc, Oracle AI Database 26ai, Amazon OpenSearch, Milvus, Weaviate, FAISS. — [NextBigFuture](https://www.nextbigfuture.com/2026/03/nvidia-structured-data-is-the-ground-truth-of-ai-120-billion-structure-data-ecosystem.html) (secondary, accessed 2026-10-28)
- Elastic says its GPU-accelerated search with cuVS gives 12x faster indexing (vendor claim). — [Elastic GTC page](https://www.elastic.co/events/nvidia-gtc) (accessed 2026-10-28)
- NVIDIA's data-processing page says cuVS "integrates with OpenSearch, Elastic, Milvus, and more". It lists Oracle AI Database (10x faster vector index generation) and Dell AI Data Platform (12x faster vector indexing). — [nvidia.com/solutions/data-processing](https://www.nvidia.com/en-us/solutions/data-processing/) (accessed 2026-10-28)

### Inferences
- Vector plus SQL on GPU is the scenario NVIDIA markets most, and it markets it through big partners (Oracle, Elastic, Dell, IBM). A startup there competes with the partners NVIDIA promotes, not just with NVIDIA.

### Gaps
- I did not research NeMo Retriever or AI-Q separately, so there is no specific evidence of their effect on RAG or vector-DB startups.

## Q6. AMD and other accelerators: is there a cuDF analogue, and does NVIDIA licensing add risk?

### Takeaway
AMD has hipDF, a ROCm port of cuDF API-compatible with cuDF 25.10. AMD is therefore commoditising this layer itself, which shrinks the room for an independent "cuDF for AMD". NVIDIA's CUDA EULA bans translation layers that run CUDA output on non-NVIDIA hardware. A source-level port (as hipDF does) seems allowed, while a binary or translation approach carries licence risk.

### Cited Findings
- **hipDF.**
  - "A ROCm port that is derived work based on the NVIDIA RAPIDS cuDF project (version 25.10)", API-compatible with cuDF 25.10. — [AMD-Ecosystem/hipDF](https://github.com/AMD-Ecosystem/hipDF), [hipDF docs](https://rocm.docs.amd.com/projects/hipDF/en/latest/what-is-hipdf.html) (accessed 2026-10-28)
  - Earlier docs (2025-05) said it was early access and "running production workloads is not recommended". — [hipDF docs 25.05](https://rocm.docs.amd.com/projects/hipDF/en/docs-25.05/index.html) (accessed 2026-10-28)
  - It is part of the ROCm-DS suite. — [AMD ROCm blog](https://rocm.blogs.amd.com/software-tools-optimization/introducing-rocm-ds-revolutionizing-data-processing-with-amd-instinct-gpus/README.html) (accessed 2026-10-28)
- Voltron Data's Theseus added AMD Instinct support in 2025. — [The Register 2025-06-18](https://www.theregister.com/2025/06/18/voltron_amd_sql/) (accessed 2026-10-28)
- A third-party repo "cuda2rocm" (created 2026-07-12) ports cuCascade and cuCollections to ROCm (gfx942, ROCm 7.2.1). — `gh api search/repositories?q=cucascade`, [github.com/mycomputercrashsalot/cuda2rocm](https://github.com/mycomputercrashsalot/cuda2rocm) (accessed 2026-10-28)
- **CUDA EULA.**
  - The EULA bars reverse-engineering CUDA output to target non-NVIDIA platforms. The clause has been online since 2021 (CUDA 11.5/11.6 era) and was added to installed EULA files, widely read as aimed at ZLUDA and Chinese GPU makers.
  - NVIDIA made no official statement on what the clause targets.
  - Sources: [Tom's Hardware, Mar 2024](https://www.tomshardware.com/pc-components/gpus/nvidia-bans-using-translation-layers-for-cuda-software-to-run-on-other-chips-new-restriction-apparently-targets-zluda-and-some-chinese-gpu-makers), [Overclock3D](https://overclock3d.net/news/gpu-displays/nvidia-bans-the-use-of-translation-layers-on-cuda-software/), [Wccftech](https://wccftech.com/nvidia-halts-use-of-cuda-on-other-platforms-lists-new-warning-in-the-eula/) (accessed 2026-10-28)

### Inferences
- The "other-accelerators" direction is squeezed from both sides. AMD funds its own cuDF port (hipDF), and NVIDIA's EULA makes translation approaches legally fragile. A startup would also trail cuDF releases with a lag.
- cuDF and RAPIDS are Apache-2.0, so porting at the source level is legal. The risk lies in the CUDA toolkit or binaries, not in libcudf.

### Gaps
- No current statement on hipDF production readiness for 26.x was found.
- I found nothing on a Cerebras, Intel or Chinese-GPU cuDF analogue.

## Q7. Platform-control risks: API churn, licences, release cadence, dropped hardware

### Takeaway
RAPIDS moves fast and drops support aggressively:
- A release roughly every 2 months.
- Pascal dropped in 24.02.
- All CUDA 11 support dropped in 25.08.
- The CUDA floor raised to 12.2 in 25.12.
- The Spark plugin was renamed in 2026.

A cuDF-based engine run by 1–3 people would spend real time keeping up.

### Cited Findings
- RSN 48: RAPIDS dropped all CUDA 11 support (containers, wheels, conda, builds from source) in v25.08; v25.06 was the last release with CUDA 11. — [RSN 48](https://docs.rapids.ai/notices/rsn0048/) (accessed 2026-10-28)
- RSN 34: Pascal was deprecated in v23.12 and removed in v24.02. Compute capability 7.0 or higher is required. — [RSN 34](https://docs.rapids.ai/notices/rsn0034/) (accessed 2026-10-28)
- RSN 54: the CUDA minimum was raised from 12.0 to 12.2 in v25.12. — [RAPIDS notices index](https://docs.rapids.ai/notices/rsn/) (accessed 2026-10-28)
- Release cadence evidence: cudf-spark v26.06.1 (2026-07-23), v26.08.0 (2026-08-20) and v26.08.1 (2026-08-28), plus cudf docs for 26.08 and 26.10. That indicates bimonthly releases. — `gh api` and [docs.nvidia.com/cudf/26.10](https://docs.nvidia.com/cudf/26.10/cudf_polars/api/index.html) (accessed 2026-10-28)
- NVIDIA's general SLA (seen in the CUTLASS Python DSL licence) bars making the software subject to an open-source licence. — [CUTLASS licence](https://docs.nvidia.com/cutlass/4.6.0/media/docs/pythonDSL/license.html) (accessed 2026-10-28)

### Inferences
- With no API-stability promise in libcudf, every bimonthly release can break a C++ executor (estimate, based on cadence). Upstream NVIDIA engines (Sirius, Velox-cuDF, cudf-polars) get co-ordinated changes, while an outside engine absorbs the breakage afterwards.

### Gaps
- No primary source was found on nvCOMP going closed and then reopened. That part of the question remains unverified.
- No libcudf ABI/API-stability policy was found.

## Per-direction NVIDIA platform-risk table

| Direction | Risk | One-line reason | Source |
|---|---|---|---|
| clickhouse (GPU plugin) | weak | NVIDIA shows ClickHouse only on Vera CPUs, not GPUs. No NVIDIA GPU ClickHouse work found, though Sirius and HeavyDB already beat it on ClickBench. | [nvidia.com data-processing](https://www.nvidia.com/en-us/solutions/data-processing/); [NVIDIA Sirius blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/) |
| lakehouse-iceberg | strong | NVIDIA owns the Spark plugin (sold with AI Enterprise, plus Aether migration), co-builds the Presto/Velox path with IBM in production, and partners with Starburst and Dell. | [IT Jungle](https://www.itjungle.com/2026/03/23/ibm-taps-nvidia-gpus-for-ai-turbocharged-data-mart/); [Aether](https://blogs.nvidia.com/blog/project-aether-accelerates-apache-spark) |
| realtime-vram-cache | strong | NVIDIA now owns HeavyDB, an in-memory GPU SQL database (the original VRAM-resident analytics design), and open-sourced the full platform in Oct 2026. | [heavyai/heavydb](https://github.com/heavyai/heavydb) |
| streaming-flink | weak (estimate) | No NVIDIA Flink or GPU stream-processing work found. cudf-polars "streaming" means out-of-core batching, not event streams. | [cudf_polars streaming docs](https://docs.rapids.ai/api/cudf/latest/cudf_polars/streaming_execution) |
| datafusion-plugin | medium | No NVIDIA DataFusion work found. NVIDIA's RapidsMPF and cuCascade runtime, plus cudf-polars and Sirius, already cover what a DataFusion-on-cuDF plugin would offer, and NVIDIA could upstream an equivalent quickly. | [NVIDIA/cuCascade](https://github.com/NVIDIA/cuCascade); [cudf_polars API 26.10](https://docs.nvidia.com/cudf/26.10/cudf_polars/api/index.html) |
| realtime-other (VeloDB/Doris/StarRocks) | medium | No NVIDIA engine here, but HeavyDB (free) competes for the same real-time GPU analytics buyer, and a selectdb.com commit in Sirius suggests Doris people may be going via Sirius. | [heavyai/heavydb](https://github.com/heavyai/heavydb); Sirius commits via gh api |
| distributed-pluggable (Databend) | medium | NVIDIA provides the distributed pieces itself (RapidsMPF, UCXExchange, cuCascade) and showcases them with Presto. A third party would be an integrator on NVIDIA parts. | [GTC 2026 write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) |
| spark-plugin | strong | spark-rapids (now "NVIDIA cuDF for Apache Spark") is NVIDIA's own supported product with a migration service. A competing plugin goes head-to-head with NVIDIA. | `gh api repos/NVIDIA/spark-rapids` → NVIDIA/cudf-spark; [BigDataWire](https://bigdatawire.com/2025/03/21/nvidia-pushes-boundaries-of-apache-spark-with-rapids-and-project-aether) |
| other-accelerators (AMD/Cerebras) | medium | No NVIDIA product competition, but AMD's own hipDF already ports cuDF 25.10, and the CUDA EULA translation-layer ban adds legal risk to any shim approach. | [hipDF](https://github.com/AMD-Ecosystem/hipDF); [Tom's Hardware](https://www.tomshardware.com/pc-components/gpus/nvidia-bans-using-translation-layers-for-cuda-software-to-run-on-other-chips-new-restriction-apparently-targets-zluda-and-some-chinese-gpu-makers) |
| vector-sql | strong | cuVS ships inside Milvus, Elastic, OpenSearch, FAISS, Oracle and Weaviate. NVIDIA's AI Data Platform (Dell and others) bundles cuDF and cuVS as a turnkey vector-plus-SQL stack. | [NVIDIA GTC 2026 blog](https://blogs.nvidia.com/blog/gtc-2026-news/) |
| (base) building on Sirius | strong | Sirius is NVIDIA-staffed (top committer and much of the commit volume), NVIDIA calls it "NVIDIA's GPU-native analytics engine", and DuckDB reportedly made it the blessed GPU extension. NVIDIA controls the roadmap. | [sirius-db/sirius](https://github.com/sirius-db/sirius); [GTC write-up](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) |
