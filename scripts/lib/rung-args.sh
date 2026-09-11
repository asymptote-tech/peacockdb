# shellcheck shell=bash
#
# The filter arguments a staged rust test binary takes on a GPU host. Inlined verbatim
# into the remote gate script of build-test-shadgpu.sh and build-test.sh — both runners
# hand every binary the developer's PCK_TEST_FILTER, and the crate's own unit-test binary
# alone also takes its rung: built with --features gpu it holds every rung, and unfiltered
# it would put the CPU unit cases through --test-threads=1 on the one serial host.
#
# The rung is part of what that binary is, never the developer's filter, so the runner's
# zero-test guard stays armed for it (test-layout.md, "The three GPU target lists").

# rung_args <binary> <lib-staged-name> <rung> <dev-filter> [<env prefix...>]
#
# One argument per line, for mapfile. An empty rung is a mode that runs its lib whole.
# A developer's filter narrows inside the rung: libtest ORs its filters, so the
# intersection is a list of exact names read off `--list <rung>`, and the empty name
# under --exact matches nothing — no match runs no test. The env prefix is whatever the
# host needs to run the binary at all.
#
# Non-zero when the listing itself fails or the rung lists nothing: either is the binary
# or the rung broken, not an empty intersection, and the caller must not run it.
rung_args() {
  local bin=$1 lib=$2 rung=$3 filter=$4 listed cases matched
  shift 4
  if [ -z "$rung" ] || [ "${bin##*/}" != "$lib" ]; then
    printf '%s\n' "$filter"
  elif [ -z "$filter" ]; then
    printf '%s\n' "$rung"
  else
    listed=$("$@" "$bin" --list "$rung") || return 1
    cases=$(printf '%s\n' "$listed" | sed -n 's/: test$//p')
    if [ -z "$cases" ]; then
      echo "rung_args: $rung lists no case in ${bin##*/}" >&2
      return 1
    fi
    matched=$(printf '%s\n' "$cases" | grep -F -- "$filter") || true
    # The empty name first: a caller reading this through `$(…)` loses trailing
    # newlines, and `--exact` left alone would run every case.
    printf '%s\n' '' --exact
    if [ -n "$matched" ]; then
      printf '%s\n' "$matched"
    else
      echo "rung_args: 0 of $(printf '%s\n' "$cases" | wc -l) $rung cases match '$filter'" >&2
    fi
  fi
}
