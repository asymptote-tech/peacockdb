# llm-corpus-sql

## Verdict
- What: a GPU SQL engine with MinHash, n-gram, list/UNNEST and connected-components operators, for exact and fuzzy dedup, filters, blocklist joins and sampling over 0.1–10 TB of text.
- Who: open, mid-size and sovereign pretrainers, plus enterprise mid-training teams. They hit OOMs and broken workflows in NVIDIA's free NeMo Curator. demand.md rates this **medium**, and no one asks for SQL [D].
- Budget: an alpha on one 8-GPU node fits, at about 1.0–1.8× the engine on peacockdb (0.4–0.8× on Sirius) (estimate). But NVIDIA gives the same GPU dedup away, and dedup is a small line in a corpus's GPU bill.

## Who needs it badly
- **demand.md rating: medium.** "About 8 posters hit GPU-curation pain … The pain is real, but it lives inside NVIDIA's free tool. Several builders choose CPU dedup tools instead, and no one asks for SQL" [D].
- **NeMo Curator users with OOMs at exactly this scale** [T1]:
  - **chenrui17**: fuzzy dedup of 8 TB on 5×A100 OOMs. The maintainer says "Internally we've typically used 16-24 GPUs" ([#471](https://github.com/NVIDIA-NeMo/Curator/issues/471), 2025-01-07).
    - In the same thread, **HuaYZhao** OOMs with 15–20 TB on 64×H800. The stale bot closed it unresolved.
  - **ronjer30**: the removal step (an anti-join) OOMs on 24 TB and on 6 TB. The right side is only about 58 GB, against 80 GB HBM and 2 TB of host RAM ([#723](https://github.com/NVIDIA-NeMo/Curator/issues/723), 2025-06-09).
  - **federico-dambrosio**: the Ray driver is OOM-killed on a 1.4 TB-RAM node over FinePDFs, with no best-practice docs ([#1169](https://github.com/NVIDIA-NeMo/Curator/issues/1169), 2025-10-07).
  - **haojiang-scitix** (Scitix): "OOMKilled happens … all the time" on a plain AddId pipeline over 30k JSONL files ([#1443](https://github.com/NVIDIA-NeMo/Curator/issues/1443), 2026-01-30, open).
  - **pham_hung2**: loading 15M records into cuDF to prep for Curator OOMs on 8 GPUs ([forum](https://forums.developer.nvidia.com/t/cudf-spill-on-demand-couldnt-find-memory-device-to-spill-spillmanager-device-memory-limit-spilled-unspilled/332484), 2025-05) [D].
- **Curator workflows that do not run as documented** [T1]:
  - **jmcmanus15**: "FuzzyDeduplicationWorkflow crashes due to multiple bugs". You must start a Ray client first, "which isn't described in the docs" ([#1228](https://github.com/NVIDIA-NeMo/Curator/issues/1228), 2025-11-09).
  - **yuchaoran2011**: "v1.0.0 Fuzzy dedup code doesn't run out of the box" ([#1292](https://github.com/NVIDIA-NeMo/Curator/issues/1292), 2025-12-07).
  - **msiraga**: `ray.shutdown()` breaks the workflow ([#1216](https://github.com/NVIDIA-NeMo/Curator/issues/1216), 2025-11-03).
  - Dependency conflicts are open epics ([#1326](https://github.com/NVIDIA-NeMo/Curator/issues/1326), [#1340](https://github.com/NVIDIA-NeMo/Curator/issues/1340), 2025-12).
  - Maintainers still have open tickets for batch sizes in the shuffle and MinHash stages ([#1332](https://github.com/NVIDIA-NeMo/Curator/issues/1332), [#1465](https://github.com/NVIDIA-NeMo/Curator/issues/1465)).
- **CPU-tool users** [T1]:
  - **dourofficer**: 750 GB on a 256-core, 1 TB box. Stage 1 took 6 h, then "my machine hanged for more than 24 hours … had to reboot" (datatrove [#222](https://github.com/huggingface/datatrove/issues/222), 2024-06-17).
  - **Maghoumi**: datatrove stage 2 degrades at 4–8 TB ([#298](https://github.com/huggingface/datatrove/issues/298), 2024-10-17).
  - SlimPajama's MinHashLSH ran out of memory on RedPajama until rewritten, and then needed 1.4 TB of peak RAM ([arXiv 2309.10818](https://arxiv.org/pdf/2309.10818)) [T1].
  - **Fabulous_Pollution10** builds fine-tuning pipelines for LLM startups: "my GPUs sit idle" ([r/dataengineering](https://www.reddit.com/r/dataengineering/comments/1o0kyd0/), 2025-10) [D].
- **Pattern:** the pain is memory blow-ups in the shuffle, LSH and removal stages, plus Ray operations. Hashing speed is not the pain. Nobody in these threads mentions SQL.
- **Best first conversations (estimate):**
  - **ronjer30** (#723) and **HuaYZhao** (#471): unresolved removal and LSH OOMs at 6–24 TB.
  - **Zyphra** (Zyda-2, built on Curator). They report only a 2× TCO gain [C].
  - **Scitix** (#1443), a GPU cloud whose customers curate data.
  - The **Marin** (Stanford) open lab, with filter OOMs ([marin #7053](https://github.com/marin-community/marin/issues/7053), 2026-07) [T1].
  - **Arcee**, about 10 people, who buy curation from DatologyAI instead [M].

## How it would work
- **Front door:** a CLI or `pip` package that runs one SQL script over a directory of Parquet or JSONL shards and writes Parquet. It is one process per node, with no Ray cluster to bootstrap.
- **SQL, using DuckDB-like list functions** (estimate of the surface):
  ```
  CREATE TABLE sig  AS SELECT id, minhash64(normalize(text), 24, 260, 42) AS h FROM 'raw/*.parquet';
  CREATE TABLE band AS SELECT id, b, hash(list_slice(h, b*13, b*13+13)) AS k
                       FROM sig, UNNEST(range(20)) AS t(b);
  CREATE TABLE edge AS SELECT a.id src, lead(a.id) OVER w dst FROM band a ... ;   -- chain edges per bucket
  CREATE TABLE comp AS SELECT * FROM connected_components('edge');             -- table function
  COPY (SELECT d.* FROM 'raw/*.parquet' d
        ANTI JOIN (SELECT id FROM comp QUALIFY row_number() OVER (PARTITION BY cid ORDER BY id) > 1) x USING (id)
        ANTI JOIN blocklist_urls u ON d.url = u.url
        WHERE lang = 'en' AND length(text) > 200 AND random() < 0.3) TO 'out/' (FORMAT parquet);
  ```
- **Operator mapping.** Curator's own fuzzy dedup is one string kernel (`str.minhash`/`minhash64` in cuDF) followed by table operations [C]:
  - melt to (doc, bucket) rows;
  - hash-shuffle on the bucket id;
  - a group-by keeping buckets with more than one doc;
  - chain edges (built on the CPU in pandas);
  - weakly connected components (cuGraph);
  - a survivor pick and an anti-join (`~isin`).
  - Exact dedup is MD5 plus a group-by [C].
  - So about 5 of the 7 steps are SQL-shaped. peacockdb would call the libcudf MinHash and hash kernels, and cuGraph for the components.
- **Execution:** a partitioned plan over all GPUs in the node. Signatures and the band table are spilled to host RAM, then NVMe.
- **Data path, end to end:**
  - Read: S3 → stage to local NVMe → pinned host → PCIe → GPU decode of Parquet or JSONL.
  - Compute the signatures, which are about 10–25% of the text size [C].
  - Shuffle the band table across GPUs over NVLink.
  - Run connected components on the GPU.
  - Re-read the text for the removal anti-join.
  - Write Parquet back to NVMe or S3.

## The human's questions
**1. Who curates at this size today, with what, and what do they complain about?**
- **NVIDIA NeMo Curator** v1.4.0 (2026-10-09, `166c1c67`) [T1].
  - It runs on Ray (Cosmos-Xenna executor) plus RAPIDS cuDF, cuML and cuGraph. v1.0 (2025-10) replaced Dask.
  - Fuzzy dedup has no CPU path, and the request for one ([#101](https://github.com/NVIDIA-NeMo/Curator/issues/101)) is still open.
  - NVIDIA uses it for Nemotron-CC (6.3T tokens), and Zyphra used it for Zyda-2.
  - Complaints: GPU and host OOMs, undocumented Ray bootstrap, dependency conflicts and idle GPUs ([#59](https://github.com/NVIDIA-NeMo/Curator/issues/59), [#1618](https://github.com/NVIDIA-NeMo/Curator/issues/1618)).
  - An independent KDD'26 paper (FED/SEDD) blames Curator's physical shuffles for "low GPU utilization" and runs up to 7.8× faster ([arXiv 2501.01046](https://arxiv.org/html/2501.01046)).
- **Hugging Face datatrove** v0.10.1 (`4c488a13`) built FineWeb [T1].
  - It is CPU only (no CUDA in the dedup code). MinHash uses 5-grams and 112 hashes in 14×8 bands, with dedup per snapshot.
  - Clustering is one task with up to 30 h and 200 GB scheduled ([fineweb.py](https://github.com/huggingface/datatrove/blob/4c488a135cc582b482df81895e631aadff2fa4c1/examples/fineweb.py)) [C].
  - Complaints: hangs, open-file limits and stage-2 slowdowns (#106, #222, #298, #335).
- **Dolma** (AllenAI) uses a Rust Bloom-filter dedup.
  - **DCLM** recommends its Rust BFF, but its README says that is "not integrable with the ray-based pipelines" [T1].
- **Data-Juicer** (Alibaba) uses CPU Ray MinHash: "Deduplicate 5TB in 2.8h using 1280 cores".
  - v1.6.1 (2026-10-19) adds GPU actor planning and an automatic retry in smaller batches after an OOM, for model ops [T1].
- **Spark:** BigCode deduplicated about 1.4 TB in under 4 h at about $15/h on Dataproc, using Python plus Spark connected components [M]. text-dedup has dropped its Spark script at HEAD [T1].
- **Daft/Eventual** ($30M raised) now sells curation for physical AI (video and sensors) [M][T1].
- **Bodo** sells a CPU+GPU Python/SQL engine with no LLM data-prep pitch [M].
- **DuckDB** has no core MinHash. A 14-star community extension `lsh` exists [M].
- **Not covered:** RedPajama's tooling and Ray Data recipes were not researched (search budget ran out).

**2. Is SQL the interface they want?** No evidence says yes.
- Every production pipeline is Python or YAML: Curator, datatrove, Data-Juicer and BigCode's [M].
  - Spark's MinHashLSH is an ML DataFrame API, not SQL [M] (lightly verified).
- SQL appears for looking at and slicing datasets: the HF SQL Console (DuckDB-WASM, 3 GB cap) and DuckDB `hf://` paths to over 150k datasets [M].
- The only SQL-native MinHash is in ClickHouse (`ngramMinHash`, `wordShingleMinHash`) and the small DuckDB extension [M].
- Modern recipes are dominated by model steps that SQL does not express:
  - quality-classifier ensembles, SemDedup embeddings and LLM rephrasing (Nemotron-CC) [C];
  - so a SQL engine would need UDF and model-inference hooks to replace a pipeline rather than one stage of it.
- DatologyAI's new Curation Studio is a UI over configurable stages, with no SQL or Python [M].
- **The case for SQL** (estimate): it is a single binary with no cluster bootstrap and declarative memory control, which is exactly what the Curator issues lack. That sells "it doesn't OOM", not "it is SQL".

**3. Where is GPU time actually spent?**
- **Hashing is cheap on the GPU.**
  - In Curator's 8×H100 MinHash benchmark, the cuDF file read per task (0.22–0.35 s) costs more than the MinHash compute (0.12–0.24 s).
  - The target is at least 2M docs/s over 1.046B RPv2 docs ([benchmarks.yaml](https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/benchmarking/benchmarks.yaml)) [C].
- **The remaining dedup time is in the LSH all-to-all shuffle and the removal join.**
  - Curator's multi-node configs set `UCX_TLS: all`, and its docs ask for InfiniBand [C].
  - Curator records minhash, LSH and connected-components percentages but does not publish them [C].
- **On the CPU, the LSH index dominates.**
  - Index insert and query are over 85% of MinHashLSH runtime (LSHBloom, [arXiv 2411.04257](https://arxiv.org/html/2411.04257)) [C].
  - Text normalisation and extraction (Trafilatura, jusText) is the largest CPU job by core count [C].
- **Inference dominates the GPU-hour bill.**
  - The FineWeb-Edu classifier took 6,000 H100-hours over 15T tokens [C].
  - Fuzzy dedup of 8 TB takes 2.05 h on one 8×H100 node, about 16 H100-hours [C].
  - So dedup is about 1% of the GPU spend where classifiers are used (estimate).
- **Speed vs cost:** GPU dedup is 10–16× faster in wall time but only about 1.7–2× cheaper, because an H100 node costs about 5× a CPU node.
  - Zyphra: 3 weeks → 2 days, with a 2× lower TCO.
  - Curator README: $315 → $190 [C].

**4. How many teams work at 0.1–10 TB, and do they pay?**
- **Frontier labs:** about 12 developers above 1e25 FLOP (Epoch AI) [M].
  - They build in-house, and their corpora exceed 10 TB of raw crawl (DeepMind's MassiveText is 10.5 TB, deduplicated with MinHash) [M].
  - They are not buyers.
- **Mid-size, open and sovereign pretrainers**, and **enterprise mid-training** (Thomson Reuters used 100B tokens, about 0.4 TB): a few dozen to about 100 organisations worldwide (estimate; no census found) [M].
  - At about 4.5 bytes per token [C], the 0.1–10 TB band is 20B–2T tokens. That suits mid-training, domain models and single-language corpora, not full 15T-token runs.
- **Enterprises fine-tuning:** their corpora are mostly GB-scale, below the band (estimate) [M].
- **Paying:**
  - DatologyAI (about $57.5M raised) sells curation as a service in the customer's VPC. Named customers are Arcee, Thomson Reuters, Deepgram, HRT and "several of the Mag 7". No pricing is public [M][T1].
  - The rest of the field has moved away: Lilac went to Databricks, Cleanlab to Handshake AI, and Nomic and Eventual changed focus [M].
  - The tools themselves are free: Curator, datatrove, Data-Juicer and Dolma are all Apache-2.0.
  - Buyers pay for expertise and results, not for an engine.

**5. How does NVIDIA's overlap (NeMo Curator) shape platform risk?**
- The overlap is direct. Curator is free (Apache-2.0), GPU-native and NVIDIA-staffed [N].
  - Releases come every 10–12 weeks: v1.0 (Ray rewrite), then v1.1 to v1.4 (Lance I/O, RAPIDS 26.08) [M].
  - NVIDIA uses it for its own Nemotron data, so it will not be abandoned [M].
- NVIDIA's AI Enterprise lists $4,500 per GPU per year. Whether it covers Curator is unverified [M].
- **The gap Curator leaves** is operational: memory blow-ups, Ray bootstrap and dependency hell (question 1).
  - NVIDIA is fixing these in the open (#1332, #1465). Data-Juicer already retries in smaller batches after an OOM [T1].
  - So the window is one or two Curator releases wide (estimate).
- Any GPU kernel advantage is NVIDIA's own: peacockdb would call the same libcudf MinHash and cuGraph components.

## Data path and cache
| Stage | Rate | Source |
|---|---|---|
| Ingest needed, Curator MinHash on 8×H100 | ≥2M docs/s ≈ 8–10 GB/s of text per node (estimate at 4–5 KB/doc) | [C] |
| Whole fuzzy pipeline, 8 TB on 4 nodes in 0.5 h | ≈4.4 GB/s aggregate, ≈1.1 GB/s per node | [C] |
| Object store per GPU on 8-GPU node | ≈1.5–6 GB/s compressed (NIC-bound) | [L] |
| Local NVMe, p5-class (8 drives) | ≈18–50 GB/s per node (estimate) | [L] |
| PCIe H2D pinned / pageable, H200 | 47.3 / 10.6 GB/s | [I] |
| Parquet decode on GPU, engine today | 6.8–8.7 GB/s per GPU | [I] |
| HBM read, H200 | 4,484 GB/s | [I] |
| Signatures | ≈0.9–1 KB/doc, 10–25% of text | [C] |
| LSH (doc, bucket) table | ≈320 B/doc; ≈0.33 TB at 1B docs (estimate) | [C] |

- **This is a batch job that reads each byte two or three times:** once for signatures, once for removal, plus filters. It is not repeated interactive queries. A VRAM cache of hot tables, which is the thesis's cure, buys little here.
- **The thesis holds in a different form.**
  - Reading from S3 bounds the run. 8–10 GB/s per node needs about 2–3 GB/s per GPU from the NIC, near its ceiling [L].
  - Decode does not bound it: 7–9 GB/s per GPU covers it with 8 GPUs.
  - datatrove even staggers its starts so it does not hammer S3's list requests [C].
- **The caches needed:**
  - (a) Stage the corpus on local NVMe once, so the second pass (removal) reads at NVMe speed.
  - (b) Keep the intermediates (signatures, band table and edges, about 10–30% of the text) in pinned host RAM, spilling to NVMe.
  - (c) Keep VRAM for the active shuffle partition only.
  - The Curator OOMs are failures of (b) and (c) [T1].
- **The real bottleneck at 1–10 TB is the shuffle and the memory, not bandwidth.**
  - The band table is GPU-sized only per partition. Multi-node runs need InfiniBand [C].
  - The alpha should stay on one node of 8 GPUs, which handled 8 TB in 2 h in Curator [C], so NVLink carries the shuffle.
- **Engine today:** none of this exists. There is no spill, no multi-GPU in the engine, pageable host buffers, a slow `read_parquet` (#232), and no writes [I].

## Build cost to alpha
Sizes are in yardstick lines; the unit is 22,830 [I]. Every figure is an estimate.

| Component | On peacockdb: reuse/adapt/new, size | On Sirius: reuse/adapt/new, size |
|---|---|---|
| SQL frontend, translator, node set, filter/project/group-by/join operators | reuse [I] | reuse (DuckDB planner and list functions) [S] |
| Anti/semi join on large keys (removal, blocklists) | reuse the join session (all join types) [I] | adapt: anti joins are hard errors today, 1–2k [S] |
| Front door: CLI or `pip`, GPU entry point, multi-statement script | new: 1.5–2.5k; no GPU entry point today [I] | reuse DuckDB CLI/Python [S] |
| String functions on device (lower, regexp_replace, length, split, n-grams) | adapt through plan, wire and executor: 2–4k | adapt: LIKE likely fails to bind; 1–2k [S] |
| MinHash, MD5/xxhash UDFs over cuDF kernels | new: 1–2k | new: 1–2k |
| List columns, UNNEST, list_slice, array_agg | new: 2–3k | adapt: 1.5–3k |
| Connected components (table function over cuGraph, or iterated min-label join) | new: 1.5–3k | new: 1.5–3k |
| JSONL reader, Parquet writer, COPY TO | new: 2–3k; read-only today [I] | adapt: results return to DuckDB on the host; GPU write 1–2k |
| Spill: GPU → host → NVMe for shuffle, join and group-by state | adapt cuCascade, partitioned operators: 5–10k [S] | reuse (Grace-style operators) [S] |
| Multi-GPU single node: worker per GPU, NVLink shuffle | new: 3–6k; test binaries only [I] | reuse (single-node multi-GPU on main) [S] |
| Fill path: chunked reader, pinned buffers, object store, NVMe staging | adapt: 1.5–3k (#149, #232) [I] | reuse (kvikio, io_uring, S3) [S] |
| Sampling, n-gram decontamination join | new: 1–2k | new: 1–2k |
| Tests (dedup recall against datasketch, determinism) | adapt the corpus harness: 1–2k | adapt: 1–2k |
| **Total** | **~22–41k** | **~9–18k** |

Total new work: 1.4× engine on peacockdb, 0.6× on Sirius (estimate)

- Ranges: 1.0–1.8× on peacockdb and 0.4–0.8× on Sirius.
- **The hard part:** running out of core across 8 GPUs without OOMs. That means a spilling shuffle on a high-cardinality hash key, a group-by that collects lists, and an anti-join where the corpus is far bigger than HBM.
  - This is exactly where Curator fails [T1], and it is the half of the work peacockdb lacks (spill plus multi-GPU, 8–16k) [I][S].
  - Sirius has spill and multi-GPU, but they are young. Task state never spills, and no published run measures active spilling [S].
- **On Sirius:** the cost is lower, but Sirius churns by about 50–60k lines a week, and NVIDIA staff are its top committers. A fork is out [S][N].

## Adoption friction
- **High.**
  - Users would rewrite Python, YAML or Ray pipelines into SQL. Nobody asks for SQL [D][M].
  - Classifiers, embeddings and LLM rewriting stay in Python, so the product replaces only the dedup and filter stages. Users must then glue stages through files, the same complaint DCLM's README makes about its Rust dedup [T1].
  - Curation teams are ML engineers who trust datasketch-style semantics. A new engine must show the same recall and precision as Curator or datatrove before anyone swaps it in (estimate).
- **In its favour:**
  - It is a single binary with no Ray cluster. That fixes #1228 and #1292 by construction.
  - It reads and writes Parquet, so one stage can be swapped at a time.

## Platform risk
- **High.**
  - NeMo Curator is the same product category from NVIDIA. It is free, actively released, dogfooded for Nemotron and built on the same libcudf and cuGraph kernels [N][M][C].
  - nvidia.md lists Curator v1.4.0 (2026-10-09), Apache-2.0 [N].
- NVIDIA's pattern is to give engines away and sell GPUs and AI Enterprise [N]. A startup cannot undercut free on the engine.
- The defensible ground left (estimate):
  - the operational simplicity Curator lacks;
  - non-NVIDIA hardware, though Curator is CUDA-only and so is this engine;
  - curation as a service, which DatologyAI already holds with about $57.5M raised [M].
- Data-Juicer (Alibaba) is adding GPU scheduling and OOM retries [T1]. Ray is the common layer under Curator, datatrove and Data-Juicer [M].

## Funding story
- **What a design partner would say** (estimate):
  - "Curator OOMs on our 6–24 TB removal step and the Ray setup is fragile. If yours finishes on one 8-GPU node without tuning, we will try it for dedup."
  - "But our GPU bill is classifiers and synthetic data, not dedup. On cost, GPU dedup only halves CPU TCO."
- **What a round would be raised on:** "the reliable data engine for LLM corpora". The comparable is DatologyAI's $46M Series A, which sold expertise and outcomes, not an engine [M].
  - The alpha's evidence would be "8 TB fuzzy dedup on one node with no OOM, the same duplicates as Curator".
  - The investor's first question is why NVIDIA's next Curator release won't fix the OOMs for free.

## Evidence against
- **The pain lives inside a free NVIDIA tool,** which NVIDIA is fixing in the open [D][T1].
- **Nobody asks for SQL.** Pipelines are Python and YAML, and modern recipes are dominated by model steps [M][C].
- **Dedup is a small cost line.** It takes about 16 H100-hours for 8 TB, against 6,000 H100-hours for one classifier pass [C].
  - GPU dedup is only about 2× cheaper than CPU [C].
  - CPU tools keep improving: LSHBloom is 12× faster, and Data-Juicer does 5 TB in 2.8 h on 1,280 cores [C][T1].
- **The market is small, and buyers pay for curation services, not engines.** It is a few dozen to about 100 teams (estimate). Most curation startups pivoted or were absorbed [M].
- **The alpha needs the two things peacockdb lacks most,** spill and multi-GPU. That is the upper end of the budget [I].
- **Any kernel advantage is NVIDIA's own.** It is the same libcudf `minhash64` and the same cuGraph [C].

## Ratings
- **Who needs it badly: medium.** demand.md rates it medium: about 8 posters with OOMs in Curator, but nobody asks for SQL [D].
- **Build cost: medium.** About 1.4× (1.0–1.8×) on peacockdb, which fits only at the top of the budget. Spill and multi-GPU are the bulk; it is 0.6× on Sirius.
- **Data path: medium.** It is a batch job that reads each byte two or three times, bound by S3 reads and the shuffle. It needs NVMe staging and a host-RAM spill for the intermediates, not a VRAM hot cache.
- **Adoption friction: weak.** Users must rewrite Python pipelines into SQL, and model steps stay outside. It replaces only one stage.
- **Platform risk: weak.** NeMo Curator is free, active, dogfooded by NVIDIA and built on the same kernels.
- **Funding story: weak.** The money in this space went to a curation service (DatologyAI), not an engine. "No OOM" is an alpha claim that NVIDIA can erase in one release.

## Follow-up experiments
- Run Curator fuzzy dedup on 1 TB and 8 TB of RPv2 on one 8×H100 node, with stage timers on. This gets the unpublished minhash/LSH/CC/removal split and peak HBM and host memory [C].
- Reproduce the #723 removal OOM (an anti-join of 6 TB against 58 GB of ids). Then measure a spilling GPU anti-join on peacockdb plus cuCascade against it.
- Measure GPU decode of JSONL.zst against Parquet for long text columns, in GB/s per GPU.
- Measure the S3 → NVMe staging rate on p5 against the 8–10 GB/s that MinHash consumes.
- Run recall and precision against datasketch on a labelled near-duplicate set (e.g. NEWS-COPY), to show semantic parity.
- Run five interviews from the first-conversations list, asking one question: is dedup cost or reliability their top pain, compared with classifier inference?

## Sources
- [D] demand.md `## Ratings` row llm-corpus-sql, and sources/discord-forums.md and sources/reddit.md rows (local, master 9d61cac8; accessed 2026-10-29)
- [I] inventory.md at f25c250d, incl. yardstick and benchmark evidence (local; accessed 2026-10-29); [S] sirius.md, Sirius @ b9e05ffc (local; accessed 2026-10-29); [N] nvidia.md rows [45][46], stance section (local; accessed 2026-10-29); [L] options/lakehouse-iceberg.md NIC/NVMe figures (local; accessed 2026-10-29)
- [T1] Tools and complaints (accessed 2026-10-29):
  - NeMo Curator:
    - Releases and README @ v1.4.0 `166c1c676af8360f2e3f151e9482a001da326ea1`: https://github.com/NVIDIA-NeMo/Curator/releases ; https://github.com/NVIDIA-NeMo/Curator/blob/v1.4.0/README.md
    - Issues: https://github.com/NVIDIA-NeMo/Curator/issues/ 59, 101, 471, 723, 1169, 1216, 1228, 1292, 1326, 1332, 1340, 1443, 1465, 1618
  - datatrove v0.10.1 `4c488a135cc582b482df81895e631aadff2fa4c1`: https://github.com/huggingface/datatrove ; issues 106, 222, 298, 335 under https://github.com/huggingface/datatrove/issues/
  - Dolma `669f534823b0`: https://github.com/allenai/dolma ; DCLM `361714bdd60b`: https://github.com/mlfoundations/dclm ; Data-Juicer `016e76050297`: https://github.com/modelscope/data-juicer ; text-dedup `7538f3ec6a28`: https://github.com/ChenghaoMou/text-dedup
  - Papers and other: SlimPajama https://arxiv.org/pdf/2309.10818 ; FED/SEDD https://arxiv.org/html/2501.01046 ; Marin https://github.com/marin-community/marin/issues/7053 ; Eventual https://www.eventual.ai/blog/eventual-raises-30m-to-build-the-future-of-data ; DatologyAI https://www.datologyai.com/blog/datologyai-raises-46m-series-a
- [C] Compute and data path (accessed 2026-10-29):
  - Curator @ `166c1c67`:
    - Benchmarks: https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/benchmarking/benchmarks.yaml
    - LSH stage: https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/nemo_curator/stages/deduplication/fuzzy/lsh/lsh.py
    - Removal: https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/nemo_curator/stages/text/deduplication/removal.py
    - Connected components: https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/nemo_curator/stages/deduplication/fuzzy/connected_components.py
    - Throughput docs: https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/fern/versions/main/pages/about/concepts/throughput.mdx
    - Fuzzy dedup docs: https://github.com/NVIDIA-NeMo/Curator/blob/166c1c676af8360f2e3f151e9482a001da326ea1/fern/versions/main/pages/curate-text/process-data/deduplication/fuzzy.mdx
  - cuDF minhash @ `785ee522`: https://github.com/rapidsai/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/python/cudf/cudf/core/accessors/string.py
  - datatrove FineWeb example: https://github.com/huggingface/datatrove/blob/4c488a135cc582b482df81895e631aadff2fa4c1/examples/fineweb.py
  - Papers: FineWeb https://arxiv.org/html/2406.17557 ; LSHBloom https://arxiv.org/html/2411.04257 ; Nemotron-CC https://arxiv.org/html/2412.02595
  - Zyphra: https://www.zyphra.com/post/building-zyda-2 ; NVIDIA 2023 blog: https://developer.nvidia.com/blog/curating-trillion-token-datasets-introducing-nemo-data-curator/ ; RPv2: https://huggingface.co/datasets/togethercomputer/RedPajama-Data-V2
- [M] Market, SQL and NVIDIA (accessed 2026-10-29):
  - Market size: Epoch AI https://epoch.ai/data-insights/models-over-1e25-flop ; Gopher https://arxiv.org/pdf/2112.11446
  - BigCode dedup: https://huggingface.co/blog/dedup
  - Vendors:
    - DatologyAI: https://www.datologyai.com/ ; https://www.datologyai.com/blog/introducing-datology-curation-studio
    - Others: Eventual https://www.eventual.ai/ ; Lilac https://www.databricks.com/blog/lilac-joins-databricks-simplify-unstructured-data-evaluation-generative-ai ; Cleanlab https://cleanlab.ai/ ; Nomic https://www.nomic.ai/ ; Bodo https://www.bodo.ai/
  - NVIDIA AI Enterprise pricing: https://docs.nvidia.com/ai-enterprise/planning-resource/licensing-guide/latest/pricing.html
  - SQL interfaces:
    - ClickHouse hash functions: https://clickhouse.com/docs/sql-reference/functions/hash-functions
    - DuckDB: https://github.com/princeton-ddss/lsh ; https://duckdb.org/2024/05/29/access-150k-plus-datasets-from-hugging-face-with-duckdb.html
    - HF SQL Console: https://huggingface.co/blog/sql-console
    - Spark ML features: https://spark.apache.org/docs/latest/ml-features.html
  - Nemotron-CC blog: https://developer.nvidia.com/blog/building-nemotron-cc-a-high-quality-trillion-token-dataset-for-llm-pretraining-from-common-crawl-using-nvidia-nemo-curator/
- Gaps (search budget exhausted during research): no Reddit or HN pass beyond demand.md; RedPajama tooling, Ray Data recipes, Snowflake and Anyscale were not covered; Curator's per-stage split is unpublished.
