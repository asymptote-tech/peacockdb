-- pbench sf1: the shapes tpch and tpcds lack (llm-wiki/tasks/pbench.md).
-- Deterministic under DuckDB 1.5.4 with threads=1: random() after setseed is version-specific,
-- so generate_pbench.sh refuses any other version.
-- Run from the output directory: COPY writes relative paths.
SET threads = 1;
SELECT setseed(0.42);

-- fact: 20,000 rows. f_k is the skewed, NULL-bearing join key — 5% NULL, half the rows on
-- 0..2, the rest spread over 0..1000. One column per type the shuffle must hash, each
-- carrying the specials of its type: NaN of both signs and -0.0 in the floats, NULLs
-- everywhere a NULL key is a shape to test, values past i32::MAX in f_ku32.
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
  CASE WHEN i % 113 = 0 THEN NULL
       ELSE ((i % 300) * 1234567890123.0001::DECIMAL(27,4))::DECIMAL(38,4) END AS f_kdec38,
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

-- dim: 2,000 rows, every d_k twice, so a join against fact is many-to-many; 1 in 40 NULL.
-- Its floats hold -0.0 and NaN of both signs too, so float64-key-join meets them on both sides.
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
  (((i // 2) % 300) * 1234567890123.0001::DECIMAL(27,4))::DECIMAL(38,4) AS d_kdec38,
  (TIMESTAMP '2024-01-01' + to_seconds((i // 2) % 400))::TIMESTAMP_S AS d_ts_s,
  (TIMESTAMP '2024-01-01' + to_milliseconds((i // 2) % 400))::TIMESTAMP_MS AS d_ts_ms,
  (TIMESTAMP '2024-01-01' + to_microseconds((i // 2) % 400))::TIMESTAMP AS d_ts_us,
  (TIMESTAMP '2024-01-01' + to_microseconds((i // 2) % 400))::TIMESTAMP_NS AS d_ts_ns,
  (DATE '2024-01-01' + ((i // 2) % 365)::INT) AS d_dt,
  'v' || ((i // 2) % 97)::VARCHAR AS d_s,
  ((i * 2654435761) % 4294967296)::UINTEGER AS d_ku32,
  {'a': (i % 7)::INT, 'b': 'v' || (i % 5)::VARCHAR} AS d_kstruct,
  (i % 40)::INT AS d_w,
  'name' || i::VARCHAR AS d_name
FROM range(2000) t(i);

-- sub: the uncorrelated and correlated NOT IN subquery source. s_y is 1 in 25 NULL, which is
-- what makes NOT IN over it a three-valued answer.
CREATE TABLE sub AS
SELECT (i % 20)::INT AS s_z, CASE WHEN i % 25 = 0 THEN NULL ELSE (i * 7 % 1000)::INT END AS s_y
FROM range(200) t(i);

-- tiny: two distinct keys over eight rows, so a four-lane probe leaves lanes with no batch.
-- t_pat holds LIKE patterns over dim's d_name.
CREATE TABLE tiny AS
SELECT i::INT AS t_id, (i % 2)::INT AS t_k, (i * 5)::INT AS t_v,
  (['name1%', '%7', 'name2_', '%', 'name1999', 'x%', '%0', 'name%5'])[i + 1] AS t_pat
FROM range(8) t(i);

-- empty: tiny's schema and no rows, for the zero-row build and probe sides.
CREATE TABLE empty AS SELECT * FROM tiny WHERE false;

COPY (SELECT * FROM fact ORDER BY f_id) TO 'fact.parquet' (FORMAT parquet, ROW_GROUP_SIZE 1000, DICTIONARY_SIZE_LIMIT 0);
COPY (SELECT * FROM dim ORDER BY d_id) TO 'dim.parquet' (FORMAT parquet, ROW_GROUP_SIZE 1000, DICTIONARY_SIZE_LIMIT 0);
COPY (SELECT * FROM sub ORDER BY s_z, s_y NULLS LAST) TO 'sub.parquet' (FORMAT parquet, ROW_GROUP_SIZE 1000);
COPY (SELECT * FROM tiny ORDER BY t_id) TO 'tiny.parquet' (FORMAT parquet, ROW_GROUP_SIZE 1000);
COPY empty TO 'empty.parquet' (FORMAT parquet, ROW_GROUP_SIZE 1000);
