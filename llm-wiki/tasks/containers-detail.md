# containers — detail

## Known state

- The branch `ENS-containers` starts from `ENS-optimizer` (`611b5dcd`, PR #165, not yet merged);
  Task 10 moves it onto master once that merges.
- Facts the plan rests on, read on `611b5dcd`: CI builds in `rapidsai/base:25.02-cuda12.0-py3.12`
  and `rapidsai/base:25.10a-cuda12-py3.12` (the leg named 26.02, #129); the committed device
  answers came from nebius-gpu's `rapids-cuda-12.2` (libcudf 25.02.02, Arrow 19.0.1); its explicit
  specs exist only on that VM; `docker/cudf-build.Dockerfile` and `scripts/docker-build.sh` were in
  master from 2026-08-07 to `dbf44bcc` (2026-10-08), a build-only image retired when the build
  moved onto nebius-gpu natively.
- The benchmark today times 26 hand-listed cases (`corpus_benchmark_cases.inc`); the registry
  enables about 534 tpch and tpcds device cells. The H200 run of the 26 took 3,602 s, 96 % of it the
  six rowgroup cases. The benchmark pool is a fixed 69 GiB.
- Deferred to Task 11, the human's: the rowgroup modes for every query, and the source of tpcds
  sf40 (15 GB, in no bucket).


## Task 1: the conda specs

- `docker/conda/cudf-25.02.spec` is `conda list --explicit` of nebius-gpu's `rapids-cuda-12.2`. It
  equals the spec that env was created from, except for the `# created-by` line. It holds
  libcudf-25.02.02 (`cuda12_250303_g8139f3c84f_0`), librmm-25.02.00, libarrow-19.0.1 and
  cuda-version-12.2.
- `docker/conda/cudf-26.02.spec` is the same export of `rapids-26.02`. It holds libcudf-26.02.01
  (`cuda12_260205_5b9658c4`), librmm-26.02.00, libarrow-21.0.0 and cuda-version-12.9.
- The wiki names two paths for the 25.02 env on nebius-gpu, under `data/miniforge3` and under
  `miniforge3`. They are one env: the first is a symlink to the second.
- The md5 after each URL is anaconda.org's record of the file,
  `https://api.anaconda.org/dist/<channel>/<name>/<version>/<subdir>/<file>`.
  `scripts/env/spec_hashes.py <spec>...` fills a line without a hash, keeps one that agrees with
  the record, and refuses the whole spec, writing nothing, if one differs. The export itself
  carried none. A downloaded file matched its record for libzlib,
  ca-certificates, librmm and cuda-version.

What Tasks 2 and 3 must take from the specs:

- 26.02 carries Arrow 21.0.0. The spec's "Arrow 19" holds for 25.02 only.
- Each prefix carries its own Python 3.12.13: pandas 2.2.3 in 25.02, 2.3.3 in 26.02. The image's
  3.13 venv must come first on PATH, because a different pandas moves every committed cost line.
- The 25.02 spec ships nvcc 12.2 (`cuda-nvcc-impl-12.2.140`). `scripts/build.sh:44` takes
  `CUDACXX=$(which nvcc)`, so with `/opt/cudf/bin` first on PATH the 25.02 image would compile
  with 12.2, and nothing would go red. The committed device answers' 25.02 build used nvcc
  12.9.86 with gcc-12 ([#260](../tickets/system-hardening.md#t260)). On nebius-gpu that nvcc
  comes from the 26.02 env through a wrapper. `NVCC_VERSION=12.9.86`: Task 2 installs it outside
  `/opt/cudf` and puts it first on PATH.

## Task 1: pin sources

The spec's pins are verbatim in `docker/pins.env`. Where the spec leaves a value open, the pin
is what the project uses today:

- `RUST_VERSION` 1.99.0: rustc on nebius-gpu.
- `PYTHON_VERSION` 3.13.0 and `NUMPY_VERSION` 2.5.4: the venv the committed costs ran in, which
  also holds pandas 3.0.6 and pyarrow 25.0.1.
- `MATPLOTLIB_VERSION` 3.11.2: a Python 3.13.0 plotting venv on the developer host, with numpy
  2.5.4. No other venv there has matplotlib.
- `PY_DUCKDB_VERSION` 1.5.4: `duckdb_result.py` refuses any other version (`build-test.md`).
- `CLAUDE_CODE_VERSION` 2.1.304: the installed Claude Code, a native install, not npm.
- `GH_VERSION` 2.102.1, the latest upstream release (2026-10-20). `GH_SHA256` is the
  `linux_amd64.tar.gz` line of its `gh_2.102.1_checksums.txt`, and a download matched it. The
  developer host's 2.45.0 is not a pin to copy: it is `2.45.0-1ubuntu0.3+esm3`, an Ubuntu Pro ESM
  rebuild that a plain ubuntu:24.04 image does not get.
- `MICROMAMBA_VERSION` 2.9.0-0 and `MICROMAMBA_SHA256`: the latest release and its published
  `micromamba-linux-64.sha256`, which a download matched. The project used none before.
- `AWS_CLI_VERSION` 2.38.3: the latest v2 release (aws-cli's CHANGELOG). The project pins none,
  and CI installs awscli v1 unpinned through pip. AWS publishes a PGP signature for the zip, not
  a checksum.
- cmake 4.2.3, ninja 1.12.1 and the DuckDB CLI zip have no sha256 in pins.env yet; Task 2 decides.
- pins.env has no flatbuffers key, because `cpp/CMakeLists.txt:99` pins `GIT_TAG v25.12.19` and
  `Cargo.lock` pins `flatc-fork`.
- pins.env has no Node key. Ubuntu 24.04's nodejs is too old for `npm i -g
  @anthropic-ai/claude-code` (it needs Node 22+). The native installer at `CLAUDE_CODE_VERSION`
  needs no Node.
