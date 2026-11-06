"""The run image: the runtime subset of each committed conda spec, the driver minimum it records,
and, in the built image, that every library the shipped binaries need resolves.

The spec and pin tests are cheap. The rest need the run images (`scripts/env/image.sh build run
<cudf>`) and a staged install per version: `PEACOCK_STAGED_INSTALLS` names a directory holding
`cudf-<v>/`, each a `cpp/install` with `rust-tests/` beside `bin/` and `lib/`, built in that
version's build image.
"""
import os
import pathlib
import re
import subprocess
import sys

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
RUNTIME_SPEC = REPO / "docker" / "runtime-spec.sh"
VERSIONS = ["25.02", "26.02"]
# What run-tests.sh hands the binaries, and nothing else gets.
LIBRARY_PATH = "/peacock/install/lib:/opt/cudf/lib"
STAGED_RUST = {"peacockdb_core_gpu_lib", "test_gpu_corpus", "test_node_timing", "peacock_gpu_benchmarks"}
# The toolkit driver of the CUDA release each cudart ships in: the release notes' component table
# names the cudart (Table 1), and "CUDA Toolkit and Corresponding Driver Versions" its Linux x86_64
# driver (Table 3). 12.2.140 is CUDA 12.2 Update 2, 12.9.79 is 12.9 Update 1.
TOOLKIT_DRIVER = {"12.2.140": "535.104.05", "12.9.79": "575.57.08"}


def pins():
    pairs = (line.partition("=") for line in (REPO / "docker" / "pins.env").read_text().splitlines()
             if line and not line.startswith("#"))
    return {name: value for name, _, value in pairs}


def package(line):
    """(name, version, build) of an explicit spec's URL line."""
    base = re.sub(r"\.(conda|tar\.bz2)$", "", line.split("#")[0].rsplit("/", 1)[1])
    name, version, build = base.rsplit("-", 2)
    return name, version, build


def runtime_spec(cudf):
    proc = subprocess.run([str(RUNTIME_SPEC), str(REPO / "docker" / "conda" / f"cudf-{cudf}.spec")],
                          capture_output=True, text=True, timeout=60)
    assert proc.returncode == 0, proc.stderr
    return proc.stdout


BUILD_ONLY = re.compile(r"^(cuda-nvcc.*|cuda-crt.*|cuda-nvvm.*|ld_impl_linux-64|cuda-cccl.*|cuda-profiler-api"
                        r"|.*-dev|.*-dev_linux-64|.*-static|.*-static_linux-64|.*-headers|.*-headers-only"
                        r"|nlohmann_json|dlpack|python|python_abi)$")
RUNTIME = ["libcudf", "librmm", "libarrow", "libparquet", "cuda-cudart", "cuda-cudart_linux-64", "cuda-nvrtc",
           "libnvjitlink", "libkvikio", "libcufile", "libcuvs", "libraft", "libstdcxx", "tzdata"]


def check_runtime_spec(cudf):
    spec = (REPO / "docker" / "conda" / f"cudf-{cudf}.spec").read_text().splitlines()
    out = runtime_spec(cudf).splitlines()
    header = spec[:spec.index("@EXPLICIT") + 1]
    assert out[:len(header)] == header, "the header through @EXPLICIT is not the spec's"
    kept = out[len(header):]
    assert kept and set(kept) <= set(spec), "a kept line is not the spec's, byte for byte"
    names = {package(line)[0]: package(line)[2] for line in kept}
    dropped = [n for n in names if BUILD_ONLY.match(n)]
    assert not dropped, f"build-only packages kept: {dropped}"
    python = [n for n, build in names.items() if re.search(r"(^|_)py(h|\d)", build)]
    assert not python, f"Python packages kept: {python}"
    missing = [n for n in RUNTIME if n not in names]
    assert not missing, f"runtime packages dropped: {missing}"
    return names


def test_the_25_02_runtime_spec_holds_the_libraries_and_nothing_to_build_with():
    names = check_runtime_spec("25.02")
    assert "nvcomp" in names


def test_the_26_02_runtime_spec_holds_the_libraries_and_nothing_to_build_with():
    names = check_runtime_spec("26.02")
    assert "libnvcomp" in names and "rapids-logger" in names


def test_the_runtime_spec_refuses_a_file_with_no_packages():
    proc = subprocess.run([str(RUNTIME_SPEC), "/dev/null"], capture_output=True, text=True, timeout=60)
    assert proc.returncode == 2 and proc.stdout == "", proc
    assert proc.stderr.startswith("runtime-spec.sh: ") and len(proc.stderr.strip().splitlines()) == 1, proc.stderr


def cudart(cudf):
    lines = (REPO / "docker" / "conda" / f"cudf-{cudf}.spec").read_text().splitlines()
    (version,) = [package(line)[1] for line in lines if line.startswith("https://") and package(line)[0] == "cuda-cudart"]
    return version


def driver_pin(cudf):
    return pins()[f"DRIVER_MINIMUM_{cudf.replace('.', '_')}"]


def test_each_driver_minimum_is_the_toolkit_driver_of_its_spec_s_cudart():
    for cudf in VERSIONS:
        version = cudart(cudf)
        assert version in TOOLKIT_DRIVER, f"cuda-cudart {version} in cudf-{cudf}.spec: add its toolkit driver to the table"
        assert driver_pin(cudf) == TOOLKIT_DRIVER[version], (cudf, driver_pin(cudf))


def image(cudf):
    name = f"ghcr.io/asymptote-tech/peacockdb-run:cudf-{cudf}"
    have = subprocess.run(["docker", "image", "inspect", name], capture_output=True, timeout=60)
    assert have.returncode == 0, f"no image {name}: scripts/env/image.sh build run {cudf}"
    return name


def staged_install(cudf):
    root = os.environ.get("PEACOCK_STAGED_INSTALLS")
    assert root, "set PEACOCK_STAGED_INSTALLS to a directory holding cudf-<v>/, each a staged cpp/install"
    install = (pathlib.Path(root) / f"cudf-{cudf}").resolve()
    rust = {p.name for p in (install / "rust-tests").iterdir()} if (install / "rust-tests").is_dir() else set()
    assert rust == STAGED_RUST, f"{install}/rust-tests holds {sorted(rust)}, want {sorted(STAGED_RUST)}"
    assert (install / "lib" / "libpeacock_gpu.so").is_file(), f"no {install}/lib/libpeacock_gpu.so"
    assert list((install / "bin").glob("peacock_*_tests")), f"no {install}/bin/peacock_*_tests"
    return install


def ldd_report(cudf):
    """`ldd` of every shipped binary in the run image, offline, as {file: [unresolved lines]}."""
    install = staged_install(cudf)
    files = (["lib/libpeacock_gpu.so"] + sorted(f"bin/{p.name}" for p in (install / "bin").glob("peacock_*_tests"))
             + sorted(f"rust-tests/{n}" for n in STAGED_RUST))
    proc = subprocess.run(
        ["docker", "run", "--rm", "--network", "none", "--entrypoint", "ldd", "-e", f"LD_LIBRARY_PATH={LIBRARY_PATH}",
         "-v", f"{install}:/peacock/install:ro", image(cudf), *(f"/peacock/install/{f}" for f in files)],
        capture_output=True, text=True, timeout=300)
    assert proc.returncode == 0, proc.stderr
    report, current = {}, None
    for line in proc.stdout.splitlines():
        if not line.startswith("\t"):
            current = line.rstrip(":").removeprefix("/peacock/install/")
            report[current] = []
        elif "not found" in line:
            report[current].append(line.strip())
    assert sorted(report) == sorted(files), f"ldd reported {sorted(report)}"
    return report


def check_libraries_resolve(cudf):
    install = staged_install(cudf)
    static = subprocess.run([str(REPO / "scripts" / "env" / "no-static-cudart.sh"), str(install / "lib" / "libpeacock_gpu.so")],
                            capture_output=True, text=True, timeout=120)
    assert static.returncode == 0, static.stderr
    unresolved = {f: lines for f, lines in ldd_report(cudf).items() if lines}
    assert not unresolved, f"unresolved in the {cudf} run image: {unresolved}"


def test_every_needed_library_resolves_25_02():
    check_libraries_resolve("25.02")


def test_every_needed_library_resolves_26_02():
    check_libraries_resolve("26.02")


def inspect(cudf, fmt):
    proc = subprocess.run(["docker", "image", "inspect", "--format", fmt, image(cudf)],
                          capture_output=True, text=True, timeout=60)
    assert proc.returncode == 0, proc.stderr
    return proc.stdout.strip()


def test_each_image_records_its_driver_minimum():
    for cudf in VERSIONS:
        label = inspect(cudf, '{{index .Config.Labels "peacockdb.driver-minimum"}}')
        assert label == driver_pin(cudf), (cudf, label)


def test_each_image_runs_the_list_as_a_non_root_user_with_no_global_library_path():
    for cudf in VERSIONS:
        assert inspect(cudf, "{{.Config.User}}") == "peacock", cudf
        assert inspect(cudf, "{{json .Config.Entrypoint}}") == '["/opt/run/run-tests.sh"]', cudf
        env = inspect(cudf, "{{json .Config.Env}}")
        assert "LD_LIBRARY_PATH" not in env, (cudf, env)


NOTHING_TO_BUILD_WITH = r"""
set -u
for tool in gcc g++ cc c++ nvcc cmake ninja make ld python python3 cargo rustc; do
  command -v "$tool" >/dev/null && echo "TOOL $tool"
done
ls /opt/cudf/bin/python* /opt/cudf/bin/nvcc 2>/dev/null | sed 's/^/FILE /'
find /opt/cudf -name '*.a' | sed 's/^/STATIC /'
ls /opt/cudf/conda-meta | grep -E '^(python|cuda-nvcc|.*-dev)-[0-9]' | sed 's/^/PKG /'
[ "$(id -u)" = 1000 ] && echo "UID 1000"
"""


def test_each_image_holds_nothing_to_build_with():
    for cudf in VERSIONS:
        proc = subprocess.run(["docker", "run", "--rm", "--network", "none", "--entrypoint", "bash", image(cudf),
                               "-c", NOTHING_TO_BUILD_WITH], capture_output=True, text=True, timeout=120)
        assert proc.stdout.splitlines() == ["UID 1000"], (cudf, proc.stdout, proc.stderr)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
