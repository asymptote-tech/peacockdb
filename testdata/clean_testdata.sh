#!/bin/bash
#
# Remove generated TPC-H test data. Does not touch tpch.minimal (checked in).
#
# Usage:
#   ./testdata/clean_testdata.sh         # remove all generated datasets
#   ./testdata/clean_testdata.sh --sf 1  # remove only tpch.sf1

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SF=""

while [ $# -gt 0 ]; do
  case "$1" in
    --sf) SF="$2"; shift ;;
    *) echo "Unknown flag: $1"; exit 1 ;;
  esac
  shift
done

if [ -n "$SF" ]; then
  target="${SCRIPT_DIR}/tpch.sf${SF}"
  if [ -d "$target" ]; then
    echo "Removing ${target}"
    rm -rf "$target"
  else
    echo "Not found: ${target}"
  fi
else
  for dir in "${SCRIPT_DIR}"/tpch.sf*; do
    if [ -d "$dir" ]; then
      echo "Removing ${dir}"
      rm -rf "$dir"
    fi
  done
fi
