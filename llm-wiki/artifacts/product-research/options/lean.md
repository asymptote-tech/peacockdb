# lean — a multi-GPU RAM/VRAM cache, as a plugin in a database people already run

## Verdict

- **What it is:** a cache that keeps hot columns decoded or lightly compressed in VRAM across 4–8 GPUs on one host, with pinned host RAM behind it. It answers ~100 GB queries in a few hundred ms and ships as a plugin to a host database (DuckDB first), so nothing is ingested.
- **Who needs it:** demand.md rates lean **medium**. About 6 posters want hot data kept on the GPU between queries. None states a 100 GB / sub-second target [D].
- **Budget:** fits on Sirius (~0.3×) and also fits on peacockdb (~1.1×). On peacockdb, though, most of that 1.1× rebuilds what Sirius already ships, and on either base NVIDIA's free Sirius `pin_table` already covers the core [S][N].

## Who needs it badly

- **Rating: medium, taken from demand.md `## Ratings`, row `lean`** [D]. "The cache thesis matches the human's own SF20 data, but users ask for it softly."
- **Yu-Ting Lin** (`LIN-Yu-Ting`) serves Superset through Spark Thrift Server and RAPIDS over a 9.6B-row Delta table. He is blocked by DISTINCT OOMs and session bugs: [cudf-spark #11064](https://github.com/NVIDIA/cudf-spark/issues/11064) (2024-06) and [#9966](https://github.com/NVIDIA/cudf-spark/discussions/9966). This is the closest match to lean's shape [D].
- **Andy Gong** (`andygongyb`) asked to cache inputs in GPU memory across consecutive Spark queries, and the answer was no: [cudf-spark #13435](https://github.com/NVIDIA/cudf-spark/discussions/13435) (2025-09) [D].
- **`anirudh-here-com`** runs production HeavyDB 6.4 with custom patches and needs GPU-memory result hand-off: [heavydb #819](https://github.com/heavyai/heavydb/issues/819) (2023-12) [D].
- **`jieguolove`** has 5 servers × 4 L20 and wants an open GPU cluster database: [heavydb #842](https://github.com/heavyai/heavydb/issues/842) (2024-12) [D].
- **Louie Meyerov, Graphistry**, says "there's no supported way to keep the result of a query on the GPU" ([cudf #23039](https://github.com/NVIDIA/cudf/issues/23039), 2026-06). His customers run log analytics on 8×A100 ([HN](https://news.ycombinator.com/item?id=47109102)) [D].
- **The human's own data:** in the SF20 study, the PoC spent longer loading than computing on 11 of 12 queries [E].
- **Missing:** nobody asks for "100 GB in a few hundred ms" [D]. No poster attaches a budget or a deadline.
- **Best first conversations:** Lin, then Gong, then Meyerov. Lin already runs a BI front end over a GPU engine, which is exactly the lean deployment.

## How it would work

- **Host:** a DuckDB extension. DuckDB keeps the parser, catalog and optimizer, as Sirius does through standard optimizer hooks (`src/sirius_extension.cpp:3676-3677` @ b9e05ffc) [S]. Unsupported plans fall back to DuckDB's CPU.
- **Cache key:** (file or table, column, row group or version), with per-chunk zone maps. ClickHouse's RFC proposes the same key: (part, column, data version) [CH2].
- **Tiers:** VRAM first, then pinned host RAM (an HGX host has 2–4 TB), then the source Parquet or DuckDB file. Columns are decoded, with optional lightweight compression (bitpack, delta, dictionary) [S][H].
- **Placement:** the fact table is hash-partitioned across GPUs on its main join key when it is loaded. Small dimensions are replicated to every GPU. Most joins then stay local, and only partial aggregates cross NVLink [BM].
- **Admission and eviction:** admit columns on repeated access; evict LRU under a VRAM budget; invalidate on write or CHECKPOINT. Sirius today pins only on explicit request, with no eviction [S].
- **Data path, end to end:**
  - **Miss:** Parquet (local NVMe, or object store) → host → chunked GPU decode → cache.
  - **Hit:** the scan reads HBM directly, then each GPU runs its local operators, then a small NVLink exchange merges the partials, and the result goes back to DuckDB as Arrow.
- **Front door:** the user runs `LOAD lean;` in their existing DuckDB and connects BI tools to DuckDB as before.

## The human's questions

See The three parts.

## The three parts

### 1. Can a single-host multi-GPU cache answer ~100 GB queries in a few hundred ms?

**FOR**
- **Capacity is not the problem.** Per-node HBM, and the time to scan 100 GB once at peak (100 GB ÷ GPUs ÷ HBM bandwidth; estimate):

  | Box | HBM per node | Scan 100 GB once |
  |---|---|---|
  | 8×H100 | 640 GB | 3.7 ms |
  | 8×H200 | 1.1 TB | 2.6 ms |
  | 8×B200 | 1.44 TB | 1.6 ms |
  | 8×B300 | 2.3 TB | 1.6 ms |
  | 4×RTX PRO 6000 | 384 GB | 16 ms |
  | 4×L40S | 192 GB | 29 ms |

  Spec sources: [H1][H2][H3][H4][H5]. Sirius compresses the 7 hottest lineitem columns 2.8× (264 → 93 GB at SF1000) [S], so 100 GB raw is about 35–50 GB resident (estimate).
- **Measured device reads come close to spec.** An H200 reads its own memory at 4,484 GB/s, against 4.8 TB/s on paper [BM].
- **Measured multi-GPU execute, scaled to ~100 GB, lands in budget.** Hand-written cuDF pipelines on 8×H100 NVLink at sf40 [BM], multiplied by 2.5 for SF100 (estimate):

  | Query | 8×H100, sf40 | 8×H100, SF100 (est.) | 1×H200, SF100 (est.) | DuckDB 128 cores, SF100 (est.) |
  |---|--:|--:|--:|--:|
  | q6 | 3.7 ms | ~9 ms | ~48 ms | ~343 ms |
  | q1 | 32.6 ms | ~82 ms | ~600 ms | ~2.3 s |
  | q3 | 17.6 ms | ~44 ms | ~100 ms | ~1.4 s |
  | q8 | 17.2 ms | ~43 ms | ~119 ms | ~2.8 s |

- **Published hot single-GPU results agree:**
  - NVIDIA GQE runs TPC-H SF1000 (22 queries) in 9.0 s total on one B200, hot. That is ~0.41 s per query at 1 TB, against DuckDB's 70.6 s [G].
  - Sirius, hot on one GH200 at SF100, is 8.3× more cost-efficient than DuckDB [SP].
- **NVLink makes shuffles cheap.** An all-to-all of 10 GB per GPU takes ~11 ms at 900 GB/s on H100/H200, and half that at 1.8 TB/s on B200 (estimate) [H2][H3].

**AGAINST**
- **Only if the data is already resident.** Refilling 100 GB from host RAM is in budget only across all 8 GPUs and only with compression. Re-reading Parquet takes seconds.

  | Path into VRAM | Rate | Time for 100 GB (est.) |
  |---|---|---|
  | Pinned host RAM, 1 GPU | 47.3 GB/s measured on H200 [BM] | 2.1 s |
  | Pinned host RAM, 8 GPUs, no contention | ~380 GB/s | ~0.26 s |
  | Pinned host RAM, 8 GPUs, compressed (35 GB) | ~380 GB/s | ~0.09 s |
  | Pageable host RAM, 1 GPU | 10.6 GB/s [BM] | 9.4 s |
  | Parquet decode, 1 GPU | 6.8–8.7 GB/s [BM] | 11.5–14.7 s |
  | Parquet decode, 8 GPUs | — | ~1.4–1.8 s |

  The engine's own path is worse. It uses `read_parquet`, which is 5× slower than the chunked reader on GB10 (#232) [I][DS].
- **PCIe-only boxes break the shuffle budget.**
  - Measured peer-to-peer on an 8×L40S-class box is ~20–22 GB/s [H6]. The same 10 GB-per-GPU shuffle then takes ~0.5 s.
  - IBM's Presto on 8×A100: 93 s with the NVLink exchange, 269 s over RDMA, 828 s over HTTP [P].
  - Sirius, distributed over 400 Gb/s InfiniBand at SF100: Q3 takes 341 (probably ms), and 233 of that is exchange [SP].
- **Shuffle-bound queries scale badly even with NVLink.** q3 gains only 1.89× and q8 2.31× at G=8 [BM]. Every laggard is a cost that stays constant in G.
- **No published system has shown it.** No one has published hot, single-node, multi-GPU per-query timings at ~100 GB. Theseus published cold multi-node runs only. Sirius's multi-GPU support is new on `main` [H][S].
- **peacockdb has none of this today:**
  - no cache;
  - no engine multi-GPU path;
  - no installed RMM pool;
  - pageable host buffers.

  The 8×H100 numbers come from hand-written test pipelines, not the engine [I].
- **Multi-GPU is not needed for capacity.** One H200 (141 GB) or B300 (288 GB) holds 100 GB once compressed. Several GPUs buy latency only, at 8× the price: Lambda rents 8×H100 for $31.92/h, against $0.45–1.35/h for a 64-core CPU box [H7][E].
- **The cost per query is roughly a wash.** The estimated GPU speedup over 128-core DuckDB is 28–65×, about the same as the 24–71× price ratio (estimate). The product sells latency, not cost.

**Holds?** Yes, on NVLink boxes with a VRAM-resident, co-partitioned working set. No on cold data. Only partly on PCIe-only boxes, where join-heavy queries miss the budget.

### 2. Plugin inside an existing database, or a standalone engine?

**FOR the plugin**
- **Over $300M of standalone GPU-database VC has produced no breakout** [PS]:
  - MapD/OmniSci/HEAVY.AI raised ~$92M and was quietly absorbed by NVIDIA in 2025, a "sort-of acquihire" [PV][PB].
  - Voltron raised $110M and died in 2025 [PV].
  - BlazingSQL closed in 2021 [DB1].
  - Kinetica survives on government contracts [PV].
  - SQream raised $111–135M and pivoted to SQream Blue, which queries data in place, "eliminating the need for data duplication" [SQ].
- **NVIDIA's strategy is acceleration in place.** Its page is titled "Accelerating existing SQL engines in place" [NP], and its FAQ steers engine builders to Velox "without forking" [N].
- **The 2026 momentum is all inside host engines:**
  - IBM watsonx.data Presto on Velox-cuDF; Nestlé's PoC went from 15 to 3 min (vendor) [IBM].
  - ClickHouse RFCs build on Sirius [CH1][CH2].
  - DuckDB PR #28022 adds Sirius as a core extension (opened 2026-10-28) [DK].
  - Doris has a Sirius backend [S].
- **Spark RAPIDS reached distribution as a plugin.** It runs on Databricks, Dataproc and EMR, and NVIDIA sells it through AI Enterprise [RS][N].
- **Standalone ingestion is a real barrier.** HeavyDB needs a load into its own tables. A VLDB 2025 study ran only 9 of 22 TPC-H queries fully on GPU [VL].

**AGAINST the plugin**
- **The plugin slot is taken.** The free Sirius extension already pins tables in VRAM or host RAM, with compression and zone maps [S]. ClickHouse builds GPU in-house [CH1]. PG-Strom sells GPU-Cache inside Postgres [D]. A third-party DuckDB cache plugin would sit beside an NVIDIA-staffed one.
- **Plugin evidence is thin too:**
  - Sirius has ~1.1k stars and no named production users [PS].
  - Spark RAPIDS case studies are NVIDIA-published and from 2020–22 [RS].
  - PG-Strom's vendor is small and founder-owned, with no named customers [PS].
- **Plugins are hard to charge for.** BlazingSQL was open source, queried files in place, and still went unpaid: "Fortune 500 customers… deploying us at scale but not paying us" ([HN 40079813](https://news.ycombinator.com/item?id=40079813), 2024-04-18).
- **The host constrains the plugin:**
  - Sirius must match the exact DuckDB revision, and loading it disables 5 optimizer rules [S].
  - The ClickHouse RFC admits the Substrait route loses PREWHERE, primary-key pruning and skip indexes [CH1].
- **Standalone engines that require ingestion can win.** Snowflake and ClickHouse did, on CPU, by fitting a broad workload (inference, not sourced here). Ingestion alone does not explain the GPU failures.
- **The host may not fit the hardware.** DuckDB users skew toward laptops and single CPU boxes. Few of them rent an 8-GPU HGX (estimate).

**Holds?** Mostly. Plugins clearly win distribution. But the best host, DuckDB, already has NVIDIA's plugin, and no plugin vendor has shown it can charge.

### 3. Did Voltron Data fail because it built a standalone engine that required ingestion?

**What the sources say**
- **Funding:** $110M ($22M seed + $88M Series A) in 2022-02. Walden Catalyst led; Lightspeed, Coatue, GV, BlackRock and others joined. NVIDIA was not an investor [TC].
- **Product:** Theseus launched in 2023-12, about 22 months after the raise [HN1][TZ].
- **Leadership:**
  - The CTO, Wes McKinney, left full-time duties in 2023-10 [WM].
  - Around 2024-10 the CEO changed and about half of ~120 staff were cut "after struggling to meet its revenue target" (headline plus search summary; the article body is paywalled) [TI][BL].
- **Last moves:** Accenture invested in 2025-02 [AC]. AMD support followed in 2025-06 [R25].
- **Shutdown:** in 2025, with no public post-mortem [DB2][PV].
- **The team went to NVIDIA:**
  - Patterson is NVIDIA VP Solutions Architecture.
  - The Aramburus presented cuCascade at GTC 2026, with Theseus as "the research system behind" it [GTC][N].
  - No acquisition is on record [N].

**Ingestion contradicts the premise**
- Theseus read Parquet in place. The company told users they "don't need to move their data" [TZ].
- The 100 TB TPC-H run was on "unsorted Parquet files, read directly from storage" [R24].
- It was embedded in partners' products (HPE Ezmeral first) on a revenue share [TZ][R24].

**Causes, ranked by support**
1. **Burn against revenue, and a late launch (strong).** The Information reports the missed revenue target [TI]. Pavlo: it "failed to launch it in a timely manner" [PV].
2. **A narrow market at the top (moderate).** Its own positioning left DuckDB below 2 TB and Spark/Trino for 2–30 TB, and claimed only the workloads above 30 TB [R24]. No source names an end customer [TZ].
3. **A partner channel on rev-share (moderate).** "Voltron Data earns revenue only when partners generate revenue" [TZ].
4. **Leadership churn (contributing).**
5. **GPU cost or supply: unsourced.**

**How far the premise holds:** not at all on ingestion. A weaker form holds: Voltron sold a new engine rather than speeding up one the customer already ran. Even that was softened by embedding through HPE.

**The lesson for lean is different.** Voltron's sources point to a small, late, top-of-market niche and too much burn. lean targets ~100 GB, the range Voltron itself conceded to CPU DuckDB [R24]. So lean must beat a strong, free CPU engine on latency alone.

## Data path and cache

- **The thesis holds, and this option is the fix.** Without a resident cache, NIC, SSD and decode throughput eat the GPU's advantage.

| Stage | Rate | Source |
|---|---|---|
| HBM read, H200 | 4,484 GB/s measured | [BM] |
| NVLink, H100/H200 → B200/B300 | 900 → 1,800 GB/s per GPU | [H2][H3] |
| PCIe Gen5 H2D, pinned | 47.3 GB/s (H200); 56 GB/s (vendor) | [BM][H8] |
| PCIe H2D, pageable | 10.6 GB/s | [BM] |
| PCIe P2P, L40S | ~20–22 GB/s | [H6] |
| Parquet decode on the GPU | 6.8–8.7 GB/s per GPU; page decode + Snappy is 80% of kernel time on q11v | [BM] |
| NVMe, 8× U.2 in a DGX | ~50 GB/s total (estimate) | [H9] |
| NIC, 8× ConnectX-7 | 8 × 400 Gb/s = 400 GB/s nominal; HDFS held 8×H100 at 15% | [H9][D] |
| Host RAM, 2-socket DDR5 | ~600 GB/s (estimate); HGX B300 minimum is 500 GB/s | [H3] |

- **Compute against feed:** compute-read is 95× the load rate on H200 [BM]. The human's PoC lost to its own load on 11 of 12 SF20 queries [E]. spark-rapids spent ~10× its GPU time in JVM reader code [E].
- **The cache this needs:**
  - **Tier 1:** decoded, lightly compressed columns in VRAM, co-partitioned across GPUs.
  - **Tier 2:** pinned host RAM, to refill in ~0.1–0.3 s across 8 GPUs.
  - **Tier 3:** Parquet through the chunked reader.
- **On Grace hosts the picture shifts.** On GB10, pinning makes no difference and compute-read is only 4× load, so residency buys less [BM]. On B200 or GB200, the decompression engine gives up to 400 GB/s effective H2D at an assumed 4× ratio (vendor) [G].
- **The first fill is still slow.** Expect 1.5–15 s per 100 GB depending on GPU count and reader (estimate). Dashboards tolerate that once; ad-hoc users do not.

## Build cost to alpha

Sizes are in yardstick lines; the unit is 22,830 [I]. Every figure is an estimate.

| Component | On peacockdb: reuse/adapt/new, size | On Sirius: reuse/adapt/new, size |
|---|---|---|
| Host plugin: DuckDB extension plus plan capture | new: DuckDB `LogicalOperator` → peacockdb node set, plus hooks. Sirius spent 13.5k on this; reusing the node vocabulary cuts it to 6–9k | reuse |
| CPU fallback for refused plans | new, in DuckDB: 0.5–1k | reuse |
| Cache manager (tiers, admission, LRU, invalidation, zone maps) | new: 3–5k | adapt `pin_table` into an automatic cache with eviction (pinned GPU entries cannot spill today): 2–4k |
| Compression in cache | new, on nvCOMP: 1–2k | reuse Simpatico |
| Multi-GPU: worker per GPU, sharded placement, exchange | new: 3–6k; test-binary code exists [I] | adapt: shard pins across devices, co-partition on load: 1–2k |
| Memory tiers and spill | adopt cuCascade, minimal: 2–4k | reuse |
| Fill path (chunked reader, pinned buffers, multi-file, DuckDB tables) | adapt: 1–2k | reuse |
| Allocator pool and accounting | adapt: 0.3k (#148) | reuse |
| Concurrent queries from BI tools | new: 1–2k | adapt (one query at a time per context [S]): 1–2k |
| Tests and differential harness | adapt the corpus harness to the DuckDB host: ~1k | adapt: 0.5–1k |
| **Total** | **~19–32k** | **~5–10k** |

Total new work: 1.1× engine on peacockdb, 0.3× on Sirius (estimate)

- **The hard part:** a cache that is correct and fast at once. It must:
  - invalidate on write, which Sirius's pins get wrong today ([sirius #106](https://github.com/sirius-db/sirius/issues/106));
  - evict under pressure without deadlocking pinned batches [S];
  - place shards so joins stay local.
- **On Sirius, the hard part is upstream churn.** About 50–60k lines change per week, so the work must be upstreamed, not forked [S].
- **On peacockdb, the hard part is that 1.1× mostly rebuilds Sirius in a host where Sirius already exists.**

## Adoption friction

- **Low for the user:** `LOAD` an extension and pin or warm the tables, with no migration.
- **High for the buyer:** they must own or rent a multi-GPU HGX next to a DuckDB process. Typical DuckDB deployments are single CPU boxes (estimate).
- **The host bites:** an exact-revision pin to DuckDB, disabled optimizer rules, and Linux, NVIDIA and `io_uring` only [S].
- **A fill is still a load.** Users wait seconds on the first touch, and writes invalidate the cache.

## Platform risk

- **nvidia.md rates realtime-vram-cache strong risk** [N].
  - NVIDIA owns HeavyDB and gave it away.
  - GQE has nvCOMP-compressed in-memory tables.
  - Sirius pins tables in VRAM.
- **Sirius is ≈92% NVIDIA staff and is entering DuckDB core** [S][DK]. An automatic cache is an obvious next feature for it. The legacy engine already cached every scanned column [S].
- **ClickHouse is designing exactly this cache,** keyed by part, column and version [CH2].
- **nvCOMP is closed source**, and compressed caching sits on it [N].

## Funding story

- **What a design partner would say:**
  - Lin: "Superset over 9.6B rows in under a second, without Spark Thrift, is worth a box."
  - Others: "Sirius already does `pin_table`. Why pay you?" (estimate)
- **What the round would be raised on:**
  - an automatic, multi-GPU, always-fresh GPU cache for BI over an existing database;
  - one paying BI or observability user;
  - a sub-second demo at 100 GB against DuckDB's 1–3 s.
- **The weakness:** the moat is a feature of a free NVIDIA-staffed project. Pavlo calls GPU databases "inviable… still niche" [PV]. Raising on that needs a named user with money.

## Evidence against

- **Demand is soft.** There are 6 posters, no latency target and no budget [D].
- **The feature already exists for free:** Sirius `pin_table`, PG-Strom GPU-Cache, Fabric's warm cache and KDB-X [D][S].
- **Price-performance is break-even.** 8×H100 costs 24–71× a CPU box and is ~28–65× faster when hot (estimate). The pitch is latency only.
- **100 GB fits on one large GPU,** so the "several GPUs" premise is weak.
- **CPU engines keep gaining.** DuckDB 2.0 saturates 128–192 vCPUs [D].
- **peacockdb lacks every lean component today** [I].
- **The Voltron premise behind this option is false** (part 3). The real warning is a niche that cannot pay for itself.

## Ratings

- **Who needs it badly — medium:** demand.md's `lean` row. About 6 posters, soft asks, no target [D].
- **Build cost — medium:** 0.3× on Sirius fits easily but means tracking a fast upstream. 1.1× on peacockdb fits but duplicates Sirius.
- **Data path — strong:** the cache is the cure for the measured load bottleneck. Hot, NVLink-resident numbers fit the budget. Cold fill and PCIe-only shuffles are the caveats.
- **Adoption friction — medium:** the plugin avoids migration, but the buyer needs a multi-GPU box beside DuckDB.
- **Platform risk — weak:** NVIDIA's Sirius, GQE and HeavyDB, plus ClickHouse, already ship or design the same cache [N].
- **Funding story — weak:** "a better pin_table" is a feature of a free NVIDIA-staffed project, and there is no paying user yet.

## Follow-up experiments

1. **peacockdb at SF100 on 8×H100 NVLink, hot.** Use the engine's own path, not test pipelines; measure per-query ms against DuckDB on 128 cores.
2. **The same run on a PCIe-only 4×L40S or RTX PRO 6000 box.** Measure q3 and q8 shuffle time against the ~0.5 s estimate.
3. **Sirius `pin_table` on 8 GPUs at SF100.** Does it shard? What are hot ms and fill time? This decides whether lean adds anything.
4. **Refill from pinned host RAM on 8 GPUs at once.** Measure contention at the root complex against the ~380 GB/s estimate.
5. **Compression ratio and decode cost** for the first user's real columns, not TPC-H.
6. **Ask Lin and Gong** for their working-set size, refresh rate and latency target.

## Sources

- [D] demand.md, `## Ratings`, `## First conversations`, `## Evidence against`: `llm-wiki/artifacts/product-research/demand.md` (local, master f36a68ec; accessed 2026-10-29)
- [E] evidence-in-hand.md (local; accessed 2026-10-29)
- [I] inventory.md, at master f25c250d (local; accessed 2026-10-29); [S] sirius.md; Sirius code @ b9e05ffc653c8ed031c789e5c1415ac1186e0619, https://github.com/sirius-db/sirius/tree/b9e05ffc653c8ed031c789e5c1415ac1186e0619 (accessed 2026-10-28)
- [N] nvidia.md (local; accessed 2026-10-29); [BM] `llm-wiki/reports/benchmark-minimal.md` (local; accessed 2026-10-29)
- [DS] `llm-wiki/reports/dgx-spark.md` §2 (local; accessed 2026-10-29); [H] research notes for this page: `../notes/lean-gpu-cache-premises/hardware.md` (accessed 2026-10-29)
- [H1] https://www.nvidia.com/en-us/data-center/h100/ (accessed 2026-10-29); [H2] https://docs.nvidia.com/dgx/dgxh100-user-guide/introduction-to-dgxh100.html (accessed 2026-10-29)
- [H3] https://docs.nvidia.com/enterprise-reference-architectures/hgx-ai-factory/latest/components.html (accessed 2026-10-29); [H4] https://www.nvidia.com/en-us/data-center/rtx-pro-6000-blackwell-server-edition/ (accessed 2026-10-29)
- [H5] https://www.bhphotovideo.com/c/product/1825422-REG/nvidia_nvl40stcgpu_kit_l40s_data_center_gpu.html (accessed 2026-10-29); [H6] https://forums.developer.nvidia.com/t/linux-enabling-persistent-pcie-gen-4-0-and-pcie-buswidth-16-for-l40s-gpu/338641 (accessed 2026-10-29)
- [H7] https://lambda.ai/pricing (accessed 2026-10-29); [H8] https://www.mitaccomputing.com/en/industryinsight/Redefining_Server_Performance_Introducing_TYAN_Transport_HX_FT65TB8050_equipped_with_NVIDIA_H100_GPU (accessed 2026-10-29)
- [H9] https://www.nvidia.com/en-us/data-center/dgx-b200/ (accessed 2026-10-29); [G] https://developer.nvidia.com/blog/designing-gpu-accelerated-query-engines-with-nvidia-gqe/ (accessed 2026-10-29)
- [SP] Sirius paper, https://arxiv.org/html/2508.04701v3 (accessed 2026-10-29); [P] Accelerating Presto with GPUs, https://arxiv.org/pdf/2606.24647 (accessed 2026-10-29)
- [PS] research notes: `../notes/lean-gpu-cache-premises/plugin_vs_standalone.md` (accessed 2026-10-29); [PV] Pavlo, 2025 databases retrospective, https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html (accessed 2026-10-29)
- [PB] https://pitchbook.com/profiles/company/98344-09 (accessed 2026-10-29); [DB1] https://dbdb.io/db/blazingsql (accessed 2026-10-29)
- [DB2] https://dbdb.io/db/theseus (accessed 2026-10-29); [SQ] https://sqream.com/product/blue/ and https://venturebeat.com/business/sqream-nabs-45m-to-accelerate-data-analytics-with-gpus (accessed 2026-10-29)
- [NP] https://perspectives.nvidia.com/cudf/accelerating-existing-sql-engines-in-place-for-faster-query/ (accessed 2026-10-29); [IBM] https://newsroom.ibm.com/2026-03-16-ibm-and-nvidia-announce-expanded-collaboration-at-gtc-2026-to-advance-ai-for-the-enterprise and https://www.itjungle.com/2026/03/23/ibm-taps-nvidia-gpus-for-ai-turbocharged-data-mart/ (accessed 2026-10-29)
- [CH1] https://github.com/ClickHouse/ClickHouse/issues/116807 (accessed 2026-10-29); [CH2] https://github.com/ClickHouse/ClickHouse/issues/124455 (accessed 2026-10-29)
- [DK] https://github.com/duckdb/duckdb/pull/28022 (accessed 2026-10-29); [RS] https://nvidia.github.io/cudf-spark/docs/download.html and https://blogs.nvidia.com/blog/att-data-science-rapids (accessed 2026-10-29)
- [VL] Li et al., VLDB 2025, https://www.vldb.org/pvldb/vol18/p4518-li.pdf (accessed 2026-10-29); [TC] https://techcrunch.com/2022/02/17/voltron-data-grabs-110m-to-build-startup-based-on-apache-arrow-project/ (accessed 2026-10-29)
- [HN1] https://www.helpnetsecurity.com/2023/12/01/voltron-data-theseus/ (accessed 2026-10-29); [TZ] https://www.techzine.eu/blogs/data-management/114003/voltron-charges-ai-data-brighter-than-apache-spark (accessed 2026-10-29)
- [WM] https://wesmckinney.com/blog/joining-posit/ (accessed 2026-10-29); [TI] https://www.theinformation.com/briefings/ai-startup-voltron-data-switches-ceos-lays-off-staff (headline and search summary only; accessed 2026-10-29)
- [BL] https://www.teamblind.com/post/what-happened-to-voltron-data-bxtoxtrf (accessed 2026-10-29); [AC] https://www.silicon.co.uk/press-release/accenture-invests-in-voltron-data-to-help-organizations-use-gpu-technology-to-simplify-large-scale-data-processing (accessed 2026-10-29)
- [R24] https://www.theregister.com/2024/03/19/voltron_data_arrow/ (accessed 2026-10-29); [R25] https://www.theregister.com/2025/06/18/voltron_amd_sql/ (accessed 2026-10-29)
- [GTC] https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html (secondhand; accessed 2026-10-29)
