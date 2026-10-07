# pbench implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A third corpus dataset, `pbench` (sf1 only, committed parquet), with 63 queries that
each show one in-scope ticket of chain J, registered with their cells off where a ticket blocks
them, planned with the small-table rule off, rendered by the cost-report widget, and checked
against DuckDB from the start.

**Architecture:** A seeded DuckDB script writes five small tables into `testdata/pbench.sf1/`,
committed like `tpch.minimal`; a `--check` mode regenerates into a temp dir and compares row
content. The corpus machinery already keys everything on a dataset name; this task adds the
name to every list that spells the datasets out, routes the planner knobs through a per-dataset
`small_table_bytes`, and writes the goldens. No engine change.

**Tech Stack:** DuckDB 1.5.4 CLI (SQL + bash), Rust test harness (`test_support`, the corpus
binaries, `plan_goldens`), `cost-report`, GitHub Actions YAML, the shad-gpu scripts.

**Spec:** [`pbench.md`](pbench.md) — committed `9e563348`. Design context:
[`join-rewrite-design.md`](join-rewrite-design.md). Lands after
[`duckdb-oracle.md`](duckdb-oracle.md) and [`stale-cells.md`](stale-cells.md).

## Global Constraints

- Test data and plumbing, plus two refusals that change no answer: #227's nullability check (Task 6c) and a wire-type refusal naming its ticket (Task 5 Step 0). No fix to anything a query shows; a failing query gets its ticket on the row.
- sf1 only, no scaling, no S3. The parquet is committed; nothing generates it in CI, on shad-gpu or on verda.
- DuckDB pinned at **1.5.4** (`duckdb_result.py`'s `PINNED`, CI's release binary `v1.5.4`, build `08e34c447b`).
- Parquet under 1 MB in total. `fact` 20,000 rows (10 row groups of 2,048 — DuckDB rounds `ROW_GROUP_SIZE`), `dim` 2,000, `sub` 200, `tiny` 8, `empty` 0.
- pbench plans with `small_table_bytes = 0`; tpch and tpcds keep `SMALL_TABLE_BYTES` (5 MB). Nothing else about the modes changes.
- Every `corpus_query!` line has nine arguments in duckdb-oracle's order: `(dataset, sf, query, cpu_modes, gpu_modes, duckdb_oracle, cpu_oracle, gpu_oracle, schema_validation)`; `all_modes` stands for the five.
- The three float rows (`float64-key-group`, `float32-key-group`, `float64-key-join`) land with SQL and goldens, their line **commented out** under `// #243 — the cpu keys -0.0 and NaN by their bits`, and their registry row tagged `243` with every cell off.
- Commits at most 10 lines; device cycles foreground (`build-test-shadgpu.sh`); never share a cargo target dir across worktrees.
- Every commit is green: no list a test reads names pbench before the commit that writes pbench's
  goldens (Task 5), and no registry row lands with an off cell that carries no ticket.
- `timestamp-s-key-group` cannot cross the wire until repartition-keys adds the fbs timestamp types:
  it is declared `NOT_RUNNABLE` on #240 (`plan_goldens.rs:442`), its gpu cells off on `240`.
- `scalar-subquery-cross`'s second subquery counts `dim WHERE d_w >= 0`: an unfiltered `count(*)`
  is answered from parquet statistics as a `PlaceholderRowExec`, which the planner refuses (#158).

## Review Focus

1. **A DuckDB other than 1.5.4 writes different random data.** `random()` after `setseed` is version-specific; a newer CLI would silently change every golden. Expected: `generate_pbench.sh` refuses any other version. Pinned in Task 1, step 2.
2. **The small-table override leaks.** tpch or tpcds planned with `small_table_bytes = 0` would move every tpch/tpcds plan golden. Expected: only `pbench` sees 0. Pinned in Task 3, step 1 (a unit test on `knobs_for` for all three names) and step 6 (tpch/tpcds plan goldens byte-identical).
3. **NaN, -NaN and -0.0 do not survive the parquet round trip.** If DuckDB or the reader canonicalized them, #243's rows would show nothing. Expected: the committed `fact.parquet` holds both NaN signs and -0.0. Pinned in Task 1, step 4 (`--check` asserts the special-value counts).
4. **A query plans differently from the spec's "plans as" column.** A DataFusion-side drift would make the query show a different ticket. Expected: each join row's plan golden at tp4-single names the join type and mode the spec says. Pinned in Task 5, step 4 (a planner test over the pbench plan golden).
5. **A commented-out line leaves its registry row inconsistent.** Expected: the row's every cell `disabled` with `243`, and the registry tests (both directions) green. Pinned in Task 5, step 6.

---

### Task 1: The generator and the committed data

**Files:**
- Create: `testdata/pbench/gen.sql`
- Create: `testdata/generate_pbench.sh`
- Create: `testdata/pbench.sf1/{fact,dim,sub,tiny,empty}.parquet` (generated, committed)

**Interfaces:**
- Produces: `testdata/pbench.sf1/` (read by `test_support::data_dir_for("pbench", "1")`), and
  `testdata/generate_pbench.sh [--check]` (used by CI in Task 7).

- [ ] **Step 1: `testdata/pbench/gen.sql`.** The prototype the spec measured, made deterministic
  (`threads=1`, rows ordered on write) and given `-NaN` and the float specials on both sides, so
  #243's join shows:

```sql
-- pbench sf1: the shapes tpch and tpcds lack. Deterministic under DuckDB 1.5.4 with threads=1.
-- Run from the output directory: COPY writes relative paths.
SET threads = 1;
SELECT setseed(0.42);

CREATE TABLE fact AS
SELECT i::BIGINT AS f_id,
  CASE WHEN r1 < 0.05 THEN NULL WHEN r1 < 0.55 THEN (i % 3)::INT ELSE (r2 * 1000)::INT END AS f_k,
  ((i % 200) - 100)::TINYINT AS f_k8,
  CASE WHEN i % 17 = 0 THEN NULL ELSE i % 2 = 0 END AS f_kb,
  CASE WHEN i % 101 = 0 THEN 'NaN'::FLOAT WHEN i % 97 = 0 THEN '-NaN'::FLOAT
       WHEN i % 103 = 0 THEN -0.0::FLOAT WHEN i % 107 = 0 THEN NULL
       ELSE ((i % 50) / 4.0)::FLOAT END AS f_kf32,
  CASE WHEN i % 101 = 0 THEN 'NaN'::DOUBLE WHEN i % 97 = 0 THEN '-NaN'::DOUBLE
       WHEN i % 103 = 0 THEN -0.0::DOUBLE WHEN i % 109 = 0 THEN 0.0::DOUBLE
       WHEN i % 107 = 0 THEN NULL ELSE ((i % 50) / 4.0)::DOUBLE END AS f_kf64,
  CASE WHEN i % 113 = 0 THEN NULL ELSE ((i % 300) / 4.0)::DECIMAL(15,2) END AS f_kdec15,
  CASE WHEN i % 113 = 0 THEN NULL ELSE ((i % 300) * 1234567890123.0001)::DECIMAL(38,4) END AS f_kdec38,
  (TIMESTAMP '2024-01-01' + to_seconds(i % 400))::TIMESTAMP_S AS f_ts_s,
  (TIMESTAMP '2024-01-01' + to_milliseconds(i % 400))::TIMESTAMP_MS AS f_ts_ms,
  (TIMESTAMP '2024-01-01' + to_microseconds(i % 400))::TIMESTAMP AS f_ts_us,
  (TIMESTAMP '2024-01-01' + to_microseconds(i % 400))::TIMESTAMP_NS AS f_ts_ns,
  (DATE '2024-01-01' + (i % 365)::INT) AS f_dt,
  CASE WHEN i % 19 = 0 THEN NULL ELSE 'v' || (i % 97)::VARCHAR END AS f_s,
  CASE WHEN i % 131 = 0 THEN NULL ELSE ((i * 2654435761) % 4294967296)::UINTEGER END AS f_ku32,
  {'a': (i % 7)::INT, 'b': 'v' || (i % 5)::VARCHAR} AS f_kstruct,
  (i % 50)::INT AS f_qty,
  ((i % 1000) / 10.0)::DECIMAL(15,2) AS f_amount
FROM (SELECT range AS i, random() AS r1, random() AS r2 FROM range(20000) ORDER BY range);

CREATE TABLE dim AS
SELECT i::BIGINT AS d_id,
  CASE WHEN i % 40 = 0 THEN NULL ELSE (i // 2)::INT END AS d_k,
  (((i // 2) % 200) - 100)::TINYINT AS d_k8,
  CASE WHEN i % 23 = 0 THEN NULL ELSE i % 2 = 0 END AS d_kb,
  CASE WHEN i % 211 = 0 THEN 'NaN'::FLOAT WHEN i % 223 = 0 THEN -0.0::FLOAT
       ELSE (((i // 2) % 50) / 4.0)::FLOAT END AS d_kf32,
  CASE WHEN i % 211 = 0 THEN 'NaN'::DOUBLE WHEN i % 227 = 0 THEN '-NaN'::DOUBLE
       WHEN i % 223 = 0 THEN -0.0::DOUBLE ELSE (((i // 2) % 50) / 4.0)::DOUBLE END AS d_kf64,
  (((i // 2) % 300) / 4.0)::DECIMAL(15,2) AS d_kdec15,
  (((i // 2) % 300) * 1234567890123.0001)::DECIMAL(38,4) AS d_kdec38,
  (TIMESTAMP '2024-01-01' + to_seconds((i // 2) % 400))::TIMESTAMP_S AS d_ts_s,
  (TIMESTAMP '2024-01-01' + to_microseconds((i // 2) % 400))::TIMESTAMP AS d_ts_us,
  (DATE '2024-01-01' + ((i // 2) % 365)::INT) AS d_dt,
  'v' || ((i // 2) % 97)::VARCHAR AS d_s,
  ((i * 2654435761) % 4294967296)::UINTEGER AS d_ku32,
  {'a': (i % 7)::INT, 'b': 'v' || (i % 5)::VARCHAR} AS d_kstruct,
  (i % 40)::INT AS d_w,
  'name' || i::VARCHAR AS d_name
FROM range(2000) t(i);

CREATE TABLE sub AS
SELECT (i % 20)::INT AS s_z, CASE WHEN i % 25 = 0 THEN NULL ELSE (i * 7 % 1000)::INT END AS s_y
FROM range(200) t(i);

CREATE TABLE tiny AS
SELECT i::INT AS t_id, (i % 2)::INT AS t_k, (i * 5)::INT AS t_v,
  (['name1%', '%7', 'name2_', '%', 'name1999', 'x%', '%0', 'name%5'])[i + 1] AS t_pat
FROM range(8) t(i);
CREATE TABLE empty AS SELECT * FROM tiny WHERE false;

COPY (SELECT * FROM fact ORDER BY f_id) TO 'fact.parquet' (FORMAT parquet, ROW_GROUP_SIZE 1000);
COPY (SELECT * FROM dim ORDER BY d_id) TO 'dim.parquet' (FORMAT parquet, ROW_GROUP_SIZE 1000);
COPY (SELECT * FROM sub ORDER BY s_z, s_y NULLS LAST) TO 'sub.parquet' (FORMAT parquet, ROW_GROUP_SIZE 1000);
COPY (SELECT * FROM tiny ORDER BY t_id) TO 'tiny.parquet' (FORMAT parquet, ROW_GROUP_SIZE 1000);
COPY empty TO 'empty.parquet' (FORMAT parquet, ROW_GROUP_SIZE 1000);
```

- [ ] **Step 2: `testdata/generate_pbench.sh`** (mode `0755`):

```bash
#!/bin/bash
# Write pbench's committed parquet, or check that it is what gen.sql makes.
#   testdata/generate_pbench.sh            # (re)write testdata/pbench.sf1/
#   testdata/generate_pbench.sh --check    # regenerate into a temp dir and compare row content
# DuckDB 1.5.4 only: random() after setseed is version-specific, and a different CLI would
# silently change every pbench golden. DUCKDB=/path/to/duckdb, or duckdb in PATH.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DUCKDB=${DUCKDB:-$(command -v duckdb || true)}
[ -x "$DUCKDB" ] || { echo "error: duckdb not found; set DUCKDB" >&2; exit 1; }
VERSION=$("$DUCKDB" --version | awk '{print $1}')
[ "$VERSION" = "v1.5.4" ] || { echo "error: pbench is pinned to duckdb v1.5.4, got $VERSION" >&2; exit 1; }
OUT="$HERE/pbench.sf1"
MODE=${1:-write}
if [ "$MODE" = "--check" ]; then OUT=$(mktemp -d); trap 'rm -rf "$OUT"' EXIT; fi
mkdir -p "$OUT"
(cd "$OUT" && "$DUCKDB" -c ".read $HERE/pbench/gen.sql" >/dev/null)
if [ "$MODE" = "--check" ]; then
  for t in fact dim sub tiny empty; do
    diff=$("$DUCKDB" -noheader -csv -c "
      SELECT (SELECT count(*) FROM (SELECT * FROM read_parquet('$OUT/$t.parquet')
                                    EXCEPT ALL SELECT * FROM read_parquet('$HERE/pbench.sf1/$t.parquet')))
           + (SELECT count(*) FROM (SELECT * FROM read_parquet('$HERE/pbench.sf1/$t.parquet')
                                    EXCEPT ALL SELECT * FROM read_parquet('$OUT/$t.parquet')))")
    [ "$diff" = "0" ] || { echo "error: $t differs from gen.sql's output in $diff rows" >&2; exit 1; }
  done
  # #243's rows need both NaN signs and -0.0 to survive the parquet round trip.
  specials=$("$DUCKDB" -noheader -csv -c "
    SELECT count(*) FILTER (WHERE isnan(f_kf64) AND NOT signbit(f_kf64)) > 0
       AND count(*) FILTER (WHERE isnan(f_kf64) AND signbit(f_kf64)) > 0
       AND count(*) FILTER (WHERE f_kf64 = 0 AND signbit(f_kf64)) > 0
    FROM read_parquet('$HERE/pbench.sf1/fact.parquet')")
  [ "$specials" = "true" ] || { echo "error: fact.parquet lost NaN/-NaN/-0.0" >&2; exit 1; }
  echo "pbench.sf1 matches gen.sql"
fi
```

- [ ] **Step 3: Write and size the data.** Run `testdata/generate_pbench.sh`; then
  `du -sh testdata/pbench.sf1` (expect under 1 MB) and
  `duckdb -c "SELECT count(DISTINCT row_group_id) FROM parquet_metadata('testdata/pbench.sf1/fact.parquet')"`
  (expect `10`). If DuckDB's `signbit` or `'-NaN'` literal behaves differently from the prototype,
  stop and record it in the detail file: the spec's #243 rows depend on it.
- [ ] **Step 4: Run the check.** `testdata/generate_pbench.sh --check` → `pbench.sf1 matches gen.sql`.
  Run it a second time to show it is stable.
- [ ] **Step 5: Commit.**
```bash
git add testdata/pbench/gen.sql testdata/generate_pbench.sh testdata/pbench.sf1
git commit -m "pbench: the seeded generator and its committed sf1 parquet"
```

### Task 2: The queries

**Files:**
- Create: `testdata/pbench-queries/<name>.sql`, 63 files.

**Interfaces:**
- Produces: query names, hyphenated; the `corpus_query!` ident is the name with `-` → `_`
  (the macros `stringify!($query).replace('_', "-")`).

- [ ] **Step 1: One file per row of the spec's four query tables, the SQL verbatim, one statement,
  no trailing semicolon** (the tpch/tpcds files' style). The files and their bodies:

| file | SQL |
|---|---|
| `float64-key-group.sql` | `SELECT f_kf64, count(*) AS n FROM fact GROUP BY f_kf64` |
| `float32-key-group.sql` | `SELECT f_kf32, count(*) AS n FROM fact GROUP BY f_kf32` |
| `bool-key-group.sql` | `SELECT f_kb, count(*) AS n FROM fact GROUP BY f_kb` |
| `int8-key-group.sql` | `SELECT f_k8, count(*) AS n FROM fact GROUP BY f_k8` |
| `timestamp-ms-key-group.sql` | `SELECT f_ts_ms, count(*) AS n FROM fact GROUP BY f_ts_ms` |
| `timestamp-us-key-group.sql` | `SELECT f_ts_us, count(*) AS n FROM fact GROUP BY f_ts_us` |
| `timestamp-ns-key-group.sql` | `SELECT f_ts_ns, count(*) AS n FROM fact GROUP BY f_ts_ns` |
| `timestamp-s-key-group.sql` | `SELECT arrow_cast(f_ts_s, 'Timestamp(Second, None)') AS ts, count(*) AS n FROM fact GROUP BY 1` |
| `decimal15-key-group.sql` | `SELECT f_kdec15, count(*) AS n FROM fact GROUP BY f_kdec15` |
| `decimal38-key-group.sql` | `SELECT f_kdec38, count(*) AS n FROM fact GROUP BY f_kdec38` |
| `float64-key-join.sql` | `SELECT f_id, d_id FROM fact JOIN dim ON f_kf64 = d_kf64` |
| `decimal15-key-join.sql` | `SELECT f_id, d_id FROM fact JOIN dim ON f_kdec15 = d_kdec15` |
| `rollup-small-keys.sql` | `SELECT f_k8, f_kb, sum(f_qty) AS q FROM fact GROUP BY ROLLUP (f_k8, f_kb)` |
| `uint-key-group.sql` | `SELECT f_ku32, count(*) AS n FROM fact GROUP BY f_ku32` |
| `uint-key-join.sql` | `SELECT f_id, d_id FROM fact JOIN dim ON f_ku32 = d_ku32` |
| `inner-join-hot-keys.sql` | `SELECT f_id, d_id FROM fact JOIN dim ON f_k = d_k` |
| `left-join-null-keys.sql` | `SELECT d_id, d_k, f_id FROM dim LEFT JOIN fact ON d_k = f_k` |
| `full-join-null-keys.sql` | `SELECT d_id, d_k, f_id FROM dim FULL JOIN fact ON d_k = f_k` |
| `right-join-null-keys.sql` | `SELECT d_id, f_id, f_k FROM dim RIGHT JOIN fact ON d_k = f_k` |
| `left-join-residual.sql` | `SELECT d_id, f_id FROM dim LEFT JOIN fact ON d_k = f_k AND f_qty > d_w` |
| `right-join-residual.sql` | `SELECT d_id, f_id FROM dim RIGHT JOIN fact ON d_k = f_k AND f_qty > d_w` |
| `full-join-residual.sql` | `SELECT d_id, f_id FROM dim FULL JOIN fact ON d_k = f_k AND f_qty > d_w` |
| `exists-null-keys.sql` | `SELECT d_id FROM dim WHERE EXISTS (SELECT 1 FROM fact WHERE f_k = d_k)` |
| `not-exists-null-keys.sql` | `SELECT d_id FROM dim WHERE NOT EXISTS (SELECT 1 FROM fact WHERE f_k = d_k)` |
| `exists-or-mark.sql` | `SELECT d_id FROM dim WHERE d_w = 0 OR EXISTS (SELECT 1 FROM fact WHERE f_k = d_k)` |
| `probe-exists-residual.sql` | `SELECT f_id FROM fact WHERE EXISTS (SELECT 1 FROM dim WHERE d_k = f_k AND d_w < f_qty)` |
| `probe-not-exists-residual.sql` | `SELECT f_id FROM fact WHERE NOT EXISTS (SELECT 1 FROM dim WHERE d_k = f_k AND d_w < f_qty)` |
| `anti-null-preserved-condition.sql` | `SELECT d_id FROM dim WHERE NOT EXISTS (SELECT 1 FROM fact WHERE f_k = d_k AND d_kb)` |
| `not-in-uncorrelated.sql` | `SELECT f_id FROM fact WHERE f_k NOT IN (SELECT s_y FROM sub)` |
| `not-in-correlated.sql` | `SELECT f_id FROM fact WHERE f_k NOT IN (SELECT s_y FROM sub WHERE s_z = f_qty)` |
| `not-in-under-or.sql` | `SELECT f_id FROM fact WHERE f_qty = 0 OR f_k NOT IN (SELECT s_y FROM sub WHERE s_z = f_qty)` |
| `sparse-probe-left.sql` | `SELECT d_id, t_id FROM dim LEFT JOIN tiny ON d_k = t_k` |
| `sparse-probe-semi.sql` | `SELECT d_id FROM dim WHERE d_k IN (SELECT t_k FROM tiny)` |
| `finish-without-probe.sql` | `SELECT d_id FROM dim WHERE d_w = 0 OR EXISTS (SELECT 1 FROM tiny WHERE t_k = d_k)` |
| `sparse-build-right.sql` | `SELECT t_id, f_id FROM tiny RIGHT JOIN fact ON t_k = f_k` |
| `sparse-build-full.sql` | `SELECT t_id, f_id FROM tiny FULL JOIN fact ON t_k = f_k` |
| `sparse-build-anti.sql` | `SELECT f_id FROM fact WHERE NOT EXISTS (SELECT 1 FROM tiny WHERE t_k = f_k)` |
| `nl-inner.sql` | `SELECT f_id, t_id FROM fact JOIN tiny ON f_qty < t_v` |
| `nl-left.sql` | `SELECT f_id, t_id FROM fact RIGHT JOIN tiny ON f_qty < t_v` |
| `nl-right.sql` | `SELECT f_id, t_id FROM tiny RIGHT JOIN fact ON f_qty < t_v` |
| `nl-full.sql` | `SELECT f_id, t_id FROM fact FULL JOIN tiny ON f_qty < t_v` |
| `nl-left-semi.sql` | `SELECT t_id FROM tiny WHERE EXISTS (SELECT 1 FROM fact WHERE t_v > f_qty)` |
| `nl-left-anti.sql` | `SELECT t_id FROM tiny WHERE NOT EXISTS (SELECT 1 FROM fact WHERE t_v > f_qty)` |
| `nl-right-semi.sql` | `SELECT f_id FROM fact WHERE EXISTS (SELECT 1 FROM tiny WHERE t_v > f_qty)` |
| `nl-right-anti.sql` | `SELECT f_id FROM fact WHERE NOT EXISTS (SELECT 1 FROM tiny WHERE t_v > f_qty)` |
| `nl-mark.sql` | `SELECT t_id FROM tiny WHERE t_k = 0 OR EXISTS (SELECT 1 FROM fact WHERE t_v > f_qty)` |
| `nl-left-decimal.sql` | `SELECT f_id, t_id FROM fact RIGHT JOIN tiny ON CAST(t_v AS DECIMAL(20,0)) > CAST(f_qty AS DECIMAL(20,0))` |
| `nl-projection.sql` | `SELECT f_id FROM fact JOIN tiny ON f_qty < t_v` |
| `cross-projection.sql` | `SELECT f_id FROM fact, tiny` |
| `cross-empty-build.sql` | `SELECT e.t_id, t.t_id FROM empty e, tiny t` |
| `scalar-subquery-cross.sql` | `SELECT CASE WHEN (SELECT count(*) FROM fact WHERE f_qty BETWEEN 1 AND 20) > 100 THEN (SELECT avg(f_amount) FROM fact WHERE f_qty BETWEEN 1 AND 20) ELSE (SELECT avg(f_amount) FROM fact WHERE f_qty BETWEEN 21 AND 40) END AS b1, (SELECT count(*) FROM dim WHERE d_w >= 0) AS b2 FROM tiny WHERE t_id = 1` |
| `outer-on-true-empty.sql` | `SELECT t.t_id, e.t_id AS e_id FROM tiny t LEFT JOIN empty e ON true` |
| `ts-key-join.sql` | `SELECT f_id, d_id FROM fact JOIN dim ON f_ts_us = d_ts_us` |
| `mark-cross-residual.sql` | `SELECT d_id FROM dim WHERE d_w = 0 OR EXISTS (SELECT 1 FROM fact WHERE f_k = d_k AND f_qty > d_w)` |
| `empty-side-left-join.sql` | `SELECT t.t_id, d.d_id FROM tiny t LEFT JOIN (SELECT * FROM dim WHERE false) d ON t.t_k = d.d_k` |
| `indf-full-join.sql` | `SELECT d.d_id, t.t_id FROM dim d FULL JOIN tiny t ON d.d_k IS NOT DISTINCT FROM t.t_k` |
| `struct-key-join.sql` | `SELECT f_id, d_id FROM fact JOIN dim ON f_kstruct = d_kstruct` |
| `like-column-pattern.sql` | `SELECT d_id, t_id FROM dim JOIN tiny ON d_name LIKE t_pat` |
| `not-not-in.sql` | `SELECT f_id FROM fact WHERE NOT (f_k NOT IN (SELECT s_y FROM sub))` |
| `not-or-not-in.sql` | `SELECT f_id FROM fact WHERE NOT (f_qty = 0 OR f_k NOT IN (SELECT s_y FROM sub))` |
| `in-is-null.sql` | `SELECT f_id FROM fact WHERE (f_k IN (SELECT s_y FROM sub)) IS NULL` |
| `interval-through-join.sql` | `SELECT d_id, t.iv FROM dim LEFT JOIN (SELECT t_k, INTERVAL '1' DAY AS iv FROM tiny) t ON d_k = t_k` |
| `struct-through-join.sql` | `SELECT d_id, d_kstruct FROM dim JOIN tiny ON d_k = t_k` |

  Each of the last eleven (and `uint-key-*`) records its plan shape in the detail file, confirmed
  from its plan golden in Task 5 — none was planned on the prototype. `empty-side-left-join` and
  `indf-full-join` name their columns, not `*`: `dim`'s `d_kstruct` has no wire type (#249), so a
  `*` would make the plan "not runnable" for a reason the query is not about.

  `scalar-subquery-cross`'s `WHERE d_w >= 0` is load-bearing: `ListingOptions` collects statistics,
  so an unfiltered `count(*)` over `dim` is answered by `AggregateStatistics` as a
  `PlaceholderRowExec`, which the translator refuses (`nodes.rs:166`, #158) — the query would then
  show #158, not #63. The filter keeps every row (`d_w` is `0..39`) and forces a real aggregate.
  `cross-empty-build.sql` returns two columns both named `t_id`; if DataFusion refuses the
  duplicate name, alias them `e.t_id AS e_id, t.t_id AS t_id` and say so in the detail file. The
  collapse readings of the spec (`collapse-not-in`, `-nested-loop`, `-cross`, `-collect-left`) add
  no file: they read `not-in-uncorrelated`, `nl-inner` and `cross-projection` (Task 5, step 5).
- [ ] **Step 2: Every file parses and plans on DuckDB.**
  `for f in testdata/pbench-queries/*.sql; do (cd testdata/pbench.sf1 && duckdb -c "CREATE VIEW fact AS SELECT * FROM 'fact.parquet'; CREATE VIEW dim AS SELECT * FROM 'dim.parquet'; CREATE VIEW sub AS SELECT * FROM 'sub.parquet'; CREATE VIEW tiny AS SELECT * FROM 'tiny.parquet'; CREATE VIEW empty AS SELECT * FROM 'empty.parquet'; EXPLAIN $(cat ../../$f)" >/dev/null) || echo "FAIL $f"; done`
  → no `FAIL` line. (DuckDB answers every query; `arrow_cast` is DataFusion's — for that file,
  `duckdb_result.py` maps it in Task 4.)
- [ ] **Step 3: Commit.** `git add testdata/pbench-queries && git commit -m "pbench: sixty-three queries, one per demonstrable ticket of chain J"`.

### Task 3: The dataset in the harness, and its small-table rule

**Files:**
- Modify: `peacockdb-core/src/test_support/mod.rs:58-69` (dataset paths), `:147-161` (`Mode::knobs`)
- Modify: `peacockdb-core/src/test_support/corpus.rs:56` (`plan_at`)
- Modify: `peacockdb-core/src/planner/tests/plan_goldens.rs:36-50` (`render_bench`), `:273`, `:372`, `:447`, `:556`, `:610-662` (the per-mode `check` cases), `:672`, `:720`, `:744`, `:809`, `:889`
- Modify: `peacockdb-core/tests/test_corpus_goldens.rs:191`, `:366`, `:410`, `:537`, `:583`, `:617`, `:637`
- Modify: `peacockdb-core/tests/test_cost_model.rs:51-54`
- Test: `peacockdb-core/src/test_support/mod.rs` (a `#[cfg(test)] mod` beside `Mode`), `peacockdb-core/src/planner/tests/plan_goldens.rs`

**Interfaces:**
- Produces:
  - `pub const CORPUS_DATASETS: &[(&str, &str)]` in `test_support` — `[("tpch", "1"), ("tpcds", "1")]`
    here; Task 5 appends `("pbench", "1")` in the commit that writes pbench's goldens, so no commit
    points a dataset loop at goldens that do not exist yet
  - `pub(crate) fn small_table_bytes_for(dataset: &str) -> u64` (`0` for `"pbench"`, `SMALL_TABLE_BYTES` otherwise)
  - `impl Mode { pub(crate) fn knobs_for(&self, dataset: &str) -> PlanKnobs }` — `knobs()` with `small_table_bytes` from the line above. `knobs()` stays for callers with no dataset.

- [ ] **Step 1: The failing unit test** (in `test_support/mod.rs`, after `MODES`):

```rust
#[cfg(test)]
mod dataset_knobs {
    use super::*;

    #[test]
    fn pbench_alone_plans_with_the_small_table_rule_off() {
        for mode in &MODES {
            assert_eq!(mode.knobs_for("pbench").small_table_bytes, 0, "{}", mode.name);
            for other in ["tpch", "tpcds"] {
                assert_eq!(mode.knobs_for(other), mode.knobs(), "{other} at {}", mode.name);
            }
        }
    }

    #[test]
    fn every_corpus_dataset_has_a_queries_dir() {
        for (dataset, _) in CORPUS_DATASETS {
            assert!(queries_dir_for(dataset).is_dir(), "{dataset}");
        }
    }
}
```

- [ ] **Step 2: Run it red.** `scripts/cargo-cudf.sh test --features rust-only -p peacockdb-core --lib dataset_knobs`
  → fails to compile: `knobs_for`, `CORPUS_DATASETS` not found.
- [ ] **Step 3: Implement** in `test_support/mod.rs`:

```rust
/// The datasets the corpus covers, each at the one scale factor its goldens are written for.
/// Every list that names the datasets reads this one. (pbench joins it in Task 5, with its goldens.)
pub const CORPUS_DATASETS: &[(&str, &str)] = &[("tpch", "1"), ("tpcds", "1")];

/// pbench's tables are all under the small-table threshold, which would plan every scan as one
/// lane; it exists to show multi-lane shapes, so it plans with the rule off.
pub(crate) fn small_table_bytes_for(dataset: &str) -> u64 {
    if dataset == "pbench" { 0 } else { SMALL_TABLE_BYTES }
}

impl Mode {
    pub(crate) fn knobs_for(&self, dataset: &str) -> PlanKnobs {
        PlanKnobs { small_table_bytes: small_table_bytes_for(dataset), ..self.knobs() }
    }
}
```

  (`PlanKnobs` derives `PartialEq` already, `planner/mod.rs:45`.)
- [ ] **Step 4: Route the corpus through it.** `corpus.rs:56`: `planner::plan(&plan, mode.knobs_for(dataset))`.
  `plan_goldens.rs`'s `render_bench(dataset, sf, mode)`: `render_query(&ctx, &sql, mode.knobs_for(dataset))`
  (the `build_session_state(mode.knobs().target_partitions)` call is unchanged — lanes are not
  per-dataset). The other `planner::plan(&plan, mode.knobs())` sites in `plan_goldens.rs`
  (`:302`, `:389`, `:904`) are inside loops over a dataset: pass `mode.knobs_for(dataset)` there too.
- [ ] **Step 5: Every dataset list reads `CORPUS_DATASETS`.** Replace each
  `for (dataset, sf) in [("tpch", "1"), ("tpcds", "1")]` in `plan_goldens.rs` (`:447`, `:556`,
  `:672`, `:720`, `:744`, `:809`) and `test_corpus_goldens.rs` (`:191`, `:366`, `:410`, `:537`,
  `:583`, `:637`) with `for (dataset, sf) in CORPUS_DATASETS`; the `for dataset in ["tpch", "tpcds"]`
  loops (`plan_goldens.rs:273`, `:372`, `:889`) with `for (dataset, _) in CORPUS_DATASETS`;
  `test_corpus_goldens.rs:617`'s `["tpch", "tpcds"].contains(..)` with
  `CORPUS_DATASETS.iter().any(|(d, s)| *d == r.dataset && *s == r.sf)`; `test_cost_model.rs:51-54`'s
  `dirs` with `CORPUS_DATASETS.iter().map(|(d, s)| golden_dir_for(d, s)).collect::<Vec<_>>()`. `plan_goldens.rs:273`'s loop
  is the payload golden: keep it to `["tpch", "tpcds"]` explicitly — `PAYLOAD_QUERIES` holds no
  pbench query, and the payload golden is not pbench's to move.
- [ ] **Step 6: Run.** `scripts/cargo-cudf.sh test --features rust-only -p peacockdb-core --lib dataset_knobs` → pass.
  `... --lib plan_goldens` and `--test test_corpus_goldens --test test_cost_model` → green, every
  tpch/tpcds golden byte-identical (the override reaches nothing else; the lists still name two
  datasets).
- [ ] **Step 7: Commit.** `git commit -m "pbench in the harness: one dataset list, and its small-table rule off"`.

### Task 4: DuckDB's answers for pbench

**Files:**
- Modify: `testdata/duckdb_result.py:20`, `:104`, `:110`
- Create: `testdata/goldens/pbench.sf1/duckdb-result.txt`

**Interfaces:**
- Consumes: duckdb-oracle's generator (with its fingerprint for over-cap sections).
- Produces: `duckdb-result.txt` for pbench, read by duckdb-oracle's comparison cases.

- [ ] **Step 1: The dataset choice.** `choices=["tpch", "tpcds", "pbench"]` (`:104`), the default
  loop `["tpch", "tpcds", "pbench"]` (`:110`), the usage line (`:20`).
- [ ] **Step 2: DataFusion-only syntax.** `timestamp-s-key-group.sql` uses `arrow_cast`, which DuckDB
  lacks. In `generate`, before executing, rewrite exactly that spelling:

```python
# DataFusion's arrow_cast to a second-unit timestamp is DuckDB's TIMESTAMP_S cast; the one
# pbench query that needs it says so, and nothing else is translated.
DUCKDB_SPELLING = {
    "arrow_cast(f_ts_s, 'Timestamp(Second, None)')": "f_ts_s::TIMESTAMP_S",
}

def duckdb_text(sql):
    for ours, theirs in DUCKDB_SPELLING.items():
        sql = sql.replace(ours, theirs)
    return sql
```
  and execute `duckdb_text(query.read_text())`.
- [ ] **Step 3: Generate.** `python3 testdata/duckdb_result.py --dataset pbench` (DuckDB 1.5.4) →
  `testdata/goldens/pbench.sf1/duckdb-result.txt`, 63 sections, no `failed:`. A `failed:` is
  investigated: a DuckDB-side spelling goes into `DUCKDB_SPELLING` with its reason; anything else
  stops the task.
- [ ] **Step 4: Commit.** `git commit -m "pbench: DuckDB's answers"`.

### Task 5: The corpus lines, the registry rows and the goldens

**Files:**
- Modify: `peacockdb-core/tests/common/corpus_cases.inc` (a `// --- pbench ---` section at the end)
- Modify: `testdata/cost-registry.csv` (62 rows and lines here — 59 live, 3 commented out on #243; `int8-key-group`'s lands in Task 8)
- Create: `testdata/goldens/pbench.sf1/{<mode>.plans.txt, <mode>-mini.cpu.txt, <mode>-mini.cost.txt, mini.result.txt}`
- Test: `peacockdb-core/src/planner/tests/plan_goldens.rs` (the plans-as check, the collapse check)

**Interfaces:**
- Consumes: Task 3's `knobs_for`, Task 4's `duckdb-result.txt`, duckdb-oracle's ninth argument and `all_modes`.

- [ ] **Step 0: pbench joins the dataset list, and gets its plan-golden cases.** `CORPUS_DATASETS`
  gains `("pbench", "1")`; `dataset_knobs` gains the check that the list names the three:

```rust
    #[test]
    fn the_corpus_datasets_are_the_three() {
        let names: Vec<&str> = CORPUS_DATASETS.iter().map(|(d, _)| *d).collect();
        assert_eq!(names, ["tpch", "tpcds", "pbench"]);
    }
```

  Five plan-golden cases beside the tpcds ones (`plan_goldens.rs:640-662`):

```rust
#[tokio::test]
async fn pbench_tp1_single() {
    check("pbench", "1", mode_named("tp1_single")).await;
}
```
  — and `pbench_tp1_rowgroup`, `pbench_tp4_single`, `pbench_tp4_rowgroup`, `pbench_tp4_sized` the
  same. `NOT_RUNNABLE` (`plan_goldens.rs:442`) gains `("pbench", "timestamp-s-key-group", "240")`
  (its line cites `(#240)` by Step 0):
  `arrow_cast` to `Timestamp(Second)` is a `CastExpr` whose target the wire has no type for until
  repartition-keys adds the fbs timestamps. `struct-key-join` gains no entry here: until
  repartition-keys' Task 5c the struct field is written as `Null` without a word and the plan
  crosses; 5c refuses it and declares it on `"249"` then. Its row carries `245` (the hasher) and
  `249` (the wire), every cell off. `empty-side-left-join` and `indf-full-join` are refused
  by our planner today (no `EmptyExec` arm; a Full nested loop on #160) and so need no entry —
  their `refused:` sections name the cause, and their rows carry `155` (join-backend folds both in)
  beside it. Everything in this task through Step 10 is one PR;
  its commits keep each test green by writing the goldens (Steps 1 and 7) before the commit that
  carries this step's list change.
- [ ] **Step 0: A wire-type refusal names its ticket.** The meta tests (`plan_goldens.rs:466`,
  `:830-834`) require every "not runnable" line to cite a ticket, and the cast's refusal from
  `wire/expr_writer.rs:197-199` (`unsupported Arrow data type: Timestamp(Second, None)`) cites none.
  The one engine change of this task besides #227's (pbench.md, Restriction):

```rust
fn data_type(data_type: &DataType) -> Result<fb::DataType, PlanError> {
    convert_data_type(data_type).map_err(|why| {
        // a timestamp target waits on the fbs timestamp types (repartition-keys); every other
        // unnamed type is #249's
        let ticket = match data_type { DataType::Timestamp(..) => "#240", _ => "#249" };
        PlanError::Unsupported(format!("{why} ({ticket})"))
    })
}
```
  A `wire/tests.rs` case: a `Cast` to `Timestamp(Second, None)` is refused with `(#240)`, a `Cast`
  to `Time64(Microsecond)` with `(#249)`. `cargo test --features rust-only -p peacockdb-core --lib wire::tests`
  → green, tpch and tpcds plan goldens unchanged (no such cast in either).
- [ ] **Step 1: Plan goldens.** `UPDATE_CANONICAL=1 scripts/cargo-cudf.sh test --features rust-only -p peacockdb-core --lib plan_goldens::pbench`
  → five `testdata/goldens/pbench.sf1/<mode>.plans.txt`. Read `tp4-single.plans.txt`: `fact`'s
  `GpuLoadParquet` must say `lanes=4` (the override reached the plan); if it says `lanes=1`, Task 3
  missed a call site.
- [ ] **Step 2: Each query's first state, from the plan goldens.** For each query: refused by our
  planner (a `refused:` section) → cpu `none`, gpu `none`, `duckdb_none`, its plan cells `disabled`
  with the ticket the refusal names (expected: #153 on the three `*-residual` outer joins, #159 on the
  two `probe-*-residual`, #160 on `nl-right`, `nl-full`, `nl-left-semi`, `nl-left-anti`,
  `nl-right-semi`, `nl-right-anti`, `nl-mark`, `outer-on-true-empty`, #59 and #80 from `nulls.rs` on
  `not-exists-null-keys`, `exists-or-mark`, `anti-null-preserved-condition`, `mark-cross-residual`
  (#59), the three `not-in-*`, `not-not-in` and `not-or-not-in` (#80), and `in-is-null` — refused
  by `nulls.rs` today, its row tagged `250`, the ticket that keeps it refused after join-backend).
  Planned → cpu `all_modes`, gpu `none`, `duckdb_exact`, `data_fusion_exact`, `golden_exact`,
  `schema_validation_enabled`. A refusal the expected list does not name is a finding: its ticket goes
  on the row, or a new one is filed. `timestamp-s-key-group` plans on the cpu (no wire) and is
  `not runnable` on the device: cpu `all_modes`, gpu `none`, row tagged `240`. The two #249 rows
  (`interval-through-join`, `struct-through-join`) cross the wire today (the unnamed type is written
  as `Null`) and are declared not runnable from repartition-keys' Task 5c on: cpu `all_modes`, gpu
  `none`, row tagged `249`.
  **`int8-key-group` is not landed here:** Int8 keys hash today and no ticket blocks its device
  cells, so a row with them off would carry no ticket, which `registry.rs:229-240` refuses. It lands
  in Task 8 with the device cells the cycle proves.
- [ ] **Step 3: The lines.** Append to `corpus_cases.inc`:

```rust
// --- pbench -------------------------------------------------------------------
// The dataset whose data holds what tpch and tpcds lack (llm-wiki/tasks/pbench.md): one query per
// demonstrable ticket of chain J, cells off where a ticket blocks them. Each later task of the chain
// turns its own on. pbench plans with the small-table rule off (test_support::small_table_bytes_for).
corpus_query!(pbench, 1, bool_key_group, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, timestamp_s_key_group, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled); // device: not runnable, #240
// ... one line per remaining query, as step 2 decided (int8_key_group lands in Task 8) ...

// #243 — the cpu keys -0.0 and NaN by their bits
// corpus_query!(pbench, 1, float64_key_group, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
// #243 — the cpu keys -0.0 and NaN by their bits
// corpus_query!(pbench, 1, float32_key_group, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
// #243 — the cpu keys -0.0 and NaN by their bits
// corpus_query!(pbench, 1, float64_key_join, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
```
- [ ] **Step 4: The plans-as check, red then green.** A test in `plan_goldens.rs` that reads
  `pbench.sf1/tp4-single.plans.txt` and asserts each join query's section names the join the spec's
  "plans as" column says:

```rust
/// pbench's queries each show a ticket through one join shape; a DataFusion drift that plans one
/// differently would make it show something else, silently.
const PBENCH_JOINS: &[(&str, &str)] = &[
    ("inner-join-hot-keys", "GpuHashJoin: join_type=Inner"),
    ("left-join-null-keys", "GpuHashJoin: join_type=Left"),
    ("full-join-null-keys", "GpuHashJoin: join_type=Full"),
    ("right-join-null-keys", "GpuHashJoin: join_type=Right"),
    ("exists-null-keys", "GpuHashJoin: join_type=LeftSemi"),
    ("sparse-probe-left", "GpuHashJoin: join_type=Right"),
    ("sparse-probe-semi", "GpuHashJoin: join_type=RightSemi"),
    ("finish-without-probe", "GpuHashJoin: join_type=LeftMark"),
    ("sparse-build-right", "GpuHashJoin: join_type=Right"),
    ("sparse-build-full", "GpuHashJoin: join_type=Full"),
    ("sparse-build-anti", "GpuHashJoin: join_type=RightAnti"),
    ("nl-inner", "GpuNestedLoopJoin: join_type=Inner"),
    ("nl-left", "GpuNestedLoopJoin: join_type=Left"),
    ("nl-left-decimal", "GpuNestedLoopJoin: join_type=Left"),
    ("nl-projection", "GpuNestedLoopJoin: join_type=Inner"),
    ("cross-projection", "GpuCrossJoin"),
    ("cross-empty-build", "GpuCrossJoin"),
];

#[test]
fn every_pbench_join_plans_as_its_spec_says() {
    let path = golden_dir_for("pbench", "1").join("tp4-single.plans.txt");
    let text = std::fs::read_to_string(&path).expect("pbench's plan golden");
    let sections: std::collections::BTreeMap<String, String> = ordered_sections(&text).into_iter().collect();
    for (query, join) in PBENCH_JOINS {
        let body = sections.get(*query).unwrap_or_else(|| panic!("no section for {query}"));
        assert!(body.contains(join), "{query} does not plan as `{join}`:\n{body}");
    }
}
```
  The refused queries are not in the list: their sections are refusals until join-backend, which
  extends `PBENCH_JOINS` as it lifts each. Write the list first with a deliberately wrong entry
  (`("inner-join-hot-keys", "GpuHashJoin: join_type=Left")`), run it red, correct it, run green.
  If `ordered_sections` yields `(String, String)` pairs under another name, use the helper
  `every_query_that_cannot_cross_the_wire_is_declared…` uses (`:452`).
- [ ] **Step 5: The collapse check.** In the same file:

```rust
/// pbench.md's "collapse to one lane": with the small-table rule off, `fact` is four lanes at tp4,
/// and a keyless join merges both sides to one. The plan says so; this reads it.
#[test]
fn pbench_shows_a_keyless_join_collapsing_four_lanes_to_one() {
    let path = golden_dir_for("pbench", "1").join("tp4-single.plans.txt");
    let text = std::fs::read_to_string(&path).expect("pbench's plan golden");
    let sections: std::collections::BTreeMap<String, String> = ordered_sections(&text).into_iter().collect();
    for query in ["nl-inner", "cross-projection"] {
        let body = &sections[query];
        assert!(body.contains("GpuMergePartitions"), "{query} does not merge:\n{body}");
        assert!(body.contains("lanes=4"), "{query} never had four lanes:\n{body}");
    }
}
```
  `collapse-not-in` is read by join-backend once the rewrite exists; `collapse-collect-left`: search
  the five pbench plan goldens for a join under `GpuMergePartitions` on both sides that is not keyless;
  record what was found (or "not reached") in the detail file.
- [ ] **Step 6: The registry rows.** One row per query in `cost-registry.csv`
  (`dataset,sf,query,<5 plan>,<5 cpu>,<5 gpu>,plan_status,features,tickets`), query with underscores
  as the other rows spell it. Plan cells from the plan golden (`enabled` where it plans, `disabled`
  where refused, as `tpcds,1,q28` is); cpu/gpu cells `enabled` exactly where the line enables them,
  else `disabled`; `plan_status` `ok`; `features` from `registry.rs:91`'s codes (`anti_join`,
  `semi_join`, `outer_join`, `nested_loop_join`, `cross_join`, `rollup`, `corr_subquery`, `avg`);
  `tickets` the expected blockers from the spec's "off on" column — every gpu cell is off now, so
  every row carries at least one ticket (the key-type rows: #206/#240/#95; `timestamp-s-key-group`
  `240`; the joins: #152 and their own). The three float rows: every cell `disabled`, tickets
  `243`. No `int8-key-group` row yet (Step 2).
- [ ] **Step 7: cpu goldens and the first run.** `UPDATE_CANONICAL=1 scripts/cargo-cudf.sh test --features rust-only -p peacockdb-core --test test_cpu_corpus -- pbench`
  → `<mode>-mini.cpu.txt`, `-mini.cost.txt`, `mini.result.txt` for pbench. Then without the variable:
  every enabled cpu cell green. A cell that fails is turned off with the ticket it fails on —
  expected: #190 on a nested loop whose projection narrows (`nl-projection`, possibly `nl-inner`,
  `nl-left`, `nl-left-decimal`), #207 on `cross-projection`, #208 on `cross-empty-build`, #212 on the
  three `sparse-build-*` at the tp4 modes, #189 on `rollup-small-keys`, `uint-key-group` and
  `uint-key-join` at the tp4 modes (comet has no unsigned arm; repartition-keys closes both); any
  other is a finding.
- [ ] **Step 8: The DuckDB oracle.** `scripts/cargo-cudf.sh test --features rust-only -p peacockdb-core --test test_cpu_corpus -- duckdb_pbench`
  → each line's DuckDB case. A row that differs is set as duckdb-oracle step 6 says:
  `duckdb_approx` for decimal `avg` digits (expected: `scalar-subquery-cross`; within one unit in its
  last place), `duckdb_divergent(<ticket>, <positions>)`
  for a real divergence (a new ticket if none exists), `duckdb_fingerprint` for a section over the cap
  (expected: the large join answers, e.g. `float64-key-join` once enabled, `inner-join-hot-keys`).
- [ ] **Step 9: Registry and golden tests.** `scripts/cargo-cudf.sh test --features rust-only -p peacockdb-core --test test_cpu_corpus --test test_corpus_goldens --test test_cost_model`
  and `--lib plan_goldens` → green, including both registry directions and
  `every_device_cell_has_a_cpu_cell_at_the_same_mode`.
- [ ] **Step 10: Commit** (two commits, data then rows):
```bash
git add testdata/goldens/pbench.sf1 && git commit -m "pbench: plan, cpu, cost and result goldens"
git add peacockdb-core testdata/cost-registry.csv && git commit -m "pbench: in the dataset list; 62 corpus lines and rows, three held on #243"
```
  The goldens commit comes first and no test reads them yet (`CORPUS_DATASETS` still names two), so
  it is green; the second commit points every list at them, also green.

### Task 6: The widget renders pbench

**Files:**
- Modify: `cost-report/src/main.rs:477-479`, `:1320`, `:2129-2133`
- Test: `cost-report/src/main.rs` (its `#[cfg(test)]` module)

- [ ] **Step 1: The failing test** (in the tests module, beside `the_pr_comment_fits_under_the_body_cap`):

```rust
#[test]
fn the_widget_renders_a_pbench_section_beside_the_benchmarks() {
    let testdata = Path::new(env!("CARGO_MANIFEST_DIR")).join("../testdata");
    let registry = Registry::load(&testdata.join("cost-registry.csv"));
    let datasets = all_datasets(&testdata, &registry);
    let labels: Vec<&str> = datasets.iter().map(|d| d.label).collect();
    assert_eq!(labels, ["TPC-H", "TPC-DS", "pbench"]);
    let pbench = &datasets[2];
    assert!(!pbench.rows.is_empty(), "pbench has registry rows");
    let html = render_html(&datasets, "https://p/", &no_links(), None, None);
    assert!(html.contains("pbench"), "the page names the section");
}
```
  (`no_links()` is the tests module's helper at `:1899`; `Dataset`'s label field is `label` (`:224`); if it has another
  name, use it.)
- [ ] **Step 2: Run red.** `cargo test -p cost-report the_widget_renders_a_pbench_section` → `all_datasets` not found.
- [ ] **Step 3: Implement.** One constructor both `main` and the tests call:

```rust
/// Every dataset the widget renders, in its order. The one list: main and the tests read it.
fn all_datasets(testdata: &Path, registry: &Registry) -> Vec<Dataset> {
    vec![
        build_dataset("TPC-H", "testdata/goldens/tpch.sf1", "testdata/tpch-queries", &testdata.join("goldens/tpch.sf1"), registry, "tpch"),
        build_dataset("TPC-DS", "testdata/goldens/tpcds.sf1", "testdata/tpcds-queries", &testdata.join("goldens/tpcds.sf1"), registry, "tpcds"),
        build_dataset("pbench", "testdata/goldens/pbench.sf1", "testdata/pbench-queries", &testdata.join("goldens/pbench.sf1"), registry, "pbench"),
    ]
}
```
  `main` (`:477-479`): `let datasets = all_datasets(&testdata, &registry);` and its later uses take
  `&datasets` (a `Vec` slices as the array did). `:2129-2133`: `let datasets = all_datasets(&testdata, &registry);`.
  `collect_cost_goldens` (`:1320`): add `"goldens/pbench.sf1"`. pbench has no `qN.duckdb_cost.txt`
  (its queries are named, like tpch's `hash-join`), so its rows render with no DuckDB cost, as those do.
- [ ] **Step 4: Run.** `cargo test -p cost-report` → green, `the_pr_comment_fits_under_the_body_cap`
  included; note its printed margin in the detail file (the body grew by ~62 rows).
- [ ] **Step 5: Commit.** `git commit -m "cost-report: the widget renders pbench beside TPC-H and TPC-DS"`.

### Task 6b: One larger tick for a fully enabled row

**Files:**
- Modify: `cost-report/src/main.rs:710-740` (`mode_cell_html`, `mode_cell_md`), the page's CSS in `render_html`
- Test: `cost-report/src/main.rs` (tests module, beside `state_glyphs_are_distinct_and_total`, `:1544`)

- [ ] **Step 1: The failing tests.**

```rust
#[test]
fn a_cell_with_all_five_modes_enabled_shows_one_larger_tick() {
    let row = row_with_states(["enabled"; 5]);
    assert_eq!(mode_cell_md(&row, ModeGroup::Plan), "✔");
    let html = mode_cell_html(&row, &no_links(), &dataset_for_tests(), ModeGroup::Plan);
    assert_eq!(html.matches('✓').count(), 1, "{html}");
    assert!(html.contains("class=\"all\""), "{html}");
}

#[test]
fn a_cell_with_any_mode_off_keeps_five_glyphs() {
    let row = row_with_states(["enabled", "enabled", "disabled", "enabled", "enabled"]);
    assert_eq!(mode_cell_md(&row, ModeGroup::Plan), "✓✓✗✓✓");
}
```
  `row_with_states` and `dataset_for_tests` are the tests module's fixtures for a `Row` and a
  `Dataset`; if they are named otherwise, use those (the module builds both for the existing
  mode-cell tests).
- [ ] **Step 2: Run red.** `cargo test -p cost-report a_cell_with` → the md cell is `✓✓✓✓✓`.
- [ ] **Step 3: Implement.**

```rust
fn all_enabled(r: &Row, group: ModeGroup) -> bool {
    MODES.iter().all(|mode| r.state(&group.column(mode)) == "enabled")
}

fn mode_cell_md(r: &Row, group: ModeGroup) -> String {
    if all_enabled(r, group) {
        return "✔".to_string(); // one heavier mark: the comment's HTML table strips font sizes
    }
    MODES.iter().map(|mode| state_glyph(r.state(&group.column(mode)))).collect::<Vec<_>>().join("")
}
```
  In `mode_cell_html`, before building the five glyphs:

```rust
    if all_enabled(r, group) {
        let first = MODES[0];
        let stem = group.file(first);
        let at = d.section_lines.get(&stem).and_then(|l| l.get(&r.stem())).map(|l| format!("#L{l}")).unwrap_or_default();
        let tick = "<span class=\"all\" title=\"all five modes\">✓</span>";
        return match links.golden_url(d.canon_rel, &stem, "txt") {
            Some(url) => format!("<td class=\"mode\"><a href=\"{url}{at}\">{tick}</a></td>"),
            None => format!("<td class=\"mode\">{tick}</td>"),
        };
    }
```
  CSS in `render_html`: `td.mode .all{font-size:1.5em;line-height:1;}`.
- [ ] **Step 4: Run.** `cargo test -p cost-report` → green, `the_pr_comment_fits_under_the_body_cap`
  included (a fully enabled row's cell shrinks from five glyphs to one).
- [ ] **Step 5: Commit.** `git commit -m "cost-report: one larger tick for a row with all five modes on"`.

### Task 6c: Nullability is checked on both engines (#227)

**Files:**
- Modify: `peacockdb-core/src/executor/cpu_backend/mod.rs:309` (`declared_as`)
- Modify: `peacockdb-core/src/test_support/schema_validation.rs` (`gpu_schema_validator`, `cpu_schema_validator`)
- Test: `peacockdb-core/src/executor/cpu_backend/tests/` (a new case beside the existing `declared_as` ones), `peacockdb-core/src/test_support/` tests of the validators

- [ ] **Step 1: The failing tests.**

```rust
#[test]
fn a_null_in_a_column_declared_non_nullable_is_refused_by_name() {
    let declared = Arc::new(Schema::new(vec![Field::new("k", DataType::Int32, false)]));
    let batch = RecordBatch::try_new(
        Arc::new(Schema::new(vec![Field::new("k", DataType::Int32, true)])),
        vec![Arc::new(Int32Array::from(vec![Some(1), None]))],
    ).unwrap();
    let err = declared_as(batch, &declared).expect_err("k is declared non-nullable");
    assert!(err.message.contains("k") && err.message.contains("non-nullable"), "{}", err.message);
}

#[test]
fn an_equal_schema_is_still_checked_for_nulls() {
    // the early return for an equal schema used to skip every check
    let schema = Arc::new(Schema::new(vec![Field::new("k", DataType::Int32, false)]));
    let batch = unsafe_batch_with_null(&schema); // RecordBatch::try_new_with_options, validation off
    assert!(declared_as(batch, &schema).is_err());
}
```
  `unsafe_batch_with_null` builds the batch with `RecordBatchOptions` that skip arrow's own
  validation, the way a kernel's output can arrive. The validator test: a hook over a node
  declaring `k: Int32 NOT NULL` fed a handle whose `k` has `null_count 1` reports the divergence
  naming `k`.
- [ ] **Step 2: Run red.** `scripts/cargo-cudf.sh test --features rust-only -p peacockdb-core --lib declared_as` → the null passes.
- [ ] **Step 3: Implement.** At the top of `declared_as`, before the early return:

```rust
    for (column, field) in batch.columns().iter().zip(declared.fields().iter()) {
        if !field.is_nullable() && column.null_count() > 0 {
            return Err(BackendError::new(format!(
                "{} holds {} NULL(s) where the node declares it non-nullable (#227)",
                field.name(), column.null_count())));
        }
    }
```
  In both validators, the same comparison against the handle's (or batch's) per-column null
  counts, reported as a schema divergence naming the column.
- [ ] **Step 4: Run** the cpu tier and `test_cpu_corpus` over all three datasets. A violation is a
  finding: file its ticket, turn its cells off on it, list it in the detail file. Device: one cycle
  with schema validation, the same rule.
- [ ] **Step 5: Commit.** `git commit -m "#227: both engines refuse a NULL where the node declares none"`.

### Task 7: CI, the scripts and exec_model

**Files:**
- Modify: `.github/workflows/pipeline.yml:178-180` (dataset-matrix's DuckDB step), `:503-509` (the GPU job's rsync list)
- Modify: `scripts/build-test.sh:196`, `:200`
- Modify: `scripts/build-test-shadgpu.sh:251`
- Modify: `scripts/exec_model/tests/corpus.py:77-79`

- [ ] **Step 1: The determinism check in CI.** In the dataset-matrix step that downloads DuckDB 1.5.4
  and runs `generate_testdata.sh` (`pipeline.yml:~165-180`), after the two generate lines:
```yaml
          # pbench is committed, not generated: this proves the committed parquet is gen.sql's.
          testdata/generate_pbench.sh --check
```
- [ ] **Step 2: Ship pbench to shad-gpu.** `pipeline.yml:503`'s `rsync_retry` list gains
  `testdata/pbench-queries testdata/pbench.sf1`; `build-test-shadgpu.sh:251`'s
  `resilient_rsync -a testdata/tpch-queries testdata/tpcds-queries` gains `testdata/pbench-queries testdata/pbench.sf1`.
  The "Generate SF-1 testdata" step (`pipeline.yml:~522-530`) is unchanged: pbench needs no generation.
- [ ] **Step 3: The testdata kinds.** `build-test.sh:196`: `parquet) echo "tpch.sf1 tpcds.sf1 tpch.minimal pbench.sf1" ;;`;
  `:200`: `queries) echo "tpch-queries tpcds-queries tpch-vec-queries pbench-queries" ;;`.
- [ ] **Step 4: exec_model.** `corpus.py:77-79`'s `_DATASETS` gains `"pbench": ("pbench.sf1",)`. exec_model
  models no pbench query (`plans.py`'s `BENCHES` is unchanged); the loader only knows where it is.
- [ ] **Step 5: Not changed, checked.** `check_s3_datasets.py` and `dataset_checks.py` key on S3 bucket
  names and tpch/tpcds table specs; pbench is in neither, so neither changes — say so in the PR.
  `testdata/.gitignore` ignores `/tpch.sf*/` and `/tpcds.sf*/` only, so `pbench.sf1/` is tracked:
  `git check-ignore testdata/pbench.sf1/fact.parquet` → no output.
- [ ] **Step 6: Run.** `bash -n scripts/build-test.sh scripts/build-test-shadgpu.sh`;
  `python3 -m pytest scripts/exec_model/tests -k corpus -q` green; the CI wiring guard
  `scripts/cargo-cudf.sh test --features rust-only -p peacockdb-core --test test_ci_coverage` green.
- [ ] **Step 7: Commit.** `git commit -m "pbench in CI and the host scripts: shipped, checked, never generated"`.

- **`build-test.md`'s goldens diagram** gains `pbench.sf1` beside tpch and tpcds at its top:
  `testdata/pbench/gen.sql` → `generate_pbench.sh` → `pbench.sf1` (parquet, committed), feeding the
  same branches; its table row for the dataset says committed, not generated per host.


### Task 8: The device cells

**Files:**
- Modify: `peacockdb-core/tests/common/corpus_cases.inc` (pbench `gpu_modes`), `testdata/cost-registry.csv`
- Create: `testdata/goldens/pbench.sf1/gpu-result.txt` (duckdb-oracle's device record)

- [ ] **Step 1: One cycle, every gpu cell.** With every pbench line's `gpu_modes` set to `all_modes`
  in a scratch edit, `scripts/build-test-shadgpu.sh` with `PCK_TEST_FILTER='gpu_pbench_'` and the
  `gpu-result.txt` regeneration variable duckdb-oracle defines; pull the goldens home.
- [ ] **Step 2: Keep what passed.** Each line's `gpu_modes` becomes exactly the modes that passed; the
  registry's gpu cells follow. `int8-key-group` lands now, its line and row with it: cpu `all_modes`,
  gpu the modes that passed (expected `all_modes`, since Int8 narrows to INT32 in the kernel); any
  mode that fails gets the ticket it fails on, so no off cell is left without one. Expected to pass:
  the key-type groups at tp1 (no shuffle), `int8-key-group` at every mode. Expected to fail: the tp4 key-type rows (#206, #240, #95), every join at a second probe
  batch (#152), `scalar-subquery-cross` (#63), `nl-left-decimal` (#215). A failure on a ticket the spec
  does not expect is a finding: the ticket on the row, or a new one.
- [ ] **Step 3: Run.** `scripts/cargo-cudf.sh test --features rust-only -p peacockdb-core --test test_cpu_corpus`
  (registry both ways, `every_device_cell_has_a_cpu_cell_at_the_same_mode`) and the DuckDB cases over
  `gpu-result.txt` → green.
- [ ] **Step 4: Commit.** `git commit -m "pbench: the device cells that pass, enabled"`.

### Task 9: The wiki and the record

**Files:**
- Modify: `llm-wiki/build-test.md` (the dataset table near `:598`, the testdata-kinds table near `:645`, the tier counts at `:25`, the gpu tier line `:398`)
- Modify: `llm-wiki/architecture.md` (the modes section: pbench and its small-table override)
- Create: `llm-wiki/tasks/pbench-detail.md`

- [ ] **Step 1: build-test.md.** A `pbench.sf1` row in the dataset table: "committed, ~0.5 MB,
  `generate_pbench.sh`, local ✓ verda ✓ shad-gpu ✓ (rsynced), no S3 bucket". The goldens table gains
  pbench beside tpch/tpcds. Tier counts from the runs (cpu `test_cpu_corpus` grows by the pbench cpu
  cells, `--lib` by the five plan cases and the three tests; gpu by the enabled cells).
- [ ] **Step 2: architecture.md.** One paragraph under the modes: pbench plans with
  `small_table_bytes = 0` (`test_support::small_table_bytes_for`), so its scans split by row group at
  tp4 where tpch and tpcds would keep a small table in one lane.
- [ ] **Step 3: The detail file.** The sizes, the `--check` output, each query's first plan and cells,
  every finding with its ticket, the collapse readings, the widget's comment margin.
- [ ] **Step 4: Commit.** `git commit -m "pbench: the wiki and the record"`.

---

## Self-review

- **Spec coverage.** The data (Task 1), the queries (2), the small-table override (3), DuckDB answers
  (4), lines/registry/goldens/#243 commented lines/collapse readings and the wire-type refusal's
  ticket (5), the widget (6, 6b), #227's nullability check (6c), CI, scripts,
  S3 checks and exec_model (7), the device cycle (8), wiki (9). The spec's verification bar maps to
  Tasks 3, 5, 6, 8.
- **Placeholders.** Task 5 step 3 shows one live line and the three commented lines in full; the
  other 57 live lines follow step 2's rule exactly and differ only in name, modes and oracle.
- **Types.** `CORPUS_DATASETS`, `small_table_bytes_for`, `Mode::knobs_for`, `all_datasets`,
  `PBENCH_JOINS` are defined once and used with the same names.
- **Review Focus.** Each of the five lines is pinned in the task it names.
