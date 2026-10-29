# LLM training-data curation: market size, SQL as an interface, NVIDIA platform risk

Method note: WebSearch hit its session-wide limit after one query, so the rest of this work is direct WebFetch of known primary URLs. Some planned lookups could not be done and are listed under Gaps: HF trained-from-scratch counts, the sovereign-AI project list, Surge/Scale revenue, Snowflake, the Anyscale×NVIDIA blog, cuDF minhash docs (fetch failed), and Daft SQL docs (DNS failure). All sources were accessed 2026-10-29.

## Q1. How many teams curate 0.1–10 TB text corpora?

### Takeaway
The core of the market is a few dozen organisations at most: roughly 12 frontier developers plus perhaps 20–50 open, mid-size and sovereign pretrainers. Frontier labs demonstrably build their dedup and curation in-house. Enterprise fine-tuning corpora are mostly far below 0.1 TB. The exception is enterprise mid-training on 100B tokens or more (for example Thomson Reuters), which is a small but real segment.

### Cited Findings
- Epoch AI counted "over 30 publicly announced AI models from 12 different AI developers" above 1e25 FLOP as of June 2025. The developers are Anthropic, Google DeepMind, OpenAI, Meta, xAI, Mistral, Zhipu, ByteDance, Inflection, NVIDIA, Huawei and Saudi Aramco. Epoch reports about 2 such models per month in 2024 (the table tally gives ~4 in 2023, ~19 in 2024 and ~10 in Jan–May 2025). — [Epoch AI](https://epoch.ai/data-insights/models-over-1e25-flop) (accessed 2026-10-29)
- In-house curation at frontier labs:
  - DeepMind's MassiveText is "2.35 billion documents, or about 10.5 TB of text".
  - It was near-deduplicated with "the MinHash algorithm to compute 13-gram Jaccard similarities", removing one of each pair whose similarity exceeds 0.8.
  - The same 13-gram/0.8 test was used against eval sets for decontamination.
  - The paper names no external tooling, so the pipeline is in-house.
  - — [Gopher paper, Rae et al. 2021](https://arxiv.org/pdf/2112.11446) (accessed 2026-10-29)
- NVIDIA builds its own pretraining data with its own tool:
  - Nemotron-CC is a 6.3T-token English Common Crawl dataset. 7.2T tokens of a 15T-token run came from it.
  - It was built with NeMo Curator: jusText, FastText language ID, exact and fuzzy (MinHash+LSH) dedup, 28 heuristic filters, a KenLM perplexity filter, a 3-classifier quality ensemble and LLM rephrasing.
  - — [NVIDIA tech blog, 2025-05-07](https://developer.nvidia.com/blog/building-nemotron-cc-a-high-quality-trillion-token-dataset-for-llm-pretraining-from-common-crawl-using-nvidia-nemo-curator/); [arXiv 2412.02595](https://arxiv.org/abs/2412.02595) (accessed 2026-10-29)
- Open pretrainers build or open-source their own tooling:
  - HF FineWeb (96 CC snapshots, ~25.9B rows, ODC-BY) was built with HF's datatrove. — [FineWeb card](https://huggingface.co/datasets/HuggingFaceFW/fineweb); [datatrove](https://github.com/huggingface/datatrove) (accessed 2026-10-29)
  - AllenAI's Dolma (~3T tokens) was built with its own Rust/Python toolkit using Bloom-filter dedup. — [allenai/dolma](https://github.com/allenai/dolma) (accessed 2026-10-29)
  - BigCode's The Stack is 3.1 TB of code. — [arXiv 2211.15533](https://arxiv.org/abs/2211.15533) (accessed 2026-10-29)
- Sovereign/national example: Swiss AI Apertus (EPFL/ETH/CSCS, 2025-09-02) has 8B and 70B models trained on ~15T tokens in 1,000+ languages, using more than 10M GPU-hours on the Alps machine. The data pipeline tooling is not named in the release. — [ETH press release](https://ethz.ch/en/news-and-events/eth-news/news/2025/09/press-release-apertus-a-fully-open-transparent-multilingual-language-model.html) (accessed 2026-10-29)
- Small-team pretrainers outsource curation: Arcee's 398B-total/13B-active MoE used "17T tokens curated by Datology", from a team of about 10 people. — [DatologyAI homepage](https://www.datologyai.com/) (accessed 2026-10-29)
- Enterprise mid-training example: Thomson Reuters mid-trained on 100B tokens, under 1% of a 15T pretraining budget, with DatologyAI curation. — [DatologyAI homepage](https://www.datologyai.com/) (accessed 2026-10-29)
- Scale reference for 0.1–10 TB jobs: BigCode deduplicated ~1.4 TB in just under 4 h at ~$15/h on GCP Dataproc Spark. — [HF blog "Large-scale Near-deduplication Behind BigCode"](https://huggingface.co/blog/dedup) (accessed 2026-10-29)

### Inferences
- Frontier labs (about 12) all run in-house pipelines and are not realistic buyers of a curation product. Their scale (more than 10 TB, petabytes of raw crawl) is also above the 0.1–10 TB band.
- The 0.1–10 TB band fits these groups: open or mid-size pretrainers (AllenAI, HF, Zyphra, LLM360, EleutherAI, Arcee, Sarvam, AI Singapore, TII, Swiss AI and similar), code/domain-model builders, and enterprise mid-training runs of 100B–1T tokens (roughly 0.4–4 TB of text at ~4 bytes/token). A plausible count is a few dozen to roughly 100 organisations worldwide. That is an estimate; I found no census.
- Enterprise SFT/RAG fine-tuning corpora are typically GB-scale, below the band. This is unsourced here because no direct citation was found.
- 1 TB ≈ 250B tokens of raw text. A 15T-token final corpus implies roughly 60 TB after filtering, so open/sovereign 15T-token projects (Apertus, Nemotron, FineWeb) are above 10 TB for raw processing.

### Gaps
- No count found of HF models trained from scratch, of open pretraining datasets over 1 TB, or of sovereign AI projects (search budget exhausted).
- No primary source for OpenAI, Anthropic, Mistral or xAI data pipelines. The Llama 3 data section was not fetched.

## Q2. Do teams pay for curation tools?

### Takeaway
DatologyAI is the one clearly funded, curation-focused vendor still on that thesis. It sells a high-touch, in-VPC service and product with no public pricing. Most other "data curation" startups have pivoted or been absorbed: Lilac went to Databricks, Cleanlab to Handshake AI, Nomic to AEC agents and Eventual to physical AI. That pattern is weak evidence of a durable paid market for text-corpus curation tooling.

### Cited Findings
- DatologyAI funding and early customers:
  - $11.65M seed in Feb 2024 led by Amplify Partners; it worked then with "a limited number of customers". — [SiliconANGLE](https://siliconangle.com/2024/02/22/datologyai-raises-11-65m-automate-data-curation-efficient-ai-training) (accessed 2026-10-29)
  - $46M Series A in May 2024 led by Felicis. Total funding is reported as $57.5M or $57.7M. — [AI Business](https://aibusiness.com/data/startup-raises-46m-to-revolutionize-ai-dataset-curation) (403 on fetch; figure from search snippet); [Simplify](https://simplify.jobs/c/DatologyAI) (accessed 2026-10-29)
- DatologyAI product and customers:
  - It positions itself as "the data refinery for AI model builders", with Clean / Calibrate / Create (synthetic) / Compose stages. It deploys "on your private cloud" and publishes no pricing.
  - Named customers are Thomson Reuters, Arcee, Deepgram, HRT and Unconventional AI.
  - — [datologyai.com](https://www.datologyai.com/) (accessed 2026-10-29)
- DatologyAI Curation Studio (launched 2026-10-09):
  - It runs in the customer's VPC with a guided UI and configurable stages. The post never mentions Python, SQL or API access. Pricing is not public and buyers are sent to "talk with a data expert".
  - Claimed users include "several of the Mag 7".
  - The company says it has processed over 23 PiB in 3 years, with more than 1 PB in a single run.
  - — [DatologyAI blog](https://www.datologyai.com/blog/introducing-datology-curation-studio); [blog index](https://www.datologyai.com/blog) (accessed 2026-10-29)
- Usage-based pricing for DatologyAI is mentioned only by a third-party job aggregator. — [Simplify](https://simplify.jobs/c/DatologyAI) (accessed 2026-10-29)
- Eventual (Daft) now sells "data curation for physical AI": video and sensor to actions and trajectories, with Understand/Search/Curate offerings. It does not mention LLM text curation, dedup or SQL on the homepage. Logos shown are Amazon, ByteDance, Together AI and Mobileye. — [eventual.ai](https://www.eventual.ai/) (accessed 2026-10-29)
- Databricks announced on 2024-03-19 that Lilac (text dataset search, cluster and analyse for RAG, fine-tuning and pretraining) was joining Databricks. — [Databricks blog](https://www.databricks.com/blog/lilac-joins-databricks-simplify-unstructured-data-evaluation-generative-ai) (accessed 2026-10-29)
- Cleanlab's homepage is now about AI-agent reliability and guardrails. It "has been acquired by Handshake AI" and shows no curation messaging or pricing. — [cleanlab.ai](https://cleanlab.ai/) (accessed 2026-10-29)
- Nomic's homepage is now an AEC (architecture/engineering/construction) agent platform with $20/month individual plans. Atlas and training-data maps are not mentioned. — [nomic.ai](https://www.nomic.ai/) (accessed 2026-10-29)
- Bodo pitches a CPU+GPU Pandas/Python/SQL engine (Apache-2.0) as "7x faster than Polars on multi-GPU, 3x faster than Dask-CuDF". It has no LLM data-prep messaging and no public pricing. — [bodo.ai](https://www.bodo.ai/) (accessed 2026-10-29)
- NVIDIA AI Enterprise list price is $4,500/GPU for 1 year, $1,125 for EDU/Inception, $22,500 for perpetual with 5 years of support, or $1/GPU-hour in cloud. The pricing page does not list NeMo Curator, and the AIE release-7 infra index points NeMo to "Application Software". — [NVIDIA AIE pricing](https://docs.nvidia.com/ai-enterprise/planning-resource/licensing-guide/latest/pricing.html); [AIE release 7](https://docs.nvidia.com/ai-enterprise/release-7/latest/index.html) (accessed 2026-10-29)

### Inferences
- The paid buyers are small pretrainers (Arcee) and enterprises mid-training on proprietary data (Thomson Reuters, Deepgram). What they buy is curation expertise plus results, packaged as a service, not a raw engine.
- The free baseline is strong: NeMo Curator, datatrove, Data-Juicer and Dolma are all Apache-2.0. A pure tool or engine sale therefore competes with free.

### Gaps
- No public pricing for DatologyAI, Databricks curation features or Anyscale.
- Not verified: whether NeMo Curator itself is covered by NVIDIA AI Enterprise support. The pricing page does not list it, and the application-software page was not fetched.
- No data gathered on Scale/Surge (labelling, not dedup), Snowflake or Anyscale curation offerings (anyscale.com DNS failed). DatologyAI's 2025–26 funding is unknown: the blog lists no funding posts.

## Q3. SQL vs Python among curation practitioners

### Takeaway
Production curation pipelines are overwhelmingly Python: datatrove, NeMo Curator, Data-Juicer YAML+Python, Dolma, and Spark through the DataFrame/ML API. SQL appears mainly for exploration and filtering (the HF SQL Console and DuckDB hf://). SQL-native MinHash exists only in niche spots: ClickHouse built-ins and a 14-star DuckDB extension. I found no evidence of practitioner demand for SQL dedup.

### Cited Findings
- BigCode near-dedup:
  - The setup was MinHash+LSH with 5-gram shingles, 256 permutations and a 0.7 Jaccard threshold.
  - The code was Python (datasketch logic inlined, HF `datasets.map`), with union-find for clustering, and Spark connected components on GCP Dataproc for large sets.
  - The post does not mention SQL, BigQuery or Dataflow.
  - — [HF blog](https://huggingface.co/blog/dedup) (accessed 2026-10-29)
- Spark MLlib `MinHashLSH` / `approxSimilarityJoin` is a DataFrame/ML API (Scala/Java/Python), not a SQL function. This comes from the fetch summary, and the LSH section was past the fetched window, so it is lightly verified. — [Spark ML features](https://spark.apache.org/docs/latest/ml-features.html) (accessed 2026-10-29)
- ClickHouse has SQL built-ins `ngramMinHash`, `wordShingleMinHash` (returning min/max hash tuples) and `ngramSimHash`. — [ClickHouse hash functions](https://clickhouse.com/docs/sql-reference/functions/hash-functions) (accessed 2026-10-29)
- DuckDB has no core MinHash. The community extension `lsh` (princeton-ddss) provides `lsh_min` and `lsh_min32` (n-gram or shingle-list input, band count/size, seed) and has 14 GitHub stars. Other relevant community extensions are hashfuncs, rapidfuzz, tantivy (BM25) and html_readability. — [DuckDB community extensions](https://duckdb.org/community_extensions/list_of_extensions); [princeton-ddss/lsh](https://github.com/princeton-ddss/lsh) (accessed 2026-10-29)
- DuckDB `hf://` paths (v0.10.3, 2024-05-29) give SQL access to more than 150k HF datasets. — [DuckDB blog](https://duckdb.org/2024/05/29/access-150k-plus-datasets-from-hugging-face-with-duckdb.html) (accessed 2026-10-29)
- HF SQL Console (2024-09-17) runs DuckDB-WASM in-browser on every dataset page, with a ~3 GB memory cap and no hf:// support in WASM. Examples are filtering instruction and reasoning datasets; a 12.6M-row filter ran in under 3 s. No usage totals are given. — [HF blog](https://huggingface.co/blog/sql-console) (accessed 2026-10-29)
- datatrove pipelines are Python lists of blocks (reader, filter, dedup, writer) with Local, Slurm, Ray and HF-Jobs executors. It covers MinHash, sentence and ExactSubstr dedup, has ~3.4k stars and is Apache-2.0. — [datatrove](https://github.com/huggingface/datatrove) (accessed 2026-10-29)
- Data-Juicer (Alibaba Tongyi) uses YAML recipes plus Python/CLI, with 200+ ops, a Ray MinHash deduplicator and "70B samples in 2 hours on 50 Ray nodes". It has ~7.2k stars, is Apache-2.0 and has no SQL interface. — [data-juicer](https://github.com/modelscope/data-juicer) (accessed 2026-10-29)
- DatologyAI Curation Studio is UI-plus-config, and Python/SQL/API access is not mentioned. — [DatologyAI blog](https://www.datologyai.com/blog/introducing-datology-curation-studio) (accessed 2026-10-29)

### Inferences
- SQL's real foothold is "look at / slice / filter a dataset" (HF SQL Console, DuckDB). It is not dedup, model-based quality scoring or LLM rephrasing, which make up the bulk of modern pipelines such as Nemotron-CC's classifier ensemble and synthetic rewriting.
- A SQL curation engine would need UDF or model-inference hooks (classifiers, embeddings, LLM calls) to cover modern recipes. Plain SQL covers mainly exact dedup, heuristic filters and joins.

### Gaps
- No forum or GitHub thread asking for SQL MinHash was found (search unavailable).
- Not verified: BigQuery MinHash UDF examples, C4's Beam/TFDS build, and Daft SQL (docs.getdaft.io DNS failure).

## Q4. NVIDIA overlap and platform risk

### Takeaway
NVIDIA is actively investing in NeMo Curator. It has a ~10–12-week release cadence, did a full Dask→Ray rewrite in v1.0 (Oct 2025), adds modalities steadily, runs its own Nemotron data pipeline on it, and keeps it Apache-2.0. It is a free, GPU-accelerated, Python-first incumbent that directly overlaps GPU text dedup and filtering. A shutdown looks unlikely; the risk is commoditisation.

### Cited Findings
- NeMo Curator facts:
  - It is Apache-2.0 and covers text, image, video and audio.
  - Its default executor is XennaExecutor (Cosmos-Xenna), with Ray-based backends.
  - It has 1.8k stars, 1,091 commits and 360 forks.
  - Claimed fuzzy dedup on a RedPajama-v2 subset with 3× H100 nodes: 10.7 h → 0.65 h (~16×) against a CPU alternative, at ~40% lower TCO ($315 → $190), scaling 2.05 → 1.01 → 0.50 h on 1→4 nodes.
  - — [NVIDIA-NeMo/Curator](https://github.com/NVIDIA-NeMo/Curator) (accessed 2026-10-29)
- Release history:
  - v1.0.0 (2025-10-01): "fundamental architecture shift from Dask to Ray", adding video and audio and Xenna / Ray Actor Pool / Ray Data executors.
  - v1.1.0 (2026-02-23): YAML pipeline config and benchmarking.
  - v1.2.0 (2026-05-14): Ray ≥2.54, vLLM default for semantic dedup, InferenceServer on Ray Serve, RayDataExecutor made non-experimental.
  - v1.3.0 (2026-07-27): SlurmRayClient, experimental Dynamo backend, Nemotron-CLIMB recipe.
  - v1.4.0 (2026-10-09): Lance reader/writer, Ray 2.57, RAPIDS 26.08, memory bounds for semantic dedup.
  - — [Curator releases](https://github.com/NVIDIA-NeMo/Curator/releases) (accessed 2026-10-29)
- The Nemotron-CC pipeline was merged into the NeMo Curator repo. Its exact and fuzzy dedup run on RAPIDS (cuDF, cuML, cuGraph), at that time with Dask, and NVIDIA claims "16X faster processing for text". — [NVIDIA blog 2025-05-07](https://developer.nvidia.com/blog/building-nemotron-cc-a-high-quality-trillion-token-dataset-for-llm-pretraining-from-common-crawl-using-nvidia-nemo-curator/) (accessed 2026-10-29)
- NVIDIA is itself a frontier-scale model developer, with a model above 1e25 FLOP. — [Epoch AI](https://epoch.ai/data-insights/models-over-1e25-flop) (accessed 2026-10-29)
- NeMo Curator docs present it as an open-source platform for text, image, video and audio, with GPU-accelerated pipelines. The landing page names no partners. — [NeMo Curator docs](https://docs.nvidia.com/nemo/curator/latest/home/welcome) (accessed 2026-10-29)
- Bodo explicitly benchmarks against Dask-cuDF and multi-GPU Polars, which shows GPU dataframe engines competing in the same space. — [bodo.ai](https://www.bodo.ai/) (accessed 2026-10-29)

### Inferences
- Signals that NVIDIA will keep investing:
  - The project has been under sustained release cadence for over a year.
  - It underwent a major re-architecture.
  - It is dogfooded for Nemotron data.
  - Each release bumps the Ray and RAPIDS versions.
  - It drives GPU demand, which serves NVIDIA's core interest.
- No signal of deprecation was found.
- The overlap is direct for GPU MinHash/LSH fuzzy dedup, exact dedup and heuristic filters on text. A competitor's differentiation would have to be the SQL interface or embedding in a database, or non-NVIDIA hardware. NeMo Curator is Python/YAML only.
- The Ray standardisation (datatrove, Data-Juicer and NeMo Curator all on Ray) makes Ray the de facto execution substrate for curation.

### Gaps
- Not verified: the cuDF `nvtext` minhash API (docs.rapids.ai fetch failed).
- Not found: Anyscale×NVIDIA partnership details, Domino/Nebius partnerships, and whether NeMo Curator is formally supported under NVIDIA AI Enterprise. Search was unavailable for all three.
