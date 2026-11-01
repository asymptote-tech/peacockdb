# shellcheck shell=bash
#
# What the bytes of a machine's sf1 follow from: the generator, the DuckDB it runs on and its
# arguments per dataset. A machine keeps one set of datasets per hash, so two worktrees whose
# generators differ never share one, and a worktree finds its own without regenerating.

# Each dataset a machine holds, with the arguments testdata/generate_testdata.sh makes it with:
# `<dataset> <argument>...`. The generator writes testdata/<dataset>/.
DATA_SETS=("tpch.sf1 --bench tpch" "tpcds.sf1 --bench tpcds")

# data_inputs <tree> — the hash, 16 hex digits.
data_inputs() {
  local tree=$1 gen duckdb
  gen=$(sha256sum < "$tree/testdata/generate_testdata.sh") || return 2
  duckdb=$(grep '^DUCKDB_VERSION=' "$tree/docker/pins.env") || true
  [[ "$duckdb" =~ ^DUCKDB_VERSION=[^[:space:]]+$ ]] \
    || { echo "data_inputs: $tree/docker/pins.env holds no single DUCKDB_VERSION" >&2; return 2; }
  { echo "testdata/generate_testdata.sh=${gen%% *}"; echo "$duckdb"; printf '%s\n' "${DATA_SETS[@]}"; } \
    | sha256sum | cut -c1-16
}
