# postgres-gpu-analytics — GPU analytic offload for teams that live in Postgres

## Verdict

- **What it is:** a Postgres extension plus a GPU sidecar process. The extension hands analytic queries to the sidecar, which serves them from a GPU-resident columnar mirror of heap tables (fed by logical replication) and from Parquet/Iceberg.
- **Who needs it:** demand.md rates it **weak**: 3 posters, curious rather than in pain [D]. The incumbent, PG-Strom, has been around for 9 years with no named customers, and it is dropping its own hot-table cache as "rarely used" [PS1][PS4].
- **Budget:** fits on peacockdb (~1.1×) and fits easily on Sirius (~0.4×). On Sirius the alpha is almost a weekend: pg_lake's DuckDB sidecar pins the same DuckDB version (v1.5.6) as Sirius `stable` [PL][S]. That is also why anyone can ship it.

## Who needs it badly

- **Rating: weak, taken from demand.md `## Ratings`, row `postgres-gpu-analytics`** [D]: "The askers are curious rather than in pain. The 2025–26 PG-Strom tracker is mostly its maintainers."
- **Víctor Romero (`IOhacker`, Fintecheando)** evaluated PG-Strom GPU-Cache under Apache Fineract (core banking) on PG16 ([pg-strom #796](https://github.com/heterodb/pg-strom/issues/796), 2024-06-25). GPU-Cache broke a Liquibase `ALTER TABLE`, and KaiGai fixed it in `5e7b418c` within a week. He wrote: "I am now testing and understanding the PG Strom". This was an evaluation, not a production deployment [PS9].
- **Mostafa Bouzari (SO)** asked how to pull SQL Server/Postgres tables straight into cuDF memory (711 views) ([SO 77960181](https://stackoverflow.com/q/77960181), 2024-02-08) [D].
- **Zain Ul Abidin (SO)** wanted PG-Strom on an NVIDIA Xavier edge box, and the Docker build failed ([SO 77300203](https://stackoverflow.com/q/77300203), 2023-10-16) [D].
- **PG-Strom's outside users:** about 20–25 distinct outside reporters filed issues from 2024 to 2026, out of 302 issues; about two-thirds came from KaiGai and a few Japanese insiders [PS1]. The reports are mostly build errors, crashes and 19 "[GPU Logic Bug]" wrong-result reports. `a-hirota` filed 8 issues (Arrow/Parquet crashes, a licence bug). Andres Freund filed #1062 (2026-09-30) [PS1].
- **The only customer story** is anonymised: an "international top-brand" semiconductor fab with about 100 TB of logs in Postgres (2020 deck) [PS5]. The docs say "major portion of PG-Strom users are Japanese" [PS1].
- **Indirect demand is the CPU analytics-on-Postgres wave.** pg_duckdb has 3,275 stars [DK]. Analytics-on-Postgres companies were bought: Crunchy by Snowflake, Neon and Mooncake by Databricks (dates from memory, unverified) [LS]. But that demand is met on CPU by DuckDB. Nobody in the corpus says DuckDB-in-Postgres is too slow and needs a GPU [D].
- **Best first conversations:**
  1. Romero (Fineract). Ask whether his GPU-Cache evaluation went anywhere.
  2. `a-hirota`, who runs PG-Strom on Arrow/Parquet.
  3. EDB's PGAA team, as a partner, not a user. They already market "GPU Postgres" through Spark RAPIDS [EDB2].

## How it would work

- **Shape:** a thin in-Postgres extension plus a separate GPU server process. Per-backend GPU work does not fit Postgres:
  - Postgres runs one process per connection.
  - PG-Strom moved all GPU work into one multi-threaded "GPU-Service" background worker in v5.0 [PS10].
  - pg_lake runs its engine (DuckDB) out of process as `pgduck_server` [PL].
  - Per-backend CUDA contexts would each pin VRAM (estimate).
- **In Postgres:**
  - A `planner_hook` takes over the whole query when every relation it reads is mirrored or external. Otherwise it falls back to `standard_planner`. pg_duckdb does exactly this (`src/pgduckdb_hooks.cpp` @ a9457c9c, lines 508–521) [DK].
  - The query is deparsed to SQL (pg_duckdb vendors Postgres `ruleutils` for this) and sent over Arrow Flight.
  - A CustomScan node streams the Arrow result back as tuples.
- **The mirror:**
  - A logical-replication slot (`pgoutput`) feeds the sidecar.
  - The initial snapshot is copied in parallel by ctid range.
  - Inserts are appended to a pinned-host delta. Updates and deletes set bits in a delete bitmap.
  - The delta is sealed into GPU chunks every N rows or T seconds. pg_mooncake/moonlink mirror heap tables into Iceberg the same way, claiming "sub-second freshness" [MC].
- **Consistency:** each query reads the mirror as of a replicated LSN. That is not the backend's own snapshot, so read-your-writes inside one transaction is lost (estimate). The extension must refuse, or fall back, when the transaction has local writes.
- **Data path, end to end:**
  - **Write:** client → heap → WAL → walsender → sidecar decode → pinned delta → seal → VRAM.
  - **Query:** client → Postgres planner hook → Flight → sidecar plan (DataFusion) → GPU over resident chunks → Arrow back → CustomScan → client.
  - **External files:** Parquet/Iceberg go through the engine's existing Parquet path, or a cache fill [I].

## The human's questions

**1. What exists, and how did it fare?**

| Project | Mechanism | Status, outcome |
|---|---|---|
| **PG-Strom** (HeteroDB) | CustomScan (GpuScan/GpuJoin/GpuPreAgg/GpuSort). Its own CUDA interpreter, no cuDF. GPU-Direct SQL via GDS. GPU Cache. Arrow_Fdw. | Alive, but run by one person. More below. |
| **pg_duckdb** (DuckDB Labs, MotherDuck) | `planner_hook` whole-query takeover into embedded DuckDB. Heap read via a Postgres plan. | MIT, 3,275★, v1.1.1 (2025-12). Active (`a9457c9c`, 2026-10-28) [DK][LS]. |
| **pg_mooncake** (Mooncake Labs) | v0.2+: logical-replication mirror into Iceberg (moonlink), queried through pg_duckdb | MIT, 2,012★. Last commit 2025-10-26. mooncake.dev redirects to databricks.com [MC][LS]. |
| **ParadeDB pg_analytics** | FDW + executor hook into DuckDB. Lake files only, never heap. | Archived 2025-03-19 ("Final release", `5eef8022`). ParadeDB pivoted to search [PA]. |
| **Hydra columnar** | Table access method, a fork of Citus columnar, vectorized | 3,042★. Dormant since 2025-02-10 (`d1f7657e`) [HY]. |
| **Citus columnar** (Microsoft) | `USING columnar` table AM. Compression and chunk skipping. Not vectorized. | AGPL. Active (`843ef6f0`, 2026-10-29) [CI]. |
| **AlloyDB columnar engine** (Google) | In-memory column store of selected columns, auto-columnarized, 30% of RAM by default | Proprietary. "Frequent updates to rows invalidate columnar data". No numbers in the docs [AL]. |
| **Timescale/TigerData hypercore** | Rowstore for recent chunks, auto-converted to a compressed columnstore. Vectorized aggregates. | Active, 23,772★. Claims "90–98% compression" and 30%–2× for newly vectorized filters [TS][LS]. |
| **pg_lake** (Snowflake, ex-Crunchy) | `USING iceberg` tables and foreign tables. DuckDB in a separate `pgduck_server` that speaks the PG wire protocol. | Apache-2.0, repo created 2025-11-04. v3.5.10 on 2026-10-27 [PL]. |
| **EDB PGAA** | Planner hook. Offloads to Seafowl (DataFusion) over Arrow Flight, or via Spark Connect to remote Spark. | Shipped. GPU only through Spark RAPIDS. More below [EDB1]. |

- **The pattern** [LS]:
  - The CPU engine inside Postgres converged on DuckDB. The only exception is EDB, which uses DataFusion.
  - Independent analytics-on-Postgres startups were acquired as on-ramps (Crunchy, Neon, Mooncake), pivoted (ParadeDB, BemiDB → pgstack) or went quiet (Hydra).
  - The survivors belong to big vendors.
- **PG-Strom in detail** [PS1–PS10]:
  - **Licence:** PostgreSQL License since `21304d38` (2021-05-26); GPL v2 before.
  - **Open core:** the proprietary `heterodb-extra` module and a licence file bound to GPU UUIDs gate multi-GPU, md-raid0 striping, NVMe-oF and GPU GiST. Without them GPU-Direct runs only as a CPU emulation ("vfs"). No price is published.
  - **Business:** HeteroDB, Tokyo, founded 2017-07-04 by KaiGai Kohei (ex-NEC, author of the CustomScan API). He wrote 85% of the 436 commits since 2025; 0-kaz wrote most of the rest. The project accepts no pull requests. Bus factor is about 1.
  - **Code:** latest tag v6.1 (`4d12ef41`, 2025-11-17); master at `29349cbf` (2026-10-28).
  - **Performance (vendor-run SSBM):**
    - About 40 GB/s from 16 NVMe into 4×V100 (2020).
    - On heap tables, about 3–5× over vanilla Postgres. The 10–30× figures need Arrow files.
    - The 2025 "~817×" result comes from skipping files by name, not from GPU compute.
  - **Drift:** the draft v6.2 notes (dated 2026-06-20) remove GPU-Cache, DPU and PostGIS as "rarely used in practice". The roadmap moves toward Parquet, S3 and a remote GPU service.
- **EDB's GPU announcement** [EDB1–EDB5]:
  - PGAA can send lake-table queries via Spark Connect to a remote Spark cluster running the RAPIDS plugin.
  - The docs limit it to "read-only queries on Parquet files in S3… or POSIX". Postgres heap never touches the GPU.
  - It shipped before PGAA 1.6.0 (2026-02-04). It was promoted at GTC on 2026-03-16 with "50–100x" claims (3 TB+), and "up to 91x" on 2026-03-31, with no methodology.
  - NVIDIA's GTC blog names EDB among cuDF/cuVS platforms [N].
- **No Sirius-in-Postgres integration was found** [LS]. pg_lake's DuckDB is exactly tag v1.5.6 (`069cc9f9`) [PL]. Sirius `stable` targets DuckDB 1.5.6 [S]. `pgduck_server` loads unsigned extensions from its extension directory when runtime installs are off (`pgduck_server/src/duckdb/duckdb.c:307-330` @ 1918df64) [PL]. So Sirius in pg_lake is a build-and-test exercise (estimate, untested).

**2. Postgres-side mechanics.**

- **CustomScan provider** (PG-Strom):
  - Paths are offered via `set_rel_pathlist_hook`, `set_join_pathlist_hook` and `create_upper_paths_hook`. The upper hook fires at FINAL, GROUP_AGG, WINDOW, DISTINCT, PARTIAL_DISTINCT and ORDERED (`planner.c` @ cc4bf6e9) [PG].
  - It keeps Postgres's planner, permissions and snapshot.
  - It must cost every path against Postgres's own, and it gets only node-sized pieces of the query.
- **Whole-query takeover** (pg_duckdb, PGAA):
  - `planner_hook` replaces the plan. pg_duckdb also hooks ExecutorStart/Finish and Explain, but not ExecutorRun [DK].
  - The engine sees the whole query. Unsupported SQL must fall back to Postgres as a whole.
  - This shape fits an engine with its own planner (DataFusion) best.
- **FDW:**
  - It is the only shape that runs on managed Postgres with no custom code.
  - `postgres_fdw` pushes down only GROUP_AGG, ORDERED and FINAL, so window functions and DISTINCT are never pushed. Its default `fetch_size` is 100 rows [PG].
  - It pulls results row by row through a cursor.
- **Sidecar via Arrow** (pg_lake, PGAA):
  - Engine crashes and VRAM stay out of the backend.
  - Postgres talks to it over the wire protocol (pg_lake) or Arrow Flight (PGAA) [PL][EDB1].
- **Heap pages versus a columnar mirror:**
  - **pg_duckdb:** reads heap by running a Postgres plan slot by slot (`ExecProcNode`, `src/scan/postgres_table_reader.cpp:300-311` @ a9457c9c). By default it uses 2 Postgres workers and 2 DuckDB threads per scan (`docs/settings.md`) [DK]. Postgres's executor caps it.
  - **PG-Strom:** DMAs raw 8 kB heap blocks to the GPU and parses `HeapTupleHeader` there. That works only for all-visible pages (`src/relscan.c` ~L620-645 @ 29349cbf). The GPU "cannot run MVCC visibility checks per row", so users must VACUUM [PS1][PS2].
  - **Core Postgres:** skips per-tuple visibility only on all-visible pages (`heap_prepare_pagescan`, `heapam.c` @ cc4bf6e9) [PG].
  - **Conclusion:** hot OLTP tables, the ones Postgres teams care about, are the ones that are not all-visible. So the product needs a mirror.
- **Logical replication into a GPU cache:**
  - pg_mooncake does it into Iceberg (`wal_level=logical`) [MC]. EDB does it via PGD "automated partition offloading" [EDB1]. AlloyDB keeps an in-memory column cache that updates invalidate [AL].
  - The cost is the initial snapshot. PeerDB copied 1.5 TB in 43 h on one thread and in under 5 h on 32 threads. That is about 10 MB/s and about 88 MB/s (estimate, derived) [PDB].
  - The other cost is sustained decode rate on one walsender (unmeasured here).

**3. Which managed Postgres providers would allow a GPU extension?**

- **None that I could verify** [MP]:
  - **Cloud SQL:** "You cannot create your own extensions in Cloud SQL" [CSQL].
  - **RDS:** a fixed list. User-built extensions go only through `pg_tle`, which takes trusted languages, not C/CUDA [RDS][TLE].
  - **Azure Flexible Server:** a fixed list, with no pg_duckdb, citus or columnar [AZ].
  - **Neon:** lists only pg_mooncake 0.1.3, as experimental, behind a flag [NEON].
  - **Supabase:** documents only SQL/plpgsql extensions [SB].
  - **Not checked:** Aurora (its page hung), AlloyDB, Crunchy Bridge (404), Tiger Cloud, EDB BigAnimal, Aiven, DigitalOcean, Heroku.
- **No managed Postgres offers GPU instance types** on any page fetched [MP].
- **What managed users can do:**
  - Point `postgres_fdw` at an external server: allowed on RDS, Azure and Neon; Cloud SQL only within the same VPC [MP].
  - Stream CDC out: pglogical and wal2json are on the RDS, Azure and Cloud SQL lists [MP].
  - So they can feed a sidecar but cannot host the extension. Seen from their seat, the sidecar is a separate warehouse, which is what this option promised to avoid.
- **Who could adopt the full extension:**
  - Self-managed Postgres on VMs or bare metal with a GPU attached, or Kubernetes operators with a custom image on GPU nodes (unverified).
  - A provider who partners: EDB (already markets GPU), TigerData, or Snowflake's pg_lake team.
  - This is a small slice. Most new Postgres runs managed (estimate).

## Data path and cache

| Stage | Rate | Source |
|---|---|---|
| Postgres heap scan through the executor | ~0.5–1 GB/s per core (estimate); pg_duckdb uses 2+2 workers by default | [DK] |
| PG-Strom NVMe → GPU, heap blocks | 7.3–7.9 GB/s on 3 NVMe; ~40 GB/s on 16 NVMe with 4×V100 | [PS5] |
| Logical-replication initial copy | ~10 MB/s per stream; ~88 MB/s at 32 streams | [PDB] (estimate, derived) |
| CDC steady state | 1M rows/s × 100 B = 0.1 GB/s (estimate) | — |
| NIC to a sidecar | 25–100 GbE = 3–12 GB/s (estimate) | — |
| Parquet decode on GPU (H200) | 6.8–8.7 GB/s materialized | [I] |
| PCIe H2D (H200) | 10.6 GB/s pageable, 47.3 GB/s pinned | [I] |
| HBM read (H200) | 4,484 GB/s | [I] |

- **Thesis holds, more strongly than for most options.** Without a cache, every query reads heap at executor speed: about 1 GB/s per core, 4–8 GB/s with parallel workers (estimate). The GPU's 4,484 GB/s is idle 99% of the time. PG-Strom avoids this only with NVMe P2P and freshly vacuumed tables, and then gains about 3–5× over vanilla Postgres [PS5].
- **The cache needed is a CDC-maintained columnar mirror, VRAM first, pinned host RAM second.**
  - CDC ingest (≤0.1 GB/s) is far below PCIe, so keeping it fresh is cheap.
  - The cost is the one-time snapshot: 1 TB takes about 3 h at 88 MB/s (estimate).
  - Restart needs a persisted mirror (Parquet) or a new snapshot.
- **Capacity fits Postgres-sized data.** Most Postgres analytic sets are 10 GB–1 TB (estimate). An L40S (48 GB) or H100 (80 GB) holds about 130–220 GB at the ~2.8× compression Sirius measured [S].
- **But Postgres-sized data is where the GPU's edge is thinnest.** pg_duckdb on a 32-core box already runs ClickBench-class queries in seconds or less (estimate). Sirius was slower than DuckDB on all 22 TPC-H queries on desk GPUs ([sirius #609](https://github.com/sirius-db/sirius/issues/609)) [D].

## Build cost to alpha

Sizes are in yardstick lines; the unit is 22,830 [I]. Every figure is an estimate.

| Component | On peacockdb: reuse/adapt/new, size | On Sirius: reuse/adapt/new, size |
|---|---|---|
| Postgres extension: planner hook, deparse, CustomScan result node, fallback | adapt a fork of pg_duckdb's Postgres side (MIT, 13.8k non-vendored lines): swap DuckDB for a Flight client, 3–5k | reuse pg_lake as is (Apache-2.0) |
| GPU server process: Flight SQL, sessions, one CUDA context, admission | new: 2–3k (no server exists [I]) | adapt `pgduck_server` to load Sirius: 0.5–1k |
| Postgres SQL semantics in DataFusion: numeric, timestamptz, collations, functions, window functions (#143) | new: 3–5k | reuse (pg_lake already maps Postgres to DuckDB) |
| Heap → mirror via logical replication: snapshot copy, pgoutput decode, update/delete, LSN fence | new: 4–7k | new: 3–5k, or require `USING iceberg` tables for the alpha: 0 |
| GPU-resident mirror cache: delta, delete bitmap, sealing, eviction | new: 3–5k | adapt `pin_table` to refresh on mirror/Iceberg change (pins go stale today, #106): 2–3k |
| In-memory Arrow source node in the engine | adapt: 0.5–1k | reuse |
| Iceberg and object store for external tables | new: 2–3k (overlaps lakehouse-iceberg) | reuse |
| Allocator pool, accounting, concurrency | adapt: 1–2k (#148) | adapt: 0.5–1k |
| Tests: Postgres oracle on the corpus harness | adapt: 1–2k | adapt: ~1k |
| **Total** | **~19.5–33k** | **~7–11k** |

Total new work: 1.1× engine on peacockdb, 0.4× on Sirius (estimate)

- **The hard part:** getting Postgres semantics right on a non-Postgres engine, with a mirror that is consistent:
  - every Postgres type, function and NULL/collation rule the users' queries touch;
  - an LSN-fenced snapshot;
  - updates and deletes applied correctly, so wrong rows never come back silently.
  - PG-Strom's 19 wrong-result reports show the cost [PS1]. Sirius has two known silent wrong-row bugs [S].
- **On Sirius, the hard part is the two version pins.** pg_lake (DuckDB 1.5.6) and Sirius (exact DuckDB revision, about 50–60k lines of churn a week) must move together [S][PL].

## Adoption friction

- **High.** The extension needs superuser, `shared_preload_libraries`, `wal_level=logical` and a GPU on the Postgres host or beside it.
- **Managed Postgres excludes the in-process route** [MP]. Most Postgres teams cannot install it at all.
- **The managed-friendly route (FDW or CDC into a sidecar) is a separate system to run.** That is the "separate warehouse" the pitch says users avoid. And postgres_fdw cannot push down window functions or DISTINCT [PG].
- **Teams that live in Postgres rarely own GPUs** (estimate). Getting a GPU box approved for a Postgres analytics speed-up competes with simply adding pg_duckdb, ClickHouse or a bigger instance.
- **The upside:** for self-managed users it needs no migration and no new SQL, just as pg_duckdb does.

## Platform risk

- **The host vendors own the slot.** Snowflake (pg_lake), Databricks (Neon, Mooncake), Google (AlloyDB columnar), Microsoft (Citus), TigerData (hypercore) and EDB (PGAA) each control analytics on their own Postgres [LS].
- **EDB is NVIDIA's named Postgres partner.** It already sells "GPU Postgres" via Spark RAPIDS [EDB4][N]. Moving that onto Sirius or cuDF in-process is a small step for them (estimate).
- **NVIDIA's pattern is acceleration in place through the host vendor** [N]:
  - Sirius is going into DuckDB core ([duckdb #28022](https://github.com/duckdb/duckdb/pull/28022), 2026-10-28) [D].
  - Once Sirius ships with DuckDB, pg_duckdb and pg_lake users get a GPU path for free, from the vendors (estimate).
- **PG-Strom is weak, but it is free (PostgreSQL License) and first in the Postgres community's mind** [PS1].

## Funding story

- **What a design partner would say** (estimate): "We run self-managed Postgres with 200 GB of hot tables and dashboards time out. We tried pg_duckdb, and it helped." The GPU has to beat that second sentence, on hardware they don't own.
- **What the round would be raised on:** "the GPU analytics layer for the largest database community", with exits like Crunchy and Neon (CPU, sizes unverified) as comparables.
- **The weakness:** those exits were platform on-ramps bought for distribution, not for engine speed. A GPU layer has no distribution of its own on managed Postgres.
- **The closest real asset would be a partnership** (EDB, TigerData or a sovereign-cloud Postgres host), not a user. The brief rules this out as the first-user story.

## Evidence against

- **Demand is weak and first-hand pain is absent.** 3 curious posters in three years [D]. The incumbent has no named customers after 9 years. It is removing its hot-table cache as "rarely used" [PS4].
- **The CPU answer is good enough and free.** pg_duckdb, pg_lake, hypercore, AlloyDB and Citus already speed up Postgres analytics by large factors. At Postgres sizes (≤1 TB), GPU-vs-DuckDB gains are small or negative ([sirius #609](https://github.com/sirius-db/sirius/issues/609)) [D].
- **Most Postgres runs where no GPU extension can be installed and no GPU exists** [MP].
- **Hot heap tables defeat direct GPU reads** (not all-visible) [PS2]. So the product must be a mirror, which makes it a replicated warehouse with extra steps.
- **The cheapest version (Sirius in pg_lake) is so cheap that its vendors or NVIDIA can ship it** [PL][S].
- **EDB already owns the "Postgres + NVIDIA" headline** [EDB4].

## Ratings

- **Who needs it badly — weak:** demand.md's rating. 3 curious posters, and the PG-Strom tracker is mostly maintainers [D].
- **Build cost — medium:** 1.1× on peacockdb fits but spends most of it on Postgres semantics and CDC. 0.4× on Sirius is easy but rests on two moving version pins.
- **Data path — medium:** heap reads would starve the GPU, so a CDC mirror is mandatory. Once the mirror is resident it is fast, and fresh writes are cheap. The snapshot takes hours per TB.
- **Adoption friction — weak:** managed Postgres forbids the extension and has no GPUs. The sidecar route is the separate system users wanted to avoid.
- **Platform risk — weak:** EDB+NVIDIA, Snowflake, Databricks, Google, Microsoft and TigerData own Postgres analytics. Sirius-in-DuckDB reaches pg_duckdb and pg_lake for free.
- **Funding story — weak:** there is no user in pain. Comparable exits were CPU distribution plays, and the best lead is a vendor partnership.

## Follow-up experiments

1. **Sirius inside pg_lake's `pgduck_server`.** Build Sirius against DuckDB v1.5.6 (`069cc9f9`) and load it unsigned. Run TPC-H SF100 on Iceberg tables, against pg_lake on CPU on the same box. This settles the Sirius cost line and the size of the GPU gain.
2. **pg_duckdb versus a GPU mirror at Postgres sizes.** Use SF10–SF100 heap tables on a 32-core box plus one L40S or H100. Measure ms per query, warm. If the GPU is not ≥5× faster, the option dies.
3. **Heap scan GB/s per core.** Measure Postgres 18 seq scan and pg_duckdb's postgres_scan on all-visible versus freshly updated pages. This replaces the 0.5–1 GB/s estimate.
4. **Logical-replication throughput into Arrow.** Measure one pgoutput walsender at sustained OLTP load (rows/s, lag), and a parallel ctid-range snapshot of 1 TB.
5. **PG-Strom v6.1 on one H100 with 4 NVMe, heap versus Arrow, against pg_duckdb.** This is the incumbent's real margin over the CPU answer.

## Sources

- [D] demand.md `## Ratings`, `## What people ask for` (local; accessed 2026-10-29); [I] inventory.md at master f25c250d (local; accessed 2026-10-29); [S] sirius.md, Sirius @ b9e05ffc653c8ed031c789e5c1415ac1186e0619 (local; accessed 2026-10-29); [N] nvidia.md (local; accessed 2026-10-29)
- [PS1] https://github.com/heterodb/pg-strom @ 29349cbfbc27ab7c12952f4880f810ae7d7b39b4: README, LICENSE, src/relscan.c, src/xpu_common.h, issues and commits via GitHub API; licence change https://github.com/heterodb/pg-strom/commit/21304d38988fb2af24cc3a384ed59dffcc8c60ee (accessed 2026-10-29)
- [PS2] https://heterodb.github.io/pg-strom/ssd2gpu/ ; [PS3] https://heterodb.github.io/pg-strom/gpucache/ ; [PS8] https://heterodb.github.io/pg-strom/install/ (accessed 2026-10-29)
- [PS4] https://github.com/heterodb/pg-strom/blob/29349cbfbc27ab7c12952f4880f810ae7d7b39b4/man/release_v6.2.md (draft v6.2 notes, L183-194) (accessed 2026-10-29)
- [PS5] https://speakerdeck.com/kaigai/data-processing-more-than-billion-rows-per-second-unleash-full-capability-of-gpu-and-nvme (2020-10-06) (accessed 2026-10-29)
- [PS6] https://speakerdeck.com/kaigai/ri-ben-fa-noossbao-su-db-pg-strom-kai-fa-noli-shi-tozhan-wang (2025-09-06); [PS7] https://speakerdeck.com/kaigai/pg-strom-v6-dot-0-xin-ji-neng-tosonoxian-nowei-lai (2025-05-23) (accessed 2026-10-29)
- [PS9] https://github.com/heterodb/pg-strom/issues/796 ; fix 5e7b418c07d963c1047bd57d1f4be5cb561e7da5 (accessed 2026-10-29)
- [PS10] https://heterodb.github.io/pg-strom/release_v5.0/ , https://heterodb.github.io/pg-strom/release_v6.1/ (accessed 2026-10-29)
- [DK] https://github.com/duckdb/pg_duckdb @ a9457c9cb3b7f2a8d189bbe017f3488ef4294a1d: src/pgduckdb_hooks.cpp, src/scan/postgres_table_reader.cpp, docs/settings.md, docs/extensions.md; DuckDB submodule 08e34c447bae34eaee3723cac61f2878b6bdf787 = tag v1.5.4 (accessed 2026-10-29)
- [PL] https://github.com/Snowflake-Labs/pg_lake @ 1918df64e9655671036037051c8507949a953595: README, duckdb_pglake/Makefile (DUCKDB_VERSION=1.5.6), submodule 069cc9f9b5be802405797faecc284961b07c70ef = tag v1.5.6, pgduck_server/src/duckdb/duckdb.c:307-330 (accessed 2026-10-29)
- [MC] https://github.com/Mooncake-Labs/pg_mooncake @ b7c67e0a08c11c45c4ac5cb5ec5ef231779c1db4; https://www.mooncake.dev/blog → 301 to databricks.com (accessed 2026-10-29)
- [PA] https://github.com/paradedb/pg_analytics @ 5eef8022aab4c98ec847926e37492365b49ba99c (archived) (accessed 2026-10-29)
- [HY] https://github.com/hydradatabase/columnar @ d1f7657ee82265b2b9f1c06fda929bb5b942411b (accessed 2026-10-29); [CI] https://github.com/citusdata/citus @ 843ef6f04959a0e73cc851c8a6c6436e99178702 (accessed 2026-10-29)
- [AL] https://docs.cloud.google.com/alloydb/docs/columnar-engine/about (accessed 2026-10-29); [TS] https://www.tigerdata.com/docs/use-timescale/latest/hypercore (accessed 2026-10-29)
- [LS] landscape research notes, scratchpad `research_notes/Postgres GPU analytics offload/pg_analytics_landscape.md` (GitHub API pulls, accessed 2026-10-29). The acquisition facts (Snowflake–Crunchy 2025-06, Databricks–Neon 2025-05, Databricks–Mooncake 2025-10) are recalled and could not be re-fetched: https://www.snowflake.com/en/news/press-releases/snowflake-to-acquire-crunchy-data-to-bring-enterprise-ready-postgres-offering-to-the-ai-data-cloud/ , https://www.databricks.com/company/newsroom/press-releases/databricks-agrees-acquire-neon-help-developers-deliver-ai-systems (blocked, accessed 2026-10-29)
- [EDB1] https://www.enterprisedb.com/docs/pgaa/latest/overview/architecture , https://www.enterprisedb.com/docs/edb-postgres-ai/analytics/ (accessed 2026-10-29)
- [EDB2] https://www.enterprisedb.com/docs/pgaa/latest/spark/spark-rapids ; [EDB3] https://www.enterprisedb.com/docs/pgaa/latest/release_notes/ (accessed 2026-10-29)
- [EDB4] https://www.prnewswire.com/news-releases/edb-accelerates-postgres-for-the-agentic-workforce-era-with-nvidia-ai-infrastructure-and-interactive-analytics-302715120.html (snippet only); [EDB5] https://www.enterprisedb.com/blog/edb-postgres-ai-q1-2026-release-highlights (accessed 2026-10-29)
- [PG] https://github.com/postgres/postgres/tree/cc4bf6e9acc9b9a385e37bd13bd55b5c941529cc: src/include/optimizer/paths.h, planner.h, src/backend/optimizer/plan/planner.c, contrib/postgres_fdw/postgres_fdw.c, src/backend/access/heap/heapam.c (accessed 2026-10-29)
- [MP] managed-Postgres research notes, scratchpad `research_notes/Postgres GPU analytics offload/managed_pg_and_mechanics.md` (accessed 2026-10-29)
- [RDS] https://docs.aws.amazon.com/AmazonRDS/latest/PostgreSQLReleaseNotes/postgresql-extensions.html ; [TLE] https://github.com/aws/pg_tle/blob/6340fd2ef19a806dc0f2588d6aec10a0a12f7e8b/README.md (accessed 2026-10-29)
- [CSQL] https://docs.cloud.google.com/sql/docs/postgres/extensions ; [AZ] https://learn.microsoft.com/en-us/azure/postgresql/extensions/concepts-extensions-versions (accessed 2026-10-29)
- [NEON] https://neon.com/docs/extensions/pg-extensions ; [SB] https://supabase.com/docs/guides/database/extensions (accessed 2026-10-29)
- [PDB] https://blog.peerdb.io/benchmarking-postgres-replication-peerdb-vs-airbyte (accessed 2026-10-29)
- Threads: https://stackoverflow.com/q/77960181 , https://stackoverflow.com/q/77300203 , https://github.com/sirius-db/sirius/issues/609 , https://github.com/duckdb/duckdb/pull/28022 (accessed 2026-10-29)
