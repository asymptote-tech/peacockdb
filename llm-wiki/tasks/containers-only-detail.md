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
