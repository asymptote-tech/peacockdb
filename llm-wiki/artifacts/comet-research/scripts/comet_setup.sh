#!/usr/bin/env bash
# comet_setup.sh [install|data|cluster|all]: set up Comet on the CPU VM (comet-research Task 4).
# Run from this folder, copied to the VM (with queries/ beside it, see comet_run.py), after
# `vm.sh push-aws cpu128`. Every step is idempotent. Long: run it detached and poll.
#
#   install  Java 17, Spark 4.1.3 at /opt/spark, the Comet 1.2.0 jar at /opt/comet, the S3A jars
#            in $SPARK_HOME/jars, and a venv at ~/comet/venv with pyarrow, boto3, zstandard.
#   data     TPC-H and TPC-DS sf40 into ~/comet/data/<ds>-sf40: chain L's copy under
#            /var/lib/peacock is reused if every size matches the bucket, else fetched from
#            eu-north1, leaving 100 GB free for Spark's spill. Then uploaded to the eu-west1
#            buckets, every size checked against eu-north1. Prints times and rates.
#   cluster  a standalone master and one worker with 128 cores, spill on ~/comet/spark-local.
#   all      the three, in order (the default).
#
# cpu128's only disk is a 0.06 GB/s network disk, so Task 4 first mounted a tmpfs and linked
# ~/comet/{data,spark-local} to it: sudo mount -t tmpfs -o size=320g,mode=1777 tmpfs /mnt/comet.
# Not /dev/shm: logind's RemoveIPC empties a user's /dev/shm files when their last session ends.
#
# Then: SPARK_HOME=/opt/spark ~/comet/venv/bin/python comet_run.py …
# URLs and versions: comet-internals.md `## Recipes` 4 and 5.
set -euo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
SPARK_DIR=/opt/spark-4.1.3-bin-hadoop3
SPARK_URL=https://archive.apache.org/dist/spark/spark-4.1.3/spark-4.1.3-bin-hadoop3.tgz
COMET_JAR=/opt/comet/comet-spark-spark4.1_2.13-1.2.0.jar
COMET_URL=https://repo1.maven.org/maven2/org/apache/datafusion/comet-spark-spark4.1_2.13/1.2.0/comet-spark-spark4.1_2.13-1.2.0.jar
JARS=(
  https://repo1.maven.org/maven2/org/apache/hadoop/hadoop-aws/3.4.2/hadoop-aws-3.4.2.jar
  https://repo1.maven.org/maven2/software/amazon/awssdk/bundle/2.29.52/bundle-2.29.52.jar
)
VENV=$HOME/comet/venv
DATA=$HOME/comet/data
LOCAL_DIRS=$HOME/comet/spark-local
SPILL_GB=100

say() { echo "[$(date -Is)] $*"; }

fetch() {  # <url> <dest>: download through a temp name, so a cut download is never taken as done
  [ -s "$2" ] && return 0
  sudo curl -fsSL --retry 5 -o "$2.part" "$1" && sudo mv "$2.part" "$2"
}

cmd_install() {
  grep -qw avx2 /proc/cpuinfo || { echo "no AVX2: Comet's amd64 build needs x86-64-v3" >&2; exit 1; }
  if ! java -version 2>&1 | grep -q 'version "17' || ! python3 -c 'import ensurepip' 2>/dev/null; then
    sudo apt-get update -qq
    sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq openjdk-17-jre-headless python3-venv curl
  fi
  if [ ! -x "$SPARK_DIR/bin/spark-submit" ]; then
    say "fetching Spark 4.1.3"
    fetch "$SPARK_URL" /opt/spark.tgz
    sudo tar -xzf /opt/spark.tgz -C /opt && sudo rm -f /opt/spark.tgz
  fi
  sudo ln -sfn "$SPARK_DIR" /opt/spark
  sudo mkdir -p /opt/comet
  fetch "$COMET_URL" "$COMET_JAR"
  local u; for u in "${JARS[@]}"; do fetch "$u" "$SPARK_DIR/jars/$(basename "$u")"; done
  [ -x "$VENV/bin/python" ] || python3 -m venv "$VENV"
  "$VENV/bin/pip" install -q pyarrow boto3 zstandard
  say "versions:"
  java -version 2>&1 | head -1
  "$SPARK_DIR/bin/spark-submit" --version 2>&1 | grep -m1 -o 'version [0-9.]*' || true
  ls -l "$COMET_JAR" "$SPARK_DIR"/jars/hadoop-aws-*.jar "$SPARK_DIR"/jars/bundle-*.jar | awk '{print $5, $9}'
  "$VENV/bin/python" -c 'import sys, pyarrow, boto3; print("python", sys.version.split()[0], "pyarrow", pyarrow.__version__, "boto3", boto3.__version__)'
}

cmd_data() {
  mkdir -p "$DATA"
  say "disk before: $(df -h --output=avail,size "$DATA" | tail -1)"
  local ds
  for ds in tpch tpcds; do
    say "$ds: local copy"
    "$VENV/bin/python" "$HERE/comet_data.py" prepare "$ds" --dest "$DATA/$ds-sf40" --min-free-gb "$SPILL_GB"
  done
  for ds in tpch tpcds; do
    say "$ds: upload to eu-west1"
    "$VENV/bin/python" "$HERE/comet_data.py" upload "$ds" --src "$DATA/$ds-sf40"
  done
  say "disk after: $(df -h --output=avail,size "$DATA" | tail -1)"
}

cmd_cluster() {
  local host; host=$(hostname)
  mkdir -p "$LOCAL_DIRS"
  # /opt/spark is root's, so logs and pids go under ~/comet; the start scripts exit 0 even when
  # the daemon never started, so the check below is what says the cluster is up.
  export SPARK_LOCAL_DIRS=$LOCAL_DIRS SPARK_WORKER_DIR=$HOME/comet/spark-work SPARK_HOME=/opt/spark \
    SPARK_LOG_DIR=$HOME/comet/spark-logs SPARK_PID_DIR=$HOME/comet/spark-pids
  pgrep -f deploy.master.Master >/dev/null || /opt/spark/sbin/start-master.sh --host "$host"
  pgrep -f deploy.worker.Worker >/dev/null || \
    /opt/spark/sbin/start-worker.sh "spark://$host:7077" --cores 128 --memory 480g
  local i; for i in $(seq 30); do
    grep -qs "Successfully registered with master" "$SPARK_LOG_DIR"/*Worker*.out && break; sleep 2
  done
  pgrep -f deploy.worker.Worker >/dev/null && grep -qs "Successfully registered" "$SPARK_LOG_DIR"/*Worker*.out \
    || { echo "cluster did not come up; see $SPARK_LOG_DIR" >&2; exit 1; }
  say "cluster up: spark://$host:7077"
}

case ${1:-all} in
  install) cmd_install;;
  data) cmd_data;;
  cluster) cmd_cluster;;
  all) cmd_install; cmd_data; cmd_cluster;;
  *) sed -n '2,17p' "$0" >&2; exit 64;;
esac
