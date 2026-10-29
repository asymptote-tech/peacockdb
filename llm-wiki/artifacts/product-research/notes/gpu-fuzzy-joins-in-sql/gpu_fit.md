# GPU fuzzy / similarity joins (entity resolution): GPU fit and existing implementations

Method note: the session-wide WebSearch budget ran out after one query. These notes come from direct fetches of known URLs and from shallow clones under scratchpad/ext. Several questions are listed as gaps for that reason.

Code snapshots used (cloned 2026-10-29):
- rapidsai/cudf at `785ee522f5688f485fc0b1eabca7f8d6d2adea0e` (2026-10-29, sparse: cpp/include/nvtext, cpp/include/cudf/strings)
- ajwinters/crosswalk at `9b801f70b25658a1a2cab39c5f6c52523851f655` (2026-10-19)
- sirius-db/sirius at `b9e05ffc653c8ed031c789e5c1415ac1186e0619` (2026-10-28)

## Q1. What libcudf / nvtext provides; is there a similarity join; are there open requests?

### Takeaway
libcudf has only row-wise or one-vs-all similarity primitives: Levenshtein `edit_distance`, n-gram `jaccard_index` and MinHash. It has no Jaro / Jaro-Winkler, no all-pairs matrix at HEAD, and no similarity-join operator. Two Jaro-Winkler / fuzzy-matching feature requests are open and have had no maintainer engagement.

### Cited Findings
- The `cpp/include/nvtext/` headers at HEAD are byte_pair_encoding, deduplicate, edit_distance, generate_ngrams, jaccard, minhash, ngrams_tokenize, normalize, replace, stemmer, tokenize, unicode_normalize and wordpiece_tokenize — [cudf@785ee52 nvtext](https://github.com/rapidsai/cudf/tree/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/nvtext) (accessed 2026-10-29)
- `nvtext::edit_distance(input, targets)` computes Levenshtein for `output[i]` = d(input[i], targets[i]). `targets.size()` must equal `input.size()` or be 1 (one-vs-all). Nulls are treated as empty strings. — [edit_distance.hpp@785ee52](https://github.com/rapidsai/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/nvtext/edit_distance.hpp) (accessed 2026-10-29)
- `edit_distance_matrix` (the older all-pairs n×n API) does not appear anywhere in `cpp/include/nvtext` at this commit. Grep returned nothing, so it was removed at some point. I could not establish when, because the clone is shallow. — [cudf@785ee52](https://github.com/rapidsai/cudf/tree/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/nvtext) (accessed 2026-10-29)
- `nvtext::jaccard_index(input1, input2, width)` is row-wise. It takes sliding-window character substrings of size `width` (must be ≥ 2), requires equal-size inputs and returns float32. — [jaccard.hpp@785ee52](https://github.com/rapidsai/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/nvtext/jaccard.hpp) (accessed 2026-10-29)
- The MinHash APIs are `minhash` (uint32) and `minhash64`, both "permuted" variants taking `seed, parameter_a, parameter_b` (a·h+b permutations), plus `minhash_ngrams` and `minhash64_ngrams` over list-of-token rows. They return a list column of per-row hashes and throw overflow if `parameter_a.size()*input.size()` exceeds the column size limit. — [minhash.hpp@785ee52](https://github.com/rapidsai/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/nvtext/minhash.hpp) (accessed 2026-10-29)
- Other preprocessing primitives present:
  - `normalize_spaces` and `normalize_characters` (`normalize_flags::STRIP_ACCENTS | PAD_PUNCTUATION`)
  - `tokenize`, `count_tokens`, `character_tokenize`, `detokenize`, `tokenize_with_vocabulary`
  - character/word n-grams and `hash_character_ngrams`
  - `build_suffix_array` / `resolve_duplicates` / `resolve_duplicates_pair` (exact duplicate substrings)
  - [normalize.hpp](https://github.com/rapidsai/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/nvtext/normalize.hpp), [tokenize.hpp](https://github.com/rapidsai/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/nvtext/tokenize.hpp), [generate_ngrams.hpp](https://github.com/rapidsai/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/nvtext/generate_ngrams.hpp), [deduplicate.hpp](https://github.com/rapidsai/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/nvtext/deduplicate.hpp) (accessed 2026-10-29)
- Grepping all of `cpp/include` for "jaro" returns nothing, so there is no Jaro / Jaro-Winkler in libcudf. — [cudf@785ee52](https://github.com/rapidsai/cudf/tree/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include) (accessed 2026-10-29)
- Issue #6503 "[FEA] Jaro-Winkler algorithm for … StringMethods.edit_distance":
  - Opened 2020-10-12 by paulhendricks; still open; no assignee and no PR.
  - Activity since: labels only, a "rotten" bot mark in Feb 2021, and a user asking for a workaround in Jul 2024 with no reply.
  - Placed on the cuDF Python board as "Todo" (2024-11, 2026-08).
  - [cudf#6503](https://github.com/rapidsai/cudf/issues/6503) (accessed 2026-10-29)
- Issue #18274 "[FEA] RapidFuzz/ Fuzzy Matching out of the box functionality" asks for Jaro-Winkler, Jaccard and Levenshtein the way RapidFuzz offers them. Opened 2025-03-14 by teskuteyi; still open. The only maintainer activity is labels and board placement (vyasr 2026-06-23; "Todo" 2026-08-12). — [cudf#18274](https://github.com/rapidsai/cudf/issues/18274) (accessed 2026-10-29)

### Inferences
- An engine built on libcudf would need to write its own Jaro-Winkler kernel. It could reuse `edit_distance` and `jaccard_index` only as row-wise scoring applied after a candidate join (gather left/right by pair index, then call the row-wise API). The "similarity join" itself is not something libcudf provides. (estimate)
- Since #6503 has been unaddressed for 6 years, RAPIDS is unlikely to ship JW soon. That makes it a differentiation opportunity, but it is also code the engine would own. (estimate)

### Gaps
- When `edit_distance_matrix` was removed and the deprecation PR number: the clone is shallow and search was unavailable.
- Whether a cuDF fuzzy *merge* issue exists beyond #18274: only the "jaro" issue search was checked.

## Q2. NVIDIA's own entity-resolution material

### Takeaway
NVIDIA's productized "fuzzy" path is NeMo Curator's document-level fuzzy dedup: MinHash, then LSH, then connected components on GPU. No NVIDIA record-linkage / Fellegi-Sunter product was found.

### Cited Findings
- NeMo Curator fuzzy dedup works as follows:
  - MinHash signatures over character n-grams (default `char_ngrams=24`, recommended ≥ 20).
  - LSH with `num_bands=20` × `minhashes_per_band=13`; approximate threshold (1/num_bands)^(1/minhashes_per_band) ≈ 0.79.
  - `BucketsToEdges`, then connected components spread across RAFT actors and balanced by edge count.
  - MinHash/LSH need an NVIDIA CUDA GPU on a Ray cluster; cuDF handles intermediate and output writes; default `input_blocksize="1GiB"`.
  - The page gives no throughput numbers.
  - [NeMo Curator fuzzy dedup docs](https://docs.nvidia.com/nemo/curator/latest/curate-text/process-data/deduplication/fuzzy.html) (accessed 2026-10-29)
- NVIDIA's RAFT vector-search blog benchmarks CAGRA against CPU HNSW on DEEP-100M at 95% recall with batch size 10. It says CAGRA beats HNSW in throughput even at small batches. Numbers appear only in charts, and kNN joins / all-pairs are not discussed. — [NVIDIA blog: RAFT vector search](https://developer.nvidia.com/blog/accelerating-vector-search-using-gpu-powered-indexes-with-rapids-raft/) (accessed 2026-10-29)

### Inferences
- The LSH-band workload pattern (minhash, explode bands, hash equi-join on (band, bucket) keys, dedup pairs, connected components) is already proven at scale on cuDF. A SQL engine with `minhash64` plus a hash join plus a CC operator covers it without new kernels. (estimate)

### Gaps
- Morpheus ER use, Senzing / Quantexa partnerships, and NVIDIA ER notebooks were not verified (no search budget). Do not claim they exist.
- cuVS brute-force / CAGRA kNN-join throughput figures (QPS at 1e6–1e8 vectors) were not obtained in text form.

## Q3. Other GPU ER implementations

### Takeaway
Two open-source GPU ER projects exist: Fast-ER (Python, "35× faster" claim) and crosswalk (cuDF blocking, a Numba Jaro-Winkler kernel and CuPy Fellegi-Sunter scoring). crosswalk shows 20 B candidate pairs scored in about 4 minutes in 1.4 GB VRAM by streaming.

### Cited Findings
- Fast-ER: "GPU-accelerated record linkage and deduplication" for CUDA GPUs, by Jacob Morrier, Sulekha Kishore and R. Michael Alvarez. It uses Jaro-Winkler as its string metric with a probabilistic match model and claims "over 35 times faster" than the leading CPU implementation, "from hours to minutes". The landing page gives no hardware or pair counts. — [Fast-ER docs](https://fast-er.readthedocs.io/en/latest/) (accessed 2026-10-29)
- crosswalk architecture:
  - Blocking (`indexing`), comparison and Fellegi-Sunter scoring all move to the GPU; standardization stays on the CPU.
  - cuDF does the blocking joins and CuPy does the Fellegi-Sunter arithmetic.
  - A custom Numba CUDA Jaro-Winkler kernel (`crosswalk/gpu/jaro_winkler.py`) is "the one primitive RAPIDS doesn't provide". It matches the CPU `jellyfish` output bit-for-bit, at a Jaro-Winkler threshold of 0.9.
  - A streaming matcher (`crosswalk/gpu/streaming.py`) bounds peak memory by chunk size, not by pair count.
  - [crosswalk README@9b801f7](https://github.com/ajwinters/crosswalk/blob/9b801f70b25658a1a2cab39c5f6c52523851f655/README.md) (accessed 2026-10-29)
- crosswalk results (RTX 4070 Ti SUPER 16 GB): — [crosswalk README@9b801f7](https://github.com/ajwinters/crosswalk/blob/9b801f70b25658a1a2cab39c5f6c52523851f655/README.md) (accessed 2026-10-29)

  | Records | Candidate pairs | CPU | GPU | Speedup |
  |--:|--:|--:|--:|--:|
  | 10 k | 1.98 M | 6.9 s | 1.2 s | 5.7× (19× on the Jaro-Winkler stage) |
  | 25 k | 12.5 M | 35.5 s | 0.5 s | 79× |
  | 1 M | ~20 B | ~16 h (extrapolated; the CPU run OOMs needing a ~320 GB pair index) | ~4 min at 1.4 GB VRAM | — |

  The brief's "14 h → 4 min" figure does not match the README, which says ~16 h.
- The crosswalk kernel pads strings to a fixed `MAXLEN = 32` uint8 matrix (n × MAXLEN). Each thread allocates `cuda.local.array` scratch of compile-time size, one pair per thread. — [jaro_winkler.py@9b801f7](https://github.com/ajwinters/crosswalk/blob/9b801f70b25658a1a2cab39c5f6c52523851f655/crosswalk/gpu/jaro_winkler.py) (accessed 2026-10-29)

### Inferences
- Implied throughput (derived from the README, so an estimate):
  - crosswalk GPU end to end at 1M records: 20e9 / 240 s ≈ 8×10⁷ pairs/s on a consumer GPU.
  - crosswalk GPU at 25k records: 12.5 M / 0.5 s ≈ 2.5×10⁷ pairs/s.
  - crosswalk's own CPU path (Python recordlinkage/jellyfish): 12.5 M / 35.5 s ≈ 3.5×10⁵ pairs/s.
- The 320 GB for 20 B pairs implies about 16 B/pair (two int64 indices) for a materialized pair index. (estimate)

### Gaps
- Academic GPU string-similarity-join papers (GPU edit-distance join, GPU set-similarity / Jaccard join, GPU record-linkage papers) and their reported speedups: not retrieved because search was exhausted.
- Fast-ER benchmark details (dataset, GPU, pairs/s) and its paper: not retrieved.

## Q4. Workload shape: compute- vs memory-bound, pairs/s, bytes/pair, pair explosion

### Takeaway
Blocking is a plain equi-join, which the engine already has. The pairwise kernel (JW or Levenshtein on strings ≤ 32–64 chars) is compute-bound per pair and ideal for one-thread-per-pair. The real risk is materializing the candidate pairs: 1e9 pairs is 8–16 GB of indices, and 1e11 is 0.8–1.6 TB. The engine therefore needs a fused join → gather → score → filter pipeline that streams chunks and never writes out the full pair set.

### Cited Findings
- crosswalk streams candidate pairs in output-bounded chunks and keeps only links above threshold. This processes "billions of pairs in a couple of gigabytes of VRAM". — [crosswalk README@9b801f7](https://github.com/ajwinters/crosswalk/blob/9b801f70b25658a1a2cab39c5f6c52523851f655/README.md) (accessed 2026-10-29)
- CPU baseline, Splink + DuckDB:
  - 7 M records deduplicated in "just over 2 minutes" on a large EC2 instance, with about 1 B comparisons (~100 per record).
  - The full benchmark took ~25 min on a c6g.2xlarge (8 vCPU, 16 GiB), for under $1 on EC2.
  - The Spark backend's `predict()` was 8.7× slower than DuckDB.
  - fastLink (R) took ~400 min for 300 k records on 8 cores (not strictly comparable).
  - [Robin Linacre, fast deduplication with Splink](https://www.robinlinacre.com/fast_deduplication/) (accessed 2026-10-29)
- NeMo Curator's pipeline ends in connected components over LSH edges, run distributed via RAFT. — [NeMo Curator fuzzy docs](https://docs.nvidia.com/nemo/curator/latest/curate-text/process-data/deduplication/fuzzy.html) (accessed 2026-10-29)

### Inferences
- Splink-DuckDB implied rate is ~1e9 / ~130 s ≈ 8×10⁶ comparisons/s end to end on a large multi-core CPU. crosswalk's GPU rate (~8×10⁷/s on a consumer card) is about 10× that, a more honest ratio than crosswalk's 79× against a Python CPU baseline. (estimate)
- Stage profile (all estimates):
  - **Blocking hash join**: memory-bound. Output size is the problem: power-law block sizes give quadratic blow-up.
  - **Gather of string bytes per pair**: memory-bound. Random access to 2 × ~16–32 B strings is ~64 B/pair plus 8–16 B of pair indices.
  - **Jaro-Winkler / Levenshtein on short strings**: compute-bound. Levenshtein is O(m·n) ≈ 1 k cell ops at m = n = 32. Bit-parallel Myers / Hyyrö for ≤ 64 chars gives O(n) word ops.
  - **Fellegi-Sunter scoring**: trivial arithmetic. Gamma levels feed a lookup table of log-likelihood weights and a sum, which is bandwidth-trivial if fused.
  - **Clustering**: connected components on the surviving edges is small after thresholding.
- At 80 B/pair (estimate), 1e9 pairs move about 80 GB of traffic, under 0.1 s of HBM bandwidth on an H100-class card. The bottleneck is therefore materialization and VRAM, not bandwidth, once the pipeline is fused. (estimate)
- VRAM capacity for pair indices at 16 B/pair (estimate):
  - 1e9 pairs = 16 GB: fits on 24–80 GB cards.
  - 1e10 = 160 GB: exceeds one GPU.
  - 1e11 = 1.6 TB: must stream.
- An operator design of "probe a chunk of the blocking-key hash join, gather, score, filter, emit" fits a pipelined executor and avoids the explosion. (estimate)

### Gaps
- RapidFuzz single-core pairs/s and cdist throughput figures were not retrieved.
- No measured GPU Levenshtein pairs/s for libcudf `edit_distance` was found.

## Q5. Embedding-similarity joins (cuVS / vector DBs)

### Takeaway
cuVS (formerly RAFT) brute-force / IVF / CAGRA is the natural GPU path for semantic ER as batched kNN. No text-form throughput numbers for a kNN join were obtained.

### Cited Findings
- CAGRA outperforms CPU HNSW in search throughput on DEEP-100M at 95% recall, even at batch size 10. The figures are charts only. — [NVIDIA blog](https://developer.nvidia.com/blog/accelerating-vector-search-using-gpu-powered-indexes-with-rapids-raft/) (accessed 2026-10-29)

### Inferences
- A kNN join (each left row's k nearest right rows) amounts to large-batch search, which is the regime where GPUs shine most. Brute force is GEMM-bound and fine for ≤ ~1e7 × 1e7 at d ≈ 384 with tiling. (estimate)

### Gaps
- cuVS docs for brute-force / CAGRA QPS numbers, and which vector DBs (Milvus GPU, etc.) expose a kNN *join*, were not verified.

## Q6. Native SQL similarity functions in other engines; Sirius; spark-rapids

### Takeaway
DuckDB has a full set of native string-similarity scalars. spark-rapids' supported-ops table has no Levenshtein row (it falls back to CPU). Sirius has no string-similarity code.

### Cited Findings
- DuckDB text functions:
  - `jaro_similarity(s1, s2[, score_cutoff])` and `jaro_winkler_similarity(s1, s2[, score_cutoff])` return 0–1, with 0 below the cutoff.
  - `levenshtein(s1, s2)` (alias `editdist3`) and `damerau_levenshtein(s1, s2)`.
  - `jaccard(s1, s2)`.
  - `hamming(s1, s2)` (alias `mismatches`; equal lengths required).
  - [DuckDB text functions](https://duckdb.org/docs/2.0/sql/functions/text.html) (accessed 2026-10-29)
- spark-rapids `supported_ops.md` (main): the expression section running alphabetically from `Least` to `StringInstr` has no `Levenshtein` and no `Soundex` row. Unlisted expressions fall back to CPU. This was checked by fetching the raw file at offsets 200000–300000; no commit SHA was captured. — [spark-rapids supported_ops.md](https://raw.githubusercontent.com/NVIDIA/spark-rapids/main/docs/supported_ops.md) (accessed 2026-10-29)
- Sirius: case-insensitive grep for levenshtein|jaro|jaccard|edit_distance across .cpp/.cu/.hpp/.h/.md found no matches at `b9e05ff`. — [sirius@b9e05ff](https://github.com/sirius-db/sirius/tree/b9e05ffc653c8ed031c789e5c1415ac1186e0619) (accessed 2026-10-29)

### Inferences
- A DuckDB-compatible function surface (`jaro_winkler_similarity`, `levenshtein`, `jaccard`, `damerau_levenshtein`, `hamming`) is the obvious SQL contract. DataFusion users would recognise it, and Splink already generates SQL that uses these names. (estimate)
- Neither of the visible GPU SQL engines (spark-rapids, Sirius) accelerates these functions, so the gap is open. (estimate)

### Gaps
- The Spark-native `levenshtein(str1, str2[, threshold])` signature, Databricks / Snowflake (`JAROWINKLER_SIMILARITY`, `EDITDISTANCE`) / BigQuery (`EDIT_DISTANCE`) native functions: not verified this session. They are widely documented but need citations before going into a report.
- Whether the DataFusion core / datafusion-functions crate already includes `levenshtein` (believed yes, unverified).
