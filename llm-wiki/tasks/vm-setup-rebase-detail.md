# vm-setup-rebase — detail

## Run log

- 2026-11-13: coordinator started; board to `building`. Plan Task 1 step 1 (the replay,
  `git merge-recursive 1eaefe39 -- HEAD e0612b23`) run by the coordinator on the working tree;
  the developer resolves the conflicts (Task 1 step 2-3), then the coordinator commits as Roman.

## Moved tests

Taken out of `scripts/env/tests/test_run_gpu.py` by the replay (master's `# --- Sirius` block,
`git show HEAD:scripts/env/tests/test_run_gpu.py`, from the line `SIRIUS_DRIVER = "580.173"` up to
`MOBY_DEFAULT_SHA256`, along with its helpers `SIRIUS_DRIVER`, `RUNNER`, `sirius_prepared` and
`sirius_runs`). Task 3 brings each back in `test_run_sirius.py`, `run-gpu.sh sirius` read as
`run-sirius.sh run` and `prepare --sirius` as `run-sirius.sh prepare`:

- `test_prepare_sirius_pulls_the_sirius_image_and_ships_its_profile`: prepare pulls the Sirius
  image last, after run and build, and ships `docker/sirius-seccomp.json` byte for byte.
- `test_prepare_sirius_short_of_disk_stops_before_the_pull_naming_both_numbers`: 8 GB free under
  Docker's root is refused, exit 2, naming `/var/lib/docker` and `needs 14`, and nothing pulled.
- `test_prepare_sirius_with_the_image_on_the_vm_measures_no_disk`: with the image already on the VM,
  a short disk is not refused.
- `test_prepare_sirius_refuses_a_driver_older_than_the_sirius_image_needs`: driver 550.54.15
  against the image's 580.65.06, exit 3, naming both.
- `test_prepare_sirius_refuses_a_sirius_image_of_other_inputs`: an inputs label with
  `docker/sirius-bench.py` changed is refused, exit 2, naming the file and the ref.
- `test_prepare_sirius_without_sf40_is_refused`: `--sirius --no-sf40` exits 2 before any nebius
  call. (With `run-sirius.sh prepare` there is no `--no-sf40`; the intent becomes: Sirius prepare
  needs sf40, so it never skips it.)
- `test_sirius_runs_the_runner_in_the_sirius_image_with_no_install`: the container's flags (seccomp
  profile under the dir, memlock -1, the VM's hostname, `--gpus all`, `--name peacock-<i>`, label
  `peacock.log=sirius`, data read-only at its own path, testdata as `/peacock/testdata`, no
  `--privileged`/`--cap-add`); its command `python3 /opt/sirius/sirius-bench.py <args> --data
  <data>/tpch.sf40 --out /peacock/testdata`; the queries shipped; no `cpp/install` made; no pull.
- `test_sirius_ships_the_sirius_results_a_resume_keeps`: `--resume` ships the H100 results tree's
  `sirius.tsv`, and the flag reaches the runner.
- `test_sirius_ships_the_answers_a_narrowed_or_resumed_run_keeps`: a whole run is sent no answers
  database; `--list` and `--resume` are sent this tree's; a newer one on the VM is kept.
- `test_sirius_detached_is_judged_by_status_and_pulled`: `--detach` runs `-d`, `status` reads
  `exited 0`, and `pull` brings `sirius.log` home. (Under the runner the log becomes
  `logs/sirius-<stamp>.log`, spec item 3.)
- `test_sirius_exits_as_the_runner_did`: runner exits 0, 1, 2, 139 give 0, 1, 2, 1.
- `test_sirius_before_prepare_is_refused`: no prepared Sirius image, exit 2 naming `prepare
  --sirius` (to become `run-sirius.sh prepare`), and no container run.
- `test_sirius_arguments_are_refused_before_any_call`: no `--`, no `--dataset`, no `--sf`, a given
  `--out` or `--data`, and `--detach` without `--`, each exit 2 naming the word, with no nebius call.

Kept in `test_run_gpu.py`, by judgement:
`test_the_sirius_profile_is_dockers_default_and_one_rule_for_six_calls` (and `MOBY_DEFAULT_SHA256`,
`SIRIUS_CALLS`). It reads `docker/sirius-seccomp.json` alone, never `run-gpu.sh`, so nothing in it
collides with vm-setup, and dropping it would lose coverage for no reason. Task 3 may move it to
`test_run_sirius.py`, beside the profile's user.

Brought back in Task 3, all 14 in `scripts/env/tests/test_run_sirius.py` (the profile test left
`test_run_gpu.py`). Common rewrites: `run-gpu.sh sirius` is `run-sirius.sh run`, and `prepare
--sirius` is `run-gpu.sh prepare` (with sf40) then `run-sirius.sh prepare`. The world's `driver=` is
`cards=[[580.173, 9.0]]`. A Sirius run is found by the image ID `sha256:sirius1`, not the tag,
since the run uses the ID. Each rewrite beyond those:

- `test_prepare_pulls_the_sirius_image_and_ships_its_profile`: same assertions.
- `test_prepare_short_of_disk_…`, `test_prepare_with_the_image_on_the_vm_measures_no_disk`: the
  short disk is set after `run-gpu.sh prepare`, so only Sirius's prepare meets it.
- `test_prepare_refuses_a_driver_older_…`, `test_prepare_refuses_a_sirius_image_of_other_inputs`:
  same words and exits.
- `test_prepare_without_sf40_is_refused` became
  `test_prepare_on_a_vm_without_sf40_is_refused_naming_run_gpu_prepare`. `run-sirius.sh prepare`
  has no `--no-sf40`. Its intent, that Sirius runs over sf40, is now a refusal, exit 2, of a VM
  `run-gpu.sh prepare --no-sf40` readied, naming `tpch.sf40` and `run-gpu.sh … prepare`, with no
  Sirius pull. (It used to make no nebius call, since it was a pure argument check. It now asks
  the VM.)
- `test_run_runs_the_runner_in_the_sirius_image_with_no_install`:
  - `--label` is the runner's `peacock.dir=<dir>`, not `peacock.log=sirius`.
  - `cpp/install` now exists, made by the user and empty, where it was absent. vm_started makes it
    so Docker cannot make it as root (spec item 3). The intent "no install" is kept: it is empty.
  - Flags, mounts and command unchanged.
- `test_run_ships_the_sirius_results_a_resume_keeps`, `test_run_ships_the_answers_a_narrowed_or_resumed_run_keeps`:
  same assertions. `world(containers={})` between runs is gone, because the runner removes its
  container.
- `test_run_detached_is_judged_by_status_and_pulled`:
  - The container is no longer `docker run -d`; the runner holds it. Asserted instead: the
    detached line names `follow`, the state's what is `sirius`, and status is `exited 0`.
  - `pull` brings `logs/sirius-<stamp>.log` home to `~/.cache/peacock/run-gpu/<i>/`, where it was
    `sirius.log`.
- `test_run_exits_as_the_runner_did`: same table, 0/1/2/139 to 0/1/2/1.
- `test_run_before_prepare_is_refused`: names `run-sirius.sh prepare`, where it was `prepare
  --sirius`.
- `test_run_arguments_are_refused_before_any_call`: same cases; each refusal also names
  `run-sirius.sh`.
- `test_the_sirius_profile_is_dockers_default_and_one_rule_for_six_calls`: unchanged.

## Deferred (spec item 9)

- a Sirius run on the H100 through the runner;
- vm-setup's by-hand nebius-gpu items;
- CI's `gpu-tests` through the runner.

## Replay (plan Task 1)

- `git merge-recursive 1eaefe39 -- HEAD e0612b23` run on top of the `building` board commit;
  exit 1. Conflicts: `run-gpu.sh`, `gpu_host.py`, `test_pins.py`, `test_run_gpu.py`,
  `build-test.md`, `tickets.md`, `tickets/testinfra.md`, `tasks.md` (as the plan expected).
- Coordinator resolved: `tasks.md` (master's side; vm-setup's entry added under chain N as
  task 2, `done`), `tickets.md` and `tickets/testinfra.md` (both sides; #312 #314 #315 beside
  #311; 98 open, next free 316, recounted from the files).
- Left to the developer with the code: `build-test.md`'s two hunks (grand total; the Python
  table's Calibration, Nebius, Images and DuckDB rows), since the rows describe tests.
- Drift on master, not this task's: #311 is used twice, `tickets/benchmarks.md` (Sirius planning
  time) and `tickets/testinfra.md` (LIMIT without ORDER BY). For the human or helper to renumber.
- Developer resolved (Task 1 step 2), each staged:
  - `scripts/env/run-gpu.sh`: vm-setup's file whole, byte for byte `e0612b23`'s. Every line master
    added since `1eaefe39` is Sirius (the verb, `prepare --sirius`, `SIRIUS_*`, `sirius_image`,
    `prepare_sirius`, `vm_docker_free`, `vm_sirius`, `cmd_sirius`, the seccomp profile in `KIT`,
    the usage lines), or else `driver_checked`'s per-image form, which only Sirius used. vm-setup's
    card check replaces it. The goldens directory in the pull set: vm-setup's `in_pulled_set`
    and `run-gpu-vm.sh` already carry it. No Sirius word is left in the file.
  - `scripts/env/tests/test_run_gpu.py`: the one conflict was the imports, where vm-setup's side
    won (no `SIRIUS_CUDF`; `until`, `NO_SPACE`). Master's Sirius block merged in cleanly and was
    taken out by hand, all but the seccomp profile's test: see `## Moved tests`.
  - `scripts/env/tests/gpu_host.py`: both. `docker info`: vm-setup's `info_delays` first, then
    master's `DockerRootDir` answer, then the runtimes. The world: master's `absent`,
    `docker_root` and `free_bytes`, with vm-setup's `cards` in place of the old `driver`. Master's
    `df` fake, `SIRIUS_CUDF`, the `cudf` argument to `label`/`inputs_ref`, and the Sirius image
    in the world merged in cleanly.
  - `scripts/env/tests/test_pins.py`: both, one after the other: master's mirror and venv-lock
    tests, then vm-setup's Dockerfile-defaults tests.
  - `llm-wiki/build-test.md`: grand total, the higher side of each count: Rust 6355 (master),
    C++ 286, Python 1877 (vm-setup), total 8487 (vm-setup). The parts sum to 8518, not 8487, so
    the line disagrees with itself until Task 4 recounts. The Python table: master never changed
    the Calibration, Nebius or DuckDB rows from the base, so vm-setup's text is taken there. The
    Images row is merged word by word against the base. Both sides' inserted text is kept,
    vm-setup's `ssh-retry.sh`/`nebius-profiles.sh` first, then master's Sirius image,
    `sirius-bench.py` and `bench-sirius.sh` text. The file list is the union, and the count is 518
    (vm-setup) over 413. Judgement: master's sentence on `run-gpu.sh prepare --sirius` and
    `sirius` sat inside the conflict hunk and describes tests this commit removes. It is replaced
    by one clause for the profile test that stays. Task 4 writes the `run-sirius.sh` text.
  - Leftover outside a conflict hunk, for Task 4: `build-test.md` still names `run-gpu.sh sirius`
    / `prepare --sirius` at lines 1709, 2400, 2757 and 3832-3845 (as of the replay's working
    tree), "Sirius on the H100" among them.
- Step 3 suites (2026-11-13, dev): the plan's loop, `needs_docker` and `on_runner` skipped, each
  file under `timeout 900 python3 <f>`. 43 files ran, 8 skipped: 41 passed, 2 failed. Both failures are
  the dev host's, not the merge's:
  - `scripts/env/tests/test_run_gpu_vm.py`,
    `test_a_failing_nvidia_smi_is_refused_naming_its_answer` (13/14). Dev has a real
    `/usr/bin/nvidia-smi` with no working driver: it exits 9, "NVIDIA-SMI has failed…". The fake
    VM's PATH ends in dev's own PATH, so when the test deletes the fake `nvidia-smi` to get exit
    127, the real one answers. The file is vm-setup's unchanged. It passes 14/14 with
    `nvidia-smi` hidden from PATH, and 14/14 under `scripts/build-test.sh -- python3 …`, which is
    how CI runs it. A test that leans on the host lacking `nvidia-smi`: it could pin the fake VM's
    PATH to the fakes and a bare system set. Not fixed in the replay, which carries vm-setup's
    work alone.
  - `scripts/calibration/tests/test_plot.py`: dev's python3 has no matplotlib. CI runs it in the
    build image (pipeline.yml says so). There it passes, 7/7.
  - `test_bench_sirius.py` 12/12 and `test_sirius_bench.py` 33/33 pass. `bash -n` is clean on
    the 15 changed `.sh` files. `image_inputs . sirius 26.08` has the same sha256 on HEAD and on
    the replay.

## Task 2 (the shared client library; the runner's two hooks)

- `scripts/lib/gpu-vm-client.sh` (new): moved from `run-gpu.sh` unchanged, `vm`, `on_vm`,
  `on_vm_with`, `VM_HELPERS`, `in_pulled_set`, `sha256_of`, `to_vm`, `from_vm`, `following`,
  `followed_no_more`, `docker`, `tree_id_read`, `tree_claimed`, `version_below`. New:
  `vm_options_checked` (the `--dir` and `--instance-id` checks), `image_read <ref> <kind> <cudf>`
  (sets `IMAGE_ID`, `IMAGE_MINIMUM`, `IMAGE_MINIMUM_PTX`; the PTX label is read and required for
  kind `run` alone, else empty), and `cards_checked <ref>` (the PTX rule skipped when
  `IMAGE_MINIMUM_PTX` is empty). It sources peacock-config, image-inputs, data-inputs, machine,
  ssh-retry and `run-gpu-vm.sh` itself, and sets `MACHINE_NEBIUS`.
- `run-gpu.sh`: sets `TREE_RECORD`/`tree_path`, sources the library, then `vm_options_checked`,
  the `--cudf` spec check, `tree_id_read`. Option checks now run dir, instance, cudf (cudf was
  second); no test saw a difference. `run_image_checked` = `image_read "$RUN_REF" run "$cudf"`,
  `RUN_ID=$IMAGE_ID`, `cards_checked "$RUN_REF"`, in place of the three `run_image_read;
  cards_checked` pairs.
- Messages in the moved code still say `run-gpu.sh:` (the dropped-ssh line, the card refusals,
  `followed_no_more`'s `run-gpu.sh … follow`, which is right for Sirius too: `follow` stays a
  `run-gpu.sh` verb). `image_read`'s missing-image refusal still says "run prepare first". Task 3
  decides what `run-sirius.sh` should say there.
- `run-gpu-vm.sh`: `vm_started <i> <what> <card lock> <judged> <docker run argument>…`, and
  `vm_runner` takes `<judged>` after `<card lock>`, in place of its `case $what in
  list|filter|bench`. `vm_run` passes 0, `vm_run_staged` 1. `vm_started` makes `cpp/install` and
  also `testdata` before the runner starts: both are binds, and Docker would make either as root
  on a dir no ship made. The plan named `cpp/install` alone; `testdata` is the same hazard
  (master's `cmd_sirius` made it with a `mkdir -p` before its `docker run`).
- Tests:
  - `gpu_host.py`: `Host.vm_call(fn, *args)` runs one `run-gpu-vm.sh` function on the fake VM.
    `vm_started` on an unshipped dir cannot be reached through `run-gpu.sh`, whose `run` refuses
    first.
  - The fake `docker run` now makes a missing `-v` source, as Docker does, and names it in the
    world's `root_made` (made as root on a real VM). Without it the cpp/install test could not
    fail.
  - `test_run_gpu_runs.py::test_a_run_on_a_dir_never_shipped_leaves_cpp_install_the_user_s` and
    `test_run_gpu_vm.py::test_the_runner_judges_only_what_it_is_told_to`. Both were seen failing
    first, for the right reasons: `root_made` held the dir; judged 0 ended 1.
  - The fake VM's PATH: `FAKE_HOST_PATH` is `host_bin` and then `machine.path_without(tmp,
    {'nvidia-smi'})`, which links every command on dev's PATH but `nvidia-smi`. So
    `test_a_failing_nvidia_smi_is_refused_naming_its_answer`'s deleted fake gives exit 127 on any
    host. Seen failing on dev before (dev's real `/usr/bin/nvidia-smi` exits 9), 14/14 after. The
    test itself is unchanged.
- CI: `pipeline.yml`'s `Nebius script tests` step lints `scripts/lib/gpu-vm-client.sh` too. The
  YAML parses, and the step's `run:` passes `bash -n`. shellcheck is not on dev or in the build
  image, so the lint itself is unproven until CI.
- Suite (2026-11-13, dev): the plan's loop, 43 files, 41 passed. The run-gpu files: test_run_gpu
  76/76, \_records 28/28, \_runs 27/27, \_vm 15/15; test_bench_each 17/17, test_bench_sirius
  12/12. The two failures are host-only:
  - `test_plot.py` (no matplotlib on dev): 7/7 under `scripts/build-test.sh -- python3 …`.
  - `test_embeddings.py` 3/7: dev's `/` is full (174G, 934M free, 100%), and the fetch refuses
    "not enough free space … have 933 MiB". Not this change's (no file of it touched; 7/7 in the
    Task 1 run). It is 7/7 with `TMPDIR` on /dev/shm. Under `build-test.sh` it fails the same
    way, since the container's `/` is the same full disk. Docker holds 62.7 GB of images, none
    pulled today. Nothing was deleted.

## Task 3 (`run-sirius.sh` on the runner)

- New: `scripts/env/run-sirius.sh` (prepare, run; `-h` prints its usage) and
  `scripts/env/run-sirius-vm.sh`, which holds `vm_sirius_ready`, `vm_docker_free` (moved),
  `vm_sirius_idle`, `vm_sent_stamped` and `vm_sirius`. `run-gpu-vm.sh` has no Sirius line.
  `VM_HELPERS` needed nothing more: `vm_sirius` uses `vm_started` and `vm_follow`,
  `vm_sent_stamped` uses `vm_stamped`, and `vm_sirius_idle` uses `vm_idle`, all of them in it.
- Message prefixes, decided:
  - The library's own messages name the calling script as `${0##*/}`, as `machine.sh` already
    does: the dropped ssh, `followed_no_more`, both card refusals, and `image_read`'s missing
    image, now "no <ref> on the VM: <script> prepare pulls it" where it was "run prepare first".
  - `following`'s hint keeps `scripts/env/run-gpu.sh … follow`, which follows any run of the dir.
  - `run-gpu-vm.sh`'s VM-side messages keep `run-gpu.sh:` (the runner, `vm_idle`, `cancel`, the
    `run-gpu.sh: cancelled` log line, which tests pin). Each names a `run-gpu.sh` verb that is
    right for a Sirius run too.
  - `run-sirius-vm.sh`'s own refusals say `run-sirius.sh:`.
- Ordering, by judgement:
  - `run` asks the VM `vm_sirius_idle` before sending, so files are never synced over a going
    run's testdata. `vm_started` asks again under `run/lock`.
  - prepare checks Docker and both sf40 sets (`vm_sirius_ready`), then the free disk, the pull
    and label (`machine_image_checked`), the profile, `image_read` and `cards_checked`.
- Stamps, by judgement beyond the plan's plain `on_vm_with … vm_stamped`:
  - `run` sends with `-R --update`, then hands the VM `<this tree's sha256>  <path>` for every
    file sent. `vm_sent_stamped` stamps a path only where the VM's copy has that sha256.
  - A file `--update` kept (newer on the VM) keeps its old line. So a file changed only on the VM
    comes home at pull (Review Focus 2), and one changed here too is a conflict, never silently
    replaced.
  - Stamping every file sent, as the plan's plain call would, loses that conflict.
    `test_a_resumed_run_over_a_result_changed_here_too_is_a_pull_conflict` was added for it, and
    seen failing with the filter replaced by `true`.
- Known edge, left: a whole run is sent no answers database (master's behaviour, pinned by a moved
  test). On a dir whose `.shipped` never had the database's line, a local database from elsewhere
  then meets the run's new one as a pull conflict (kept, exit 1). After any pull of the dir the
  line exists, and the next whole run's database comes home.
- Mutation checks: with `--update` dropped, the resume and answers tests fail; with
  `vm_sent_stamped`'s filter made `true`, the new conflict test fails. Both were restored, and the
  test file passes whole.
- Tests: `test_run_sirius.py`, 28. That is the 14 moved, the plan's 11 new, the both-sides resume
  conflict, the answers stamped and pulled home, and `-h`. All 28 pass.
- The wiki, kept in agreement at this checkpoint:
  - The Images row's Sirius sentence names `run-sirius.sh`'s tests, and `test_run_sirius.py` is in
    its file list.
  - The CI lint list (wiki and `pipeline.yml`) gains `run-sirius.sh` and `run-sirius-vm.sh`.
  - The "Sirius on the H100" prose, the counts and `bench-sirius.sh` stay Task 4's. `bench-sirius.sh`
    still calls `run-gpu.sh … sirius`, which now exits 2 naming `run-sirius.sh`: the bench is broken
    until Task 4. `test_bench_sirius.py` fakes `run-gpu.sh`, so it still passes.
- Suite (2026-11-13, dev; plan's loop, `timeout 900 python3 <f>`, needs_docker/on_runner
  skipped): 44 files, 42 passed. test_run_sirius 28/28, test_run_gpu 75/75 (the profile test moved
  out), \_records 28/28, \_runs 27/27, \_vm 15/15, test_sirius_bench 33/33, test_pins 10/10,
  test_bench_each 17/17, test_bench_sirius 12/12. Host-only, as in Task 2:
  - `test_embeddings.py`, 3/7 (dev's `/` has 932M free); 7/7 with `TMPDIR=/dev/shm/…`.
  - `test_plot.py` (no matplotlib); 7/7 under `scripts/build-test.sh -- python3 …`.
- `image_inputs . sirius 26.08` has the same sha256 on master and the branch (80a56ae8…), and the
  seven hashed inputs have no diff against master.

## Task 4 (`bench-sirius.sh`, the docs, the counts)

- `bench-sirius.sh` calls `${RUN_SIRIUS[@]} --instance-id … [--profile …] run -- …` with
  `RUN_SIRIUS=("${RUN_SIRIUS_SH:-$here/run-sirius.sh}")`, then `${RUN_GPU[@]} … pull`. Options,
  exits and usage are unchanged, but for the moved verb's names. `--dry-run` against the real
  scripts prints `run-sirius.sh … run -- …`, `run-gpu.sh … pull`, then the render.
- `test_bench_sirius.py`:
  - A fake `run-sirius.sh` beside the fake `run-gpu.sh`. `FAKE_RC` maps `<script> <verb>`; it
    mapped the verb alone in `FAKE_RUN_GPU_RC`.
  - Expected calls: `("run-sirius.sh", [*H100, "run", "--", …])` and
    `("run-gpu.sh", [*H100, "pull"])`.
  - `test_run_gpu_s_exits_pass_through_and_stop_the_run` became
    `test_the_vm_scripts_exits_pass_through_and_stop_the_run`, with the same codes (3, 4, 75, 69,
    1 from `run`, 75 from `pull`).
  - Seen failing (10 of 12) before the script changed, 12/12 after.
- `build-test.md`:
  - "Sirius on the H100" rewritten for `run-sirius.sh prepare` and `run`: the runner and its card
    lock, `logs/sirius-<stamp>.log`, the stamps, and `run-gpu.sh status|follow|cancel|pull` for a
    Sirius run.
  - It gains one sentence on the known gap: a whole run sends no answers database, so a local
    `answers.duckdb` the dir never pulled meets the run's new one as a pull conflict; remove the
    stale local database, or pull the dir, before the run.
  - Also edited: the data-flow line (bench-sirius over run-sirius.sh run), the `prepare` bullet of
    the run-gpu section (Sirius has its own script, and the old verbs are refused), the nebius-h100
    disk note, and the Images row's `bench-sirius.sh` sentence.
- `run-gpu.sh`'s usage names `run-sirius.sh`, and its verbs serve Sirius runs.
- The plan's sweep (`grep -rn 'run-gpu.sh[^|]*sirius\|prepare --sirius' llm-wiki scripts docker
  README.md .github`, outside `llm-wiki/tasks` and `archive`) finds only lines about the move.
  `architecture.md` and `README.md` never named the old verbs.
- The recount, by the header's own rule (the sum of the N columns):
  - Rows whose N differs between base, master and vm-setup: Corpus cpu, Benchmark report page,
    Sirius results reader and DuckDB result rendering (master alone); Calibration, Nebius and
    DuckDB benchmark script (vm-setup alone); Images (both sides).
  - The one-sided rows keep their side's N. Calibration (122) and Nebius (186) were recounted on
    the branch anyway, as `def test_` in `scripts/calibration/tests` and `scripts/nebius/tests`.
    Both agree.
  - Images: its N is the number of `def test_` across every `scripts/env/tests/test_*.py`. That
    rule gives master 413 and vm-setup 518 exactly, and the branch 610.
  - The header was Rust 6355, carried from the base's header, one below its rows; vm-setup's
    recount had fixed that. It is now the row sums: Rust 6356, C++ 286, Python 1970, total 8612.
    `/tmp` script check: the header equals the sums.
- `pipeline.yml`'s shellcheck list already held `gpu-vm-client.sh`, `run-sirius.sh` and
  `run-sirius-vm.sh` (Tasks 2-3).
- cost-report's CI guard (`ci_images.rs`) was not run: its binary predates the replay, and a build
  on a disk with under 1 GB free could fill it. By reading: it joins continuation lines and
  exempts the `shellcheck` command by its first word, so the longer file list cannot trip it. CI
  is the check.
- Suite (2026-11-13, dev; plan's loop): 44 files, 42 passed. test_bench_sirius 12/12,
  test_run_sirius 28/28, test_run_gpu 75/75, \_records 28/28, \_runs 27/27, \_vm 15/15,
  test_sirius_bench 33/33. Host-only, as before:
  - `test_embeddings.py`, 3/7 (932M free on `/`); 7/7 with `TMPDIR=/dev/shm/…`.
  - `test_plot.py`; 7/7 under `scripts/build-test.sh -- python3 …`.

## Coordinator, plan Task 5

- The developer's suites are green but for two host-only files (`test_embeddings.py`: dev's `/`
  has under 1 GB free; `test_plot.py`: no matplotlib on dev), each proven green another way.
- `cargo test -p cost-report --test test_source_formatting` not run on dev: under 1 GB free, a
  build could fill the disk. CI's run is the check.
- No GPU workflow is dispatched (spec item 9): `done` waits on the PR's CPU gates alone.

- PR #206 opened against master, 7 commits.

## Review round 1

0 blocking, 1 important, 6 nits. Replay's resolutions verified clean.
- I1: `gpu-vm-client.sh:168` sets `IMAGE_ID`, read only by callers; shellcheck alone on the
  library likely raises SC2034 and fails "Nebius script tests". Sent to the developer.
- Nits sent with it (cheap, in code being touched): N1 run-sirius.sh's exit list names exits
  it cannot produce; N2 "with no install" should be "needs no install" (the runner mounts
  `cpp/install:ro`); N4 process-history parentheticals in test_run_sirius.py comments; N5 blank
  lines in test_run_sirius.py. N3 (a same-day local Sirius log can conflict like the answers DB)
  folded into the build-test.md sentence. N6: PR body shortened by the coordinator.
- #205 must be closed by hand after the merge (a PR is not closed by keywords).
- CI run 43505157447 (511280c8): "Nebius script tests" red on exactly I1, SC2034 at gpu-vm-client.sh:168; every Python test there passed.

## Review round 1 fixes

- I1: `gpu-vm-client.sh`'s `IMAGE_ID` is set in the library and read only by its callers, so
  `shellcheck -x` on the library alone gave SC2034. It is fixed by a targeted
  `# shellcheck disable=SC2034` with its reason, ssh-retry.sh's pattern.
  - shellcheck ran on dev: `koalaman/shellcheck:stable` (0.11.0, 20 MB, pulled, then removed),
    `docker run --rm -v "$PWD:/mnt:ro" -w /mnt koalaman/shellcheck:stable -x <the 13 files
    pipeline.yml's "Nebius script tests" lints>`.
  - Before: rc 1, that one SC2034 alone. After: rc 0.
  - The other candidates were clean. `IMAGE_MINIMUM` and `IMAGE_MINIMUM_PTX` are read inside the
    library. `RUN_ID`, `STAMPS` and `MACHINE_NEBIUS` are not flagged.
- N1: `run-sirius.sh`'s exit list names only codes it can produce. 4 and 5 are gone. Added: the
  image pull's exits (unpublished, full disk, docker's), 64 as 2, the driver query, a copied
  tree's record, 130/143, 254 and 255.
- N2: "needs no install", in `run-sirius.sh`'s usage, build-test.md's "Sirius on the H100" and the
  Images row: the runner mounts `cpp/install:ro` and adds `--init`.
- N3: the known-gap sentence adds that a same-day local `log/sirius_<date>.log` from another dir
  does the same. `run` never sends the log directory, and Sirius names the file by day.
- N4, N5: the section comments lose their "(master's …)" notes; the blank lines around two tests
  are two each.

## Review round 2

0 blocking, 0 important; I1 and nits resolved by 9c89fc86. One nit (duplicate heading) fixed here.

## Completeness pass

- Reviewer (what is wrong): 0 blocking, 1 important, reproduced. A whole Sirius run never sends
  or stamps `answers.duckdb`, so on a dir whose `.shipped` lacks it, `pull` calls the run's new
  database "changed here and on the VM" and exits 1 — `bench-sirius.sh --dataset all` stops after
  tpch's run, unrendered (spec items 5, 6). Fix proposed: on a whole run with a local database,
  remove the VM's copy and `.wal` before the runner, and stamp this tree's sha256 for the path
  directly (no send); add a test; shrink build-test.md's known-gap sentence to the log case.
- CI for `done` is the run on code-bearing commits; doc-only pushes skip the code jobs.
- Analyst (what is missing): the same important gap, reproduced on its own. Also: item 7's CPU
  gates are proven only by a full run on a code commit (43509814679 on 9c89fc86, then the fix's).
  Divergences for the signoff, not gaps: `run-gpu.sh` keeps a usage note and two `sirius moved:`
  refusals (the plan's Review Focus 5 chose a named refusal over "unknown"); the two-commit
  squash (item 1) is the coordinator's, before `done`, with a force-push; the PR must merge by
  merge commit, not GitHub's squash, to keep rk00's authorship. architecture.md: nothing falsified.
- The gap went to the developer.

## Completeness gap: a whole run's answers and the day log

- The gap (completeness pass, reproduced by a reviewer and an analyst):
  - A whole run neither sent nor stamped `calibration/sirius/<ds>.sf<sf>/answers.duckdb`.
  - On a dir whose `.shipped` had no line for it, `pull` named the run's new database a conflict,
    kept the stale local one and exited 1. "No line" means another dir's database, or one pulled
    by master's `run-gpu.sh` before stamps; this worktree holds a 2.6 GB tpch one.
  - So `bench-sirius.sh --dataset all` stopped at tpch's pull.
- Fix:
  - `run-sirius-vm.sh` has `vm_sirius_answers_cleared <i> <path> <sha256 or nothing>`. It removes
    the VM's database and `.wal` before a whole run, as `sirius-bench.py` does (that file is
    untouched). With a local database, it hands `vm_stamped` this tree's sha256 for the path
    directly, without sending the file.
  - Then `pull` takes the run's new database. A run that writes none leaves nothing on the VM, so
    the local copy stays.
  - The day log, `log/sirius_<date>.log`, is fixed too, cheaply. `run` now sends the dataset's
    `log/` directory with `--update`, stamped through `vm_sent_stamped` like the other files.
    Sirius appends to the day's file, so the VM's copy is this tree's plus the run's lines, and
    `pull` takes it.
- Tests in `test_run_sirius.py`, each seen failing before the fix:
  - `test_a_whole_run_s_answers_replace_a_local_database_the_dir_never_pulled`: failed with pull
    exit 1 on the conflict.
  - `test_a_whole_run_that_writes_no_answers_leaves_the_local_database`: failed because the VM's
    old database outlived the whole run.
  - `test_a_same_day_sirius_log_from_another_dir_comes_home_with_the_run_s_lines`: failed with
    pull exit 1.
  - `test_run_ships_the_answers_a_narrowed_or_resumed_run_keeps` is unchanged: a whole run is
    still sent no database.
- build-test.md "Sirius on the H100":
  - `run`'s bullets describe the log directory and a whole run's database.
  - The pull sentence ("a conflict only for a file changed here and on the VM") now holds.
  - The known-gap bullet shrinks to what remains for any sent file: `--update` keeps a VM file
    only while it is newer than this tree's copy, so pull a run's files before running from
    another tree. That was master's rule as well.
- Counts: Images 610 to 613, Python 1970 to 1973, grand total 8612 to 8615. The header equals the
  row sums.
- shellcheck (0.11.0, image pulled and removed) over pipeline.yml's 13 files: rc 0.
- Reviewer re-check of 36ddf639: closed, 0 blocking, 0 important. Edge noted: a whole run cancelled while waiting for the card has already cleared the VM's unpulled answers; bench-sirius.sh always pulls after each run.
- Signoff written; board to `completeness approved`. Branch then squashed to two commits (replay, port) and force-pushed; only the `done` board commit may follow.
