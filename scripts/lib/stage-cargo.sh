# shellcheck shell=bash
#
# Staging a cargo test binary under its target name, for a run elsewhere: sourced by
# shadgpu-env.sh (build-test-shadgpu.sh) and by scripts/env/stage-install.sh (the build image).
# The caller sets `set -o pipefail` and CARGO_TARGET_DIR.

# stage_cargo_test_binary <target> <staging-dir> [extra cargo args...]
#
# Build one integration test and copy its binary into the staging dir under its
# target name.
stage_cargo_test_binary() {
  local target=$1 staging=$2
  shift 2
  stage_cargo_binary "$target" test "$target" "$staging" --test "$target" "$@"
}

# stage_cargo_lib_binary <staged-name> <staging-dir> [extra cargo args...]
#
# The crate's own unit-test binary, whose artifact line has kind ["lib"] and the lib's
# name, `peacockdb_core`. It is staged under an explicit name because the run loops glob the
# staging directory, and that bare name beside the target binaries reads as one of them.
stage_cargo_lib_binary() {
  local staged=$1 staging=$2
  shift 2
  stage_cargo_binary peacockdb_core lib "$staged" "$staging" --lib "$@"
}

# stage_cargo_binary <cargo target name> <kind> <staged-name> <staging-dir> <cargo args...>
#
# The built path carries a metadata hash, so it is read out of cargo's json artifact
# lines rather than guessed: globbing `deps/<target>-*` picks up every stale hash from
# previous builds. The kind is matched too, since the lib and its test binary share a
# name and only the latter is what `test: true` builds.
stage_cargo_binary() {
  local name=$1 kind=$2 staged=$3 staging=$4
  shift 4
  local exec_path
  # `set -o pipefail` in the caller is what makes a compile failure land here as a
  # build failure rather than as an empty result reported as a missing binary. The
  # reader drains stdin rather than stopping at the match: cargo writes a final
  # `build-finished` line after the artifact, and a closed pipe there is EPIPE.
  if ! exec_path=$(cargo test --no-run -p peacockdb-core "$@" \
      --message-format=json \
    | python3 -c '
import json, sys
name, kind = sys.argv[1], sys.argv[2]
found = None
for line in sys.stdin:
    try: m = json.loads(line)
    except ValueError: continue
    target = m.get("target") or {}
    if found is None and m.get("executable") and target.get("name") == name and kind in target.get("kind", []):
        found = m["executable"]
if found: print(found)
' "$name" "$kind"); then
    echo "ERROR: building $staged failed (cargo output above)" >&2
    return 1
  fi
  if [ -z "$exec_path" ] || [ ! -f "$exec_path" ]; then
    echo "ERROR: $staged built, but no artifact line named its executable" >&2
    return 1
  fi
  mkdir -p "$staging"
  cp -f "$exec_path" "$staging/$staged"
  echo "--- Staged: $staging/$staged"
}
