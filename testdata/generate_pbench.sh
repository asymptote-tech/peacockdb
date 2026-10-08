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
[ "$MODE" = "--check" ] || exit 0

for t in fact dim sub tiny empty; do
  diff=$("$DUCKDB" -noheader -csv -c "
    SELECT (SELECT count(*) FROM (SELECT * FROM read_parquet('$OUT/$t.parquet')
                                  EXCEPT ALL SELECT * FROM read_parquet('$HERE/pbench.sf1/$t.parquet')))
         + (SELECT count(*) FROM (SELECT * FROM read_parquet('$HERE/pbench.sf1/$t.parquet')
                                  EXCEPT ALL SELECT * FROM read_parquet('$OUT/$t.parquet')))")
  [ "$diff" = "0" ] || { echo "error: $t differs from gen.sql's output in $diff rows" >&2; exit 1; }
done

# #243's rows need both NaN signs and -0.0 to survive the parquet round trip, and EXCEPT ALL
# cannot see it: DuckDB's set operations compare -0.0 equal to 0.0 and NaN equal to -NaN, so a
# canonicalized column passes the comparison above row for row. The counts are what catches it.
# They are exact rather than nonzero because the writer loses the signs SILENTLY and PARTLY:
# a dictionary-encoded page keeps whichever sign reached its dictionary first, so a per-row-group
# count is the only thing that distinguishes the committed data from the same data rewritten
# without DICTIONARY_SIZE_LIMIT 0.
specials=$("$DUCKDB" -noheader -csv -c "
  SELECT count(*) FILTER (WHERE isnan(f_kf64) AND NOT signbit(f_kf64))
      || ' ' || count(*) FILTER (WHERE isnan(f_kf64) AND signbit(f_kf64))
      || ' ' || count(*) FILTER (WHERE f_kf64 = 0 AND signbit(f_kf64))
      || ' ' || count(*) FILTER (WHERE f_kf64 = 0 AND NOT signbit(f_kf64))
      || ' ' || count(*) FILTER (WHERE isnan(f_kf32) AND NOT signbit(f_kf32))
      || ' ' || count(*) FILTER (WHERE isnan(f_kf32) AND signbit(f_kf32))
      || ' ' || count(*) FILTER (WHERE f_kf32 = 0 AND signbit(f_kf32))
  FROM read_parquet('$HERE/pbench.sf1/fact.parquet')")
[ "$specials" = "199 204 191 563 199 204 191" ] || {
  echo "error: fact.parquet's float specials are '$specials', not '199 204 191 563 199 204 191'" >&2
  exit 1; }
specials=$("$DUCKDB" -noheader -csv -c "
  SELECT count(*) FILTER (WHERE isnan(d_kf64) AND NOT signbit(d_kf64))
      || ' ' || count(*) FILTER (WHERE isnan(d_kf64) AND signbit(d_kf64))
      || ' ' || count(*) FILTER (WHERE d_kf64 = 0 AND signbit(d_kf64))
      || ' ' || count(*) FILTER (WHERE isnan(d_kf32) AND NOT signbit(d_kf32))
      || ' ' || count(*) FILTER (WHERE d_kf32 = 0 AND signbit(d_kf32))
  FROM read_parquet('$HERE/pbench.sf1/dim.parquet')")
[ "$specials" = "10 8 8 10 8" ] || {
  echo "error: dim.parquet's float specials are '$specials', not '10 8 8 10 8'" >&2; exit 1; }

groups=$("$DUCKDB" -noheader -csv -c "
  SELECT count(DISTINCT row_group_id) FROM parquet_metadata('$HERE/pbench.sf1/fact.parquet')")
[ "$groups" = "10" ] || { echo "error: fact.parquet has $groups row groups, not 10" >&2; exit 1; }
echo "pbench.sf1 matches gen.sql"
