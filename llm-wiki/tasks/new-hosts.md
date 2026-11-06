# Two more Nebius VMs behind nebius.sh, each with its idle guards, data and run environment

Kind: production

The human created two VMs for this chain. Both are up only while a run needs them.

| name (this chain's) | instance | region | profile | role |
|---|---|---|---|---|
| `nebius-h100` | `computeinstance-e00e6yena4nzjgaym0` | eu-north1 | `editor-eu`, the existing eu-north1 profile | H100 80GB HBM3, 16 vCPU, 196 GB; runs peacockdb benchmarks |
| `nebius-cpu128` | `computeinstance-e01t2mm4265y8x88ey` | eu-west1 | `eu-west1`, on a new service account (created by the human) | 128 cores; runs DuckDB |

Today `scripts/gpu-host.sh`, its idle watcher and `gpu-host-watch.yml` serve one VM,
`computeinstance-e00tnrse7ayntnzcyt` (nebius-gpu, L40S), whose id is hard-coded in the script and
in `scripts/gpu-host/idle_watch.py`. This task names them for the provider (item 10) and makes
them serve three. Below, every name is the new one.

## Done when

1. **`nebius.sh` takes `--instance-id <id>` and `--profile <name>`.** With neither, it behaves
   exactly as today: the existing `INSTANCE_ID` and the caller's active profile. Every verb works
   on any of the three VMs. Each instance gets its own ssh `HostKeyAlias` and known-hosts entry, so
   one VM's reprovisioned key never collides with another's. The login is `dmitry` on all three,
   with the same key (`NEBIUS_GPU_SSH_KEY` in CI). The exit-code table is unchanged.
2. **An on-VM idle watcher on both new VMs** stops the VM after one hour unused, as nebius-gpu's
   does. `idle_watch.py` takes its instance id from its install and not from a constant. On the CPU
   VM, a running DuckDB process, a data fetch, or an interactive ssh session counts as use; there is
   no card there. The 4-hour cap on one process stays on the GPU VMs. On the CPU VM, it applies
   to DuckDB too, unless the builder shows a full timing run needs longer.
3. **`install-watch` installs only the credentials that VM needs.** Today it copies the whole
   `~/.nebius/config.yaml`, which carries every profile and no key file. The eu-west1 profile keeps
   its key in a separate PEM (`--private-key-file-path`). For each VM, install-watch installs one
   profile, self-contained (key embedded, as `.github/actions/nebius-setup` does with
   `--private-key-file`), as root's only profile. The PEM is copied, never read or printed. The
   repository rule against opening a Nebius credential file holds throughout. The final check,
   that root's profile reaches the API, runs on both VMs.
4. **`nebius-watch.yml` checks all three VMs every hour.** Each VM is a separate step or matrix
   leg. It is skipped with a notice, not failed, when:
   - its secrets are absent;
   - the instance no longer exists (`nebius.sh` exit 69);
   - its region's profile cannot be created.

   The human may delete either new VM later, and the workflow must stay green when that happens.
   One VM's failure does not stop another's check. The eu-west1 credentials come from
   `NEBIUS_EU_WEST1_{SA_ID,PUBLIC_KEY_ID,PROJECT_ID,PRIVATE_KEY}` in the `nebius-gpu` environment.
   They are set. The H100 uses the existing `NEBIUS_*` secrets. `gpu-tests` stays on
   nebius-gpu only, and no other workflow is added.
5. **Data on the VMs:** TPC-H sf40 from the `tpch-sf40` bucket and TPC-DS sf40 from the
   `tpcds-sf40` bucket, on both VMs. Each object is checked, and a second run fetches nothing it
   already holds. The buckets are in eu-north1, so the CPU VM fetches across regions. That is
   expected; the endpoint and region come from the env file, as containers' bucket scripts do. The
   CPU VM also gets DuckDB 1.5.4 (`docker/pins.env`) and the native `.duckdb` databases that
   `benchmarks/duckdb_minimal.sh` builds.
6. **The H100 runs the device tiers and the benchmark in containers' run image.** That means
   containers' unfinished device runner (task 7 of
   `origin/ENS-containers:llm-wiki/tasks/containers-impl.md`), as
   `scripts/env/run-gpu.sh prepare|ship|run|pull`, with `nebius-h100` as its first VM. By the
   human's decision there are no host adapters: no `scripts/env/host/`, no host files under
   `~/.config/peacock/hosts/`. `run-gpu.sh` takes `nebius.sh`'s `--instance-id` and `--profile`
   and calls `scripts/nebius.sh` directly for start, stop, ssh and rsh. The sf1 GPU corpus
   passes there, as the first proof the environment works. `create_nsys_profile.sh` runs there
   too: nsys is present, and the GPU's performance counters are open to non-root
   (`NVreg_RestrictProfilingToAdminUsers=0`), so the metrics pass does not stop at
   `ERR_NVGPUCTRPERM`.
7. **Containers' fresh-VM check, on `nebius-cpu128`** (containers' tasks 6 and 6a, as
   containers-rebase hands them over), before the DuckDB setup:
   - `provision-vm.sh dmitry@<its address>` from the laptop, which runs `bootstrap-vm.sh`;
   - the machine ends ready: Docker, gh and Claude Code at the pinned versions, the repository
     and sf1 (no machine marker: containers-rebase dropped it with the launcher);
   - a second run skips everything;
   - 6a's fake-`ssh` tests exist and pass;
   - the GPU-host key arrives: `provision-vm.sh` with its default `--gpu-key`, `~/.config/peacock/gpu_ed25519`, a copy of dev's `~/.ssh/id_ed25519` (which
     nebius-gpu and the H100 already accept), and the VM's `peacock.env` names `GPU_SSH_KEY_FILE`.
     `ssh -i "$GPU_SSH_KEY_FILE" dmitry@<H100's address> true` from the CPU VM succeeds while the
     H100 runs.

   The agent-container launcher is gone (containers-rebase). Anything the run shows broken in
   the two scripts is fixed here.
8. **`build-test.md` documents it all:**
   - the section "GPU host", retitled "Nebius host": the L40S (nebius-gpu), the H100 and the
     128-core CPU VM, their profiles, watchers and secrets, and `run-gpu.sh`;
   - the `tpcds-sf40` bucket in the dataset table (it exists and is not documented);
   - the human's one-time setup: the eu-west1 key pair uploaded in the web console, the
     `nebius profile create` line, and the `gh secret set` lines.

9. **The guarding reaches master during this task.** GitHub runs and dispatches `nebius-watch.yml`
   from master's copy only, and this chain sits on an unmerged base. So the VM-guarding part goes
   to master as one small commit, `ENS-nebius-guard`, approved in advance:
   - item 10's rename, on master's files;
   - `nebius.sh`'s instance and profile selection;
   - the idle watcher, its install, and its tests;
   - the workflow and the setup action.

   That commit is prepared as soon as the guards' offline tests pass, and the human lands it on
   master mid-task; the guards' tests must pass on master. Item 4's live check of all three VMs
   is then dispatched from master, before `done`; it never holds the task back from review.
   Landing is read from master's content (the workflow is there and matches), not from a hash,
   since a landing may squash or rebase. No VM is kept up across that wait: the check runs with
   the VMs stopped, and one VM started just for it. Nobody in the chain pushes master. The rest
   of the task stays on the chain.
10. **Every file and directory named `gpu-host` is named `nebius`:**
    - `scripts/gpu-host.sh` → `scripts/nebius.sh`, `scripts/gpu-host-sync.sh` →
      `scripts/nebius-sync.sh`, `scripts/gpu-host-test.sh` → `scripts/nebius-test.sh`;
    - `scripts/gpu-host/` → `scripts/nebius/`, its tests `test_gpu_host.py` → `test_nebius.py`
      and `test_gpu_host_test.py` → `test_nebius_test.py`;
    - `.github/workflows/gpu-host-watch.yml` → `.github/workflows/nebius-watch.yml`;
    - the state directory `~/.config/peacock/gpu-host/` → `~/.config/peacock/nebius/`, its log
      `nebius.log`.

    Every reference follows: `pipeline.yml`'s `gpu-tests` and script-test steps, the CI wiring
    guard in `cost-report/tests/test_source_formatting/runners.rs`, `build-test.md`, `prompts.md`,
    `.claude/commands/ensemble.md`, `.gitignore`, the tickets, and every later chain L plan. The
    watcher's own names on the VMs stay: `peacock-gpu-idle.service`, `.timer` and its log. Records
    (`llm-wiki/archive/`, `llm-wiki/reports/`, other chains' task files) keep the old names. So
    does `docker/`: its files are hashed into the published images' labels, and an edit there
    would make every image refuse the tree. A stale name there is recorded in the detail file.
11. **The standing rules name the three hosts and their roles.** `prompts.md` (the developer's
    GPU step, the coordinator's host and hard-stop rules), `.claude/commands/ensemble.md` and
    `build-test.md` say, for every chain from now on:
    - the L40S (nebius-gpu) is for correctness tests, the L40S list; no sf40 and no benchmarks
      there;
    - the H100 (nebius-h100) runs sf40, the benchmarks and Nsight, through `run-gpu.sh`;
    - the 128-core CPU VM (nebius-cpu128) runs DuckDB and stages the H100's install: binaries
      for H100 runs are built on the CPU VM; local and verda builds stay for development. The
      recipe, "The CPU-VM build", is in `build-test.md`, "Nebius host";
    - the coordinator pre-starts no VM: a VM starts right before its first remote command and
      stops after its last pull (the VM-time rule); its one start is the exit-75 retry, once a
      developer's build is done;
    - the coordinator's hard stops cover all three VMs.
      On every exit trigger the coordinator hard-stops all three (`stop --force`), whichever chain
      started them. The human accepts the risk that this cuts another chain's run short (decided
      2026-11-03). That run resumes with `PEACOCK_BENCH_RESUME=1`, and the standing rule says so.

    The rules cite `build-test.md`, never a task plan (plans are deleted at archive). A
    coordinator holds `prompts.md` and `ensemble.md` from its startup, so right after the
    rename (item 10) and these rules land on the chain branch it re-reads `prompts.md`,
    `.claude/commands/ensemble.md` and `build-test.md` in place and notes it in the detail file.
    It does not exit or restart: a restart with the board unchanged spends the watchdog's idle
    allowance. Until the rename lands it uses the old names. The exit-75 retry is the
    coordinator's: it retries the VM's `start` itself, never a developer dispatch per try.

    `build-test.md`'s "Out until further notice: the sf40 binaries, `--run-benchmarks`, Nsight"
    goes. Later chain L tasks rely on these rules and carry no host override.

## Scope

| path | change |
|---|---|
| `scripts/gpu-host*` → `scripts/nebius*`, `gpu-host-watch.yml` → `nebius-watch.yml`, and every reference | the rename (item 10) |
| `scripts/nebius.sh`, `scripts/nebius-sync.sh`, `scripts/nebius-test.sh` | instance and profile selection, per-instance host-key alias, credential install |
| `scripts/nebius/idle_watch.py`, `check.py`, `*.service`, `*.timer`, `tests/` | instance id from the install; the CPU-VM activity rule |
| `.github/workflows/nebius-watch.yml`, `.github/actions/nebius-setup/` | three VMs, each skippable; a named profile per region |
| `scripts/env/` (containers' tree) | `run-gpu.sh` for `nebius-h100`, over `scripts/nebius.sh`; data fetch for the two new VMs; fixes `provision-vm.sh`/`bootstrap-vm.sh` need |
| `llm-wiki/build-test.md` | "Nebius host": hosts, their roles, data, secrets, setup steps |
| `llm-wiki/prompts.md`, `.claude/commands/ensemble.md` | the rename; the three hosts' roles and hard stops (item 11) |

Component-level API: none.

## Constraints

- Containers' "no change to the existing GPU host scripts" restriction was containers' own.
  This task changes them on purpose, and nebius-gpu's behaviour must not move: its tests in
  `scripts/nebius/tests/` still pass, unmodified after the rename's path edits, with new cases
  added beside them.
- Every ssh to the VMs from the Bash tool needs the sandbox off (`build-test.md`).
- A VM this task starts is stopped again by this task, or left to its watcher, which must
  already be installed.

## Signoff (2026-11-05)

Solved under its constraints: three VMs named, guarded and proven live (watch runs 41899844185,
41900251751); sf40 on both new VMs with agreeing pins; DuckDB and `tpch.sf40.duckdb` on the CPU VM;
the sf1 list, one sf40 benchmark case and Nsight on the H100 through `run-gpu.sh`; provision verified
twice on a fresh VM; host roles in the standing rules. The rename and guard reached master as
`bf937124`/`e20db7e9`. Shortcuts: the sf40 C++ pair (`peacock_tpch_tests`, `peacock_tpchv_tests`)
has no H100 path (`run-tests.sh` skips it; build-test.md says so); `scripts/env/tests` still runs by
hand only, not in CI; `create_nsys_profile.sh --metrics` uses nsys's deprecated
`--gpu-metrics-device` spelling, accepted on the H100.
