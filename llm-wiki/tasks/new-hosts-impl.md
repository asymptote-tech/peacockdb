# new-hosts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `nebius.sh` and its two idle guards serve three Nebius VMs (nebius-gpu, nebius-h100,
nebius-cpu128); both new VMs hold sf40; the CPU VM is provisioned by containers' scripts and holds
DuckDB 1.5.4 with its native sf40 database; the H100 runs the device tiers, the benchmark and
Nsight through containers' device runner, `scripts/env/run-gpu.sh`, over `scripts/nebius.sh`.
First, every `gpu-host` name becomes `nebius`.

**Architecture:** `nebius.sh` stays the one door to every Nebius VM; `--instance-id`/`--profile`
pick the VM and its CLI profile, and the default is nebius-gpu exactly as today. The watcher reads
its instance id and its kind (`gpu`|`cpu`) from a file `install-watch` writes, and applies a CPU
activity rule on the CPU VM. The hourly workflow runs one matrix leg per VM through a new
`scripts/nebius/watch.sh`. There are no host adapters: `run-gpu.sh` takes the VM's
`--instance-id`/`--profile` and calls `scripts/nebius.sh` for start, stop, ssh and rsh, so the
host-key and IP handling exist once. The rename (Task 1) is one commit; the guard files
(Tasks 2–5) touch nothing under `scripts/env/`; so the item-9 master commit (Task 12) can take
both alone.

**Tech Stack:** bash, Python 3 stdlib (the `scripts/nebius/tests` harness: fakes on PATH, no
network), GitHub Actions, the `nebius` CLI 0.12.x, Docker + NVIDIA Container Toolkit, Nsight
Systems, aws CLI v2, DuckDB CLI 1.5.4.

**Spec:** [`new-hosts.md`](new-hosts.md). Base: `ENS-containers-rebase` (containers-rebase).
Branch: `ENS-new-hosts`.

## Global Constraints


- **VM time is minimal** (the human's rule, 2026-11-03). A VM starts right before its first
  remote command and stops right after its last `pull`. It is never left up across a wait: a local
  build, CI, a review, the human. Binaries are built and staged first, and the H100 starts only
  once they are ready to ship. No step leaves a VM running for a later task. Each start and stop
  goes into the detail file's `## VM time` table, in the host's local time, with the hours per VM
  for the task. The watchers (1 h idle, 4 h per process, the hourly GitHub check) are the backstop,
  not the plan.
- Hosts: nebius-gpu `computeinstance-e00tnrse7ayntnzcyt` (default, caller's active profile);
  nebius-h100 `computeinstance-e00e6yena4nzjgaym0`, eu-north1, profile `editor-eu`;
  nebius-cpu128 `computeinstance-e01t2mm4265y8x88ey`, eu-west1, profile `eu-west1`. Login
  `dmitry` on all three, key `~/.ssh/id_ed25519` on dev, `NEBIUS_GPU_SSH_KEY` in CI.
- With neither flag, `nebius.sh` behaves exactly as today. The exit-code table is unchanged:
  0, 2, 75, 69, 1 (and 64 for usage).
- nebius-gpu's behaviour must not move: every existing file in `scripts/nebius/tests/` passes
  unmodified after Task 1's commit (`fakes.py` may only gain keys whose defaults keep today's
  behaviour).
- Names (spec item 10): `scripts/nebius.sh`, `scripts/nebius-sync.sh`, `scripts/nebius-test.sh`,
  `scripts/nebius/`, `.github/workflows/nebius-watch.yml`, state `~/.config/peacock/nebius/`;
  the device runner `scripts/env/run-gpu.sh`. No `gpu-host` name remains outside records.
- Never open a Nebius credential file (`~/.nebius/config.yaml`, `credentials.yaml`, any `*.pem`)
  on any host, `/root/.nebius/` included: no cat/grep/sed/Read/redaction. Copy with rsync/install
  only. Learn the CLI from `nebius … --help`.
- Every ssh from the Bash tool needs the sandbox off (`build-test.md`). A VM this task starts is
  stopped again (`nebius.sh … stop`) or left to its watcher, installed before the start.
- The 4-hour cap on one process stays on the GPU VMs; on the CPU VM it applies to DuckDB too.
- Pins from `docker/pins.env`: `DUCKDB_VERSION=1.5.4` + `DUCKDB_SHA256`, `AWS_CLI_VERSION` +
  `AWS_CLI_SHA256`. Buckets `tpch-sf40`, `tpcds-sf40`; endpoint and region from the env file
  (`AWS_ENDPOINT_URL`, `AWS_REGION`, `scripts/lib/s3.sh`). tpcds.sf40's aggregate sha256
  (`sha256sum *.parquet | sha256sum`) is
  `58610fc16152a12ea9769d645a6e72a4e03002e2498ec8d357862fcbd9bebb36`.
- `gpu-tests` stays on nebius-gpu only; no workflow is added. eu-west1 secrets:
  `NEBIUS_EU_WEST1_{SA_ID,PUBLIC_KEY_ID,PROJECT_ID,PRIVATE_KEY}` in Environment `nebius-gpu`.
- No host adapters (`scripts/env/host/`) and no host files (`~/.config/peacock/hosts/`):
  `run-gpu.sh` calls `scripts/nebius.sh --instance-id <id> --profile <p>` directly. The
  agent-container launcher is gone (containers-rebase).

## Review Focus

1. **A watcher that stops the wrong VM.** A missing or stale install file must never leave the
   H100 or the CPU VM's watcher calling stop on nebius-gpu's id. → Task 4,
   `test_install_watch_writes_the_instance_and_kind`; Task 6's live check that the log line's
   `instance` is the VM's own.
2. **A soft stop on the CPU VM.** There is no `nvidia-smi`; read as "unreachable", today's probe
   would stop a VM mid-DuckDB-run. → Task 2, `test_soft_stop_refused_while_duckdb_runs_on_a_cpu_vm`.
3. **One leg's trouble greys the whole watch.** Absent secrets, a deleted VM (69), or an
   unusable profile must each be a notice, never a red run, and never stop another leg's check.
   → Task 5, `test_watch_gone_instance_is_a_notice`, `test_workflow_legs_are_independent`.
4. **A profile that still points at a PEM.** eu-west1's key lives in a file; a VM profile that
   only works while the staged PEM exists breaks at the first reboot or cleanup. → Task 4,
   `test_install_watch_checks_the_profile_after_the_key_is_gone`.
5. **A fetch cut short.** A preempted fetch leaves a short object; a rerun must refetch it, not
   count it held. → Task 8, `test_fetch_refetches_a_short_object`.
6. **A caller still on the old name.** A missed `gpu-host.sh` in `pipeline.yml` breaks
   `gpu-tests` only on its next device run. → Task 1 Step 5: the grep is empty and the CI wiring
   guard passes; Task 12 the same on master.

---

### Task 1: the scripts named for the provider (spec item 10)

A rename and nothing else, so Task 2 onward diff against named files. Task 12 replays the same
recipe on master.

**Files:**
- Rename (`git mv`): `scripts/gpu-host.sh` → `scripts/nebius.sh`; `scripts/gpu-host-sync.sh` →
  `scripts/nebius-sync.sh`; `scripts/gpu-host-test.sh` → `scripts/nebius-test.sh`;
  `scripts/gpu-host/` → `scripts/nebius/`; `scripts/nebius/tests/test_gpu_host.py` →
  `test_nebius.py`; `scripts/nebius/tests/test_gpu_host_test.py` → `test_nebius_test.py`;
  `.github/workflows/gpu-host-watch.yml` → `.github/workflows/nebius-watch.yml`
- Modify: every file `git grep -l -E 'gpu-host|gpu_host|GPU host' -- . ':!llm-wiki/archive' ':!llm-wiki/reports' ':!llm-wiki/tasks'`
  lists on the fork. Expected among them: `.github/workflows/pipeline.yml` (`gpu-tests` :424-561,
  the script-test step :684-695), `.github/actions/nebius-setup/action.yml`, `.gitignore` (:62),
  `.claude/commands/ensemble.md`, `cost-report/tests/test_source_formatting/runners.rs`,
  `llm-wiki/{build-test,prompts}.md`, `llm-wiki/tickets/*.md`, `docker/run-tests.sh`,
  `scripts/{build-test,build-test-shadgpu,cargo-cudf,create_nsys_profile,setup-glibc}.sh`,
  `scripts/calibration/`, `scripts/lib/`, and comments in `peacockdb-core/`

**Interfaces:**
- Produces: the names above; `STATE_DIR=${GPU_HOST_STATE_DIR:-$HOME/.config/peacock/nebius}`, its
  log `$STATE_DIR/nebius.log`; in `runners.rs`, `pub(crate) const NEBIUS_TEST: &str = "scripts/nebius-test.sh";`
  (was `GPU_HOST_TEST`); `nebius-watch.yml`'s concurrency group `nebius-watch`; the section
  `## Nebius host` in `build-test.md`, every `"GPU host"` section reference in code and wiki
  reading `"Nebius host"`. Unchanged: env names (`GPU_HOST_STATE_DIR`, `GPU_HOST_USER`,
  `GPU_HOST_KEY`, `GPU_HOST_RSYNC`), the job name `GPU Tests (nebius-gpu)`, and on the VMs
  `peacock-gpu-idle.service`, `.timer`, `/var/log/peacock-gpu-idle.log`.

- [ ] **Step 1: baseline.** `for f in scripts/gpu-host/tests/test_*.py; do python3 "$f" || echo "FAIL $f"; done`
  and `cargo test -p cost-report --test test_source_formatting` → no `FAIL`, PASS.
- [ ] **Step 2: rename** with the `git mv` lines of Files, then the mechanical part:

```bash
git grep -l -E 'gpu-host|gpu_host|GPU host' -- . ':!llm-wiki/archive' ':!llm-wiki/reports' ':!llm-wiki/tasks' |
  xargs sed -i -E 's|gpu-host-sync|nebius-sync|g; s|gpu-host-test|nebius-test|g; s|gpu-host-watch|nebius-watch|g;
    s|gpu-host\.sh|nebius.sh|g; s|gpu-host\.log|nebius.log|g; s|peacock/gpu-host|peacock/nebius|g;
    s|scripts/gpu-host/|scripts/nebius/|g; s|test_gpu_host_test|test_nebius_test|g; s|test_gpu_host|test_nebius|g;
    s|GPU_HOST_TEST|NEBIUS_TEST|g; s|"GPU host"|"Nebius host"|g; s|^## GPU host$|## Nebius host|; s|#gpu-host\)|#nebius-host)|g'
```
- [ ] **Step 3: by hand.** `git grep -n -E 'gpu-host|gpu_host' -- . ':!llm-wiki/archive' ':!llm-wiki/reports' ':!llm-wiki/tasks'`
  → each remaining hit is prose ("gpu-host's scripts", "no gpu-host test files matched"); reword
  each to `nebius`. A `GPU host` that names the machine kind, not the section, stays.
- [ ] **Step 4: dev's state.** `[ -d ~/.config/peacock/gpu-host ] && cp -a ~/.config/peacock/gpu-host ~/.config/peacock/nebius && mv ~/.config/peacock/nebius/gpu-host.log ~/.config/peacock/nebius/nebius.log`
  (a copy: master's checkout and other chains keep the old directory until Task 12 lands).
- [ ] **Step 5: verify.** Step 1's loop over `scripts/nebius/tests/test_*.py` → no `FAIL`;
  `cargo test -p cost-report --test test_source_formatting` → PASS; Step 3's grep → nothing;
  `git diff -M --stat HEAD` shows each moved file as a rename;
  `docker run --rm -v "$PWD:/repo" -w /repo rhysd/actionlint:latest .github/workflows/nebius-watch.yml .github/workflows/pipeline.yml`
  → exit 0.
- [ ] **Step 6: Commit** `nebius: the GPU-host scripts, their tests and the watch workflow named for the provider`.

### Task 2: `nebius.sh` picks its instance and profile

**Files:**
- Modify: `scripts/nebius.sh` (header 2–16, constants 21–22, `cli` 44, `ssh_opts` 94–101,
  `trust_current_key` 105–115, `cmd_stop` 189–211, verb dispatch 247–256)
- Modify: `scripts/nebius-sync.sh` (rsync line 18–20, ssh line 29): passes its leading
  `--instance-id/--profile` through to every `nebius.sh` call
- Modify: `scripts/nebius/tests/fakes.py` (additive keys `no_gpu: False`, `cpu_busy: ""`)
- Create: `scripts/nebius/tests/test_nebius_instances.py`

**Interfaces:**
- Produces: `nebius.sh [--instance-id <id>] [--profile <name>] <verb> …`, options before the
  verb, each at most once. `<id>` must match `^computeinstance-[a-z0-9]+$`, `<name>`
  `^[A-Za-z0-9_.-]+$`, else exit 64 naming the flag. `--profile` adds `--profile <name>` to every
  `nebius` call. Host-key alias: `nebius-gpu` for the default id, `nebius-<id>` for any other, in
  the same `$STATE_DIR/known_hosts`; `trust_current_key` replaces only its own alias's line.
- Produces: the soft-stop probe, one remote command:
  `if command -v nvidia-smi >/dev/null; then nvidia-smi --query-compute-apps=pid,process_name --format=csv,noheader; else pgrep -a -x duckdb || true; fi`.
  Non-empty output → exit 2. Log text: `busy: …` (the card's apps or the DuckDB pids).
- Produces: `nebius-sync.sh [--instance-id <id>] [--profile <name>] [--dest <dir>]`.

- [ ] **Step 1: Write the failing tests** in `test_nebius_instances.py` (same `run()` helper as
  `test_nebius.py`):
  - `test_instance_id_reaches_every_cli_call`: `--instance-id computeinstance-e00e6yena4nzjgaym0 status`
    → every recorded `nebius` argv holds `--id computeinstance-e00e6yena4nzjgaym0`.
  - `test_profile_reaches_every_cli_call`: `--profile eu-west1 start` → every `nebius` argv holds
    `--profile eu-west1`; without `--profile` no argv holds `--profile`.
  - `test_each_instance_has_its_own_alias`: first contact on the default and on the H100 id →
    `known_hosts` holds a `nebius-gpu ` line and a `nebius-computeinstance-e00e6yena4nzjgaym0 `
    line; a changed key on the H100 replaces only its own line, the `nebius-gpu` line is byte-equal.
  - `test_explicit_default_id_keeps_the_nebius_gpu_alias`.
  - `test_bad_instance_id_or_profile_is_64`: `--instance-id 'x;rm'`, `--profile ''`, a flag twice
    → 64, no `nebius` call.
  - `test_soft_stop_refused_while_duckdb_runs_on_a_cpu_vm`: `World(state="RUNNING", no_gpu=True,
    cpu_busy="4242 duckdb")` → `stop` exit 2, no `stop` call to nebius.
  - `test_soft_stop_on_an_idle_cpu_vm_stops`: `no_gpu=True, cpu_busy=""` → 0, state STOPPED.
  - `test_sync_passes_the_instance_through`: `nebius-sync.sh --instance-id <h100> --dest d` with
    `GPU_HOST_RSYNC` faked → the `-e` transport string holds `--instance-id <h100>`.
  Fake `ssh` change: when the remote command holds `nvidia-smi` and `no_gpu` is false, print
  `card` (today's behaviour); when `no_gpu` is true, print `cpu_busy`.
- [ ] **Step 2: Run** `python3 scripts/nebius/tests/test_nebius_instances.py` → FAIL (unknown
  flag reaches the `*)` usage branch, exit 64 everywhere).
- [ ] **Step 3: Implement** the option loop before `verb=`, `INSTANCE_ID=${opt:-computeinstance-e00tnrse7ayntnzcyt}`,
  the alias rule, `cli()` adding the profile, the probe above in `cmd_stop`. Update the header
  comment. `nebius-sync.sh` collects the two options, builds `gh=("$here/nebius.sh" "${opts[@]}")`
  and uses `-e "${gh[*]} rsh"`. `nebius-test.sh` runs on the VM and needs no change: confirm.
- [ ] **Step 4: Run all six suites**
  `for f in scripts/nebius/tests/test_*.py; do python3 "$f" || echo "FAIL $f"; done` → every file
  ends `N passed, 0 failed`, no `FAIL` line; `git diff --stat -- scripts/nebius/tests/test_nebius.py`
  empty.
- [ ] **Step 5: Commit** `nebius: --instance-id and --profile pick the VM; an alias per instance; a soft stop knows a CPU VM`.

### Task 3: the watcher's instance and kind come from its install; the CPU rule

**Files:**
- Modify: `scripts/nebius/idle_watch.py` (constants 13–20, `decide` 23–48, `log_line` 51–56,
  `main` 138–179)
- Modify: `scripts/nebius/check.py` (`_judge` 41–52)
- Create: `scripts/nebius/tests/test_idle_watch_kinds.py`, `scripts/nebius/tests/test_check_kinds.py`

**Interfaces:**
- Produces: install file `INSTALL = "/etc/peacock-gpu-idle.json"`, JSON
  `{"instance_id": "<id>", "kind": "gpu"|"cpu"}`. `load_install(path) -> (instance_id, kind, problem|None)`.
  Missing file → `("computeinstance-e00tnrse7ayntnzcyt", "gpu", None)`: the pre-task nebius-gpu
  install, which the unmodified tests run as. Unreadable/invalid file (bad JSON, id not matching
  `^computeinstance-[a-z0-9]+$`, kind not gpu/cpu) → id `None`, kind `"gpu"`, a problem string;
  `_stop(None)` returns `"stop: no instance id installed"` without calling nebius, so the tick
  logs and exits 1, and the hourly checker (Task 5) is the stop.
- Produces: `decide(state, now, boot, card, tests, build, sessions=())`; `sessions` is a list of
  last-input epoch seconds of interactive terminals: `last_test = max(last_test, s)` for each
  `s` with `boot <= s <= now`. No cap applies to a session.
- Produces: `_stop(instance_id)`; `log_line(…, error=None, sessions=(), instance=None, kind="gpu")`
  adds keys `sessions`, `instance`, `kind`.
- Produces: kind `cpu`: no `nvidia-smi` probe (`card == []`); capped activity
  `CPU_TEST_NAMES = ("duckdb", "aws")` (a DuckDB run, a data fetch), carried in `test_procs`;
  `CPU_BUILD_NAMES = BUILD_NAMES + ("apt-get", "dpkg", "docker", "git", "curl")` (bootstrap's
  work, the 2-hour rule); sessions = `st_atime` of each `/dev/pts/<n>` (an ssh login's terminal;
  non-interactive `nebius.sh ssh` has none). Kind `gpu`: today's probes, `sessions == []`.
- Produces: `check.py` passes `l.get("sessions", [])` (floats) to `decide`; a line without the key
  (an old watcher) is judged as today.

- [ ] **Step 1: Write the failing tests.** `test_idle_watch_kinds.py` (reuse the `World` pattern of
  `test_idle_watch.py`, patching `w.INSTALL` to a temp file):
  - `test_install_file_names_the_instance_stopped`: `{"instance_id": "computeinstance-e01t2mm4265y8x88ey", "kind": "cpu"}`,
    idle 2 h → the one nebius argv holds `--id computeinstance-e01t2mm4265y8x88ey`; the line's
    `instance` is that id and `kind` is `cpu`.
  - `test_missing_install_file_is_nebius_gpu`: no file → `--id computeinstance-e00tnrse7ayntnzcyt`, `error is None`.
  - `test_broken_install_file_never_stops_a_guessed_vm`: `{"instance_id": "x"}` and `not json` →
    no nebius call, `error` holds `install` and `no instance id`, `main()` returns 1.
  - `test_cpu_kind_never_runs_nvidia_smi`: no argv[0] `nvidia-smi` in calls.
  - `test_cpu_duckdb_is_activity_and_capped`: ps `" 7 60 duckdb\n"` → keep; `" 7 14411 duckdb\n"` →
    `cap-stop`, reason holds `pid 7`.
  - `test_cpu_fetch_is_activity`: ps `" 9 60 aws\n"` → keep.
  - `test_cpu_session_input_is_activity`: last test 2 h ago, a session with input 5 min ago → keep;
    input 70 min ago → `idle-stop`.
  - `test_gpu_kind_ignores_sessions` (a gpu install with a fresh pts still idle-stops).
  `test_check_kinds.py`: `test_session_in_the_line_keeps` (line with `last_test` 2 h old and
  `sessions: [NOW-300]` → `None`); `test_line_without_sessions_judged_as_before`;
  `test_cpu_duckdb_cap_recomputed_is_stop` (`test_procs` with a 4 h `duckdb`).
- [ ] **Step 2: Run** both files → FAIL (`INSTALL` missing, `decide` takes no `sessions`).
- [ ] **Step 3: Implement** in `idle_watch.py` and `check.py`; module docstring names both kinds.
  `peacock-gpu-idle.service`/`.timer` stay as they are (same names on all three VMs, so
  `nebius-watch` reads one log path everywhere).
- [ ] **Step 4: Run** every `scripts/nebius/tests/test_*.py` → all `0 failed`;
  `git diff --stat -- scripts/nebius/tests/test_idle_watch.py scripts/nebius/tests/test_check.py` empty.
- [ ] **Step 5: Commit** `nebius: the watcher's instance and kind from its install; DuckDB, fetches and terminals are use on a CPU VM`.

### Task 4: `install-watch` installs one self-contained profile and the install file

**Files:**
- Modify: `scripts/nebius.sh` `cmd_install_watch` (213–245) and dispatch
- Create: `scripts/nebius/tests/test_install_watch.py`

**Interfaces:**
- Consumes: Task 2's options, Task 3's `INSTALL` path and JSON.
- Produces: `nebius.sh [--instance-id …] [--profile <p>] install-watch [--key-file <pem>]`.
  `--key-file` names the PEM a key-path profile uses (eu-west1); it is rsynced, never read.
  Sequence on the VM, in one `vm_ssh` whose text never contains the literal `HOME=/root` (the
  existing test counts exactly one such ssh, the final check):
  1. stage `~/peacock-gpu-idle/nebius-config.yaml` (and `sa.pem`) into `/root/.nebius/staging/`
     (`sudo install -m 600`), remove the user's copies;
  2. `P=<--profile, or $(nebius -c S profile active)>`; with a key: `nebius -c S profile update "$P" --private-key-file S/sa.pem`;
  3. every other profile deleted: names from `nebius -c S profile list --format <fmt>` through a
     Python filter that prints names only and exits 1 without echoing input on an unexpected shape;
     `nebius -c S profile activate "$P"`;
  4. `S/sa.pem` removed; `S/config.yaml` moved to `/root/.nebius/config.yaml` (600); staging removed;
  5. `/etc/peacock-gpu-idle.json` written: `{"instance_id": "$INSTANCE_ID", "kind": "gpu"}` if
     `command -v nvidia-smi` else `cpu`; it prints `kind=<k>`;
  6. then the existing final check (`sudo env HOME=/root timeout 60 /usr/local/bin/nebius compute instance get --id $INSTANCE_ID >/dev/null`).
  All `nebius` output in steps 2–4 goes to `/dev/null` except the filtered names count.

- [ ] **Step 1: Find the list format, safely.** On dev, in the scratchpad: generate a throwaway
  key (`openssl genpkey -algorithm ed25519 -out t.pem`), `nebius -c ./t.yaml profile create t1 --skip-auth --endpoint api.nebius.cloud --service-account-id serviceaccount-x --public-key-id publickey-x --private-key-file t.pem --parent-id project-x`,
  the same for `t2`, then `nebius -c ./t.yaml profile list --format json` and `profile active`.
  Record the shape in the detail file; delete `t.*`. Never point `-c` at `~/.nebius`.
- [ ] **Step 2: Write the failing tests** in `test_install_watch.py` (a fake `HOME` with an empty
  `.nebius/config.yaml` and a `k.pem` holding `FAKE`):
  - `test_install_watch_writes_the_instance_and_kind`: `--instance-id <h100> install-watch` → one
    ssh command holds `/etc/peacock-gpu-idle.json` and `<h100>`, and `command -v nvidia-smi`.
  - `test_install_watch_keeps_one_profile`: `--profile editor-eu` → the VM script holds
    `profile activate "$P"`, `profile delete`, and `P=editor-eu`; no rsync source is a `.pem`.
  - `test_install_watch_copies_the_key_file_unread`: `--profile eu-west1 install-watch --key-file <home>/k.pem`
    → an rsync source is `k.pem`; `FAKE` appears in no argv, stdout or stderr.
  - `test_install_watch_checks_the_profile_after_the_key_is_gone`: in the ssh call order, the
    command removing `sa.pem` precedes the `HOME=/root … compute instance get` call.
  - `test_install_watch_refuses_a_missing_key_file` → exit 1 naming `--key-file`, no ssh.
- [ ] **Step 3: Run** → FAIL. **Step 4: Implement.** **Step 5: Run** every
  `scripts/nebius/tests/test_*.py` → `0 failed` (the two existing install-watch tests unmodified).
- [ ] **Step 6: Commit** `nebius: install-watch installs one profile, key embedded, and the watcher's instance`.

### Task 5: the hourly watch over three VMs

**Files:**
- Create: `scripts/nebius/watch.sh` — the body of today's check step (`nebius-watch.yml`
  44–90), parameterised
- Modify: `.github/workflows/nebius-watch.yml`, `.github/actions/nebius-setup/action.yml`
- Modify: `scripts/nebius/tests/fakes.py` (additive: `uptime: "99999"`, `watch_log: ""` answered
  for `/proc/uptime` and `tail -1 /var/log/peacock-gpu-idle.log`)
- Create: `scripts/nebius/tests/test_watch.py`, `scripts/nebius/tests/test_watch_workflow.py`

**Interfaces:**
- Produces: `watch.sh <vm name> [nebius.sh options…]`, env `CLOCK_OFFSET` (validated as today),
  `GPU_WATCH_WINDOW` (default 900 s), `GPU_WATCH_PAUSE` (default 60 s). Exit 0 for: not running;
  instance gone (`status` rc 69 → `::notice::<vm>: instance no longer exists; skipped`); kept;
  hard-stopped. Exit 1 only when `status` fails otherwise or the stop fails. Warnings name `<vm>`.
- Produces: action input `profile` (default `ci`): `nebius profile create <profile>`, activation as
  today. pipeline.yml's call is unchanged.
- Produces: workflow job `watch`, `strategy: {fail-fast: false, matrix: {include: [...]}}` with legs
  `{vm: nebius-gpu, instance: "", profile: ci, prefix: NEBIUS}`,
  `{vm: nebius-h100, instance: computeinstance-e00e6yena4nzjgaym0, profile: ci, prefix: NEBIUS}`,
  `{vm: nebius-cpu128, instance: computeinstance-e01t2mm4265y8x88ey, profile: eu-west1, prefix: NEBIUS_EU_WEST1}`.
  Steps: `secrets` (id; env from `secrets[format('{0}_SA_ID', matrix.prefix)]` and the other three
  plus `NEBIUS_GPU_SSH_KEY`; output `present`; notice when any is empty) → `setup` (`if: present`,
  `continue-on-error: true`) → notice step `if: steps.setup.outcome == 'failure'`
  (`::notice::<vm>: the <profile> profile could not be created; skipped`) → check step
  `if: steps.setup.outcome == 'success'` running
  `scripts/nebius/watch.sh <vm> ${instance:+--instance-id $instance} --profile <profile>`.

- [ ] **Step 1: Write the failing tests.** `test_watch.py` runs the real `nebius.sh` against
  `fakes.World`, `GPU_WATCH_WINDOW=2 GPU_WATCH_PAUSE=0`:
  - `test_watch_stopped_vm_is_a_no_op` (rc 0, no `stop`);
  - `test_watch_gone_instance_is_a_notice` (`exists=False` → rc 0, stdout holds `::notice::nebius-h100`);
  - `test_watch_fresh_keep_line_keeps` (`watch_log` a keep line from `time.time()-60`);
  - `test_watch_stale_line_hard_stops` (→ STOPPED, `::warning::hard-stopping nebius-cpu128`);
  - `test_watch_ssh_down_hard_stops`; `test_watch_young_vm_waits` (`uptime="300"`);
  - `test_watch_passes_instance_and_profile` (every nebius argv holds both);
  - `test_watch_bad_clock_offset_is_1`.
  `test_watch_workflow.py` (text and regex over the YAML, no parser needed):
  `test_workflow_has_three_legs` (the three `vm:` values and two instance ids verbatim);
  `test_workflow_legs_are_independent` (`fail-fast: false`; `continue-on-error: true` on `setup`);
  `test_workflow_secrets_reach_only_env` (every `secrets[` sits under an `env:` key, never in a
  `run:` block); `test_workflow_cpu_leg_uses_eu_west1` (`prefix: NEBIUS_EU_WEST1`, `profile: eu-west1`);
  `test_gpu_tests_untouched` (`pipeline.yml`'s nebius-setup call has no `profile:`).
- [ ] **Step 2: Run** both → FAIL. **Step 3: Implement** `watch.sh` by moving the step body, and
  the workflow and action. **Step 4: Run** both files and every `scripts/nebius/tests/test_*.py`
  → `0 failed`; `docker run --rm -v "$PWD:/repo" -w /repo rhysd/actionlint:latest .github/workflows/nebius-watch.yml`
  → no output, exit 0.
- [ ] **Step 5: Commit** `nebius-watch: three VMs, one leg each, skipped with a notice when a VM or its secrets are gone`.

### Task 6: watchers live on nebius-h100 and nebius-cpu128

**Files:** none in the tree; detail file only.

- [ ] **Step 1: The key files, as the human named them (2026-11-03).** On dev: profile `eu-west1` →
  `~/.nebius/eu-west1-sa.pem`; profile `editor-eu` → `~/.nebius/sa-key.pem`. Check only that both
  names exist (`ls -1 ~/.nebius/`, names only; open nothing). Each VM's `install-watch` passes its
  profile's file as `--key-file`.
- [ ] **Step 2: H100.** `H="scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu"`;
  `$H start && $H install-watch --key-file ~/.nebius/sa-key.pem` → last log line `install-watch: watcher installed, root's profile reaches the API`;
  `$H ssh 'sudo cat /etc/peacock-gpu-idle.json; sleep 70; tail -1 /var/log/peacock-gpu-idle.log'`
  → install file with the H100 id and `gpu`; the log line's `instance` is the H100 id, `kind` `gpu`.
- [ ] **Step 3: CPU VM.** The same with `--instance-id computeinstance-e01t2mm4265y8x88ey --profile eu-west1`
  and `install-watch --key-file ~/.nebius/eu-west1-sa.pem` → `kind=cpu`; the log line's `card` is `[]`, `kind` `cpu`.
  `sudo ls /root/.nebius/` shows no `.pem` and no `staging` (names only).
- [ ] **Step 4: The CI key on both.** `scripts/nebius.sh start` (nebius-gpu), then on each of the
  three run `ssh-keygen -lf ~/.ssh/authorized_keys` (public keys only). Every fingerprint nebius-gpu
  holds and a new VM lacks is appended to that VM's `authorized_keys` from nebius-gpu's line. Stop
  nebius-gpu.
- [ ] **Step 5: Dispatch from the branch** with both new VMs running and up > 10 min:
  `gh workflow run nebius-watch.yml --ref ENS-new-hosts` → three legs green; nebius-gpu
  `not running`, the two new legs `in use; keeping it` or a keep after their watcher's verdict.
  Then `gh workflow run nebius-watch.yml --ref ENS-new-hosts -f clock_offset=7200` → both new
  legs `::warning::hard-stopping …`, both VMs `STOPPED` per `status`. Record run ids.

### Task 7: the fresh-VM check on nebius-cpu128 (containers' Tasks 6 and 6a)

**Files:**
- Modify (only what the run shows broken): `scripts/env/provision-vm.sh`, `scripts/env/bootstrap-vm.sh`,
  their tests. Known: `provision-vm.sh`'s last step runs `claude auth login` over `ssh -t`; from a
  non-terminal it fails the whole run.
- Modify: `scripts/env/tests/test_provision.py`

**Interfaces:**
- Produces: `provision-vm.sh` with stdin not a terminal skips the Claude login and prints
  `Claude Code is not logged in on <dev>: run ssh -t <dev> 'claude auth login'`, exit 0.

- [ ] **Step 1: Failing test** `test_provision_without_a_terminal_names_the_claude_login` (stdin
  `/dev/null`, no credentials on the fake VM → rc 0, no `-t` ssh, stderr names `claude auth login`).
  Run `python3 scripts/env/tests/test_provision.py` → FAIL. Implement (`[ -t 0 ]` guard). Run → pass.
- [ ] **Step 2: 6a's fake-ssh tests.** `for f in scripts/env/tests/test_{provision,bootstrap}.py; do python3 "$f"; done`
  → each `0 failed`.
- [ ] **Step 3: Preconditions on dev.** `test -s ~/.config/peacock/github-pat && echo yes` (never
  print it). Missing → stall for the human: `provision-vm.sh` reads the PAT from a terminal. Push
  the branch.
- [ ] **Step 4: Run it.** `C="scripts/nebius.sh --instance-id computeinstance-e01t2mm4265y8x88ey --profile eu-west1"`;
  `$C start`; `time scripts/env/provision-vm.sh dmitry@$($C ip) --name nebius-cpu128 --ref ENS-new-hosts </dev/null` (the GPU key is the default `~/.config/peacock/gpu_ed25519`, a copy of dev's `~/.ssh/id_ed25519`, put there 2026-11-03)
  → exit 0. Check over `$C ssh`: `docker --version`, `gh --version` = `GH_VERSION`,
  `~/.local/bin/claude --version` = `CLAUDE_CODE_VERSION`, `git -C ~/peacockdb branch --show-current`
  = `ENS-new-hosts`, an sf1 set under `/var/lib/peacock/<hash>/{tpch.sf1,tpcds.sf1}`,
  and no `~/.config/peacock/machine` (`test ! -e`). Bootstrap's work counts under the 2-hour build rule; if a
  run outlasts it, record it and rerun (the rerun resumes).
- [ ] **Step 5: Rerun** the same command → bootstrap runs again and reports every step skipped,
  wall time recorded, exit 0.
- [ ] **Step 6: The GPU-host key reaches the H100.** `$C ssh 'grep -c "^GPU_SSH_KEY_FILE=" ~/.config/peacock/peacock.env; stat -c %a ~/.config/peacock/gpu_ed25519'`
  → `1` and `600`; its fingerprint equals dev's `ssh-keygen -lf ~/.ssh/id_ed25519.pub` (public
  halves only). Nothing is authorized anew: the H100 already accepts it. With the H100 started:
  `$C ssh 'ssh -i ~/.config/peacock/gpu_ed25519 -o BatchMode=yes -o StrictHostKeyChecking=accept-new dmitry@<H100 ip> true'`
  → exit 0. Stop the H100.
  Fix anything broken with a failing test first; commit
  `containers: provision-vm.sh and bootstrap-vm.sh verified on a fresh VM; <fixes>`.

### Task 8: sf40 on both VMs; DuckDB and its database on the CPU VM

**Files:**
- Create: `scripts/env/fetch-sf40.sh`, `scripts/env/install-duckdb.sh`
- Create: `scripts/env/tests/test_fetch_sf40.py`, `scripts/env/tests/test_install_duckdb.py`

**Interfaces:**
- Produces: `fetch-sf40.sh [--data-dir <d>] [--env <file>] <dataset>…`, datasets `tpch.sf40`,
  `tpcds.sf40` (bucket = name with `.`→`-`). Runs on the VM. Data dir default `${PEACOCK_DATA_DIR:-/var/lib/peacock}`;
  writes `<d>/<dataset>/<table>.parquet`. Keys from `--env` (default `~/.config/peacock/peacock.env`,
  `-` = stdin), exported to aws only, never argv. Uses `${AWS:-aws}`; if absent installs the pinned
  aws CLI v2 into `~/.local/aws-cli` (zip sha256 = `AWS_CLI_SHA256`). Per object: the bucket's
  `manifest.json` gives each size; a local file of that size is kept, any other is fetched to
  `<file>.part`, size-checked, renamed. After all: every manifest file present at its size, no extra
  `.parquet`; aggregate sha256 compared when pinned (`tpcds.sf40` above; `tpch.sf40` pinned in
  Step 4). Prints `<dataset>: fetched <n>, kept <m>`. Exit 2 bad input, 1 a check failed.
- Produces: `install-duckdb.sh [--prefix <dir>]` → `/usr/local/bin/duckdb` at `DUCKDB_VERSION`
  (sha256 checked); skipped when `duckdb --version` already starts `v1.5.4`.

- [ ] **Step 1: Failing tests** (fake `aws` on PATH reading a fake bucket dir):
  `test_fetch_takes_endpoint_and_region_from_env` (aws argv holds `--endpoint-url`/`--region`
  values; no key value in any argv); `test_fetch_second_run_fetches_nothing` (second run: no `s3 cp`
  of a parquet, prints `fetched 0, kept N`); `test_fetch_refetches_a_short_object`;
  `test_fetch_refuses_a_wrong_aggregate` (pinned sha differs → rc 1, the dataset named);
  `test_fetch_refuses_an_unknown_dataset` (rc 2); `test_fetch_keys_from_stdin` (`--env -`);
  `test_install_duckdb_checks_the_sha` and `test_install_duckdb_skips_the_pinned_version`.
- [ ] **Step 2: Run** → FAIL. **Step 3: Implement.** **Step 4: Run** → `0 failed`; commit.
- [ ] **Step 5: CPU VM, live.** `$C start`; `$C ssh 'df -h /var/lib/peacock'` — at least 110 GB
  free (parquet 56 GB + the tpch db ~38 GB + 10 %), else stall. Pull the branch on the VM, then
  `$C ssh 'cd ~/peacockdb && time scripts/env/fetch-sf40.sh tpch.sf40 tpcds.sf40'` → both lines,
  exit 0; again → `fetched 0` twice. Record tpch.sf40's aggregate sha256 and pin it in
  `fetch-sf40.sh` (it must match the H100's in Task 10).
- [ ] **Step 6: DuckDB.** `$C ssh 'cd ~/peacockdb && scripts/env/install-duckdb.sh && time benchmarks/duckdb_minimal.sh --dir /var/lib/peacock --sf 40 --skip-download --duckdb /usr/local/bin/duckdb'`
  → `/var/lib/peacock/tpch.sf40.duckdb` exists, row counts match, every query verified. Record the
  timing run's wall time: over 4 h means `CAP` for `cpu` is raised in `idle_watch.py` with that
  measurement in the comment, else it stays. (The tpcds database comes with sf40-baseline's
  extension of the script.) `$C stop`. Commit the pin.

### Task 9: `run-gpu.sh` and the staged install (containers' Task 7, no host adapters)

**Files:**
- Create: `scripts/env/run-gpu.sh`, `scripts/env/stage-install.sh`
- Modify: `scripts/create_nsys_profile.sh` (`--ssh <command>`)
- Create: `scripts/env/tests/test_run_gpu.py`, `scripts/env/tests/test_stage_install.py`

**Interfaces:**
- Consumes: Task 2's `scripts/nebius.sh [--instance-id <id>] [--profile <name>] start|stop [--force]|status|ip|ssh <cmd…>|rsh <host> <cmd…>`
  (exit 64 on a bad option); Task 8's `fetch-sf40.sh --env -`; containers' `machine_data_set`,
  `machine_image_checked` (`scripts/lib/machine.sh`).
- Produces: `run-gpu.sh [--instance-id <id>] [--profile <name>] [--dir <i>] [--cudf <v>] prepare | ship [--install <dir>] | run <list|filter|bench> | pull`.
  The two VM options go, as given, to every `${NEBIUS_SH:-<repo>/scripts/nebius.sh}` call; with
  neither, the VM is nebius.sh's default, nebius-gpu. Remote commands go through `nebius.sh … ssh`,
  every rsync through `-e "<nebius.sh> <options> rsh"`; the login is nebius.sh's. `run-gpu.sh`
  never starts or stops the VM: the caller does, with `scripts/nebius.sh`.
  `<i>` defaults to the repo root's basename; `--install` defaults to the repo root's `cpp/install`
  (where `stage-install.sh` stages it; a missing dir → exit 2 naming it); host dir `~/peacock/<i>/` holding `cpp/install/` and
  `testdata/` (the tree's committed testdata, datasets excluded, plus absolute symlinks
  `testdata/<ds>.sf1 -> /var/lib/peacock/<hash>/<ds>.sf1`, `testdata/<ds>.sf40 -> /var/lib/peacock/<ds>.sf40`).
  `prepare`: Docker and the NVIDIA
  Container Toolkit if missing; `nsys` if missing; `/etc/modprobe.d/peacock-profiling.conf` =
  `options nvidia NVreg_RestrictProfilingToAdminUsers=0` and, while `/proc/driver/nvidia/params`
  shows `RmProfilingAdminOnly: 1`, `update-initramfs -u` and **exit 4** ("restart the host");
  sf1 via `machine_data_set` in the build image; sf40 via `fetch-sf40.sh --env -` with the env file
  (`~/.config/peacock/peacock.env`) on stdin; `docker pull` the run image and the inputs check (`machine_image_checked`, exit 2
  naming the file); the driver check against `peacockdb.driver-minimum` (**exit 3**, naming both);
  the run image's `/opt/cudf` copied to `~/peacock/cudf-<v>/` (stamped with the image id; skipped
  when the stamp matches) for host-side Nsight. `run`: the driver check (exit 3), then
  `docker run -d --name peacock-<i> --gpus all --user <uid:gid> --hostname "$(hostname)" -v ~/peacock/<i>/cpp/install:/peacock/install:ro -v ~/peacock/<i>/testdata:/peacock/testdata -v /var/lib/peacock:/var/lib/peacock:ro [-e PCK_TEST_FILTER] [-e PEACOCK_BENCH_RESUME] <run image> <mode>`,
  `docker logs -f`, exit = `docker wait`; a running `peacock-<i>` refuses (exit 2). `pull`:
  `testdata/benchmark-results/`, `testdata/calibration/` and the run's log home. Exit codes: 0, 1
  run failed, 2 bad input (nebius.sh's 64 included)/stale image/busy, 3 old driver, 4 restart
  needed, else nebius.sh's (75, 69).
- Produces: `stage-install.sh [--cudf <v>]`, run inside the build image at the tree root:
  `scripts/build.sh --configure --build --install` (`--cudf_ROOT /opt/cudf --gcc-version "$GCC_VERSION"`),
  then `cargo test --no-run --features gpu` for `test_gpu_corpus`, `test_node_timing`,
  `peacock_gpu_benchmarks` and `--lib` (staged as `peacockdb_core_gpu_lib`) into
  `cpp/install/rust-tests/`, `peacock_gpu_benchmarks` again `--release` into
  `cpp/install/rust-benchmarks/`, each `strip --strip-debug`.
- Produces: `create_nsys_profile.sh --ssh <command>` (default `ssh`): every `ssh "$REMOTE"` and
  every rsync's `-e` use it; `--ssh 'scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu rsh'`
  reaches the H100.

- [ ] **Step 1: Failing tests**, a fake `NEBIUS_SH` recording its argv and fakes on PATH as
  `scripts/env/tests/laptop.py` does: containers' — `test_run_refuses_old_driver` (exit 3 before
  any rsync, stderr holds both versions), `test_ship_dir_is_named_by_dir`,
  `test_ship_defaults_to_the_staged_install`, `test_run_passes_the_machines_hostname`,
  `test_prepare_refuses_stale_run_image` — and
  `test_run_gpu_passes_instance_and_profile_to_every_nebius_call` (every recorded call holds
  `--instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu` before its verb),
  `test_run_gpu_without_options_reaches_the_default_vm` (no call holds `--instance-id` or
  `--profile`), `test_run_gpu_bad_instance_id_is_2` (fake answers 64 → exit 2, no rsync),
  `test_run_gpu_never_starts_or_stops_the_vm` (no `start` or `stop` call over all four verbs),
  `test_prepare_sends_keys_on_stdin_only` (no key value in any argv),
  `test_prepare_says_restart_when_counters_are_admin_only` (exit 4),
  `test_run_mounts_the_data_dir_at_its_own_path`, `test_run_refuses_a_run_in_progress`,
  `test_stage_install_stages_four_tests_and_a_release_bench` (fake cargo/build.sh recording argv),
  `test_nsys_profile_uses_the_given_ssh` (fake ssh and rsync; every call through the `--ssh` command).
- [ ] **Step 2: Run** → FAIL. **Step 3: Implement.** **Step 4: Run** every
  `scripts/env/tests/test_*.py` that needs no image → `0 failed`;
  `git ls-files scripts/env/host 'scripts/env/gpu.sh'` → nothing. **Step 5: Commit**
  `containers: run-gpu.sh, the device run over scripts/nebius.sh; the staged install; Nsight over any ssh`.

### Task 10: the H100 runs the device tiers, the benchmark and Nsight

**Files:** none expected; fixes get a failing test first. Detail file: counts, wall times, ids.

- [ ] **Step 1: Prepare.** `N="scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu"`;
  `G="scripts/env/run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu"`; `$N start`;
  `$G prepare` → exit 4 the first time; `$N stop --force && $N start`;
  `$G prepare` → exit 0, `fetched` lines for both sf40 sets. Check: `$N ssh 'grep RmProfilingAdminOnly /proc/driver/nvidia/params; nsys --version; ldd --version | head -1'`
  → `RmProfilingAdminOnly: 0`, an nsys version, glibc ≥ 2.39 (else stall: the exported prefix
  needs the image's glibc). tpch.sf40's aggregate sha256 equals Task 8's pin. Second
  `$G prepare` → `fetched 0` twice, no pull of a new image, exit 0.
- [ ] **Step 2: The staged install, on nebius-cpu128** (128 cores, build image present):
  `$C start`; `$C ssh 'cd ~/peacockdb && git pull --ff-only && docker run --rm --user $(id -u):$(id -g) -e HOME=/tmp -v $PWD:$PWD -w $PWD -v /var/lib/peacock:/var/lib/peacock:ro ghcr.io/asymptote-tech/peacockdb-build:cudf-25.02 scripts/env/stage-install.sh'`
  → exit 0; rsync `cpp/install/` to dev at `/build/peacock/new-hosts/install` with `-e "$C rsh"`; `$C stop`.
- [ ] **Step 3: The sf1 corpus.** `$G ship --install /build/peacock/new-hosts/install && $G run list`
  → exit 0; record per-binary counts beside the latest `GPU Tests (nebius-gpu)` run's.
- [ ] **Step 4: One benchmark case.** `PCK_TEST_FILTER=bench_tpch_sf40_q6_tp1_single $G run bench && $G pull`
  → exit 0, a `testdata/benchmark-results/<h100 slug>/tpch.sf40/tp1_single.benchmark.txt` with
  `host=` the VM's hostname. Not committed (sf40-baseline owns results): `git restore`/remove.
- [ ] **Step 5: Nsight.** `PCK_TEST_FILTER=bench_tpch_sf40_q6_tp1_single scripts/create_nsys_profile.sh --trace --ssh "$N rsh" --host nebius-h100 --remote-dir /home/dmitry/peacock/<i> --remote-cudf-root /home/dmitry/peacock/cudf-25.02`
  → `the trace pass ran 1 tests`, `calls.tsv` written. Counters: on the H100 as dmitry,
  `nsys profile --gpu-metrics-devices=all -o /tmp/m --force-overwrite=true env LD_LIBRARY_PATH=… <bench binary> --exact bench_tpch_sf40_q6_tp1_single --test-threads=1`
  → exit 0, no `ERR_NVGPUCTRPERM` in its output. Remove the local derived files (not committed).
- [ ] **Step 6:** `$N stop` → `STOPPED`. Commit any fixes.

### Task 11: `build-test.md`

**Files:** Modify `llm-wiki/build-test.md`: "Nebius host" (Task 1's retitle of "GPU host"): the
three VMs, the L40S (nebius-gpu), the H100 and the 128-core CPU VM, each with its instance id,
region, profile and role; `nebius.sh`'s flags and aliases, `install-watch --key-file`, the CPU
kind's rule, the install file, the three-leg watch and its notices, the secrets; `run-gpu.sh`'s
verbs, options and exit codes, `stage-install.sh`. "Datasets" (rows for TPC-H+V sf40 and TPC-DS
sf40 with the two new VMs as host columns; `tpcds-sf40` in the bucket column). "Containers"
(`fetch-sf40.sh`, `install-duckdb.sh` on the CPU VM). "The human's one-time setup": the eu-west1
key pair uploaded in the web console,
`nebius profile create eu-west1 --endpoint api.nebius.cloud --service-account-id <sa> --public-key-id <key> --private-key-file-path <pem> --parent-id <project>`,
and `gh secret set NEBIUS_EU_WEST1_SA_ID --env nebius-gpu` (and the other three, values on stdin).

- [ ] **Step 1:** write it, short sentences, every value from the code (`grep` each flag and path
  named against the scripts). **Step 2:** every test file under `scripts/nebius/tests/` and
  `scripts/env/tests/` (no image needed) → `0 failed`; `cargo test -p cost-report --test
  test_source_formatting` → PASS. `git grep -n -E 'gpu-host|scripts/env/gpu\.sh|host/nebius|host/ssh|config/peacock/hosts' -- . ':!llm-wiki/archive' ':!llm-wiki/reports' ':!llm-wiki/tasks'`
  → nothing. **Step 3: Commit**
  `new-hosts: build-test.md documents the three Nebius hosts, their guards, data and setup`.

### Task 12: the master commit (spec item 9), prepared

Once the task is `done`. The developer prepares it on a branch; landing it on master is the
coordinator's or the human's step, approved in advance. The developer never pushes master.

**Files:** on a worktree of `origin/master`, branch `ENS-nebius-guard`, its own `target/`:
- Task 1's renames and reference edits, replayed with Task 1's commands on master's tree
  (master has no `scripts/env/`, no `docker/run-tests.sh`; the same grep decides the files)
- Taken whole from `ENS-new-hosts`: `scripts/nebius.sh`, `scripts/nebius-sync.sh`,
  `scripts/nebius/` (watcher, checker, `watch.sh`, units, tests), `.github/workflows/nebius-watch.yml`,
  `.github/actions/nebius-setup/action.yml`
- `llm-wiki/build-test.md`: "Nebius host" as Task 11 wrote it, less the sentences naming what
  master lacks (`run-gpu.sh`, `stage-install.sh`, `fetch-sf40.sh`, the run image)

**Interfaces:**
- Consumes: Tasks 1–5 and 11 on `ENS-new-hosts`.
- Produces: `ENS-nebius-guard` = `origin/master` + one developer commit; its diff against master
  holds only the Files above.

- [ ] **Step 1:** `git worktree add <dir> -b ENS-nebius-guard origin/master`; Task 1 Steps 2–3
  there; `git checkout ENS-new-hosts -- <the whole-file paths>`; the "Nebius host" section.
- [ ] **Step 2: verify on master's tree.** `for f in scripts/nebius/tests/test_*.py; do python3 "$f" || echo "FAIL $f"; done`
  → no `FAIL`; `cargo test -p cost-report --test test_source_formatting` → PASS; actionlint over
  `nebius-watch.yml` and `pipeline.yml` → exit 0; Task 1 Step 3's grep → nothing;
  `git diff --stat origin/master` names only the Files above.
- [ ] **Step 3: Commit** (developer identity)
  `nebius: the Nebius VM scripts named for the provider; three VMs guarded, each by its watcher and the hourly watch`;
  `git push -u origin ENS-nebius-guard`; record the hash in the detail file for the landing.
