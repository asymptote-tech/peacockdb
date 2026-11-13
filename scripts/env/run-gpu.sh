#!/usr/bin/env bash
# The device run on a Nebius VM, in the run image, over scripts/nebius.sh (llm-wiki/build-test.md,
# "Nebius host").
#
#   run-gpu.sh --instance-id <id> [--profile <name>] [--dir <i>] [--cudf <v>] <verb>
#
#   prepare [--no-sf40]        ready the VM: Docker, the NVIDIA Container Toolkit and nsys if
#                                missing; the GPU's counters open to non-root; the build and run
#                                images, each refused unless its peacockdb.inputs label is this
#                                tree's; sf1, generated in the build image; sf40, the keys of
#                                ~/.config/peacock/peacock.env on stdin; the run image's /opt/cudf
#                                as ~/peacock/cudf-<v>/, for Nsight on the VM itself. --no-sf40,
#                                for the L40S list alone: no sf40, no env file, no nsys, counters
#                                or /opt/cudf copy
#   ship [--install <dir>]     the staged install (this tree's cpp/install by default, which
#                                scripts/env/stage-install.sh makes) and the tree's committed
#                                testdata to ~/peacock/<i>/ on the VM, the datasets linked in; four
#                                rsync streams at once, each retried; then, in the VM dir's .shipped,
#                                the sha256 of each shipped file that pull brings home
#   run [--detach] <list|filter|bench>
#                              the run image's run-tests.sh <mode> as container peacock-<i>, held by
#                                a runner on the VM that outlives the session, its log followed, or,
#                                with --detach, not; PCK_TEST_FILTER, PEACOCK_BENCH_RESUME,
#                                PEACOCK_BENCH_CASE_TIMEOUT_S and PCK_WRITE_GPU_RESULT pass on when set
#   run <name> [--exact <test>]
#                              a binary staged in the install's rust-benchmarks/ (stage-install.sh's
#                                prototype_fused_benchmarks), run in the unchanged run image as its
#                                entrypoint with run-tests.sh's environment, --nocapture
#                                --test-threads=1, and <test> or PCK_TEST_FILTER as the filter;
#                                PEACOCK_BENCH_RESUME passes on when set. Judged as run-tests.sh
#                                judges: a failed exit, a segfault or `running 0 tests` is exit 1
#   follow                     the dir's run's log from its start, then live; its exit the run's
#   cancel                     ends the dir's run: its wait for the card given up, or its container
#                                stopped and removed; a container no runner holds removed
#   status                     the dir's run: `none`, `waiting` for the card, `running`, `exited
#                                <code>` or `ended without a status`, then the last 40 lines of its
#                                log, which ~/peacock/<i>/logs/<mode or name>-<stamp>.log keeps for pull
#   pull [--clear]             testdata/benchmark-results/, testdata/calibration/ and the device's
#                                answers (testdata/goldens/*/gpu-result*.txt) into this tree, and
#                                the runs' logs into ~/.cache/peacock/run-gpu/<i>/: each file whose
#                                sha256 on the VM is not its .shipped line's, unless it changed here
#                                too (named, kept); then .shipped takes what is the same on both
#                                sides. --clear, refused while the dir's run goes: then each record
#                                and log the VM holds as its copy here is removed there
#
# Sirius is scripts/env/run-sirius.sh's, prepare and run, on this runner: status, follow, cancel and
# pull serve its runs, logged as logs/sirius-<stamp>.log, as any run of the dir.
#
# --instance-id and --profile go, as given, to every scripts/nebius.sh call ($NEBIUS_SH), which
# picks the VM. --instance-id is required, nebius-gpu's too: without it nebius.sh picks nebius-gpu
# silently. run-gpu.sh never starts or stops the VM: the caller does, with nebius.sh. <i> is this
# tree's directory name by default, <v> 25.02. prepare and run use the images of this tree's inputs,
# by their inputs tag (scripts/lib/image-inputs.sh), or PEACOCK_RUN_IMAGE and PEACOCK_BUILD_IMAGE
# when set (CI's scripts/env/image-ref.sh refs), checked alike. A run is given the run image by the
# ID read with its labels, so a tag moved since is not run. A run waits for the card's lock, one for
# every user of the VM (PEACOCK_CARD_LOCK), and holds it while its container lives. Ctrl-C, a TERM or
# a dropped ssh ends the following alone; follow takes it up again. A run's calibration record is
# kept as testdata/calibration/records-<stamp>.tsv once it ends, as is a records.tsv a run left
# whose runner died, under that run's stamp, or its own time with no run/state.
#
# ~/peacock/<i>/ is one tree's: its .tree holds the id of the tree that made it, or of the first to
# reach a dir made before .tree, and every other tree's prepare, ship, run, pull and cancel is
# refused before it touches the dir; status and follow claim nothing. A tree's id is made at its
# first claim and kept in its git-ignored .run-gpu-tree with the tree's path, so a copied or moved
# tree is refused it before any call. PEACOCK_TREE_ID, when set, is the id instead, and no file is
# read or written: for a checkout made anew on every run, as CI's is.
#
# Exit 0; 1 the run failed or ended with no status, a ship stream gave up, ssh dropped while
# following, or a pull kept a file changed here and on the VM; 2 bad input (nebius.sh's 64 too), a
# stale image, one not published (the images.yml dispatch named), a run in progress, a step missing
# (prepare, ship), a driver query that fails or lists no GPU, another tree's dir, a copied tree's
# .run-gpu-tree, or an image pull that filled the VM's disk; a pull that failed otherwise, docker's;
# 3 a card below compute capability 8.0, or one whose driver is older than the run image needs; 4
# restart the VM (nebius.sh stop --force, start), then prepare again; 5 status: the run goes on; 130
# or 143 a run or follow interrupted, the run going on; 254 a step whose command exited 255 on the
# VM; 255 an ssh to the VM dropped, the VM perhaps stopped (nebius.sh status, start, then the step
# again); else nebius.sh's (75, 69).
# status: 0 exited 0, 1 exited non-zero or ended without a status, 2 none. follow: as run, 2 no run.
# cancel: 0, or 1 when the run ended with no status or its container stays.
set -euo pipefail
shopt -s inherit_errexit

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "run-gpu.sh: $*" >&2; exit 2; }

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(git -C "$here" rev-parse --show-toplevel)
NEBIUS=("${NEBIUS_SH:-$root/scripts/nebius.sh}")
dir=$(basename "$root") cudf=25.02 install="" mode="" instance="" staged="" exact="" no_sf40=0 detach=0 clear=0
while [ $# -gt 0 ]; do
  case $1 in
    -h|--help) usage ;;
    --instance-id|--profile) [ -n "${2:-}" ] || die "$1 needs a value"; NEBIUS+=("$1" "$2")
                             [ "$1" = --profile ] || instance=$2; shift 2 ;;
    --dir) [ -n "${2:-}" ] || die "--dir needs a name"; dir=$2; shift 2 ;;
    --cudf) [ -n "${2:-}" ] || die "--cudf needs a version such as 25.02"; cudf=$2; shift 2 ;;
    -*) die "unknown option $1" ;;
    *) break ;;
  esac
done
verb=${1:-}
shift || true
case $verb in
  status|follow|cancel) [ $# -eq 0 ] || die "$verb takes no argument: $*" ;;
  pull)
    if [ "${1:-}" = --clear ]; then clear=1; shift; fi
    [ $# -eq 0 ] || die "unknown argument to pull: $*" ;;
  prepare)
    while [ $# -gt 0 ]; do
      case $1 in
        --no-sf40) no_sf40=1; shift ;;
        --sirius) die "sirius moved: scripts/env/run-sirius.sh prepare | run" ;;
        *) die "unknown argument to prepare: $1" ;;
      esac
    done ;;
  ship)
    while [ $# -gt 0 ]; do
      case $1 in
        --install) [ -n "${2:-}" ] || die "--install needs a directory"; install=$2; shift 2 ;;
        *) die "unknown argument to ship: $1" ;;
      esac
    done ;;
  run)
    if [ "${1:-}" = --detach ]; then detach=1; shift; fi
    [ $# -ge 1 ] || die "run takes a mode, list, filter or bench, or a staged binary's name"
    case $1 in
      list|filter|bench) [ $# -eq 1 ] || die "run $1 takes no argument: ${*:2}"; mode=$1 ;;
      *)
        [ "$detach" -eq 0 ] || die "run --detach $1: --detach takes a mode, list, filter or bench; a staged binary runs attached"
        [[ "$1" =~ ^[A-Za-z0-9_]+$ ]] || die "run $1: neither list, filter, bench nor a binary's name"
        staged=$1
        shift
        while [ $# -gt 0 ]; do
          case $1 in
            --exact) [ -n "${2:-}" ] || die "--exact needs a test name"; exact=$2; shift 2 ;;
            *) die "unknown argument to run $staged: $1" ;;
          esac
        done
        [ -z "$exact" ] || [ -z "${PCK_TEST_FILTER:-}" ] \
          || die "--exact $exact and PCK_TEST_FILTER=$PCK_TEST_FILTER both name the tests: give one" ;;
    esac ;;
  sirius) die "sirius moved: scripts/env/run-sirius.sh prepare | run" ;;
  *) die "unknown verb ${verb:-(none)}: prepare, ship, run, follow, cancel, status or pull" ;;
esac
# The tree's id, which a VM dir keeps as its owner's (the usage above): PEACOCK_TREE_ID, else
# .run-gpu-tree's, made by tree_claimed.
TREE_RECORD=$root/.run-gpu-tree
tree_path=$(cd "$root" && pwd -P)
. "$root/scripts/lib/gpu-vm-client.sh"
vm_options_checked
[ -f "$root/docker/conda/cudf-$cudf.spec" ] || die "no cuDF $cudf: docker/conda has no cudf-$cudf.spec"
tree_id_read

BUILD_REF=${PEACOCK_BUILD_IMAGE:-$MACHINE_IMAGE:$(image_inputs_tag "$root" build "$cudf")}
RUN_REF=${PEACOCK_RUN_IMAGE:-ghcr.io/asymptote-tech/peacockdb-run:$(image_inputs_tag "$root" run "$cudf")}
SHIP_PAUSE=${RUN_GPU_SHIP_PAUSE:-15}  # seconds, times the attempt; the tests set 0
LOGS_HOME=${XDG_CACHE_HOME:-$HOME/.cache}/peacock/run-gpu/$dir  # where pull keeps the VM dir's logs
SF40_SETS=(tpch.sf40 tpcds.sf40)
# What prepare runs on the VM, at the same paths under ~/peacock/<i>/: machine_data_set and
# fetch-sf40.sh, with what each reads.
KIT=(scripts/env/fetch-sf40.sh scripts/env/sf40.sha256 scripts/lib/peacock-config.sh scripts/lib/s3.sh
     scripts/lib/pinned.sh scripts/lib/data-inputs.sh scripts/lib/machine.sh docker/pins.env
     testdata/generate_testdata.sh)

# run_image_checked: RUN_ID, the ID of the run image RUN_REF names on the VM, read with its labels
# (image_read), and every card held to it (cards_checked).
run_image_checked() {
  image_read "$RUN_REF" run "$cudf"
  RUN_ID=$IMAGE_ID
  cards_checked "$RUN_REF"
}

# --- the verbs -----------------------------------------------------------------------------------

cmd_prepare() {
  if [ "$no_sf40" -eq 0 ]; then
    [ -f "$PEACOCK_ENV" ] || die "no $PEACOCK_ENV: the storage keys (llm-wiki/build-test.md, \"The env file\")"
    peacock_env_has_storage "$PEACOCK_ENV"
  fi
  tree_claimed
  (cd "$root" && to_vm "peacock/$dir/" -R "${KIT[@]}")
  on_vm vm_system
  [ "$no_sf40" -eq 1 ] || on_vm vm_nsight
  machine_image_checked "$BUILD_REF" "$root" "$cudf"
  machine_image_checked "$RUN_REF" "$root" "$cudf" run
  run_image_checked
  on_vm vm_data_set "$dir" "$BUILD_REF"
  if [ "$no_sf40" -eq 0 ]; then
    vm "cd ~/peacock/$dir && scripts/env/fetch-sf40.sh --env - ${SF40_SETS[*]}" "$PEACOCK_ENV"
    on_vm vm_cudf "$RUN_ID" "$cudf"
  fi
  echo "==> prepare: the VM is ready for $RUN_REF"
}

# ship_stream <name> <rsync arguments…>: one of ship's streams, tried 5 times, attempt × SHIP_PAUSE
# seconds apart; its output in $SHIP_LOGS/<name>. CI's runner reaches a VM over a long, lossy path:
# one rsync stream measured 0.24 MB/s on 2026-10-15, so a fresh 600 MB install took 40 minutes.
# So the install mirrors into the VM's persistent ~/peacock/<i>/ and only a change crosses, as
# rsync's delta; zstd on the wire; four streams at once, because each TCP stream backs off alone;
# --partial-dir, so a retry resumes a big file. --checksum, not size and mtime: a rebuilt file of
# the same size and second would be skipped, an unchanged one with a new mtime sent.
ship_stream() {
  local name=$1 attempt=1 t0=$SECONDS
  shift
  until rsync -a --checksum --timeout=120 --partial-dir=.rsync-partial --stats --compress \
          --compress-choice=zstd --compress-level=3 -e "${NEBIUS[*]} rsh" "$@" >> "$SHIP_LOGS/$name" 2>&1; do
    if [ "$attempt" -ge 5 ]; then
      echo "run-gpu.sh: ship: the $name stream failed $attempt times; its last lines:" >&2
      tail -5 "$SHIP_LOGS/$name" >&2
      return 1
    fi
    echo "run-gpu.sh: ship: $name stream: attempt $attempt failed; again in $((attempt * SHIP_PAUSE)) s" >&2
    sleep "$((attempt * SHIP_PAUSE))"
    attempt=$((attempt + 1))
  done
  echo "==> ship: $name stream: done in $((SECONDS - t0)) s; $(grep -h '^Total bytes sent' "$SHIP_LOGS/$name" | tail -1)"
}

cmd_ship() {
  local links=() entry ds sub dest pids=() pid failed=0 path
  install=${install:-$root/cpp/install}
  [ -d "$install" ] || die "no staged install at $install: make one with scripts/env/stage-install.sh, or name it with --install"
  for sub in lib rust-tests; do
    [ -d "$install/$sub" ] || die "no $install/$sub: not an install scripts/env/stage-install.sh staged"
  done
  for entry in "${DATA_SETS[@]}"; do
    ds=${entry%% *}
    links+=("$ds:$(data_inputs "$root")/$ds")
  done
  for ds in "${SF40_SETS[@]}"; do links+=("?$ds:$ds"); done
  tree_claimed
  on_vm vm_ship_ready "$dir" "${links[@]}"
  SHIP_LOGS=$(mktemp -d)
  trap 'rm -rf "$SHIP_LOGS"' EXIT
  # A file, not a pipe: a retry reads the list again.
  git -C "$root" ls-files -z -- testdata > "$SHIP_LOGS/testdata.files"
  dest=vm:peacock/$dir
  ship_stream lib --delete "$install/lib/" "$dest/cpp/install/lib/" & pids+=($!)
  ship_stream rust-tests --delete "$install/rust-tests/" "$dest/cpp/install/rust-tests/" & pids+=($!)
  # The rest of the install, bin/ and rust-benchmarks/; the excluded two are also kept from --delete.
  ship_stream install --delete --exclude=/lib/ --exclude=/rust-tests/ "$install/" "$dest/cpp/install/" & pids+=($!)
  ship_stream testdata --from0 --files-from="$SHIP_LOGS/testdata.files" "$root/" "$dest/" & pids+=($!)
  # Every stream finishes, so one failure does not kill another's transfer midway.
  for pid in "${pids[@]}"; do wait "$pid" || failed=1; done
  [ "$failed" -eq 0 ] || exit 1
  # The stamp is content, not a time: rsync gives a shipped file this tree's mtime, and a checkout
  # resets that.
  while IFS= read -r -d '' path; do
    if in_pulled_set "$path"; then printf '%s\n' "$path"; fi
  done < "$SHIP_LOGS/testdata.files" > "$SHIP_LOGS/stamped"
  on_vm_with "$SHIP_LOGS/stamped" vm_shipped "$dir"
  echo "==> ship: $install and the committed testdata to ~/peacock/$dir/"
}

cmd_run() {
  local envs=()
  tree_claimed
  # The VM's run image as prepare left it: no pull, so a run never takes an image prepare did not
  # check, and a tree whose image inputs moved since is refused.
  run_image_checked
  [ -z "${PCK_TEST_FILTER:-}" ] || envs+=("PCK_TEST_FILTER=$PCK_TEST_FILTER")
  [ -z "${PEACOCK_BENCH_RESUME:-}" ] || envs+=("PEACOCK_BENCH_RESUME=$PEACOCK_BENCH_RESUME")
  # The engine's two measurement switches (architecture.md, "cuDF options"): the cache's size and
  # the scan's reader.
  [ -z "${PEACOCK_PINNED_HOST_CACHE_BYTES:-}" ] || envs+=("PEACOCK_PINNED_HOST_CACHE_BYTES=$PEACOCK_PINNED_HOST_CACHE_BYTES")
  [ -z "${PEACOCK_SCAN_READER:-}" ] || envs+=("PEACOCK_SCAN_READER=$PEACOCK_SCAN_READER")
  [ -z "${PEACOCK_BENCH_CASE_TIMEOUT_S:-}" ] || envs+=("PEACOCK_BENCH_CASE_TIMEOUT_S=$PEACOCK_BENCH_CASE_TIMEOUT_S")
  [ -z "${PCK_WRITE_GPU_RESULT:-}" ] || envs+=("PCK_WRITE_GPU_RESULT=$PCK_WRITE_GPU_RESULT")
  if [ "$detach" -eq 1 ]; then
    on_vm vm_run "$dir" "$RUN_ID" "$mode" 1 "$PEACOCK_CARD_LOCK" "${envs[@]}"
  else
    following vm_run "$dir" "$RUN_ID" "$mode" 0 "$PEACOCK_CARD_LOCK" "${envs[@]}"
  fi
}

cmd_status() { on_vm vm_status "$dir"; }
cmd_follow() { following vm_follow "$dir"; }
cmd_cancel() {
  tree_claimed
  on_vm vm_cancel "$dir"
}

cmd_run_staged() {
  local envs=() args=(--nocapture --test-threads=1)
  tree_claimed
  run_image_checked
  [ -z "${PCK_TEST_FILTER:-}" ] || envs+=("PCK_TEST_FILTER=$PCK_TEST_FILTER")
  [ -z "${PEACOCK_BENCH_RESUME:-}" ] || envs+=("PEACOCK_BENCH_RESUME=$PEACOCK_BENCH_RESUME")
  if [ -n "$exact" ]; then args+=(--exact "$exact"); elif [ -n "${PCK_TEST_FILTER:-}" ]; then args+=("$PCK_TEST_FILTER"); fi
  following vm_run_staged "$dir" "$RUN_ID" "$staged" "$PEACOCK_CARD_LOCK" "${envs[@]}" -- "${args[@]}"
}

# pull: the files vm_pull_list names come to a temp dir, then each: with no copy here, or one the same
# as its stamp (unchanged here), it is taken; one the same as the VM's is left; else it changed on
# both sides and is kept, named. A record that differs is never replaced: its rows are a run's.
cmd_pull() {
  local digest stamped path here have kept=0
  tree_claimed
  PULL=$(mktemp -d)
  trap 'rm -rf "$PULL"' EXIT
  on_vm vm_pull_ready "$dir" "$clear" > "$PULL/clearable"
  on_vm vm_pull_list "$dir" > "$PULL/list"
  cut -d' ' -f3- "$PULL/list" > "$PULL/files"
  if [ -s "$PULL/files" ]; then from_vm "peacock/$dir/" "$PULL/in/" --files-from="$PULL/files"; fi
  : > "$PULL/same"
  while read -r digest stamped path; do
    here=$(pulled_home "$path")
    digest=$(sha256_of "$PULL/in/$path")  # the copy that came, should the VM's have changed since
    have=""
    if [ -f "$here" ]; then have=$(sha256_of "$here"); fi
    if [ -z "$have" ] || { [ "$have" = "$stamped" ] && [[ $path != testdata/calibration/records-[0-9]* ]]; }; then
      mkdir -p "$(dirname "$here")"
      mv "$PULL/in/$path" "$here"
    elif [ "$have" != "$digest" ]; then
      echo "run-gpu.sh: pull: kept $path: it changed here and on the VM" >&2
      kept=1
      continue
    fi
    printf '%s  %s\n' "$digest" "$path" >> "$PULL/same"
  done < "$PULL/list"
  if [ -s "$PULL/same" ]; then on_vm_with "$PULL/same" vm_stamped "$dir"; fi
  echo "==> pull: testdata/benchmark-results/, testdata/calibration/ and testdata/goldens/*/gpu-result*.txt; the runs' logs in $LOGS_HOME"
  if [ "$clear" -eq 1 ]; then
    while IFS= read -r path; do
      here=$(pulled_home "$path")
      if [ -f "$here" ]; then echo "$(sha256_of "$here") $path"; fi
    done < "$PULL/clearable" > "$PULL/home"
    on_vm_with "$PULL/home" vm_cleared "$dir"
  fi
  [ "$kept" -eq 0 ] || exit 1
}

# pulled_home <path under ~/peacock/<i>/>: where pull keeps it: a log in the cache, else in this tree.
pulled_home() {
  case $1 in
    logs/*) echo "$LOGS_HOME/${1#logs/}" ;;
    *) echo "$root/$1" ;;
  esac
}

if [ -n "$staged" ]; then cmd_run_staged; else "cmd_$verb"; fi
