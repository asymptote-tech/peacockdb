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
