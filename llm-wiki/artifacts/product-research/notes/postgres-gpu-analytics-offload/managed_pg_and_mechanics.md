# Managed Postgres policies and Postgres-side mechanics for GPU analytics offload

Research conditions: the session-wide WebSearch budget was used up after one query. Everything below comes from fetching URLs I already knew and from pulling source over git/curl. postgresql.org was unreachable (ECONNRESET / DNS failure) from this sandbox, so PG claims cite source code at a pinned SHA, not the docs. Many vendors (Aurora, AlloyDB, Supabase GPU, Tembo, Aiven, DO, Heroku, Tiger, EDB, Xata, Nile, K8s operators) could not be checked and are listed under Gaps. Items marked "(prior knowledge, unverified)" are things I believe but did not re-confirm this session.

Pinned SHAs (from `git ls-remote`, accessed 2026-10-29):
- postgres REL_18_STABLE `cc4bf6e9acc9b9a385e37bd13bd55b5c941529cc`
- duckdb/pg_duckdb HEAD `a9457c9cb3b7f2a8d189bbe017f3488ef4294a1d`
- Mooncake-Labs/pg_mooncake HEAD `b7c67e0a08c11c45c4ac5cb5ec5ef231779c1db4`
- heterodb/pg-strom HEAD `29349cbfbc27ab7c12952f4880f810ae7d7b39b4`
- Snowflake-Labs/pg_lake HEAD `1918df64e9655671036037051c8507949a953595`
- aws/pg_tle HEAD `6340fd2ef19a806dc0f2588d6aec10a0a12f7e8b`

## Part A — Can managed Postgres customers run a third-party GPU analytics extension, or a sidecar?

### Takeaway
None of the providers I could verify let customers load an arbitrary C extension. AWS RDS, Cloud SQL, Azure Flexible Server and Neon all run fixed allow-lists. On RDS the only route to user-defined extensions is pg_tle, which takes trusted languages only. No verified provider lists pg_duckdb, citus or columnar on its main managed-Postgres SKU. Neon lists only pg_mooncake 0.1.3, as experimental. On the sidecar side, postgres_fdw is on every list checked, though Cloud SQL limits its targets. pglogical and wal2json are on RDS, Azure and Cloud SQL, so CDC out is feasible there. The practical managed-cloud route is (b), a sidecar fed by CDC or queried through FDW. Route (a), an in-process extension, needs either the vendor's partnership or self-managed Postgres.

### Cited Findings
**AWS RDS for PostgreSQL**
- RDS extension tables for PG 18.6 and 17.11 do not list pg_duckdb, pg_mooncake, pg_analytics, citus or columnar. — [RDS PG extension versions](https://docs.aws.amazon.com/AmazonRDS/latest/PostgreSQLReleaseNotes/postgresql-extensions.html) (accessed 2026-10-29)
- Listed versions on PG 18: postgres_fdw 1.2, mysql_fdw 2.9.3, oracle_fdw 2.8.0, tds_fdw 2.0.5, pglogical 2.4.8, pg_tle 1.5.2, pgvector 0.8.2. plrust 1.2.7 is listed on PG 17 only, not on 18. — [same page](https://docs.aws.amazon.com/AmazonRDS/latest/PostgreSQLReleaseNotes/postgresql-extensions.html) (accessed 2026-10-29)
- The page frames supported extensions as the ones AWS lists per engine version. pg_tle is the documented way to build your own extensions "in a safe environment". — [same page](https://docs.aws.amazon.com/AmazonRDS/latest/PostgreSQLReleaseNotes/postgresql-extensions.html); [RDS user guide, extensions](https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/Appendix.PostgreSQL.CommonDBATasks.Extensions.html) (accessed 2026-10-29)
- pg_tle is meant for "environments that do not provide access to the filesystem" and is built on PostgreSQL "trusted languages", which restrict filesystem access and some networking. Its examples are PL/pgSQL, JavaScript and Perl. — [aws/pg_tle README @6340fd2e](https://github.com/aws/pg_tle/blob/6340fd2ef19a806dc0f2588d6aec10a0a12f7e8b/README.md) (accessed 2026-10-29)

**Google Cloud SQL for PostgreSQL**
- "You can install only the extensions that Cloud SQL supports" and "You cannot create your own extensions in Cloud SQL." — [Cloud SQL PG extensions](https://docs.cloud.google.com/sql/docs/postgres/extensions) (accessed 2026-10-29)
- pg_duckdb, citus, columnar, mysql_fdw and pg_tle are not listed. postgres_fdw 1.1, oracle_fdw 1.2 and pglogical (2.4.6 on PG 18) are. — [same](https://docs.cloud.google.com/sql/docs/postgres/extensions) (accessed 2026-10-29)
- postgres_fdw targets must be Cloud SQL private-IP instances in the same VPC, or databases on the same instance. Only `cloudsqlsuperuser` can own the FDW. Private Service Connect setups need PSC outbound enabled. — [same](https://docs.cloud.google.com/sql/docs/postgres/extensions) (accessed 2026-10-29)

**Azure Database for PostgreSQL Flexible Server**
- The full allow-list (doc dated 2026-07-10, git commit `4a5f98633b8b14f3bd24894079c899176543a53d`) has no pg_duckdb, pg_mooncake, citus, columnar or pg_tle. It does include postgres_fdw 1.2 (PG 18), oracle_fdw, tds_fdw, dblink, pglogical 2.4.6, wal2json 2.6, timescaledb 2.24.0, azure_storage 1.9, pg_ivm and pg_failover_slots (the last not supported on PG 17/18). — [Azure Flexible Server extensions list](https://learn.microsoft.com/en-us/azure/postgresql/extensions/concepts-extensions-versions) (accessed 2026-10-29)
- postgres_fdw has "special considerations" on a linked page that I did not fetch. — [same](https://learn.microsoft.com/en-us/azure/postgresql/extensions/concepts-extensions-versions) (accessed 2026-10-29)

**Neon (Databricks)**
- pg_mooncake 0.1.3 is supported on PG 14–17 but not PG 18. It is experimental, needs `SET neon.allow_unstable_extensions='true'`, and Neon recommends running it in a separate project. — [Neon supported extensions](https://neon.com/docs/extensions/pg-extensions) (accessed 2026-10-29)
- pg_duckdb, pg_analytics, citus and columnar are not on the page. postgres_fdw is supported (1.2 on PG 18). file_fdw is not supported. — [same](https://neon.com/docs/extensions/pg-extensions) (accessed 2026-10-29)
- Requests for new extensions go through support or Discord with no guarantee. Neon says it does "not actively maintain third-party extension code". — [same](https://neon.com/docs/extensions/pg-extensions) (accessed 2026-10-29)

**Supabase**
- The extensions page covers only pure-SQL and plpgsql extensions, plus database.dev as a package manager. It doesn't mention custom C extensions, pg_tle, pg_duckdb, citus, columnar or GPUs. — [Supabase extensions](https://supabase.com/docs/guides/database/extensions) (accessed 2026-10-29)

**Crunchy Bridge (Snowflake)**
- The extensions URL I tried, docs.crunchybridge.com/extensions-and-languages/extensions, returned 404. — (accessed 2026-10-29)
- pg_lake (Snowflake-Labs) is open source. It creates Iceberg tables from Postgres (`CREATE TABLE ... USING iceberg`) and runs queries in a separate `pgduck_server` process. That process speaks the PG wire protocol on port 5332 over a unix socket, runs DuckDB inside, and defaults to 80% of system memory. — [pg_lake README @1918df64](https://github.com/Snowflake-Labs/pg_lake/blob/1918df64e9655671036037051c8507949a953595/README.md) (accessed 2026-10-29)

### Inferences
- Adoption path (a), an in-process GPU extension: on RDS, Aurora (likely the same policy as RDS, unverified), Cloud SQL, Azure Flexible Server and Neon, it would need the vendor to add it to the allow-list. Because the extension is C/CUDA, pg_tle cannot carry it. GPU-attached Postgres instance types were not found on any page fetched (estimate: none of these offer them).
- The architectural precedent matters: pg_lake runs DuckDB in a separate process (`pgduck_server`) rather than in the backend. A GPU engine could take the same shape and talk to Postgres over the wire protocol. That gives the vendor process isolation, a useful argument when seeking allow-listing (estimate).
- Adoption path (b), a sidecar: postgres_fdw (RDS, Azure, Neon; Cloud SQL only within its VPC) supports pull queries from Postgres into a GPU server that speaks the PG protocol. pglogical and wal2json allow-listed on RDS, Azure and Cloud SQL suggest logical replication out to an external consumer is supported (native publications/pgoutput likely too, unverified here). Path (b) works across all verified providers.
- Self-managed (CloudNativePG, Crunchy PGO, StackGres): any extension can be baked into the image and pods scheduled on GPU nodes (prior knowledge, unverified; see Gaps).

### Gaps
- Not fetched, so unverified: Aurora PG, AlloyDB (has its own columnar engine, prior knowledge), Tembo (its Tembo Analytics stack shipped pg_analytics/ParadeDB, prior knowledge; company pivoted in 2025, unverified), Aiven, DigitalOcean, Heroku, Timescale/Tiger Cloud, EDB BigAnimal / Postgres AI (EDB markets an analytics accelerator, prior knowledge), Xata, Nile, and the Crunchy Bridge allow-list.
- Supabase's pg_tle support (believed present, prior knowledge) and whether it lists pg_duckdb are unverified.
- Not found: GPU instance types for any managed Postgres.
- Not found: provider docs on logical replication to external subscribers (RDS `rds.logical_replication`, Cloud SQL `cloudsql.logical_decoding`, Azure `wal_level=logical`, Neon "logical replication" docs). All prior knowledge, unverified this session.
- Not fetched: K8s operator docs for custom images and GPU node affinity (CloudNativePG `imageName`/ImageCatalog, PGO, StackGres extension mechanism).

## Part B — Postgres-side mechanics of offloading analytics

### Takeaway
There are five ways in, from narrowest to widest.
- CustomScan paths through `set_rel_pathlist_hook`, `set_join_pathlist_hook` and `create_upper_paths_hook`: the PG-Strom model, which coexists with PG's own planner.
- Whole-query takeover through `planner_hook` plus Executor hooks: the pg_duckdb model. pg_duckdb does not hook ExecutorRun.
- FDW: postgres_fdw pushes down only GROUP_AGG, ORDERED and FINAL upper stages.
- Table access method.
- An external sidecar fed by logical decoding. pg_mooncake's current design mirrors heap tables into Iceberg via moonlink over logical replication (`wal_level=logical`).

Heap scans gain a fast path when the page is all-visible. I could not get published GB/s-per-core heap-scan figures. The only replication throughput number found is PeerDB's initial-load benchmark (about 10 MB/s per thread, scaling to 32 threads); no CDC-lag numbers.

### Cited Findings
**Planner and executor hooks (PG 18, SHA cc4bf6e9)**
- `set_rel_pathlist_hook` and `set_join_pathlist_hook` (plus `join_search_hook`) are declared in `src/include/optimizer/paths.h` (lines 30–49). `set_rel_pathlist_hook` is invoked in `allpaths.c` line 542 after the core paths for a base rel are built. — [paths.h](https://github.com/postgres/postgres/blob/cc4bf6e9acc9b9a385e37bd13bd55b5c941529cc/src/include/optimizer/paths.h); [allpaths.c](https://github.com/postgres/postgres/blob/cc4bf6e9acc9b9a385e37bd13bd55b5c941529cc/src/backend/optimizer/path/allpaths.c) (accessed 2026-10-29)
- `planner_hook` and `create_upper_paths_hook` are declared in `src/include/optimizer/planner.h` (lines 26–38). `planner.c` calls `create_upper_paths_hook` per upper stage: UPPERREL_FINAL (l.2391), GROUP_AGG (l.4384), WINDOW (l.4824), DISTINCT (l.5067), PARTIAL_DISTINCT (l.5242), ORDERED (l.5712). A custom provider can therefore offer GPU aggregate, window and sort paths. — [planner.h](https://github.com/postgres/postgres/blob/cc4bf6e9acc9b9a385e37bd13bd55b5c941529cc/src/include/optimizer/planner.h); [planner.c](https://github.com/postgres/postgres/blob/cc4bf6e9acc9b9a385e37bd13bd55b5c941529cc/src/backend/optimizer/plan/planner.c) (accessed 2026-10-29)
- `ExecutorStart_hook`, `ExecutorRun_hook`, `ExecutorFinish_hook`, `ExecutorEnd_hook` and `ExecutorCheckPerms_hook` are declared in `src/include/executor/executor.h` (lines 75–96). — [executor.h](https://github.com/postgres/postgres/blob/cc4bf6e9acc9b9a385e37bd13bd55b5c941529cc/src/include/executor/executor.h) (accessed 2026-10-29)
- pg_duckdb installs `planner_hook`, `ExecutorStart_hook`, `ExecutorFinish_hook`, `ExplainOneQuery_hook` and `emit_log_hook`. It does not set `ExecutorRun_hook`. It replaces the whole plan at planning time: `DuckdbPlannerHook` falls back to `prev_planner_hook` or `standard_planner`. — [pg_duckdb src/pgduckdb_hooks.cpp @a9457c9c, lines 40–44, 508–521](https://github.com/duckdb/pg_duckdb/blob/a9457c9cb3b7f2a8d189bbe017f3488ef4294a1d/src/pgduckdb_hooks.cpp) (accessed 2026-10-29)
- PG-Strom describes itself as accelerating "batch and analytics workloads with utilization of GPU and NVME-SSD, and Apache Arrow columnar". — [pg-strom README @29349cbf](https://github.com/heterodb/pg-strom/blob/29349cbfbc27ab7c12952f4880f810ae7d7b39b4/README.md) (accessed 2026-10-29)

**FDW API and postgres_fdw pushdown**
- The FdwRoutine has `GetForeignJoinPaths` and `GetForeignUpperPaths` callbacks, alongside `BeginForeignScan`/`IterateForeignScan`, where IterateForeignScan returns one TupleTableSlot per call. — [fdwapi.h](https://github.com/postgres/postgres/blob/cc4bf6e9acc9b9a385e37bd13bd55b5c941529cc/src/include/foreign/fdwapi.h) (accessed 2026-10-29)
- `postgresGetForeignUpperPaths` returns early unless the stage is UPPERREL_GROUP_AGG, UPPERREL_ORDERED or UPPERREL_FINAL. So window functions, DISTINCT and set operations are never pushed down by postgres_fdw. — [postgres_fdw.c](https://github.com/postgres/postgres/blob/cc4bf6e9acc9b9a385e37bd13bd55b5c941529cc/contrib/postgres_fdw/postgres_fdw.c) (accessed 2026-10-29)
- postgres_fdw's default `fetch_size` is 100 rows per cursor FETCH (`fpinfo->fetch_size = 100`). It can be overridden per server or table. Results come back row-by-row through a cursor. — [postgres_fdw.c](https://github.com/postgres/postgres/blob/cc4bf6e9acc9b9a385e37bd13bd55b5c941529cc/contrib/postgres_fdw/postgres_fdw.c) (accessed 2026-10-29)

**Table access method**
- The table AM API (`tableam.h`) exposes `scan_getnextslot`, a slot-at-a-time scan, plus TID-range scans. — [tableam.h](https://github.com/postgres/postgres/blob/cc4bf6e9acc9b9a385e37bd13bd55b5c941529cc/src/include/access/tableam.h) (accessed 2026-10-29)

**Heap read cost: visibility**
- `heap_prepare_pagescan` checks `PageIsAllVisible(page) && !snapshot->takenDuringRecovery`. If the page is all-visible it calls `page_collect_tuples(..., all_visible=true)`, which skips per-tuple `HeapTupleSatisfiesVisibility`. Otherwise every tuple gets an MVCC check (heapam.c lines 501–633). The PD_ALL_VISIBLE bit (set by VACUUM and the visibility map) therefore decides whether scanning costs per-tuple xmin/xmax/hint-bit work. — [heapam.c](https://github.com/postgres/postgres/blob/cc4bf6e9acc9b9a385e37bd13bd55b5c941529cc/src/backend/access/heap/heapam.c) (accessed 2026-10-29)

**Mirroring to columnar**
- pg_mooncake (current HEAD) "creates a columnstore mirror of your Postgres tables in Iceberg" with "sub-second freshness". Ingestion runs through moonlink, and the DuckDB-based engine comes via pg_duckdb. Setup requires `shared_preload_libraries='pg_duckdb,pg_mooncake'` and `wal_level = logical`. A mirror is created with `CALL mooncake.create_table('trades_iceberg','trades')`. It supports PG 14–18 and is built with Rust/pgrx. — [pg_mooncake README @b7c67e0a](https://github.com/Mooncake-Labs/pg_mooncake/blob/b7c67e0a08c11c45c4ac5cb5ec5ef231779c1db4/README.md) (accessed 2026-10-29)
- pg_lake writes Iceberg natively through `USING iceberg` tables, with transactional guarantees, and offloads scans to `pgduck_server`. The README says nothing about heap-to-Iceberg replication lag. — [pg_lake README @1918df64](https://github.com/Snowflake-Labs/pg_lake/blob/1918df64e9655671036037051c8507949a953595/README.md) (accessed 2026-10-29)

**Replication and initial-snapshot throughput**
- PeerDB's benchmark covered 1.5 TB (1578 GB), 6 billion rows, 16 columns, from an RDS source to Snowflake. The fetched summary reported load times of 43 hours with 1 thread, under 9 hours with 8 threads, and under 5 hours with 32 or 48 threads, against 83 hours for Airbyte. — [PeerDB blog, PeerDB vs Airbyte](https://blog.peerdb.io/benchmarking-postgres-replication-peerdb-vs-airbyte) (accessed 2026-10-29)
- Derived from those times (estimate): about 10 MB/s (≈39k rows/s) single-threaded, and about 88 MB/s (≈333k rows/s) at 32 threads. The article reports no CDC lag figures. — [same](https://blog.peerdb.io/benchmarking-postgres-replication-peerdb-vs-airbyte) (accessed 2026-10-29)

### Inferences
- CustomScan (the PG-Strom style) keeps PG's planner, permissions and transaction semantics, but makes the GPU engine consume PG's tuple slots or raw pages. Planner-hook takeover (the pg_duckdb style) gives the GPU engine the whole query, but has to reimplement or fall back on unsupported SQL. FDW is the only shape that runs out of process with no custom code inside managed PG, but pushdown is narrow (no window or DISTINCT) and the default 100-row fetch makes bulk transfer slow (estimate).
- Hot OLTP tables have many pages that are not all-visible. Any in-process heap reader pays per-tuple visibility checks there, so a mirrored columnar copy (pg_mooncake/moonlink, or a CDC sidecar) avoids both visibility and deform costs at query time (estimate).
- Initial snapshot is the slow step for a CDC-fed sidecar. One copy stream runs at about 10 MB/s, so a 1 TB table takes many hours unless the copy is parallelised by key or ctid range, as PeerDB does (estimate).

### Gaps
- Not found, because searching was unavailable: heap tuple-deform throughput per core, and published heap-scan GB/s per core for vanilla PG, pg_duckdb or PG-Strom. PG-Strom documents GPU-Direct SQL numbers at heterodb.github.io/pg-strom; I did not fetch them.
- TOAST costs were not researched.
- No CDC throughput or lag numbers from Debezium, Materialize, ReadySet, Epsio, or PeerDB's own CDC posts. The pgoutput single-walsender throughput ceiling is also unsourced.
- No measured replication lag for pg_mooncake/moonlink beyond the README's "sub-second freshness" claim. The moonlink README grep returned nothing usable.
- Arrow Flight / ADBC sidecars for Postgres were not researched.
- The PostgreSQL docs pages (custom-scan-path, postgres-fdw "Remote Query Optimization") were unreachable from the sandbox. The findings above rest on source code instead.
