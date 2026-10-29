# gpu-cpu-router

## Verdict

- **What it is:** a layer that sizes each query before it runs. It sends the query to a GPU engine only when the GPU is likely to win, and otherwise to a CPU engine. It reports where each query ran and why. It ships as a DuckDB extension, a SQL proxy, or a Trino Gateway routing service.
- **Who needs it:** in theory, teams that pay for GPUs and run mixed job sizes. demand.md rates the demand **thin**: one poster (necubi), who says himself that it is "not mass-market".
- **Budget:** easily fits, at about 0.45× the engine on peacockdb and 0.25× on Sirius (estimate). But it is a **feature, not a product**. It belongs inside whichever engine option is chosen, best of all inside lean or gpu-bi-dashboards, because the deciding signal is whether the data is already in GPU memory.

## Who needs it badly

- **Rating: thin**, from demand.md `## Ratings`, row `gpu-cpu-router`: "It answers the top objection, but only one person asks for it, and engines build it in" [D]. Thin means fewer than three first-person posters. In the six-criterion ratings below it counts as weak.
- **necubi on HN (2026-01-03), on the Sirius thread** [HN1]:
  - "you're paying for the GPU whether you're fully using it or not … if you're only using 10% of the capacity of the GPU it doesn't really matter that it's 10x faster."
  - "you need a scheduling layer that estimates the size of the job and routes it to the appropriate hardware."
  - He adds that it fits only niches ("we run the same 100TB job every day") or "large and sophisticated internal infra teams", "but it's not mass-market."
  - In reply, srcreigh: "If it's 10x cheaper, it doesn't matter if you only use 0.1% of the GPU, right?" ClickHouse's CTO: GPU databases fit "a small subset of production workloads" [HN1].
- **threeseed on HN (2025-05-12):** "I have run GPU accelerated Spark clusters for years … that's like 20% of what a typical job does" [HN2]. This is the source of the "~20% of a typical Spark job" claim. It is one practitioner's estimate. NVIDIA's own qualification blog makes no such statement [Q2].
- **People who found the GPU slower and went back to the CPU by hand:**
  - A Polars user's join on 10k rows × 5,000 columns took about 0.1 s on CPU and about 7 s on an L40S. An NVIDIA engineer said the GPU is "very unlikely … ever" to win on short, wide tables. The user stayed on CPU [PL1] (2025-01-13).
  - A GKE practitioner was promised 4–5× by NVIDIA's qualification tool. Some jobs ran slower than on CPU end to end, and the advice was to keep RAPIDS opt-in per workload [HK] (2026-10-20).
  - MaxNevermind aimed for a 50% cost cut on T4s against r6id and missed it. NVIDIA's replies blamed the instance type and the disk IO [MN] (2025-01-21).
  - A maintainer measured a low-cardinality join at 151 s on GPU against 32 s on CPU at 4 partitions [SR7529] (2023-01-18).
- **Silent fallback is the related, louder pain.** demand.md counts 5 threads where users measured the CPU without knowing it, e.g. Sirius #2174 [D][S2174]. A router that always says "ran on CPU, because …" answers that too.
- **Idle GPUs exist.** In a 2024 survey, 15% of respondents use under half their GPUs at peak, and only 7% exceed 85% [CL]. But no one in the corpus asks to put analytics on idle AI GPUs [PD].
- **Best first conversations:** necubi (HN), MaxNevermind (cost-focused Spark ETL), and the GKE author [HK]. None has a budget or a deadline in public.

## How it would work

- **Where it sits.** Three shapes, one decision core:
  1. **A DuckDB extension beside Sirius.** A pre-optimizer hook reads the plan and sets `gpu_execution` per query. This is exactly what Sirius issue #990 proposes [S990].
  2. **A SQL proxy** (Flight SQL or the Postgres wire protocol) in front of a CPU engine and a GPU engine that speak the same dialect.
  3. **A Trino Gateway EXTERNAL routing service.** The gateway POSTs the parsed tables and query type; the service runs `EXPLAIN` and returns a routing group, "gpu" or "cpu" [TG]. For Spark, the same core flips `spark.rapids.sql.enabled` per job [SRC]. For Presto, it would set the new `native_cudf_enabled` property [P28678].
- **The decision features**, in order of weight (estimate, from [PD]):
  1. Is the data resident in VRAM or pinned host RAM? Hot vs cold swings Sirius from 11–18× faster than DuckDB to 0.1–0.5× on ClickBench [PD].
  2. Bytes to scan after row-group pruning, divided by the PCIe or decode rate, against an estimated CPU time.
  3. Operator mix. Joins and grouped aggregations win. Selective scans, top-N, `LIKE` on strings and wide tables lose [PD][SP].
  4. Estimated output size, and whether it fits in VRAM at all.
- **The decision rule.** Start with a hand rule: "GPU if resident, or if bytes ÷ feed rate is well under the CPU estimate, and every operator is supported." Calibrate the constants per machine from shadow runs: replay a sample of the user's own queries on both engines.
- **Safety net.** If the GPU fails or runs out of memory, rerun on CPU. Log every choice with its reason.
- **Plan splitting is out of the alpha.** MotherDuck's dual execution shows how. It inserts "bridge" operators and minimises estimated transfer bytes [MD]. Wayang splits jobs across engines by cost, but has no GPU target [WY].
- **Data path, end to end:**

```
client SQL ─► router: parse, plan, read Parquet footers, check cache residency
               │                      │
         resident or big+join     cold, small, selective, string-heavy
               ▼                      ▼
   GPU engine: VRAM/pinned cache  CPU engine: page cache → DuckDB/DataFusion
   (else NVMe → PCIe → decode)
               └─────── Arrow result + "ran on X because Y" ──► client
```

## The human's questions

**1. What exists?** Nothing routes a whole query by predicted benefit in a shipped default. Everything falls back by capability, often silently, and offers manual switches.

- **spark-rapids (now cudf-spark):**
  - **QualX** is an XGBoost model trained on paired CPU/GPU runs, mostly NDS. It predicts speedup offline, per SQL and per application, from CPU event logs [Q1][Q2].
  - NVIDIA publishes no accuracy figure. It warns its compatibility assumptions can be "optimistic" [Q3].
  - A user who trained it on 1,600+ queries found it "very prone to misclassifying low-speed tasks as high-speed tasks", because about 90% of the training jobs sped up [T1995].
  - **At run time,** placement is per operator by capability, with CPU fallback on by default [SRC].
  - An experimental `CostBasedOptimizer` keeps an operator on CPU when GPU cost plus transition cost exceeds CPU cost. It is internal and off by default (`spark.rapids.sql.optimizer.enabled=false`). It uses flat constants: 0.0002 s/row CPU, 0.0001 s/row GPU, 1M rows when unknown [CBO].
- **Sirius:**
  - Falls back to DuckDB's CPU for unsupported plans or GPU errors, silently by default [S2174].
  - It does no costing: tiny queries go to the GPU too [S].
  - #990 (open, 2026-06-22) proposes a size gate: skip the GPU "if the whole query's source cardinality is tiny" [S990].
  - #2147 (open, 2026-10-13) uses size estimates only to pick a GPU count. It found the old estimates 4–7× off on a third of TPC-H queries [S2147].
- **Presto / Velox:**
  - PR #28678 (open, 2026-10-19) adds a manual per-query `native_cudf_enabled` switch, so users can compare GPU and CPU without restarting workers [P28678].
  - Velox cuDF falls back per operator (`allowCpuFallback=true`), with the reason logged [VX].
- **Microsoft Fabric:**
  - Fabric Data Warehouse ships GPU query acceleration [D].
  - The CoddSpeed paper (SIGMOD 2026 industrial) describes a GPU engine derived from TQP. It claims "up to 30x on TPC-H 1TB" over Fabric's CPU engine [CS].
  - The public abstract does not say how queries are routed. The blog returned 403 here. Routing details are unknown.
- **ClickHouse GPU RFC #116807:** eligibility per subtree by Substrait capability, with no cost model [CH].
- **Query routers:**
  - Trino Gateway (release 22, 2026-10-20) routes by MVEL rules on user, headers, tables and query type, or by queue length. It has an EXTERNAL hook. Nothing calls EXPLAIN [TG].
  - Lyft, Bloomberg, Pinterest, Shopify and others run it. Inter&Co uses it to split low-latency from analytical traffic [TGU].
  - No paid standalone router was found [RT].
- **"Small to DuckDB, big to Spark":**
  - This is guidance (Palantir: Spark above ~50 GB [PF]), a vendor router (Ilum [IL]), or in-house code (Watershed [WS]).
  - None has a GPU target.
  - Bauplan's site describes a managed serverless platform, with no per-query engine choice visible [BP]. Fivetran/Tobiko engine selection was not verified (search budget ran out).
- **Research:**
  - HyPE/CoGaDB learn per-device cost functions for CPU/GPU operator placement [HY]. Breß warns co-processors "can significantly slow down a DBMS when not used appropriately" [BR].
  - No accuracy figure was found for any GPU-vs-CPU predictor.
  - Zero-shot cost models reach median Q-error under 1.54, but only on fixed hardware with Postgres [ZS].

**2. Can a cost model predict GPU-vs-CPU wins well enough?** For the clear cases, yes. For the middle band, no one has shown it.

- **The easy part.** Residency dominates. Hot vs cold moves the GPU/CPU ratio about 20–40× on ClickBench. Across queries it moves about 10× [PD]. Crystal gives an analytic bound: with data in host RAM, GPU time ≥ bytes ÷ PCIe bandwidth, so a fast CPU wins any scan-bound query [CR]. A rule on residency, bytes and operator mix gets these cases right (estimate).
- **The hard part.** Cardinality estimates are off 4–7× on a third of TPC-H queries, in both directions, even in DuckDB [S2147]. Learned classifiers inherit skewed training data [T1995]. NVIDIA has kept its own cost model off for years (estimate from [CBO]). The middle band, roughly 0.7–3× either way, is where misroutes cost money (estimate).
- **What peacockdb brings:**
  - **The cost model is not an optimizer input.** It is Σ output bytes per node from the CPU goldens, compared with DuckDB's materialized bytes. Nothing is timed [I][A].
  - **Its bytes are still the right first feature.** DuckDB's `storage_read_total` is decoded bytes of the surviving row groups [A], and the planner already knows them at plan time, because it maps row groups to batches (`scan_mapping/`) [I].
  - **PR #165** estimates joins from stats sidecars and Parquet footers, reorders joins by DPhyp, and replans when a build misses its estimate [I]. On TPC-DS it made 388 of 405 runs cheaper and 8 dearer, but in pandas bytes, not device time, and only in Python [I]. Its estimator is a usable source of cardinalities. Its replanning is the right answer to misestimates: start on one device and switch if the build misses.
  - **Missing:** any timing. A router needs ms per byte per operator on both devices. That means shadow runs, which the corpus harness could drive [I].
- **Verdict:** a calibrated rule is cheap and likely good enough to stop the embarrassing cases, such as the Polars 70× and the ClickBench cold Q24's 68 s vs 2.9 s [PL1][PD]. Proving it beats "always CPU unless pinned" needs a measurement (see Follow-up).

**3. Who would pay?**

- **Teams with idle GPUs:**
  - Their GPU-hours are already paid, so the break-even speedup is near zero. Any win is free.
  - But they are AI teams, not data teams, and no one in the corpus asks for this [PD][CL].
  - Their GPUs are also busy with training at the wrong moments (estimate).
- **Teams with GPU budgets to justify:**
  - MaxNevermind-style cost cutters, and Spark shops that bought GPU nodes on a qualification score [MN][HK].
  - On AWS on demand, a p5.4xlarge costs 2.1× a c7a.16xlarge, and 4.2× on spot [PD]. So the GPU must win by 2–4× to pay.
  - A router would mainly tell them "most of your jobs should stay on CPU". That is useful, but it shrinks the GPU spend that justified the purchase.
- **Who would not pay:** single-box DuckDB users. Their pain is cold data, and for cold data the router's answer is "CPU".
- **Pricing evidence:** none. Gateways are free (Trino Gateway). Routing value is captured inside platforms (Databricks Photon, MotherDuck) or sold as cost optimisation (Greybeam, no public price) [RT].

**4. Product, or feature of another option?** It is a **feature**:

- **Of lean and gpu-bi-dashboards first.** The cache knows what is resident, and residency is the decision. Without a cache, peacockdb has nothing hot to route to, since every batch re-reads Parquet [I].
- **Of workstation-gpu-sql.** It already lists a "small-query CPU gate" at 0.3–0.6k lines [WK].
- **Of spark-plugin,** as per-stage fallback with a cost check, like spark-rapids' CBO.
- **On Sirius,** it is issue #990, a small upstream PR [S990].
- It could stand alone only as a multi-engine "GPU FinOps" layer over Spark, Presto and Trino. That needs a buyer who already runs two engines, and none was found.

## Data path and cache

- **The thesis holds, and the router is its sharpest test.** The router's main output, without a cache, is "CPU". The data path decides almost every route.

| Stage | Rate | Source |
|---|---|---|
| HBM read, H200 | 4,484 GB/s measured | [BM] |
| PCIe Gen5 H2D, pinned / pageable | 47.3 / 10.6 GB/s (H200) | [BM] |
| Parquet decode on GPU | 6.8–8.7 GB/s per GPU; decode + Snappy is 80% of kernel time on q11v | [BM] |
| NVMe, 8× U.2 in a DGX | ~50 GB/s total (estimate) | [L] |
| NIC, 8× ConnectX-7 | 400 GB/s nominal; HDFS held 8×H100 at 15% | [L][D] |
| CPU memory bandwidth, 2-socket DDR5 | ~600 GB/s (estimate) | [L] |

- **What this means for the router.**
  - A cold query moves bytes at ~7–10 GB/s into the GPU.
  - A 64-core DuckDB reads its page cache far faster. So cold, scan-bound queries belong on the CPU.
  - Evidence: the human's PoC spent longer loading than computing on 11 of 12 SF20 queries [E]. Sirius cold on one H100 won 0–1 of 43 ClickBench queries; hot, it won 42 of 42 [PD]. Crystal: 25× resident, 1.4× slower than Hyper over PCIe [CR].
- **Fixed overheads matter for small queries.**
  - CUDA context plus pool setup costs about 0.9 s once per process [PL1].
  - The warm floor per query is about 1–3 ms [PD].
  - `cudaMalloc` without a pool costs about 2.4 ms a call, which alone made Sirius lose on desk GPUs [S609].
  - So the GPU engine must stay up as a warm server, with a pool. peacockdb installs no pool today (#148) [I].
- **The cache needed:**
  - decoded columns in VRAM for the hot set;
  - pinned host RAM behind it;
  - a residency table the router can read in microseconds.
  - Sirius has `pin_table` with no eviction [S]. peacockdb has nothing [I]. The cache is lean's job; the router reads it.

## Build cost to alpha

Sizes in yardstick lines; the unit is 22,830 [I]. Every figure is an estimate.

| Component | On peacockdb: reuse/adapt/new, size | On Sirius: reuse/adapt/new, size |
|---|---|---|
| Front door: Flight SQL/pgwire proxy, or Trino Gateway EXTERNAL service | new: 1–2k | new: 1–2k (or a DuckDB extension, ~0.3k) |
| Plan features: bytes after pruning, operator mix, string share, output estimate | adapt `scan_mapping/` and `memory_estimation.rs`: 0.5–1k | new in an optimizer hook, using DuckDB estimates and #2147's scan sizes: 0.5–1k |
| Time model per device, calibrated per machine | new (cost-report is bytes, not time): 1–2k | new: 1–2k |
| Calibration by shadow runs on both engines | adapt the corpus harness: 0.5–1k | new: 0.5–1k |
| Whole-query CPU fallback, retry on GPU OOM | adapt the CPU backend: 0.3–0.8k | reuse `enable_duckdb_fallback`; add OOM retry: ~0.2k |
| GPU entry point as a warm server, with an RMM pool | adapt (no entry point today; #148): 0.5–1k | reuse |
| "Ran on X because Y" report | new: 0.3–0.5k | adapt, and fix #2174: 0.2–0.4k |
| Residency signal | new minimal pinned-table cache, or wait for lean: 2–4k | reuse `pin_table`, read its state: 0.2–0.5k |
| Tests and a misroute-regret report | adapt: 0.5–1k | ~0.5k |
| **Total** | **~7–13k** | **~4–8k** |

Total new work: 0.45× engine on peacockdb, 0.25× on Sirius (estimate)

- **The hard part is not code. It is getting the middle band right on someone else's workload.**
  - Estimates are 4–7× off in both directions [S2147].
  - Device speed depends on the cuDF version: one allocator bug flipped Sirius from 2.26× to 0.28× [S609].
  - A misroute of a long job costs more than the router saves on many short ones.
  - So the alpha needs per-site calibration and replanning when a build misses, not just a model.
- **On Sirius it is small, and it lands upstream.** Once #990 merges, the whole product is a config default (estimate).

## Adoption friction

- **Low where the router sits inside one engine.** A DuckDB extension, or a Trino Gateway hook, keeps the user's SQL unchanged [TG][S990].
- **High where it fronts two different engines.** Both must give the same answers. necubi: "this all assumes that your jobs can be transparently run across different executors from a correctness and performance standpoint" [HN1]. DuckDB+Sirius gets this nearly free: same parser, same planner. peacockdb's CPU side is DataFusion, so it matches its own GPU side, but not the user's DuckDB, Spark or Trino.
- **A precondition few meet.** The buyer must already run a warm GPU engine beside a CPU engine. Most of the corpus runs one or the other [D].
- **Trust is the real lever.** "Tells you where it ran" is cheap, and it is wanted (5 silent-fallback threads) [D].

## Platform risk

- **Weak (high risk).** Every host engine is building this in:
  - Sirius #990 is a maintainer proposal [S990].
  - Presto/Velox added the per-query switch in the same month [P28678][VX].
  - spark-rapids has QualX plus a cost-based optimizer it can turn on [Q1][CBO].
  - Fabric routes inside its own warehouse [CS].
- **The incumbents own both sides of the switch.** The decision needs plan internals and residency state that only the engine has. NVIDIA staffs Sirius and Velox-cuDF and gets coordinated changes [N].
- nvidia.md's stance: the risk is "commoditization by free, NVIDIA-staffed engines" [N]. A router is the most copyable piece of all: a heuristic plus a config flag.

## Funding story

- **What a design partner would say:**
  - necubi-type: "Useful, but we would build it in-house; it is a scheduler rule" [HN1].
  - A cost-cutter like MaxNevermind: "Show me my bill going down." The router can show that, mostly by keeping jobs off the GPU.
- **What the round would be raised on:** "GPU FinOps for analytics: every query on the cheapest hardware that meets its SLA." This is a services-and-savings story, like Greybeam's Snowflake-to-DuckDB pitch [RT]. It does not need a GPU engine of our own, which undercuts the reason to build on peacockdb.
- **No paying user, no budget, one asker.** The round would rest on savings that no one has measured yet.
- **As a feature, it strengthens another option's story.** "Our GPU engine never makes your query slower" is a strong line for lean or workstation-gpu-sql.

## Evidence against

- **One asker, who calls it niche.** demand.md: thin [D][HN1].
- **The engines build it themselves, this year:** Sirius #990, Presto #28678, spark-rapids CBO, Fabric [S990][P28678][CBO][CS].
- **NVIDIA has had the cost model for years and left it off.** That suggests simple constants mispredict [CBO]. QualX over-predicts in the field [HK][T1995].
- **The router mostly says "CPU".** Without a resident cache, cold GPU reads lose to a tuned CPU on most scan-bound queries [PD][E]. With a cache, the hard work is the cache, which is another option.
- **It defeats its own buyer's case.** Teams that bought GPUs want them used. A router that proves most jobs belong on CPU lowers the GPU spend that justified the purchase (estimate).
- **No moat.** The core is a heuristic and a flag. The data that makes it good, shadow runs, belongs to the customer.
- **For it, honestly:** it answers the top objection to GPU analytics ("slower on small inputs", "paid GPU idles"). It is cheap, and it raises trust in whatever engine it fronts.

## Ratings

- **Who needs it badly — weak:** demand.md rates it thin. One poster, who says "not mass-market" [D][HN1].
- **Build cost — strong:** 0.45× on peacockdb, 0.25× on Sirius. Well inside the budget, though the middle-band accuracy is unproven.
- **Data path — medium:** it routes around the data path rather than fixing it. Without a cache its answer is mostly CPU.
- **Adoption friction — medium:** an extension or gateway hook is easy. But the buyer must already run a warm GPU engine and a CPU engine that give the same answers.
- **Platform risk — weak:** Sirius, Presto/Velox, spark-rapids and Fabric are all building or shipping the switch.
- **Funding story — weak:** a heuristic and a flag, no paying user, and savings that mostly come from not using the GPU.

## Follow-up experiments

- **Break-even map.** On one GPU box, run TPC-H, TPC-DS and ClickBench at sf1, 10 and 100 on DuckDB and DuckDB+Sirius (and the peacockdb GPU path), cold and hot. Fit "GPU wins" against bytes scanned, residency and operator mix. This settles how big the middle band is.
- **Misroute regret.** Train the hand rule on half the queries and test on the rest. Report the misroute rate and the regret in total time and dollars, against "always CPU" and "GPU if pinned".
- **Estimate error.** Compare DataFusion's and DuckDB's estimates of scanned bytes, and PR #165's join estimates, with actual bytes. Does the decision survive the 4–7× errors in #2147?
- **Fixed overheads on peacockdb.** Measure the first-query and warm per-query floors with an RMM pool installed (#148). This sets the smallest query worth routing.
- **Ask necubi and MaxNevermind** whether they would install a routing layer, and what a misroute may cost them.

## Sources

- [D] demand.md, `## Ratings` row `gpu-cpu-router`, and `## First conversations` (local; accessed 2026-10-29)
- [I] inventory.md at master f25c250d (local; accessed 2026-10-29). [A] `llm-wiki/architecture.md`, "Cost model and DuckDB's cost oracle" (local; accessed 2026-10-29). [E] evidence-in-hand.md (local; accessed 2026-10-29)
- [S] sirius.md, Sirius @ b9e05ffc653c8ed031c789e5c1415ac1186e0619, `src/planner/gpu_admission.cpp:26-52` (local; accessed 2026-10-29). [N] nvidia.md (local; accessed 2026-10-29)
- [L] options/lean.md, `## Data path and cache`, with its sources [H2][H3][H9] (local; accessed 2026-10-29). [BM] `llm-wiki/reports/benchmark-minimal.md` via inventory.md (local; accessed 2026-10-29). [WK] options/workstation-gpu-sql.md (local; accessed 2026-10-29)
- [PD] Research notes "predictability-demand.md", `../notes/gpu-cpu-query-routing/`: ClickBench arithmetic on https://github.com/ClickHouse/ClickBench/blob/main/sirius/results/20260309/lambda-GH200.json, https://github.com/ClickHouse/ClickBench/blob/main/sirius/results/20260309/p5.4xlarge.json and https://github.com/ClickHouse/ClickBench/tree/main/duckdb/results/20260511; AWS prices from https://instances.vantage.sh/aws/ec2/p5.4xlarge, https://instances.vantage.sh/aws/ec2/c7a.16xlarge (accessed 2026-10-29)
- [RT] Research notes "routers-engine-selection.md", same folder (accessed 2026-10-29)
- [HN1] https://news.ycombinator.com/item?id=46480338, via https://hn.algolia.com/api/v1/items/46480338 (accessed 2026-10-29)
- [HN2] https://news.ycombinator.com/item?id=43968287 (accessed 2026-10-28, via sources/hn-stackexchange.md)
- [PL1] https://github.com/pola-rs/polars/issues/20696 (accessed 2026-10-29)
- [HK] https://hackernoon.com/benchmarking-nvidia-rapids-on-gke-estimated-vs-actual-spark-speedups (accessed 2026-10-29)
- [MN] https://github.com/NVIDIA/spark-rapids/discussions/11992 (accessed 2026-10-29)
- [SR7529] https://github.com/NVIDIA/cudf-spark/issues/7529 (accessed 2026-10-29)
- [Q1] https://docs.nvidia.com/cudf-spark/26.08/qualification/qualx.html (search snippet; accessed 2026-10-29). [Q2] https://developer.nvidia.com/blog/predicting-performance-on-apache-spark-with-gpus (accessed 2026-10-29). [Q3] https://docs.nvidia.com/spark-rapids/user-guide/latest/qualification/overview.html (accessed 2026-10-29)
- [T1995] https://github.com/NVIDIA/cudf-spark-tools/issues/1995; qualx-conf.yaml @ 4701b31a6c774e2599def665335fa92c2792b684 (accessed 2026-10-29)
- [SRC] https://github.com/NVIDIA/cudf-spark/blob/v26.08.1/docs/configs.md (tag v26.08.1, SHA 15899e9f; accessed 2026-10-29)
- [CBO] https://github.com/NVIDIA/cudf-spark/blob/v26.08.1/sql-plugin/src/main/scala/com/nvidia/spark/rapids/CostBasedOptimizer.scala; https://github.com/NVIDIA/cudf-spark/blob/49d3be946ca9cd660c3b74b15a684a8aec92e058/sql-plugin/src/main/scala/com/nvidia/spark/rapids/RapidsConfEntries.scala (accessed 2026-10-29)
- [S990] https://github.com/sirius-db/sirius/issues/990; [S2147] https://github.com/sirius-db/sirius/pull/2147; [S2174] https://github.com/sirius-db/sirius/issues/2174; [S609] https://github.com/sirius-db/sirius/issues/609 (accessed 2026-10-29)
- [SP] Sirius paper, https://arxiv.org/html/2508.04701v3 (accessed 2026-10-29)
- [P28678] https://github.com/prestodb/presto/pull/28678 (test merge SHA dada068b); https://github.com/facebookincubator/velox/pull/19637 (accessed 2026-10-29)
- [VX] https://github.com/facebookincubator/velox/blob/bcdab35c839ef3dd4af8b75d5b38ae84b9c51fa0/velox/experimental/cudf/CudfConfig.h (accessed 2026-10-29)
- [CS] CoddSpeed, SIGMOD 2026 industrial, doi 10.1145/3788853.3803077: https://www.microsoft.com/en-us/research/publication/coddspeed-hardware-accelerated-query-processing-in-microsoft-fabric/ (accessed 2026-10-29). Fabric blog https://community.fabric.microsoft.com/t5/Fabric-Updates-Blog/A-new-analytics-frontier-GPU-accelerated-Fabric-Data-Warehouse/bc-p/5192701 returned 403 (accessed 2026-10-29)
- [CH] https://github.com/ClickHouse/ClickHouse/issues/116807 (accessed 2026-10-29)
- [TG] https://trinodb.github.io/trino-gateway/routing-rules/; trino-gateway @ cfd7fc270fb3b1e1cacf83dc406154498521e117, `gateway-ha/src/main/java/io/trino/gateway/ha/router/` (accessed 2026-10-29). [TGU] https://github.com/trinodb/trino-gateway/blob/main/docs/users.md (accessed 2026-10-29)
- [MD] Atwal et al., MotherDuck, CIDR 2024: https://www.cidrdb.org/cidr2024/papers/p46-atwal.pdf (accessed 2026-10-29)
- [WY] https://github.com/apache/wayang (accessed 2026-10-29)
- [PF] https://palantir.com/docs/foundry/transforms-python/compute-engines/ (snippet; accessed 2026-10-29). [IL] https://ilum.cloud/docs/features/engines/ (snippet; accessed 2026-10-29). [WS] https://watershed.com/blog/how-watershed-uses-duckdb-to-reliably-compute-customer-footprints (snippet; accessed 2026-10-29). [BP] https://www.bauplanlabs.com/ (accessed 2026-10-29)
- [HY] https://wwwiti.cs.uni-magdeburg.de/iti_db/research/gpu/hype/current/doc/index.htm (accessed 2026-10-29). [BR] https://repo.bibliothek.uni-halle.de/bitstream/1981185920/12067/1/dissertation_bress.pdf (snippet; accessed 2026-10-29)
- [ZS] Hilprecht and Binnig, PVLDB 15(11), https://www.vldb.org/pvldb/vol15/p2361-hilprecht.pdf (accessed 2026-10-29)
- [CR] Crystal, SIGMOD 2020, https://anilshanbhag.com/static/papers/crystal_sigmod20.pdf (accessed 2026-10-29)
- [CL] https://clear.ml/blog/the-state-of-ai-infrastructure-at-scale-2024 (accessed 2026-10-29)
