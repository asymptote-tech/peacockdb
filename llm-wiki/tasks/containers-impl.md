# containers implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a build image and a run image per cuDF version (25.02, 26.02) on
`ghcr.io/asymptote-tech/`, one script that brings up an isolated ensemble instance on a fresh
CPU VM, one script that runs the device tiers or the benchmark in the run image on any GPU host
reached over ssh, and with them the benchmark of every device-enabled query on an 80 GB A100 or
H100, each result naming its device.

**Architecture:** Docker images built from committed inputs (`docker/`): a conda prefix from an
explicit spec, toolchain tarballs at pinned versions, everything a build fetches pre-fetched. A
CPU VM runs N containers of one build image; per-instance state lives in the instance's worktree
and home, machine directories owned by its user, and shared data is mounted read-only from the
machine. GPU hosts are reached through a provider adapter with one
interface; the binaries built in the build image are shipped with the data and run in the run
image. The benchmark harness gains a registry-driven case set and per-device provenance.

**Tech stack:** Docker (BuildKit), conda/mamba explicit specs, bash, Python 3 for script tests
(the `scripts/gpu-host/tests` harness pattern: fakes on `PATH`, no network), Rust (`test_support`),
GitHub Actions (one dispatch-only workflow), ghcr.io, Nebius S3.

**Spec:** [`containers.md`](containers.md).

## Global constraints

- No secret in any image layer; the images are public. Logins and keys come from the machine at
  `docker run` time. Claude Code is never in an image: the machine's install is mounted.
- Never edit `pipeline.yml`, `exec-model-corpus.yml`, `scripts/gpu-host.sh`, `gpu-host-sync.sh`,
  `gpu-host-test.sh`, `scripts/gpu-host/*`, `build-test-shadgpu.sh`, `scripts/lib/shadgpu-env.sh`.
- No new script or image names a machine ID, an address, `/home/<user>/…` or a provider; the
  provider lives only in `scripts/env/host/<provider>.sh`, its CLI binary in the build image with
  no configuration.
- Pins, verbatim: cuDF 25.02 from nebius-gpu's `rapids-cuda-12.2` (libcudf 25.02.02, Arrow
  19.0.1), cuDF 26.02 from its `rapids-26.02`; gcc-12 (25.02), gcc-14 (26.02); nvcc 12.9;
  cmake 4.2.3; ninja 1.12.1; DuckDB CLI 1.5.4; Python 3.13, pandas 3.0.6, pyarrow 25.0.1,
  duckdb 1.5.4; Rust stable ≥ 1.85, one version pinned; Claude Code one version pinned,
  `DISABLE_AUTOUPDATER=1`; CUDA architectures `80;90` unchanged.
- Agents run as a non-root user: the machine user's uid:gid (`docker run --user`). The image works
  for any uid; its default user is `peacock` (uid 1000).
- Build artefacts on the developer's host only under `/build`, never `/home`.
- Every new script has a `--help` and exits non-zero with a one-line reason on a missing input.
- The new tests (`scripts/env/tests/`) run locally and in the build image; wiring them into the
  test pipeline is later work, like the switch from the old GPU path.

## Review focus

1. A machine where Claude Code or gh is not logged in, or a needed key is missing: the launcher
   refuses to start and names what is missing, never starts an ensemble that fails an hour later on
   auth. → Task 6, `test_up_refuses_missing_login`, `test_up_refuses_missing_key`.
2. A chain started while a container of that chain exists on the machine, for this user or another:
   refused, naming the user, never two coordinators on one board. → Task 6,
   `test_up_refuses_duplicate_instance`; Task 6a, `test_up_refuses_a_chain_another_user_runs`.
3. A GPU host whose driver is older than the run image's CUDA needs: the run stops before
   shipping data, naming driver and required minimum. → Task 7, `test_run_refuses_old_driver`.
4. A benchmark interrupted mid-run (preemption, watcher): what was measured is kept, a rerun under
   `PEACOCK_BENCH_RESUME=1` skips completed cases (without it every case is measured again), and
   good results are never overwritten with a partial file. → Task 9,
   `test_partial_run_keeps_completed_cases`.
5. A result directory for a device name with spaces or slashes (`NVIDIA H100 80GB HBM3`): the
   directory name is a stable slug (`h100-80gb-hbm3`), the full name in the file. → Task 8,
   `device_slug_is_stable`.

---

### Task 1: the committed inputs — conda specs and pins

**Files:**
- Create: `docker/conda/cudf-25.02.spec`, `docker/conda/cudf-26.02.spec` (explicit specs)
- Create: `docker/pins.env` (every version pin above, `NAME=value` lines)
- Create: `scripts/env/tests/test_pins.py`

**Interfaces:**
- Produces: `docker/pins.env` keys `CMAKE_VERSION NINJA_VERSION RUST_VERSION DUCKDB_VERSION
  PYTHON_VERSION PANDAS_VERSION PYARROW_VERSION CLAUDE_CODE_VERSION GH_VERSION GCC_25_02 GCC_26_02
  NVCC_VERSION`; spec files named `cudf-<version>.spec`.

- [x] **Step 1:** Export both specs from nebius-gpu with `conda list --explicit -p <env>`
  (`rapids-cuda-12.2`, `rapids-26.02`; the host is reached by hand, ssh as `dmitry`, address from
  the human). Record in the detail file the libcudf, librmm, libarrow and cuda-version lines of each.
- [x] **Step 2: failing test** `test_pins.py::test_specs_hold_the_pinned_cudf` — parse each spec;
  assert the 25.02 spec has `libcudf-25.02.02` and `libarrow-19.0.1`, the 26.02 spec a
  `libcudf-26.02.` line; `test_pins_match_the_spec` — every key above present in `pins.env`,
  `DUCKDB_VERSION=1.5.4`, `PANDAS_VERSION=3.0.6`, `PYARROW_VERSION=25.0.1`, `CMAKE_VERSION=4.2.3`.
- [x] **Step 3:** write `pins.env`; run `python3 scripts/env/tests/test_pins.py` → PASS.
- [x] **Step 4:** commit.

### Task 2: the build image

**Files:**
- Create: `docker/build.Dockerfile` (`ARG CUDF=25.02|26.02`)
- Create: `scripts/env/image.sh` — `image.sh build <build|run> <cudf>` and `image.sh push <build|run> <cudf>`
- Create: `scripts/env/tests/test_image.py`
- Modify: `testdata/generate_testdata.sh` only if needed so it finds pre-installed DuckDB
  extensions (an env var, no behaviour change when unset)

**Interfaces:**
- Consumes: Task 1's `pins.env` and specs.
- Produces: image `ghcr.io/asymptote-tech/peacockdb-build:cudf-<v>` (and `:cudf-<v>-<git sha>`);
  inside: `CUDF_ROOT=/opt/cudf`, default user `peacock` (uid 1000) and any uid under `--user`,
  `/opt/prefetch` holding the CPM cache (`CPM_SOURCE_CACHE`), rapids-cmake and flatbuffers sources,
  DuckDB extensions under `/opt/duckdb-ext` (`PEACOCK_DUCKDB_EXTENSION_DIR`, read by
  `generate_testdata.sh`), the Python venv at `/opt/venv`. No Claude Code and no plugin: both are the
  machine's (Task 6).

- [x] **Step 1: failing test** `test_image.py::test_build_args` — `image.sh build build 25.02
  --dry-run` prints a `docker build` line with `--build-arg CUDF=25.02`, every `pins.env` value as a
  build arg, tag `ghcr.io/asymptote-tech/peacockdb-build:cudf-25.02`; refuses `cudf 25.10`.
- [x] **Step 2:** write the Dockerfile: base `ubuntu:24.04`; apt toolchain; micromamba creates
  `/opt/cudf` from the spec; cmake/ninja tarballs; rustup at `RUST_VERSION`; DuckDB zip; venv;
  gh; aws; a prefetch stage
  that configures the C++ build once (`scripts/build.sh --configure` against `/opt/cudf`) and keeps
  only the fetched sources; `INSTALL tpch; INSTALL tpcds;` into `/opt/duckdb-ext`.
- [x] **Step 3:** `image.sh build build 25.02` on the developer's host (BuildKit cache under
  `/build`); record image size in the detail file.
- [x] **Step 4: verify, network off** — in `docker run --network none` with the tree mounted:
  `cargo test -p peacockdb-core --features rust-only --lib` green; the FFI build (`cargo build -p
  peacockdb-core`) and `cargo build --features gpu --tests` compile; `generate_testdata.sh --bench
  tpch` at sf1 succeeds. Paste counts into the detail file.
- [x] **Step 5: verify byte-identity** — the cheap Python tier (48 files, 521 passed) and
  `scripts/exec_model/run.py --bench tpch --query q3 q5 --mode tp1-single` into a temp dir,
  diffed against the committed sections: identical.
- [x] **Step 6:** the same for `26.02` (Steps 3–4; Step 5 is 25.02's alone).
- [x] **Step 7:** commit.

### Task 3: the run image

**Files:**
- Create: `docker/run.Dockerfile` (`ARG CUDF`)
- Modify: `scripts/env/image.sh` (the `run` kind)
- Create: `scripts/env/tests/test_run_image.py`

**Interfaces:**
- Consumes: Task 1's specs (the runtime subset: no compilers, no `-dev` packages).
- Produces: `ghcr.io/asymptote-tech/peacockdb-run:cudf-<v>`; mounts `/peacock/install` (ro, the
  shipped `cpp/install`), `/peacock/testdata` (rw); entry `/opt/run/run-tests.sh <list|bench|filter>`
  that sets `LD_LIBRARY_PATH=/peacock/install/lib:/opt/cudf/lib` for the binaries only.

- [x] **Step 0:** `scripts/env/no-static-cudart.sh` (Task 2) runs on every `libpeacock_gpu.so`
  this task's test takes, so a library pulling a second CUDA runtime in fails here.
- [x] **Step 1: failing test** `test_run_image.py::test_every_needed_library_resolves` — given
  `ldd` output of `libpeacock_gpu.so` and each staged binary run in the image (captured by the
  test via `docker run --entrypoint ldd`), no line reads `not found`.
- [x] **Step 2:** write the Dockerfile (micromamba create from the spec minus build-only
  packages), build both versions, run the test against binaries built in Task 2's image.
- [x] **Step 3:** record both images' sizes; commit.

### Task 4: publish

**Files:**
- Create: `.github/workflows/images.yml` — `workflow_dispatch` only, inputs `kind`, `cudf`; builds
  with `image.sh` and pushes with `GITHUB_TOKEN` (`packages: write`)
- Create: `scripts/env/tests/test_images_workflow.py`
- Modify: `llm-wiki/build-test.md` — pulling and running the images

- [x] **Step 0a:** the four images rebuilt from a clean tree, so each carries its
  `:cudf-<v>-<commit>` tag; `image.sh push … --dry-run` names both tags of each.
- [x] **Step 0b: failing test** `scripts/env/tests/test_images_workflow.py`, then `images.yml`:
  dispatch only, `packages: write` and `contents: read`, built and pushed through `image.sh`,
  the runner's disk freed and checked against the largest build's measured peak.
- [x] **Step 0c:** `build-test.md`'s "Images", and the `scripts/env/tests` cases in its counts.
- [x] **Step 1:** until the workflow is on master (GitHub dispatches only workflows the default
  branch holds), publish with `image.sh push` from the developer's host, with a token holding
  `write:packages` (the human adds the scope: `gh auth refresh -s write:packages`). Push all four
  images, set the packages public, and check a pull on a host with no credentials succeeds.
- [x] **Step 2:** commit.

### Task 5: data — embeddings from S3

**Files:**
- Modify: `testdata/fetch_embeddings.sh` — a `--from-s3` source beside the current ones
- Create: `scripts/env/upload-embeddings.sh`
- Test: `scripts/env/tests/test_embeddings.py`

**Interfaces:**
- Produces: the bucket `embeddings`, keys `<sha256>/<file>`, pinned by
  `testdata/embeddings.sha256`; `fetch_embeddings.sh --from-s3` reads `AWS_*`.

- [x] **Step 1: failing test** — with a fake `aws` on `PATH`, `fetch_embeddings.sh --from-s3`
  downloads exactly the pinned keys and refuses a file whose sha256 differs.
- [x] **Step 2:** implement; commit the pin.
- [ ] **Step 3 (when the origins are needed, on no run path):** create the bucket, upload once
  from a dev host. No corpus query reads a vector column, and the sf1 goldens hold for the
  synthetic vectors CI generates, so the external embeddings are an input of `tpch.sf40` (already
  in its bucket), of `query_params.jsonl` and of the vector goldens only, regenerated on a dev
  host. The bucket keeps those inputs should the origins move or vanish.

### Task 6: the CPU-VM launcher and isolated instances

**Files:**
- Create: `scripts/env/bootstrap-vm.sh` — prepares the machine once: Docker if missing, Claude Code
  at `CLAUDE_CODE_VERSION` with the `superpowers` plugin at `SUPERPOWERS_VERSION` and
  `DISABLE_AUTOUPDATER=1` set for the machine user, gh at `GH_VERSION` from the same tarball as the
  image and at the same path, `/usr/local/bin/gh` (`gh auth setup-git` writes that absolute path
  into `~/.gitconfig`, and the container must resolve it),
  the repository cloned, the cuDF checkout of the commit it pins fetched (one a commit, with the
  sparse paths the build image's prefetch takes, which the FFI build's configure reads), sf1 generated for the clone's generator inputs with the generator's
  default vectors and `PEACOCK_NO_FETCH=1`, as CI generates it (in the build image, into the
  machine's data directory; no embeddings are fetched: no corpus query reads a vector column); it ends by printing the two
  logins left to do by hand
- Create: `scripts/env/up.sh` — `up.sh <chain> [--cudf <v> | --image <tag>] [--env <file>]`;
  `down.sh <chain>` (the instance is the chain: Task 6a)
- Create: `scripts/env/tests/test_up.py`
- Create: `scripts/lib/image-inputs.sh` — the files an image is made from, each with its
  sha256: `docker/conda/cudf-<v>.spec`, `docker/pins.env`, the kind's Dockerfile and
  `.dockerignore`, and what that Dockerfile copies (`venv.lock`, `cmake-wrapper.sh`,
  `runtime-spec.sh`, `run-tests.sh`); for the build image also `third_party/cudf`'s commit and the
  flatbuffers `GIT_TAG` in `cpp/CMakeLists.txt`, which its prefetch fixes
- Modify: `scripts/env/image.sh` — every image gets the label `peacockdb.inputs`, that list
- Create: `scripts/lib/data-inputs.sh` — the hash of what sf1's bytes follow from:
  `testdata/generate_testdata.sh`, the `DUCKDB_VERSION` line of
  `docker/pins.env`, and the generator's arguments per dataset
- Create: `scripts/lib/machine.sh` — the machine's layout and the three build-image steps
  `bootstrap-vm.sh` and `up.sh` share: the image pulled and checked, the cargo fetch, the data set
- Create: `scripts/lib/s3.sh` — the S3 endpoint and region every bucket script passes, from
  `AWS_ENDPOINT_URL` and `AWS_REGION` (the names the aws CLI itself reads), so the storage moves
  with the env file and no new script names a provider's endpoint. `calibration-bucket.sh` and
  `embeddings-bucket.sh` source it; where the variables are unset, the old path's value stays in
  `calibration-bucket.sh` alone, so `build-test-shadgpu.sh` keeps working unedited. Access is the
  S3 API with static keys only: no bucket script calls a provider's API
- Modify: `llm-wiki/build-test.md` — a "Containers" section

**Interfaces:**
- Consumes: Task 2's image, Task 5's fetch.
- Produces: per instance, named after its chain: a worktree of the machine's clone, and the clone's `.git`
  read-write, both at their machine paths inside the container, so the worktree's `.git` pointer
  resolves and commits land; the build in the worktree's own `target/`, and a home of its own on
  the machine mounted at `$HOME` (a named volume would be root's, which the uid cannot write); its
  own `/etc/passwd` and `/etc/group`, the image's with the machine user added, read-only. From
  the machine, read-only: the Claude Code install at its machine path (its plugin and marketplace records hold absolute
  paths, and `~/.local/bin/claude` links into `~/.local/share/claude/versions/<v>`), sf1, the
  machine's cuDF checkout of the commit the worktree's gitlink names over its `third_party/cudf`
  (`<data dir>/cudf/<commit>/`, fetched once a commit under a lock),
  cargo's registry (filled by `bootstrap-vm.sh` with `cargo fetch` in the build image;
  read-only is what makes it safe to share, since cargo's package lock `$CARGO_HOME/.package-cache`
  is outside it and per container). Read-write: Claude's config and session data as a directory —
  `CLAUDE_CONFIG_DIR` set to one directory on the machine and in the container, not a single-file
  mount of `~/.claude.json` (a file replaced by rename leaves a bind mount on the old inode) —
  gh's config, ccache. HOME is the machine user's home path and writable in the container (Docker
  creates a mount point's missing parents as root): Claude and gh write `~/.cache`, `~/.local/state`.
  Git's identity and gh's credential helper come from the machine's `~/.gitconfig`, mounted
  read-only. For the GPU steps a chain runs: the ssh key to GPU hosts, mounted read-only at the
  path `GPU_SSH_KEY_FILE` names; `~/.config/peacock/hosts/` (Task 7's host files) read-only; a
  provider CLI's config read-only, at the path `PROVIDER_CONFIG_DIR` names, where that provider is
  used. The container runs with
  `--user <machine uid>:<gid>` and `DISABLE_AUTOUPDATER=1`. The env file, `~/.config/peacock/peacock.env` unless
  `--env <file>` names another, is required for every start, carries the
  values that are not a login and is passed only as `--env-file`: `AWS_ACCESS_KEY_ID`,
  `AWS_SECRET_ACCESS_KEY`, `AWS_ENDPOINT_URL` and `AWS_REGION` required, `GPU_SSH_KEY_FILE` and a provider's values optional.
  The registry is read-only, so a `Cargo.lock` that names a crate the machine lacks would fail
  every build: `up.sh` runs `cargo fetch` for the instance's worktree in the build image, with the
  registry writable for that one step only, before it starts the instance.
  An image built from other inputs than the instance's tree fails later, mid-build or mid-run,
  with an error that does not name the cause. `up.sh` pulls `:cudf-<v>`, or the tag `--image`
  names (a `:cudf-<v>-<commit>` keeps an instance on one image), and compares its
  `peacockdb.inputs` with `image-inputs.sh` over the instance's worktree; a difference or a missing
  label refuses the start, naming each file that differs. The images published before this label
  are refused until republished. Task 7's `prepare` makes the same check of the run image.
  The data is generated once per machine and per generator, never per instance or per start: the
  machine's data directory (`/var/lib/peacock`, shared by the machine's users, unless
  `PEACOCK_DATA_DIR` names another; Task 6a) holds
  `<hash>/{tpch.sf1,tpcds.sf1}`, the hash being `data-inputs.sh` over a
  worktree. `up.sh` mounts the set matching the instance's worktree read-only at its
  `testdata/<dataset>` paths; when none matches, it generates that set once in the build image,
  under a lock, so two instances starting together generate it once. Two chains whose branches
  differ in the generator each get their own set; an old set is removed by hand.

- [x] **Step 1: failing tests** (fakes on `PATH` recording argv): `test_up_refuses_missing_login`
  (no Claude credentials, or `gh auth status` failing → exit 2, stderr names which);
  `test_up_refuses_missing_key` (`--env` file without one of the four `AWS_*` values, each in
  turn, absent or empty → exit 2, stderr names it and no value); `test_up_fetches_before_it_starts` (the one writable-registry step precedes the
  instance, and the instance's mount is `:ro`); `test_up_commits_from_the_container` (a commit made in the instance's
  worktree inside the container lands in the machine's clone, with the machine's git identity);
  `test_up_refuses_duplicate_instance`; `test_up_mounts_per_instance` (two instances → disjoint
  worktrees and homes, the same machine mounts, the data and the Claude install `:ro`,
  the GPU ssh key, the host files and a provider's config `:ro`);
  `test_up_runs_as_the_machine_user` (`--user` is the caller's uid:gid, never 0);
  `test_up_never_passes_keys_as_args` (no key value in argv; only `--env-file`);
  `test_up_mounts_data_by_generator_inputs` (a present set is mounted `:ro` and nothing is
  generated; a changed `generate_testdata.sh` makes a new hash, generated once, the old set
  untouched; two `up.sh` at once generate it once); `test_s3_endpoint_from_env` (each bucket
  script's aws argv carries `AWS_ENDPOINT_URL`'s value; `embeddings-bucket.sh` names no endpoint);
  `test_image_labels_its_inputs` (`image.sh build --dry-run` passes the label, equal to
  `image-inputs.sh` over the tree; a changed `venv.lock` changes the build image's label, not the
  run image's); `test_up_refuses_stale_image` (a fake `docker image inspect` whose label differs
  in `venv.lock` → exit 2, stderr names `docker/venv.lock`; no label → exit 2).
- [x] **Step 2:** implement; `scripts/ensemble-watchdog.sh <chain>` starts in the instance's
  worktree inside the container.
- [ ] **Step 3: verify on a fresh VM, with Task 6a** (a verda or nebius CPU VM the human creates):
  from nothing, on the laptop, `provision-vm.sh <login>@<address> --ref <this branch>`: the PAT
  recipe, the Claude login, and nothing else by hand. A scratch branch off this one, pushed and
  never merged, holds a board with two scratch chains, A and B, each `(base: <scratch>)` so that
  its first branch carries that board; then, from the laptop,
  `up.sh A --board origin/<scratch> && up.sh B --board origin/<scratch>`. Both build rust-only at once, and one builds the FFI tier
  once; `docker system df` shows one image copy; in each, `claude --version`, `gh auth status` and
  the ensemble's first coordinator turn, with no login asked. A second developer on the same VM
  (`provision-vm.sh … --user <other>`) reuses sf1 and the cuDF checkout, generating and fetching
  nothing, and `up.sh A` as them is refused naming the first. A rerun of `provision-vm.sh` skips
  every step. Record wall times.
- [ ] **Step 4:** commit.

### Task 6a: one command per machine, one per chain

A person readies a VM with one command run on their own machine (the laptop) and starts a chain
with another. Nothing in either names a person, an address or a password manager; several
developers share the storage keys and keep their own token, GPU key and Linux user.

**Files:**
- Create: `scripts/env/provision-vm.sh` — `provision-vm.sh <login>@<address> [--env <file>]
  [--user <name>] [--name <vm>] [--ref <branch>] [--new-pat]`, run on the laptop
- Create: `scripts/lib/peacock-config.sh` — `~/.config/peacock/`'s layout on the laptop and on a
  VM, the env file's checks, the `vm` file, and the forwarding `up.sh` and `down.sh` share
- Create: `scripts/lib/board.sh` — a chain's base and first branch, read from the board
- Modify: `scripts/env/up.sh`, `scripts/env/down.sh` — the chain is the instance; labels; the
  default env file; forwarding from the laptop; the chain's branch in a new worktree
- Modify: `scripts/env/bootstrap-vm.sh` — the machine-wide data directory, the machine marker
- Modify: `scripts/lib/machine.sh` — the data directory's default, group-writable files in it
- Create: `scripts/env/tests/test_provision.py`, `scripts/env/tests/laptop.py` (a laptop and a VM
  reached through a fake `ssh`)
- Create: `scripts/env/tests/test_up_branch.py` (the branch a new worktree is on)
- Modify: `scripts/env/tests/machine.py`, `test_up.py`, `test_bootstrap.py`
- Modify: `llm-wiki/build-test.md` — "Trust", "A fresh VM", "The env file", "Instances";
  `llm-wiki/README.md` — "Run a chain" points at them for a chain on a VM

**Interfaces:**
- **The laptop's files**, in `~/.config/peacock/` (mode 700, every file 600):
  - `peacock.env`: exactly `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_ENDPOINT_URL`,
    `AWS_REGION`, each once and not empty; blank and `#` lines are skipped, as docker skips them.
    Shared between developers, received from whoever administers the storage. Any other key is
    refused by its name, never its value, so a token never rides along in a forwarded file. It is
    kept with every CR dropped and a final newline.
  - `github-pat`: the person's fine-grained PAT. Personal; never in `peacock.env`.
  - `gpu_ed25519`, `gpu_ed25519.pub`: the person's key to GPU hosts, made with an empty
    passphrase when missing.
  - `vm`: one line a VM this laptop provisioned, `<name> <user>@<address>`; the last is the
    default.
- **`provision-vm.sh`.** `<login>` is the VM's login user, with sudo. Every argument is checked
  before the first side effect. On the laptop: `--env` is needed only while the laptop has no
  `peacock.env`, and replaces it when given; with no `github-pat`, or with `--new-pat`, it prints
  the URL for a new fine-grained token and its recipe (resource owner and repository from the
  clone's `origin`; Contents rw, Pull requests rw, Workflows rw, Actions rw, Commit statuses r,
  Metadata r; an expiry), reads the token hidden, and refuses one that is not `github_pat_…` or
  cannot read the repository, checked with the token in gh's environment (`GH_TOKEN`), never
  argv; the GPU key pair is made when missing and its `.pub` printed with what it is for; the git
  identity is the laptop's `user.name` and `user.email`, refused naming the one missing. On the VM:
  - as `<login>`, with sudo: groups `docker` and `peacock`; the developer's user (`--user`, the
    laptop's `id -un` by default) in both, with `<login>`'s `authorized_keys` when it has none,
    and passwordless sudo (the docker group is root already, so it widens nothing; bootstrap
    installs with it). Two developers on one VM are two Linux users;
  - every later step as that user, over ssh: `bootstrap-vm.sh` copied and run (`--ref` passed
    on), skipped when the machine marker is there, and then a `--ref` the VM's clone is not on is
    refused;
  - secrets only on ssh's stdin, never argv, a remote command line or shell history:
    `peacock.env` and the key pair into `~/.config/peacock/`, the VM's `peacock.env` gaining
    `GPU_SSH_KEY_FILE` at the VM's key, on a line of its own; the PAT into `gh auth login --with-token
    --insecure-storage` when gh has no login there or with `--new-pat`, then `gh auth setup-git`.
    The PAT is never in the VM's `peacock.env`;
  - git's identity set for that user; the VM recorded in the laptop's `vm`, `--name` (the address
    by default) replacing a line of that name and becoming the default;
  - last, `claude auth login` over `ssh -t`, skipped when Claude's credentials are there.
  `provision-vm.sh --new-pat`, with or without a target, replaces the PAT on the laptop and on
  every VM in `vm` it can reach, naming each it cannot.
- **The machine marker** `~/.config/peacock/machine`, written by `bootstrap-vm.sh` as its last
  step: this machine runs instances. `up.sh` and `down.sh` on a machine without it forward their
  arguments over ssh to the default VM (`--vm <name>` another), as the recorded user, to the
  VM's `~/peacockdb/scripts/env/`; with no marker and no `vm` file they exit 2, naming
  `provision-vm.sh`. `--vm` on a machine with the marker is refused. The remote command carries
  `PEACOCK_FORWARDED=1`, and a call that carries it is never forwarded again. Both scripts and
  `provision-vm.sh` refuse a bash older than 4.4 (macOS ships 3.2) in two lines of bash 3.2
  before any source, and print their help with bash alone.
- **The data directory** is machine-wide: `/var/lib/peacock` unless `PEACOCK_DATA_DIR` names
  another, group `peacock`, setgid, mode 2775, made by `bootstrap-vm.sh` with sudo and owned by
  the user who ran it. What `up.sh` and bootstrap write there — locks, `.partial` stages, cuDF
  checkouts, the data sets, the generator's output in the build image — is made under umask 002,
  so another developer in `peacock` reuses a set, takes a lock and removes a crashed stage. A cuDF
  checkout mounted into another uid's container is trusted there by git (`safe.directory`, passed
  as `GIT_CONFIG_*`). The cargo registry stays per user. `up.sh` refuses a data directory that is missing or not writable.
- **`up.sh <chain> [--branch <b> | --board <ref>] [--cudf <v> | --image <tag>] [--env <file>]
  [--vm <name>]`, `down.sh <chain> [--vm <name>]`.** The instance is the chain: the worktree
  `~/peacock-instances/<chain>`, its home and the container `peacock-<user>-<chain>`, labelled
  `peacockdb.chain=<chain>` and `peacockdb.user=<user>`; `down.sh` finds its own container by the
  two labels, since `-` in both names makes the name ambiguous. `--env` defaults to
  `~/.config/peacock/peacock.env`, still required: missing or incomplete is exit 2 naming it, and
  so is `GH_TOKEN` or `GITHUB_TOKEN` in it, by name. A container labelled with the chain, any user's and
  in any state, refuses the start (exit 2, naming the user): the board is one per repository, so a
  chain runs once. A machine-wide lock in the data directory holds from that check to the start.
  The watchdog always starts: `scripts/ensemble-watchdog.sh <chain>`.
- **The chain's branch is never guessed.** The board names no branch, and branches are named
  unlike their tasks (`ENS-optimizer` for `join-optimizer.md`, `ENS1-…`). A new worktree is made
  from origin, fetched with hooks off.
  - `--branch <b>`, a chain that ran before: the worktree is on `<b>`, a local branch tracking
    `origin/<b>`, made from it when the clone has none, fast-forwarded when behind it, refused when
    diverged from it, naming both, or when another worktree has it checked out, naming that
    worktree. Refused: no `origin/<b>`; no `## Chain <chain>` on its board.
  - Without `--branch`, a new chain: its section on the board at `--board <ref>` (`origin/master`
    by default) gives its base, `(base: <b>)`, and its first task, the lowest-numbered
    `### <n>. [...](<task>.md)` heading. Before anything is made, the board at the tip of every
    `origin/ENS*` branch is read: one whose `## Chain <chain>` holds that task at `building`,
    `reviewing`, `completing`, `completeness approved` or `done` (`blocked(…)` and `rebase
    needed(…)` counting as the state they carry, words after a state changing nothing) refuses the start,
    every such branch named: "chain <c> already ran on <branches>; resume with --branch <b>". So
    does an `ENS-<task>` origin or the clone has already. Otherwise the worktree is on a new local
    `ENS-<task>` at `origin/<base>`: the first branch checked out before the watchdog starts, as
    `llm-wiki/README.md`, "Run a chain", has it. Refused too: no section for `<chain>` on the
    board (define the chain there first), a section with no task or no base, a base origin lacks,
    a base whose board lacks the chain: the coordinator reads the board the new branch takes
    from its base.
  - An existing worktree is reused as it is, its branch left alone; a `--branch` naming another
    branch, or a `--board`, is refused.

- [ ] **Step 1: failing tests** (fakes on `PATH`: `ssh` recording argv and stdin and running the
  remote command in a fake VM's home, `sudo`, `getent`, `groupadd`, `useradd`, `usermod`,
  `install`, `gh`, `claude`, `docker`; a stub `bootstrap-vm.sh` recording its arguments):
  - `test_provision.py::test_provision_readies_a_vm` — the laptop's directory 700 and its files
    600; the recipe printed with the origin's owner and repository; gh asked with the token in
    `GH_TOKEN`; on the VM the developer's user in `docker` and `peacock` with the login's
    `authorized_keys`; bootstrap run as that user with `--ref`; the VM's `peacock.env` the four
    keys and `GPU_SSH_KEY_FILE` at the VM's key, both key files there; gh logged in with the PAT
    and `setup-git` run; the laptop's git identity on the VM; `vm` holds `<name> <user>@<address>`;
    the last ssh is `-t` and `claude auth login`;
  - `test_provision_never_puts_the_pat_in_argv_or_the_env_file` — neither the PAT nor a key's
    value is in any ssh, gh or sudo argv, in stdout or stderr, or in the VM's `peacock.env`;
  - `test_provision_refuses_an_extra_key_by_name` — a `peacock.env` with `GH_TOKEN=github_pat_…`
    or `FOO=…` → exit 2, stderr names the key and holds no value, no ssh made, nothing copied;
  - `test_provision_refuses_an_incomplete_env_file` — each of the four absent, empty or twice;
  - `test_provision_needs_an_env_file_the_first_time` — no `peacock.env` and no `--env` → exit 2
    naming `--env`;
  - `test_provision_refuses_a_missing_git_identity` — each of `user.name`, `user.email` named;
  - `test_provision_refuses_a_token_it_cannot_use` — `gho_…`, and a `github_pat_…` gh cannot read
    the repository with → exit 2, nothing saved, no ssh;
  - `test_provision_rerun_skips_what_is_done` — a second run reads no token, adds no user, runs no
    bootstrap, logs gh and Claude in again never, and leaves one `vm` line;
  - `test_provision_new_pat_replaces_it_everywhere` — two VMs recorded, one unreachable: the
    laptop's PAT replaced, the reachable VM's gh given the new one, the other named, exit 0;
  - `test_provision_refuses_bad_arguments_before_any_call`, `test_provision_has_help`;
  - `test_up.py::test_up_forwards_from_a_laptop_to_its_default_vm` — no marker: one ssh to the
    last `vm` line's target running its `~/peacockdb/scripts/env/up.sh` with the same arguments,
    `--vm` picking another, the remote exit status returned, no docker call; the same for
    `down.sh`; `test_up_without_a_vm_says_to_provision`;
  - `test_up_takes_the_default_env_file` — no `--env`: `--env-file` is
    `~/.config/peacock/peacock.env`; that file missing → exit 2 naming it;
  - `test_up_names_the_container_after_user_and_chain` — the name and both labels;
  - `test_up_refuses_a_chain_another_user_runs` — another user's container labelled with the chain
    → exit 2 naming that user;
  - `test_up_reuses_a_data_set_another_user_made` — a second user, another home, the same data
    directory: no generation, no cuDF fetch, the first user's set mounted, the checkout's path in
    the container's `safe.directory`;
  - `test_up_leaves_the_shared_directory_writable_by_the_group` — the locks, the cuDF checkout and
    the generated set group-writable under a caller's umask 022, the generator run under umask 002;
  - `test_up_refuses_a_missing_data_directory`; `test_the_data_directory_is_machine_wide_by_default`;
  - `test_two_users_starting_one_chain_at_once_start_it_once` — two users' `up.sh A` together,
    the fake `docker ps` holding a window open: one instance, one refusal naming the lock;
  - `test_down_finds_the_container_by_its_labels` — user `a-b`'s `down.sh C` leaves user `a`'s
    chain `b-C` alone; `test_up_never_forwards_twice`; `test_up_refuses_a_token_in_the_env_file`;
  - `test_help_needs_no_sed`; `test_laptop_scripts_need_bash_4_4` — 3.2 and 4.3 refused naming
    `brew install bash`, the same two lines before the first source in each of the three scripts;
  - `test_up_branch.py::test_up_resumes_a_branch_named_unlike_its_task`;
    `test_up_refuses_a_chain_that_ran_before` — every origin branch whose board has the first task
    past `approved to build` named, none at `approved to build`, and an `ENS-<task>` on origin or in
    the clone refused; `test_up_refuses_an_unknown_branch` (no such origin branch; a board without
    the chain); `test_up_brings_a_local_branch_up_to_its_origin` (fast-forward; diverged refused);
    `test_up_starts_a_new_chain_at_its_base`; `test_up_reads_a_new_chain_from_the_board_it_is_given`;
    `test_up_refuses_a_chain_the_board_lacks` (no section; a section with no task);
    `test_up_refuses_a_base_whose_board_lacks_the_chain`; `test_up_refuses_a_branch_checked_out_elsewhere`
    (the branch not moved); `test_board_ran_reads_a_state_past_approved_to_build` (`blocked(done)`
    and `rebase needed(reviewing)` ran; `blocked(new)`, `blocked(approved to build)` and
    `approved to build (resumed …)` did not);
    `test_up_rerun_keeps_the_worktree_s_branch` (another `--branch`, or `--board`, refused);
  - `test_bootstrap.py::test_bootstrap_makes_the_machine_wide_data_directory` — `sudo install -d
    -o <user> -g peacock -m 2775 <data dir>`, before the first download;
  - `test_provision_normalises_the_env_file` — a received file without a final newline, and one
    with CRLF, kept as plain lines; a laptop file without a final newline still gives the VM
    `GPU_SSH_KEY_FILE` on a line of its own; `--env` naming the laptop's own `peacock.env` keeps
    it whole; `test_provision_takes_comments_in_the_env_file`;
    `test_provision_refuses_a_ref_the_vm_clone_is_not_on` (its git call with hooks and fsmonitor off).
- [ ] **Step 2:** implement; `llm-wiki/build-test.md`'s "Trust", "A fresh VM" and "Instances" say
  what a developer does once and per chain, that every developer on a VM is in the docker group
  and so can read the others' PAT and keys, and that `peacock.env` alone may be passed on.
- [ ] **Step 3:** verified with Task 6's Step 3, on one fresh VM.
- [ ] **Step 4:** commit.

### Task 7: host adapters and the device run

**Files:**
- Create: `scripts/env/host/ssh.sh`, `scripts/env/host/nebius.sh` — each implements
  `start|stop|status|ip|ssh <cmd>` for `HOST_NAME` read from `~/.config/peacock/hosts/<name>.env`
- Create: `scripts/env/gpu.sh` — `gpu.sh <host> prepare|ship|run <list|bench|filter> [--cudf v]|pull`
- Modify: `scripts/env/provision-vm.sh` — `provision-vm.sh --gpu-host <name> <user>@<address>
  [--provider <p>]`: writes the host file on the laptop and copies `hosts/` to every VM in the
  `vm` file it can reach
- Create: `scripts/env/tests/test_host.py`, `test_gpu.py`

**Interfaces:**
- Consumes: Tasks 2–3 images.
- Produces: host file keys `PROVIDER=ssh|nebius HOST USER [PASSWORD_FIRST_LOGIN=1]
  [INSTANCE_ID]`, in `~/.config/peacock/hosts/<name>.env` on the laptop, written by
  `provision-vm.sh --gpu-host` and copied to each VM, where an instance mounts `hosts/` read-only.
  A host file names no key: the key to every GPU host is the person's `gpu_ed25519`, which the VM's
  `peacock.env` names as `GPU_SSH_KEY_FILE` (Task 6a). `prepare` installs that key's `.pub` on a
  password-only host, then Docker + NVIDIA
  Container Toolkit if missing, puts the data on the machine once (sf1 generated in the build
  image there under the `data-inputs.sh` hash with the default vectors, as Task 6 keeps it; sf40 from S3 — skipped when
  present and its checksum holds), then
  `docker pull` the run image; `ship` rsyncs `cpp/install` and the query files to
  `~/peacock/<instance>/` (instance-named, never the old path's dirs), never the data;
  `run` executes the run image with `--gpus all` and `--hostname` set to the machine's name, which
  the benchmark writes as `host=` (Task 8); `pull` brings `benchmark-results/`, the record
  and logs home.

- [ ] **Step 1: failing tests** (fake `ssh`/`rsync`/`nebius`): `test_ssh_adapter_start_is_noop`;
  `test_nebius_adapter_needs_instance_id`; `test_prepare_installs_key_on_password_host`;
  `test_run_refuses_old_driver` (fake `nvidia-smi` reporting a driver below the run image's CUDA
  minimum → exit 3 before `ship`, message names both); `test_ship_dir_is_instance_named`;
  `test_run_passes_the_machines_hostname` (`docker run` gets `--hostname` with the name the fake
  host's `hostname` answers); `test_prepare_refuses_stale_run_image` (the run image's
  `peacockdb.inputs` differs from the tree → exit 2 before `ship`, naming the file).
- [ ] **Step 2:** implement.
- [ ] **Step 3: verify on nebius-gpu** (the L40S, by hand-given ssh): `gpu.sh nebius-l40s prepare`,
  `ship`, `run list` with binaries from the build image: the device tiers pass with the same
  counts as the latest `GPU Tests (nebius-gpu)` run. Record counts and wall times.
- [ ] **Step 4:** commit.

### Task 8: benchmark provenance and the per-device tree (#226)

**Files:**
- Modify: `peacockdb-core/src/test_support/corpus_benchmark.rs:54-58`,
  `peacockdb-core/tests/peacock_gpu_benchmarks.rs:127-131`,
  `peacockdb-core/tests/test_corpus_goldens/benchmark.rs:384-386`, the trailer and record writers
- Modify: `scripts/calibration/split_record.py`, `testdata/calibration/records.sha256` (per device)
- Move: `testdata/benchmark-results/{tpch,tpcds}.sf40` → `testdata/benchmark-results/h200/`
- Modify: `llm-wiki/tickets/benchmarks.md` (#226 closed), `llm-wiki/tasks/verify-26.02.md`
  (its `cudf-<version>/` layout reconciled)

**Interfaces:**
- Produces: `fn device_slug(name: &str) -> String`; a `Provenance { device, driver, cuda, cudf,
  host }` written as `device=… driver=… cuda=… cudf=… host=…` in each trailer and record heading;
  results at `benchmark-results/<slug>/<dataset>.sf<sf>/<mode>`.

- [x] **Step 1: failing tests** — `device_slug_is_stable` (`"NVIDIA H100 80GB HBM3"` →
  `"h100-80gb-hbm3"`, `"NVIDIA H200"` → `"h200"`, no `/` or space ever);
  `trailer_names_its_provenance` (all five keys, in that order); the existing benchmark golden test
  reads the moved H200 tree.
- [x] **Step 2:** implement; the device name, driver and CUDA from the runtime (an additive ABI
  symbol only if the existing reads cannot give them); move the H200 files; record pins per device.
  The H200's own two pins keep their flat shape: moving them waits on the migration #226 lists.
- [x] **Step 3:** rust-only tiers green; commit.

### Task 9: the registry-driven case set

**Files:**
- Modify: `peacockdb-core/tests/common/corpus_benchmark_cases.inc` (generated or replaced),
  `peacockdb-core/tests/peacock_gpu_benchmarks.rs`
- Test: in `peacockdb-core/tests/test_corpus_goldens/benchmark.rs` or a new rust-only test

**Interfaces:**
- Consumes: `testdata/cost-registry.csv` `gpu_*` columns (rows `enabled` per mode).
- Produces: the benchmark's case list = every tpch/tpcds row's enabled device modes at sf40; a
  case's failure is recorded (`status=failed reason=…` in its section) and the run continues; a
  rerun under `PEACOCK_BENCH_RESUME=1` skips cases whose complete section exists, and without it
  measures every case.

- Note: `every_declared_case_has_a_section_in_the_tree` holds the case list to every device
  directory, so it goes red once the list declares more than `h200/` holds. This task decides
  which trees the list is held to.
- [x] **Step 1: failing tests** — `the_benchmark_cases_follow_the_registry` (the case list equals
  the registry's enabled device cells for tpch and tpcds, mode by mode);
  `a_failed_case_is_recorded_and_the_run_goes_on`; `test_partial_run_keeps_completed_cases`.
- **Step 2:**
  - [x] implement;
  - [x] rust-only tiers green;
  - [x] commit.

### Task 10: onto master

- [ ] **Step 1:** when `ENS-optimizer` is merged, rebase this branch onto master (backup branch
  first, rerere off); every commit passes rust-only and the cost-report crate; Task 9's case list
  regenerated from master's registry. Record conflicts in the detail file. If not yet merged when
  Tasks 1–9 are done, stop and report.

### Task 11: the benchmark run on an 80 GB A100 or H100

- [ ] **Step 1:** the human creates the machine on nebius and settles the rowgroup modes; record
  it in the detail file. tpcds sf40 comes from the bucket `tpcds-sf40`, so every device reads the
  same parquet files (row-group layout is part of what the rowgroup modes measure). It is
  published (detail file, "tpcds sf40 in its bucket"); its sums are compared with the copy the
  H200 results were measured on once shad-gpu is back, and `prepare` checks the files against
  the bucket.
- [ ] **Step 2:** binaries built in the build image (`--features gpu`, the benchmark target);
  `gpu.sh <host> prepare && ship && run bench && pull`, the idle watcher's four-hour limit handled
  (the run in segments, or the watcher configured for it on that host).
- [ ] **Step 3:** publishing a run made of segments, resumed (`PEACOCK_BENCH_RESUME=1`) cases and
  failures. Each segment writes its own record (`run-tests.sh bench` refuses one holding rows),
  their `# run: allocator=` headings differ (it names the free GiB each process saw), a held case
  writes no rows, and a segment killed between appending a case's rows and writing its section
  leaves rows for a case it has no section for, which the resumed segment measures again, so its
  rows are in two segments' records; `split_record.py` refuses a record short of the list.
  Decide how the segments' records join into one per dataset, and what a failed or deferred case
  means to the publish; failing tests first, in `scripts/calibration/tests/`.
- [ ] **Step 4:** results under `benchmark-results/<slug>/`, the record split, uploaded and pinned
  per device; the benchmark golden tests green on the new tree. Commit. Task 9 settled which trees
  the list holds: a new device's tree is held to the whole list, `h200/` to its own
  `h200/cases.inc`. `every_declared_case_has_a_section_in_the_tree` is red if the run times fewer
  cases than the list declares (the deferred rowgroup modes); committing such a tree needs a named
  exception in that test (device, modes, reason) that the human approves.


### Task 12: the ensemble end to end on a scratch chain

After Task 6 Step 3; its device step after Task 7 Step 3. The ensemble carries one small task
from `approved to build` to `done` inside an instance, with nobody at the machine: Task 6 Step 3
shows the first coordinator turn, this shows the whole path a real chain takes.

- [ ] **Step 1:** a scratch branch off master, pushed, whose board has one chain `<c>`,
  `(base: <scratch>)` so that its first branch carries that board, and one task, approved to build: a change small enough for one developer pass, touching code and its test,
  with one device step through `gpu.sh` on the Task 7 host. Never merged.
- [ ] **Step 2:** `up.sh <c> --board origin/<scratch>` from the laptop, onto the Task 6 machine;
  nothing else is done by hand
  until the chain stops. Done when: the task reaches `done` (its commit pushed, its PR open, CI
  green); the device step's results pulled into the worktree; no login or key asked for at any
  point; `<c>.status` says why the watchdog stopped. Record wall times per board state.
- [ ] **Step 3:** the PR closed unmerged, the scratch branches deleted on origin, `down.sh <c>`;
  `docker volume ls` and the machine clone's `git worktree list` show nothing of the instance.
  Record the run in the detail file.
