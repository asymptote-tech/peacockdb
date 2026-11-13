# vm-setup-rebase Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** vm-setup's change on master, with Sirius moved out of `run-gpu.sh` into
`scripts/env/run-sirius.sh`, whose runs go through vm-setup's runner.

**Architecture:** Task 1 replays vm-setup in one commit authored by Roman, taking vm-setup's side
wherever it collides with Sirius. Task 2 moves the dev-side code both scripts need from
`run-gpu.sh` into `scripts/lib/gpu-vm-client.sh`. It makes the image read and the card check work
for any image, and gives the runner two generic hooks. `run-gpu.sh`'s behaviour does not change.
Task 3 builds `run-sirius.sh` and its VM side, `scripts/env/run-sirius-vm.sh`, on that library.
Task 4 moves `bench-sirius.sh` and the docs over. Task 5 is the sweep and the PR.

**Tech Stack:** bash, Docker, rsync, flock; Python 3 script tests (`scripts/env/tests`,
`scripts/nebius/tests`, `scripts/calibration/tests`), run as `python3 <file>`.

**Spec:** [`vm-setup-rebase.md`](vm-setup-rebase.md). Branch `ENS-vm-setup-rebase`, from master.
vm-setup's tip is `e0612b23` and its base is `1eaefe39`, both on `origin/ENS-vm-setup`. Read
vm-setup's spec and plan after Task 1, where the replay brings them in as
`llm-wiki/tasks/vm-setup.md` and `vm-setup-impl.md`. Master's Sirius code before this task:
`git show master:scripts/env/run-gpu.sh`, `…:scripts/env/tests/test_run_gpu.py` (lines 1200-1400)
and `…:scripts/env/tests/gpu_host.py`.

## Global Constraints

- Developers never commit, push or dispatch. A step "report a checkpoint `<msg>`" means: stop and
  report it, and the coordinator commits and pushes. A step marked *(coordinator)* is the
  coordinator's.
- Commit messages: at most 10 lines; no process words, no people's names in the text. Prefix
  `vm-setup-rebase: `, except Task 1's commit, which is `vm-setup: ` (Task 1 gives its message).
- No engine change, and no test is weakened. A test of a moved shape (`run-gpu.sh sirius` →
  `run-sirius.sh run`) is moved with its intent. The detail file lists each move under
  `## Moved tests`.
- The Sirius image's hashed inputs are never edited: `docker/sirius.Dockerfile`,
  `sirius.Dockerfile.dockerignore`, `sirius-venv.lock`, `sirius-fetch.sh`, `sirius-config.yaml`,
  `sirius-bench.py`, `sirius-LICENSE`. Proof: `bash -c '. scripts/lib/image-inputs.sh &&
  image_inputs . sirius 26.08'` gives the same output on master and on the branch.
- No VM is started, no ssh leaves dev, and no workflow is dispatched.
- Never open a Nebius credential file (prompts.md, "Shared rules").
- Never push to `ENS-vm-setup`.
- `run-gpu-vm.sh` holds no Sirius line. Sirius's VM side is `run-sirius-vm.sh`.
- shellcheck is not on dev. CI's `Nebius script tests` step lints the scripts with `-x`. Add
  `run-sirius.sh` and the new library to its list in `pipeline.yml`, wherever that step names
  `run-gpu.sh`.
- Each task keeps `llm-wiki/build-test.md` in agreement within the same checkpoint. The counts
  are recounted once, in Task 4.

## Review Focus

1. **A Sirius run and a peacockdb run on one card**: each waits for the other's card lock. A
   Sirius run must never start a container while another holds the card. → Task 3
   `test_a_sirius_run_waits_for_a_run_holding_the_card` and its reverse.
2. **`--resume` after an unpulled run**: the VM's results file is newer than this tree's. `run`
   must keep it (`--update`), and `pull` must bring it home with no conflict. → Task 3
   `test_a_resumed_sirius_run_keeps_the_vm_s_newer_rows_and_pull_takes_them`.
3. **A dir never shipped**: `run-sirius.sh run` on a fresh dir must not leave a root-owned
   `cpp/install`, which a later `ship` could not write. → Task 3
   `test_a_sirius_run_on_a_dir_never_shipped_leaves_cpp_install_the_user_s`.
4. **A Sirius tag moved after prepare**: the run uses the ID read in the same call as the labels,
   never the tag. → Task 3 `test_a_sirius_run_runs_the_image_by_the_id_it_checked`.
5. **The old verbs typed from habit**: `run-gpu.sh sirius` and `run-gpu.sh prepare --sirius`
   exit 2 and name `run-sirius.sh`. → Task 3 `test_run_gpu_names_run_sirius_for_the_old_verbs`.

---

### Task 1: vm-setup replayed onto master, as Roman's commit

**Files:** whatever the replay touches. Conflicts are expected in `scripts/env/run-gpu.sh`,
`scripts/env/tests/gpu_host.py`, `test_pins.py`, `test_run_gpu.py`, `llm-wiki/build-test.md`,
`llm-wiki/tickets.md`, `llm-wiki/tickets/testinfra.md` and `llm-wiki/tasks/tasks.md`.

**Interfaces:**
- Produces: vm-setup's `run-gpu.sh` and `run-gpu-vm.sh` on master, with no Sirius in them.

- [ ] **Step 1 *(coordinator)*: replay.** `ENS-vm-setup-rebase` is already on origin: the helper
  made it at master's commit that holds this plan, and the workspace has it checked out.

```bash
git merge-recursive 1eaefe39 -- HEAD e0612b23   # exits 1 with conflicts
```

- [ ] **Step 2: resolve each conflict, by these rules**
  - `run-gpu.sh`: vm-setup's side whole. Every Sirius line master added goes, conflicting or
    not: the `sirius` verb, `prepare --sirius`, `SIRIUS_*`, `sirius_image`, `prepare_sirius`,
    `vm_docker_free`, `vm_sirius`, `cmd_sirius`, the seccomp profile in `KIT`, and the usage
    lines. The goldens directory in the pull set stays: vm-setup's `in_pulled_set` already
    carries it.
  - `test_run_gpu.py`: vm-setup's side. Master's Sirius tests go (lines 1200-1400 on master),
    and Task 3 brings them back. List them in the detail file's `## Moved tests`.
  - `gpu_host.py`: both sides. Keep master's fake Sirius image, its labels and its
    `docker info` root, which Task 3 uses, in vm-setup's fake VM.
  - `test_pins.py`: both. vm-setup's `DRIVER_MINIMUM_PTX` and its defaults check, and master's
    Sirius pins.
  - `tickets.md`, `tickets/testinfra.md`: both sides' tickets. The open count and the next free
    number are recomputed from the files.
  - `build-test.md`: both sides' text. Take the higher-numbered side of each count for now,
    since Task 4 recounts them all.
  - `tasks.md`: master's, plus vm-setup's entry under chain N, at `done`.
  - Any other conflict: both sides' intent. Name it in the detail file.

- [ ] **Step 3: run the Python suites**

Run: `for f in scripts/env/tests/test_*.py scripts/nebius/tests/test_*.py scripts/calibration/tests/test_*.py; do python3 "$f" > /tmp/t.log 2>&1 || echo "FAIL $f"; done`
(skip `pipeline.yml`'s `needs_docker` and `on_runner` files).
Expected: no FAIL. `test_bench_sirius.py` passes, since it fakes `run-gpu.sh`.

- [ ] **Step 4 *(coordinator)*: commit as Roman**

```bash
git add -A
git commit --author="rk00 <00romankolesnik@gmail.com>" -F <message>
```

Message: the subject `vm-setup: a developer's VM, end to end`; one line, `Replayed from
e0612b23 onto master; Sirius returns in the commits after it.`; then vm-setup's 35 subjects, from
`git log --format='- %s' 1eaefe39..e0612b23`. The 10-line limit does not apply to that list.
Push.

### Task 2: the shared client library; the runner's two hooks

**Files:**
- Create: `scripts/lib/gpu-vm-client.sh`
- Modify: `scripts/env/run-gpu.sh`, `scripts/env/run-gpu-vm.sh`
- Test: `scripts/env/tests/test_run_gpu_vm.py`, `test_run_gpu_runs.py`

**Interfaces:**
- Consumes: Task 1's `run-gpu.sh`.
- Produces, in `scripts/lib/gpu-vm-client.sh`, sourced after the caller sets `root`, `dir`,
  `NEBIUS` (an array), `instance`, `TREE_RECORD`, `tree_path`, and a `die` that exits 2 with its
  script's name:
  - moved from `run-gpu.sh` unchanged: `vm`, `on_vm`, `on_vm_with`, `VM_HELPERS`,
    `in_pulled_set`, `sha256_of`, `to_vm`, `from_vm`, `following`, `followed_no_more`, `docker`,
    `tree_id_read`, `tree_claimed`, and the `--dir` and `--instance-id` checks as
    `vm_options_checked`.
  - `image_read <ref> <kind> <cudf>`: run_image_read made general. It sets `IMAGE_ID`,
    `IMAGE_MINIMUM` and `IMAGE_MINIMUM_PTX`. The PTX label is required for `run` and absent for
    `sirius`, where it is set empty. Its refusals keep their texts, with `<ref>` and `<kind>` in
    them.
  - `cards_checked <ref>`: as today, over `IMAGE_*`. With `IMAGE_MINIMUM_PTX` empty, the PTX
    rule is skipped.
  - `run-gpu.sh` keeps `RUN_ID`, set from `IMAGE_ID`, so its callers do not change.
- Produces, in `run-gpu-vm.sh`:
  - `vm_started <i> <what> <card lock> <judged> <docker run argument>…`. `<judged>` is 1 when a
    staged binary's judgment applies, else 0. `vm_runner` takes the same argument, in place of
    its `case $what in list|filter|bench`. `vm_run` passes 0, and `vm_run_staged` passes 1.
  - `vm_started` makes `~/peacock/<i>/cpp/install` as the user before the runner starts, so
    Docker never makes it as root.

- [ ] **Step 1: write the failing tests**
  - In `test_run_gpu_runs.py`, `test_a_run_on_a_dir_never_shipped_leaves_cpp_install_the_user_s`.
    vm_started on a fresh dir must leave `cpp/install` a directory owned by the test's uid.
  - In `test_run_gpu_vm.py`, `test_the_runner_judges_only_what_it_is_told_to`. A runner started
    with `<judged>` 0, whose log has `running 0 tests`, ends with its own code.
- [ ] **Step 2: run them and see both fail.**
- [ ] **Step 3: make the library and the hooks**, as Interfaces says. `run-gpu.sh` sources the
  library right after `tree_id_read`'s current place, and keeps only its own verbs, `KIT`, and the
  `RUN_REF`/`BUILD_REF` lines.
- [ ] **Step 4: run every `scripts/env/tests/test_run_gpu*.py`, `test_bench_each.py` and the new
  tests.** Expected: all pass, with no existing assertion changed.
- [ ] **Step 5: report a checkpoint** `vm-setup-rebase: the VM client shared from a library; the
  runner told when to judge, and the install dir made by its user`.

### Task 3: `run-sirius.sh` on the runner

**Files:**
- Create: `scripts/env/run-sirius.sh`, `scripts/env/run-sirius-vm.sh`,
  `scripts/env/tests/test_run_sirius.py`
- Modify: `scripts/env/run-gpu.sh` (the old verbs' refusal), `scripts/env/tests/gpu_host.py`
  (only if the fake VM lacks what a Sirius run needs)

**Interfaces:**
- Consumes: Task 2's library, `vm_started`, `vm_stamped` and `in_pulled_set`.
- Produces:
  - `run-sirius.sh [--instance-id <id>] [--profile <p>] [--dir <i>] prepare`. Today's
    `prepare --sirius`, without the rest of prepare:
    - It refuses, exit 2, a VM without Docker or without `<data>/<ds>.sf40`, naming
      `run-gpu.sh prepare`.
    - Then today's free-disk check (`SIRIUS_IMAGE_GB=4`, `SIRIUS_SPARE_GB=10`), with its text.
    - Then `machine_image_checked` of the Sirius image, the seccomp profile sent to
      `~/peacock/<i>/docker/sirius-seccomp.json`, `image_read <ref> sirius <cudf>` and
      `cards_checked`.
  - `run-sirius.sh … run [--detach] -- <docker/sirius-bench.py arguments>`. Today's `sirius`:
    - Its argument checks and their texts stay, with `run-sirius.sh` in place of
      `run-gpu.sh`.
    - It sends today's files, with `--update`.
    - It stamps each file it sent with this tree's sha256:
      `on_vm_with <file of "<sha256>  <path>" lines> vm_stamped "$dir"`.
    - Then `vm_sirius` runs attached through `following`, or through `on_vm` with `--detach`.
  - In `run-sirius-vm.sh`:
    - `vm_docker_free`, moved.
    - `vm_sirius <i> <image ID> <detach> <card lock> <ds> <sf> <runner argument>…`: it refuses
      a missing profile or missing data, exit 2, with today's texts. Then
      `vm_started <i> sirius <lock> 0 --security-opt seccomp=<profile> --ulimit memlock=-1:-1
      <image ID> python3 /opt/sirius/sirius-bench.py <args> --data <data>/<ds>.sf<sf>
      --out /peacock/testdata`. Then `vm_follow`, or with `--detach` the line
      `==> peacock-<i>: sirius-bench.py, detached; run-gpu.sh … status says how it goes, …
      follow follows it`.
  - `run-sirius.sh` sources `run-gpu-vm.sh` and `run-sirius-vm.sh`, and adds `vm_sirius`'s
    helpers to `VM_HELPERS`.
  - `run-gpu.sh sirius …` and `run-gpu.sh prepare --sirius` exit 2 with
    `sirius moved: scripts/env/run-sirius.sh prepare | run`.
- [ ] **Step 1: port master's Sirius tests** (Task 1's `## Moved tests`) into
  `test_run_sirius.py`, calling `run-sirius.sh` with the same assertions. Paths and texts change
  only where the runner changes them: the log is now `logs/sirius-<stamp>.log`, and the run uses
  the ID, not the tag. List each rewrite in `## Moved tests`.
- [ ] **Step 2: write the new tests**
  - `test_a_sirius_run_waits_for_a_run_holding_the_card`: the fake VM holds the card lock for a
    `run list`. `run-sirius.sh run --detach` makes `status` say `waiting`, and the container
    starts once the lock is let go.
  - `test_a_run_waits_for_a_sirius_run_holding_the_card`: the reverse.
  - `test_cancel_and_follow_reach_a_sirius_run`: `run-gpu.sh follow` gives its log and its exit,
    and `run-gpu.sh cancel` stops it and writes `cancelled`.
  - `test_a_resumed_sirius_run_keeps_the_vm_s_newer_rows_and_pull_takes_them`: the VM's
    `benchmark-results/h100-80gb-hbm3/...` file is newer, and `run … --resume` keeps it.
    `run-gpu.sh pull` exits 0, and the local file then equals the VM's.
  - `test_a_sirius_result_changed_on_both_sides_is_a_pull_conflict`: pull exits 1 naming it,
    and the local copy stays.
  - `test_pull_clear_clears_a_sirius_run_s_log`.
  - `test_a_sirius_run_runs_the_image_by_the_id_it_checked`: the fake tag moves after the label
    read, and the container is made from the first ID.
  - `test_a_card_below_the_sirius_minimum_is_refused_with_exit_3`.
  - `test_a_sirius_run_on_a_dir_never_shipped_leaves_cpp_install_the_user_s`.
  - `test_run_gpu_names_run_sirius_for_the_old_verbs`.
  - `test_a_sirius_run_s_exit_2_is_its_own_and_never_judged_as_a_binary`.
- [ ] **Step 3: run them and see them fail.**
- [ ] **Step 4: write `run-sirius.sh`, `run-sirius-vm.sh` and the refusal in `run-gpu.sh`**,
  as Interfaces says. Its usage header follows `run-gpu.sh`'s style, and `-h` prints it.
- [ ] **Step 5: run `test_run_sirius.py`, every `test_run_gpu*.py`, `test_sirius_bench.py` and
  `test_pins.py`.** Expected: all pass. Run the image-inputs proof from Global Constraints.
  Expected: identical.
- [ ] **Step 6: report a checkpoint** `vm-setup-rebase: Sirius in run-sirius.sh, its runs held
  by the runner, its files stamped, its image run by ID`.

### Task 4: `bench-sirius.sh`, the docs and the counts

**Files:**
- Modify: `scripts/env/bench-sirius.sh`, `scripts/env/tests/test_bench_sirius.py`,
  `.github/workflows/pipeline.yml` (the shellcheck list), `llm-wiki/build-test.md`,
  `llm-wiki/architecture.md` (only if it names `run-gpu.sh sirius`)

**Interfaces:**
- Consumes: Task 3's `run-sirius.sh run`.
- Produces: `bench-sirius.sh` calls `${RUN_SIRIUS[@]} … run -- …`, with
  `RUN_SIRIUS=("${RUN_SIRIUS_SH:-$here/run-sirius.sh}")`, then `${RUN_GPU[@]} … pull`. Its
  options, its exit codes and its usage text stay, except where they name the moved verb.

- [ ] **Step 1: change `test_bench_sirius.py`'s expected calls** to
  `("run-sirius.sh", [*H100, "run", "--", …])` and `("run-gpu.sh", [*H100, "pull"])`, with a fake
  `run-sirius.sh` beside the fake `run-gpu.sh`. Run it, and see it fail.
- [ ] **Step 2: change `bench-sirius.sh`.** Run the test, and see it pass.
- [ ] **Step 3: the docs.** In `build-test.md`, "Sirius on the H100" names `run-sirius.sh
  prepare` and `run`, the runner's lock, `logs/sirius-<stamp>.log`, and `run-gpu.sh
  follow|status|cancel|pull` for a Sirius run. The run-gpu sections lose `sirius`. `grep -rn
  'run-gpu.sh[^|]*sirius\|prepare --sirius' llm-wiki scripts docker README.md .github` finds
  nothing that is not about the move. Then add `run-sirius.sh` and the library to
  `pipeline.yml`'s shellcheck list.
- [ ] **Step 4: recount `build-test.md`'s inventory**: the "Images (Python)" row and the grand
  total, from the test files on the branch, the same way vm-setup counted them (its plan's last
  recount).
- [ ] **Step 5: report a checkpoint** `vm-setup-rebase: bench-sirius.sh through run-sirius.sh;
  the docs and the counts`.

### Task 5: the sweep and the PR

- [ ] **Step 1: every Python suite**, as Task 1 Step 3 runs them. Expected: no FAIL. Then
  CI's own line, `scripts/build-test.sh -- cargo test -p cost-report --test test_source_formatting`,
  since the CI wiring guard reads `pipeline.yml`. Run it in a tree under `/build`, as vm-setup's
  plan says. Expected: pass. If the image is not on dev, CI's run of it is the check.
- [ ] **Step 2: the detail file's `## Deferred`**:
  - a Sirius run on the H100 through the runner;
  - vm-setup's by-hand nebius-gpu items (its spec, item 7's last bullet);
  - CI's `gpu-tests` through the runner.
- [ ] **Step 3 *(coordinator)*: the PR against master.** Title `vm-setup-rebase: vm-setup on
  master; Sirius in run-sirius.sh on the runner`. The body says it supersedes #205 and lists the
  deferred items. Wait for the CPU gates. `GPU Tests (nebius-gpu)` is not dispatched.
