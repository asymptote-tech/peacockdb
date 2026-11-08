# containers-only — run detail

Branch `ENS-containers-only`, forked from `ENS-tp4-sized` at `9058709c`; its PR targets
`ENS-tp4-sized`.

## Progress

- 2026-11-07T11:23Z: branched; board `building`; developer dispatched at Task 1 Step 1.
- Task 1: removed from dev's docker, to make room for Step 6: `peacockdb-run:cudf-26.02`
  (`sha256:bc3ff3712f2c…`) and `peacockdb-run:cudf-25.02` (`sha256:10bf83bb1ba1…`). Re-pull from
  ghcr when needed. `/` went from 13 GB to 30 GB free.
- Task 1 Step 6: also removed `peacockdb-build:cudf-25.02` (`sha256:2a0c84414baa…`, revision
  `b50d9b2a4ea0`, the ghcr digest `peacockdb-build@sha256:2a0c84414baa…`) and `mambaorg/micromamba:2.9.0`
  (Step 1's export). The 25.10 build left 3.8 GB free, too little for Step 6's cargo build; after the
  removal, 19 GB. Re-pull 25.02 from ghcr for Task 2 on.
- Task 4: dev's docker swapped 25.02 → 25.10 → 25.02 (`## Task 4`); clones and scratch removed.

## Task 1

- Step 1 (2026-11-07, dev): `docker run --rm mambaorg/micromamba:2.9.0 … micromamba create -p /tmp/e
  -c rapidsai -c conda-forge -c nvidia cudf=25.10 cuvs=25.10 python=3.12 cuda-version=12.9 && micromamba
  env export --explicit --md5`. 179 packages, every line with an md5; 164 conda-forge, 15 rapidsai,
  none from nvidia. micromamba writes no `# created-by:` line: one was added, `# created-by: micromamba
  2.9.0`, to match the other specs' header. The lines that pin it:
  - `libcudf-25.10.00-cuda12_251008_f4e35ca0.conda#317192f50f3bda357e7640bc03310c3c`
  - `libarrow-21.0.0-hcc2f0d9_39_cpu.conda#8ffc51729aae1b45ade38f0b26ea4b02`
  - `cuda-version-12.9-h4f385c5_3.conda#b6d5d7f1c171cbd228ea06b556cfa859` (the 26.02 spec's line exactly)
- Names in the 26.02 spec and not in 25.10's: `libcuvs-headers` (rapids), `icu`, `six` (neither is a
  rapids library). `libcuvs-headers` was first published at 25.12 (`micromamba search -c rapidsai
  libcuvs-headers`: 25.12.00 … 26.10.00); 25.10's `libcuvs` carries its own headers. Every other rapids
  library of the 26.02 spec is in 25.10's (`libcudf libcuvs libraft libraft-headers
  libraft-headers-only librmm libkvikio libucxx rapids-logger` and the Python ones). Only in 25.10's:
  `libpython`.
- Steps 3 and 5: red on the new and changed tests only (no 25.10 spec, no `GCC_25_10`, no
  `image_inputs_tag`, no `--moving`); then test_pins 4, test_image 33, test_images_workflow 17 and
  every other image-free `scripts/env/tests/test_*.py` passed, 0 failed. images.yml: actionlint clean.
- Push design: images.yml's Push step reads `DEFAULT_BRANCH: ${{ github.event.repository.default_branch }}`
  and passes `--moving` when `$GITHUB_REF` is `refs/heads/$DEFAULT_BRANCH`, with no `if:` (the
  workflow test forbids one). `image.sh push` now also refuses unless the inputs tag exists locally
  and names the commit tag's image.
- Beyond the plan's file list: `llm-wiki/build-test.md` "Images" now states the three tags, `--moving`,
  the master-only move, 25.10 as build-only and its size, so the commit keeps the wiki in agreement.
- Step 6 (dev): `scripts/env/image.sh build build 25.10` from the dirty tree, exit 0, about 9 min
  (the conda layer 412 s). Local tags `:cudf-25.10` and `:cudf-25.10-in-6bbfdd6dbd8f`; its label
  matches the tree (`image_inputs_differ` prints nothing). **Size: 10.81 GB** uncompressed (the sum of
  `docker history`, the measure of the "Images" table), 5.76 GB compressed; `docker images` shows
  16.6 GB, the two added. The run images' inputs are unchanged by Task 1 (`image_inputs` diffed against
  HEAD's tree): only the build images' inputs moved.
- Step 6's proof, in that image (`--user` dev's uid, `-e HOME=/tmp/h`, `-v $PWD:$PWD -w $PWD`,
  `-e CARGO_TARGET_DIR=$PWD/target-img-25.10`): `scripts/build.sh --configure --build --install
  --cudf_ROOT /opt/cudf --gcc-version 14` exit 0 (GNU 14.2.0, nvcc 12.9.86; cudf, rmm, raft and cuvs
  found at 25.10.0, so 25.10's `libcuvs` does carry its headers), then `cargo test --no-run -p
  peacockdb-core --lib` exit 0 in 11 min 6 s. Not the plan's exact command: `cpp/build` and
  `cpp/install` were bind-mounted from scratch directories, because the worktree's `cpp/build` is
  dev's host configure (g++-12, the conda env) and an in-place reconfigure would have overwritten it.
  The C++ build printed three warnings: two `-Wstringop-overflow` from gcc 14's
  `bits/stl_algobase.h`, and `cudf::round(…, rounding_method, …)` deprecated in 25.10
  (`cpp/src/expr.cpp:753`). The `target-img-25.10` (7.7 GB) and the scratch were removed after; the
  log is `/tmp/co-step6-run.log` on dev.
- After Task 1: dev holds the 25.10 build image (`:cudf-25.10`, `:cudf-25.10-in-6bbfdd6dbd8f`) and
  no other peacockdb image; 19 GB free on `/`. Next: Step 7, the checkpoint commit (coordinator).

## Task 2

- Dispatched 2026-11-07 at Steps 1-5 (developer, dev, no VM).
- dev's docker: removed `peacockdb-build:cudf-25.10` and `:cudf-25.10-in-6bbfdd6dbd8f` (local id
  `sha256:5c6c1d97c966…`, Task 1 Step 6's dirty-tree build, not the published `sha256:9d862501b165…`),
  for room: 19 GB → 29 GB free on `/`. Pulled `peacockdb-build:cudf-25.02-in-50a0c1d7779e` (the tree's
  inputs tag, recomputed with `image_inputs_tag`; all five match run 42371113005's).
- Step 4, mid-run: `/` fell to 1.7 GB free while the rust-only build filled `target-img-25.02`
  (about 11 GB by the CLI build). Ran `docker builder prune -f`: 16.48 GB of build cache, all from
  Task 1's local image builds (the images are published). `/` back to 16 GB free.
- Steps 1-2: `test_image_ref.py` (7) and `test_build_test.py` (18) written first; both red (no
  `image-ref.sh`; the old host-shaped `build-test.sh`, which also failed its own rust-only build on
  `prototype_fused_benchmarks`, below).
- Step 3: both green, 7/7 and 18/18. `image-ref.sh` against the real docker and ghcr: `build 25.02`
  → the local `:cudf-25.02-in-50a0c1d7779e`; `build 26.02`, `build 25.10`, `run 25.02` → the digests
  run 42371113005 published (`9199d3ef…`, `9d862501…`, `3a69a03c…`); `run 25.10` exit 2.
- The guard red then green, in the image (`scripts/build-test.sh -- cargo test -p cost-report --test
  test_source_formatting`): before runners.rs changed, 40 passed, 1 failed,
  `the_three_gpu_target_lists_agree` at runners.rs:313 (build-test.sh no longer names a `--gpu`
  lib); after, 42 passed (the new `the_run_tests_reader_refuses_a_rung_that_cannot_reach_the_lib`).
- Step 4 (dev, local, image `:cudf-25.02-in-50a0c1d7779e`): both Python files 7/7 and 18/18, and every
  other image-free `scripts/env/tests/test_*.py` green (262 tests in 20 files). The guard in the
  image: 42 passed. `RUST_TEST_THREADS=2 scripts/build-test.sh --rust-only --build --run`: exit 0,
  build about 15 min cold (registry and target empty), no warnings; log `/tmp/co-rust-only.log` on dev.
  Counts against build-test.md's cpu table (the plan's 780/2009 are stale):

  | target | passed | table | note |
  |---|---|---|---|
  | `--lib` | 1209 (+2 ignored) | 1211 | the 2 are `#[ignore]`d under #182 (`tests/end_to_end/accounting.rs:31`, `:264`); the table counts cases |
  | `test_cpu_corpus` | 2451 | 2451 | 354 s |
  | `test_corpus_goldens` | 66 | 66 | |
  | `test_cost_model` | 3 | 3 | |
  | `test_golden_format` | 41 | 41 ("Everything else", golden text format) | in the rust-only derivation, not the cpu block |

  `target-img-25.02` is about 11 GB and kept, warm for Task 3; `/` has 16 GB free.
  `scripts/case-inventory.sh cudf` on the host: exit 2, naming the image command.
  shellcheck is on neither dev's host nor the build image; `bash -n` only for the three scripts.
- Decisions, beyond or against the plan's text:
  - **An unstamped `cpp/build` is refused, never reconfigured** (`test_refuses_an_unstamped_cpp_build`):
    exit 2, "cpp/build holds a configure no build here made … rm -rf cpp/build cpp/install". Same
    for a stamp of another version. `--rust-only` never looks at `cpp/build`
    (`test_rust_only_build_leaves_cpp_build_alone`), so Step 4's run left dev's host configure
    (g++-12, the conda 25.02 env) as it was. `build-test-shadgpu.sh` still configures `cpp/build`
    on the host, unstamped, so the refusal also protects a shad-gpu configure. A cpu `--run` needs
    the stamp of its own version.
  - **The rust-only derivation skips files gated `#![cfg(feature = "gpu")]`.** The old derivation
    put `prototype_fused_benchmarks` (query-fuser, `required-features = ["gpu", "test-support"]`)
    in the rust-only and cpu sets, and cargo refuses to build it there: `cargo test --no-run
    --features rust-only -p peacockdb-core --test prototype_fused_benchmarks` → "target … requires
    the features: `gpu`, `test-support`", exit 101 (probed on dev). So the old `build-test.sh
    --rust-only --build` could not have passed since query-fuser. Mutation-checked: without the
    line, two derivation tests fail.
  - HOME in the image is `<tree>/.home-img` (Global Constraints; .gitignore has it), not the
    Interfaces' `/tmp/home`, which does not exist in the image and which no uid could create.
  - `RUST_TEST_THREADS` and `CARGO_BUILD_JOBS` join the forwarded list, so the memory bounds of
    build-test.md "Memory constraints" reach the container without `--pass-env`.
  - A command form (`-- <cmd>`) refuses `--build`, `--run`, a mode or `--no-release-bench` beside it.
  - `--no-release-bench` is passed to `stage-install.sh`, which learns it in Task 6 (plan line
    480); until then a real `--gpu --build --no-release-bench` stops at stage-install's "unknown
    argument". Tested over a stub only.
  - `test_crate_privacy` stays excluded by name; its reason is now "the cost-report job runs it on
    its own" (no staging remains for the cpu modes).
  - runners.rs: `container_gpu_lib()` reads the staged name off stage-install.sh's
    `stage_cargo_lib_binary "$LIB_STAGED"` line (resolving the variable), the plan's "`stage <name>
    …--lib…` line" being pipeline.yml's shape, not stage-install's. `run_tests_rung_reaches_the_binary`
    also requires run-tests.sh's own `LIB_STAGED` to be that name, since its rung is keyed on it.
  - device_answer.rs: two panic messages named `cargo-cudf.sh` (lines 77 and 90, not 62); both changed.
  - `.gitignore` already had `/target-img-*/` and `/.home-img/`: unchanged.
  - Wiki, for agreement with this commit: build-test.md "Local build workflows and caches" (the
    table's cudf rows, the cargo-cudf bullet, the `cpp/build`/`cpp/build26` bullets, the FFI stamp
    bullet), the case-inventory row, the Images (Python) row and the CI wiring guard row, and the
    counts (Python +25 → 1629, scripts/env 262 → 287, Rust +1 → 6304, total 8219). Left for Task 9,
    which rewrites them: build-test.md's `--host`/`--push-*`/`--run-benchmarks` uses (Remote hosts'
    verda rows ~2502-2503 and golden regen ~2551, the benchmark flow ~1616 and the 26.02-host steps
    ~3045-3055, the `PEACOCK_TESTDATA_DIR` note ~1384, `cpp/build26` ~3410), prompts.md:192, and
    `scripts/create_nsys_profile.sh:23-27` ("a host … provisioned by build-test.sh", a conda root;
    Task 8's `miniforge|envs/rapids` grep finds it).
    Each of those flags now exits 2 naming its replacement.
  - Pre-existing drift, not this task's: build-test.md said 262 scripts/env tests where the files
    held 271 by `def test_` before this task; rustfmt `--check` reports diffs in
    `cost-report/src/main.rs` and 15 peacockdb-core files nobody here touched.

## Task 3

- Dispatched 2026-11-07 at Steps 2, 3 and Step 4's local half (developer, dev, no VM). Image
  `:cudf-25.02-in-50a0c1d7779e`, `target-img-25.02` warm; `/` stayed at 16 GB free.
- Step 2: `ci_images.rs` written with a checker that accepted everything: the four doctored tests
  red. Checker written: they pass, and `every_listed_job_runs_in_the_build_image` red on the
  unchanged workflow, naming the missing action, `dtolnay`, three `pip install` lines and every
  bare `cargo`/`python3` line of both jobs. Step 3: green, `test_source_formatting` 47 passed
  (41 at the baseline, +1 Task 2, +5 here).
- The checker (`image_problems`): a job's lines run from `  <job>:` to the next two-space key;
  `run:` values inline or block; `\` lines folded (`fold_continuations`); `$(…)` read as its own
  line; a simple command's first word, past keywords and `NAME=` words (`NAME=(…)` is one word),
  is the command. Comment lines are skipped by every rule, `OFF_IMAGE` too. "Unmounted" is a
  deny list, not the plan's allow list: `HOME`, `GITHUB_{OUTPUT,ENV,STEP_SUMMARY,PATH,STATE,
  EVENT_PATH}`, `RUNNER_*` but `RUNNER_TEMP`, and `~`. A value like `$REPORT_SHA` is not a path,
  and an allow list cannot tell. Words after a redirection are the runner's. A `RUNNER_SIDE`
  entry that matches no line is refused.
- **cost-report's own `every_workflow_flag_is_one_the_binary_accepts` (`main.rs`) broke**: it took
  every `--word` of a line with `cargo run -p cost-report`, so `build-test.sh`'s `--pass-env`
  read as a cost-report flag. Fixed: a line is read from `cargo run` on. Mutation-checked: a
  `--bogus` on the wrapped cost-diff line is still refused.
- Diverged from the plan:
  - **`benchmarks/tests` is not in cost-report**: `test_duckdb_minimal.py` asserts the generated
    sf1 (`testdata/tpch.sf1/lineitem.parquet`), which that job has not. Moved to Task 4's 25.02 leg
    (plan note at Task 4). It is "manual" today, so no coverage moves.
  - **Three `scripts/env/tests` files run on the runner** (`on_runner`, one `RUNNER_SIDE` entry):
    `test_fetch_sf40.py` needs `en_US.UTF-8`, and the image has only `C`, `C.utf8`, `POSIX` and no
    locale sources (#309, new). `test_bootstrap.py` (`pwd.getpwuid`) and `test_provision.py`
    (`provision-vm.sh`'s `id -un`) need a user name, and the runner's uid 1001 has none in the
    image. Found only by running as 1001: as dev's uid 1000 (the image's `peacock`) they pass.
    The scripts are right to want one: they run as a VM's or laptop's own user. All three are
    stdlib only and pass on python 3.12.3 (17, 20, 23).
  - The action has a second input, `ccache` (default `'false'`), key `ccache-img-<cudf>-<sha>`.
    Unconditional, cost-report would save an empty `~/.ccache` under the key the C++ jobs share,
    and a prefix restore would then hand them the empty one. `img` keeps the conda-era caches out.
  - `--pass-env` names what the program reads, not what the step sets: `GITHUB_REPOSITORY` on
    both report lines, plus `GITHUB_SHA` on the cost-diff line, which has no `--sha`. `REPORT_SHA`,
    `PAGES_URL`, `BASE_SHA` expand on the runner into argv.
  - The registry the action names is the runner's `~/.cargo/registry`, which Swatinem caches; the
    pull step reads the ref from the find step's output, not from `GITHUB_ENV`.
  - `PEACOCK_DPHYP_LIB` is `$PWD/target-img-$PEACOCK_CUDF/release/…`, passed by `--pass-env`.
- Local evidence (dev, 2026-11-07):
  - Every `run:` of the edited cost-report job executed as rendered (`/tmp/co-sim/run_job.py`,
    bash -e, `RUNNER_TEMP`/`GITHUB_OUTPUT` as temp paths, event `pull_request`), after the action's
    "Find the build image" step run as written. All green but shellcheck, absent on dev; its line
    passes under the actionlint image's shellcheck 0.11.0. Report, `join-optimizer` page,
    `pr_comment.md` and the gate (1017 compared, 0 changed, `rc=0` to `GITHUB_OUTPUT`) produced.
  - Counts against `## Counts before`, cost-report job, log order: every Python line identical,
    both `1 runs …, 1 failed` lines included (`run.py:243` prints `1 runs in 1.1 s, 1 failed`).
    Rust: `--bin cost-report` 49, `test_source_formatting` 41 → 47 (above), the rest equal.
  - As the runner's uid: a clone owned by 1001:118, no submodules, every in-image Python file of
    the job `docker run --user 1001:118`: 91 of 93 files green, the two above red. `cargo run
    --cost-diff` as 1001 with an empty registry: crates fetched, built, `git show` of the base
    fine.
  - `image-ref.sh`'s remote branch with an empty `DOCKER_CONFIG`: `buildx imagetools inspect`
    and `docker pull …@sha256:b8da5a94553c…` both anonymous, exit 0.
  - The new step as rendered: 17 files in the image, 201 tests; 3 on the runner, 60; exit 0.
  - actionlint: the six findings HEAD's files already have, none in these jobs. YAML parses;
    `bash -n` of all 17 `run:` blocks; shellcheck of the action's three scripts clean.
- Wiki: build-test.md's cost-report and s3-datasets bullets, the second trap, the "Images" test
  paragraph, the CI wiring guard row (+5, 20) and the Images (Python) row: its N was 287 and the
  files hold 296 (`def test_`; the run's 201+60 plus 35 by hand), the drift Task 2 recorded. Totals
  now Rust 6309, Python 1638, 8233. Ticket #309 in testinfra.md, tickets.md's index, total 93 and
  next free 310.
- Plan bookkeeping: Task 2 Step 5 ticked (committed `1a1b9e56`), Task 4 Step 7 unticked (it was
  ticked, not done).
- What could differ on the runner: free disk after the removals against `need_gb=25` (the image
  is 10 GB uncompressed, 15.4 GB in `docker images` here); Swatinem runs the runner's own cargo
  for its key and cleanup (`~/.cargo/bin`, preinstalled), not the image's; the first run's
  target and registry are cold (15 min was the old cold time); `docker run` per Python file, about
  100 of them, adds 1-2 min. s3-datasets now pays the disk freeing and the pull, a few minutes.

## Task 4

- Dispatched 2026-11-07 at Steps 1-5 and Step 6's local half (developer, dev, no VM).
- Step 1: `IN_IMAGE_JOBS` gains `dataset-matrix`; new `every_cudf_leg_has_a_build_image` (each
  `- cudf:` leg has `docker/conda/cudf-<v>.spec`, no line of the job starts `image:`, the build-image
  step takes `cudf: ${{ matrix.cudf }}`) and its doctored cases `a_leg_off_its_build_image_is_refused`.
  Red on the unchanged workflow for the intended reasons (the two `image:` keys, `${{ matrix.image }}`,
  no action, every bare line).
- Beyond the plan, in the same guard:
  - **A listed job's local actions are checked too** (`local_actions`, `action_problems`;
    `a_local_action_a_listed_job_uses_is_held_to_the_job_rules`). Without it, cpp-build could go back
    to `conda install` with the job still green. The doctored case found a real gap, red first: a
    bare `./scripts/build.sh` was not seen, because `PROJECT_COMMANDS` was matched without
    stripping `./`. `cmake` and `scripts/build.sh` joined `PROJECT_COMMANDS`.
  - **The rung matchers read from `cargo` on** (`cargo_words` in ci_coverage.rs). Wrapping the cpu
    rung made `each_rung_has_its_ci_line_and_the_cli_is_built` red: `lib_run_filter` took
    build-test.sh's `--` as cargo's and read the filter as `cargo`. Pinned first by
    `a_rung_line_wrapped_in_build_test_is_still_its_rung`, red, then fixed. Task 3's
    `every_workflow_flag_is_one_the_binary_accepts` fix was the same shape.
  - Guard: 51 passed (47 + those 4). `cargo test -p cost-report --bin cost-report` 49 passed;
    `scripts/nebius/tests/test_watch_workflow.py` 10 passed.
- **`cpp-build-2502` keeps today's action as `.github/actions/cpp-build-conda/`**, HEAD's
  `cpp-build/action.yml` with a new name and description. The job still runs in a rapids
  container until Task 6, and a container job cannot `docker run` build-test.sh's image. Task 6
  deletes it (added to its Files list).
- The cpp-build action (as built): one step. It checks the input against `PEACOCK_CUDF` (the
  build-image action's), reads `GCC_<v>` from `docker/pins.env`, then runs
  `scripts/build-test.sh --pass-env LDFLAGS --pass-env CCACHE_MAXSIZE -- scripts/build.sh
  --configure --build --install --cudf_ROOT /opt/cudf --gcc-version "$gcc" --compiler-launcher
  ccache`, with `LDFLAGS=-Wl,-rpath-link,/opt/cudf/lib` and `CCACHE_MAXSIZE=2G`. Then `ccache -s` in
  the image, with the same bound so its report shows 2 GiB, not ccache's default 5. The old
  `CCACHE_COMPILERCHECK=content` went: it was for conda's gcc reinstalled each run, and the image's
  compiler is fixed by its digest. The ccache is the build-image action's (`ccache: 'true'`, key
  `ccache-img-<cudf>-<sha>`).
- Step 2, sf1 byte-identity (dev). Two fresh `git clone --shared` copies at `cd63a421` in
  `/tmp/co-t4/{img,host}`, not `git worktree add`, which would write into the main repo's git dir.
  The testdata scripts are unchanged by this task. Host: the release zip as pipeline.yml had it (sha256
  `1f2fa724…80cf`, the pin), `HOME` a scratch dir so the extensions were fetched fresh, as in CI.
  Image: `scripts/build-test.sh -- testdata/generate_testdata.sh --bench tpch` and `tpcds` in
  `:cudf-25.02-in-50a0c1d7779e`. Both 32 files, identical, aggregate (sha256 of the
  `sha256sum tpch.sf1/*.parquet tpcds.sf1/*.parquet` listing)
  **`0e41c31d4930212fa8cfd610fd244ff93e6b0d7713bdec9a3d2a17227837a492`**, also the main worktree's
  own sf1. Regenerated from empty twice more through the new step's text, as uid 1001: in the 25.02
  image and in the 25.10 image. Same aggregate both times. `generate_pbench.sh --check` passed each time.
  The host clone was removed after.
- Step 3, FFI runtime path: **`LD_LIBRARY_PATH: /opt/cudf/lib` is needed**, and is set on both
  Rust steps alike. Without it `cargo test -p peacockdb-ffi --test test_ffi` exits 127:
  "libcudf.so: cannot open shared object file". With it, 4 passed, and nothing recompiled between a
  `--no-run` without it and a run with it. The cause: peacockdb-ffi's build script runs
  `cmake --install` into cargo's `OUT_DIR`, and that `libpeacock_gpu.so` has no RUNPATH. The ctest
  binary keeps its build-tree RUNPATH `[$ORIGIN:/opt/cudf/lib:]`, so ctest needs nothing.
- Runner-shaped local runs. The image clone was made self-contained (`git repack -a -d`, alternates
  dropped), its cudf submodule taken from the local shallow module, and the clone chowned to 1001:118.
  A fake `id` on PATH answering 1001/118 made the unmodified build-test.sh run `docker run --user
  1001:118`. The scratch registry and ccache were 1001's. Each step's `run:` was executed as rendered
  from the edited files (`bash -eo pipefail`, the job's and step's env), by
  `/tmp/co-t4/tools/step.py`:
  - 25.02 (`:cudf-25.02-in-50a0c1d7779e`): cpp-build 75 s cold, 85 ccache misses, gcc 12.4.0 and
    nvcc 12.9.86; ctest `1 of 1`; Generate SF-1; `test_duckdb_minimal.py` 9 passed (105 s);
    `batching_report.py --check`; validate 29/29 and 31/31; `test_ffi` 4 passed.
  - 25.10 (`:cudf-25.10-in-6bbfdd6dbd8f`, pulled for this): cpp-build 108 s cold, gcc 14.2.0; ctest
    `1 of 1`; Generate SF-1; validate 29/29 and 31/31; `test_ffi` 4 passed (166 s cold build). The ffi
    rung, as the two Rust steps run it: `cargo test --no-run -p peacockdb-core --lib` 503 s cold,
    target-img-25.10 then 4.1 GB (debuginfo=0, default features, plus test_ffi), and `--lib --
    ffi_tests::` 5 passed, 1211 filtered out, the baseline's "26.02" line, with no `Compiling`.
  - Warnings, none new: two `-Wstringop-overflow` from libstdc++'s `stl_algobase.h` on both gccs, and
    on 25.10 `cudf::round(…, rounding_method, …)` deprecated (`cpp/src/expr.cpp:753`), as Task 1 found.
  - Not run locally: the rust-only steps (`--lib`, `test_cpu_corpus`, the three golden targets, the CLI)
    and the exec-model step. None of them sees cuDF, and Task 2 ran the same targets in the 25.02 image
    (1209 + 2 ignored, 2451, 66, 3, 41). A cold CI-shaped target for them would not fit dev's disk
    beside the image.
- Checks on the edited files: YAML parses (pipeline.yml, images.yml); `bash -n` of all 25 `run:`
  blocks of dataset-matrix and the three actions; shellcheck (the actionlint image's) over them:
  only SC2129 in `cpp-build-conda`, HEAD's text. actionlint: the six findings HEAD has, none in
  these jobs (pipeline.yml:363 is cpp-build-2502's staging step).
- #129 closed: moved to `archive/archived-tickets.md` Done with a closing note. Links that followed:
  #94's in the archive, #244's in system-hardening.md (and its "a CI leg that is really 26.02"),
  performance.md's two 25.10a / 26.02-leg lines, `ci_coverage.rs`'s doc comment, and the comment
  in `cpp/tests/cpu/test_executor.cpp:555` (a test comment, not an image input: only
  `cpp/CMakeLists.txt`'s flatbuffers tag is). tickets.md: testinfra 5 → 4, 93 → 92 open. Left as
  they are: the dated reports under `llm-wiki/reports/bugfix-proposals/` that say "26.02 leg", and
  the links to `testinfra.md#t129` in the frozen spec, `tasks.md:201` and `verify-26.02.md:6`,
  which now dangle. tasks.md changes only with a state, and the spec is frozen.
- build-test.md: the dataset-matrix bullet rewritten (the image per leg, the two actions, the
  `LD_LIBRARY_PATH` reason, test_duckdb_minimal on 25.02), the cuDF-scope bullet, the "Two traps"
  shell line, the test table's DuckDB benchmark row `manual` → `dataset-matrix (25.02 leg)` and the
  header's by-hand list, and the CI wiring guard row (+4, 24). Totals: Rust 6313, grand 8237.
- dev's disk: removed `peacockdb-build:cudf-25.02-in-50a0c1d7779e` (`sha256:b8da5a94553c…`, the
  ghcr digest) for room; pulled `:cudf-25.10-in-6bbfdd6dbd8f` (338 s). Then removed
  the clone, its scratch registry and ccache, and the 25.10 image (`sha256:9d862501b165…`, the
  published digest), and re-pulled the 25.02 image. Scratch logs stay in `/tmp/co-t4/` (no data).
- What could differ on the runner:
  - **Disk.** The gate checks 25 GB free before the pull. After it the leg holds the image (about
    15 GB in `docker images`), the target dir, `cpp/build` and `cpp/install`, the cudf submodule,
    sf1 (0.8 GB), ccache and the registry, and Swatinem's save writes a tarball of the target. The
    old job had all of that beside a smaller rapids image. The cost-report job's "Free the disk" log
    in run 42379249690 gives the margin.
  - Cold caches on the first run: a new Swatinem key per leg (`rust-image-<v>`) and a new ccache
    key (`ccache-img-<v>-…`), so both legs compile everything, as the first local runs did.
  - Swatinem runs the runner's own cargo for its key and cleanup, as in cost-report.
  - The checkout's git objects and submodule are the runner's (uid 1001, same owner as the
    container's user), so git needs no `safe.directory`; locally the same held for the clone.
  - The 25.10 image comes from ghcr by digest (image-ref.sh's remote branch, proven anonymous in
    Task 3); locally image-ref.sh found the pulled local tag.

## Task 5

- Dispatched 2026-11-07 at Steps 1-4 (developer, dev, no VM). All in `scripts/env/run-gpu.sh`,
  `scripts/env/tests/test_run_gpu.py` (50 → 68) and the fake docker in `gpu_host.py` (a detached
  container's state, `--label`, `inspect`, `logs --tail`).
- Red first, each for the missing feature: 16 of the 18 new tests failed on the unchanged script
  (`prepare takes no argument: --no-sf40`, `unknown verb status`, `run --detach: neither list…`,
  the tag pulled instead of the given ref, two rsync calls instead of four streams). The two that
  passed already are refusals (`test_run_detach_refuses_a_staged_binary`, and the bad-argument
  lines added to `test_run_gpu_refuses_an_unknown_verb_or_option`), now refused for the right reason.
- Mutation checks, the script edited and restored each time: no `--checksum` → the same-size
  rebuild and the flags test red; the lib stream not backgrounded → `four_streams_at_once` red (its
  fake rsync waits for 4 calls at once); sf40 links required again → `runs_cis_sequence_on_nebius_gpu`
  and `ship_after_prepare_no_sf40_links_sf1_alone` red; `PEACOCK_BUILD_IMAGE` dropped →
  `prepare_pulls_the_given_images` red; `vm_nsight` run under `--no-sf40` →
  `prepare_no_sf40_skips_the_nsight_setup` red; a single try → `retries_a_failed_stream_and_resumes`
  and `gives_up_after_five` red. Each suite run takes about 2 min; six in one call outran its limit.
- **Beyond the plan, needed by Task 6:**
  - **`PEACOCK_BUILD_IMAGE`**, beside the plan's `PEACOCK_RUN_IMAGE`. `prepare` pulls the build image
    too (sf1 is generated in it), and on this branch master's `:cudf-25.02` build image has other
    inputs than the tree (Task 1 moved them: tag digest `2a0c8441…`, inputs tag `b8da5a94…`). So
    `prepare` from CI would refuse it, exit 2. **Task 6's step 5 must also write
    `PEACOCK_BUILD_IMAGE=$(scripts/env/image-ref.sh build 25.02)` to `$GITHUB_ENV`.**
  - **`ship` links sf40 only when the VM has it** (`?` marks an optional set in `vm_ship_ready`;
    an absent one's stale link is dropped). Without it, `ship` after `prepare --no-sf40` refused,
    exit 2, "no …/tpch.sf40 on the VM: run prepare first". Pinned by
    `test_ship_after_prepare_no_sf40_links_sf1_alone` and `test_run_gpu_runs_cis_sequence_on_nebius_gpu`
    (Task 6's three calls, `--instance-id computeinstance-e00tnrse7ayntnzcyt`, no profile,
    `--install cpp/install` relative).
  - **`--no-sf40` skips nsys too**, not only the counters and the `/opt/cudf` copy: `vm_system` is
    split, and `vm_nsight` (nsys, counters, exit 4) runs only without the flag. nsys is Nsight's alone.
  - Nothing else refused nebius-gpu: `--instance-id` was required, never compared with an H100 id,
    and `nebius.sh` takes nebius-gpu's id (alias `nebius-gpu`). The refusal message and the header no
    longer say nebius-gpu "runs no sf40"; they say the id is required, nebius-gpu's too.
- Shapes Task 6 relies on:
  - Container `peacock-<basename of the repo root>`; on the runner `peacock-peacockdb`, as the cleanup
    step names it. `run list` follows the log and exits as the run did (0, 2, else 1).
  - `ship` streams: `lib/` and `rust-tests/` each `--delete`; the rest of the install `--delete
    --exclude=/lib/ --exclude=/rust-tests/` (bin/, rust-benchmarks/, include/…); testdata as
    `--files-from` a file of `git ls-files -z -- testdata`, not a pipe, so a retry reads it again.
    The install must hold `lib/` and `rust-tests/` (ci-ship's check). 5 tries, attempt × 15 s
    (`RUN_GPU_SHIP_PAUSE` overrides the 15, for the tests). Each stream prints `==> ship: <name>
    stream: done in N s; Total bytes sent: …`, which the job log can be read for.
  - `run --detach <mode>` sets the label `peacock.log=<mode>` (a staged run sets its name);
    `status` keeps an ended run's whole log in `~/peacock/<i>/logs/<label>.log` and does not remove
    the container: the next `run` clears it, as before. Exit 5 running, 2 none, 0/1 as exited.
- Not proven here: the L40S's driver against `DRIVER_MINIMUM_25_02=535.104.05`, Docker and the
  toolkit on nebius-gpu, and its disk; Task 6 Step 5's by-hand `prepare --no-sf40` finds them.
- `scripts/nebius/tests/test_ci_ship.py`, case by case (the file goes in Task 6):
  - `test_run_dir_holds_the_tree_with_symlinks_and_exec_bits` → `test_ship_keeps_symlinks_and_exec_bits`
    (the testdata half is the existing `test_ship_sends_the_committed_testdata_and_links_the_data`).
  - `test_the_cache_persists_and_mirrors_each_run` → the install half is the existing
    `test_ship_keeps_what_a_run_wrote` (a file the install lost is gone on the VM) and the new
    `test_ship_sends_a_rebuilt_file_of_the_same_size_and_time`. **Dropped: a golden removed from the
    tree is not removed on the VM.** run-gpu.sh ships committed testdata without `--delete`, so that
    a run's results, record and answers there survive a ship; mirroring testdata would delete them.
  - `test_a_rerun_of_the_same_run_dir_starts_clean` → `test_ship_restores_a_committed_file_a_run_changed`.
    There is no per-run copy: `~/peacock/<i>/` persists, the install is mounted read-only, and a
    committed file a run changed is sent again by checksum.
  - `test_a_stream_that_keeps_failing_is_red_and_no_run_dir_is_made` → `test_ship_gives_up_after_five`
    (exit 1, the stream named, no final `==> ship:` line).
  - `test_a_flaky_stream_recovers_on_retry` → `test_ship_retries_a_failed_stream_and_resumes`.
  - `test_every_call_compresses_resumes_and_mirrors` → `test_every_ship_rsync_compresses_resumes_and_checksums`;
    plus `test_ship_runs_four_streams_at_once`, which ci-ship never had.
  - `test_refuses_a_run_dir_that_is_the_cache_or_outside_home` → **dropped**, no cache and no run
    dir: `--dir` is the only path input, refused unless a plain name (`--dir ../up` in
    `test_run_gpu_refuses_an_unknown_verb_or_option`).
  - `test_an_install_without_lib_is_refused_before_any_transfer` →
    `test_ship_refuses_an_install_without_lib_or_rust_tests`.
- build-test.md "The H100's device run": the `--instance-id` sentence, the two image variables,
  `prepare --no-sf40`, ship's streams and its retry, `run --detach`, `status`, exit 1 and 5. The
  Images (Python) row: the new cases, 296 → 314; Python 1638 → 1656, grand 8237 → 8255. "The
  scripts" still names `nebius-test.sh`, which Task 6 deletes.

## Task 6

- Dispatched 2026-11-07 at Steps 1-3 (developer, dev, no VM). Checkpoint reported, not committed:
  `containers-only: gpu-tests ships and runs on nebius-gpu through run-gpu.sh; the old runner goes`.
  The plan's Step 7 box was pre-ticked by mistake: unticked. Steps 4-6 are next (push and PR run,
  the by-hand `prepare --no-sf40` on nebius-gpu, the CI device run).
- **Red first.** The guard, with the new readers and tests over the old pipeline: 51 passed, 4
  failed, each for its reason: `every_listed_job_runs_in_the_build_image` (cpp-build-2502's
  `container:`, `rapidsai/`, `conda install`, no build-image step),
  `the_gpu_cleanup_removes_the_run_container`, `the_gpu_step_cannot_report_success_over_a_failure`
  (the step ran `nebius-test.sh`), `the_pipeline_stages_through_stage_install`. Python:
  `test_sync_never_generates_data` red on the ssh carrying `generate_testdata.sh`;
  `test_stage_install_no_release_bench_builds_no_release_benchmark` red on "unknown argument".
- **Green.** Guard 55 passed, 0 failed (HEAD 51: runners.rs 6 → 9, ci_images.rs 8 → 9). Nebius
  tests, in the image as CI runs them: 11 files, 153 passed (HEAD 171: `test_ci_ship.py` 8 and
  `test_nebius_test.py` 11 deleted, +1 `test_sync_never_generates_data`). `scripts/env/tests`, as
  CI's step lays them out: 17 files in the image and 3 on the host, 280 passed (Images row 314 =
  280 + 34 by hand). `test_run_tests.py` (needs the run image, not on dev) was not run: one case
  deleted, the file parses.
- **Counts that move in cost-report's job** (for Step 4's comparison against `## Counts before`):
  `test_source_formatting` 51 at HEAD → 55 (the baseline's 41 is before Tasks 2-5); the Nebius step loses the
  `test_ci_ship.py` and `test_nebius_test.py` lines (8 and 11) and `test_nebius_instances.py` goes
  11 → 12; `test_stage_install.py` 6 → 7. The wiki: CI wiring row 24 → 28, Nebius row 171 → 153,
  Images 314 (one case out of `test_run_tests.py`, one into `test_stage_install.py`), by hand 35 →
  34; grand 8255 → 8241 (Rust 6317, Python 1638).
- **Guard readers, as built (where they differ from the plan's words).**
  - `gpu_job_staged_targets()` reads stage-install.sh's one-line `GPU_TESTS=( … )` array, not a
    `for t in …` line: the plan's "shape the guards read" was pipeline.yml's, and stage-install.sh
    (new-hosts') already had the array. shad-gpu's `RUST_TESTS` goes through the same reader.
  - CI's lib is `stage_cargo_lib_binary <name> … --features gpu` in stage-install.sh (name literal
    or `$VAR`), and its rung run-tests.sh's `LIB_STAGED`/`LIB_RUNG`. The old pipeline readers
    (`staged_by_name_of`, `lib_rung_of`, `gpu_job_lib_rung`) and their fixtures went. Two runners
    now, not three: CI is the image path. `the_staged_lib_is_the_crates_own_test_binary` holds
    stage-cargo.sh's `stage_cargo_lib_binary` to `peacockdb_core lib … --lib` and `-p
    peacockdb-core` (the old `line.contains("-p peacockdb-core")` check, moved).
  - `the_pipeline_stages_through_stage_install` also asserts build-test.sh runs stage-install.sh,
    so the chain pipeline → build-test.sh → stage-install.sh is held at both links.
  - Beyond the plan: `the_run_tests_failure_reader_refuses_each_way_to_hide_a_failure` breaks
    the committed run-tests.sh in a copy five ways (`exit 0`, a judge dropped, the bench unjudged,
    no segfault grep, an exported library path), each refused.
- **Pipeline.** cpp-build-2502: checkout (submodules), build-image `cudf: "25.02"`, `ccache:
  'true'`, Swatinem `rust-gpu-build-image` over `target-img-25.02`, `scripts/build-test.sh --gpu
  --build --no-release-bench`, pack, upload; job env `CMAKE_GENERATOR` and `RUSTFLAGS` as
  dataset-matrix's. build.sh turns ccache on by itself when the image has it (it does), so the
  ccache restore is used. No `LDFLAGS` rpath-link: new-hosts' CPU-VM build ran stage-install.sh in
  the same image without it and its `bin/peacock_*_tests` linked and ran on the H100.
  gpu-tests: the plan's ten steps, plus `PEACOCK_BUILD_IMAGE` (Task 5) in the same step as
  `PEACOCK_RUN_IMAGE`. Each ref is assigned before the `>> $GITHUB_ENV`, so a refusal of
  image-ref.sh fails the step (inside `echo "X=$(…)"` it would not). `REMOTE_DIR` went.
  actionlint: one finding, the `changes` job's pre-existing SC2001; HEAD also had an SC2086 in the
  old staging step, gone. YAML parsed; `bash -n` of every `run:` of both jobs and the Nebius step.
- **shellcheck.** The plan's new list (`build-test.sh`, `run-gpu.sh`, `image-ref.sh`,
  `stage-install.sh`) failed under plain `shellcheck`, rc 1, on infos only: SC1091 (sourced files
  not followed) and SC2012 (`ls cudf-*.spec`) in build-test.sh and image-ref.sh. CI's line is now
  `shellcheck -x` (clears SC1091), and the two `ls` lines became a glob loop, the known-version
  message pinned first by an assertion in `test_build_test.py` and `test_image_ref.py` (a mutant
  with the space on the wrong side went red). Lowering the severity was rejected: it would weaken
  the check on the four scripts already listed. shellcheck 0.11.0 (actionlint image), rc 0.
- **nebius-sync.sh** keeps the rsync, its retry, the options and `--no-data` (a no-op). Gone: the
  sf1 generation and its `peacock-env.sh`, and the `P /.gpu-test/` filter, which protected
  nebius-test.sh's run state; `.gitignore`'s `.gpu-test/` went with it.
  `test_sync_passes_the_instance_through` lost its "sf1 generation goes through nebius.sh too"
  half; `test_sync_no_data_skips_generation` lost its "without --no-data the generation runs" half.
- **Deleted:** `scripts/nebius-test.sh`, `scripts/nebius/ci-ship.sh`,
  `scripts/nebius/tests/test_ci_ship.py`, `scripts/nebius/tests/test_nebius_test.py`,
  `.github/actions/cpp-build-conda/action.yml`, and `test_run_tests.py`'s
  `test_the_list_rules_are_nebius_test_s` (its comparison partner is gone; the behaviour it held is
  `test_list_runs_the_l40s_list_with_each_binary_s_arguments` and runners.rs).
- **run-tests.sh list against nebius-test.sh: they agree** on what CI runs. Same sf40 pair
  skipped; every executable `bin/peacock_*_tests` with no arguments; every executable
  `rust-tests/*` with `--nocapture --test-threads=1`, then `gpu_tests::` for
  `peacockdb_core_gpu_lib`, then `--skip bench_` for `peacock_gpu_benchmarks`, in that order; the
  same 0-test regexes; the same `Segmentation fault|core dumped`; every binary runs after a
  failure; the library path per command, never exported (`install/lib` then the cuDF lib). Where
  they differ, none touching CI's list: an empty glob is exit 2 before anything runs (was rc 1 at
  the end); a death by a signal is named as one (both fail on its non-zero exit); `PCK_TEST_FILTER` is `filter`
  mode's, and `list` refuses it set (nebius-test.sh applied it in `run`); `filter` fails a filter
  that matches nothing. No run-image input changed: `image-ref.sh run 25.02` →
  `peacockdb-run@sha256:3a69a03c7b69…` (tag `cudf-25.02-in-4f971122ddad`, Task 1's); `build 25.02`
  → `cudf-25.02-in-50a0c1d7779e`. C++ counts from the sources: cpu 30+34+3 = 67, gpu 5 (+1
  `DISABLED_` probe), join 56, plan 136, the baseline's four.
- **What could differ on the VM or the runner (Steps 5-6).**
  - The L40S's driver against `DRIVER_MINIMUM_25_02=535.104.05`, Docker and the toolkit on it,
    its disk: unknown until Step 5. nebius-gpu's 96 GB now gets the run image (~8 GB), the build
    image (~15 GB), sf1 under `/var/lib/peacock` and `~/peacock/peacockdb/`. Nothing prunes an
    image of older inputs: each publication of new inputs adds a pull. `~/peacockdb-ci/` (the
    old cache) can go.
  - sf1 is now generated by the build image's pinned DuckDB 1.5.4; the old per-run sf1 used the
    VM conda env's DuckDB, of a version not recorded. Correct now; a count could move if a device
    test skipped on the old data.
  - The rust binaries' log order follows the glob's collation: the run image's locale may order
    `peacock_gpu_benchmarks`/`peacockdb_core_gpu_lib` otherwise than the VM did. Compare counts
    per binary name, not by position.
  - testdata on the VM is all committed testdata, not ci-ship's named subset, and not mirrored:
    a file deleted from the tree stays on the VM (Task 5's dropped case).
  - The container and dir are `peacock-peacockdb` and `~/peacock/peacockdb/`, from the checkout's
    basename. A developer running `run-gpu.sh` on nebius-gpu from a checkout named `peacockdb`
    shares both with CI, and CI's cleanup would remove that run's container.
  - The runner's shellcheck (apt's, older than 0.11.0) on `-x` and the two glob loops; the
    runner's rsync for `--compress-choice=zstd` (ci-ship used the same flags).
  - cpp-build-2502's cold build: the C++ (ccache restored from the 25.02 leg's key) and a
    `--features gpu` cargo build at `CARGO_BUILD_JOBS=3` (build-test.sh's throttle on a 16 GB
    runner).
- **Step 5, `prepare --no-sf40` on nebius-gpu by hand (2026-11-07, developer): exit 0, first try.**
  `PEACOCK_BUILD_IMAGE=…peacockdb-build:cudf-25.02-in-50a0c1d7779e` and
  `PEACOCK_RUN_IMAGE=…peacockdb-run@sha256:3a69a03c7b69…`, from `image-ref.sh` as the CI step
  sets them; `timeout 2400 scripts/env/run-gpu.sh --instance-id computeinstance-e00tnrse7ayntnzcyt
  prepare --no-sf40`. Wall time 569 s (17:32:35 → 17:42:04 UTC), almost all of it the two pulls
  and sf1's generation. No code change.
  - What the VM already had: Docker, `nvidia-ctk` and the `nvidia` runtime, so `vm_system`
    installed nothing and restarted nothing. Driver 580.173.02 on the L40S, above
    `DRIVER_MINIMUM_25_02=535.104.05`: no exit 3. No exit 4 (`--no-sf40` skips `vm_nsight`).
  - `df -h /`: 22 GB free of 96 GB at start, 35 GB after the cleanup below, 20 GB after
    prepare. The images took 15 GB (`/var/lib/docker`): the build image 10.0 GB
    (`sha256:1727527707dd…`, repo digest `b8da5a94553c…`), the run image 5.1 GB
    (`sha256:a88a44cefed4…`). They share one base layer. sf1 took 774 MB under
    `/var/lib/peacock/d608b4d0ede659a7` (tpch.sf1 367 MB, tpcds.sf1 408 MB). The kit went
    to `~/peacock/peacockdb-beta/` (88 KB). CI's prepare uses `~/peacock/peacockdb/` and finds the
    same sf1 dir already there.
  - **Removed:** `~/peacockdb-peacockdb-beta/target-cudf-rapids-cuda-12.2` (14 GB). It was the old
    conda build's target dir in this chain's own synced tree, last written 2026-11-04. 22 GB was
    above the 20 GB line, but prepare needed about 16 GB, which would have left about 6 GB free.
  - **Kept, on purpose:** `~/peacockdb-ci/` (755 MB, modified 2026-11-07). Master's `gpu-tests`
    still ships into `~/peacockdb-ci/cache` through `ci-ship.sh` (master's pipeline.yml line 533).
    Deleting it would make master's next device run resend everything. build-test.md "Disk" says
    "nothing reads now". That is true only once this branch is on master. Until then, do not
    remove it.
  - **Disk left for Step 6:** 20 GB free. CI's ship adds the install and testdata under
    `~/peacock/peacockdb/`: about 1–2 GB, going by the old runner's cache of the same install and
    testdata, 755 MB, and all committed testdata, 0.85 GB. That leaves about 18–19 GB. Other large users if room is needed later:
    `~/miniforge3` 17 GB (the conda envs and a 13 GB `pkgs` cache; removing it is the human's
    call), and this tree's own stale `~/peacockdb-peacockdb-beta/cpp/install` (4.9 GB, the old
    conda install, now unread). `~/peacockdb/` (991 MB) is a tree of unknown owner and was left alone.

## Task 7

- Dispatched 2026-11-07 at Steps 1-3 and Step 4's dev half (developer, dev, no VM). Checkpoint
  reported, not committed: `containers-only: the manual workflows run in the build image; every CI
  job is accounted for`. Next: the coordinator's `gh workflow run exec-model-corpus.yml --ref
  ENS-containers-only -f filter=q1`.
- **Red first.** `every_job_runs_in_the_image_or_says_why` on the unchanged workflows: 56 passed,
  1 failed, naming `corpus`, `call-cost` and `validate-large` as unlisted, the three `pip install`
  lines and `dtolnay/rust-toolchain`. Green after the move: `test_source_formatting` 57 passed
  (HEAD 55, +2); `--bin cost-report` 49 passed.
- **The guard, as built, beyond the plan.**
  - It reads every `.github/workflows/*.yml`, not only the three, so a new workflow cannot slip
    past it. `HOST_JOBS` therefore also names images.yml's `image` and nebius-watch.yml's `watch`.
  - A host job's `run:` lines are held to the image rule too: a bare `cargo`, `python3`, `ctest`,
    `cmake`, `scripts/build.sh` or `testdata/` command in one is refused. Without that, `HOST_JOBS`
    would be a way out of the rule rather than a list of reasons. All five host jobs pass it.
  - `off_image_problems` (no `conda install`, `pip install`, `rapidsai/` or
    `dtolnay/rust-toolchain` outside a comment) runs over every workflow line and every
    `.github/actions/*/action.yml`. `container:` is refused anywhere.
  - Each rule was checked by a mutation (`/tmp/co-t7/mutate.py`): `on:` keys read as jobs, the
    host-job check dropped, the container check dropped, the stale-entry check dropped, the
    both-lists check dropped, and a `pip install` line in nebius-setup's action. All six went red.
    The container mutation first stayed green: the doctored case used `rapidsai/base`, and the
    off-image rule named that line instead. The case now uses `ubuntu:24.04`.
  - `ci_images.rs` was not rustfmt-clean at HEAD (Task 6's lines). The whole file is now formatted.
- **`call-cost` moved too.** The plan's Step 2 names only `corpus`, but `call-cost` is
  exec-model-corpus.yml's second job and had `pip install pandas`. It now uses the action, then
  `scripts/build-test.sh --pass-env AWS_ACCESS_KEY_ID --pass-env AWS_SECRET_ACCESS_KEY --
  scripts/calibration/fetch_record.sh tpch.sf40` (the image's aws) and the check through
  build-test.sh. `fetch_record.sh` needs no endpoint variable: with `AWS_ENDPOINT_URL` unset,
  `calibration-bucket.sh` uses the fixed Nebius endpoint and region. A `-f filter=q1` dispatch
  skips this job (`if: inputs.filter == ''`), so only a no-filter dispatch runs it in CI.
- **corpus.** The action with `cudf: "25.02"`. Steps: `build-test.sh -- cargo build --release -p
  peacockdb-dphyp`, then `duckdb --version` and both `generate_testdata.sh` benches through
  build-test.sh, then the four files through `build-test.sh --pass-env PEACOCK_DPHYP_LIB`, with
  `PEACOCK_DPHYP_LIB=$PWD/target-img-$PEACOCK_CUDF/release/libpeacockdb_dphyp.so`, as the pipeline
  has it. `PCK_SHARD` needs no `--pass-env`: it is in build-test.sh's `FORWARDED` list. `DUCKDB`
  is dropped: `corpus.py`'s `duckdb_binary()` finds the image's `duckdb` on PATH. Its error text
  named the curl download that went, and now names build-test.sh. `defaults.run.shell: bash` on
  both jobs. No Swatinem cache: DPhyp has no dependencies and builds in 7 s.
- **validate-large.** The ssh steps are unchanged. The action comes right before the S3 step,
  after the shad-gpu half, so a failed ssh half pays no disk freeing and no pull. The S3 step is
  s3-datasets' line. `pip install awscli` and the pyarrow/numpy self-heal are gone.
- **Env for the S3 checks.** `check_s3_datasets.py` passes `--endpoint-url` and `--region` to
  every aws call and to pyarrow, so it needs only the two keys, as s3-datasets passes them.
  `peacock.env` also sets `AWS_ENDPOINT_URL` and `AWS_REGION`. build-test.sh does not pass them,
  so the container saw what CI's sees.
- **Step 4, dev half (2026-11-07):** the plan's command, as written, exit 0: tpch-sf40 PASS (8
  tables), tpch-sf200 SKIP (no manifest, 0 objects), tpcds-sf40 PASS (24), tpcds-sf200 PASS (24).
  **The ssh half was not run**: validate-large dispatches only against shad-gpu, which is down.
- **Runner-shaped local runs** (dev, 2026-11-07). A `git clone --depth 1 --no-hardlinks` of HEAD
  with the edits laid over it, owned by 1001:118, an empty cargo registry and ccache of 1001's, the
  build-image action's exports and a fake `id` answering 1001/118 (`/tmp/co-t7/tools/`). Each
  step's `run:` executed as rendered by `step.py` (Task 4's, extended for `matrix.shard` and
  `inputs.filter`, and leaving `${{ secrets.* }}` to the caller's environment).
  - corpus: DPhyp 7 s; sf1 237 s, 32 files, aggregate `0e41c31d…a492` (Task 4's); then all three
    shards with `filter=q1`, each exit 0: shard 0 737 s, shard 1 510 s, shard 2 686 s. Per file,
    shards 0/1/2: answers 7/7/7, dynamic filters 5/5/4, reassembled 7/7/6, optimized 7/7/6, so 75
    tests and none failed. Every shard of every file selects at least 4 tests under q1.
  - call-cost, with only the two keys from `peacock.env`: the record fetched, 254160 rows, sha256
    `1013141a…def2` as pinned, written by uid 1001; the check 1 passed.
  - The clone, its registry and ccache were removed. Logs stay in `/tmp/co-t7/`.
- **Checks on the edited files.** YAML parses (both). `bash -n` of all 9 `run:` blocks. actionlint:
  4 findings (SC2029 ×3, SC2086), the same 4 HEAD has, all in the shad-gpu ssh steps this task
  leaves alone. No shell script changed, so no shellcheck run.
- **Wiki.** build-test.md: the validate-large.yml bullet says where its S3 step runs, a new
  exec-model-corpus.yml bullet, and the CI wiring guard row (+2, 30, two new links). Totals:
  Rust 6319, grand 8243.
- **What could differ on the runner.**
  - Disk: each of the three shards and call-cost now frees the disk and pulls the 10 GB image, a
    few minutes each, before work the old jobs began at once. The gate is `need_gb=25`; sf1 is
    0.8 GB beside the image.
  - Time: dev's shards took 8-12 min each on a larger CPU. A runner shard will be slower. The
    runs have no job timeout beyond GitHub's 6 h.
  - The cargo registry is the runner's `~/.cargo/registry`, empty. With an empty registry the local
    run updated the crates.io index and downloaded no crate (DPhyp has no dependencies): 7 s.
  - The uid: proven as 1001:118 locally. The runner's checkout is owned by its own uid, as the
    clone was.

## Task 8

- Dispatched 2026-11-07 at Steps 1-2 (developer, dev, no VM). Checkpoint reported, not committed:
  `containers-only: the conda installers and machine-setup scripts go`.
- **Nothing executed them.** Before the `rm`, nothing outside the five named them: no workflow, no
  action, no other script, no test, nothing in `.claude/` (the ignored ensemble scratch included).
  `init.sh` and `init_fedora43.sh` called `conda-install-cudf.sh`; nothing called
  `conda-install-cudf-2506.sh` or `create_verda_instance.sh`. No guard enumerates `scripts/`.
  The parent branch had the same picture.
- **What init.sh set up, and what covers it now.** A `build` user with passwordless sudo and
  root's `authorized_keys`: `provision-vm.sh` (the developer's user, sudo, `<login>`'s keys). The
  toolchain (gcc-14, CUDA 12.6, cmake 4.2.3, ninja, boost, openssl, zlib, patchelf) and the conda
  cuDF: the build image. Docker: `bootstrap-vm.sh` on a CPU VM, `run-gpu.sh prepare` (with the
  NVIDIA Container Toolkit) on a Nebius GPU VM. A developer's own machine: Docker by hand, one
  line added under build-test.md's build-command table. No script was kept as a Docker installer.
  `create_verda_instance.sh` booted a verda instance from an existing disk; verda keeps its disk,
  and `list_verda_instances.sh` stays.
- **Edits beyond the deletion**, so Step 2's second grep holds only what it should:
  - `testdata/generate_testdata.sh`: the DuckDB version-mismatch hint named `conda install
    duckdb-cli`; it now names the build image (`scripts/build-test.sh -- testdata/generate_testdata.sh
    ...`). Checked with a fake `duckdb` printing `v0.0.0`: old text before, new text after, exit 1.
  - `scripts/create_nsys_profile.sh`: the header's "another host" example was verda-gpu with a
    miniforge env "provisioned by build-test.sh" (untrue since Task 2). It now says a Nebius GPU
    VM's cuDF root is `~/peacock/cudf-<v>/`, which `run-gpu.sh prepare` lays down; the die
    message's example likewise. `test_run_gpu.py` (which runs this script) 69 passed.
  - `.gitignore`: the `target-cudf*/` comment named `scripts/build-test*.sh`; it now names
    shad-gpu's `lib/shadgpu-env.sh`, and the old host builds' leftover dirs.
- **Step 2's second grep, what stays and why** (tracked files; the literal `grep -r .` also
  walks ignored build output: `target-cudf-rapids-cuda-12.2/`, `cpp/build/`, `target/`,
  `.claude/ensemble/`, 358415 lines, none tracked):
  - shad-gpu: `scripts/lib/shadgpu-env.sh:13,72`; `.gitignore:4` (its target dir's name).
  - The guard's own pattern: `ci_images.rs:76,564,594,944` and its row, `build-test.md:1233`.
  - **Task 9's:** `build-test.md:3102-3114`, "The same six steps on a 26.02 host" (verda-gpu
    through `build-test.sh --host`, gone since Task 2, and a miniforge `--remote-cudf-root`).
    Task 9 Step 1's grep (`build-test\.sh --host`) catches it.
  - Records, not instructions, left as written: `llm-wiki/reports/` (18 lines, header paths cited
    by past reviews) and `llm-wiki/tickets/system-hardening.md` (#244, #260: the envs where
    things were measured).
  - `build-test-shadgpu.sh` and `setup-glibc.sh` never matched; the plan's list was wider than
    the grep.
- **Checks.** `scripts/build-test.sh -- cargo test -p cost-report --test test_source_formatting`:
  57 passed. `scripts/build-test.sh -- python3 scripts/env/tests/test_run_gpu.py`: 69 passed.
  `bash -n` on both edited scripts. Neither is in CI's shellcheck list; dev has no shellcheck.
  Scratch in `/tmp/co-t8/`.

## Task 9

- Dispatched 2026-11-07 at Steps 1, 2 (dev half) and 3 (developer, dev, no VM). Not committed.
- **Step 1, what the rewrite changed.** build-test.md is 28 lines shorter.
  - "Build and run" replaces "Containers": a short intro (one path; shad-gpu the exception), then
    "Images" and "CPU VMs" (the old Containers text) under it. Code comments that cited "Containers"
    now cite "CPU VMs": `bootstrap-vm.sh`, `lib/machine.sh`, `lib/peacock-config.sh`. None is an
    image input (`image_inputs` reads only `docker/` files, pins and two sources).
  - "Local build workflows and caches": one row per shape, all `build-test.sh`, the target-dir
    column gone (always `target-img-<v>`). The day-to-day loop and the one-off cudf command go
    through the image. Its prose had drifted under "Inspecting the tree"; it is back in place.
  - "CI structure": one paragraph on jobs in the image and the inputs tag; dataset-matrix's bullet
    no longer repeats it. 25.10 as CI-only is said once, in "Images".
  - "Nebius host": no conda sentence; the CPU-VM build says why it is a plain `docker run` (no
    `.git` on the VM); `--no-data` dropped from the commands (nebius-sync.sh ignores it).
  - "Remote hosts": verda and verda-gpu build with `build-test.sh` on themselves; golden regen is
    `UPDATE_CANONICAL=1 scripts/build-test.sh --rust-only --run`; the `--host`/`--push-*`/`--pull-*`
    text, the `/media/data` symlink notes and "Remote CPU runs ship binaries" are gone. A new
    "shad-gpu" subsection says it is the one exception (no Docker) and holds its own bullets (glibc
    patch, `--pull-results`, `--run-detached`, `PEACOCK_REMOTE_REPO`).
  - "Datasets": sf1 is generated in the build image (the command). Golden flow and gpu-result: the
    device tier is the L40S's, `run-gpu.sh … pull` brings its answers. Benchmarks: "The same six
    steps on a 26.02 host" is replaced by the H100 path; the multi-GPU build is `--cudf 26.02` in
    the image. "Memory constraints": `RUST_TEST_THREADS=2`, since `-- --test-threads` now means a
    command.
  - prompts.md: the verda dispatch rule (line 192), and "in the build image" on the dev-build line.
    The L40S bracket was already Task 6's. `.claude/agents/*.md` and `architecture.md` name no
    removed path: unchanged.
  - `scripts/env/check-build-image.sh`'s header said nebius-gpu "builds with its own scripts"; now
    "builds nothing". Comment only.
- **A failure that was the clone's, not the wiki's.** `scripts/build-test.sh -- cargo test -p
  cost-report` failed once, `both_base_arms_read_the_same_section_of_a_multi_section_file`:
  `git show HEAD:…` inside the image found no object. The `--shared` clone's alternates point at
  `/home/dmitry/workspace/peacockdb/.git/objects`, which the container does not mount
  (`unable to normalize alternate object path`). A worktree is fine: build-test.sh mounts its
  common git dir. `git repack -a -d` and dropping the alternates file (Task 4's recipe) fixed it;
  the rerun was green. No script change: a `--shared` clone is a test artifact, not a checkout the
  wiki describes.
- **A failure the wiki caused, fixed in the wiki.** The multi-GPU line first read
  `scripts/build-test.sh -- cmake --build cpp/build --target peacock_multi_gpu_tpch_tests`, on the
  default 25.02. It fails, exit 1: `multi_gpu.hpp:32` includes `rmm/mr/cuda_memory_resource.hpp`,
  and 25.02's rmm has it under `rmm/mr/device/` (`cpp/include/peacock/rmm_pool.hpp:13` says rmm
  flattened them after 25.02). That is why the old line said `cpp/build26`. The line now says
  `--cudf 26.02`. Not rerun: dev has only the 25.02 build image. The 26.02 one (about 15 GB in
  `docker images`) and a cold cpu build (about 15 GB of target) would not fit in dev's 29 GB free.
- **Two gcc warnings**, both builds: `-Wstringop-overflow` in flatbuffers' own
  `_deps/flatbuffers-src/src/reflection.cpp:650` (a fetched dependency). Not ours, not new.
- **Disk.** The clone, `/tmp/co-t9/clone`, peaked at 16 GB. Removed between shapes: its
  `target-img-25.02` after rust-only (14 GB) and, with `cpp/build` and `cpp/install`, after the cpu
  shape (15 GB). The whole clone was removed at the end; `/` back to 29 GB free. No image was pulled
  or removed. Logs stay in `/tmp/co-t9/logs/` (no data).

- **VM half (Step 2), a fix the recipes needed.** Every moving tag on ghcr differs from the image
  of this tree's inputs, and the two build images carry other inputs: master's `:cudf-25.02` and
  `:cudf-26.02` build images differ in `docker/build.Dockerfile` (Task 1's edit). The run images'
  moving tags carry the same inputs. So, as written, `run-gpu.sh prepare` (nebius-gpu and the H100)
  and `check-build-image.sh` would each pull master's build image (10 GB) and refuse it, exit 2. The
  CPU-VM `docker run` named `:cudf-25.02` outright. CI set the refs from `image-ref.sh` (Task 6);
  the developer recipes did not. Fixed before any VM started:
  - `check-build-image.sh` takes `$PEACOCK_BUILD_IMAGE` when set, as `run-gpu.sh` does; else
    `:cudf-<v>`. Test first: `test_check_build_image_pulls_and_checks_the_given_image` and
    `…_refuses_a_given_image_of_other_inputs`, both red (exit 1: the script pulled the tag, which
    the fake registry no longer held), then green; the file 8/8.
  - build-test.md: the L40S, H100 and CPU-VM recipes export the refs from `image-ref.sh` before the
    VM starts. A short paragraph after the L40S recipe says why. The CPU-VM `docker run` names
    `$PEACOCK_BUILD_IMAGE`. "The CPU-VM build" gains how another build runs there:
    `build-test.sh`'s in-image half in a `docker run` with `-e PEACOCK_IN_IMAGE=1`, in its own
    `--dest` dir. The multi-GPU build is given that way. The Images (Python) row: 314 → 316, Python
    1638 → 1640, grand 8243 → 8245.
  - prompts.md's L40S bracket: one clause, the refs before `start`.
  - Not changed: `run-gpu.sh`'s default stays `:cudf-<v>`. Resolving with `image-ref.sh` on every
    `run` would add a registry query to each of `bench-each.sh`'s per-case calls, which is H100
    time. Once this branch lands and `images.yml` moves the tags on master, the default works again.
  - After the fix, `scripts/env/tests/` the way CI runs it: 17 files in the image, 3 on dev, every
    one green (283 cases; `test_run_gpu.py` 69, `test_check_build_image.py` 8).
  - CI's shellcheck list (pipeline.yml:679) does not name `check-build-image.sh`. dev and the image
    have no shellcheck; `bash -n` is clean. Not added: outside this step.
- **Disk, VM half.** dev: removed `/tmp/co-t9vm/install-cpu128` (749 MB, the H100's staged
  install) and this worktree's `target-img-25.02` (15 GB) after the runs. The October host
  `cpp/build` and `cpp/install` went before the build (above). The new image-built `cpp/build` and
  `cpp/install` stay. nebius-cpu128: removed `~/peacockdb-beta-26` (16 GB) and the 26.02 build image
  `peacockdb-build@sha256:9199d3efd54c…` (re-pullable); the 25.02 inputs-tag image stays for the
  next CPU-VM build. Logs in `/tmp/co-t9vm/logs/` (no data).

## Notes for a restart

- PR #203, a draft against `ENS-tp4-sized`, opened after Task 2 so Task 3 on has PR runs. The board
  moves to `reviewing` only at the plan's last step.
- `images.yml` is on master (workflow "Images", id 409646666), from the merge of #193; Task 1
  Step 8's master check passes. A dispatch runs the ref's own copy of the file.
- dev's disk: 13 GB free on `/` at branch time; local images `peacockdb-build:cudf-25.02` (15.4 GB),
  `peacockdb-run:cudf-26.02` (10.2 GB), `peacockdb-run:cudf-25.02` (8.3 GB). The run images are
  re-pullable and may be removed to make room for Task 1 Step 6's 25.10 build.
- `testdata/calibration/hbm-parts/` in this worktree is an untracked leftover of tp4-sized: never
  stage it.
- verda: no address known (`list_verda_instances.sh` lacks `VERDA_CLIENT_ID`); treated as down.

## Counts before

The count baseline: `ENS-tp4-sized`'s run 42365843466 (workflow_dispatch at `b95d08a0`, every
job green, GPU included). Its last PR run, 42367917405 at `9058709c`, was documentation-only and
skipped every test job, so the dispatch, the parent's last full green run, stands in. Per job, the
lines matching `test result:`, `[  PASSED  ]` and the Python summaries, in log order (timings cut).
The cost-report job's two `1 runs …, 1 failed` lines are in the baseline as they are.

```
=== 0_GPU Tests (nebius-gpu).txt
[  PASSED  ] 67 tests.
[  PASSED  ] 5 tests.
[  PASSED  ] 56 tests.
[  PASSED  ] 136 tests.
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 650 filtered out
test result: ok. 748 passed; 0 failed; 0 ignored; 0 measured; 1216 filtered out
test result: ok. 1009 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
=== 2_CI Pipeline (cudf 25.02).txt
100% tests passed, 0 tests failed out of 1
4 passed, 0 failed
3 passed, 0 failed
2 passed, 0 failed
2 runs, 0 failed
1 runs, 0 failed
1 passed, 0 failed
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 41 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 66 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
== SUMMARY == 29/29 passed; 0 failed. (12 exhaustive, 4 sampled)
== SUMMARY == 31/31 passed; 0 failed. (21 exhaustive, 7 sampled)
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 1211 filtered out
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 1209 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out
test result: ok. 2451 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
=== 3_Cost report (coverage + ratio).txt
Ran 41 tests in 0.002s
Ran 29 tests in 0.122s
Ran 11 tests in 0.062s
18 passed, 0 failed
5 passed, 0 failed
4 passed, 0 failed
23 passed, 0 failed
28 passed, 0 failed
18 passed, 0 failed
5 passed, 0 failed
15 passed, 0 failed
9 passed, 0 failed
14 passed, 0 failed
4 passed, 0 failed
17 passed, 0 failed
72 passed, 0 failed
7 passed, 0 failed
2 passed, 0 failed
12 passed, 0 failed
2 passed, 0 failed
9 passed, 0 failed
3 passed, 0 failed
12 passed, 0 failed
2 passed, 0 failed
4 passed, 0 failed
26 passed, 0 failed
8 passed, 0 failed
6 passed, 0 failed
8 passed, 0 failed
4 passed, 0 failed
15 passed, 0 failed
9 passed, 0 failed
5 passed, 0 failed
3 passed, 0 failed
7 passed, 0 failed
5 passed, 0 failed
11 passed, 0 failed
10 passed, 0 failed
7 passed, 0 failed
10 passed, 0 failed
5 passed, 0 failed
8 passed, 0 failed
1 passed, 0 failed
7 passed, 0 failed
21 passed, 0 failed
19 passed, 0 failed
9 passed, 0 failed
3 passed, 0 failed
1 passed, 0 failed
18 passed, 0 failed
1 runs, 1 failed
1 runs, 1 failed
12 passed, 0 failed
41 passed, 0 failed
10 passed, 0 failed
6 passed, 0 failed
3 passed, 0 failed
17 passed, 0 failed
9 passed, 0 failed
12 passed, 0 failed
7 passed, 0 failed
4 passed, 0 failed
12 passed, 0 failed
19 passed, 0 failed
7 passed, 0 failed
5 passed, 0 failed
8 passed, 0 failed
30 passed, 0 failed
14 passed, 0 failed
11 passed, 0 failed
1 passed, 0 failed
29 passed, 0 failed
11 passed, 0 failed
11 passed, 0 failed
15 passed, 0 failed
10 passed, 0 failed
test result: ok. 49 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 41 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
=== 4_CI Pipeline (build 25.02 for GPU).txt
=== 5_CI Pipeline (cudf 26.02).txt
100% tests passed, 0 tests failed out of 1
== SUMMARY == 29/29 passed; 0 failed. (12 exhaustive, 4 sampled)
== SUMMARY == 31/31 passed; 0 failed. (21 exhaustive, 7 sampled)
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 1211 filtered out
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 1209 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out
test result: ok. 2451 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
=== 7_Changed paths.txt
```

## VM time

| VM | start (local) | stop (local) | hours |
|---|---|---|---|
| nebius-gpu | 2026-11-07 17:30:37 UTC | 2026-11-07 17:42:58 UTC | 0.21 (Task 6 Step 5, prepare) |
| nebius-gpu | 2026-11-07 23:08:20 UTC | 2026-11-07 23:27:20 UTC | 0.32 (Task 9 Step 2: prepare --no-sf40, ship, run list) |
| nebius-cpu128 | 2026-11-07 23:27:33 UTC | 2026-11-07 23:47:43 UTC | 0.34 (Task 9 Step 2: the CPU-VM build and the 26.02 multi-GPU build, in parallel) |
| nebius-h100 | 2026-11-07 23:47:52 UTC | 2026-11-08 00:36:22 UTC | 0.81 (Task 9 Step 2: prepare twice, ship, run list, pull; about 10 min idle after the first prepare was killed, below) |
| nebius-cpu128 | 2026-11-08 02:13:33 UTC | 2026-11-08 02:17:05 UTC | 0.06 (review finding 8: the CPU-VM build, warm) |
| nebius-h100 | 2026-11-08 02:17:11 UTC | 2026-11-08 02:40:25 UTC | 0.39 (review finding 8: prepare 15.5 min, ship, run bench, bench-each twice, the fused batch, one Nsight trace, pulls) |
| nebius-cpu128 | 2026-11-08 05:44:34 UTC | 2026-11-08 06:08:44 UTC | 0.40 (completeness fixes: the by-hand image tests on the published images, two stage-installs) |

Per VM for the task: nebius-gpu 0.53 h, nebius-cpu128 0.80 h, nebius-h100 1.20 h; 2.53 h in all.
Every stop was a soft `nebius.sh stop`, exit 0, and `status` read STOPPED after it.

## Task 1 Step 8 (publish)

- Checkpoint `ff74cb3e` committed and pushed. `images.yml` dispatched 2026-11-07T12:07:55Z on
  `ENS-containers-only`, kind=all cudf=all: run 42371113005.
- `:cudf-<v>` digests before the dispatch (must be unchanged after):
  build 25.02 `sha256:2a0c84414baa…f6bb786`, build 26.02 `sha256:146fcbaa2f41…b938e38`,
  run 25.02 `sha256:10bf83bb1ba1…6e9158`, run 26.02 `sha256:bc3ff3712f2c…b3a1ad`.
- Run 42371113005 green, five jobs (build 25.02/25.10/26.02, run 25.02/26.02). Inputs tags → digests:
  build `cudf-25.02-in-50a0c1d7779e` `sha256:b8da5a94553c…`, build `cudf-25.10-in-6bbfdd6dbd8f`
  `sha256:9d862501b165…`, build `cudf-26.02-in-ca572f6c40c1` `sha256:9199d3efd54c…`, run
  `cudf-25.02-in-4f971122ddad` `sha256:3a69a03c7b69…`, run `cudf-26.02-in-f77b2cdf62d2`
  `sha256:5117c1abcce9…`. Every `:cudf-<v>` digest is the one before the dispatch; `:cudf-25.10`
  does not exist (only master moves it). The 25.10 build package pulls anonymously (HTTP 200).
  Task 1 done; next: Task 2.

## Task 3 Step 4 (PR run)

- `4cf4749f` pushed; PR run 42379249690 green: cost-report, s3-datasets (in the image), both legs,
  the GPU build. Counts against `## Counts before`: every line equal (timings aside), except
  cost-report's `test_source_formatting` 41 → 47 (Task 2 +1, `ci_images.rs` +5) and the new
  "Image and run script tests" step's 20 files (17 in the image, 3 on the runner), as the developer's
  local run had them. Task 3 done; next: Task 4.

## Task 4 Step 6 (PR run)

- `ffa2674d` pushed; PR run 42385300988 green, every job. The 25.02 leg's counts equal the
  baseline's 25.02 leg, plus the new `test_duckdb_minimal.py` line (`9 passed`); the 25.10 leg's
  equal the baseline's "26.02" leg line for line (ctest 1/1, validate 29/29 and 31/31, ffi 5, test_ffi
  4, `--lib` 1209 + 2 ignored, `test_cpu_corpus` 2451). `Generate SF-1`'s `generate_pbench.sh
  --check` step passed. Task 4 done; next: Task 5.
- Dangling links to `testinfra.md#t129` left by #129's archiving: the frozen spec, and
  `verify-26.02.md:6`; the board's line is the coordinator's at the next state change.

## Task 6 Step 4 (PR run)

- `7548f425` pushed; PR run 42396552689: every job green but cost-report, whose "Nebius script
  tests" step failed in `shellcheck -x`: the runner's shellcheck reports SC2015 (info) at
  `scripts/build-test.sh:84` and `scripts/env/run-gpu.sh:172`, which 0.11.0 on dev did not. Handed to
  the developer. `cpp-install-25.02` (168.9 MB) holds `lib/libpeacock_gpu.so`, the C++
  `bin/peacock_*_tests` and the four `rust-tests/` files.
- SC2015 fixed: both `A && B || die` become `if <not A> || <not B>; then die; fi`. Neither had
  the bug SC2015 names (`die` exits, so it ran exactly when A or B was false): a rewrite, same
  behaviour. Pinned first: `test_refuses_nothing_to_do_and_a_command_beside_an_action` names
  "a command runs alone" for each of `--build --run --gpu --rust-only --no-release-bench` beside
  `--`; new `test_prepare_refuses_a_run_image_without_a_driver_minimum` (label absent, label
  empty). Each failed with its guard's B dropped. ubuntu-latest (24.04 noble) ships shellcheck
  0.9.0-1. The CI step's eight files, `shellcheck -x`: 0.9.0 reproduced the two findings, exit 1;
  after the fix exit 0 under `koalaman/shellcheck:v0.9.0`, apt's 0.9.0 in `ubuntu:24.04` and
  `koalaman/shellcheck:v0.11.0`. `test_build_test.py` 18 passed, `test_run_gpu.py` 69 passed.
- dev's disk filled during that work (0 bytes free): an image test pulled both run images. Removed
  this worktree's `target-img-25.02` and `target-img-25.10`, the two run images, the shellcheck
  and ubuntu images, then `docker image prune`: 29 GB free. dev holds only the 25.02 build image.

## Task 6 Step 6 (device run through CI)

- `c3668d65` (the SC2015 fix) pushed; `pipeline.yml` dispatched 2026-11-07T18:27:10Z on
  `ENS-containers-only`: run 42401363998. A restart that finds it unfinished dispatches it again.
- Run 42401363998 green, every job (s3-datasets skipped on a dispatch, as in the baseline).
  `gpu-tests` used run image `peacockdb-run@sha256:3a69a03c7b69…` and build image
  `peacockdb-build@sha256:b8da5a94553c…` (the inputs tags'), logged `run-tests.sh list: green`, no
  `miniforge` anywhere. Per-binary counts equal the baseline's `gpu-tests`: C++ 67 / 5 / 56 / 136,
  rust 9 (650 filtered) / 748 (1216 filtered) / 1009 / 1. Wall times: prepare 24 s (the VM was
  prepared by hand in Step 5), ship 13 s, run 13 min 54 s (the old runner's ship alone had a
  50-minute limit). The job printed no `df -h /` (no step runs one); Step 5's reading stands. The
  cleanup soft-stopped the VM (19:11:01Z, `stopped`); `nebius.sh status` afterwards: STOPPED.
  Task 6 done; next: Task 7.

## Task 7 Step 4 (live)

- `884be32f` pushed. `exec-model-corpus.yml -f filter=q1` dispatched 2026-11-07T20:02:55Z: run
  42409128248 green, three shards, each file ≥ 4 tests, 75 in all, as the developer's local run had
  them; `call-cost` skipped under a filter, as designed (proven on dev only). PR run 42409132496
  green, every job. Task 7 done; next: Task 8.

## Wiki commands run

Task 9 Step 2. Dev half: 2026-11-07, host dev, in `/tmp/co-t9/clone`, a `git clone --shared` of
`27cf22e5` with `third_party/cudf` cloned from the local module, image
`peacockdb-build:cudf-25.02-in-50a0c1d7779e`. Full runs with `RUST_TEST_THREADS=2`, as "Memory
constraints" says. Counts are the wiki's rows ("Test categories").

| command | host | exit | result |
|---|---|---|---|
| `scripts/env/image-ref.sh build 25.02` | dev | 0 | the local `:cudf-25.02-in-50a0c1d7779e` |
| `scripts/build-test.sh -- testdata/generate_testdata.sh --bench tpch`, then `--bench tpcds` | dev | 0, 0 | 13 s, 23 s; 8 + 24 files, aggregate `0e41c31d…a492`, Task 4's |
| `scripts/build-test.sh --rust-only --build --run` | dev | 0 | 17 min 48 s cold, no warning. `--lib` 1209 + 2 ignored (#182) = 1211, `test_cpu_corpus` 2451, `test_corpus_goldens` 66, `test_cost_model` 3: the cpu row's 3731; `test_golden_format` 41 |
| `scripts/build-test.sh -- cargo test --features rust-only -p peacockdb-core --test test_cost_model` | dev | 0 | 3 passed |
| `scripts/build-test.sh -- cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens` | dev | 0 | 25 passed |
| `scripts/build-test.sh -- scripts/case-inventory.sh rust-only` | dev | 0 | 3795 lines |
| `UPDATE_CANONICAL=1 scripts/build-test.sh --rust-only --run` | dev | 0 | 9 min; same counts; `git status` empty after |
| `scripts/build-test.sh -- cargo test -p cost-report` | dev | 101, then 0 | first run: the clone's alternates (`## Task 9`); after the repack 49 + 57 passed |
| `scripts/build-test.sh -- scripts/cost-report-preview.sh` | dev | 0 | 301 kB HTML |
| `python3 scripts/ticket_widget.py` | dev | 0 | `testdata/report.html` unchanged |
| `scripts/build-test.sh --help` | dev | 0 | 27 lines |
| `scripts/build-test.sh --build --run` | dev | 0 | 37 min 25 s cold. ctest `-L cpu` 1/1; ffi row: `--lib -- ffi_tests::` 5 (lib 1214 = 1209 + 5), `peacockdb-ffi --test test_ffi` 4; the cpu targets as above |
| `scripts/build-test.sh -- scripts/case-inventory.sh cudf` | dev | 0 | 4812 lines |
| `scripts/build-test.sh -- cargo test -p peacockdb-core --lib --features gpu --no-run` | dev | 0 | 5 min 28 s |
| `scripts/build-test.sh -- scripts/case-inventory.sh gpu` | dev | 0 | 1967 lines; `gpu_tests::` 748, the gpu row |
| `scripts/build-test.sh -- cmake --build cpp/build --target peacock_multi_gpu_tpch_tests` | dev | 1 | rmm headers of 25.02; the wiki now says `--cudf 26.02` (`## Task 9`); not rerun |
| `scripts/build-test.sh --gpu --build` | dev | 0 | 25 min cold; six binaries staged. Listed in the image: `gpu_tests::` 748, `test_gpu_corpus` 1009, `peacock_gpu_benchmarks` 659, `test_node_timing` 1, `prototype_fused_benchmarks` 8: the gpu row's 2425 |
| `scripts/env/image-ref.sh build 25.10` | dev | 0 | `peacockdb-build@sha256:9d862501b165…`, the published digest; no pull |
| `scripts/env/image.sh build build 25.02 --dry-run` | dev | 0 | one `docker build` line |

VM half: 2026-11-07/08, from this worktree (`6e4740be` plus the fix below), sandbox off. The refs
were resolved on dev before each VM started: build `peacockdb-build:cudf-25.02-in-50a0c1d7779e`
(the local tag; `b8da5a94553c…` on ghcr), run `peacockdb-run@sha256:3a69a03c7b69…`, 26.02 build
`peacockdb-build@sha256:9199d3efd54c…`.

| command | host | exit | result |
|---|---|---|---|
| `scripts/build-test.sh --gpu --build` | dev | 2, then 0 | first: refused this worktree's October `cpp/build`, a host (conda) configure with no `.peacock-cudf` stamp, as "Local build workflows" says. `rm -rf cpp/build cpp/install` (both gitignored, as the message says), rerun: 30 min 46 s, six binaries staged |
| `export PEACOCK_BUILD_IMAGE PEACOCK_RUN_IMAGE`; each `=$(scripts/env/image-ref.sh build\|run 25.02)` | dev | 0, 0 | the two refs above |
| `scripts/nebius.sh start` | dev → nebius-gpu | 0 | 63 s; `df -h /` 19 GB free |
| `$G prepare --no-sf40` (nebius-gpu) | nebius-gpu | 0 | 76 s; both images already there; sf1 generated again under `46088dcae6d3296d`: Task 8 (`27cf22e5`) edited `generate_testdata.sh`, which the data hash covers |
| `$G ship` | nebius-gpu | 0 | 64 s, four streams |
| `$G run --detach list` | nebius-gpu | 0 | detached in 16 s |
| `$G status`, every 60 s | nebius-gpu | 5 ×15, then 0 | `exited 0`, `run-tests.sh list: green` at 23:26. C++ 67 / 5 / 56 / 136; rust 9 (650 filtered) / 748 (1216 filtered) / 1009 / 1; corpus 794.5 s |
| `scripts/nebius.sh stop` | nebius-gpu | 0 | 17 GB free before it |
| `C=…`; `export PEACOCK_BUILD_IMAGE`; `=$(scripts/env/image-ref.sh build 25.02)` | dev | 0 | |
| `$C start` | dev → nebius-cpu128 | 0 | 73 s; `df -h / /var/lib/peacock` 59 GB free of 213 GB |
| `D=$(scripts/nebius-sync.sh --instance-id … --profile eu-west1)` | dev → nebius-cpu128 | 0 | `peacockdb-peacockdb-beta` |
| `scripts/env/check-build-image.sh --instance-id … --profile eu-west1` | nebius-cpu128 | 0 | pulled the inputs tag; label this tree's. As first written (`:cudf-25.02`) it would refuse, exit 2: `docker/build.Dockerfile` differs (the fix, `## Task 9`) |
| `$C ssh "cd ~/$D && docker run … $PEACOCK_BUILD_IMAGE scripts/env/stage-install.sh"` | nebius-cpu128 | 0 | 8 min 20 s, `target-img-25.02` warm from 11-04; six binaries staged; no warning |
| `rsync -a -e "$C rsh" "nebius-cpu128:$D/cpp/install/" /tmp/co-t9vm/install-cpu128/` | dev | 0 | 15 s, 749 MB |
| `scripts/nebius-sync.sh … --dest peacockdb-beta-26` | dev → nebius-cpu128 | 0 | |
| `PEACOCK_BUILD_IMAGE=<26.02 ref> scripts/env/check-build-image.sh … --cudf 26.02` | nebius-cpu128 | 0 | pulled; label this tree's |
| `$C ssh "cd ~/peacockdb-beta-26 && docker run … -e PEACOCK_IN_IMAGE=1 … <26.02 ref> scripts/build-test.sh --cudf 26.02 --build"` | nebius-cpu128 | 0 | 9 min cold, beside the 25.02 build. 4 warnings, none this task's: two cuDF 26.02 deprecations in `cpp/src/expr.cpp` (`cudf::round` :753, `cudf::strings::like` :922) and two gcc 14 `-Wstringop-overflow` in `stl_algobase.h:452` |
| the same, `scripts/build-test.sh --cudf 26.02 -- cmake --build cpp/build --target peacock_multi_gpu_tpch_tests` | nebius-cpu128 | 0 | 13 s, `[8/8] Linking CXX executable peacock_multi_gpu_tpch_tests`, no warning. The wiki's multi-GPU line, first proof |
| `$C stop` | nebius-cpu128 | 0 | after removing `~/peacockdb-beta-26` (16 GB) and the 26.02 image: 45 GB free |
| `N=…`, `G=…`, the two refs as for nebius-gpu | dev | 0 | |
| `$N start` | dev → nebius-h100 | 0 | 63 s; `df -h / /var/lib/peacock` 80 GB free of 174 GB |
| `$G prepare` | nebius-h100 | killed, then 0 | first: pulled both refs (it had only the moving tags), sf1 under `46088dcae6d3296d`, sf40 `fetched 0, kept 8` and `kept 24`, `/opt/cudf` copied at 00:09; then my tool call's time limit killed it before its last line. Not the script's. Rerun: exit 0 in 41 s, all `already`. No exit 4 |
| `$G ship --install /tmp/co-t9vm/install-cpu128` | nebius-h100 | 0 | 24 s |
| `$G run list` | nebius-h100 | 0 | 14 min 28 s, `run-tests.sh list: green`; the L40S's counts exactly, corpus 843.4 s |
| `$G pull` | nebius-h100 | 0 | 6 s; nothing new in the tree: `git status` and a `find -newer` over `testdata/` unchanged; logs in `~/.cache/peacock/run-gpu/peacockdb-beta/` |
| `$N stop` | nebius-h100 | 0 | 65 GB free before it |

Not run, by the coordinator's decision (long measurements): `bench-each.sh`, Nsight
(`create_nsys_profile.sh --ssh`), `run prototype_fused_benchmarks`, and `run bench`. Run once each
after round 1, below.

Review finding 8, 2026-11-08, from this worktree at `5f396a94` (round 1's fixes), sandbox off,
one case each. No `PEACOCK_*_IMAGE` exported: the scripts' own inputs-tag default (round 1's
item 7) chose every image. `N`, `G` as "The H100's device run"; scratch under `/tmp/co-f8/`.

| command | host | exit | result |
|---|---|---|---|
| `$C start` | dev → nebius-cpu128 | 0 | 68 s |
| `D=$(scripts/nebius-sync.sh --instance-id … --profile eu-west1)` | dev → nebius-cpu128 | 0 | `peacockdb-peacockdb-beta`, 1 s |
| `I=$(scripts/env/check-build-image.sh --instance-id … --profile eu-west1)` | nebius-cpu128 | 0 | stdout the ref alone, `…peacockdb-build:cudf-25.02-in-50a0c1d7779e`; label this tree's |
| `$C ssh "cd ~/$D && docker run … $I scripts/env/stage-install.sh"` | nebius-cpu128 | 0 | 72 s warm, no warning; `rust-tests/` four, `rust-benchmarks/peacock_gpu_benchmarks` and `prototype_fused_benchmarks` |
| `rsync -a -e "$C rsh" "nebius-cpu128:$D/cpp/install/" /tmp/co-f8/install/` | dev | 0 | 14 s, 749 MB |
| `$C stop` | nebius-cpu128 | 0 | 45 GB free before it |
| `$N start` | dev → nebius-h100 | 0 | 72 s |
| `$G prepare` | nebius-h100 | 0 | 932 s. Build image up to date; pulled the run image's inputs tag `cudf-25.02-in-4f971122ddad`, a newer digest (`6aa01d4f…`) than 11-07's, likely the in-flight `images.yml` dispatch's; sf1 kept, sf40 `fetched 0, kept 8` / `kept 24`; `/opt/cudf` copied. No exit 4 |
| `$G ship --install /tmp/co-f8/install` | nebius-h100 | 0 | 21 s |
| `PCK_TEST_FILTER=bench_tpch_sf40_q6_tp1_single $G run bench` | nebius-h100 | 0 | 27 s; prefill 9.0 GB in 5.5 s; q6 75.6 ms (second-smallest of 10); 1 benchmark file, 81 calibration rows; green |
| `$G pull`, then `$N ssh 'rm -f …/calibration/records.tsv'` | nebius-h100 | 0, 0 | 6 s; `tpch.sf40/tp1_single.benchmark.txt` changed (q6), `records.tsv` home with 81 rows |
| `scripts/env/bench-each.sh --instance-id … --profile editor-eu --cases <one line: tpch q22> --mode tp1_single --timeout 1800 --records /tmp/co-f8/records` | nebius-h100 | 1 | 27 s. The binary printed `kept`: the shipped committed tree holds q22 from this host, driver, CUDA and cuDF (11-05), and `PEACOCK_BENCH_RESUME=1` keeps it. No file changed, so no slug: "1 of 1 cases … have no section; rerun". A rerun repeats it. The wiki was wrong (below) |
| `$G run prototype_fused_benchmarks --exact bench_fused_tpcds_sf40_first3_tp1_single` | nebius-h100 | 0 | 31 s, the smallest batch (15 GB prefill against tpch first3's 42); fused 606.9 ms, the answers checked; ok |
| `PCK_BENCH_HBM_PEAK_BW=3.35e12 PCK_BENCH_HBM_FREQ=10000 PCK_TEST_FILTER=bench_tpch_sf40_q6_tp1_single scripts/create_nsys_profile.sh --trace --ssh "$N rsh" --host nebius-h100 --remote-dir /home/dmitry/peacock/peacockdb-beta --remote-cudf-root /home/dmitry/peacock/cudf-25.02` | nebius-h100 | 0 | 17 s; trace pass ran 1 test; capture 2.1 MB home; `loads.tsv` 2 rows, `calls.tsv` 58 rows (q6 checked against the sf1 goldens) |
| `$G pull`; `$N ssh 'rm -rf ~/peacock/peacockdb-beta/testdata/benchmark-results/h100-80gb-hbm3'` | nebius-h100 | 0, 0 | the device tree cleared after `ship`, as the wiki now says (a narrower removal, the mode's file, is what it gives) |
| the same `bench-each.sh`, `--records /tmp/co-f8/records2` | nebius-h100 | 0 | 31 s; q22 207.3 ms; record set aside as `tpch-q22-tp1_single.1.tsv` (251 rows), `device` = `h100-80gb-hbm3`, "1 cases at tp1_single, every one with its section" |
| `$G pull`; `$N ssh 'rm -f …/calibration/{records.tsv,capture.*}'` | nebius-h100 | 0, 0 | 13 s |
| `$N stop` | nebius-h100 | 0 | 60 GB free before it; no process on the card |

Round 1's `run-gpu.sh` changes held on a real VM: the inputs-tag default picked both images, and
`--dir` defaulted to `peacockdb-beta`. `--dir` itself was not given. The device answers recorded
with `PCK_WRITE_GPU_RESULT=1` (q22, through `bench-each.sh`) left `goldens/tpch.sf40/gpu-result.txt`
byte-identical. Afterwards dev's tree was restored: the three tracked files the pulls and the
capture changed (`checkout`), the run's ignored files deleted (`capture.*`, `loads.tsv`), and the
pre-existing ignored `calibration/fused/tpcds.sf40/first3.records.tsv`, which the pull overwrote,
put back from a tar taken before. `hbm-parts/` was not touched: the VM's 45 files there match dev's
by size and mtime, so no pull wrote one.

The wiki fix: "The H100's device run" says `bench-each.sh` measures into an empty file. After
`ship`, remove the mode's `<device>/<ds>.sf<sf>/<mode>.benchmark.txt` on the VM and on dev, since
on dev a committed section counts as measured once the slug is known (code: `has_section`).
"sf40 baseline" step 2 cleared the VM's tree before step 3's `ship`, which sends it back; that
worked on 11-05 only because no H100 tree was committed yet. It now clears after `ship`. No
script changed: `bench-each.sh` does what its header says. What it does not say is that a
committed section of the same host counts as done; the wiki now says it.

Not run on dev, by design: the run-image commands of "Images" (a pull, `docker run --gpus`,
`docker image inspect` of a run image: no run image on dev, and none is pulled), `image.sh push`,
`gh workflow run`, `docker login`, verda (no address; down). The VM half (nebius-gpu, the CPU-VM
build, the H100) is next.

### Review round 2, important 1 (2026-11-08)

On dev, in the worktree, image `peacockdb-build:cudf-25.02-in-50a0c1d7779e`, `PEACOCK_DPHYP_LIB` as
the recipes set it. `git status` after every run: nothing but this round's edits.

| command | host | exit | result |
|---|---|---|---|
| `scripts/build-test.sh -- cargo build --release -p peacockdb-dphyp` | dev | 0 | `target-img-25.02/release/libpeacockdb_dphyp.so` |
| build-test.md's gate loop, `py scripts/exec_model/tests/$f.py` over its six files | dev | 0 ×6 | 9, 7, 21, 1, 2, 2 passed |
| `py scripts/exec_model/tests/optimizer/test_ordered_corpus.py` | dev | 0 | 2 passed |
| `UPDATE_CANONICAL=1 py scripts/exec_model/tests/optimizer/test_cardinality_corpus.py` | dev | 0 | 3 passed; golden unchanged |
| `scripts/build-test.sh -- testdata/gen_duckdb_cost.sh --gen`, over a scratch copy (`<tree>/target-img-25.02/gen-check`: the sf1 parquet linked, profiles, dynamic filters and sf1 goldens copied) | dev | 0 | tpch 48, tpcds 99, 0 failed; every profile, dynamic-filter file and `.duckdb_cost.txt` byte-identical to the committed ones. The pyarrow warning (25.0.1, pinned 19.0.1) moves nothing |
| `scripts/build-test.sh -- cargo test -p cost-report every_registry_query_has_a_duckdb_cost` | dev | 0 | 1 passed |
| `UPDATE_CANONICAL=1 scripts/build-test.sh -- cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens -- the_benchmark_cases_follow_the_registry` | dev | 0 | 1 passed; `corpus_benchmark_cases.inc` unchanged |
| the same, `-- the_benchmark_report_matches_the_trees` | dev | 0 | 1 passed; the page unchanged |
| the same, `-- the_h100_list_is_the_lists_measured` | dev | 0 | 1 passed; `h100-80gb-hbm3/cases.csv` unchanged |
| `scripts/build-test.sh -- python3 testdata/duckdb_result.py --dataset tpch` (the sf1 form; the wiki's sf40 `--answers` form needs the CPU VM's 13.5 GB database) | dev | 0 | 6 min 20 s; `goldens/tpch.sf1/duckdb-result.txt` byte-identical |
| `scripts/build-test.sh -- python3 scripts/calibration/plot.py --record … --record … --hbm … --out-dir …` (the H100's two fetched records and its tpch `hbm.tsv`, into `target-img-25.02/plot-check`, not the committed `plots/`) | dev | 0 | 18 min; 509 panels and `index.html`. A first try, under the tool's 10-minute background limit, was killed: its container went with it |
| README: `scripts/build-test.sh -- python3 -c "import pandas, pyarrow; …"` | dev | 0 | `3.0.6 25.0.1` |
| README: `py …/engine/test_determinism.py`, then the cheap-tier loop | dev | 0 | 4 passed; 48 files, 0 failed |
| README: `py …/engine/test_plan.py test_height_is_distance_to_root` | dev | 0 | 1 passed |
| README: the dataset tier, `py` over `test_stats_sidecar`, `test_cardinality_corpus`, `test_run_corpus` (`test_ordered_corpus` and the `UPDATE_CANONICAL` form above) | dev | 0 ×3 | 4, 3, 1 passed |
| README: `PCK_MODE=tp4-single scripts/build-test.sh --pass-env PCK_MODE -- python3 …/plans/test_engine_answers.py test_tpch_q3` (one query, not a shard) | dev | 0 | 1 passed |
| README: `py …/optimizer/test_engine_optimized.py test_tpch_q3` | dev | 0 | 1 passed |
| README: `py scripts/exec_model/run.py --bench tpch --query q3 join-int --mode tp1-single --jobs 2 --out target-img-25.02/exec-model-run`, then `cost_report.py --out …` | dev | 0, 0 | 2 runs in 37.9 s, 0 failed; `cost_report.html` written |

Not run: the README's `test_stats_embeddings.py` (dev has no embeddings cache, and fetching it is
the manual tier's own step), `fetch_record.sh` and `test_call_cost_measured.py` (no bucket keys in
this shell; CI's call-cost job runs that pair in the image), and the whole-corpus `run.py --jobs
16` (5 h; the two-run form above is the same path).

## Task 9, the VM half (coordinator's dispatch notes)

- Task 8's PR run 42412769493 green. Task 9 Step 1, Step 3 and Step 2's dev half committed as
  `d55ad41b`. Open: Step 2's VM commands (nebius-gpu, the CPU-VM build, the H100), and the
  multi-GPU build line on 26.02 (not run on dev: no room for the 26.02 build image; run it on
  nebius-cpu128 during the CPU-VM session).
- Watch: the CPU-VM recipe names `peacockdb-build:cudf-25.02`, the moving tag, whose inputs are
  master's, not this tree's, until the branch lands; `check-build-image.sh` and the recipe must agree
  with the inputs-tag rule (`image-ref.sh`).

## Review

- 2026-11-08T00:52:43Z: plan complete at `a9ff18cc` (every box ticked); board `reviewing`; PR #203 marked ready.
  VM time for the task: 1.68 h (nebius-gpu 0.53, nebius-cpu128 0.34, nebius-h100 0.81). Round 1
  dispatched.

### Round 1 findings (reviewer, 2026-11-08): 0 blocking, 8 important, 10 nits

Important, all routed to the developer:
1. CI's `run-gpu.sh` dir and container (`~/peacock/peacockdb/`, `peacock-peacockdb`) are the human's
   main clone's too; CI's cleanup can kill a by-hand run. Fix: `--dir ci`, cleanup `peacock-ci`,
   the guard reads the name from the run step.
2. `build-test.sh` never label-checks a given `$PEACOCK_BUILD_IMAGE` (stale image, or a 25.02 ref
   under `--cudf 26.02`); the wiki says it refuses. Fix: check it, test exit 2 with no `docker run`.
3. `test_image.py::test_the_inputs_are_the_same_in_every_locale` skips-as-ok in the image (no
   locale, #309). Fix: run the file on the runner, or fail instead of skip; correct #309.
4. CI no longer tests `docker/run-tests.sh`'s rules (`test_run_tests.py` is `needs_docker`).
   Fix: run it in `images.yml`'s run job between build and push.
5. `ci_images.rs` misses project commands behind `timeout`/`env`/`exec`/`nice`, `bash -c`, `bash
   <script>`, shebang scripts under `scripts/` etc., and `python`. Fix and pin each as refused.
6. `build-test.sh`'s `exec docker run` has no `--init`: an interrupted build keeps running in the
   container, holding cargo's lock. Fix: `--init`, `-it` on a terminal, a named container and a
   trap.
7. After the merge `bootstrap-vm.sh`, `check-build-image.sh` and `run-gpu.sh` refuse master's
   `:cudf-<v>` until someone moves it. Fix: default all three to the inputs tag.
8. Spec item 6: `bench-each.sh`, Nsight, `run prototype_fused_benchmarks` and `run bench` were not
   run. Coordinator: run each once at the smallest scope (one case) on the H100 after the fixes.

Nits taken because they are cheap and real: 2 (the GPU run step's guard does not refuse
`continue-on-error` or an `if:`), 8 (the Rust cache key ignores the image), 9 (wiki drift:
`stage-install.sh` usage, a long line, "passes none of the step's environment"). Dropped: 1, 3, 4,
5, 6, 7, 10.

### Round 1 fixes (developer, 2026-11-08)

Each code fix had its test red first. No image input changed: `image_inputs_tag . build 25.02` is
still `cudf-25.02-in-50a0c1d7779e`, the local image's; `docker/`, `cpp/CMakeLists.txt` untouched.
No republish.

1. CI's dir and container: the three `run-gpu.sh` calls in `gpu-tests` take `--dir ci`
   (`~/peacock/ci/`, container `peacock-ci`); cleanup removes `peacock-ci`. The guard moved to
   `ci_gpu.rs::the_gpu_job_owns_its_dir_and_removes_its_container`: it reads `--dir` off the
   `Run the L40S list` step, refuses none or `peacockdb` (the checkout's name, a developer's
   clone's on the VM), another call naming another dir, and a cleanup of anything but
   `peacock-<dir>` (red on the old pipeline.yml). `runners.rs`'s `GPU_RUN` carries `--dir ci`;
   `test_run_gpu.py`'s CI-sequence test runs `--dir ci` and checks the dir and container name.
   build-test.md's CI section, disk section and tests table say `~/peacock/ci/`, `peacock-ci`.
2. A given `$PEACOCK_BUILD_IMAGE` is label-checked: `image-ref.sh --ref <ref> build <v>` looks the
   ref up as it does the inputs tag (local, else the registry's digest) and refuses a label that
   differs from the tree's for `<v>`, naming the inputs (a 25.02 image under 26.02 names
   `docker/conda/cudf-25.02.spec`). `build-test.sh` calls it for a given ref; exit 2, no `docker
   run`. Tests: `test_image_ref.py::test_checks_a_given_ref_against_the_tree`,
   `test_refuses_a_given_ref_of_other_inputs`; `test_build_test.py::test_refuses_a_given_image_of_other_inputs`;
   `test_runs_in_the_image_as_the_caller` now expects the `--ref` call (it pinned "no check").
   CI: the action's `PEACOCK_BUILD_IMAGE` is image-ref.sh's digest, pulled, so the local
   `docker image inspect <repo>@sha256:…` answers. Checked by hand on dev: the local inputs tag
   passes, and under `--cudf 26.02` it is refused naming the two inputs.
3. `test_image.py::test_the_inputs_are_the_same_in_every_locale` fails, not skips, with no locale
   sorting unlike C (red in the image: "no installed locale sorts unlike C"), and the file joins
   the cost-report step's `on_runner` list beside `test_fetch_sf40.py`; RUNNER_SIDE's reason says
   so. #309 now names both files.
4. `images.yml` has a `Test` step between Build and Push: for `KIND=run`, `IMAGE=$(image-ref.sh run
   $CUDF)` (the image just built, label-checked) and `python3 scripts/env/tests/test_run_tests.py`;
   a build image skips it inside the script, so no step has an `if:`. `test_run_tests.py` needs the
   run image and docker only: fake binaries, `--network none`, no `--gpus`, stdlib python. It reads
   `IMAGE`; with it set, `test_each_image_carries_the_committed_script` (was
   `test_both_images_…`) checks that one image instead of both moving tags. Tests:
   `test_images_workflow.py::test_the_job_runs_its_five_steps_unconditionally`,
   `test_the_run_image_is_tested_before_it_is_pushed`,
   `test_the_test_step_runs_test_run_tests_in_the_run_image_of_the_job_s_cudf` (the step run over
   fakes: run → image-ref then python3 with IMAGE; build → nothing; image-ref failing → no
   python3). Not run on dev against an image: dev holds no run image and must not pull one; with a
   fake docker the file passes `IMAGE` through to every `docker run`.
5. `ci_images.rs` finds a command behind `timeout` (with its duration), `env` (options and
   assignments), `exec`, `nice`, `nohup`, `command` (`-v` runs nothing), `sudo`, `xargs`; reads
   `bash`/`sh` `-c` text as a line and `bash <script>` as the script; and treats any python and
   any file of `scripts/`, `benchmarks/`, `testdata/`, `docker/` run by its path as project code.
   RUNNER_SIDE entries now name a command (from its command word, at a word boundary) for any owner:
   in-image job, host job or local action; an entry naming no command is refused. The reader moved
   to `ci_workflow.rs` (ci_images.rs was at 966 lines). A multi-line array assignment
   (`elsewhere=(… \n …)`) is one line, else its paths read as commands. New entries with reasons:
   gpu-tests (`check-actor.sh`, `image-ref.sh`, `nebius.sh`, `run-gpu.sh`), image (`image.sh`,
   `image-ref.sh`, `python3 scripts/env/tests/test_run_tests.py`), watch (`watch.sh`), the
   build-image action (`image-ref.sh`). Tests: `a_project_command_behind_a_prefix_word_or_a_shell_is_found`
   (22 refused, 8 accepted), `a_runner_side_entry_names_a_command`, and the host-job case in
   `a_job_neither_in_the_image_nor_said_why_is_refused`.
6. `build-test.sh`: `docker run --init --name peacock-build-<tree>-<v>-<pid>` with labels
   `peacock.tree`, `peacock.cudf`, `peacock.pid`; `-it` only when stdin and stdout are terminals.
   docker run goes to the background (`<&0`) so an INT/TERM trap runs at once and `docker kill`s
   the container; exit 130/143, else the container's. Before the run, a labelled container of this
   tree and version whose pid is gone (`/proc/<pid>`) is refused, exit 2, naming `docker kill`.
   The pid is in the name so two runs in one tree (an agent's background build and a quick test)
   still work. Tests over the fake docker: `test_the_container_has_an_init_and_a_name_by_worktree_and_version`,
   `test_a_terminal_gets_it` (a pty), `test_a_signal_kills_the_container` (TERM 143, INT 130),
   `test_a_leftover_container_is_refused`; exit passthrough unchanged. By hand on dev: `timeout 8
   scripts/build-test.sh -- sleep 100` returns at 8 s with no container left; PID 1 is
   docker-init.
7. `run-gpu.sh`, `check-build-image.sh` and `bootstrap-vm.sh` default to
   `…:$(image_inputs_tag <tree> <kind> <v>)`; the `PEACOCK_*_IMAGE` overrides stay (bootstrap had
   none). `bootstrap-vm.sh` sources `image-inputs.sh` from its clone, as before, so a raw-view
   fetch of the one file still works; a clone at a ref older than `image_inputs_tag` is refused
   naming it and `--ref`. `check-build-image.sh` prints the checked ref alone on stdout (its other
   output to stderr), so the CPU-VM recipe is `I=$(check-build-image.sh …)`. The wiki's three
   recipes and prompts.md's L40S line lose their exports. Tests: `gpu_host.py` keys its images by
   the tree's inputs tag (`h.run_ref`, `h.build_ref`), which turned 48 of test_run_gpu's tests and 4
   of check-build-image's red until the scripts changed; `test_check_build_image.py` checks stdout;
   `test_bootstrap.py::test_bootstrap_uses_the_image_of_the_clones_inputs`,
   `test_bootstrap_refuses_a_clone_older_than_the_inputs_tag`.
- Nit 2: `runners.rs::gpu_run_step_problems` also refuses `continue-on-error:` and `if:` in the
  run step; `the_gpu_run_step_reader_refuses_each_way_to_hide_a_failure` pins each way.
- Nit 8: the three `Swatinem/rust-cache` steps take `key: ${{ env.PEACOCK_BUILD_IMAGE }}`;
  `ci_images.rs::a_rust_cache_is_keyed_by_the_build_image` (red on the old pipeline.yml).
- Nit 9: build-test.md gives `stage-install.sh [--cudf <v>] [--no-release-bench]` and what the flag
  does, the long line there is rewrapped, and "passes none of the step's environment" now says
  `build-test.sh` forwards a fixed list by name.

Pre-existing, not touched: actionlint (its image's shellcheck 0.11) reports SC2001 (style) in
`changes`' run block at pipeline.yml:53, the same on HEAD; shellcheck 0.9.0 reports SC2032/SC2033
on `bootstrap-vm.sh`'s `docker() { sudo docker …; }` line, the same on HEAD and outside CI's list.
- Round 1 fixes committed as `385b0cb7` and pushed: PR run 42437388041. `images.yml -f kind=run -f
  cudf=25.02` dispatched to prove the new Test step (`test_run_tests.py` in the run job): run
  42437382608. Finding 8's H100 commands dispatched to the developer next.
- Round 1 runs: PR run 42437388041 green; images run 42437382608 green, its new Test step ran
  `test_run_tests.py` against the built run image, 23 passed. Finding 8 done (`## Wiki commands
  run`). Round 2 dispatched.

### Round 2 findings (reviewer, 2026-11-08): 0 blocking, 2 important, 7 nits

Round 1's important 1–7 and nits 2, 8, 9 verified fixed, each with a test.
1. Important: the wiki still shows host builds — bare `cargo test` at build-test.md ~1541, 2965,
   3222, 3284, and the exec-model block (~1528-1541) needing host DuckDB/pyarrow/DPhyp. Fix: through
   `scripts/build-test.sh --`, run each once.
2. Important: `gpu-tests` with `--dir ci` never ran live. Coordinator: `pipeline.yml` dispatched
   2026-11-08T02:58:53Z at `f4515fb4`, run 42440753811.
Nits taken: 1 (a backgrounded `build-test.sh` on a terminal hangs on SIGTTOU: also require the
foreground process group), 2 (an early INT/TERM is lost before the container exists; HUP not
trapped), 4 (`ci_gpu.rs`'s cleanup check is a substring test: accepts a second container), 5 (an
unpublished inputs tag now fails with docker's "manifest unknown", exit 1, instead of exit 2 naming
the inputs and the `images.yml` dispatch — Review Focus 1), 6 (the `bench-each.sh` wiki instruction
wipes the mode's committed file for a partial run; dated history line; the script never learns the
device when every case is already measured). Dropped: 3 (more reader shapes), 7 (line length, a count
off by one, test file size).

### Round 2 fixes (developer, 2026-11-08)

Each code fix had its test red first, for the reason the finding gives. No image input changed:
`docker/`, `cpp/CMakeLists.txt` and `third_party/` untouched, `image_inputs_tag . build 25.02` still
`cudf-25.02-in-50a0c1d7779e`, the local image's. No republish.

1. **Important 1, the wiki's host commands.** build-test.md: the exec-model gates block runs as CI's
   steps do: `scripts/build-test.sh -- cargo build --release -p peacockdb-dphyp`,
   `PEACOCK_DPHYP_LIB=$PWD/target-img-25.02/release/libpeacockdb_dphyp.so`, each python file
   through `scripts/build-test.sh --pass-env PEACOCK_DPHYP_LIB --` (a `py` helper), and
   `gen_duckdb_cost.sh` and the cost-report test through `build-test.sh --`. The three
   `UPDATE_CANONICAL=1 cargo test … test_corpus_goldens` lines go through `build-test.sh --`, which
   forwards `UPDATE_CANONICAL` by name. `duckdb_result.py`: the `duckdb` module is the image's (the
   venv recipe is gone), and the sf40 step copies the answers database into the tree. The shad-gpu
   recipe's step 6, `plot.py`, runs on dev: `/usr/bin/python3` became `build-test.sh -- python3`.
   A new rule beside "A one-off cudf/FFI cargo command": a python script needing more than the
   standard library runs in the image; a stdlib-only one may use the host's `python3`.
   scripts/exec_model/README.md: Environment is the image (Python 3.13.0, pandas 3.0.6, pyarrow
   25.0.1, DuckDB 1.5.4) and shad-gpu as the one host with its own python3; DPhyp built in the image
   with the `py` helper beside it; every command through `py` or `build-test.sh`; `PCK_MODE` by
   `--pass-env` (not in the forwarded list); `run.py --out` inside the tree; the pytest line dropped
   (the image has no pytest); the record fetch as CI's call-cost job does it. Left alone by the new
   rule: `python3 scripts/ticket_widget.py` (prompts.md), `join_records.py` and `split_record.py`
   (build-test.md; stdlib only, and their `<dir>`/`<joined>` may lie outside the tree, which an
   image run cannot see). Left as prose, not commands to run: `cargo test --features rust-only -p
   peacockdb-core` in prompts.md and "What rust-only means", coding-style.md's `cargo build -p
   peacockdb`, architecture.md's `cargo build`. Runs: `## Wiki commands run`, "Review round 2".
   Found while running: `duckdb_result.py --only q1` at sf1 writes q1's section alone, dropping the
   rest of the file (10,100 lines). The file was put back from HEAD at once, byte-identical (md5).
   The wiki never gives `--only` at sf1; tooling, not production behaviour, so no ticket.
2. **Nit 1, `-it` and a background job.** `build-test.sh` reads its process group and its terminal's
   foreground group from `/proc/$$/stat` and adds `-it` only when stdin and stdout are terminals and
   it is not a background job of that terminal: `tpgid` is its own group, or -1 (no controlling
   terminal, so no SIGTTOU can stop it). Test: `test_a_background_job_on_a_terminal_does_not_get_it`,
   a session leader holding a pty as its controlling terminal runs the script in its foreground
   group (`-it`) and in a background one (none); red before (`-it` in the background). The first fix
   (`pgrp == tpgid` alone) turned `test_a_terminal_gets_it` red: its pty is no controlling terminal,
   `tpgid` -1, which is the case the -1 clause covers.
3. **Nit 2, early signals and HUP.** HUP, INT and TERM each only record 129, 130 or 143. After the
   wait ends early, the script kills the container by name every 0.5 s for as long as `docker run`
   lives, so a signal before docker has created the container still kills it. The fake docker's
   `run` now creates its container after `FAKE_DOCKER_CREATE_S`, and its `kill` fails before that,
   as docker's does. Tests: `test_a_signal_before_the_container_exists_still_kills_it` (TERM, INT,
   HUP; red before: the script waited out the 60 s container), `test_a_signal_kills_the_container`
   now with HUP (red before: died of the signal). By hand on dev: a `plot.py` run whose
   `build-test.sh` the tool's background limit killed left no container behind.
4. **Nit 4, the cleanup check parses.** `ci_gpu.rs` reads every command of each gpu-tests step, and
   after a `nebius.sh … ssh` the commands of its remote text. A `docker rm|kill|stop` (or `docker
   container …`) naming anything but `peacock-<dir>` alone is refused anywhere in the job, and the
   cleanup must hold a `docker rm -f peacock-<dir>`. Pinned as refused: `peacock-cix` (red before:
   the substring test took it), `peacock-ci peacock-peacockdb`, a second `docker rm` of
   `peacock-peacockdb`, a `docker kill` in another step, a `docker stop -t 5`, a `$(docker ps -q)`.
   Mutation: with the "names anything else" loop off, "a second docker rm: accepted".
5. **Nit 5, an unpublished inputs tag.** `machine_image_checked` keeps docker pull's stderr in a
   file (not `$(…)`, so a docker wrapper's `die` still ends the script) and prints it. A failed pull
   whose message says `: not found` (dev's docker 28) or `manifest unknown` (older ones) returns 2:
   "<ref> is not published", the tag of the tree's inputs, and `gh workflow run images.yml --ref
   <branch> -f kind=<kind> -f cudf=<v>` (`<this tree's branch>` when detached). Any other failure
   keeps docker's status (a network error, or ssh's 75 through run-gpu.sh) and names the same
   dispatch "if the image was never published". Messages taken from dev's docker: a missing tag
   says `failed to resolve reference …: not found`; an unknown host `dial tcp: lookup …: no such
   host`. The fakes (`gpu_host.py`, `machine.py`) now fail a pull with those words. Tests, red
   before (exit 1, docker's line alone): `test_check_build_image_names_the_publish_of_an_unpublished_image`,
   `…_names_a_pull_that_failed_otherwise`; `test_run_gpu.py::test_prepare_names_the_publish_of_an_unpublished_image`
   (run and build kinds), `test_prepare_names_a_pull_that_failed_otherwise`;
   `test_bootstrap.py::test_bootstrap_names_the_publish_of_an_unpublished_image`. Headers of
   run-gpu.sh and check-build-image.sh, and build-test.md ("Images"), say so.
6. **Nit 6, `bench-each.sh`.** The script: when a case's run changes no tree file (it kept a section
   this VM measured before), the device tree is the one whose section of the case names the VM's
   `hostname` (one `nebius.sh ssh hostname`, once). Small, so no ticket. Test:
   `test_bench_each_learns_the_device_from_a_section_the_run_kept`, a fresh `--records` over a tree
   the VM measured, with an L40S tree of another host holding q1 too: one run, the device learnt,
   the rest kept on dev, exit 0. Red before: all four kept, exit 1, "4 of 4 cases … have no
   section". The fake `run-gpu.sh` now keeps a section whose trailer is this run's under resume, as
   the harness does (its first cut split sections without their last newline and never matched;
   caught because the test passed through the old learning path). The wiki: "It keeps what this VM
   measured before"; the whole mode is re-measured by removing the mode's file on both sides after
   `ship`, which a partial run must not do; some cases by removing just their sections (an `awk`
   line, checked on a copy of the H100 tpch tp1_single file: q6's 31 lines gone, 47 sections left);
   the dated sentence is gone.

Verification, after the last edit: the guard (`test_source_formatting` 61 passed, `--bin
cost-report` 49 passed); every image-free file of `scripts/env/tests` (20 files) and
`scripts/nebius/tests` (11), 0 failed (test_build_test 25, test_run_gpu 71, test_check_build_image
10, test_bootstrap 20, test_bench_each 13); shellcheck 0.9.0 over CI's list clean. Outside the list, `bench-each.sh` keeps SC2015 and SC2088
and `bootstrap-vm.sh` SC2032/SC2033, each the same on HEAD.
- Round 2 important 2 closed: device run 42440753811 (`--dir ci`, cold `~/peacock/ci/`) green, every
  job; gpu-tests counts as the baseline (C++ 67/5/56/136, rust 9/748/1009/1), `run-tests.sh list:
  green`, cleanup removed `peacock-ci` and the VM stopped. Round 2 fixes committed as `ff9dbf5b`.
  Round 3 dispatched.

### Round 3 (reviewer, 2026-11-08): 0 blocking, 0 important, 1 nit

Round 2's fixes verified by mutation (each reverted fix turned its test red). PR run 42445243365
(`ff9dbf5b`) green. Nit, taken (a real silent failure): `machine.sh`'s pull now sends stderr to a
temp file, so when `check-build-image.sh`'s `docker()` wrapper dies on `nebius.sh`'s exit 64 the
script exits 2 with empty stderr and leaks the temp file; fix with an EXIT trap, and make
`test_check_build_image_refuses_bad_input` assert the message. Board `completing`.

Fixed (developer): `machine_image_checked` sets an EXIT trap around the pull that prints and removes the temp file, then runs the caller's own EXIT trap, and puts the caller's trap back afterwards. The trap runs inside the pull's `2> "$err"`, so it writes to a saved copy of stderr. A plain `cat "$err" >&2` reads its own output file and fails, and under `set -e` the script then exits 1. `test_check_build_image_refuses_bad_input` now asserts that both refusal messages reach stderr and that `TMPDIR` is left empty.

## Completeness pass

- Analyst: `containers-only-analysis.md` (0 blocking, 6 important, falsified sentences, shortcuts).
  Reviewer: 0 blocking, 2 important — verda cannot run an rsync'd tree (no `.git`) and was never
  run; `build-test.sh` does not forward `PCK_UPDATE_SECTIONS` / `PEACOCK_REWRITE_RECIPE_BYTES`.
- Coordinator, in `19151f53` and `9ef051b0`: the falsified sentences (build-test.md, prompts.md,
  #226, #244), `ticket_widget.py` through the image, #309 widened to the uid-name pair, #310 filed
  (nebius-gpu's disk fills with images), verda taken off the image path in prompts.md and
  build-test.md.
- To the developer: the two forwarded variables (+ `COST_FILTER`); `dphyp.py`'s refusal text; two
  stale comments; a 25.10 any-user case and the by-hand image tests run once on the published
  images; the timing-dependent signal test made robust.

### Completeness fixes (developer, 2026-11-08)

1. **build-test.sh forwards the regen knobs.** `FORWARDED` and the header now carry
   `PCK_UPDATE_SECTIONS`, `PEACOCK_REWRITE_RECIPE_BYTES`, `COST_FILTER` and `PCK_GPU_RESULT_VERSION`.
   `test_runs_in_the_image_as_the_caller` sets all four and wants each by name, never `NAME=value`;
   red before (`PCK_UPDATE_SECTIONS` missing from the `-e` list). The sweep: every `env::var` in
   Rust and `os.environ` in Python, against the wiki's commands. Added `PCK_GPU_RESULT_VERSION`: the
   `duckdb_gpu_*` cases read it in the local cpu corpus (build-test.md, "The device's own answers").
   Left out, each with its reason: `PCK_WRITE_GPU_RESULT`, `PEACOCK_BENCH_RESUME`,
   `PEACOCK_BENCH_CASE_TIMEOUT_S`, `PEACOCK_RECORD_PATH`, `PEACOCK_BENCHMARK_CAPTURE`,
   `PEACOCK_PINNED_HOST_CACHE_BYTES` and `PEACOCK_SCAN_READER` act on a device run, which is
   run-gpu.sh's and forwards its own; `PEACOCK_TESTDATA_DIR` and `PEACOCK_EMBEDDINGS_CACHE` name a
   path, and the container sees the tree alone; `PCK_MODE` and `PEACOCK_DPHYP_LIB` are already given
   with `--pass-env` wherever the wiki and CI set them; `DUCKDB` names a host binary, and the image
   has `duckdb` on PATH; `CUDF_ROOT` and `CUDF_BUILD_FROM_SOURCE` are the image's.
2. **dphyp.py's refusal** names `scripts/build-test.sh -- cargo build --release -p peacockdb-dphyp`,
   then `export PEACOCK_DPHYP_LIB=$PWD/target-img-25.02/release/libpeacockdb_dphyp.so`, passed with
   `--pass-env`, as the README's DPhyp section does. `test_run.py`'s
   `test_a_missing_dataset_or_dphyp_library_is_refused_before_any_work` now matches both strings; red
   before on the old `CARGO_TARGET_DIR=/build/peacock/rust-only-target` text. `test_run.py`: 12 passed.
   No other test reads the text; no `rust-only-target` is left outside `llm-wiki/tasks/`.
3. **Two stale comments.** build-test.sh's `gpu_runtime_targets` now names stage-install.sh's
   `GPU_TESTS`, which runners.rs reads, not pipeline.yml's array. gen_reconstruction_report.py says no
   test reads the reports, and `run-gpu.sh ship` sends them anyway (it ships every committed testdata/
   file).
4. **The early-signal test is deterministic.** The cause of the 2-in-12 failures: the fake `docker
   run` created its container on a 2 s timer, whatever the script did. A first `docker kill` that
   started late (a python fake, 0.7-5 s to start on a loaded dev) found the container, so only one
   kill was recorded and `len(kills) >= 2` failed. With the timer at 0.01 s the count was 1 every
   time. Now the fake's `kill` writes `kill.missed` when it finds no container, and `run` given
   `FAKE_DOCKER_CREATE_AFTER_MISS` creates its container only after that. So the first kill always
   misses, and the script must kill again, however slowly either process starts. Mutation: with the
   retry loop cut to one kill per wake, the test fails (`TimeoutExpired`: the script waits out the
   container). 30 runs, all at once on dev's 8 cores: 30 passed (90 signals).
5. **25.10 any-user case and the by-hand image tests**, on nebius-cpu128, 2026-11-08 05:44-06:08 UTC.
   Added `test_the_25_10_image_runs_as_any_user` (red on dev, no 25.10 image there). Counts: Images
   row 335 → 336, the image-needing tests 34 → 35, Python 1659 → 1660, grand total 8268 → 8269.
   The tests look up `peacockdb-<kind>:cudf-<v>`, the local tag `image.sh build` makes. The VM held
   an older image under `peacockdb-build:cudf-25.02` (`2a0c84414baa`). So each published ref was
   pulled (build images through `check-build-image.sh`, run images by the digest `image-ref.sh`
   printed on dev) and tagged `:cudf-<v>` for the run. Afterwards the tag went back to
   `2a0c84414baa`, and the images that were not there before were removed: 45 GB free before and
   after.
   - `test_image_any_user.py`: 25.02 (`sha256:b8da5a94…`) ok, 25.10 (`sha256:9d862501…`) ok,
     26.02 (`sha256:9199d3ef…`) ok.
   - `test_run_image.py`: 9 passed, run images 25.02 (`sha256:6aa01d4f…`) and 26.02
     (`sha256:5117c1ab…`). No GPU needed: ldd, inspect and a shell in the image. The staged installs
     were built on the VM from this tree: `stage-install.sh` in the 25.02 build image (synced to the
     default dir, warm), and `stage-install.sh --cudf 26.02 --no-release-bench` in the 26.02 image
     (`--dest peacockdb-beta-26`, cold, then removed). They were named by
     `PEACOCK_STAGED_INSTALLS`. The VM has no binutils, and `no-static-cudart.sh` calls `nm`. So an
     `nm` on PATH ran the 25.02 build image's `nm`: 1194 and 881 dynamic symbols read, exit 0 both.

Checks after the fixes, on dev: the guard
(`scripts/build-test.sh -- cargo test -p cost-report --test test_source_formatting`) 61 passed;
every image-free `scripts/env/tests` file (20) and every `scripts/nebius/tests` file (11) passed,
twice. The first parallel batch had one `test_stage_install_fails_naming_a_build_that_fails`
timeout (60 s; under 1 s alone). It did not come back in 32 more runs, 30 of them at once, and the
second batch passed whole. In that batch three tiny files took 44 s each at the same moment, so
the likely cause is a stall on dev, not the test. It was not disabled. shellcheck v0.9.0 over CI's
list: clean, and the image removed.
- 2026-11-08T06:10:16Z: signoff written; board `completeness approved`.
- Final device run: hard stop of all three VMs, then `pipeline.yml` dispatched 2026-11-08T06:10:24Z at `ecc27e12`: run 42453649934. A restart that finds it unfinished dispatches it again.
