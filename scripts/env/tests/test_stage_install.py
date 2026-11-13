"""`stage-install.sh`: the install a device run ships, built in the build image at the tree root.

It runs in a copy of the tree whose `scripts/build.sh` is a stub recording its argv. `cargo` is a
fake that records its argv and target dir and answers `test --no-run --message-format=json` with
an artifact line naming an executable it wrote, as cargo does; `strip` records its argv. The build
image's GCC_VERSION is set as the image sets it.
"""
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
TREE = ["scripts/env/stage-install.sh", "scripts/lib/stage-cargo.sh", "docker/pins.env",
        "docker/conda/cudf-25.02.spec", "docker/conda/cudf-26.02.spec"]
GPU_TESTS = ["test_gpu_corpus", "test_node_timing", "peacock_gpu_benchmarks"]

STUB_BUILD = '#!/bin/sh\nprintf "%s\\n" "$*" >> "$FAKE_LOG_DIR/build.log"\n'
FAKE_STRIP = '#!/bin/sh\nprintf "%s\\n" "$*" >> "$FAKE_LOG_DIR/strip.log"\n'
FAKE_CARGO = r'''#!/usr/bin/env python3
import json, os, pathlib, sys
a = sys.argv[1:]
log = pathlib.Path(os.environ["FAKE_LOG_DIR"]) / "cargo.log"
with open(log, "a") as f:
    f.write(json.dumps({"argv": a, "target": os.environ.get("CARGO_TARGET_DIR")}) + "\n")
if "--lib" in a:
    name, kind = "peacockdb_core", "lib"
else:
    name, kind = a[a.index("--test") + 1], "test"
if name in os.environ.get("FAKE_CARGO_FAILS", "").split():
    print(f"error: could not compile `{name}`", file=sys.stderr)
    sys.exit(101)
profile = "release" if "--release" in a else "debug"
features = a[a.index("--features") + 1] if "--features" in a else "default"
exe = pathlib.Path(os.environ["CARGO_TARGET_DIR"]) / profile / "deps" / f"{name}-0123abcd"
exe.parent.mkdir(parents=True, exist_ok=True)
exe.write_text(f"{name} {kind} {profile} {features}\n")
exe.chmod(0o755)
print(json.dumps({"reason": "compiler-artifact", "target": {"name": name, "kind": [kind]}, "executable": None}))
print(json.dumps({"reason": "compiler-artifact", "target": {"name": name, "kind": [kind]}, "executable": str(exe)}))
print(json.dumps({"reason": "build-finished", "success": True}))
'''


class Tree:
    def __init__(self, tmp):
        tmp = pathlib.Path(tmp)
        self.root = tmp / "tree"
        for rel in TREE:
            (self.root / rel).parent.mkdir(parents=True, exist_ok=True)
            if (REPO / rel).exists():
                shutil.copy2(REPO / rel, self.root / rel)
        (self.root / "scripts/build.sh").write_text(STUB_BUILD)
        (self.root / "scripts/build.sh").chmod(0o755)
        self.logs = tmp / "logs"
        self.logs.mkdir()
        self.bin = tmp / "bin"
        self.bin.mkdir()
        for name, text in (("cargo", FAKE_CARGO), ("strip", FAKE_STRIP)):
            (self.bin / name).write_text(text)
            (self.bin / name).chmod(0o755)
        self.env = {k: v for k, v in os.environ.items() if k not in ("CARGO_TARGET_DIR", "GCC_VERSION")}
        self.env.update(PATH=f"{self.bin}:{os.environ['PATH']}", FAKE_LOG_DIR=str(self.logs), GCC_VERSION="12")
        self.install = self.root / "cpp" / "install"

    def run(self, *args, env=None):
        return subprocess.run([str(self.root / "scripts/env/stage-install.sh"), *args], cwd=self.root,
                              env={**self.env, **(env or {})}, capture_output=True, text=True, timeout=60)

    def lines(self, name):
        p = self.logs / name
        return p.read_text().splitlines() if p.exists() else []

    def cargo(self):
        return [json.loads(ln) for ln in self.lines("cargo.log")]


def ok(r):
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def test_stage_install_stages_four_tests_and_a_release_bench():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        ok(t.run())
        assert t.lines("build.log") == ["--configure --build --install --cudf_ROOT /opt/cudf --gcc-version 12"], \
            t.lines("build.log")
        staged = {p.name: p.read_text().strip() for p in (t.install / "rust-tests").iterdir()}
        assert staged == {
            "test_gpu_corpus": "test_gpu_corpus test debug gpu",
            "test_node_timing": "test_node_timing test debug gpu",
            "peacock_gpu_benchmarks": "peacock_gpu_benchmarks test debug gpu",
            "peacockdb_core_gpu_lib": "peacockdb_core lib debug gpu",
        }, staged
        bench = {p.name: p.read_text().strip() for p in (t.install / "rust-benchmarks").iterdir()}
        assert bench["peacock_gpu_benchmarks"] == "peacock_gpu_benchmarks test release default", bench
        for call in t.cargo():
            a = call["argv"]
            assert a[:4] == ["test", "--no-run", "-p", "peacockdb-core"], a
            assert call["target"] == str(t.root / "target-img-25.02"), call
        stripped = sorted(t.lines("strip.log"))
        want = sorted([f"--strip-debug cpp/install/rust-tests/{n}" for n in GPU_TESTS + ["peacockdb_core_gpu_lib"]]
                      + [f"--strip-debug cpp/install/rust-benchmarks/{n}"
                         for n in ("peacock_gpu_benchmarks", "prototype_fused_benchmarks")])
        assert stripped == want, stripped


def test_stage_install_builds_the_prototype_target():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        ok(t.run())
        bench = {p.name: p.read_text().strip() for p in (t.install / "rust-benchmarks").iterdir()}
        assert bench == {
            "peacock_gpu_benchmarks": "peacock_gpu_benchmarks test release default",
            "prototype_fused_benchmarks": "prototype_fused_benchmarks test release gpu,test-support",
        }, bench
        assert "--strip-debug cpp/install/rust-benchmarks/prototype_fused_benchmarks" in t.lines("strip.log")


def test_stage_install_no_release_bench_builds_no_release_benchmark():
    # CI's staging: the device rung alone, as the L40S list runs it; no bench is run there.
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        (t.install / "rust-benchmarks").mkdir(parents=True)
        (t.install / "rust-benchmarks" / "peacock_gpu_benchmarks").write_text("an earlier build")
        r = ok(t.run("--no-release-bench"))
        staged = sorted(p.name for p in (t.install / "rust-tests").iterdir())
        assert staged == sorted(GPU_TESTS + ["peacockdb_core_gpu_lib"]), staged
        assert not (t.install / "rust-benchmarks").exists()
        assert not [c for c in t.cargo() if "--release" in c["argv"]], t.cargo()
        assert not [ln for ln in t.lines("strip.log") if "rust-benchmarks" in ln], t.lines("strip.log")
        assert "rust-benchmarks" not in r.stdout, r.stdout


def test_stage_install_stages_from_empty():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        for d in ("rust-tests", "rust-benchmarks"):
            (t.install / d).mkdir(parents=True)
            (t.install / d / "renamed_away").write_text("old")
        ok(t.run())
        assert not (t.install / "rust-tests/renamed_away").exists()
        assert not (t.install / "rust-benchmarks/renamed_away").exists()


def test_stage_install_keeps_a_given_target_dir():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        ok(t.run(env={"CARGO_TARGET_DIR": f"{tmp}/elsewhere"}))
        assert {c["target"] for c in t.cargo()} == {f"{tmp}/elsewhere"}


def test_stage_install_fails_naming_a_build_that_fails():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        r = t.run(env={"FAKE_CARGO_FAILS": "peacockdb_core"})
        assert r.returncode == 1, (r.returncode, r.stdout, r.stderr)
        assert "peacockdb_core_gpu_lib" in r.stderr, r.stderr
        assert not (t.install / "rust-benchmarks").exists()


def test_stage_install_refuses_outside_the_build_image_or_another_cudf():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        r = t.run(env={"GCC_VERSION": ""})
        assert r.returncode == 2 and "build image" in r.stderr, (r.returncode, r.stderr)
        r = t.run("--cudf", "26.02")
        assert r.returncode == 2 and "26.02" in r.stderr, (r.returncode, r.stderr)
        r = t.run("--cudf", "24.12")
        assert r.returncode == 2 and "24.12" in r.stderr, (r.returncode, r.stderr)
        r = t.run("--release")
        assert r.returncode == 2 and "--release" in r.stderr, (r.returncode, r.stderr)
        assert t.lines("build.log") == [] and t.cargo() == []
        ok(t.run("--cudf", "26.02", env={"GCC_VERSION": "14"}))
        assert {c["target"] for c in t.cargo()} == {str(t.root / "target-img-26.02")}


if __name__ == "__main__":
    raise SystemExit(main(globals()))
