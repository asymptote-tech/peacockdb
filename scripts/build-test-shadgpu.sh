#!/bin/bash

set -e

CUDF_ROOT=/home/dmitry/data/miniforge3/envs/rapids-cuda-12.2
export CUDF_ROOT

# Rust integration tests that link libpeacock_gpu.so and need to run on the GPU host.
# After build, each binary is staged under cpp/install/rust-tests/<name> so the
# existing rsync step picks them up alongside the C++ binaries.
RUST_TESTS=(test_gpu_executor)
RUST_TESTS_STAGING=cpp/install/rust-tests

BUILD=0
RSYNC=0
PATCH=0
RUN=0

if [ $# -eq 0 ]; then
  echo "Usage: $0 [--build] [--rsync] [--patch] [--run] [--all]"
  exit 1
fi

while [ $# -gt 0 ]; do
  case "$1" in
    --build) BUILD=1 ;;
    --rsync) RSYNC=1 ;;
    --patch) PATCH=1 ;;
    --run)   RUN=1 ;;
    --all)   BUILD=1; RSYNC=1; PATCH=1; RUN=1 ;;
    *) echo "Unknown flag: $1"; exit 1 ;;
  esac
  shift
done

if [ "$BUILD" -eq 1 ]; then
  ./scripts/build.sh --cudf_ROOT "$CUDF_ROOT" --configure
  ./scripts/build.sh --cudf_ROOT "$CUDF_ROOT" --build
  ./scripts/build.sh --cudf_ROOT "$CUDF_ROOT" --install

  if ! command -v jq >/dev/null; then
    echo "ERROR: jq is required to locate cargo test binaries"; exit 1
  fi
  mkdir -p "$RUST_TESTS_STAGING"
  for t in "${RUST_TESTS[@]}"; do
    # cargo test --no-run prints a json artifact line per built target; the
    # integration test we want has .target.name == $t and a non-null .executable.
    exec_path=$(cargo test --no-run -p peacockdb-core --test "$t" \
        --message-format=json \
      | jq -r --arg name "$t" \
          'select(.executable != null) | select(.target.name == $name) | .executable' \
      | head -1)
    if [ -z "$exec_path" ] || [ ! -f "$exec_path" ]; then
      echo "ERROR: failed to locate built binary for $t"; exit 1
    fi
    cp -f "$exec_path" "$RUST_TESTS_STAGING/$t"
    echo "--- Staged rust test: $RUST_TESTS_STAGING/$t"
  done
fi

if [ "$RSYNC" -eq 1 ]; then
  rsync -r -P cpp/install/* shad-gpu:/home/info/peacockdb/cpp/install/
fi

if [ "$PATCH" -eq 1 ]; then
  ssh shad-gpu "/home/info/setup-glibc.sh --repo-dir=/home/info/peacockdb --patch-only"
fi

if [ "$RUN" -eq 1 ]; then
  ssh shad-gpu 'PEACOCK_TESTDATA_DIR=/home/info/peacockdb/testdata LD_LIBRARY_PATH=/usr/local/cuda-12.5/compat:/home/info/glibc-2.35/lib:$HOME/miniforge3/envs/rapids-cuda-12.2/lib:$LD_LIBRARY_PATH /home/info/peacockdb/cpp/install/bin/peacock_plan_tests'
fi
