#!/usr/bin/env bash
# Fail when a shared library defines CUDA runtime symbols, which means a static cudart was linked
# into it. In the 25.02 build image nvcc 12.9 puts its own cudart_static on the link line beside
# the prefix's 12.2 libcudart.so, so a runtime call that only 12.9 has would bind to the static
# copy instead of failing to link (llm-wiki/tasks/containers-detail.md, "the CUDA mix").
#
#   no-static-cudart.sh <lib.so>...      e.g. the FFI build's libpeacock_gpu.so
set -euo pipefail

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "no-static-cudart.sh: $*" >&2; exit 2; }

for arg in "$@"; do case "$arg" in -h|--help) usage ;; esac; done
[ $# -gt 0 ] || die "want one or more shared libraries"
for lib in "$@"; do [ -f "$lib" ] || die "no such file: $lib"; done

status=0
for lib in "$@"; do
  defined=$(nm -D --defined-only "$lib" | awk '$2 == "T" && $3 ~ /^(cuda|__cuda)/ { print $3 }')
  if [ -n "$defined" ]; then
    echo "no-static-cudart.sh: $lib defines CUDA runtime symbols:" $defined >&2
    status=1
  fi
done
exit "$status"
