"""Each build image under `docker run --user 4242:4242 --network none`: a uid with no passwd entry
and HOME=/, which it cannot write. Rust, the venv, DuckDB's pre-installed extensions, the sf1
generator, ccache, gh and a configure that fetches what the C++ build fetches all work, offline.

Needs the images built (`scripts/env/image.sh build build <cudf>`), so it is not in the cheap tier.
"""
import pathlib
import re
import subprocess
import sys

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
UID = "4242:4242"


def pins():
    pairs = (line.partition("=") for line in (REPO / "docker" / "pins.env").read_text().splitlines()
             if line and not line.startswith("#"))
    return {name: value for name, _, value in pairs}


def flatbuffers_tag():
    text = (REPO / "cpp" / "CMakeLists.txt").read_text()
    (tag,) = re.findall(r"GIT_REPOSITORY https://github\.com/google/flatbuffers\.git\s+GIT_TAG\s+(\S+)", text)
    return tag


# A project fetching what cpp/CMakeLists.txt does, by the same routes: rapids-cmake's branch zip
# (FetchContent), flatbuffers' tag (a FetchContent git clone, populated without being built) and
# gtest through rapids-cmake's CPM.
PROJECT = """cmake_minimum_required(VERSION 3.30.4)
include(FetchContent)
FetchContent_Declare(rapids-cmake URL https://github.com/rapidsai/rapids-cmake/archive/refs/heads/@BRANCH@.zip)
FetchContent_MakeAvailable(rapids-cmake)
include(rapids-cmake)
include(rapids-cpm)
rapids_cpm_init()
project(t LANGUAGES CXX)
FetchContent_Declare(flatbuffers GIT_REPOSITORY https://github.com/google/flatbuffers.git
  GIT_TAG @TAG@ GIT_SHALLOW TRUE SOURCE_SUBDIR not-built)
FetchContent_MakeAvailable(flatbuffers)
include(${rapids-cmake-dir}/cpm/gtest.cmake)
rapids_cpm_gtest(BUILD_STATIC)
"""

CHECK = r"""
set -euo pipefail
ok() { echo "OK $1"; }
[ "$(id -u)" = 4242 ] && [ ! -w "$HOME" ] && ok "uid 4242, HOME $HOME not writable"
cd /tmp
cargo new -q --vcs none /tmp/probe && (cd /tmp/probe && cargo build -q --offline) && [ -w "$CARGO_HOME" ] && ok cargo
[ "$(rustc --version | cut -d' ' -f2)" = "$RUST_VERSION" ] && ok rustc
[ "$(python3 -c 'import numpy, pandas, pyarrow, duckdb, matplotlib, sys
print(sys.version.split()[0], numpy.__version__, pandas.__version__, pyarrow.__version__, duckdb.__version__, matplotlib.__version__)')" \
  = "$PYTHON_VERSION $NUMPY_VERSION $PANDAS_VERSION $PYARROW_VERSION $PY_DUCKDB_VERSION $MATPLOTLIB_VERSION" ] && ok python
[ "$(duckdb :memory: -csv -noheader -c "SET extension_directory='$PEACOCK_DUCKDB_EXTENSION_DIR'; INSTALL tpch; LOAD tpch; CALL dbgen(sf=0.01); SELECT count(*) FROM lineitem;" | tail -1)" = 60175 ] && ok duckdb
mkdir /tmp/gen && cp /generate_testdata.sh /tmp/gen/ && /tmp/gen/generate_testdata.sh --bench tpch --sf 0.01 >/dev/null \
  && [ -s /tmp/gen/tpch.sf0.01/lineitem.parquet ] && ok generate_testdata
echo 'int main(void) { return 0; }' > /tmp/m.c && ccache gcc -c /tmp/m.c -o /tmp/m.o && ok ccache
gh --version >/dev/null && ok gh
mkdir -p /tmp/tree/cpp /tmp/tree/third_party/cudf && cp /opt/prefetch/rapids-cmake.branch /tmp/tree/third_party/cudf/RAPIDS_BRANCH
sed "s/@BRANCH@/$(cat /opt/prefetch/rapids-cmake.branch)/; s/@TAG@/$FLATBUFFERS_TAG/" /tmp/project.cmake > /tmp/tree/cpp/CMakeLists.txt
cmake -S /tmp/tree/cpp -B /tmp/tree/build > /tmp/configure.log 2>&1 || { tail -20 /tmp/configure.log; exit 1; }
grep -q "^FETCHCONTENT_SOURCE_DIR_RAPIDS-CMAKE:.*=/opt/prefetch/rapids-cmake$" /tmp/tree/build/CMakeCache.txt \
  && grep -q "at /opt/prefetch/cpm/gtest/" /tmp/configure.log && [ -f /tmp/tree/build/_deps/flatbuffers-src/CMakeLists.txt ] \
  && ok configure
"""


def check(cudf):
    image = f"ghcr.io/asymptote-tech/peacockdb-build:cudf-{cudf}"
    have = subprocess.run(["docker", "image", "inspect", image], capture_output=True, timeout=60)
    assert have.returncode == 0, f"no image {image}: scripts/env/image.sh build build {cudf}"
    env = [f"-e{k}={v}" for k, v in pins().items()] + [f"-eFLATBUFFERS_TAG={flatbuffers_tag()}"]
    proc = subprocess.run(
        ["docker", "run", "--rm", "-i", "--network", "none", "--user", UID, *env,
         "-v", f"{REPO / 'testdata' / 'generate_testdata.sh'}:/generate_testdata.sh:ro",
         image, "bash", "-c", "cat > /tmp/project.cmake\n" + CHECK],
        input=PROJECT, capture_output=True, text=True, timeout=900)
    passed = [line[3:] for line in proc.stdout.splitlines() if line.startswith("OK ")]
    want = ["uid 4242, HOME / not writable", "cargo", "rustc", "python", "duckdb", "generate_testdata",
            "ccache", "gh", "configure"]
    assert proc.returncode == 0 and passed == want, f"passed {passed}\n{proc.stdout[-2000:]}\n{proc.stderr[-2000:]}"


def test_the_25_02_image_runs_as_any_user():
    check("25.02")


def test_the_26_02_image_runs_as_any_user():
    check("26.02")


if __name__ == "__main__":
    raise SystemExit(main(globals()))
