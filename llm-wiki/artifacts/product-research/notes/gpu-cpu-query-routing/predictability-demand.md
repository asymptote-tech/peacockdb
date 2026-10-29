# GPU vs CPU query routing: predictability and demand (gpu-cpu-router), as of 2026-10

All sources accessed 2026-10-29. I ran out of web-search budget partway through, so I covered HeavyDB, Theseus, Presto-GPU and spark-rapids small-data docs only lightly (see Gaps). The ClickBench numbers below are my own arithmetic on the raw result JSONs in the ClickBench repo. They are not benchmarks I ran.

## 1. Break-even points, fixed overheads, winning and losing query shapes

### Takeaway
Whether the data is already in GPU memory decides the outcome more than anything else. With data resident, Sirius beats DuckDB on every ClickBench query (about 11-18× geomean). Including the cold first run, the same Sirius results lose to DuckDB on half or more of the queries (geomean 0.1-0.5×). Small, wide, selective and string-heavy queries lose. Joins and grouped aggregations win.

### Cited Findings
- ClickBench Sirius, hot runs (best of runs 2-3), 2026-03-09, versus DuckDB 2026-05-11 (my arithmetic): GH200 is 18.5× geomean faster than DuckDB on c6a.metal, 17.0× versus c7a.metal-48xl and 12.2× versus c8g.metal-48xl. p5.4xlarge (1×H100) is 17.1×, 15.7× and 11.3×. The GPU wins 42/42 comparable queries in every pairing. Its fastest hot queries take 1-3 ms, so a warm per-query fixed cost is in the low ms. — [Sirius GH200 result](https://github.com/ClickHouse/ClickBench/blob/main/sirius/results/20260309/lambda-GH200.json), [Sirius p5 result](https://github.com/ClickHouse/ClickBench/blob/main/sirius/results/20260309/p5.4xlarge.json), [DuckDB results](https://github.com/ClickHouse/ClickBench/tree/main/duckdb/results/20260511) (accessed 2026-10-29)
- Same files, cold run (run 1): GH200 Sirius geomean is 0.51× versus DuckDB c6a.metal and wins 23/43. Summed time is 208.5 s versus 162.9 s. Against the much smaller c6a.4xlarge it is 0.39× and wins 18/43. p5.4xlarge Sirius cold is 0.07-0.10× and wins 0-1/43, with 977.9 s summed versus 115-163 s. Sirius load time is 25 s on GH200 and 237 s on p5. — same sources (accessed 2026-10-29)
- Worst cold losers on GH200 versus DuckDB c6a.metal:
  - Q24 `SELECT * … WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10`: 68.4 s versus 2.9 s.
  - Highly selective `CounterID = 62` queries Q37-Q43: 1.4-13.9 s versus 0.06-0.19 s. These are tiny results that still pay the column transfer.
  - [queries.sql](https://github.com/ClickHouse/ClickBench/blob/main/sirius/queries.sql), result JSONs above (accessed 2026-10-29)
- The Sirius paper reports hot runs only: "The numbers reported are the hot runs when the data is already cached in the GPU memory." Cold costs occur "only during the cold run (initial data load) and when returning results." TPC-H was tested only at SF100.
  - Weak spots it names: string `contains` (q23), top-N (q24, q26), very large aggregations (q27), group-by on string keys and low-cardinality group-bys.
  - It assumes the input fits in the caching region, which is 50% of GPU memory.
  - Headline claims: 8.3× cost efficiency on TPC-H and 7.4× on ClickBench versus CPU engines; up to 12.5× versus Doris distributed (Q1 12.5×, Q3 2.5×, Q6 2.4×).
  - [arXiv 2508.04701 v3](https://arxiv.org/html/2508.04701v3) (accessed 2026-10-29)
- Sirius issue #609 (2026-04-10, jingnanquan), "Some questions regarding the performance of Sirius": "for all 22 TPC-H experiments, Sirius was slower than the CPU implementation on DuckDB" on a DGX Spark-class GB10 at SF10/SF100. It was "even slower" on an RTX 5090 and an A6000.
  - liuweibupt, A100-40GB, SF100: first 0.83× average. With the official script, 1.45× geomean and 18/22 queries faster. With DuckDB properly warmed, 9.27× was reported, but only together with a cuDF fix.
  - The root cause was a cuDF 26.04 bug: `get_current_device_resource_ref()` returned the default resource, so allocations bypassed the RMM pool. Bare `cudaMalloc` measured about 2,363 µs per call versus about 67 µs for the pool. Total `cudaMalloc` time fell from 2,822 ms to 415 ms after the fix.
  - Q1 after the fix: Sirius 1.46 s cold / 1.43 s warm versus DuckDB 2.38 s / 0.63 s. That flips Sirius from 0.28× to 2.26× warm.
  - The issue is closed; the fix is in cuDF 26.08.
  - [sirius-db/sirius#609](https://github.com/sirius-db/sirius/issues/609) (accessed 2026-10-29)
- Polars issue #20696 (2025-01-13), "Polars GPU engine slower than CPU when joining": a 10k-row × 5,000-float-column join ran in about 0.1 s on CPU versus about 7 s on an L40S.
  - vyasr (NVIDIA), from a profile of a 12.6 s run: 9.09 s Join.evaluate, 1.10 s to_polars, 0.86 s CUDA context and memory-pool setup ("one-time costs").
  - The bottleneck was host↔device copies plus per-column allocations. In his words, "it is very unlikely that the GPU engine will ever be performant relative to the CPU for short and wide tables… GPUs provide the biggest advantage over CPUs when your performance is gated on compute, not memory."
  - The user stayed on CPU.
  - [pola-rs/polars#20696](https://github.com/pola-rs/polars/issues/20696) (accessed 2026-10-29)
- Polars GPU docs:
  - Unsupported queries "transparently fall back to the standard Polars engine." `raise_on_fail=True` disables that fallback.
  - Unsupported types include Date, Categorical, Enum, Time, Array, Binary and Object. UDFs, the eager API and resampling are also unsupported.
  - On speed: speedups are most likely for workflows "dominated by grouped aggregations and joins." "I/O bound queries typically show similar performance on GPU and CPU."
  - Sizing guidance: "1 TiB … fit well with a GPU with 80GiB."
  - The docs give no overhead figures.
  - [Polars GPU support](https://docs.pola.rs/user-guide/gpu-support/) (accessed 2026-10-29)
- Crystal (Shanbhag, Madden, Yu, SIGMOD 2020), SSB SF20 on a V100: data resident in GPU memory gave about 25× over the CPU.
  - With the GPU as a coprocessor (PCIe 12.8 GB/s measured, CPU memory 54 GB/s, GPU memory 880 GB/s), the GPU was 1.5× faster than MonetDB but "1.4× slower than Hyper; for all queries … bound by the PCIe transfer time."
  - The authors conclude GPUs win "when the dataset fits into GPU memory."
  - [Crystal paper PDF](https://anilshanbhag.com/static/papers/crystal_sigmod20.pdf) (accessed 2026-10-29)
- spark-rapids discussion #11992 (MaxNevermind, 2025-01-21), "Can't achieve a significant performance/$ reduction on a benchmark."
  - Goal: "Achieve 50% compute cost reduction of basic ETL pipeline" on 4× g4dn.12xlarge (T4s, $1,799/month reserved) versus 4× r6id.24xlarge ($3,330/month).
  - Workload: 40M rows, 24 GB gzip Parquet, 18 string columns, some values up to 500K characters, then dropDuplicates plus a window count. Results were posted only as an image.
  - NVIDIA replies: tgravescs said g4dn.12xlarge is "not the most cost efficient", suggested re-enabling AQE and avoiding gzip. viadea said r6id has "3x better local disk io" and "perf/$ ratio may heavily depend on … job characteristics."
  - The 50% target was inspired by a conference talk.
  - [NVIDIA/spark-rapids discussion #11992](https://github.com/NVIDIA/spark-rapids/discussions/11992) (accessed 2026-10-29)

### Inferences
- A router's first-order features are cheap to get: whether the data is resident in VRAM (or cached Parquet), bytes scanned versus VRAM, the estimated output and selectivity, the string-column share and table width, and operator mix (join or group-by versus scan/filter/top-N). (estimate)
- A selective scan on cold data is the canonical query to keep on CPU.
- Fixed overheads I found: CUDA context plus pool setup of about 0.9 s once per process (Polars), warm per-query floor of about 1-3 ms (ClickBench hot), and `cudaMalloc` of about 2.4 ms per call without a pool. PCIe ran at 12.8 GB/s in 2020 (Crystal); PCIe4 is listed at 25.6 GB/s in the Sirius paper.
- The software stack is fragile: a single-line cuDF bug turned Sirius into a 0.28-0.83× loser on several GPUs. A router with a CPU fallback hedges against regressions like this too.

### Gaps
- I found no published row-count break-even such as "GPU wins above N rows" for any engine (Sirius, HeavyDB, cuDF, spark-rapids, Theseus, Presto-GPU). The search budget ran out before I could check HeavyDB, Theseus or Presto-Velox-cuDF docs.
- I have no measured ms figure for Parquet decode on GPU versus CPU.

## 2. Predictive models of GPU vs CPU speedup

### Takeaway
Research placement optimizers exist, such as HyPE/CoGaDB with learned per-device cost functions, but I found no recent published accuracy figure for predicting GPU-vs-CPU speedup. Zero-shot cost models are accurate, but only on a fixed CPU DBMS and fixed hardware. Crystal's model and the cold/hot ClickBench split both point to data location and PCIe as the dominant term.

### Cited Findings
- HyPE (Magdeburg/Ilmenau) is a C++ library that "automatically selects processing units for coprocessing." It relies on "relevant features of the datasets that affect execution time." It learns cost functions per device and also weighs device load and data placement. Its docs give no accuracy figures. — [HyPE docs](https://wwwiti.cs.uni-magdeburg.de/iti_db/research/gpu/hype/current/doc/index.htm); [Ocelot/HyPE demo](https://hgpu.org/?p=12666) (accessed 2026-10-29)
- Breß (dissertation): co-processors "can significantly slow down a DBMS when not used appropriately," so the optimizer avoids GPU placement when it expects a degradation. — [Breß dissertation](https://repo.bibliothek.uni-halle.de/bitstream/1981185920/12067/1/dissertation_bress.pdf) (accessed 2026-10-29; seen as a search snippet only)
- Zero-shot cost models (Hilprecht and Binnig, PVLDB 15(11), 2022) were evaluated on Postgres v12. With DeepDB cardinalities, all median Q-errors are below 1.54, versus 8.62 worst case for scaled optimizer costs. The authors "assume a fixed hardware and DBMS"; transfer across hardware setups is "future work." — [PVLDB paper](https://www.vldb.org/pvldb/vol15/p2361-hilprecht.pdf) (accessed 2026-10-29)
- Crystal gives an analytic bound: with a GPU coprocessor, runtime is at least data size divided by PCIe bandwidth. Because CPU memory bandwidth exceeds PCIe bandwidth, "running the query on CPU yields a lower runtime" when data starts in host memory. — [Crystal PDF](https://anilshanbhag.com/static/papers/crystal_sigmod20.pdf) (accessed 2026-10-29)

### Inferences
- A simple rule such as "GPU if resident, or if bytes ÷ PCIe bandwidth is much smaller than the CPU time estimate, and the operators are supported" captures most of the signal: the cold/hot swing is about 20-40× on ClickBench, compared with about 10× variation across queries. (estimate)
- The research gap is a learned cross-device cost model with reported accuracy, so a product could publish the first such figures.

### Gaps
- No accuracy figures for HyPE or CoGaDB; I could not open the full papers.
- I could not run the searches needed to check for 2023-26 learned GPU-database cost-model papers.

## 3. First-person demand for routing and GPU utilization complaints

### Takeaway
The clearest statement of demand is necubi on the HN Sirius thread. He spells out the router architecture but argues that only sophisticated infra teams can run it ("not mass-market"). ClickHouse's CTO replied with skepticism about GPU databases in general.

### Cited Findings
- HN, comment 46480338 by necubi, 2026-01-03, on the Sirius DB story [46441781](https://news.ycombinator.com/item?id=46441781) (2025-12-31). It answers stogot's question, "what are the downsides that a company needs to consider?" The full quote: "One downside is that you're paying for the GPU whether you're fully using it or not. It takes big queries to saturate a GH200, and if you're only using 10% of the capacity of the GPU it doesn't really matter that it's 10x faster. In a typical company you'll have jobs, some scheduled, some ad-hoc, at a range of sizes. Most of them won't be cost-effective to run on a GPU instance, so you need a scheduling layer that estimates the size of the job and routes it to the appropriate hardware. But now what if the job is too big to run on your GPU machine? Now we either have to scale up our GPU cluster or retry it on our more flexible CPU cluster. And this all assumes that your jobs can be transparently run across different executors from a correctness and performance standpoint. There are niches where this makes sense (we run the same 100TB job every day and we need to speed it up), as well and large and sophisticated internal infra teams that can manage a heterogenous cluster + scheduling systems, but it's not mass-market." — [HN 46480338](https://news.ycombinator.com/item?id=46480338), via [Algolia items API](https://hn.algolia.com/api/v1/items/46480338) (accessed 2026-10-29)
- Replies:
  - srcreigh: "If it's 10x cheaper, it doesn't matter if you only use 0.1% of the GPU, right?"
  - zX41ZdbW (Alexey Milovidov, ClickHouse): "GPU databases can run a small subset of production workloads in a narrow combination of conditions," listing MapD/OmniSci/HeavyDB, AresDB, BlazingSQL, Kinetica, BrytlytDB, SQream and Alenka as "very niche" or "not even usable."
  - same source (accessed 2026-10-29)
- The Polars #20696 reporter fell back to CPU ("For now we will stick to CPU"). MaxNevermind missed his 50% cost target (section 1). — sources above

### Gaps
- I did not search HN or Reddit further for "automatic GPU/CPU routing" asks, because the web-search budget was exhausted.
- I found no reports of companies running analytics on idle AI-training GPUs.

## 4. Who has GPU budgets or idle GPUs

### Cited Findings
- ClearML / AIIA / FuriosaAI "State of AI Infrastructure at Scale 2024" (2024-03-13), on utilization at peak periods:
  - 15% of respondents say "less than 50% of their available and purchased GPUs are in use."
  - 53% estimate 51-70% utilization.
  - 25% estimate about 85%.
  - "Only 7%" report more than 85%.
  - The page does not give a sample size.
  - [ClearML blog](https://clear.ml/blog/the-state-of-ai-infrastructure-at-scale-2024) (accessed 2026-10-29)

### Inferences
- If utilization at peak is about 50-70%, roughly 30-50% of purchased GPU-hours are idle (estimate). That idle capacity, already paid for, is the strongest economic case for routing analytics onto it.

### Gaps
- I could not reach NVIDIA's AI-factory / AI Data Platform pitch: DNS failed for nvidia.com.
- The Alibaba (NSDI'22) and Microsoft Philly (ATC'19) cluster-utilization papers did not load.

## 5. Cloud price ratio and break-even speedup

### Cited Findings
- AWS us-east-1 on-demand prices:

  | Instance | Hardware | On-demand | Spot |
  |---|---|---|---|
  | p5.4xlarge | 1×H100 80 GB, 16 vCPU, 256 GiB | $6.880/h | $4.489/h |
  | c7a.16xlarge | 64 vCPU, 128 GiB | $3.284/h | $1.063/h |
  | c6a.metal | 192 vCPU | $7.344/h | $2.351/h |

  — [p5.4xlarge](https://instances.vantage.sh/aws/ec2/p5.4xlarge), [c7a.16xlarge](https://instances.vantage.sh/aws/ec2/c7a.16xlarge), [c6a.metal](https://instances.vantage.sh/aws/ec2/c6a.metal) (accessed 2026-10-29)
- The Sirius paper prices a Lambda GH200 at $1.5/h in Table 1 but $3.2/h in Sec. 2.1, an internal conflict. It uses c8g.metal-48xl at $7.6/h and c7a.metal-48xl at $9.8/h. — [arXiv 2508.04701v3](https://arxiv.org/html/2508.04701v3) (accessed 2026-10-29)
- Crystal (2020): p3.2xlarge with a V100 at $3.06/h versus r5.2xlarge at $0.504/h, about a 6× price ratio. With a 25× speedup that is "4× more cost effective." — [Crystal PDF](https://anilshanbhag.com/static/papers/crystal_sigmod20.pdf) (accessed 2026-10-29)

### Inferences
- The break-even speedup equals the GPU price divided by the CPU price.
  ```
  p5.4xlarge vs c7a.16xlarge (on-demand)   6.88 / 3.284 ≈ 2.1×
  p5.4xlarge vs c7a.16xlarge (spot)        4.489 / 1.063 ≈ 4.2×
  p5.4xlarge vs c6a.metal (on-demand)      ≈ 0.94×  (GPU box is cheaper)
  GH200 at $1.5/h vs c6a.metal              ≈ 0.2×   (Sirius paper prices)
  ```
- Hot ClickBench speedups of 11-18× clear every one of these ratios easily. The cold results (0.1-0.5×) clear none.
- So the economic decision depends on residency and utilization, not raw speed. That supports necubi's framing that a router helps whoever already owns the GPU and keeps its VRAM warm. (estimate)
