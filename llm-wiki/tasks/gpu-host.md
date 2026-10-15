# The GPU host is a VM the tools start, use and stop

Kind: production

shad-gpu will be down for an extended time. The device tier moves to nebius-gpu, an L40S VM
that costs money while it runs. Today it is started and stopped by hand
(`dev:~/stop-nebius-gpu.sh`), its IP changes on every start, and chain J stalled on a changed host
key. This task makes the VM something the ensemble and CI bring up for a GPU step and take
down after it. Two independent guards stop it when nobody does.

Implemented in a helper session with subagents, on a branch with a PR. Not an ensemble task.

## The host

- Instance `computeinstance-e00tnrse7ayntnzcyt`, project `project-e00tdypyen10mmyep61c8q`. The
  boot disk `computedisk-e00h63n47zhs9h6gcw` survives a stop. NVIDIA L40S, 46 GB; Ubuntu 24.04,
  glibc 2.39; cuDF 25.02 in `~/data/miniforge3/envs/rapids-cuda-12.2`.
- The IP is ephemeral. The VM is preemptible: it can stop, change IP or vanish mid-step.
- Only what fits an L40S runs there. The sf40 binaries (`peacock_tpch_tests`,
  `peacock_tpchv_tests`), `--run-benchmarks` and Nsight captures are out until further notice.

## The work

### 1. `scripts/gpu-host.sh` — the one door to the VM

Replaces `dev:~/stop-nebius-gpu.sh`. The instance ID is a constant in the script. It uses the
`nebius` CLI with whatever profile the caller has.

- `status`: prints the state, and the IP when the instance is RUNNING.
- `start`: starts the instance, waits for RUNNING, then for ssh. Idempotent: on a running
  instance it only waits for ssh.
- `stop [--force]`: as the old script. A soft stop refuses while a process is on the card
  (exit 2); an unreachable host is stopped on the API's word. `--force` stops regardless.
- `ssh <cmd…>`: runs a command on the VM. `ip` prints the current IP.
- `install-watch`: installs the idle watcher (section 4) and its credentials on the VM.

Exit codes: 0 done; 2 card busy (soft stop); 75 no capacity or quota, so retry later; 69 the
instance no longer exists; 1 anything else.

**No cached IP.** Every call resolves the IP through the API. No ssh config entry is edited.
**Host keys:** every ssh uses `HostKeyAlias=nebius-gpu` with its own known-hosts file, so a new
IP never warns. When the key itself changes, the script checks through the API that the IP
belongs to this instance, replaces the stored key, and logs both fingerprints. That check is the
trust anchor, so no agent ever has to accept a key by hand.

### 2. `scripts/gpu-host-sync.sh` — the tree to the VM

Chain J's rsync, scripted: the working tree, uncommitted, to `~/peacockdb-<name>` on the VM.
`--delete-after`, `--exclude=.git`, `--filter=':- .gitignore'`, so ignored build dirs, target
dirs and generated data survive on both sides. Retries as `resilient_rsync` does. It generates
the sf1 tpch and tpcds data on the VM if it is missing.

### 3. `scripts/gpu-host-test.sh` — the L40S test list, detached

Runs on the VM, against a tree that holds `cpp/install/`. The list is chain J's: every
`peacock_*_tests` except the sf40 pair, and the staged rust binaries with `--test-threads=1`,
`peacockdb_core_gpu_lib` filtered to `gpu_tests::`, and `peacock_gpu_benchmarks --skip bench_`.
It runs detached under `nohup`, with a log and an rc file per binary, so a dropped ssh does not
kill it. `--status` reports which binaries are done, running or failed. It keeps the existing zero-tests
guards. The developer and CI both use it, so there is one list.

### 4. The idle watcher, on the VM

A systemd timer, enabled, firing every minute. It survives reboots. Each tick appends one line to
`/var/log/peacock-gpu-idle.log`: time, boot time, the card's processes with their start times,
build processes seen, and the decision. State lives in `/var/lib/peacock-gpu-idle/`.

- **Test activity:** any process on the card (`nvidia-smi`), or a running test binary from
  section 3's list.
- **Build activity:** `rsync`, `cargo`, `cmake`, `ninja`, `nvcc` or `cc1plus` running.
- **Idle stop:** no test activity for 1 hour, or 2 hours if build activity was seen in that
  time.
- **Hung-run cap:** the same process on the card for 4 hours.
- The clocks start at boot at the earliest, so a fresh start is never stopped on old state.
- It stops the instance through the API with `nebius compute instance stop`. It does not shut
  down the guest. The credentials are the same service account's, installed root-only by
  `install-watch`.

### 5. `.github/workflows/gpu-host-watch.yml` — the hourly checker

Scheduled hourly (`0 * * * *`) and dispatchable. It keeps no state. If the instance is RUNNING,
it reads the watcher's log over ssh and **hard-stops** on any of these:

- the log's last line is more than an hour old (the watcher is dead);
- the log shows the idle rule or the hung-run cap met;
- ssh does not answer for a 15-minute retry window.

### 6. The GPU job in `pipeline.yml`

- Triggers: `workflow_dispatch` is added. `gpu-tests` runs on a push to master and on a
  dispatch, never on `pull_request`. A PR still runs the build job.
- **Who can run it.** A dispatch needs write access. The job's first step fails unless
  `github.actor` is in the repo variable `GPU_ALLOWED_ACTORS`. The secrets sit in a GitHub
  Environment `nebius-gpu`, which only the GPU jobs reference. A fork PR gets no secrets and
  no GPU job.
- **Secrets**, placeholders until the human fills them: `NEBIUS_SA_ID`, `NEBIUS_PUBLIC_KEY_ID`,
  `NEBIUS_PROJECT_ID`, `NEBIUS_PRIVATE_KEY`, and `NEBIUS_GPU_SSH_KEY` for the VM login.
- **Steps:** install the nebius CLI; `gpu-host.sh start`, retried for up to 30 minutes, then
  fail; ship `cpp/install/` and the testdata the job ships today to a per-run directory; run
  `gpu-host-test.sh` there; always remove the run directory and soft-stop.
- The concurrency group becomes `nebius-gpu`. The glibc patch step and the sf40 precondition
  go: CI's binaries need glibc 2.35 and the VM has 2.39.

### 7. The ensemble's lifecycle

- **Every GPU step names its bring-up and its stop.** A spec or `-impl.md` step that builds for
  or runs on the device says: `gpu-host.sh start` → `gpu-host-sync.sh` → build on the VM →
  `gpu-host-test.sh` (or a named subset) → soft stop. The developer runs it.
- **A lost host mid-step** (ssh fails, the VM is preempted or moves): `status`, `start` again
  under the retry rule below, sync, rerun the step. Exit 69 (the instance is gone) is a stall:
  rebuilding the disk, the env and the data is the human's.
- **Start failures:** on exit 75 the coordinator retries every 15 minutes for 10 hours, then
  writes `stalled: gpu host unavailable` and exits.
- **Hard stops by the coordinator:** on entering `completeness approved`, on entering `done`, and on
  every exit (stalled, tight window, `stop`). Everyone else stops softly.
- **The final run is CI's.** In `completeness approved`, after its hard stop, the coordinator
  dispatches `gh workflow run pipeline.yml --ref <branch>` and waits for that run. `done` needs
  its `gpu-tests` green as well as the PR's CI. The gh account the coordinator runs as must be
  in `GPU_ALLOWED_ACTORS`.
- **No agent ever opens a Nebius credential file.** That means `~/.nebius/config.yaml`,
  `~/.nebius/credentials.yaml` and any `*.pem` under `~/.nebius/`, on any host, including the
  VM's copy. No `cat`, `grep`, `sed`, Read, or redaction attempt either: a redaction leaked the
  private key into a transcript on 2026-10-14. Agents use the CLI through `gpu-host.sh` only. The
  scripts never print these files and never echo the CLI's config. The rule goes into
  `prompts.md`'s Shared rules, so every role carries it.
- **Contention is accepted.** Two chains and CI share one VM. One party's stop can kill another's
  run, and the loser reruns its step.

## Scope

| path | change |
|---|---|
| `scripts/gpu-host.sh`, `scripts/gpu-host-sync.sh`, `scripts/gpu-host-test.sh` | new |
| `scripts/gpu-host/` | the watcher script, its systemd service and timer units |
| `scripts/tests/` (or beside the scripts) | the fake-`nebius` and watcher-rule tests |
| `.github/workflows/pipeline.yml` | `workflow_dispatch`; `gpu-tests` moved to nebius-gpu |
| `.github/workflows/gpu-host-watch.yml` | new |
| `llm-wiki/prompts.md` | Coordinator and Developer: section 7; the verda/shad-gpu GPU wording |
| `.claude/commands/ensemble.md` | wherever it routes GPU work |
| `llm-wiki/build-test.md` | a "GPU host" section in place of shad-gpu routing; Remote hosts; CI structure |
| `llm-wiki/tasks/tasks.md` | chains J and K's host overrides shrink to a pointer to that section |

Component-level API: none. No facade, trait, ABI symbol or wire change.

## Restriction

The engine is untouched. `scripts/build-test-shadgpu.sh` stays as it is. Its `--build` is what
runs on the VM, and its shad-gpu `--run` and benchmark paths are left alone. Recreating a
terminated instance, cuDF 26.02 on the VM, sf40 data and benchmarks are out. No lock between
parties sharing the VM.

## Tests

- `shellcheck` clean on every new script.
- `gpu-host.sh` against a fake `nebius` on `PATH` and a fake `ssh`: start, stop, soft stop
  refused while busy, `--force`, status with and without an IP, exit 75 on a capacity error,
  exit 69 on a missing instance, the host-key replacement when the API vouches for the IP, and a
  refusal when it does not.
- The watcher's decision as a function of a fake process list and clock: idle 1 h, build 2 h,
  the 4 h cap, and a fresh boot over old state not stopping.
- The checker's decision over sample logs: stale log, idle, cap, unreachable.
- The actor check: an actor not in the list fails the step (run with a fake `github.actor`).

## Verification bar

By hand, once, recorded in the PR:

- a real `start`, `status`, `ssh`, soft and hard `stop`;
- `install-watch`, a reboot, and the timer still firing afterwards;
- the watcher's API stop leaving the instance STOPPED (shortened thresholds);
- the checker's decision script run by hand against the running, idle VM, stopping it;
- `pipeline.yml` dispatched on this PR's branch: `gpu-tests` green on nebius-gpu, the VM
  stopped afterwards;
- a dispatch by an actor not in `GPU_ALLOWED_ACTORS` failing at the check step.

GitHub reads a scheduled workflow only from master, and only dispatches a workflow whose file
master already has. So `gpu-host-watch.yml` is verified after the merge: one dispatch, and one
scheduled run seen in the Actions log. If `pipeline.yml` cannot be dispatched on the PR branch
before master has the trigger, a one-line commit adding `workflow_dispatch` lands on master
first. No job would run on the dispatch until this branch's `gpu-tests` exists.
