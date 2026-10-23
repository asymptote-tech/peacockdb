# check=skip=InvalidDefaultArgInFrom
# The build image for one cuDF version: every build mode (rust-only, the FFI build, --features gpu,
# the staged test and benchmark binaries) and the agents that drive them. Built by
# scripts/env/image.sh, which passes every docker/pins.env line as a build arg; decisions and
# sizes are in llm-wiki/tasks/containers-detail.md. No secret, host, user home or provider is in
# any layer: the image is public, which is also why Claude Code is not in it (the machine's install
# is mounted). It runs as any uid: nothing is found through HOME, and what a tool writes is
# world-writable in the container's own layer or under /tmp.
# No default: without --build-arg CUDF the FROM below names no stage and the build stops (line 1).
ARG CUDF

# The gcc each cuDF version is built with, chosen by stage so the rest of the file reads one name.
FROM ubuntu:24.04 AS cudf-25.02
ARG GCC_25_02
ENV GCC_VERSION=${GCC_25_02}

FROM ubuntu:24.04 AS cudf-26.02
ARG GCC_26_02
ENV GCC_VERSION=${GCC_26_02}

FROM cudf-${CUDF} AS toolchain
ARG CUDF
ARG NVCC_VERSION
ARG MICROMAMBA_VERSION
ARG MICROMAMBA_SHA256
ARG CMAKE_VERSION
ARG CMAKE_SHA256
ARG NINJA_VERSION
ARG NINJA_SHA256
SHELL ["/bin/bash", "-euo", "pipefail", "-c"]
ENV DEBIAN_FRONTEND=noninteractive LANG=C.UTF-8

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      ca-certificates curl git unzip xz-utils zstd build-essential pkg-config \
      "gcc-${GCC_VERSION}" "g++-${GCC_VERSION}" ccache patchelf \
      openssh-client rsync jq less procps file \
 && rm -rf /var/lib/apt/lists/*

# The default user, so the image never runs as root unasked; `docker run --user` gives the machine
# user's uid instead. ubuntu:24.04's own `ubuntu` holds uid 1000.
RUN userdel -r ubuntu && useradd -m -u 1000 -s /bin/bash peacock

# /opt/cudf is the cuDF environment the committed device answers came from, package for package.
# Its Python (3.12, its own pandas) and its nvcc are never on PATH. /opt/nvcc is nvcc
# NVCC_VERSION with the headers and runtime it compiles against, taken line by line from the
# 26.02 spec: the nvcc both versions' device answers were built with. Its include directory is
# the toolkit's for CMake, so it also holds the math libraries' headers raft and cuVS include
# (the -dev packages); their libraries are linked from /opt/cudf, which CMAKE_PREFIX_PATH finds first.
COPY docker/conda/ /tmp/conda/
RUN nvcc_names='cuda-version|cuda-nvcc-impl|cuda-nvcc-tools|cuda-nvcc-dev_linux-64|cuda-crt-dev_linux-64|cuda-crt-tools|cuda-nvvm-dev_linux-64|cuda-nvvm-impl|cuda-nvvm-tools|cuda-cudart|cuda-cudart_linux-64|cuda-cudart-dev|cuda-cudart-dev_linux-64|cuda-cudart-static|cuda-cudart-static_linux-64|cuda-cccl_linux-64|libcublas-dev|libcurand-dev|libcusparse-dev|libcusolver-dev|libcufile-dev' \
 && { sed -n '1,/^@EXPLICIT$/p' /tmp/conda/cudf-26.02.spec; \
      grep -E "/(${nvcc_names})-[0-9][^/-]*-[^/-]+\.(conda|tar\.bz2)#" /tmp/conda/cudf-26.02.spec; } > /tmp/nvcc.spec \
 && [ "$(grep -c '^https://' /tmp/nvcc.spec)" -eq "$(tr '|' '\n' <<< "$nvcc_names" | wc -l)" ] \
 && grep -q "/cuda-nvcc-impl-${NVCC_VERSION}-" /tmp/nvcc.spec \
 && curl -fsSL -o /tmp/micromamba \
      "https://github.com/mamba-org/micromamba-releases/releases/download/${MICROMAMBA_VERSION}/micromamba-linux-64" \
 && echo "${MICROMAMBA_SHA256}  /tmp/micromamba" | sha256sum -c - \
 && chmod +x /tmp/micromamba \
 && export MAMBA_ROOT_PREFIX=/tmp/mamba \
 && /tmp/micromamba create -y --no-rc -p /opt/cudf -f "/tmp/conda/cudf-${CUDF}.spec" \
 && /tmp/micromamba create -y --no-rc -p /opt/nvcc -f /tmp/nvcc.spec \
 && rm -rf /tmp/mamba /tmp/micromamba /tmp/nvcc.spec /tmp/conda \
 && /opt/nvcc/bin/nvcc --version | grep -q "V${NVCC_VERSION}"

# cmake behind docker/cmake-wrapper.sh, which points a configure at the prefetched rapids-cmake
# and at the flatbuffers mirror.
COPY docker/cmake-wrapper.sh /usr/local/bin/cmake
RUN curl -fsSL -o /tmp/cmake.tar.gz \
      "https://github.com/Kitware/CMake/releases/download/v${CMAKE_VERSION}/cmake-${CMAKE_VERSION}-linux-x86_64.tar.gz" \
 && echo "${CMAKE_SHA256}  /tmp/cmake.tar.gz" | sha256sum -c - \
 && mkdir /opt/cmake && tar -xzf /tmp/cmake.tar.gz -C /opt/cmake --strip-components=1 && rm /tmp/cmake.tar.gz \
 && ln -s /opt/cmake/bin/ctest /opt/cmake/bin/cpack /usr/local/bin/ \
 && curl -fsSL -o /tmp/ninja.zip "https://github.com/ninja-build/ninja/releases/download/v${NINJA_VERSION}/ninja-linux.zip" \
 && echo "${NINJA_SHA256}  /tmp/ninja.zip" | sha256sum -c - \
 && unzip -q /tmp/ninja.zip -d /usr/local/bin && rm /tmp/ninja.zip \
 && [ "$(cmake --version | head -1)" = "cmake version ${CMAKE_VERSION}" ] \
 && [ "$(ninja --version)" = "${NINJA_VERSION}" ]

ENV PATH=/opt/nvcc/bin:${PATH} \
    CUDACXX=/opt/nvcc/bin/nvcc \
    CUDF_ROOT=/opt/cudf \
    CC=gcc-${GCC_VERSION} \
    CXX=g++-${GCC_VERSION}

# One configure of the C++ build against /opt/cudf, kept for what it fetched: the CPM cache
# (gtest, CPM itself), rapids-cmake and the flatbuffers tag. CPM takes a lock in each package's
# directory of the cache, so those directories and their lock files are writable by any uid. third_party/cudf comes at the commit
# the tree's index pins, as far as the configure reads it. The flatbuffers mirror holds the one
# tag cpp/CMakeLists.txt names, so a configure asking for another fails naming it.
FROM toolchain AS prefetch
ARG CUDF_SUBMODULE
COPY cpp/ /src/cpp/
COPY flatbuffers/ /src/flatbuffers/
COPY scripts/build.sh /src/scripts/build.sh
WORKDIR /src
RUN git init -q third_party/cudf \
 && git -C third_party/cudf fetch -q --depth 1 --filter=blob:none https://github.com/rapidsai/cudf.git "${CUDF_SUBMODULE}" \
 && git -C third_party/cudf sparse-checkout set --no-cone /cmake/ /cpp/cmake/ /VERSION /RAPIDS_BRANCH \
 && git -C third_party/cudf checkout -q FETCH_HEAD \
 && tag=$(awk '/google\/flatbuffers\.git/ { repo = 1 } repo && $1 == "GIT_TAG" { print $2; exit }' cpp/CMakeLists.txt) \
 && [ -n "$tag" ] \
 && git clone -q --bare --depth 1 --branch "$tag" https://github.com/google/flatbuffers.git /opt/prefetch/flatbuffers.git \
 && CPM_SOURCE_CACHE=/opt/prefetch/cpm ./scripts/build.sh --configure --cudf_ROOT /opt/cudf --gcc-version "${GCC_VERSION}" \
 && cp -a cpp/build/_deps/rapids-cmake-src /opt/prefetch/rapids-cmake \
 && cp third_party/cudf/RAPIDS_BRANCH /opt/prefetch/rapids-cmake.branch \
 && find /opt/prefetch/cpm -mindepth 1 -maxdepth 1 -type d -exec chmod a+rwx {} + \
 && find /opt/prefetch/cpm -mindepth 2 -maxdepth 2 -name cmake.lock -exec chmod a+rw {} +

FROM toolchain
ARG DUCKDB_VERSION
ARG DUCKDB_SHA256
ARG PYTHON_VERSION
ARG PYTHON_STANDALONE_TAG
ARG PYTHON_SHA256
ARG PANDAS_VERSION
ARG PYARROW_VERSION
ARG NUMPY_VERSION
ARG MATPLOTLIB_VERSION
ARG PY_DUCKDB_VERSION
ARG RUST_VERSION
ARG GH_VERSION
ARG GH_SHA256
ARG AWS_CLI_VERSION
ARG AWS_CLI_SHA256
ARG NEBIUS_VERSION
ARG NEBIUS_SHA256

# DuckDB and its tpch/tpcds extensions: INSTALL finds them installed and fetches nothing, once
# the extension directory is set to PEACOCK_DUCKDB_EXTENSION_DIR, as testdata/generate_testdata.sh
# does. DuckDB reads no environment variable for it, and its default is under the home.
RUN curl -fsSL -o /tmp/duckdb.zip \
      "https://github.com/duckdb/duckdb/releases/download/v${DUCKDB_VERSION}/duckdb_cli-linux-amd64.zip" \
 && echo "${DUCKDB_SHA256}  /tmp/duckdb.zip" | sha256sum -c - \
 && unzip -q /tmp/duckdb.zip -d /usr/local/bin && rm /tmp/duckdb.zip \
 && duckdb :memory: -c "SET extension_directory='/opt/duckdb-ext'; INSTALL tpch; INSTALL tpcds;" \
 && [ "$(duckdb --version | cut -d' ' -f1)" = "v${DUCKDB_VERSION}" ]

# Python for scripts/exec_model and the calibration scripts: the committed costs depend on
# these versions' bytes, so the venv is docker/venv.lock exactly. It is root's, so nothing at run
# time moves a pin.
COPY docker/venv.lock /tmp/venv.lock
RUN curl -fsSL -o /tmp/python.tar.gz \
      "https://github.com/astral-sh/python-build-standalone/releases/download/${PYTHON_STANDALONE_TAG}/cpython-${PYTHON_VERSION}%2B${PYTHON_STANDALONE_TAG}-x86_64-unknown-linux-gnu-install_only.tar.gz" \
 && echo "${PYTHON_SHA256}  /tmp/python.tar.gz" | sha256sum -c - \
 && mkdir /opt/python && tar -xzf /tmp/python.tar.gz -C /opt/python --strip-components=1 && rm /tmp/python.tar.gz \
 && /opt/python/bin/python3 -m venv /opt/venv \
 && /opt/venv/bin/pip install --no-cache-dir -q --require-hashes --no-deps -r /tmp/venv.lock \
 && /opt/venv/bin/pip freeze > /opt/venv/freeze.txt \
 && diff /opt/venv/freeze.txt <(sed -n 's/ --hash=.*//p' /tmp/venv.lock) \
 && rm /tmp/venv.lock \
 && [ "$(/opt/venv/bin/python -c 'import numpy, pandas, pyarrow, matplotlib, duckdb as d; \
      print(numpy.__version__, pandas.__version__, pyarrow.__version__, matplotlib.__version__, d.__version__)')" \
      = "${NUMPY_VERSION} ${PANDAS_VERSION} ${PYARROW_VERSION} ${MATPLOTLIB_VERSION} ${PY_DUCKDB_VERSION}" ] \
 && [ "$(/opt/venv/bin/python --version)" = "Python ${PYTHON_VERSION}" ]

# The toolchain is read-only to its users. Any uid may write CARGO_HOME itself (cargo's package
# lock and caches) and its registry/ and git/; bin/ stays root's.
ENV RUSTUP_HOME=/opt/rust/rustup CARGO_HOME=/opt/rust/cargo
RUN curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs \
      | sh -s -- -y -q --no-modify-path --profile minimal --default-toolchain "${RUST_VERSION}" -c rustfmt -c clippy \
 && chmod -R a+rX /opt/rust \
 && mkdir /opt/rust/cargo/registry /opt/rust/cargo/git \
 && chmod a+rwx /opt/rust/cargo /opt/rust/cargo/registry /opt/rust/cargo/git \
 && [ "$(/opt/rust/cargo/bin/rustc --version | cut -d' ' -f2)" = "${RUST_VERSION}" ]

# gh, aws and nebius as tools only: no config, profile or credential; gh's device id is removed.
RUN curl -fsSL -o /tmp/gh.tar.gz \
      "https://github.com/cli/cli/releases/download/v${GH_VERSION}/gh_${GH_VERSION}_linux_amd64.tar.gz" \
 && echo "${GH_SHA256}  /tmp/gh.tar.gz" | sha256sum -c - \
 && tar -xzf /tmp/gh.tar.gz -C /usr/local --strip-components=1 --no-same-owner "gh_${GH_VERSION}_linux_amd64/bin/gh" \
 && rm /tmp/gh.tar.gz \
 && curl -fsSL -o /usr/local/bin/nebius \
      "https://artifacts.nebius.cloud/cli/release/${NEBIUS_VERSION}/linux/x86_64/nebius" \
 && echo "${NEBIUS_SHA256}  /usr/local/bin/nebius" | sha256sum -c - \
 && chmod 755 /usr/local/bin/nebius \
 && curl -fsSL -o /tmp/awscli.zip "https://awscli.amazonaws.com/awscli-exe-linux-x86_64-${AWS_CLI_VERSION}.zip" \
 && echo "${AWS_CLI_SHA256}  /tmp/awscli.zip" | sha256sum -c - \
 && unzip -q /tmp/awscli.zip -d /tmp && /tmp/aws/install -i /usr/local/aws-cli -b /usr/local/bin \
 && rm -rf /tmp/awscli.zip /tmp/aws \
 && gh --version | grep -q "gh version ${GH_VERSION} " \
 && aws --version | grep -q "^aws-cli/${AWS_CLI_VERSION} " \
 && [ "$(nebius version)" = "${NEBIUS_VERSION}" ] \
 && rm -rf /root/.local/state/gh

# The prefetched git repositories belong to root, and git refuses a repository another uid owns
# unless a protected config names it. The command-line scope the cmake wrapper could set is
# cleared for a local clone's source, so the names go in the system config: these paths only.
COPY --from=prefetch /opt/prefetch /opt/prefetch
RUN git config --system --add safe.directory /opt/prefetch/flatbuffers.git \
 && find /opt/prefetch/cpm -mindepth 3 -maxdepth 3 -name .git -printf '%h\n' \
      | xargs -r -n1 git config --system --add safe.directory \
 && [ "$(git config --system --get-all safe.directory | wc -l)" -ge 2 ]

ENV PATH=/opt/venv/bin:/opt/nvcc/bin:/opt/rust/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    CPM_SOURCE_CACHE=/opt/prefetch/cpm \
    PEACOCK_DUCKDB_EXTENSION_DIR=/opt/duckdb-ext \
    CCACHE_DIR=/tmp/ccache
USER peacock
WORKDIR /tmp
