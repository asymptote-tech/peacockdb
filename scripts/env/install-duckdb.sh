#!/usr/bin/env bash
# install-duckdb.sh [--prefix <dir>] — the DuckDB CLI at docker/pins.env's DUCKDB_VERSION
#
# Installs <dir>/bin/duckdb, /usr/local/bin/duckdb by default, from the release zip the build
# image takes, checked against DUCKDB_SHA256. Skipped when that duckdb already reports the
# version. Takes sudo where <dir>/bin is not writable.
#
# Exit 0; 2 bad input; 1 the zip, or the duckdb in it, is not the pinned one: nothing installed.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(dirname "$(dirname "$here")")
. "$root/scripts/lib/peacock-config.sh"
. "$root/scripts/lib/pinned.sh"

refuse() { echo "${0##*/}: $*" >&2; exit 2; }

prefix=/usr/local
while [ $# -gt 0 ]; do
  case $1 in
    --prefix) [ $# -ge 2 ] || refuse "--prefix needs a directory"; prefix=$2; shift 2 ;;
    -h | --help) peacock_usage "$0" ;;
    *) refuse "unknown argument $1" ;;
  esac
done

version=$(pin "$root/docker/pins.env" DUCKDB_VERSION)
sha=$(pin "$root/docker/pins.env" DUCKDB_SHA256)
bin=$prefix/bin
duckdb=$bin/duckdb

version_of() { "$1" --version 2> /dev/null | awk 'NR == 1 { print $1 }'; }

if [ -x "$duckdb" ] && [ "$(version_of "$duckdb")" = "v$version" ]; then
  echo "${0##*/}: $duckdb is v$version already"
  exit 0
fi

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
download_checked "https://github.com/duckdb/duckdb/releases/download/v$version/duckdb_cli-linux-amd64.zip" "$sha" \
  "$stage/duckdb.zip"
unzip_into "$stage/duckdb.zip" "$stage/cli"
got=$(version_of "$stage/cli/duckdb")
if [ "$got" != "v$version" ]; then
  echo "${0##*/}: the zip's duckdb reports ${got:-no version}, pinned v$version" >&2
  exit 1
fi

as_root=()
if ! mkdir -p "$bin" 2> /dev/null || [ ! -w "$bin" ]; then
  as_root=(sudo)
  sudo mkdir -p "$bin"
fi
"${as_root[@]}" install -m 755 "$stage/cli/duckdb" "$duckdb"
echo "${0##*/}: $duckdb is v$version"
