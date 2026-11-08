"""build-test.sh: the one build command, run in the build image over the checkout.

Outside the image it re-executes itself in a `docker run`; inside (PEACOCK_IN_IMAGE=1) it builds and
runs. It runs in a throwaway git repo holding a copy of the script and fake test targets, one per
class the mode ladder derives. `docker`, `cargo` and `ctest` are fakes on PATH recording their
argv; `scripts/build.sh`, `scripts/env/stage-install.sh` and `scripts/env/image-ref.sh` are stubs in
the tree that do the same. Nothing builds, pulls or runs a container.
"""
import json
import os
import pathlib
import pty
import shutil
import signal
import subprocess
import sys
import tempfile
import time

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
COPIED = ["scripts/build-test.sh", "docker/pins.env", "docker/conda/cudf-25.02.spec",
          "docker/conda/cudf-25.10.spec", "docker/conda/cudf-26.02.spec"]
NOT_RUST_ONLY = '#![cfg(not(feature = "rust-only"))]\n'
# One target per class: rust-only by default, cmake-gated, gpu-runtime, gated on the gpu feature,
# and the two excluded by name.
TARGETS = {
    "peacockdb-core/tests/test_cpu_a.rs": "",
    "peacockdb-core/tests/test_cpu_b.rs": "",
    "peacockdb-core/tests/test_crate_privacy.rs": "",
    "peacockdb-core/tests/test_gpu_corpus.rs": NOT_RUST_ONLY,
    "peacockdb-core/tests/test_node_timing.rs": NOT_RUST_ONLY,
    "peacockdb-core/tests/peacock_gpu_benchmarks.rs": NOT_RUST_ONLY,
    "peacockdb-core/tests/prototype_fused_benchmarks.rs": '#![cfg(feature = "gpu")]\n',
    "peacockdb-ffi/tests/test_ffi.rs": "",
}

RECORD = '#!/bin/sh\nprintf "%s\\n" "$*" >> "$FAKE_LOG_DIR/{log}"\n'
FAKE_IMAGE_REF = r'''#!/bin/sh
printf "%s\n" "$*" >> "$FAKE_LOG_DIR/image-ref.log"
if [ -n "${FAKE_IMAGE_REF_EXIT:-}" ]; then echo "$FAKE_IMAGE_REF_ERR" >&2; exit "$FAKE_IMAGE_REF_EXIT"; fi
if [ "$1" = --ref ]; then echo "${FAKE_IMAGE_REF:-$2}"; exit 0; fi
echo "${FAKE_IMAGE_REF:-ghcr.io/asymptote-tech/peacockdb-build:cudf-x-in-0}"
'''
# `ps` answers FAKE_DOCKER_PS; a `run` given FAKE_DOCKER_SLEEP writes run.started, then after
# FAKE_DOCKER_CREATE_S (its container created) its pid to run.pid, and waits. `kill` ends it, as a
# container killed ends its `docker run`, and before run.pid fails as docker does on no such container.
FAKE_DOCKER = r'''#!/usr/bin/env python3
import json, os, signal, sys, time
a = sys.argv[1:]
logs = os.environ["FAKE_LOG_DIR"]
with open(os.path.join(logs, "docker.log"), "a") as f:
    f.write(json.dumps(a) + "\n")
if a[0] == "ps":
    print(os.environ.get("FAKE_DOCKER_PS", ""), end="")
    sys.exit(0)
if a[0] == "kill":
    if not os.path.exists(os.path.join(logs, "run.pid")):
        print(f"Error response from daemon: No such container: {a[-1]}", file=sys.stderr)
        sys.exit(1)
    with open(os.path.join(logs, "run.pid")) as f:
        os.kill(int(f.read()), signal.SIGKILL)
    sys.exit(0)
if a[0] == "run" and os.environ.get("FAKE_DOCKER_SLEEP"):
    open(os.path.join(logs, "run.started"), "w").close()
    time.sleep(float(os.environ.get("FAKE_DOCKER_CREATE_S", "0")))
    with open(os.path.join(logs, "run.pid.tmp"), "w") as f:
        f.write(str(os.getpid()))
    os.rename(os.path.join(logs, "run.pid.tmp"), os.path.join(logs, "run.pid"))
    time.sleep(float(os.environ["FAKE_DOCKER_SLEEP"]))
sys.exit(int(os.environ.get("FAKE_DOCKER_EXIT", "0")))
'''
FAKE_CARGO = r'''#!/usr/bin/env python3
import json, os, sys
a = sys.argv[1:]
with open(os.path.join(os.environ["FAKE_LOG_DIR"], "cargo.log"), "a") as f:
    f.write(json.dumps({"argv": a, "target": os.environ.get("CARGO_TARGET_DIR"),
                        "ld": os.environ.get("LD_LIBRARY_PATH", "")}) + "\n")
if a[0] != "test" or "--no-run" in a:
    sys.exit(0)
name = "lib" if "--lib" in a else a[a.index("--test") + 1]
if name in os.environ.get("FAKE_CARGO_ZERO", "").split():
    print("running 0 tests\n\ntest result: ok. 0 passed; 0 failed")
    sys.exit(0)
print("running 1 test\ntest x ... ok\n\ntest result: ok. 1 passed; 0 failed")
sys.exit(101 if name in os.environ.get("FAKE_CARGO_FAILS", "").split() else 0)
'''

# What the environment of the caller must not leak into a run that does not name it.
SCRUBBED = ("CARGO_TARGET_DIR", "PEACOCK_IN_IMAGE", "PEACOCK_CUDF", "PEACOCK_BUILD_IMAGE", "RUNNER_TEMP",
            "RUSTFLAGS", "CMAKE_GENERATOR", "LDFLAGS", "LD_LIBRARY_PATH", "PCK_TEST_FILTER", "PCK_SHARD",
            "UPDATE_CANONICAL", "PEACOCK_NO_FETCH", "RUST_TEST_THREADS", "CARGO_BUILD_JOBS", "GCC_VERSION",
            "PEACOCK_CARGO_REGISTRY", "PEACOCK_CCACHE_DIR", "XDG_CACHE_HOME", "AWS_SECRET_ACCESS_KEY")


def git(repo, *args):
    return subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True,
                          text=True).stdout.strip()


def write(path, text, mode=None):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    if mode:
        path.chmod(mode)


class Tree:
    """A committed repo holding build-test.sh, its stubs and fake targets; fakes on PATH beside it."""

    def __init__(self, tmp, root=None):
        tmp = pathlib.Path(tmp)
        self.tmp = tmp
        self.root = (root or tmp / "tree")
        self.root.mkdir(parents=True, exist_ok=True)
        for rel in COPIED:
            write(self.root / rel, (REPO / rel).read_text() if (REPO / rel).exists() else "")
        (self.root / "scripts/build-test.sh").chmod(0o755)
        for rel, head in TARGETS.items():
            write(self.root / rel, head + "#[test]\nfn t() {}\n")
        write(self.root / "scripts/build.sh", RECORD.format(log="build.log"), 0o755)
        write(self.root / "scripts/env/stage-install.sh", RECORD.format(log="stage.log"), 0o755)
        write(self.root / "scripts/env/image-ref.sh", FAKE_IMAGE_REF, 0o755)
        if root is None:
            git(self.root, "init", "-q")
            git(self.root, "add", "-A")
            git(self.root, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "tree")
        self.root = self.root.resolve()
        self.logs = tmp / "logs"
        self.logs.mkdir(exist_ok=True)
        self.bin = tmp / "bin"
        self.bin.mkdir(exist_ok=True)
        write(self.bin / "docker", FAKE_DOCKER, 0o755)
        write(self.bin / "cargo", FAKE_CARGO, 0o755)
        write(self.bin / "ctest", RECORD.format(log="ctest.log"), 0o755)
        self.cache = (tmp / "cache").resolve()
        self.env = {k: v for k, v in os.environ.items() if k not in SCRUBBED}
        self.env.update(PATH=f"{self.bin}:{os.environ['PATH']}", FAKE_LOG_DIR=str(self.logs),
                        XDG_CACHE_HOME=str(self.cache))

    def environ(self, env, inside):
        e = {**self.env, **(env or {})}
        if inside:
            e.update(PEACOCK_IN_IMAGE="1", GCC_VERSION=e.get("GCC_VERSION", "12"))
        else:
            e.setdefault("PEACOCK_BUILD_IMAGE", "img:test")
        return {k: v for k, v in e.items() if v is not None}

    def run(self, *args, env=None, inside=False):
        return subprocess.run([str(self.root / "scripts/build-test.sh"), *args], cwd=self.root,
                              env=self.environ(env, inside), capture_output=True, text=True,
                              stdin=subprocess.DEVNULL, timeout=60)

    def start(self, *args, env=None, **streams):
        """build-test.sh <args> outside the image, left running."""
        streams = {"stdin": subprocess.DEVNULL, "stdout": subprocess.PIPE, "stderr": subprocess.PIPE, **streams}
        return subprocess.Popen([str(self.root / "scripts/build-test.sh"), *args], cwd=self.root,
                                env=self.environ(env, False), **streams)

    def lines(self, name):
        p = self.logs / name
        return p.read_text().splitlines() if p.exists() else []

    def docker(self):
        return [json.loads(line) for line in self.lines("docker.log")]

    def cargo(self):
        return [json.loads(line) for line in self.lines("cargo.log")]

    def runs(self):
        return [c for c in self.docker() if c[0] == "run"]

    def the_docker_run(self):
        calls = self.runs()
        assert len(calls) == 1 and calls[0][:2] == ["run", "--rm"], calls
        return calls[0]

    def stamp(self):
        p = self.root / "cpp/build/.peacock-cudf"
        return p.read_text().strip() if p.exists() else None


def ok(r):
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def values(argv, flag):
    return [argv[i + 1] for i, token in enumerate(argv[:-1]) if token == flag]


def after_image(argv, image):
    return argv[argv.index(image) + 1:]


def test_runs_in_the_image_as_the_caller():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        ok(t.run("--rust-only", "--build", "--run", env={"RUSTFLAGS": "-D warnings", "RUST_TEST_THREADS": "2"}))
        argv = t.the_docker_run()
        root = str(t.root)
        assert values(argv, "--user") == [f"{os.getuid()}:{os.getgid()}"], argv
        assert f"{root}:{root}" in values(argv, "-v"), argv
        assert values(argv, "-w") == [root], argv
        env = values(argv, "-e")
        for want in ("PEACOCK_IN_IMAGE=1", "PEACOCK_CUDF=25.02", f"HOME={root}/.home-img",
                     f"CARGO_TARGET_DIR={root}/target-img-25.02", "RUSTFLAGS", "RUST_TEST_THREADS"):
            assert want in env, (want, env)
        assert "PCK_SHARD" not in env and "LDFLAGS" not in env, env
        assert not [e for e in env if e.startswith("RUSTFLAGS=")], "a forwarded value is in argv"
        mounts = values(argv, "-v")
        assert f"{t.cache}/peacock/cargo-registry:/opt/rust/cargo/registry" in mounts, mounts
        assert f"{t.cache}/peacock/ccache-25.02:/tmp/ccache" in mounts, mounts
        assert (t.cache / "peacock/cargo-registry").is_dir() and (t.cache / "peacock/ccache-25.02").is_dir()
        assert (t.root / ".home-img").is_dir()
        assert after_image(argv, "img:test") == ["scripts/build-test.sh", "--rust-only", "--build", "--run"], argv
        assert t.lines("image-ref.log") == ["--ref img:test build 25.02"], "PEACOCK_BUILD_IMAGE is checked"
        assert t.cargo() == [] and t.lines("build.log") == [], "nothing builds outside the image"


def test_the_container_exit_is_the_scripts():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        r = t.run("--rust-only", "--run", env={"FAKE_DOCKER_EXIT": "7"})
        assert r.returncode == 7, (r.returncode, r.stderr)


def label_values(argv):
    return dict(v.split("=", 1) for v in values(argv, "--label"))


def test_the_container_has_an_init_and_a_name_by_worktree_and_version():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        ok(t.run("--cudf", "26.02", "--rust-only", "--build"))
        argv = t.the_docker_run()
        assert "--init" in argv[:argv.index("img:test")], argv
        assert "-it" not in argv and "-t" not in argv and "-i" not in argv, "no terminal, no -it"
        [name] = values(argv, "--name")
        labels = label_values(argv)
        assert labels["peacock.tree"] == str(t.root) and labels["peacock.cudf"] == "26.02", labels
        assert name == f"peacock-build-tree-26.02-{labels['peacock.pid']}", (name, labels)
        [ps] = [c for c in t.docker() if c[0] == "ps"]
        assert f"label=peacock.tree={t.root}" in values(ps, "--filter"), ps
        assert "label=peacock.cudf=26.02" in values(ps, "--filter"), ps


def test_a_terminal_gets_it():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        primary, secondary = pty.openpty()
        try:
            p = t.start("--rust-only", "--build", stdin=secondary, stdout=secondary)
            assert p.wait(timeout=60) == 0, p.stderr.read()
        finally:
            os.close(secondary)
            os.close(primary)
        argv = t.the_docker_run()
        assert "-it" in argv[:argv.index("img:test")], argv
        p = t.start("--rust-only", "--build", stdin=subprocess.DEVNULL)
        assert p.wait(timeout=60) == 0
        assert "-it" not in t.runs()[-1], "stdin is no terminal"


# A session leader holding the pty named by argv[1] as its controlling terminal runs argv[3:] on
# it, in its own foreground process group when argv[2] is `fg`, else in a background one.
ON_A_TERMINAL = r'''
import os, sys
os.setsid()
tty = os.open(sys.argv[1], os.O_RDWR)
pid = os.fork()
if pid == 0:
    if sys.argv[2] != "fg":
        os.setpgid(0, 0)
    os.dup2(tty, 0)
    os.dup2(tty, 1)
    os.execv(sys.argv[3], sys.argv[3:])
sys.exit(os.waitstatus_to_exitcode(os.waitpid(pid, 0)[1]))
'''


def test_a_background_job_on_a_terminal_does_not_get_it():
    """docker run -it sets the terminal's modes, and a process group in the background that does is
    stopped by SIGTTOU: `build-test.sh … &` from a shell would hang."""
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        primary, secondary = pty.openpty()
        try:
            for group, want in (("fg", True), ("bg", False)):
                p = subprocess.run([sys.executable, "-c", ON_A_TERMINAL, os.ttyname(secondary), group,
                                    str(t.root / "scripts/build-test.sh"), "--rust-only", "--build"],
                                   cwd=t.root, env=t.environ(None, False), stdin=subprocess.DEVNULL,
                                   capture_output=True, text=True, timeout=60)
                assert p.returncode == 0, (group, p.returncode, p.stderr)
                argv = t.runs()[-1]
                assert ("-it" in argv[:argv.index("img:test")]) == want, (group, argv)
        finally:
            os.close(secondary)
            os.close(primary)


def started_run(t, *args, env=None, up="run.pid"):
    """build-test.sh <args>, once its fake `docker run` has written <up>: run.pid once its container
    runs, run.started before."""
    p = t.start(*args, env={"FAKE_DOCKER_SLEEP": "60", **(env or {})})
    deadline = time.monotonic() + 30
    while not (t.logs / up).exists():
        assert p.poll() is None and time.monotonic() < deadline, (p.poll(), p.stderr.read() if p.poll() else "")
        time.sleep(0.05)
    return p


def test_a_signal_kills_the_container():
    for sig, code in ((signal.SIGTERM, 143), (signal.SIGINT, 130), (signal.SIGHUP, 129)):
        with tempfile.TemporaryDirectory() as tmp:
            t = Tree(tmp)
            p = started_run(t, "--rust-only", "--build")
            t0 = time.monotonic()
            p.send_signal(sig)
            assert p.wait(timeout=30) == code, (sig, p.returncode, p.stderr.read())
            assert time.monotonic() - t0 < 20, "the script waited out the container"
            [name] = values(t.the_docker_run(), "--name")
            assert ["kill", name] in t.docker(), t.docker()


def test_a_signal_before_the_container_exists_still_kills_it():
    """docker run is up and its container not yet created: the first `docker kill` finds nothing, and
    the container must not then run the build out."""
    for sig, code in ((signal.SIGTERM, 143), (signal.SIGINT, 130), (signal.SIGHUP, 129)):
        with tempfile.TemporaryDirectory() as tmp:
            t = Tree(tmp)
            p = started_run(t, "--rust-only", "--build", env={"FAKE_DOCKER_CREATE_S": "2"}, up="run.started")
            t0 = time.monotonic()
            p.send_signal(sig)
            assert p.wait(timeout=30) == code, (sig, p.returncode, p.stderr.read())
            assert time.monotonic() - t0 < 20, "the script waited out the container"
            [name] = values(t.the_docker_run(), "--name")
            kills = [c for c in t.docker() if c == ["kill", name]]
            assert len(kills) >= 2, ("killed once, before the container existed", t.docker())


def test_a_leftover_container_is_refused():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        gone = 2 ** 22 + 7
        while pathlib.Path(f"/proc/{gone}").exists():
            gone += 1
        left = f"peacock-build-tree-25.02-{gone} {gone}\n"
        r = t.run("--rust-only", "--build", env={"FAKE_DOCKER_PS": left})
        assert r.returncode == 2, (r.returncode, r.stdout, r.stderr)
        assert f"peacock-build-tree-25.02-{gone}" in r.stderr and "docker kill" in r.stderr, r.stderr
        assert t.runs() == [], t.docker()
        # One whose build-test.sh still runs is another build here, not a leftover.
        alive = f"peacock-build-tree-25.02-{os.getpid()} {os.getpid()}\n"
        ok(t.run("--rust-only", "--build", env={"FAKE_DOCKER_PS": alive}))


def test_target_dir_is_per_worktree_and_version():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        root = str(t.root)
        ok(t.run("--cudf", "26.02", "--rust-only", "--build",
                 env={"PEACOCK_BUILD_IMAGE": None, "FAKE_IMAGE_REF": "ref:2602"}))
        argv = t.the_docker_run()
        assert t.lines("image-ref.log") == ["build 26.02"], t.lines("image-ref.log")
        assert after_image(argv, "ref:2602")[0] == "scripts/build-test.sh", argv
        env = values(argv, "-e")
        assert f"CARGO_TARGET_DIR={root}/target-img-26.02" in env and "PEACOCK_CUDF=26.02" in env, env
        assert f"{t.cache}/peacock/ccache-26.02:/tmp/ccache" in values(argv, "-v"), argv

        r = t.run("--rust-only", "--build", env={"CARGO_TARGET_DIR": "/elsewhere"})
        assert r.returncode == 2 and "/elsewhere" in r.stderr, (r.returncode, r.stderr)
        assert len(t.runs()) == 1, "a refused target dir ran a container"

        ok(t.run("--rust-only", "--build", env={"CARGO_TARGET_DIR": f"{root}/target-mine"}))
        assert f"CARGO_TARGET_DIR={root}/target-mine" in values(t.runs()[-1], "-e"), t.runs()[-1]

        ok(t.run("--rust-only", "--build", env={"PEACOCK_CUDF": "26.02"}))
        assert "PEACOCK_CUDF=26.02" in values(t.runs()[-1], "-e"), t.runs()[-1]

        r = t.run("--cudf", "24.12", "--rust-only", "--build")
        assert r.returncode == 2 and "24.12" in r.stderr, (r.returncode, r.stderr)
        assert "docker/conda holds specs for 25.02 25.10 26.02\n" in r.stderr, r.stderr


def test_worktree_mounts_its_git_dir():
    with tempfile.TemporaryDirectory() as tmp:
        main_tree = Tree(tmp)
        ok(main_tree.run("--rust-only", "--build"))
        root = str(main_tree.root)
        assert [m for m in values(main_tree.runs()[-1], "-v") if ".git" in m] == [], \
            "a plain clone's git dir is inside the mounted tree"
        wt = pathlib.Path(tmp) / "wt"
        git(main_tree.root, "worktree", "add", "-q", str(wt))
        t = Tree(tmp, root=wt)
        ok(t.run("--rust-only", "--build"))
        common = f"{root}/.git"
        mounts = values(t.runs()[-1], "-v")
        assert f"{common}:{common}" in mounts and f"{t.root}:{t.root}" in mounts, mounts
        assert values(t.runs()[-1], "-w") == [str(t.root)]


def test_pass_env_never_puts_a_value_in_argv():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        ok(t.run("--pass-env", "AWS_SECRET_ACCESS_KEY", "--", "python3", "scripts/check_s3_datasets.py",
                 env={"AWS_SECRET_ACCESS_KEY": "s3cr3t"}))
        argv = t.the_docker_run()
        assert "AWS_SECRET_ACCESS_KEY" in values(argv, "-e"), argv
        assert not [a for a in argv if "s3cr3t" in a], argv
        assert after_image(argv, "img:test")[-2:] == ["python3", "scripts/check_s3_datasets.py"], argv
        r = t.run("--pass-env", "AWS_SECRET_ACCESS_KEY=s3cr3t", "--", "true")
        assert r.returncode == 2 and "--pass-env" in r.stderr, (r.returncode, r.stderr)
        assert len(t.runs()) == 1


def test_refuses_a_stale_image():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        err = ("image-ref.sh: ghcr.io/asymptote-tech/peacockdb-build:cudf-25.02-in-0 was built from "
               "other inputs than this tree: docker/pins.env")
        r = t.run("--rust-only", "--build",
                  env={"PEACOCK_BUILD_IMAGE": None, "FAKE_IMAGE_REF_EXIT": "2", "FAKE_IMAGE_REF_ERR": err})
        assert r.returncode == 2, (r.returncode, r.stdout, r.stderr)
        assert "docker/pins.env" in r.stderr, r.stderr
        assert t.docker() == [], t.docker()


def test_refuses_a_given_image_of_other_inputs():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        err = "image-ref.sh: img:2502 was built from other inputs than this tree: docker/conda/cudf-25.02.spec"
        r = t.run("--cudf", "26.02", "--rust-only", "--build",
                  env={"PEACOCK_BUILD_IMAGE": "img:2502", "FAKE_IMAGE_REF_EXIT": "2", "FAKE_IMAGE_REF_ERR": err})
        assert r.returncode == 2, (r.returncode, r.stdout, r.stderr)
        assert "img:2502" in r.stderr and "docker/conda/cudf-25.02.spec" in r.stderr, r.stderr
        assert t.lines("image-ref.log") == ["--ref img:2502 build 26.02"], t.lines("image-ref.log")
        assert t.docker() == [], t.docker()


def test_refuses_cpp_build_of_another_cudf():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        write(t.root / "cpp/build/.peacock-cudf", "26.02\n")
        for mode in ([], ["--gpu"]):
            r = t.run(*mode, "--build", inside=True)
            assert r.returncode == 2, (mode, r.returncode, r.stdout, r.stderr)
            assert "25.02" in r.stderr and "26.02" in r.stderr and "cpp/build" in r.stderr, r.stderr
        assert t.lines("build.log") == [] and t.lines("stage.log") == [] and t.cargo() == []
        assert t.stamp() == "26.02"


def test_refuses_an_unstamped_cpp_build():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        write(t.root / "cpp/build/CMakeCache.txt", "CMAKE_CXX_COMPILER:FILEPATH=/usr/bin/g++-12\n")
        r = t.run("--build", inside=True)
        assert r.returncode == 2, (r.returncode, r.stdout, r.stderr)
        assert "cpp/build" in r.stderr and ".peacock-cudf" in r.stderr, r.stderr
        assert (t.root / "cpp/build/CMakeCache.txt").exists(), "a host configure was removed"
        assert t.lines("build.log") == [] and t.cargo() == [] and t.stamp() is None


def test_cpu_build_configures_stamps_and_builds_every_target():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        (t.root / "cpp/build").mkdir(parents=True)
        ok(t.run("--build", inside=True))
        assert t.lines("build.log") == [
            "--configure --build --install --cudf_ROOT /opt/cudf --gcc-version 12 --compiler-launcher ccache"
        ], t.lines("build.log")
        assert t.stamp() == "25.02"
        calls = [c["argv"] for c in t.cargo()]
        assert calls == [
            ["test", "--no-run", "-p", "peacockdb-core", "--test", "test_cpu_a"],
            ["test", "--no-run", "-p", "peacockdb-core", "--test", "test_cpu_b"],
            ["test", "--no-run", "-p", "peacockdb-ffi", "--test", "test_ffi"],
            ["test", "--no-run", "-p", "peacockdb-core", "--lib"],
            ["build", "--features", "rust-only", "-p", "peacockdb"],
        ], calls
        assert {c["target"] for c in t.cargo()} == {f"{t.root}/target-img-25.02"}
        ok(t.run("--build", inside=True))
        assert t.stamp() == "25.02" and len(t.lines("build.log")) == 2, "its own stamp is not refused"


def test_rust_only_build_leaves_cpp_build_alone():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        write(t.root / "cpp/build/CMakeCache.txt", "a host configure\n")
        ok(t.run("--rust-only", "--build", inside=True))
        assert t.lines("build.log") == [] and t.stamp() is None
        calls = [c["argv"] for c in t.cargo()]
        assert calls == [
            ["test", "--no-run", "--features", "rust-only", "-p", "peacockdb-core", "--test", "test_cpu_a"],
            ["test", "--no-run", "--features", "rust-only", "-p", "peacockdb-core", "--test", "test_cpu_b"],
            ["test", "--no-run", "--features", "rust-only", "-p", "peacockdb-core", "--lib"],
            ["build", "--features", "rust-only", "-p", "peacockdb"],
        ], calls


def test_gpu_build_is_stage_install():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        ok(t.run("--gpu", "--build", inside=True))
        ok(t.run("--cudf", "25.02", "--gpu", "--build", "--no-release-bench", inside=True))
        assert t.lines("stage.log") == ["--cudf 25.02", "--cudf 25.02 --no-release-bench"], t.lines("stage.log")
        assert t.stamp() == "25.02"
        assert t.cargo() == [] and t.lines("build.log") == []
        r = t.run("--rust-only", "--build", "--no-release-bench", inside=True)
        assert r.returncode == 2 and "--no-release-bench" in r.stderr, (r.returncode, r.stderr)


def test_gpu_run_names_run_gpu():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        for args in (["--gpu", "--run"], ["--gpu", "--build", "--run"]):
            r = t.run(*args)
            assert r.returncode == 2, (args, r.returncode, r.stderr)
            assert "scripts/env/run-gpu.sh --instance-id computeinstance-e00tnrse7ayntnzcyt ship" in r.stderr, r.stderr
            assert " run " in r.stderr, r.stderr
        assert t.docker() == [] and t.lines("image-ref.log") == []


def test_removed_flags_name_their_replacement():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        cases = {
            ("--host", "x", "--run"): "run-gpu.sh",
            ("--push-goldens",): "run-gpu.sh",
            ("--pull-parquet",): "run-gpu.sh",
            ("--remote-dir", "d", "--build"): "run-gpu.sh",
            ("--gcc-version", "12", "--build"): "--cudf",
            ("--local-cudf-root", "/x", "--build"): "--cudf",
            ("--all",): "--build --run",
            ("--build-benchmarks",): "--gpu --build",
            ("--run-benchmarks",): "run-gpu.sh",
            ("--pull-benchmarks",): "run-gpu.sh",
            ("--update-canonical", "--run"): "UPDATE_CANONICAL=1",
        }
        for args, replacement in cases.items():
            r = t.run(*args)
            assert r.returncode == 2, (args, r.returncode, r.stderr)
            assert args[0] in r.stderr and replacement in r.stderr, (args, r.stderr)
        r = t.run("--bogus")
        assert r.returncode == 2 and "--bogus" in r.stderr, (r.returncode, r.stderr)
        assert t.docker() == [] and t.lines("image-ref.log") == []


def test_refuses_nothing_to_do_and_a_command_beside_an_action():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        for args in ([], ["--rust-only"], ["--build", "--", "true"], ["--gpu", "--", "true"], ["--"]):
            r = t.run(*args)
            assert r.returncode == 2 and r.stderr, (args, r.returncode, r.stderr)
        for flag in ("--build", "--run", "--gpu", "--rust-only", "--no-release-bench"):
            r = t.run(flag, "--", "true")
            assert r.returncode == 2 and "a command runs alone" in r.stderr, (flag, r.returncode, r.stderr)
        r = t.run("--gpu", "--rust-only", "--build")
        assert r.returncode == 2 and "--gpu" in r.stderr and "--rust-only" in r.stderr, r.stderr
        assert t.docker() == []


def test_run_runs_every_target_after_a_failure():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        r = t.run("--rust-only", "--run", inside=True, env={"FAKE_CARGO_FAILS": "test_cpu_a"})
        assert r.returncode == 1, (r.returncode, r.stdout, r.stderr)
        assert "test_cpu_a" in r.stdout + r.stderr
        calls = [c["argv"] for c in t.cargo()]
        assert calls == [
            ["test", "--features", "rust-only", "-p", "peacockdb-core", "--test", "test_cpu_a"],
            ["test", "--features", "rust-only", "-p", "peacockdb-core", "--test", "test_cpu_b"],
            ["test", "--features", "rust-only", "-p", "peacockdb-core", "--lib"],
        ], calls

        (t.logs / "cargo.log").unlink()
        write(t.root / "cpp/build/.peacock-cudf", "25.02\n")
        r = t.run("--run", inside=True, env={"FAKE_CARGO_FAILS": "lib"})
        assert r.returncode == 1, (r.returncode, r.stdout, r.stderr)
        assert t.lines("ctest.log") == ["--test-dir cpp/build -L cpu --output-on-failure"], t.lines("ctest.log")
        calls = [c["argv"] for c in t.cargo()]
        assert calls == [
            ["test", "-p", "peacockdb-core", "--test", "test_cpu_a"],
            ["test", "-p", "peacockdb-core", "--test", "test_cpu_b"],
            ["test", "-p", "peacockdb-ffi", "--test", "test_ffi"],
            ["test", "-p", "peacockdb-core", "--lib"],
        ], calls
        assert all("/opt/cudf/lib" in c["ld"].split(":") for c in t.cargo()), t.cargo()


def test_run_fails_a_target_that_ran_no_test_unless_filtered():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        r = t.run("--rust-only", "--run", inside=True, env={"FAKE_CARGO_ZERO": "test_cpu_b"})
        assert r.returncode == 1 and "test_cpu_b ran 0 tests" in r.stdout + r.stderr, (r.returncode, r.stdout)
        (t.logs / "cargo.log").unlink()
        ok(t.run("--rust-only", "--run", inside=True,
                 env={"FAKE_CARGO_ZERO": "test_cpu_b", "PCK_TEST_FILTER": "q1"}))
        assert all(c["argv"][-2:] == ["--", "q1"] for c in t.cargo()), t.cargo()


def test_command_form_passes_the_exit_code():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        assert t.run("--", "false", inside=True).returncode == 1
        assert t.run("--", "sh", "-c", "exit 3", inside=True).returncode == 3
        r = ok(t.run("--", "sh", "-c", 'echo "$PWD"', inside=True))
        assert r.stdout.strip() == str(t.root), r.stdout
        assert t.docker() == []


def test_runner_temp_is_mounted_when_set():
    with tempfile.TemporaryDirectory() as tmp:
        t = Tree(tmp)
        runner_temp = str((pathlib.Path(tmp) / "runner-temp").resolve())
        os.mkdir(runner_temp)
        ok(t.run("--", "true", env={"RUNNER_TEMP": runner_temp}))
        argv = t.runs()[-1]
        assert f"{runner_temp}:{runner_temp}" in values(argv, "-v") and "RUNNER_TEMP" in values(argv, "-e"), argv
        ok(t.run("--", "true"))
        argv = t.runs()[-1]
        assert not [v for v in values(argv, "-v") if "runner-temp" in v], argv
        assert "RUNNER_TEMP" not in values(argv, "-e"), argv


if __name__ == "__main__":
    raise SystemExit(main(globals()))
