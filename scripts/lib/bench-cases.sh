# shellcheck shell=bash
#
# The benchmark case list `$BENCH_CASES` names, read for scripts. Sourced by shadgpu-env.sh
# (build-test-shadgpu.sh, create_nsys_profile.sh) and by scripts/env/bench-each.sh.

# Every corpus_query_benchmark! line of the case list as `<dataset> <sf> <query> <modes>`, the
# modes `|`-separated with no spaces. Read as `read_cases` and split_record.py read it — one
# invocation per line, leading space allowed — and refused (return 1, nothing on stdout) unless
# every invocation line parsed: a line skipped here is a dataset no run links in and a case
# no capture takes.
bench_lines() {
  local w='[[:space:]]*' ident='\([A-Za-z_][A-Za-z0-9_]*\)' sf='\([0-9][0-9]*\)' lines parsed
  lines=$(grep -c "^${w}corpus_query_benchmark!" "$BENCH_CASES") || lines=0
  parsed=$(sed -n "s/^${w}corpus_query_benchmark!$w($w$ident$w,$w$sf$w,$w$ident$w,\([^)]*\)).*/\1 \2 \3 \4/p" \
    "$BENCH_CASES")
  if [ "$lines" -eq 0 ] || [ "$(grep -c . <<< "$parsed")" -ne "$lines" ]; then
    echo "$BENCH_CASES: $lines corpus_query_benchmark! lines, $(grep -c . <<< "$parsed") of" \
      "them read as (dataset, sf, query, modes)" >&2
    return 1
  fi
  awk '{ modes = ""; for (i = 4; i <= NF; i++) modes = modes $i; print $1, $2, $3, modes }' \
    <<< "$parsed"
}

# Each `<dataset>.sf<sf>` the case list times, once, a line each.
bench_datasets() {
  local lines
  lines=$(bench_lines) || return 1
  awk '{ print $1 ".sf" $2 }' <<< "$lines" | LC_ALL=C sort -u
}

# Each timed case, a line each: `<dataset> <sf> <query> <mode>`, the macro's spellings.
# `none` declares a query timed at no mode, so it gives no case.
bench_cases() {
  local lines
  lines=$(bench_lines) || return 1
  awk '$4 != "none" { n = split($4, modes, "|"); for (i = 1; i <= n; i++) print $1, $2, $3, modes[i] }' \
    <<< "$lines"
}
