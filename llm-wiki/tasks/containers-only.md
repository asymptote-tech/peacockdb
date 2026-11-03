# Retire the non-container build and run paths

Kind: production

**This task closes part (a) of [#129](../tickets/testinfra.md#t129) (26.02 CI leg image).** Part (b), the `s3-datasets` comment, is fixed alongside if it still applies.

containers-rebase brings the build and run images (`ghcr.io/asymptote-tech/peacockdb-{build,run}:cudf-{25.02,26.02}`).
new-hosts brings `scripts/env/run-gpu.sh`, the device run on any GPU host over ssh, proven on the
H100 by the chain's later tasks. Until this task, every machine but shad-gpu still has a second,
hand-built way to build and run:

- **CI.** `pipeline.yml` runs its build jobs in `rapidsai/base` images and installs the rest with
  `conda install` steps: `dataset-matrix` on 25.10a and 25.02, and `cpp-build-2502`.
  `exec-model-corpus.yml` and `validate-large.yml` have their own setups.
- **The L40S** (nebius-gpu): `nebius-sync.sh` and `nebius-test.sh` (renamed by new-hosts) run
  against a conda env installed by hand on the VM. CI's `gpu-tests` job uses them.
- **Local and verda:** `build-test.sh`, `build.sh` and `cargo-cudf.sh`, against a conda cuDF env
  from `conda-install-cudf.sh` / `conda-install-cudf-2506.sh`, on machines set up by `init.sh`,
  `init_fedora43.sh` and `create_verda_instance.sh`.

Two ways to do one thing means two to keep working, and the wiki describing both. This task leaves
one: the images, everywhere but shad-gpu.

## Done when

1. **CI builds and tests in the build image.** Every job in `pipeline.yml`, `exec-model-corpus.yml`
   and `validate-large.yml` that compiles or runs project code runs in
   `peacockdb-build:cudf-<v>`, pinned by the same digest or tag rule `image.sh` writes. No
   `conda install` step is left for project dependencies.
   - **The cuDF matrix: 25.02 and 25.10.** `dataset-matrix` keeps two legs. The second leg,
     labelled `26.02` today but run in `rapidsai/base:25.10a` (#129, 26.02 CI leg image), becomes
     an honest `cudf: "25.10"` leg in a new **`peacockdb-build:cudf-25.10`** image. That image is
     for CI only: a build image with no run image. It is built from committed conda specs and pins,
     as the other build images are (`docker/conda/cudf-25.10.spec`, `docker/pins.env`). It is
     published through `images.yml` and carries the same `peacockdb.inputs` label check. This
     closes #129 part (a). `build-test.md` and every place that names the 26.02 CI leg say 25.10.
     The 26.02 images stay for developer and device use.
   - CI stays green on the task's PR, the cost-report job included, with the same case counts
     per tier. Any count that moves is explained.
2. **The L40S runs through `run-gpu.sh`.** `gpu-tests` prepares, ships and runs on nebius-gpu
   through `run-gpu.sh` and the run image. The hand-built conda env on the VM is no longer read.
   `nebius-test.sh` and `nebius-sync.sh` are deleted, or reduced to what `run-gpu.sh` lacks, each
   kept line justified. The CI wiring guard (`cost-report` `test_source_formatting`) follows:
   - the three GPU target lists still agree;
   - `--test-threads=1` is still asserted for both GPU runners.
3. **Local and verda builds go through the build image.** One documented command builds every
   shape (rust-only, cudf/FFI, `--features gpu`, the staged binaries and the benchmark binary)
   inside the image, over the checkout. `build-test.sh` either becomes that command or is deleted.
   The `cargo-cudf.sh` and `CUDF_ROOT` rules for a host build go with it. The per-worktree
   `CARGO_TARGET_DIR` rule stays: a build script bakes `CARGO_MANIFEST_DIR`.
4. **Deleted:** `conda-install-cudf.sh` and `conda-install-cudf-2506.sh`, plus every reference to
   them. `init.sh`, `init_fedora43.sh` and `create_verda_instance.sh` are deleted, or reduced to
   installing Docker and the NVIDIA toolkit where `bootstrap-vm.sh` / `run-gpu.sh prepare` do not
   already, with no conda.
5. **shad-gpu is untouched.** `build-test-shadgpu.sh`, `setup-glibc.sh`, `lib/shadgpu-env.sh` and
   their docs stay as they are, because shad-gpu has no Docker. Its binaries may still be built
   in the build image and shipped; that is the builder's choice.
6. **The wiki describes one path.** `build-test.md` and `prompts.md` lose the conda and host-build
   instructions:
   - the "Containers" section becomes the build and run section;
   - "Nebius host" names `run-gpu.sh` for every Nebius GPU VM;
   - shad-gpu's section says it is the one exception, and why.

   Every command in the wiki was run once on the image path during the task.

## Scope

| path | change |
|---|---|
| `.github/workflows/pipeline.yml`, `exec-model-corpus.yml`, `validate-large.yml`, `.github/actions/` | jobs in the build image |
| `docker/conda/cudf-25.10.spec`, `docker/pins.env`, `docker/build.Dockerfile`, `scripts/env/image.sh`, `.github/workflows/images.yml` | the CI-only 25.10 build image |
| `llm-wiki/tickets/testinfra.md` | #129 |
| `scripts/build-test.sh`, `scripts/build.sh`, `scripts/cargo-cudf.sh`, `scripts/conda-install-cudf*.sh`, `scripts/init.sh`, `scripts/init_fedora43.sh`, `scripts/create_verda_instance.sh`, `scripts/nebius-sync.sh`, `scripts/nebius-test.sh` | reduced or deleted |
| `scripts/env/` | the local build command; whatever `run-gpu.sh` needs for CI |
| `cost-report/tests/test_source_formatting/` | the CI wiring guard |
| `llm-wiki/build-test.md`, `llm-wiki/prompts.md`, `.claude/agents/*.md` where they name the old paths | one path |

Component-level API: none.

## Constraints

- Publishing the new and changed images needs `.github/workflows/images.yml` on master. It lands
  there as a separate commit by the human, approved in advance, before this task's first dispatch.

- No engine change, and no test is weakened to make a job pass in the image. A test that fails in
  the image and passed outside it is a finding: fix the image or ticket it.
- The CI GPU job keeps its actor allowlist and environment secrets (`build-test.md` "CI and its
  secrets").
- The dataset and DuckDB pins stay those of `docker/pins.env`, and the generated sf1 stays
  byte-identical to today's.
