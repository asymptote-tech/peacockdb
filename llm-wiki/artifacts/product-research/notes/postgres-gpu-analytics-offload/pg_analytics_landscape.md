# Postgres analytics accelerators (CPU) landscape, EDB + NVIDIA GPU announcements, and other GPU-for-Postgres efforts (as of 2026-10-29)

Method note: the session-wide WebSearch budget ran out after one search. Most findings therefore come from (a) the GitHub API via `gh` (all stars/SHA/dates were pulled live on 2026-10-29), (b) EDB docs (markdown endpoints, live), and (c) a few WebFetch calls. databricks.com, snowflake.com and hpcwire.com were blocked or unreachable from this sandbox (Cloudflare block / DNS failure). Claims marked **[TK]** come from my training knowledge (cutoff mid-2026) and were NOT re-verified this session. The URL given with them is the canonical source as I remember it, so the report writer should treat them as lower confidence. All "accessed" dates are 2026-10-29.

## Q1. Per-project sheets: mechanism, licence, activity, company outcome, speedups, users

### Takeaway
Every CPU accelerator that put a vectorized engine inside Postgres converged on DuckDB (pg_duckdb, pg_mooncake, pg_analytics, pg_lake, and Crunchy Warehouse before them) or on DataFusion (EDB PGAA/Seafowl). The companies behind them were mostly acquired by data-platform vendors in 2025: Neon and Mooncake went to Databricks, and Crunchy went to Snowflake. Some pivoted away from analytics: ParadeDB to search, and BemiDB to search/audit as "PgStack". Hydra went dormant. The projects still alive in late 2026 are those owned by a large company: pg_duckdb (DuckDB Labs/MotherDuck), pg_lake (Snowflake), Citus (Microsoft), TimescaleDB hypercore (TigerData), AlloyDB (Google) and PGAA (EDB).

### Cited Findings

**GitHub snapshot (live `gh api`, 2026-10-29)**

| repo | stars | licence (GitHub SPDX) | archived | latest commit SHA | commit date |
|---|---|---|---|---|---|
| duckdb/pg_duckdb | 3,275 | MIT | no | a9457c9cb3b7f2a8d189bbe017f3488ef4294a1d | 2026-10-28 |
| Mooncake-Labs/pg_mooncake | 2,012 | MIT | no | b7c67e0a08c11c45c4ac5cb5ec5ef231779c1db4 ("Support Postgres 18") | 2025-10-26 |
| Mooncake-Labs/moonlink | 367 | NOASSERTION | no | 401c1d1ad5354f77bf64cbc4f008c4e62a16781d | 2026-02-16 |
| paradedb/pg_analytics | 537 | PostgreSQL | **yes** | 5eef8022aab4c98ec847926e37492365b49ba99c ("Final release") | 2025-03-19 |
| paradedb/paradedb (pg_search) | 9,458 | AGPL-3.0 | no | 17e7ff568046d25d9c665935ed855a09dc133aca | 2026-10-29 |
| hydradatabase/columnar (`hydradatabase/hydra` redirects here) | 3,042 | Apache-2.0 (repo); columnar dir is AGPL-3.0 per README | no | d1f7657ee82265b2b9f1c06fda929bb5b942411b ("README updates") | 2025-02-10 |
| citusdata/citus | 12,830 | AGPL-3.0 | no | 843ef6f04959a0e73cc851c8a6c6436e99178702 | 2026-10-29 |
| timescale/timescaledb | 23,772 | NOASSERTION (Apache-2 + TSL) | no | 21b65abd9437a082be93f4b7480e2809ab14ac91 | 2026-10-29 |
| Snowflake-Labs/pg_lake | 1,665 | Apache-2.0 | no | 1918df64e9655671036037051c8507949a953595 | 2026-10-27 |
| BemiHQ/BemiDB → now pgstack-io/pgstack | 1,537 | AGPL-3.0 | no | 769643b86c284bce8034bd1cac884d580d4210c3 | 2026-10-23 |
| orioledb/orioledb | 4,287 | Apache-2.0 | no | 0fe4a2c2f217e94d83f6cf309b1916e61a92a771 | 2026-10-29 |
| xataio/xata | 1,101 | Apache-2.0 | no | f197e7bad539a81b424018765ee4e8799ed184f0 | 2026-10-23 |
| heterodb/pg-strom | 1,408 | NOASSERTION (GPL-2.0 per project) | no | 29349cbfbc27ab7c12952f4880f810ae7d7b39b4 | 2026-10-28 |
| sirius-db/sirius | 1,101 | Apache-2.0 | no | 0b183b832e2fc0d7b35faf1a997ab650a543555f | 2026-10-29 |

Sources: https://github.com/duckdb/pg_duckdb, https://github.com/Mooncake-Labs/pg_mooncake, https://github.com/Mooncake-Labs/moonlink, https://github.com/paradedb/pg_analytics, https://github.com/paradedb/paradedb, https://github.com/hydradatabase/columnar, https://github.com/citusdata/citus, https://github.com/timescale/timescaledb, https://github.com/Snowflake-Labs/pg_lake, https://github.com/pgstack-io/pgstack, https://github.com/orioledb/orioledb, https://github.com/xataio/xata, https://github.com/heterodb/pg-strom, https://github.com/sirius-db/sirius (all accessed 2026-10-29). `crunchydata/pg_lake` returns 404, so the code lives only under Snowflake-Labs.

**pg_duckdb (DuckDB Labs + MotherDuck, originally with Hydra)**
- Mechanism: embeds DuckDB in the Postgres backend. With `duckdb.force_execution`, ordinary `SELECT`s on regular heap tables run in DuckDB's engine. A planner hook replaces the whole query plan with a DuckDB custom scan node. It reads Postgres heap pages through DuckDB's Postgres scanner, and also reads Parquet/Iceberg/Delta and MotherDuck tables. README: "Built in collaboration with Hydra and MotherDuck… Out-of-the-box support of MotherDuck as compute provider… No special syntax needed" — [pg_duckdb README @ a9457c9](https://github.com/duckdb/pg_duckdb) (accessed 2026-10-29). The planner-hook/custom-scan detail is **[TK]**, from the repo's `src/pgduckdb_planner.cpp`, and was not re-read this session.
- Releases: v0.3.1 2025-02-13; v1.0.0 2025-09-04; v1.1.0 2025-12-11; v1.1.1 2025-12-18 — [releases](https://github.com/duckdb/pg_duckdb/releases) (accessed 2026-10-29). Still committing in Oct 2026, for example a macOS signal-handling fix (a9457c9, 2026-10-28).
- Users: pg_mooncake is built on top of it ("make pg_duckdb… shared_preload_libraries = 'pg_duckdb,pg_mooncake'") — [pg_mooncake README @ b7c67e0](https://github.com/Mooncake-Labs/pg_mooncake) (accessed 2026-10-29). **[TK]** Neon and Supabase listed pg_duckdb as an available extension in 2025.
- Hydra's own fork `hydradatabase/pg_duckdb` was last pushed 2025-03-11 — [hydradatabase org](https://github.com/hydradatabase) (accessed 2026-10-29).

**pg_mooncake + moonlink (Mooncake Labs → Databricks)**
- Mechanism (v0.2+): a logical-replication sidecar. It requires `wal_level = logical`. `CALL mooncake.create_table('trades_iceberg','trades')` creates a "columnstore mirror of your Postgres tables in Iceberg" that is kept in sync by moonlink (Rust), giving "sub-second freshness". Queries against the mirror run through pg_duckdb/DuckDB. The README claims "ranking top 10 on ClickBench" — [pg_mooncake README @ b7c67e0](https://github.com/Mooncake-Labs/pg_mooncake) (accessed 2026-10-29). **[TK]** v0.1 (Jan 2025) was a table access method writing Delta/Iceberg columnstore tables directly. v0.2 switched to the replication-mirror design.
- Licence MIT (pg_mooncake). Last GitHub release v0.1.2 (2025-02-12). Last commit 2025-10-26 (PG18 support). moonlink's last commit is 2026-02-16 — [gh API](https://github.com/Mooncake-Labs/pg_mooncake/releases) (accessed 2026-10-29).
- Company outcome: `https://www.mooncake.dev/blog` now returns a 301 to `https://www.databricks.com/`, so the company site has been folded into Databricks — WebFetch of [mooncake.dev/blog](https://www.mooncake.dev/blog) (accessed 2026-10-29). **[TK]** Databricks announced the acquisition of Mooncake Labs in early Oct 2025 to strengthen Lakebase (its Postgres OLTP built on Neon), feeding Postgres changes into Delta/Iceberg — [Databricks blog](https://www.databricks.com/blog/databricks-acquires-mooncake-labs). This page was blocked by Cloudflare from the sandbox, so the date is unverified.
- Activity slowed after acquisition: no pg_mooncake commit since 2025-10-26 (gh, above).

**ParadeDB pg_analytics (formerly pg_lakehouse)**
- Mechanism: an FDW per format (parquet/csv/delta/iceberg/S3 etc.) plus an executor hook that pushes the whole query down to embedded DuckDB v1.1.0. It reads only external object-store files, not heap. "uses the foreign data wrapper (FDW) API… and the executor hook API to push queries to DuckDB" — [pg_analytics README @ 5eef802](https://github.com/paradedb/pg_analytics) (accessed 2026-10-29).
- Outcome: archived. "has been discontinued and is archived… ParadeDB's work on Postgres analytics is now being done in our primary extension, pg_search… The code in this repository is no longer maintained." Final commit 2025-03-19 — [README](https://github.com/paradedb/pg_analytics) (accessed 2026-10-29).
- Pivot: paradedb/paradedb (pg_search, AGPL-3.0, 9,458 stars) is now described as "full-text search, vector retrieval, and aggregations". Analytics now means aggregations over BM25/Tantivy indexes, not a DuckDB lakehouse — [paradedb repo](https://github.com/paradedb/paradedb) (accessed 2026-10-29). **[TK]** ParadeDB raised a $12M Series A in 2025.

**Hydra columnar (Hydra, Inc.)**
- Mechanism: a table access method (tableam, PG12+) columnar store. It is a fork of Citus columnar with vectorized execution and parallel scans. "Hydra is a Postgres extension, not a fork. Hydra makes use of tableam." Columnar dir is AGPL-3.0, everything else Apache-2.0. Benchmarks reference ClickBench — [hydradatabase/columnar README @ d1f7657](https://github.com/hydradatabase/columnar) (accessed 2026-10-29).
- Outcome: dormant. The last commit to columnar was 2025-02-10 (README update), and the newest org activity was hydra-cli on 2025-05-01 — [org repo list](https://github.com/hydradatabase) (accessed 2026-10-29). The `hydradatabase/hydra` repo now redirects to `columnar`. **[TK]** In 2024–25 Hydra's commercial product moved to a pg_duckdb-based serverless analytics offering ("Hydra" on hydra.so) before going quiet. hydra.so did not respond from the sandbox, so current company status is unverified (estimate: wound down or inactive).

**Citus columnar (Microsoft)**
- Mechanism: a `USING columnar` table access method with stripes/chunk groups, compression and projection/chunk-group skipping. It is not a vectorized executor. AGPL-3.0, actively maintained (commit 843ef6f, 2026-10-29) — [citusdata/citus](https://github.com/citusdata/citus) (accessed 2026-10-29). **[TK]** It shipped in Citus 10 (2021). Hydra columnar forked from it. It is used on Azure Cosmos DB for PostgreSQL. Microsoft has not published standalone columnar speedups beyond compression ratios.

**Google AlloyDB columnar engine**
- **[TK]** Mechanism: an in-memory columnar cache of selected heap columns that is auto-populated by the engine's ML-based recommendations, and the planner uses it for scans/aggregations. It is a mirror of heap data in memory, not a storage format. Google markets "up to 100x faster analytical queries" than standard PostgreSQL. It is proprietary and part of AlloyDB / AlloyDB Omni — [AlloyDB columnar engine docs](https://cloud.google.com/alloydb/docs/columnar-engine/about). The fetch timed out from the sandbox, so this is unverified.

**Timescale → TigerData, hypercore**
- timescaledb has 23,772 stars and is very active (commit 21b65ab, 2026-10-29). Licence: Apache-2.0 core plus Timescale License (TSL) for compression/hypercore features (GitHub reports NOASSERTION) — [timescale/timescaledb](https://github.com/timescale/timescaledb) (accessed 2026-10-29).
- **[TK]** Mechanism: hypercore is a hybrid row/columnar engine. Recent chunks are heap rows and older chunks are converted into compressed columnar batches. It is exposed as a table access method (TimescaleDB 2.18, early 2025) and has vectorized aggregation over compressed batches. Timescale renamed itself TigerData in June 2025 — [hypercore docs](https://www.tigerdata.com/docs/use-timescale/latest/hypercore). This fetch timed out from the sandbox.

**Crunchy Data Warehouse / Crunchy Bridge for Analytics → Snowflake; pg_lake**
- pg_lake mechanism: Postgres extensions plus a separate `pgduck_server` process that "implements the Postgres wire-protocol (locally), and underneath uses DuckDB to execute queries". Tables are `USING iceberg` (a table access method on Iceberg in S3) or foreign tables over Parquet/CSV/JSON. It lets you "Combine heap, Iceberg, and external Parquet/CSV/JSON files in the same SQL queries… with full transactional guarantees" — [pg_lake README @ 1918df6](https://github.com/Snowflake-Labs/pg_lake) (accessed 2026-10-29). So execution is in a DuckDB sidecar process, not inside the backend, which differs from pg_duckdb.
- Licence Apache-2.0. The repo was created 2025-11-04. Releases are frequent (v3.5.7 to v3.5.10, 12–27 Oct 2026) — [gh API](https://github.com/Snowflake-Labs/pg_lake/releases) (accessed 2026-10-29).
- **[TK]** Snowflake announced its acquisition of Crunchy Data on 2 Jun 2025 (reported around $250M) to create "Snowflake Postgres". pg_lake is the open-sourced code of Crunchy Data Warehouse / Bridge for Analytics, released Nov 2025 — [Snowflake press release](https://www.snowflake.com/en/news/press-releases/snowflake-to-acquire-crunchy-data-to-bring-enterprise-ready-postgres-offering-to-the-ai-data-cloud/). It returned 404 / was blocked from the sandbox, so the date is unverified. The repo creation date (2025-11-04) matches the open-sourcing timing.
- The Crunchy blog continues under its own brand (posts through 2 Sep 2026, e.g. "Postgres 19 Compression"). The blog list shows no analytics/warehouse posts in 2025-26 — [crunchydata.com/blog](https://www.crunchydata.com/blog) (accessed 2026-10-29).

**Neon / Supabase**
- **[TK]** Databricks agreed to acquire Neon in May 2025 (reported around $1B). Neon became the base of "Databricks Lakebase" (OLTP Postgres). Analytics there comes through syncing into Delta/Unity Catalog, and Mooncake was acquired for this path — [Databricks press release](https://www.databricks.com/company/newsroom/press-releases/databricks-agrees-acquire-neon-help-developers-deliver-ai-systems). It was blocked by Cloudflare from the sandbox.
- **[TK]** Supabase acquired the OrioleDB team (2024). OrioleDB is a row-oriented storage engine (TAM + patched PG), not columnar. Supabase analytics relies on extensions (pg_duckdb was offered) and on ETL to Iceberg ("Supabase ETL / analytics buckets", 2025). OrioleDB is active (0fe4a2c, 2026-10-29) — [orioledb/orioledb](https://github.com/orioledb/orioledb) (accessed 2026-10-29).

**Others**
- BemiDB has been renamed and pivoted. `BemiHQ/BemiDB` now redirects to `pgstack-io/pgstack`: "Postgres read replicas for Search and Audit… Indexing and queries run outside your application database through a separate Postgres-compatible endpoint" (AGPL-3.0, v2.2.0 2026-10-23) — [pgstack README](https://github.com/pgstack-io/pgstack) (accessed 2026-10-29). **[TK]** BemiDB (2024–25) was a logical-replication read replica that wrote Iceberg and was queried with embedded DuckDB over the PG wire protocol. The analytics positioning has been dropped.
- Xata: the repo is now "cloud native Postgres platform with copy-on-write branching and scale-to-zero". It has no analytics engine — [xataio/xata](https://github.com/xataio/xata) (accessed 2026-10-29).
- **[TK]** Epsio does incremental materialized-view maintenance via a CDC sidecar. It is not a columnar or vectorized scan accelerator. Its site was not reachable from the sandbox.
- EDB PGAA is EDB's CPU analytics accelerator: a planner hook plus the Seafowl (DataFusion) engine over Arrow Flight. See Q2.

### Inferences
- Taxonomy for the report:
  - (1) Whole-query takeover via planner/executor hook into an embedded engine: pg_duckdb, pg_analytics, PGAA DirectScan.
  - (2) FDW over lake files: pg_analytics, pg_lake foreign tables, PGAA.
  - (3) Table access method columnar: Citus, Hydra, hypercore, pg_mooncake v0.1, pg_lake `USING iceberg`.
  - (4) Logical-replication sidecar into a columnar or Iceberg mirror: pg_mooncake v0.2/moonlink, BemiDB, Databricks Lakebase sync.
  - (5) In-memory columnar cache of heap: AlloyDB.
- Pattern (estimate): standalone "analytics-on-Postgres" startups did not survive as independent analytics vendors through 2025-26. Their value was absorbed by Databricks and Snowflake (as on-ramps into their lakehouses), or the startups pivoted (ParadeDB, BemiDB) or went dormant (Hydra). DuckDB is the de facto CPU engine. Only EDB uses a non-DuckDB engine (DataFusion/Seafowl).
- Real-user evidence in primary sources is thin. Published speedups are vendor-run ClickBench numbers (Hydra, pg_mooncake "top 10") or marketing figures (AlloyDB 100x [TK]).

### Gaps
- Exact acquisition dates and prices (Neon/Databricks, Crunchy/Snowflake, Mooncake/Databricks) could not be re-verified: search budget was exhausted and databricks.com/snowflake.com were blocked. They are marked [TK].
- Current Hydra, Inc. status (shutdown vs. dormant) is unverified.
- AlloyDB and hypercore speedup figures were not re-fetched.
- Named production users for pg_duckdb, pg_lake and pg_mooncake were not found in this session.

## Q2. What EDB announced with NVIDIA (GTC 2026): what runs on GPU, shipped vs roadmap, dates

### Takeaway
EDB's GPU story is not GPU execution inside Postgres. PGAA, a Postgres extension whose planner hook offloads queries on Parquet/Iceberg/Delta lake tables, can route queries via Spark Connect to a remote Apache Spark cluster running the NVIDIA RAPIDS Accelerator (cuDF). GPU support shipped as a PGAA feature before GTC and was promoted at GTC on 16–17 Mar 2026. It is currently limited to read-only queries over Parquet in S3 or POSIX storage. EDB claims "50–100x" and "up to 91x" speedups without published benchmark details.

### Cited Findings
- PGAA architecture: "The PGAA extension hooks into the Postgres query planner to identify queries that can be offloaded… Seafowl is the default, DataFusion-based query engine… The Postgres instance communicates with the Seafowl engine using Apache Arrow Flight." It has two execution paths. In DirectScan the engine does all filtering, joins and aggregation and returns only results. In CompatScan the engine scans/filters and Postgres finishes the query (used for unsupported SQL or for joins with local heap tables) — [PGAA architecture](https://www.enterprisedb.com/docs/pgaa/latest/overview/architecture) (accessed 2026-10-29); [Optimizing query performance](https://www.enterprisedb.com/docs/pgaa/latest/optimize_performance/) (accessed 2026-10-29). EXPLAIN shows nodes `SeafowlDirectScan`, `SparkDirectScan`, `SeafowlCompatScan` and `SparkCompatScan` (same source).
- Data sources: PGAA reads lake files (Parquet/Delta/Iceberg) in object storage, not heap. Heap data gets there via "Replicate with PGD… automated partition offloading" (tiered storage) — [EDB Postgres AI analytics docs](https://www.enterprisedb.com/docs/edb-postgres-ai/analytics/) (modified 2026-08-07, accessed 2026-10-29).
- GPU execution: `pgaa.executor_engine` selects between Seafowl (in-process alongside Postgres), Spark Connect on CPU, or "Spark + RAPIDS (GPU)… Remote Spark with NVIDIA GPUs… Best for massive joins & aggregations" — [Accelerating with Spark](https://www.enterprisedb.com/docs/pgaa/latest/spark/) (accessed 2026-10-29).
- RAPIDS page:
  - "completing complex queries up to 100x faster than standard Postgres tables". "While Spark manages distribution at 100 GB, RAPIDS becomes the primary performance driver at 3 TB and beyond".
  - Unsupported operations fall back automatically to Spark CPU or Postgres.
  - **Limitation:** "Currently, GPU acceleration with Apache Spark is optimized for read-only queries on Parquet files in S3-compatible object storage or a shared POSIX filesystem. Support for Iceberg (read/write) is coming soon."
  - Requirements: PG16+, Spark 3.4+, RAPIDS 25.10 (`rapids-4-spark_2.12:25.10.0`). Demo stack at github.com/EnterpriseDB/spark-rapids-tutorial, aimed at NVIDIA Brev.
  - Source: [GPU-accelerated Spark with NVIDIA RAPIDS](https://www.enterprisedb.com/docs/pgaa/latest/spark/spark-rapids) (accessed 2026-10-29).
- Shipped status: the PGAA release notes list "GPU-accelerated analytics: Support for the Spark RAPIDS plugin allows you to leverage GPU acceleration… when using the Spark Connect executor" in a release older than 1.6.0 (4 Feb 2026). PGAA 1.9.0 (7 May 2026) extended Spark Connect "with optional RAPIDS GPU acceleration" to PGAA on WarehousePG (MPP). Latest version is 1.11.0 (28 Aug 2026) — [PGAA release notes](https://www.enterprisedb.com/docs/pgaa/latest/release_notes/) (accessed 2026-10-29). The exact version number of the RAPIDS entry was not extracted (it is at line ~184 of the notes, below 1.6.0).
- Press release (16 Mar 2026, PR Newswire; republished 17 Mar on HPCwire/BigDATAwire): "expanded integrations with NVIDIA cuDF for Apache Spark that accelerates Postgres on NVIDIA AI infrastructure". "PG AI Analytics Engine offloads analytical workloads to GPUs, enabling 50–100x faster, predictable analytics on large datasets (3TB+)". The release also pitched GPU workload isolation and Iceberg lakehouse support — [PR Newswire](https://www.prnewswire.com/news-releases/edb-accelerates-postgres-for-the-agentic-workforce-era-with-nvidia-ai-infrastructure-and-interactive-analytics-302715120.html); [BigDATAwire](https://www.hpcwire.com/bigdatawire/this-just-in/edb-accelerates-postgres-with-nvidia-ai-infrastructure-and-interactive-analytics/). Both were seen only as search snippets; direct fetches failed (ECONNRESET/DNS), accessed 2026-10-29.
- EDB Q1 2026 blog (31 Mar 2026): "at NVIDIA GTC… we unveiled deep integration between our PGAA extension and NVIDIA RAPIDS for Apache Spark… up to 91x faster analytical queries on live operational data". It gives no baseline, dataset or hardware. The McKnight benchmark on the same page covers WarehousePG, not the GPU path — [EDB blog](https://www.enterprisedb.com/blog/edb-postgres-ai-q1-2026-release-highlights) (accessed 2026-10-29).
- NVIDIA side: a search snippet of NVIDIA's GTC 2026 live blog says EDB Postgres AI is among enterprise data platforms supporting cuDF and cuVS. My fetch of the first 100k characters of the page did not contain "EDB" (the remaining 52k were not read). NVIDIA's own cuDF number is generic Spark ("up to 5x faster query performance… 10 TB" vs CPU, 16 Mar 2026), not EDB-specific — [NVIDIA GTC 2026 live blog](https://blogs.nvidia.com/blog/gtc-2026-news/) (accessed 2026-10-29).
- EDB analytics product line in 2026: PGAA, WarehousePG (open-source Greenplum-derived MPP), and "EDB Postgres AI for ClickHouse" — [EDB analytics docs](https://www.enterprisedb.com/docs/edb-postgres-ai/analytics/) (accessed 2026-10-29). No DuckDB is mentioned in PGAA docs. The engine is Seafowl/DataFusion (EDB acquired Splitgraph/Seafowl, 2023 [TK]).

### Inferences
- On GPU, only the remote Spark executors' physical operators (scan/filter/join/aggregate via RAPIDS/cuDF) run, and only over Parquet. Postgres itself, Seafowl and heap tables never touch the GPU. Postgres acts as a SQL front-end and orchestrator over Spark Connect.
- cuVS appears only in NVIDIA's ecosystem list. I found no EDB doc describing cuVS in pgvector/EDB products (estimate: roadmap or partner-badge level only).
- The 91x/50–100x numbers are probably GPU Spark on ≥3 TB Parquet vs. plain Postgres heap tables (estimate, based on the docs phrase "faster than standard Postgres tables"). That is not an apples-to-apples comparison against a CPU columnar engine.

### Gaps
- Exact PGAA version and date when RAPIDS support first shipped (pre-Feb-2026).
- Whether NVIDIA's GTC blog explicitly names EDB: the snippet says yes, but my partial fetch didn't confirm it.
- No published EDB benchmark methodology for 91x / 50–100x.

## Q3. Other GPU efforts for Postgres besides PG-Strom, and how they fared

### Takeaway
PG-Strom remains the only maintained in-Postgres GPU executor, with active commits in Oct 2026. Other GPU work around Postgres either died (PGOpenCL, Alenka, Brytlyt-on-Postgres) or happens outside Postgres. Examples of the outside approach are Sirius (a GPU engine for DuckDB, very active) and EDB's Spark RAPIDS route. I found no maintained Postgres + cuDF FDW and no Sirius-in-pg_duckdb integration in this session.

### Cited Findings
- PG-Strom (HeteroDB): custom-scan GPU executor plus GPU-Direct SQL. Active, with commit 29349cb on 2026-10-28 ("pgstrom.gpucache_info() returned wrong JSON string", issue #1065). 1,408 stars — [heterodb/pg-strom](https://github.com/heterodb/pg-strom) (accessed 2026-10-29).
- Sirius: "GPU-native composable analytics engine", Apache-2.0, 1,101 stars, commit 0b183b8 on 2026-10-29 (GPU aggregate FILTER fix) — [sirius-db/sirius](https://github.com/sirius-db/sirius) (accessed 2026-10-29). **[TK]** It plugs into DuckDB (and Doris) via Substrait/extension. Because pg_duckdb embeds DuckDB, a pg_duckdb→Sirius path is technically plausible (estimate). I found no public repo or blog demonstrating it.
- **[TK]** Brytlyt began as a GPU database built on PostgreSQL (BrytlytDB, around 2016–2019) and later repositioned around GPU visual analytics/AI. Current status unverified.
- **[TK]** Alenka (GPU SQL engine, GitHub antonmks/Alenka) has been inactive since around 2015–2016. PGOpenCL (Tim Child, around 2010–2011) was a PL/OpenCL experiment that was never maintained. Neither was re-checked this session.
- Not checked: generic "pg_gpu" and Postgres + cuDF FDW GitHub projects (GitHub search was not run because of the tool budget).

### Inferences
- The surviving GPU strategies for Postgres are: (a) PG-Strom's in-process custom scan (niche, one vendor); (b) offloading to an external GPU engine over lake files (EDB→Spark RAPIDS); and (c) GPU engines for DuckDB (Sirius) that could be reached through DuckDB-in-Postgres extensions. No one has shipped (c) as of 2026-10-29 as far as I found (estimate).

### Gaps
- GitHub code search for `cudf fdw postgres`, `pg_gpu`, and Sirius + pg_duckdb was not performed (budget).
- Brytlyt, Alenka and PGOpenCL statuses are from training knowledge only.
