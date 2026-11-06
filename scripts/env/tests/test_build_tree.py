"""build-tree.sh: stage-tree.sh in the build image on a machine with docker, outside an instance.

    build-tree.sh [--cudf-source <dir>] [--cudf <v> | --image <tag>] [--registry <dir>] [--ccache <dir>] [--bench]

Each test runs the script from a scratch git tree that pins a scratch cuDF repository at
third_party/cudf. docker is a fake that records its argv, pulls only the build image, answers its
label as image-inputs.sh computes it over the tree, and builds nothing. A named container is a file
holding its pid, held while FAKE_HOLD exists until a kill; every host path under the scratch root
stands for the image's own /home/peacock, which a container cannot enter.
"""
import fcntl
import json
import os
import pathlib
import shutil
import signal
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from machine import REPO, flag_values, git, lib, path_without  # noqa: E402

TREE_FILES = ["scripts/env/build-tree.sh", "scripts/env/stage-tree.sh", "scripts/lib/peacock-config.sh",
              "scripts/lib/machine.sh", "scripts/lib/image-inputs.sh", "scripts/lib/data-inputs.sh",
              "scripts/lib/portable.sh", "cpp/CMakeLists.txt"]
BUILD = "ghcr.io/asymptote-tech/peacockdb-build"
INSIDE = "/peacock/tree"

FAKE_DOCKER = r'''#!/usr/bin/env python3
import json, os, pathlib, sys, time
a = sys.argv[1:]
with open(os.environ["FAKE_DOCKER_LOG"], "a") as f:
    f.write(json.dumps(a) + "\n")
labels = json.loads(os.environ["FAKE_LABELS"])
boxes = pathlib.Path(os.environ["FAKE_CONTAINERS"])
if a[0] == "pull":
    sys.exit(0 if a[1] in labels else f"Error response from daemon: manifest unknown: {a[1]}")
if a[:2] == ["image", "inspect"] and a[3] == '{{index .Config.Labels "peacockdb.inputs"}}':
    print(labels[a[4]])
    sys.exit(0)
if a[:2] == ["container", "inspect"]:
    sys.exit(0 if (boxes / a[2]).exists() else 1)
if a[0] in ("kill", "rm"):
    box = boxes / a[-1]
    if not box.exists():
        sys.exit(f"Error response from daemon: No such container: {a[-1]}")
    if a[0] == "rm":
        box.unlink()
        sys.exit(0)
    (boxes / f"{a[-1]}.killed").write_text("")
    while box.exists():
        time.sleep(0.05)
    sys.exit(0)
if a[0] != "run":
    sys.exit(f"fake docker: no answer for {a}")
closed = os.environ["FAKE_CLOSED"].rstrip("/") + "/"
inside = [a[i + 1] for i in range(len(a) - 1) if a[i] == "-w"]
inside += [kv.split("=", 1)[1] for i in range(len(a) - 1) if a[i] == "--mount"
           for kv in a[i + 1].split(",") if kv.startswith("dst=")]
for path in inside + [w for w in a if w.startswith("/")]:
    if path.startswith(closed):
        sys.exit(f"docker: {path}: Permission denied")
name = a[a.index("--name") + 1]
box = boxes / name
box.write_text(str(os.getpid()))
while os.path.exists(os.environ.get("FAKE_HOLD", "/nonexistent")) and not (boxes / f"{name}.killed").exists():
    time.sleep(0.05)
killed = (boxes / f"{name}.killed").exists()
for f in (box, boxes / f"{name}.killed"):
    if f.exists():
        f.unlink()
sys.exit(137 if killed else int(os.environ.get("FAKE_RUN_EXIT", "0")))
'''


class Tree:
    """A git tree pinning a cuDF repository's commit, and a docker that knows the tree's build images."""

    def __enter__(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = root = pathlib.Path(self.tmp.name)
        self.cudf = root / "cudf"
        for rel in ("VERSION", "cmake/rapids.cmake", "cpp/cmake/thirdparty.cmake"):
            (self.cudf / rel).parent.mkdir(parents=True, exist_ok=True)
            (self.cudf / rel).write_text(f"{rel}\n")
        git(self.cudf, "init", "-q", "-b", "main")
        git(self.cudf, "add", "-A")
        git(self.cudf, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "cudf")
        self.commit = git(self.cudf, "rev-parse", "HEAD")
        self.tree = root / "tree"
        for rel in TREE_FILES:
            (self.tree / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPO / rel, self.tree / rel)
        shutil.copytree(REPO / "docker", self.tree / "docker")
        git(self.tree, "init", "-q", "-b", "master")
        git(self.tree, "add", "-A")
        git(self.tree, "update-index", "--add", "--cacheinfo", f"160000,{self.commit},third_party/cudf")
        git(self.tree, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "tree")
        (self.tree / "third_party/cudf").mkdir(parents=True)  # a submodule never initialised, as a clone leaves it
        self.home = root / "home"
        self.home.mkdir()
        self.bin = root / "bin"
        self.bin.mkdir()
        (self.bin / "docker").write_text(FAKE_DOCKER)
        (self.bin / "docker").chmod(0o755)
        self.log = root / "docker.log"
        self.containers = root / "containers"
        self.containers.mkdir()
        self.hold = root / "hold"
        self.env = {"PATH": f"{self.bin}:{os.environ['PATH']}", "HOME": str(self.home), "LANG": "C.UTF-8",
                    "FAKE_DOCKER_LOG": str(self.log), "FAKE_LABELS": json.dumps(self.labels(self.tree)),
                    "FAKE_CONTAINERS": str(self.containers), "FAKE_CLOSED": str(root), "FAKE_HOLD": str(self.hold)}
        return self

    def __exit__(self, *exc):
        self.tmp.cleanup()

    def labels(self, tree):
        out = {}
        for cudf in ("25.02", "26.02"):
            label = lib(tree, "image-inputs.sh", f'image_inputs "{tree}" build {cudf} | paste -sd" " -')
            out[f"{BUILD}:cudf-{cudf}"] = out[f"{BUILD}:cudf-{cudf}-0123456789ab"] = label
        return out

    def run(self, *args, env=None, tree=None, background=False):
        full = {k: v for k, v in {**self.env, **(env or {})}.items() if v is not None}
        cmd = [str((tree or self.tree) / "scripts/env/build-tree.sh"), *map(str, args)]
        if background:
            return subprocess.Popen(cmd, env=full, cwd=self.root, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                    text=True)
        return subprocess.run(cmd, env=full, cwd=self.root, capture_output=True, text=True, timeout=60)

    def calls(self):
        return [json.loads(ln) for ln in self.log.read_text().splitlines()] if self.log.exists() else []

    def runs(self):
        return [c for c in self.calls() if c[0] == "run"]


def ok(r):
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def refused(r, *words):
    assert r.returncode == 2, (r.returncode, r.stdout, r.stderr)
    assert len(r.stderr.strip().splitlines()) == 1 and r.stderr.startswith("build-tree.sh: "), r.stderr
    for word in words:
        assert word in r.stderr, (word, r.stderr)


def command(argv):
    """The command a docker run runs, after its image."""
    return argv[argv.index(f"{BUILD}:cudf-25.02") + 1:]


def mounts(argv):
    """Each bind of a docker run as (src, dst, readonly); a tmpfs has no src."""
    out = []
    for value in flag_values(argv, "--mount"):
        kv = dict(p.split("=", 1) if "=" in p else (p, True) for p in value.split(","))
        if kv["type"] == "bind":
            out.append((kv["src"], kv["dst"], "readonly" in kv))
    return out


def env_of(argv):
    return dict(argv[i + 1].split("=", 1) for i, w in enumerate(argv) if w == "-e")


def test_it_fetches_then_stages_in_the_build_image_its_label_checked():
    with Tree() as t:
        registry, ccache = t.root / "registry", t.root / "ccache"
        registry.mkdir()
        ccache.mkdir()
        ok(t.run("--cudf-source", t.cudf, "--registry", registry, "--ccache", ccache, "--bench"))
        assert [c[0] for c in t.calls()] == ["container", "pull", "image", "run", "run"], t.calls()
        assert t.calls()[1] == ["pull", f"{BUILD}:cudf-25.02"], t.calls()[1]
        fetch, stage = t.runs()
        assert command(fetch) == ["cargo", "fetch", "--locked"], fetch
        assert command(stage) == ["bash", "scripts/env/stage-tree.sh", "--bench"], stage
        for argv in (fetch, stage):
            assert argv[argv.index("--user") + 1] == f"{os.getuid()}:{os.getgid()}", argv
            assert argv[argv.index("-w") + 1] == INSIDE and "--init" in argv, argv
            assert argv[argv.index("--name") + 1].startswith("peacock-build-"), argv
            binds = {dst: (src, readonly) for src, dst, readonly in mounts(argv)}
            assert binds[INSIDE] == (str(t.tree), False), binds
            assert binds[f"{INSIDE}/third_party/cudf"] == (str(t.cudf), True), binds
            assert binds["/opt/rust/cargo/registry"] == (str(registry), False), binds
            env = env_of(argv)
            assert binds[env["CCACHE_DIR"]] == (str(ccache), False), (env, binds)
            assert env["GIT_CONFIG_KEY_0"] == "safe.directory", env
            assert env["GIT_CONFIG_VALUE_0"] == f"{INSIDE}/third_party/cudf", env
            tmpfs = [v for v in flag_values(argv, "--mount") if v.startswith("type=tmpfs")]
            assert tmpfs == [f"type=tmpfs,dst={env['HOME']}"], (tmpfs, env)


def test_the_caches_default_to_the_machine_s_and_the_tree_s_own_cudf_is_taken():
    with Tree() as t:
        own = t.tree / "third_party/cudf"
        own.rmdir()
        git(t.root, "clone", "-q", str(t.cudf), str(own))
        r = ok(t.run())
        assert "ship" in r.stdout, r.stdout
        fetch, stage = t.runs()
        assert command(stage) == ["bash", "scripts/env/stage-tree.sh"], stage
        binds = {dst: src for src, dst, _ in mounts(fetch)}
        assert binds[f"{INSIDE}/third_party/cudf"] == str(own), binds
        assert binds["/opt/rust/cargo/registry"] == str(t.home / ".cache/peacock/cargo-registry"), binds
        assert binds[env_of(fetch)["CCACHE_DIR"]] == str(t.home / ".cache/peacock/ccache"), binds
        assert (t.home / ".cache/peacock/cargo-registry").is_dir() and (t.home / ".cache/peacock/ccache").is_dir()


def test_a_worktree_is_mounted_as_any_tree_and_names_the_cudf_it_lacks():
    with Tree() as t:
        worktree = t.root / "worktree"
        git(t.tree, "worktree", "add", "-q", "--detach", str(worktree))
        refused(t.run(tree=worktree), "--cudf-source", str(worktree / "third_party/cudf"))
        ok(t.run("--cudf-source", t.cudf, "--image", "cudf-25.02", tree=worktree))
        for argv in t.runs():
            binds = {dst: src for src, dst, _ in mounts(argv)}
            assert binds[INSIDE] == str(worktree) and binds[f"{INSIDE}/third_party/cudf"] == str(t.cudf), binds
        names = {argv[argv.index("--name") + 1] for argv in t.runs()}
        ok(t.run("--cudf-source", t.cudf))
        assert len(names | {argv[argv.index("--name") + 1] for argv in t.runs()}) == 2, "one name a tree"


def test_bad_inputs_are_refused_before_any_docker_call():
    with Tree() as t:
        elsewhere = t.root / "elsewhere"
        git(t.root, "clone", "-q", str(t.cudf), str(elsewhere))
        (elsewhere / "VERSION").write_text("moved on\n")
        git(elsewhere, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-am", "next")
        plain = t.root / "plain"
        shutil.copytree(t.cudf, plain, ignore=shutil.ignore_patterns(".git"))
        src = ("--cudf-source", t.cudf)
        (t.root / "no-docker").mkdir()
        (t.root / "no-flock").mkdir()
        cases = [
            (("--bogus",), {}, "--bogus"),
            (("stray",), {}, "stray"),
            ((*src, "--bench", "--bench"), {}, "--bench"),
            ((*src, "--cudf"), {}, "--cudf"),
            ((*src, "--cudf", "25.02", "--image", "cudf-25.02"), {}, "--cudf"),
            ((*src, "--image", "latest"), {}, "latest"),
            ((*src, "--cudf", "24.10"), {}, "24.10"),
            (("--cudf-source", t.root / "nowhere"), {}, "nowhere"),
            (("--cudf-source", elsewhere), {}, t.commit),
            (("--cudf-source", plain), {}, str(plain)),
            ((*src, "--registry", t.root / "no-registry"), {}, "no-registry"),
            ((*src, "--ccache", t.root / "no-ccache"), {}, "no-ccache"),
            (src, {"PEACOCK_INSTANCE": "dev1-A"}, "stage-tree.sh"),
            (src, {"PATH": str(path_without(t.root / "no-docker", {"docker"}))}, "docker"),
            (src, {"PATH": f"{t.bin}:{path_without(t.root / 'no-flock', {'flock', 'docker'})}"}, "flock"),
        ]
        for args, env, word in cases:
            refused(t.run(*args, env=env), str(word))
        assert t.calls() == [], t.calls()


def test_an_image_of_other_inputs_is_refused_before_it_runs():
    with Tree() as t:
        r = t.run("--cudf-source", t.cudf, env={"FAKE_LABELS": json.dumps(
            {k: v.replace("cpp/CMakeLists.txt", "cpp/Other.txt") for k, v in t.labels(t.tree).items()})})
        assert r.returncode == 2 and "other inputs" in r.stderr, (r.returncode, r.stderr)
        assert t.runs() == [], t.runs()


def test_a_cmake_cache_made_at_another_path_is_refused_up_front():
    with Tree() as t:
        cache = t.tree / "cpp/build/CMakeCache.txt"
        cache.parent.mkdir(parents=True)
        cache.write_text(f"CMAKE_CACHEFILE_DIR:INTERNAL={t.tree}/cpp/build\n")
        refused(t.run("--cudf-source", t.cudf), "cpp/build", str(t.tree))
        assert t.calls() == [], t.calls()
        cache.write_text(f"CMAKE_CACHEFILE_DIR:INTERNAL={INSIDE}/cpp/build\n")
        ok(t.run("--cudf-source", t.cudf))


def test_relative_caches_are_mounted_by_their_absolute_paths():
    with Tree() as t:
        (t.root / "reg").mkdir()
        (t.root / "cc").mkdir()
        ok(t.run("--cudf-source", t.cudf, "--registry", "reg", "--ccache", "cc"))
        binds = {dst: src for src, dst, _ in mounts(t.runs()[0])}
        assert binds["/opt/rust/cargo/registry"] == str(t.root / "reg"), binds
        assert str(t.root / "cc") in binds.values(), binds


def started(t, count):
    deadline = time.time() + 30
    while len(t.runs()) < count or not any(t.containers.iterdir()):
        assert time.time() < deadline, "the build never started"
        time.sleep(0.05)


def test_an_interrupted_build_kills_its_container_and_a_second_build_is_refused():
    with Tree() as t:
        t.hold.write_text("")
        build = t.run("--cudf-source", t.cudf, background=True)
        started(t, 1)
        (name,) = [c[c.index("--name") + 1] for c in t.runs()]
        refused(t.run("--cudf-source", t.cudf), name)
        build.send_signal(signal.SIGTERM)
        out, err = build.communicate(timeout=60)
        assert build.returncode == 130 and name in err, (build.returncode, err)
        assert ["kill", name] in t.calls() and not (t.containers / name).exists(), t.calls()
        assert len(t.runs()) == 1, "the build went on after its fetch was killed"


def test_the_fetch_waits_for_the_registry_s_lock():
    with Tree() as t:
        registry = t.home / ".cache/peacock/cargo-registry"
        registry.parent.mkdir(parents=True)
        with open(f"{registry}.lock", "w") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            build = t.run("--cudf-source", t.cudf, background=True)
            deadline = time.time() + 30
            while "waiting" not in (err := build.stderr.readline()):
                assert err and time.time() < deadline, "the build never said it waits"
            assert t.runs() == [], "the fetch ran while another held the registry"
            fcntl.flock(lock, fcntl.LOCK_UN)
        out, err = build.communicate(timeout=60)
        assert build.returncode == 0 and len(t.runs()) == 2, (build.returncode, err)


def test_a_failed_step_stops_it():
    with Tree() as t:
        r = t.run("--cudf-source", t.cudf, env={"FAKE_RUN_EXIT": "101"})
        assert r.returncode == 1 and len(t.runs()) == 1, (r.returncode, r.stderr, t.runs())


def test_build_tree_has_help():
    with Tree() as t:
        r = ok(t.run("--help"))
        assert "build-tree.sh [--cudf-source <dir>]" in r.stdout and not r.stdout.startswith("#"), r.stdout
        assert t.calls() == []


if __name__ == "__main__":
    raise SystemExit(main(globals()))
