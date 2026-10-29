# LLM training-corpus curation tools (0.1–10 TB) and user complaints

All URLs accessed 2026-10-29 unless stated otherwise. GitHub issue data came from the `gh` CLI on 2026-10-29. Each issue line gives: number, open date, author handle.
Method note: the session's WebSearch budget ran out partway through. Reddit, HN, Bodo, DuckDB, Polars, Lance, Cleanlab, Snorkel, Scale and Nomic were therefore **not researched** (see Gaps). GitHub was also flaky (connection resets), so some issue bodies are missing.

## Q1. NVIDIA NeMo Curator: version, architecture, GPU stages, speedups, issues, users

### Takeaway
Curator v1.4.0 (2026-10-09) is a Ray-based (Cosmos-Xenna executor), RAPIDS-accelerated pipeline framework. It replaced the earlier Dask/dask-cuda stack, starting with v1.0.0 in October 2025. NVIDIA's headline claim is "~16× faster fuzzy dedup" on an 8 TB RedPajama-v2 subset. The issue tracker shows the same complaints year after year:
- GPU-memory OOMs in the LSH, shuffle and removal stages, even on 5–64 GPUs.
- Host-RAM OOMs on 1.2–1.4 TB nodes.
- Examples that don't run out of the box after the Ray migration.
- Dependency conflicts when installing.

### Cited Findings
**Version / releases**
- Releases (from `gh release list`, accessed 2026-10-29):
  - v1.4.0: 2026-10-09 (Latest)
  - v1.3.0: 2026-07-27
  - v1.2.0: 2026-05-14
  - v1.1.0: 2026-02-23
  - v1.0.0: 2025-10-01
  - v0.9.0: 2025-07-28
  - v0.8.0: 2025-05-09
  - Source: [GitHub releases](https://github.com/NVIDIA-NeMo/Curator/releases)
- Tag v1.4.0 points to commit `166c1c676af8360f2e3f151e9482a001da326ea1`, from a local shallow clone. — [repo](https://github.com/NVIDIA-NeMo/Curator/tree/v1.4.0) (accessed 2026-10-29)

**Architecture**
- README at v1.4.0 says: "NeMo Curator leverages NVIDIA RAPIDS™ (cuDF, cuML, cuGraph) with Ray to scale across multi-node, multi-GPU environments." — [README@v1.4.0](https://github.com/NVIDIA-NeMo/Curator/blob/v1.4.0/README.md) (accessed 2026-10-29)
- README news items (same source):
  - "2026-02 — NeMo Curator 26.02: Ray-based pipeline architecture for all modalities — text, image, video, and audio."
  - "2026-04 — 26.04: Cosmos-Xenna 0.2.0 upgrade, simplified Resources API, Ray runtime upgrade."
- README on executors (same source): "the XennaExecutor (Cosmos-Xenna) is the production default, with experimental Ray-based backends also available."
- Maintainers on executor choice: "users should use Xenna always, unless they want to toy around with Ray Data. And shouldn't really consider RayActorPoolExecutor since it serializes objects to memory". @praateekmahajan, issue #1169 (2025-10). — [#1169](https://github.com/NVIDIA-NeMo/Curator/issues/1169) (accessed 2026-10-29)
- Dask history:
  - Dec 2024: #471 shows the fuzzy-dedup stack failing inside `distributed.worker` / `dask_serialize_cudf_object`, i.e. the Dask + cuDF era. — [#471](https://github.com/NVIDIA-NeMo/Curator/issues/471) (accessed 2026-10-29)
  - @sarahyurick in #101: "we are currently moving Curator away from Dask in favor of Ray", with performance given as "our main motivation for shifting to Ray". — [#101](https://github.com/NVIDIA-NeMo/Curator/issues/101) (accessed 2026-10-29)
- Why no CPU fuzzy dedup: @ayushdg in #101 said: "we use special kernels for GPU dedup (GPU minhash, GPU shuffling/transfers, GPU connected components)". The CPU-version request, #101 (opened 2024-06-07 by @simplew2011), is still open. — [#101](https://github.com/NVIDIA-NeMo/Curator/issues/101) (accessed 2026-10-29)

**Published speedups (sources conflict; flag this)**
- README@v1.4.0 table, citing the scaling docs:
  - Fuzzy dedup on a RedPajama v2 subset, 3× H100 80 GB nodes vs a "CPU-based alternative": 10.7 h → 0.65 h (~16×).
  - Total cost of ownership: $315 → $190 (~40% lower).
  - GPU scaling 1→2→4 nodes: 2.05 h → 1.01 h → 0.50 h.
  - Source: [README@v1.4.0](https://github.com/NVIDIA-NeMo/Curator/blob/v1.4.0/README.md) (accessed 2026-10-29)
- The throughput docs page gives only the 1/2/4× H100 node scaling. It names the dataset as "8 TB RedPajama v2 dataset (1.78 trillion tokens)" and does not state the CPU baseline hardware, the costs or the software versions. — [Curator throughput docs](https://docs.nvidia.com/nemo/curator/latest/about/concepts/scaling/throughput) (accessed 2026-10-29)
- An NGC catalog listing reportedly says the 16× is vs "alternative libraries", with numbers from v25.07. This was seen in a search snippet only, not verified. — [NGC 26.07](https://catalog.ngc.nvidia.com/orgs/nvidia/-/containers/nemo-curator/26.07) (accessed 2026-10-29)
- Third-party "skill" pages repeat a different set of figures: 8 TB RPv2, ~120 h CPU vs ~7.5 h GPU, with inconsistent hardware descriptions (256 vs 16 cores, 8× A100). Treat as unreliable. — [hermes-agent docs](https://hermes-agent.nousresearch.com/docs/user-guide/skills/optional/mlops/mlops-nemo-curator) (accessed 2026-10-29)
- Original August 2023 NVIDIA blog (authors Jennings, Patwary, Dattagupta, Catanzaro et al.):
  - Dedup of 4.5 TB of RedPajama: 37 h on 20 CPU nodes (48 cores, 188 GB each) vs 3 h on 4 DGX A100 nodes (8×80 GB).
  - The blog calls GPU dedup "20× faster and 5× cheaper".
  - NVIDIA's own 8.7 TB corpus was processed on >6,000 CPUs (2T tokens, 43B model).
  - Source: [NVIDIA blog 2023-08-08](https://developer.nvidia.com/blog/curating-trillion-token-datasets-introducing-nemo-data-curator/) (accessed 2026-10-29)
- Independent academic comparison (FED, renamed "SEDD" in v4; Son, Kim, Lee, Seoul National University; arXiv v4 dated 2026-05-18; KDD 2026):
  - NeMo Curator on 100 GB, 1 node with 4× V100: ~1,049 s (RealNews), ~1,436 s (C4 sample).
  - The authors say it "remain[s] suboptimal due to high communication overhead", citing physical shuffling and low GPU utilization.
  - Their own system is up to 7.8× faster than Curator and up to 158× faster than a datasketch CPU baseline.
  - It processes RedPajama-1T (1.2T tokens) in 10,458 s on 8 nodes × 4 V100.
  - CPU MinHash generation on RealNews: ~18,400 s vs 49 s.
  - Source: [arXiv 2501.01046](https://arxiv.org/html/2501.01046) (accessed 2026-10-29)

**Issue tracker: complaints (date = opened)**
- #59, 2024-05-09, @vipulraheja: GPU MinHash/bucket steps "crashing non-deterministically"; "multiple processes that get spawned on one GPU, which eventually goes OOM, while others stay idle". Root cause was importing spaCy before cluster creation. — [#59](https://github.com/NVIDIA-NeMo/Curator/issues/59)
- #112, 2024-06-13, @simplew2011: "exact_deduplication.py out_of_memory". — [#112](https://github.com/NVIDIA-NeMo/Curator/issues/112)
- #142, 2024-07-08, @yyu22: "Running into OOM with add id". — [#142](https://github.com/NVIDIA-NeMo/Curator/issues/142)
- #295, 2024-10-11, @pappagari: "Unmanaged memory is high and frozen execution". — [#295](https://github.com/NVIDIA-NeMo/Curator/issues/295)
- #283, 2024-10-08, @praateekmahajan: "Semantic Dedup doesn't work with UCX". — [#283](https://github.com/NVIDIA-NeMo/Curator/issues/283)
- #471, 2025-01-07, @chenrui17: fuzzy_dedup OOM on 5× A100 with 8 TB.
  - Maintainer @ayushdg: for 8 TB on 5 A100s (~400 GB) "the memory requirements to hold intermediates during stages like LSH might lead to OOM's", and "Internally we've typically used 16-24 GPUs for processing data at this scale".
  - @HuaYZhao: "parquet files of around 15 to 20 T on 64 H800 … also throws an oom error".
  - Closed by stale-bot, unresolved.
  - Source: [#471](https://github.com/NVIDIA-NeMo/Curator/issues/471)
- #529, 2025-02-07, @praateekmahajan: "Exact / Fuzzy Duplicate Removal Improvements at Scale" (maintainer epic). — [#529](https://github.com/NVIDIA-NeMo/Curator/issues/529)
- #723, 2025-06-09, @ronjer30: "Fuzzy Duplicate Removal fails at scale".
  - The removal join OOMs on a 24 TB dataset, and also at 6 TB, even though the right side (~58 GB) is smaller than H100 memory (80 GB) and host RAM (2 TB).
  - Lowering the RMM pool size didn't help.
  - Maintainer reply: migrate to the Ray version and re-run identification.
  - Source: [#723](https://github.com/NVIDIA-NeMo/Curator/issues/723)
- #722, 2025-06-06, @federico-dambrosio: "SemDedup fails at creating embeddings". — [#722](https://github.com/NVIDIA-NeMo/Curator/issues/722)
- #1169, 2025-10-07, @federico-dambrosio: OOM of the driver process (128 CPUs, 1.4 TB RAM) on ~2k × 128 MB parquet files of FinePDFs with RayActorPoolExecutor. He also complained about missing best-practice docs. — [#1169](https://github.com/NVIDIA-NeMo/Curator/issues/1169)
- #1188, 2025-10-20, @jrbourbeau: "Error running on node with multiple GPU types". — [#1188](https://github.com/NVIDIA-NeMo/Curator/issues/1188)
- #1216, 2025-11-03, @msiraga: "`create_id_generator_actor` calls `ray.shutdown()`, breaking FuzzyDeduplicationWorkflow". — [#1216](https://github.com/NVIDIA-NeMo/Curator/issues/1216)
- #1228, 2025-11-09, @jmcmanus15: "FuzzyDeduplicationWorkflow crashes due to multiple bugs". Reproduced with the official 25.09 container plus the documented example; "You have to start the ray client before running the workflow (which isn't described in the docs…)". — [#1228](https://github.com/NVIDIA-NeMo/Curator/issues/1228)
- #1292, 2025-12-07, @yuchaoran2011: "v1.0.0 Fuzzy dedup code doesn't run out of the box" (`ray.get_actor` failure).
  - @ayushdg: it "requires a cluster to be started using the RayClient API … We're working on updating the example code".
  - Source: [#1292](https://github.com/NVIDIA-NeMo/Curator/issues/1292)
- #1295, 2025-12-12, @kipraveen: "Ray temp directory memory leak" (open). — [#1295](https://github.com/NVIDIA-NeMo/Curator/issues/1295)
- Install pain (all open):
  - #1326, 2025-12-17, @pablo-garay: "Installing all optional dependencies together results in dependency conflict" (epic). — [#1326](https://github.com/NVIDIA-NeMo/Curator/issues/1326)
  - #1333: "flash-attn build issue: ModuleNotFoundError: No module named 'torch'". — [#1333](https://github.com/NVIDIA-NeMo/Curator/issues/1333)
  - #1340: "nemo_toolkit[asr] requires transformers<4.55 vllm requires transformers>=4.55". — [#1340](https://github.com/NVIDIA-NeMo/Curator/issues/1340)
- #1332, 2025-12-19, @ayushdg: "Support batch sizes for shuffle stages/dedup stages" (open). — [#1332](https://github.com/NVIDIA-NeMo/Curator/issues/1332)
- #1343, 2025-12-23, @Chyroprase: "ID Generator creation is non blocking and can cause failure for large files". — [#1343](https://github.com/NVIDIA-NeMo/Curator/issues/1343)
- #1443, 2026-01-30, @haojiang-scitix (comment thread addresses @jianghao999): "OOMKilled happens … all the time" on a plain AddId pipeline over 30k JSONL files (<512 MB each), on 80-CPU / 1.2 TB-RAM workers (open). — [#1443](https://github.com/NVIDIA-NeMo/Curator/issues/1443)
- #1465, 2026-02-05, @ayushdg: "Support batchsize/process_batch in the minHash stage" (open). — [#1465](https://github.com/NVIDIA-NeMo/Curator/issues/1465)
- #1518, 2026-02-18, @abdr17: "Please add multinode tutorial for fuzzy deduplication". — [#1518](https://github.com/NVIDIA-NeMo/Curator/issues/1518)
- #1533, 2026-02-20, @ayushdg: "Justext extraction runs into OOM issues due to memory fragmentation or leaks". — [#1533](https://github.com/NVIDIA-NeMo/Curator/issues/1533)
- #1568, 2026-03-03, @ayushdg: "Speed up file partitioning stage (for cloud IO)" (open). — [#1568](https://github.com/NVIDIA-NeMo/Curator/issues/1568)
- #1618, 2026-03-17, @praateekmahajan: "RayDataExecutor: start GPU actor pools at full concurrency instead of scaling up one-by-one" (open). This is an idle-GPU symptom. — [#1618](https://github.com/NVIDIA-NeMo/Curator/issues/1618)
- #1672, 2026-03-26, @abhinavg4: "vLLM worker crashes (SIGSEGV) during initialization on multi-node Ray clusters (8+ nodes)" (open). — [#1672](https://github.com/NVIDIA-NeMo/Curator/issues/1672)
- #1758, 2026-04-07, @arhamm1: "Scale-Dependent Semantic Deduplication". — [#1758](https://github.com/NVIDIA-NeMo/Curator/issues/1758)

**Public users**
- Nemotron-CC:
  - 6.3T tokens (4.4T globally deduplicated real tokens plus 1.9T synthetic), built with Curator.
  - The pipeline has been merged into the Curator repo.
  - Synthetic rephrasing used Mistral NeMo 12B with FP8 TensorRT-LLM.
  - Sources: [NVIDIA blog: Building Nemotron-CC](https://developer.nvidia.com/blog/building-nemotron-cc-a-high-quality-trillion-token-dataset-for-llm-pretraining-from-common-crawl-using-nvidia-nemo-curator); [arXiv 2412.02595](https://arxiv.org/html/2412.02595v1) (accessed 2026-10-29)
- External users visible in issues:
  - Scitix (@haojiang-scitix, #1443).
  - A FinePDFs-subset user (@federico-dambrosio, #1169).
  - A 64× H800 user (@HuaYZhao, #471).
  - Sources: issues above.

### Inferences
- The scale of hardware involved: NVIDIA's own advice is 16–24 A100s for 8 TB of fuzzy dedup. Users run 1.2–1.4 TB-RAM CPU nodes and still hit OOMs in non-GPU stages (AddId, the Ray driver). The 0.1–10 TB range is exactly where Curator's memory-hungry shuffle and LSH design bites.
- Much of the reported pain is operational rather than algorithmic:
  - Cluster bootstrap that isn't documented.
  - Executor choice.
  - Batch-size knobs that are still open feature requests (#1332, #1465).
  - Dependency conflicts.
- The "16×" figure has at least three inconsistent provenances: 3 nodes vs 1 node, an unspecified CPU baseline, and the 2023 "20×" claim at 4.5 TB.

### Gaps
- The exact CPU-baseline configuration behind the 10.7 h figure isn't in the docs page I fetched.
- Bodies for #1228 and the later comments on several issues were partly unavailable because GitHub API calls kept resetting.
- No public list of non-NVIDIA production users was found.

## Q2. Hugging Face datatrove (FineWeb)

### Takeaway
Datatrove is CPU-only for dedup: the dedup code has no CUDA paths at v0.10.1. It ships local, Slurm and Ray executors. FineWeb's MinHash config is 5-grams with 112 hashes (14 bands × 8), deduplicated per dump. The FineWeb paper gives GPU hours only for ablations and the classifier, not CPU hours for dedup. Issues complain about stage-2 slowness, hangs and memory overhead at 0.75–8 TB.

### Cited Findings
- Releases:
  - v0.10.1: 2026-09-30 (latest)
  - v0.10.0: 2026-08-13
  - v0.9.0: 2026-03-04
  - Tag v0.10.1 points to commit `4c488a135cc582b482df81895e631aadff2fa4c1`.
  - Executors in `src/datatrove/executor/`: base, local, slurm, ray, jobs.
  - Source: [datatrove v0.10.1](https://github.com/huggingface/datatrove/tree/v0.10.1) (accessed 2026-10-29)
- Grepping `src/datatrove/pipeline/dedup/*.py` for "cuda|gpu" at v0.10.1 finds no matches. — [dedup dir](https://github.com/huggingface/datatrove/tree/v0.10.1/src/datatrove/pipeline/dedup) (accessed 2026-10-29)
- FineWeb MinHash setup and compute (FineWeb paper):
  - "computed MinHashes using 112 hash functions in total, split into 14 buckets of 8 hashes each", on 5-grams.
  - Global dedup across 96 snapshots was tried and rejected; "individually deduplicating each snapshot" was adopted.
  - It "scales efficiently to many CPU nodes" but no CPU-hour figure is given.
  - Ablations took "an estimated total of 80,000 H100 GPU hours".
  - The FineWeb-Edu classifier over 15T tokens "required 6,000 H100 GPU hours".
  - Source: [arXiv 2406.17557](https://arxiv.org/html/2406.17557) (accessed 2026-10-29)
- FineWeb2 reused the FineWeb MinHash settings and ran global dedup per language. Seen in a search snippet only. — [arXiv 2506.20920](https://arxiv.org/pdf/2506.20920) (accessed 2026-10-29)
- Issues:
  - #106, 2024-02-26, @jordane95: "MinhashDedupCluster runs too slow". — [#106](https://github.com/huggingface/datatrove/issues/106)
  - #161, 2024-04-24, @jordane95: "Memory overhead in multiprocessing". The fastText model is copied per process. In the same thread:
    - @justHungryMan: a token-count job on ~2 TB JSONL "gets stuck every time", with memory filling up.
    - @SinclairCoder (2024-08-26): "unexpected OOM resulting in failed jobs", suspecting long-context docs.
    - Source: [#161](https://github.com/huggingface/datatrove/issues/161)
  - #204, 2024-05-31, @justHungryMan: "Memory overflow issue with long-context data". — [#204](https://github.com/huggingface/datatrove/issues/204)
  - #222, 2024-06-17, @dourofficer: deduplicating ~750 GB (750 × 1 GB JSONL) on a 1 TB-RAM, 256-core machine.
    - "my machine hanged for more than 24 hours. I couldn't even Ctrl+C … had to reboot".
    - Stage 1 took 6 h.
    - Maintainer @guipenedo suggested `tasks=num_buckets*50`.
    - Source: [#222](https://github.com/huggingface/datatrove/issues/222)
  - #238, 2024-07-05, @stas00: "datatrove fails to handle tasks >1k with slurm job arrays". — [#238](https://github.com/huggingface/datatrove/issues/238)
  - #270, 2024-08-19, @hiennm15: "Filter very slow". — [#270](https://github.com/huggingface/datatrove/issues/270)
  - #298, 2024-10-17, @Maghoumi: "Unexpected performance degradation behavior in minhash deduplication stage 2".
    - Benchmarks on 4 TB and 8 TB inputs on Slurm, with 1200/2400 stage-1 tasks and 300/600 stage-2 tasks.
    - @guipenedo: stage 2 depends on the number of files from step 1 (priority-queue merge).
    - Source: [#298](https://github.com/huggingface/datatrove/issues/298)
  - #335, 2025-01-30, @parkwonjae: "too many open files in MinhashDedupBuckets". — [#335](https://github.com/huggingface/datatrove/issues/335)
  - #347, 2025-02-20, @klarakaleb: "memory leak on linux". — [#347](https://github.com/huggingface/datatrove/issues/347)
  - #198, 2024-05-28, @Apsod: "Minhash mersenne hashing overflow issues". — [#198](https://github.com/huggingface/datatrove/issues/198)
- A community benchmark (fastdedup, HF forum) on FineWeb sample-10BT: datatrove stage 1 alone ran >3 h 50 m on one machine before being stopped. The bottleneck was spaCy word tokenization, with a single task, which the author noted is not the intended configuration. — [HF forum](https://discuss.huggingface.co/t/fastdedup-rust-based-dataset-deduplication-benchmarks-on-fineweb-sample-10bt/173672) (accessed 2026-10-29)
- Comparable CPU cost figure, not FineWeb: global Bloom-filter dedup over 1.4T tokens "requires 1.5TB RAM memory and 933 CPU hours", and "MinHash … requires even more". Seen in a search snippet; the exact source paper was not identified. — [search result set incl. arXiv 2609.07699](https://arxiv.org/pdf/2609.07699) (accessed 2026-10-29; attribution uncertain)

### Inferences
- At 0.75 TB, a single 256-core / 1 TB box can take 6 h for stage 1 and then hang in stage 2 (#222). Stage 2, the bucket merge, is the serial bottleneck.

### Gaps
- No published CPU core-hour figure for FineWeb dedup was found.

## Q3. Other OSS tools: Dolma, RedPajama, DCLM, Data-Juicer, Ray Data, Daft, Spark/text-dedup, Bodo, DuckDB, Polars, Lance

### Takeaway
Almost everything is CPU-based. Dolma and DCLM rely on Rust Bloom-filter tools. Data-Juicer is Ray-based with CPU MinHash; it reports 5 TB in 2.8 h on 1280 cores and has recently added GPU actor planning for model ops. DCLM's README admits its Rust dedup can't be integrated with its Ray pipeline. text-dedup no longer ships its Spark script at HEAD.

### Cited Findings
- **Dolma toolkit** (AllenAI):
  - README feature: "Fast Deduplication: Speedy document deduplication using a Rust Bloom filter."
  - HEAD `669f534823b0` (2025-09-18).
  - Source: [allenai/dolma README](https://github.com/allenai/dolma/blob/669f534823b0/README.md) (accessed 2026-10-29)
  - No dedup-memory issues were returned by `gh search`.
- **DCLM** (HEAD `361714bdd60b`, 2025-09-08):
  - README: "Our most efficient (and recommended) deduplication implementations were written in Rust … In DCLM-Baseline, we specifically use BFF (dedup/bff)" and "Unfortunately, as of now, these rust implementations are not integrable with the ray-based pipelines".
  - Ray is used for local filters.
  - For tokenize-shuffle, the README advises copying S3 data locally because its S3 path "is sometimes unreliable", and says the job needs "enough disk space to hold 2X the entire dataset".
  - Slurm support for Ray is "still a work-in-progress".
  - Source: [mlfoundations/dclm README](https://github.com/mlfoundations/dclm/blob/361714bdd60b/README.md) (accessed 2026-10-29)
- **Data-Juicer** (Alibaba / ModelScope; HEAD `016e76050297`, 2026-10-29):
  - README claims: "Process 70B samples in 2h on 50 Ray nodes (6400 cores)", "Deduplicate 5TB in 2.8h using 1280 cores", and "Automatic OP fusion (2-10x speedup)".
  - v1.6.1 (2026-10-19) adds:
    - "Throughput-Aware GPU Actor Planning".
    - "Adaptive GPU Microbatching", which retries rows "in smaller microbatches after an OOM".
    - "Leaner Ray MinHash Dedup", which moves edge buffers into packed NumPy arrays "reducing memory overhead".
  - Source: [data-juicer README](https://github.com/modelscope/data-juicer/blob/016e76050297/README.md) (accessed 2026-10-29)
  - The Ray docs describe the BTS union-find MinHash dedup: "terabyte-sized datasets on 8 nodes with 1280 CPU cores in 3 hours". — [Ray docs: Data-Juicer](https://docs.ray.io/en/latest/ray-more-libs/data_juicer_distributed_data_processing.html) (accessed 2026-10-29)
  - Paper: [Data-Juicer 2.0, arXiv 2501.14755](https://arxiv.org/html/2501.14755v1) (accessed 2026-10-29)
- **text-dedup** (ChenghaoMou):
  - HEAD `7538f3ec6a28` (2026-03-09) ships `minhash.py`, `simhash.py`, `suffix_array.py` and `bloom_filter.py`, with no Spark module.
  - The benchmarks are small: MinHash 11.09 s on pinecone/core and 3.01 s on NEWS-COPY.
  - Source: [text-dedup README](https://github.com/ChenghaoMou/text-dedup/blob/7538f3ec6a28/README.md) (accessed 2026-10-29)
  - Spark-era issues: [#71](https://github.com/ChenghaoMou/text-dedup/issues/71) (2023-10-27, @prikmm: "Consistently seeing more rows being dropped in minhash_spark.py compared to minhash.py").
  - [#80](https://github.com/ChenghaoMou/text-dedup/issues/80) (2024-02-07, @Jason3900: max_iteration for small/large-star connected components).
  - [#85](https://github.com/ChenghaoMou/text-dedup/issues/85) (2024-04-18, @Yang-QW: `[UNABLE_TO_INFER_SCHEMA]`).
  - [#88](https://github.com/ChenghaoMou/text-dedup/issues/88) (2024-05-17, @cjmp1: "Deduplication results vary depending on whether Spark is used or not").
  - [#92](https://github.com/ChenghaoMou/text-dedup/issues/92) (2024-06-18, @alielfilali01: "Run MinHash dedup on Multi-Nodes").
  - [#81](https://github.com/ChenghaoMou/text-dedup/issues/81) (2024-03-08, @simplew2011: asks for a Ray executor).
- **Daft** (Eventual):
  - [Eventual-Inc/Daft#2235](https://github.com/Eventual-Inc/Daft/issues/2235) (2024-05-06, @simplew2011): "Can you provide an example of large-scale text deduplication".
  - Daft-side complaints on dedup specifically were not found.
- **SlimPajama** (Cerebras):
  - "vanilla MinHashLSH implementation did not scale to trillion token datasets like RedPajama without running out of memory".
  - Fixed on 64 CPU cores with 1.4 TB peak memory.
  - Source: [arXiv 2309.10818](https://arxiv.org/pdf/2309.10818) (accessed 2026-10-29)
- **LSHBloom**:
  - The MinHashLSH index can reach "277 TB for 5B documents".
  - Index inserts and queries take >85% of runtime.
  - Its Bloom-filter replacement gives a 12× speedup and 18× less disk.
  - Source: [arXiv 2411.04257](https://arxiv.org/html/2411.04257v3) (accessed 2026-10-29)
- **Marin** (Stanford open lab): [marin-community/marin#7053](https://github.com/marin-community/marin/issues/7053) (2026-07-09, opened by a bot) reports "consolidate-filter OOM-killed on mega-doc shards (4g/worker too small)". (accessed 2026-10-29)

### Inferences
- The common pattern is that dedup lives in a separate, special-purpose binary from the rest of the pipeline (DCLM's Rust BFF vs its Ray filters). That forces users to glue stages together through files.
- Data-Juicer is moving toward GPU scheduling for model operators. Its dedup is still CPU.

### Gaps
- RedPajama-v2 tooling (Together), BigCode's Spark dedup, Ray Data dedup recipes, Bodo, DuckDB, Polars and LanceDB/Lance were not researched because the search budget ran out. No claims are made about them.

## Q4. Commercial vendors: pricing and funding

### Takeaway
Funding was confirmed for DatologyAI (~$57.5M) and Eventual/Daft ($30M). Lilac was acquired by Databricks in March 2024 (terms undisclosed). No pricing was found for any of them.

### Cited Findings
- **DatologyAI**:
  - $11.65M seed in February 2024, led by Amplify Partners; angels included Jeff Dean and Yann LeCun.
  - $46M Series A in May 2024, led by Felicis.
  - The company says it has raised more than $57.5M in total.
  - Sources: [DatologyAI blog](https://www.datologyai.com/blog/datologyai-raises-46m-series-a); [SiliconANGLE 2024-02-22](https://siliconangle.com/2024/02/22/datologyai-raises-11-65m-automate-data-curation-efficient-ai-training) (accessed 2026-10-29)
- **Eventual (Daft)**:
  - Announced 2025-06-24: a $20M Series A "led by Astasia Myers from Felicis", with M12 and Citi.
  - The seed was "led by Brittany Walker from CRV", with YC, Essence VC and Array Ventures.
  - $30M raised in total.
  - Named customers: Amazon, CloudKitchens, Essential AI, Together AI, which the post says process "petabytes of multimodal data daily".
  - Eventual Cloud was launched as a waitlist.
  - The post does not mention GPUs sitting idle.
  - Source: [Eventual blog](https://www.eventual.ai/blog/eventual-raises-30m-to-build-the-future-of-data) (accessed 2026-10-29)
  - Seed size conflicts across outlets: SiliconANGLE says $10M, TechCrunch says $7.5M. — [SiliconANGLE](https://siliconangle.com/2025/06/24/eventual-launches-30m-funding-streamline-multimodal-data-processing/); [TechCrunch](https://www.techcrunch.com/2025/06/24/how-a-data-processing-problem-at-lyft-became-the-basis-for-eventual/) (accessed 2026-10-29)
  - A related Eventual post (not verified) claims Flotilla is "up to 18x faster than Spark and Ray Data". — [Eventual blog](https://www.eventual.ai/blog/eventual-raises-30m-to-build-the-future-of-data) (accessed 2026-10-29)
- **Lilac**:
  - Acquired by Databricks in March 2024; terms not disclosed.
  - Founders Daniel Smilkov and Nikhil Thorat (ex-Google, TensorFlow.js).
  - Folded into Mosaic AI; Databricks' Mosaic team had used it for about a year.
  - Source: [VentureBeat](https://venturebeat.com/ai/databricks-acquires-lilac-to-supercharge-data-quality-efforts-for-gen-ai-apps); [SiliconANGLE 2024-03-19](https://siliconangle.com/?p=649484) (accessed 2026-10-29)

### Inferences
- The same investor (Felicis, Astasia Myers) led both the DatologyAI and Eventual Series A rounds. That suggests investors see "AI data curation and processing" as one category.

### Gaps
- Cleanlab, Scale, Snorkel, Nomic and Anyscale funding and pricing were not researched (search budget exhausted).
- DatologyAI and Eventual Cloud pricing is not public in the sources I fetched.

## Q5. Public threads (Reddit / HN / forums) on slow, expensive or OOM dedup, or idle GPUs

### Takeaway
GitHub issues are the richest source (Q1–Q3). One search for Reddit threads returned none. HN and other Reddit searches could not be run.

### Cited Findings
- Idle-GPU statements found:
  - Curator #59: "others stay idle". — [#59](https://github.com/NVIDIA-NeMo/Curator/issues/59)
  - Curator #1618: GPU actor pools scaling up "one-by-one". — [#1618](https://github.com/NVIDIA-NeMo/Curator/issues/1618)
  - The FED/SEDD paper cites "low GPU utilization" for Curator. — [arXiv 2501.01046](https://arxiv.org/html/2501.01046)
- Cerebras' Model Zoo docs say MinHash generation "can be a very slow process" and recommend running it as a separate stage. — [Cerebras docs](https://docs.cerebras.ai/en/latest/wsc/Model-zoo/Components/Data-preprocessing/data_dedup.html) (accessed 2026-10-29)

### Gaps
- No Reddit (r/LocalLLaMA, r/MachineLearning, r/dataengineering) or HN threads were found or checked, because of search-budget exhaustion. A follow-up pass should search these.
