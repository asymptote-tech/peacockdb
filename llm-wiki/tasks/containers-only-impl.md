# containers-only Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** one way to build and run everywhere but shad-gpu: every CI job that compiles or runs
project code runs in `peacockdb-build:cudf-<v>` (25.02, and a new CI-only 25.10), `gpu-tests` runs
on nebius-gpu through `run-gpu.sh` and the run image, local and verda builds go through
`scripts/build-test.sh` re-executing itself in the build image, and the conda/host-build scripts
and their wiki text are gone.

**Architecture:** CI jobs stay on `ubuntu-latest` and run each project command through
`scripts/build-test.sh -- <cmd>`, which `docker run`s it in the build image over the checkout as
the caller's uid. They do not use `container:`: the image is about 10 GB, and the runner's disk
has to be freed before the pull (build-test.md "Images", "The runner's disk"). The image a job
uses is found by its inputs, not by a moving tag. `image.sh` adds a third tag,
`:cudf-<v>-in-<hash12>`, where the hash covers the `peacockdb.inputs` label.
`scripts/env/image-ref.sh` computes that tag from the tree and resolves it to a digest after the
label check. So a branch that publishes images never moves what master's CI runs. The GPU job
ships the staged install with `run-gpu.sh` and runs it in the run image. The old L40S runner
(`nebius-test.sh`, `ci-ship.sh`) goes; `nebius-sync.sh` stays, cut down to the tree transfer that
new-hosts' CPU-VM build and the wiki use.

**Tech Stack:** bash, Docker (BuildKit, `docker buildx imagetools`), GitHub Actions (composite
actions, `Swatinem/rust-cache`, `actions/cache`), Python 3 script tests (fakes on PATH, the
`scripts/env/tests` harness), Rust guard tests in `cost-report/tests/test_source_formatting/`.

**Spec:** [`containers-only.md`](containers-only.md). Branch `ENS-containers-only`, on the chain's
previous task branch, `ENS-tp4-sized`; its PR targets that branch. Code base: master's engine plus containers' `docker/`, `scripts/env/`,
`scripts/lib/{image-inputs,machine}.sh` (read with `git show origin/ENS-containers:<path>` until
the branch holds them) and new-hosts' renames: `scripts/nebius.sh`, `scripts/nebius-sync.sh`,
`scripts/nebius-test.sh`, `scripts/nebius/` (watcher, `check-actor.sh`, `ci-ship.sh`, `tests/`),
`.github/workflows/nebius-watch.yml`, `scripts/env/run-gpu.sh`, `scripts/env/stage-install.sh`.

## Global Constraints


- Developers never commit, push, or dispatch a workflow (prompts.md). A step that says *Commit
  `<msg>`* means: report a checkpoint `<msg>` and stop; the coordinator stages, commits and pushes, then
  dispatches the next step. Only the plan's last step is green. A step marked *(coordinator)* is the coordinator's; the developer stops before it
  and says what it needs. The coordinator's steps are every push, workflow dispatch and
  reading of a CI run or its log. A CI failure reaches the developer as a failing test, with its signature.
- **Count baseline** (spec item 1): the parent branch's last green PR run (`ENS-tp4-sized`), or,
  when it has none, this branch's own first PR run before any job moves. Not master: the chain's
  earlier tasks add tests. The coordinator records it in the detail file's `## Counts before`.
- Hosts (build-test.md, Nebius host, host roles): nebius-gpu, the L40S, is the correctness host
  and runs the L40S list only; the H100 is for sf40, benchmarks and Nsight.
- **VM time is minimal** (the human's rule, 2026-11-03). A VM starts right before its first
  remote command and stops right after its last `pull`. It is never left up across a wait: a local
  build, CI, a review, the human. Binaries are built and staged first, and the H100 starts only
  once they are ready to ship. No step leaves a VM running for a later task. Each start and stop
  goes into the detail file's `## VM time` table, in the host's local time, with the hours per VM
  for the task. The watchers (1 h idle, 4 h per process, the hourly GitHub check) are the backstop,
  not the plan.
- No engine change, and no test is weakened to make a job pass in the image. A test that fails in
  the image and passed outside it is a finding: fix the image or ticket it. The one source touch is
  the text of `device_answer.rs`'s panic message, which names the deleted `cargo-cudf.sh`.
- CI stays green at every commit of the branch, the cost-report job included, with the same case
  counts per tier as the count baseline. Any count that moves is explained in the detail file.
- `gpu-tests` keeps its actor allowlist (`check-actor.sh` on both actors), `environment: nebius-gpu`,
  `permissions: contents: read`, `concurrency: nebius-gpu`, the push-to-master/dispatch `if:`, and
  never runs on a pull_request.
- Dataset and DuckDB pins stay those of `docker/pins.env` (`DUCKDB_VERSION=1.5.4`,
  `DUCKDB_SHA256`). The generated sf1 stays byte-identical to today's.
- shad-gpu is untouched: `build-test-shadgpu.sh`, `setup-glibc.sh`, `lib/shadgpu-env.sh`,
  `validate-large.yml`'s ssh-to-shad-gpu steps and their docs.
- The per-worktree `CARGO_TARGET_DIR` rule stays: a build script bakes `CARGO_MANIFEST_DIR`.
  Never share a target dir across worktrees. The image's target dir is
  `<worktree>/target-img-<cudf>` (the name containers-rebase uses), and the image runs' home is
  `<worktree>/.home-img`; both are gitignored (`/target-img-*/`, `/.home-img/`; Task 2 adds the
  lines if the fork's `.gitignore` lacks them).
- `docker/run-tests.sh` and `docker/run.Dockerfile` are not edited (run-image inputs). The only
  build-image inputs edited are those of Task 1, so one image publication serves the whole branch.
- No secret value in any argv. Secrets reach a container as `-e NAME`, which reads the
  caller's environment. Never open a file under `~/.nebius/` or any `*.pem`.
- Every ssh from the Bash tool needs the sandbox off. A VM this task starts is stopped again
  (`scripts/nebius.sh stop`) or left to its watcher.

## Review Focus

1. **A tree whose image inputs were never published** (a PR touching `docker/`). CI should fail at
   the image step, name the inputs and the dispatch that publishes them, and never run on a
   neighbour's image. → Task 2, `test_image_ref.py::test_refuses_unpublished_inputs`,
   `test_refuses_a_label_that_differs`.
2. **A developer's local image older than the tree.** `build-test.sh` should refuse and name the
   inputs, not build in a stale image. → Task 2, `test_build_test.py::test_refuses_a_stale_image`.
3. **One worktree switching cuDF versions.** `cpp/build` configured for 26.02, then a 25.02
   build: refuse, naming both. Cargo target dirs are separate per version and always inside the
   worktree. → Task 2, `test_refuses_cpp_build_of_another_cudf`,
   `test_target_dir_is_per_worktree_and_version`.
4. **A cancelled `gpu-tests` run leaves `peacock-peacockdb` running on the VM.** The next run
   should not hang or skip. Cleanup removes the container on every outcome, and `run` refuses a
   busy name with exit 2. → Task 6, `ci_images.rs::the_gpu_cleanup_removes_the_run_container`.
5. **A secret passed into the image.** The AWS keys for `s3-datasets`/`validate-large`
   should never appear in argv or `ps`. → Task 2,
   `test_pass_env_never_puts_a_value_in_argv`.

---

### Task 1: the CI-only 25.10 build image, and the inputs tag

**Files:**
- Create: `docker/conda/cudf-25.10.spec`
- Modify: `docker/pins.env` (`GCC_25_10=14`; no `DRIVER_MINIMUM_25_10`), `docker/build.Dockerfile`
  (stage `FROM ubuntu:24.04 AS cudf-25.10` / `ARG GCC_25_10` / `ENV GCC_VERSION=${GCC_25_10}`,
  beside lines 13–19 of containers' file)
- Modify: `scripts/lib/image-inputs.sh` (add `image_inputs_tag`), `scripts/env/image.sh`,
  `.github/workflows/images.yml` (`cudf` options, matrix `exclude`)
- Test: `scripts/env/tests/test_pins.py`, `test_image.py`, `test_images_workflow.py`

**Interfaces:**
- Produces: `image_inputs_tag <tree> <build|run> <cudf>` → `cudf-<v>-in-<h>`, where `<h>` is the
  first 12 hex of `sha256` over `image_inputs` output (its lines as printed, newline-terminated).
- Produces: `image.sh build` tags `:cudf-<v>`, `:cudf-<v>-in-<h>` (always), `:cudf-<v>-<commit>`
  (clean tree). `image.sh push [--moving]` pushes `:cudf-<v>-in-<h>` and `:cudf-<v>-<commit>`,
  under the same refusal rules, and `:cudf-<v>` only with `--moving`. `images.yml` passes
  `--moving` only when `github.ref` is the default branch's, so a dispatch from any other ref never
  moves `:cudf-<v>`: `:cudf-<v>` moves only from master, and every earlier chain tree keeps passing
  its label check.
- Produces: **a build-only version** is one with a spec and no `DRIVER_MINIMUM_<v_>` in
  `pins.env`. `image.sh build run <v>` and `push run <v>` refuse it, exit 2, with one line naming
  `DRIVER_MINIMUM_25_10`.
- Produces: `ghcr.io/asymptote-tech/peacockdb-build:cudf-25.10` (+ the inputs tag); no run image.

- [x] **Step 1: export the spec on dev.** `docker run --rm -v "$PWD/docker/conda:/out"
  mambaorg/micromamba:<MICROMAMBA_VERSION without -0> bash -c 'micromamba create -y -p /tmp/e
  -c rapidsai -c conda-forge -c nvidia cudf=25.10 cuvs=25.10 python=3.12 cuda-version=12.9 &&
  micromamba env export -p /tmp/e --explicit --md5 > /out/cudf-25.10.spec'`. Match the header
  shape of `cudf-26.02.spec` (comment lines, `@EXPLICIT`, `https://…#<md5>`). For every
  rapids-library package name in the 26.02 spec (`libcudf libcuvs libraft* librmm libkvikio
  rapids-logger`, and others), check it is present in the 25.10 spec. Record in the detail file
  the `libcudf`, `libarrow` and `cuda-version` lines, plus any name missing with its reason.
- [x] **Step 2: failing tests.**
  - `test_pins.py`: `PINS["GCC_25_10"] = "14"`; `IDENTITY["25.10"]` = the libcudf `25.10.*`,
    libarrow and `cuda-version 12.9` lines from Step 1;
    `test_a_build_only_version_has_no_driver_minimum`: `DRIVER_MINIMUM_25_10` not in pins.
  - `test_image.py`: `test_build_refuses_an_unknown_cudf` now uses `25.06`, and stderr names
    `25.02`, `25.10` and `26.02`. `test_build_25_10`: `CUDF=25.10` among the build args, tag
    `…peacockdb-build:cudf-25.10`. `test_run_refuses_a_build_only_cudf`: `build run 25.10
    --dry-run` exits 2, gives one stderr line containing `DRIVER_MINIMUM_25_10`, and makes no
    docker call. `test_build_adds_the_inputs_tag`: on both a dirty and a clean `Repo()`, the
    `-t` values include `…:cudf-25.02-in-<h>`, where `<h>` equals what
    `bash -c '. scripts/lib/image-inputs.sh; image_inputs_tag …'` prints.
    `test_push_pushes_the_inputs_tag`. `test_push_leaves_the_moving_tag_without_moving`: `push`
    without `--moving` pushes the inputs and commit tags and no bare `:cudf-25.02`; with it, all three.
  - `test_images_workflow.py`: `test_a_build_only_version_has_no_run_job`:
    `job()["strategy"]["matrix"]["exclude"] == [{"kind": "run", "cudf": v}]` for each build-only
    `v`. Update `test_all_runs_every_other_choice…` so `exclude` is not read as a matrix axis.
    `test_only_the_default_branch_moves_the_tag`: the push step passes `--moving` only under a
    condition on `github.ref` equal to the default branch's ref.
- [x] **Step 3: run** `for f in scripts/env/tests/test_{pins,image,images_workflow}.py; do python3 $f; done`
  → FAIL on the new tests only.
- [x] **Step 4: implement.** Write the spec, pin, Dockerfile stage, `image_inputs_tag`, and the
  `image.sh` tags and refusal. images.yml: `options: ['25.02', '25.10', '26.02', all]` and
  `exclude: [{kind: run, cudf: '25.10'}]` in block form, which the test's reader accepts.
- [x] **Step 5: run** Step 3's loop, then every `scripts/env/tests/test_*.py` that needs no image
  → `0 failed`.
- [x] **Step 6: build and prove 25.10 on dev.** Run `scripts/env/image.sh build build 25.10`.
  Then, in `docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp/h -v "$PWD:$PWD" -w "$PWD"
  -e CARGO_TARGET_DIR="$PWD/target-img-25.10" <image>`, run `scripts/build.sh --configure
  --build --install --cudf_ROOT /opt/cudf --gcc-version 14`, then `cargo test --no-run -p
  peacockdb-core --lib` → both succeed. Record the image size in the detail file.
- [x] **Step 7: commit** `containers-only: a CI-only cuDF 25.10 build image; images carry an inputs tag`.
- [x] **Step 8: publish (coordinator).** `images.yml` must be on master, because GitHub dispatches
  only the default branch's workflows. If `gh workflow list --all | grep -w images.yml` finds
  nothing, stop, `blocked(building)`, and ask the human to land this branch's
  `.github/workflows/images.yml` alone on master, as a separate commit (approved in advance). Never
  push to master. Then the coordinator pushes the branch and runs `gh workflow run images.yml --ref
  ENS-containers-only -f kind=all -f cudf=all`. Expect five jobs green: the build images for
  25.02, 25.10 and 26.02, and the run images for 25.02 and 26.02. Then the developer, for each,
  `docker buildx imagetools inspect ghcr.io/asymptote-tech/peacockdb-<kind>:$(bash -c '. scripts/lib/image-inputs.sh;
  image_inputs_tag . <kind> <v>')` → a digest, and `docker buildx imagetools inspect
  ghcr.io/asymptote-tech/peacockdb-<kind>:cudf-<v>` → the digest it had before the dispatch (the
  branch did not move it). Record the five digests.

### Task 2: `build-test.sh` is the image command; `image-ref.sh`

**Files:**
- Create: `scripts/env/image-ref.sh`, `scripts/env/tests/test_image_ref.py`,
  `scripts/env/tests/test_build_test.py`
- Modify (rewrite): `scripts/build-test.sh` (844 lines today, for a host build that ships to
  verda)
- Modify: `cost-report/tests/test_source_formatting/runners.rs` (lines 155–171 and 310–335: the
  third lib source)
- Modify: `scripts/case-inventory.sh` (lines 6, 26–40: runs in the image, no `cargo-cudf.sh`),
  `scripts/lib/rung-args.sh:4`, `peacockdb-core/tests/test_crate_privacy.rs:6`,
  `testdata/gen_reconstruction_report.py:15` (comments),
  `peacockdb-core/src/test_support/device_answer.rs:62` (message text: "build in the build image,
  which sets CUDF_ROOT (scripts/build-test.sh)")
- Delete: `scripts/cargo-cudf.sh`
- Modify: `.gitignore` (`/target-img-*/`, `/.home-img/`, if absent)

**Interfaces:**
- Consumes: Task 1's `image_inputs_tag`; `image_inputs_differ` (`scripts/lib/image-inputs.sh`).
- Produces: `scripts/env/image-ref.sh <build|run> <cudf>` prints one image ref and exits 0.
  The order:
  1. the local image `ghcr.io/asymptote-tech/peacockdb-<kind>:<inputs tag>` if present
     (`docker image inspect`), printed as that tag;
  2. otherwise the remote one, via `docker buildx imagetools inspect <ref> --format '{{json .}}'`,
     printed as `ghcr.io/asymptote-tech/peacockdb-<kind>@sha256:<digest>`.

  Either way, the image's `peacockdb.inputs` label must give an empty `image_inputs_differ`;
  otherwise exit 2, with each differing input on stderr. Exit 2 also covers: no image (stderr
  names `scripts/env/image.sh build <kind> <v>` and `gh workflow run images.yml --ref <branch> -f
  kind=<kind> -f cudf=<v>`), and run of a build-only version.
- Produces: `scripts/build-test.sh [--cudf <v>] [--rust-only|--gpu] [--build] [--run]
  [--no-release-bench] [--pass-env NAME]… [-- <command…>]`, where `<v>` defaults to
  `${PEACOCK_CUDF:-25.02}`. The image is `$PEACOCK_BUILD_IMAGE` when set, else `image-ref.sh
  build <v>`.
  - **Outside the image** (no `PEACOCK_IN_IMAGE=1`), it re-execs itself:
    `docker run --rm --user <uid>:<gid> -v <root>:<root>`, plus `-v <git common dir>:<same>` when
    it lies outside `<root>`, `-w <root> -e PEACOCK_IN_IMAGE=1 -e PEACOCK_CUDF=<v> -e
    HOME=/tmp/home -e CARGO_TARGET_DIR=<root>/target-img-<v> -v <registry>:/opt/rust/cargo/registry
    -v <ccache>:/tmp/ccache`, then `-e NAME` for each of `RUSTFLAGS CMAKE_GENERATOR LDFLAGS
    LD_LIBRARY_PATH PCK_TEST_FILTER PCK_SHARD UPDATE_CANONICAL PEACOCK_NO_FETCH` that is set and
    each `--pass-env NAME`, then `<image> scripts/build-test.sh <args>`.
  - When `RUNNER_TEMP` is set (a CI runner), it is mounted at its own path (`-v
    $RUNNER_TEMP:$RUNNER_TEMP`) and passed (`-e RUNNER_TEMP`), so cost-report's `--md`/`--html
    "$RUNNER_TEMP/…"` files land where the later github-script steps read them.
    `$GITHUB_OUTPUT`, `$GITHUB_ENV` and `$GITHUB_STEP_SUMMARY` are never mounted: a step writes
    them on the runner, redirecting the wrapped command's stdout (`scripts/build-test.sh -- <cmd>
    >> "$GITHUB_OUTPUT"`).
  - `<registry>` = `${PEACOCK_CARGO_REGISTRY:-${XDG_CACHE_HOME:-$HOME/.cache}/peacock/cargo-registry}`,
    and `<ccache>` = `${PEACOCK_CCACHE_DIR:-…/peacock/ccache-<v>}`; both are created if missing.
  - A caller's `CARGO_TARGET_DIR` outside `<root>` is refused with exit 2.
  - **Inside the image**, `-- <cmd>` execs the command, and its exit code is the script's.
  - `--build` (cpu, the default mode) builds the C++ library and its tests with `scripts/build.sh
    --configure --build --install --cudf_ROOT /opt/cudf --gcc-version "$GCC_VERSION"
    --compiler-launcher ccache`, stamped `cpp/build/.peacock-cudf`; a stamp of another version
    exits 2, naming both. It then runs `cargo test --no-run` over the derived set, plus `cargo
    build --features rust-only -p peacockdb`.
  - `--rust-only --build` skips the C++ build.
  - `--gpu --build` runs `scripts/env/stage-install.sh --cudf <v> [--no-release-bench]`, which
    stages `cpp/install/` with `rust-tests/` and `rust-benchmarks/`.
  - `--run` (cpu and rust-only modes) runs every derived target and the lib's rung with `cargo
    test`, and in cpu mode `ctest --test-dir cpp/build -L cpu`. It runs each even after a failure,
    and exits 1 if any failed.
  - `--gpu --run` exits 2, naming `scripts/env/run-gpu.sh --instance-id computeinstance-e00tnrse7ayntnzcyt ship` and `run`.
  - The mode ladder, `rust_only_targets`, `needs_cmake_targets` and the `GPUSET` heredoc of
    `gpu_runtime_targets` stay as they are (runners.rs reads them).
  - Removed flags exit 2, each naming its replacement: `--host --remote-dir --local-cudf-root
    --remote-cudf-root --gcc-version --push-* --pull-* --all --build-benchmarks --run-benchmarks
    --pull-benchmarks --update-canonical`.

- [x] **Step 1: failing tests**, with fake `docker`, `cargo`, `ctest` and `stage-install.sh` on
  PATH recording argv, in a fake tree as `test_image.py`'s `Repo()` does.
  - `test_image_ref.py`:
    - `test_prefers_the_local_inputs_tag`;
    - `test_resolves_the_remote_digest` (stdout `…peacockdb-build@sha256:<64 hex>`);
    - `test_refuses_unpublished_inputs` (exit 2; stderr contains `images.yml` and `image.sh build
      build 25.02`);
    - `test_refuses_a_label_that_differs` (label with another `docker/pins.env` hash → exit 2,
      stderr contains `docker/pins.env`);
    - `test_refuses_the_run_image_of_a_build_only_cudf`.
  - `test_build_test.py`:
    - `test_runs_in_the_image_as_the_caller` (`--user <uid>:<gid>`, `-v <root>:<root>`, `-w
      <root>`, `PEACOCK_IN_IMAGE=1`, image = `$PEACOCK_BUILD_IMAGE`);
    - `test_target_dir_is_per_worktree_and_version` (`--cudf 26.02` →
      `CARGO_TARGET_DIR=<root>/target-img-26.02`; `CARGO_TARGET_DIR=/elsewhere` → exit 2);
    - `test_worktree_mounts_its_git_dir` (a `git worktree add` tree → the common dir mounted);
    - `test_pass_env_never_puts_a_value_in_argv` (`AWS_SECRET_ACCESS_KEY=s3cr3t`, `--pass-env
      AWS_SECRET_ACCESS_KEY` → `-e AWS_SECRET_ACCESS_KEY` in argv, `s3cr3t` in none);
    - `test_refuses_a_stale_image` (fake `image-ref.sh` exits 2 naming `docker/pins.env` → exit 2,
      the same name on stderr, no `docker run`);
    - `test_refuses_cpp_build_of_another_cudf`;
    - `test_gpu_build_is_stage_install`;
    - `test_gpu_run_names_run_gpu`;
    - `test_removed_flags_name_their_replacement` (`--host x`, `--push-goldens`, `--gcc-version
      12`);
    - `test_run_runs_every_target_after_a_failure`;
    - `test_command_form_passes_the_exit_code` (`-- false` → 1);
    - `test_runner_temp_is_mounted_when_set` (`RUNNER_TEMP=<tmp>` → `-v <tmp>:<tmp>` and `-e
      RUNNER_TEMP` in argv; unset → neither).
- [x] **Step 2: run** both files → FAIL (no script).
- [x] **Step 3: implement** `image-ref.sh` and the new `build-test.sh`. Delete `cargo-cudf.sh`. In
  runners.rs, replace `build_test_gpu_lib()` with `container_gpu_lib()`. It returns the staged
  name from `scripts/env/stage-install.sh`'s `stage <name> …--lib…` line, and `LIB_RUNG=` from
  `docker/run-tests.sh` (`script_lib`). Add `run_tests_rung_reaches_the_binary()`: in run-tests.sh's
  `for t in "${rust[@]}"` loop, `args+=("$LIB_RUNG")` sits under `[ "$name" = "$LIB_STAGED" ]`,
  and every `run_binary` line passes `"${args[@]}"`. The `runners` loop at line 334 keeps
  SHADGPU only; run-tests.sh is checked by the new function. Update the failure messages that
  name `build-test.sh --gpu`.
- [x] **Step 4: run** both Python files → `0 failed`. Then run
  `scripts/build-test.sh -- cargo test -p cost-report --test test_source_formatting` → all pass.
  Then `scripts/build-test.sh --rust-only --build --run` on dev → green, with the cpu counts of
  build-test.md's table (`--lib` 780, `test_cpu_corpus` 2009, …). Record them in the detail file.
- [ ] **Step 5: commit** `containers-only: build-test.sh builds and runs in the build image; image-ref.sh; cargo-cudf.sh gone`.

### Task 3: the CPU-only jobs in the image (`cost-report`, `s3-datasets`)

**Files:**
- Create: `.github/actions/build-image/action.yml`
- Create: `cost-report/tests/test_source_formatting/ci_images.rs`, registered in
  `cost-report/tests/test_source_formatting.rs`
- Modify: `.github/workflows/pipeline.yml` (`cost-report` 567–866, `s3-datasets` 893–925)

**Interfaces:**
- Consumes: Task 2's `build-test.sh`, `image-ref.sh`.
- Produces: action `./.github/actions/build-image` with input `cudf` (required). Its steps:
  1. Free the disk: the same removals as images.yml's step, plus a `need_gb=25` gate.
  2. `ref=$(scripts/env/image-ref.sh build "$CUDF")`, then write `PEACOCK_BUILD_IMAGE=$ref`,
     `PEACOCK_CUDF=$CUDF`, `PEACOCK_CARGO_REGISTRY=$HOME/.cargo/registry` and
     `PEACOCK_CCACHE_DIR=$HOME/.ccache` to `$GITHUB_ENV`.
  3. `docker pull "$ref"`, retried 3 times.
  4. The ccache `actions/cache` (key `ccache-<cudf>-<sha>`, prefix restore). This moves here from
     cpp-build.
- Produces: `ci_images.rs` with `IN_IMAGE_JOBS: &[(&str, &str)]` (workflow, job). Each job listed
  uses `./.github/actions/build-image`, has no `container:`, and none of its lines contain `conda
  install`, `pip install`, `rapidsai/` or `dtolnay/rust-toolchain`. In a listed job, every `run:`
  line that invokes `cargo`, `ctest`, `python3` or a `testdata/` script starts its command with
  `scripts/build-test.sh`, unless the line is in `RUNNER_SIDE: &[(&str, &str, &str)]` (job, line
  prefix, reason). Every `$NAME` a wrapped command's arguments name is `RUNNER_TEMP` or a
  checkout path (both mounted); `GITHUB_OUTPUT`, `GITHUB_ENV` and `GITHUB_STEP_SUMMARY` appear only
  as the runner shell's redirect after the wrapped command. This task lists `pipeline.yml`
  `cost-report` and `s3-datasets`.

- [ ] **Step 1: counts before (coordinator).** The count baseline (Global Constraints): `id=$(gh
  run list --workflow pipeline.yml --branch ENS-tp4-sized --event pull_request --status success -L1
  --json databaseId -q '.[0].databaseId'); gh run view $id --log > <scratch>/before.log` (this
  branch's own first PR run, before Step 3, if the parent has none). Save, per job, the lines
  matching `test result:`, `\[  PASSED  \]` and the Python harness's summary, to the detail file
  `## Counts before`, naming the run.
- [ ] **Step 2: failing tests** `ci_images.rs::every_listed_job_runs_in_the_build_image` (as in
  Interfaces), and its doctored-workflow cases: `a_bare_cargo_line_in_a_listed_job_is_refused`
  (a `cost-report`-shaped job with `run: cargo test -p cost-report` → refused, naming the job and
  line; the same line in `RUNNER_SIDE` → accepted) and `a_wrapped_command_writing_an_unmounted_path_is_refused`
  (`scripts/build-test.sh -- cargo run -p cost-report -- --md "$GITHUB_OUTPUT"` → refused;
  `--md "$RUNNER_TEMP/r.md"` and `scripts/build-test.sh -- … >> "$GITHUB_OUTPUT"` → accepted). Run `scripts/build-test.sh -- cargo test -p cost-report --test
  test_source_formatting ci_images` → FAIL (the action is absent; cost-report still has
  `dtolnay`).
- [ ] **Step 3: move the jobs.**
  - `cost-report`: after checkout (fetch-depth 0), use the action with `cudf: "25.02"`, then
    Swatinem with `shared-key: rust-cost-report-image` and `workspaces: ". ->
    target-img-25.02"`. Every `python3 …`/`cargo …` line becomes `scripts/build-test.sh -- <same
    command>`, and each self-heal `pip install` line goes. The `"Generate report"` and
    cost-regression steps add `--pass-env` for each env var they set (`REPORT_SHA PAGES_URL
    GITHUB_REPOSITORY BASE_SHA …`, as the step lists them).
  - The nebius script step: the Python tests run in the image, and `shellcheck` stays on the
    runner. It is a linter over the scripts, not project code, and the runner has it. Say so in
    the step's comment and in its `RUNNER_SIDE` entry.
  - `$GITHUB_OUTPUT` writes (master's `pipeline.yml:814`) stay on the runner, as the wrapped
    command's redirected stdout; the `$RUNNER_TEMP` report files go through Task 2's mount.
  - Add the step "Image and run script tests":
    `scripts/build-test.sh -- python3 <f>` for each `scripts/env/tests/test_*.py`, except a
    named list `needs_docker=(test_image_any_user.py test_run_image.py test_run_tests.py)`. Each
    name on that list must exist, as the exec-model step's `elsewhere` list does. The same step
    runs each `benchmarks/tests/test_*.py` (sf40-baseline's `test_duckdb_minimal.py` among them;
    the image has duckdb), so no script suite is left without a CI job.
  - `s3-datasets`: use the action, then `scripts/build-test.sh --pass-env AWS_ACCESS_KEY_ID
    --pass-env AWS_SECRET_ACCESS_KEY -- python3 scripts/check_s3_datasets.py --endpoint …`. Its
    fork guard and `exit 0` on missing keys stay on the runner.
- [ ] **Step 4: run** the guard locally → PASS. Then (coordinator) push; on the branch's PR run,
  `cost-report` and `s3-datasets` are green, and the counts equal Step 1's baseline, plus the
  `benchmarks/tests` suites now run. The coordinator hands any red job or moved count to the
  developer as a failing test, with the venv pins (`pandas 3.0.6`, `matplotlib`) as the first
  suspect for a count; the developer explains it in the detail file.
- [ ] **Step 5: commit** `containers-only: cost-report and s3-datasets run in the build image`.

### Task 4: `dataset-matrix` in the image, legs 25.02 and 25.10 (#129 a)

**Files:**
- Modify: `.github/workflows/pipeline.yml:87–305`, `.github/actions/cpp-build/action.yml`
- Modify: `cost-report/tests/test_source_formatting/ci_images.rs`, `ci_coverage.rs:400–408`
  (doc comment naming #129)
- Modify: `llm-wiki/tickets/testinfra.md` (#129), `llm-wiki/tickets.md` (index line 36, the
  "N open tickets" total at :22),
  `llm-wiki/archive/archived-tickets.md` (Done), every place in `llm-wiki/` naming the 26.02 CI leg
  (`grep -rn '26\.02' llm-wiki/build-test.md | grep -i 'ci\|leg\|matrix'`)

**Interfaces:**
- Consumes: Task 3's action and `IN_IMAGE_JOBS`.
- Produces: matrix `include: [{cudf: "25.10"}, {cudf: "25.02"}]` with no `image` key. The job
  name `CI Pipeline (cudf ${{ matrix.cudf }})`; the `if: matrix.cudf == '25.02'` gates are
  unchanged.
- Produces: `cpp-build` action with input `cudf`. It reads `gcc=GCC_<v with _>` from
  `docker/pins.env`, then runs `scripts/build-test.sh --pass-env LDFLAGS -- scripts/build.sh
  --configure --build --install --cudf_ROOT /opt/cudf --gcc-version "$gcc" --compiler-launcher
  ccache` with `LDFLAGS=-Wl,-rpath-link,/opt/cudf/lib`, then `ccache -s` in the image. No conda,
  no cmake/ninja download: the image has both at the pins.

- [ ] **Step 1: failing tests** in `ci_images.rs`. First, add `dataset-matrix` to
  `IN_IMAGE_JOBS`. Then `every_cudf_leg_has_a_build_image`: each `- cudf: "<v>"` leg in
  pipeline.yml has `docker/conda/cudf-<v>.spec`, no leg has an `image:` key, and the job passes
  `${{ matrix.cudf }}` to the action. Run → FAIL.
- [ ] **Step 2: byte-identity of sf1 on dev.** In a fresh worktree, run `scripts/build-test.sh --
  testdata/generate_testdata.sh --bench tpch` and the same for `--bench tpcds`. Separately, on the
  host, generate into another fresh worktree with the release zip, exactly as pipeline.yml lines
  156–176 do today. Then `(cd testdata && sha256sum tpch.sf1/*.parquet tpcds.sf1/*.parquet)` in
  each → identical. Record the two aggregate hashes.
- [ ] **Step 3: FFI runtime path on dev.** Run `scripts/build-test.sh -- cargo test -p
  peacockdb-ffi --test test_ffi` after a cpp build. If a library is not found, set
  `LD_LIBRARY_PATH: /opt/cudf/lib` in the two Rust steps' env, where it reaches the image through
  the fixed pass-through. Otherwise drop the variable. Record which in the detail file.
- [ ] **Step 4: move the job.** Remove `container:`, `Install git`, `dtolnay` and the DuckDB curl.
  Checkout keeps `submodules: recursive`. Use the action with `cudf: ${{ matrix.cudf }}` and the
  cpp-build action. Swatinem gets `shared-key: rust-image-${{ matrix.cudf }}` and
  `workspaces: ". -> target-img-${{ matrix.cudf }}"`. Every `ctest`/`cargo`/`python3`/
  `testdata/…` line becomes `scripts/build-test.sh -- <same command>`, and the `pip install`
  self-heals go. Job env keeps `CMAKE_GENERATOR` and `RUSTFLAGS`; the step env loses
  `CUDF_ROOT: /opt/conda` and `LD_LIBRARY_PATH: /opt/conda/lib`. Every comment that names the
  rapids container, the tmpfs or glibc-per-image is rewritten to what is true now: one image
  per leg, and a target dir per leg.
- [ ] **Step 5: #129.** Close it the way `tickets.md` line 17 says. Its text records that the
  second leg is now `25.10` in `peacockdb-build:cudf-25.10` and that (b) was closed earlier.
  Update `testinfra.md`'s Contents and count, the index line, and `tickets.md`'s open-ticket
  total (line 22, one fewer). Update `build-test.md`'s CI
  section and every other line that names a 26.02 CI leg to 25.10. The 26.02 images stay for
  developer and device use.
- [ ] **Step 6: run** `scripts/build-test.sh -- cargo test -p cost-report --test
  test_source_formatting` → PASS. Then (coordinator) push; both legs are green on the PR, their
  counts per step equal the count baseline's 25.02 leg, and the 25.10 leg's equal the baseline's
  "26.02" leg; `Generate SF-1` logs `generate_pbench.sh --check` passing. A moved count or a red
  step reaches the developer as a failing test; the developer explains it in the detail file.
- [x] **Step 7: commit** `containers-only: dataset-matrix in the build image, an honest 25.10 leg (closes #129)`.

### Task 5: what `run-gpu.sh` needs for CI and for agents

**Files:**
- Modify: `scripts/env/run-gpu.sh`
- Test: `scripts/env/tests/test_run_gpu.py` (new-hosts' file for `run-gpu.sh`; extend it)

**Interfaces:**
- Consumes: `run-gpu.sh --instance-id <id> [--profile <name>] [--dir <i>] [--cudf <v>] prepare | ship [--install <d>] | run list|filter|bench|<staged binary> | pull` (new-hosts, `filter` a mode word read with `PCK_TEST_FILTER`; query-fuser, any other word a staged binary; `--instance-id` required since new-hosts' review round 1, so nebius-gpu, the L40S, is `--instance-id computeinstance-e00tnrse7ayntnzcyt`),
  exit codes 0, 1 run failed, 2 bad input/stale image/busy, 3 old driver, 4 restart needed;
  container `peacock-<instance>`, instance = the repo root's basename; it calls
  `scripts/nebius.sh` for the host.
- Produces, each added only where the as-built script lacks it (the test fails first either way):
  - `PEACOCK_RUN_IMAGE=<ref>`: when it is set, `prepare` and `run` use that ref instead of
    `:cudf-<v>`, with the same inputs and driver checks.
  - `prepare --no-sf40`: no sf40 fetch and no env file read, and none of the Nsight-only setup:
    no `/opt/cudf` copy to `~/peacock/cudf-<v>/` and no profiling-counters setting (so no exit 4
    from it). The L40S list reads none of them. This changes new-hosts' `run-gpu.sh prepare`; with
    the flag absent, `prepare` is as new-hosts left it.
  - `ship`: each rsync is retried 5 times with pauses of attempt × 15 s, using
    `--partial-dir=.rsync-partial --compress-choice=zstd --compress-level=3 --checksum --timeout=120`.
    `cpp/install/lib`, `cpp/install/bin`, `cpp/install/rust-tests` and testdata go as four
    parallel streams. This is ci-ship.sh's transport (`scripts/nebius/ci-ship.sh` `send`/`stream`),
    moved here with its comment on the 0.24 MB/s path.
  - `run --detach <mode>` returns once the container is up. `status` prints `running`, `exited
    <code>` or `none`, with the last 40 log lines. Its exit code is 0 for exited 0, 1 for exited
    non-zero, 5 for running, and 2 for none. This is what `nebius-test.sh run`/`status` gave
    agents.

- [ ] **Step 1: failing tests**: `test_run_uses_the_given_run_image`,
  `test_prepare_no_sf40_reads_no_env_file` (no env read from stdin, no `fetch-sf40.sh` call),
  `test_prepare_no_sf40_skips_the_nsight_setup` (counters admin-only on the fake host → exit 0, no
  `peacock-profiling.conf` written, no `/opt/cudf` copy),
  `test_ship_retries_a_failed_stream_and_resumes` (fake rsync fails twice, then passes → exit 0,
  3 calls with `--partial-dir`), `test_ship_gives_up_after_five` (exit 1, stderr names the
  stream), `test_status_reports_running_then_exit`, `test_detached_run_returns_while_running`.
  Port every case of `scripts/nebius/tests/test_ci_ship.py` that still applies, and name any
  dropped case and why in the detail file.
- [ ] **Step 2: run** `python3 scripts/env/tests/test_run_gpu.py` → FAIL on the new ones.
- [ ] **Step 3: implement.** Run `shellcheck scripts/env/run-gpu.sh` → clean.
- [ ] **Step 4: run** → `0 failed`; every other `scripts/env/tests/test_*.py` needing no image → `0 failed`.
- [ ] **Step 5: commit** `containers-only: run-gpu.sh takes CI's run image, ships like ci-ship, runs detached`.

### Task 6: `cpp-build-2502` in the image; `gpu-tests` through `run-gpu.sh`

**Files:**
- Modify: `.github/workflows/pipeline.yml:310–563`, `scripts/env/stage-install.sh`
- Modify: `cost-report/tests/test_source_formatting/runners.rs` (`NEBIUS_TEST`, new-hosts' name
  for the old `GPU_HOST_TEST` → `RUN_TESTS = "docker/run-tests.sh"`, the staging list from
  `stage-install.sh`, the run step),
  `ci_coverage.rs:32–35, 556–578` (F5 reads stage-install.sh), `ci_images.rs`
- Delete: `scripts/nebius-test.sh`, `scripts/nebius/ci-ship.sh`, and their tests under
  `scripts/nebius/tests/` (`test_ci_ship.py`, `test_nebius_test.py`)
- Modify (kept, spec item 2's "reduced to what `run-gpu.sh` lacks"): `scripts/nebius-sync.sh`
  becomes the tree transfer alone, the uncommitted workspace to a VM, which new-hosts' CPU-VM
  build and the wiki's recipes use. Its sf1 generation goes (it sourced the conda
  `peacock-env.sh`; sf1 on a VM now comes from `run-gpu.sh prepare` or `bootstrap-vm.sh`);
  `--no-data` stays accepted, a no-op, so every caller keeps working. new-hosts'
  `test_sync_passes_the_instance_through` and `test_sync_no_data_skips_generation` stay and
  pass; a new `test_sync_never_generates_data` (no `generate_testdata` in any recorded ssh, with
  or without `--no-data`) replaces any test of the old default
- Modify: the cost-report nebius-script step's `shellcheck` list (drop the two deleted files,
  add `scripts/build-test.sh scripts/env/run-gpu.sh scripts/env/image-ref.sh scripts/env/stage-install.sh`)

**Interfaces:**
- Consumes: Task 5's `run-gpu.sh`; Task 3's action; `stage-install.sh [--cudf <v>]`.
- Produces: `stage-install.sh --no-release-bench` skips the `rust-benchmarks/` release build;
  CI runs no bench. Its staging keeps the shape the guards read: one line
  `for t in test_gpu_corpus test_node_timing peacock_gpu_benchmarks; do` and one
  `stage peacockdb_core_gpu_lib …--lib…` line, as pipeline.yml lines 391–403 have today.
- Produces: the `gpu-tests` steps, in order:
  1. checkout;
  2. the actor check (unchanged);
  3. download and unpack `cpp-install-25.02`;
  4. nebius-setup (unchanged, no `profile:`);
  5. `echo "PEACOCK_RUN_IMAGE=$(scripts/env/image-ref.sh run 25.02)" >> "$GITHUB_ENV"`;
  6. the start loop, through `scripts/nebius.sh start`;
  7. `Prepare nebius-gpu`: `scripts/env/run-gpu.sh --instance-id computeinstance-e00tnrse7ayntnzcyt prepare --no-sf40` (timeout 40);
  8. `Ship the staged install`: `scripts/env/run-gpu.sh --instance-id computeinstance-e00tnrse7ayntnzcyt ship --install cpp/install`
     (timeout 50);
  9. `Run the L40S list`: `scripts/env/run-gpu.sh --instance-id computeinstance-e00tnrse7ayntnzcyt run list`;
  10. cleanup, `if: always() && …start.outcome` as today:
      `scripts/nebius.sh ssh 'docker rm -f peacock-peacockdb' || true`, then `scripts/nebius.sh stop
      || echo …`.
- Produces: `cpp-build-2502`:
  1. the action with `cudf: "25.02"`;
  2. Swatinem (`shared-key: rust-gpu-build-image`, `workspaces: ". -> target-img-25.02"`);
  3. step `Build and stage rust GPU test binaries`:
     `scripts/build-test.sh --gpu --build --no-release-bench`;
  4. pack and upload as today.

  "Bundle Arrow/Parquet runtime libraries" goes: the run image's `/opt/cudf` holds them.

- [ ] **Step 1: failing guard tests.**
  - runners.rs: `gpu_job_staged_targets()` reads `scripts/env/stage-install.sh`.
    `the_pipeline_stages_through_stage_install`: the step `GPU_STAGING_STEP` contains
    `build-test.sh --gpu --build`.
  - `the_gpu_step_cannot_report_success_over_a_failure` is rewritten for run-tests.sh. The step
    contains `run-gpu.sh --instance-id computeinstance-e00tnrse7ayntnzcyt run list` and no `|| true`/`; true`. run-tests.sh ends `exit
    "$rc"`. Every `run_binary` line in `run_list`/`run_bench` is followed by a `judge` line.
    `judge` greps `Segmentation fault`. No `export LD_LIBRARY_PATH`.
  - `both_gpu_runners_pass_test_threads_one` reads `[RUN_TESTS, SHADGPU]`. For run-tests.sh,
    every `args=(` line in its rust loop and in `run_bench` contains `--test-threads=1`.
    `the_runner_reader_…` gains a run-tests.sh-shaped fixture.
  - `the_benchmark_binary_runs_without_its_cases_on_ci`: run-tests.sh's loop adds `--skip
    bench_` for `$BENCH_TARGET`.
  - `the_three_gpu_target_lists_agree`: stage-install.sh's list, shadgpu's `RUST_TESTS` and
    build-test.sh's GPUSET. Its message names those three files.
  - `ci_images.rs::the_gpu_cleanup_removes_the_run_container`: the cleanup step contains
    `docker rm -f peacock-peacockdb` and its `if:` starts `always()`. Add `cpp-build-2502` to
    `IN_IMAGE_JOBS`.

  Run → FAIL.
- [ ] **Step 2: implement** the pipeline, the `stage-install.sh` flag, the guard readers and
  the deletions. Then `grep -rn 'nebius-test\.sh\|ci-ship\.sh\|peacock-env\.sh\|miniforge'
  --exclude-dir=archive --exclude-dir=tasks .` → only shad-gpu's files and their docs; `grep -n
  'generate_testdata\|peacock-env' scripts/nebius-sync.sh` → nothing.
- [ ] **Step 3: run** `scripts/build-test.sh -- cargo test -p cost-report --test
  test_source_formatting` → PASS. Run `python3` on each `scripts/nebius/tests/test_*.py` →
  `0 failed`.
- [ ] **Step 4: commit; (coordinator) push; the PR is green.** `cpp-build-2502` uploads an
  artifact that holds `lib/libpeacock_gpu.so`, `bin/peacock_*_tests` and the four `rust-tests/`
  files.
- [ ] **Step 5: `prepare` on nebius-gpu by hand, once, before any CI dispatch.** With the sandbox
  off: `scripts/nebius.sh start`; `scripts/nebius.sh ssh 'df -h /'` (below 10 GB free: stop, an
  obstacle in the detail file, build-test.md "Disk"); `timeout 2400 scripts/env/run-gpu.sh --instance-id computeinstance-e00tnrse7ayntnzcyt prepare
  --no-sf40` → exit 0 (Docker, the toolkit, sf1 and the run image in place). An exit 4 is handled
  by `scripts/nebius.sh stop --force && scripts/nebius.sh start` and a second `prepare --no-sf40`.
  `scripts/nebius.sh ssh 'df -h /'` again; record both, the wall time and what the run image and
  sf1 took; `scripts/nebius.sh stop`.
- [ ] **Step 6: device run through CI (coordinator).** The coordinator runs `gh workflow run
  pipeline.yml --ref ENS-containers-only` (its actor is in `GPU_ALLOWED_ACTORS`) and watches the
  run. Pass means `gpu-tests` green and its log shows the run image's digest, `run-tests.sh
  list: green`, no `miniforge`, and per-binary counts equal the count baseline's `gpu-tests`, or
  master's last green `gpu-tests` when the baseline holds none. The coordinator records wall
  times for prepare, ship and run, and the job log's `df -h /`, and hands any failure or moved
  count to the developer as a failing test. Pass also means the job stopped the VM, or the
  watcher did.
- [x] **Step 7: commit** any fix with its failing test first:
  `containers-only: gpu-tests ships and runs on nebius-gpu through run-gpu.sh; the old runner goes`.

### Task 7: `exec-model-corpus.yml` and `validate-large.yml` in the image; the guard covers every job

**Files:**
- Modify: `.github/workflows/exec-model-corpus.yml`, `.github/workflows/validate-large.yml`
- Modify: `cost-report/tests/test_source_formatting/ci_images.rs`

**Interfaces:**
- Produces: `ci_images.rs::every_job_runs_in_the_image_or_says_why`. Every job of
  `pipeline.yml`, `exec-model-corpus.yml` and `validate-large.yml` is either in `IN_IMAGE_JOBS`
  or in `HOST_JOBS: &[(&str, &str, &str)]` (workflow, job, reason). The `HOST_JOBS` entries:
  - `changes`: gh api only;
  - `deploy-pages`;
  - `gpu-tests`: runs only scripts/nebius.sh and run-gpu.sh on the runner, and the code in the
    run image on the VM.

  No job of the three has a `container:` key, and no line in them or in `.github/actions/` contains
  `conda install`, `pip install` or `rapidsai/`.

- [ ] **Step 1: failing test** (as above) → FAIL (`corpus` and `validate-large` are unlisted).
- [ ] **Step 2: move.**
  - `corpus` uses the action with `cudf: "25.02"`. `scripts/build-test.sh --
    testdata/generate_testdata.sh --bench tpch` and the same for tpcds; the `pip install` and the
    curl go. The run step becomes `scripts/build-test.sh -- python3 "$f" ${FILTER:+"$FILTER"}`, with
    `PCK_SHARD` passed through and `DUCKDB` dropped (on PATH in the image).
  - `validate-large`: the two ssh steps stay as they are (shad-gpu). The S3 step uses the action,
    then `scripts/build-test.sh --pass-env AWS_ACCESS_KEY_ID --pass-env AWS_SECRET_ACCESS_KEY --
    python3 scripts/check_s3_datasets.py --endpoint …`; `pip install awscli` goes (aws is in the
    image).
- [ ] **Step 3: run** the guard → PASS.
- [ ] **Step 4: live.**
  - (coordinator) `gh workflow run exec-model-corpus.yml --ref ENS-containers-only -f filter=q1`.
    Pass means three shards green, each running ≥ 1 test; a red shard reaches the developer as a
    failing test.
  - validate-large dispatches only against shad-gpu, which is down. Prove its S3 step on dev
    instead: `set -a; . ~/.config/peacock/peacock.env; set +a; scripts/build-test.sh --pass-env
    AWS_ACCESS_KEY_ID --pass-env AWS_SECRET_ACCESS_KEY -- python3 scripts/check_s3_datasets.py
    --endpoint https://storage.eu-north1.nebius.cloud:443` → exit 0. Record in the detail file
    that the ssh half was not run, and why.
- [ ] **Step 5: commit** `containers-only: the manual workflows run in the build image; every CI job is accounted for`.

### Task 8: delete the conda and machine-setup scripts

**Files:**
- Delete: `scripts/conda-install-cudf.sh`, `scripts/conda-install-cudf-2506.sh`, `scripts/init.sh`,
  `scripts/init_fedora43.sh`, `scripts/create_verda_instance.sh`
- Modify: every file naming them (`grep -rln 'conda-install-cudf\|init_fedora43\|create_verda_instance\|scripts/init\.sh'
  --exclude-dir=archive --exclude-dir=tasks .`)

**Interfaces:**
- Produces: a host needs only Docker. A CPU VM gets it from `bootstrap-vm.sh`, and a GPU host
  gets Docker and the NVIDIA Container Toolkit from `run-gpu.sh prepare`. A developer's own
  machine installs Docker by hand; the wiki says so in one line. No script is reduced to a
  Docker installer, since both installs already exist.

- [ ] **Step 1:** delete the five files (`rm`; the coordinator stages the deletions).
- [ ] **Step 2:** run the grep → empty outside `llm-wiki/archive/` and `llm-wiki/tasks/`. Run
  `grep -rn 'conda install\|miniforge\|rapids-cuda-12.2\|envs/rapids' --exclude-dir=archive
  --exclude-dir=tasks . | grep -v 'third_party/'` → only shad-gpu's files (`build-test-shadgpu.sh`,
  `lib/shadgpu-env.sh`, `setup-glibc.sh`) and the shad-gpu wiki section.
- [ ] **Step 3: commit** `containers-only: the conda installers and machine-setup scripts go`;
  (coordinator) push; the PR is green.

### Task 9: the wiki describes one path

**Files:**
- Modify: `llm-wiki/build-test.md`:
  - "Containers" becomes "Build and run", holding "Images";
  - "Local build workflows and caches" table: one row per shape, all `scripts/build-test.sh`;
  - "CI structure": jobs in the image, the inputs tag, legs 25.02/25.10;
  - "GPU host"/"Nebius host": `run-gpu.sh` for every Nebius GPU VM, no conda env;
  - "Remote hosts": verda builds and runs `build-test.sh` on itself; no `--host`;
  - "Datasets": `generate_testdata.sh` in the image;
  - shad-gpu's section says it is the one exception: no Docker.
- Modify: `llm-wiki/prompts.md` (line 192: the verda dispatch rule; lines 376–381: the L40S
  bracket builds first, then starts the VM: `scripts/build-test.sh --gpu --build` (local, in
  the build image, the L40S stopped) → `scripts/nebius.sh start` →
  `scripts/env/run-gpu.sh --instance-id computeinstance-e00tnrse7ayntnzcyt ship` → `run --detach list`, polled with `status` →
  `scripts/nebius.sh stop`; the host roles and the H100 bracket new-hosts wrote beside it stay:
  nebius-gpu runs the L40S list only, the H100 sf40, benchmarks and Nsight, nebius-cpu128
  DuckDB and the H100's staged install (binaries for H100 runs are built on the CPU VM; local
  and verda builds stay for development), the CPU-VM build through `nebius-sync.sh`),
  `.claude/agents/*.md` wherever they name a removed path,
  `llm-wiki/architecture.md` if it names one

**Interfaces:**
- Produces: the detail file's `## Wiki commands run`: every command in the edited sections,
  with the host it ran on and its exit code.

- [ ] **Step 1:** `grep -rn 'build-test\.sh --host\|--local-cudf-root\|cargo-cudf\|CUDF_ROOT=\|conda\|nebius-test\|gpu-host-test\|gpu-host-sync\|rapidsai/base'
  llm-wiki/*.md .claude/agents/` → the list to rewrite. Rewrite it in short sentences
  (prompts.md "Shared rules"). Name the CI-only 25.10 image once in "Images", with "no run image".
- [ ] **Step 2: run every command once on dev** in a fresh worktree:
  - `scripts/build-test.sh --rust-only --build --run`;
  - `scripts/build-test.sh --build --run` (FFI; counts = build-test.md's ffi row);
  - `scripts/build-test.sh --gpu --build`;
  - `scripts/build-test.sh -- testdata/generate_testdata.sh --bench tpch`;
  - `scripts/env/image-ref.sh build 25.10`;
  - `scripts/env/image.sh build build 25.02 --dry-run`;
  - `scripts/case-inventory.sh` per its usage line.

  Then, with the sandbox off:
  - `scripts/nebius.sh start`;
  - `scripts/env/run-gpu.sh --instance-id computeinstance-e00tnrse7ayntnzcyt prepare --no-sf40` (Task 6 Step 5 ran it first; this rerun is a no-op);
  - `ship`;
  - `run --detach list`;
  - `status` until it exits 0;
  - `scripts/nebius.sh stop`.

  The CPU-VM build (`build-test.md`, "Nebius host", "The CPU-VM build") runs once as written,
  the CPU VM up only for it, over the cut-down `nebius-sync.sh`.
  Every command named in nebius-h100's section runs on the H100 the same way, with
  `scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu`, as
  new-hosts' section words them. Record each in `## Wiki commands run`. A command that fails is
  fixed in the wiki or the script, test first, before the commit.
- [ ] **Step 3:** `grep` from Step 1 → only shad-gpu lines and archive/tasks.
- [ ] **Step 4: commit** `containers-only: one build-and-run path in the wiki; shad-gpu the one exception`.

---

## Self-review

- **Spec coverage.**
  - Done-when 1 (CI in the image, 25.10 leg, images.yml, label check, no conda): Tasks 1, 3, 4
    and 7.
  - Green with counts: Steps 3.1/3.4, 4.6, 6.6 and 7.4, against the count baseline.
  - Done-when 2 (L40S through `run-gpu.sh`, old scripts deleted, guard follows): Tasks 5 and 6.
  - Done-when 3 (one command, `cargo-cudf.sh` gone, per-worktree target dir): Task 2.
  - Done-when 4 (deletions): Task 8.
  - Done-when 5 (shad-gpu untouched): Global Constraints, and Step 8.2's grep.
  - Done-when 6 (wiki): Task 9.
  - #129 (b) is already closed (`testinfra.md`). Task 4 records that and changes no comment.
- **Interfaces.** These names are used the same way throughout: `image_inputs_tag`,
  `image-ref.sh`, `PEACOCK_BUILD_IMAGE`/`PEACOCK_RUN_IMAGE`/`PEACOCK_CUDF`, `target-img-<v>`,
  `IN_IMAGE_JOBS`/`HOST_JOBS`, `RUN_TESTS`, `--no-release-bench` and `--no-sf40`.
