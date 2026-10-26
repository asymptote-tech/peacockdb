"""no-static-cudart.sh over shared libraries built here with gcc: one that only calls the CUDA
runtime passes, one that defines a runtime symbol (a static cudart linked in) fails naming it."""
import pathlib
import subprocess
import sys
import tempfile

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "no-static-cudart.sh"


def shared_library(directory, name, source):
    c = pathlib.Path(directory) / f"{name}.c"
    c.write_text(source)
    lib = pathlib.Path(directory) / f"lib{name}.so"
    subprocess.run(["gcc", "-shared", "-fPIC", "-o", str(lib), str(c)], check=True, timeout=60)
    return lib


def run(*args):
    return subprocess.run([str(SCRIPT), *map(str, args)], capture_output=True, text=True, timeout=60)


def test_a_library_that_only_calls_the_runtime_passes():
    with tempfile.TemporaryDirectory() as tmp:
        lib = shared_library(tmp, "caller", "int cudaMalloc(void **, unsigned long);\n"
                                            "int peacock_call(void **p) { return cudaMalloc(p, 8); }\n")
        r = run(lib)
        assert r.returncode == 0 and r.stderr == "", r


def test_a_library_defining_runtime_symbols_fails_naming_them():
    with tempfile.TemporaryDirectory() as tmp:
        lib = shared_library(tmp, "static", "int cudaStreamGetDevice(void *s, int *d) { return 0; }\n"
                                            "void __cudaRegisterFatBinary(void) {}\n"
                                            "int peacock_call(void) { return 0; }\n")
        r = run(lib)
        assert r.returncode == 1, r
        assert len(r.stderr.strip().splitlines()) == 1, r.stderr
        assert str(lib) in r.stderr and "cudaStreamGetDevice" in r.stderr and "__cudaRegisterFatBinary" in r.stderr
        assert "peacock_call" not in r.stderr, r.stderr


def test_every_library_is_checked():
    with tempfile.TemporaryDirectory() as tmp:
        bad = shared_library(tmp, "static", "int cudaGetDriverEntryPointByVersion(void) { return 0; }\n")
        good = shared_library(tmp, "caller", "int peacock_call(void) { return 0; }\n")
        r = run(bad, good, bad)
        assert r.returncode == 1 and len(r.stderr.strip().splitlines()) == 2, r


def test_a_missing_library_is_refused_before_any_check():
    with tempfile.TemporaryDirectory() as tmp:
        good = shared_library(tmp, "caller", "int peacock_call(void) { return 0; }\n")
        r = run(good, pathlib.Path(tmp) / "absent.so")
        assert r.returncode == 2 and r.stderr.startswith("no-static-cudart.sh: ") and "absent.so" in r.stderr, r
        assert len(r.stderr.strip().splitlines()) == 1, r.stderr


def test_no_argument_is_refused():
    r = run()
    assert r.returncode == 2 and r.stderr.startswith("no-static-cudart.sh: "), r


def test_help():
    r = run("--help")
    assert r.returncode == 0 and "no-static-cudart.sh <lib.so>" in r.stdout, r


if __name__ == "__main__":
    raise SystemExit(main(globals()))
