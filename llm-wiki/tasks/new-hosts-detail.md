# new-hosts — detail

## Run log

- 2026-11-04: `ENS-new-hosts` branched from `ENS-containers-rebase` at `687b61fe` (containers-rebase
  `done`, PR #193). Board `building`. verda unreachable (no `VERDA_CLIENT_ID`); CPU runs on dev.
- Order (plan): Tasks 1–5, 5a, 12 (master commit, the human lands it), 6–11. Until Task 1's
  commit the coordinator uses the old names (`scripts/gpu-host.sh`).

## VM time

| VM | start (UTC) | stop (UTC) | hours | why |
|---|---|---|---|---|
| nebius-gpu | 2026-11-04 04:51:06 | 04:55:26 | 0.07 | Task 6 Step 4: copy its `authorized_keys` |
| nebius-h100 | 2026-11-04 04:55:52 | 04:58:53 | 0.05 | Task 6 Step 2: install-watch (failed: wrong `--key-file`), checks, keys |
| nebius-cpu128 | 2026-11-04 05:00:07 | 05:03:02 | 0.05 | Task 6 Step 3: install-watch, checks, keys |
| nebius-h100 | 2026-11-04 05:04:14 | 05:07:13 | 0.05 | Task 6 Step 2 again: install-watch without `--key-file` |
| nebius-cpu128 | 2026-11-04 18:58:00 | 19:10:11 | 0.20 | Task 7 Steps 4–6: provision, its checks, the rerun, the key checks |
| nebius-h100 | 2026-11-04 19:07:41 | 19:09:38 | 0.03 | Task 7 Step 6: the CPU VM's ssh to it |
| nebius-cpu128 | 2026-11-04 19:13:00 | 19:14:57 | 0.03 | Task 8 Step 5: the disk check (74 G free; stalled) |

Hours so far: nebius-gpu 0.07, nebius-h100 0.13, nebius-cpu128 0.28. Dev's local time is UTC.

## Task 1: the rename

Developer, 2026-11-04, on dev. VM time: none.

**Kept old names** (each deliberate; Step 3's grep is not empty because of the second group):
- `docker/run-tests.sh:7`, "by scripts/gpu-host-test.sh's rules". An image input: an edit changes
  the run images' `peacockdb.inputs` label.
- `host=gpu-host` / `host: "gpu-host"` / `gpu-host-1`: fixture hostnames of a hypothetical H100/H200
  box in `Provenance` and benchmark-record tests, not references to the scripts:
  `peacockdb-core/src/test_support/{benchmark_trailer,record}/tests.rs`,
  `peacockdb-core/tests/peacock_gpu_benchmarks.rs`,
  `peacockdb-core/tests/test_corpus_goldens/{benchmark,benchmark_run}.rs`,
  `scripts/calibration/tests/test_split.py:32`, and the record example at `build-test.md` (`host=gpu-host`
  under "A failed case, and a rerun").
- Records: `llm-wiki/archive/archived-tasks.md` (17), `containers-rebase-{detail,impl}.md`, this
  task's own spec and plan.
- `llm-wiki/tasks/tasks.md:12-16`, chain J's host override. The board, outside the developer's
  writes. Task 12 Step 4 flags it to the human for the master landing.
- On the VMs: `peacock-gpu-idle.service`, `.timer`, `/var/log/peacock-gpu-idle.log`. Env names:
  `GPU_HOST_STATE_DIR`, `GPU_HOST_USER`, `GPU_HOST_KEY`, `GPU_HOST_RSYNC`, and the rest of `GPU_HOST_*`.
- "GPU host" where it names the machine kind (code comments, `build-test.md` prose).

**What the sed missed.** `nebius.sh`'s `install-watch` shipped `"$here/gpu-host/idle_watch.py"` and the
two unit files, which matches no Step 2 pattern. Every suite stayed green, because the fake rsync only
records. `scripts/nebius/tests/test_install_watch_sources.py` now fails on a missing source, and it
did fail before the fix. Also by hand: the `*GPU host*` section references in `build-test.md`, the CI
step "Nebius script tests", the workflow name "Nebius watch", and three `nebius:` message prefixes.

**Later chain L plans.** Only `containers-only-impl.md` named the old scripts (lines 462, 624, 643). The
sed doubled two of them ("Nebius host"/"Nebius host", `nebius-test\|nebius-test`), and those were
deduplicated by hand. No spec matched.

**Not the rename's.** actionlint (`rhysd/actionlint:latest`, shellcheck 0.11.0) reports SC2001 at
`pipeline.yml:52` and SC2086 at `:356`, the same on HEAD `06ec7e0f`. So Step 5's "exit 0" holds for
`nebius-watch.yml` alone and with `-shellcheck=`. `scripts/env/tests/test_run_tests.py`
pulls the run images implicitly through `docker run`, and on a cold dev its 120 s `timeout` failed once
mid-pull. 32 reruns then passed.
- Task 1 committed `0651a08a`. Standing rules re-read after Task 1 (rename only: `scripts/nebius.sh`, "Nebius host", `~/.config/peacock/nebius/nebius.log`); next: Task 2 Step 1.

## Task 2: `nebius.sh` picks its instance and profile

Developer, 2026-11-04, on dev, offline. VM time: none.

- **The fake models a CPU VM, not just the plan's "print `cpu_busy`".** With `no_gpu` the fake ssh
  answers an unguarded `nvidia-smi` with `command not found`, exit 127, and prints `cpu_busy` only
  to a probe that checks `command -v nvidia-smi` first. Printing `cpu_busy` for any `nvidia-smi`
  command would let today's probe pass the CPU tests. With the flags done and the old probe still
  in place, both CPU soft-stop tests failed for the Review Focus 2 reason: the log read "host
  unreachable over ssh; stopping on the API's state", and a DuckDB run was stopped (rc 0).
- **`nebius-sync.sh` now honours `GPU_HOST_RSYNC`**, as `nebius.sh` does. The plan's sync test
  assumes that hook. Without it, the real rsync ran against the fake ssh, failed with protocol
  error 12 and retried with 15 s backoff until the test's 60 s timeout.
- `nebius-sync.sh` now refuses an unknown option with 64 (`test_sync_unknown_option_is_64`). Before,
  it ignored one silently, so a typo in `--instance-id` would have synced to nebius-gpu. It does not
  check the option values itself: `nebius.sh` does, inside rsync's transport. So a malformed id
  goes through the five rsync retries (about 150 s) and then exits 1, with `nebius.sh`'s 64 message
  on stderr.
- `--profile` goes last on each `nebius` argv (`cli … --profile <p>`), so the existing tests'
  `c[:3]`/`c[:4]` prefixes still match. Offline only: the real CLI was not run to confirm that it
  takes the global flag after the subcommand. Task 6's live run is the first real check.
- Install-watch's final check on the VM still runs root's profile with no `--profile`. Task 4
  makes that root's only profile.
- `build-test.md` "Nebius host" got the minimum that keeps it true at this commit: the options,
  the per-instance alias, the CPU soft stop and the sync options. The full rewrite is Task 11's.
- `nebius compute instance get --help` lists `-p, --profile` among its global options, so `--profile` after the subcommand is accepted (coordinator, 2026-11-04).

## Task 3: the watcher's instance and kind come from its install

Developer, 2026-11-04, on dev, offline. VM time: none.

- **For Task 4.** The install file is `/etc/peacock-gpu-idle.json`, `{"instance_id": "<id>", "kind": "gpu"|"cpu"}`.
  The id must match `^computeinstance-[a-z0-9]+$`. Anything else (bad JSON, not an object, other kind)
  gives id `None`: no stop is called, the line's `error` holds `install …` and `stop: no instance id
  installed`, and the tick exits 1. No file is nebius-gpu, kind `gpu`. Until Task 4 writes the file,
  a watcher installed on the H100 or the CPU VM would stop **nebius-gpu**: `build-test.md` says to
  install on nebius-gpu only, and Task 4 must lift that line.
- **For Task 5/6.** Each log line gains `sessions` (floats), `instance` and `kind`. `check.py` passes
  `l.get("sessions", [])`, so an old watcher's line is judged as before; a mistyped `sessions` is
  "malformed", a stop. Task 6's live check reads `instance` from the last line.
- **Departure: `docker` matches only itself.** The plan's `CPU_BUILD_NAMES` puts `docker` beside the
  prefix-matched names. `dockerd` and `docker-proxy` start with `docker` and run all day once Task 7
  installs Docker, so every CPU VM would sit on the 2-hour limit forever. `EXACT_NAMES = ("docker",)`
  makes `matches()` compare that one by whole name; the GPU lists hold no such name, so nebius-gpu
  is unchanged. `test_cpu_docker_daemons_are_not_a_build` fails with prefix matching (mutation run:
  the decision went to `keep`, 5400 s of 7200 s).
- **Sessions** are the `st_atime` of numbered entries under `/dev/pts` (`ptmx` skipped). The tty
  driver moves atime on input, so a forgotten tmux pane stops counting after an hour of no typing.
  A session counts only when `boot <= atime <= now`, and it has no cap. A `gpu` VM reads no
  terminals (`sessions == []`).
- `idle_watch.INSTANCE_ID` is now `DEFAULT_INSTANCE_ID`; nothing else referenced it.
- The unit files keep their names; only their `Description=` lines lost "nebius-gpu".
- The existing `test_idle_watch.py` does not patch `INSTALL`. Run on a host that holds
  `/etc/peacock-gpu-idle.json` with kind `cpu` (one of the new VMs after Task 6), its card tests
  would fail. They run on dev and CI runners only, so this is no problem today.
- Stale, not fixed here: the `build-test.md` test table's "Nebius scripts (Python)" row says N 104.
  The files held 116 tests before this task and 134 now. Task 11 can set it.

## Task 4: `install-watch` installs one profile and the install file

Developer, 2026-11-04, on dev, offline. VM time: none.

**Step 1, the CLI's profile commands (nebius 0.12.289).** Learned in `/dev/shm` with a scratch
`HOME`, `-c <scratch>/t.yaml`, throwaway ed25519 keys and dead endpoints; the dir is deleted.
- `profile list` ignores `--format` (json and yaml print the same). Stdout is one name per line,
  the active one suffixed ` [default]`: `t1`, `t2`, `t3 [default]`. `profile active` prints the
  name. Both print nothing else.
- `profile delete` of the active profile is refused (`cannot delete default profile`, rc 3). So
  install-watch activates first, then deletes: the plan's order the other way round.
- `profile update <p> --private-key-file <pem>` alone fails: it wants `--service-account-id`, and
  it prompts for the endpoint and the public key id when they are not given. With no tty the
  prompt reads EOF (`Error: ^D`, rc 2). So install-watch reads `endpoint`, `service-account-id`
  and `public-key-id` with `config get` (ids, never the key) and passes all three. `parent-id` and
  `tenant-id` survive an update untouched. The update also activates the profile.
- After that update the profile holds both `private-key` and the old `private-key-file-path`. So
  install-watch also runs `config unset private-key-file-path` on the copy. `config get` prints
  an empty line for an unset key; `-p <name>` reads another profile.
- A missing config is rc 4 (`missing configuration`).
- Two early `profile update` tries prompted against `api.nebius.cloud` before the dead endpoint
  was in place. They had fake ids and `--skip-auth`, and failed at the prompt.

**What install-watch does now.** On dev: the temp copy, the one profile (embedded and path-free
with `--key-file`), activated, the rest deleted, the copy alone rsynced. On the VM, in one ssh:
the install file first, then the config to `/root/.nebius/config.yaml`, then the timer. The VM
copy in `~/peacock-gpu-idle/` is removed by a trap, so a failed install does not leave it there.
The dev temp dir is removed by an EXIT trap. It prints `kind=<k>`.

**Departures from the plan.**
- `nebius.sh` reads the source config from `GPU_HOST_NEBIUS_CONFIG` (default
  `~/.nebius/config.yaml`), like the other `GPU_HOST_*` hooks. `World.env()` points it at an empty
  file in the world. That keeps the two install-watch tests in `test_nebius.py` and
  `test_install_watch_sources.py` unmodified: their `HOME` has no `.nebius/`, and the copy needs a
  file. `test_install_watch.py` drops the hook, so it runs against the real default path.
- `profile update` gets `--skip-auth`, so nothing on dev calls the API. The VM's final check
  proves the credentials.
- A profile with a key path and no `--key-file` is refused before any VM work. Shipped, it
  would name a PEM that exists only on dev.
- Four tests beyond the plan's six: the temp dir goes on failure, the key-path refusal, an
  unexpected `profile list` shape (exit 1, nothing echoed), and an unknown option (64).
- `fakes.py` models `profile active|list|activate|delete|update` and `config get|unset` the way
  the CLI does above. New world keys: `profiles`, `active_profile`, `profile_list`,
  `profile_config`. The defaults change nothing that existed.

**Mutations, each red:** no EXIT trap (3 tests); the install file written after `enable --now`
(`…writes_the_instance_and_kind`); no `config unset` (`…profile_needs_no_pem_on_the_vm`); delete
before activate (`…keeps_one_profile`).

**For Task 6.** The final check may leave a token cache beside `/root/.nebius/config.yaml`. Step 3's
"`config.yaml` alone" means no `.pem`. Whether the real CLI writes a cache there is unknown.

## Task 5: the hourly watch over three VMs

Developer, 2026-11-04, on dev, offline. VM time: none. No workflow dispatched.

- **`scripts/nebius/watch.sh <vm> [nebius.sh options…]`** is the old check step's body, moved.
  New: `status` rc 69 is `::notice::<vm>: instance no longer exists; skipped`, exit 0; the window
  and pause come from `GPU_WATCH_WINDOW` (900) and `GPU_WATCH_PAUSE` (60); every message names the
  VM; a failed hard stop prints `::error::<vm>: hard stop failed (rc=<n>)` and exits 1, whatever
  `nebius.sh`'s code. No VM name is exit 64. An empty `CLOCK_OFFSET` is 0, as an unset one.
  The ssh-window reason reads `ssh unanswered for <window>s` (was "for 15 minutes").
- **The workflow.** One job, `strategy.fail-fast: false`, three `include` legs. Steps: `secrets`
  (the leg's four by `secrets[format('{0}_…', matrix.prefix)]` and `NEBIUS_GPU_SSH_KEY`, all in
  `env:`; output `present`; a notice naming the absent ones) → `setup` (`if: present`,
  `continue-on-error: true`, `profile: ${{ matrix.profile }}`) → a notice step on
  `steps.setup.outcome == 'failure'` → `check` on `== 'success'`. Matrix values reach every
  `run:` through `env:`; no `run:` holds a `${{ }}`. The job's display name is `watch (<vm>)`.
- **Departure: secrets in `with:` too.** The plan's `test_workflow_secrets_reach_only_env` says
  "every `secrets[` sits under an `env:` key". The setup step must take them as action inputs, and
  a step output cannot carry a secret (GitHub drops it). So the test allows `env:` or `with:` and
  forbids any `${{` or `secrets` inside a `run:` block.
- **Beyond the plan.** `test_watch.py` adds `is_executable`, `status_failure_is_1`,
  `no_log_hard_stops`, `clock_offset_shifts_the_checker`, `reason_is_one_line`,
  `failed_stop_is_1`, `without_a_vm_name_is_64`. `test_watch_workflow.py` adds
  `profile_failure_is_a_notice`, `action_profile_input_defaults_to_ci`, and runs two steps for
  real: the `secrets` body with each secret blanked (notice, `present=false`), and the `check`
  body against a stub `watch.sh` for each leg's argv (the gpu leg has no `--instance-id`).
- **Mutations, each red:** the newline strip removed (`reason_is_one_line`); the rc 69 branch off
  (`gone_instance_is_a_notice`); `fail-fast: true` and `continue-on-error` deleted
  (`legs_are_independent`); the SSH-key check dropped (`secrets_step_absent_is_a_notice`).
- **`pipeline.yml`: `watch.sh` added to the "Nebius script tests" shellcheck line.** Only that
  line; the gpu-tests nebius-setup call is unchanged (`test_gpu_tests_untouched`). Task 12 must
  carry that line to master with the rename, since pipeline.yml is not among its whole files.
- actionlint (pulled fresh, shellcheck 0.11.0): `nebius-watch.yml` no output, exit 0. Over
  `pipeline.yml` only the two pre-existing findings (Task 1 above). shellcheck over the six
  Nebius scripts, `watch.sh` included: exit 0. The image was removed after.
- **For Task 6 Step 5.** The live stopped-VM run should print `<vm>: STOPPED` then
  `not running; nothing to do` in each leg; the young CPU VM `up <n>s, under 10 minutes; keeping
  it until the next check`. The eu-west1 profile uses endpoint `api.nebius.cloud`, as `ci` does;
  that is unproven until the CPU leg's setup step runs live. If it fails, the leg is a notice,
  not red, so read the notice, not the run's colour.
- Stale, left for Task 11: `build-test.md`'s "Nebius scripts (Python)" row (N 104; 169 tests in
  13 files now, and it links neither `test_watch*.py`).
- Task 5a written by the coordinator (markdown): prompts.md (hosts, the exit-75 retry as the coordinator's one start, hard stops on all three VMs, the developer's brackets per VM, definition of done), ensemble.md (three hard stops), build-test.md (host-roles table; the sf40/benchmarks/Nsight line). `test_source_formatting` 32 passed (the coordinator ran it, a slip of its no-run rule; a guard over the wiki only). "The CPU-VM build" subsection is Task 11's. Standing rules re-read after Task 5a; next: Task 12.
- Task 12 Step 1: worktree `/home/dmitry/workspace/peacockdb-nebius-guard`, branch `ENS-nebius-guard` from origin/master `50a44df5`.
- Task 12 done: `ENS-nebius-guard` = origin/master `50a44df5` + `03360f3b`, pushed (37 files).
  Master's `pipeline.yml` and `nebius-test.sh` keep master's own wording; master's "Nebius scripts"
  row added (N 169, total 6008), and a short "Other VMs" paragraph in "Nebius host". actionlint's
  two `pipeline.yml` findings (SC2001 :52, SC2086 :356) are master's own. `fakes.py:27` cites
  `new-hosts-detail.md`, which master gains only at the chain's merge.
  **For the human:** please land `ENS-nebius-guard` on master (approved in advance). Note: chain
  J's host override in `tasks.md` names `scripts/gpu-host*.sh` and "GPU host", which the landing
  renames, so that override wants a one-line update then. After landing, run `install-watch` on
  nebius-gpu again (build-test.md: after any change under `scripts/nebius/` reaches master).
  Task 6 Step 5 (the live three-leg dispatch) waits on the landing, read by content.

## Task 6: watchers live on nebius-h100 and nebius-cpu128

Developer, 2026-11-04, Steps 1–4. VM time: four rows in `## VM time`, 0.22 h in all. All three VMs
STOPPED at 05:07:21 UTC. Step 5 is the coordinator's and waits on the guard's landing.

- **Step 1.** `ls -1 ~/.nebius/` (names only) lists `eu-west1-sa.pem` and `sa-key.pem`, with
  `config.yaml`, `credentials.yaml`, the two `*.pub.pem`, `eu-west-s3-key-id` and
  `eu-west-s3-secret-key`. `nebius profile list`: `dataowner-eu`, `editor-eu [default]`, `eu-west1`.
- **The plan's key file for editor-eu is wrong.** `config get private-key-file-path` is empty for
  editor-eu (and dataowner-eu) and `/home/dmitry/.nebius/eu-west1-sa.pem` for eu-west1. editor-eu is a
  service-account profile with its key already embedded. `install-watch --key-file ~/.nebius/sa-key.pem`
  replaced that key, and the VM's final check failed: `Signed JWT rejected: Invalid signature`
  (`InvalidArgument`, token-service `IncorrectJwt`). Reproduced on dev with a scratch HOME and a copy
  of the config: the untouched copy reached the API (rc 0), the copy with `sa-key.pem` embedded did
  not (rc 7, same message). `sa-key.pem` does not match editor-eu's `public-key-id`. **The H100 takes
  install-watch with no `--key-file`.** The eu-west1 copy with `eu-west1-sa.pem` embedded and the path
  unset did reach the API (rc 0) before the CPU VM went up.
- **A token cache hides a bad key.** The first dev reproduction shared one scratch HOME, and the
  bad-key copy passed: the CLI reused the token the good copy had cached. Only a fresh HOME showed
  the failure. So a re-install with a bad key over a good install could pass its final check on
  `/root/.nebius/credentials.yaml`'s cached token and fail later. Not fixed; unproven on a VM.
- **Fix: `nebius.sh install-watch` refuses `--key-file` for a profile with no key path** (exit 1,
  before any VM work: "profile <p> embeds its key; run install-watch without --key-file").
  `test_install_watch_refuses_a_key_file_for_a_profile_that_embeds_its_key` was red first (rc 0, went
  on to the VM). `test_install_watch_never_reads_the_key_file` now uses a key-path profile, the only
  shape `--key-file` is for. All 13 `scripts/nebius/tests/test_*.py`: 170 passed, 0 failed.
  shellcheck (koalaman/shellcheck:stable) rc 0. Live with the H100 stopped: the refusal, rc 1,
  nothing touched. `build-test.md` "The scripts" says it.
- **Step 4 (first).** nebius-gpu's `~/.ssh/authorized_keys`, 5 keys:
  `SHA256:JAxgKC1nLYM7cb8urk13iGpN6VjFZiDx6DC2w8uK/OM` dmitry@socrates (RSA),
  `SHA256:krZmL/esBoLRO3KfbsF5mgMODYvrIV19RlbUXW3/h8o` dmitry@computeinstance-e00tnrse7ayntnzcyt,
  `SHA256:2aXlKfnh+7h7M2NXk/UrO+NTIUNAd8D+cZk3mOJUBnw` (no comment),
  `SHA256:7JwIgk0OxHzXkPv+qvWWTtcjf16r5JJGrIM1eBSrxVQ` gh-nebius-gpu (the CI key),
  `SHA256:zBpsLs7UuJbn8C3NidcfcuvJcFCU9HUQdrZZwPv0kG8` rk00@rk00-M1010. **The H100 and the CPU VM held
  all five already; nothing was appended** (before and after lists identical on each).
- **Step 2, H100** (second start, no `--key-file`): `install-watch: profile editor-eu goes to the VM
  alone; 2 other(s) left out`, `kind=gpu`, `install-watch: watcher installed, root's profile reaches
  the API`. `/etc/peacock-gpu-idle.json`: `{"instance_id": "computeinstance-e00e6yena4nzjgaym0",
  "kind": "gpu"}`. Last log line after 70 s: `"instance": "computeinstance-e00e6yena4nzjgaym0",
  "kind": "gpu"`, `"card": []`, `"decision": "keep"`, `"reason": "idle 61s of 3600s"`, `"error": null`.
  Timer `active`.
- **Step 3, CPU VM**: `profile eu-west1 goes to the VM alone; 2 other(s) left out`, `kind=cpu`, final
  check passed first time. Install file `{"instance_id": "computeinstance-e01t2mm4265y8x88ey", "kind":
  "cpu"}`; no `nvidia-smi`. Last log line: `"instance": "computeinstance-e01t2mm4265y8x88ey", "kind":
  "cpu"`, `"card": []`, `"sessions": []`, `"decision": "keep"`, `"reason": "idle 60s of 3600s"`,
  `"error": null`. Timer `active`.
- **`sudo ls -1A /root/.nebius/`**, the same on both: `.last_check_update.yaml`, `config.yaml`,
  `credentials.yaml` (the CLI's token cache, Task 4's caveat). No `.pem`.
- Task 6 Steps 1–4 done (developer; see Task 6 note). The key-file fix carried to `ENS-nebius-guard` as `95e532bf` (same two files, byte-equal to the chain's; build-test.md sentence and count +1 on master's text). Next: Task 7. Task 6 Step 5 waits on the landing.

## Task 7: the fresh-VM check on nebius-cpu128

Developer, 2026-11-04, Steps 1–3 (offline). VM time: none.

- **Step 1.** `provision-vm.sh` skips Claude's login when stdin is not a terminal and exits 0. It
  prints on stderr `Claude Code is not logged in on <dev>: run ssh -t <dev> 'DISABLE_AUTOUPDATER=1 ~/.local/bin/claude auth login'`.
  **Departure from the plan's message** (`'claude auth login'`): a plain `ssh <host> <cmd>` has no
  `~/.local/bin` on its PATH on Ubuntu (it is added by `~/.profile`, login shells only), so the
  plan's command would fail with `claude: command not found`. The test runs the printed command
  through the fake ssh from a terminal and checks the login lands. Mutation with the plan's text:
  rc 127, `claude: command not found`. Mutation with the guard removed: rc 1, the bug itself.
- **The fakes now model the terminal.** The fake ssh gives `-t` a terminal only when its stdin is
  one, and otherwise prints ssh's "Pseudo-terminal will not be allocated…". The fake claude's login
  fails without one. `Laptop.provision(…, tty=True)` runs on a pty; `stdin=None` is `/dev/null`.
  Before the guard, 8 of 20 tests failed: the new one and every other run that reached the login
  without a terminal (rc 1). `test_provision_readies_a_vm` and `…_rerun_skips_what_is_done` now run
  from a terminal, which their login assertions always assumed.
- **A hazard in the fakes, fixed.** The fake VM's PATH ended in dev's own PATH, which holds dev's real
  `claude` in `~/.local/bin`. The first mutation run therefore started the real `claude auth login`
  three times (HOME a temp dir); the 120 s timeout killed only the fake ssh, and the three were
  left running until killed by hand. `laptop.vm_path()` now drops every PATH directory holding a
  `claude`.
- **Step 2.** `test_provision.py` 20 passed 0 failed; `test_bootstrap.py` 17 passed 0 failed.
  shellcheck (koalaman/shellcheck:stable) over both scripts: the same findings as HEAD's copies,
  none new; CI does not shellcheck `scripts/env/`. `build-test.md`: the Containers paragraph on
  the login, the Images row (157), the grand total.
- **Step 3, preconditions on dev.** `~/.config/peacock/github-pat`: **missing**. Step 4 with
  `</dev/null` would stop at "no token given", exit 2, before any ssh. The human must create it
  (`provision-vm.sh` asks from a terminal). GPU key and `.pub`: present; fingerprint
  `SHA256:2aXlKfnh+7h7M2NXk/UrO+NTIUNAd8D+cZk3mOJUBnw` (ED25519), equal to `~/.ssh/id_ed25519.pub`'s.
  The `.pub` is mode 644, the private key 600. `~/.config/peacock/peacock.env`: present, 600, the
  four storage keys only (`peacock_env_is_storage_only` and `…_has_storage` pass). Git identity
  set. Image inputs unchanged since the branch base `687b61fe`, so the build image's label check
  should pass.
- **Live-run risks, not shown by any fake:**
  - apt under sudo with no terminal and no `DEBIAN_FRONTEND=noninteractive` (sudo's env_reset
    would drop it anyway): a debconf question from `docker.io` or needrestart's prompt could
    block or fail. If seen: `sudo DEBIAN_FRONTEND=noninteractive NEEDRESTART_MODE=a apt-get …`.
  - Host key: provision uses plain ssh with `StrictHostKeyChecking=accept-new` into
    `~/.ssh/known_hosts`, which has no entry for 185.175.110.226 (the CPU VM's address at Task 6).
    The key is accepted unverified. Compare it with what `nebius.log` stored at 05:01:09
    (`ssh-ed25519 …LxeiLw+4VOM`). A recycled IP with a stale entry fails as "cannot reach".
  - Bootstrap's long ssh has no `ServerAliveInterval`; a silent phase (sf1 in docker) may drop it.
    A rerun resumes.
  - Bootstrap's stdin is at EOF: `claude plugin marketplace add` and `plugin install` at the pin,
    and the Claude installer, are unproven with no terminal.
  - Not exercised here because login = user = dmitry: `useradd`, the authorized_keys copy and the
    sudoers `install -m 440 /dev/stdin`. Still exercised: `groupadd --system docker` before
    `docker.io`'s postinst, `usermod -aG`, and `gh auth login --with-token --insecure-storage` from ssh's stdin.
  - Disk: the build image plus sf1 on the CPU VM's boot disk, size not checked.
  - Step 5 expects bootstrap to "report every step skipped". It prints no per-step report: a rerun
    is silent but for docker pull and cargo fetch, which run every time, and the closing text.
    Judge the skip by the absence of apt/curl/sudo work and by wall time.
  - With `</dev/null` Claude Code ends installed, not logged in: the human runs the printed command.

Developer, 2026-11-04, Steps 4–6 (live). Branch pushed at `01041c1f`; `~/.config/peacock/github-pat`
present (`test -s`, not printed). No code changed: the run showed nothing broken.

- **Host key, before provision.** The CPU VM came up at 66.201.6.39. Dev's `~/.ssh/known_hosts` had
  no entry for it. `ssh-keyscan -t ed25519` gave the same key `nebius.sh` recorded at Task 6
  (`…UKe8XLxeiLw+4VOM`, alias `nebius-computeinstance-e01t2mm4265y8x88ey`). So provision's
  `accept-new` took a key already checked. After the run, `~/.ssh/known_hosts` holds that key.
- **Step 4.** `scripts/env/provision-vm.sh dmitry@66.201.6.39 --name nebius-cpu128 --ref ENS-new-hosts </dev/null`:
  exit 0, wall 453 s (7.6 min), well inside the 2-hour rule. Log: `/tmp/t7/run1.log` (578 lines,
  dev only). None of the live-run risks above came true: apt installed `docker.io` with no prompt,
  the Claude installer and `plugin marketplace add`/`plugin install` worked from ssh's EOF stdin,
  and the long ssh did not drop. Last line on stderr:
  `Claude Code is not logged in on dmitry@66.201.6.39: run ssh -t dmitry@66.201.6.39 'DISABLE_AUTOUPDATER=1 ~/.local/bin/claude auth login'`.
  Checks over `$C ssh`, all as the plan wants:
  - `docker --version`: 29.1.3 (Ubuntu's `docker.io`).
  - `gh --version`: 2.102.1 = `GH_VERSION`, at `/usr/local/bin/gh`.
  - `~/.local/bin/claude --version`: 2.1.304 = `CLAUDE_CODE_VERSION`.
  - `git -C ~/peacockdb branch --show-current`: `ENS-new-hosts`, HEAD `01041c1f`.
  - sf1: `/var/lib/peacock/d608b4d0ede659a7/{tpch.sf1,tpcds.sf1}`, 8 and 24 files. Also
    `/var/lib/peacock/cudf/43505bb9…`, the cuDF source bootstrap fetches.
  - `test ! -e ~/.config/peacock/machine`: true.
  - Also: groups `dmitry docker peacock`; `gh auth status` logged in (account alephonea); git
    `user.name` set.
  - Disk after provision: `/dev/vda1` 96 G, 22 G used, 74 G free. **For Task 8:** sf40 (about 56 G)
    leaves about 18 G for DuckDB's native sf40 database. Run `df -h / /var/lib/peacock` first, as
    build-test.md says; I did not check whether `/var/lib/peacock` is on `/`.
- **Step 5, the rerun.** The same command: exit 0, wall 6 s. Its whole output is the build image's
  `docker pull` ("Image is up to date"), bootstrap's closing text and the Claude line. No apt, curl,
  clone, plugin or sf1 work; cargo's fetch printed nothing. Log: `/tmp/t7/run2.log`.
- **Step 6, the GPU key.** On the CPU VM: `grep -c '^GPU_SSH_KEY_FILE='` in `peacock.env` is `1`
  after both runs (provision rewrites the file, then appends the line). `gpu_ed25519` is mode 600.
  Its fingerprint, from the `.pub` and derived from the private key with `ssh-keygen -yf`, is
  `SHA256:2aXlKfnh+7h7M2NXk/UrO+NTIUNAd8D+cZk3mOJUBnw`, equal to dev's `~/.ssh/id_ed25519.pub`.
  The H100 started at 89.169.123.170. Its one line under alias `nebius-computeinstance-e00e6yena4nzjgaym0`
  in dev's `~/.config/peacock/nebius/known_hosts` went to the CPU VM's
  `~/.config/peacock/h100_known_hosts` (1 line, mode 600). Then, on the CPU VM,
  `ssh -i ~/.config/peacock/gpu_ed25519 -o BatchMode=yes -o StrictHostKeyChecking=yes -o UserKnownHostsFile=~/.config/peacock/h100_known_hosts -o HostKeyAlias=nebius-computeinstance-e00e6yena4nzjgaym0 dmitry@89.169.123.170 true`:
  exit 0. Nothing was authorized anew. The H100 then the CPU VM stopped softly, each exit 0, and
  `status` shows both `STOPPED`.
- **Drift, not fixed (no wrong behaviour today).** `provision-vm.sh` records the VM as
  `nebius-cpu128 dmitry@66.201.6.39` in `~/.config/peacock/vm`. build-test.md ("Nebius host") says an
  IP can change on every start and is never written down. So a later `provision-vm.sh --new-pat`
  goes to that IP. If the IP has moved, ssh fails (no answer, or a host-key mismatch) and the script
  names the VM as unreachable, as designed. It does not reach the moved VM. A Nebius VM
  needs `provision-vm.sh dmitry@$($C ip) --name nebius-cpu128 …` again, or a by-hand `gh auth login`,
  after a PAT change. The same goes for the entry dev's `~/.ssh/known_hosts` now holds for
  66.201.6.39.
- Claude Code on nebius-cpu128 is installed, not logged in. That is the human's optional step 4
  in "Blocked" above.

## Task 8: sf40 and DuckDB on the VMs

Developer, 2026-11-04, Steps 1–4 (offline). VM time: none.

- **Files.** `scripts/env/fetch-sf40.sh`, `scripts/env/install-duckdb.sh`, `scripts/env/sf40.sha256`
  (the aggregate pins), `scripts/lib/pinned.sh` (`pin`, `download_checked`, `unzip_into`, shared by
  the two), tests `scripts/env/tests/test_fetch_sf40.py` (23) and `test_install_duckdb.py` (8).
- **The pin is one line a dataset**, `<sha256>  <dataset>`, in `scripts/env/sf40.sha256`. Step 5 adds
  `<sha>  tpch.sf40`. Until then the fetch prints `tpch.sf40: aggregate <sha>, not pinned in
  scripts/env/sf40.sha256`: copy that value. `test_committed_pin_is_tpcds_sf40s` accepts a tpch line.
- **The aggregate is in C order.** The per-file sums of the bucket's tpcds copy are in
  `containers-detail.md` (commit `6445eec0`). Hashed as listed, in C order, they give the pin
  `58610fc1…`; in `en_US` order (`customer_address` before `customer`) they give `cc962a30…`. So the
  script exports `LC_ALL=C`. `test_fetch_aggregate_is_in_the_c_locales_order` runs under `en_US.UTF-8`.
  The aggregate is computed on every run, also when nothing is fetched (`sha256sum` in parallel,
  `nproc` wide), so a second run still reads all 56 GB once.
- **Departures from the plan.**
  - `scripts/lib/pinned.sh` and `scripts/env/sf40.sha256`, beyond the plan's four files. The pin
    lives in a file, so the tests can pin stand-ins in a copy of the tree, as `test_embeddings.py` does.
  - Zips are unpacked by python3 (`unzip_into`), keeping each member's mode. A fresh Ubuntu VM may
    have no `unzip`; bootstrap installs python3. Symlinks in a zip are not handled; the aws CLI
    installer makes its own.
  - `fetch-sf40.sh` locks `<data dir>/.<dataset>.lock` and writes under umask 002, as `machine.sh`
    does in the shared data dir. It stops at the first dataset that fails.
  - A manifest name other than `<letters, digits, _>.parquet` is refused (exit 1) before any fetch.
  - The keys are read from the file alone. The caller's exported `AWS_*` are unset first, so no
    child but aws ever sees a key (`test_fetch_keeps_the_keys_from_every_command_but_aws`).
  - `install-duckdb.sh` checks the unpacked duckdb's `--version` before installing, so a refused zip
    leaves the old binary in place. `--prefix <d>` installs `<d>/bin/duckdb`, with sudo where
    `<d>/bin` is not writable.
  - The no-key-in-argv check reads every process's argv through strace, where strace is on PATH.
    Without strace it reads only the fake aws's argv. A mutation passing the keys through `env …`
    was caught only by the strace check.
- **Mutations, each red:** no `unset` of the caller's keys; no `LC_ALL=C`; kept by existence rather
  than size; no `.part` size check; keys through `env` argv; no extra-parquet check; no manifest name
  check; the pin ignored. `test_install_duckdb_refuses_a_zip_of_another_version` was red first:
  the version was checked after the install, so the wrong binary was left in place.
- shellcheck (`koalaman/shellcheck:stable`, `-x`) over the three scripts: no output, rc 0. Image removed.

**For Step 5 (CPU VM, live).**
- The CPU VM probably has no aws CLI. The fetch then installs `AWS_CLI_VERSION` into
  `~/.local/aws-cli`, which needs `curl` and `python3` on the VM. The H100 needs the same.
- `$C ssh "cd ~/$D && time scripts/env/fetch-sf40.sh tpch.sf40 tpcds.sf40"` reads
  `~/.config/peacock/peacock.env` (provision writes it). It prints one line for each object it
  fetches, then `tpch.sf40: aggregate <sha>, not pinned…`, then `tpch.sf40: fetched 8, kept 0` and
  `tpcds.sf40: fetched 24, kept 0`. The second run prints `fetched 0, kept 8` and `fetched 0, kept 24`.
- tpcds.sf40 is the live check of the pin's order. A mismatch there is not a locale question
  (settled above). It means the bucket's bytes differ from the published copy.
- `/var/lib/peacock` must exist and be writable by dmitry: group `peacock`, 2775, made by
  bootstrap (Task 7). The ssh session must be a fresh login after provision, for the group.

**For Task 9 (`run-gpu.sh prepare` on the H100).** `fetch-sf40.sh --env -` needs on the host:
`scripts/env/fetch-sf40.sh`, `scripts/env/sf40.sha256`, `scripts/lib/{peacock-config,s3,pinned}.sh`
and `docker/pins.env` (the aws pin), at the same relative paths. It also needs python3, flock, curl,
and a writable `/var/lib/peacock`, which `prepare` must make with sudo: no bootstrap ran there. Only
the four storage keys are read from stdin; `GPU_SSH_KEY_FILE` and other lines are ignored.

**For Step 6 (`benchmarks/duckdb_minimal.sh`).**
- `--dir /var/lib/peacock --sf 40` reads the parquet from `/var/lib/peacock/tpch.sf40/`, the fetch's
  layout. **`--skip-download` is required:** without it the script lists the bucket with a bare
  `aws` (not `$AWS`), with the keys from aws's own config. The VM's aws is in `~/.local/aws-cli/bin`,
  which is not on PATH, and its keys are only in `peacock.env`.
- It writes `/var/lib/peacock/tpch.sf40.duckdb` and `/var/lib/peacock/duckdb_tmp/` in the shared
  dir. Its disk preflight then counts the local parquet (~40 GB) as the db estimate, + 10 %.
- It reads `testdata/tpch_query_sql.sh`, `testdata/tpch-vec-queries/query_params.jsonl` and
  `testdata/goldens/tpch.sf40/` (16 files) from the synced tree. None is gitignored, so
  `nebius-sync.sh --no-data` carries them.
- It needs python3. `vmtouch` is optional (it falls back to `/proc/meminfo`). `lsblk` is used for the
  device report only. Timed runs use all cores unless `--threads`; the verify pass uses one thread.
- It builds the tpch db only. The tpcds db is sf40-baseline's extension (plan).

Developer, 2026-11-04, Step 5 (live): **stalled at the disk gate.** No fetch ran. No file changed
but this one.

- Offline first: `test_fetch_sf40.py` 23 passed 0 failed, `test_install_duckdb.py` 8 passed 0 failed.
- The CPU VM started at 66.201.6.39. `df -h / /var/lib/peacock`: both are `/dev/vda1`, 96 G, 22 G
  used, **74 G free**. `lsblk`: one 100 G disk (`vda`), no data disk (`vdb` is 1 M). The plan's gate
  is at least 110 GB free (parquet 56 GB + the tpch db ~38 GB + 10 %), so the plan says stall.
- What uses the disk: the build image `peacockdb-build:cudf-25.02`, 15.4 GB; `/var/lib/peacock`,
  775 M (sf1 and the cuDF source). Removing both still leaves about 90 G, short of 110. The image
  is needed again for Task 10 Step 1, the CPU-VM build.
- Fetching the parquet alone (56 of 74 G) was not done. It would leave about 18 G: too little for
  DuckDB's database (its preflight wants the local parquet's ~40 G + 10 %), and too little for the
  later CPU-VM build's `target-img-25.02/` and C++ build.
- Also seen: no `aws` and no `duckdb` on the VM yet, as expected; `curl`, `python3`, `flock` present.
  128 cores, 503 G RAM.
- The VM was up 19:13:00–19:14:57 and stopped softly (exit 0, `STOPPED`).
- **Unblocks it:** the human grows the CPU VM's disk. Step 5 alone needs 22 + 110 = 132 G in all.
  The later CPU-VM build and sf40-baseline's tpcds database need more, so 200 G or more is safer.
  Either grow the boot disk (and the root partition with it), or mount a data disk at
  `/var/lib/peacock`, kept `dmitry:peacock` 2775 (bootstrap's mode). Then rerun Step 5 from the
  `df` check.

## Task 9: `run-gpu.sh` and the staged install

Developer, 2026-11-04, Steps 1–4 (offline: fakes only, no VM, no image pulled or built). VM time: none.

- **Files.** New: `scripts/env/run-gpu.sh`, `scripts/env/stage-install.sh`, `scripts/lib/stage-cargo.sh`
  (`stage_cargo_{test,lib,}binary`, moved out of `shadgpu-env.sh` unchanged, which now sources it),
  tests `scripts/env/tests/test_run_gpu.py` (29, the Nsight `--ssh` test among them),
  `test_stage_install.py` (5) and their fakes `gpu_host.py`. Changed: `create_nsys_profile.sh`
  (`--ssh`), `scripts/lib/shadgpu-env.sh` (`REMOTE_SSH`, `remote_ssh`; `pull_one` and
  `resilient_rsync` go through it, default `ssh`, so build-test-shadgpu.sh behaves as before),
  `scripts/lib/machine.sh` (`machine_image_checked … [<kind>]`, `build` by default).
- **How it is built.** Every VM step is a bash function in `run-gpu.sh` (`vm_*`), sent by `on_vm`
  as `nebius.sh … ssh "bash -c <declare -f text> run-gpu.sh <args>"`, so there is no script on the VM
  to keep in step. The image checks run on dev: `docker` is a function forwarding to the VM, so
  `machine_image_checked` compares the VM's labels with this tree. `prepare` first sends a kit to
  `~/peacock/<i>/` (fetch-sf40.sh and sf40.sha256, the five libraries they and `machine_data_set`
  read, `docker/pins.env`, `testdata/generate_testdata.sh`). Order: env file → kit → system (exit
  4 here, before any data) → build and run image checks (2) → driver (3) → sf1 → sf40 → cuDF copy.
- **Departures from the plan.**
  - `prepare` checks the build image's label too: sf1 is generated in it, as bootstrap does.
  - `ship` refuses while `peacock-<i>` runs, and before `prepare` (a dataset missing: exit 2), before
    it sends anything. It sends `git ls-files testdata` (working-tree contents, no `--delete`, so a
    run's results stay) and the install with `--delete`.
  - `run` clears a finished `peacock-<i>` left by a dropped session; its exit is the container's
    0 or 2, else 1. Its log is also written to `~/peacock/<i>/logs/<mode>.log`.
  - `pull` brings `logs/` to `~/.cache/peacock/run-gpu/<i>/` (outside the tree, so `git status` stays
    clean); results and calibration with `--update`, so a file edited here since `ship` is kept.
  - A test hook: `PEACOCK_NVIDIA_PARAMS` replaces `/proc/driver/nvidia/params`, as nebius.sh's
    `GPU_HOST_*` do.
  - `stage-install.sh` refuses an image whose `GCC_VERSION` is not `docker/pins.env`'s `GCC_<v>`
    (the wrong version's image), and builds into `target-img-<v>` unless `CARGO_TARGET_DIR` is set.
    The release benchmark is built without `--features gpu`, as `--build-benchmarks` builds it.
- **Red, then green.** Before: `test_run_gpu.py` 0/29 (no `run-gpu.sh`; `create_nsys_profile.sh`:
  `unknown argument: --ssh`), `test_stage_install.py` 0/5 (no script). After: 29/29, 5/5. Twenty
  mutations, each red in its named test: 64 not mapped, the driver compare, `--hostname`, the data
  mount path, keys through `env` in the ssh command, no busy check, no exit 4, no run-image check, a
  `start` call, ship without `--delete`, `--install` ignored, the exit not mapped, the cuDF stamp
  ignored, `pull_one` or `resilient_rsync` or the trace pass on bare ssh, one strip dropped, the
  benchmark without `--release`, no staging from empty, the gcc check dropped.
- Every image-free `scripts/env/tests/test_*.py`: 0 failed (16 files). `scripts/nebius/tests/`: 0
  failed. `scripts/calibration/tests/test_plot.py` fails on dev for want of matplotlib, as before.
  shellcheck (`koalaman/shellcheck:stable -x -P SCRIPTDIR`): the three new files clean; the four
  changed ones no new finding against HEAD's. Image removed.

**For Task 10 (the H100, live). What the fakes cannot show:**
- **First `prepare` installs.** The NVIDIA Container Toolkit from `nvidia.github.io/libnvidia-container`
  (keyring, list, `apt-get install nvidia-container-toolkit`), then `nvidia-ctk runtime configure`
  and `systemctl restart docker` when `docker info` names no `nvidia` runtime. nsys: one already
  under `/opt/nvidia/nsight-systems*/*/bin/` or `/usr/local/cuda*/bin/` is linked to
  `/usr/local/bin/nsys`; else `nsight-systems-cli` from
  `developer.download.nvidia.com/devtools/repos/ubuntu2404/amd64/` (key `nvidia.pub`). Both
  unverified. A failure there is exit 1 with apt's or curl's message.
- **Docker group.** Without docker access the user joins `docker`, and that session uses
  `sudo docker`. Every later call is a new ssh login and has the group.
- **Exit 4.** `/etc/modprobe.d/peacock-profiling.conf` is written on every `prepare`;
  `update-initramfs -u` and exit 4 only while the params file says `RmProfilingAdminOnly: 1`. If a
  restart leaves it at 1, look for another modprobe file setting the option (the later file wins).
- **Disk.** The H100 pulls both images (build ~10 GB for sf1, run ~5 GB), copies the run image's
  `/opt/cudf` to `~/peacock/cudf-25.02/` (several GB), and fetches sf40 (~56 GB) into
  `/var/lib/peacock`, which `prepare` makes with sudo: owner dmitry, his group, 2775 (no `peacock`
  group there). Check `df -h / /var/lib/peacock` before.
- **sf40 on the H100** installs the aws CLI into `~/.local/aws-cli` (curl, python3). The keys are
  dev's `~/.config/peacock/peacock.env` (present on dev), on stdin.
- **`run` holds the ssh session** while it follows the log. If ssh drops, the container goes on;
  the next `run` refuses (exit 2) until it ends, then clears it. Its log is in
  `docker logs peacock-<i>` meanwhile, and in `~/peacock/<i>/logs/<mode>.log`.
- **`pull` leaves the host's `records.tsv`**: Step 4's explicit `rm` is still needed.
- **Nsight on the host.** `--remote-dir /home/dmitry/peacock/<i>` holds `cpp/install/` and
  `testdata/`, its dataset links absolute, so the binary runs outside the container too, with
  `<dir>/cpp/install/lib:/home/dmitry/peacock/cudf-25.02/lib`. That needs the H100's glibc ≥ 2.39
  (Step 2's check). containers' record: `libnvrtc.so` and `libnvJitLink.so` resolve in neither
  image, so possibly not in the copy either. `PCK_BENCH_HBM_PEAK_BW` defaults to an H200's
  4.8e12; a `--metrics` pass on the H100 must set its own.
- **The CPU-VM build** runs `stage-install.sh` with no cargo registry mounted and ccache in `/tmp`:
  every build downloads the crates and compiles C++ cold. `target-img-25.02/` is gitignored, so
  it survives `nebius-sync.sh` on the VM. `third_party/cudf` arrives with the sync (only `.git` is
  excluded).

## Task 11: `build-test.md`

Developer, 2026-11-04, Steps 1–2 (offline). VM time: none.

- "Nebius host" rewritten as one section: the three VMs (instance, region, profile, machine, role),
  `nebius.sh`'s options and aliases, install-watch's one profile (embedded vs PEM, editor-eu's
  refusal) with the three install lines, `run-gpu.sh` (verbs, options, exits 0–4), `stage-install.sh`,
  "The CPU-VM build" (the plan's five commands verbatim), the watcher's install file and CPU rule,
  the three legs with their notices, the nine secrets, disk on the new VMs, hard stops on all three,
  and "The human's one-time setup".
- Datasets: two columns, nebius-h100 and nebius-cpu128. They track sf40 alone and hold `—` until an
  audited fetch. Containers: `duckdb_minimal.sh --dir /var/lib/peacock --sf 40 --skip-download`.
- Counts: "Nebius scripts (Python)" 104 → 170 (13 files, each run: 170 passed). Grand total
  6864 → 6930, Python 766 → 832; the N columns sum to both. Images stays 222 (19 files, counted).
- **Left for the live tasks** (no guess written): the sf40 ✓ in both Nebius columns and tpch.sf40's pin
  line in `scripts/env/sf40.sha256` (Task 8 Step 5); fetch and DuckDB wall times, and the native
  database's size (Task 8); the H100's glibc, driver, disk, the first `prepare`'s installs and exit 4,
  the sf1 corpus counts, the benchmark's wall time, and the H100's `PCK_BENCH_HBM_PEAK_BW` value
  (Task 10); the CPU VM's disk and whether provision ran clean (Task 7).
- **Drift seen, not changed:** "Benchmark data flow" still says nebius-gpu runs no benchmark, so no
  record is published while shad-gpu is down. `run-gpu.sh pull` brings a record home but does not
  publish it, so that holds; Task 10 may want a line. The Datasets bullet keeps "sf40 on shad-gpu is
  a hard precondition of the GPU job", though `gpu-tests` now runs on nebius-gpu without sf40.

## Blocked (2026-11-04)

`blocked(building)`: no `~/.config/peacock/github-pat` on dev. `provision-vm.sh` needs the human's
fine-grained PAT; Task 7 Steps 3–6, Task 8 Steps 5–6 and Task 10 wait on it. Task 6 Step 5 waits on
`ENS-nebius-guard` (`95e532bf`) landing on master. A fresh analyst confirmed no sanctioned way
around it (hand-bootstrapping would spend the fresh VM item 7 tests; H100 binaries are built only
on the CPU VM).

Human actions, each with what it unblocks:
1. On dev, in a terminal: `scripts/env/provision-vm.sh --new-pat` and paste a fine-grained PAT
   (owner `asymptote-tech`, only `peacockdb`; Contents, Pull requests, Workflows, Actions
   read-write; Commit statuses, Metadata read). It writes `~/.config/peacock/github-pat` (600)
   and touches no VM. Then clear the block. → Task 7 live, then Task 8 live, then Task 10.
2. Land `ENS-nebius-guard` on master (approved in advance); update chain J's host override line
   in `tasks.md` (it names `scripts/gpu-host*.sh`). → Task 6 Step 5, needed before `done`.
3. After landing, `scripts/nebius.sh install-watch` on nebius-gpu (standing rule after a change
   under `scripts/nebius/` reaches master).
4. Optional: log Claude Code in on nebius-cpu128 with the command provision prints.

## Rebase onto master (2026-11-04)

Rebased onto `ENS-containers-rebase` (9afe0e7b, itself on master). Dropped as already on master
(the human's `bf937124` + `e20db7e9`, from `ENS-nebius-guard`): 0651a08a, c324fc9d, 865d3830,
d34fe96e, ae7efe3e, 620184ae. Record-only commits that went empty (0f108108, 4ee2fdca, b5be7943,
add2a081) are restored by this file and the plan taken from the old tip (`L-pre-rebase-new-hosts`).
`scripts/nebius*`, `.github/` and `scripts/env/` equal the old tip's but for master's own changes.
The human lifted the block (PAT on dev); the task resumes at Task 7 Step 3 once re-proven.

**build-test.md, merged by hand (developer, 2026-11-04).** The replay kept HEAD's page. It was
rebuilt from `git merge-file --diff3` of HEAD, `687b61fe` and the old tip: 17 conflict hunks.
- "Nebius host" is the old tip's Task 11 section, whole. Master's short "Other VMs" list was Task
  12's stand-in for it; each of its facts is in the full section. Every value there was checked
  against `nebius-watch.yml`, `watch.sh` and `idle_watch.py`.
- Test categories: master's "Nebius scripts" row and its place, plus links to the three test
  files the old tip named. The old tip's "Images" row, with fetch-sf40, install-duckdb, run-gpu,
  stage-install and the Nsight `--ssh`.
- Datasets: the two Nebius columns, and master's calibration-record row with both left blank.
- Workflows: the old tip's `nebius-watch.yml` line (one leg per VM). CI structure: master's.
- A clean-merge line HEAD had lost: the run image's `list` follows `nebius-test.sh`'s rules, not
  `gpu-host-test.sh`'s.
- Counts, by the page's rule (the header sums the N columns): Nebius 170 (13 files, run), Images
  156 → 222 (19 files, run). Python 1401 → 1467, total 7593 → 7659. Rust 5959 and C++ 233 are
  unchanged, and the cpu, ffi and gpu block headers equal their rows (3414, 7, 2409).

**Re-proven on this base (developer, 2026-11-04). VM time: none.**
- `scripts/nebius/tests/test_*.py`: 13 files, each rc 0, 170 passed, 0 failed.
- shellcheck 0.11.0 (in `rhysd/actionlint:latest`) over CI's six Nebius scripts: exit 0.
- actionlint: `nebius-watch.yml` no output, exit 0. `pipeline.yml` has only SC2001 (:52) and
  SC2086 (:371). It is identical to master's, and master's copy gives the same two.
- `cargo test -p cost-report --test test_source_formatting`: 32 passed, before and after the page.
- `scripts/env/tests/test_*.py`: 17 of 19 files green, 219 passed. Among them are Task 7's
  `test_provision` 20 (with `…without_a_terminal_names_the_claude_login`) and `test_bootstrap`
  17, Task 8's `test_fetch_sf40` 23 and `test_install_duckdb` 8, and Task 9's `test_run_gpu` 29
  (with `test_nsys_profile_uses_the_given_ssh`) and `test_stage_install` 5.
  Three cases fail for want of inputs, as in containers-rebase's record. The cause is dev, not
  code:
  `test_image_any_user` 26.02 (no 26.02 build image; dev cannot hold it beside the run images,
  39 GB free) and `test_run_image`'s two library checks (no `PEACOCK_STAGED_INSTALLS`). No image
  input differs from the old tip: `git diff L-pre-rebase-new-hosts HEAD` over `docker/`,
  `scripts/env/`, `scripts/lib/`, `third_party/` and `images.yml` is empty.
- The images match the tree: `image_inputs_differ` prints nothing for build and run, 25.02 and
  26.02. The missing build image's label was read remotely with `docker buildx imagetools inspect`.
  A bogus label prints the inputs, so the check can go red.
- `grep 'Out until further notice' build-test.md`: nothing. `stop --force` in `ensemble.md`: 3.
  `git ls-files scripts/env/host scripts/env/gpu.sh`: nothing. `git status --short -- docker`:
  nothing.
- Task 11's `gpu-host` grep matches only the fixture hostname `host=gpu-host` (`-1`). It is in
  peacockdb-core's record and trailer tests, `test_split.py` and the page's sample trailer. The
  old tip has the same matches. It names a host in a sample record, not a script, so it stays.
  `docker/run-tests.sh:7` is still the one recorded exception.

## Analyst: disk (2026-11-04)

The obstacle is Task 8 Step 5's gate (at least 110 GB free) against nebius-cpu128's 74 G. Read
only: no VM started, no API call; the CLI read was `nebius compute disk update --help` alone.
**Verdict: the human can resolve it; the ensemble cannot.** No sanctioned way exists for the
ensemble to grow the disk, and nothing that meets the spec fits in 74 G. Task 6 Step 5 and Task 10
can go ahead first.

**1. Growing the disk is the human's job.**
- The shared rules send every cloud call through `scripts/nebius.sh`. Its verbs are `status`,
  `ip`, `start`, `stop`, `ssh`, `rsh` and `install-watch`, and none of them touches a disk. The
  spec's Scope gives `nebius.sh` instance and profile selection, host-key aliases and the
  credential install, and nothing more.
- build-test.md leaves the VM itself to the human. Exit 69 says "stall; rebuilding it is the
  human's". Step 2 of "A fresh VM" says "Have someone create the VM", and the spec begins "The
  human created two VMs". "The human's one-time setup" covers credentials only. The "Disk"
  section's answer to a full disk is to clean what can be rebuilt and record an obstacle. It
  never grows a disk.
- The CLI can do it: `nebius compute disk update --id <disk> --size-gibibytes <n>`. But a bigger
  disk is billed while the VM is stopped, and it cannot shrink again. That is a standing cost,
  so it is the human's call, as the VMs were. Chain M's direct CLI use came from its own spec,
  not from a standing rule.

**2. No spec-compliant way fits in 74 G.** These are measured sizes, not the plan's estimates:
- sf40 parquet: tpch 42,442,683,465 B + tpcds 15,787,929,533 B = 54.2 GiB. Source: chain M's
  copy of both buckets, `origin/ENS-comet-research:llm-wiki/tasks/comet-research-detail.md`,
  Task 4.
- The native tpch.sf40 database: 32.7 GiB, DuckDB 1.5.4 on a 128-core Nebius VM. Source:
  `llm-wiki/reports/benchmark-minimal.md`, the setup table.
- Task 8 alone ends at 86.9 GiB. That is 12.9 GiB more than the 74 free, with no headroom at all.
  `duckdb_minimal.sh`'s own preflight (local mode, `--skip-download`) wants 1.1 × the local tpch
  parquet, 43.5 GiB, still free after the fetch. So the fetch needs about 98 GiB free before it
  starts. The plan's 110 GB is that figure plus some slack.
- Each way around it fails:
  - Dropping tpcds's database saves nothing. Only tpch's database is due in this task: item 5
    asks for the databases "that `benchmarks/duckdb_minimal.sh` builds", and today the script
    builds only tpch (`DB=…/tpch.sf${SF}.duckdb`). The plan's 110 already leaves tpcds out.
  - The parquet stays. Item 5 says both sets go "on both VMs".
  - DuckDB cannot read the parquet directly. Views are not "native" (item 5). They are also not
    the protocol sf40-baseline's done-when 2 requires: "native storage, page cache warmed and its
    residency checked". The script's header gives the reason: the numbers would be mostly
    parquet decode.
  - Building the database straight from S3 (`--s3-direct`) and removing the local parquet fails
    item 5 too. It also breaks the plan's later runs. In local mode, every run compares the
    database's row counts with the local parquet's and deletes the database (`rm -f "$DB"`) if
    they differ, so once the parquet is gone the next run deletes the database. On top of that,
    `--s3-direct` lists the bucket with a bare `aws` and reads `~/.aws`, and the VM has neither.
  - Removing the build image (15.4 GB) and `/var/lib/peacock` (0.8 G) gives about 89 G free.
    The end state would fit with about 2 G to spare, but the script's preflight still refuses.
    And the image is needed next, for the CPU-VM build (Task 10 Step 1) and every later chain L
    build.
- **tmpfs is the one way that fits, and it does not meet the spec.** The VM has 503 GiB of RAM,
  and chain M fetched both sets onto a tmpfs in about 110 s. But a tmpfs is lost at every stop,
  and the VM-time rule stops the VM after every step, so every later session would fetch and
  import again. Item 5's "a second run fetches nothing it already holds" and the Datasets ✓
  would then mean nothing past a stop. On a tmpfs, `duckdb_minimal.sh` finds no block device,
  so `sectors_read` returns 0 and every query reads `warm yes` by construction. sf40-baseline's
  disk-read check would go blind. Using tmpfs changes both the spec and the protocol, so only
  the human could choose it. If chosen, mount a tmpfs of its own (`mount -t tmpfs`) rather than
  using `/dev/shm`: chain M found that logind's RemoveIPC empties `/dev/shm`.

**Size to ask for: 200 GiB or more**, as the developer wrote. On root, after the chain's CPU-VM
work:

| item | size |
|---|---|
| used today | 22 G |
| sf40 parquet | 54.2 GiB |
| tpch database | 32.7 GiB |
| tpcds database | gated at 30 GB by sf40-baseline Task 8 Step 2; its parquet is 14.7 GiB |
| CPU-VM build: `target-img-25.02/` and the C++ build | not measured; dev's gpu target dir is 8.8 G |
| total | about 140–160 G |

Either layout works:
- Grow the boot disk: `nebius compute disk update --id <boot disk> --size-gibibytes 200`, with
  the VM stopped. cloud-init normally grows `/dev/vda1` at the next boot. If `df -h /` still
  shows 96 G, run `sudo growpart /dev/vda 1 && sudo resize2fs /dev/vda1`.
- Or add a data disk, mounted at `/var/lib/peacock` (fstab by UUID, `nofail`), owned
  `dmitry:peacock`, mode 2775, with sf1 and the cuDF checkout moved onto it.

The boot disk is a NETWORK_SSD at about 0.062 GB/s (chain M measured it with O_DIRECT and with a
copy). That costs VM time, not space. A rerun after a stop reads all 58 GB cold for the aggregate,
which takes about 16 minutes.

**The H100 may hit the same wall.** Its disk size is recorded nowhere. `run-gpu.sh prepare`
puts these on it, beside the OS and the NVIDIA stack:
- two images: the build image (15.4 GB, as measured on the CPU VM) and the run image (about 5 GB);
- the run image's `/opt/cudf` copy;
- sf40 (54.2 GiB) and sf1 (1.75 GB);
- the shipped install.

That comes to about 85–100 G. If the H100 has the same 100 GiB boot disk, Task 10 Step 2 stalls
the same way. Ask the human to check it in the same visit.

**3. What can go ahead without the disk.**
- **Task 6 Step 5 can run now.** It is the coordinator's step and needs no disk. The guard's
  three files are on master and equal to the branch's: `git diff origin/master HEAD` over
  `scripts/nebius/`, `scripts/nebius.sh`, `nebius-watch.yml` and `nebius-setup/` is empty.
- **Task 10 Step 1, the CPU-VM build, can run now.** The build image is already on the VM. Its
  output is not measured, but 74 G leaves room for it, and the 200 GiB sizing above counts it.
- **Task 10 Steps 2–6 can run on the H100, if its own disk holds `prepare`.** Run `df -h /
  /var/lib/peacock` before `prepare`, as build-test.md "Disk" says. One departure, to record:
  Step 2 checks the H100's tpch.sf40 aggregate against Task 8's pin, and that pin does not exist
  yet. Pin it from the H100's fetch instead (the unpinned line prints the value). Task 8 Step 5
  then checks the CPU VM's fetch against it, so two independent fetches are still compared.
  The H100's `prepare` also checks tpcds's pin live, so a partial fetch on the CPU VM now would
  prove nothing new.
- **`install-duckdb.sh` alone fits too.** It is the first half of Task 8 Step 6 and could run in
  Step 1's CPU VM session. This is optional.
- **Task 11's live values.** The H100's values follow Task 10: glibc, driver, disk, what
  `prepare` installs, the corpus counts, the benchmark's wall time and `PCK_BENCH_HBM_PEAK_BW`.
  The CPU VM's values wait on the disk: the sf40 ✓, the tpch pin line, the fetch and DuckDB wall
  times, and the database's size.
- **Waiting on the disk:** Task 8 Steps 5–6, and with them `reviewing`, because the plan lets
  only Task 6 Step 5 lag behind review. So run Task 6 Step 5 and Task 10 first, then mark Task 8
  `blocked(building)` with the human action below.
- **Later in the chain:** pinned-loads and bench-rows use the CPU VM only for the build, which
  fits. sf40-baseline's Task 8 Step 2 (the tpcds database, at least 30 GB) and Task 11 (DuckDB's
  timings) need the larger disk.

**The human action, for the block:**
1. Grow nebius-cpu128's disk to 200 GiB or more, either way above.
2. Check the H100's boot disk, and grow it to 150 GiB or more if it is 100 GiB.
3. Clear the block.

Task 8 then resumes at Step 5's `df` check.
