#!/bin/sh
# cmake, with what a configure of this project would fetch taken from the image's copies.
# - rapids-cmake: cpp/CMakeLists.txt downloads a branch zip at every configure, and no environment
#   variable redirects it. The copy stands in when the tree's third_party/cudf/RAPIDS_BRANCH names
#   the branch it is of; a different branch clears a copy an earlier configure cached.
# - flatbuffers: its GitHub URL points at a mirror holding the one tag the image was built for.
# Every other mode is plain cmake. Tested by scripts/env/tests/test_cmake_wrapper.py.
real=/opt/cmake/bin/cmake
copy=/opt/prefetch/rapids-cmake
case "${1:-}" in
  --build|--install|--open|--workflow|--find-package|--version|--help|-E|-P) exec "$real" "$@" ;;
esac

n=${GIT_CONFIG_COUNT:-0}
export "GIT_CONFIG_KEY_$n=url.file:///opt/prefetch/flatbuffers.git.insteadOf"
export "GIT_CONFIG_VALUE_$n=https://github.com/google/flatbuffers.git"
export GIT_CONFIG_COUNT=$((n + 1))

src= skip=
for arg in "$@"; do
  if [ -n "$skip" ]; then
    [ "$skip" = source ] && src=$arg
    skip=
    continue
  fi
  case "$arg" in
    -S) skip=source ;;
    -S*) src=${arg#-S} ;;
    -B|-G|-T|-A|-C|-D|-U) skip=value ;;
    -*) ;;
    *) if [ -z "$src" ] && [ -f "$arg/CMakeLists.txt" ]; then src=$arg; fi ;;
  esac
done
branch=${src:+$src/../third_party/cudf/RAPIDS_BRANCH}
if [ -z "$branch" ] || [ ! -f "$branch" ]; then exec "$real" "$@"; fi
if [ -f "$copy.branch" ] && [ "$(cat "$branch")" = "$(cat "$copy.branch")" ]; then
  exec "$real" "-DFETCHCONTENT_SOURCE_DIR_RAPIDS-CMAKE=$copy" "$@"
fi
exec "$real" "-DFETCHCONTENT_SOURCE_DIR_RAPIDS-CMAKE=" "$@"
