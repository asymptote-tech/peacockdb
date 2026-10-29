# GPU acceleration for data lakes: competitors, platform risks, integration interfaces (as of 2026-10-29)

All URLs accessed 2026-10-29 unless noted. "Search snippet" means the claim came from a search-result summary, not a full page fetch. Lines marked "(prior knowledge, not fetched)" come from background knowledge and were not re-checked this session. No repos were cloned, so there are no commit SHAs; PR and issue numbers are given instead.

## Q1. NVIDIA's own stack (Spark RAPIDS, Velox+cuDF/Presto GPU, Polars GPU, GTC 2026, partnerships, acquisitions)

### Takeaway
NVIDIA does not sell a lakehouse engine. It ships cuDF and pushes it into every major open engine (Spark, Presto/Velox, DuckDB via Sirius, Polars, Trino via Starburst Torin), and in 2025 it bought HEAVY.AI. Its Iceberg support exists but has gaps: Spark RAPIDS covers Iceberg v1/v2 only, with no deletion vectors and no nested types. Presto/Velox GPU work shows Parquet scan is 60–70% of GPU query time, which is the problem a decode tier would target. It also shows that NVIDIA and its partners are attacking that problem inside the engines themselves.

### Cited Findings
**Spark RAPIDS: Iceberg**
- The 26.06 docs call Iceberg support "experimental, limited":
  - Versions: Spark 3.5.0–3.5.7 with Iceberg 1.6.1 (Scala 2.12), and Iceberg 1.9.2 for Spark 3.5.4–3.5.7 (Scala 2.13).
  - Table format: v1 and v2 only. v3 is not mentioned.
  - Catalogs: S3Tables, REST and Hadoop are official; Hive and JDBC are experimental.
  - Types: primitives only. Struct, list and map fall back to CPU.
  - Files: Parquet only (no ORC or Avro). Data files, equality deletes and position deletes are supported; deletion vectors are not.
  - Metadata tables run on CPU.
  - GPU writes are supported for INSERT, MERGE, DELETE, UPDATE, CTAS and RTAS.
  - Source: [cuDF-Spark 26.06 Iceberg](https://docs.nvidia.com/cudf-spark/26.06/additional-functionality/iceberg-support.html) (accessed 2026-10-29)
- Older docs (25.06/25.08) supported only the Hadoop filesystem catalog. Databricks Iceberg is not supported. — [26.02 Iceberg docs](https://docs.nvidia.com/spark-rapids/user-guide/26.02/additional-functionality/iceberg-support.html) (search snippet, accessed 2026-10-29)

**Spark RAPIDS: Delta**
- 26.04/26.06 read tables with deletion vectors and liquid clustering on GPU by pushing the DV predicate into the Parquet scan.
- DML that would create DVs falls back to CPU.
- On Databricks, DBR 14.3 DV reads fall back to CPU. On DBR 17.3 they run on GPU only with the metadata row index and RAPIDS DV pushdown.
- Source: [cuDF-Spark 26.06 Delta](https://docs.nvidia.com/cudf-spark/26.06/additional-functionality/delta-lake-support.html) (search snippet, accessed 2026-10-29)
- Spark RAPIDS was rebranded in its docs path as "cudf-spark" (docs.nvidia.com/cudf-spark/…). — same URLs (accessed 2026-10-29)

**Velox + cuDF / Presto GPU**
- In June 2026 Presto published nightly GPU-accelerated Presto C++ images. cuDF runs scans, filters, projections, joins and aggregations, and is switched on with `cudf.enabled`. — [prestodb.io blog 2026-06-24](https://prestodb.io/blog/2026/06/24/gpu-accelerated-presto-c/) (search snippet, accessed 2026-10-29)
- IBM Research paper "Accelerating Presto with GPUs" (arXiv 2606.24647, VLDB 2026):
  - About 50% of Velox operators have GPU-aware implementations, enough to run all 22 TPC-H and 99 TPC-DS queries fully on GPU.
  - Up to 6× cheaper than CPU-only Presto.
  - GPU↔CPU transfers for fallback operators "can dominate query execution time".
  - Source: [arXiv 2606.24647](https://arxiv.org/html/2606.24647v1) (search snippet, accessed 2026-10-29)
- Velox cudf backend activity, October 2026:
  - PR #19790 (2026-10-23) adds GPU DATE/TIMESTAMP→VARCHAR casts and references the "cuDF Hive or Iceberg scan" path. So a cuDF Iceberg scan exists inside Velox. — [velox PR #19790](https://github.com/facebookincubator/velox/pull/19790) (search snippet, accessed 2026-10-29)
  - PR #19590 adds strict-mode CPU-operator allowlists and records fallback per operator. — [velox PR #19590](https://github.com/facebookincubator/velox/pull/19590) (accessed 2026-10-29)
  - Issue #19607 tracks 37 Presto e2e tests that fail with cuDF enabled. — [velox PR #19590 context](https://github.com/facebookincubator/velox/pull/19590) (search snippet; issue not fetched)

**GTC 2026 sessions (third-party recap)**
- S81563, "Unlock Fast, Cost-Effective Interactive Analytics on Massive Data Lakehouses" (NVIDIA plus IBM Research):
  - Table scan and Parquet I/O were 60–70% of Presto GPU runtime at SF100/SF1K ("mostly an I/O problem").
  - Delta Binary Packed encoding "makes a huge difference" on GPU.
  - NUMA pinning plus DBP gave about 30%.
  - Velox's async cache coalescing cut requests from about 700 to about 200.
  - AWS throttles S3 throughput per VM. The workaround is parallel CPU instances that read S3 and write straight to GPU memory.
  - Presto GPU was about 30× faster than an 8-node Grace cluster on TPC-H SF1K with 4×B200. With CuDFExchange, SF1K ran 690 s → 60 s.
  - Source: [GTC 2026 accelerated analytics recap](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (third-party blog, accessed 2026-10-29)
- Other items from the same recap:
  - NVIDIA released cuCascade, which handles GPU/host/disk memory tiers and spilling, derived from Theseus: [github.com/NVIDIA/cuCascade](https://github.com/NVIDIA/cuCascade).
  - RAPIDS MPF (rapidsmpf) handles multi-GPU shuffle.
  - cuDF gained Vortex file format support.
  - HEAVY.AI/OmniSci is "to be open-sourced late Q2 2026".
  - Speakers include Joshua Patterson and Todd Mostak (NVIDIA) and Felipe and Rodrigo Aramburu (NVIDIA).
  - Source: [same recap](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-29)

**GTC 2026 announcements and partners**
- NVIDIA announced cuDF/cuVS integrations with IBM watsonx.data and Dell AI Data Platform. Google BigQuery appears only in one secondary source.
- Claimed results: Spark up to 5× faster and 10× TCO at 10 TB; Snap cut costs 76% using cuDF on GKE.
- Source: [NVIDIA GTC 2026 blog](https://blogs.nvidia.com/blog/gtc-2026-news/) and [Atlan recap](https://atlan.com/know/nvidia-gtc-2026-keynote-recap/) (search snippets, accessed 2026-10-29)
- IBM watsonx.data uses Presto accelerated by cuDF via Velox (open source):
  - Nestlé refresh went from 15 min to 3 min with 83% cost savings.
  - "30× price-performance" claimed on an order-to-cash data mart.
  - The expanded partnership was announced at GTC (March 2026).
  - Source: [IT Jungle 2026-03-23](https://www.itjungle.com/2026/03/23/ibm-taps-nvidia-gpus-for-ai-turbocharged-data-mart/), [IBM community blog 2025-10](https://community.ibm.com/community/user/blogs/ali-leclerc/2025/10/07/accelerating-data-analytics-gpu-velox-nvidia) (search snippets, accessed 2026-10-29)
- NVIDIA acquired HEAVY.AI (MapD/OmniSci) in 2025; deal terms are not public. — [Wikipedia HEAVY.AI](https://en.wikipedia.org/wiki/HEAVY.AI), [NEA portfolio](https://www.nea.com/portfolio/heavy-ai) (secondary sources, accessed 2026-10-29)

**Polars GPU engine**
- cudf-polars is in Open Beta, single-GPU, for the Polars Lazy API, and falls back to CPU transparently.
- I found no evidence that it accelerates `scan_iceberg` or Delta scans on GPU.
- Source: [cudf-polars docs](https://docs.rapids.ai/api/cudf/latest/cudf_polars) (accessed 2026-10-29)

### Inferences
- NVIDIA's strategy is to sit inside the engine (cuDF as an operator library plus GPU-native scans), not to run a separate scan tier. That is a direct platform risk: each engine NVIDIA enables gets its own GPU Parquet/Iceberg scan for free.
- Iceberg v3 deletion vectors, equality deletes and nested types are recurring gaps (Spark RAPIDS has no DVs or nested types; Sirius falls back on equality deletes, see Q2). "Full Iceberg v2/v3 correctness on GPU" is a real wedge today, but a closing one (estimate: 12–18 months).
- The S81563 finding that scan is 60–70% of runtime and limited by S3 per-VM throughput suggests the bottleneck is object-store I/O and data placement, not GPU decode FLOPs. A cache and prefetch tier near the GPU has more value than decode speed alone.

### Gaps
- No primary NVIDIA statement on a Databricks Photon GPU or a Snowflake GPU engine. Searches turned up no Databricks or Snowflake GPU SQL announcements.
- Whether watsonx.data GPU Presto is GA or preview is unconfirmed.
- No NVIDIA-branded standalone "lakehouse" product found. The "AI Data Platform" is a reference design that Dell and other OEMs build.
- Whether the Voltron Data team joined NVIDIA is not confirmed by an acquisition source. It is inferred from the GTC 2026 speaker affiliations above (Patterson, the Aramburus) and cuCascade being "from Theseus".

## Q2. Sirius (sirius-db/sirius)

### Takeaway
Sirius is a GPU-native engine, from UW-Madison with NVIDIA support, that plugs into DuckDB and through it reads Iceberg and remote Parquet. GPU Iceberg scans were re-enabled in September 2026 with position-delete and v3 deletion-vector support. Distribution comes through a host's coordinator (Apache Doris; StarRocks is "coming soon"), not through a Sirius lakehouse service.

### Cited Findings
- The repo describes Sirius as a "GPU-native composable analytics engine". It plugs into hosts via Substrait; DuckDB is supported and StarRocks is "coming soon". It needs Volta+ GPUs and CUDA 13. — [sirius-db/sirius](https://github.com/sirius-db/sirius) (search snippet, accessed 2026-10-29)
- PR #1425, merged 2026-09-15:
  - Re-enables GPU execution for Iceberg scans.
  - V2 positional deletes and V3 deletion vectors are applied in the GPU scan path; equality deletes fall back to DuckDB.
  - Verified against cuDF 26.06.1 and DuckDB v1.5.4.
  - Source: [sirius PR #1425](https://github.com/sirius-db/sirius/pull/1425) (search snippet, accessed 2026-10-29)
- Issue #2057 (opened 2026-10-07) is about replacing ABI-dependent DuckDB scan verification, so the scan-hook mechanism is fragile and still being reworked. — [sirius issue #2057](https://github.com/sirius-db/sirius/issues/2057) (accessed 2026-10-29)
- CIDR 2026 paper:
  - Hosts are DuckDB and Apache Doris. For distributed runs Sirius uses the host coordinator for the control plane.
  - Up to 12.5× speedup with Doris.
  - 8.3× / 7.4× better cost-efficiency on TPC-H / ClickBench vs. single-node DuckDB.
  - Source: [CIDR 2026 p12](https://vldb.org/cidrdb/papers/2026/p12-yogatama.pdf), [arXiv 2508.04701](https://arxiv.org/html/2508.04701v3) (accessed 2026-10-29)
- GTC 2026 recap:
  - Q9 on 1 TB Parquet ran in 2.5 s vs. 16 s for DuckDB CPU, with data exceeding GPU memory.
  - Sirius keeps its own pinned buffer, moves data compressed, and decompresses on GPU (up to 2× transfer throughput on GB300).
  - The recap calls Sirius an "official DuckDB core extension". A separate search found no confirmation of that, so it is unverified.
  - Source: [GTC recap](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) vs. [NVIDIA ClickBench blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/) (accessed 2026-10-29)
- Sirius held #1 and #2 on the ClickBench hot-run leaderboard (GH200, H100). — [GTC recap](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-29)

### Inferences
- Sirius already covers "GPU Iceberg for DuckDB", including v3 DVs, which Spark RAPIDS lacks. A DuckDB-facing decode service would compete with a free, NVIDIA-backed extension.
- "Transfer compressed, decompress on GPU" is the opposite of a remote decode service that ships decoded columns. It confirms that the bandwidth-efficient path is to move encoded bytes and decode where you compute.

### Gaps
- Whether Sirius is an official DuckDB core extension (the sources conflict).
- No public Sirius roadmap for a standalone distributed or lakehouse service.

## Q3. Other GPU and accelerator vendors: does anyone sell a "GPU scan/cache tier feeding other engines"?

### Takeaway
No vendor found sells a GPU decode/cache tier that feeds third-party CPU engines. Every GPU product is either a full engine (Theseus, Kinetica, SQream, HEAVY.AI, Torin) or an in-engine plugin (RAPIDS, Velox-cuDF, Sirius). The nearest analogues in shape are non-GPU: Speedata and NeuroBlade ASIC/FPGA cards, which offload Parquet decode, filter and project into Spark/Presto as a plugin. The biggest new competitor is Starburst Torin (October 2026), a cuDF engine inside Trino.

### Cited Findings
**Starburst Torin**
- Launched 2026-10-21 in limited GA for Starburst Enterprise only, not Galaxy.
- Trino-based, with SQL operators built on cuDF. It runs alongside the CPU engine and routes GPU-capable steps one operation at a time.
- Claims up to 7× faster TPC-H on Blackwell (g7e.4xlarge; best query 11.4×) and 3.4× ClickBench throughput on an L4 at similar hourly cost.
- Internals (JNI vs. native) and Iceberg-specific scan behavior are not disclosed.
- Source: [Starburst press release](https://www.starburst.io/press-releases/starburst-introduces-torin-gpu-native-analytics-engine-built-on-nvidia-cudf/), [Moor Insights](https://moorinsightsstrategy.com/field-notes/starburst-torin-gpu-trino-sql/) (accessed 2026-10-29)
- Dell AI Data Platform (powered by Starburst) cites up to 6× on Blackwell. — [Blocks & Files 2026-09-18](https://www.blocksandfiles.com/architecture/2026/09/18/starburst-supports-gpus-for-faster-distributed-data-analysis/5297256) (search snippet, accessed 2026-10-29)

**Voltron Data Theseus**
- GPU distributed SQL engine; Accenture Ventures invested in February 2025. — [iConnect007](https://iconnect007.com/article/144193/accenture-invests-in-voltron-data-to-help-organizations-use-gpu-technology-to-simplify-largescale-data-processing/144190/milaero) (accessed 2026-10-29)
- Theseus ran TPC-H 100 TB on 2 DGX A100 using GPUDirect Storage. NVIDIA now presents it as a "research engine from Voltron" behind cuCascade. — [GTC recap](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-29)
- I found no 2026 commercial news.

**HEAVY.AI**
- Acquired by NVIDIA in 2025 ([Wikipedia](https://en.wikipedia.org/wiki/HEAVY.AI)). Open-sourcing was planned for late Q2 2026 ([GTC recap](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html)). (accessed 2026-10-29)

**SQream Blue**
- Marketed as a GPU "data lakehouse" for data preparation on cloud storage. The materials are old marketing with no 2026 news. — [SQream ad-tech brief](https://sqream.com/resources/adtech-one-pager/) (accessed 2026-10-29)

**Kinetica**
- GTC 2026 lists it as a partner. — [GTC recap](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-29)
- No Iceberg-scan-tier offering found.

**Transwarp**
- Claimed TPC-DS 150 GB 26× faster than CPU DuckDB on the same GPU (GTC 2026). — [GTC recap](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-29)

**Speedata (APU, C200 PCIe ASIC)**
- The hardware pipeline handles Parquet decompression, column decoding, filter, projection and nested-data flattening.
- Its Dash software hooks into Spark Catalyst and falls back to CPU.
- Raised a $44M Series B in June 2025. Claims a "50× average" (vendor, unverified).
- Source: [SiliconANGLE 2025-06-03](https://siliconangle.com/2025/06/03/data-analytics-chip-startup-speedata-closes-44m-funding-round/), [Carahsoft Speedata](https://carahsoft.com/speedata) (accessed 2026-10-29)

**NeuroBlade (SPU)**
- PCIe card and AWS EC2 F2 (FPGA) offering, integrating with Spark, Presto, ClickHouse and others through a software layer.
- Its Presto-Velox talk mentions Iceberg and Parquet compatibility.
- Partners with Dell PowerEdge.
- Source: [prestodb.io NeuroBlade video](https://prestodb.io/videos/streamlining-data-analytics-with-neuroblades-spu-hw-acceleration/), [Grove VC](https://www.grovevc.com/news/neuroblade-redefines-cloud-analytics-with-purpose-built-data-acceleration/) (accessed 2026-10-29)

**AWS AQUA for Redshift**
- A "distributed and hardware-accelerated cache" near storage, GA in April 2021 on RA3. — [AWS what's new](https://aws.amazon.com/about-aws/whats-new/2021/04/aws-announces-general-availability-of-aqua-for-amazon-redshift/) (accessed 2026-10-29)
- Retirement is only a forum rumor, not confirmed. — [Fool forum](https://discussion.fool.com/t/aws-redshift-shutting-down-aqua/78217) (accessed 2026-10-29)
- (Prior knowledge, not fetched: AWS later folded AQUA in as an automatic, invisible feature rather than a product.)

**BlazingSQL**
- Its founders' work fed into Voltron/Theseus; the project is inactive (prior knowledge, not fetched).

### Inferences
- The "decode-and-cache service feeding any engine" niche is empty among GPU vendors. The closest precedents are AWS AQUA (a near-storage scan offload for one engine, which AWS ended up hiding inside Redshift) and the Speedata/NeuroBlade cards (in-engine plugins, not network services). Every successful accelerator has integrated as an in-process plugin with CPU fallback, not as a remote tier (estimate, based on the pattern above).
- Torin plus Velox-cuDF means the two largest Iceberg SQL engines in the Trino/Presto family will have vendor GPU paths by 2027. A third-party GPU tier for Trino would then compete with Starburst's own product.

### Gaps
- No 2026 status for Bodo, Ocient, Yellowbrick or Kinetica on Iceberg GPU scans (searches returned nothing usable).
- Torin's internals: whether it reuses Velox, uses JNI into libcudf, or runs a sidecar.

## Q4. Integration interfaces for a GPU scan/decode tier, and the cost of shipping decoded data

### Takeaway
For in-process integration, the realistic interface is the Arrow C Data/Stream interface (CPU) or the Arrow C Device Data Interface (GPU-resident, supported by libcudf). For a remote service it is Arrow Flight. Flight measured about 1 GB/s per stream and about 2 GB/s over InfiniBand in the 2022 benchmark, at or below what a CPU engine gets by fetching compressed Parquet itself. Since decoded columns are 3–10× larger than encoded ones, a remote decode tier sends 3–10× more bytes over the network.

### Cited Findings
**Arrow C Device Data Interface**
- Status: experimental.
- `ArrowDeviceArray` wraps an ArrowArray plus a device id, a device type (CPU, CUDA, CUDA_HOST pinned, ROCm, Metal, Vulkan, …) and a sync event.
- It is motivated explicitly by cuDF, to keep data on the device across library boundaries.
- Source: [Arrow C Device Data Interface](https://arrow.apache.org/docs/format/CDeviceDataInterface.html) (accessed 2026-10-29)
- libcudf can export `ArrowDeviceArray` views of GPU columns. Some types are copied on export: BOOL8 becomes a bitmap, and DECIMAL32/64 become decimal128. — [libcudf Interop Arrow 26.10](https://docs.nvidia.com/cudf/26.10/libcudf/api_docs/interop_arrow/) (accessed 2026-10-29)
- I found no evidence that DuckDB or DataFusion consume `ArrowDeviceArray` with non-CPU device types (search returned nothing).

**Arrow Flight throughput**
- Localhost DoGet/DoPut: about 1 GB/s on a single stream, up to about 10 GB/s with 16 parallel streams; more than 16 streams degrades.
- InfiniBand cluster: DoGet 1.5–2 GB/s.
- Headline peaks: 6000 MB/s DoGet and 4800 MB/s DoPut, using up to 95% of link bandwidth.
- Source: [Benchmarking Apache Arrow Flight, arXiv 2204.03032](https://arxiv.org/pdf/2204.03032) (search snippet, accessed 2026-10-29)

**Engine-side consumption surfaces (prior knowledge, not fetched)**
- Trino connector SPI is Java-only (ConnectorPageSource returns Trino `Page`/`Block`). A GPU service would need a Java connector that converts Arrow to Trino Pages, for example over Flight. Starburst built Torin inside the engine instead. — [Trino connector dev docs](https://trino.io/docs/current/develop/connectors.html)
- Spark has two surfaces:
  - DataSource V2 with `SupportsReportPartitioning` and columnar reads (`PartitionReader<ColumnarBatch>`), which accept Arrow-backed ColumnVectors.
  - The Spark plugin and columnar rule API used by RAPIDS. — [Spark DSv2 API](https://spark.apache.org/docs/latest/api/java/org/apache/spark/sql/connector/read/package-summary.html)
- DuckDB has table-function extensions, replacement scans, and `arrow_scan` / Arrow C Stream ingestion, zero-copy for most types. — [DuckDB Arrow docs](https://duckdb.org/docs/stable/guides/python/sql_on_arrow)
- DataFusion offers a custom `TableProvider` plus `ExecutionPlan` emitting Arrow `RecordBatch` streams. iceberg-rust ships an `iceberg-datafusion` integration. — [DataFusion custom table providers](https://datafusion.apache.org/library-user-guide/custom-table-providers.html), [apache/iceberg-rust](https://github.com/apache/iceberg-rust)

**Evidence on data movement**
- The IBM Presto GPU paper: moving data between GPU and CPU for fallback operators can dominate query time. — [arXiv 2606.24647](https://arxiv.org/html/2606.24647v1) (search snippet, accessed 2026-10-29)
- Sirius moves compressed data and decompresses on GPU for up to 2× transfer throughput. — [GTC recap](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-29)

### Inferences
- Bandwidth arithmetic (estimate):
  - PCIe Gen5 ×16 is about 50–60 GB/s per direction in practice. A 100 Gb NIC is about 12 GB/s, and a 400 Gb NIC about 50 GB/s.
  - A CPU engine reading Parquet at an encoded-to-decoded ratio of 1:5 consumes about 1 GB/s of compressed input per 5 GB/s of decoded output.
  - A remote GPU decode tier must therefore push about 5× more bytes over the network than the engine would pull from S3 or a cache.
  - At Flight's measured 1–2 GB/s per stream over a real network, the service becomes the bottleneck unless it filters or aggregates heavily (pushdown) before shipping.
- Implication: a "decode service for any engine" only pays off if it returns reduced data (filtered, projected, pre-aggregated, or late-materialized) or co-locates in-process with the GPU engine. Plain decode-and-ship is network-bound.
- Realistic interfaces by deployment (estimate):
  - Same host: Arrow C Stream / C Device into DuckDB or DataFusion.
  - Remote: Flight (or Flight SQL with pushdown) behind a Trino Java connector or a Spark DSv2 source.
  - Trino is the hardest target because of Java-only page conversion.

### Gaps
- No current (2025–2026) Flight benchmarks on 100–400 GbE.
- I did not verify DuckDB or DataFusion handling of `ArrowDeviceArray`.
- No source quantifies the encoded-vs-decoded size ratio. The 3–10× figure comes from the task brief and is unverified here.

## Q5. Non-GPU scan acceleration and caching layers (competitors in shape)

### Takeaway
Caching tiers are mature and largely bundled into engines or vendors: Starburst Warp Speed and its Alluxio-based filesystem cache, Trino's filesystem cache, the Databricks disk cache, Dremio C3/Reflections. Standalone caching (Alluxio) gets absorbed rather than bought separately, as Starburst's deprecation and later re-adoption of Alluxio shows. Hardware scan offload (AQUA, Speedata, NeuroBlade) survives only as engine-integrated or engine-invisible features.

### Cited Findings
- Starburst Warp Speed adds a transparent indexing and caching layer for the Hive, Iceberg and Delta connectors. It requires EKS, AKS or GKE. — [Starburst Warp Speed docs 462-e](https://dell.starburst.io/462-e/data-engineering/cost-and-performance/set-mode-ws.html) (accessed 2026-10-29)
- Starburst first deprecated RubiX/Alluxio caching because of supportability problems, then from 443-e brought back file system caching "powered by Alluxio". — [Starburst blog](https://starburst.io/blog/sep-reviving-advanced-file-caching-file-system-caching) (accessed 2026-10-29)
- AWS AQUA was a hardware-accelerated near-storage cache for Redshift (2021). Its current status is unconfirmed. — [AWS](https://aws.amazon.com/about-aws/whats-new/2021/04/aws-announces-general-availability-of-aqua-for-amazon-redshift/) (accessed 2026-10-29)
- Velox's async data cache with read coalescing cut S3 requests from about 700 to about 200 in Presto GPU. The cache is built into the engine. — [GTC recap](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-29)
- Prior knowledge, not fetched: Trino OSS has had a native filesystem cache (Alluxio library-based) since around v439; the Databricks disk cache stores decoded-ish data on local NVMe; Dremio has C3 columnar cloud cache and Reflections.

### Inferences
- The worked/failed record:
  - Engine-integrated caches and offloads work: Velox cache, Databricks disk cache, Warp Speed, RAPIDS plugin.
  - Separate-tier products struggle or get folded in: Alluxio deprecated then re-embedded by Starburst, AQUA made invisible, BlazingSQL dead, Voltron's commercial status unclear.
  - This argues for shipping as an engine plugin (DuckDB extension, DataFusion TableProvider, Spark plugin) rather than a network service (estimate).

### Gaps
- No confirmed failure post-mortem of a GPU-to-CPU-engine offload product.
- No 2026 Alluxio product status or pricing collected.
- Databricks/Dremio cache facts were not re-verified this session.
