#!/bin/bash
#
# Generate TPC-H test data as Parquet files using DuckDB.
#
# Usage:
#   ./testdata/generate_testdata.sh             # generate tpch.sf1
#   ./testdata/generate_testdata.sh --sf 10     # generate tpch.sf10
#
# Requires duckdb in PATH, or set DUCKDB=/path/to/duckdb.

set -euo pipefail

SF=1
while [ $# -gt 0 ]; do
  case "$1" in
    --sf) SF="$2"; shift ;;
    *) echo "Unknown flag: $1"; exit 1 ;;
  esac
  shift
done

DUCKDB=${DUCKDB:-$(which duckdb 2>/dev/null)} || { echo "error: duckdb not found in PATH"; exit 1; }
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OUTDIR="${SCRIPT_DIR}/tpch.sf${SF}"

if [ -d "$OUTDIR" ]; then
  echo "Directory $OUTDIR already exists, skipping generation."
  echo "Run testdata/clean_testdata.sh first to regenerate."
  exit 0
fi

mkdir -p "$OUTDIR"

echo "Generating TPC-H SF=${SF} into ${OUTDIR}..."

$DUCKDB :memory: <<SQL
INSTALL tpch;
LOAD tpch;
CALL dbgen(sf=${SF});

COPY nation    TO '${OUTDIR}/nation.parquet'    (FORMAT parquet);
COPY region    TO '${OUTDIR}/region.parquet'    (FORMAT parquet);
COPY supplier  TO '${OUTDIR}/supplier.parquet'  (FORMAT parquet);
COPY customer  TO '${OUTDIR}/customer.parquet'  (FORMAT parquet);
COPY part      TO '${OUTDIR}/part.parquet'      (FORMAT parquet);
COPY partsupp  TO '${OUTDIR}/partsupp.parquet'  (FORMAT parquet);
COPY orders    TO '${OUTDIR}/orders.parquet'    (FORMAT parquet);
COPY lineitem  TO '${OUTDIR}/lineitem.parquet'  (FORMAT parquet);
SQL

echo "Done. Files in ${OUTDIR}:"
ls -lh "$OUTDIR"
