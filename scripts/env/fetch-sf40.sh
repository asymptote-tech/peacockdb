#!/usr/bin/env bash
# fetch-sf40.sh [--data-dir <dir>] [--env <file>] <dataset>… — the sf40 sets onto this machine
#
# Datasets: tpch.sf40 and tpcds.sf40, each from the bucket of its name with `.` as `-`, into
# <dir>/<dataset>/<table>.parquet. <dir> is $PEACOCK_DATA_DIR, else /var/lib/peacock.
#
# The storage keys come from --env, ~/.config/peacock/peacock.env by default, `-` for stdin; they
# reach the aws CLI's environment alone, never an argv. The endpoint and region are the file's
# AWS_ENDPOINT_URL and AWS_REGION. The aws CLI is $AWS, else `aws`; with neither, the pinned
# aws CLI v2 is installed into ~/.local/aws-cli.
#
# Each object's size comes from the bucket's manifest.json. A file of that size is kept; any
# other is fetched to <file>.part, size-checked, and renamed. Then every manifest file must be
# there at its size and no other .parquet, and the aggregate sha256 (`sha256sum *.parquet |
# sha256sum`, in C order) must equal the pin in scripts/env/sf40.sha256 where it has one; an
# unpinned dataset's is printed. Prints `<dataset>: fetched <n>, kept <m>`.
#
# Exit 0; 2 bad input; 1 a fetch or a check failed.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(dirname "$(dirname "$here")")
. "$root/scripts/lib/peacock-config.sh"
. "$root/scripts/lib/s3.sh"
. "$root/scripts/lib/pinned.sh"

die() { echo "${0##*/}: $*" >&2; exit 1; }
refuse() { echo "${0##*/}: $*" >&2; exit 2; }

data=${PEACOCK_DATA_DIR:-/var/lib/peacock}
env_file=$PEACOCK_ENV
datasets=()
while [ $# -gt 0 ]; do
  case $1 in
    --data-dir) [ $# -ge 2 ] || refuse "--data-dir needs a directory"; data=$2; shift 2 ;;
    --env) [ $# -ge 2 ] || refuse "--env needs a file, or - for stdin"; env_file=$2; shift 2 ;;
    -h | --help) peacock_usage "$0" ;;
    -*) refuse "unknown option $1" ;;
    tpch.sf40 | tpcds.sf40) datasets+=("$1"); shift ;;
    *) refuse "unknown dataset $1: tpch.sf40 or tpcds.sf40" ;;
  esac
done
[ ${#datasets[@]} -gt 0 ] || refuse "no dataset: tpch.sf40, tpcds.sf40 or both"

# The keys, from the file alone, kept out of every environment but aws's.
unset "${PEACOCK_STORAGE_KEYS[@]}"
read_storage_keys() {
  local line
  while IFS= read -r line || [ -n "$line" ]; do
    line=${line%$'\r'}
    if [[ "$line" =~ ^([A-Z_]+)=(.*)$ ]] && [[ " ${PEACOCK_STORAGE_KEYS[*]} " == *" ${BASH_REMATCH[1]} "* ]]; then
      printf -v "${BASH_REMATCH[1]}" '%s' "${BASH_REMATCH[2]}"
    fi
  done
}
if [ "$env_file" = - ]; then
  read_storage_keys
elif [ -f "$env_file" ]; then
  read_storage_keys < "$env_file"
else
  refuse "no env file $env_file: --env names the storage keys"
fi
for key in "${PEACOCK_STORAGE_KEYS[@]}"; do
  [ -n "${!key:-}" ] || refuse "the env file ${env_file/#-/(stdin)} has no $key"
done
s3=()
s3_args s3 || exit 2

pins=$here/sf40.sha256
pins_env=$root/docker/pins.env

if [ -n "${AWS:-}" ]; then
  command -v "$AWS" > /dev/null || refuse "AWS=$AWS is not a command"
  aws=$AWS
elif command -v aws > /dev/null; then
  aws=aws
else
  aws=$HOME/.local/aws-cli/bin/aws
  if [ ! -x "$aws" ]; then
    version=$(pin "$pins_env" AWS_CLI_VERSION)
    sha=$(pin "$pins_env" AWS_CLI_SHA256)
    stage=$(mktemp -d)
    trap 'rm -rf "$stage"' EXIT
    download_checked "https://awscli.amazonaws.com/awscli-exe-linux-x86_64-$version.zip" "$sha" "$stage/awscli.zip"
    unzip_into "$stage/awscli.zip" "$stage"
    "$stage/aws/install" -i "$HOME/.local/aws-cli" -b "$HOME/.local/aws-cli/bin" > /dev/null
    rm -rf "$stage"
  fi
fi

aws_s3() {
  AWS_ACCESS_KEY_ID=$AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY=$AWS_SECRET_ACCESS_KEY "$aws" "${s3[@]}" "$@"
}

size_of() { stat -c %s "$1" 2> /dev/null || echo -1; }

# fetch <dataset> — the dataset whole and checked, under a lock: the data dir is the machine's.
fetch() {
  local ds=$1 bucket=${1/./-} dir=$data/$1 manifest name size got fetched=0 kept=0 f agg pinned lock
  mkdir -p "$dir"
  exec {lock}> "$data/.$ds.lock"
  flock "$lock"
  manifest=$(aws_s3 s3 cp --only-show-errors "s3://$bucket/manifest.json" -) \
    || die "$ds: cannot read s3://$bucket/manifest.json"
  manifest=$(python3 -I -c '
import json, re, sys
m = json.loads(sys.argv[1])
assert isinstance(m, dict) and m
for name, size in m.items():
    assert re.fullmatch(r"[A-Za-z0-9_]+\.parquet", name) and type(size) is int and size >= 0, name
    print(name, size)
' "$manifest") || die "$ds: s3://$bucket/manifest.json is not {\"<table>.parquet\": <bytes>, …}"
  declare -A want=()
  while read -r name size; do
    want[$name]=$size
    if [ "$(size_of "$dir/$name")" = "$size" ]; then
      kept=$((kept + 1))
      continue
    fi
    echo "$ds: $name, $size bytes"
    rm -f "$dir/$name.part"
    aws_s3 s3 cp --only-show-errors "s3://$bucket/$name" "$dir/$name.part" \
      || { rm -f "$dir/$name.part"; die "$ds: cannot fetch s3://$bucket/$name"; }
    got=$(size_of "$dir/$name.part")
    if [ "$got" != "$size" ]; then
      rm -f "$dir/$name.part"
      die "$ds: $name came $got bytes, the manifest says $size"
    fi
    mv "$dir/$name.part" "$dir/$name"
    fetched=$((fetched + 1))
  done <<< "$manifest"
  for f in "$dir"/*.parquet; do
    name=${f##*/}
    [ -n "${want[$name]:-}" ] || die "$ds: $f is not in s3://$bucket/manifest.json"
    [ "$(size_of "$f")" = "${want[$name]}" ] || die "$ds: $f is not ${want[$name]} bytes"
  done
  agg=$(cd "$dir" && printf '%s\0' *.parquet | xargs -0 -n 1 -P "$(nproc)" sha256sum | sort -k 2 | sha256sum)
  agg=${agg%% *}
  pinned=$(awk -v ds="$ds" '$2 == ds { print $1 }' "$pins")
  if [ -z "$pinned" ]; then
    echo "$ds: aggregate $agg, not pinned in scripts/env/sf40.sha256"
  elif [ "$agg" != "$pinned" ]; then
    die "$ds: aggregate sha256 $agg, pinned $pinned"
  fi
  exec {lock}>&-
  echo "$ds: fetched $fetched, kept $kept"
}

export LC_ALL=C
umask 002
for ds in "${datasets[@]}"; do
  fetch "$ds"
done
