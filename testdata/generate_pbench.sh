#!/bin/bash
# Write pbench's committed parquet, or check that it is what gen.sql makes.
#   testdata/generate_pbench.sh            # (re)write testdata/pbench.sf1/
#   testdata/generate_pbench.sh --check    # regenerate into a temp dir and compare row content
# DuckDB 1.5.4 only: random() after setseed is version-specific, and a different CLI would
# silently change every pbench golden. DUCKDB=/path/to/duckdb, or duckdb in PATH.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
COMMITTED="$HERE/pbench.sf1"

# Argument first, before anything is written: `--chek` used to fall through to the write mode
# and overwrite the committed data, so a typo shipped files rather than failing.
usage() { echo "usage: generate_pbench.sh [--check]" >&2; exit 1; }
[ "$#" -le 1 ] || { echo "error: at most one argument, got $#" >&2; usage; }
case "${1-}" in
"") MODE=write ;;
--check) MODE=check ;;
*) echo "error: unrecognized argument '$1'" >&2; usage ;;
esac

DUCKDB=${DUCKDB:-$(command -v duckdb || true)}
[ -x "$DUCKDB" ] || { echo "error: duckdb not found; set DUCKDB" >&2; exit 1; }
VERSION=$("$DUCKDB" --version | awk '{print $1}')
[ "$VERSION" = "v1.5.4" ] || { echo "error: pbench is pinned to duckdb v1.5.4, got $VERSION" >&2; exit 1; }
OUT="$COMMITTED"
if [ "$MODE" = check ]; then OUT=$(mktemp -d); trap 'rm -rf "$OUT"' EXIT; fi
mkdir -p "$OUT"
(cd "$OUT" && "$DUCKDB" -c ".read $HERE/pbench/gen.sql" >/dev/null)
[ "$MODE" = check ] || exit 0

for t in fact dim sub tiny empty; do
  diff=$("$DUCKDB" -noheader -csv -c "
    SELECT (SELECT count(*) FROM (SELECT * FROM read_parquet('$OUT/$t.parquet')
                                  EXCEPT ALL SELECT * FROM read_parquet('$COMMITTED/$t.parquet')))
         + (SELECT count(*) FROM (SELECT * FROM read_parquet('$COMMITTED/$t.parquet')
                                  EXCEPT ALL SELECT * FROM read_parquet('$OUT/$t.parquet')))")
  [ "$diff" = "0" ] || { echo "error: $t differs from gen.sql's output in $diff rows" >&2; exit 1; }
done

# `$1`'s fact float specials: NaN, -NaN, -0.0 and 0.0 on f_kf64, then NaN, -NaN and -0.0 on
# f_kf32. The f_kf32 0.0 count is f_kf64's and is not read twice.
fact_specials() {
  "$DUCKDB" -noheader -csv -c "
    SELECT count(*) FILTER (WHERE isnan(f_kf64) AND NOT signbit(f_kf64))
        || ' ' || count(*) FILTER (WHERE isnan(f_kf64) AND signbit(f_kf64))
        || ' ' || count(*) FILTER (WHERE f_kf64 = 0 AND signbit(f_kf64))
        || ' ' || count(*) FILTER (WHERE f_kf64 = 0 AND NOT signbit(f_kf64))
        || ' ' || count(*) FILTER (WHERE isnan(f_kf32) AND NOT signbit(f_kf32))
        || ' ' || count(*) FILTER (WHERE isnan(f_kf32) AND signbit(f_kf32))
        || ' ' || count(*) FILTER (WHERE f_kf32 = 0 AND signbit(f_kf32))
      FROM read_parquet('$1/fact.parquet')"
}

# `$1`'s dim float specials: NaN, -NaN and -0.0 on d_kf64, then NaN and -0.0 on d_kf32.
dim_specials() {
  "$DUCKDB" -noheader -csv -c "
    SELECT count(*) FILTER (WHERE isnan(d_kf64) AND NOT signbit(d_kf64))
        || ' ' || count(*) FILTER (WHERE isnan(d_kf64) AND signbit(d_kf64))
        || ' ' || count(*) FILTER (WHERE d_kf64 = 0 AND signbit(d_kf64))
        || ' ' || count(*) FILTER (WHERE isnan(d_kf32) AND NOT signbit(d_kf32))
        || ' ' || count(*) FILTER (WHERE d_kf32 = 0 AND signbit(d_kf32))
      FROM read_parquet('$1/dim.parquet')"
}

row_groups() {
  "$DUCKDB" -noheader -csv -c "
    SELECT count(DISTINCT row_group_id) FROM parquet_metadata('$1/fact.parquet')"
}

# #243's rows need both NaN signs and -0.0 to survive the parquet round trip, and EXCEPT ALL
# cannot see it: DuckDB's set operations compare -0.0 equal to 0.0 and NaN equal to -NaN, so a
# canonicalized column passes the comparison above row for row. The counts are what catches it,
# exact rather than nonzero because the writer loses the signs SILENTLY and PARTLY — a
# dictionary-encoded page keeps whichever sign reached its dictionary first.
# Over the regeneration AND the committed file. Reading the committed file alone would pass a
# gen.sql that stopped producing the specials at all, which is what dropping
# DICTIONARY_SIZE_LIMIT 0 does, and that is the one thing this step is cited in CI as proving.
FACT_SPECIALS="199 204 191 563 199 204 191"
DIM_SPECIALS="10 8 8 10 8"
for dir in "$OUT" "$COMMITTED"; do
  got=$(fact_specials "$dir")
  [ "$got" = "$FACT_SPECIALS" ] || {
    echo "error: $dir/fact.parquet's float specials are '$got', not '$FACT_SPECIALS'" >&2
    exit 1; }
  got=$(dim_specials "$dir")
  [ "$got" = "$DIM_SPECIALS" ] || {
    echo "error: $dir/dim.parquet's float specials are '$got', not '$DIM_SPECIALS'" >&2
    exit 1; }
  got=$(row_groups "$dir")
  [ "$got" = "10" ] || { echo "error: $dir/fact.parquet has $got row groups, not 10" >&2; exit 1; }
done
echo "pbench.sf1 matches gen.sql"
