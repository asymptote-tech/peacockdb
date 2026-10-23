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
- cmake 4.2.3, ninja 1.12.1 and the DuckDB CLI zip had no sha256 in Task 1; Task 2 pins them (below).
- pins.env has no flatbuffers key, because `cpp/CMakeLists.txt:99` pins `GIT_TAG v25.12.19` and
  `Cargo.lock` pins `flatc-fork`.
- pins.env has no Node key. Ubuntu 24.04's nodejs is too old for `npm i -g
  @anthropic-ai/claude-code` (it needs Node 22+). The native installer at `CLAUDE_CODE_VERSION`
  needs no Node.


## Task 2: the build image — decisions

State: both images are built from a dirty tree, so they have no commit tag and are not pushed.
Steps 3–6 are verified (below).

**The pins added to pins.env.**
- `CMAKE_SHA256`: the `linux-x86_64.tar.gz` line of Kitware's `cmake-4.2.3-SHA-256.txt`.
- `DUCKDB_SHA256`: GitHub's asset digest of `duckdb_cli-linux-amd64.zip` v1.5.4. It equals the
  sha256 of `/build/peacock/duckdb-1.5.4/duckdb.zip`, the CLI the committed data came from.
- `NINJA_SHA256` and `AWS_CLI_SHA256` are measured on a download: ninja publishes no checksum and
  GitHub has no digest for its 2024 assets; AWS signs with PGP only.
- `PYTHON_STANDALONE_TAG` 20241008 and `PYTHON_SHA256`: Python 3.13.0 is python-build-standalone's
  build, with the `.sha256` that release publishes. Ubuntu 24.04 has 3.12 only; the dev host's
  3.13.0 is linuxbrew's.
- `NEBIUS_VERSION` 0.12.293, the stable release on 2026-10-21, and `NEBIUS_SHA256`, measured:
  nebius publishes no checksum (`…/nebius.sha256` is a 404). Only the binary goes in
  (`/usr/local/bin/nebius`); its installer, which writes `~/.nebius` and the shell rc files, is
  not run. `nebius version` writes nothing to the home.
- Not pinned by hash:
  - rustup-init (`sh.rustup.rs`): rustup checks the 1.99.0 toolchain against its channel manifest;
  - the apt packages and the `ubuntu:24.04` tag;
  - DuckDB's tpch and tpcds extensions, which `INSTALL` fetches from DuckDB's repository at image
    build. DuckDB checks their signature at `LOAD`. In the image built on 2026-10-21 their sha256
    are `a6516e48…` (`tpch.duckdb_extension`) and `3a4eb5c5…` (`tpcds.duckdb_extension`).

   Ubuntu 24.04's gcc-12 is 12.4.0 and its
  gcc-14 is 14.2.0, the versions #260 records for the committed builds.

**The build context** is the repository, narrowed by `docker/build.Dockerfile.dockerignore`
(BuildKit reads `<Dockerfile>.dockerignore` before the context's own): the specs, the cmake wrapper,
`cpp/` without its build outputs, `flatbuffers/` and `scripts/build.sh`. It is the working tree, so
`image.sh` adds the `:cudf-<v>-<commit>` tag only when `git status --porcelain` is empty, and labels
a dirty build `org.opencontainers.image.revision=<commit>-dirty`. `push` needs HEAD's commit tag
locally and the version tag naming the same image ID, so an image built from a dirty tree is never
published under a commit.

**third_party/cudf is not in the context**: it is uninitialised in some worktrees (this one).
The prefetch stage fetches it at the commit the index pins (`CUDF_SUBMODULE`, from
`git ls-files -s`), sparse: `cmake/`, `cpp/cmake/`, `VERSION`, `RAPIDS_BRANCH`.

**gcc per version is a stage** (`cudf-25.02`, `cudf-26.02`), so pins.env passes verbatim and the
rest of the file reads `GCC_VERSION`. The image sets `CC`, `CXX` and `GCC_VERSION`. Two scripts read
them differently:
- `scripts/cargo-cudf.sh` keys gcc on the root's basename; `cudf` is unknown, so it takes
  `GCC_VERSION`. Its target dir is `target-cudf-cudf`.
- `scripts/build.sh` resets its own `GCC_VERSION` and defaults to 14, so in 25.02 it needs
  `--gcc-version "$GCC_VERSION"`. Without it, it fails loudly, since gcc-14 is not installed.

**nvcc.** `/opt/nvcc` holds 21 packages taken line by line from `cudf-26.02.spec`: nvcc-impl,
-tools and -dev, crt, nvvm, cudart (with -dev and -static), cccl, and the `-dev` packages of
cublas, curand, cusparse, cusolver and cufile. So the nvcc bytes are the 26.02 env's, the nvcc
nebius-gpu builds 25.02 with. The Dockerfile counts the lines it selected and checks
`nvcc --version`. `/opt/nvcc/bin` is first on PATH and `CUDACXX` names it.
`/opt/cudf/bin` is not on PATH at all: it holds Python 3.12 with its own pandas, and nvcc 12.2.
- **Why the math headers.** CMake takes the toolkit's include directory from nvcc:
  `/opt/nvcc/targets/x86_64-linux/include`. Without those headers the first image's FFI build
  stopped in `cpp/tests/gpu/test_tpchv.cpp`. The chain is cuVS, then raft, then
  `#include <cublas_v2.h>`. raft and cuVS also include `curand`, `cusolver` and `cusparse`
  headers. The 26.02 env has these `-dev` packages and no `libcufft-dev`. Only the headers are
  taken, not the runtime packages, so their `lib*.so` symlinks dangle in `/opt/nvcc/lib` and
  `targets/x86_64-linux/lib`. No link names them: the math libraries are linked from
  `/opt/cudf` (next point).

**The CUDA mix in the 25.02 image.** In 26.02 there is no mix: `/opt/cudf` is 12.9 too. In 25.02
the library compiles against 12.9 and links mostly against 12.2.
- **Compiled against 12.9.** nvcc 12.9.86 compiles every `.cu` and `.cpp`, so `CUDART_VERSION` is
  12090. The CUDA headers and the math libraries' headers are 12.9 too, all from
  `/opt/nvcc/targets/x86_64-linux/include`, the toolkit directory CMake takes from nvcc.
- **Linked against the 12.2 shared libraries.** Every `CUDA_*_LIBRARY` CMake finds is in
  `/opt/cudf/lib`, so 12.2 here. That is cudart, cudart_static, cublas, cublasLt, curand,
  cusolver, cusparse, cufile, cufile_rdma, culibos and nvptxcompiler_static. `find_library`
  searches `CMAKE_PREFIX_PATH` (`/opt/cudf`, set from `cudf_ROOT`) before the toolkit hints.
  `cpp/build/CMakeCache.txt` and the FFI tree's cache agree.
- **Linked against 12.9 static archives as well.** nvcc adds its own implicit link directory,
  `/opt/nvcc/targets/x86_64-linux/lib`, with `-lcudadevrt -lcudart_static`. So
  `libpeacock_gpu.so` links `/opt/cudf/lib/libcudart.so` (12.2) and also 12.9's
  `libcudadevrt.a` and `libcudart_static.a`. The places to see it:
  - 25.02: `build.ninja`'s link line for `libpeacock_gpu.so` in the configured `cpp/build`;
  - 26.02: the FFI tree's `CMakeFiles/peacock_gpu.dir/link.d`.
- **The risk.** 36 runtime entry points are defined in 12.9's `libcudart_static.a` and absent
  from 12.2's `libcudart.so`, for instance `cudaStreamGetDevice` and
  `cudaGetDriverEntryPointByVersion`. A call to one does not fail to link: it binds to the static
  12.9 runtime, a second runtime inside the library. cutlass's `cuda_host_adapter.hpp:102` switches
  to `cudaGetDriverEntryPointByVersion` when `__CUDACC_VER_MINOR__ >= 5`, which nvcc 12.9
  satisfies.
- **The guard.** `scripts/env/no-static-cudart.sh <lib.so>` fails when a library defines a
  CUDA runtime symbol (`nm -D --defined-only`, a `T` symbol named `cuda…` or `__cuda…`). It
  passes today on both versions' FFI `libpeacock_gpu.so`. Its test is
  `scripts/env/tests/test_no_static_cudart.py`.
- **The driver.** The library carries SASS for sm_80 and sm_90 and PTX for compute_80. A card
  with neither SASS, the L40S (sm_89) among them, JIT-compiles the PTX, and PTX from nvcc 12.9
  needs a driver at CUDA 12.9's minimum. So for the 25.02 image, Task 7's old-driver check must
  use 12.9's minimum, not 12.2's.
- **Open.** This follows the native nebius-gpu build and is not changed here. nebius-gpu sets the
  same prefix path, so its libraries are probably the 12.2 env's too, and its headers come from
  its nvcc wrapper's toolkit. Comparing its `cpp/build/build.ninja` and `CMakeCache.txt` with the
  image's will settle it when the host is next up.

**What a configure fetches, and where it comes from.**
- **rapids-cmake.** `rapids_config.cmake` downloads the zip of a branch at every configure,
  before `project()`. A toolchain file and `CMAKE_PROJECT_INCLUDE` both run too late. No
  environment variable redirects a FetchContent URL. Changing `cpp/CMakeLists.txt` or `build.rs`
  is outside this task's scope. So `/usr/local/bin/cmake` is `docker/cmake-wrapper.sh`: a
  configure whose source tree's `third_party/cudf/RAPIDS_BRANCH` equals the prefetched branch
  gets `-DFETCHCONTENT_SOURCE_DIR_RAPIDS-CMAKE=/opt/prefetch/rapids-cmake`, before the caller's
  arguments, so a later `-D` of the caller's wins.
  - A tree with a `RAPIDS_BRANCH` naming another branch gets the variable set empty. That clears
    a copy an earlier configure cached in the same build tree, and FetchContent downloads again.
    A submodule bump therefore reaches the network instead of building against the wrong
    rapids-cmake.
  - A tree with no `RAPIDS_BRANCH` (flatc-fork's cmake build, say) and every non-configure mode
    are plain cmake.
  - The source is taken from `-S <dir>`, `-S<dir>` or the first positional directory holding a
    `CMakeLists.txt`. The values of `-B -G -T -A -C -D -U` are skipped.
  - `branch-25.10` last moved on 2025-10-08.
  - Tested by `scripts/env/tests/test_cmake_wrapper.py`, which runs a copy of the wrapper against
    a fake cmake.
- **flatbuffers.** A bare mirror holding only the tag `cpp/CMakeLists.txt` names
  (`/opt/prefetch/flatbuffers.git`). Every configure through the wrapper exports one
  `GIT_CONFIG_KEY_<n>`/`VALUE_<n>` pair, appended after any the caller set, which makes the GitHub
  URL's `insteadOf` the mirror. Nothing outside a configure sees the rewrite. A bumped tag fails
  naming the tag.
- Verified at the offline configure: the cache holds the override, git cloned flatbuffers from
  the mirror (`HEAD is now at 7e16302 … 25.12.19`), and CPM added GTest 1.16.0 from
  `/opt/prefetch/cpm`. In the 26.02 image, a tree whose `RAPIDS_BRANCH` reads `branch-99.99` got
  no override, in both the `-S` and the positional form. `cmake -E` passes through.
- An earlier version set the rewrite in the system gitconfig. The first build then failed,
  because the rewrite also applied to the clone that makes the mirror: git cloned the empty
  destination into itself ("Remote branch v25.12.19 not found"). This was reproduced on the host.
  Scoping the rewrite to the wrapper's configures removes that case.
- **CPM** (gtest, and CPM.cmake itself) reads `CPM_SOURCE_CACHE=/opt/prefetch/cpm`. CPM takes a
  file lock inside the cache, so `/opt/prefetch` is writable by any uid. Each container's writes
  stay in its own layer.

**DuckDB extensions** are in `/opt/duckdb-ext`, root-owned, and the image sets
`PEACOCK_DUCKDB_EXTENSION_DIR=/opt/duckdb-ext`. When that variable is set,
`testdata/generate_testdata.sh` writes `SET extension_directory='<dir>';` before its `INSTALL`;
without it, the SQL it feeds DuckDB is unchanged byte for byte
(`scripts/env/tests/test_duckdb_extensions.py`).
- DuckDB 1.5.4 reads no environment variable for its extension directory: the binary's strings
  name only `DUCKDB_EDITOR`, `_FILE`, `_HISTORY`, `_PAGER` and `_JE_MALLOC_CONF`. Its default is
  under the home.
- An earlier version used a `~/.duckdb/extensions` symlink in `peacock`'s home instead. Under
  any other uid HOME is elsewhere (`/` for a uid with no passwd entry), and DuckDB failed to
  create `/.duckdb`.
- `INSTALL tpch` with the extension already installed needs no network.
- A hand-run `duckdb` gets the image's copies with the same `SET`. Without it, it looks under its
  home.

**Python.** The venv `/opt/venv` is `docker/venv.lock` exactly. The lock pins 14 packages, each with
the sha256 of the one wheel pip installs (cp313 manylinux x86_64, or py3).
- **How it is installed.** `pip install --require-hashes --no-deps -r`, so nothing is resolved at
  build time. The build then checks that `pip freeze` equals the lock and that the five pins.env
  versions hold. `scripts/env/tests/test_venv_lock.py` checks that every line is pinned and
  hashed and that the lock agrees with pins.env.
- **What the lock holds.**
  - numpy 2.5.4, pandas 3.0.6, pyarrow 25.0.1, python-dateutil 2.9.0.post0 and six 1.17.0 are
    `pip freeze` of `/build/peacock/venv-exec-model-313`, the venv the committed costs ran in.
    Its `dist-info/WHEEL` tags are the same wheels the lock names.
  - matplotlib 3.11.2 and duckdb 1.5.4, with contourpy 1.4.0, cycler 0.12.1, fonttools 4.67.0,
    kiwisolver 1.5.1, packaging 26.3, pillow 13.0.0 and pyparsing 3.3.3, are what `62b78a38` held
    when it reproduced those costs.
  - There is no tzdata, as in the reference venv.
- **Why the lock.** Before it, pip resolved the dependencies at each build. The rebuilds of
  2026-10-23 (`03b4e3d6`, `e114cb9d`) picked python-dateutil 3.0.0 and a new tzdata 2026.5.
- The venv belongs to root, so nothing at run time moves a pin.

**Claude Code is not in the image**, and neither is the plugin. This is the human's decision of
2026-10-23, in the spec's "The images". The native binary is proprietary and the images are
public. A login belongs to the machine, not to a container. So Claude Code, the
`superpowers@claude-plugins-official` plugin and gh's login live on the machine, installed once
by Task 6's `bootstrap-vm.sh`. The container runs the machine's install, mounted read-only, with
`~/.claude`, `~/.claude.json` and gh's config mounted from the machine.
- pins.env keeps `CLAUDE_CODE_VERSION`, `SUPERPOWERS_VERSION` and `SUPERPOWERS_SHA` for
  bootstrap-vm.sh. image.sh still passes every pins.env line; the Dockerfile declares none of
  these three, and BuildKit is silent about an unused build arg.
  `test_image.py::test_the_dockerfile_declares_every_pin_but_the_machine_s` and
  `test_no_claude_code_is_installed_in_the_image` hold both.
- For bootstrap-vm.sh, what the earlier image found about pinning the plugin:
  - The official marketplace pins each plugin to a commit of its source; for superpowers that is
    `bb92a777`, tag v7.0.0 of obra/superpowers.
  - A marketplace can be added at a branch or tag only: `#<commit>` failed with "Remote branch …
    not found", and the official marketplace has no tags.
  - So the pin can only be checked after the install: `~/.claude/plugins/installed_plugins.json`
    must record version 7.0.0 and that `gitCommitSha`. The image build did this check
    successfully.
  - Claude Code's `install.sh` is not pinned by hash. It checks the binary it downloads against
    the release manifest. It first runs the latest release, and that release installs the pinned
    version (round-1 review).
- gh stays in the image, since agents run it inside. Its login is the machine's mounted config,
  and the image deletes the device id `gh --version` writes at build.

**Any uid.** The container runs as the machine user's uid:gid, whatever it is. Nothing in the
image depends on being `peacock`:
- **HOME.** Nothing is found through HOME. For a uid with no passwd entry, Docker sets HOME to `/`,
  which that uid cannot write. Task 6 sets HOME to the machine user's.
- **Rust.** `RUSTUP_HOME=/opt/rust/rustup` is root's and readable by all. Writable by any uid:
  `CARGO_HOME=/opt/rust/cargo` itself, for cargo's caches and its package lock
  `$CARGO_HOME/.package-cache`, and its `registry/` and `git/`. `bin/` stays root's.
  - For Task 6 (the reviewer's note): the package lock lies outside `registry/`. So a lock per
    container plus the machine's registry mounted `:ro` is safe.
  - A crate missing from that registry fails the build, so bootstrap must fill it with
    `cargo fetch` in the build image.
- **The prefetch cache and the venv.** CPM takes a lock in each package's directory of the cache
  (`/opt/prefetch/cpm/<package>/cmake.lock`), so those directories and lock files are writable by
  any uid. Nothing else under `/opt/prefetch` is: the flatbuffers mirror and the checkouts stay
  root's, read-only. The venv is root's and read-only.
- **gh** is unpacked with `tar --no-same-owner`, so it belongs to root, not to the tarball's
  uid 1001.
- **ccache.** `CCACHE_DIR=/tmp/ccache` unless the launcher mounts a shared one.
- **DuckDB.** See above.
- **git ownership.** git refuses a repository another uid owns unless protected config names it.
  The `GIT_CONFIG_*` pairs the wrapper exports are no help here: git clears them for the process
  that serves a local clone's source. `/etc/gitconfig` therefore lists `safe.directory` for
  `/opt/prefetch/flatbuffers.git` and for the gtest checkout in the CPM cache, those paths only.
  Without them, a configure as uid 4242 fails with "detected dubious ownership".
- **The default user and the working directory.** `peacock` (uid 1000) stays as the default
  user, so the image never runs as root unasked. WORKDIR is `/tmp`: `peacock`'s home is closed
  to other uids.
- **Tested.** `scripts/env/tests/test_image_any_user.py` runs each image as
  `--user 4242:4242 --network none` and checks:
  - the uid, and that HOME is not writable;
  - a cargo build, and that CARGO_HOME is writable;
  - rustc, and the venv's six versions against the pins;
  - raw DuckDB `INSTALL tpch` with `dbgen`, and `generate_testdata.sh --bench tpch --sf 0.01`;
  - a ccache compile, and gh;
  - a configure fetching what `cpp/CMakeLists.txt` fetches by the same routes: rapids-cmake's
    branch zip (it got the prefetched copy), the flatbuffers tag (cloned from the mirror) and
    rapids-cmake's CPM gtest (from the cache).

  It needs the images, so it is not in the cheap tier.

**Left out on purpose.**
- No Node. The nebius CLI is in, as a tool with no config or profile (the spec's "provider CLIs
  as tools with no data in them").
- No global `LD_LIBRARY_PATH`: conda's libraries would shadow the system ones under git, ssh and
  Python.
- No `RUSTFLAGS`, `CMAKE_GENERATOR` or `LDFLAGS`, as on the native hosts. `-rpath-link` is not
  needed. The FFI build and the `--features gpu --tests` link passed without it, because CMake
  writes `-Wl,-rpath,/opt/cudf/lib` into every link line (`build.ninja`), and GNU ld also searches
  `-rpath` directories for a shared library's own dependencies.
- `/root` keeps micromamba's `.mamba/pkgs/urls.txt` and `.conda/environments.txt`. They hold
  package URLs and the two prefixes' paths, nothing else.
- ubuntu:24.04's own `ubuntu` user (uid 1000) is removed, so the default user `peacock` takes uid
  1000.

**`docker build --check`.** It warns that `ARG CUDF` has no default. That is deliberate: a build
without the arg must stop. Line 1 skips that one rule.

## Task 2: what was built and verified

Host: the developer's. Docker data root `/build/docker-data`. All output is under
`/build/peacock/containers/` and the logs under `/build/peacock/tmp/containers/`.

**Images.** Both are tagged `:cudf-<v>` only, because the tree was dirty. Four builds so far:

| image ID | cuDF | size | wall time | what changed |
|---|---|---|---|---|
| `c83f7208` | 25.02 | 10.26 GB | about 260 s + 184 s | the first build, across two attempts (the mirror fix in between) |
| `62b78a38` | 25.02 | 10.26 GB | 420 s | the math-library headers in `/opt/nvcc`: cold from that layer on |
| `ae80842c` | 26.02 | 9.97 GB | 456 s | the first 26.02 build: cold apart from the `ubuntu:24.04` layer |
| `acb5ff44` | 25.02 | 10.35 GB | 255 s | after the review: the wrapper, nebius, labels, gh state. Cold from the wrapper `COPY` on |
| `8f929fc0` | 26.02 | 10.05 GB | 165 s | the same changes for 26.02 |
| `03b4e3d6` | 25.02 | 10.0 GB | 119 s | no Claude Code; any uid. Cold from the prefetch stage's last step on |
| `e114cb9d` | 26.02 | 9.74 GB | 116 s | the same for 26.02 |
| `4a37caf9` | 25.02 | 10.03 GB | 95 s | the venv from `docker/venv.lock`; narrower permissions; gh's owner |
| `518effa3` | 26.02 | 9.74 GB | 96 s | the same for 26.02 |

The last two are the images now on the host. The others are deleted.

- The 25.02 image's first attempt spent about 260 s in the toolchain stage, 215 s of it on the two
  micromamba creates.
- In 25.02, `/opt/cudf` is 6.8 GB and `/opt/nvcc` 264 MB. In 26.02 micromamba hard-links
  `/opt/nvcc` to the same packages as `/opt/cudf`.
- The rest of the image: rust 637 MB, venv 464 MB, cmake 187 MB, Python 147 MB,
  DuckDB extensions 35 MB, prefetch 26 MB.

**Inside the 25.02 image, read back offline.**
- Tools: Python 3.13.0, nvcc 12.9.86, cmake 4.2.3, ninja 1.12.1, DuckDB v1.5.4 `08e34c447b`,
  rustc and cargo 1.99.0, gh 2.102.1, aws-cli 2.38.3, nebius 0.12.293 (from `acb5ff44` on).
  The images up to `acb5ff44`/`8f929fc0` also held Claude Code 2.1.304 and superpowers 7.0.0
  `bb92a777`; the current ones do not.
- `pip freeze`, in `62b78a38` and again, from the lock, in `4a37caf9` and `518effa3`:
  contourpy 1.4.0, cycler 0.12.1, duckdb 1.5.4, fonttools 4.67.0, kiwisolver 1.5.1,
  matplotlib 3.11.2, numpy 2.5.4, packaging 26.3, pandas 3.0.6, pillow 13.0.0, pyarrow 25.0.1,
  pyparsing 3.3.3, python-dateutil 2.9.0.post0, six 1.17.0. `03b4e3d6` and `e114cb9d`, built
  before the lock, had python-dateutil 3.0.0 and tzdata 2026.5 instead.
- Those earlier images' `~/.claude` held no credentials file. Their `~/.claude.json` held a
  `userID` and a `machineID` written at build time, which every container would have shared. That
  is one more reason the install belongs to the machine.

**How Step 4 ran.** One container per step, `docker run --network none --memory=10g
--memory-swap=10g` with `CARGO_BUILD_JOBS=4` and `CMAKE_BUILD_PARALLEL_LEVEL=4`. Mounted:
- the worktree at `/work`;
- the primary checkout's `third_party/cudf` read-only over the empty submodule (at the commit the
  index pins, `43505bb9`);
- a cargo registry under `/build` that `cargo fetch` filled first, with network (385 MB);
- the target dirs, `cpp/build` and the generated sf1 under `/build`.

**Which image each result below came from.**
- On `c83f7208`: 25.02's first sf1 generation, its first rust-only run and its first configure.
- On `62b78a38`: 25.02's FFI build and gpu `--tests` compile, and Step 5.
- On `ae80842c`: 26.02's Step 4.
- On the images now on the host, `4a37caf9` (25.02) and `518effa3` (26.02): every result Task 2
  publishes, in the last section.

**Step 4, the 25.02 image, network off.**
- sf1 tpch and tpcds generated in 48 s together. All 32 parquet files are byte-identical to the
  primary checkout's sf1. `lineitem.parquet` is `91e14da3…`, the sha #260 records.
- `cargo test -p peacockdb-core --features rust-only --lib`: 770 passed, 0 failed, 2 ignored.
  The cold compile took 13 min and the tests 105 s. The first run, without the sf1 tables
  mounted, failed 48 tests, every one on "register the tables: … NotFound".
- `scripts/build.sh --configure --cudf_ROOT /opt/cudf --gcc-version "$GCC_VERSION"`: green.
  It found cudf 25.02.02 and Arrow 19.0.1 from `/opt/cudf`, with GNU 12.4.0 as host compiler.
- `cargo build -p peacockdb-core` (FFI): green. The first image failed after 12.5 min on the
  missing `cublas_v2.h`. After the fix the build finished in 1 min, on the objects the failed run
  left.
- `cargo build -p peacockdb-core --features gpu --tests`: green, 1 min 45 s.

**Step 5, the 25.02 image, network off.**
- `cargo build --release -p peacockdb-dphyp`, then the cheap tier as README.md selects it:
  48 files, 521 passed, 0 failed, 3 min 7 s with the build.
- `run.py --bench tpch --query q3 q5 --mode tp1-single --out <tmp>`: 2 runs, 0 failed.
- q3's and q5's sections are byte-identical to the committed
  `scripts/exec_model/testdata/goldens/tpch/tp1-single.{cpu,optimizer,costs,plandiff}.txt` and
  `.optimizer.md`. That is 10 of 10, compared with `sections()`.

**Step 4, the 26.02 image, network off.**
- sf1 generated in 48 s. All 32 files are identical to 25.02's.
- rust-only lib: 770 passed, 0 failed, 2 ignored.
- configure: cudf 26.02.01, Arrow 21.0.0, GNU 14.2.0, nvcc 12.9.86.
- FFI build: green, 13 min 22 s cold.
- `--features gpu --tests`: green, 1 min 47 s.

**After the review, on the final images, network off.**
- **`acb5ff44` (25.02).** The run started from an empty target dir, an empty `cpp/build` and no
  sf1.
  - sf1 tpch and tpcds generated in 49 s. All 32 files are identical to the primary checkout's.
  - rust-only lib: 770 passed, 0 failed, 2 ignored. The cold compile took 13 min and the tests
    89 s.
  - configure: rapids-cmake came from the copy and gtest from the CPM cache. flatbuffers was
    cloned from the mirror through the rewrite the wrapper exports. No system gitconfig exists.
  - FFI build: green, 12 min 35 s cold. Its `link.d` lists `/opt/cudf/lib/libcudart.so` and also
    `/opt/nvcc/…/libcudadevrt.a` and `libcudart_static.a`.
  - `no-static-cudart.sh` on its `libpeacock_gpu.so`: passes.
  - A build tree configured with `branch-25.10` cached the copy. After a reconfigure with
    `branch-26.02` the cached value is empty.
- **`8f929fc0` (26.02).** configure: cudf 26.02.01, Arrow 21.0.0, GNU 14.2.0, nvcc 12.9.86,
  rapids-cmake from the copy, flatbuffers from the mirror, gtest from the CPM cache.
  `no-static-cudart.sh` passes on the `libpeacock_gpu.so` built on `ae80842c`. The toolchain
  layers are unchanged between the two images.
- **Kept for Task 3.** In `/build/peacock/containers/26.02/keep/` (4.8 GB), built on `ae80842c`:
  - `rust-tests/`: `peacockdb_core-…`, `test_gpu_corpus-…`, `test_node_timing-…` and
    `peacock_gpu_benchmarks-…`;
  - `lib/`: `libpeacock_gpu.so` with the libraries the FFI build bundles beside it.

  The 26.02 cargo targets are otherwise deleted.

**No Claude Code, any uid: `03b4e3d6` (25.02) and `e114cb9d` (26.02), network off.**
- `test_image_any_user.py` passes on both images as uid 4242: nine checks each.
- On 25.02, as the default user, with the mounts of Step 4:
  - sf1 tpch and tpcds generated through `PEACOCK_DUCKDB_EXTENSION_DIR` in 48 s. All 32 files
    are identical to the primary checkout's.
  - rust-only lib: 770 passed, 0 failed, 2 ignored, in 14 min 36 s with a cold compile.
  - The offline configure took 9 s: cudf 25.02.02, Arrow 19.0.1, GNU 12.4.0, nvcc 12.9.86.
    rapids-cmake came from the copy, flatbuffers from the mirror, gtest from the CPM cache.
- The FFI builds, the gpu `--tests` compile, the cheap tier and `run.py` were not rerun on these
  two. The next section reruns them on their successors.

**The images now on the host: `4a37caf9` (25.02) and `518effa3` (26.02), network off.**
- Both venvs' `pip freeze` equals `docker/venv.lock`.
- Writable by other users, outside `/tmp`, are exactly six paths. Under `/opt/rust/cargo`: the
  directory itself, `registry/` and `git/`. Under `/opt/prefetch/cpm`: `cpm/`, `gtest/` and
  `gtest/cmake.lock`. gh belongs to root.
- `test_image_any_user.py` passes on both.
- **Step 5, on 25.02 (`4a37caf9`).**
  - DPhyp release build, then the cheap tier: 48 files, 521 passed, 0 failed, 3 min 5 s.
  - `run.py --bench tpch --query q3 q5 --mode tp1-single`: 2 runs, 0 failed.
  - q3's and q5's sections are byte-identical to the committed ones: 10 of 10.
- **The C++ builds, on 25.02 (`4a37caf9`), from an empty target dir.**
  - FFI build: green, 14 min 14 s cold.
  - `no-static-cudart.sh` on its `libpeacock_gpu.so`: passes.
  - `--features gpu --tests`: green, 2 min.
- **Step 4, on 25.02 (`4a37caf9`), from no sf1, an empty `cpp/build` and an empty target dir.**
  - sf1 tpch and tpcds generated in 48 s. All 32 files are identical to the primary checkout's.
  - rust-only lib: 770 passed, 0 failed, 2 ignored, in 15 min 54 s with a cold compile.
  - The offline configure took 9 s: cudf 25.02.02, Arrow 19.0.1, GNU 12.4.0, nvcc 12.9.86.
    rapids-cmake came from the copy, flatbuffers from the mirror, gtest from the CPM cache.
- **26.02 (`518effa3`).** `test_image_any_user.py` passes. Its FFI, gpu and rust-only results
  stand on `ae80842c`, whose toolchain layers this image keeps.

