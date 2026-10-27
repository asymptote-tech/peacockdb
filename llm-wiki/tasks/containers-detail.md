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
- **The driver.** The library carries SASS for sm_80 and sm_90 and PTX for compute_80 and
  compute_90, PTX ISA 8.8 (read off the fatbin of both staged copies in Task 3). An sm_80 cubin runs
  on every 8.x device, so the L40S (sm_89) runs SASS and JIT-compiles nothing; the A100 and the
  H100/H200 have exact SASS. Only a card of another major (Blackwell, say) JIT-compiles the PTX,
  which needs a driver of CUDA 12.9 or later. The driver each image needs is set by its cudart
  (Task 3).
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


## Task 3: the run image — decisions

State: both images are built from a dirty tree (`7bc901f27790-dirty`), so they have no commit
tag and are not pushed.

**What `/opt/cudf` holds.** `docker/runtime-spec.sh <spec>` prints the spec's header and every
package line verbatim, less:
- compilers and their parts: `cuda-nvcc*`, `cuda-crt*`, `cuda-nvvm*`, `ld_impl_linux-64`;
- `-dev`, `-static` (and their `_linux-64` halves), `-headers`, `-headers-only`, `cuda-cccl*`,
  `cuda-profiler-api`, `nlohmann_json`, `dlpack`;
- Python: `python`, `python_abi` and every package with a `py` build string (cudf, pylibcudf,
  cupy, numba, pyarrow, rmm's Python half …).

25.02 keeps 107 of 167 packages, 26.02 114 of 181. A micromamba explicit install resolves nothing,
so the prefix is exactly that list; the build checks the count and that no `bin/python` exists.
The prefix stage then deletes every `*.a`: runtime packages ship static archives too
(`libcuvs_static.a` alone is 990 MB; 18 files, 1.2 GB in 25.02). Headers inside runtime packages
(`include/`, 90 MB) stay: cuDF's JIT may read them, and nothing here proves it does not.
- **Shared libraries in the dropped packages.** Read off the build image's `conda-meta/*.json`:
  the `-dev` packages carry only unversioned symlinks (`libcudart.so`, `libcufile.so`,
  `libcublas.so` …); `cuda-nvvm-impl` carries `libnvvm.so.4`, which nvcc and numba use; 26.02's
  `ucxx` Python package carries `libucxx_python.so`. The names the kept libraries could `dlopen`
  (strings of libcudf, libkvikio, libcuvs, libnvcomp, librmm, libcufile, libnvJitLink) are
  versioned (`libcufile.so.0`, `libcublas.so.12`, `libnvcomp.so.4` …) or resolve in neither image:
  - `libnvrtc.so` and `libnvJitLink.so` (libcudf): no `cuda-nvrtc-dev` or `libnvjitlink-dev` in
    either spec;
  - `libcufile_rdma.so` (libcufile, beside `libcufile_rdma.so.1`, which is kept): the rdma library
    itself needs `libibverbs.so.1`, `libmlx5.so.1` and `librdmacm.so.1`, in neither image;
  - `libnvvm.so` (25.02's libnvJitLink): the build image has it under `nvvm/lib64/`, on no search
    path.
- **The CUDA runtime** is the prefix's `cuda-cudart`: 12.2.140 in 25.02, 12.9.79 in 26.02, at
  `/opt/cudf/lib/libcudart.so.12`. The driver (`libcuda.so.1`) is the host's, mounted by the NVIDIA
  Container Toolkit under `--gpus all`. Nothing shipped has it as `NEEDED`: libcudf and libnvcomp
  `dlopen` it, which is why `ldd` resolves everything in an image with no driver.

**RUNPATH, measured (drift from the Task 2 notes and the plan).** CMake writes
`-rpath /opt/cudf/lib` into the build tree's `libpeacock_gpu.so` only. The installed copy
(`cpp/install/lib`, what is shipped) has no RUNPATH, and neither have the rust test binaries.
So in the run image everything resolves through `LD_LIBRARY_PATH=/peacock/install/lib:/opt/cudf/lib`,
which `run-tests.sh` gives the binaries alone, as `gpu-host-test.sh` does on nebius-gpu.
`libstdc++.so.6` and `libgcc_s.so.1` come from `install/lib` (the copies the install bundles),
glibc from the image. The prefix stays at `/opt/cudf` for parity with the build image; its own
libraries find each other through `$ORIGIN`.

**The driver minimum, per image:** label `peacockdb.driver-minimum`, from the pins
`DRIVER_MINIMUM_25_02=535.104.05` and `DRIVER_MINIMUM_26_02=575.57.08`, the `GCC_25_02` pattern:
a version stage each, chosen by `FROM run-${CUDF}`.
- **What sets it: the cudart.** Both libraries carry SASS for sm_80 and sm_90 (Task 2's "The
  driver"), and an sm_80 cubin runs on every 8.x device, so the A100, H100, H200 and L40S
  JIT-compile none of our PTX. libcudf carries SASS for 70–120. What remains is the runtime: the
  driver that ships with the CUDA release of the image's cudart.
- **25.02: cudart 12.2.140 is CUDA 12.2 Update 2** (Table 1 of
  `docs.nvidia.com/cuda/archive/12.2.2/cuda-toolkit-release-notes/`, "CUDA 12.2 Update 2
  Component Versions"), whose Table 3 gives >= 535.104.05 on Linux x86_64.
- **26.02: cudart 12.9.79 is CUDA 12.9 Update 1** (Table 1 of
  `docs.nvidia.com/cuda/archive/12.9.1/cuda-toolkit-release-notes/`), whose Table 3 gives
  >= 575.57.08 (12.9 GA: 575.51.03).
- Field evidence for 25.02: shad-gpu runs these nvcc-12.9 binaries against the 25.02 env on
  driver 535.247.01 with the cuda-12.5 compat libraries.
- `test_run_image.py` reads each spec's `cuda-cudart` line and holds the pin to a table of
  cudart → toolkit driver, so a spec whose cudart moves fails until its minimum does.
- **The PTX condition, for Task 7.** A card whose compute capability major is neither 8 nor 9
  (Blackwell, say) has no matching SASS and JIT-compiles our PTX, ISA 8.8 from nvcc 12.9.86. The
  CUDA Compatibility guide's "Minor Version Compatibility" page: applications that compile device
  code to PTX will not work on older drivers. Such a card needs >= 575.51.03 for either image.
  Task 7's check compares `nvidia-smi --query-gpu=driver_version` with the label, and for such a
  card with 575.51.03 as well.

**`/opt/run/run-tests.sh <list|filter|bench>`** (`docker/run-tests.sh`), the entrypoint:
- `list` is `gpu-host-test.sh`'s list: every `bin/peacock_*_tests` but the sf40 pair; every
  `rust-tests/*` with `--nocapture --test-threads=1`, `gpu_tests::` for `peacockdb_core_gpu_lib`,
  `--skip bench_` for `peacock_gpu_benchmarks`. Every binary runs after a failure. A non-zero exit,
  a signal (named: `SIGSEGV`), `Segmentation fault|core dumped` in a log, or 0 tests fails the run.
- `filter` is the list with `PCK_TEST_FILTER` on every rust binary but the lib, as
  `gpu-host-test.sh` does. One deliberate difference: a narrowed binary that runs 0 tests does not
  fail; the filter matching no test in any narrowed binary does (as `build-test-shadgpu.sh`
  judges a filtered run). `gpu-host-test.sh` fails each narrowed binary that matches nothing,
  which makes nearly every filtered run red. The lib still fails on 0 tests.
- `list` refuses a set `PCK_TEST_FILTER` and `filter` an empty one, so the mode is stated, never
  implied by the environment.
- `bench` mirrors `build-test-shadgpu.sh`'s benchmark script: `rust-benchmarks/peacock_gpu_benchmarks`,
  `PEACOCK_RECORD_PATH=<testdata>/calibration/records.tsv`, `PEACOCK_TPCH_VEC_PARAMS`,
  `benchmark-results/` created; red on a failure or on 0 passed. The dataset links of the old path
  are not here: Task 7 decides how the data is mounted.
- **The record is never dropped.** `record.rs` writes its header only into a fresh file, so the
  record must start empty. The old path deletes it at every start; here a record with rows (an
  earlier segment of a long run, or a rerun) is refused, naming it: pull it home, then remove it.
  An empty one is removed.
- Exit 2, one line, before any binary runs: a bad mode, no `lib/libpeacock_gpu.so`, an empty
  C++ or rust list, `filter` with no rust binary but the lib, no benchmark binary, an unpulled
  record, testdata not a writable directory.
- An exit above 128 is named as a signal only when `kill -l` knows the number (`exit 255` reads
  "exited 255").
- The only `LD_LIBRARY_PATH=` is on `run_binary`'s command line, and nothing is exported: with the
  prefix on bash's own path, bash loads the prefix's `libtinfo`. A static test holds it, and another
  that the sf40 pair, the lib's rung and the `--skip bench_` target are `gpu-host-test.sh`'s.
- Logs go to the terminal and to a `mktemp -d` in the container; Task 7 captures stdout. No third
  mount.

**The image.** A `prefix` stage (ubuntu:24.04, curl, micromamba) creates `/opt/cudf`; the
final stage is `run-<cudf>` (ubuntu:24.04 with its driver label) with the prefix copied in, `run-tests.sh`, and the default user
`peacock` (uid 1000), WORKDIR `/tmp`, no global `LD_LIBRARY_PATH`. No apt layer, curl or
micromamba in the final image. The run image's context is narrowed by
`docker/run.Dockerfile.dockerignore` to the specs, `runtime-spec.sh` and `run-tests.sh`. image.sh
passes every pin to both Dockerfiles; `test_image.py` now asks only that some Dockerfile declares
each pin.

## Task 3: what was built and verified

**Inputs, staged in each build image the way `build-test-shadgpu.sh --build` stages them**
(`--network none --cpuset-cpus 0-3 --memory 10g`; worktree at `/work` read-only, the primary
checkout's `third_party/cudf` at `43505bb9`, the cargo target at `/cache/target/cudf`, the
registry and ccache from `/build/peacock/containers/`):
- `scripts/build.sh --configure`, `--build`, `--install` into `cpp/install`: 145 s (25.02), 165 s
  (26.02), from configured build trees.
- 25.02: `test_gpu_corpus`, `test_node_timing`, `peacock_gpu_benchmarks` (`--test`) and the lib
  (`--lib`, staged as `peacockdb_core_gpu_lib`), each `cargo test --no-run --features gpu`, path
  read off cargo's json lines. All four were fresh in Task 2's target: 12 s.
- 26.02: the same four from `26.02/keep/rust-tests/`, renamed to their target names.
- Every staged rust binary `strip --strip-debug`, as `--push-binaries` does: about 110–120 MB each.
- Result: `/build/peacock/containers/<v>/install` (549 MB each), linked as
  `/build/peacock/containers/staged/cudf-<v>`, the `PEACOCK_STAGED_INSTALLS` the image tests read.
  The 25.02 cargo target (16 GB) is deleted.

**Images.**

| image ID | cuDF | size | wall time | note |
|---|---|---|---|---|
| `29b4b139` | 25.02 | 6.27 GB | 242 s | first build, cold prefix; with the static archives; deleted |
| `d7bf7329` | 25.02 | 5.09 GB | 48 s | `*.a` deleted (the create layer cached) |
| `a2806c31` | 26.02 | 6.17 GB | 256 s | cold prefix; deleted |
| `ec9eca70` | 25.02 | 5.09 GB | 373 s | round 1: per-version driver label, N8, the new `run-tests.sh`; cold prefix |
| `1a4cd591` | 26.02 | 6.17 GB | 251 s | the same for 26.02 |
| `8d283e66` | 25.02 | 5.09 GB | 2 s | round 2: the record message; only the script layer rebuilt |
| `f753b6cf` | 26.02 | 6.17 GB | under 1 s | the same for 26.02 |

Every image but the last two is deleted. The two on the host are `8d283e66` (label 535.104.05) and
`f753b6cf` (label 575.57.08).

`/opt/cudf` is 4.7 GB in 25.02 and 5.8 GB in 26.02 (6.8 GB in the 25.02 build image).

**Tests, round 0** (before review). `PEACOCK_STAGED_INSTALLS=/build/peacock/containers/staged`:
- `test_run_image.py`: 9 passed. Both versions: `no-static-cudart.sh` passes on the staged
  `libpeacock_gpu.so`; `ldd` in the run image (`--network none`, the library path
  `run-tests.sh` gives) over `libpeacock_gpu.so`, the six `bin/peacock_*_tests` and the four rust
  binaries finds no `not found`. With `/opt/cudf/lib` taken out of the path the test goes red on
  `libcudf.so`, `libarrow.so.1900` and `libcudart.so.12`.
- `test_run_tests.py`: 18 passed, as uid 4242 with no network, over fake binaries. Each of five
  rules taken out of a copy of the script (zero tests, the segfault grep, the sf40 skip, the
  lib's rung, the fresh record) turns at least one test red.
- `test_image.py` 18, `test_pins.py` 3, and the rest of `scripts/env/tests/` green.
- `shellcheck` is not installed on the developer host; `bash -n` only.

**Open, for Task 7.**
- No device run here: nothing proves the `dlopen`s (`libcuda.so.1`, nvrtc, cufile) succeed under
  `--gpus all`.
- Neither image (nor the build image) has `/usr/share/zoneinfo`; the prefix's `tzdata` is under
  `/opt/cudf/share/zoneinfo`. The native hosts have the system one.
- The PTX condition above, for a card of another major.
- `gpu.sh … pull` must move the record aside on the host after copying it: a copied record still
  has rows, so `bench` refuses the next segment until it is gone.
- For Task 4's `build-test.md` notes: a hand-run `docker run --entrypoint <binary>` needs
  `-e LD_LIBRARY_PATH=/peacock/install/lib:/opt/cudf/lib`, since the shipped binaries carry no
  RUNPATH.

## Task 3: review round 1

- I1: the driver minimum rested on a false premise (the L40S JITs nothing, above). Two pins, each
  image its own, the test reading the spec's cudart. The Task 2 note is corrected.
- I2: `bench` refuses a record with rows instead of deleting it.
- Nits: `filter` with only the lib refused; a refusal test for read-only testdata; an exit above
  128 that is no signal named as an exit; `judge` takes the status and the zero-tests check is a
  call of its own (`require_tests`); the two static tests above; the header at ten lines; the
  `dlopen` names above.
- N8: micromamba's package cache (`/tmp/mamba`, 8.7 GB of BuildKit cache for 26.02) and the `*.a`
  deletion are folded into the create `RUN`, so neither reaches a cached layer.
- The behaviour tests of `test_run_tests.py` now mount the working tree's `docker/run-tests.sh`
  over the image's, so they test the script without a rebuild;
  `test_both_images_carry_the_committed_script` holds the images to it.
- Rebuilt once `/build` had room (table above). Every file in `scripts/env/tests/` is green on
  them: `test_run_image.py` 9, `test_run_tests.py` 24, `test_image.py` 18, `test_pins.py` 3 and
  the other six files.
- **What N8 saved.** The create step's BuildKit record is now the size of the prefix it leaves:
  5.01 GB for 25.02, 6.09 GB for 26.02, the same as the `COPY --from=prefix` layer. Before, 26.02's
  record was 8.71 GB, so 2.6 GB less for 26.02. 25.02's earlier records (the create with the package
  cache and the archives, then a separate `*.a` deletion on top) were pruned before they were
  measured. The image sizes do not change, since the final stage only ever copied `/opt/cudf`.
- Round 2: `test_both_images_carry_the_committed_script` also runs each image's own entrypoint,
  unmounted, as uid 4242 with no network: `--help` exits 0 with the usage, a bare run exits 2 with
  one line. The rung check requires exactly one rung line in `gpu-host-test.sh`. The record refusal
  says "pull it home, then remove it". All ten files green again on `8d283e66` and `f753b6cf`.
- `docker builder prune --filter id=<record>` removes nothing on this Docker (29.1.3), so the
  superseded images' cache records go only with a prune of all unused cache.

## Task 4: publishing

State: everything up to the push. Nothing is pushed, and no package exists on ghcr.io yet.

**The images, rebuilt from the clean tree at `c0c7b9cd`.** `image.sh build` for each kind and
version: every step a cache hit, so only the image config changed (the revision label, and the
commit tag). Each image's layers are those of its predecessor: 15 for a build image, 6 for a run
image, compared by `RootFS.Layers`. The predecessors are deleted.

| image | ID | tags | revision | driver label |
|---|---|---|---|---|
| build 25.02 | `a891af1b594d` | `cudf-25.02`, `cudf-25.02-c0c7b9cdc210` | `c0c7b9cdc210` | — |
| build 26.02 | `a93e558c529c` | `cudf-26.02`, `cudf-26.02-c0c7b9cdc210` | `c0c7b9cdc210` | — |
| run 25.02 | `226ff85bf54d` | `cudf-25.02`, `cudf-25.02-c0c7b9cdc210` | `c0c7b9cdc210` | 535.104.05 |
| run 26.02 | `2299865830b7` | `cudf-26.02`, `cudf-26.02-c0c7b9cdc210` | `c0c7b9cdc210` | 575.57.08 |

`image.sh push <kind> <v> --dry-run` prints, for each, `docker push` of the commit tag and then of
the version tag, and exits 0. Every file in `scripts/env/tests/` is green on these four (the run
image tests with `PEACOCK_STAGED_INSTALLS=/build/peacock/containers/staged`).

**The commit tag must be HEAD's at the push.** `c0c7b9cdc210` is Task 3's commit. Once Task 4 is
committed, `image.sh push` refuses until the images are built again at the new HEAD, from a clean
tree. That rebuild is all cache hits and takes seconds.

**What one build holds on disk at once, measured on the developer host.** BuildKit's records
(`docker buildx du --verbose`), and the download size of each spec's packages (`Content-Length`
of every URL):
- build image: the create step leaves a 7.48 GB layer (25.02) or 7.19 GB (26.02). While it runs,
  micromamba's downloads sit beside it: 2.56 GB (25.02 spec 2.48 GB, nvcc's 21 packages 0.08 GB),
  2.76 GB (26.02). Peak about 10.6 GB for either version; the image is 10.0 or 9.74 GB.
- run image: the prefix stage's layer is 5.01 GB (25.02) or 6.09 GB (26.02), and the final stage
  copies it, so the build holds it twice: 10.0 GB and 12.2 GB. The create step's own peak is lower
  (25.02: 5.01 GB, plus 1.2 GB of archives and 2.23 GB of downloads, both deleted in the step).
- So the largest is the 26.02 run image, 12.2 GB.
- Not measured: a runner whose Docker keeps images in the containerd store also keeps the
  compressed blobs. This host uses overlay2, where a push compresses as it streams.

**The runner-disk finding.** GitHub documents 14 GB of disk for a standard Linux runner, and the
OS and the preinstalled tools share it with Docker. 12.2 GB does not fit beside them with any
margin, and the build images' 10.6 GB barely. So every job frees the disk first: it removes the
preinstalled Android SDK, .NET, GHC, ghcup and CodeQL, and prunes the preinstalled images. Then it
measures what is free under Docker's data root and stops with an error below 16 GB. The step
prints `df -h /` before freeing, so the first run records what a runner really has. If a run stops
there, the images that did not fit are pushed by `image.sh push` from a host, and the workflow
comment and build-test.md must then say which.

**images.yml.**
- Dispatch only. Inputs `kind` (build, run, all) and `cudf` (25.02, 26.02, all), both `choice`.
- One job per image, from a matrix over the inputs: `all` expands to the other choices through
  `fromJSON`, in the matrix itself, so no planning job is needed. `fail-fast: false`.
- `permissions: contents: read, packages: write` at the top; no job widens it. The token is
  `github.token`; no secret is named. `docker login ghcr.io` reads it from stdin.
- The checkout takes no submodules: `image.sh` reads `third_party/cudf`'s commit from the index.
- `concurrency: images`, queued, since two runs would push the same tags. `timeout-minutes: 120`.
- `actions/checkout@v4`, pinned as every other workflow here pins it.
- If the job's push is denied, the package (first pushed from a host) needs this repository given
  the Write role under its "Manage Actions access".
- GitHub can dispatch it only once master holds it. A dispatch then runs the workflow and the
  commit of the ref it names (`--ref`, master by default), and moves `:cudf-<v>`.

**The test**, `scripts/env/tests/test_images_workflow.py`, 11 cases.
- The build image has no YAML parser, and the tests run there too, so the test carries a reader
  for the block subset the workflows use. It fails on any line it does not read. On images.yml it
  agrees with PyYAML except that it keeps `false` as text.
- Red first: 10 failed on the missing file; the reader's own case passed.
- The disk gate is run, not read: the step's script under fake `sudo`, `docker` and `df`. Short
  by 1 GB, it fails naming both numbers; at the threshold it passes, and the freeing comes before
  the measurement.
- Each of these mutations of images.yml turned one case red: a `push` trigger, `packages: read`,
  `need_gb=12`, the gate's `exit 1` removed, `all` expanding to one kind, `actions/checkout@main`,
  the `rm -rf` removed, the prune removed, `docker build` in place of `image.sh build`.
- Green locally (11 passed), and in the 25.02 build image as uid 4242 with no network.
- `test_ci_coverage`, which reads every workflow, is green with images.yml in place: 11 passed
  (`--features rust-only`, target under `/build`).

**build-test.md** gains "Images" and an `images.yml` line under CI structure. Its counts had never
held `scripts/env/tests/` (100 cases in 11 files after round 1) or `scripts/gpu-host/tests/` (104 in
6 files, by the files' own harness, which the cost-report job runs). Both are rows now: Python 1246,
the total 5888, the header the sum of the rows.

**What remains for the push** (Task 4 Step 1, the human's token):
1. commit Task 4, then rebuild the four images at that HEAD from a clean tree;
2. `gh auth refresh -s write:packages`, `gh auth token | docker login ghcr.io -u <user> --password-stdin`;
3. `scripts/env/image.sh push <kind> <v>` for the four;
4. set the two packages public; give this repository Write under their Actions access if a
   workflow push is later denied;
5. on a host with no ghcr.io credentials, `docker pull` each of the eight tags.
- **ghcr.io's limits, for the push.** ghcr.io caps a layer at 10 GB and times out a layer's upload,
  and each layer goes up in one upload. The largest is the build image's conda layer, 7.48 GB
  uncompressed (25.02); it is pushed compressed, and on a slow link it is the one at risk. A push
  that dies there is rerun: `docker push` skips the layers the registry already holds.

## Task 4: review round 1

- **Dispatch wording**: images.yml, build-test.md (CI structure, "Images") now say GitHub can
  dispatch it only once master holds it, and a dispatch runs the ref it names and moves
  `:cudf-<v>`.
- **The step shapes are pinned.** The job's keys are exactly `name runs-on timeout-minutes strategy
  defaults env steps`; the steps are checkout (`uses`, `with`), "Free the disk" and "Build"
  (`name`, `run`), "Push" (`name`, `env`, `run`), so no `if` or `continue-on-error` anywhere.
  Push's whole `run` is pinned, `--password-stdin` included. No `run` may match
  `\bdocker\s+(image\s+)?(build|push)\b|\bbuildx\b`.
- Red on each mutation: `docker image push …` added to "Free the disk"; `docker login -p
  "$GH_TOKEN"`; `continue-on-error: true` and `if: false` on "Free the disk"; a job-level
  `if: false`; `persist-credentials: true`; `need_gb=13`; the GiB arithmetic; the driver print
  removed. The earlier set (a `push` trigger, `docker build`, `exit 1` removed, the prune removed)
  is still red.
- **filter** in build-test.md: every rust binary but `peacockdb_core_gpu_lib` narrowed, the C++
  suites and the lib's device rung whole, red when the filter matches nothing in the narrowed
  binaries (`run_list` in `docker/run-tests.sh`).
- **Pointers**: images.yml, the test and both Dockerfiles point at build-test.md "Images", which
  now holds the disk basis; the detail link is gone. Two older pointers into this file remain,
  `scripts/env/no-static-cudart.sh:5` ("the CUDA mix") and `scripts/env/tests/test_run_image.py:7`
  (Task 3): their content is not in build-test.md.
- **Counts**: the GPU host scripts row (104, cost-report) and the image row (100); the header
  names the image tests as run by hand.
- Nits: "a card newer than compute capability 9.x"; the reader refuses a tab in the indent and a
  value starting with `{ & * ! > | % @` or a backquote, and a case feeds it eight bad inputs;
  `need_gb` and the measured sizes are both GB of 10^9 bytes (`df -B1`, divided by 10^9), with a
  3 GB margin pinned (`MARGIN_GB`); the step prints docker's storage driver; the checkout has
  `persist-credentials: false`; "Manage Actions access" is conditional on a denied push.
- The workflow test is 15 cases now. The Dockerfile changes are comments, so the images'
  layers do not move.

## Task 4: published

- Pushed from the developer's host with `image.sh push`, f75cba7e's images; both tags of each
  image are one digest:
  - `peacockdb-run:cudf-25.02` `sha256:885cd51eec3924469df44bcb9f74c6a54d0c9bf2099d48d73502ac0bbde29f27`
  - `peacockdb-run:cudf-26.02` `sha256:4f446175bfba3743e7f67bad5d00d284d6dc902766dde225cd485c9d481a196b`
  - `peacockdb-build:cudf-25.02` `sha256:df5179809f69c91f587a098be5f092e2b182038bf7eb2f85dcab0b268dd9e28d`
  - `peacockdb-build:cudf-26.02` `sha256:5bc62eec994ed85853b7c4120bb7e3f18316b233e3f08f039b0c0abde0a82e09`
- A push creating a package in the organization is denied (`permission_denied: create_package`)
  to an outside collaborator, whatever the token's scopes: the pusher is an organization member.
  A package is created private; making it public needs the organization's package creation
  setting to allow public packages, and is done in the package's settings (no REST endpoint).
- With an empty `DOCKER_CONFIG`, all eight tags pull and resolve to the digests above, and each
  package serves a layer blob to an anonymous token (HTTP 200).

## Task 5: embeddings from S3 (steps 1 and 2's code; nothing uploaded)

- One pin, `testdata/embeddings.sha256` (sha256sum format, names relative to the cache): the four
  inline SHAs moved there, and both sources read it. The origin path's base check is still the
  sf1 prefix gate over `deep_base.sf1.fbin`'s pin; `--from-s3` needs a whole-file pin for
  `deep_base.sf<SF>.fbin`, so it serves sf1 only until another slice is pinned.
- The bucket is one value, `EMBEDDINGS_BUCKET=embeddings` in `scripts/lib/embeddings-bucket.sh`
  (sourced by both scripts, as `calibration-bucket.sh` is), keys `<sha256>/<file>`. Not yet
  confirmed by the human, and the bucket does not exist yet. The alternative is the existing
  `calibration` bucket with an `embeddings/` key prefix, a one-line change to `embeddings_key`.
- The guard: CI and GITHUB_ACTIONS refuse the origins only; PEACOCK_NO_FETCH ("no fetch")
  refuses both sources. ssh and docker bootstraps do not carry CI/GITHUB_ACTIONS anyway, so what
  the exemption admits is a CI job reading our own pinned bucket.
- Origin byte-identity: the old and new script run over a copy of the real sf1 cache, with a
  curl that fails loudly. Corrected in round 2, which compared HEAD's script with round 2's:
  all cached, a bad flag and a damaged query file (the refetch attempt and its exit status)
  print identical output. `CI=1` differs by design since round 1, in the refusal's third line.
  GloVe extraction (the text removed, the zip cached) was not compared before round 2; it
  extracts the same bytes, sha256 95dde4df…, and differs only in the text's mtime: `unzip -o -d`
  restored the archive's (2014), `unzip -p` and a move leave the time of extraction.
- Red on each mutation (19): the key without the sha, a call without the endpoint, the download
  not verified, the refused file moved into place, a present file skipped unverified, the cache
  never skipped, the sf1 pin used for any SF, the guard over `--from-s3`, the guard without
  PEACOCK_NO_FETCH; for the upload: the key, the endpoint, no skip of a present object,
  `--cache` ignored, no sha check, the check merged into the upload loop, the presence check
  dropped, a stray argument accepted, no `--help`; a line dropped from the pin.
- Step 2's upload and the pin's commit, and step 3, are open.

## Task 5: review round 1

- Run through a symlink: the lib and the pin are found from `realpath "${BASH_SOURCE[0]}"`, the
  cache stays beside `$0` (how `/build/peacock/embeddings` was made). Tests through a link in
  another directory, for both sources.
- The origin path past the guard, tested: a full cache whose base slice is a sparse file of the
  real 307200008 bytes (the fixture pins its sha) fetches nothing, four `cached+verified` lines;
  a missing query set is fetched from `…/DEEP/query.public.10K.fbin` with `-C -` and `-o` into
  the cache, no aws call.
- PEACOCK_NO_FETCH refuses `--from-s3` too, before any call; CI and GITHUB_ACTIONS do not. The
  reason is corrected here, in the script and in build-test.md; build-test.md's sidecar row and
  `scripts/exec_model/README.md` no longer say no CI host may fetch the cache.
- An SF the pin does not name: `--from-s3 serves only the scale factors <pin> names, not SF=N;
  without --from-s3, SF=N is fetched from the origins.`
- GloVe is extracted by `unzip -p` into `glove.6B.100d.txt.partial` and moved into place; a
  failed unzip removes the partial. Tested with an unzip stub that writes part of the member and
  fails.
- The top comment and the guard comment are within the ten-line cap; what build-test.md already
  says left them. The upload and the bucket comment say an object is skipped at the file's size.
- The test docstring names the ~2.5 GiB the free-space check needs in the temp directory.
- 20 tests. Red on each new mutation: the S3 branch always taken in `fetch` and for the base
  (`|| true`), the sf1 prefix gate pinned from the query file, the query fetched without `-C -`,
  the lib and pin found from `$0`, the cache beside the real script, CI or GITHUB_ACTIONS
  ignored, CI refusing `--from-s3`, PEACOCK_NO_FETCH refusing the origins only, the other-SF
  message reverted to the broken-pin one, unzip straight into place, a failed unzip leaving its
  partial.
- Open: the bucket choice and the plan's Interfaces wording (with the human), the upload, the
  pin's commit, step 3.

## Task 5: review round 2

- build-test.md's cache bullet says what the guard is: by hand only from the origins; CI,
  GITHUB_ACTIONS and PEACOCK_NO_FETCH refuse it; the check is on the environment, so a manual
  run on shad-gpu passes and automation that shells in sets PEACOCK_NO_FETCH=1.
- Not corrected everywhere: `.github/workflows/pipeline.yml`'s comment on `test_stats_embeddings.py`
  still says "which no CI host may fetch". It is left as is, because the spec's restriction keeps
  `pipeline.yml` unchanged and the human agreed to leave it.
- The byte-identity record above is corrected (`CI=1`, GloVe extraction).
- The unzip test now interrupts: the fake writes part of the member and SIGKILLs the shell
  running the script, so no handler runs; the final text must not exist (a `.partial` may), and a
  re-run with a full extraction gives the expected bytes. Red on: unzip into place with an
  `|| rm` handler, the old `unzip -o -d`, the move dropped, unzip straight into place. The
  `|| { rm -f …partial; … }` handler is gone: a failed unzip stops the script by `set -e`, and
  the partial it leaves, as a kill does, is never accepted and is overwritten by the next run.
- `testdata/tpch-vec-queries/NOTICE`'s last sentence (the human's wording): not in git; a copy
  verified against the pin is in the project's private bucket, fetched by `--from-s3`.
- Still 20 tests; the counts do not move.
