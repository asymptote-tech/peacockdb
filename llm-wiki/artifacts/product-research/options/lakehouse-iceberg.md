# lakehouse-iceberg (+ gpu-scan-tier)

## Verdict
- What: a GPU query engine that reads Iceberg tables (later Delta/Hudi/Paimon) in object storage, with a tiered NVMe/RAM/VRAM cache. The "decode service that feeds any engine" variant does not hold up.
- Who: no lakehouse user asks for a GPU. Cost pain on Trino/Iceberg is real, but the users fix it with compaction and caching. demand.md rates this option weak [2].
- Budget: an alpha fits, at about 0.6–1.1× the engine on peacockdb (estimate). But cold scans are NIC-bound, and NVIDIA, Starburst, IBM, Sirius and Bodo already ship this.

## Who needs it badly
- demand.md rating: **weak**. "R/U/C 3/2/5 · 5 posters … Remote reads cap the GPU, and the askers are vendors" [2].
- The only real GPU-on-Iceberg ask comes from vendors:
  - Aaron Spiegel (Qbeast, field CTO) and Chen Junjie (Tencent) want file skipping on large append-only Iceberg tables in S3, in Sirius ([sirius #1760](https://github.com/sirius-db/sirius/issues/1760), 2026-09) [46].
  - The Paimon PMC wants Paimon tables in Sirius's GPU scan ([paimon #9923](https://github.com/apache/paimon/issues/9923), 2026-09-17) [49].
- IO-bound GPU users:
  - HDFS held 8×H100 at 15% ([cudf-spark #11431](https://github.com/NVIDIA/cudf-spark/discussions/11431), 2024-09) [47].
  - u/fmoralesh: 11 TB of S3 Parquet with a nested-JSON column. 5 TB took 6.5 h, and dask_cudf was "not practical" (2025-06-12) [48].
- Lakehouse users with cost pain but no GPU ask:
  - Vantage (2024-03-21): S3 GET charges sometimes exceed storage. Insider cut S3 cost 90% by moving to Iceberg. Junglee Games cut request cost 70% with Warp Speed caching [50].
  - Dune Analytics (Trino Fest 2024): object-store API calls are expensive, and most queries reread the same subset [51].
  - Branch (Trino Summit 2024): moved one unit to Trino + Iceberg for "7-figure savings" a year [52].
  - Netflix: over 1 EB of Iceberg and 50K Trino vCPUs. It builds in-house; tables with over 100M files needed a distributed metadata scan [53].
- Decode is expensive, but only academia says so: decoding is 46% of DuckDB TPC-H runtime on Parquet. The proposed fix is a SmartNIC, not a GPU (ETH, DaMoN '26) [54].
- Best first conversations (estimate):
  - Spiegel (Qbeast): he is building TPC-H Iceberg on S3 to test GPU pruning.
  - Carl Boettiger: H3 joins over Parquet on MinIO, with a public GPU-vs-DuckDB record [59].
  - Yu-Ting Lin: Superset over a 9.6B-row Delta table on RAPIDS [61].
  - Branch and Dune, as cost-driven Trino shops. Neither has asked for GPUs.

## How it would work
- **Planner (Rust).** iceberg-rust loads the catalog (REST first), snapshot, manifest list and manifests. It prunes by partition summaries, then by per-file column bounds and null counts [31][32]. The output is a list of (file, row-group byte ranges, position-delete bitmap or DV, equality-delete files). DataFusion 45 plans the SQL. peacockdb's existing row-group mapping assigns (lane, batch) [1].
- **I/O (C++).** Many concurrent, coalesced range GETs (KvikIO `MULTI_POLL`, up to 256 in flight [14]) land in pinned host buffers. The reads go through an NVMe page cache keyed by (file path, range).
- **Decode (GPU).** libcudf `hybrid_scan_reader` reads filter columns first, then the payload columns that survive the row mask. It prunes with page indexes and bloom filters, and the caller owns the I/O [29].
- **Deletes (GPU).**
  - Position deletes and v3 DVs become a row mask (libcudf's experimental DV reader [30]; cuDF itself calls deletes "a big challenge" [60]).
  - Equality deletes become a hash anti-join on the existing join session. Velox-cuDF does the same with `distinct_hash_join` [37].
- **Execute.** Run the existing node set. Hot decoded columns stay pinned in VRAM across queries.
- **Front door.** Flight SQL. "Feeding any engine" means a connector that pushes a whole subquery down to this engine and receives the reduced result. It does not mean shipping decoded columns.
- **End-to-end data path:** S3 → NIC → kernel TCP → pinned host → PCIe → HBM decode → execute. On a warm hit: NVMe or host RAM → PCIe → HBM, or VRAM directly.

## The human's questions
**Q: How should data be ingested from the lake properly?** The sub-questions are answered in turn below.

- **Object-store throughput per node and per GPU.**
  - No cloud publishes a per-client GB/s figure. The limits are request rates and the general-purpose NIC [7][9].
    - S3: 5,500 GETs per second per prefix [7].
    - EC2: one TCP flow is capped at 5 Gbps (10 Gbps in a placement group) [8].
    - Azure: 200 Gbps egress per account in major regions [12].
  - Measured, with many parallel range reads:
    - KvikIO reached about 10 Gbps on g4dn.xlarge (64–128 threads, 16 MiB tasks), against about 2 Gbps for boto3 [13].
    - Dask-cuDF reading Parquet reached about 20 Gbps on g4dn.12xlarge, against about 5 Gbps for boto3 [13].
    - Mountpoint reached about 4.85 GB/s on c6i.16xlarge [15].
  - Without parallelism you get one flow, at most 5 Gbps. At a typical 50–100 MB/s per GET, filling 100 Gbps needs about 125–250 GETs in flight (estimate).
  - The NIC that storage traffic can actually use:
    - g6/g5.48xlarge: 100 Gbps. g6e.48xlarge: 400 Gbps. g6.4xlarge: 10/25 Gbps [9].
    - GCP A3 Ultra and A4: storage uses the 400 Gbps gVNIC. The 3,200 Gbps of CX-7 is "dedicated for only … GPU to GPU" traffic [10].
    - AWS p5/p6 list 3,200 Gbps of EFA and publish no figure for the ENA (TCP) path that reaches S3 [9].
    - Azure ND H100 v5 lists 80 Gbps frontend in one table and 2.4 Gbps in another [11].
  - Per GPU on an 8-GPU node, that is about 1.5–6 GB/s of compressed input per GPU (estimate).
- **Local NVMe caching.**
  - p5, p5en and p6 each have 8×3.8 TB of NVMe, rated at 4.4M random-read IOPS [9]. That is about 18–50 GB/s aggregate (estimate; AWS publishes no sequential figure). One node's NVMe is 1.5–4× its usable NIC.
  - Iceberg data files are immutable, so a cache keyed by file path never needs invalidation. Only snapshot expiry evicts (estimate).
  - Trino's filesystem cache already does this on CPU: raw file bytes in 1 MB pages on local SSD [44].
- **GPUDirect Storage and KvikIO.**
  - GDS helps on local NVMe, mostly at small I/O sizes. SNIA found the gain "negligible at large IOs" [17].
  - FSx for Lustre with EFA and GDS reaches up to 1,200 Gbps per p5 client [18].
  - S3, GCS and Azure Blob have no GDS path:
    - KvikIO's remote reader always bounces through a 16 MiB pinned host buffer and makes no cuFile calls (`remote_handle.cpp` @bb8639d7) [14].
    - cuObject (S3 over RDMA) needs RDMA-capable S3 servers, which no public cloud offers [19].
  - KvikIO defaults are wrong for S3: 1 thread and 4 MiB tasks [14]. cuDF sets 4 threads in compat (POSIX) mode [27].
- **Parquet decode on GPU (cuDF).** peacockdb's own numbers:
  - Load runs at 6.8–8.7 GB/s on H200.
  - The q6 load is about 20× its execute time.
  - Snappy is 38% of kernel time on q11v [1][6].
- **Known slow paths in cuDF:**
  - Many types decode at under 50 GB/s on H100, against a 100 GB/s target. Snappy takes longer than decode for decimal and timestamp columns (cudf #18350) [24].
  - Footer metadata used to be re-parsed per reader: 8 s on SF100 lineitem. Readers can reuse it since PR #20693 (2025-12) [23].
  - Files with offset indexes but no column indexes make reader setup read almost the whole file. That bug was reported on GB10, has existed since 24.04, and was fixed 2026-10-09 (cudf #24000, commit 4f4de11) [22].
  - Presto on GPU found the cuDF 25.04 reader "10× slower than the theoretical I/O rate" [26].
  - GPU-aware files reach up to 125 GB/s on A100: about 100 pages per chunk, row groups of millions of rows, and no compression where it saves under 10% [25].
  - On Blackwell, the hardware decompression engine (up to 600 GB/s) is silently skipped for buffers from `cudaMallocAsync` or pinned host memory [28].
- **Ticket #232 (read_parquet 5.6× slower than a one-chunk chunked reader on GB10).**
  - At cudf @785ee522 both paths run the same `reader_impl`. Only the `read_mode` flag differs (READ_ALL vs CHUNKED_READ) [21].
  - No public issue explains the gap. The likeliest public suspects:
    - #24000's over-read, if the test files have offset-only page indexes [22];
    - per-call footer parsing [23];
    - the KvikIO thread default [27].
  - The fix belongs in the alpha anyway: move to the chunked or hybrid-scan reader, plus pinned buffers (#149).
- **Iceberg metadata pruning.**
  - The stages [31]:
    1. The manifest list's partition summaries skip whole manifests.
    2. Manifest entries skip files by partition tuple and by per-column lower/upper bounds and null/NaN counts.
    3. Row-group statistics, page indexes and bloom filters skip inside a file.
  - Delete scoping [31]:
    - v2 position deletes apply to data files with a sequence number ≤ the delete file's.
    - v2 equality deletes apply to data files with a strictly lower sequence number, in the same partition.
    - v3 replaces position-delete files with one Puffin DV per data file.
  - Library status:
    - iceberg-rust main applies position deletes, equality deletes and v3 DVs, but its latest release is 0.10.1, and 0.11 failed its vote twice [32][33][35].
    - iceberg-cpp 0.4.0 has v2 delete loaders and DV blobs, and is pre-1.0 [34].
  - No public study measures how many bytes pruning saves on real tables. It depends on sort order (gap).
  - Planning stays on CPU everywhere, so tables with many small files hit an Amdahl ceiling on the CPU side (Netflix [53], Bodo [41]).
- **Which cache layer makes the GPU pay off.** Worked example from peacockdb's q6 at sf40: 1.5 GB of compressed columns [6], 19 ms execute on H200, against 137 ms for DuckDB warm [1].
  - From S3 at 12.5 GB/s (100 Gbps) it is about 120 ms; at 3 GB/s (g6.4xlarge) about 500 ms. Cold, the GPU loses or ties (estimate).
  - From NVMe at about 25 GB/s it is about 60 ms; from pinned host RAM at 47 GB/s, about 32 ms, plus decode (estimate).
  - Decode at today's 6.8–8.7 GB/s costs about 170–220 ms of decoded output alone. At a fixed 50 GB/s it would be about 30 ms (estimate, assuming ~3× expansion).
  - So:
    - **VRAM holds decoded hot columns.** Only this tier beats warm DuckDB by 5–10×.
    - **Host RAM holds lightly compressed, GPU-friendly columns** (bitpack, delta, dictionary), as in Sirius's compressed pins: 264 GB → 93 GB for the 7 hottest lineitem columns [4]. That avoids both Parquet's slow decode and 3× PCIe bytes.
    - **NVMe holds encoded Parquet pages** as a capacity tier.
  - Decoded in host RAM beats encoded while decode stays under about 15 GB/s. Once the reader is fixed, the two roughly tie (estimate).
- **Which engines would consume a GPU scan tier, and through what interface.**
  - Trino's connector SPI is Java-only, so it would need a Java connector converting Arrow to Pages over Flight [62]. Starburst built Torin inside Trino instead [39].
  - Spark: DSv2 columnar reads, but NVIDIA's cudf-spark already holds that slot [36].
  - DuckDB: an extension or Arrow C Stream scan, but Sirius holds that slot [38].
  - DataFusion: a `TableProvider` emitting Arrow, next to iceberg-datafusion [63].
  - The Arrow C Device Data Interface could keep data on the GPU in-process. It is experimental, and no evidence was found that DuckDB or DataFusion accept CUDA-resident arrays [42].
  - Remotely, Flight measured about 1 GB/s per stream, about 10 GB/s with 16 streams on localhost, and 1.5–2 GB/s DoGet over InfiniBand [43].
  - Decoded columns are about 3–5× the encoded bytes (estimate). A "decode and ship to a CPU engine" tier therefore moves more bytes than the engine would pull from S3 itself. It pays only with filter, projection or aggregation pushdown, which makes it a query engine again.
  - Every successful offload has been in-process: AQUA folded into Redshift, Speedata and NeuroBlade ship as engine plugins, and Alluxio was deprecated and then re-embedded by Starburst [3][40].
- **Do Sirius or NVIDIA already cover it? Largely yes.**
  - Sirius merged GPU Iceberg scans with v2 position deletes and v3 DVs on 2026-09-15 (PR #1425). Equality deletes fall back to DuckDB [38]. It already has S3, KvikIO, io_uring and a pin cache (44,678 lines of scan, I/O and cache) [4].
  - cudf-spark reads Iceberg v1/v2 on GPU, with position and equality deletes. It has no DV reads and no nested types, and planning stays on CPU [36]. It also has a PerfIO S3 reader [58].
  - Velox-cuDF has an Iceberg connector with GPU DV and equality-delete readers [37]. Presto GPU nightlies have shipped since 2026-06, and IBM watsonx.data runs it in production [3].
  - A GTC 2026 talk (S81563) found Parquet scan and I/O at 60–70% of Presto GPU runtime, with per-VM S3 throughput as a limit [40].
  - Starburst Torin (cuDF inside Trino) reached limited GA on 2026-10-21 [39]. Bodo shipped a GPU-native Iceberg reader on 2026-06-04 [41].
  - The gaps left are equality deletes in Sirius, DV reads and nested types in cudf-spark, and open-source Trino. Each is a few months of vendor work (estimate).

## Data path and cache
| Hop | Bandwidth | Source |
|---|---|---|
| S3 per flow | ≤ 5 Gbps (0.6 GB/s) | [8] |
| S3 per node, measured | 1.25–2.5 GB/s (g4dn, KvikIO); 4.85 GB/s (Mountpoint, c6i) | [13][15] |
| Storage NIC per node | 3 GB/s (g6.4xl) · 12.5 (g6.48xl) · 50 (g6e.48xl, GCP A3U/A4) | [9][10] |
| Local NVMe, 8 drives | ~18–50 GB/s (estimate) | [9] |
| PCIe H2D, H200 measured | 10.6 GB/s pageable, 47.3 GB/s pinned | [1] |
| cuDF Parquet decode | 6.8–8.7 GB/s (peacockdb, H200); <50 GB/s per type (H100); ≤125 GB/s with tuned files (A100) | [1][24][25] |
| HBM read | 4,484 GB/s measured (H200); 3.35 TB/s (H100 spec) | [1][20] |

- **The thesis holds for this option, and more strongly than for any local-data option.** Cold reads from object storage are 1–2 orders of magnitude below decode and 2–3 below HBM.
  - The human's PoC already loaded longer than it computed on 11 of 12 SF20 queries, from local files [5].
  - Theseus cut a cold S3 TPC-H SF10k run from 3,204 s to 531 s by changing only the S3 datasource and prefetching [16].
- **Which cache is needed:**
  - VRAM for decoded hot columns;
  - pinned host RAM for GPU-friendly compressed columns;
  - local NVMe for encoded pages.
  - Plus coalesced, prefetched range reads so that cold misses run at NIC speed.
- **Without a cache, the product is an S3 client with a GPU attached.** With one, it is a warm-data engine. Then the value depends on the user rereading the same columns, which Dune's talk says is common [51].
- **The decode fix comes first.** At 7–9 GB/s, decode is slower than a 100 Gbps NIC, so even NVMe hits are decode-bound until #232 and the Snappy cost are fixed [6][24].

## Build cost to alpha
| Component | On peacockdb: reuse/adapt/new, size | On Sirius: reuse/adapt/new, size |
|---|---|---|
| SQL front end, operators, driver | reuse (planner, nodes, executor, join session) | reuse (DuckDB + Sirius operators) |
| Object-store I/O (S3 first; coalesce, prefetch, many in flight) | new, 1.5–2.5k | reuse (`src/io`, 10.8k) + adapt cloud tuning, 0.5–1k |
| Parquet reader: chunked or hybrid scan, pinned buffers, multi-file tables (#232, #149) | adapt, 0.5–1.5k [4] | reuse |
| Iceberg catalog, manifests, partition and stats pruning | new via iceberg-rust in the planner, 1.5–3k | reuse (DuckDB iceberg + Sirius scan) |
| Deletes: position and DV mask; equality anti-join | new, 1–2k | adapt: equality deletes on GPU, 0.5–1k |
| Cache: VRAM decoded + host compressed + NVMe pages, automatic eviction | new, 2–4k [4] | adapt: make `pin_table` automatic and add an NVMe tier, 2–3k |
| Out-of-core (data > VRAM) | adapt cuCascade, minimal spill, 3–6k [4] | reuse (cuCascade) |
| Whole-query CPU fallback | new, 0.3–0.8k [4] | reuse |
| Server (Flight SQL) + GPU entry point | new, 1–2k | new around DuckDB, 1–2k |
| One consumer connector (Trino Java over Flight, with subquery pushdown) | new, 1.5–3k | new, 1.5–3k |
| Delta via delta-kernel-rs (optional for alpha) | new, 0.5–1k | reuse (DuckDB delta) |
| **Sum** | **12.8–25.8k** | **6–11k** |

Total new work: 0.6–1.1× engine on peacockdb, 0.3–0.5× on Sirius (estimate)

- **The hard part on peacockdb:** the cache plus out-of-core work. Lake tables exceed VRAM, and today any build or sort that does not fit fails the query [1]. Cloud I/O tuning is second: the 5 Gbps per-flow cap and the S3 request limits [7][8].
- **The hard part on Sirius:** none of the lines. It is owning a product on a 154k-line base with no releases, 92% NVIDIA commits, and a feature set NVIDIA is already shipping itself [4]. The Sirius base is cheaper because Sirius already is most of this product.

## Adoption friction
- **For:**
  - Iceberg is open, so the engine reads the user's tables with no migration and no copy.
  - It can run beside Trino or Spark on the same catalog and take only the hot dashboards.
- **Against:**
  - It is a new engine to operate, with GPU nodes, NVMe and a cache to size.
  - Trino users can't plug it in without a Java connector, and Starburst customers will get Torin from their vendor [39].
  - Spark users already have cudf-spark on every managed Spark [3].
  - DuckDB users have Sirius [38].
  - The only empty slot is open-source Trino and DataFusion users who want GPU without a vendor. Nobody in the evidence asks for that [2].
- **History:** separate scan or cache tiers get folded into engines (AQUA, Alluxio at Starburst, BlazingSQL) [40].

## Platform risk
- nvidia.md rates the platform risk for lakehouse-iceberg **strong**, the worst level: "NVIDIA's own cudf-spark already reads Iceberg and Delta. IBM Presto/Velox-cuDF is in production. Starburst has Trino-cuDF in development. libcudf deletion-vector readers lower the bar for everyone" [3].
- Since then:
  - Torin reached limited GA (2026-10-21) [39].
  - Sirius gained GPU Iceberg with DVs (2026-09-15) [38].
  - Bodo shipped a GPU Iceberg reader (2026-06) [41].
- NVIDIA owns the reader (libcudf hybrid scan and DV reader), the I/O (KvikIO, cuObject) and the spill tier (cuCascade) [29][30][14][4]. Every fix to the slow paths above lands for all competitors at once.
- nvidia.md says the safe ground is "catalog and lakehouse integration, multi-tenant serving, CPU/GPU routing" [3]. That is integration work on NVIDIA parts, not an engine moat.

## Funding story
- **What a design partner would say** (estimate):
  - A Trino/Iceberg shop like Branch or Dune would say: "Cut our per-query cost by half on the dashboards we rerun, without moving data."
  - They would also ask why not Starburst Torin, or simply compaction and Warp Speed caching, which already cut S3 request cost 70–90% [50].
- **What the round would be raised on:**
  - "The GPU lakehouse engine for open-source Trino/Iceberg users, with a warm cache that makes GPUs pay on S3."
  - The proof would be one partner's $/query on their own Iceberg tables, against Trino with caching.
- **Weakness:** no named user is in pain for this today [2]. Every incumbent and NVIDIA partner is pitching the same thing. The AWS EKS benchmark shows the GPU wins on $/job ($6.27 vs $31.28), not on speed, and the CPU engine Gluten was fastest [55]. A cost story is easier to copy than a speed story.

## Evidence against
- **Demand is weak and vendor-driven.**
  - The askers are Qbeast, Tencent and the Paimon PMC [2][46][49].
  - Lakehouse users name S3 GETs, small files and metadata as their pain, and fix them without compute [50][51].
  - Capgemini chose CPU Photon over GPU RAPIDS because GPU saved only 6% [56].
  - Starburst's day-one NVIDIA support was for the Vera CPU, not GPUs [57].
- **Physics.**
  - Cold reads are NIC-bound by 1–2 orders of magnitude [9][10][13].
  - Planning and metadata stay on the CPU [41][53].
  - The GPU helps only on warm, repeated, scan-heavy queries. That is the "lean" cache option with an S3 reader added.
- **Crowded.** Five funded or NVIDIA-staffed efforts ship GPU Iceberg scans today: cudf-spark, Velox/Presto, Sirius, Torin and Bodo [36][37][38][39][41].
- **The "scan tier for any engine" variant fails on arithmetic.** Shipping decoded columns costs 3–5× the bytes over a 1–2 GB/s Flight stream [43]. Nobody sells this tier, and the closest precedents were absorbed [40].
- **peacockdb is furthest behind here.** It has no object store, no Iceberg, no cache, no spill and no multi-GPU [1]. Sirius has all of these [4].

## Ratings
- Who needs it badly: **weak** — demand.md rates it weak: 5 posters, mostly vendors; users fix cost with layout and caching [2].
- Build cost: **medium** — 0.6–1.1× on peacockdb fits the budget, but only at its upper half, and spill is unavoidable.
- Data path: **weak** — cold S3 is 1–2 orders below decode. It works only with a three-tier cache and a fixed reader.
- Adoption friction: **medium** — it reads existing Iceberg tables with no migration, but it is a new engine to run, and Trino plug-in is Java-hard.
- Platform risk: **weak** — nvidia.md rates the risk strong; Torin, Velox-cuDF, cudf-spark, Sirius and Bodo already ship it [3].
- Funding story: **weak** — no partner in pain; the pitch is $/query, which incumbents can match.

## Follow-up experiments
1. **S3 → GPU on cloud.** Cold TPC-H SF100 Iceberg on g6e.48xlarge or p5: KvikIO `MULTI_POLL` against Rust `object_store`, at 16–256 GETs in flight. Measure GB/s per node and per GPU.
2. **#232 root cause.** Check whether the sf40 files have offset indexes without column indexes (#24000). Profile `read_parquet`, the one-chunk chunked reader and `hybrid_scan` on GB10 and H200 with cudf 26.10.
3. **Which host tier.** q1 and q6 at sf40 from pinned host RAM, comparing encoded Parquet, decoded columns and Sirius-style light compression.
4. **Pruning yield.** Bytes read after manifest, stats and page-index pruning on a sorted against an unsorted Iceberg table, using the design partner's query log if possible.
5. **Sirius on cold Iceberg in S3.** Time cold, warm-host and pinned-GPU runs against DuckDB CPU. If Sirius is close, the build-on-Sirius cost line holds.
6. **Flight to Trino.** GB/s of decoded columns into a Trino page source on 100 GbE, against Trino reading Parquet itself.
7. **Reread rate.** From a partner's Trino logs, the share of scanned bytes reread within 24 h. This sets the cache hit rate the product needs.

## Sources
1. peacockdb inventory at master `f25c250d`, `llm-wiki/artifacts/product-research/inventory.md` (local; accessed 2026-10-29)
2. `llm-wiki/artifacts/product-research/demand.md`, `## Ratings` and `## First conversations` (local; accessed 2026-10-29)
3. `llm-wiki/artifacts/product-research/nvidia.md`, "Per direction", "Partners and ecosystem" (local; accessed 2026-10-29)
4. `llm-wiki/artifacts/product-research/sirius.md`, Sirius @ `b9e05ffc653c8ed031c789e5c1415ac1186e0619` (local; accessed 2026-10-29)
5. `llm-wiki/artifacts/product-research/evidence-in-hand.md` (local; accessed 2026-10-29)
6. `llm-wiki/tickets/performance.md` #232, #150, #149 (local; accessed 2026-10-29)
7. https://docs.aws.amazon.com/AmazonS3/latest/userguide/optimizing-performance-design-patterns.html (accessed 2026-10-29)
8. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/ec2-instance-network-bandwidth.html (accessed 2026-10-29)
9. https://docs.aws.amazon.com/ec2/latest/instancetypes/ac.html (accessed 2026-10-29)
10. https://docs.cloud.google.com/compute/docs/gpus/gpu-network-bandwidth (accessed 2026-10-29)
11. https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/gpu-accelerated/ndh100v5-series (accessed 2026-10-29)
12. https://learn.microsoft.com/azure/storage/common/scalability-targets-standard-account (accessed 2026-10-29)
13. https://developer.nvidia.com/blog/high-performance-remote-io-with-nvidia-kvikio (accessed 2026-10-29)
14. https://github.com/rapidsai/kvikio/tree/bb8639d71bd11d336f36a6b1dde76bb978bc36f3 — `cpp/src/remote_handle.cpp`, `cpp/src/defaults.cpp` (accessed 2026-10-29)
15. https://aws.amazon.com/blogs/hpc/improve-the-speed-and-cost-of-hpc-deployment-with-mountpoint-for-amazon-s3 (search snippet; accessed 2026-10-29)
16. https://arxiv.org/html/2508.05029v1 (Theseus; accessed 2026-10-29)
17. https://www.snia.org/sites/default/files/SDCEMEA/2021/snia-analyzing-effects-of-GPU-direct-storage.pdf (accessed 2026-10-29)
18. https://aws.amazon.com/about-aws/whats-new/2024/11/amazon-fsx-lustre-elastic-fabric-adapter-nvidia-gpudirect-storage/ (accessed 2026-10-29)
19. https://docs.nvidia.com/gpudirect-storage/cuobject (accessed 2026-10-29)
20. https://www.nvidia.com/en-us/data-center/h100/ (accessed 2026-10-29)
21. https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/src/io/parquet/reader_impl.cpp (accessed 2026-10-29)
22. https://github.com/NVIDIA/cudf/issues/24000 (fix PR #24001, commit 4f4de11; accessed 2026-10-29)
23. https://github.com/NVIDIA/cudf/issues/18890 and https://github.com/NVIDIA/cudf/pull/20693 (accessed 2026-10-29)
24. https://github.com/NVIDIA/cudf/issues/18350 (accessed 2026-10-29)
25. https://arxiv.org/html/2602.17335v1 ("Do GPUs Really Need New Tabular File Formats?"; accessed 2026-10-29)
26. https://arxiv.org/html/2606.24647 ("Accelerating Presto with GPUs"; accessed 2026-10-29)
27. https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/src/io/utilities/config_utils.cpp (accessed 2026-10-29)
28. https://docs.nvidia.com/cuda/nvcomp/decompression_engine_faq.html (accessed 2026-10-29)
29. https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/io/experimental/hybrid_scan.hpp (accessed 2026-10-29)
30. https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/io/experimental/deletion_vectors.hpp (accessed 2026-10-29)
31. https://iceberg.apache.org/spec/ (accessed 2026-10-29)
32. https://github.com/apache/iceberg-rust/blob/beba39c8d62810fa101740bb2dbb03ab0d8f07ac/crates/iceberg/src/arrow/caching_delete_file_loader.rs (accessed 2026-10-29)
33. https://github.com/apache/iceberg-rust/releases (accessed 2026-10-29)
34. https://iceberg.apache.org/blog/apache-iceberg-cpp-0.4.0-release/ (accessed 2026-10-29)
35. https://www.dremio.com/blog/preview-of-upcoming-apache-iceberg-1-12-and-iceberg-rust-0-11-new-features-and-breaking-changes/ (accessed 2026-10-29)
36. https://docs.nvidia.com/spark-rapids/user-guide/latest/additional-functionality/iceberg-support.html (accessed 2026-10-29)
37. https://github.com/facebookincubator/velox/tree/bcdab35c839ef3dd4af8b75d5b38ae84b9c51fa0/velox/experimental/cudf/connectors/hive/iceberg (accessed 2026-10-29)
38. https://github.com/sirius-db/sirius/pull/1425 (search snippet; accessed 2026-10-29)
39. https://www.starburst.io/press-releases/starburst-introduces-torin-gpu-native-analytics-engine-built-on-nvidia-cudf/ (accessed 2026-10-29)
40. https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html (third-party GTC recap); https://aws.amazon.com/about-aws/whats-new/2021/04/aws-announces-general-availability-of-aqua-for-amazon-redshift/ ; https://starburst.io/blog/sep-reviving-advanced-file-caching-file-system-caching (accessed 2026-10-29)
41. https://www.bodo.ai/blog/building-a-native-gpu-iceberg-reader-for-apache-iceberg (read via summary; accessed 2026-10-29)
42. https://arrow.apache.org/docs/format/CDeviceDataInterface.html (accessed 2026-10-29)
43. https://arxiv.org/pdf/2204.03032 (Arrow Flight benchmark; search snippet; accessed 2026-10-29)
44. https://trino.io/docs/current/object-storage/file-system-cache.html (accessed 2026-10-29)
45. https://learn.microsoft.com/azure/databricks/optimizations/disk-cache (accessed 2026-10-29)
46. https://github.com/sirius-db/sirius/issues/1760 (accessed 2026-10-29)
47. https://github.com/NVIDIA/cudf-spark/discussions/11431 (accessed 2026-10-29)
48. https://www.reddit.com/r/dataengineering/comments/1la1lcw/ (accessed 2026-10-29)
49. https://github.com/apache/paimon/issues/9923 (accessed 2026-10-29)
50. https://vantage.sh/blog/s3-bill-increase-athena-trino-hive-fix-iceberg-caching (accessed 2026-10-29)
51. https://trino.io/assets/blog/trino-fest-2024/dune.pdf (search summary; accessed 2026-10-29)
52. https://trino.io/assets/blog/trino-summit-2024/trino-summit-2024-branch.pdf (accessed 2026-10-29)
53. https://trino.io/assets/blog/trino-summit-2024/trino-summit-2024-netflix.pdf (accessed 2026-10-29)
54. https://arxiv.org/abs/2602.18775 (accessed 2026-10-29)
55. https://awslabs.github.io/data-on-eks/docs/benchmarks/spark-rapids-gpu-benchmark (accessed 2026-10-29)
56. https://developer.nvidia.com/blog/gpus-for-etl-run-faster-less-costly-workloads-with-nvidia-rapids-accelerator-for-apache-spark-and-databricks/ (accessed 2026-10-29)
57. https://www.businesswire.com/news/home/20260316162054/en/ (search summary; accessed 2026-10-29)
58. https://docs.nvidia.com/spark-rapids/user-guide/latest/additional-functionality/perfio-s3.html (accessed 2026-10-29)
59. https://github.com/boettiger-lab/mcp-data-server/issues/227 (accessed 2026-10-29)
60. https://github.com/NVIDIA/cudf/issues/21732 ("read deletes is a big challenge for cudf"; accessed 2026-10-29)
61. https://github.com/NVIDIA/cudf-spark/issues/11064 (accessed 2026-10-29)
62. https://trino.io/docs/current/develop/connectors.html (not re-fetched; accessed 2026-10-29)
63. https://datafusion.apache.org/library-user-guide/custom-table-providers.html (not re-fetched; accessed 2026-10-29)
