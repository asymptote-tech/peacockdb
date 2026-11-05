"""stage-tree.sh: the tree gpu.sh ships, built and staged in an instance as build-test-shadgpu.sh stages it.

Each test runs the script from a copy in a scratch tree whose scripts/build.sh, cargo and strip are
fakes that record their argv, so nothing compiles; the lists are read from the scripts' text.
"""
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
SCRIPT = REPO / "scripts/env/stage-tree.sh"
IMAGE_ENV = {"CUDF_ROOT": "/opt/cudf", "GCC_VERSION": "12"}

FAKE_BUILD = r'''#!/bin/sh
printf '%s\n' "$*" >> "$FAKE_LOG.build"
[ "${FAKE_BUILD_EXIT:-0}" = 0 ] || exit "$FAKE_BUILD_EXIT"
mkdir -p cpp/install/lib cpp/install/bin
: > cpp/install/lib/libpeacock_gpu.so
: > cpp/install/bin/peacock_gpu_tests
'''

FAKE_CARGO = r'''#!/usr/bin/env python3
import json, os, pathlib, sys
a = sys.argv[1:]
with open(os.environ["FAKE_LOG"] + ".cargo", "a") as f:
    f.write(json.dumps(a) + "\n")
if os.environ.get("FAKE_CARGO_EXIT"):
    print("error: could not compile", file=sys.stderr)
    sys.exit(int(os.environ["FAKE_CARGO_EXIT"]))
name = a[a.index("--test") + 1] if "--test" in a else "peacockdb_core"
kind = ["test"] if "--test" in a else ["lib"]
exe = pathlib.Path(os.environ["FAKE_TARGET"]) / f"{name}-{'release' if '--release' in a else 'debug'}"
exe.parent.mkdir(parents=True, exist_ok=True)
exe.write_text(" ".join(a))
print(json.dumps({"reason": "compiler-artifact", "target": {"name": "peacockdb_core", "kind": ["lib"]},
                  "executable": None}))
if name not in os.environ.get("FAKE_CARGO_NONE", "").split():
    print(json.dumps({"reason": "compiler-artifact", "target": {"name": name, "kind": kind},
                      "executable": str(exe)}))
print(json.dumps({"reason": "build-finished", "success": True}))
'''

FAKE_STRIP = r'''#!/bin/sh
printf '%s\n' "$*" >> "$FAKE_LOG.strip"
'''


def listed(path, name):
    """The value of `name=` in a script, a bash array's words or a scalar."""
    text = path.read_text()
    m = re.search(rf"^{name}=\(([^)]*)\)", text, re.M)
    if m:
        return m.group(1).split()
    m = re.search(rf"^{name}=(\S+)", text, re.M)
    assert m, (path, name)
    return m.group(1)


class Tree:
    def __enter__(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = pathlib.Path(self.tmp.name)
        self.tree = root / "tree"
        (self.tree / "scripts/env").mkdir(parents=True)
        shutil.copy2(SCRIPT, self.tree / "scripts/env/stage-tree.sh")
        for rel, text in (("scripts/build.sh", FAKE_BUILD),):
            (self.tree / rel).write_text(text)
            (self.tree / rel).chmod(0o755)
        self.bin = root / "bin"
        self.bin.mkdir()
        for name, text in (("cargo", FAKE_CARGO), ("strip", FAKE_STRIP)):
            (self.bin / name).write_text(text)
            (self.bin / name).chmod(0o755)
        self.log = root / "log"
        self.env = {"PATH": f"{self.bin}:{os.environ['PATH']}", "HOME": str(root), "LANG": "C.UTF-8",
                    "FAKE_LOG": str(self.log), "FAKE_TARGET": str(root / "target"), **IMAGE_ENV}
        return self

    def __exit__(self, *exc):
        self.tmp.cleanup()

    def run(self, *args, env=None, cwd=None):
        full = {k: v for k, v in {**self.env, **(env or {})}.items() if v is not None}
        return subprocess.run([str(self.tree / "scripts/env/stage-tree.sh"), *args], env=full,
                              cwd=cwd or self.tree, capture_output=True, text=True, timeout=60)

    def calls(self, tool):
        path = pathlib.Path(f"{self.log}.{tool}")
        if not path.exists():
            return []
        lines = path.read_text().splitlines()
        return [json.loads(ln) for ln in lines] if tool == "cargo" else lines

    def staged(self, sub):
        d = self.tree / "cpp/install" / sub
        return sorted(p.name for p in d.iterdir()) if d.is_dir() else []


SHADGPU = REPO / "scripts/build-test-shadgpu.sh"
RUN_TESTS = REPO / "docker/run-tests.sh"


def test_its_lists_are_the_ones_build_test_shadgpu_and_run_tests_use():
    for name in ("RUST_TESTS", "RUST_LIB_STAGED", "RUST_TESTS_STAGING", "BENCH_TARGET", "BENCH_STAGING"):
        assert listed(SCRIPT, name) == listed(SHADGPU, name), name
    assert listed(SCRIPT, "RUST_LIB_STAGED") == listed(RUN_TESTS, "LIB_STAGED")
    assert listed(SCRIPT, "BENCH_TARGET") == listed(RUN_TESTS, "BENCH_TARGET")


def test_it_stages_and_strips_the_device_rung():
    with Tree() as t:
        r = t.run(cwd=t.tree / "scripts")
        assert r.returncode == 0, r
        assert t.calls("build") == ["--cudf_ROOT /opt/cudf --gcc-version 12 --all"], t.calls("build")
        names = listed(SHADGPU, "RUST_TESTS") + [listed(SHADGPU, "RUST_LIB_STAGED")]
        assert t.staged("rust-tests") == sorted(names) and t.staged("rust-benchmarks") == [], t.staged("rust-tests")
        for argv in t.calls("cargo"):
            assert argv[:4] == ["test", "--no-run", "-p", "peacockdb-core"] and "--features" in argv, argv
            assert argv[argv.index("--features") + 1] == "gpu" and "--release" not in argv, argv
        selectors = sorted(a[a.index("--test") + 1] if "--test" in a else "--lib" for a in t.calls("cargo"))
        assert selectors == sorted(listed(SHADGPU, "RUST_TESTS") + ["--lib"]), selectors
        lib = t.tree / "cpp/install/rust-tests" / listed(SHADGPU, "RUST_LIB_STAGED")
        assert "--lib" in lib.read_text(), lib.read_text()
        stripped = sorted(t.calls("strip"))
        assert stripped == sorted(f"--strip-debug cpp/install/rust-tests/{n}" for n in names), stripped


def test_bench_adds_the_release_harness_unstripped():
    with Tree() as t:
        r = t.run("--bench")
        assert r.returncode == 0, r
        assert t.staged("rust-benchmarks") == [listed(SHADGPU, "BENCH_TARGET")], t.staged("rust-benchmarks")
        release = [a for a in t.calls("cargo") if "--release" in a]
        assert len(release) == 1 and "--features" not in release[0], release
        assert release[0][release[0].index("--test") + 1] == listed(SHADGPU, "BENCH_TARGET"), release
        assert not [s for s in t.calls("strip") if "rust-benchmarks" in s], t.calls("strip")


def test_a_failed_build_leaves_nothing_to_ship():
    with Tree() as t:
        old = t.tree / "cpp/install/lib/libpeacock_gpu.so"
        old.parent.mkdir(parents=True)
        old.write_text("stale")
        (t.tree / "cpp/install/rust-tests").mkdir()
        (t.tree / "cpp/install/rust-tests/test_gpu_corpus").write_text("stale")
        r = t.run(env={"FAKE_BUILD_EXIT": "1"})
        assert r.returncode != 0, r
        assert not old.exists() and t.staged("rust-tests") == [], t.staged("rust-tests")
        assert t.calls("cargo") == []


def test_a_target_cargo_names_no_executable_stops_the_staging():
    with Tree() as t:
        r = t.run(env={"FAKE_CARGO_NONE": "test_node_timing"})
        assert r.returncode == 1 and "test_node_timing" in r.stderr, r
        assert listed(SHADGPU, "RUST_LIB_STAGED") not in t.staged("rust-tests"), t.staged("rust-tests")
        assert t.calls("strip") == []


def test_a_failed_compile_stops_the_staging():
    with Tree() as t:
        r = t.run(env={"FAKE_CARGO_EXIT": "101"})
        assert r.returncode != 0 and t.staged("rust-tests") == [], r
        assert len(t.calls("cargo")) == 1


def test_refused_outside_the_build_image_and_on_a_bad_argument():
    for args, env, word in (((), {"CUDF_ROOT": None}, "CUDF_ROOT"), ((), {"GCC_VERSION": None}, "GCC_VERSION"),
                            (("--bogus",), {}, "--bogus"), (("stray",), {}, "stray")):
        with Tree() as t:
            r = t.run(*args, env=env)
            assert r.returncode == 2 and word in r.stderr and r.stderr.startswith("stage-tree.sh: "), (args, r)
            assert len(r.stderr.strip().splitlines()) == 1, r.stderr
            assert t.calls("build") == [] and t.calls("cargo") == [], args


def test_stage_tree_has_help():
    with Tree() as t:
        r = t.run("--help")
        assert r.returncode == 0 and "stage-tree.sh [--bench]" in r.stdout, r
        assert t.calls("build") == []


if __name__ == "__main__":
    raise SystemExit(main(globals()))
