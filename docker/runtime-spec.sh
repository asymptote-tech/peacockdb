#!/usr/bin/env bash
# The runtime subset of an explicit conda spec, on stdout: the header through @EXPLICIT, then each
# package line verbatim unless the package is only for building or is Python. Dropped: compilers
# and their parts (nvcc, crt, nvvm, the linker), -dev and -static packages, header-only packages,
# and Python with every package built for it (a py build string). An explicit install resolves
# nothing, so what is left is exactly what the run image's /opt/cudf holds.
#
#   runtime-spec.sh <spec>      e.g. docker/conda/cudf-25.02.spec
set -euo pipefail

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "runtime-spec.sh: $*" >&2; exit 2; }

for arg in "$@"; do case "$arg" in -h|--help) usage ;; esac; done
[ $# -eq 1 ] || die "want one spec, got $#"
spec=$1
[ -f "$spec" ] || die "no such file: $spec"
grep -q '^@EXPLICIT$' "$spec" && grep -q '^https://' "$spec" || die "$spec is not an explicit spec with packages"

build_only='^(cuda-nvcc.*|cuda-crt.*|cuda-nvvm.*|ld_impl_linux-64|cuda-cccl.*|cuda-profiler-api|.*-dev|.*-dev_linux-64|.*-static|.*-static_linux-64|.*-headers|.*-headers-only|nlohmann_json|dlpack|python|python_abi)$'
# <name>-<version>-<build>.<ext>#<md5>: neither version nor build holds a '-'.
awk -v build_only="$build_only" '
  !explicit { print; if ($0 == "@EXPLICIT") explicit = 1; next }
  /^https:\/\// {
    base = $0; sub(/#.*/, "", base); sub(/.*\//, "", base); sub(/\.(conda|tar\.bz2)$/, "", base)
    n = split(base, part, "-"); build = part[n]
    name = part[1]; for (i = 2; i <= n - 2; i++) name = name "-" part[i]
    if (name ~ build_only || build ~ /(^|_)py(h|[0-9])/) next
    print
  }' "$spec"
