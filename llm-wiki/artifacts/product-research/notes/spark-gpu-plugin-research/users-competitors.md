# GPU-accelerated Spark: user reports and the accelerator landscape (accessed 2026-10-29)

All links were accessed 2026-10-29. Labels used below: **[VENDOR]** means the vendor or its marketing said it, including on its own blog. **[USER]** means the user company or an individual practitioner said it. **[3P]** means a third party (academic, independent benchmark). **(estimate)** marks my own inference.

## Q1. Production reports for spark-rapids (now "NVIDIA cuDF for Apache Spark", repo NVIDIA/cudf-spark): named companies, speedups, savings, and who verified them

### Takeaway
I found almost no 2024-2026 production report that a user company published itself with measured cost savings. The named-customer numbers are overwhelmingly NVIDIA-published. Taboola's 20x is told on NVIDIA's blog. AT&T and the per-cloud savings come from NVIDIA decks built on its own NDS benchmark. I found no ByteDance, Snap, Pinterest, Adobe or Cloudera user-authored numbers in this pass.

### Cited Findings
- The repo is now `NVIDIA/cudf-spark`, described as "NVIDIA cuDF for Apache Spark plugin". It has about 1,013 stars, 312 forks and 2,166 open issues, and was last pushed 2026-10-29 — [GitHub API](https://github.com/NVIDIA/cudf-spark)
- Latest releases are v26.08.0 (2026-08-20) and v26.08.1 (2026-08-28). Before that came v26.06.0 (2026-06-23) and v26.06.1 (2026-07-23) — [GitHub releases](https://github.com/NVIDIA/cudf-spark/releases)
- **Taboola [VENDOR, published on NVIDIA's blog].** Taboola moved its Spark CPU cluster to GPUs and reports a 20x speedup and "significant cost savings". Its first attempt only reached about 3-5x on some simple queries, and other queries crashed. Tuning, including 1-2 GB partition sizes, fixed much of this — [NVIDIA blog](https://developer.nvidia.com/blog/gpu-integration-propels-data-center-efficiency-and-cost-savings-for-taboola/)
- **NVIDIA NDS benchmark [VENDOR, 2023].** NDS SF3000 shows a 5.7x speedup and 4.5x cost savings. Per-cloud savings are 78% on GCP Dataproc, 42% on AWS EMR and 34-39% on Databricks (the Databricks comparison is against Photon 10.4). NVIDIA states that NDS results are not TPC-DS compliant. The same deck includes an AT&T case study on a multistage AI pipeline — [TIES/PEARC-23 deck, OSU NOWLAB](https://nowlab.cse.ohio-state.edu/static/media/workshops/presentations/ties23/TIES-PEARC-23.pdf)
- **General marketing [VENDOR].** NVIDIA claims "up to 5X" faster, or the same work at "4X less infrastructure cost". These figures exclude the NVIDIA AI Enterprise license cost — [NVIDIA public-sector overview PDF](https://images.nvidia.com/aem-dam/Solutions/documents/public-sector-solution-overview-spark.pdf)
- **Spark RAPIDS ML [VENDOR].** 6x faster and 3x more cost-efficient than CPU PySpark MLlib on Databricks AWS — [NVIDIA blog](https://developer.nvidia.com/blog/reduce-apache-spark-ml-compute-costs-with-new-algorithms-in-spark-rapids-ml-library/)
- **CBA (Commonwealth Bank of Australia) [VENDOR/press, GTC 2025].** CBA was cited as facing Spark latency and cost problems. The article gives no verified savings figure — [BigDATAwire 2025-03-21](https://www.bigdatawire.com/2025/03/21/nvidia-pushes-boundaries-of-apache-spark-with-rapids-and-project-aether/)
- **Walmart Global Tech [USER, Medium].** A getting-started tutorial series and an exploratory "Is it GPU?" post. The search result showed no production benchmark numbers — [part I](https://medium.com/walmartglobaltech/getting-started-with-apache-spark-gpu-rapids-part-i-938664771092), [What next](https://medium.com/walmartglobaltech/apache-spark-what-next-is-it-gpu-33ce4a3851c1)
- **Anonymous HN user rockostrich [USER].** Reports "deploying a cloud-based data platform powered by GPUs (i.e. Spark-RAPIDS)", 2025-05-12 — [HN](https://news.ycombinator.com/item?id=43967147)
- **HN user threeseed [USER].** "Many of us have been using GPU accelerated Spark for years" (noted as supported on EMR), 2025-05-12 — [HN](https://news.ycombinator.com/item?id=43967289)
- **Competitor critique [VENDOR, competitor].** Voltron Data argues spark-rapids is "constrained by Spark's CPU architecture" and claims Theseus has a 10x cost-performance advantage over it — [Voltron Data blog](https://voltrondata.com/blog/nvidia-s-spark-rapids-hits-the-wall-constrained-by-spark-s-cpu-architecture)

### Inferences
- Every headline number I found (20x, 5.7x, 78%/42%/34-39% savings) traces back to NVIDIA material. The independent production evidence is anecdotal ("we use it") and has no numbers. (estimate)
- The 50% cost-reduction target that users bring to PoCs comes from NVIDIA presentations. The user in #11992 says so explicitly; see Q2.

### Gaps
- I could not find user-authored 2024-2026 posts or talks from ByteDance, Snap, Pinterest, Adobe, AT&T (other than NVIDIA's deck), Cloudera or Dataproc customers that give numbers. Search did not surface them, and I did not search Data+AI Summit or GTC session catalogs one by one.
- I found no academic paper that independently reproduces spark-rapids production savings.

## Q2. User complaints: GPU slower than CPU, partial acceleration, fallback, OOM, executor-per-GPU, Photon, cost target missed, I/O-bound

### Takeaway
The complaints are consistent and documented in public threads. The common reasons are:
- I/O-bound jobs, where the GPU sits idle.
- CPU fallback for unsupported ops, UDFs and formats.
- One GPU per executor scheduling, which leaves CPU cores idle.
- GPU node startup and quota delays on Kubernetes.
- OOM in joins.
- Cheap GPU instances with weaker local disks than the CPU instances they replace.

NVIDIA engineers' standard answer is "tune and send us event logs".

### Cited Findings
- **cudf-spark discussion #11992 (2025-01-21, user MaxNevermind), "Can't achieve a significant performance/$ reduction on a benchmark".** The goal was a 50% compute cost reduction on a basic ETL job. The setup compared a g4dn.12xlarge (4×T4, about $1,799/month reserved) against an r6id.24xlarge (about $3,330/month), Spark 3.5.2, RAPIDS 24.10.1, on synthetic nested string-heavy data. The results were posted only as an image and the target was not met. The user said the 50% figure was "inspired by its mentions in few presentations" and linked an NVIDIA talk.
  - NVIDIA's tgravescs answered that g4dn.12xlarge is "not the most cost efficient" and that smaller g4dn.2xlarge nodes are usual. He also questioned the user's AQE=off and gzip settings.
  - NVIDIA's viadea said the r6id has "3x better local disk io performance", which matters for I/O- or spill-heavy jobs, and asked for event logs.
  - The thread has only 3 comments and no resolution — [GitHub](https://github.com/NVIDIA/cudf-spark/discussions/11992)
- **cudf-spark discussion #11431 (2024-09-06, melvin-koh), HGX 8×H100.** The workload was an NDS transcode, and the GPUs were "at most around 15% utilised". NVIDIA's abellina noted that the CSV scan is off by default because of encoding and decimal incompatibilities (NDS data can contain ISO-8859-1 characters), and advised 8 executors, 1 per GPU. The user concluded "the bottleneck is my HDFS/network": the job was reading from HDFS in another cluster — [GitHub](https://github.com/NVIDIA/cudf-spark/discussions/11431)
- **spark-rapids discussion #8082.** The GPU run on g5.24xlarge took 4.9 min against 4.3 min on CPU, and the user concluded the GPU was more expensive. The maintainer's advice was to grep the driver log for "cannot run on GPU because". He noted that small data loses to kernel-launch overhead and that fallback round-trips thrash the CPU cache — [GitHub](https://github.com/NVIDIA/spark-rapids/discussions/8082). Related: [#7021 CPU vs GPU time comparison](https://github.com/NVIDIA/spark-rapids/discussions/7021)
- **Independent practitioner benchmark on GKE [USER/3P], HackerNoon, 2026-10-20 by "chetan" (Senior Staff Engineer).** Setup: Spark 3.5.5, RAPIDS 25.12.0, g2-standard-32 with 1×L4 per executor, on several production jobs. The Qualification Tool predicted 4-5x, but "Some of the RAPIDS runs finished slower" than CPU. No per-job numbers were published. Causes the author found:
  - GPU node cold start and regional GPU quota.
  - Pending executors when maxExecutors exceeded the quota.
  - Pod scheduling overhead.
  - CPU fallback for text output, some Iceberg versions, Cartesian joins, PySpark UDFs and skewed shuffles.
  - Idle cores with 1 GPU per executor, fixed with task.resource.gpu.amount=0.001 and concurrentGpuTasks=3.
  - GPU OOM in joins, fixed by lowering batchSizeBytes and concurrency or by repartitioning skew.

  The author's recommendation is to keep RAPIDS opt-in per workload — [HackerNoon](https://hackernoon.com/benchmarking-nvidia-rapids-on-gke-estimated-vs-actual-spark-speedups)
- **NVIDIA's own docs [VENDOR].** "Significant performance benefits can still be gained even when not every operation runs on the GPU". Heavy shuffle and RDD operations are poor fits — [Workload qualification](https://docs.nvidia.com/spark-rapids/user-guide/25.12/getting-started/workload-qualification.html)
- **Databricks limitation [VENDOR doc].** On a multi-GPU node the plugin can use only one GPU, "due to a Databricks limitation" — [NVIDIA Databricks guide](https://docs.nvidia.com/spark-rapids/user-guide/latest/getting-started/databricks.html). NVIDIA's CUDA-X docs say users must turn off Photon when launching GPU nodes on classic clusters, so the plugin and Photon are mutually exclusive — [NVIDIA CUDA-X Databricks docs](https://docs.nvidia.com/datascience/deployment/26.10/platforms/databricks/index.html)
- **ParaQuery founder winwang [USER/vendor].** A basic spark-rapids setup is easy, but "Most of the issues come after the initial PoC, especially for teams not wanting to manage infra". Multi-column UDFs and some incompatible operations are not accelerated — [HN](https://news.ycombinator.com/item?id=43967477), [HN](https://news.ycombinator.com/item?id=43965330)
- **HN user andygrove (Comet/DataFusion maintainer).** "Spark compatibility is notoriously difficult with Spark accelerators built with non-JVM languages" — [HN](https://news.ycombinator.com/item?id=43969326)

### Inferences
- Two failure modes recur:
  - The cost baseline is wrong: storage- or network-bound jobs, or GPU instances with worse disks.
  - Coverage is partial: fallback, UDFs, CSV and text.

  Raw GPU speed is rarely the issue. (estimate)
- The executor-per-GPU model shows up as idle CPU cores, and on Databricks as a single-GPU-per-node cap.

### Gaps
- I did not find a specific Reddit r/dataengineering or r/apachespark thread. Search returned GitHub and blogs instead, so Reddit sentiment is unverified.
- The #11992 numeric results are only in an embedded image, so I could not extract the numbers.
- I found no Databricks Community forum threads in this pass.

## Q3. Independent benchmarks across spark-rapids, CPU Spark, Photon, Gluten and Comet (TPC-DS/TPC-H/NDS), with cost per query

### Takeaway
I found no independent benchmark that compares all of them. The only head-to-heads are vendor-run (NVIDIA against Photon, Theseus against spark-rapids) or CPU-only (AWS on Comet and Gluten). Cost-per-query data comparing GPU and CPU from a third party is essentially absent.

### Cited Findings
- **AWS Labs, Gluten vs Comet [3P relative to both projects], TPC-DS 3TB on EKS with Graviton4.** Gluten was about 9% faster overall (2,239.93 s vs 2,467.49 s), but Comet won more individual queries. No cost figures were given — [Comet docs, Gluten comparison](https://datafusion.apache.org/comet/about/gluten_comparison.html)
- **AWS (Manabu McCloskey and Vara Bonthu) on Comet [USER-ish; AWS staff on the Apache blog], 2026-06-03.** TPC-DS 3TB on 12× r8gd.12xlarge with Spark 3.5.8. The speedup grew across releases:
  - Comet 0.14: 11% slower than Spark.
  - Comet 0.15: 11% faster.
  - Comet 0.16: 32% faster on Parquet and 37% faster on Iceberg. q86 sped up 3.47x.

  Problems they hit:
  - About 500x more DNS queries than vanilla Spark.
  - About 67% more memory than vanilla Spark.
  - Missing DPP support.
  - A DPP+AQE driver crash.

  No dollar figures and no GPU — [DataFusion blog](https://datafusion.apache.org/blog/2026/06/03/comet-eks/)
- **NVIDIA NDS vs Databricks Photon 10.4 [VENDOR, 2023].** 34-42% savings — [TIES deck](https://nowlab.cse.ohio-state.edu/static/media/workshops/presentations/ties23/TIES-PEARC-23.pdf)
- **Other vendor-run claims.**
  - Onehouse Quanton is about 25% faster than Photon on TPC-DS 10TB — [Onehouse](https://www.onehouse.ai/blog/apache-datafusion-comet)
  - VecRuntime is about 2.1x faster than Gluten and 2.3x faster than Comet on TPC-DS 3TB — [GitHub PR](https://github.com/vecruntime/vecruntime/pull/839)
  - LakeSail claims up to 98% lower cost than Spark on TPC-H — [LakeSail](https://lakesail.com/performance/)
- **CIDR 2026 paper.** It describes Comet as giving about 2x on TPC-DS SF1000 — [CIDR 2026 Sirius paper](https://vldb.org/cidrdb/papers/2026/p12-yogatama.pdf) (via search snippet; not verified in full text)

### Inferences
- A credible public GPU-vs-CPU-native (Gluten/Comet/Photon) cost-per-query comparison does not exist as of 2026-10. Publishing one would fill a real gap. (estimate)

### Gaps
- No independent spark-rapids TPC-DS/NDS run with cost per query was found.
- No independent Photon cost benchmark was found.

## Q4. Competitors: Gluten + Velox cuDF, Comet, ParaQuery, Photon GPU, Theseus, Bodo, Auron, Sirius, Project Aether, and other 2026 items

### Takeaway
- **Gluten's GPU path** is experimental. In September 2026 only 48 of 103 TPC-DS SF1 queries passed with CPU fallback disabled, and scans and spill still run on CPU.
- **Comet** is CPU-only and maturing fast (1.0 in August 2026, 1.1 in October 2026).
- **ParaQuery** is a one-person YC startup that runs spark-rapids as a managed service.
- **Photon GPU** was announced in 2024. I found no general-availability evidence.
- **Theseus** has gone quiet, with no 2026 news found.
- **Auron (formerly Blaze)** is CPU-native Rust/DataFusion.
- **Sirius** targets DuckDB and Doris, not Spark.

### Cited Findings

**Apache Gluten with the Velox cuDF backend**
- VeloxGPU.md status: "Experimental (TPC-H SF1 validated)" and "up to 8.1x speedup" on some queries against the Spark Java engine. Operator support:

  | Operator | Status |
  |---|---|
  | Scan | Not supported |
  | Spill | Not supported |
  | Project, Aggregation, Join | Partial ("TPCH-compatible") |
  | Filter, OrderBy | Implemented |

  RMM is a global memory manager that "cannot align with Spark memory system". A Docker image `apache/gluten:centos-9-jdk17-cuda13.1-cudf` exists. The doc was moved on 2026-10-21 — [VeloxGPU.md](https://github.com/apache/gluten/blob/main/docs/backends/velox/get-started/VeloxGPU.md)
- Issue #13122 (2026-09-24, ReemaAlzaid):
  - **Pass rate:** with cuDF enabled and `allowCpuFallback=false`, **48 of 103 TPC-DS SF1 queries pass**. Table scans still run on CPU even then.
  - **Failure causes:** `row_constructor_with_null` (15 queries), `spark_legacy_cast`/`decimal_lessthan` (14), `isnull` (8), and a CudfTopN bug (q4, q11, q51).
  - **Environment:** Velox fork `IBM/velox` `dft-2026_09_22` with cuDF 26.10, on 2× L40S.

  — [GitHub](https://github.com/apache/gluten/issues/13122)
- The umbrella issue #9098 "[VL] Gluten GPU support" was opened 2025-03-21 by jinchengchenghh — [GitHub](https://github.com/apache/gluten/issues/9098). Other open items:
  - GPU shuffle reader, #10933 (2025-10) — [GitHub](https://github.com/apache/gluten/issues/10933)
  - Mixed GPU/CPU cluster scheduling, #11524 (2026-01) — [GitHub](https://github.com/apache/gluten/issues/11524)
  - Not loading GPU libraries on CPU nodes, #11844 — [GitHub](https://github.com/apache/gluten/issues/11844)
- Gluten's latest release is v1.7.0 (2026-08-26) — [GitHub releases](https://github.com/apache/gluten/releases)
- Who works on it: the IBM Velox fork (`IBM/velox`) is the cuDF integration branch, and the GPU umbrella issue was opened by Intel-affiliated committer jinchengchenghh — [#13122](https://github.com/apache/gluten/issues/13122), [#9098](https://github.com/apache/gluten/issues/9098). NVIDIA's involvement on the Velox side is (estimate), not verified in this pass.

**Apache DataFusion Comet**
- Releases: 0.17.0 (2026-06-20), 0.17.1 (2026-07-17), **1.0.0 (2026-08-07)** and **1.1.0 (2026-10-06)** — [GitHub releases](https://github.com/apache/datafusion-comet/releases)
- It is CPU-only. The only GPU-related hit in its issues is a 2024 "Planning to publish Roadmap?" question (#19), and I found no GPU plan — [GitHub](https://github.com/apache/datafusion-comet/issues/19)
- Adoption and speedups: AWS EKS 32-37% on TPC-DS 3TB (see Q3) — [DataFusion blog](https://datafusion.apache.org/blog/2026/06/03/comet-eks/)

**ParaQuery (YC Spring/X25, founder Win Wang, NYC)**
- What it is: "Managed GPU-accelerated Spark + SQL at 2x speed and half the cost". It is Spark-compatible, cloud-agnostic, and compared against Databricks and BigQuery. The launch post says "up to 5x cost efficiency compared to BigQuery". **Team size 1, jobs 0**, and no 2026 news on the YC page [VENDOR] — [YC company page](https://www.ycombinator.com/companies/paraquery), [Launch YC](https://www.ycombinator.com/launches/NSE-paraquery-5x-your-data-warehouse-s-efficiency)
- YC's tweet claims "saves users 60%+ on costs while being 2x faster" [VENDOR] — [X/YC](https://x.com/ycombinator/status/1922064243555217895)
- It is built on spark-rapids. winwang: "All the Spark GPU acceleration right now is done via the Spark-RAPIDS plugin". It aims for compatibility ("we're not 'accelerating all operations' -- merely compatible"). It mainly uses T4/L4 GPUs. He describes a 450TB shuffle run at near-100% GPU utilization — [HN 43967868](https://news.ycombinator.com/item?id=43967868), [HN 43969464](https://news.ycombinator.com/item?id=43969464), [HN 43965330](https://news.ycombinator.com/item?id=43965330), [Launch HN thread](https://news.ycombinator.com/item?id=43965165)
- Users asked how it differs from spark-rapids on EMR or Databricks — [HN 43966969](https://news.ycombinator.com/item?id=43966969), [HN 44028871](https://news.ycombinator.com/item?id=44028871)
- Funding is about $500K from YC, per aggregators (lower reliability) — [startupintros](https://startupintros.com/orgs/paraquery)

**Databricks Photon on GPU**
- At DAIS in June 2024, Databricks announced plans for native NVIDIA support "into Photon" — [Databricks press release](https://www.databricks.com/company/newsroom/press-releases/databricks-and-nvidia-strengthen-partnership-accelerate-enterprise), [NVIDIA blog](https://blogs.nvidia.com/blog/databricks-data-ai/)
- I found no general-availability announcement for 2025-2026. On current classic GPU clusters Photon must be off — [NVIDIA CUDA-X Databricks docs](https://docs.nvidia.com/datascience/deployment/26.10/platforms/databricks/index.html)
- NVIDIA's cuDF-for-Spark qualification docs mention Photon as "Experimental". Most likely this means the qualification tool's support for Photon event logs, not Photon running on GPU (estimate) — [NVIDIA qualification overview](https://docs.nvidia.com/cudf-spark/latest/qualification/overview.html)

**Voltron Data Theseus**
- Vendor claims [VENDOR]:
  - TPC-H SF100,000 (100TB) in under an hour on 5TB of GPU memory (2024).
  - "45x faster than Spark, 20x less cost".
  - A retailer going from 1,400 CPU machines to 14 GPU servers.

  — [BigDATAwire 2024](https://www.hpcwire.com/bigdatawire/2024/02/01/voltron-aims-to-unblock-ai-with-gpu-accelerated-data-processing/), [VentureBeat](https://venturebeat.com/ai/voltron-data-just-partnered-with-accenture-to-solve-one-of-ais-biggest-headaches)
- AMD Instinct support was in preview as of June 2025 — [The Register](https://www.theregister.com/2025/06/18/voltron_amd_sql/)
- A Carahsoft government partnership was announced in April 2025 — [Carahsoft](https://www.carahsoft.com/voltron-data)
- I found no 2026 news. An anonymous Blind post claims about 50% layoffs; this is unverified and low reliability — [Blind](https://www.teamblind.com/post/what-happened-to-voltron-data-bxtoxtrf)

**Apache Auron (formerly Blaze, Kuaishou)**
- It is a Rust/DataFusion native engine called over JNI from Spark, and CPU-only.
- History: entered the ASF incubator 2025-08-05. Releases were 6.0.0-incubating (2025-10-17), 7.0.0 (2026-03-04) and 8.0.0 (2026-07-18).
- It claims "~2x faster, ~50% cluster resources saved" against Spark 3.5 [VENDOR/project].

— [Apache incubator page](https://incubator.apache.org/projects/auron.html), [auron.apache.org](https://auron.apache.org/)

**Sirius (UW-Madison, Xiangyao Yu)**
- A GPU-native SQL engine with drop-in acceleration via Substrait. It works with DuckDB, with StarRocks "coming soon", and was demonstrated with Apache Doris distributed at up to 12.5x (CIDR 2026). No Spark backend was found — [CIDR 2026 paper](https://vldb.org/cidrdb/papers/2026/p12-yogatama.pdf), [arXiv 2508.04701](https://arxiv.org/html/2508.04701v3), [GitHub](https://github.com/sirius-db/sirius), [sirius-db.com](https://www.sirius-db.com/)

**NVIDIA Project Aether**
- An automation framework with predict, optimize, validate and migrate phases. It has a CLI, a REST API and a dashboard, launched with an EMR integration.
- NVIDIA claims jobs are migrated "in as little as four days" against up to a year by hand [VENDOR].
- Free assistance is available by application.

— [NVIDIA EMR blog](https://developer.nvidia.com/blog/migrate-apache-spark-workloads-to-gpus-at-scale-on-amazon-emr-with-project-aether/), [BigDATAwire](https://www.hpcwire.com/bigdatawire/this-just-in/nvidia-accelerates-apache-spark-workloads-with-rapids-and-project-aether/), [NGC listing, v26.4.0](https://catalog.ngc.nvidia.com/orgs/nvidia/cudf-router/resources/aether/-)

**NVIDIA LLM/agent UDF rewriting**
- The cudf-spark repo has a `skills/` directory of agent skills for UDF test generation, conversion to SQL, cuDF RapidsUDF or native CUDA RapidsUDF, and benchmarking. There is also an "Aether Assistant" for UDF conversion — [GitHub skills](https://github.com/NVIDIA/cudf-spark/tree/main/skills)
- Data+AI Summit session: "Coding with AI Agents to Supercharge Your Spark UDFs with GPU-Acceleration" — [DAIS session](https://www.databricks.com/dataaisummit/session/coding-ai-agents-supercharge-your-spark-udfs-gpu-acceleration)

### Inferences
- The only production-grade GPU Spark engine is still spark-rapids. ParaQuery is a managed wrapper around it, and Gluten-cuDF is around 47% TPC-DS SF1 coverage without fallback.
- The CPU-native engines are closing much of the gap NVIDIA's cost claims relied on: Comet at 1.x, Gluten, Auron and Photon. A GPU pitch now has to beat a 1.3-2x CPU-native baseline, not vanilla Spark. (estimate)
- NVIDIA is attacking the "only part of the job is accelerated" problem with tooling (Aether, agent-based UDF rewriting) rather than with engine changes alone.

### Gaps
- **Bodo:** not researched in this pass. I hit the tool-call limit.
- **Velox-cuDF via Presto (IBM/Meta):** not researched in detail.
- **NVIDIA "Spark Connect" GPU work:** not found.
- **Photon GPU:** GA status is unconfirmed. Next step would be the DAIS 2025/2026 keynotes.
- **ParaQuery:** actual 2026 customers and status are unknown beyond the static YC page.
- **Theseus:** 2026 status is unknown.
- **Gluten GPU contributors:** affiliations beyond IBM-velox and the issue authors are unverified.

## Q5. Gaps users report that none of these fill

### Takeaway
The evidence points to these unmet needs:
- Operational friction after the PoC: infra, GPU quota and node startup, tuning.
- I/O-bound workloads where the GPU idles.
- Small or cheap-cluster economics.
- Reliable cost prediction.
- Mixed CPU/GPU clusters.

I found little or no direct user discussion of caching across queries, BI/serving latency, air-gapped installs, or unified-memory boxes (DGX Spark/GH200) for Spark.

### Cited Findings
- **Post-PoC operations and managed infra** are where teams struggle — [HN winwang](https://news.ycombinator.com/item?id=43967477)
- **Prediction does not match reality.** Qualification predicted 4-5x but some runs were slower. Wall-clock time is dominated by GPU node provisioning and pending executors — [HackerNoon 2026-10-20](https://hackernoon.com/benchmarking-nvidia-rapids-on-gke-estimated-vs-actual-spark-speedups)
- **I/O-bound jobs leave the GPU idle** (15% utilization on 8×H100 reading remote HDFS) — [#11431](https://github.com/NVIDIA/cudf-spark/discussions/11431). Cheap GPU instances have weaker local disks — [#11992](https://github.com/NVIDIA/cudf-spark/discussions/11992)
- **Mixed GPU/CPU cluster scheduling** is an open request in Gluten — [#11524](https://github.com/apache/gluten/issues/11524), [#11828](https://github.com/apache/gluten/issues/11828)
- **The memory manager is not integrated with Spark** (global RMM) in Gluten GPU — [VeloxGPU.md](https://github.com/apache/gluten/blob/main/docs/backends/velox/get-started/VeloxGPU.md)
- **Multi-column UDFs are not accelerated**. Per winwang this would need "compiler"-style work — [HN](https://news.ycombinator.com/item?id=43965330)
- **Single GPU per node on Databricks**, and Photon cannot be combined with GPU — [NVIDIA Databricks guide](https://docs.nvidia.com/spark-rapids/user-guide/latest/getting-started/databricks.html), [CUDA-X docs](https://docs.nvidia.com/datascience/deployment/26.10/platforms/databricks/index.html)

### Inferences
- All of the GPU Spark offerings follow a batch-ETL, job-at-a-time model. Nothing in the evidence targets persistent GPU-resident data or caching across queries, or interactive BI latency. This is where engines like Sirius and Theseus position themselves outside Spark. (estimate)
- Single-node, large-memory GPU boxes (GH200, DGX Spark) map poorly onto executor-per-GPU Spark: #11431 shows one HGX node struggling to keep its GPUs fed. (estimate)

### Gaps
- I found no user threads on air-gapped installs, cross-query caching, BI serving latency, or DGX Spark/GH200 for Spark specifically. These are unconfirmed as user-voiced gaps.
