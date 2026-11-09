# vm-setup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a developer gets a CPU VM from `provision-vm.sh` that can build in the build image and
reach Nebius, every ssh survives a dropped connection, `run-gpu.sh` regains what the earlier
containers branch had, the build images build with a plain `docker build`, and one table of
command lines works as written.

**Architecture:** everything that needs no Nebius access and no new VM comes first (Tasks 1–14),
against fakes: a fake ssh that writes its `-E` log, a fake `nebius` CLI that keeps its state in
the config file it is given, and `gpu_host.py`'s fake VM grown a held device run. One ssh helper,
`scripts/lib/ssh-retry.sh`, wraps every ssh the scripts make. The nebius config's path, the
profile parser, the two-profile cut and the card lock live in `scripts/lib/peacock-config.sh`, so
`provision-vm.sh`, `nebius-profiles.sh`, `nebius.sh`, `run-gpu.sh` and `create_nsys_profile.sh`
share them. Item 7 keeps run-gpu.sh's model: VM-side functions sent as text by `on_vm`; a run is
now held by a `setsid` runner on the VM. The steps that need the outside world (Tasks 15–17) are
the coordinator's and come last.

**Tech Stack:** bash, Docker/BuildKit, OpenSSH, rsync, flock, the `nebius` CLI 0.12.293, GitHub
Actions; Python 3 script tests (`scripts/env/tests`, `scripts/nebius/tests`,
`scripts/calibration/tests`, the stdlib harness); the Rust guard in
`cost-report/tests/test_source_formatting/`.

**Spec:** [`vm-setup.md`](vm-setup.md). Branch `ENS-vm-setup` (the name below), on
`ENS-containers-only` (7aeec989); its PR targets that branch. The reference for Tasks 9–12 is
`4e785104` (on `origin/ENS-containers`): `git show 4e785104:scripts/env/{gpu.sh,on-gpu-host.sh}`,
`scripts/lib/machine.sh` and `scripts/env/tests/{test_gpu,test_gpu_records,test_gpu_locks}.py`.

## Global Constraints

- Developers never commit, push or dispatch a workflow (prompts.md). A step "report a checkpoint
  `<msg>`" means: stop and report it; the coordinator stages, commits and pushes. A step marked
  *(coordinator)* is the coordinator's; the developer stops before it and says what it needs.
- Commit messages and every committed text: at most 10 lines; no process words (review, task,
  the human's choice), no people's names, no paths outside git. Prefix `vm-setup: `.
- No engine change, and no test is weakened. An existing assertion that names a shape this
  change replaces (a `docker run -d`, a ref where the ID now runs, a log path) is rewritten to the
  new shape with the same intent; the detail file lists each, under `## Reshaped tests`.
- Never print, `cat`, `grep`, Read or parse a Nebius credential file: `~/.nebius/*`, the
  developer's `~/.config/peacock/nebius.yaml`, their copies, on any host. Only the CLI reads them,
  and only its answers to `profile list`, `profile active`, `config get` and `version` are used.
  Tests use a fake CLI and fake config files, and query those through the fake CLI. Never read
  `~/.config/peacock/peacock.env`'s values or anything under `~/.config/peacock/hosts/`; a
  presence check is `test -f`.
- Secrets travel on ssh's stdin alone: never in an argv, never in a file outside
  `~/.config/peacock/`, which keeps its name.
- Tests: `python3 <file>` on the host for `scripts/env/tests/`, `scripts/nebius/tests/` and
  `scripts/calibration/tests/`. Anything needing the build image goes through
  `scripts/build-test.sh`, run in a tree under `/build` only: it keeps `target-img-<v>` and
  `.home-img` in the tree, and a tree in `/home` would put them there. If the workspace is not
  under `/build`, use a worktree there (`git worktree add --detach /build/<name> HEAD`) at the
  workspace's HEAD, with `git -C <workspace> diff HEAD | git -C /build/<name> apply`.
- The CI image runs `scripts/env/tests` as a uid it does not know: VM-side code names users by
  `id -u`, never `id -un`, on any path a test reaches. A test that needs a user name (real ssh
  does) goes on `pipeline.yml`'s `on_runner` list with its reason.
- New test files where an existing one would pass 1000 lines (`test_run_gpu.py` is 1183):
  `test_run_gpu_vm.py`, `test_run_gpu_runs.py`, `test_run_gpu_records.py`.
- Every `docker/` and `pins.env` edit is Task 1's, so one `images.yml` dispatch (Task 15) serves
  the branch. Nothing on the branch pulls an image of its inputs before Task 15: CI, `run-gpu.sh
  prepare`, Task 17's `bootstrap-vm.sh`.
- shellcheck: CI's `Nebius script tests` step lints `nebius.sh`, `run-gpu.sh`, `build-test.sh`
  and others with `-x`. It is not installed on dev; run it where it is, else CI is the check.
- Each task keeps `llm-wiki/build-test.md` in agreement in the same checkpoint: the sections it
  names, and the test-inventory rows "Images (Python)", "Nebius scripts (Python)" and "CI wiring
  guard (Rust)", text and `N` recounted.
- VM time is minimal: a VM starts right before its first remote command and stops right after its
  last pull. Every start and stop goes in the detail file's `## VM time`, in the host's local
  time. Every ssh from the Bash tool needs the sandbox off.
- Out of scope: `up.sh`/`down.sh`, shad-gpu and its scripts, the GPU VMs' idle watcher.

## Review Focus

1. **A Nebius VM restarted under a run** (they are preemptible): `run/state` names a pid of the
   last boot, which may now be another process. It must read as ended without a status, `run`
   must start, and its `records.tsv` must come home under its stamp. → Task 10
   `test_a_run_from_before_a_restart_is_not_going`; Task 12
   `test_a_record_left_by_a_runner_that_died_is_kept_under_its_run_s_stamp`.
2. **Two calls on one dir at once** (two terminals, or `run` beside `pull --clear`): one starts,
   the other is refused exit 2 naming the run; never two runners. → Task 10
   `test_two_runs_started_at_once_in_one_dir_start_one`.
3. **Ctrl-C on an attached `run`**: the run goes on, and run-gpu.sh says how to follow it. →
   Task 10 `test_an_interrupted_attached_run_goes_on_and_names_follow`.
4. **A tree moved, or re-cloned under the same name**: refused in one message naming both steps
   that work (remove `.run-gpu-tree`, then `--dir <a name of its own>`). → Task 11
   `test_a_copied_tree_is_refused_its_record_before_any_vm_call`.
5. **A VM dir from before this branch** (nebius-h100's, CI's `~/peacock/ci/`): no `.tree`, no
   `.shipped`, an old `records.tsv`, logs without a stamp. The dir is claimed, the record comes
   home under its time and stops blocking the next bench, and a local copy that differs is named,
   never replaced. → Task 11 `test_a_dir_made_before_tree_is_claimed_by_the_first_tree`; Task 12
   `test_a_record_from_before_the_runner_is_kept_under_its_time`,
   `test_a_dir_with_no_ship_stamp_names_each_local_file_that_differs`.

---

### Task 1: a plain `docker build` builds either image; the PTX driver minimum

**Files:**
- Modify: `docker/build.Dockerfile`, `docker/run.Dockerfile`, `docker/pins.env`
- Test: `scripts/env/tests/test_pins.py`, `test_image_ref.py`, `test_bootstrap.py`
- Docs: `build-test.md` "Rebuilding and publishing", "Images" (the run image's labels)

**Interfaces:**
- Produces: every `ARG <pin>` in either Dockerfile is `ARG <pin>=<its pins.env value>`; the first
  line before any `FROM` is `ARG CUDF=25.02`, and a stage's bare `ARG CUDF` inherits it;
  `ARG CUDF_SUBMODULE=43505bb975f46ce4e140fdaf55192611d6e231ac` (the index's `third_party/cudf`).
  `# check=skip=InvalidDefaultArgInFrom` and the "No default" comments go.
- Produces: `DRIVER_MINIMUM_PTX=575.51.03` in `pins.env` (comment: the driver of
  `NVCC_VERSION`'s CUDA, 12.9, which a card other than 8.x or 9.x needs to JIT the PTX). In
  `run.Dockerfile`'s final stage: `ARG DRIVER_MINIMUM_PTX=575.51.03` and
  `LABEL peacockdb.driver-minimum-ptx=${DRIVER_MINIMUM_PTX}`, so every run image's inputs hash it
  and its labels carry it (Task 9 reads it).
- Unchanged: `image.sh` passes every pin; `image_inputs`' awk already strips `=…` from an `ARG`.

- [ ] **Step 1: failing tests** in `test_pins.py`:
  - `PINS["DRIVER_MINIMUM_PTX"] = r"575\.51\.03"`.
  - `test_every_pin_a_dockerfile_declares_defaults_to_the_pin`: for each `^ARG (\w+)(=(\S+))?`
    in either file whose name is in `pins.env`, the default is present and equals the pin;
    the message names the file, the line and the pin.
  - `test_cudf_defaults_to_25_02_before_the_first_from`: in both files.
  - `test_cudf_submodule_defaults_to_the_index`: the default equals the commit `git ls-files -s
    third_party/cudf` gives.
  - `test_the_run_image_labels_the_ptx_driver_minimum`: after the last `FROM` of
    `run.Dockerfile`, the `ARG` and the `LABEL` above.
- [ ] **Step 2: run** `python3 scripts/env/tests/test_pins.py` → FAIL on those five.
- [ ] **Step 3: implement** the defaults, the pin and the label.
- [ ] **Step 4: pin what the spec says of a plain build's refusal** (passes at once; the code
  exists): `test_image_ref.py::test_refuses_an_image_without_a_label` also asserts
  `scripts/env/image.sh` in stderr; new `test_bootstrap.py::test_a_plain_build_is_refused_naming_image_sh`
  (world label `None` → exit 2, stderr names `peacockdb.inputs` and `scripts/env/image.sh`, no
  `docker run`). `machine_image_label_checked` is also run-gpu.sh's.
- [ ] **Step 5: run** every `scripts/env/tests/test_*.py` but the three needing images
  (`test_image_any_user.py test_run_image.py test_run_tests.py`) → `0 failed`.
  `test_image.py`'s label tests must pass unchanged.
- [ ] **Step 6: docs.** "Images": the run image's `peacockdb.driver-minimum-ptx` label.
  "Rebuilding and publishing": a bullet with
  `docker build -f docker/<kind>.Dockerfile .` from the root, `--build-arg CUDF=<v>` or none for
  25.02: what `image.sh build <kind> <v>` builds, layer for layer, less its labels; it has no
  `peacockdb.inputs` label, so `build-test.sh`, `run-gpu.sh` and `bootstrap-vm.sh` refuse it,
  naming `image.sh`; why defaults and not a file read by the build (spec item 1's last bullet).
- [ ] **Step 7: report a checkpoint** `vm-setup: a plain docker build builds either image; the run image names the PTX driver minimum`.

### Task 2: the plain builds proved against `image.sh`'s, on dev

Needs Docker on dev and downloads; no Nebius, no published image. It also leaves this branch's
build and run images for 25.02 local, which Tasks 13, 14 and 16 use.

- [ ] **Step 1:** in the tree under `/build` (Global Constraints), `time scripts/env/image.sh
  build build 25.02`, then at once `docker build -f docker/build.Dockerfile --progress=plain -t
  peacock-plain:build-25.02 . 2>&1 | tee <scratch>/plain-build.log`.
- [ ] **Step 2:** every `#<n> [<stage> <i>/<k>] RUN|COPY` line of the plain log has its
  `#<n> CACHED` line. `docker image inspect --format '{{json .RootFS.Layers}}'` of
  `ghcr.io/asymptote-tech/peacockdb-build:$(bash -c '. scripts/lib/image-inputs.sh;
  image_inputs_tag . build 25.02')` and of `peacock-plain:build-25.02` are equal.
- [ ] **Step 3:** Steps 1–2 for `run` (`run.Dockerfile`, tag `peacock-plain:run-25.02`).
- [ ] **Step 4:** `docker build -f docker/run.Dockerfile --build-arg CUDF=25.10 .` → non-zero,
  stopping at the final `FROM` (its output names `run-25.10`).
- [ ] **Step 5:** `docker rmi peacock-plain:build-25.02 peacock-plain:run-25.02`. Record in the
  detail file's `## Plain builds`: each command, wall time, the CACHED count against the step count,
  the sha256 of each pair's layer JSON, and Step 4's last line. A step not taken from the cache is
  a failing test for Task 1: find which `ARG` changed its key, and fix it there.
- [ ] **Step 6: report a checkpoint** `vm-setup: the plain builds proved against image.sh's`.

### Task 3: one ssh helper, retried only before authentication

**Files:**
- Create: `scripts/lib/ssh-retry.sh`, `scripts/env/tests/test_ssh_retry.py`
- Modify: `.github/workflows/pipeline.yml` (`Image and run script tests`: `test_ssh_retry.py`
  on `on_runner`; its comment gains "real ssh needs a user name"); `build-test.md` "Rebuilding
  and publishing" (the on-runner list)

**Interfaces:**
- Produces `ssh_retry <ssh> <ssh argument>…` → ssh's exit. Each attempt runs
  `<ssh> -E <log> -o LogLevel=VERBOSE -o ControlPath=none <ssh argument>…` with a fresh log
  (ours first: ssh takes an option's first value). It retries only an exit 255 whose log has no
  line beginning `Authenticated to ` and none holding `Host key verification failed` or
  `REMOTE HOST IDENTIFICATION HAS CHANGED`. At most 4 attempts, pauses from
  `${PEACOCK_SSH_PAUSES:-5 10 20}` seconds. Each retry is one stderr line:
  `ssh: attempt <n> of 4 failed before authenticating (<the log's last line, less its CR>); again in <s> s`. After the
  attempt that is returned, its log goes to stderr less the lines `LogLevel=VERBOSE` adds, those
  beginning `Authenticated to `, `Transferred: ` or `Bytes per second: ` (measured on OpenSSH
  9.6: a refused connection's log is the plain stderr line; a session adds exactly these three).
  stdin is the caller's, never read between attempts.
- Produces `sync_retry <command>…` → its exit: re-run whole on exit 255 (ssh's) or 30 (rsync's
  own I/O timeout, which `to_vm`'s and `from_vm`'s `--timeout=90` reach before ssh's ServerAlive
  bound on a silently dead route), same pauses and bound, stderr `<command's name>: attempt <n>
  of 4 exited <rc>; again in <s> s`.

- [ ] **Step 1: failing tests**, a fake `ssh` on PATH driven by a JSON list of attempts
  (`log` lines written to the `-E` file, `rc`, `stdout`, `read_stdin`), `PEACOCK_SSH_PAUSES="0 0 0"`
  unless said:
  - `test_a_connection_that_never_authenticates_is_tried_four_times`: 4 calls, exit 255, three
    retry lines naming attempts 1–3, the refusal line on stderr once.
  - `test_a_255_after_authenticating_is_returned_at_once`: one call, exit 255, no
    `Authenticated to` on stderr.
  - `test_a_host_key_refusal_is_not_retried`: one call; `Host key verification failed.` on stderr.
  - `test_the_command_s_own_status_passes_through`: authenticated, rc 3 → one call, exit 3.
  - `test_a_retry_s_stdin_reaches_the_remote_whole`: two pre-auth refusals reading nothing, then
    an attempt echoing stdin → stdout equals the 64 KiB input.
  - `test_each_attempt_has_its_own_log_and_no_shared_connection`: every argv has `-E <file>`,
    `-o LogLevel=VERBOSE`, `-o ControlPath=none` before the destination; stderr after a third
    attempt holds that attempt's log lines alone.
  - `test_the_pauses_are_5_10_and_20_seconds`: `PEACOCK_SSH_PAUSES` unset, a fake `sleep`
    records `5`, `10`, `20`.
  - `test_verbose_only_lines_stay_off_stderr`: a session's log `Warning: Permanently added …`,
    `Authenticated to …`, `Transferred: …`, `Bytes per second: …` → stderr holds the warning alone.
  - `test_the_real_ssh_at_a_closed_port_is_tried_four_times_and_leaves_stdin`: the real `ssh`,
    `-o BatchMode=yes -p <a port bound and closed by the test> 127.0.0.1 true`, inside
    `bash -c '. ssh-retry.sh; ssh_retry ssh …; cat'` fed `payload\n` → three retry lines,
    `Connection refused` on stderr, exit of the function 255, stdout `payload`.
  - `test_sync_retry_reruns_a_command_that_exits_255`: 255, 255, 0 → 3 runs, exit 0; a first
    exit 1 → one run, exit 1.
- [ ] **Step 2: run** `python3 scripts/env/tests/test_ssh_retry.py` → FAIL (no library).
- [ ] **Step 3: implement** `ssh-retry.sh` (`# shellcheck shell=bash`, a header saying why each
  rule holds, under 10 lines), and the `on_runner` entry.
- [ ] **Step 4: run** → `0 failed`.
- [ ] **Step 5: report a checkpoint** `vm-setup: ssh is retried while it cannot connect, never once it has authenticated`.

### Task 4: every ssh the scripts make goes through the helper

**Files:**
- Modify: `scripts/env/provision-vm.sh` (`on`, `remote_test`, the login's `ssh -t`, the
  `--new-pat` loop through `on`), `scripts/nebius.sh` (`vm_ssh` both calls; `cmd_start`'s ssh
  wait; install-watch's two rsyncs through `sync_retry`), `scripts/env/run-gpu.sh` (`to_vm`,
  `from_vm` through `sync_retry`), `scripts/calibration/fetch_record.sh` (`--ssh`)
- Modify fakes: `scripts/nebius/tests/fakes.py` (SSH: its own lines go to `-E` when given; an
  authenticated path writes `Authenticated to <ip> ([<ip>]:22) using "publickey".`; world
  `ssh_refusals: n` refuses the first n calls before authenticating, `ssh_seconds` sleeps per
  call; RSYNC: world `rsync_255: n`; `World.env()` sets `PEACOCK_SSH_PAUSES="0 0 0"`),
  `scripts/env/tests/laptop.py` (FAKE_SSH the same; every fake ssh answers `-G` with
  `loglevel <its -o LogLevel=, else INFO>` and does not count it as an attempt, `FAKE_SSH_REFUSALS=<n>` counted in a file),
  `scripts/env/tests/gpu_host.py` (FAKE_NEBIUS: `rsh` exits 255 the first `FAKE_NEBIUS_RSH_FAILS`
  times; `Host.env` sets the pauses to 0)
- Create: `scripts/env/tests/test_run_gpu_vm.py`
- Test: `test_provision.py`, `scripts/nebius/tests/test_nebius.py`, `test_install_watch.py`,
  `scripts/calibration/tests/test_fetch.py`
- Docs: `build-test.md` "The scripts" (ssh retried; `start` bounded by wall clock), "A fresh VM"

**Interfaces:**
- Consumes: Task 3's `ssh_retry`, `sync_retry`. nebius.sh passes `$SSH` (`GPU_HOST_SSH`) as
  `<ssh>`; its host-key repair still greps the helper's stderr, so it keeps working.
- Produces: `nebius.sh rsh`, an rsync's transport, runs plain ssh, not `ssh_retry`: the sync
  around it is retried whole by `sync_retry`, so a pre-auth outage costs at most four attempts,
  not four inside each of four.
- Produces: `cmd_start` waits for ssh until `SECONDS` passes a deadline
  `GPU_HOST_START_TIMEOUT` seconds after the wait began, the helper's retries inside it; the
  `waited += POLL` counter goes.

- [ ] **Step 1: failing tests:**
  - `test_provision.py::test_provision_retries_a_refused_connection`: `FAKE_SSH_REFUSALS=2` → exit
    0, the VM user exists (the root step's heredoc arrived whole), `bootstrap.log` one line.
  - `test_provision.py::test_provision_never_retries_a_session_that_authenticated`: the bootstrap
    call authenticates, then exits 255 → bootstrap ran once, provision exits non-zero.
  - `test_nebius.py::test_ssh_retries_a_refused_connection`: `ssh_refusals: 2` → `ssh true`
    exit 0, three ssh calls.
  - `test_nebius.py::test_a_changed_host_key_is_repaired_without_a_retry`: `ssh: changed` → one
    ssh before the keyscan, one after; exit 0.
  - `test_nebius.py::test_start_bounds_its_ssh_wait_by_the_wall_clock`: ssh down,
    `ssh_seconds: 1`, `GPU_HOST_START_TIMEOUT=3` → exit 1 within 15 s wall; the VM it started
    is stopped.
  - `test_install_watch.py::test_install_watch_retries_an_rsync_that_exits_255`: `rsync_255: 1`
    → exit 0, that rsync called twice.
  - `test_run_gpu_vm.py::test_a_sync_whose_transport_drops_is_retried`: `FAKE_NEBIUS_RSH_FAILS=1`
    → `prepare` exit 0 with its kit on the VM; then `pull` exit 0.
  - `test_fetch.py::test_fetch_over_ssh_retries_a_refused_connection`: a fake `ssh` refusing
    once before authenticating, then printing the pinned record's bytes → exit 0, the record
    checked against its sha.
- [ ] **Step 2: run** each file → FAIL on those.
- [ ] **Step 3: implement.** Source the library in the four scripts.
- [ ] **Step 4: run** every file of the three suites → `0 failed`.
- [ ] **Step 5: report a checkpoint** `vm-setup: every ssh the scripts make goes through the retrying helper`.

### Task 5: the `nebius` CLI on a CPU VM

**Files:**
- Modify: `scripts/env/bootstrap-vm.sh` (after gh); `scripts/env/tests/test_bootstrap.py`
  (`SERVING_CURL` serves `$FAKE_NEBIUS_BINARY` for a URL holding `artifacts.nebius.cloud`, the gh
  tarball otherwise; every PATH in these tests leaves out `nebius`)
- Docs: `build-test.md` "A fresh VM" (`bootstrap-vm.sh` installs it); header of `bootstrap-vm.sh`

**Interfaces:**
- Produces: when `nebius version` on PATH is not `NEBIUS_VERSION`, bootstrap downloads
  `https://artifacts.nebius.cloud/cli/release/<NEBIUS_VERSION>/linux/x86_64/nebius` (the build
  image's URL) to a temp file, checks it against `NEBIUS_SHA256` (else exit 2 naming
  `NEBIUS_SHA256` and the URL), then `sudo install -m 755 <tmp> /usr/local/bin/nebius`.

- [ ] **Step 1: failing tests:** `test_bootstrap_installs_the_pinned_nebius_cli` (whole run, the
  pin rewritten to the served bytes' sha: curl asked for the URL above with `0.12.293`'s pin
  value, sudo log `install -m 755 <tmp> /usr/local/bin/nebius`);
  `test_bootstrap_refuses_a_nebius_cli_of_another_sha` (exit 2, `NEBIUS_SHA256` on stderr, no
  install line); `test_bootstrap_skips_the_nebius_cli_at_its_pin` (a fake `nebius` on PATH prints
  the pin → no curl of the URL, no install).
- [ ] **Step 2: run** `python3 scripts/env/tests/test_bootstrap.py` → FAIL on the three.
- [ ] **Step 3: implement**, beside gh's install and in its idiom (`pin`).
- [ ] **Step 4: run** → `0 failed`.
- [ ] **Step 5: report a checkpoint** `vm-setup: bootstrap-vm.sh installs the pinned nebius CLI`.

### Task 6: the two-profile cut, `nebius-profiles.sh`, one cut for install-watch too

**Files:**
- Create: `scripts/env/nebius-profiles.sh`; `scripts/env/tests/nebius_cli.py` (a fake CLI and a
  fixture writer); `scripts/env/tests/test_nebius_profiles.py`
- Modify: `scripts/lib/peacock-config.sh`; `scripts/nebius.sh` (install-watch on the shared cut,
  its usage lines, `profile_names` gone); `scripts/nebius/tests/fakes.py` (NEBIUS: skips
  `--config`; per-profile key paths, world `profile_key_paths: {name: path}`);
  `scripts/nebius/tests/test_install_watch.py`
- Docs: `build-test.md` "The scripts" (install-watch's key cases: an embedded key goes as it is; a
  key path gets the file it names embedded; `--key-file` names a PEM other than that one; a key
  path naming no file is refused. eu-west1's install-watch line loses `--key-file`), "The human's
  one-time setup" (steps 2 and 4), "Credential files" (the one reader of `~/.nebius/config.yaml`)

**Interfaces:**
- Produces in `peacock-config.sh` (refusals print one line and return 2, as the file's others):
  - `PEACOCK_NEBIUS_CONFIG=$PEACOCK_CONFIG/nebius.yaml`; `PEACOCK_NEBIUS_PROFILES=(editor-eu eu-west1)`,
    the first the active one.
  - `peacock_nebius_cli`: sets `PEACOCK_NEBIUS` to `nebius` on PATH, else `~/.nebius/bin/nebius`;
    else refuses naming `curl -sSL https://storage.eu-north1.nebius.cloud/cli/install.sh | bash`.
  - `peacock_nebius_profiles`: `profile list` on stdin → the names, one a line, in order; returns
    1 printing nothing on a line that is not `<name>` or `<name> [default]`, or a second
    `[default]`. Bash alone (a laptop's tools).
  - `peacock_nebius_cut <cli> <src> <dest> <active> <profile>[=<pem>]…`: copies `<src>` to
    `mktemp "<dest>.XXXXXX"`, mode 600; every CLI call `<cli> --config <copy> --no-check-update …
    </dev/null`, its output dropped but `profile list`'s and `config get`'s. Per listed profile:
    no such profile → refuse naming it; a `<pem>` beside a profile that embeds its key → refuse
    (another PEM would replace a working key); a key path (`config get private-key-file-path`) →
    embed `<pem>` if given, else the file the path names (`profile update <p> --skip-auth
    --endpoint … --service-account-id … --public-key-id … --private-key-file <file>`, then
    `config unset private-key-file-path`); no `<pem>` and a path naming no file → refuse naming
    the profile and the path. Then `profile activate <active>`, `profile delete` of every other name, and the copy
    moved to `<dest>`, mode 600. Sets `PEACOCK_NEBIUS_LEFT_OUT`. Any refusal removes the copy.
    A `profile list` it cannot read is refused naming `<cli> version`'s answer, never the list.
  - `peacock_nebius_config_checked <cli> <file>` (Task 7's): refuses a missing `<file>` naming
    `scripts/env/nebius-profiles.sh`; a list it cannot read, naming the CLI's version; names other
    than `PEACOCK_NEBIUS_PROFILES`, naming each extra or missing; a profile with a key path, naming
    it and `nebius-profiles.sh`.
- Produces `scripts/env/nebius-profiles.sh` (`-h|--help` only): `peacock_nebius_cli`; refuses a
  missing `~/.nebius/config.yaml`; `install -d -m 700 ~/.config/peacock`; `peacock_nebius_cut
  "$PEACOCK_NEBIUS" ~/.nebius/config.yaml "$PEACOCK_NEBIUS_CONFIG" editor-eu editor-eu eu-west1`;
  prints one line, `<dest>: editor-eu (active), eu-west1; <n> other profiles left out`. Exit 0, 2.
- Produces in install-watch: `peacock_nebius_cut <cli fn> "$NEBIUS_CONFIG" "$install_dir/config.yaml"
  "$p" "$p${key:+=$key}"`; `p` is `--profile`'s or `cli profile active`'s. A key-path profile
  without `--key-file` gets the file its path names embedded, as `nebius-profiles.sh` does, where
  it was refused; `--key-file` stays, for a PEM other than that one, and is still refused beside an
  embedded key. A refusal of the cut is install-watch's exit 1, before any ssh or rsync.
- `nebius_cli.py`: `FAKE_NEBIUS_CLI` keeps its state as JSON in the config file it is given
  (`-c`/`--config`, else `$HOME/.nebius/config.yaml`; an unreadable one fails as the CLI would);
  answers `version` (`0.12.293`), `profile list|active|activate|delete|update`, `config get|unset`;
  `update` needs the three ids and an existing `--private-key-file`; logs every argv to
  `FAKE_NEBIUS_LOG`; `FAKE_NEBIUS_LIST` replaces `profile list`'s output; every call prints
  `FAKE-KEY-MATERIAL` on stdout. `nebius_config(path, active, profiles)` writes a fixture,
  `profiles = {name: {"key_path": str | None}}`.

- [ ] **Step 1: failing tests** (`test_nebius_profiles.py`, the source holding `editor-eu`
  embedded, `eu-west1` with a key path to an existing PEM, `ci` and `old`, `old` active):
  - `test_cuts_the_two_profiles_out_of_several`: exit 0; through the fake CLI on the dest:
    `profile list` is `editor-eu [default]`, `eu-west1`; both `config get
    private-key-file-path` empty; the log has `profile update eu-west1 … --private-key-file
    <the PEM>` on the copy; dest mode 600; the source's sha256 unchanged; stdout one line naming
    the two and `2`.
  - `test_refuses_a_missing_profile_naming_it`: no `eu-west1` → exit 2, `eu-west1` on stderr, no
    dest, no `nebius.yaml.*` left.
  - `test_refuses_a_key_path_naming_no_file`: exit 2, stderr names `eu-west1` and the path.
  - `test_a_rerun_replaces_the_file`: a dest of other bytes at mode 644 → sha changed, mode 600.
  - `test_prints_only_the_names_and_a_count`: `FAKE-KEY-MATERIAL` in neither stream.
  - `test_needs_a_cli_naming_the_installer`; `test_takes_the_cli_from_dot_nebius_bin`;
    `test_refuses_a_profile_list_it_cannot_read_naming_the_version` (stderr has `0.12.293`, not
    the list).
  - `test_install_watch.py`: `test_install_watch_refuses_a_key_path_profile_without_a_key_file`
    becomes two. `test_install_watch_embeds_the_pem_a_key_path_names`: `--profile eu-west1`, its
    key path an existing file, no `--key-file` → exit 0; on the copy, `profile update eu-west1 …
    --private-key-file <that path>`, then `config unset private-key-file-path`; the shipped profile
    has no key path. `test_install_watch_refuses_a_key_path_naming_no_file`: the path names no file
    → exit 1, stderr names `eu-west1` and the path, no `update`, no ssh or rsync, no temp dir left.
    `test_install_watch_and_nebius_profiles_share_one_cut` (`profile delete` appears in
    `peacock-config.sh` alone among the three scripts). The other install-watch tests stay green,
    `test_install_watch_refuses_a_key_file_for_a_profile_that_embeds_its_key` among them.
- [ ] **Step 2: run** both files → FAIL.
- [ ] **Step 3: implement.**
- [ ] **Step 4: run** both, and every `scripts/nebius/tests/test_*.py` → `0 failed`.
- [ ] **Step 5: report a checkpoint** `vm-setup: nebius-profiles.sh cuts the two profiles; install-watch shares the cut`.

### Task 7: what the VM holds — `nebius.yaml` checked and shipped; nebius.sh's config and key

**Files:**
- Modify: `scripts/env/provision-vm.sh`, `scripts/nebius.sh`, `.github/actions/nebius-setup/action.yml`
- Modify fakes: `scripts/env/tests/laptop.py` (the laptop has `FAKE_NEBIUS_CLI` on PATH, a
  valid `~/.config/peacock/nebius.yaml` of the two embedded profiles, and `~/.nebius/config.yaml`
  at mode 000 in every test)
- Test: `test_provision.py`, `scripts/nebius/tests/test_nebius.py`, `test_watch_workflow.py`
- Docs: `build-test.md` "A fresh VM" (what provision copies, read from its code: the PAT into
  `gh auth login` on stdin, kept by gh; `peacock.env`, the GPU key pair and `nebius.yaml` as files
  on ssh's stdin, mode 600), "The env file" (the VM's is the laptop's, no `GPU_SSH_KEY_FILE`),
  "Trust" (the VM holds the two service accounts' keys, with their rights), "The scripts"
  (`--config`), "CI and its secrets"

**Interfaces:**
- Consumes: Task 6's `peacock_nebius_cli`, `peacock_nebius_config_checked`, `PEACOCK_NEBIUS_CONFIG`.
- Produces in nebius.sh (sources `lib/peacock-config.sh`):
  `NEBIUS_CONFIG=${GPU_HOST_NEBIUS_CONFIG:-$PEACOCK_NEBIUS_CONFIG}`; a missing file → exit 1
  before any call, stderr naming `scripts/env/nebius-profiles.sh`; `cli()` passes
  `--config "$NEBIUS_CONFIG"`; `ssh_opts` adds `-i "$PEACOCK_GPU_KEY"` when `GPU_HOST_KEY` is
  unset and that file exists.
- Produces in provision-vm.sh: in "What this machine must hold", `peacock_nebius_cli` and
  `peacock_nebius_config_checked "$PEACOCK_NEBIUS" "$PEACOCK_NEBIUS_CONFIG"`; the secrets' stage
  gains `nebius.yaml` (a `cp`, never read); the remote stops appending `GPU_SSH_KEY_FILE`.
- Produces in the action: `echo "GPU_HOST_NEBIUS_CONFIG=$HOME/.nebius/config.yaml" >> "$GITHUB_ENV"`.

- [ ] **Step 1: failing tests:**
  - `test_provision.py`: `test_provision_ships_nebius_yaml_on_stdin_mode_600` (the VM's
    `~/.config/peacock/nebius.yaml` mode 600, sha256 equal to the laptop's; no argv holds it);
    `test_provision_refuses_a_missing_nebius_yaml_naming_nebius_profiles`,
    `…_a_nebius_yaml_with_another_profile`, `…_a_nebius_yaml_lacking_a_profile`,
    `…_a_profile_that_reads_its_key_from_a_file`, `…_without_a_nebius_cli_naming_the_installer`,
    `…_a_profile_list_it_cannot_read_naming_the_version` (each `test_provision_refuses_…`: exit 2,
    no ssh call, no `github-pat` written); `test_provision_reads_no_cli_config_of_its_own` (a whole
    run with `~/.nebius/config.yaml` at 000; every fake CLI call names `--config <nebius.yaml>`);
    `test_the_vm_env_file_is_the_laptop_s` (bytes equal, no `GPU_SSH_KEY_FILE`).
    `test_provision_readies_a_vm` loses any `GPU_SSH_KEY_FILE` expectation.
  - `test_nebius.py`: `test_every_cli_call_names_the_config`;
    `test_a_missing_config_is_refused_naming_nebius_profiles` (exit 1, no CLI call);
    `test_the_default_config_is_peacock_s` (no `GPU_HOST_NEBIUS_CONFIG`, a temp HOME holding
    `.config/peacock/nebius.yaml` and `.nebius/config.yaml` at 000 → `status` exit 0, calls name the
    first); `test_the_peacock_gpu_key_is_the_identity_without_gpu_host_key`;
    `test_gpu_host_key_wins_over_the_peacock_key`.
  - `test_watch_workflow.py::test_action_names_the_config_it_makes`.
- [ ] **Step 2: run** → FAIL on those.
- [ ] **Step 3: implement.**
- [ ] **Step 4: run** `test_provision.py`, every `scripts/nebius/tests/test_*.py` → `0 failed`.
- [ ] **Step 5: report a checkpoint** `vm-setup: the VM holds nebius.yaml and reads its GPU key; nebius.sh names its config`.

### Task 8: container paths; a tree the image's home hides is refused

**Files:**
- Modify: `scripts/lib/machine.sh`, `scripts/build-test.sh`
- Modify fakes: `scripts/env/tests/machine.py` and `gpu_host.py` FAKE_DOCKER map a container path
  to its host path through the `--mount … dst=` that holds it, and refuse, exit 126, a `dst` or
  `-w` under a home directory, as the image refuses its `/home/peacock` (mode 750, uid 1000) to
  another uid
- Test: `test_bootstrap.py`, `test_run_gpu_vm.py`, `test_build_test.py`
- Docs: `build-test.md` "What a VM holds" (registry, data), "Local build workflows and caches"

**Interfaces:**
- Produces in machine.sh: `MACHINE_TREE=/peacock/tree`, `MACHINE_STAGE=/peacock/stage`.
  `machine_cargo_fetch` mounts the tree read-only at `$MACHINE_TREE`, `-w $MACHINE_TREE`;
  `machine_data_set` mounts the stage at `$MACHINE_STAGE`, `-w $MACHINE_STAGE`, and runs
  `bash $MACHINE_STAGE/generate_testdata.sh`. Used by `bootstrap-vm.sh` and `run-gpu.sh prepare`.
- Produces in build-test.sh, before any side effect: the tree under
  `${PEACOCK_IMAGE_HOME:-/home/peacock}/` and `id -u` not 1000 → exit 2: the image's home is mode
  750, uid 1000, so this uid cannot reach a tree mounted there; the tree keeps its own path because
  `cpp/build`'s CMake cache and `compile_commands.json`, which host tools read, name it.
  `PEACOCK_IMAGE_HOME` is the tests' knob alone.

- [ ] **Step 1: failing tests:** `test_bootstrap_fetches_the_registry_with_the_clone_read_only`
  now expects `type=bind,src=<clone>,dst=/peacock/tree,readonly` and `-w /peacock/tree`;
  `test_bootstrap.py::test_the_generator_runs_at_a_container_path` (each generator run:
  `dst=/peacock/stage`, `-w /peacock/stage`, `/peacock/stage/generate_testdata.sh`);
  `test_run_gpu_vm.py::test_prepare_generates_sf1_at_a_container_path`;
  `test_build_test.py::test_refuses_a_tree_under_the_image_home_for_another_uid` (fake `id`
  giving 1001 → exit 2 before any docker call, stderr names the home and `1000`) and
  `test_a_tree_under_the_image_home_is_taken_for_uid_1000`.
- [ ] **Step 2: run** the three files → FAIL on those.
- [ ] **Step 3: implement.**
- [ ] **Step 4: run** the three, `test_run_gpu.py`, `test_check_build_image.py` → `0 failed`.
- [ ] **Step 5: report a checkpoint** `vm-setup: the build image reaches the tree and the stage at container paths`.

### Task 9: the run image by its ID; every card's driver

**Files:**
- Modify: `scripts/env/run-gpu.sh`; `gpu_host.py` (FAKE_DOCKER renders a `--format` of several
  `{{.Id}}`, `{{index .Config.Labels "<l>"}}` and `{{"\n"}}` fields; the run image's world labels
  gain `peacockdb.driver-minimum-ptx: 575.51.03`; fake `nvidia-smi` answers
  `--query-gpu=driver_version,compute_cap --format=csv,noheader` from world
  `cards: [["550.54.15", "8.9"]]`, one line a card, and nothing else)
- Test: `test_run_gpu_vm.py`; `test_run_gpu.py` (its driver tests take `cards`)
- Docs: `build-test.md` "The scripts" (exit 3), "Images" (the two driver labels)

**Interfaces:**
- Consumes: Task 1's label.
- Produces `run_image_read`: one `docker image inspect --format` on the VM of `RUN_REF`, setting
  `RUN_ID` (`sha256:<64 hex>`), and the labels `peacockdb.inputs`, `peacockdb.driver-minimum`,
  `peacockdb.driver-minimum-ptx`; a label missing → exit 2 naming it and `scripts/env/image.sh`;
  the inputs compared with `image_inputs_differ` as `machine_image_label_checked` does.
- Produces `cards_checked` (replaces `driver_checked`): one `nvidia-smi --query-gpu=driver_version,compute_cap
  --format=csv,noheader`; per line `<driver>, <major>.<minor>`, else exit 2 naming the line.
  `major` < 8 → exit 3 naming the capability; the need is the image's minimum, or for a major other
  than 8 or 9 the higher of it and the PTX minimum; a driver below → exit 3 naming both and why.
- Produces: `run` and `run <staged>` give the VM `RUN_ID`, never the ref. `prepare` pulls by ref,
  then `run_image_read` and `cards_checked`.

- [ ] **Step 1: failing tests** (`test_run_gpu_vm.py`):
  `test_run_runs_the_image_id_its_labels_were_read_from` (the device `docker run`'s image is
  `sha256:run1…`; one inspect of the ref per `run`); `test_every_card_s_driver_is_held_to_the_minimum`
  (`[["550.54.15","8.9"],["530.30.02","8.9"]]` → exit 3, stderr `530.30.02` and `535.104.05`);
  `test_a_card_below_compute_capability_8_is_refused` (`7.5` → exit 3, `7.5` on stderr);
  `test_a_card_past_9_needs_the_driver_that_compiles_its_ptx` (`10.0` at `560.35.03` → exit 3
  naming `575.51.03` and PTX; at `575.51.03` → passes; `9.0` at `560.35.03` → passes);
  `test_a_driver_answer_that_is_no_version_is_refused` (exit 2, the line on stderr);
  `test_a_run_image_without_the_ptx_minimum_is_refused` (exit 2, the label and `image.sh`).
  Ported from `test_gpu.py`: `test_run_refuses_old_driver`,
  `test_a_card_past_compute_capability_9_needs_the_driver_that_compiles_its_ptx`,
  `test_a_driver_answer_that_is_no_version_is_refused`, `test_the_image_run_is_the_one_whose_label_was_read`.
- [ ] **Step 2: run** `python3 scripts/env/tests/test_run_gpu_vm.py` → FAIL on those.
- [ ] **Step 3: implement.**
- [ ] **Step 4: run** `test_run_gpu_vm.py`, `test_run_gpu.py` → `0 failed`.
- [ ] **Step 5: report a checkpoint** `vm-setup: run-gpu.sh runs the image by the ID it checked and holds every card to its driver`.

### Task 10: the runner — `follow`, `cancel`, `--init`, one card lock

**Files:**
- Modify: `scripts/env/run-gpu.sh` (VM side: `vm_runner`, `vm_idle` replacing `vm_not_running`,
  `vm_run`, `vm_run_staged`, `vm_status`, `vm_follow`, `vm_cancel`; dev side: `cmd_follow`,
  `cmd_cancel`, an INT/TERM trap on an attached `run` or `follow`, the usage text);
  `scripts/lib/peacock-config.sh` (the card lock); `scripts/create_nsys_profile.sh`
  (`remote_pass` takes the lock); `scripts/nebius.sh` (`cmd_stop`'s probe)
- Modify fakes: `gpu_host.py` (a device `docker run` without `-d`: the container `running` in the
  world while `<tmp>/hold` exists, then the world's output and `run_code`; `docker stop` ends it
  with 143; `Host.second_user()` with another VM home sharing the world, data and lock;
  `__exit__` kills every runner its `run/state` files name), `fakes.py` (SSH answers the stop
  probe's lock check from world `card_lock: "held"`)
- Create: `scripts/env/tests/test_run_gpu_runs.py`
- Test: `test_run_gpu.py` (reshaped: `docker_runs()` means a run with `--gpus`; the run and status
  tests from line 558 on, and the staged binary's, to the runner's shape), `test_nebius.py`
- Docs: `build-test.md` "The scripts" and "The H100's device run" (verbs, the runner, the lock)

**Interfaces:**
- Produces in `peacock-config.sh`: `PEACOCK_CARD_LOCK=${PEACOCK_CARD_LOCK:-/run/lock/peacock-card.lock}`
  (the tests' knob; the dev side passes the value to the VM as an argument) and
  `peacock_card_take <lock>`: makes the file mode 666 when missing (umask 000), opens it on fd
  `PEACOCK_CARD_FD`, and when it is held prints `waiting for the card: another run holds <lock>`
  and waits in the background, so a TERM trap runs at once; returns once held.
- Produces, per VM dir `~/peacock/<i>/`: `run/state` = `<runner pid> <boot id> <what> <stamp>`
  (`<what>` the mode or the staged name; boot id `/proc/sys/kernel/random/boot_id`;
  stamp `date -u +%Y%m%dT%H%M%S.%3NZ`); `run/status` = `<stamp> <code>`; `run/lock`, flocked
  around a run's start; `logs/<what>-<stamp>.log`. A run *goes* while the state's boot id is this
  boot's and `kill -0 <pid>` holds.
- Produces `vm_run`: under `run/lock`, `vm_idle` (exit 2 while the dir's run goes, naming
  `run-gpu.sh … follow`; exit 2 for a container `peacock-<i>` with no runner, naming `cancel`),
  then the state, then `setsid nohup bash -c '<runner text>' … </dev/null >/dev/null 2>&1 &` (every
  fd redirected: the session's pipes must close), then `vm_follow`, or with `--detach` return 0
  naming `follow` and `status`.
- Produces `vm_runner <i> <image id> <what> <stamp> <lock> <docker run argument>…`: output to the
  log; `peacock_card_take`; `docker run --init --name peacock-<i> --gpus all …` attached, in the
  background; then `docker rm peacock-<i>`; a staged binary judged as `vm_run_staged` judges now;
  `run/status`. Its TERM trap: `run-gpu.sh: cancelled` in the log, the card wait given up or
  `docker stop -t 30 peacock-<i>` until the client ends, `docker rm -f`, then the status.
- Produces `follow` (`vm_follow <i>`): `tail -n +1 --pid=<runner> -f <log>`, then exit as `run`
  exits for that stamp's status (0 and 2 as is, else 1); no state → exit 2 naming `status`; no
  status → exit 1 "ended with no status". An attached `run`'s or `follow`'s ssh lost (255) or an
  INT/TERM on dev → stderr names `run-gpu.sh … follow`; exit 1, or 130/143.
- Produces `cancel` (`vm_cancel <i>`): a going runner gets TERM and is waited for → `cancelled the
  <what> run started <stamp> (exit <code>)`, or `… ended by itself …`; no runner but a container
  `peacock-<i>` → removed, said; else `no run of <i> goes`. Exit 0; 1 when the run ended with no
  status or the container stays.
- Produces `status`: first line `none` (2), `waiting` (5: the runner goes, no container yet),
  `running` (5), `exited <code>` (0 or 1), `ended without a status` (1); then the log's last 40.
- Produces: `create_nsys_profile.sh`'s `remote_pass` heredoc carries `$(declare -f
  peacock_card_take)` and takes the lock before `nsys profile`, held to the end of the pass.
  `nebius.sh stop`'s probe prints `card lock <lock> held` when the file exists and
  `flock -n <lock> true` fails → busy, exit 2.

- [ ] **Step 1: failing tests** (`test_run_gpu_runs.py`):
  - `test_a_run_outlives_its_session_and_is_followed`: an attached held `run list`; its process
    group killed; the runner goes; `follow` prints the log from its first line; released, `follow`
    exits the run's code.
  - `test_an_interrupted_attached_run_goes_on_and_names_follow`: SIGINT → exit 130, stderr
    `follow`, `status` exit 5.
  - `test_every_run_has_an_init`: `--init` in the device run's argv, a staged binary's too.
  - `test_run_while_the_dir_s_run_goes_is_refused_naming_follow`: exit 2, one device run.
  - `test_two_runs_started_at_once_in_one_dir_start_one`: two `run --detach list` at once → one
    0, one 2, one device run.
  - `test_a_run_from_before_a_restart_is_not_going`: a state of another boot id and a live pid →
    `status` `ended without a status`, exit 1; `run` starts.
  - `test_cancel_stops_the_run_and_keeps_what_an_ended_run_keeps`: exit 0, stdout `cancelled`,
    the log ends `run-gpu.sh: cancelled`, `docker stop` then `docker rm` of `peacock-<i>`,
    `status` `exited 143`.
  - `test_cancel_ends_a_run_still_waiting_for_the_card`: never a device run for it; its log has
    the waiting line, then `cancelled`.
  - `test_cancel_removes_a_container_a_killed_runner_left`; `test_cancel_with_nothing_going_says_so`;
    `test_cancel_leaves_other_dirs_alone`.
  - `test_one_card_lock_for_every_dir_and_user`: dir A held; dir B's run and a second user's
    wait (log and `status` `waiting`); A released → B runs, then the other.
  - `test_the_card_lock_is_mode_666`: made under umask 022.
  - `test_a_staged_binary_runs_under_the_runner_and_is_judged`: a segfault line → exit 1.
  - `test_nsys_capture_waits_for_the_card_lock`: the lock held by a test process → no `nsys` call
    until it is let go.
  - `test_nebius.py::test_soft_stop_refused_while_the_card_lock_is_held`: exit 2, the lock named;
    `stop --force` stops.
  Ported from `test_gpu.py`/`test_gpu_locks.py`: `test_one_device_run_at_a_time_per_host`,
  `test_a_dropped_session_leaves_one_run_that_a_rerun_follows`, the five `test_cancel_*`,
  `test_a_run_that_ends_before_it_is_followed_gives_its_status`,
  `test_a_container_of_the_instance_still_on_the_host_refuses_a_new_run`,
  `test_the_shared_files_stay_group_writable`,
  `test_a_run_whose_instance_is_busy_is_neither_started_nor_followed`. The hold, heartbeat, ship
  server, install-lock and power cases are not ported (no such design here); the detail file's
  `## Ported and dropped cases` says so, case by case.
- [ ] **Step 2: run** `python3 scripts/env/tests/test_run_gpu_runs.py` and `test_nebius.py` →
  FAIL on those.
- [ ] **Step 3: implement**, and reshape `test_run_gpu.py`'s run tests to the runner.
- [ ] **Step 4: run** `test_run_gpu*.py`, `test_bench_each.py`, `scripts/nebius/tests/test_*.py` → `0 failed`.
- [ ] **Step 5: report a checkpoint** `vm-setup: every run is held by a runner on the VM; follow, cancel and one card lock`.

### Task 11: one tree per VM dir

**Files:**
- Modify: `scripts/env/run-gpu.sh`, `.gitignore` (`/.run-gpu-tree`)
- Create: `scripts/env/tests/test_run_gpu_records.py`
- Docs: `build-test.md` "The scripts" (`--dir`, the tree's id, `PEACOCK_TREE_ID`)

**Interfaces:**
- Produces `tree_id_read`: `$PEACOCK_TREE_ID` when set (`[A-Za-z0-9._-]+`, no file read or
  written); else `<root>/.run-gpu-tree`, two lines, an id (32 hex from `/dev/urandom`) and the
  tree's `pwd -P`, made by the first call that claims a dir. A file whose path is not this tree's
  → exit 2 before any VM call: `.run-gpu-tree was written for <path>, not this tree: a copy or a
  moved tree needs an id of its own: remove .run-gpu-tree, then name a dir of its own with --dir
  <name>`.
- Produces `vm_tree_claimed <i> <id>`, the first VM step of `prepare`, `ship`, `run`, `pull` and
  `cancel` (before any rsync, docker run, pull or clear): a missing dir is made with its `.tree`;
  a dir without `.tree` is claimed (`set -C`); another id → exit 2 `~/peacock/<i> on the VM is
  another tree's: name a dir of your own with --dir <name>`. `status` and `follow` claim nothing.

- [ ] **Step 1: failing tests:** `test_a_tree_records_its_id_and_its_dir_on_the_vm`;
  `test_a_copied_tree_is_refused_its_record_before_any_vm_call` (no nebius call; stderr names
  `.run-gpu-tree` and `--dir`); `test_another_tree_s_call_is_refused_before_it_ships_runs_pulls_or_clears`
  (a second tree of the same name: `ship`, `run list`, `pull`, `pull --clear`, `cancel` each exit 2
  naming `--dir`, no rsync of the dir, no device run, nothing removed);
  `test_a_dir_made_before_tree_is_claimed_by_the_first_tree`;
  `test_a_fixed_tree_id_owns_its_dir_and_writes_no_record` (`PEACOCK_TREE_ID=ci --dir ci` from two
  fresh trees → both accepted, no `.run-gpu-tree`); `test_status_and_follow_claim_nothing`.
  Ported from `test_gpu_records.py`: `test_a_tree_records_its_instance_and_later_commands_take_it`,
  `test_a_copied_tree_is_refused_the_record_it_carries`,
  `test_a_second_tree_under_a_taken_name_is_refused_on_the_host`.
- [ ] **Step 2: run** `python3 scripts/env/tests/test_run_gpu_records.py` → FAIL.
- [ ] **Step 3: implement.**
- [ ] **Step 4: run** `test_run_gpu*.py` → `0 failed`.
- [ ] **Step 5: report a checkpoint** `vm-setup: a VM dir is one tree's`.

### Task 12: records and logs under a stamp; ship's stamp; `pull`, `pull --clear`; bench-each

**Files:**
- Modify: `scripts/env/run-gpu.sh` (the runner keeps the record; `vm_records_kept`; `cmd_ship`'s
  stamp, `vm_shipped`; `cmd_pull`, `vm_pull_list`, `vm_stamped`; `vm_cleared`; the usage lines of
  `ship` and `pull`), `scripts/env/bench-each.sh`; `test_bench_each.py`'s FAKE_RUN_GPU (a bench
  run's record as `records-<stamp>.tsv`; `pull --clear` copies and clears as below)
- Test: `test_run_gpu_records.py`, `test_bench_each.py`, `test_run_gpu.py`
  (`test_pull_brings_the_results_the_record_and_the_log_home` reshaped)
- Docs: `build-test.md` "The scripts" (ship's stamp; pull's rule), "The H100's device run"
  (records, bench-each), the record paragraphs naming `records.tsv` (lines near 2304, 2352, 3278,
  3350 today)

**Interfaces:**
- Produces: when a run ends, any way, the runner moves `testdata/calibration/records.tsv` to
  `records-<stamp>.tsv`, before its status. `vm_records_kept <i>`, in `run` before the start and in
  `pull`, while no run goes: a `records.tsv` → `records-<the state's stamp>.tsv`, or with no state
  the file's mtime as a stamp.
- The pulled set: the VM's `testdata/benchmark-results/`, `testdata/calibration/` but
  `records.tsv`, the goldens' `gpu-result*.txt`, and `logs/`, whose home is
  `~/.cache/peacock/run-gpu/<i>/`.
- Produces in `ship`: once every stream is done, `vm_shipped <i>` takes on stdin the testdata
  stream's paths that lie in the pulled set and writes `~/peacock/<i>/.shipped` whole, through a
  temp file: `<sha256>  <path under ~/peacock/<i>/>` of the VM's copy of each. A failed ship
  leaves the stamp before it. The stamp is content, not a time: rsync gives a shipped file dev's
  mtime, and a file a run wrote before a ship was never shipped.
- Produces `pull [--clear]`: `--clear` while the dir's run goes → exit 2 naming `follow`, before
  any transfer. `vm_pull_list <i>` prints `<sha256> <stamped sha256, or -> <path>` for each file
  of the pulled set whose sha256 is not its `.shipped` line's. A file whose sha is the stamped one
  is the copy the stamp names (shipped, or brought home by a pull), and is skipped without a word
  whatever its local time. The listed files come to a temp dir (`from_vm … --files-from`), then
  each: no local copy → moved in; the local bytes the VM's → nothing; the local bytes the stamped
  ones (unchanged here) → replaced; else the local copy stays, stderr `run-gpu.sh: pull: kept
  <path>: it changed here and on the VM`, and the exit is 1 once the rest is home. A local
  `records-*.tsv` that differs from the VM's is never replaced, stamped or not. Then
  `vm_stamped <i>` takes on stdin `<sha256>  <path>` of each listed file now the same on both
  sides and, under `run/lock`, sets its `.shipped` line (making the file when there is none), so a
  later run's change to a file pulled home is taken, not named: bench-each's next section in the
  same `<mode>.benchmark.txt`. A dir with no `.shipped` (shipped before this branch) lists every
  file with `-`. With `--clear`, then: `<sha256> <path under ~/peacock/<i>/>`
  for each pulled record and log, of its local copy, on ssh's stdin to `vm_cleared <i>`, which under
  `run/lock` removes each file of that sha, names each it keeps, and never touches `records.tsv`.
- Produces in bench-each.sh: no ssh for records. Before the loop, and per case after `run bench`:
  `run-gpu.sh pull --clear`, then each `testdata/calibration/records-*.tsv` new since before the
  pull set aside as today (`<dir>/<ds>-<q>-<mode>.<attempt>.tsv` by its last row's case; one with
  no row removed). Exits as today; a pull's non-zero stops it with that exit (1: a file changed
  here and on the VM, which the pull named).

- [ ] **Step 1: failing tests** (`test_run_gpu_records.py`): `test_each_run_logs_under_its_stamp`;
  `test_each_bench_run_keeps_its_own_record` (two benches → two `records-<stamp>.tsv`, the second
  not refused); `test_a_cancelled_or_killed_bench_keeps_its_record` (cancel; and `run_code` 137);
  `test_a_record_left_by_a_runner_that_died_is_kept_under_its_run_s_stamp`;
  `test_a_record_from_before_the_runner_is_kept_under_its_time`;
  `test_pull_never_replaces_a_local_record_that_differs` (exit 1, named, bytes unchanged);
  `test_ship_stamps_what_it_shipped` (`.shipped` holds each committed file of the pulled set with
  the VM copy's sha256, and no file a run wrote before the ship; a ship whose stream fails leaves
  the stamp before it);
  `test_pull_skips_a_shipped_file_a_checkout_touched` (after `ship`, the local copy of a committed
  `benchmark-results` file given other bytes and an mtime an hour on → exit 0, stderr empty, the
  local bytes unchanged);
  `test_pull_names_a_file_the_run_changed_that_differs_here` (after `ship`, the VM's copy
  rewritten and the local one edited → exit 1, stderr names the path, both copies unchanged; the
  local mtime set to 2001-09-09 → exit 1 again);
  `test_pull_says_nothing_of_an_unchanged_identical_file` (`ship`, then `pull` → exit 0, stderr
  empty, the local mtime unchanged);
  `test_pull_takes_each_section_a_run_adds_to_a_shipped_file` (a committed `<mode>.benchmark.txt`
  appended on the VM and pulled, twice → exit 0 both times, the local bytes the VM's);
  `test_a_file_a_run_wrote_before_a_ship_still_comes_home` (a new result and a
  `records-<stamp>.tsv`, then `ship`, then `pull` → both home);
  `test_a_dir_with_no_ship_stamp_names_each_local_file_that_differs` (no `.shipped`: a differing
  local copy named, exit 1; an absent one taken; an equal one silent);
  `test_pull_clear_removes_what_came_home_and_keeps_the_rest` (records and logs of equal sha gone
  from the VM; an edited local copy's record kept and named; `records.tsv` left);
  `test_pull_clear_is_refused_while_a_run_goes` (exit 2, no rsync). `test_bench_each.py`:
  `test_bench_each_moves_each_record_aside` reshaped (the run's `records-<stamp>.tsv`; the VM's
  cleared by `pull --clear`), `test_bench_each_sets_aside_a_record_a_killed_run_left` reshaped,
  `test_bench_each_removes_no_vm_record_by_hand` (no `nebius.sh ssh` naming `records`).
  Ported from `test_gpu_records.py`: `test_each_bench_run_keeps_its_own_record`,
  `test_a_killed_bench_s_record_comes_home`, `test_pull_clear_removes_the_host_s_records_that_are_home_and_keeps_the_rest`,
  `test_pull_clear_is_refused_while_the_instance_s_run_goes`,
  `test_the_host_s_clear_keeps_records_tsv_and_what_did_not_come_home`,
  `test_pull_brings_the_results_home_and_keeps_a_newer_local_file` (reshaped to the stamp: an edit
  here after the pull is kept without a word while the VM's copy stands; once a run changes the
  VM's, exit 1 naming it, whatever the local mtime).
- [ ] **Step 2: run** `test_run_gpu_records.py`, `test_bench_each.py` → FAIL on those.
- [ ] **Step 3: implement.**
- [ ] **Step 4: run** `test_run_gpu*.py`, `test_bench_each.py` → `0 failed`.
- [ ] **Step 5: the file's length.** `wc -l scripts/env/run-gpu.sh` above 1000 → move the
  "the VM's side" section whole to `scripts/env/run-gpu-vm.sh` (`# shellcheck shell=bash`, sourced
  by run-gpu.sh, its functions still sent as text by `on_vm`; `gpu_host.TREE_FILES` gains it), a
  move with no other change; run Step 4 again.
- [ ] **Step 6: report a checkpoint** `vm-setup: each run keeps its log and record under its stamp; pull skips what ship stamped, keeps a file changed on both sides, and clears what is home`.

### Task 13: CI's `gpu-tests` names a fixed tree and cleans up with `cancel`

**Files:**
- Modify: `.github/workflows/pipeline.yml` (`gpu-tests`: job `env: PEACOCK_TREE_ID: ci`; the
  cleanup step runs `scripts/env/run-gpu.sh --instance-id computeinstance-e00tnrse7ayntnzcyt --dir ci
  cancel || echo "cancel failed (rc=$?)"`, then the soft stop; the `docker rm -f peacock-ci` line
  and its comment go)
- Modify: `cost-report/tests/test_source_formatting/ci_gpu.rs`
- Docs: `build-test.md` the gpu-tests description (near line 1893) and the CI wiring guard row

**Interfaces:**
- Produces `ci_gpu.rs::the_gpu_job_owns_its_dir_and_cancels_its_run` (replacing
  `the_gpu_job_owns_its_dir_and_removes_its_container`): the run step's `--dir` is not the
  checkout's and every `run-gpu.sh` call names it (kept); a step whose `if:` starts `always()` runs
  `run-gpu.sh … --dir <dir> cancel`; any `docker rm|kill|stop` names `peacock-<dir>` alone (kept);
  the job's `env:` sets `PEACOCK_TREE_ID` to a literal, no `${{`. Doctored cases: no cancel, cancel
  of another dir, cancel under `success()`, cancel commented out, no `PEACOCK_TREE_ID`, one from
  `${{ github.run_id }}`, and the kept ones.

- [ ] **Step 1: failing test** as above.
- [ ] **Step 2: run** `scripts/build-test.sh -- cargo test -p cost-report --test
  test_source_formatting ci_gpu` (a tree under `/build`, Task 2's image) → FAIL.
- [ ] **Step 3: implement** the job and the guard; run the whole `test_source_formatting` → PASS.
- [ ] **Step 4: report a checkpoint** `vm-setup: gpu-tests names its tree and cancels its run on every outcome`.

### Task 14: one short table of command lines, each run as written

**Files:**
- Modify: `llm-wiki/build-test.md` "Local build workflows and caches" (the table), `README.md`
  (a short "Building and testing" section linking that table and "A fresh VM")
- Modify, for each line that fails: `scripts/build-test.sh`, with a case in `test_build_test.py`

**Interfaces:**
- Produces the table's rows (spec item 6), each a `scripts/build-test.sh` line:
  - rebuild after a source change: rust-only `--rust-only --build`; cpu `--build` (the C++ library
    and the FFI); the gpu install `--gpu --build`; another cuDF `--cudf 26.02 --build`, after
    `rm -rf cpp/build cpp/install` (both are stamped by version and refused otherwise);
  - run a category with what was built: rust-only `--rust-only --run`; cpu `--run` (FFI and C++
    linked, `ctest -L cpu`);
  - one test, and a filter: `-- cargo test --features rust-only -p peacockdb-core --test <target>
    -- <test>`; `-- cargo test -p peacockdb-ffi --test test_ffi -- <test>`; `-- ctest --test-dir
    cpp/build -R <name>`; `PCK_TEST_FILTER=<f> scripts/build-test.sh --rust-only --run`.

- [ ] **Step 1:** write the table and the README section.
- [ ] **Step 2: run every line once as written**, in a tree under `/build` with Task 2's image,
  with real targets and test names: from the host, then as
  `scripts/build-test.sh -- bash -c '<the line>'` (a shell in the build image). Record each in the
  detail file's `## The table, run as written`: line, where, exit, wall time.
- [ ] **Step 3: each failing line**, test first: a case in `test_build_test.py` → FAIL; fix
  `build-test.sh`; → PASS; rerun the line. The first suspect is a `-- ctest` or an FFI `-- cargo
  test` with no `/opt/cudf/lib` on the loader path: then
  `test_a_command_has_cudf_s_libraries_on_the_loader_path` (inside the image, `-- env` shows
  `LD_LIBRARY_PATH` starting `/opt/cudf/lib`, a caller's value after it), and the export moves
  above the command's `exec`.
- [ ] **Step 4: run** `python3 scripts/env/tests/test_build_test.py` → `0 failed`.
- [ ] **Step 5: report a checkpoint** `vm-setup: one table of build and test lines, each run as written`.

### Task 15 *(coordinator)*: publish the branch's images; the PR is green

Needs GitHub alone: the branch pushed with Task 1's commit. It may run as soon as that commit is
pushed, and must before any CI run on the branch is read or any VM pulls an image.

- [ ] **Step 1:** `gh workflow run images.yml --ref ENS-vm-setup -f kind=all -f cudf=all` → five
  jobs green (build 25.02, 25.10, 26.02; run 25.02, 26.02).
- [ ] **Step 2:** for each, `docker buildx imagetools inspect
  ghcr.io/asymptote-tech/peacockdb-<kind>:$(bash -c '. scripts/lib/image-inputs.sh; image_inputs_tag
  . <kind> <v>')` → a digest; `…:cudf-<v>` keeps its digest from before. Record the five.
- [ ] **Step 3:** the PR's CI green on the branch's last commit; a red job reaches the developer
  as a failing test.

### Task 16 *(coordinator)*: nebius-gpu by hand; CI's `gpu-tests` through the runner

Needs: Task 15; dev's `~/.config/peacock/nebius.yaml`, made on dev with
`scripts/env/nebius-profiles.sh` before this task (from this branch on, `nebius.sh` on dev reads
it); nebius-gpu, the L40S; the sandbox off; an actor in `GPU_ALLOWED_ACTORS`.

- [ ] **Step 1:** `test -f ~/.config/peacock/nebius.yaml`, then `scripts/nebius.sh status` →
  exit 0. In the `/build` tree: `scripts/build-test.sh --gpu --build --no-release-bench`.
- [ ] **Step 2:** `scripts/nebius.sh start`; with `G="scripts/env/run-gpu.sh --instance-id
  computeinstance-e00tnrse7ayntnzcyt"`: `$G prepare --no-sf40`; `$G ship`; `$G run list`, its local
  process killed once the log runs; `$G status` → `running`; `$G follow` → the log from its first
  line; `$G cancel` → `cancelled …`; `$G status` → `exited …`.
- [ ] **Step 3:** `PCK_TEST_FILTER=<one corpus query> $G run filter` → exit 0;
  `$G pull --clear` → the run's log and record home, gone from the VM
  (`scripts/nebius.sh ssh 'ls ~/peacock/<dir>/logs'`); `scripts/nebius.sh stop`.
- [ ] **Step 4:** `gh workflow run pipeline.yml --ref ENS-vm-setup` → `gpu-tests` green: its log
  shows `run-tests.sh list: green`, the cleanup's `cancel` saying no run goes, and the VM stopped.
- [ ] **Step 5:** record every command, exit and wall time in `## nebius-gpu by hand`, and the VM
  times. A failure reaches the developer as a failing test; the fix is checkpointed and the step
  run again.

### Task 17 *(coordinator)*: a fresh verda CPU VM, provisioned from this branch

Needs: the verda CPU VM, with only Claude Code installed and logged in; its address, its login
user and the user to provision, which come with it and are assumed nowhere before; Tasks 15 and
16; the branch pushed. On dev, `~/.config/peacock/` as it stands, holding `github-pat` before the
run, made with the recipe `provision-vm.sh` prints (under `</dev/null` the run cannot ask for
one), and `nebius.yaml`. Claude Code's login is never run, and the login user's own Claude Code is
left as it was.

- [ ] **Step 1:** `test -s ~/.config/peacock/github-pat && test -s ~/.config/peacock/nebius.yaml`
  (presence alone, never read); a missing PAT is made first, in a terminal, with the recipe
  `scripts/env/provision-vm.sh --new-pat` prints. Then from dev, sandbox off: `time
  scripts/env/provision-vm.sh <login>@<address> --user <user> --ref ENS-vm-setup </dev/null`
  (`<user>` the user to provision) → exit 0, ending in either "Claude Code is logged in on
  <user>@<address>" or the command that logs it in; record which.
- [ ] **Step 2:** the same command again → exit 0, each done step skipped; record both wall times.
- [ ] **Step 3:** as the developer on the VM: `cd ~/peacockdb && scripts/nebius.sh --instance-id
  computeinstance-e01t2mm4265y8x88ey --profile eu-west1 status` → a state; `scripts/build-test.sh
  --rust-only --build --run` → green.
- [ ] **Step 4:** `## A fresh VM` in the detail file: each step's wall time, every refusal hit and
  its fix. A refusal that needs code goes to the developer as a failing test, the fix is
  checkpointed, and the run after it is recorded too.

---

## Self-review

- **Spec coverage.** Item 1: Tasks 1, 2. Item 2: Tasks 3, 4 (provision, `vm_ssh`, `fetch_record.sh`;
  `run-gpu.sh`, `check-build-image.sh`, `bench-each.sh`, `watch.sh` through `nebius.sh`; `to_vm`,
  `from_vm`, install-watch's rsyncs; `start`'s wall clock). Item 3: Task 5. Item 4: Task 7 (the
  key read, the dead line gone, the docs). Item 5: Tasks 6, 7 (the cut, the check, the ship,
  `--config`, CI's action, mode-000 tests). Item 6: Task 14. Item 7: Tasks 8–13 (run survives,
  follow, cancel, `--init`, runner; card lock, nsys, soft stop; tree id; stamps, ship's stamp,
  pull, clear, bench-each; image ID, per-card driver, PTX pin from Task 1; container paths, the `/home/peacock`
  refusal; CI) and Task 16 (by hand, `gpu-tests`). Item 8: Task 17. Constraints: Global
  Constraints, Task 15.
- **Names used across tasks:** `ssh_retry`, `sync_retry`, `PEACOCK_SSH_PAUSES`;
  `PEACOCK_NEBIUS_CONFIG`, `PEACOCK_NEBIUS_PROFILES`, `peacock_nebius_cli`/`PEACOCK_NEBIUS`,
  `peacock_nebius_profiles`, `peacock_nebius_cut`/`PEACOCK_NEBIUS_LEFT_OUT`,
  `peacock_nebius_config_checked`; `PEACOCK_CARD_LOCK`, `peacock_card_take`/`PEACOCK_CARD_FD`;
  `MACHINE_TREE`, `MACHINE_STAGE`, `PEACOCK_IMAGE_HOME`; `run_image_read`/`RUN_ID`,
  `cards_checked`; `run/state`, `run/status`, `run/lock`, `vm_idle`, `vm_runner`, `vm_follow`,
  `vm_cancel`; `tree_id_read`, `.run-gpu-tree`, `.tree`, `PEACOCK_TREE_ID`, `vm_tree_claimed`;
  `vm_records_kept`, `vm_cleared`, `records-<stamp>.tsv`, `logs/<what>-<stamp>.log`; `.shipped`,
  `vm_shipped`, `vm_pull_list`, `vm_stamped`.
