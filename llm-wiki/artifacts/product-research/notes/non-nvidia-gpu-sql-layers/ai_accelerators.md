# SQL / relational operators on AI accelerators (Cerebras, Groq, TPU, SambaNova, Graphcore, Tenstorrent, Trainium), as of 2026-10-29

All URLs accessed 2026-10-29. I made about 20 tool calls. Search-engine summaries were cross-checked against primary pages where possible. Where only a secondary source exists, that is flagged.

## Q1: Existing SQL or dataframe engines on these chips

### Takeaway
The only published SQL-on-AI-accelerator result is Microsoft's TQP (VLDB 2022). It compiles SQL to PyTorch tensor programs and ran one TPC-H query (Q6, SF1) on a TPU via PyTorch/XLA, at 25 ms against 1 ms on a V100. The paper itself says PyTorch/XLA lacks needed ops and cannot fall back to the CPU. I found no SQL or dataframe engine, paper or product for Cerebras, Groq, SambaNova, Tenstorrent or Trainium. On Graphcore IPU, the closest work is graph traversal (BFS, PageRank), not joins or sort.

### Cited Findings
**TQP (Microsoft / UW / UCSD)**
- "Query Processing on Tensor Computation Runtimes", by He, Nakandala, Banda, Sen, Saur, Park, Curino, Camacho-Rodríguez, Karanasos and Interlandi. PVLDB 15(11): 2811–2825, 2022. It compiles SQL into tensor programs on PyTorch, TVM and ONNX Runtime, runs the full TPC-H benchmark, and claims up to 10× over CPU-only and GPU-only systems. — [arXiv 2203.01877](https://arxiv.org/abs/2203.01877) (accessed 2026-10-29)
- TPU result (Sec. 6.9 Portability, Table 4): TPC-H Q6 at SF1 with a hand-optimized plan took 25 ms on a TPU (PyTorch on XLA). The other devices took 1 ms (V100), 5 ms (K80), 17 ms (AMD Radeon Pro 5300M), 62 ms (Intel UHD 630) and 1900 ms (Chrome/browser). — [ar5iv 2203.01877](https://ar5iv.labs.arxiv.org/html/2203.01877) (accessed 2026-10-29)
- The paper says "PyTorch/XLA does not support all the necessary tensor operations" and "the execution fallback to regular CPU for part of the query is not available." On TPU, only Q6 is shown. — [ar5iv 2203.01877](https://ar5iv.labs.arxiv.org/html/2203.01877) (accessed 2026-10-29)
- The TQP demo paper says users can run queries on "CPU, GPU, TPU". — [arXiv 2209.04579](https://arxiv.org/pdf/2209.04579v1.pdf) (accessed 2026-10-29)
- In a 2023 keynote, Carlo Curino (Microsoft) said TQP runs SQL on GPUs and APUs from several vendors, and "with limited effort we can port to TPUs or other custom tensor-based devices". This is a portability claim, not a benchmark. — [CEUR-WS Vol-3478 keynote03](https://ceur-ws.org/Vol-3478/keynote03.pdf) (accessed 2026-10-29)
- Follow-on work is GPU-only:
  - "Terabyte-Scale Analytics in the Blink of an Eye" is a distributed TQP on multi-GPU clusters. — [arXiv 2506.09226](https://arxiv.org/html/2506.09226v2) (accessed 2026-10-29)
  - "GPU Acceleration of SQL Analytics on Compressed Data" uses PyTorch tensor programs and is described as portable across accelerators, but is evaluated on an A100. — [arXiv 2506.10092](https://arxiv.org/html/2506.10092v2) (accessed 2026-10-29)

**TCUDB (matrix-unit joins)**
- TCUDB (Hu, Li, Tseng; arXiv 2112.07552, SIGMOD 2022) runs natural joins and group-by aggregates as matrix multiplications on NVIDIA Tensor Cores. It reports up to 288× over a GPU query-engine baseline. The authors note that dense matmul joins don't always pay off, because the operands can be sparse. This is the strongest evidence that a systolic or matrix unit *can* do joins. It targets NVIDIA TCUs, not TPUs. — [DeepAI TCUDB](https://deepai.org/publication/tcudb-accelerating-database-with-tensor-processors) (accessed 2026-10-29)

**Google / BigQuery**
- I found no evidence that BigQuery or any Google SQL engine executes on TPUs. Searches for TPU SQL, BigQuery TPU and "run SQL on TPU" returned only TQP. See Gaps.

**Graphcore IPU**
- iPUG (Burchard, Cai, Langguth; HiPC 2021) is a Graph500 BFS on 8 IPUs. It reports about 2× the performance of the same number of V100s running state-of-the-art CUDA code. — [Simula](https://www.simula.no/publications/ipug-multiple-graphcore-ipus-optimizing-performance-and-scalability-parallel-breadth) (accessed 2026-10-29)
- iPUG single-IPU BFS paper, in ISC High Performance 2021 (Springer). — [SINTEF](https://www.sintef.no/en/publications/publication/2324074) (accessed 2026-10-29)
- Gepner et al. evaluated PageRank, SSSP and BFS on the IPU against CPUs and GPUs and report that the IPU outperformed them. The arXiv ID 2002.11670 comes only from an aggregator and is unverified. — [Bohrium mirror](https://bohrium.dp.tech/paper/arxiv/2002.11670) (accessed 2026-10-29)
- I found no IPU hash-join, sort or SQL paper at DaMoN or elsewhere. DaMoN 2023 sort and join papers target FPGAs, not IPUs. — [dblp DaMoN 2023](https://dblp1.uni-trier.de/db/conf/damon/damon2023.html) (accessed 2026-10-29)

**Cerebras WSE**
- Published non-AI work on the WSE covers stencils, sparse matrix kernels and dataflow languages. None of it is about databases or sorting:
  - "Stencil Computations on Cerebras Wafer-Scale Engine". — [arXiv 2605.07954](https://arxiv.org/pdf/2605.07954) (accessed 2026-10-29)
  - "Exploring Sparse Matrix Multiplication Kernels on the Cerebras CS-3". It describes PEs with 48 KB local memory each and a router to the four neighbours. — [arXiv 2604.27985](https://arxiv.org/pdf/2604.27985) (accessed 2026-10-29)
  - SPADA spatial dataflow language (arXiv 2511.09447) and "Beyond Exascale: Dataflow Domain Translation on a Cerebras Cluster" (arXiv 2511.11542). These are listed in an HLRS April 2026 talk; I did not open the papers. — [HLRS slides](https://www.wssp.hlrs.de/wp-content/uploads/2026/04/20260423_schaefer.pdf) (accessed 2026-10-29)
- I found no database, hash-join or sort work on the WSE.

**SambaNova and Groq**
- I found no SQL, dataframe or analytics offerings or papers. Coverage of both is entirely LLM training and inference. — [IntuitionLabs comparison (secondary)](https://intuitionlabs.ai/articles/cerebras-vs-sambanova-vs-groq-ai-chips) (accessed 2026-10-29)

**Tenstorrent tt-metal**
- Tenstorrent positions TT-Metalium for "non-ML and ML use cases" and for customers who want to "run non-machine learning code". — [Tenstorrent TT-Metalium](https://tenstorrent.com/software/tt-metalium); [tt-metal docs](https://docs.tenstorrent.com/tt-metal/latest/tt-metalium/get_started/get_started.html) (accessed 2026-10-29)
- The documented examples are DRAM loopback, eltwise binary and matmul. I found no sort, hash or database kernels via search. I did not grep the repo, so no commit SHA is recorded.

**AWS Trainium NKI**
- NKI is open source and supports Trainium 1/2/3 and Inferentia2. It became Stable (v0.3.0) in Neuron 2.29 in April 2026, and Neuron 2.30 shipped NKI 0.4.0. AWS frames the SDK around AI training and inference. — [NKI about](https://awsdocs-neuron.readthedocs-hosted.com/en/latest/nki/get-started/about/index.html); [NKI architecture](https://awsdocs-neuron.readthedocs-hosted.com/en/v2.28.1/nki/guides/architecture/index.html); [Softprom summary of 2.29](https://softprom.com/aws-neuron-sdk-2290-nki-stable-cpu-simulator-expanded-library) (accessed 2026-10-29)
- I found no NKI sort, join or SQL work.

### Inferences
- The only realistic route today is TQP-style compilation of SQL to tensor ops, run through each vendor's PyTorch or XLA frontend. That route showed a TPU 25× slower than a 2017 V100 on the easiest TPC-H query (Q6: scan, filter, sum). (estimate: no newer TPU numbers exist)
- Graph traversal on the IPU is the closest "irregular data" success. It suggests the IPU's MIMD tiles with large SRAM could handle hash joins. Graphcore was acquired by SoftBank in 2024 and the IPU line is effectively legacy (estimate; not verified in this session).

### Gaps
- I found no evidence on whether Google uses TPUs inside BigQuery or F1/Napa. A null result here is not proof of absence.
- I did not repo-grep tt-metal or nki-samples, so I have no SHAs. A `git grep -i "sort\|hash"` over ttnn ops would settle whether a sort op exists.
- I did not open the Cerebras CS-3 datasheet, so external I/O bandwidth is unconfirmed.

## Q2: Architectural fit of relational operators

### Takeaway
Scan, filter, projection and simple aggregation map well onto tensor ops. Hash joins, high-cardinality group-by, variable-length strings and anything with data-dependent output size fit poorly. The reasons are static shapes (XLA, Groq), small per-core SRAM with no shared hashable memory (Cerebras 48 KB/PE, Groq ~230 MB/chip), and missing CPU fallback. TQP had to turn joins and group-by into sort-based tensor algorithms and strings into zero-padded char matrices. TPU v7's 192 GiB HBM at 7.4 TB/s is the only design here with GPU-class memory capacity.

### Cited Findings
**How TQP maps operators to tensors** — [ar5iv 2203.01877](https://ar5iv.labs.arxiv.org/html/2203.01877) (accessed 2026-10-29)
- Filters are bitmap based: `lt` plus `masked_select`.
- Group-by is sort-based: concatenate the keys, sort, `uniqueConsecutive`, then aggregate over the inverse indexes.
- There are two joins. The sort-based join sorts keys, builds a histogram, takes a prefix sum, then calls `bucketize`. The hash join interleaves build and probe, repeated "as many times as the maximum number of elements" sharing a hash value. The sort-based join is faster once more than 15 elements collide.
- Strings become "(n×m) numeric tensors, where m is the maximum character length", with one character per column, right-padded with 0s. "TCRs commonly do not support strings or date data types."
- Decimals are unsupported; doubles are used instead. The paper notes "nonzero requires host/device synchronization", and most GPU time goes to "sorting, unique and nonzero".

**Static shapes (TPU/XLA)**
- XLA requires array shapes to be known at compile time, so data-dependent-size ops (`unique`, `nonzero`, `union1d`) fail under `jit` unless a static `size=` is given. Results are truncated or padded to that size. A join, filter or group-by output whose cardinality depends on the data therefore needs a worst-case buffer. — [JAX jnp.unique docs](https://docs.jax.dev/en/latest/_autosummary/jax.numpy.unique.html); [jax.numpy module](https://docs.jax.dev/en/latest/jax.numpy.html) (accessed 2026-10-29)

**Groq**
- Groq states that its compiler enables "static scheduling and deterministic execution" and that SRAM is primary storage, not cache. — [Groq LPU architecture (via scour mirror)](https://scour.ing/@matmat/p/https://groq.com/lpu-architecture) (accessed 2026-10-29)
- The current groq.com page quotes rack-level figures: "128 GB of on-chip SRAM per rack" and "40 PB/s SRAM bandwidth". — [groq.com/lpu-architecture](https://groq.com/lpu-architecture) (accessed 2026-10-29)
- The ~230 MB SRAM per chip figure comes from secondary sources only. — [WebProNews](https://www.webpronews.com/the-deterministic-bet-how-groqs-lpu-is-rewriting-the-rules-of-ai-inference-speed) (accessed 2026-10-29)

**Cerebras WSE-3**
- 900,000 cores. — [cerebras.ai/chip](https://www.cerebras.ai/chip) (accessed 2026-10-29)
- 44 GB on-chip SRAM, 21 PB/s memory bandwidth, 214 Pb/s fabric, and 48 KB SRAM per core. These figures come from secondary sources; the Cerebras page did not show them. — [Introl](https://introl.com/blog/cerebras-wafer-scale-engine-cs3-alternative-ai-architecture-guide-2025); [arXiv 2604.27985](https://arxiv.org/pdf/2604.27985) (accessed 2026-10-29)
- Weights stream from external MemoryX. — [Introl](https://introl.com/blog/cerebras-wafer-scale-engine-cs3-alternative-ai-architecture-guide-2025) (accessed 2026-10-29)
- A commentator argues the WSE excels at dense linear algebra but "may not be the best fit" for irregular access such as graph analytics. This is opinion. — [Bhavana T. Substack](https://bhavanat.substack.com/p/cerebras-harnessing-the-massive-scale) (accessed 2026-10-29)

**Google TPU HBM per chip** — [Google Cloud TPU7x docs](https://docs.cloud.google.com/tpu/docs/tpu7x) (accessed 2026-10-29)

| Chip | HBM per chip | Bandwidth | Peak BF16 |
|---|---|---|---|
| TPU7x (Ironwood) | 192 GiB | 7,380 GB/s | 2,307 TFLOPs |
| v6e (Trillium) | 32 GiB | 1,638 GB/s | 918 TFLOPs |
| v5p | 95 GiB | 2,575 or 2,765 GB/s | not captured |

The v5p bandwidth differs between language versions of the page.

**Graphcore IPU**
- The IPU has no traditional cache hierarchy. A university thesis argues that graph algorithms with irregular access may suit it. — [Heidelberg thesis (Alles)](https://archiv.ub.uni-heidelberg.de/volltextserver/38593/1/Alles_Christian.pdf) (accessed 2026-10-29)

**Joins as matmul**
- TCUDB caveat: dense matmul joins can lose when the data is sparse. — [DeepAI TCUDB](https://deepai.org/publication/tcudb-accelerating-database-with-tensor-processors) (accessed 2026-10-29)

### Inferences
- **TPU.** This is the best fit of the group: large HBM, XLA sort and scatter primitives, and an existing PyTorch/JAX route. It is still hampered by static shapes (worst-case padding on joins and filters), weak integer and string throughput relative to its MXU, and no CPU fallback (estimate).
- **Groq.** Fully static schedules and on the order of 100s of MB SRAM per chip make data-dependent hash tables and variable-cardinality outputs essentially infeasible. A TB-scale table would need thousands of chips just to hold it (estimate: 1 TB / 230 MB ≈ 4,350 chips).
- **Cerebras.** 44 GB of SRAM is spread as 48 KB per PE, and access is mesh message passing. A hash join would have to be a partitioned, radix-style shuffle across the fabric. That is plausible as a research kernel in CSL but is not supported by any toolchain. Data that doesn't fit must stream through host I/O (estimate).
- **Tenstorrent and Trainium.** These have the most general programming models (RISC-V data-movement cores in tt-metal; NKI's ISA-level API), so custom sort and partition kernels are possible. Nobody has published any (estimate).

### Gaps
- I found no academic paper that explicitly analyses relational operators on systolic or dataflow accelerators beyond TQP and TCUDB.
- I could not confirm host-ingest bandwidth per chip for Cerebras CS-3, Groq or Trainium from primary docs.
- I did not verify gather/scatter/atomics capability for each chip.

## Q3: Demand: who asks for SQL on these chips

### Takeaway
I found no users, forum threads, HN stories or vendor programs asking for SQL or dataframes on Cerebras, Groq, TPU, SambaNova, Graphcore, Tenstorrent or Trainium. All interest is academic (Microsoft's TQP line, which moved on to GPUs only) or Graphcore-era graph research.

### Cited Findings
- An HN Algolia story search for "SQL TPU" returned no relevant story. The closest hit (PromptTools, objectID 36958175) mentions TPUs only for running models. — [HN Algolia API query](https://hn.algolia.com/api/v1/search?query=SQL%20TPU&tags=story) (accessed 2026-10-29)
- Vendor and comparison coverage of Cerebras, SambaNova and Groq is entirely about LLM training and inference, with no analytics positioning. — [IntuitionLabs](https://intuitionlabs.ai/articles/cerebras-vs-sambanova-vs-groq-ai-chips) (accessed 2026-10-29)
- Microsoft's TQP follow-ups (2025) target multi-GPU clusters, not TPUs. — [arXiv 2506.09226](https://arxiv.org/html/2506.09226v2) (accessed 2026-10-29)
- AWS describes the Neuron SDK's primary goal as AI training and inference. — [NKI about](https://awsdocs-neuron.readthedocs-hosted.com/en/latest/nki/get-started/about/index.html) (accessed 2026-10-29)

### Inferences
- Demand for SQL on AI accelerators appears to be effectively zero outside research. A plausible niche is "SQL + ML inference in one pipeline" on TPU (TQP claimed up to 9× on such mixed queries vs CPU), but no one is publicly asking for it (estimate).

### Gaps
- I searched HN only with "SQL TPU". Reddit (r/LocalLLaMA, r/dataengineering), Cerebras and Groq community forums, and Tenstorrent Discord were not searched.
- I did not search for or check any SambaNova data-analytics material beyond the general comparison.
