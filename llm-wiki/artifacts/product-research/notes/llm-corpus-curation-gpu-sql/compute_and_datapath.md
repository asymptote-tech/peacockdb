# LLM corpus curation: where compute goes, and the GPU data path

Method note: the session's WebSearch budget was used up before this researcher started (0 searches available), so the sources are known primary URLs fetched directly, plus local clones of the code:
- NVIDIA-NeMo/Curator @ `166c1c676af8360f2e3f151e9482a001da326ea1` (commit date 2026-10-29)
- huggingface/datatrove @ `4c488a135cc582b482df81895e631aadff2fa4c1`
- rapidsai/cudf @ `785ee522f5688f485fc0b1eabca7f8d6d2adea0e`

Code links take the form `https://github.com/<org>/<repo>/blob/<sha>/<path>`. Every source was accessed 2026-10-29. Several pages could not be fetched (time-outs or bot walls): the DCLM full text, the Dolma paper, the NVIDIA "Mastering LLM techniques: data preprocessing" blog and the FineWeb blog space. Their gaps are listed below.

## Q1. Stage breakdown, and which stages dominate wall time (CPU vs GPU)

### Takeaway
Few per-stage wall-time splits are published.
- CPU pipelines (datatrove/FineWeb):
  - The per-document pass (WARC→Trafilatura extraction plus heuristics) is the largest job by scheduled core count.
  - The MinHash clustering step is a single-node serial bottleneck, with up to 30 h and 200 GB RAM scheduled.
  - In MinHashLSH systems, LSH index insert/query is over 85% of runtime.
- GPU (NeMo Curator): fuzzy dedup is split into minhash / LSH (shuffle) / connected-components timers, but NVIDIA publishes only totals.
  - On GPU, MinHash compute per batch is smaller than the cuDF file read that feeds it (0.12–0.24 s vs 0.22–0.35 s per task).
- Model-based classifier inference is a large, separately priced GPU job: 6,000 H100-h for FineWeb-Edu over 15T tokens.

### Cited Findings
**Canonical stage lists**
- FineWeb's reference pipeline in datatrove has two parts (`examples/fineweb.py` L30–L180):
  - Per-dump stages, all in one CPU job: WarcReader (s3://commoncrawl) → URLFilter → Trafilatura(favour_precision) → LanguageFilter → GopherRepetition → GopherQuality → C4Quality → FineWebQuality → JsonlWriter. Each filter also writes its rejected docs out to an exclusion writer.
  - MinHash in four separate jobs: signatures → buckets → cluster → filter.
  - Source: [datatrove fineweb.py @4c488a1](https://github.com/huggingface/datatrove/blob/4c488a135cc582b482df81895e631aadff2fa4c1/examples/fineweb.py) (accessed 2026-10-29)
- Scheduled resources in that file (Slurm requests, not measured runtimes):

  | Stage | Tasks | Time limit | Memory |
  |---|---|---|---|
  | Base processing | 8,000 | 10 h | 2 GB/CPU |
  | Signatures | 1,000 | 5 h | – |
  | Buckets | 14 buckets × 50 = 700 | 2 h | 3 CPUs × 4 GB |
  | Clustering | **1 task** | **30 h** ("can also be quite slow. Usually not this slow though") | 8 CPUs × 25 GB = 200 GB ("if you dedup a full dump, you do need a lot of memory") |
  | Filter | 1,000 | 5 h | 4 GB/CPU |

  Source: [datatrove fineweb.py @4c488a1](https://github.com/huggingface/datatrove/blob/4c488a135cc582b482df81895e631aadff2fa4c1/examples/fineweb.py) (accessed 2026-10-29)
- The generic datatrove example `examples/minhash_deduplication.py` schedules more generously: buckets and clustering at 90 h each, clustering at 2 CPUs × 70 GB, and filter at 50 h — [datatrove minhash_deduplication.py @4c488a1](https://github.com/huggingface/datatrove/blob/4c488a135cc582b482df81895e631aadff2fa4c1/examples/minhash_deduplication.py) (accessed 2026-10-29)
- NeMo Curator fuzzy dedup stages:
  - MinHashStage → LSHStage (a shuffle) → BucketsToEdges → ConnectedComponents, then IdentifyDuplicates and removal.
  - The result metadata reports `total_time, num_duplicates, minhash_time, lsh_time, connected_components_pipeline_time`.
  - Source: [Curator fuzzy.mdx @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/fern/versions/main/pages/curate-text/process-data/deduplication/fuzzy.mdx) (accessed 2026-10-29)
- The nightly benchmark config reports `minhash_percent_time, lsh_percent_time, connected_components_percent_time` for fuzzy dedup over a Common Crawl JSONL set. Its expected `num_duplicates` is 59,226,133.
  - The percentages are computed internally but not published in the repo.
  - Source: [Curator benchmarks.yaml @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/benchmarking/benchmarks.yaml) (L337–L361) (accessed 2026-10-29)
- The older Curator GPU dedup (NeMo Framework 24.07 docs) had extra stages: minhash → `minhash_buckets` (Buckets.parquet) → `jaccard_map_buckets` → `jaccard_shuffle` (joins the text back to the bucket anchors) → `jaccard_compute` (an explicit false-positive check) → connected components with jaccard threshold 0.8.
  - `buckets-per-shuffle` higher is "better but might lead to oom".
  - The current workflow has no Jaccard false-positive stage; it goes LSH → edges → CC directly.
  - Sources: [NeMo 24.07 GPU dedup docs](https://docs.nvidia.com/nemo-framework/user-guide/24.07/datacuration/gpudeduplication.html) (accessed 2026-10-29); [Curator workflow.py @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/nemo_curator/stages/deduplication/fuzzy/workflow.py) (accessed 2026-10-29)

**Measured splits**
- LSHBloom paper, on peS2o with 39M docs and Polaris nodes (32-core Zen 3, 512 GB RAM):
  - MinHashLSH (datasketch-style): index insert/query is **over 85% of runtime**.
  - Its own LSHBloom before optimization: hashing integer band vectors was over 90% of insert/query time.
  - After a Rust hashing rewrite, **MinHash signature generation becomes the majority** of LSHBloom runtime.
  - End to end: MinHashLSH over 37 h (the intro says over 35 core-hours) vs LSHBloom about 3 h (about 12×).
  - Source: [LSHBloom arXiv 2411.04257](https://arxiv.org/html/2411.04257) (accessed 2026-10-29)
- Curator MinHash GPU benchmark on 8×H100 nightly:
  - Data: RPv2 2023-14 English Parquet, 1,046,344,809 docs, 512 MiB blocks.
  - Required throughput is at least **2,000,000 docs/s**.
  - Per-task worker means: `minhash_compute` 0.12–0.24 s vs `minhash_input_prep` ("cuDF reading the source files") 0.22–0.35 s.
  - Source: [Curator benchmarks.yaml @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/benchmarking/benchmarks.yaml) (L364–L400) (accessed 2026-10-29)
- Classifier inference cost:
  - FineWeb-Edu classifier over 15T tokens: **6,000 H100 GPU-hours**.
  - Ablations: about 80,000 H100-h across more than 70 models.
  - Text extraction is called "relatively costly" with no figure.
  - Source: [FineWeb paper arXiv 2406.17557](https://arxiv.org/html/2406.17557) (accessed 2026-10-29)
- Nemotron-CC:
  - Pipeline: jusText extraction (chosen over Trafilatura), global fuzzy dedup with NeMo Curator, and exact substring dedup over eighths of snapshots (deduplicate-text-datasets).
  - Quality ensemble: 2 regression heads on Snowflake-arctic-embed-m plus the DCLM fastText model.
  - Synthetic data: Mistral NeMo 12B in FP8 on TensorRT-LLM, producing about 1.8–1.9T synthetic tokens.
  - GPU-hours are reported for none of these.
  - Source: [Nemotron-CC arXiv 2412.02595](https://arxiv.org/html/2412.02595) (accessed 2026-10-29)
- The Curator throughput doc's illustrative (not measured) streaming example claims GPU workers stay "busy over 99% of the time" after warm-up, through per-stage autoscaling — [Curator throughput.mdx @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/fern/versions/main/pages/about/concepts/throughput.mdx) (accessed 2026-10-29)

### Inferences
- On CPU there are two hot spots:
  - The embarrassingly parallel extract+filter pass: 8,000 tasks × 10 h ≈ 80k task-hours scheduled per dump in datatrove's example. These are upper-bound requests, not measurements.
  - The serial or skewed clustering/union-find step.
- On GPU the string hashing itself is cheap, so wall time shifts to:
  - IO and parse (reading Parquet/JSONL into cuDF),
  - the all-to-all LSH shuffle,
  - classifier/LLM inference, which is the largest single GPU line item when present (6k H100-h for FineWeb-Edu).
- SemDedup and LLM synthetic data generation are inference-dominated and dwarf MinHash dedup in GPU-hours. I found no published number for this.

### Gaps
- No published per-stage percentage split (minhash/LSH/CC/removal) for NeMo Curator. The fields exist in the benchmark harness, but the numbers are internal (Slack sink).
- No measured per-stage timings for FineWeb/datatrove, Dolma, DCLM or RedPajama processing; the DCLM and Dolma paper pages timed out.
- Language-ID, PII and decontamination costs: none found.

## Q2. How much is relational/SQL-shaped vs string kernels vs inference vs graph

### Takeaway
Fuzzy and exact dedup are mostly relational operators wrapped around one string kernel:
- string kernel: hash shingles to a fixed-width signature;
- relational: explode/melt signatures into (doc_id, bucket_id) rows, hash-shuffle on bucket_id, group-by with count > 1, group into lists;
- graph: emit edges, then weakly connected components;
- relational again: a shuffle on group id to choose survivors, then an anti-join (`~isin`) to remove.

Exact dedup is MD5 plus shuffle plus group-by. Model filtering is pure inference; heuristics are per-row string UDFs.

### Cited Findings
- cuDF string kernels used by Curator:
  - `Series.str.minhash(seed, a, b, width)` / `minhash64`: MurmurHash3_x86_32, or x86_128 when a, b are uint64. Each hash is permuted as `(hv*a + b) % (2^61-1)` over character substrings of `width`.
  - `str.minhash_ngrams(ngrams, seed, a, b)` / `minhash64_ngrams` hash word n-grams from list columns.
  - Sources: [cudf accessors/string.py @785ee52](https://github.com/rapidsai/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/python/cudf/cudf/core/accessors/string.py) (L5396–L5600) (accessed 2026-10-29); usage in [Curator minhash.py @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/nemo_curator/stages/deduplication/fuzzy/minhash.py) (L136–L181) (accessed 2026-10-29)
- LSH is relational (`nemo_curator/stages/deduplication/fuzzy/lsh/lsh.py`):
  - `minhash_to_bands`: `list.take(indices).hash_values()` per band, prefixed `"b{i}_"`, then a melt to long format.
  - The data goes through a `BulkRapidsMPFShuffler` (rapidsmpf all-to-all) on bucket id.
  - `group_by_bucket` keeps buckets with more than one doc and aggregates their ids into lists.
  - Source: [Curator lsh.py @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/nemo_curator/stages/deduplication/fuzzy/lsh/lsh.py) (L34–L260) (accessed 2026-10-29)
- Edges and graph:
  - `BucketsToEdgesStage` emits `pairwise(bucket_docs)`, i.e. a chain of n−1 edges per bucket rather than all n² pairs. This is done on CPU with pandas/pyarrow.
  - `ConnectedComponentsStage` runs pylibcugraph multi-GPU `weakly_connected_components` (MGGraph) across RAFT actors.
  - Sources: [buckets_to_edges.py @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/nemo_curator/stages/deduplication/fuzzy/buckets_to_edges.py) (L72–L80) (accessed 2026-10-29); [connected_components.py @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/nemo_curator/stages/deduplication/fuzzy/connected_components.py) (L20–L188) (accessed 2026-10-29)
- Survivor selection and removal:
  - `IdentifyDuplicates` shuffles on group id, then runs nunique and removal-id selection.
  - Removal is `df[~df[id_field].isin(removal_ids)]`, an anti-join.
  - Sources: [identify_duplicates.py @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/nemo_curator/stages/deduplication/fuzzy/identify_duplicates.py) (L84–L127) (accessed 2026-10-29); [removal.py @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/nemo_curator/stages/text/deduplication/removal.py) (L88) (accessed 2026-10-29)
- Exact dedup is MD5-hash the text, shuffle on the hash, then group (nunique) — [exact/identification.py @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/nemo_curator/stages/deduplication/exact/identification.py) (L112–L261) (accessed 2026-10-29)
- Removal tuning: `input_blocksize` 256–512 MiB with `identification_batchsize` is recommended, targeting 2–6 GB batches. This "improves both shuffle throughput and removal performance compared to the 2GiB default" — [Curator docs (fern main) @166c1c6](https://github.com/NVIDIA-NeMo/Curator/tree/166c1c676af8360f2e3f151e9482a001da326ea1/fern/versions/main/pages) (accessed 2026-10-29)
- Multi-node GPU fuzzy-dedup tuning points to the network: benchmark config sets `UCX_TLS: all` for exact and fuzzy dedup on 4×GB200, and the docs tell users to use InfiniBand/high-speed Ethernet for multi-node — [4xGB200-64CPU.yaml @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/benchmarking/4xGB200-64CPU.yaml) (accessed 2026-10-29); [throughput.mdx @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/fern/versions/main/pages/about/concepts/throughput.mdx) (accessed 2026-10-29)

### Inferences
- By operator count, about 5 of the roughly 7 fuzzy-dedup steps are SQL-shaped: explode/melt, hash shuffle, group-by HAVING count > 1, array_agg, anti-join. The remaining two are a string UDF (minhash) and graph WCC. The graph step could be phrased as iterative label propagation (a repeated join plus min group-by) in SQL.
- The heavy operators are a distributed shuffle on high-cardinality hash keys and an anti-join of the corpus against a removal-id list. GPU SQL engines already target exactly these.
- By GPU-hours, inference (quality classifiers, embeddings for SemDedup, LLM rewriting) likely dominates where it is used. That work is not SQL-shaped. This is an inference from the 6k H100-h figure and the absence of comparable dedup GPU-hour numbers.

### Gaps
- No public dollar or time split of relational vs string vs graph vs inference in any production pipeline.

## Q3. Data sizes

### Takeaway
- Typical scale: tens of trillions of tokens and tens of billions of documents.
- Signatures: about 0.9–1 KB per doc (112×64-bit in datatrove; NeMo default 20 bands × 13 = 260×32-bit).
- LSH index sizes: an on-disk MinHashLSH index extrapolates to 278 TB at 5B docs and 555 TB at 100B; Bloom-filter LSH (LSHBloom) brings this to 8–32 TB.
- Raw text: about 4.5 bytes/token, from 8 TB ≈ 1.78T tokens on RPv2.

### Cited Findings
- Corpus sizes:

  | Corpus | Size | Source |
  |---|---|---|
  | FineWeb | 15T tokens, 96 CC snapshots; Hub default config 25.9B rows, Parquet | [FineWeb paper](https://arxiv.org/html/2406.17557); [HF dataset card](https://huggingface.co/datasets/HuggingFaceFW/fineweb) |
  | RedPajama-v2 | over 100B docs, 84 snapshots; head_middle 50.6T tokens raw / 30.4T deduped (32.9B → 20.8B docs); about 1 TB disk per snapshot; docs and quality signals as json.gz, minhash and duplicate ids as Parquet | [RPv2 card](https://huggingface.co/datasets/togethercomputer/RedPajama-Data-V2) |
  | Dolma | 3T tokens | [allenai/dolma](https://github.com/allenai/dolma) |
  | DCLM-Pool | 240T tokens | [DCLM abstract](https://arxiv.org/abs/2406.11794) |
  | Nemotron-CC | 6.3T tokens (4.4T real + 1.9T synthetic), 99 snapshots; TB not stated | [Nemotron-CC](https://arxiv.org/html/2412.02595) |
  | Zyda-2 | 5T tokens from an input pool of about 12B docs / 11.5T tokens | [Zyphra blog](https://www.zyphra.com/post/building-zyda-2) |

  All accessed 2026-10-29.
- Bytes per token: Curator's benchmark set is "8 TB RedPajama v2 dataset (1.78 trillion tokens)". 8e12 / 1.78e12 ≈ **4.5 bytes/token** on disk, with format and compression unspecified — [Curator throughput.mdx @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/fern/versions/main/pages/about/concepts/throughput.mdx) (accessed 2026-10-29)
- Dedup removal rates:
  - FineWeb: per-dump dedup gives about 20T tokens; global dedup would give about 4T and removed up to 90% of the oldest dumps.
  - Zyda-2: cross-dedup removed about 11% of tokens (13% of docs); DCLM and FineWeb-Edu are each about 80% internally duplicated.
  - Sources: [FineWeb](https://arxiv.org/html/2406.17557); [Zyda-2 arXiv 2411.06068](https://arxiv.org/html/2411.06068) (accessed 2026-10-29)
- MinHash parameters:
  - FineWeb: 5-gram words, 112 hashes = 14 buckets × 8, 64-bit sha1 (`precision=64`).
  - Zyda-2: 128 perms, char 25-grams, 8 bands, threshold about 0.85.
  - RPv2: 128 hashes; bands/rows 14/9 at J = 0.7, 9/13 at 0.8, 5/25 at 0.9.
  - NeMo default: char_ngrams 24, num_bands 20, minhashes_per_band 13, 32-bit hash unless `use_64_bit_hash`. Threshold ≈ (1/b)^(1/r).
  - Sources: [datatrove fineweb.py @4c488a1](https://github.com/huggingface/datatrove/blob/4c488a135cc582b482df81895e631aadff2fa4c1/examples/fineweb.py); [Zyphra blog](https://www.zyphra.com/post/building-zyda-2); [RPv2 card](https://huggingface.co/datasets/togethercomputer/RedPajama-Data-V2); [Curator fuzzy.mdx @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/fern/versions/main/pages/curate-text/process-data/deduplication/fuzzy.mdx) (accessed 2026-10-29)
- LSH index size (LSHBloom paper):
  - peS2o (39M docs): MinHashLSH index over 200 GB vs LSHBloom about 11 GB.
  - Extrapolated: MinHashLSH 277.68 TB at 5B docs and 555.35 TB at 100B docs, vs LSHBloom 8.3–15.5 TB (5B) and 16.7–31.8 TB (100B), depending on false-positive rate.
  - At 10B docs, 9 bands, FP 1e-10: 590 GB vs 46 TB.
  - Source: [LSHBloom](https://arxiv.org/html/2411.04257) (accessed 2026-10-29)
- GPU memory controls:
  - LSH RMM pool defaults to 90% of free GPU memory; the spill-to-host limit is 80% of the pool; `bands_per_iteration` (default 5) trades memory for passes.
  - Docs give only "32GB+ VRAM recommended" for large-scale dedup.
  - Sources: [Curator fuzzy.mdx @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/fern/versions/main/pages/curate-text/process-data/deduplication/fuzzy.mdx); [Curator docs @166c1c6](https://github.com/NVIDIA-NeMo/Curator/tree/166c1c676af8360f2e3f151e9482a001da326ea1/fern/versions/main/pages) (accessed 2026-10-29)
- Zyphra ran its benchmark on one GCP node with 208 vCPUs, 1.9 TB RAM and 8×H100 80 GB — [Zyphra blog](https://www.zyphra.com/post/building-zyda-2) (accessed 2026-10-29)

### Inferences
- Signature tables:
  - NeMo default: 260 × 4 B ≈ 1.04 KB/doc, so the 1.046B-doc RPv2 2023-14 benchmark yields about 1.1 TB of signatures.
  - FineWeb: 112 × 8 B = 896 B/doc, or about 23 TB for 25.9B docs.
  - Signatures are therefore about 10–25% of raw text size at typical 4–10 KB docs.
- The long (doc_id, bucket_id) LSH table:
  - It has docs × bands rows, about 20 × 16 B ≈ 320 B/doc for NeMo defaults: about 0.33 TB for 1B docs, about 8 TB for 25B docs.
  - These are row counts times key widths, not measured.
  - This is why Curator processes `bands_per_iteration` bands at a time and spills to host.
- Peak memory for 1 TB / 8 TB fuzzy dedup is not published. The 8 TB run fit on 1 node of 8×H100 (640 GB HBM) in 2.05 h, so with spilling and band iteration, peak HBM need is well below the dataset size.

### Gaps
- No NVIDIA hardware-sizing table (GPUs per TB) for current fuzzy dedup. The 24.07 docs give none either.
- No measured LSH bucket-table sizes for NeMo runs.
- FineWeb TB-on-disk is not on the dataset card.

## Q4. Bandwidth, IO, and GPU decode of JSONL vs Parquet

### Takeaway
- In Curator's own GPU MinHash benchmark, the cuDF read of Parquet costs more per task than the MinHash kernel: 0.22–0.35 s vs 0.12–0.24 s. Throughput is at least 2M docs/s on 8×H100.
- IO and parse therefore bounds GPU dedup once hashing is on GPU.
- Curator docs list "I/O bound reader" as the first common bottleneck, and say to keep files at about 500 MB and use NVMe/parallel FS.
- No published GPU-utilization traces exist beyond the illustrative "over 99% busy" claim.

### Cited Findings
- MinHash benchmark metrics separate `minhash_input_prep_worker_time_s_mean` ("FileGroupTask input prep measures cuDF reading the source files"), required 0.22–0.35 s, from `minhash_compute_worker_time_s_mean`, 0.12–0.24 s. Docs/s must be at least 2,000,000 for 1.046B RPv2 docs — [benchmarks.yaml @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/benchmarking/benchmarks.yaml) (accessed 2026-10-29)
- The benchmark harness records per-GPU stats at 1 s intervals to `gpustats.csv`, but no results are published — [benchmarks.yaml @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/benchmarking/benchmarks.yaml) (L35–L40) (accessed 2026-10-29)
- Bench input formats:
  - Exact dedup reads RPv2 Parquet (`raw_content`, 3 GiB blocks).
  - Fuzzy dedup reads Common Crawl **JSONL** (1.5 GiB blocks).
  - So both formats are first-class GPU inputs through the cuDF readers.
  - Source: [benchmarks.yaml @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/benchmarking/benchmarks.yaml) (accessed 2026-10-29)
- Curator guidance on files and writes:
  - Split a 50 GB JSONL into about 100 × 500 MB files.
  - Use `max_batch_bytes=512MiB` for 5 GB Parquet shards.
  - KvikIO direct-IO writes are off by default (`KVIKIO_AUTO_DIRECT_IO_WRITE=0`) because they can reduce throughput on Lustre.
  - Source: [Curator docs (fern main) @166c1c6](https://github.com/NVIDIA-NeMo/Curator/tree/166c1c676af8360f2e3f151e9482a001da326ea1/fern/versions/main/pages) and [fuzzy.mdx](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/fern/versions/main/pages/curate-text/process-data/deduplication/fuzzy.mdx) (accessed 2026-10-29)
- Common bottlenecks per Curator: "I/O bound reader: increase reader parallelism or use faster storage (NVMe, parallel file system)"; "Low GPU utilization often indicates an upstream bottleneck (I/O, CPU processing)" — [throughput.mdx @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/fern/versions/main/pages/about/concepts/throughput.mdx) (accessed 2026-10-29)
- datatrove reads WARCs straight from `s3://commoncrawl` and staggers starts with `randomize_start_duration=180` "don't hit the bucket all at once with the list requests". This points to S3 request-rate pressure at 8,000 tasks — [datatrove fineweb.py @4c488a1](https://github.com/huggingface/datatrove/blob/4c488a135cc582b482df81895e631aadff2fa4c1/examples/fineweb.py) (accessed 2026-10-29)

### Inferences
- At 2M docs/s and about 4–5 KB/doc, the 8×H100 node ingests on the order of 8–10 GB/s of text. That is near local-NVMe and NIC limits, so S3 or network storage would bound the run unless data is staged locally.
- The 8 TB / 0.5 h figure on 4 nodes implies about 4.4 GB/s aggregate across the whole fuzzy pipeline, or about 1.1 GB/s per node.
- Both figures are back-of-envelope.

### Gaps
- No published cuDF GPU JSONL(.zst) vs Parquet string-decode throughput comparison for curation data.
- No reported measurement of S3-bound curation runs.

## Q5. Cost: CPU core-hours vs GPU-hours

### Takeaway
NVIDIA and Zyphra report 10–20× wall-clock speedups but only about 1.7–5× TCO gains, because an H100 node costs about 5× a CPU node.

| Claim | CPU | GPU | Speedup | Cost |
|---|---|---|---|---|
| NeMo 2023 (A100) | 37 h on 20 nodes × 48 cores (~35.5k core-h) | 3 h on 4 DGX A100 (96 A100-h) | ~12× wall (headline "20×") | headline "5× cheaper" |
| Curator README (H100) | 10.7 h | 0.65 h on 3 H100 nodes | ~16× | $315 → $190 |
| Zyda-2 | ~3 weeks on 8 CPU nodes | ~2 days on 8 GPU nodes | ~10× | 2× lower TCO |

### Cited Findings
- NeMo Data Curator launch blog (A100 era):
  - Fuzzy dedup of **4.5 TB RedPajama** took **37 h on 20 CPU nodes** (48 cores, 188 GB RAM each) vs **3 h on 4 DGX A100** (8×80 GB each).
  - Headline: "20x faster and 5x cheaper".
  - The production run curated 8.7 TB on more than 6K CPUs.
  - Source: [NVIDIA blog: Curating trillion-token datasets](https://developer.nvidia.com/blog/curating-trillion-token-datasets-introducing-nemo-data-curator/) (accessed 2026-10-29)
- Current Curator README:
  - Fuzzy dedup on an RPv2 subset: **10.7 h → 0.65 h (~16×)** on **3× H100 80 GB nodes** vs a "CPU-based alternative".
  - TCO **$315 → $190 (~40% lower)**.
  - Scaling: 1/2/4 nodes = 2.05 h / 1.01 h / 0.50 h on 8 TB (1.78T tokens).
  - The README warns "token counts and exact subset sizes vary across published panels".
  - Sources: [Curator README @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/README.md) (L118–L127); [throughput.mdx @166c1c6](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/fern/versions/main/pages/about/concepts/throughput.mdx) (accessed 2026-10-29)
- Zyphra (Zyda-2):
  - Full pipeline "from 3 weeks to 2 days": about 3 weeks on 8 CPU nodes vs about 2 days on 8 GPU nodes, and 10× on a single-node benchmark vs their CPU package Zyda-Processing.
  - "CPU nodes ... typically cost 5 times less than an H100 node"; TCO reduced 2×.
  - Sources: [Zyphra blog](https://www.zyphra.com/post/building-zyda-2); [NVIDIA Zyda-2 blog](https://developer.nvidia.com/blog/train-highly-accurate-llms-with-the-zyda-2-open-5t-token-dataset-processed-with-nvidia-nemo-curator/) (accessed 2026-10-29)
- CPU-side algorithmic alternative (LSHBloom): 12× faster than MinHashLSH on CPU and 18× less disk. Extrapolated single-process time at 5B docs is about 200 days vs 15 days. This shows CPU dedup cost is algorithm-sensitive, not only a matter of hardware — [LSHBloom](https://arxiv.org/html/2411.04257) (accessed 2026-10-29)
- Inference reference point: FineWeb-Edu classifier over 15T tokens = 6,000 H100-h — [FineWeb](https://arxiv.org/html/2406.17557) (accessed 2026-10-29)

### Inferences
- The two NVIDIA claims are not directly comparable:
  - 2023 claim, in resource-hours: about 35.5k CPU core-hours (20 × 48 × 37 h) vs 96 A100-hours (32 × 3 h).
  - 2026 claim: 3 H100 nodes = 24 H100 × 0.65 h ≈ 15.6 H100-hours for an RPv2 subset. The CPU baseline hardware is unspecified.
  - $190 / 15.6 H100-h implies about $12/H100-h, or about $97/node-hour. This only holds if the $190 is all GPU nodes, which is unverified.
- GPU dedup at the 8 TB scale costs on the order of tens of H100-hours. Classifier inference over a whole corpus costs thousands of H100-hours. Per-corpus GPU spend is dominated by inference, not by SQL-shaped dedup.
- The GPU dedup advantage is mostly wall-clock: about 10–16×. On cost it is only about 1.7–2×, since node prices differ about 5×.

### Gaps
- CPU baseline details for the 10.7 h / $315 figure (which framework, how many nodes) are not given in the repo.
- No independent community benchmark of NeMo Curator vs datatrove at the same scale was found, since search was unavailable.
- No published core-hours for FineWeb, Dolma or DCLM processing.
