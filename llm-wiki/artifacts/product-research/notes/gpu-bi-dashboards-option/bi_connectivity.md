# BI connectivity front doors for peacockdb (GPU SQL engine, Rust + DataFusion 45 + libcudf)

Context: peacockdb pins `datafusion = "45"` and `arrow = "54.2.0"` in its Cargo.toml files (peacockdb master f36a68eca3704bb4add89d0292060ed545660948, local repo). All links accessed 2026-10-29. Repos cloned to `/tmp/claude-1000/-media-data-peacockdb/ba2658c9-3cbe-4aa7-bcb8-c0a874d6fde0/scratchpad/ext` at the SHAs given.

## 1. Arrow Flight SQL: which BI tools connect, Rust building blocks, and how much code

### Takeaway
Flight SQL is the cheapest front door for a DataFusion engine: a working reference server is about 1,200 lines of Rust, and it reaches Superset (built-in DataFusion engine spec), DBeaver/DataGrip/Tableau (Arrow Flight SQL JDBC), Grafana (FlightSQL plugin) and Python/ADBC clients. It does not reach Power BI, Looker or Metabase without vendor-specific connector work, because those tools ship one connector per vendor.

### Cited Findings
**BI tool coverage**
- Dremio's client matrix lists the **Arrow Flight SQL JDBC** driver for DBeaver, DataGrip, DbVisualizer, Domo, MicroStrategy and **Tableau**, and the **Arrow Flight SQL ODBC** driver for Excel, PowerPivot, Alteryx and SAP BO. It lists **"ADBC Flight SQL" for Power BI** and the *bundled Dremio JDBC (Legacy)* for Looker, Cognos and ThoughtSpot. Superset and Preset show "N/A" (their own connector) — [Dremio docs](https://docs.dremio.com/current/client-applications/) (accessed 2026-10-29)
- Arrow has shipped a Flight SQL JDBC driver since Arrow 10.0.0, published on Maven Central as `org.apache.arrow:flight-sql-jdbc-driver` — [Arrow blog](https://arrow.apache.org/blog/2022/11/01/arrow-flight-sql-jdbc/), [Maven](https://mvnrepository.com/artifact/org.apache.arrow/flight-sql-jdbc-driver) (accessed 2026-10-29)
- DBeaver connects by adding the JDBC jar in Driver Manager with the URL scheme `jdbc:arrow-flight-sql://` — [voltrondata guide](https://github.com/voltrondata/setup-arrow-jdbc-driver-in-dbeaver) (accessed 2026-10-29)
- The Flight SQL **ODBC driver now lives in the Arrow repo** under `cpp/src/arrow/flight/sql/odbc/` (CMake config, `entry_points.cc`, `connection-options.md`), at apache/arrow a99464f9c625c3f85cade9d7d50eb25297ca1f8d — [apache/arrow tree](https://github.com/apache/arrow/tree/a99464f9c625c3f85cade9d7d50eb25297ca1f8d/cpp/src/arrow/flight/sql/odbc) (accessed 2026-10-29). Arrow's 2022 blog said a Dremio ODBC driver existed outside Arrow because of licensing — [Arrow blog](https://arrow.apache.org/blog/2022/11/01/arrow-flight-sql-jdbc/) (accessed 2026-10-29)
- **Superset**: master ships `superset/db_engine_specs/datafusion.py` (`DataFusionEngineSpec`). It uses `drivers = {"flightsql": "Arrow Flight SQL protocol for DataFusion"}`, `pypi_packages: ["flightsql-dbapi"]` and the URI `datafusion://host:port`, and its time grains use `DATE_BIN`. Source: apache/superset 7ad1d7ff50e193bc37a33cc4ae55bdd307b1fd2e — [datafusion.py](https://github.com/apache/superset/blob/7ad1d7ff50e193bc37a33cc4ae55bdd307b1fd2e/superset/db_engine_specs/datafusion.py) (accessed 2026-10-29)
- `flightsql-dbapi` (InfluxData) is a DB-API 2 and SQLAlchemy adapter built to make Flight SQL work in Superset. Its main dialect targets DataFusion. It is marked experimental, and InfluxData warns that development "may be spotty" — [GitHub](https://github.com/influxdata/flightsql-dbapi), [PyPI](https://pypi.org/project/flightsql-dbapi) (accessed 2026-10-29). Alternatives: `sqlalchemy-adbc`, a generic dialect with URIs like `adbc+flightsql://host:port` — [PyPI](https://pypi.org/project/sqlalchemy-adbc/) (accessed 2026-10-29)
- **Grafana**: InfluxData publishes the `influxdata-flightsql-datasource` plugin (v1.1.1, Grafana ≥ 9.2.5) — [Grafana plugin page](https://grafana.com/grafana/plugins/influxdata-flightsql-datasource/) (accessed 2026-10-29). The official InfluxDB data source later absorbed it (Grafana ≥ 10.3) — [InfluxData blog](https://www.influxdata.com/blog/official-influxdb-v3-data-source-grafana-released/) (accessed 2026-10-29). Flight SQL runs over gRPC, so any proxy in front must support HTTP/2 — [InfluxDB 3 docs](https://docs.influxdata.com/influxdb3/core/visualize-data/grafana/) (accessed 2026-10-29)
- **Power BI**: Microsoft is replacing embedded ODBC drivers with ADBC drivers, for example Simba Spark ODBC → Databricks ADBC and Simba Snowflake ODBC → Snowflake ADBC — [Microsoft Learn "Transition to ADBC"](https://learn.microsoft.com/power-query/transition-to-adbc) (accessed 2026-10-29). Databricks says Power BI plans to move all its connections to ADBC in September 2026; this needs Desktop ≥ 2.145.1105.0 — [Databricks docs](https://docs.databricks.com/aws/partners/bi/power-bi-adbc) (accessed 2026-10-29). These are per-vendor certified connectors. I found no generic "Flight SQL" connector in Power BI.
- InfluxDB 3 has its own Power BI connector (DirectQuery) built on the Arrow Flight SQL ODBC driver — [InfluxData](https://www.influxdata.com/integration/microsoft-power-bi/) (accessed 2026-10-29)
- **Looker**: supports Dremio 26.0+ "(Arrow Flight SQL)" through the bundled Arrow Flight SQL JDBC driver, but as a Dremio dialect — [Looker docs](https://docs.cloud.google.com/looker/docs/db-config-dremio) (accessed 2026-10-29)
- **Metabase**: no official Flight SQL support. A community forum thread asks how to build a plugin for the Arrow Flight SQL JDBC driver and gets no shipped answer — [Metabase discourse](https://discourse.metabase.com/t/arrow-flight-sql-jdbc-driver/241917) (accessed 2026-10-29)
- Other engines with Flight SQL endpoints: StarRocks — [docs](https://docs.starrocks.io/docs/unloading/arrow_flight/); a DuckDB Flight SQL server — [Starlake](https://docs.starlake.ai/qod/duckdb/flight-sql-server) (accessed 2026-10-29)

**Rust crates and reference servers**
- `datafusion-contrib/datafusion-flight-sql-server` @ 45eb14560cc2d4034987bda7aeafb7f9b3272795 (34 stars):
  - Size: the whole crate is ~2,468 lines of Rust including tests and examples. The core is `src/service.rs` at **1,172 lines**, plus `state.rs` (120) and `session.rs` (31). The bearer-auth example is 238 lines — [repo](https://github.com/datafusion-contrib/datafusion-flight-sql-server/tree/45eb14560cc2d4034987bda7aeafb7f9b3272795) (accessed 2026-10-29)
  - What it implements: `get_flight_info_statement`, `substrait_plan` and `prepared_statement`, plus the catalog, schema, table, table-type and SQL-info calls and `create`/`close_prepared_statement`. Primary/exported/imported keys, xdbc_type_info, transactions, savepoints and cancel are present but stubbed: `service.rs` contains 23 `unimplemented`/`Status::unimplemented` hits — [service.rs](https://github.com/datafusion-contrib/datafusion-flight-sql-server/blob/45eb14560cc2d4034987bda7aeafb7f9b3272795/datafusion-flight-sql-server/src/service.rs) (accessed 2026-10-29)
  - Dependencies: `arrow-flight` 59.3 with the `flight-sql` feature and `datafusion` 55.1
  - Version history: crate 0.4.6 targeted `datafusion ^44` / `arrow-flight ^53.3`, and 0.4.7 targeted `datafusion ^46.0.1` / `arrow-flight ^54`. No release targets DataFusion 45 — [crates.io API](https://crates.io/api/v1/crates/datafusion-flight-sql-server/0.4.7/dependencies) (accessed 2026-10-29)
- The `arrow-flight` crate (feature `flight-sql`) provides the `FlightSqlService` trait. A server overrides the `get_flight_info_*`, `do_get_*` and `do_action_*` methods it needs; datafusion-flight-sql-server is a thin implementation of exactly this — same repo/SHA as above (accessed 2026-10-29)
- **GQE (rapidsai/gqe)** @ 50facc46fb99ecf4a955697e78803707335814b3:
  - Its node manager `service` subclasses `arrow::flight::FlightServerBase`, *not* `FlightSqlServerBase`. The header says this is because the SQL base marks its top-level `GetFlightInfo`/`DoGet`/`DoPut` `final`, which blocks dispatching non-SQL command types. Dispatch goes through `google::protobuf::Any` — [service.hpp](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/include/gqe/node_manager/service.hpp) (accessed 2026-10-29)
  - The only Flight SQL commands handled in `src/node_manager` / `include/gqe/node_manager` are `CommandStatementSubstraitPlan` and `CommandGetTables`. `CommandStatementQuery` (SQL text) is never handled. Callbacks take Substrait SELECT plans and serialized `PhysicalRelation` plans and return a host `arrow::RecordBatch` — grep at the same SHA (accessed 2026-10-29)
  - Conclusion: GQE's endpoint is a Substrait/plan RPC for programmatic clients, not a BI front door.

### Inferences
- With arrow-flight 54 (matching peacockdb's arrow 54.2), a peacockdb Flight SQL server is roughly **1.0–2.5 kLOC** of Rust (estimate):
  - statement execution and prepared statements: ~400
  - catalog/schema/table metadata from DataFusion's catalog: ~300
  - SqlInfo: ~100
  - auth/TLS/handshake: ~200
  - plus tests
  - Effort: about **1–3 engineer-weeks** to MVP (estimate). The upstream crate can't be dropped in at DF 45; it would need a fork/backport of 0.4.6/0.4.7, or a DataFusion upgrade.
- Tools unlocked (estimate): **Superset** (DataFusion engine spec already speaks DataFusion SQL, so the dialect fits), **DBeaver/DataGrip**, **Tableau** via the generic JDBC "Other Databases" path (Tableau's SQL generation for a generic JDBC source is weaker than a TDVT-certified connector), **Grafana** via the FlightSQL plugin, and **Python/ADBC notebooks**.
- Not unlocked: **Power BI**, **Looker** and **Metabase** each need a per-vendor connector (Power Query .mez / certified connector, Looker dialect, Metabase driver plugin), regardless of protocol (estimate).
- Result batches serialize from host memory, so the libcudf device result must be copied to host and encoded as Arrow IPC. For dashboard-sized results (≤ 10⁴ rows) that cost is negligible (estimate).

### Gaps
- No confirmation whether Tableau has a dedicated generic "Arrow Flight SQL" connector, beyond the "Other Databases (JDBC)" path implied by Dremio's matrix.
- Did not verify whether Power BI's in-box Dremio connector can point at a non-Dremio Flight SQL server.
- Ballista and InfluxDB 3 Flight SQL code was not inspected, and no LOC was measured for them. Dremio's Flight SQL server is Java and was not inspected.

## 2. Postgres wire protocol: Rust options, BI needs, effort and pitfalls

### Takeaway
pgwire gives the protocol for free, and datafusion-postgres gives a DataFusion-backed pg_catalog. The real cost is catalog and function emulation: datafusion-postgres is ~25 kLOC, and it still lists Power BI as unsupported. Postgres wire reaches the widest set of BI tools (every one ships a Postgres connector), but each tool's metadata queries are a long tail of compatibility work.

### Cited Findings
- `pgwire` (sunng87) is at v0.42.1 (807 stars). It supports:
  - startup and auth: cleartext, MD5, SCRAM-SHA-256, OAuth
  - simple and extended query, COPY, cancellation
  - protocol 3.0/3.2, TLS via `aws-lc-rs`/`ring`
  - not yet: streaming replication
  - The user implements `StartupHandler`, `SimpleQueryHandler`, `ExtendedQueryHandler`, `CopyHandler` and `CancelHandler`, plus `QueryParser`/`PortalStore` for extended query — [docs.rs](https://docs.rs/pgwire/latest/pgwire/) (accessed 2026-10-29)
- pgwire's README lists GreptimeDB, CeresDB and SpacetimeDB as users. It names datafusion-postgres as "a more complete library built on top of pgwire, with datafusion as query engine, as well as pg_catalog implementation" (pgwire b84ada7e5ce128685745daa0f68b6e1461f9e79e) — [README](https://github.com/sunng87/pgwire/blob/b84ada7e5ce128685745daa0f68b6e1461f9e79e/README.md) (accessed 2026-10-29)
- **RisingWave** writes its own Postgres protocol crate (`src/utils/pgwire/src/`: `pg_protocol.rs`, `pg_extended.rs`, `pg_server.rs`, `ldap_auth.rs`, …) instead of using sunng87/pgwire — [risingwave tree](https://github.com/risingwavelabs/risingwave/tree/main/src/utils/pgwire/src) (accessed 2026-10-29; the commit SHA could not be resolved because the GitHub API connection failed). Superset ships a `risingwave.py` engine spec — [superset 7ad1d7f](https://github.com/apache/superset/tree/7ad1d7ff50e193bc37a33cc4ae55bdd307b1fd2e/superset/db_engine_specs) (accessed 2026-10-29)
- **datafusion-postgres** (`datafusion-contrib/datafusion-postgres`) @ 87f338367305d65d85dcb9c67715eda7de79b418 (165 stars, v0.19.0 released 2026-10-20) — [repo](https://github.com/datafusion-contrib/datafusion-postgres/tree/87f338367305d65d85dcb9c67715eda7de79b418) (accessed 2026-10-29):
  - Size by crate: `datafusion-postgres` 4,924 lines, `datafusion-pg-catalog` 8,616, `datafusion-pg-functions` 7,001, `arrow-pg` 4,593. About 25.4 kLOC of Rust in total.
  - Largest files: `pg_catalog.rs` 2,313, `sql/rules.rs` 1,731 (SQL rewrite rules for client queries), `arrow-pg/src/datatypes/df.rs` 1,790, `encoder.rs` 1,205, `handlers.rs` 905, `auth.rs` 641, `hooks/set_show.rs` 625.
  - README client checklist: psql, DBeaver, pgcli, VSCode SQLTools, **Metabase** and **Grafana** are ticked. IntelliJ DataGrip and **Power BI** are **unticked** — [README](https://github.com/datafusion-contrib/datafusion-postgres/blob/87f338367305d65d85dcb9c67715eda7de79b418/README.md) (accessed 2026-10-29)
  - DataFusion compatibility: 0.3.0 required `datafusion ^42` / `pgwire ^0.25`, and 0.4.0 required `datafusion ^47` / `pgwire ^0.30.2`. No release targets DataFusion 45 — [crates.io deps 0.4.0](https://crates.io/api/v1/crates/datafusion-postgres/0.4.0/dependencies) (accessed 2026-10-29). Current master uses arrow 59.
- **Power BI's PostgreSQL connector** depends on the Npgsql .NET driver. It has version-specific breakage: the October 2024 Desktop and Npgsql 4.0.16 regression, and gateway messages demanding "Npgsql version 4.0.10.0 or earlier" — [Fabric community](https://community.fabric.microsoft.com/t5/Desktop/Cannot-connect-to-postgres-SQL-to-power-BI-deskto.../td-p/4319244), [npgsql#5818](https://github.com/npgsql/npgsql/issues/5818) (accessed 2026-10-29)
- **Kinetica** (a GPU database) as precedent: "PostgreSQL simple query syntax is fully supported, while PostgreSQL extended query syntax is largely supported", on port 5432. The documented clients are psql, psycopg2 and `jdbc:postgresql://` — [Kinetica docs](https://docs.kinetica.com/7.2/connectors/psql_wire_protocol/) (accessed 2026-10-29)

### Inferences
- MVP effort for peacockdb on pgwire (estimate):
  - simple and extended query, Arrow → pg type encoding, TLS and SCRAM: **~3–5 kLOC, 3–6 engineer-weeks**
  - pg_catalog/information_schema emulation good enough for psql, DBeaver, Metabase, Grafana and Superset (`postgresql://` dialect): **+5–10 kLOC, +1–3 months**, judging by datafusion-postgres's catalog and functions crates (~15.6 kLOC)
  - Power BI and Tableau quirks (Npgsql type OID introspection, cursors, `SET`/`SHOW` handling, `pg_type` joins, `current_setting`, `version()` string): open-ended
- The cheapest route is to vendor or backport datafusion-postgres. Its DF 42 → 47 jump means pinning an old release won't work, so a backport or a DataFusion upgrade is needed (estimate).
- Pitfalls (estimate, drawn from the README checklist, the Npgsql threads and the Kinetica caveat):
  - extended protocol/prepared statements: parameter type inference in DataFusion
  - binary vs text result formats
  - Postgres type OIDs for Arrow types (decimal, timestamptz, interval)
  - BI tools sending Postgres-specific functions DataFusion lacks (`to_char`, `date_trunc` variants, `generate_series`, `::regclass` casts)
  - row-oriented wire encoding: each row is serialized separately, which is fine for dashboard results but bad for big extracts
- Tools unlocked: psql, DBeaver, Metabase, Grafana and Superset are proven on datafusion-postgres. Tableau and Looker (via their PostgreSQL connectors) and Power BI (via Npgsql) are plausible but unproven (estimate). Postgres wire is the only single protocol that could reach all seven tools named in the brief.

### Gaps
- No primary source found listing the exact pg_catalog queries that Tableau, Power BI or Metabase issue at connect time.
- GreptimeDB's pg compatibility layer was not inspected. RisingWave's pgwire LOC was not measured, and its SHA was not resolved because the GitHub API connection failed.

## 3. Plugging into existing systems instead of a standalone server

### Takeaway
Plugin routes inherit the host's catalog, auth, SQL dialect and every BI driver the host already has. The Sirius precedents show the pattern: a DuckDB core-extension loader, and ClickHouse RFCs that send Substrait to a separate GPU worker process. The Spark Thrift + RAPIDS evidence shows the cost: the plugin inherits the host's deployment and classloading fragility, and its GPU memory-budgeting bugs show up as BI-facing failures.

### Cited Findings
- **DuckDB PR #28022 "Add Sirius as a core extension"**:
  - Status: open, by smvv, created 2026-10-28, head 4c8b4c22512c822ed812a77ca239a5de6371c1bf
  - What it adds: a *lightweight Sirius loader* to DuckDB's core extension group on supported Linux targets
  - What it keeps out: `sirius_cuda12` and `sirius_cuda13` stay out of the core build and "will be distributed as community extensions". Sirius is pinned to `5350b8f951c32cb79f4b1971a48c5915f0a6908f`
  - Related: duckdb/extension-ci-tools#458, duckdb/duckdb-ci#86, sirius-db/sirius#2398, duckdb/community-extensions#3447
  - [PR](https://github.com/duckdb/duckdb/pull/28022) (accessed 2026-10-29)
- **ClickHouse #116807 "[RFC] ClickHouse GPU v1"** (scanhex12, 2026-08-27, open) — [issue](https://github.com/ClickHouse/ClickHouse/issues/116807) (accessed 2026-10-29):
  - Two options: (a) reuse Sirius through its Substrait entry point (`SubstraitSerializer`, `SiriusStep`, `SiriusEligibility`), or (b) a deeper integration lifting Sirius components (tiered memory over cuCascade, downgrade executors, GPU codecs, kvikio/GDS I/O, CUDA kernels) plus a ClickHouse Block↔cudf bridge.
  - Stated limits of the Substrait route: PREWHERE, primary-key granule pruning, projections and skip indexes are lost. The plan passes through four representations (QueryPlan → Substrait → DuckDB logical → Sirius), each lossy. ClickHouse functions and types without a Substrait mapping fall back to CPU.
- **ClickHouse #124455 "[RFC] GPU Offload of Query Plan Subtrees to Sirius"** (rienath, 2026-10-07, open) — [issue](https://github.com/ClickHouse/ClickHouse/issues/124455) (accessed 2026-10-29):
  - Design: `clickhouse-server` runs unmodified, and "a GPU backend process embeds Sirius". `SiriusStep` sends Substrait plans and gets back "results as columns".
  - Data: the backend keeps a GPU cache keyed by (part, column, data version), and misses are served by ClickHouse's MergeTree reader.
  - Reasons for a separate process: a GPU segfault or CUDA fault takes down the backend, not the server. The process doubles as the future distributed GPU worker.
  - Failure policy: "a GPU-path failure produces a clear error with no CPU fallback".
  - Sirius's interface: it ships "behind `SiriusContext` (Substrait bytes in, Arrow batches out)".
  - Performance claim: Sirius-in-DuckDB was reported 38× faster than ClickHouse on TPC-H SF100, citing CIDR 2026 — [paper](https://vldb.org/cidrdb/papers/2026/p12-yogatama.pdf) (accessed 2026-10-29)
- **spark-rapids discussion #9966 "[BUG] Unable to use Spark Rapids with Spark Thrift Server"** (LIN-Yu-Ting, 2023-11-27) — [discussion](https://github.com/NVIDIA/spark-rapids/discussions/9966) (accessed 2026-10-29). What the user hit:
  - Setup: Spark 3.3.0 + RAPIDS 23.10 + Thrift Server. Superset metadata queries failed with `NoClassDefFoundError: com/nvidia/spark/rapids/RuleNotFoundRunnableCommandMeta` from `GpuOverrides`.
  - Narrowing it down: beeline and PyHive worked. Only Superset failed, specifically on `SHOW VIEWS IN …`, and the user linked it to Superset opening **multiple concurrent sessions**.
  - The user built a custom Spark and recorded a YouTube repro. NVIDIA's suggested single-Spark-version "conventional" jar build did not help at first.
  - Fix: the conventional jar (`-DallowConventionalDistJar=true`) on a *static* classpath (`$SPARK_HOME/jars` or `--driver-class-path`) worked. `--jars` still failed.
  - Maintainer gerashegalov diagnosed "a race condition that affects all the sessions if the user classloader deployment --jars is used". The workaround was to "pre-warm" the Thrift server with one beeline session (`SHOW FUNCTIONS; SHOW SCHEMAS; SHOW TABLES; SHOW VIEWS`) before letting Superset in. He also saw `TTransportException` on metadata queries with CPU Spark.
- **spark-rapids issue #11064** (LIN-Yu-Ting, 2024-06-14, closed 2024-07-17) — [issue](https://github.com/NVIDIA/spark-rapids/issues/11064) (accessed 2026-10-29):
  - The same user, serving SQL through Spark Thrift Server + RAPIDS, ran `SELECT DISTINCT runName` on a Delta table partitioned by `runName` and got GPU OOM (`RMM failure … limiting_resource_adaptor … Exceeded memory limit` in `ColumnarPartitionReaderWithPartitionValues`).
  - Diagnosis (jlowe): batch-size budgeting ignores partition columns. When only the partition column is read, Parquet supplies only row counts, so max-size batches are loaded and then blow past the budget once partition values are attached.
  - Tuning `spark.rapids.sql.concurrentGpuTasks=1` did not help. The fix landed in #11155 (24.08). The user also said the #9966 workaround was still needed.
- **User's pain, summarized**: a dashboard tool (Superset) acts like many concurrent sessions issuing metadata-heavy queries. That exposed (1) plugin classloading races that a single beeline session never showed, and (2) GPU OOM on a trivial "DISTINCT partition column" query, the kind a dashboard filter dropdown issues. Both took weeks of maintainer back-and-forth, custom builds and a warm-up hack — sources: the #9966 and #11064 links above (accessed 2026-10-29)
- Superset ships engine specs for `duckdb.py`, `spark.py`, `hive.py`, `postgres.py`, `dremio.py`, `datafusion.py` and `risingwave.py` — [superset 7ad1d7f db_engine_specs](https://github.com/apache/superset/tree/7ad1d7ff50e193bc37a33cc4ae55bdd307b1fd2e/superset/db_engine_specs) (accessed 2026-10-29)

### Inferences
- What a plugin gets for free (estimate):
  - **DuckDB extension**: DuckDB's SQL dialect and catalog, Superset's `duckdb` spec, Python/R/JDBC/ODBC drivers, MotherDuck. No network server, though, so BI still needs a front door (a DuckDB Flight SQL server or the Postgres-compatible proxies).
  - **ClickHouse offload**: ClickHouse's native, HTTP and MySQL/Postgres wire endpoints, RBAC, and its official Tableau/Power BI/Grafana/Metabase/Superset connectors.
  - **Spark Thrift/Trino**: HiveServer2 JDBC/ODBC, which most BI tools support.
- What a plugin costs (estimate): the integration contract is Substrait, so peacockdb would need a Substrait consumer. DataFusion has `datafusion-substrait`, which is a natural fit. Plan fidelity is lost, as the ClickHouse RFC lists. The host's operational quirks become yours (#9966), and peacockdb gives up its own planner choices.
- The ClickHouse RFC's "separate GPU process speaking Substrait in / Arrow out" is functionally what a peacockdb Flight server taking `CommandStatementSubstraitPlan` would be, which is the GQE design. One server could therefore serve both BI (Flight SQL text) and host engines (Substrait plans) (estimate).
- A Trino connector would push only scans, filters, projections and aggregates to peacockdb through Trino's SPI. The GPU win would depend on how much Trino pushes down, and joins would mostly run in Trino (estimate).

### Gaps
- Did not read Trino's connector SPI docs or a Superset → Trino → custom-connector precedent, and have no effort numbers for a Trino connector.
- No info on whether DuckDB's Sirius extension exposes a network endpoint.
- Comment threads on ClickHouse #116807 (4 comments) were not read.

## 4. Dashboard workload shape, and how HeavyDB, Kinetica and Fabric expose themselves

### Takeaway
Dashboards send bursts of small, concurrent, mostly aggregate queries, and BI tools send many metadata queries on connect (seen in #9966). Incumbents meet this with a proprietary native protocol for their own UI plus JDBC/ODBC/Postgres-wire for third-party BI (HeavyDB, Kinetica), or by bypassing SQL altogether with an in-memory semantic engine (Fabric Direct Lake/VertiPaq). For peacockdb, per-query overhead and result caching matter more than wire-protocol throughput (estimate).

### Cited Findings
- **Fabric Direct Lake** — [Microsoft Learn](https://learn.microsoft.com/en-us/fabric/fundamentals/direct-lake-overview) (page dated 2026-09-02; accessed 2026-10-29):
  - How it works: Power BI loads Delta-table columns from OneLake into the VertiPaq engine. "Direct Lake and Import queries are processed by the VertiPaq query engine, whereas DirectQuery federates queries to the underlying data source". Refresh ("framing") copies only metadata and "can take a few seconds".
  - Fallback: "Direct Lake on SQL" falls back to DirectQuery through the warehouse's SQL analytics endpoint for SQL views, SQL-based RLS or guardrail breaches. "Direct Lake on OneLake" never falls back.
  - Guardrails: rows per table range from 300 M (F2–F32) to 24 B (F1024+). Max memory is 3 GB on F2 and 400 GB on F1024+.
  - Microsoft's own framing: DirectQuery sources "aren't typically optimized for heavy query load coming from reports and aggregated queries needed by the visuals".
- **HeavyDB** (heavyai/heavydb @ 72c6df9f1bce2cd5b849670b38c7e7e4592c564b), Thrift service in `heavy.thrift` — [heavy.thrift](https://github.com/heavyai/heavydb/blob/72c6df9f1bce2cd5b849670b38c7e7e4592c564b/heavy.thrift) (accessed 2026-10-29):
  - Row/column calls: `sql_execute(session, query, column_format, nonce, first_n, at_most_n)`
  - Arrow result calls: `sql_execute_df(session, query, TDeviceType device_type, device_id, first_n, TArrowTransport)`, `sql_execute_gdf(session, query, device_id, first_n)` and `deallocate_df(session, TDataFrame, device_type, device_id)`
  - `TDataFrame { sm_handle, sm_size, df_handle, df_size, execution_time_ms, arrow_conversion_time_ms, df_buffer }`
  - Server-side rendering: `render_vega(...)` and `get_result_row_for_pixel(...)` for its Immerse UI
- HEAVY.AI docs list Thrift, JDBC, ODBC (Tableau/Excel from Windows) and `pyheavydb` (DB-API 2 over Thrift) — [HEAVY.AI ODBC docs](https://docs.heavy.ai/v6.4.3/apis-and-interfaces/odbc), [pyheavydb](https://pypi.org/project/pyheavydb/) (accessed 2026-10-29)
- Current Superset master has **no** HeavyDB/OmniSci/MapD engine spec file. I grepped `db_engine_specs` for heavy/omnisci/mapd/kinetica and found nothing — [superset 7ad1d7f](https://github.com/apache/superset/tree/7ad1d7ff50e193bc37a33cc4ae55bdd307b1fd2e/superset/db_engine_specs) (accessed 2026-10-29)
- **Kinetica** (GPU DB) uses three front doors — [Kinetica JDBC/ODBC guide](https://docs.kinetica.com/7.2/connectors/sql_guide), [Kinetica Tableau](https://docs.kinetica.com/content/bi/tableau), [Kinetica pgwire](https://docs.kinetica.com/7.2/connectors/psql_wire_protocol/) (accessed 2026-10-29):
  - native HTTP API on :9191, with JDBC `jdbc:kinetica:URL=http://host:9191` (`com.kinetica:kinetica-jdbc`) and ODBC
  - Postgres wire on :5432
  - Tableau goes through Kinetica ODBC/JDBC, and the docs say ODBC may outperform JDBC
- **HeavyDB stability under dashboard load**: heavydb issue #819 (2023-12-08, closed) reports random SIGSEGV crashes in 6.4.0 ("causing the DB to crash and create outages") — [issue](https://github.com/heavyai/heavydb/issues/819) (accessed 2026-10-29). **Note:** this issue is *not* about `select_ipc_gpu` or GPU IPC, despite the brief's reference.

### Inferences
- A typical dashboard refresh is N tiles × 1 query, usually N ≈ 5–30 (estimate), plus filter-dropdown `SELECT DISTINCT` queries and metadata calls, all fired at once.
  - Needs: a connection/session pool without per-session GPU state, a cheap plan-cache keyed on normalized SQL, and a result cache keyed on (SQL, data version). ClickHouse's (part, column, version) GPU cache is a data-side analogue.
  - Admission control: queue many small queries onto one GPU, rather than giving each session its own `concurrentGpuTasks` slot (estimate).
- Latency targets: "sub-second" per tile means p95 ≲ 300–500 ms end-to-end, including protocol overhead and the D→H copy (estimate). gRPC/Flight and Postgres wire each add only single-digit milliseconds for small results (estimate).
- **Power BI is the special case**: the high-performance path is Import/Direct Lake into VertiPaq, so a GPU engine only matters to Power BI in DirectQuery mode. DirectQuery needs a Power Query connector (ADBC- or ODBC-based) (estimate).

### Gaps
- Found no published per-tool numbers for queries per dashboard refresh or default concurrency (for example Superset's chart parallelism, or Tableau's connection pool) in this pass.
- Did not confirm whether HeavyDB ever had a Superset dialect in any Superset release (the precedent named in the brief). Current master has none.
- Fabric Warehouse's TDS protocol details were not fetched in this pass. That it is a SQL analytics endpoint queried by DirectQuery fallback is confirmed by the Learn page above.

## 5. GPU-resident result hand-off as a front door

### Takeaway
No standard protocol ships GPU-resident results across processes. Arrow Flight is host-memory, and the Arrow C Device Data Interface is explicitly in-process. HeavyDB's precedent is a proprietary Thrift call that returns CUDA IPC handles. Graphistry's demand (cudf#23039) is for in-process device-resident chaining. So peacockdb's GPU hand-off front door should be an in-process library API that exports `ArrowDeviceArray`, with an optional same-host CUDA-IPC side channel (estimate).

### Cited Findings
- **Arrow C Device Data Interface** (experimental) — [Arrow docs](https://arrow.apache.org/docs/format/CDeviceDataInterface.html) (accessed 2026-10-29):
  - Struct: `ArrowDeviceArray { ArrowArray array; int64 device_id; ArrowDeviceType device_type; void* sync_event; reserved[3] }`
  - Device types: `ARROW_DEVICE_CUDA` (cudaMalloc/cuMemAlloc) and `ARROW_DEVICE_CUDA_HOST` (pinned)
  - Sync: for CUDA, `sync_event` is a `cudaEvent_t*` the consumer must wait on
  - Streams: a synchronous device stream, plus an Async Device Stream (`ArrowAsyncDeviceStreamHandler` with `on_schema`/`on_next_task`/`on_error`, and the producer's `request(n)`/`cancel()` for back-pressure)
  - Scope: zero-copy sharing "between independent runtimes and components running in the same process". The spec says nothing about cross-process or CUDA IPC, and nothing about Flight transport for device memory.
- **HeavyDB** precedent (heavyai/heavydb @ 72c6df9f…) — [heavy.thrift](https://github.com/heavyai/heavydb/blob/72c6df9f1bce2cd5b849670b38c7e7e4592c564b/heavy.thrift) (accessed 2026-10-29):
  - `sql_execute_gdf` and `sql_execute_df(device_type=GPU)` return a `TDataFrame` with `sm_handle` and `df_handle` binary handles (schema/shared-memory handle and data handle) plus `arrow_conversion_time_ms`.
  - `deallocate_df` must be called to free the server-owned buffer. That is explicit cross-process lifetime management over Thrift.
- **cudf issue #23039** (lmeyerov / Graphistry, 2026-06-30, open), "cudf-polars: keep collect() results or intermediate persist() values on-device" — [issue](https://github.com/NVIDIA/cudf/issues/23039) (accessed 2026-10-29):
  - Context: PyGraphistry's GFQL/Cypher engine uses cudf-polars, but `LazyFrame.collect()` and `.sink_*()` force D→H.
  - Pain 1: GPU benchmarks include a host-transfer tax.
  - Pain 2: iterative crossfilter / multi-hop loops on a GPU-resident graph can't chain D→D.
  - The workaround is a private in-memory-executor API ("caveat emptor").
  - Quote: lack of D→D "in practice, eliminates utility for many of our non-trivial use cases". Single-node, single-GPU is the primary case.

### Inferences
- Options for peacockdb (estimates):
  - (a) **In-process API** exporting results as `ArrowDeviceArray` / device stream: cheap (~0.5–1 kLOC, mostly lifetime and stream-sync plumbing). libcudf already has `to_arrow_device`. This serves Python and GPU consumers such as Graphistry, cuDF and Polars-GPU in the same process.
  - (b) **Same-host cross-process**: CUDA IPC handles (`cudaIpcGetMemHandle`) carried in a Flight `DoGet`/`DoAction` app-metadata, or in a custom Flight command, with explicit release, as in HeavyDB's `deallocate_df`. Moderate effort (~1–2 kLOC plus a pinned-allocation lifetime policy). It is non-standard, and no BI tool would use it.
  - (c) **Plain Flight / Flight SQL**: host copy. Universal, and for dashboard-size results the D→H cost is irrelevant.
- BI tools render on CPU, so GPU hand-off is not a BI front door. It is a front door for GPU analytics and visual apps (Graphistry-like crossfilter), and it should sit alongside Flight SQL, not replace it.

### Gaps
- `select_ipc_gpu` was not found in `heavy.thrift` at 72c6df9f. It is likely the client-side pymapd/pyomnisci/heavyai Python wrapper around `sql_execute_gdf` (unverified; GitHub code search timed out).
- heavyai/heavydb issue #819 is unrelated to GPU IPC (it is a SIGSEGV report). The issue the brief intended may be a different number.
- No source found for a standard cross-process Arrow device transport (for example a Flight extension carrying CUDA IPC or UCX). I did not check whether Arrow Flight's UCX transport supports device memory.
