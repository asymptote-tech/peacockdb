# shellcheck shell=bash
# shellcheck disable=SC2164  # each function runs under on_vm's set -euo pipefail, cd's failure fatal
# run-sirius.sh's VM side: functions run-sirius.sh sources and on_vm sends to the VM as text, with
# run-gpu-vm.sh's helpers. A Sirius run is one of run-gpu-vm.sh's runner, of <what> sirius.

# --- the VM's side: sent by on_vm, never run here --------------------------------------------------

# vm_sirius_ready <data set>…: refuses, exit 2, a VM without Docker or without a data set, which
# run-gpu.sh's prepare installs and fetches.
vm_sirius_ready() {
  local data=${PEACOCK_DATA_DIR:-/var/lib/peacock} set
  command -v docker > /dev/null \
    || { echo "run-sirius.sh: no Docker on the VM: run-gpu.sh … prepare installs it" >&2; exit 2; }
  for set in "$@"; do
    [ -d "$data/$set" ] || {
      echo "run-sirius.sh: no $data/$set on the VM: Sirius runs over sf40, which run-gpu.sh … prepare" \
           "fetches (not --no-sf40)" >&2
      exit 2
    }
  done
}

# vm_docker_free: Docker's root directory and the bytes free under it.
vm_docker_free() {
  local root
  vm_docker
  root=$(docker info --format '{{.DockerRootDir}}')
  echo "$root $(df --output=avail -B1 "$root" | tail -1)"
}

# vm_sirius_idle <i>: refuses, exit 2, while the dir's run goes, before a run's files are sent over
# what it writes. vm_started asks again under run/lock.
vm_sirius_idle() {
  vm_docker
  vm_idle "$1"
}

# vm_sent_stamped <i>: each `<sha256>  <path>` on stdin, a file run-sirius.sh sent with this tree's
# sha256, made its .shipped line where the VM's copy is that file. One --update kept, newer on the
# VM, keeps its line: pull then takes it unless it changed here too.
vm_sent_stamped() {
  local digest path have
  cd "$HOME/peacock/$1"
  while read -r digest path; do
    [ -f "$path" ] || continue
    have=$(sha256sum < "$path")
    if [ "${have%% *}" = "$digest" ]; then printf '%s  %s\n' "$digest" "$path"; fi
  done | vm_stamped "$1"
}

# vm_sirius_answers_cleared <i> <path> <sha256 or nothing>: a whole run's answers database, <path>
# under ~/peacock/<i>/, removed with its .wal before the run, as sirius-bench.py removes them. With
# a sha256, this tree's copy's, it is made the path's .shipped line: pull then takes the run's new
# database over this tree's, unless this tree's changed since. A run that writes none leaves none.
vm_sirius_answers_cleared() {
  rm -f "$HOME/peacock/$1/$2" "$HOME/peacock/$1/$2.wal"
  if [ -n "$3" ]; then printf '%s  %s\n' "$3" "$2" | vm_stamped "$1"; fi
}

# vm_sirius <i> <image ID> <detach> <card lock> <ds> <sf> <runner argument>…: docker/sirius-bench.py
# as peacock-<i>, held by a runner of what sirius: over <data>/<ds>.sf<sf>, read-only at its own
# path, writing the shipped testdata; Docker's seccomp profile less its io_uring and NUMA refusals,
# memlock unlimited. Followed, it exits as vm_follow: 0 or 2 as the runner's, else 1; detached (1),
# returns once its runner is started. Never judged as a staged binary.
vm_sirius() {
  local dir=$1 image=$2 detach=$3 lock=$4 sets=$5.sf$6 data=${PEACOCK_DATA_DIR:-/var/lib/peacock}
  local profile=$HOME/peacock/$1/docker/sirius-seccomp.json
  shift 6
  [ -f "$profile" ] || { echo "run-sirius.sh: no $profile on the VM: run-sirius.sh … prepare sends it" >&2; exit 2; }
  [ -d "$data/$sets" ] || { echo "run-sirius.sh: no $data/$sets on the VM: run-gpu.sh … prepare fetches it" >&2; exit 2; }
  vm_started "$dir" sirius "$lock" 0 --security-opt "seccomp=$profile" --ulimit memlock=-1:-1 \
    "$image" python3 /opt/sirius/sirius-bench.py "$@" --data "$data/$sets" --out /peacock/testdata
  if [ "$detach" -eq 1 ]; then
    echo "==> peacock-$dir: sirius-bench.py, detached; run-gpu.sh … status says how it goes, … follow follows it"
    return 0
  fi
  vm_follow "$dir"
}
