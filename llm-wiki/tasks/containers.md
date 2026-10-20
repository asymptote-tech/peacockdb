# Build and device-run environments as published images, and a full benchmark on a new device

Kind: production

**This task closes [#226](../tickets/benchmarks.md#t226).** Standing up a machine is slow and paid
for by the hour: a CPU host gets uv, Python 3.13, a venv and an rsync of the tree and the data by
hand; a GPU host gets a user, two conda environments (~14 GB) and ssh access by hand. This task
makes both a pulled image and one command: on a fresh VM, a file of keys and one script bring up
a container in which the agent ensemble runs, and on a fresh GPU machine, ssh access and one
script run the device tiers or the benchmark in a container and bring the results home. Its first
real use is the benchmark of every device-enabled query on an 80 GB A100 or H100, each result
file naming the device it came from.

## Done when

1. A fresh CPU VM: one script and a keys file bring up a container with the keys in place and sf1
   generated, and the ensemble starts on a chain. Nothing else is done by hand.
2. A fresh GPU machine of any provider, created by hand: given ssh access, one script installs
   Docker and the NVIDIA Container Toolkit if they are missing, pulls the run image, mounts the
   binaries (built in the build image) and the data, runs the device tests or the benchmark, and
   brings the results home.
3. The benchmark of every tpch and tpcds query the registry enables on the device, at sf40, on an
   A100 or H100 80 GB, is committed, each file naming its device, driver, CUDA, cuDF and host.
4. A build image and a run image for each of cuDF 25.02 and 26.02 are public on
   `ghcr.io/asymptote-tech/`.

## The images

**Build image, one per cuDF version.** Everything every build mode needs — rust-only, the FFI
build against `CUDF_ROOT`, `--features gpu`, the staged test binaries and the benchmark binary —
chosen at run time, not by image:
- the cuDF conda prefix, from an explicit spec (`conda list --explicit`) of the nebius-gpu
  environment the committed device answers came from: `rapids-cuda-12.2` for 25.02 (libcudf
  25.02.02, Arrow 19.0.1) and `rapids-26.02` for 26.02. The specs are committed. Not
  `rapidsai/base` patched afterwards, whose Arrow differs;
- the compiler and nvcc for the version (gcc-12 for 25.02, gcc-14 for 26.02; nvcc 12.9), cmake
  4.2.3, ninja 1.12.1, a pinned Rust stable (≥ 1.85), ccache, patchelf;
- DuckDB CLI exactly 1.5.4; Python 3.13 with pandas 3.0.6, pyarrow 25.0.1, numpy, matplotlib and
  duckdb 1.5.4 for `scripts/exec_model` — the committed costs depend on these bytes;
- git, gh, ssh, rsync, jq, curl, aws CLI, and provider CLIs as tools with no data in them;
- for agentic development: Claude Code at a pinned version with updates off, and the
  `superpowers@claude-plugins-official` plugin the project's agents start from;
- everything a build or data generation fetches at run time today, fetched at image build:
  rapids-cmake, gtest (CPM), flatbuffers (FetchContent), DuckDB's `tpch` and `tpcds` extensions.
  A fresh VM then needs only the registry and S3.

Agents run as an ordinary user: `claude --dangerously-skip-permissions`, as
`scripts/ensemble-watchdog.sh` runs it, refuses root.

**Run image, one per cuDF version.** The runtime of the same cuDF — libcudf, rmm, Arrow 19 and
their dependencies, the CUDA runtime — and nothing to build with. The binaries and testdata are
mounted; testdata read-write, since a run writes `benchmark-results/` and its record. It runs on
any host with an NVIDIA driver and `docker run --gpus all`.

**No secret is in any layer**: the images are public. Keys come at run time from a file outside
the image (`--env-file`) or read-only mounts: `CLAUDE_CODE_OAUTH_TOKEN`, `GH_TOKEN` (with
`workflow` scope, or git over ssh), the ssh key to GPU hosts, the S3 keys, and a provider's values
only where that provider is used.

## Several instances on one VM

As many as the VM holds, each isolated: its own clone or worktree, target directories, Claude
session data (`CLAUDE_CONFIG_DIR` on a volume) and ensemble state. What never changes is on disk
once: the image layers, shared copy-on-write, and read-only volumes for sf1, the embeddings and
cargo's downloaded crates; ccache may be one shared directory. Two instances never collide on a
path: not in a worktree's absolute `.git` pointer, not in the directory a remote host is synced
into.

## Data

sf1 is generated in the container. The external embeddings (GloVe, DEEP1B, ~1.1 GB, fetched today
by `fetch_embeddings.sh` from outside sites) go to S3 once and come from there, so vector queries
answer as they do on the developers' hosts. sf40 comes from S3 where a bucket holds it.

## Hosts, of any provider

Neither the images nor the new scripts name a machine, an address, a user's home or a provider.
Machines are created by hand; the tooling gets ssh access — host, user and key, or a password for
a first login, after which it installs its own key. Everything provider-specific is a thin adapter
behind one interface, `start / stop / status / ip / ssh`; where a provider has no API worth using,
the adapter is ssh alone and `start`/`stop` do nothing. nebius is the first real one.

The existing path stays untouched: `scripts/gpu-host.sh`, `gpu-host-sync.sh`, `gpu-host-test.sh`,
`gpu-host/ci-ship.sh`, `build-test-shadgpu.sh` and their hard-coded IDs and paths serve CI and
today's runs. The new path is built beside them; switching to it and retiring the old one is
later work.

## The benchmark

- The branch moves onto master before the run, so the case set holds what master's engine runs.
- The case set follows the registry's device columns (`testdata/cost-registry.csv` `gpu_*`) instead
  of the hand-written list in `corpus_benchmark_cases.inc`; a query that fails on the device is
  recorded as failed and the run goes on.
- Provenance (#226): device, driver, CUDA, cuDF and host in each result file's trailer and in the
  record's heading. Results lie under `testdata/benchmark-results/<device>/<dataset>.sf<sf>/<mode>`;
  the committed H200 files move under `h200/`. The per-call record is per device: its own S3 key
  and its own line in `records.sha256`. `verify-26.02.md`'s `cudf-<version>/` layout is reconciled
  with this one.
- The card is an A100 or H100 with 80 GB, as the provider has one: the benchmark's pool is a fixed
  69 GiB. The L40S and the H200 are out of this task.
- The benchmark is one long process, and nebius-gpu's idle watcher stops a VM after four hours of
  one test process; the run accounts for it.

**Decided at the run, not before** (the human's): whether the rowgroup modes run for every query
(estimated days) or the run takes the three other modes first (estimated one to two hours), and
whether tpcds sf40 (15 GB, in no bucket) is generated on the GPU machine or put in S3 once.

## Scope

| path | change |
|---|---|
| `docker/` | new: the two Dockerfiles, the committed conda specs, the pins |
| `scripts/env/` | new: image build and publish, the CPU-VM launcher, the host adapters, GPU-host bootstrap, the device run, its tests |
| `.github/workflows/images.yml` | new, dispatch only: builds and publishes the images. The test pipeline is untouched |
| `testdata/fetch_embeddings.sh`, S3 | the embeddings from S3 |
| benchmark harness (`peacockdb-core/src/test_support/corpus_benchmark.rs`, `tests/peacock_gpu_benchmarks.rs`, `tests/test_corpus_goldens/benchmark.rs`, `tests/common/corpus_benchmark_cases.inc`), the record scripts under `scripts/calibration/` | the registry-driven case set, provenance, the per-device tree |
| `testdata/benchmark-results/` | the H200 files under `h200/`; the new device's results |
| `llm-wiki/build-test.md`, `llm-wiki/tickets/benchmarks.md`, `llm-wiki/tasks/verify-26.02.md` | how to use the images and hosts; #226 closed; the reconciled layout |

Component-level API: none in the engine. If the device's identity needs an ABI query beyond the
existing cuDF version read, it is one additive C ABI symbol.

## Restriction

No change to the test pipeline (`pipeline.yml`, `exec-model-corpus.yml`) or to the existing GPU
host scripts. No change to engine behaviour.

## Verification bar

- Byte-identity: in the build image, the cheap tier and the prototype's committed outputs
  reproduce exactly; rust-only and the FFI build pass as on the native hosts.
- Network: after the image is pulled, the build and the data generation succeed with only the
  registry and S3 reachable.
- Isolation: two instances on one VM build and run at once without touching each other, and the
  disk holds one copy of the image.
- Device: in the run image on a GPU host, the device tiers the old path runs there pass with the
  same counts.
- The benchmark run of Done-when 3, committed.
