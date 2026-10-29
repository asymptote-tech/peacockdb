# GPU acceleration in real-time OLAP, streaming and distributed analytics engines

Scope note: desk research on 2026-10-28. The web-search budget for the session ran out partway through, and the GitHub API rate limit was hit at the end. Areas left thin because of this are listed under Gaps. GitHub issue and PR facts were read directly with `gh` from the repos.

## Q1. Which engines have GPU issues, PRs or roadmap items, and who asked for them?

### Takeaway
Two named OLAP incumbents now have real GPU work in progress. ClickHouse is merging cuDF into its own tree, and Apache Doris and StarRocks are getting GPU backends through Sirius, built by the Doris PMC Chair and by NVIDIA engineers. Velox/Presto already runs all of TPC-H and TPC-DS on GPU. Pinot, Druid, Databend, Flink, RisingWave, Firebolt and Tinybird show no GPU work, and Databend explicitly declined it as "IO (S3)-bound". The requests on the open-source side come from anonymous individuals ("GPU faster x100"). The serious drivers are vendors (ClickHouse Inc., VeloDB/Doris, NVIDIA, IBM), not named end customers.

### Cited Findings

**ClickHouse (in-tree GPU, built by ClickHouse itself)**
- RFC "[RFC] ClickHouse GPU v1" (#116807, opened 2026-08-27 by scanhex12/Konstantin Vedernikov; "Company or project name: ClickHouse"). Path 1 is to build ClickHouse on SiriusDB via Substrait. The listed limits: PREWHERE, primary-key granule pruning, projections and skip indexes have no Substrait form; the plan goes through four lossy hops; Sirius "does not support the full set of ClickHouse compression codecs, its scan batch size is incompatible with the granule size, and Compact parts are not supported". The alternative it names is "build our own solution based on cuDF, cuCascade and NCCL" — [GitHub #116807](https://github.com/ClickHouse/ClickHouse/issues/116807) (accessed 2026-10-28)
- The same RFC on MergeTree: "MergeTree codecs can't be GPU-decoded in the hot path — a codec only pays off above the ~370 GB/s link, LZ4/ZSTD sit below it. So transcode at pin time: read the Wide part normally, re-encode into Simpatico, store resident … invalidate on merge/mutation/TTL." It also says "Results go D2H straight into `MutableColumns` — no Arrow, we are in-process", and that a block is 65,536 rows while the scan batch is 8 GB — [GitHub #116807](https://github.com/ClickHouse/ClickHouse/issues/116807) (accessed 2026-10-28)
- A second RFC, "GPU Offload of Query Plan Subtrees to Sirius" (#124455, 2026-10-07), comes from rienath (Raufs Dunamalijevs, GitHub company "@ClickHouse"). It describes a separate GPU backend process that embeds Sirius, plus a GPU cache "keyed by (part, column, data version)". On a cache miss ClickHouse decodes the part with the normal MergeTree reader and streams decoded columns. Compressed-block GPU decode is listed as future work. The setting is `allow_experimental_gpu_offload`, and the planned benchmark is ClickBench on GH200 against CPU ClickHouse. The RFC notes that "small queries will be slower on GPU" and that "the first query over cold data … may be slower than the CPU path" — [GitHub #124455](https://github.com/ClickHouse/ClickHouse/issues/124455) (accessed 2026-10-28); [rienath profile](https://github.com/rienath) (accessed 2026-10-28)
- Serxa (GitHub company "ClickHouse Inc.") asked in the RFC thread for GPU memory tracking as "a first-class citizen just like we do with main memory" — [GitHub #116807 comments](https://github.com/ClickHouse/ClickHouse/issues/116807) (accessed 2026-10-28)
- GPU PRs merged into ClickHouse master:
  - [GPU] Add nvcc cmake (#117734)
  - add cudf contrib (#117869)
  - Add cuCascade to contrib (#120803)
  - utils to work with GPU memory and cuDF (#122867, "Merged into 26.11")
  - upgrade cuDF & CCCL for streaming aggregations (#127166)
  - `GDeflate` codec on the CPU (#127422, "~SOTA codec for GPU decompression - highly important for our future pipeline", in 26.11)

  Still open: aggregation (#119502), inner hash joins (#121061), and "MergeTree compressed blocks reader" (#127604, 2026-10-23, "first part of the whole GPU pipeline") — [PR #117869](https://github.com/ClickHouse/ClickHouse/pull/117869), [PR #122867](https://github.com/ClickHouse/ClickHouse/pull/122867), [PR #127422](https://github.com/ClickHouse/ClickHouse/pull/127422), [PR #127604](https://github.com/ClickHouse/ClickHouse/pull/127604) (all accessed 2026-10-28)
- Earlier history: an OpenCL bitonic sort was merged in 2020 and removed in 2020 (#10232, #15023), and CUDA aggregate prototypes were closed in 2020 and 2022 (#11083, #37307). In May 2024 the ClickHouse PM (melvynator) answered the "GPU support" issue #63392 with "a big task that we haven't planned it for the short/mid term" — [GitHub #63392](https://github.com/ClickHouse/ClickHouse/issues/63392) (accessed 2026-10-28). That position has clearly reversed by Aug–Oct 2026.
- Arrow out of MergeTree today: ClickHouse supports `Arrow`/`ArrowStream` output, uncompressed by default, and chDB can return Arrow bytes in-process — [ClickHouse ArrowStream docs](https://clickhouse.com/docs/reference/formats/Arrow/ArrowStream.md) (accessed 2026-10-28). That path still decodes on the CPU. ClickHouse's own GPU design skips Arrow and transcodes to Simpatico at pin time, per #116807 above.

**Apache Doris / VeloDB**
- Sirius PR #1842, merged 2026-09-19 and authored by morningman (bio "Apache Doris PMC Chair"), adds `experimental/doris`. It is a Rust pseudo-BE that joins an *unmodified* Doris 4.1.4 FE, translates fragments to Substrait and runs them on GPU. It passes TPC-H 22/22 and reports a 3.01× geomean speedup on an L40S at SF100 — [sirius PR #1842](https://github.com/sirius-db/sirius/pull/1842) (accessed 2026-10-28)
- Tracking issue #2025 (2026-10-06): SF100 took "154.4 s with Doris's own BE and 40.6 s with this backend". Current limits: "One node only", "One query at a time", "`local()` parquet only, read as whole files" — [sirius #2025](https://github.com/sirius-db/sirius/issues/2025) (accessed 2026-10-28)
- The Sirius paper reports "up to 12.5x speedup when integrated with Apache Doris distributed engine" — [arXiv 2508.04701 / CIDR 2026](https://arxiv.org/abs/2508.04701) (accessed 2026-10-28)
- Doris itself has no GPU execution PRs. The only GPU-related Doris PR found is a FAISS build switch (`FAISS_ENABLE_GPU`, default OFF) — [Doris PR #62133](https://github.com/apache/doris/pull/62133) (accessed 2026-10-28)

**StarRocks / CelerData**
- In 2024 a user (coffeecms) opened "Integrate with CUDA". A StarRocks maintainer (kangkaisen) replied "We will consider it, which is a huge work", and the issue was closed as stale — [StarRocks #45508](https://github.com/StarRocks/starrocks/issues/45508) (accessed 2026-10-28)
- Sirius is building a StarRocks *compute node* (#826, opened 2026-05-29 by mbrobbel/Matthijs Brobbel, "@NVIDIA"). Through Oct 2026 there are active PRs by aocsa (Alexander Ocsa, "Senior Solutions Architect, Data Processing at NVIDIA"): runtime filters, RIGHT SEMI joins, two-phase AVG, and end-to-end TPC-H Q16/Q21/Q22 through the CN. The Sirius README says "currently supports DuckDB and Starrocks (coming soon)" — [sirius #826](https://github.com/sirius-db/sirius/issues/826), [sirius #2199](https://github.com/sirius-db/sirius/issues/2199), [sirius README](https://github.com/sirius-db/sirius) (accessed 2026-10-28). No CelerData statement was found.

**Databend**
- A 2024 GPU feature request was closed "not planned". The maintainer said "Databend is predominantly IO (S3)-bound rather than CPU-bound, and there is no GPU execution path on the roadmap. Reopen with a concrete workload" — [Databend #15606](https://github.com/databendlabs/databend/issues/15606) (accessed 2026-10-28)

**Pinot / Druid / Firebolt / Tinybird / SingleStore**
- A GitHub search for "GPU" in apache/pinot returned only "Vector embeddings support" (#10919) and an unrelated text_match bug. apache/druid returned one unrelated Kafka ingestion bug. Neither has GPU execution issues — [Pinot #10919](https://github.com/apache/pinot/issues/10919) (accessed 2026-10-28)
- Web search found no GPU material for Firebolt, Tinybird or Imply. Firebolt engines are configured by "node count and CPU, RAM, and SSD mix" — [Firebolt vs Druid](https://www.firebolt.io/comparison/druid-vs-firebolt) (accessed 2026-10-28). SingleStore was not checked (search budget exhausted).

**Velox / Presto / Gluten / Daft**
- IBM Research and NVIDIA ("Accelerating Presto with GPUs", arXiv 2606.24647, June 2026; authors include Todd Mostak of NVIDIA, the HEAVY.AI founder):
  - "about 50% of Velox operators have GPU-aware implementations", enough to run all 22 TPC-H and 99 TPC-DS queries entirely on GPU
  - up to 6× better cost/performance than CPU Presto
  - SF1K on 8× B200 in 19.9 s; SF30K on 24–40 GB200 in about 4.5 min

  — [arXiv 2606.24647](https://arxiv.org/html/2606.24647v1) (accessed 2026-10-28)
- Velox has many active cudf issues (Oct 2026): CPU fallbacks for casts, `array_agg` ORDER BY, GPU exchange, and "Migrate GPU execution from DriverAdapter to physical plan nodes" — [Velox #19573](https://github.com/facebookincubator/velox/issues/19573), [Velox #15021](https://github.com/facebookincubator/velox/issues/15021) (accessed 2026-10-28)
- The Gluten GPU (cuDF) doc calls it experimental: "TPC-H SF1 validated", Scan "not supported", up to 8.1× on some queries. A Sept 2026 issue lists TPC-DS SF1 failures with CPU fallback disabled — [Gluten VeloxGPU doc](https://gluten.apache.org/docs/v1.5.0/get-started/VeloxGPU.html), [Gluten #13122](https://github.com/apache/gluten/issues/13122) (accessed 2026-10-28)
- Daft (Eventual) design issue #7374 (2026-08-11) covers device-resident columns. Its measurements: "an elementwise GPU kernel costs 0.17 ms while the per-op host round trip costs 34–70 ms — a 200–700x transfer tax" — [Daft #7374](https://github.com/Eventual-Inc/Daft/issues/7374) (accessed 2026-10-28)

**Streaming**
- Flink's GPU support is FLIP-108 (Flink 1.11, 2020). It covers *resource scheduling* of GPUs, not GPU operators. FLIP-577 "AI-Native Flink" is about inference, multimodal data and "GPU-backed execution" in pipelines, not GPU SQL operators — [FLIP-108](https://cwiki.apache.org/confluence/spaces/FLINK/pages/148640003/FLIP-108+Add+GPU+support+in+Flink), [FLIP-577](https://cwiki.apache.org/confluence/spaces/FLINK/pages/421957275/FLIP-577+AI-Native+Flink+%E2%80%94+An+Umbrella+Proposal+for+Multimodal+Data+Processing) (accessed 2026-10-28)
- No cuDF-based Flink operator was found. In research, Xekalaki et al. (VLDB 2022) ran Java UDFs in Flink on GPU/FPGA (up to 65× and 184×). Slider (VLDB 2021) and Springald (TPDS 2024) cover GPU window aggregation — [Slider (ACM)](https://dl.acm.org/doi/pdf/10.1145/3465998.3466014), [Springald](https://dl.acm.org/doi/abs/10.1109/TPDS.2024.3431611) (accessed 2026-10-28)
- RisingWave: a GitHub "GPU" search returned only a SIMD compile bug. Materialize was not checked (rate-limited).
- StreamFusion builds a Rust/DataFusion accelerator plugged into Flink's JVM job graph, on CPU. It notes DataFusion "cannot cover the whole spectrum of Flink SQL operators" — [StreamFusion blog](https://streamfusion.tech/blogs/streaming-for-the-ai-age/) (accessed 2026-10-28)

### Inferences
- The "who asked" signal is vendor-driven, not customer-driven. Every substantive GPU effort is led by the engine vendor or by NVIDIA. I found no named end-user company on any issue asking for GPU OLAP. The individual requests (coffeecms on StarRocks and Databend, zhanglistar on ClickHouse) carry no workload detail.
- Sirius's adapter pattern (pseudo-BE or CN that joins an unmodified FE) is the same move peacockdb would make as "GPU backend for engine X". NVIDIA and the Doris PMC Chair are already doing it for Doris and StarRocks, and ClickHouse is doing it in-house.

### Gaps
- The question of named companies asking for GPU in Pinot, Druid, SingleStore, Tinybird and Firebolt is still open. These were not searched in depth because the search budget was exhausted.
- I could not confirm whether scanhex12 is a ClickHouse employee. The profile has no company, but scanhex12 merges their own PRs into ClickHouse master, which implies committer rights.

## Q2. Which incumbents or NVIDIA already ship it (platform risk)?

### Takeaway
NVIDIA's strategy is to put cuDF into every existing engine instead of backing a new one:
- Velox, which covers Presto and Gluten/Spark
- Sirius, which covers DuckDB, Doris and StarRocks, with ClickHouse evaluating it
- Spark-RAPIDS

NVIDIA also absorbed HEAVY.AI in 2025. Platform risk is very high for any "GPU plugin for engine X" direction and for "GPU-resident hot cache".

### Cited Findings
- NVIDIA's cuDF FAQ (updated 2026-08-19) recommends adding GPU execution "through Velox … at the physical operator layer", so the engine's planner and API stay unchanged. It claims Presto "up to 12x" — [NVIDIA perspectives FAQ](https://perspectives.nvidia.com/cudf/faq/we-want-to-add-a-gpu-execution-path-to-our-existing-query-engine-but-don-t-want-to-fork-the-mainline-6627a6/) (accessed 2026-10-28). The page notes it contains AI-generated content.
- Velox site: "GPU-Accelerated Presto C++ is Here", with nightly GPU Presto C++ images — [velox-lib.io](https://velox-lib.io/) (accessed 2026-10-28)
- Sirius is built by UW-Madison with NVIDIA engineering support. On ClickBench it reports "at least 7.2x higher cost-efficiency than the top CPU-based systems" on a Lambda GH200 at $1.50/h. The README now says the ClickBench result came from a "legacy implementation" pinned at f79a6f42 — [NVIDIA blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/), [sirius README](https://github.com/sirius-db/sirius) (accessed 2026-10-28)
- Sirius already has GPU-resident hot data:
  - `pin_table` loads columns into GPU or pinned host memory
  - Simpatico compresses the pinned data
  - tiered GPU/host/disk memory via cuCascade
  - `sirius.max_concurrent_queries` defaults to 1

  Licence Apache-2.0, about 1.1k stars — [sirius README](https://github.com/sirius-db/sirius) (accessed 2026-10-28)
- HEAVY.AI (MapD/OmniSci) was "acquired by Nvidia" in 2025 per Wikipedia and NEA's portfolio, and dbdb.io says the project was abandoned after the acquisition. No NVIDIA press release was found — [Wikipedia HEAVY.AI](https://en.wikipedia.org/wiki/HEAVY.AI), [NEA portfolio](https://www.nea.com/portfolio/heavy-ai), [dbdb.io HeavyDB](https://dbdb.io/db/heavydb) (accessed 2026-10-28). Todd Mostak now appears as an NVIDIA author on the GPU Presto paper — [arXiv 2606.24647](https://arxiv.org/html/2606.24647v1) (accessed 2026-10-28)
- Kinetica sells a GPU in-memory database with a Confluent-certified Kafka connector and claims "billions of records per minute" ingest (vendor claim, undated). It names USPS for real-time route optimisation — [Confluent blog](https://www.confluent.io/blog/kinetica-releases-confluent-certified-connector-for-apache-kafka/), [QCM partner page](https://qcmtech.com/partner-kinetica/kinetica-overview) (accessed 2026-10-28). Its pivot to vector search and AI is reported only by a personal blog — [theaivibe.org](https://theaivibe.org/blog/why-i-built-gpudb-gpu-sql-engine-cuda-apple-silicon) (accessed 2026-10-28; low-quality source)
- Voltron Data Theseus got an Accenture Ventures investment in Feb 2025. No 2026 release or customer news was found — [iconnect007](https://iconnect007.com/article/144193/accenture-invests-in-voltron-data-to-help-organizations-use-gpu-technology-to-simplify-largescale-data-processing/144190/milaero) (accessed 2026-10-28). Sirius contributors include former BlazingSQL/Voltron names (felipeblazing, wmalpica) and NVIDIA staff (mbrobbel, mike-wendt) — [sirius contributors](https://github.com/sirius-db/sirius/graphs/contributors) (accessed 2026-10-28)
- Streaming: NVIDIA AI Enterprise lists Morpheus as "End of Support" — [NVIDIA EOL notices](https://docs.nvidia.com/ai-enterprise/lifecycle/latest/eol-notices.html) (accessed 2026-10-28). cuStreamz (Streamz + Dask + cuDF, with a Kafka reader "at least 2x" faster than confluent-kafka-python) is 2020-era material, and its current status is unverified — [RAPIDS cuStreamz blog](https://medium.com/rapids-ai/the-custreamz-series-the-accelerated-kafka-datasource-4faf0baeb3f6) (accessed 2026-10-28)
- Cloudflare acquired Arroyo (Rust/Arrow/DataFusion streaming SQL) in April 2025 to power Pipelines. The engine stays open source — [Cloudflare blog](https://blog.cloudflare.com/cloudflare-acquires-arroyo-pipelines-streaming-ingestion-beta/) (accessed 2026-10-28)
- OpenAI acquired Rockset in June 2024 to "power our retrieval infrastructure across products". Rockset did continuous ingest from Kafka, MongoDB, DynamoDB and S3 with sub-second SQL on semi-structured data. The motive was retrieval and indexing for LLM products, not GPU analytics — [OpenAI](https://openai.com/index/openai-acquires-rockset/), [InfoWorld](https://www.infoworld.com/article/3715564/openai-acquires-rockset-to-enhance-chatgpt-real-time-data-processing.html) (accessed 2026-10-28)

### Inferences
- NVIDIA funds integrations into incumbents, staffs Sirius and absorbed the HEAVY.AI talent. A small team's "GPU engine for incumbent X" competes with NVIDIA solution architects working with the X PMC. That is platform risk in its most direct form.
- GPU stream processing has no incumbent product, and NVIDIA's streaming bets (Morpheus, cuStreamz) appear wound down. That gap may mean low demand rather than an opening (estimate).

### Gaps
- Kinetica's 2025–2026 status, customers and funding were not verified from a primary source.
- How OpenAI uses Rockset technology after the acquisition is not public in the sources found.

## Q3. Where is the customer pain real (cost, latency, concurrency), and is the GPU the right fix?

### Takeaway
The pain is real in user-facing analytics: Pinot users run 10k+ QPS at under 100 ms p99. But that pain is about concurrency on small, pre-indexed queries, and current GPU engines are worst at exactly that (one query at a time, slow small queries). The GPU wins clearly on large scans, joins and aggregations, provided data is resident in VRAM or the decode path is fixed. The data path is the bottleneck everywhere. The cuDF Parquet reader runs 10× below I/O rate, CPU↔GPU staging dominates, Databend's workload is S3-bound, and ClickHouse plans to transcode to a GPU codec rather than decode LZ4/ZSTD on the GPU.

### Cited Findings
- **Concurrency pain:**
  - Stripe runs Pinot at "10,000 QPS or more", and Pinot production clusters serve 100K+ QPS — [StarTree](https://startree.ai/resources/achieving-99th-percentile-latency-sla-using-apache-pinot/) (accessed 2026-10-28; vendor)
  - A user-facing analytics deck lists 700M users, 10,000+ QPS, a p99 under 100 ms and freshness in seconds — [Databricks-hosted Kafka+Pinot deck](https://microsites.databricks.com/sites/default/files/2022-07/Optimizing-Speed-and-Scale-of-User-Facing-Analytics-Using-Apache-Kafka-and-Pinot.pdf) (accessed 2026-10-28)
  - A StarTree customer case: scan about 2B rows per request at 200+ QPS with p95 of 2 s. It was solved with a star-tree index (precomputation), not more compute — [StarTree blog part 3](https://startree.ai/blog/star-tree-index-in-apache-pinot-part-3-understanding-the-impact-in-real-customer) (accessed 2026-10-28; vendor)
- **GPU mismatch with high-QPS small queries:**
  - ClickHouse's RFC: "Small queries will be slower on GPU as kernel-launch and plan-translation overhead make the CPU faster below some number of rows" — [ClickHouse #124455](https://github.com/ClickHouse/ClickHouse/issues/124455) (accessed 2026-10-28)
  - Sirius `max_concurrent_queries` defaults to 1 — [sirius README](https://github.com/sirius-db/sirius) (accessed 2026-10-28)
  - The Sirius Doris backend runs "One query at a time" — [sirius #2025](https://github.com/sirius-db/sirius/issues/2025) (accessed 2026-10-28)
- **Data path:**
  - IBM and NVIDIA measured the cuDF 25.04 Parquet reader at "10× slower than the theoretical I/O rate", because Parquet's nested metadata "prevents GPUs from fully exploiting memory bandwidth". Their custom Arrow-like format reached 95% of theoretical throughput (about 22 GB/s). CPU↔GPU movement "has a dramatic negative effect on overall performance". Cold vs hot runs: 190 s vs 108 s. Queries that exceed GPU memory fail, because spilling is not done yet — [arXiv 2606.24647](https://arxiv.org/html/2606.24647v1) (accessed 2026-10-28)
  - The EPFL CIDR 2020 GPU-HTAP work found that PCIe interconnects, not GPU compute, dominate GPU-DBMS limits — [Proteus CIDR2020](https://proteus.epfl.ch/publications/CIDR2020-gpuhtap) (accessed 2026-10-28)
  - Databend: "IO (S3)-bound rather than CPU-bound" — [Databend #15606](https://github.com/databendlabs/databend/issues/15606) (accessed 2026-10-28)
  - ClickHouse RFC: LZ4/ZSTD can't be GPU-decoded in the hot path, so the plan is to transcode at pin time; ClickHouse merged a GDeflate codec for GPU decompression — [#116807](https://github.com/ClickHouse/ClickHouse/issues/116807), [#127422](https://github.com/ClickHouse/ClickHouse/pull/127422) (accessed 2026-10-28)
  - Daft: a 200–700× host round-trip tax over kernel time — [Daft #7374](https://github.com/Eventual-Inc/Daft/issues/7374) (accessed 2026-10-28)
- **Streaming:**
  - The pain in Flink is state size and checkpoints. "While Flink supports storing TBs of state in RocksDB, in practice this proves operationally difficult" — [Arroyo blog](https://www.arroyo.dev/blog/stateful-stream-processing/) (accessed 2026-10-28)
  - RisingWave checkpoints every 1 s by default with remote state — [RisingWave vs Flink](https://docs.risingwave.com/reference/risingwave-flink-comparison) (accessed 2026-10-28)
  - Neither source puts the pain in operator compute.
- **HTAP / freshness:** HeraDB (SIGMOD 2026) targets "real-time analysis of transaction-centric HTAP with CPU-GPU hybrid query execution" (seen only on a faculty listing) — [NEU faculty page](https://faculty.neu.edu.cn/zhangyf/en/lwcg/39667/content/37662.htm) (accessed 2026-10-28). RateupDB (VLDB 2021) is an industrial CPU/GPU hybrid — [VLDB paper](https://www.vldb.org/pvldb/vol14/p2999-lee.pdf) (accessed 2026-10-28)
- **Observability:**
  - ClickHouse frames observability in 2026 as an analytics problem on columnar stores — [ClickHouse](https://clickhouse.com/resources/engineering/what-is-observability) (accessed 2026-10-28)
  - GPU-cloud operators (Coupang job post) cite high-cardinality GPU telemetry (GPU IDs, tenants, workloads) as a design challenge on a Loki/Mimir stack — [Coupang job](https://www.coupang.jobs/jp/jobs/7992094/sr-staff-observability-engineer-gpu-cloud-telemetry-platform/) (accessed 2026-10-28)
  - Datadog added GPU monitoring in Apr 2026 — [pulse.bot](https://www.pulse.bot/ai/news/datadog-digs-down-into-gpu-efficiency-as-ai-costs-soar-f6693eb9-c6af-4a04-96ba-48c5f550412d/) (accessed 2026-10-28; aggregator)

### Inferences
- Real-time user-facing analytics (Pinot/Druid/StarRocks territory) is a poor GPU fit. The pain is QPS × small latency, which indexes and pre-aggregation already solve on CPU. A GPU would need batching of many concurrent queries into one kernel launch ("query batching"/shared scans) to compete. No product does this, which is a research-grade risk (estimate).
- GPU helps most where queries are scan- and join-heavy over a hot set that fits in VRAM (tens to hundreds of GB) and ad-hoc, so indexes don't help: interactive exploration, dashboards over recent data, heavy joins. Even then, cold data is slower on GPU unless decode is solved. That matches the human's spark-rapids observation.
- peacockdb's Parquet-with-row-group-pruning path faces the same "10× below I/O rate" problem as cuDF. Whoever owns a fast GPU decode path, or keeps data VRAM-resident in a GPU-friendly encoding, holds the real advantage (estimate).

### Gaps
- I found no third-party cost-per-QPS numbers for real-time OLAP.
- I found no evidence that any named Pinot, Druid or StarRocks user asked for a GPU.
- No paper on GPU incremental view maintenance was checked (search budget exhausted).

## Q4. Per-slug verdicts and new candidate directions

### Takeaway
Of the named slugs, clickhouse, realtime-other and distributed-pluggable are effectively taken by the vendors plus NVIDIA. streaming-flink lacks evidence of compute pain. realtime-vram-cache is real but is exactly what Sirius and ClickHouse are building. The more defensible gaps are narrow places NVIDIA's integrations don't reach: GPU-resident data that already lives on the GPU (ML/feature pipelines), small single-node "bring your own Parquet/Kafka" setups, and the decode path itself.

### (a) Named slugs — one-line cited reason
- **clickhouse**: Kill as a third-party plugin. ClickHouse Inc. is already merging cuDF, cuCascade and a GDeflate codec into master (26.11), with a GPU MergeTree compressed-block reader in review and a Sirius-offload RFC by a ClickHouse employee. It bypasses Arrow and transcodes parts at pin time — [#117869](https://github.com/ClickHouse/ClickHouse/pull/117869), [#127422](https://github.com/ClickHouse/ClickHouse/pull/127422), [#124455](https://github.com/ClickHouse/ClickHouse/issues/124455), [#116807](https://github.com/ClickHouse/ClickHouse/issues/116807) (accessed 2026-10-28)
- **realtime-vram-cache**: Real technique but crowded. Sirius already ships `pin_table` and Simpatico-compressed GPU/host residency, and ClickHouse's RFC designs a part-keyed GPU cache. HEAVY.AI, the original GPU-resident product, was absorbed by NVIDIA in 2025. Hot vs cold runs are 108 s vs 190 s in GPU Presto — [sirius README](https://github.com/sirius-db/sirius), [#124455](https://github.com/ClickHouse/ClickHouse/issues/124455), [Wikipedia HEAVY.AI](https://en.wikipedia.org/wiki/HEAVY.AI), [arXiv 2606.24647](https://arxiv.org/html/2606.24647v1) (accessed 2026-10-28)
- **streaming-flink**: Weak. Flink's GPU work is resource scheduling (FLIP-108) and AI inference (FLIP-577), not GPU SQL operators. NVIDIA Morpheus is End of Support, the Rust streaming engine Arroyo went to Cloudflare, and streaming pain is state and checkpointing, not compute — [FLIP-108](https://cwiki.apache.org/confluence/spaces/FLINK/pages/148640003/FLIP-108+Add+GPU+support+in+Flink), [NVIDIA EOL](https://docs.nvidia.com/ai-enterprise/lifecycle/latest/eol-notices.html), [Arroyo blog](https://www.arroyo.dev/blog/stateful-stream-processing/) (accessed 2026-10-28)
- **realtime-other**: Taken or not wanted. The Doris PMC Chair merged a Sirius GPU pseudo-BE (TPC-H SF100 154.4 s → 40.6 s), NVIDIA staff are building a Sirius StarRocks CN, Databend declined GPU as S3-bound, Pinot and Druid have no GPU asks, and Rockset went to OpenAI for retrieval, not GPU — [sirius #2025](https://github.com/sirius-db/sirius/issues/2025), [sirius #826](https://github.com/sirius-db/sirius/issues/826), [Databend #15606](https://github.com/databendlabs/databend/issues/15606), [OpenAI](https://openai.com/index/openai-acquires-rockset/) (accessed 2026-10-28)
- **distributed-pluggable**: Taken by NVIDIA and IBM. Velox-cuDF runs all 22 TPC-H and 99 TPC-DS in Presto at up to 6× cost/perf, Gluten inherits it (experimental), and NVIDIA's official advice is "add GPU through Velox". Daft is building device-resident columns itself — [arXiv 2606.24647](https://arxiv.org/html/2606.24647v1), [Gluten doc](https://gluten.apache.org/docs/v1.5.0/get-started/VeloxGPU.html), [NVIDIA FAQ](https://perspectives.nvidia.com/cudf/faq/we-want-to-add-a-gpu-execution-path-to-our-existing-query-engine-but-don-t-want-to-fork-the-mainline-6627a6/), [Daft #7374](https://github.com/Eventual-Inc/Daft/issues/7374) (accessed 2026-10-28)

### (b) New candidate directions

The yardstick is 22,830 non-test lines (Y). All build sizes are estimates.

| slug | one-line | evidence anyone needs it (named group, pain, link) | GPU-centred? | rough build size vs Y (estimate) | platform risk |
|---|---|---|---|---|---|
| gpu-resident-sql-for-ml | SQL over data already on the GPU: torch/cupy tensors, DLPack and Arrow C Device in, filter/join/aggregate without a host round trip, results out as device tensors | Eventual (Daft) measured a "200–700x transfer tax" for per-op host round trips — [Daft #7374](https://github.com/Eventual-Inc/Daft/issues/7374). Flink's FLIP-577 wants "GPU-backed execution" in AI pipelines — [FLIP-577](https://cwiki.apache.org/confluence/spaces/FLINK/pages/421957275/FLIP-577+AI-Native+Flink+%E2%80%94+An+Umbrella+Proposal+for+Multimodal+Data+Processing) (accessed 2026-10-28). No named end user found. | Yes; the data never leaves the GPU, which sidesteps the data-path problem | 0.5–1× Y: DLPack/Arrow-device ingest, Python binding, device result export (estimate) | Medium-high: Daft, cuDF-pandas/Polars GPU and NVIDIA could all cover it |
| gpu-decode-scan-service | Fast GPU Parquet/lakehouse → Arrow-on-device decode (with transcoding to a GPU codec and a VRAM-resident hot set) as a library or sidecar for CPU engines (DuckDB, DataFusion, Daft) | IBM+NVIDIA: cuDF Parquet reader "10× slower than the theoretical I/O rate" — [arXiv 2606.24647](https://arxiv.org/html/2606.24647v1). ClickHouse needs to transcode to GDeflate/Simpatico because LZ4/ZSTD don't pay — [#116807](https://github.com/ClickHouse/ClickHouse/issues/116807). Human's spark-rapids reader finding (internal). | Yes | 0.5–1× Y (estimate) | High: NVIDIA owns the cuDF Parquet reader, nvCOMP and kvikio; Sirius has Simpatico |
| kafka-window-sql-gpu | Keep the last N minutes or hours of one or more Kafka topics in VRAM (columnar, possibly GPU-compressed) and serve ad-hoc SQL with joins over it, without a separate OLAP cluster | Kinetica sells exactly this (Kafka connector, USPS route optimisation) — [Confluent blog](https://www.confluent.io/blog/kinetica-releases-confluent-certified-connector-for-apache-kafka/). cuStreamz had a Kafka→cuDF reader — [RAPIDS blog](https://medium.com/rapids-ai/the-custreamz-series-the-accelerated-kafka-datasource-4faf0baeb3f6) (accessed 2026-10-28). No named small user found. | Yes | 0.5–1× Y: Kafka consumer, append-only device columns, retention/eviction (estimate) | Medium: Kinetica is an incumbent; ClickHouse with a GPU cache could cover it later |
| gpu-telemetry-analytics | Ad-hoc SQL over high-cardinality GPU-cluster telemetry (DCGM metrics, job logs) running on the operator's own idle GPUs | Coupang GPU-cloud observability role names high-cardinality GPU IDs/tenants as a challenge — [Coupang job](https://www.coupang.jobs/jp/jobs/7992094/sr-staff-observability-engineer-gpu-cloud-telemetry-platform/). Last9: GPU signals sit in systems that "don't talk to each other" — [Last9](https://last9.io/blog/from-gpu-silicon-to-business-metrics-the-8-layers-of-gpu-observability) (accessed 2026-10-28) | Partly: the GPUs are free hardware for this user, but the workload is ingest-heavy | 1× Y including ingest and connectors (estimate) | Medium-high: ClickHouse, Datadog, Grafana |
| htap-gpu-replica | A GPU analytic replica of Postgres/MySQL fed by CDC, with VRAM-resident fresh data for heavy joins over operational data | HeraDB SIGMOD 2026 and RateupDB VLDB 2021 show research and industrial interest — [NEU listing](https://faculty.neu.edu.cn/zhangyf/en/lwcg/39667/content/37662.htm), [RateupDB](https://www.vldb.org/pvldb/vol14/p2999-lee.pdf) (accessed 2026-10-28). No named user found. | Yes | 1–1.5× Y: CDC apply, update/delete merge on device (estimate) | Medium: no incumbent GPU product found, but CPU HTAP (TiDB, SingleStore) is strong |
| query-batching-gpu-for-high-qps | Batch many concurrent small dashboard/API queries into shared GPU scans to win on QPS per dollar | Pain is real (Stripe 10k+ QPS on Pinot) — [StarTree](https://startree.ai/resources/achieving-99th-percentile-latency-sla-using-apache-pinot/), but current GPU engines are slow on small queries and run one query at a time — [#124455](https://github.com/ClickHouse/ClickHouse/issues/124455), [sirius #2025](https://github.com/sirius-db/sirius/issues/2025) (accessed 2026-10-28) | Yes | 1–2× Y; research risk (estimate) | Low today, but technically unproven. Flag as a moonshot, not alpha material |
| sirius-style-pseudo-backend-for-uncovered-engine | Join an unmodified FE (Trino worker, Pinot server, Druid historical) as a GPU backend speaking its native protocol | Sirius proved the pattern for Doris (3.01× geomean, unmodified FE) — [sirius #1842](https://github.com/sirius-db/sirius/pull/1842) (accessed 2026-10-28). No evidence that Trino, Pinot or Druid users want it. | Yes | 1–1.5× Y per engine protocol (estimate) | Very high: NVIDIA is expanding Sirius host coverage ("support for more engines") |

### Inferences
- Across all of this, "a GPU plugin for an incumbent engine" is the space NVIDIA is actively claiming. The directions with lower platform risk move data the incumbents don't reach: data already on the GPU, a short Kafka window, or CDC. Those are also the directions where peacockdb's own planner and executor, rather than an adapter, would be the product (estimate).

### Gaps
- None of the new candidates has a named end user with a stated pain. The best evidence is engineering-level (Daft's measurement, the IBM decode measurement). Finding 1–2 users needs direct outreach, not desk research.
- Not checked because of search and API limits: SingleStore and Materialize GPU status, the current RAPIDS status of cuStreamz/libcudf_kafka, papers on GPU incremental view maintenance, and Tinybird/Firebolt customer asks.
