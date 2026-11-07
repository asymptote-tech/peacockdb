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

## Notes for a restart

- `images.yml` is on master (workflow "Images", id 409646666), from the merge of #193; Task 1
  Step 8's master check passes. A dispatch runs the ref's own copy of the file.
- dev's disk: 13 GB free on `/` at branch time; local images `peacockdb-build:cudf-25.02` (15.4 GB),
  `peacockdb-run:cudf-26.02` (10.2 GB), `peacockdb-run:cudf-25.02` (8.3 GB). The run images are
  re-pullable and may be removed to make room for Task 1 Step 6's 25.10 build.
- `testdata/calibration/hbm-parts/` in this worktree is an untracked leftover of tp4-sized: never
  stage it.
- verda: no address known (`list_verda_instances.sh` lacks `VERDA_CLIENT_ID`); treated as down.

## Counts before

(Task 3 Step 1, coordinator.)

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
