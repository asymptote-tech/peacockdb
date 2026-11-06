"""A fake CPU VM for bootstrap-vm.sh's tests: a home, a clone of the scripts, and fakes on PATH.

The machine user's home holds a Claude Code install and login and a git identity; `~/peacockdb`
is a git clone holding the scripts under test and the image inputs. The docker CLI is a fake that
records its argv and answers from a JSON world, which holds the image's label. It refuses a bind
whose source is missing, as docker does. A `docker run` of `generate_testdata.sh` makes the
dataset directory it would have made; it and `cargo fetch` take the world's `seconds`, and
`cargo fetch` records whether two ran at once. cuDF's origin is a local repository of two commits,
named by the clone's .gitmodules; the clone pins the first, its own `third_party/cudf` an empty
directory, as a fresh clone leaves it. The machine has a machine-wide data directory, setgid.
"""
import json
import os
import pathlib
import shutil
import subprocess
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[3]
CLONE_FILES = [
    "scripts/env/bootstrap-vm.sh", "scripts/lib/image-inputs.sh", "scripts/lib/data-inputs.sh",
    "scripts/lib/machine.sh", "testdata/generate_testdata.sh", "cpp/CMakeLists.txt",
]
GITCONFIG = "[user]\n\tname = Machine User\n\temail = machine@example.test\n"

FAKE_DOCKER = r'''#!/usr/bin/env python3
import fcntl, json, os, pathlib, sys, time
a = sys.argv[1:]
with open(os.environ["FAKE_DOCKER_LOG"], "a") as f:
    f.write(json.dumps(a) + "\n")
world_path = os.environ["FAKE_DOCKER_WORLD"]

def update(fn):
    with open(world_path + ".lock", "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        w = json.load(open(world_path))
        fn(w)
        json.dump(w, open(world_path, "w"))
        return w

w = update(lambda w: None)
if a[0] in ("info", "pull"):
    sys.exit(0)
if a[:2] == ["image", "inspect"] and "peacockdb.inputs" in a[a.index("--format") + 1]:
    print(w["label"] if w["label"] is not None else "<no value>")
    sys.exit(0)
if a[0] == "run":
    i = 1
    while i < len(a) and a[i].startswith("-"):
        if a[i] == "--rm":
            i += 1; continue
        if a[i] == "--mount":
            kv = dict(p.split("=", 1) if "=" in p else (p, True) for p in a[i + 1].split(","))
            if not os.path.exists(kv["src"]):
                print(f"docker: invalid mount config: bind source path does not exist: {kv['src']}", file=sys.stderr)
                sys.exit(125)
        i += 2
    cmd = a[i + 1:]
    if cmd[:2] == ["bash", "-c"] and cmd[2].startswith("umask 002 &&"):
        os.umask(0o002)
    if cmd[:2] == ["cargo", "fetch"]:
        def begin(w):
            w["fetching"] += 1
            w["fetch_overlap"] = w["fetch_overlap"] or w["fetching"] > 1
        update(begin)
        time.sleep(w["seconds"])
        update(lambda w: w.update(fetching=w["fetching"] - 1))
        sys.exit(0)
    gen = [c for c in cmd if c.endswith("/generate_testdata.sh")]
    if gen:
        stage = pathlib.Path(gen[0]).parent
        bench = cmd[cmd.index("--bench") + 1]
        time.sleep(w["seconds"])
        (stage / f"{bench}.sf1").mkdir()
        (stage / f"{bench}.sf1" / "generated").write_text(bench)
        sys.exit(0)
sys.exit(f"fake docker: no answer for {a}")
'''

FAKE_ROOT_ID = '''#!/bin/sh
case "$1" in -u|-g) echo 0 ;; *) exec /usr/bin/id "$@" ;; esac
'''

KEYS = {
    "AWS_ACCESS_KEY_ID": "AKIAFAKEACCESSVALUE7",
    "AWS_SECRET_ACCESS_KEY": "fake/secret+value/Zq81",
    "AWS_ENDPOINT_URL": "https://s3.storage.example.test:443",
    "AWS_REGION": "test-north9",
}


def git(repo, *args, **kw):
    return subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True, text=True,
                          **kw).stdout.strip()


def lib(tree, source, call):
    """The output of a library function over a tree, as the scripts compute it."""
    return subprocess.run(["bash", "-c", f'. "{tree}/scripts/lib/{source}"; {call}'], check=True,
                          capture_output=True, text=True).stdout.strip()


class Machine:
    """A temporary machine; `run` runs the clone's scripts with the fakes on PATH."""

    def __enter__(self):
        self.tmp = tempfile.TemporaryDirectory()
        tmp = pathlib.Path(self.tmp.name)
        self.home = tmp / "home"
        self.clone = self.home / "peacockdb"
        self.data_dir = tmp / "shared-data"
        self.data_dir.mkdir()
        self.data_dir.chmod(0o2775)
        self.make_home(self.home)
        self.make_clone()
        self.bin = tmp / "bin"
        self.bin.mkdir()
        self.install("docker", FAKE_DOCKER)
        self.docker_log, self.world_path = tmp / "docker.log", tmp / "world.json"
        self.docker_log.touch()
        self.world_path.write_text(json.dumps({"label": self.label("25.02"), "seconds": 0, "fetching": 0,
                                               "fetch_overlap": False}))
        self.env = {"PATH": f"{self.bin}:{os.environ['PATH']}", "HOME": str(self.home), "LANG": "C.UTF-8",
                    "FAKE_DOCKER_LOG": str(self.docker_log), "FAKE_DOCKER_WORLD": str(self.world_path),
                    "PEACOCK_DATA_DIR": str(self.data_dir)}
        return self

    def __exit__(self, *exc):
        self.tmp.cleanup()

    def make_home(self, h):
        (h / ".claude").mkdir(parents=True)
        (h / ".claude" / ".credentials.json").write_text('{"claudeAiOauth": {}}')
        version = h / ".local" / "share" / "claude" / "versions" / "2.1.304"
        version.parent.mkdir(parents=True)
        version.write_text("#!/bin/sh\necho 2.1.304\n")
        version.chmod(0o755)
        (h / ".local" / "bin").mkdir(parents=True)
        (h / ".local" / "bin" / "claude").symlink_to(version)
        (h / ".gitconfig").write_text(GITCONFIG)

    def make_clone(self):
        for rel in CLONE_FILES:
            (self.clone / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPO / rel, self.clone / rel)
        shutil.copytree(REPO / "docker", self.clone / "docker")
        self.cudf_origin = self.home.parent / "cudf-origin"
        self.cudf_shas = []
        for version in ("25.02.00", "26.02.00"):
            (self.cudf_origin / "cmake").mkdir(parents=True, exist_ok=True)
            (self.cudf_origin / "cmake" / "rapids_config.cmake").write_text(f"# {version}\n")
            (self.cudf_origin / "VERSION").write_text(version + "\n")
            (self.cudf_origin / "README.md").write_text("outside the sparse paths\n")
            if not self.cudf_shas:
                git(self.cudf_origin, "init", "-q")
                git(self.cudf_origin, "config", "uploadpack.allowAnySHA1InWant", "true")
            git(self.cudf_origin, "add", "-A")
            git(self.cudf_origin, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", version)
            self.cudf_shas.append(git(self.cudf_origin, "rev-parse", "HEAD"))
        self.cudf_sha = self.cudf_shas[0]
        (self.clone / ".gitmodules").write_text(
            f'[submodule "third_party/cudf"]\n\tpath = third_party/cudf\n\turl = file://{self.cudf_origin}\n')
        (self.clone / "third_party" / "cudf").mkdir(parents=True)  # an uninitialised submodule
        git(self.clone, "init", "-q", "-b", "master")
        git(self.clone, "add", "-A")
        git(self.clone, "update-index", "--add", "--cacheinfo", f"160000,{self.cudf_sha},third_party/cudf")
        git(self.clone, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "scripts")

    def install(self, name, text):
        (self.bin / name).write_text(text)
        (self.bin / name).chmod(0o755)

    def label(self, cudf, tree=None):
        tree = tree or self.clone
        return lib(tree, "image-inputs.sh", f'image_inputs "{tree}" build {cudf} | paste -sd" " -')

    def world(self, **changes):
        w = json.loads(self.world_path.read_text())
        w.update(changes)
        self.world_path.write_text(json.dumps(w))
        return w

    def run(self, script, *args, env=None):
        full_env = {**self.env, **(env or {})}
        cmd = [str(self.clone / "scripts" / "env" / script), *map(str, args)]
        # Under the umask a login shell commonly has, so what a script leaves group-writable is its doing.
        return subprocess.run(cmd, env=full_env, stdin=subprocess.DEVNULL, capture_output=True, text=True,
                              timeout=120, umask=0o022)

    def docker_calls(self):
        return [json.loads(line) for line in self.docker_log.read_text().splitlines()]


def path_without(tmp, names):
    """A PATH directory linking every command on PATH but `names`, as on a machine without them."""
    out = pathlib.Path(tmp) / "system-bin"
    out.mkdir()
    for d in os.environ["PATH"].split(os.pathsep):
        if not os.path.isdir(d):
            continue
        for entry in os.listdir(d):
            link = out / entry
            if entry not in names and not link.exists() and not link.is_symlink():
                link.symlink_to(os.path.join(d, entry))
    return out
