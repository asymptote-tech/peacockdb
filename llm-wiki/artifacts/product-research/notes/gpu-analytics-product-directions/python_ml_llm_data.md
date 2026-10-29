# GPU acceleration in Python dataframes, ETL, ML feature engineering, LLM-data pipelines and vector search: product directions for peacockdb

All links accessed 2026-10-28. "(estimate)" marks my own guesses. Desk research only; vendor benchmarks are marked as vendor claims.

## Q1. Python dataframe GPU engines (Polars GPU, cudf.pandas, Dask-cuDF, Ray Data, Daft, Ibis/Narwhals): what exists, what is missing, who adopted, and is there a gap a SQL engine fills?

### Takeaway
NVIDIA has put its dataframe effort into the Polars GPU engine. As of cudf-polars 26.06 (Aug 2026) it has a streaming, spilling, multi-GPU executor, and its SQL API is listed as supported. That closes most of the gap a "GPU SQL over Parquet on a workstation" product would target. What is left: it is still "Open Beta", silent CPU fallback hides poor coverage, there is a steady stream of CPU/GPU result-mismatch bugs, many dtypes are unsupported, and I found no named production adopters. The DuckDB route is covered by Sirius (UW-Madison with NVIDIA engineers). A generic "SQL on Parquet on a GPU workstation" product would compete with two free tools that NVIDIA backs.

### Cited Findings
- On 28 Aug 2026 Polars published an NVIDIA-authored post. It says that from cudf-polars 26.06 the GPU engine runs on a new streaming backend built on RapidsMPF, replacing the Dask executor that was experimental in 25.06. Chunks spill to host memory, so one GPU can process data many times its VRAM. The same executor scales to many GPUs through `RayEngine`, `DaskEngine` and `SPMDEngine`, with shuffles over UCX/MPI — [Polars blog: A new streaming backend for the Polars GPU engine](https://pola.rs/posts/gpu-streaming-backend/) (accessed 2026-10-28)
- Vendor PDS-H/PDS-DS numbers from the same post: at SF1K, one B200 against a dual-socket Xeon took 84.7 s vs 271.9 s (3.2×) on PDS-H and 333.5 s vs 734.0 s (2.2×) on PDS-DS. At SF3K, eight B200s gave 23.2× on PDS-H and 11.0× on PDS-DS. The post says the CPU engine "remains the right choice for much interactive and medium-scale work". It names no customers — [Polars blog](https://pola.rs/posts/gpu-streaming-backend/) (accessed 2026-10-28)
- The engine is still "Open Beta… undergoing rapid development". Unsupported plans "transparently fall back to the standard Polars engine and run on the CPU" — [cuDF docs: Polars GPU engine](https://docs.nvidia.com/cudf/latest/cudf_polars/) (accessed 2026-10-28)
- Documented limitations:
  - Needs Volta (CC 7.0) or newer, CUDA 12/13, and Linux or WSL2.
  - Unsupported: the eager API, time-series resampling, folds, UDFs, and Excel/database formats.
  - Unsupported dtypes: Date, Categorical, Enum, Time, Array, Binary, Object.
  - The guide says "~1 TiB raw fits well on an 80 GiB GPU". Speedups mostly come from group-bys and joins, and I/O-bound queries see little gain.
  - The SQL API is listed as supported.
  - The engine passes 99.2% of Polars unit tests with fallback and 88.8% without it.
  — [Polars user guide: GPU support](https://docs.pola.rs/user-guide/gpu-support/) (accessed 2026-10-28)
- The Polars GitHub tracker shows a steady stream of GPU-engine correctness and coverage issues from 2025–2026, each with few comments. Examples:
  - wrong `group_by().agg(datetime.mean())` — [#25616](https://github.com/pola-rs/polars/issues/25616)
  - wrong `quantile(0.5)` — [#26151](https://github.com/pola-rs/polars/issues/26151)
  - `pl.lit()` incorrect results — [#26152](https://github.com/pola-rs/polars/issues/26152)
  - `is_in` inconsistency — [#26188](https://github.com/pola-rs/polars/issues/26188)
  - GPU rejects `str.slice` with a column offset — [#30187](https://github.com/pola-rs/polars/issues/30187)
  - GPU rejects `dt.offset_by` — [#30193](https://github.com/pola-rs/polars/issues/30193)
  - hive-partitioned scans still open since Jan 2025 — [#20577](https://github.com/pola-rs/polars/issues/20577)
  - a request for "versioned GPU-engine capability boundaries" — [#28931](https://github.com/pola-rs/polars/issues/28931)
  - a request for "cost-aware CPU/GPU subplan execution" — [#29991](https://github.com/pola-rs/polars/issues/29991)
  (all accessed 2026-10-28)
- A third-party PR reports that the GPU engine lags the CPU release line: no cudf-polars build supported polars ≥ 1.43 at the time. Unverified — [MihaiA24/polars-pill PR #1](https://github.com/MihaiA24/polars-pill/pull/1) (accessed 2026-10-28)
- RAPIDS 24.12 added chunked Parquet reads and CUDA Unified Memory for the Polars GPU engine, so workloads that fit in GPU+CPU memory no longer OOM — [NVIDIA blog: RAPIDS 24.12](https://developer.nvidia.com/blog/rapids-24-12-introduces-cudf-on-pypi-cuda-unified-memory-for-polars-and-faster-gnns) (accessed 2026-10-28)
- cudf.pandas known issues: CPU fallback copies data, so pandas mutability semantics are not guaranteed; joins do not preserve pandas row order; some workflows run slower when they mix GPU and CPU work — [cudf.pandas FAQ](https://docs.rapids.ai/api/cudf/latest/cudf_pandas/faq/) (accessed 2026-10-28)
- Sirius is a GPU-native SQL engine shipped as a DuckDB extension. It takes Substrait plans and runs them on libcudf/RMM, and was built at UW-Madison with NVIDIA engineers. The README claims about 5× over DuckDB on TPC-H SF1000 on a DGX Station GB300. NVIDIA reports a record ClickBench result — [sirius-db/sirius](https://github.com/sirius-db/sirius); [NVIDIA blog: Sirius ClickBench record](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/) (accessed 2026-10-28). An NVIDIA page lists a kit to "Use SiriusDB to Accelerate DuckDB by up to 8x" and a Presto-on-GPU kit — [NVIDIA cuDF topic page](https://developer.nvidia.com/topics/ai/data-science/cuda-x-data-science-libraries/cudf?lang=en-US) (accessed 2026-10-28)
- Ray Data and Daft (multimodal and AI data): an Anyscale benchmark found GPU utilisation "less than 20% for both systems", "fundamentally starved by not having enough CPUs". Eventual (Daft) says Spark "struggles to keep GPUs fully utilized" and was OOM-prone. Both benchmarks are vendor-authored — [Anyscale: Benchmarking multimodal AI workloads on Ray Data](https://www.anyscale.com/blog/ray-data-daft-benchmarking-multimodal-ai-workloads); [Eventual: Benchmarks for multimodal AI](https://www.eventual.ai/blog/benchmarks-for-multimodal-ai-workloads) (accessed 2026-10-28). Daft has an as-of / point-in-time join branch — [CodSpeed: Daft asof_join branch](https://codspeed.io/Eventual-Inc/Daft/branches/euanlimzx%3Aeuan%2Fasof_join) (accessed 2026-10-28)
- Voltron Data (sponsor of Ibis and Arrow, and maker of the Theseus GPU SQL engine) changed CEO and laid off about half of roughly 120 staff after missing revenue targets in 2024 — [The Information](https://www.theinformation.com/briefings/ai-startup-voltron-data-switches-ceos-lays-off-staff) (accessed 2026-10-28). The layoffs hit many Arrow maintainers, and QuantStack set up a team to support Arrow — [QuantStack on Medium](https://medium.com/@QuantStack/quantstack-steps-up-to-support-apache-arrow-with-new-dedicated-team-9ddc952f20e2) (accessed 2026-10-28)

### Inferences
- For a peacockdb SQL front end over Polars/pandas users' Parquet on a workstation GPU, the free competitors are cudf-polars (with SQL API, streaming and multi-GPU) and Sirius (DuckDB SQL). Both are funded or staffed by NVIDIA. That is high platform risk with no sign that users are begging for a third option (estimate).
- What peacockdb could offer differently is predictability: SQL coverage that is complete on GPU or fails loudly, instead of silent fallback, plus correctness. The GitHub issues show that GPU/CPU result mismatches are a real but quiet complaint. It is a weak wedge on its own (estimate).
- The low comment counts on GPU issues suggest a small active user base for Polars GPU. That is consistent with finding no named adopters (estimate).

### Gaps
- I found no named production adopters of Polars GPU, cudf.pandas or Dask-cuDF in 2025–2026, only NVIDIA-authored material.
- I could not query the rapidsai/cudf issue tracker. `gh search` returned a permission error, so I have no complaints list from that repo.
- Narwhals and Modin: I found no 2025–2026 evidence on their GPU backends. Ibis maintenance after the Voltron layoffs is unverified.
- DGX Spark and DGX Station uptake for dataframe or SQL work: no data found.

## Q2. LLM data pipelines: pretraining curation, dedup and filtering, synthetic data, eval-log and agent-trace analytics. Do teams use SQL? What are the pains? Is the work GPU-centric?

### Takeaway
Pretraining curation is done with Python pipeline frameworks, not SQL:
- NeMo Curator runs on GPU and Ray.
- Datatrove, Dolma's tooling and Data-Juicer run on CPU.

NVIDIA already owns GPU dedup through NeMo Curator, which offers exact, fuzzy and semantic methods and is free. SQL does appear, but for exploration: Hugging Face runs DuckDB over Parquet for its dataset SQL Console. For eval and agent traces, the incumbents are ClickHouse (which bought Langfuse in Jan 2026) and custom engines (Braintrust Brainstore). The data is nested JSON with full-text search, which is not a GPU-shaped workload.

### Cited Findings
- NeMo Curator offers three dedup methods, all GPU-accelerated: exact (MD5), fuzzy (MinHash + LSH) and semantic (embeddings) — [NeMo Curator docs: Deduplication](https://docs.nvidia.com/nemo/curator/curate-text/process-data/deduplication) (accessed 2026-10-28). Semantic dedup embeds documents, runs K-means, then computes pairwise similarity within each cluster, using cuDF — [NeMo Curator docs: SemDedup](https://docs.nvidia.com/nemo/curator/curate-text/process-data/deduplication/semdedup) (accessed 2026-10-28)
- NeMo Curator 25.09 completed a "fundamental architecture shift from Dask to Ray". The Dask API has been removed. Ray pipelines are strictly linear, with no branches or fan-outs — [NeMo Curator 25.09 release notes](https://docs.nvidia.com/nemo/curator/25.09/about/release-notes/index.html); [Migration FAQ](https://docs.nvidia.com/nemo/curator/about/release-notes/migration-faq) (accessed 2026-10-28)
- NVIDIA claims GPU fuzzy dedup is 20× faster and 5× cheaper than CPU. A 4.5 TB RedPajama run went from 37 h to 3 h (vendor claim, older post) — [NVIDIA blog: Curating trillion-token datasets](https://developer.nvidia.com/blog/curating-trillion-token-datasets-introducing-nemo-data-curator/) (accessed 2026-10-28)
- AI2's Dolma toolkit filters at about 122 CPU-hours per TB. The authors estimate about 5 days for 200 TB raw on a 192-vCPU instance — [Dolma paper, arXiv 2402.00159](https://arxiv.org/pdf/2402.00159) (accessed 2026-10-28)
- FineWeb was built with Datatrove, using per-snapshot MinHash (5-grams, 14 bands × 8 rows). A third-party guide recommends Datatrove on SLURM with 1,000+ CPU cores as the alternative to multi-node NeMo Curator. This is a secondary source and its throughput tables are labelled "estimated" — [Spheron 2026 guide](https://www.spheron.network/blog/ai-pretraining-data-curation-nemo-curator-datatrove-fineweb-gpu-cloud/) (accessed 2026-10-28)
- IBM's Data-Prep-Kit paper describes NeMo Curator as NLP-focused and built on Dask with RAPIDS, and notes that GPUs may be unnecessary for jobs CPUs can handle — [Data-Prep-Kit, arXiv 2409.18164](https://arxiv.org/pdf/2409.18164) (accessed 2026-10-28)
- Zilliz (Jul 2025, updated Jul 2026) describes an unnamed "top-tier LLM provider" deduplicating tens of billions of documents with MinHash-LSH inside Milvus. Its pain points:
  - Semantic dedup was "too expensive at scale".
  - Standalone dedup pipelines caused data transfer and duplicated logic.
  - uint32 hashes stored in float32 lost precision.
  The post does not mention GPUs — [Zilliz blog: Data deduplication at trillion scale](https://zilliz.com/blog/data-deduplication-at-trillion-scale-solve-the-biggest-bottleneck-of-llm-training) (accessed 2026-10-28)
- Hugging Face's dataset SQL Console runs DuckDB-WASM over auto-converted Parquet (the first 5 GB). The DuckDB `hf://` protocol queries Hub datasets from the CLI. This is evidence that LLM-data people do use SQL to explore and slice datasets, at small scale, on CPU — [HF blog: SQL Console](https://huggingface.co/blog/sql-console); [HF docs: DuckDB](https://huggingface.co/docs/hub/en/datasets-duckdb); [HF blog: DuckDB on 50,000+ datasets](https://huggingface.co/blog/hub-duckdb) (accessed 2026-10-28)
- ClickHouse acquired Langfuse, an open-source platform for LLM observability, evals and prompt management, on 16 Jan 2026. Langfuse already ran entirely on ClickHouse — [ClickHouse blog](https://clickhouse.com/blog/clickhouse-acquires-langfuse-open-source-llm-observability); [InfoWorld](https://www.infoworld.com/article/4118621/clickhouse-buys-langfuse-as-data-platforms-race-to-own-the-ai-feedback-loop.html) (accessed 2026-10-28)
- Braintrust built its own database, Brainstore: a Rust streaming engine on object storage with Tantivy full-text search. Its stated reasons: AI traces are "deeply nested, variably structured", need full-text plus analytical queries, and are mutable because scores arrive later. Warehouses lack full-text search and real-time updates, and document databases lack columnar aggregation. It claims sub-second queries on TBs (vendor claim) — [Braintrust: Brainstore](https://braintrust.dev/encyclopedia/brainstore); [Braintrust architecture docs](https://www.braintrust.dev/docs/reference/platform/architecture) (accessed 2026-10-28)
- Voltron Data positions Theseus for AI pre-processing, with GPU UDFs for embeddings and vector search. Accenture Ventures invested in Feb 2025 — [VentureBeat](https://venturebeat.com/ai/voltron-data-just-partnered-with-accenture-to-solve-one-of-ais-biggest-headaches/) (accessed 2026-10-28). Third-party trackers estimate Voltron headcount at about 23–24 in 2026 (unverified) — [Unify headcount insights](https://www.unifygtm.com/insights-headcount/voltron-data) (accessed 2026-10-28)

### Inferences
- Pretraining-scale dedup and filtering is the clearest "GPU already there, data prep on CPU" story in LLM land. But NVIDIA ships the GPU answer (NeMo Curator) for free, and frontier labs build their own. A SQL engine adds little to classifier-heavy filtering, which is model inference (estimate).
- A narrower opening exists: mid-size teams doing fine-tuning, continued pretraining or synthetic data, with 0.1–10 TB of Parquet/JSONL, who want declarative dedup, joins and sampling across many dataset versions. Today they use DuckDB, Polars or ad-hoc Python. Evidence of acute pain is thin: HF's SQL Console shows they reach for SQL, but at small scale (estimate).
- Eval and trace analytics is a crowded, CPU-shaped (JSON plus full-text) incumbent space. ClickHouse/Langfuse and Brainstore already exist, and the ClickHouse GPU plugin is already on the founders' list. I do not recommend it as a separate direction (estimate).

### Gaps
- I found no evidence on how RL-rollout or agent-trace teams analyse rollouts at scale (Parquet+DuckDB? ClickHouse?), or on which pains are GPU-relevant.
- I found no evidence on synthetic-data pipeline analytics or SQL use there.
- I found no independent, published benchmark of NeMo Curator against Datatrove on cost per TB. NVIDIA's Ray benchmarks are "being finalized" according to the migration FAQ.

## Q3. ML feature engineering and training-data prep: feature stores, point-in-time and as-of joins, recsys data prep (NVTabular/Merlin), loading data into GPU training. Is there a named group in pain?

### Takeaway
There are two concrete signals:
1. **Point-in-time and as-of joins** are the expensive core of offline training-set generation in feature stores. KX is building GPU as-of joins for KDB-X and is actively looking for design-partner customers, which shows that finance and time-series users want this on GPU. Tecton was absorbed into Databricks in Aug 2025, which leaves non-Databricks users looking elsewhere.
2. **GPU starvation in training data loading** is well documented. Meta measured stalls costing up to 56% of training throughput, and Anyscale measured under 20% GPU utilisation. NVIDIA's dedicated GPU recsys prep tool (NVTabular/Merlin) appears to have stalled since 2023.

Neither signal names a small team today that would adopt a new engine. Finance quants on kdb+ are the most specific named group.

### Cited Findings
- Tecton's docs describe training-set construction as an AS OF (point-in-time) join and call per-sample point-in-time-correct generation "fairly tricky to implement" — [Tecton docs: Constructing training data](https://docs.tecton.ai/docs/reading-feature-data/reading-feature-data-for-training/constructing-training-data) (accessed 2026-10-28)
- Tecton is joining Databricks (announced Aug 2025) — [Databricks blog](https://www.databricks.com/blog/tecton-joining-databricks-power-real-time-data-personalized-ai-agents); [Tacnode: Tecton alternatives](https://tacnode.io/post/tecton-alternatives) (accessed 2026-10-28)
- A vendor-neutral explainer says that with millions of entity-timestamp pairs joined against billions of feature rows, the as-of join "can take hours". Most feature stores materialise it as a Spark or BigQuery job. This is a secondary source — [apxml: Point-in-time correctness](https://apxml.com/courses/feature-stores-for-ml/chapter-3-data-consistency-quality/point-in-time-correctness) (accessed 2026-10-28)
- Chalk raised a $50M Series A at a $500M valuation (May 2025). Its "Compute Engine… compiles features into high-performance Rust pipelines", and it offers temporally consistent offline recomputation over history — [Business Wire: Chalk Series A](https://www.businesswire.com/news/home/20250528882026/en/Chalk-Raises-$50M-Series-A-to-Power-AI-Inference); [Chalk: Training data](https://chalk.ai/capabilities/training-data) (accessed 2026-10-28)
- KX, 6 Nov 2025: KDB-X GPU offload covers as-of joins (`.gpu.aj`), sorts (on cuDF), GPUDirect Storage load/save and multi-GPU VaR. Vendor numbers: about 4× on an L4 at 1M rows, about 5.5× on an A100 at 10M rows for as-of joins, and about 58× for VaR on five H100s. It is not GA; KX's incubation team is "seeking customers to collaborate", aimed at market-data, trades-to-quotes and end-of-day workflows — [KX blog: GPU acceleration in KDB-X](https://kx.com/?p=14992) (accessed 2026-10-28)
- Meta's DPP paper: "reading and preprocessing data… requires substantially more compute, memory, and network resources than are available on trainers". Data stalls can cut training throughput by 56%. DPP scales to hundreds of separate preprocessing nodes per job — [Zhao et al., arXiv 2108.09373](https://arxiv.org/pdf/2108.09373) (accessed 2026-10-28)
- Anyscale: preprocessing either starves the GPUs or slows "the entire pipeline to CPU speed" — [Anyscale: Architecting multimodal data pipelines](https://www.anyscale.com/blog/architecting-multimodal-data-pipelines-that-scale-with-ray) (accessed 2026-10-28)
- NVTabular's latest listed release is v23.08.00 (2023). It has no formal deprecation notice, and the README still lists Python 3.7+ and CUDA 11.0+ — [NVTabular releases](https://github-redirect.dependabot.com/NVIDIA-Merlin/NVTabular/releases); [NVIDIA-Merlin/NVTabular](https://github.com/NVIDIA-Merlin/NVTabular) (accessed 2026-10-28). A third-party index shows 206 open issues — [ecosyste.ms](https://awesome.ecosyste.ms/projects/github.com%2Fnvidia-merlin%2Fnvtabular) (accessed 2026-10-28)

### Inferences
- NVTabular's stall looks like abandonment in practice, though nothing official says so. Inferred from no release since 2023 and stale toolchain requirements (estimate). Recsys teams that relied on it have nowhere obvious to go except Spark-RAPIDS, Ray Data or in-house code. That is a possible niche: SQL-defined feature transforms executed on the training GPU and handed to PyTorch zero-copy via DLPack. Demand is unproven (estimate).
- GPU as-of / point-in-time join is the most specific operator-level opening:
  - It is a join type cuDF does not obviously expose as a first-class primitive. KX had to build `.gpu.aj` itself, and I could not confirm a cuDF `merge_asof`.
  - It is central to both feature-store training sets and finance tick data.
  - peacockdb already has the plan and join machinery.
  (estimate)
- The kdb+/quant world is the named group, but KX itself is moving into GPU. A peacockdb product would compete with KX, or would have to target non-kdb shops doing tick analytics on Parquet (estimate).

### Gaps
- I could not confirm whether libcudf or cudf-polars supports as-of joins natively on GPU. Search returned nothing definitive; testing with `raise_on_fail=True` would settle it.
- I found no practitioner complaint (Reddit/HN) quantifying point-in-time join cost or time in 2025–2026; only vendor and explainer text.
- Feast and Featureform current status and pains: not researched in depth.

## Q4. GPU ETL and data prep vendors, and complaints about cloud-warehouse cost for ETL

### Takeaway
The independent GPU-ETL vendor landscape has consolidated or shrunk:
- **BlazingSQL**: closed in 2021.
- **Voltron Data / Theseus**: cut about 50% of staff in 2024 and is now about 23 people (estimate from trackers).
- **HEAVY.AI**: acquired by NVIDIA in 2025.
- **DataPelago**: acquired by NetApp in July 2026.
- **SQream**: still independent; no 2026 news found.

Warehouse-cost complaints are real, but the popular fix is moving to DuckDB on CPU, not to GPUs.

### Cited Findings
- **BlazingSQL**: listed as abandoned. The company closed in 2021 and the team joined Voltron Data. No reason is given — [dbdb.io: BlazingSQL](https://dbdb.io/db/blazingsql) (accessed 2026-10-28)
- **Voltron Data**: layoffs and CEO change (2024) — [The Information](https://www.theinformation.com/briefings/ai-startup-voltron-data-switches-ceos-lays-off-staff). Theseus Test Drive launched and Accenture invested in Feb 2025 — [VentureBeat](https://venturebeat.com/ai/voltron-data-just-partnered-with-accenture-to-solve-one-of-ais-biggest-headaches/). The Theseus paper says "most of the hard problems are when, where, and how to move data among GPU, host memory, storage, and the network" — quoted in [iConnect007: Accenture invests in Voltron Data](https://iconnect007.com/article/144193/accenture-invests-in-voltron-data-to-help-organizations-use-gpu-technology-to-simplify-largescale-data-processing/144190/milaero) (all accessed 2026-10-28)
- **HEAVY.AI (HeavyDB)**: acquired by NVIDIA in 2025 — [Wikipedia: HEAVY.AI](https://en.wikipedia.org/wiki/HEAVY.AI); [NEA portfolio](https://www.nea.com/portfolio/heavy-ai). dbdb says the project was abandoned after the acquisition; this is unsourced and conflicts with other pages — [dbdb.io: HeavyDB](https://dbdb.io/db/heavydb/revisions/9) (accessed 2026-10-28)
- **DataPelago**: NetApp acquired it in July 2026. Its Nucleus engine runs on CPU+GPU "at the storage layer". Vendor claims are "up to 80% lower cost, 10× faster". No named customers — [StorageNewsletter](https://www.storagenewsletter.com/2026/07/21/netapp-acquires-datapelago-making-data-ai-ready-at-the-infrastructure-layer/) (accessed 2026-10-28)
- **SQream Blue**: a managed GPU lakehouse for data preparation (denormalisation, pre-aggregation, feature generation) over open files in the customer's own storage — [SQream Blue datasheet](https://info.sqream.com/hubfs/Ask%20bigger%20resources/BLUE%20materials/BLUE%20-%20Data%20sheet%20.pdf). Its last funding was a $45M Series C in 2023 — [VentureBeat](https://venturebeat.com/business/sqream-nabs-45m-to-accelerate-data-analytics-with-gpus) (accessed 2026-10-28)
- **Capgemini**: built the same ETL on Databricks Photon (CPU) and on Spark-RAPIDS (GPU). Run times on Azure were statistically indistinguishable, and RAPIDS saved only about 6% DBU/min. Photon went to production because it was finished a few days sooner. GPU gains were large for CROSS JOIN and negligible for UNION — [NVIDIA blog: GPUs for ETL with Databricks](https://developer.nvidia.com/blog/gpus-for-etl-run-faster-less-costly-workloads-with-nvidia-rapids-accelerator-for-apache-spark-and-databricks/) (accessed 2026-10-28)
- **Warehouse cost and DuckDB**:
  - An r/dataengineering thread (May 2024) on "a 70% cheaper data warehouse" built by moving off Snowflake to DuckDB. Commenters said it is worth it if the Snowflake bill is over $500k a year, and noted that DuckDB is slow on JSON/JSONL — [r/dataengineering mirror](https://redlib.ssps.io/r/dataengineering/comments/1d4dskd/how_we_built_a_70_cheaper_data_warehouse) (accessed 2026-10-28)
  - A 2025 case put DuckDB as a cache in front of Snowflake BI and cut BI spend by 79% — [MotherDuck newsletter, Sep 2025](https://motherduck.com/blog/duckdb-ecosystem-newsletter-september-2025.md) (accessed 2026-10-28)

### Inferences
- In GPU ETL, the established exit for startups is acquisition by an infrastructure vendor (NVIDIA, NetApp), not sales of a product with users. That is relevant to YC framing: an acquirer exists, but the pull from end users is weak (estimate).
- The Capgemini result shows GPU ETL wins are workload-specific: heavy joins win, scan/union-heavy jobs do not. Any GPU ETL pitch needs a workload where joins or aggregations dominate (estimate).
- "Warehouse too expensive" pain is being absorbed by cheap CPU engines (DuckDB, Polars). The GPU pitch has to beat DuckDB-on-a-big-box, not Snowflake (estimate).

### Gaps
- Theseus's 2026 status (GA? customers? pricing) is unconfirmed; no primary 2026 source.
- I found no named SQream Blue customers for 2025–2026.
- Bodo was not researched; it is CPU, per the brief.

## Q5. Vector + SQL: cuVS, Milvus GPU, LanceDB, OpenSearch GPU, similarity joins, embedding dedup. Who needs it?

### Takeaway
GPU in vector land has settled on index build. cuVS CAGRA now powers GPU index builds in OpenSearch, on AWS, and in NetApp Instaclustr, all on separate GPU workers while CPUs serve queries. Embedding dedup at LLM scale uses either NeMo Curator SemDedup (GPU, cluster-then-pairwise) or MinHash in Milvus. Lance/LanceDB is where multimodal training-data teams (Midjourney, Runway, Character.AI) keep embeddings next to data. SQL similarity joins remain niche. This overlaps the founders' existing "vector+SQL with vector joins" direction.

### Cited Findings
- AWS OpenSearch Service offloads vector index builds to GPU workers via cuVS while CPU nodes keep serving — [AWS Big Data blog](https://aws.amazon.com/blogs/big-data/how-gpu-acceleration-builds-billion-scale-vector-indexes-on-amazon-opensearch-service/) (accessed 2026-10-28). NetApp Instaclustr made GPU-node index build GA, claiming up to 6.7× faster total build and 24× faster force-merge (vendor, Sep 2026) — [Instaclustr blog](https://www.instaclustr.com/blog/build-opensearch-vector-indexes-faster-with-netapp-instaclustr-and-nvidia-cuvs/) (accessed 2026-10-28)
- cuVS is NVIDIA's GPU ANN and clustering library and is used optionally by Faiss — [NVIDIA/cuvs](https://github.com/NVIDIA/cuvs); [Faiss](https://github.com/facebookresearch/faiss) (accessed 2026-10-28)
- LanceDB raised a $30M Series A (June 2025, led by Theory Ventures with Databricks Ventures and YC participating) for a "Multimodal Lakehouse" covering search, EDA, feature engineering and training. Midjourney and Character.AI were named as customers in the 2024 round, and Runway participated in the round. It reports over 20M OSS downloads — [LanceDB: Series A](https://www.lancedb.com/blog/series-a-funding); [LanceDB newsletter, June 2025](https://lancedb.substack.com/p/june-2025-30m-series-a-multimodal) (accessed 2026-10-28)
- NeMo Curator SemDedup does embed, K-means, then within-cluster pairwise similarity on GPU. That is effectively a blocked similarity self-join — [NeMo Curator docs: SemDedup](https://docs.nvidia.com/nemo/curator/curate-text/process-data/deduplication/semdedup) (accessed 2026-10-28)
- A research system, FED, does GPU-accelerated dataset dedup — [FED, arXiv 2501.01046](https://arxiv.org/html/2501.01046v1) (accessed 2026-10-28)

### Inferences
- Embedding near-dup removal, and "find training examples similar to these eval items" (decontamination), are natural SQL similarity-join use cases on GPU. They overlap the existing vector+SQL direction rather than being new (estimate).
- The index-build offload niche is already served by NVIDIA plus cloud vendors (estimate).

### Gaps
- I found no evidence of user demand for a SQL similarity-join syntax specifically. No issues or threads were found.
- LanceDB GPU index-build details: only a third-party claim of 5–10×, unverified.

## Candidate product directions

### Takeaway
None of these has strong evidence of "1–2 users who need it badly today". The most specific named demand signals are:
1. KX seeking design partners for GPU as-of joins (finance tick data).
2. The documented GPU starvation in training-data loading, together with NVTabular's stall.
3. Mid-size LLM data teams using DuckDB/SQL to explore datasets, with CPU-hour-heavy dedup.

Generic "GPU SQL for Python users on a workstation" is now squarely NVIDIA's territory (cudf-polars, Sirius).

Format: slug | one-line | evidence anyone needs it | GPU-centred? | rough build size vs yardstick 22.8k (estimate) | platform risk

1. **asof-gpu** | A GPU as-of / point-in-time join and windowed-aggregation SQL engine over Parquet, for (a) offline feature-store training-set generation and (b) tick and quote analytics outside kdb+ | KX building `.gpu.aj` and "seeking customers" ([KX](https://kx.com/?p=14992)); Tecton calls PIT training generation "fairly tricky" ([Tecton docs](https://docs.tecton.ai/docs/reading-feature-data/reading-feature-data-for-training/constructing-training-data)); Tecton absorbed into Databricks, which orphans non-Databricks users ([Databricks](https://www.databricks.com/blog/tecton-joining-databricks-power-real-time-data-personalized-ai-agents)); "can take hours" ([apxml](https://apxml.com/courses/feature-stores-for-ml/chapter-3-data-consistency-quality/point-in-time-correctness)) (all accessed 2026-10-28). No named small user found. | Yes: as-of join plus sort is GPU-friendly, and KX reports 4–10× | 0.3–0.6× (ASOF JOIN in the planner and a cuDF-side kernel built from sort plus search; window aggregations; Feast offline-store adapter) (estimate) | KX (incumbent in finance); Databricks/Tecton and Chalk (feature stores); NVIDIA could add asof to cudf-polars (estimate)

2. **trainer-side-sql-prep** | SQL-defined training-data transforms (filters, joins to dimension tables, categorical encoding, bucketing) executed on the training GPU, streamed into PyTorch via DLPack, to remove CPU data stalls; for recsys and tabular-DL teams | Meta: stalls cut throughput by up to 56% ([arXiv 2108.09373](https://arxiv.org/pdf/2108.09373)); GPUs under 20% utilised in Ray Data and Daft benchmarks ([Anyscale](https://www.anyscale.com/blog/ray-data-daft-benchmarking-multimodal-ai-workloads)); NVTabular's last release 23.08 ([releases](https://github-redirect.dependabot.com/NVIDIA-Merlin/NVTabular/releases)) (accessed 2026-10-28). No named team found. | Yes, strongly: it uses the GPU the customer already pays for | 0.4–0.8× (iterator/dataloader API, DLPack hand-off, shuffling and sharding, categorical-encoding ops) (estimate) | NVIDIA (could revive Merlin or push cudf-polars into DataLoader); Ray Data / Anyscale; Daft (estimate)

3. **llm-corpus-sql** | A GPU SQL engine with built-in dedup operators (exact hash, MinHash/LSH banding, n-gram decontamination against eval sets) and fast JSONL/Parquet scans, for mid-size teams curating 0.1–10 TB of fine-tuning, continued-pretraining or synthetic data who think in SQL rather than in NeMo Curator stages | HF built a DuckDB SQL Console because people query datasets in SQL ([HF](https://huggingface.co/blog/sql-console)); Dolma at 122 CPU-h/TB ([arXiv 2402.00159](https://arxiv.org/pdf/2402.00159)); a Zilliz customer found standalone dedup pipelines costly ([Zilliz](https://zilliz.com/blog/data-deduplication-at-trillion-scale-solve-the-biggest-bottleneck-of-llm-training)); DuckDB "slow on JSON/JSONL" ([r/dataengineering](https://redlib.ssps.io/r/dataengineering/comments/1d4dskd/how_we_built_a_70_cheaper_data_warehouse)) (accessed 2026-10-28). No named small team in pain found. | Yes: hashing, string ops and shuffles on GPU; NVIDIA's own claim is 20× for fuzzy dedup | 0.4–0.7× (JSONL scan, string/hash UDF-like functions, LSH-band self-join, sampling) (estimate) | High: NVIDIA NeMo Curator is free and GPU-native; Datatrove/HF on CPU; DuckDB (estimate)

4. **polars-gpu-sql-strict** | A drop-in GPU SQL engine for Polars/pandas users' Parquet on a single workstation (DGX Station/Spark, RTX Pro), positioned on "full SQL on GPU or a loud error, with results identical to CPU" | Only indirect evidence: Polars GPU issues show CPU/GPU mismatches and unsupported ops ([#25616](https://github.com/pola-rs/polars/issues/25616), [#26151](https://github.com/pola-rs/polars/issues/26151), [#29991](https://github.com/pola-rs/polars/issues/29991)); the engine is still beta ([cuDF docs](https://docs.nvidia.com/cudf/latest/cudf_polars/)) (accessed 2026-10-28). No users asking. | Yes | 0.2–0.4× (Python package, Arrow in/out, Narwhals/Ibis backend) (estimate) | Very high: NVIDIA-funded cudf-polars (with SQL API, streaming and multi-GPU) and Sirius/DuckDB. Overlaps the "DataFusion plugin" direction (estimate)

5. **multimodal-lake-sql** | GPU SQL over Lance/Parquet tables with embedding columns: filter, join, similarity-join, dedup and sample for multimodal training-set assembly | LanceDB's multimodal lakehouse customers (Midjourney, Character.AI) and its $30M raise ([LanceDB](https://www.lancedb.com/blog/series-a-funding)); NeMo SemDedup is a blocked similarity self-join ([docs](https://docs.nvidia.com/nemo/curator/curate-text/process-data/deduplication/semdedup)) (accessed 2026-10-28) | Yes | 0.5–1.0× (Lance reader, vector columns, cuVS-based kNN join) (estimate) | LanceDB (host and incumbent), NVIDIA. Heavy overlap with the existing "vector+SQL with vector joins" direction (estimate)

6. **trace-analytics-gpu** (not recommended) | GPU engine for LLM eval and agent-trace analytics | ClickHouse bought Langfuse ([ClickHouse](https://clickhouse.com/blog/clickhouse-acquires-langfuse-open-source-llm-observability)); Braintrust built Brainstore because warehouses fail on nested, mutable, full-text traces ([Braintrust](https://braintrust.dev/encyclopedia/brainstore)) (accessed 2026-10-28) | Weak: the workload is JSON, full-text and mutation, not GPU-shaped | 1–2× (estimate) | ClickHouse/Langfuse, Braintrust, Datadog. Overlaps the "ClickHouse GPU plugin" direction (estimate)

Notes on the listed directions this research touched:
- Lakehouse and storage-side GPU ETL: DataPelago has now been absorbed by NetApp ([StorageNewsletter](https://www.storagenewsletter.com/2026/07/21/netapp-acquires-datapelago-making-data-ai-ready-at-the-infrastructure-layer/), accessed 2026-10-28).
- Spark plugin: NVIDIA's own Spark-RAPIDS already serves this, with mixed customer outcomes (Capgemini chose Photon).
- DuckDB GPU: Sirius occupies it.
