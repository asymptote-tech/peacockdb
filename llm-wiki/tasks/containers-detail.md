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
- Decided for Task 11's run: the rowgroup modes check correctness and are not benchmarked, so a
  run times the other three; tpcds sf40 comes from the bucket `tpcds-sf40` ("tpcds sf40 in its
  bucket").


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
  sha256 of the CLI zip the committed data came from.
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
    `pip freeze` of the venv the committed costs ran in.
    Its `dist-info/WHEEL` tags are the same wheels the lock names.
  - matplotlib 3.11.2 and duckdb 1.5.4, with contourpy 1.4.0, cycler 0.12.1, fonttools 4.67.0,
    kiwisolver 1.5.1, packaging 26.3, pillow 13.0.0 and pyparsing 3.3.3, are what `62b78a38` held
    when it reproduced those costs.
  - There is no tzdata, as in the reference venv.
- **Why the lock.** Before it, pip resolved the dependencies at each build. The rebuilds of
  2026-10-23 (`03b4e3d6`, `e114cb9d`) picked python-dateutil 3.0.0 and a new tzdata 2026.5.
- The venv belongs to root, so nothing at run time moves a pin.

**Claude Code is not in the image**, and neither is the plugin. The spec's "The images" says
so. The native binary is proprietary and the images are
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
    version.
- gh stays in the image, since agents run it inside. Its login is the machine's mounted config,
  and the image deletes the device id `gh --version` writes at build.

**Any uid.** The container runs as the machine user's uid:gid, whatever it is. Nothing in the
image depends on being `peacock`:
- **HOME.** Nothing is found through HOME. For a uid with no passwd entry, Docker sets HOME to `/`,
  which that uid cannot write. Task 6 sets HOME to the machine user's.
- **Rust.** `RUSTUP_HOME=/opt/rust/rustup` is root's and readable by all. Writable by any uid:
  `CARGO_HOME=/opt/rust/cargo` itself, for cargo's caches and its package lock
  `$CARGO_HOME/.package-cache`, and its `registry/` and `git/`. `bin/` stays root's.
  - For Task 6: the package lock lies outside `registry/`. So a lock per
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

Host: the developer's, building under `/build` as build-test.md says, the Docker data root there too.

**Images.** Both are tagged `:cudf-<v>` only, because the tree was dirty. Four builds so far:

| image ID | cuDF | size | wall time | what changed |
|---|---|---|---|---|
| `c83f7208` | 25.02 | 10.26 GB | about 260 s + 184 s | the first build, across two attempts (the mirror fix in between) |
| `62b78a38` | 25.02 | 10.26 GB | 420 s | the math-library headers in `/opt/nvcc`: cold from that layer on |
| `ae80842c` | 26.02 | 9.97 GB | 456 s | the first 26.02 build: cold apart from the `ubuntu:24.04` layer |
| `acb5ff44` | 25.02 | 10.35 GB | 255 s | the wrapper, nebius, labels, gh state. Cold from the wrapper `COPY` on |
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

**On the final images, network off.**
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
- **Kept for Task 3**, on the build host (4.8 GB), built on `ae80842c`:
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
registry and ccache on the build host):
- `scripts/build.sh --configure`, `--build`, `--install` into `cpp/install`: 145 s (25.02), 165 s
  (26.02), from configured build trees.
- 25.02: `test_gpu_corpus`, `test_node_timing`, `peacock_gpu_benchmarks` (`--test`) and the lib
  (`--lib`, staged as `peacockdb_core_gpu_lib`), each `cargo test --no-run --features gpu`, path
  read off cargo's json lines. All four were fresh in Task 2's target: 12 s.
- 26.02: the same four from `26.02/keep/rust-tests/`, renamed to their target names.
- Every staged rust binary `strip --strip-debug`, as `--push-binaries` does: about 110–120 MB each.
- Result: a staged install per version (549 MB each), in a directory holding `cudf-<v>/`, the
  `PEACOCK_STAGED_INSTALLS` the image tests read.
  The 25.02 cargo target (16 GB) is deleted.

**Images.**

| image ID | cuDF | size | wall time | note |
|---|---|---|---|---|
| `29b4b139` | 25.02 | 6.27 GB | 242 s | first build, cold prefix; with the static archives; deleted |
| `d7bf7329` | 25.02 | 5.09 GB | 48 s | `*.a` deleted (the create layer cached) |
| `a2806c31` | 26.02 | 6.17 GB | 256 s | cold prefix; deleted |
| `ec9eca70` | 25.02 | 5.09 GB | 373 s | per-version driver label, N8, the new `run-tests.sh`; cold prefix |
| `1a4cd591` | 26.02 | 6.17 GB | 251 s | the same for 26.02 |
| `8d283e66` | 25.02 | 5.09 GB | 2 s | the record message; only the script layer rebuilt |
| `f753b6cf` | 26.02 | 6.17 GB | under 1 s | the same for 26.02 |

Every image but the last two is deleted. The two on the host are `8d283e66` (label 535.104.05) and
`f753b6cf` (label 575.57.08).

`/opt/cudf` is 4.7 GB in 25.02 and 5.8 GB in 26.02 (6.8 GB in the 25.02 build image).

**Tests on the first images**, `PEACOCK_STAGED_INSTALLS` naming the staged installs:
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

## Task 3: the driver minimum, the record refusal, the checks

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
- `test_both_images_carry_the_committed_script` also runs each image's own entrypoint,
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
image tests with `PEACOCK_STAGED_INSTALLS` naming the staged installs).

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

**The runner's disk.** GitHub documents 14 GB of disk for a standard Linux runner, and the
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
held `scripts/env/tests/` (100 cases in 11 files then) or `scripts/gpu-host/tests/` (104 in
6 files, by the files' own harness, which the cost-report job runs). Both are rows now: Python 1246,
the total 5888, the header the sum of the rows.

**What remains for the push** (Task 4 Step 1, with a token holding `write:packages`):
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

## Task 4: the dispatch and the step shapes

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
  (sourced by both scripts, as `calibration-bucket.sh` is), keys `<sha256>/<file>`. The bucket
  does not exist yet. The alternative is the existing
  `calibration` bucket with an `embeddings/` key prefix, a one-line change to `embeddings_key`.
- The guard: CI and GITHUB_ACTIONS refuse the origins only; PEACOCK_NO_FETCH ("no fetch")
  refuses both sources. ssh and docker bootstraps do not carry CI/GITHUB_ACTIONS anyway, so what
  the exemption admits is a CI job reading our own pinned bucket.
- Origin byte-identity: the old and new script run over a copy of the real sf1 cache, with a
  curl that fails loudly. HEAD's script compared with the new one:
  all cached, a bad flag and a damaged query file (the refetch attempt and its exit status)
  print identical output. `CI=1` differs by design, in the refusal's third line.
  GloVe extraction (the text removed, the zip cached)
  extracts the same bytes, sha256 95dde4df…, and differs only in the text's mtime: `unzip -o -d`
  restored the archive's (2014), `unzip -p` and a move leave the time of extraction.
- Red on each mutation (19): the key without the sha, a call without the endpoint, the download
  not verified, the refused file moved into place, a present file skipped unverified, the cache
  never skipped, the sf1 pin used for any SF, the guard over `--from-s3`, the guard without
  PEACOCK_NO_FETCH; for the upload: the key, the endpoint, no skip of a present object,
  `--cache` ignored, no sha check, the check merged into the upload loop, the presence check
  dropped, a stray argument accepted, no `--help`; a line dropped from the pin.
- Step 2's upload and the pin's commit, and step 3, are open.

## Task 5: the symlinked script and the origin path

- Run through a symlink: the lib and the pin are found from `realpath "${BASH_SOURCE[0]}"`, the
  cache stays beside `$0`, as a cache reached through a link is made. Tests through a link in
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
- Open: the bucket choice and the plan's Interfaces wording, the upload, the
  pin's commit, step 3.

## Task 5: the guard's wording and pipeline.yml's comment

- build-test.md's cache bullet says what the guard is: by hand only from the origins; CI,
  GITHUB_ACTIONS and PEACOCK_NO_FETCH refuse it; the check is on the environment, so a manual
  run on shad-gpu passes and automation that shells in sets PEACOCK_NO_FETCH=1.
- Not corrected everywhere: `.github/workflows/pipeline.yml`'s comment on `test_stats_embeddings.py`
  still says "which no CI host may fetch". It is left as is, because the spec's restriction keeps
  `pipeline.yml` unchanged.
- The byte-identity record above is corrected (`CI=1`, GloVe extraction).
- The unzip test now interrupts: the fake writes part of the member and SIGKILLs the shell
  running the script, so no handler runs; the final text must not exist (a `.partial` may), and a
  re-run with a full extraction gives the expected bytes. Red on: unzip into place with an
  `|| rm` handler, the old `unzip -o -d`, the move dropped, unzip straight into place. The
  `|| { rm -f …partial; … }` handler is gone: a failed unzip stops the script by `set -e`, and
  the partial it leaves, as a kill does, is never accepted and is overwritten by the next run.
- `testdata/tpch-vec-queries/NOTICE`'s last sentence: not in git; a copy
  verified against the pin is in the project's private bucket, fetched by `--from-s3`.
- Still 20 tests; the counts do not move.

## Task 6: the launcher (steps 1 and 2's code; no VM yet)

- **No named volume for target or HOME.** A new named volume mounted where the image has no
  directory is root's 0755, so the machine uid cannot write it. The worktree's own `target/` is
  the per-instance target, and the per-instance home is a machine directory,
  `~/peacock-instances/.home/<i>`, bind-mounted at `$HOME`. The same rule is why `up.sh` creates
  every mount point as the user first: docker makes a missing one as root inside the enclosing
  bind, on the machine. `testdata/tpch.sf1` in the developer's worktree is such a root-owned
  directory, left by an earlier `docker run`.
- **Mounts are `--mount type=bind`, never `-v`.** `-v` creates a missing source as root; `--mount`
  refuses it, and the fake docker in `tests/machine.py` refuses it too.
- **The image's PATH is read from the image** (`docker image inspect … .Config.Env`), and
  `~/.local/bin` is put in front, so `claude` is found without spelling the image's PATH twice.
- **`scripts/lib/machine.sh`** holds what `bootstrap-vm.sh` and `up.sh` share. Its step functions
  are called as plain statements: set -e is off inside `$(…)` and before `||`, so a function
  called there runs on past a failed step. Results come back in globals (`DATA_SET`).
- **Trust.** An instance writes the clone's `.git`, so every git call on the machine
  goes through `machine_git` (`-c core.hooksPath=/dev/null -c core.fsmonitor=false`); bootstrap
  has a copy, `safe_git`, for the calls before the clone exists. `up.sh` also refuses a clone with
  a hook or either key in `.git/config`. The `-c` is the guard: it also covers an `include.path`
  or a global `core.hooksPath`, which the refusal does not look at. `test_up_never_runs_the_machine_s_git_hooks`
  catches `git worktree add` running `reference-transaction`; no fsmonitor call was seen on that path.
- **The data set** is generated from a copy of the worktree's `generate_testdata.sh` in
  `<hash>.partial/`: the generator writes beside itself, so the copy's directory is the set's.
  The stage is renamed to `<hash>/` only when both datasets are there. One `docker run` a
  dataset, `--network none`, `PEACOCK_NO_FETCH=1`, under `flock` on `.<hash>.lock`. `cargo fetch`
  takes `cargo-registry.lock` beside the registry.
- **Keys.** `--env` is required for every start. `up.sh` greps `^NAME=` (present once) and
  `^NAME=.` (not empty); it never captures a key's value. It reads two values, both paths and both
  required absolute: `GPU_SSH_KEY_FILE` and `PROVIDER_CONFIG_DIR`, the generic name for "a
  provider's config". It cannot tell a read key from a write key.
- **Logins as the container sees them.** gh counts only with `~/.config/gh/hosts.yml` holding
  an `oauth_token`, every one a fine-grained PAT (`github_pat_`), and with `gh auth status` passing
  under `env -u GH_TOKEN -u GITHUB_TOKEN`. A keyring login writes hosts.yml without a token. The
  checks count lines with `grep -c`, so no token reaches the output. The token goes in with
  `gh auth login --with-token --insecure-storage`; without the flag gh writes it to a desktop
  keyring where there is one, and `up.sh` refuses that. The flag is in gh 2.102.1 (`GH_VERSION`),
  checked in the build image. gh's help favours `GH_TOKEN` over `--with-token` for fine-grained
  tokens; the container never sees `GH_TOKEN`, so the token lives in hosts.yml. git needs the
  `/usr/local/bin/gh auth git-credential` helper for github.com in `~/.gitconfig`.
- **passwd and group** per instance, in `~/peacock-instances/.etc/<i>/`, not in the instance's
  home: the home is writable from inside, and a passwd the instance could rewrite is not wanted
  even when it gains nothing. The image's files come from `docker run … cat`; entries with the
  machine uid or name are dropped and the machine user's line is added with `$HOME`.
- **third_party/cudf.** One checkout a commit, `<data dir>/cudf/<commit>/`, fetched by
  `machine_cudf_source` (bootstrap for the clone, `up.sh` for a worktree) from `.gitmodules`' URL,
  sparse with the prefetch's paths, under a lock, renamed from `.partial` when whole. Each
  instance mounts the one its gitlink names, read-only. A single checkout in the clone
  would have moved under every running instance when one needed another commit. The
  clone's own `third_party/cudf` is never read, so a human's `git submodule update --init` (a
  gitfile into `.git/modules`) cannot confuse it. Step 3 must show the sparse paths are enough
  for the FFI build, not only for a configure.
- **S3.** `s3.sh`'s `s3_args <array>` needs both variables. `calibration-bucket.sh` keeps the
  old endpoint and region whenever `AWS_ENDPOINT_URL` is unset; an endpoint without a region is
  refused. The embeddings scripts call `s3_args` only on the S3 path, so the origins need no
  storage named.
- **Image inputs.** The label is one line of `<input>=<value>` joined by spaces. Files: what the
  Dockerfile reads, the 25.02 build image also taking `cudf-26.02.spec` for nvcc. Pins: one hash
  of the `pins.env` lines whose names the Dockerfile declares as `ARG`, sorted under `LC_ALL=C`.
  The locale was a real bug: `sort` orders `GCC_…`, `GH_…` and `G_…` differently under `C` and
  `en_US.UTF-8`, so a label made on one host would refuse a VM in another locale. Its test picks
  an installed locale that orders sample names otherwise than C and prints a SKIP line when none
  is installed. A pin is left out of a version's label only where the Dockerfile declares it
  inside another version's stage (`ARG GCC_26_02` under `FROM … AS cudf-26.02`); the versions are
  the names of `docker/conda/cudf-*.spec`. So `ARROW_19_0` stays in every label, and a 26.02 pin
  that a shared stage reads stays in the 25.02 label too. The flatbuffers tag is
  read from the `FetchContent_Declare(flatbuffers …)` block; more than one is refused.
  `test_the_label_holds_every_docker_file_its_dockerfile_reads` resolves each `COPY docker/…` to the
  files the Dockerfile names by their copied path, `${CUDF}` substituted, for both versions. It
  holds the label to what the Dockerfiles name today; a file read under another path, or through
  a variable other than `${CUDF}`, would escape it. `scripts/build.sh`, copied by the prefetch
  stage, is not an input, as the plan lists.
- **bootstrap-vm.sh** takes `--ref`, since this branch is not on master; a rerun passes it when
  the clone is on that branch. One `apt-get update` precedes the one `apt-get install` of
  whatever is missing, Docker included. With Docker just installed, the group is not active in
  the running shell, so its docker steps go through `sudo docker` as `--user <uid>:<gid>`.
  `claude plugin` has no way to install at a commit (`marketplace add` takes a repo, URL or path;
  `install` no ref), so the pin is checked in `installed_plugins.json` afterwards, and the error
  says to bump the pins. Its tests stop at the first download.
- **For step 3, what the fakes cannot show:**
  - the images must be republished before anything starts: none has the label, so `up.sh`
    refuses them;
  - `--init` needs `docker-init`, which Ubuntu's `docker.io` must ship;
  - Claude Code under a uid the image does not know (`os.userInfo`), now with the passwd entry;
  - `.claude.json`'s location if `/login` ran before `CLAUDE_CONFIG_DIR` was set; no onboarding
    question with only `CLAUDE_CONFIG_DIR` mounted;
  - `claude` through the read-only `~/.local/bin/claude` bind;
  - the plugin install with no login, and that it installs the pinned version at all;
  - ssh to a GPU host and `whoami` inside, as a uid other than 1000 too;
  - the gh credential helper `/usr/local/bin/gh` resolving in the container, and a real commit
    and push from inside landing with the machine's identity;
  - the FFI tier built once from the sparse `third_party/cudf`;
  - `cargo build` touching nothing in the read-only registry; a crate added mid-chain needs
    `down.sh` and `up.sh`;
  - two concurrent builds with no `--memory` or CPU bounds on 8 cores and 32 GB: an OOM can hit
    the neighbour;
  - the data generation under `--network none` with the `.partial` stage, and `cargo fetch
    --locked` with the worktree read-only;
  - gh accepts the fine-grained PAT through `--with-token --insecure-storage`; its help warns of
    "confusing behaviour" with fine-grained tokens there;
  - under the fine-grained PAT, the coordinator's `gh pr create`, `gh workflow run` and
    `gh api -X PATCH …/pulls/<n>` work, and the organisation allows fine-grained PATs;
  - whether the storage honours `If-None-Match: *` on PUT (412 for a writer, 403 for a reader):
    only then could `up.sh` test a key for write access without writing.

## Task 6a: one command per machine, one per chain (steps 1 and 2's code; no VM yet)

- **The developer's user has passwordless sudo** (`/etc/sudoers.d/peacock-<user>`). Bootstrap runs
  as that user and installs with sudo; the docker group is root already, so the grant widens
  nothing. The root step skips the keys and the sudoers file when `--user` is the login user.
- **Groups before the first login.** The root step puts the user in `docker` and `peacock` before
  any ssh as that user, so every later session has both active. A user who runs `bootstrap-vm.sh`
  by hand joins `peacock` only at the next login; the data directory it makes is its own (owner,
  group `peacock`, 2775), so its writes in that session do not need the group.
- **Shared writes.** `machine_cudf_source`, `machine_data_set` and up.sh's chain lock set umask
  002 for what they create; the generator runs as `bash -c 'umask 002 && exec "$@"'` in the build
  image. The container's `--user uid:gid` carries no supplementary group, so a second user's
  container reads the first user's set through the other-bits, which umask 002 leaves readable.
  The cuDF checkout is mounted into another uid's container, and git refuses a repository another
  uid owns: up.sh passes `safe.directory=<worktree>/third_party/cudf` as `GIT_CONFIG_*` (command
  scope counts as protected configuration). Not seen failing yet: a VM check.
- **The marker** is `~/.config/peacock/machine`, bootstrap's last write, so a bootstrap that
  stopped midway is rerun by `provision-vm.sh`. Forwarding passes the arguments as one string,
  each `printf %q`-quoted, `$HOME` left to the VM's shell (bash: the root step makes the user with
  `--shell /bin/bash`). A path given to `--env` is the VM's.
- **The chain's branch is never guessed.** The board names no branch, and origin's names do not
  follow the tasks: `ENS-optimizer` carries `join-optimizer.md`, chain C's first task on
  `ENS-optimizer`'s board, and there are `ENS1-*` and `ENS-bp-*` branches. So `--branch <b>`
  resumes a chain, and without it up.sh starts only a new one: it reads the board at every
  `origin/ENS*` tip and stops, naming them, where the chain's first task is past `approved to
  build`: `building`, `reviewing`, `completing`, `completeness approved` or `done`, read as the
  state's first words. `blocked(…)` and `rebase needed(…)` count as the state they carry, so
  `blocked(done)` is a run and `blocked(new)` is not; `approved to build (resumed …)` is not. A new
  chain's first branch is a local `ENS-<task>` at `origin/<base>`, checked out rather than a
  detached HEAD, as `llm-wiki/README.md`, "Run a chain", starts a chain. The first task is the
  lowest-numbered heading, read from the link target: the bracket text carries backticks on the
  real board.
- **`--board <ref>`** is where a new chain's section is read, `origin/master` by default, so a
  scratch chain defined on another branch can run before a merge. Its section needs `(base:
  <that branch>)`: the coordinator reads its own branch's board, and a first branch cut from
  master would not hold the chain. up.sh refuses a base whose board lacks the chain. A
  `--branch` another worktree has checked out is refused before the fast-forward, which would
  move it under that worktree.
- **Forwarding** puts `PEACOCK_FORWARDED=1` on the remote command line; a call carrying it on a
  machine without the marker stops rather than forward again. The laptop scripts check
  `BASH_VERSINFO` for 4.4 in two lines of bash 3.2, before any source: a library that bash 3.2
  cannot parse would fail first. They print their help with bash alone.
- **The env file** is copied with CRs dropped and a final newline (awk), into a temporary file
  beside the copy and moved into place: `--env` may name the laptop's own `peacock.env`, which a
  redirection would empty before awk read it. The VM appends
  `GPU_SSH_KEY_FILE` after a newline whether or not the file ends in one: without both, a file
  with no final newline ran `GPU_SSH_KEY_FILE=` onto `AWS_REGION`'s line and every check passed.
- **`provision-vm.sh --ref`** exists because, before this branch is on master, bootstrap's
  default (master) has no `up.sh`, so the VM check needs it.
- **Claude's login** is `claude auth login` (in 2.1.316 here; the pinned 2.1.304 unchecked),
  with `CLAUDE_CONFIG_DIR=$HOME/.claude`, since a non-interactive ssh does not read the
  `.bashrc` lines bootstrap appends.
- **Fakes.** The VM is a directory reached by a fake ssh that runs the remote command with the
  VM's fakes first on PATH; sudo runs its command as the caller; `install` writes `/etc` and
  `/var` under the VM's `root/`. A check of a real `/etc` path in the root script would read the
  host's, so the script writes the sudoers file every run rather than testing for it.
- **For the VM check, what the fakes cannot show:**
  - useradd, usermod, groupadd and `install -m 440 /dev/stdin` as root on Ubuntu; sudo working for
    the new user with no tty;
  - `docker.io`'s postinst with a `docker` group the root step made first;
  - bootstrap copied over ssh and run with stdin at EOF: the Claude installer, apt and the plugin
    install reading nothing from it;
  - ssh as the new user with the copied keys, and `StrictHostKeyChecking=accept-new` on a first
    contact;
  - `claude auth login` over `ssh -t` in the pinned version, its credentials landing in
    `~/.claude`;
  - a second developer: the shared set and checkout reused, a lock taken, a crashed `.partial`
    removed, and git in that user's container accepting the first user's cuDF checkout;
  - `gh auth login --with-token --insecure-storage` reading the token from ssh's stdin;
  - sshd accepting a key login for the new user, whose password useradd leaves locked, on an
    image whose sshd has `UsePAM no`.

## Task 7: host adapters and the device run (steps 1 and 2's code; no GPU host yet)

- **Where gpu.sh runs.** Where the built tree is: in an instance, whose container `up.sh` gives
  `PEACOCK_INSTANCE=<user>-<chain>` and `PEACOCK_IMAGE_TAG`, or on any machine with a built tree,
  the host file and `GPU_SSH_KEY_FILE`, where `--instance` names the dir and is required.
  `--instance` and `--image` inside an instance are refused, so one tree never ships into two dirs
  or runs an image other than its instance's. The image tag decides the cuDF version; `--cudf`
  beside a tag is refused.
- **A gpu.sh host is a VM of its own.** The old path on nebius-gpu (`scripts/gpu-host.sh`, its idle
  watcher, `gpu-host-test.sh`) knows neither `.device.lock` nor `.activity.lock`: CI's run and a
  gpu.sh run would share the L40S, and the old watcher could stop the VM under a run that waits. The
  old path is not changed, so the two are kept apart by host.
- **The key goes on a host at provisioning, never in prepare.** An agent cannot type a password, so
  `provision-vm.sh --gpu-host` makes the one interactive ssh, with no `-i` and no BatchMode, the
  key's `.pub` on its stdin, appended unless the line is there. It checks the key login before and
  after. `PASSWORD_FIRST_LOGIN` is not a host-file key and is refused by name.
- **What an adapter takes** is its own answer, so `provision-vm.sh` names no provider: `--needs`
  (`INSTANCE_ID` maps to `--instance-id`; a key with no flag is refused) and `--config-name`
  (`--provider-config` is refused for an adapter with none). Power is the adapter's `powers` verb:
  `ssh.sh` never powers; `nebius.sh` powers when `hosts/<name>/config.yaml` is there and the CLI is
  on PATH. The nebius CLI is pinned in the build image (`NEBIUS_VERSION`, `NEBIUS_SHA256`), so an
  instance has it; the config comes through `hosts/`, so it reaches every instance read-only.
- **nebius start and stop** follow `scripts/gpu-host.sh`: `--async`, and each wait a `$SECONDS`
  deadline `PEACOCK_HOST_WAIT` (900 s) after it began, `PEACOCK_HOST_POLL` (10 s) between gets: a
  poll is a CLI call and an ssh with a 20 s connect timeout, so counting polls would overrun. start
  skips RUNNING and STARTING, waits out STOPPING (the API rejects a start then), and waits for the
  key to log in; a host it started that never reaches RUNNING or ssh is stopped again, one it found
  running is not. stop waits for STOPPED, and stops a host still STARTING after its wait, as
  gpu-host.sh does. The fake CLI holds STARTING and STOPPING for two gets, the first of them the
  adapter's own state read, so a start that does not wait out STOPPING fails against it.
- **ssh to a host** is `-i $GPU_SSH_KEY_FILE -o IdentitiesOnly=yes -o BatchMode=yes
  -o StrictHostKeyChecking=accept-new -o HostKeyAlias=peacock-<name>`. The alias keeps one host key
  per host name, so a nebius address that moves on every start still matches. The key path may hold
  no space: rsync's `-e` is split on spaces. A failed login is reported with ssh's own last line.
- **gpu.sh's own side** is `on-gpu-host.sh`, shipped by `prepare` and `ship` with tar over ssh
  (rsync may not be on a fresh host yet; `prepare` installs it), `--unlink-first`, so a runner bash
  is still reading keeps its own script. With it go `peacock-config.sh` (the storage keys' names),
  `machine.sh`, `data-inputs.sh`, `s3.sh`, `generate_testdata.sh` and `pins.env`. `busy` and `hold`
  are sent on ssh's stdin (`bash -s`), so a soft stop and a command's hold need nothing shipped:
  bash reads a script from a pipe no further than the command it runs, so `hold`'s own reads of
  stdin get what follows the script. The sf1 hash is computed over the instance dir, so it equals
  the tree's. `scripts/lib/portable.sh` gives gpu.sh and the adapters `sha256sum` (through
  `shasum`), a `timeout` and a version comparison on a macOS laptop; paste is called with `-`, and
  sort never with `-V`.
- **Exit codes.** 3 is the driver's alone. On the host, `set -E` and `trap 'exit 1' ERR` make every
  failure that is not a `die` (2) exit 1, so a tool's own status (aws's 255 or 2) never reaches
  gpu.sh as a lost connection or a refusal. On gpu.sh's side, the adapter, the provider CLI, rsync,
  ssh's 255 and a step on the host map to 1 and name what failed: the scripts' tar, `sf40-missing`,
  ship's `mkdir`, the hold, and the driver and label reads all go through it. A login sshd refuses
  (the key) is 2; one that gets no answer is 1. An adapter's `powers` is 0 or 1; any other answer is
  said and stops gpu.sh, never read as "powered by hand". `run` keeps the run's 0, 1 or 2, anything
  else 1.
- **The image ID and its label are one read.** `label` reads the ID the tag names once, then the
  label of that ID; gpu.sh compares the label with `image_inputs_differ` over the tree and passes
  the ID to `run`. A tag another tree's `prepare` moves between the two reads, or while a run waits
  for the device lock, does not change what is checked or run. The fake docker answers for an ID as
  the image it was, and its `move.json` moves a tag right after an ID is read.
- **The driver check** reads `DRIVER_MINIMUM_<v>` and `DRIVER_MINIMUM_PTX` from the tree's
  `pins.env`, before anything is shipped or pulled, in `prepare` and in a `run` that starts one.
  Every line of `nvidia-smi --query-gpu=driver_version,compute_cap` must be
  `<version>, <major>.<minor>`; a card below 8.0 is refused (exit 2), one of major 8 or 9 needs the
  version's minimum, any other the newer of it and the PTX minimum (575.51.03, 12.9 GA's driver: a
  card it has no SASS for JIT-compiles the PTX). `DRIVER_MINIMUM_PTX` is a pin no image reads;
  test_image.py lists it with the machine's.
- **The data directory** is `/var/lib/peacock`, made by `install` with
  `sudo install -d -o <user> -g docker -m 2775` when missing, so every user of the host, all in the
  docker group, shares one set of data and one device lock. `PEACOCK_DATA_DIR`, set in the host's
  `/etc/environment` for every user's non-interactive ssh, names another, which must be absolute and
  there: it is never made, since a missing one is most likely a disk that is not mounted. A user
  who has just joined the docker group has it only from the next ssh session, which is every later
  step's; so `install` leaves the check that needs the group to them.
- **The activity lock.** `<data dir>/.activity.lock`. Every on-gpu-host.sh step but `busy` and
  `fetch` takes it shared, read-only; a runner inherits it from the `run` that started it. gpu.sh
  also holds it for its whole command through `hold`, from right after its login, before the driver
  check and the install: an ssh session in a coproc that gets the script and then a heartbeat line
  every `PEACOCK_HOLD_BEAT` (60) seconds; the host side reads with `read -t PEACOCK_HOLD_TIMEOUT`
  (300) and lets go when the beat stops, so a half-open session holds the lock for minutes, not
  ssh's hours. The heartbeat beats only while gpu.sh lives (`kill -0 $$`), so a gpu.sh killed alone,
  with no EXIT trap, lets go within the timeout too. A ship's hold does not: its rsync outlives the
  killed gpu.sh, so the hold waits on past its read while an rsync server of this user receives into
  the instance's dir (`pgrep -f "rsync --server .* \. <dir>/"`, the dir's dots escaped; a pull's
  server, a `--sender`, is not counted), and its run and activity locks go only with the transfer
  done. Ship's rsyncs pass `--timeout=PEACOCK_HOLD_TIMEOUT`, which rsync hands its server: the
  server then writes a keepalive every half of it, so one whose client vanished ends when the host's
  TCP gives up on those writes, about 15 minutes with Linux's defaults, where a silent server waits
  for TCP keepalive, about 2 hours. They pass `RSYNC_PROTECT_ARGS=0` too: protected args would keep
  the dir out of the server's argv. A hold that ends before gpu.sh releases it, its ssh lost or its
  read timed out, is said at release, whatever its status. `PEACOCK_HOLD_BEAT` and
  `PEACOCK_HOLD_TIMEOUT` are whole seconds, the beat below the timeout, checked before anything
  runs; the heartbeat's sleep is killed with it. `hold` answers `unheld` on a data directory this
  user cannot write yet, a new user's before the install puts them in the docker group, as on one
  that is missing. `hold` exits once its read ends: read from stdin, bash would otherwise go on to
  read its next command there and keep the lock with it. On a host with no data directory yet the
  session holds nothing and says `unheld`; a first `prepare` takes the hold again after its install
  made the directory. The lock files and `runs/` are made under umask 002, so another user of the
  host takes them under any umask of their own. The fake ssh streams stdin for the hold. `busy`
  tests it exclusively (`flock -n -x`), beside the compute processes and the device lock, and says
  `idle` or `busy: <why>`; a busy that answers neither declines the stop. Unreachable over ssh, the
  soft stop asks the adapter's `status`: it stops only a host reported STOPPED or STOPPING, and
  otherwise declines, naming `stop --force`, in the EXIT trap too, since a running host that does
  not answer may still run a detached run.
- **Power on every exit.** On a powered host, gpu.sh starts the host and sets `started`; an EXIT
  trap releases the hold and then soft-stops, so a refusal, exit 3 and a failure stop the host as a
  green run does. The trap's own failure is said and keeps the status. A sequence of commands starts
  and stops the host once each. `stop` without `--force` stops unchecked only when the adapter
  reports STOPPED or STOPPING.
- **The detached run.** `run` on the host takes the instance's lock (`<data dir>/runs/<host
  user>-<instance>.lock`, keyed by the host user as run/state is), then starts
  `setsid nohup bash on-gpu-host.sh runner …` in the background, which inherits that lock and the
  activity lock (flock belongs to the open file), and follows the log with `tail --pid`. The state
  is `run/state`, `<pid> <mode> <log> <stamp> <digest> <envhash>`, written whole after the start;
  the digest is of `cpp/install`'s names, sizes and mtimes, the envhash of the `NAME=value`s passed
  through, which `run/env` lists. With the lock held, the run is going only when its state's pid is
  alive and has no status yet; a lock held otherwise, by a ship or a run being started (whose state
  is waited for up to 10 s), is "busy", refused: no rerun follows a dead pid. The container is
  `peacock-run-<host user>-<instance>`, and a run is refused while one of that name exists: a runner
  killed with its container alive lets go of both locks, and the name keeps a second container of
  the same instance off the card. It is no lock for other instances: once the runner and its client
  are gone, the device lock is free, and another instance's runner starts its container beside the
  orphan (on the real-host list). The runner takes `<data dir>/.device.lock` (saying
  `waiting on <host>` into the log), runs the container, renames a bench's `records.tsv` to
  `records-<stamp>.tsv`, and writes its status to `<log>.status`, which stays beside the log for
  pull and for a later follow. gpu.sh's `run` reads `run-state` first: a run still going is
  followed, with no driver or image check, when it is of the same mode, the same environment and the
  same digest; another mode or environment is refused, both named, and so are changed binaries,
  since the status would not be theirs. `follow` takes the stamp `run-state` gave: a run that ended
  in between gives its own log and status, or, not being this run, is refused saying it ended, with
  its status and log, and that the command now starts the run asked for. `ship` holds the instance's
  run lock through its whole rsync, so it is refused while the run is going and no run starts under
  it. A run killed without a status (a reboot, a kill) is reported as ended with no status, exit 1.
  The test drops the session by killing gpu.sh's process group and checks one container, the lock
  held, and the rerun following; without `setsid` it fails. The fake run is held by a file until the
  test has seen what it waits for: a fixed sleep raced each gpu.sh call (about 1.5 s here).
- **A killed bench's record.** A run killed mid-bench leaves `records.tsv` at run-tests.sh's one
  path. The next `pull` (its `settle` step) or `run` renames it `records-<the killed run's
  stamp>-interrupted.tsv`, from the old `run/state`, under the instance's run lock, so `pull` brings
  it home and the new bench starts clean.
- **One card, one run.** The fake docker refuses a device run beside another (an `O_EXCL` marker),
  so a runner that says it waits and does not fails the lock test every time; the test gives such a
  run two seconds to reach docker before it lets the first run end.
- **sf40.** `prepare` needs the four `AWS_*` values whatever the host holds, so it refuses before
  its first side effect. It asks the host which sets are missing first (`sf40-missing`, no key
  sent), and sends the keys only when one is. On the host they go from ssh's stdin into a bash
  array, checked against `PEACOCK_STORAGE_KEYS`, then by `printf` (a builtin) into `docker run -i`'s
  stdin; the container exports them and runs `aws s3 cp --recursive` through `s3_args`. Not
  `docker run -e NAME`: the daemon writes a container's environment to its config on disk. The set
  is staged in `<set>.partial`, checked against `manifest.json` (name and size), summed into
  `SHA256SUMS`, and renamed. A set whose sums and sizes hold is skipped; hashing about 30 GB costs a
  minute or two a `prepare`.
- **pull** makes its three sources on the host first and rsyncs whole directories, since macOS's
  rsync (2.6.9, or openrsync) has no `--ignore-missing-args`; the fake rsync refuses it. It stages
  in `.gpu-test/<name>/pull` (gitignored) and merges. Records (`records-*.tsv`, under
  `testdata/calibration/`, gitignored) are never replaced: a local one that differs is kept and
  named. Results and logs: a local file newer and different is kept and named. Either is exit 1.
  Joining a run's segments into one record per dataset is Task 11's. `pull` removes nothing on the
  host.
- **Docker, rsync and the toolkit** install one prepare at a time on the host, under a flock on
  `PEACOCK_INSTALL_LOCK` (`/run/lock/peacock-install.lock`; the rig's own in the tests); a second
  says `waiting on <host>`. They install only when `docker`, `rsync` or `nvidia-ctk` is missing;
  docker restarts only after the toolkit was just installed.
- **Fakes.** The host is a directory reached through laptop.py's fake ssh, which checks that `-i`'s
  `.pub` is authorized, takes a password login only without BatchMode on a host with a `password`
  file, drops a command matching `drop_on` with 255, and streams stdin; the host's `bin/` comes
  first and `environment` names its data directory. The fake rsync maps `<user>@<address>:<path>`
  and runs the real rsync. nvidia-smi and `docker image inspect` answer only the exact queries made.
- **For the real host (Step 3, on a GPU VM of its own, never nebius-gpu), what the fakes cannot
  show:**
  - `provision-vm.sh --gpu-host` against a password-only host: ssh's password prompt with the key on
    its stdin; sshd taking the key afterwards;
  - `HostKeyAlias` with `accept-new` inside an instance, whose `~/.ssh/known_hosts` is the
    instance's home; the key file mounted read-only accepted by ssh (owner and mode 600);
  - `sudo` without a password for the host's user; `apt-get install docker.io rsync`, the NVIDIA apt
    repository, `nvidia-ctk runtime configure`, `install -d -g docker -m 2775`, and a user who has
    just joined the docker group finishing `prepare` in one pass;
  - `PEACOCK_DATA_DIR` in `/etc/environment` reaching a non-interactive ssh session;
  - `setsid nohup` on the host: the runner surviving sshd closing the session, `tail --pid`
    following it, and the inherited flocks held across `exec`; a host reboot mid-run reported as no
    status; what a killed `docker run` client leaves of its container on the card;
  - the hold: `bash -s` reading the script from a pipe no further than it runs, sshd delivering the
    heartbeat and EOF, the `read -t` timeout releasing a half-open session, and bash's coproc fds as
    gpu.sh closes them;
  - `--name peacock-run-<user>-<instance>` and `docker container inspect` of a container a killed
    client left;
  - gpu.sh on a macOS laptop: bash 5 from Homebrew, `shasum`, BSD paste and sort, and the portable
    timeout around the nebius CLI;
  - `docker run --gpus all` by image ID, as a uid the image does not know, and `--hostname` written
    as `host=`; `docker image inspect --format '{{.Id}}'` giving `sha256:<64 hex>`, and the label
    read by that ID;
  - `nvidia-smi --query-gpu=driver_version,compute_cap` answering `<version>, <major>.<minor>` on
    the driver there, and `--query-compute-apps=pid` empty on an idle card;
  - the build image's `aws` reading the keys from the environment the container exported, against
    the real endpoint; `manifest.json` as upload_datasets.sh wrote it parsed by the tr pipeline;
    the time to hash both sf40 sets;
  - sf1 generation in the build image on the host with `--network none`;
  - the nebius CLI: `--config <file>` and `--no-check-update` as global flags, the config on a
    read-only mount (the CLI may want to write a token cache beside it); the JSON of
    `compute instance get`; `start --async` and `stop --async`; the profile recipe in build-test.md,
    made with `nebius --config <file> profile create … --private-key-file`;
  - the rsync server's argv on the host, `rsync --server … . <dir>/`, which a ship's hold waits on
    after a gpu.sh killed alone, and a pull's with `--sender`, which it does not; a half-open ship
    server ending when the host's TCP gives up, not at `--timeout`;
  - pull from a macOS laptop with its own rsync, and ship, whose `--relative --delete` and
    `--timeout` must hold on openrsync and on rsync 2.6.9, which also must ignore
    `RSYNC_PROTECT_ARGS`;
  - two first `prepare`s at once on a fresh host: the install lock in `/run/lock` serialising apt,
    the toolkit's keyring and its sources list;
  - an orphan container of one instance beside another instance's run, once both runner and client
    of the first are gone;
  - the read-only opens of the shared locks by a second Unix user;
  - two chains' `run` on one real card: the second waits for the first's container to exit;
  - the device tiers' counts equal the latest `GPU Tests (nebius-gpu)` run's.

## Task 8: provenance and the per-device tree

Worked in its own worktree, branch `ENS-containers-task8` from `0d9e1dc4`, to be replayed onto
`ENS-containers` after Task 5. Built and tested in `peacockdb-build:cudf-25.02` with no network.

**Where each of the five comes from**:
- `device`, `cuda`: one additive ABI symbol, `peacock_device_info(PeacockDeviceInfo*)`
  (`cpp/include/peacock_gpu.h`), the name and `runtime`. It reads `cudaGetDevice`,
  `cudaGetDeviceProperties` and `cudaRuntimeGetVersion`. No existing read gives either:
  `peacock_gpu_version` is the engine's `0.1.0`, and the pool's info holds byte counts only.
  Rust wraps it as `executor::device_info()`.
- `driver=` is the kernel module's release, `580.95.05`, read from `/proc/driver/nvidia/version`.
- `cudf=` is the release, `device_answer::cudf_release()`, `25.02.02`: the same build-time copy of
  `version_config.hpp` as the `gpu-result.txt` stamp, which stays `major.minor`.
- `host=` is `/proc/sys/kernel/hostname`. Inside a container that is the container's own name
  unless `docker run --hostname` sets it: the GPU-host runner must pass the machine's.

**The slug.** `device_slug` drops a leading `NVIDIA `, lowercases ASCII and joins the runs of
`[a-z0-9]` with `-`. It has two copies: `test_support/provenance.rs`, which names the results tree,
and `scripts/calibration/record.py`, which names the record's pin and key. Both tests read one
table, `testdata/device_slugs.tsv`.

**The record's key and pin.**
- New shape, by device: key `<slug>/<dataset>.sf<sf>/<sha256>.tsv`, pin line
  `<sha256>  <slug>/<dataset>.sf<sf>/records.tsv`, fetched to
  `testdata/calibration/<slug>/<dataset>.sf<sf>/records.tsv`. The rule is the old one, `name` =
  the pin path's directory, so `calibration-bucket.sh` did not change and
  `build-test-shadgpu.sh --pull-benchmarks` publishes per device unchanged: its names come from
  `split_record.py`, which now prints `<slug>/<dataset>.sf<sf>`.
- `fetch_record.sh` takes `[<slug>/]<dataset>.sf<sf>`.
- The H200's two records keep their objects and their pin lines, `tpch.sf40/records.tsv` and
  `tpcds.sf40/records.tsv`, so `records.sha256` is unchanged. Moving them under `h200/` needs an
  object at `h200/<dataset>.sf40/<sha256>.tsv`, and nothing is uploaded here.
- **What a later step must do to move them** (the list now lives in #226): copy each of the two
  objects in the bucket,
  `s3://calibration/<dataset>.sf40/<sha256>.tsv` to `s3://calibration/h200/<dataset>.sf40/<sha256>.tsv`,
  and read each back against its sha256. Then rename the two pin lines to `h200/…`, point
  `scripts/exec_model/optimizer/call_cost.py`'s `RECORD` at `calibration/h200/tpch.sf40/`, and
  change `exec-model-corpus.yml`'s `fetch_record.sh tpch.sf40` to `h200/tpch.sf40`. This task may
  not edit that workflow. `create_nsys_profile.sh` reads `calibration/<dataset>.sf<sf>/records.tsv`
  and needs the device level too before an Nsight pass runs on another card.
- The new device's record (Task 11) needs no copy: its first publish writes the per-device key.
  The new GPU-host path's pull must publish as `build-test-shadgpu.sh` does: split, upload under
  `<slug>/…`, read back, pin.

**The tree.** `benchmark-results/<slug>/<dataset>.sf<sf>/<mode>.benchmark.txt`; the H200's six
files are under `h200/`, byte-identical (moved in the working tree, not staged).
`every_declared_case_has_a_section_in_the_tree` now holds the case list to every device directory.
Task 9's registry-driven list will fail it against `h200/`, which holds the 26 cases it was run
on. Task 9 decides which trees the list is held to.

**verify-26.02.** One tree per card, not per cuDF: its 25.02 run is committed first, the 26.02 run
on the same card rewrites the files, and `git diff` with the trailers' `cudf=` is the comparison.
Its spec's step 4 and Scope row say so, and its impl plan's Task 5 carries a note that Steps 1 and
3 are superseded.

**#226** stays open: it lists what remains, the first run with
the five lines (Task 11) and the H200 migration with its two hazards. `nsys_hbm.py` compares a
capture's GPUs with the record's `device=`.

**A slip.** The H200 move was first made with `git mv`, which staged it. `git restore --staged
-- testdata/benchmark-results` put the index back to HEAD; the move stays in the working tree.

**Tests.** Each new case was red first: the Rust ones against stubs (the slug returned unchanged,
an empty version, a trailer and a heading without the five), the two golden cases against the
unmoved tree, the Python ones against the old `split_record.py` and `fetch_record.sh`. Two
`test_fetch.py` cases pin what the old script already did and passed against it: the H200's pin
fetched as it was, never shown red, and the refusal of a malformed name, red only on the mutation
below. Each other case went red again on a mutation of the code it guards, the code then restored:
- the vendor strip removed: `device_slug_is_stable` and `test_the_device_slug_is_the_harness_s`;
- `% 100` for `% 1000 / 10`: `cuda_versions_read_as_major_minor`;
- `driver` and `cuda` swapped in `fields`: `trailer_names_its_provenance` and
  `record_heading_names_its_provenance`;
- `device=NVIDIA L40S` added to one `h200/` trailer: `every_benchmark_file_lies_under_its_device`;
- a record with two `device=` lines accepted: `test_a_record_naming_no_device_or_two_is_refused`;
- `fetch_record.sh`'s name check loosened to `\.sf[0-9]+$`:
  `test_a_name_that_is_not_a_pin_path_is_refused_before_any_fetch`.

A cpu-tier gtest of the null refusal stayed green with the null check removed, because with no
device the runtime refuses first. It was dropped, and the refusal is asserted in the gpu gtest.

**Results**, in `peacockdb-build:cudf-25.02`, network off:
- `cargo test --features rust-only -p peacockdb-core`: lib 774 passed and 2 ignored (776),
  `test_cpu_corpus` 2009, `test_corpus_goldens` 28, `test_cost_model` 4, `test_ci_coverage` 11,
  `test_module_layout` 18, `test_golden_format` 43. No warnings.
- FFI build (default features): green, no warnings. `peacock_gpu_benchmarks -- --skip bench_
  --test-threads=1` 8 passed, three runs; `--lib -- ffi_tests::` 4; `peacockdb-ffi --test
  test_ffi` 3. The binaries need `LD_LIBRARY_PATH=/opt/cudf/lib`. Without `--test-threads=1`,
  `an_append_under_a_different_heading_is_refused` fails: two cases set `PEACOCK_RECORD_PATH`
  process-wide, which is why the binary is documented single-threaded.
- `cargo test -p peacockdb-core --features gpu --no-run`: green. One warning, already on the
  base: an unused `AsArray` import in `src/tests/gpu_tests/aggregate_dimension_cases.rs`.
- `scripts/build.sh --configure --build` (gtests included): green, no warnings.
  `peacock_cpu_tests` 15 passed. `git clang-format --diff HEAD` changes nothing.
- `scripts/calibration/tests/`: 35 passed (calls 6, fetch 3, hbm 8, plot 7, split 11).

**Not exercised: anything on a device.** That covers `peacock_device_info` on a card and
`PeacockGpu.DeviceInfoDescribesTheDevice`, which is compiled and not run. It also covers
`provenance::of_this_run`, a `bench_` case writing under `benchmark-results/<slug>/` with the five
lines, and `host=` inside the run image.

## Task 8: the five keys' sources

- **I1, `cudf=` with the patch level.** `cudf_release()` beside `cudf_version()`, one header
  reader (`config_field`) for both. It writes `25.02.02`, or `26.02.01` on 26.02. The
  `gpu-result.txt` stamp stays `25.02`.
- **I2, `driver=` is the driver release.** It is read from `/proc/driver/nvidia/version`, with no
  fallback. The NVIDIA container toolkit puts that file into a container's `/proc`, so the run
  image reads the host's driver the same way the host does. `/sys/module/nvidia/version` would
  hold the same number, and a second source is a second way to disagree. The parser takes the
  file's one `NVRM version:` line and that line's one word of digits and dots, and refuses
  anything else. An unreadable file is refused naming its path. The ABI field `driver` is
  dropped, so `PeacockDeviceInfo` is `name` and `runtime`. Not seen on a device: the file inside
  the run image.
- **I3.** `--hostname "$(hostname)"` is in build-test.md's run-image command, with a line saying
  why. Task 7's Interfaces carry it, with `test_run_passes_the_machines_hostname`.
- **I4.** #226 is reopened: back in `tickets/benchmarks.md` and the index, out of the archive. The
  links from `tasks.md`, `containers.md` and build-test.md resolve again.
- **`nsys_hbm.py`.** A join refuses a capture unless every row of its `TARGET_INFO_GPU` has the
  slug of the record heading's `device=`. A record with no `device=` is
  taken as the H200's. The
  comparison is by slug, the name the results tree files a device under: a spelling difference
  between nsys and the runtime is not a refusal, and a different card always is. Every row is
  checked, since a host with two kinds of card cannot say which ran the calls. Not seen against a real
  export: the fixture's `TARGET_INFO_GPU (id, name)` is my reading of nsys's schema.
- **`device_slug` in Python** moved from `split_record.py` to `record.py`, which every
  calibration script already shares.
- **I5.** The migration list and the two hazards are in #226. The plan's Step 2 says the H200 pins
  wait on it.
- **I6.** The architecture count is twenty-six symbols, with `device_info` in instrumentation.
  `record.py`'s docstring names the nine `# run:` lines.
- **Nits.**
  - N1: `# mode = ` regained its space; an Edit had stripped it.
  - N2: the gpu gtest asserts `runtime >= 11000` and the null refusal.
  - N3: `testdata/device_slugs.tsv`.
  - N4: the comment on the provenance module's `allow(dead_code)`.
  - N5: every `device=` line of a trailer is read; more than one fails. None is allowed: the
    H200's sections have none.
  - N6, N7: wording and reflow.
  - N8: notes in the plan's Task 9 and Task 11.
- **A slip, fixed:** a first insertion put `cudf_release` between `cudf_version`'s doc comment and
  its item. It is now below `cudf_version`.

**Red, then a mutation.** The new Rust and Python cases failed against stubs or the old code first.
Each mutation turned its cases red, and the code was then restored:

| mutation | red cases |
|---|---|
| patch printed unpadded | `the_cudf_release_is_read_with_its_patch_padded` |
| the two-module-lines check removed | `the_driver_release_is_read_from_the_module_version` |
| the path dropped from the read error | `an_unreadable_or_unknown_driver_version_is_refused` |
| a wrong row added to `device_slugs.tsv` | `device_slug_is_stable` and `test_the_device_slug_is_the_harness_s` |
| two `device=` lines in one `h200/` trailer | `every_benchmark_file_lies_under_its_device` |

`test_hbm.py`'s two device cases failed before `check_device` existed.

**Results**, in `peacockdb-build:cudf-25.02`, network off:
- `cargo test --features rust-only -p peacockdb-core`: lib 777 passed and 2 ignored (779),
  `test_cpu_corpus` 2009, `test_corpus_goldens` 28, `test_cost_model` 4, `test_ci_coverage` 11,
  `test_module_layout` 18, `test_golden_format` 43. No warnings.
- FFI build: green, no warnings. `peacock_gpu_benchmarks -- --skip bench_ --test-threads=1` 8,
  `--lib -- ffi_tests::` 4, `peacockdb-ffi --test test_ffi` 3.
- `--features gpu --no-run`: green, with the one warning already on the base.
- C++ rebuilt with no warnings: `peacock_cpu_tests` 15 passed. `peacock_gpu_tests` lists its three
  cases (not run). `git clang-format --diff HEAD` changes nothing.
- `scripts/calibration/tests/`: 37 passed (calls 6, fetch 3, hbm 10, plot 7, split 11).
- Staged into a scratch index, `testdata/benchmark-results/` shows six renames at 100 %.
- Not exercised: anything on a device, now including the driver file inside the run image and
  `check_device` against a real Nsight export.

## Task 8: records that predate provenance

- **A record naming no device is the H200's.** `nsys_hbm.py` has the constant
  `PREDATES_PROVENANCE = "NVIDIA H200"`, with its reason: both hosts that ran the benchmark
  before provenance, shad-gpu and verda-gpu, are H200s, and only shad-gpu published. The refusal text says when it was assumed. The
  capture is still held to it, every `TARGET_INFO_GPU` row. So the pending HBM pass on shad-gpu
  over the two pinned records can run. The docstring's usage example names the per-device record
  path and says where the H200's lie. A capture with no `TARGET_INFO_GPU` table is a one-line
  refusal naming it, not a sqlite traceback.
- **The golden holds a new tree to the five keys.** `PREDATES_PROVENANCE = ["h200"]` in
  `test_corpus_goldens/benchmark.rs`, with the same reason. In any other tree, every timed
  trailer holds each of `device driver cuda cudf host` once, as its last five keys, and
  `device=` slugs to the directory. An `h200/` section may carry none of them, or all five. The
  check is `device_tree_problem(root)`. `a_tree_without_provenance_under_another_device_is_refused`
  feeds it the committed `h200/` copied under `h100-80gb-hbm3/` in a temp directory.
- **#226** gains a line: both constants go once the H200 is re-run with provenance.
- build-test.md:
  - the cuDF GPU smoke row is corrected (item 3);
  - the Nsight sentence names the H200 assumption;
  - a segmented run needs the same `--hostname` in every `docker run` (N5).
- **Nits.**
  - N1: the stale references to `split_record.py`'s copy now name `record.py`.
  - N2: the `benchmark_trailer` comment names `run_section`.
  - N3: the split fixture uses `driver=580.95.05` and `cudf=25.02.02`.
  - N4: three more cases. A module line naming two releases, inside the driver test. A capture
    listing an H200 and an L40S. A capture with no GPU table.
  - N6: both script headers are ten lines.

**Red, then a mutation (code restored each time):**

| mutation | red cases |
|---|---|
| `PREDATES_PROVENANCE` set to the H100 | `test_a_record_naming_no_device_is_the_h200_s` and `test_a_device_less_record_still_refuses_another_card_s_capture` |
| only `gpus[0]` checked | `test_one_foreign_card_among_the_capture_s_gpus_is_refused` |
| `Some(release)` for the second-release check | `the_driver_release_is_read_from_the_module_version` ("a module line naming two releases") |
| the five-key condition replaced by `false` | `a_tree_without_provenance_under_another_device_is_refused` |

Before the change, three nsys cases failed. The device-less H200 join and the foreign capture
were refused under the old rule. The missing table raised `OperationalError`.

**Results**, in `peacockdb-build:cudf-25.02`, network off:
- rust-only, every target: lib 777 passed and 2 ignored, `test_cpu_corpus` 2009,
  `test_corpus_goldens` 29, `test_cost_model` 4, `test_ci_coverage` 11, `test_module_layout` 18,
  `test_golden_format` 43. No warnings.
- FFI: `peacock_gpu_benchmarks -- --skip bench_ --test-threads=1` 8, `ffi_tests::` 4,
  `test_ffi` 3. `--features gpu --no-run` is green, with the one warning already on the base.
  `peacock_cpu_tests` 15 passed.
- `scripts/calibration/tests/`: 40 passed (calls 6, fetch 3, hbm 13, plot 7, split 11).

## Task 8: the slug comparison

- **I1, the slug comparison and the accept path are now reached.**
  - `synthesized_tree` copies one committed `h200/` file into a temp tree under a device
    directory, strips any provenance lines, and can append the five after each `allocator=`.
  - Three cases use it. The bare copy under `h100-80gb-hbm3/` is refused, asserted on the
    no-provenance refusal's own wording, which holds even once `h200/` gains five-key sections.
    `device=NVIDIA L40S` there is refused ("measured on"). `device=NVIDIA H100 80GB HBM3` passes.
  - With the slug check deleted, the L40S case is red. With it inverted, the L40S case and the
    H100 case are both red.
- **I2.** `nsys_hbm.py`'s `PREDATES_PROVENANCE` sits after the metric names, and `TUPLE` has its
  comment back.
- **N1.** `split_record.py`'s header is ten lines. The sentence on why a filtered run is refused
  left it; build-test.md's *Benchmark data flow* has it.
- **N2.** Two refusal wordings. A trailer with none of the five outside `h200/` names the allowed
  trees. One with only some of the five, or out of order, names its keys.
- **N3.** The reason everywhere is now: both hosts that ran the benchmark before provenance,
  shad-gpu and verda-gpu, are H200s, and only shad-gpu published. That covers the golden's
  constant, `nsys_hbm.py`'s constant and refusal, build-test.md, and "records that predate provenance" above.
  #226 states no reason.
- **N4.** An empty GPU table is refused naming the table, with its own case. It is red with the
  old merged `not gpus or …` condition put back.
- **N5.** Line lengths: `provenance.rs`, `nsys_hbm.py`'s docstring paragraph, and build-test.md's
  Corpus goldens paragraph.

**Results** (build image 25.02, network off):
- rust-only `test_corpus_goldens`: 31 passed.
- `--lib`: 777 passed and 2 ignored.
- `test_module_layout`: 18 passed.
- `scripts/calibration/tests/`: 41 passed (calls 6, fetch 3, hbm 14, plot 7, split 11).
- Counts: total 5912 = Rust 4480, C++ 174, Python 1258.

## Task 9: the registry-driven case set

Built and tested in `peacockdb-build:cudf-25.02` with no network, as Task 8 was.

**The list.** `corpus_benchmark_cases.inc` stays, written from the registry: every tpch and tpcds
row's enabled `gpu_*` cells, at sf40, one `corpus_query_benchmark!` line per query, the registry's
row order, the modes in `MODES` order. 534 cases over 110 queries (tpch 191, tpcds 343, the same
counts as the device corpus); the 32 rows with no enabled device cell get no line, so no `none`
markers. `the_benchmark_cases_follow_the_registry` (`test_corpus_goldens`, rust-only) compares the
two sets of `(dataset, query, mode)` both ways and names what differs; under `UPDATE_CANONICAL=1`
it rewrites the file first. Two rewrites give the same bytes. Task 10 regenerates it from master's
registry with that command.
- Why a written file and not a build script or a runtime read: the old path reads the list as
  text (`shadgpu-env.sh`'s `bench_lines`, `split_record.py --cases`), and neither may change here.
  Both parse the new file: 534 cases, `tpch.sf40` and `tpcds.sf40`. And a `bench_` test per case
  keeps libtest's name filter, which a segmented run selects by.
- `every_timed_case_is_enabled_on_a_device` stays: it holds the list to `corpus_cases.inc`, and
  the registry is held to that file only on the device rung.

**Which trees the list is held to.** A new device's tree is held to the whole list; a tree in
`PREDATES_PROVENANCE` (`h200/`) is held to the list it was measured from, `h200/cases.inc`.
The spec asks for "the benchmark of
every tpch and tpcds query the registry enables on the device" (Done-when 3), so the whole list is
what a new tree must hold. And it puts the H200 out of this task ("The L40S and the H200 are out
of this task"), so `h200/` can never gain the 508 cases it lacks here; holding it to the list
would leave the test red for good. The constant already names the trees the earlier harness wrote,
and the one re-run that retires it (#226) would also complete the list, so one constant carries
both. `every_declared_case_has_a_section_in_the_tree` reads `missing_case_problem`;
`a_new_device_tree_is_held_to_every_declared_case` feeds it one committed H200 file under
`h100-80gb-hbm3/`. A failed case's record counts as its section.

**The rowgroup modes** are in the list, as the registry enables them: the list follows the
registry and the run decides what it times. Task 11 must decide:
- Running every mode makes the tree whole.
- Deferring the rowgroup modes leaves the new tree short, and
  `every_declared_case_has_a_section_in_the_tree` goes red. Committing that tree then needs a
  named exception in that test (device, modes, reason) that the human approves. Nothing here
  builds one.
- `docker/run-tests.sh bench` takes one positional filter, so "the three other modes" is two
  passes (`_single`, `_sized`). The second pass needs the first's record pulled and removed,
  since `run-tests.sh` refuses a record holding rows.

**A failed case.** `benchmark_case` reads the capture variable and the provenance, installs the
pool, sets event timing and checks the pool and the release build, all before the boundary. Then
`measure_into` (`test_support/benchmark_section.rs`) runs the case under `catch_unwind`. A panic
becomes the case's section:

    == <query>
    status=failed reason=<the panic message, whitespace runs folded to one space>
    device=… driver=… cuda=… cudf=… host=…   (five lines, as in a trailer)

Then the test fails naming the file and the reason, and libtest goes on to the next case. The
record's rows are appended before the section is written, so a failed case appends
none. A captured run does not go through `measure_into`: it writes no section,
so a panic there is only a panic. The checks before the boundary are the run's, not a case's:
caught, one refusal would be written into every section of the tree. `device_tree_problem` holds a
failed record to its status line and the five, under `h200/` too.

**A rerun** (only under `PEACOCK_BENCH_RESUME=1`). `measure_into` keeps a section whose `--- run ---` trailer ends with this run's five
lines exactly, and skips the case (`CaseOutcome::Held`, printed). It runs again a failed case,
one never reached, and one another device, driver, CUDA, cuDF or host measured. So a 26.02 rerun
on the same card (verify-26.02) measures everything again. `merge_section` publishes by rename, so
a kill mid-case leaves the file as the last finished case left it.

**For Task 11** (not built here):
- A segmented run's records. Each segment writes its own record, since `run-tests.sh` refuses one
  holding rows, and a case that segment held writes no rows. Two segments' headings also differ in
  `allocator=`, which names the free GiB its process saw, so `append_records` refuses them as one
  file. `split_record.py` refuses a record short of the list, so a failed case, a held case or a
  deferred mode all stop the publish. The record scripts need a way to publish a run made of
  segments and failures.
- A case that kills the process (a segfault, an abort, the OOM killer) writes nothing, and every
  rerun reaches it again. Skip it by name and record why.
- Drift I may not fix: `build-test-shadgpu.sh`'s publish comment says "a failing case appends its
  rows before it panics". That is no longer so, and the global constraints forbid editing it.

**Red, then a mutation (code restored each time).** The two new rust-only cases on the run's
write path failed first against a stub `measure_into` that ran the case and wrote its body: the
case's panic went through. The list test failed first against the hand-written list: 508 enabled
cells not listed. Then:

| mutation | red cases |
|---|---|
| q6's line dropped from the list | `the_benchmark_cases_follow_the_registry` (5 enabled, not listed) |
| `tp4_single` added to q11, disabled there | `the_benchmark_cases_follow_the_registry` (1 listed, not enabled) |
| the `PREDATES_PROVENANCE` skip removed | `every_declared_case_has_a_section_in_the_tree` |
| every tree skipped | `a_new_device_tree_is_held_to_every_declared_case` |
| the failed record's five-key check off | `a_failed_record_without_its_provenance_is_refused` |
| no `catch_unwind` | `a_failed_case_is_recorded_and_the_run_goes_on`, `test_partial_run_keeps_completed_cases` |
| the reason not folded to one line | `a_failed_case_is_recorded_and_the_run_goes_on` |
| no provenance in the failed record | `a_failed_case_is_recorded_and_the_run_goes_on` |
| nothing held | `test_partial_run_keeps_completed_cases` (q1 run again) |
| held whatever the provenance | `test_partial_run_keeps_completed_cases` (q6 held) |
| a failed record held | `test_partial_run_keeps_completed_cases` (q17 held) |
| sections sorted as text (FFI build) | `the_sections_of_a_file_are_ordered_numerically` |

**Results**, in `peacockdb-build:cudf-25.02`, network off:
- `cargo test --features rust-only -p peacockdb-core`: lib 777 passed and 2 ignored,
  `test_cpu_corpus` 2009, `test_corpus_goldens` 36, `test_cost_model` 4, `test_ci_coverage` 11,
  `test_module_layout` 18, `test_golden_format` 43. No warnings.
- FFI build: `peacock_gpu_benchmarks -- --skip bench_ --test-threads=1` 8 passed, 534 filtered;
  `--list` 542, of them 534 `bench_`. `--lib -- ffi_tests::` 4, `peacockdb-ffi --test test_ffi`
  3. No warnings.
- `cargo test -p peacockdb-core --features gpu --no-run`: green, with the one warning already on
  the base (`AsArray`). Built with `CARGO_PROFILE_DEV_DEBUG=0` in its own target dir: with debug
  info each test binary is 1.2 GB, and the gpu set beside the FFI set filled `/build`.
- The list rewritten twice gives the same bytes. `shadgpu-env.sh`'s readers and
  `split_record.declared` both parse it as 534 cases.
- Counts: total 6445 = Rust 4993, C++ 174, Python 1278.

**Not exercised: anything on a device.** That covers `benchmark_case` through `measure_into` on a
card: a real refusal recorded, a held case printed, the pre-flight refusals, and a capture pass.
It also covers a run killed mid-case and its rerun.

## Task 9: resume and failed cases

- **I1, a section is kept only on resume.**
  - `PEACOCK_BENCH_RESUME` (`RESUME_ENV`): `1` keeps a section whose trailer ends with this
    run's five lines; unset or `0` measures every case, as at d2a0f88c.
  - Any other value is refused naming the two. The refusal comes before the boundary, read
    beside the capture variable.
  - The setting is `Resume { No, KeepMeasured }`, an explicit argument of `measure_into`.
    `Resume::from_env` reads the variable only in `benchmark_case`.
  - A kept case prints `kept, <file> holds its section … and PEACOCK_BENCH_RESUME=1`.
  - The rust-only tests pass the enum rather than set the variable: the golden binary is
    multi-threaded, and only one case there touches the variable, the one that reads it.
  - With resume off by default, a rerun after a run that died re-measures everything. That
    removes I6's default-path loss.
- **I2, `h200/` held to its own list.** `testdata/benchmark-results/h200/cases.inc` is
  d2a0f88c's list, its 14 lines (26 cases) under a four-line header. It is text only, and
  nothing expands it. It lies in the tree it describes, so retiring the H200 constants removes it
  in the same place. `PREDATES_LIST = "cases.inc"`; `missing_case_problem` reads it for a tree
  in `PREDATES_PROVENANCE`. Nothing else under `benchmark-results/` reads a non-`.benchmark.txt`
  file: the tree readers walk `.benchmark.txt` only, and the pulls are additive.
- **I3.** `a_file_short_of_a_declared_section_is_refused`, over the synthesized one-file tree.
- **I4, run-level refusals before the boundary.**
  - `assert_dataset_present(dataset, sf, what)` checks that `testdata/<dataset>.sf<sf>` is a
    directory.
  - The record's heading is checked before measuring, through `record_heading_mismatch`. It can
    be: the heading holds only `timing_mode`, `build`, `allocator`, `capture` and the five, all
    known once the pool is installed. The allocator string is now read once, before the case,
    rather than after the runs; `install_rmm_pool` is idempotent, so it is the same string.
  - `append_records` uses the same reader. Its own refusal could still fire if the file changed
    mid-case: then the section is written, and the test panics naming the mismatch.
- **I5.** The H200's Nsight pass runs from a checkout carrying the 26-case list (d2a0f88c or
  earlier). This is written in build-test.md, at the derived-files bullet and at step 5, and in
  #226. #226 is 30 lines, its cap.
- **I6.** Task 11 gains a step 3: publishing a run made of segments, held cases and failures. It
  also says what Task 9 settled and that a deferred mode needs a named exception. build-test.md
  says a resumed run's record is not published today.
- **N1.** The two headers are ten lines. The list was rewritten, header only.
- **N2.** A missing tpcds dataset is a refusal of every tpcds case before it records anything.
- **N3, code.** `measure` returns `(body, rows)`. `measure_into` is generic,
  `CaseOutcome::Measured(T)`. `benchmark_case` appends the rows after the section is written,
  and a captured run appends them directly.
- **N4.** The list test compares bytes with `case_list_text` and names the differing cells. The
  rewrite publishes through `<list>.inc.new` and a rename.
- **N5.** The list is a row in build-test.md's Golden files table.
- **N6.** The completeness test's doc names the exception.

**Red, then a mutation (code restored each time).**
- The byte comparison went red on the header change of N1, with every cell equal, before the
  rewrite.
- The new cases were written in the same pass as the code they guard, so each one's red is its
  mutation below.

| mutation | red cases |
|---|---|
| the hold ignores `resume` | `a_measured_section_is_measured_again_unless_the_run_resumes` |
| `yes` read as `0` | `the_resume_variable_is_unset_zero_or_one` |
| `h200/`'s q19 section renamed | `every_declared_case_has_a_section_in_the_tree` |
| `if false && !missing.is_empty()` | `a_file_short_of_a_declared_section_is_refused` |
| the dataset check vacuous | `a_missing_dataset_is_refused_before_any_case` |
| the heading reader accepts any heading | `a_record_another_run_wrote_is_refused_before_any_case` |

The calls in `benchmark_case` itself are device code: the refusals' placement before the
boundary and the kept line were compiled and not run.

## Task 9: the H200's list

- **R1.** The H200's list is named by `02f6a4d8` ("benchmarks: 26 device-enabled cases at sf40,
  the per-call record in S3 by its content"), an ancestor of ENS-optimizer, PR #165's head, so
  it reaches master. Its case lines equal `h200/cases.inc`'s: compared with `diff`, and no commit
  between it and d2a0f88c touches the list. The places: build-test.md (the derived-files bullet,
  the H200 paragraph, step 5), #226 (still 30 lines), and `h200/cases.inc`'s header.
- **N-a, rows before the section.**
  - `measure_into` takes `before_section: impl FnOnce(T)`. It runs on what the case returned
    beside its body, inside the same failure boundary, and before `merge_section`.
  - `benchmark_case` passes `|rows| append_records(&rows, &meta)`. `CaseOutcome` is no longer
    generic: `Held`, `Measured`, `Failed(reason)`.
  - A crash between the append and the rename leaves no section, so a resumed run measures the
    case again; its rows from the dead attempt stay in that segment's record.
  - A panic in the append, the record refusing the rows, is the case's failure. Its section is
    `status=failed reason=<the record's reason>` with the five. The record then holds none of
    the case's rows when the refusal came before the write (a heading mismatch, an unopenable
    file), and part of them if `write_all` failed partway.
  - A failed measurement never reaches the hook, so it still appends nothing.
- **N-b.** The plan's Review focus 4 and Task 9's Interfaces say a rerun skips completed cases
  only under `PEACOCK_BENCH_RESUME=1`.
- **N-c.** The run-image `--hostname` bullet gives the real reason: a resumed segment keeps only
  sections whose `host=` is its own. It also says `docker run` needs `-e PEACOCK_BENCH_RESUME=1`.
- **N-d.** The refusal prints the bare value: `PEACOCK_BENCH_RESUME=yes is neither …`.

**Red, then a mutation (code restored each time).** The two new cases were written with the
code.

| mutation | red case |
|---|---|
| the section merged before the hook (rows after the section) | `the_rows_are_appended_before_the_section_is_written` |
| the hook called outside `catch_unwind` | `a_case_whose_rows_the_record_refuses_is_recorded_as_failed` |
| the refusal prints `{:?}` of `to_str()` | `the_resume_variable_is_unset_zero_or_one` |

## Task 9: a refused append

- **N2, a refused append leaves no rows.** `append_records` writes through `append_whole(file,
  text, write)`. It reads the file's length first, and on a write error `set_len`s back to it,
  then panics with the write's error. If the cut back fails too, the panic names both errors and
  says the last row may be cut off. `write` is a parameter only so a test can stop a write
  partway; production passes `write_all`. build-test.md now says a refused case's record holds
  none of its rows.
- **N1.** Task 11 Step 3 names the case it must design for: a segment killed between a case's
  rows and its section leaves rows with no section, and the resumed segment measures that case
  again, so its rows are in two segments' records.
- Task 9 Step 2's code items are ticked, the commit is not.

**Red.** `an_append_that_stops_partway_leaves_the_file_as_it_was` (`record/tests.rs`, `--lib`)
writes four of a call's bytes and fails. With `set_len` replaced by `Ok(())` it is red: the file
ends `row 1\nrow ` instead of `row 1\n`.

## tpcds sf40 in its bucket

Generated by `testdata/generate_testdata.sh --bench tpcds --sf 40` at `7bc901f2` (the script's
last change) with the DuckDB 1.5.4 CLI checked against `docker/pins.env`, on a 16-vCPU, 62 GB
Ubuntu 24.04 VM with its output and DuckDB's temp directory on a separate volume: 14 min 37 s,
peak RSS 50.5 GiB, the temp directory 8 GB at its largest, exit 0. Published by
`scripts/upload_datasets.sh tpcds.sf40`, now knowing the bucket: 24 parquet, 15,787,929,533
bytes, sizes checked against the manifest before the manifest went up. Read back with the
reader's key, each object's sha256 equals the generated file's, and `check_s3_datasets.py
--buckets tpcds-sf40 tpch-sf40` passes both (tables, columns, row counts at SF=40). The writer's
key was on the VM for the upload alone.

The copy the H200 results were measured on is on shad-gpu; its sums are to be compared with these
when that host is back. Equal sums make the bucket's copy that copy.

```
bd6bd17b402a68dfe7e7330a4b08a200a28200f6f1260b9bbe4055231b34d3a9  call_center.parquet
c0d7f6839cfd2bbbe720cb8bab4171edba1536577839cfdc2dd98d9dedaeb4ad  catalog_page.parquet
6fbc403e4f568702a189311122e5f64e5fe28d486fdff2f8460f54779f057b60  catalog_returns.parquet
22f60ce589a91b648ba45c2738cb4c9755bde3aa9229bba6c3ae5c2dcf0bf6fa  catalog_sales.parquet
47f6e1cf947eb9c95d64422dc35f8bbca17d95388e7c6e80991d2b394af665d8  customer.parquet
5b79edc0d4a54a66198525ebe1ea7523d46a4fd55a453d801fc07d3cd4a71296  customer_address.parquet
3d0366fd106c6be1dfd25c9caa2e22ccd2a460d23180b904db7e7fe8364e3a84  customer_demographics.parquet
5a5e0eca50811a5ac5acdbb3d092d43c3fa919bfde42139abdecca4e4fff3238  date_dim.parquet
8ae2e96e8c7c7205389e0ef7ce9868b35df9d550b54a9d9d9437792208ae7245  household_demographics.parquet
10cf9bb5668cbb4ea9156f43d8272f090893ded291107416a2b1820c207a32ea  income_band.parquet
f69b51f6fd0bc3aac0a6f8cb768f8a02cb3bcdab3fc442e36b6373f24a15ce56  inventory.parquet
7cf69201f5e2e87bfbd13a1a170fed53d24f646bb1d35390ac8b2a6a3b4bef0b  item.parquet
a86e1cd92ab09450b95ebd7785348e7e8af9b35959ffdc862c7b469f73a90d15  promotion.parquet
918408ad5c5543926344b75ef7d6cfe26a1a94a39658ad964dcf4904e77643bb  reason.parquet
8cc825ac279f0cc0543e8309fda14e7df530bd7381def39a1743641aafb31d6b  ship_mode.parquet
f3011ba323476fa4a570b6d6603d19439418e6cd04506472133c0935fc7d0cac  store.parquet
e29f3dacb85b3d2f1a518aae9b59065698c031f9ec905033119b884443292ad3  store_returns.parquet
faec525af4350da7617753a44ee13c784a3ded428c6def82d652dc961000ca87  store_sales.parquet
de57b4949380e619190730a1e0d06ccb7308ecadd7869fd8bdb04fd38c90452a  time_dim.parquet
d66797eb05e0d97164002903db510e7035914e3ce338079c95cb875169326cea  warehouse.parquet
b7626aa6adac62d78df734b1ba25acb52732bde74dc0972e50af1a58f77d61c7  web_page.parquet
4426332ff54c04e804eda168947c4c8bab82743e7fc694e55cdd479c9d6c1071  web_returns.parquet
e1db88a04d8f808735d2cb57c6e3659fff1963b0696281bcad20786ea0ce2cff  web_sales.parquet
a07f1b70678847300ee61d5aa6e3968eab84c814dc03d367ed865cfd56926d46  web_site.parquet
```

