# containers implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a build image and a run image per cuDF version (25.02, 26.02) on
`ghcr.io/asymptote-tech/`, one script that brings up an isolated ensemble instance on a fresh
CPU VM, one script that runs the device tiers or the benchmark in the run image on any GPU host
reached over ssh, and with them the benchmark of every device-enabled query on an 80 GB A100 or
H100, each result naming its device.

**Architecture:** Docker images built from committed inputs (`docker/`): a conda prefix from an
explicit spec, toolchain tarballs at pinned versions, everything a build fetches pre-fetched. A
CPU VM runs N containers of one build image; per-instance state lives in named volumes, shared
read-only data in shared volumes. GPU hosts are reached through a provider adapter with one
interface; the binaries built in the build image are shipped with the data and run in the run
image. The benchmark harness gains a registry-driven case set and per-device provenance.

**Tech stack:** Docker (BuildKit), conda/mamba explicit specs, bash, Python 3 for script tests
(the `scripts/gpu-host/tests` harness pattern: fakes on `PATH`, no network), Rust (`test_support`),
GitHub Actions (one dispatch-only workflow), ghcr.io, Nebius S3.

**Spec:** [`containers.md`](containers.md).

## Global constraints

- No secret in any image layer; the images are public. Keys only at `docker run` time.
- Never edit `pipeline.yml`, `exec-model-corpus.yml`, `scripts/gpu-host.sh`, `gpu-host-sync.sh`,
  `gpu-host-test.sh`, `scripts/gpu-host/*`, `build-test-shadgpu.sh`, `scripts/lib/shadgpu-env.sh`.
- No new script or image names a machine ID, an address, `/home/<user>/…` or a provider; the
  provider lives only in `scripts/env/host/<provider>.sh`.
- Pins, verbatim: cuDF 25.02 from nebius-gpu's `rapids-cuda-12.2` (libcudf 25.02.02, Arrow
  19.0.1), cuDF 26.02 from its `rapids-26.02`; gcc-12 (25.02), gcc-14 (26.02); nvcc 12.9;
  cmake 4.2.3; ninja 1.12.1; DuckDB CLI 1.5.4; Python 3.13, pandas 3.0.6, pyarrow 25.0.1,
  duckdb 1.5.4; Rust stable ≥ 1.85, one version pinned; Claude Code one version pinned,
  `DISABLE_AUTOUPDATER=1`; CUDA architectures `80;90` unchanged.
- Agents run as a non-root user in the image (uid 1000, `peacock`).
- Build artefacts on the developer's host only under `/build`, never `/home`.
- Every new script has a `--help` and exits non-zero with a one-line reason on a missing input.
- The new tests (`scripts/env/tests/`) run locally and in the build image; wiring them into the
  test pipeline is later work, like the switch from the old GPU path.

## Review focus

1. A keys file missing one value: the launcher refuses to start and names the value, never starts
   an ensemble that fails an hour later on `gh` or `claude` auth. → Task 6, `test_up_refuses_missing_key`.
2. A second instance started with the same name as a running one: refused, never two containers on
   one worktree volume. → Task 6, `test_up_refuses_duplicate_instance`.
3. A GPU host whose driver is older than the run image's CUDA needs: the run stops before
   shipping data, naming driver and required minimum. → Task 7, `test_run_refuses_old_driver`.
4. A benchmark interrupted mid-run (preemption, watcher): what was measured is kept and the rerun
   skips completed cases, never overwrites good results with a partial file. → Task 9,
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

- [ ] **Step 1:** Export both specs from nebius-gpu with `conda list --explicit -p <env>`
  (`rapids-cuda-12.2`, `rapids-26.02`; the host is reached by hand, ssh as `dmitry`, address from
  the human). Record in the detail file the libcudf, librmm, libarrow and cuda-version lines of each.
- [ ] **Step 2: failing test** `test_pins.py::test_specs_hold_the_pinned_cudf` — parse each spec;
  assert the 25.02 spec has `libcudf-25.02.02` and `libarrow-19.0.1`, the 26.02 spec a
  `libcudf-26.02.` line; `test_pins_match_the_spec` — every key above present in `pins.env`,
  `DUCKDB_VERSION=1.5.4`, `PANDAS_VERSION=3.0.6`, `PYARROW_VERSION=25.0.1`, `CMAKE_VERSION=4.2.3`.
- [ ] **Step 3:** write `pins.env`; run `python3 scripts/env/tests/test_pins.py` → PASS.
- [ ] **Step 4:** commit.

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
  inside: `CUDF_ROOT=/opt/cudf`, user `peacock` (uid 1000), `/opt/prefetch` holding the CPM cache
  (`CPM_SOURCE_CACHE`), rapids-cmake and flatbuffers sources, DuckDB extensions under
  `/opt/duckdb-ext`, the Python venv at `/opt/venv`, `claude` with the `superpowers` plugin
  installed in the image's user scope.

- [ ] **Step 1: failing test** `test_image.py::test_build_args` — `image.sh build build 25.02
  --dry-run` prints a `docker build` line with `--build-arg CUDF=25.02`, every `pins.env` value as a
  build arg, tag `ghcr.io/asymptote-tech/peacockdb-build:cudf-25.02`; refuses `cudf 25.10`.
- [ ] **Step 2:** write the Dockerfile: base `ubuntu:24.04`; apt toolchain; micromamba creates
  `/opt/cudf` from the spec; cmake/ninja tarballs; rustup at `RUST_VERSION`; DuckDB zip; venv;
  `npm i -g @anthropic-ai/claude-code@$CLAUDE_CODE_VERSION`; gh; aws; the plugin; a prefetch stage
  that configures the C++ build once (`scripts/build.sh --configure` against `/opt/cudf`) and keeps
  only the fetched sources; `INSTALL tpch; INSTALL tpcds;` into `/opt/duckdb-ext`.
- [ ] **Step 3:** `image.sh build build 25.02` on the developer's host (BuildKit cache under
  `/build`); record image size in the detail file.
- [ ] **Step 4: verify, network off** — in `docker run --network none` with the tree mounted:
  `cargo test -p peacockdb-core --features rust-only --lib` green; the FFI build (`cargo build -p
  peacockdb-core`) and `cargo build --features gpu --tests` compile; `generate_testdata.sh --bench
  tpch` at sf1 succeeds. Paste counts into the detail file.
- [ ] **Step 5: verify byte-identity** — the cheap Python tier (48 files, 521 passed) and
  `scripts/exec_model/run.py --bench tpch --query q3 q5 --mode tp1-single` into a temp dir,
  diffed against the committed sections: identical.
- [ ] **Step 6:** the same for `26.02` (Steps 3–4; Step 5 is 25.02's alone).
- [ ] **Step 7:** commit.

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

- [ ] **Step 1: failing test** `test_run_image.py::test_every_needed_library_resolves` — given
  `ldd` output of `libpeacock_gpu.so` and each staged binary run in the image (captured by the
  test via `docker run --entrypoint ldd`), no line reads `not found`.
- [ ] **Step 2:** write the Dockerfile (micromamba create from the spec minus build-only
  packages), build both versions, run the test against binaries built in Task 2's image.
- [ ] **Step 3:** record both images' sizes; commit.

### Task 4: publish

**Files:**
- Create: `.github/workflows/images.yml` — `workflow_dispatch` only, inputs `kind`, `cudf`; builds
  with `image.sh` and pushes with `GITHUB_TOKEN` (`packages: write`)
- Modify: `llm-wiki/build-test.md` — pulling and running the images, the hosts, the keys file

- [ ] **Step 1:** until the workflow is on master (GitHub dispatches only workflows the default
  branch holds), publish with `image.sh push` from the developer's host, with a token holding
  `write:packages` (the human adds the scope: `gh auth refresh -s write:packages`). Push all four
  images, set the packages public, and check a pull on a host with no credentials succeeds.
- [ ] **Step 2:** commit.

### Task 5: data — embeddings from S3

**Files:**
- Modify: `testdata/fetch_embeddings.sh` — a `--from-s3` source beside the current ones
- Create: `scripts/env/upload-embeddings.sh`
- Test: `scripts/env/tests/test_embeddings.py`

**Interfaces:**
- Produces: S3 key prefix `embeddings/<sha256>/{glove,deep1b}…` in the existing datasets bucket,
  pinned by `testdata/embeddings.sha256`; `fetch_embeddings.sh --from-s3` reads `AWS_*`.

- [ ] **Step 1: failing test** — with a fake `aws` on `PATH`, `fetch_embeddings.sh --from-s3`
  downloads exactly the pinned keys and refuses a file whose sha256 differs.
- [ ] **Step 2:** implement; upload once from a dev host; commit the pin.
- [ ] **Step 3:** in the build image, sf1 generated with the S3 embeddings answers the tpchv
  goldens as the dev hosts do (`cargo test --features rust-only --test test_cpu_corpus -- tpchv`
  green). Commit.

### Task 6: the CPU-VM launcher and isolated instances

**Files:**
- Create: `scripts/env/up.sh` — `up.sh <instance> --keys <file> [--chain <c>] [--cudf 25.02]`;
  `down.sh <instance>`; `scripts/env/bootstrap-vm.sh` (installs Docker on Ubuntu if missing)
- Create: `scripts/env/tests/test_up.py`
- Modify: `llm-wiki/build-test.md` — a "Containers" section

**Interfaces:**
- Consumes: Task 2's image, Task 5's fetch.
- Produces: per instance `<i>`: volumes `peacock-<i>-work` (clone + worktrees, mounted at
  `/work/<i>` so absolute `.git` paths never collide), `peacock-<i>-target`, `peacock-<i>-claude`
  (`CLAUDE_CONFIG_DIR`); shared: `peacock-sf1` (ro after first generation), `peacock-embeddings`
  (ro), `peacock-cargo-registry`, `peacock-ccache`. Keys file keys: `CLAUDE_CODE_OAUTH_TOKEN
  GH_TOKEN AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY` required; `GPU_SSH_KEY_FILE` and
  `PROVIDER_*` optional.

- [ ] **Step 1: failing tests** (fake `docker` on `PATH` recording argv):
  `test_up_refuses_missing_key` (exit 2, stderr names `GH_TOKEN`);
  `test_up_refuses_duplicate_instance`; `test_up_mounts_per_instance_volumes` (two instances →
  disjoint `-work`/`-target`/`-claude` volumes, the same shared ones, shared ones `:ro` except
  ccache and the first sf1 run); `test_up_never_passes_keys_as_args` (no key value in argv; only
  `--env-file`).
- [ ] **Step 2:** implement; first start clones the public repo, generates sf1 once into the
  shared volume, fetches embeddings, and with `--chain` starts `scripts/ensemble-watchdog.sh <c>` in
  a worktree.
- [ ] **Step 3: verify on a fresh VM** (a verda or nebius CPU VM the human creates): from nothing,
  `bootstrap-vm.sh && up.sh a --keys k && up.sh b --keys k`; both build rust-only at once;
  `docker system df` shows one image copy; in each, `claude --version`, `gh auth status` and the
  ensemble's first coordinator turn on a scratch chain. Record wall times in the detail file.
- [ ] **Step 4:** commit.

### Task 7: host adapters and the device run

**Files:**
- Create: `scripts/env/host/ssh.sh`, `scripts/env/host/nebius.sh` — each implements
  `start|stop|status|ip|ssh <cmd>` for `HOST_NAME` read from `~/.config/peacock/hosts/<name>.env`
- Create: `scripts/env/gpu.sh` — `gpu.sh <host> prepare|ship|run <list|bench|filter> [--cudf v]|pull`
- Create: `scripts/env/tests/test_host.py`, `test_gpu.py`

**Interfaces:**
- Consumes: Tasks 2–3 images.
- Produces: host file keys `PROVIDER=ssh|nebius HOST USER KEY [PASSWORD_FIRST_LOGIN=1]
  [INSTANCE_ID]`; `prepare` installs its own key on a password-only host, then Docker + NVIDIA
  Container Toolkit if missing, then `docker pull` the run image; `ship` rsyncs `cpp/install` and
  the needed testdata to `~/peacock/<instance>/` (instance-named, never the old path's dirs);
  `run` executes the run image with `--gpus all`; `pull` brings `benchmark-results/`, the record
  and logs home.

- [ ] **Step 1: failing tests** (fake `ssh`/`rsync`/`nebius`): `test_ssh_adapter_start_is_noop`;
  `test_nebius_adapter_needs_instance_id`; `test_prepare_installs_key_on_password_host`;
  `test_run_refuses_old_driver` (fake `nvidia-smi` reporting a driver below the run image's CUDA
  minimum → exit 3 before `ship`, message names both); `test_ship_dir_is_instance_named`.
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

- [ ] **Step 1: failing tests** — `device_slug_is_stable` (`"NVIDIA H100 80GB HBM3"` →
  `"h100-80gb-hbm3"`, `"NVIDIA H200"` → `"h200"`, no `/` or space ever);
  `trailer_names_its_provenance` (all five keys, in that order); the existing benchmark golden test
  reads the moved H200 tree.
- [ ] **Step 2:** implement; the device name, driver and CUDA from the runtime (an additive ABI
  symbol only if the existing reads cannot give them); move the H200 files; record pins per device.
- [ ] **Step 3:** rust-only tiers green; commit.

### Task 9: the registry-driven case set

**Files:**
- Modify: `peacockdb-core/tests/common/corpus_benchmark_cases.inc` (generated or replaced),
  `peacockdb-core/tests/peacock_gpu_benchmarks.rs`
- Test: in `peacockdb-core/tests/test_corpus_goldens/benchmark.rs` or a new rust-only test

**Interfaces:**
- Consumes: `testdata/cost-registry.csv` `gpu_*` columns (rows `enabled` per mode).
- Produces: the benchmark's case list = every tpch/tpcds row's enabled device modes at sf40; a
  case's failure is recorded (`status=failed reason=…` in its section) and the run continues; a
  rerun skips cases whose complete section exists.

- [ ] **Step 1: failing tests** — `the_benchmark_cases_follow_the_registry` (the case list equals
  the registry's enabled device cells for tpch and tpcds, mode by mode);
  `a_failed_case_is_recorded_and_the_run_goes_on`; `test_partial_run_keeps_completed_cases`.
- [ ] **Step 2:** implement; rust-only tiers green; commit.

### Task 10: onto master

- [ ] **Step 1:** when `ENS-optimizer` is merged, rebase this branch onto master (backup branch
  first, rerere off); every commit passes rust-only and the cost-report crate; Task 9's case list
  regenerated from master's registry. Record conflicts in the detail file. If not yet merged when
  Tasks 1–9 are done, stop and report.

### Task 11: the benchmark run on an 80 GB A100 or H100

- [ ] **Step 1:** the human creates the machine on nebius and settles the two run-time decisions
  (rowgroup modes; tpcds sf40 source). Record both in the detail file.
- [ ] **Step 2:** binaries built in the build image (`--features gpu`, the benchmark target);
  `gpu.sh <host> prepare && ship && run bench && pull`, the idle watcher's four-hour limit handled
  (the run in segments, or the watcher configured for it on that host).
- [ ] **Step 3:** results under `benchmark-results/<slug>/`, the record split, uploaded and pinned
  per device; the benchmark golden tests green on the new tree. Commit.

