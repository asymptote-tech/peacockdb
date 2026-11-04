#!/usr/bin/env bash
# Tests for vm.sh, with `nebius`, `ssh`, `systemd-run`, `systemctl` and gpu-host.sh stubbed.
# Every case runs in a fresh temp HOME, so neither the real Nebius CLI (~/.nebius/bin) nor a real
# AWS file is ever reachable. Run: bash llm-wiki/artifacts/comet-research/scripts/tests/test_vm_log.sh
set -uo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
VM=$HERE/../vm.sh
PASS=0; FAIL=0

# setup: a fresh sandbox. $STUB holds the stubs, their call logs, the VM state and boot id.
setup() {
  T=$(mktemp -d); TMPS+=("$T"); STUB=$T/stub; mkdir -p "$STUB/bin" "$T/home"
  echo STOPPED > "$STUB/state"; echo boot-aaaa > "$STUB/boot_id"
  cat > "$STUB/bin/nebius" <<'EOF'
#!/usr/bin/env bash
echo "$*" >> "$STUB/nebius.calls"
case "$*" in
  *"instance get"*)
    printf '{"status":{"state":"%s","network_interfaces":[{"public_ip_address":{"address":"10.0.0.9/32"}}]}}\n' "$(cat "$STUB/state")";;
  *"instance start"*)
    if [ -n "${STUB_NO_CAPACITY:-}" ]; then echo 'Error: rpc error: code = ResourceExhausted desc = not enough resources' >&2; exit 1; fi
    echo RUNNING > "$STUB/state";;
  *"instance stop"*) echo STOPPED > "$STUB/state";;
esac
EOF
  cat > "$STUB/bin/ssh" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$STUB/ssh.calls"
if [ -n "${STUB_CAPTURE_STDIN:-}" ]; then cat >> "$STUB/ssh.stdin"; fi
case "$*" in *boot_id*) cat "$STUB/boot_id";; esac
exit 0
EOF
  cat > "$STUB/gpu-host.sh" <<'EOF'
#!/usr/bin/env bash
echo "$*" >> "$STUB/gpuhost.calls"
case $1 in
  status) cat "$STUB/state";;
  start) echo RUNNING > "$STUB/state";;
  stop) echo STOPPED > "$STUB/state";;
  ip) echo 10.0.0.8;;
  ssh) shift; exec ssh l40s-stub "$@";;
esac
EOF
  for s in systemd-run systemctl; do
    printf '#!/usr/bin/env bash\necho "%s $*" >> "$STUB/systemd.calls"\n' "$s" > "$STUB/bin/$s"
  done
  chmod +x "$STUB"/bin/* "$STUB/gpu-host.sh"
  export STUB HOME=$T/home PATH=$STUB/bin:$ORIG_PATH COMET_VM_LOG=$T/vm-log.tsv \
    COMET_GPU_HOST=$STUB/gpu-host.sh COMET_VM_POLL=0
  unset COMET_DEADMAN STUB_NO_CAPACITY STUB_CAPTURE_STDIN
}
ORIG_PATH=$PATH
TMPS=()
trap 'rm -rf "${TMPS[@]}"' EXIT

ok() { PASS=$((PASS + 1)); echo "PASS $1"; }
bad() { FAIL=$((FAIL + 1)); echo "FAIL $1: $2"; }
# events <vm>: the log's event column (and detail, tab-joined) for one VM, one per line.
events() { awk -F'\t' -v v="$1" '$2==v {print $3 ($4=="" ? "" : " " $4)}' "$COMET_VM_LOG" 2>/dev/null; }
count() { grep -c -- "$2" "$1" 2>/dev/null || true; }

t_use_stopped_starts_then_stop_used_stops() {
  setup; local n=use_on_STOPPED_starts_it_and_stop_used_stops_it
  "$VM" use cpu128 >/dev/null 2>&1 || { bad $n "use exited $?"; return; }
  local ev; ev=$(events cpu128 | grep -E '^use' | head -2 | tr '\n' '|')
  [ "$ev" = "use-requested|use started|" ] || { bad $n "events: $ev"; return; }
  [ "$(count "$STUB/nebius.calls" 'instance start')" = 1 ] || { bad $n "start not called once"; return; }
  "$VM" stop-used >/dev/null 2>&1 || { bad $n "stop-used exited $?"; return; }
  [ "$(count "$STUB/nebius.calls" 'instance stop')" = 1 ] || { bad $n "stop not called"; return; }
  events cpu128 | grep -q '^stop' || { bad $n "no stop logged"; return; }
  [ "$(cat "$STUB/state")" = STOPPED ] || { bad $n "VM not STOPPED"; return; }
  ok $n
}

t_use_running_found_and_force_stopped() {
  setup; local n=use_on_RUNNING_is_found_and_stop_used_force_stops_it
  echo RUNNING > "$STUB/state"
  "$VM" use cpu128 >/dev/null 2>&1 || { bad $n "use exited $?"; return; }
  events cpu128 | grep -qx 'use found' || { bad $n "no 'use found': $(events cpu128 | tr '\n' '|')"; return; }
  [ "$(count "$STUB/nebius.calls" 'instance start')" = 0 ] || { bad $n "start called on a RUNNING VM"; return; }
  "$VM" stop-used >/dev/null 2>&1
  [ "$(count "$STUB/nebius.calls" 'instance stop')" = 1 ] || { bad $n "stop not called"; return; }
  ok $n
}

t_use_twice_one_stop() {
  setup; local n=use_twice_then_stop_used_stops_once
  "$VM" use cpu128 >/dev/null 2>&1; "$VM" use cpu128 >/dev/null 2>&1
  "$VM" stop-used >/dev/null 2>&1
  [ "$(count "$STUB/nebius.calls" 'instance stop')" = 1 ] || { bad $n "stop calls: $(count "$STUB/nebius.calls" 'instance stop')"; return; }
  [ "$(events cpu128 | grep -c '^stop')" = 1 ] || { bad $n "stop rows: $(events cpu128 | grep -c '^stop')"; return; }
  ok $n
}

t_lost_when_stopped() {
  setup; local n=stop_used_on_an_already_STOPPED_VM_logs_lost_and_calls_no_stop
  "$VM" use cpu128 >/dev/null 2>&1
  echo STOPPED > "$STUB/state"
  "$VM" stop-used >/dev/null 2>&1
  events cpu128 | grep -q '^lost' || { bad $n "no lost: $(events cpu128 | tr '\n' '|')"; return; }
  [ "$(count "$STUB/nebius.calls" 'instance stop')" = 0 ] || { bad $n "stop was called"; return; }
  ok $n
}

t_lost_when_rebooted() {
  setup; local n=stop_used_on_a_rebooted_VM_logs_lost_and_still_force_stops
  "$VM" use cpu128 >/dev/null 2>&1
  echo boot-bbbb > "$STUB/boot_id"
  "$VM" stop-used >/dev/null 2>&1
  events cpu128 | grep -q '^lost' || { bad $n "no lost: $(events cpu128 | tr '\n' '|')"; return; }
  [ "$(count "$STUB/nebius.calls" 'instance stop')" = 1 ] || { bad $n "stop not called"; return; }
  ok $n
}

t_hours_two_pairs() {
  setup; local n=hours_over_two_30_minute_pairs_is_1.00
  printf '%s\t%s\t%s\t%s\n' \
    2026-11-04T10:00:00+01:00 cpu128 use found \
    2026-11-04T10:30:00+01:00 cpu128 stop force \
    2026-11-04T12:00:00+01:00 cpu128 use found \
    2026-11-04T12:30:00+01:00 cpu128 stop force \
    2026-11-04T12:00:00+01:00 l40s use found \
    2026-11-04T13:00:00+01:00 l40s stop force > "$COMET_VM_LOG"
  local h; h=$("$VM" hours cpu128 2>/dev/null)
  [ "$h" = 1.00 ] && ok $n || bad $n "got '$h'"
}

t_hours_lost_stops_at_heartbeat() {
  setup; local n=hours_over_a_lost_use_stops_at_the_last_heartbeat
  printf '%s\t%s\t%s\t%s\n' \
    2026-11-04T10:00:00+01:00 cpu128 use-requested '' \
    2026-11-04T10:03:00+01:00 cpu128 use started \
    2026-11-04T10:03:10+01:00 cpu128 boot boot-aaaa \
    2026-11-04T10:12:00+01:00 cpu128 heartbeat boot-aaaa \
    2026-11-04T10:20:00+01:00 cpu128 heartbeat boot-bbbb \
    2026-11-04T11:00:00+01:00 cpu128 lost 'boot id changed' > "$COMET_VM_LOG"
  # A started use counts from its request (billing starts at the start call): 10:00 to 10:12.
  local h; h=$("$VM" hours cpu128 2>/dev/null)
  [ "$h" = 0.20 ] && ok $n || bad $n "got '$h'"
}

t_deadman_minutes_left() {
  setup; local n=deadman_is_armed_with_the_minutes_left_in_the_cap
  printf '%s\t%s\t%s\t%s\n' \
    2026-11-04T08:00:00+01:00 cpu128 use found \
    2026-11-04T08:30:00+01:00 cpu128 stop force > "$COMET_VM_LOG"
  "$VM" use cpu128 >/dev/null 2>&1 || { bad $n "use exited $?"; return; }
  grep -E 'systemd-run' "$STUB/ssh.calls" | grep -qE -- '--on-active=(89|90)m' \
    || { bad $n "ssh calls: $(tr '\n' '|' < "$STUB/ssh.calls")"; return; }
  grep 'systemd-run' "$STUB/ssh.calls" | grep -q poweroff || { bad $n "no poweroff"; return; }
  events cpu128 | grep -qE '^deadman guest (89|90)m' || { bad $n "no deadman row: $(events cpu128 | tr '\n' '|')"; return; }
  ok $n
}

t_deadman_override_on_l40s() {
  setup; local n=deadman_minutes_override_and_l40s_through_gpu_host
  "$VM" use l40s --deadman-min 3 >/dev/null 2>&1 || { bad $n "use exited $?"; return; }
  grep -qx start "$STUB/gpuhost.calls" || { bad $n "gpu-host start not called"; return; }
  grep systemd-run "$STUB/ssh.calls" | grep -q -- '--on-active=3m' || { bad $n "ssh: $(tr '\n' '|' < "$STUB/ssh.calls")"; return; }
  events l40s | grep -qx 'use started' || { bad $n "no 'use started'"; return; }
  "$VM" stop-used >/dev/null 2>&1
  grep -qx 'stop --force' "$STUB/gpuhost.calls" || { bad $n "no 'stop --force': $(tr '\n' '|' < "$STUB/gpuhost.calls")"; return; }
  [ ! -e "$STUB/nebius.calls" ] || { bad $n "nebius called directly for l40s"; return; }
  ok $n
}

t_deadman_local_fallback() {
  setup; local n=deadman_local_fallback_arms_a_user_timer_running_vm.sh_stop
  COMET_DEADMAN=local "$VM" use cpu128 >/dev/null 2>&1 || { bad $n "use exited $?"; return; }
  grep 'systemd-run' "$STUB/systemd.calls" | grep -- '--user' | grep -E -- '--on-active=(119|120)m' \
    | grep -q 'vm.sh stop cpu128' || { bad $n "systemd: $(tr '\n' '|' < "$STUB/systemd.calls" 2>/dev/null)"; return; }
  ! grep -q 'systemd-run' "$STUB/ssh.calls" || { bad $n "guest dead-man armed too"; return; }
  ok $n
}

t_unknown_vm() {
  setup; local n=use_h100_exits_1_calls_no_nebius_and_logs_nothing
  local rc=0; "$VM" use h100 >/dev/null 2>&1 || rc=$?
  [ "$rc" = 1 ] || { bad $n "rc $rc"; return; }
  [ ! -e "$STUB/nebius.calls" ] || { bad $n "nebius called"; return; }
  [ ! -s "$COMET_VM_LOG" ] || { bad $n "log written"; return; }
  ok $n
}

t_no_capacity() {
  setup; local n=use_with_no_capacity_exits_75_with_no_open_use
  local rc=0; STUB_NO_CAPACITY=1 "$VM" use cpu128 >/dev/null 2>&1 || rc=$?
  [ "$rc" = 75 ] || { bad $n "rc $rc"; return; }
  [ "$(events cpu128 | tr '\n' '|')" = "use-requested|" ] || { bad $n "events: $(events cpu128 | tr '\n' '|')"; return; }
  ok $n
}

t_ssh_heartbeat() {
  setup; local n=ssh_logs_a_heartbeat_with_the_boot_id
  "$VM" use cpu128 >/dev/null 2>&1
  local out; out=$("$VM" ssh cpu128 echo hi 2>/dev/null) || { bad $n "ssh exited $?"; return; }
  events cpu128 | grep -qx 'heartbeat boot-aaaa' || { bad $n "events: $(events cpu128 | tr '\n' '|')"; return; }
  grep -q 'HostKeyAlias=comet-cpu128' "$STUB/ssh.calls" || { bad $n "no HostKeyAlias"; return; }
  grep -q 'dmitry@10.0.0.9' "$STUB/ssh.calls" || { bad $n "wrong login/ip"; return; }
  ok $n
}

t_push_aws() {
  setup; local n=push_aws_sends_profiles_on_stdin_only
  mkdir -p "$HOME/.aws"
  printf '[default]\naws_access_key_id = AKIADEFAULTID\naws_secret_access_key = SECRETDEFAULTVALUE\n\n[eu-west1]\naws_access_key_id = AKIAWESTID\naws_secret_access_key = SECRETWESTVALUE\n\n[other]\naws_access_key_id = AKIAOTHER\naws_secret_access_key = SECRETOTHER\n' > "$HOME/.aws/credentials"
  printf '[default]\nregion = eu-north1\n\n[profile eu-west1]\nregion = eu-west1\nendpoint_url = https://storage.eu-west1.nebius.cloud:443\n' > "$HOME/.aws/config"
  "$VM" use cpu128 >/dev/null 2>&1
  local out; out=$(STUB_CAPTURE_STDIN=1 "$VM" push-aws cpu128 2>&1) || { bad $n "push-aws exited $?: $out"; return; }
  grep -q SECRETDEFAULTVALUE "$STUB/ssh.stdin" && grep -q SECRETWESTVALUE "$STUB/ssh.stdin" \
    || { bad $n "keys not on stdin"; return; }
  ! grep -q SECRETOTHER "$STUB/ssh.stdin" || { bad $n "an unasked profile was sent"; return; }
  ! grep -qE 'SECRET|AKIA' "$STUB/ssh.calls" || { bad $n "a key is in ssh argv"; return; }
  ! grep -qE 'SECRET|AKIA' <<<"$out" || { bad $n "a key is in the output"; return; }
  grep -q default <<<"$out" && grep -q eu-west1 <<<"$out" || { bad $n "profile names not printed: $out"; return; }
  : > "$STUB/ssh.stdin"
  "$VM" use l40s >/dev/null 2>&1
  out=$(STUB_CAPTURE_STDIN=1 "$VM" push-aws l40s 2>&1) || { bad $n "push-aws l40s exited $?"; return; }
  grep -q SECRETDEFAULTVALUE "$STUB/ssh.stdin" || { bad $n "l40s: default not sent"; return; }
  ! grep -q SECRETWESTVALUE "$STUB/ssh.stdin" || { bad $n "l40s got eu-west1"; return; }
  ok $n
}

for t in $(declare -F | awk '$3 ~ /^t_/ {print $3}'); do "$t"; done
echo "vm.sh: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
