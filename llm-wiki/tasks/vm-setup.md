# A developer's VM, end to end

Kind: production

**This task closes no ticket.**

A developer gets a CPU VM from `provision-vm.sh`, builds there in the build image, and runs on a
Nebius GPU VM through `run-gpu.sh`. Each step has a gap:

- **The images build only through `image.sh`.** `docker build -f docker/run.Dockerfile .` stops at
  the parse: with no `CUDF`, `FROM run-` names no stage. With `CUDF` alone it stops at the
  `DRIVER_MINIMUM_25_02` check, and `build.Dockerfile` fails installing `gcc-`. Every version comes
  from `docker/pins.env`, and only `image.sh` passes it. An empty `CUDF_SUBMODULE` does not fail at
  all: `git fetch <url> ""` exits 0, so the prefetch would take cuDF's default branch.
- **One dropped connection fails a long step.** No ssh is retried. Only ship's rsync streams and
  `nebius-sync.sh` retry.
- **A provisioned VM cannot reach Nebius.** It has no `nebius` CLI and no profile. The GPU key
  `provision-vm.sh` puts there is read by nothing: `GPU_SSH_KEY_FILE` lost its reader with the
  earlier containers branch's `gpu.sh`, and `nebius.sh` reads `GPU_HOST_KEY`.
- **`run-gpu.sh` lacks five things that earlier branch had** (`4e785104`: `scripts/env/gpu.sh`,
  `on-gpu-host.sh`, `scripts/lib/machine.sh`). Item 7 lists them.
- **The command lines a developer needs** are spread over `build-test.md`, and some were never run
  as written.

## Done when

1. **A plain `docker build` builds either image.** `docker build -f docker/<kind>.Dockerfile .` from
   the repository's root, with `--build-arg CUDF=<v>` or without it for 25.02, builds the image
   `image.sh build <kind> <v>` builds, layer for layer, less the labels `image.sh` adds.
   - Each Dockerfile gives every `pins.env` pin it declares a default equal to the pin, on the `ARG`
     that declares it, so each image is still hashed over the same pins (`image_inputs`). `CUDF`
     defaults to 25.02, the scripts' default. `CUDF_SUBMODULE` defaults to the commit the index
     pins for `third_party/cudf`.
   - `test_pins.py` holds every default equal to its pin, and `CUDF_SUBMODULE`'s to the index. A pin
     declared without a default fails it.
   - `image.sh` still passes every pin. `pins.env` stays what a published image is built from; the
     defaults serve the plain build alone.
   - A plain build has no `peacockdb.inputs` label, so `build-test.sh`, `run-gpu.sh` and
     `bootstrap-vm.sh` still refuse it, naming `image.sh`. `build-test.md` ("Rebuilding and
     publishing") says so beside the command.
   - Proof, on dev: a plain build of each kind for 25.02, right after `image.sh`'s, takes every step
     from the cache, and the two images' `RootFS.Layers` are equal. A build-only version's run image
     still stops at its `FROM`.
   - Why defaults rather than one file read by the build: `FROM`, `ENV` and `LABEL` cannot read a
     file, and a `COPY` of `pins.env` would make any pin change rebuild the 7.5 GB cuDF layer.
2. **ssh is retried when the connection fails, never once the command may have started.** One
   helper in `scripts/lib/` runs ssh with its own log (`-E`, `LogLevel=VERBOSE`) and connection
   sharing off (`ControlPath=none`), so every call authenticates for itself. It retries only an
   exit 255 whose log has no `Authenticated to` line: the remote command cannot have started. A
   255 after that line is the command's own or a dropped session, and is returned, never retried. A
   host-key refusal is not retried either; `nebius.sh` repairs it, only when the attempt had not
   authenticated: after that the same text is the command's own. Four attempts, 5, 10 and 20 s
   apart, each named on stderr. After the last one, ssh's own lines reach stderr as they would
   without `-E`. A retry's stdin reaches the remote whole: on a refused connection ssh reads none of
   it (measured).
   - Every ssh the scripts make goes through it, shad-gpu's aside: `provision-vm.sh` (`on`,
     `remote_test`, the login), `nebius.sh`'s `vm_ssh`, and `fetch_record.sh --ssh`. `run-gpu.sh`,
     `check-build-image.sh`, `bench-each.sh` and `watch.sh` reach it through `nebius.sh`.
   - An rsync's transport (`nebius.sh rsh`) is one attempt through the helper, never retried
     itself, so its host-key repair follows the same rule. A sync is safe to run again whole, so
     `to_vm`, `from_vm` and `install-watch`'s rsyncs are retried, within the same bound, on 255 and
     on 30, rsync's own I/O timeout, which their `--timeout=90` reaches before ssh notices a
     silently dead route. Ship's streams and `nebius-sync.sh` keep their own retries.
   - `nebius.sh start`'s wait for ssh is a retry loop of its own. It keeps `GPU_HOST_START_TIMEOUT`
     as a wall-clock bound, with the helper's retries inside it.
   - Tested over a fake ssh that writes the log, and over the real ssh against a closed local port:
     four attempts, and the stdin intact.
3. **The `nebius` CLI on a CPU VM.** `bootstrap-vm.sh` installs `/usr/local/bin/nebius` at
   `NEBIUS_VERSION`, from the URL the build image takes, checked against `NEBIUS_SHA256`. It skips
   the install when that version is there. `nebius.sh` on the VM finds it on PATH.
   `test_bootstrap.py` covers the install, a wrong checksum and the skip.
4. **`~/.config/peacock/` stays the one place, on the laptop and on the VM.** What
   `provision-vm.sh` copies today, read from its code: the PAT goes into `gh auth login` on stdin,
   and gh keeps it; it is not a file on the VM. `peacock.env`, `gpu_ed25519` and its `.pub` go as
   files, on ssh's stdin, mode 600. The gap is the key, which nothing on the VM reads. `nebius.sh`
   adds `~/.config/peacock/gpu_ed25519` as an identity when `GPU_HOST_KEY` is unset and that file
   exists. `provision-vm.sh` stops writing the dead `GPU_SSH_KEY_FILE` line. `build-test.md` ("A
   fresh VM", "The env file") says what the VM holds.
5. **Two Nebius profiles on the VM, never another.** The developer's
   `~/.config/peacock/nebius.yaml` holds `editor-eu` and `eu-west1` and nothing else, each with its
   key embedded, `editor-eu` the active one. These are the service accounts the developer's own CLI
   already uses; "Trust" says a VM holds their keys, with those accounts' rights.
   - **`scripts/env/nebius-profiles.sh` cuts the file** from the developer's `~/.nebius/config.yaml`,
     the way `install-watch` cuts its one profile: a temp copy, mode 600; a profile whose key sits in
     a file (`config get private-key-file-path`; eu-west1 today) gets that file embedded and the path
     dropped; `editor-eu` activated; every other profile deleted; the copy moved into place, mode
     600. A rerun replaces the file. It refuses, naming the profile, when either profile is missing
     or a key path names no file. It prints only the two names and how many profiles it left out.
     `install-watch`'s cut and this one are one function, which keeps a list of profiles.
   - `provision-vm.sh` checks the file before anything is written, with the laptop's own `nebius`
     (on PATH, or `~/.nebius/bin/`). It refuses one that is missing (naming `nebius-profiles.sh`),
     one whose `nebius --config <file> profile list` names another profile or lacks one of the two,
     and one with a profile that still reads its key from a file, which would not work on the VM.
     No CLI on the laptop, or a `profile list` of a shape the parser does not know, is refused too,
     naming the installer or the CLI's version. The file reaches the VM's
     `~/.config/peacock/nebius.yaml` with the other secrets, on ssh's stdin, mode 600. `profile
     list`'s output has one parser, shared with `install-watch`.
   - `nebius.sh` gives every CLI call `--config` with `GPU_HOST_NEBIUS_CONFIG`, else
     `~/.config/peacock/nebius.yaml`, on every machine. A missing file is refused, naming
     `nebius-profiles.sh`. `install-watch` cuts its copy from the same file; both keys are embedded
     there, so eu-west1's line in `build-test.md` loses its `--key-file`. A profile that still
     reads its key from a file, in a config `GPU_HOST_NEBIUS_CONFIG` names, gets that file embedded
     by the same cut instead of a refusal; `--key-file` stays, for a PEM other than the one the
     profile names. CI's `nebius-setup` sets `GPU_HOST_NEBIUS_CONFIG` to the config it makes.
   - `~/.nebius/config.yaml` is read by `nebius-profiles.sh` alone, through the CLI, on a copy.
     The tests run `provision-vm.sh` and `nebius.sh` with that file at mode 000. The cut's tests use
     a fake CLI: two profiles kept out of several, one embedded and one from a file, a missing
     profile, a missing PEM, a rerun.
6. **One short table of command lines, each run as written.** The table in `build-test.md`'s "Local
   build workflows and caches" becomes the one a developer needs. The root `README.md` gains a short
   section that points to it and to "A fresh VM". Its rows:
   - rebuild after a source change: rust-only, cpu (the C++ library and the FFI), the gpu install,
     and another cuDF version, with what moving `cpp/build` to it takes;
   - run a category with what was built: rust-only, and cpu (FFI and C++ linked, `ctest -L cpu`);
   - one test, and a filter: a Rust target or test, an FFI test, a C++ test, and `PCK_TEST_FILTER`
     across a tier.

   Each line works from the host and from a shell in the build image
   (`scripts/build-test.sh -- bash`). `build-test.sh` already provides them
   (`--rust-only|--gpu`, `--build`, `--run`, `-- <cmd>`). Each is run once on this branch as
   written. A line that fails is fixed in the script, not worded around, and `test_build_test.py`
   gains a case for the fix. The first suspect: `-- ctest`, or `-- cargo test` of an FFI target,
   with no `/opt/cudf/lib` on the loader path.
7. **`run-gpu.sh` gains what the earlier branch had and it lacks, test first.** The reference is
   `4e785104`'s `on-gpu-host.sh` and `gpu.sh`, and their tests `test_gpu.py`, `test_gpu_records.py`
   and `test_gpu_locks.py`. Their cases are ported where the shape still applies.
   - **A run survives a dropped ssh, is followed again, and is cancelled.** Exists: every run is a
     `docker run -d` container, `peacock-<i>`, so it outlives the session; `run --detach`;
     `status`. Lacks: a `follow` verb (the going run's log from its start, then live; its exit the
     run's); `cancel`; and `--init`, without which `run-tests.sh`, as PID 1, ignores `docker stop`'s
     TERM. Every run, a staged binary's too, is held by a runner on the VM, detached from the
     session. On `cancel`'s TERM the runner gives up its wait for the card or stops the container
     (`docker stop`, then `docker rm`). It keeps what any ended run keeps, and writes `cancelled` in
     the log. `run` while the dir's run goes is still refused, now naming `follow`. CI's
     `gpu-tests` cleans up with `cancel`.
   - **One card lock for every user of the VM, and one tree per VM dir.** Exists: `vm_not_running`,
     per dir only, so two dirs share the card and fail each other's pools. Lacks: a lock every user
     of the VM can take whatever group owns the data directory (in `/run/lock`, mode 666, as the
     earlier branch's install lock), held by the runner for the container's whole life. A run that
     waits says so in its log and in `status`. `create_nsys_profile.sh` takes the same lock around
     each capture. `nebius.sh stop`'s soft check counts a held lock as busy. And the dir: `--dir`
     defaults to the tree's name, so two clones named `peacockdb` share `~/peacock/peacockdb/`.
     The dir records the id of the tree that made it, in `.tree`. The tree keeps its id and its
     path in a git-ignored file, so a copied tree is refused its record, naming the fix. Another
     tree's call is refused before it ships, runs, pulls or clears, naming `--dir`. A dir made
     before `.tree` is claimed by the first tree to reach it. CI's checkout is new on every run, so
     its job names a fixed id and `~/peacock/ci/` stays CI's.
   - **Records and logs under a timestamp, never overwritten by a pull, cleared once home.** Exists:
     `pull` with `rsync --update`; one log per mode, overwritten by the next run; a bench's
     `records.tsv` left in place, which `run-tests.sh` then refuses for the next bench until someone
     removes it. Lacks: each run's log as `logs/<mode>-<stamp>.log`, and its record as
     `records-<stamp>.tsv` once it ends, a killed or cancelled run's too. A pull never replaces a
     local record that differs. `ship` leaves a stamp in the dir, the sha256 of each file it
     shipped, and every pull adds what it brought home. A file the VM has not changed since its
     stamp is skipped without a word, whatever its local time, since a checkout resets that. One
     the VM changed is brought home over a local copy that still matches the stamp. One changed on
     both sides is a conflict: the local copy stays, and the pull names it and exits 1. `pull --clear`
     then removes from the VM each record and log whose local copy has the same sha256; it is
     refused while a run goes, and never clears `records.tsv`.
     `bench-each.sh`, which today moves the pulled `records.tsv` aside and removes the VM's copy by
     hand, takes the run's `records-<stamp>.tsv` and clears with `pull --clear`; its tests follow.
   - **The image run by its ID, and the driver checked per card.** Exists: the label checked on the
     tag, then `docker run` of the tag; the first card's driver alone. Lacks: the ID and the labels
     read in one call and the run given that ID, so a tag that moved in between is not run. Every
     card's driver is held to the image's minimum, and a card below compute capability 8 is refused.
     A card other than 8.x or 9.x JIT-compiles the PTX, so its driver is also held to
     `DRIVER_MINIMUM_PTX`, a new pin: the driver of `NVCC_VERSION`'s CUDA, 575.51.03 for 12.9, in
     `pins.env` and `test_pins.py`. Exit 3 as today.
   - **Container paths, so a uid other than 1000 can build.** Exists: `machine_cargo_fetch` and
     `machine_data_set` mount the tree and the data being made at their host paths. Such a path can
     cross a directory of the image the uid cannot enter: `/home/peacock`, mode 750, uid 1000. Lacks:
     `/peacock/tree` and `/peacock/stage` as their container paths, as the earlier branch had. Both
     `bootstrap-vm.sh` and `run-gpu.sh prepare` use these steps. `build-test.sh` keeps the tree at
     its own path, because `cpp/build`'s CMake cache and `compile_commands.json`, which host tools
     read, name it. It refuses up front, saying why, a tree under `/home/peacock` for a uid other
     than 1000.
   - Once on nebius-gpu, by hand: a detached `run list` whose ssh is killed, followed, then
     cancelled; a `run filter`, pulled with `--clear`. CI's `gpu-tests` is green through the runner.
8. **A fresh CPU VM, provisioned from this branch.** This is the last step, run once items 1 to 7
   are in and green. The VM is a verda CPU VM with only Claude Code installed and logged in. Its
   address, its login user and the user to provision come with it; nothing before then assumes
   them. The PAT is in dev's `~/.config/peacock/github-pat` before the run, made by the recipe
   `provision-vm.sh` prints, since under `</dev/null` the run cannot ask for one. From dev, with
   dev's `~/.config/peacock/` as it stands:
   `scripts/env/provision-vm.sh <login>@<address> --user <the user> --ref <this task's branch> </dev/null`,
   end to end, then once more to show that a rerun skips what is done. Then, as the developer on
   the VM: `scripts/nebius.sh --instance-id <nebius-cpu128's id> --profile eu-west1 status`
   answers, and the table's rust-only build and run pass.
   - Claude Code's login is never run: only the account owner can log in. With no terminal,
     `provision-vm.sh` reports either that Claude Code is logged in for the developer's user, or the
     command that logs it in, and exits 0. Either is a pass, and the detail file says which. The
     login user's own Claude Code is left as it was.
   - The detail file records each step's wall time and every refusal hit, with its fix. A refusal
     that needs a code change gets one, with a test, and the run after the fix is recorded too.

## Scope

| path | change |
|---|---|
| `docker/build.Dockerfile`, `docker/run.Dockerfile` | the pins' defaults |
| `docker/pins.env`, `scripts/env/tests/test_pins.py` | `DRIVER_MINIMUM_PTX`; the defaults held to the pins and the index |
| `scripts/lib/`: the ssh helper (new), `peacock-config.sh`, `machine.sh` | the retry; the nebius config's path and the profile check; the container paths |
| `scripts/env/nebius-profiles.sh` (new) | the two-profile cut |
| `scripts/env/provision-vm.sh`, `scripts/env/bootstrap-vm.sh` | the retry, `nebius.yaml`, the dead key line; the CLI |
| `scripts/nebius.sh`, `scripts/calibration/fetch_record.sh` | the retry; `--config`; the key; the soft stop's lock check |
| `scripts/env/run-gpu.sh`, and the VM's side in a file of its own if it would pass 1000 lines | item 7 |
| `scripts/env/bench-each.sh`, `scripts/create_nsys_profile.sh` | the new records and pull; the card lock |
| `scripts/build-test.sh` | the table's fixes; the `/home/peacock` refusal |
| `.github/actions/nebius-setup/action.yml`, `.github/workflows/pipeline.yml` | `GPU_HOST_NEBIUS_CONFIG`; `gpu-tests`' fixed tree id and `cancel` |
| `cost-report/tests/test_source_formatting/` | the CI wiring guard, where it reads the lines `gpu-tests` changes |
| `.gitignore` | the tree's id file |
| `scripts/env/tests/`, `scripts/nebius/tests/` | the tests named above |
| `README.md`, `llm-wiki/build-test.md` | the table; "A fresh VM", "Trust", "The env file", "Nebius host", "Rebuilding and publishing" |

Component-level API: none.

## Constraints

- No engine change, and no test is weakened.
- Never print, `cat`, `grep` or parse a Nebius credential file: `~/.nebius/config.yaml`, its PEMs,
  the developer's `nebius.yaml`, and their copies. Only the CLI reads them, and only its answers to
  `profile list`, `profile active` and `config get` (the key path, the endpoint, the ids) are
  used. The tests use a fake CLI.
- Secrets travel on ssh's stdin alone: never in an argv, never in a file outside
  `~/.config/peacock/`. That directory keeps its name.
- The Dockerfiles' change moves every image's inputs tag. So `images.yml` publishes the branch's
  images, dispatched on the branch, before anything there pulls them: CI, `run-gpu.sh prepare`, and
  the trial's `bootstrap-vm.sh`.
- From this branch on, `nebius.sh` on dev reads `~/.config/peacock/nebius.yaml`. That file is made
  on dev with `nebius-profiles.sh` before the first dispatch.
- Out of scope: the agent-container launcher (`up.sh`, `down.sh`); shad-gpu and its scripts; the
  GPU VMs' idle watcher.
