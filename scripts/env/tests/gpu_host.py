"""A GPU VM behind a fake scripts/nebius.sh, and a tree on dev, for run-gpu.sh's tests.

The tree is a git clone of the scripts run-gpu.sh ships and reads: docker/, the libraries, the
generator and a little committed testdata, with an untracked dataset that must never ship, and a
staged cpp/install. Its fetch-sf40.sh is a stub that records its argv and stdin and makes each
dataset's directory. `NEBIUS_SH` is a fake that records each call's argv and, for `ssh` and `rsh`,
runs the command on the VM: bash in the VM user's home, with the VM's fakes first on PATH, the
data directory as PEACOCK_DATA_DIR, and its own copy of /proc/driver/nvidia/params. `rsync`
is the real one, through that rsh. On the VM, docker answers from a JSON world (images and their
labels, containers, the driver), sudo records and runs its command, install writes /etc into the
VM's root, and hostname names the machine. On dev, ssh and docker are fakes that fail loudly: no
call may bypass nebius.sh.
"""
import json
import os
import pathlib
import shutil
import subprocess
import tempfile

from machine import KEYS, REPO, git, lib

TREE_FILES = [
    "scripts/env/run-gpu.sh", "scripts/env/sf40.sha256", "scripts/lib/image-inputs.sh",
    "scripts/lib/data-inputs.sh", "scripts/lib/machine.sh", "scripts/lib/peacock-config.sh",
    "scripts/lib/s3.sh", "scripts/lib/pinned.sh", "testdata/generate_testdata.sh", "cpp/CMakeLists.txt",
]
CUDF = "25.02"
RUN_REF = f"ghcr.io/asymptote-tech/peacockdb-run:cudf-{CUDF}"
BUILD_REF = f"ghcr.io/asymptote-tech/peacockdb-build:cudf-{CUDF}"
H100 = ["--instance-id", "computeinstance-e00e6yena4nzjgaym0", "--profile", "editor-eu"]
HOSTNAME = "h100-test-node"
DRIVER_MINIMUM = "535.104.05"

STUB_FETCH = r'''#!/usr/bin/env python3
import json, os, pathlib, sys
stdin = sys.stdin.read() if "-" in sys.argv else ""
with open(os.environ["FAKE_FETCH_LOG"], "a") as f:
    f.write(json.dumps({"argv": sys.argv[1:], "stdin": stdin}) + "\n")
for ds in (a for a in sys.argv[1:] if a.endswith(".sf40")):
    (pathlib.Path(os.environ["PEACOCK_DATA_DIR"]) / ds).mkdir(parents=True, exist_ok=True)
    print(f"{ds}: fetched 0, kept 8")
'''

FAKE_NEBIUS = r'''#!/usr/bin/env python3
import json, os, subprocess, sys
a = sys.argv[1:]
with open(os.environ["FAKE_NEBIUS_LOG"], "a") as f:
    f.write(json.dumps(a) + "\n")
if os.environ.get("FAKE_NEBIUS_RC"):
    print("nebius: refused by the test", file=sys.stderr)
    sys.exit(int(os.environ["FAKE_NEBIUS_RC"]))
i = 0
while i < len(a) and a[i] in ("--instance-id", "--profile"):
    i += 2
verb, rest = a[i], a[i + 1:]
if verb == "ssh":
    command = " ".join(rest)
elif verb == "rsh":
    command = " ".join(rest[1:])
else:
    sys.exit(0)
env = {k: v for k, v in os.environ.items() if k.startswith("FAKE_")}
env.update(HOME=os.environ["FAKE_HOST_HOME"], PATH=os.environ["FAKE_HOST_PATH"], LANG="C.UTF-8",
           PEACOCK_DATA_DIR=os.environ["FAKE_HOST_DATA"], PEACOCK_NVIDIA_PARAMS=os.environ["FAKE_HOST_PARAMS"])
sys.exit(subprocess.run(["bash", "-c", command], env=env, cwd=os.environ["FAKE_HOST_HOME"]).returncode)
'''

FAKE_DOCKER = r'''#!/usr/bin/env python3
import fcntl, json, os, pathlib, sys
a = sys.argv[1:]
with open(os.environ["FAKE_DOCKER_LOG"], "a") as f:
    f.write(json.dumps(a) + "\n")
path = os.environ["FAKE_DOCKER_WORLD"]
lock = open(path + ".lock", "w")
fcntl.flock(lock, fcntl.LOCK_EX)
w = json.load(open(path))

def save():
    json.dump(w, open(path, "w"))

def flag(name):
    return a[a.index(name) + 1]

if a[0] == "info":
    if "--format" in a:
        print(json.dumps({r: {} for r in w["runtimes"]}))
    sys.exit(0)
if a[0] == "pull":
    sys.exit(0 if a[-1] in w["images"] else 1)
if a[:2] == ["image", "inspect"]:
    image = w["images"].get(a[-1])
    if image is None:
        print(f"Error: No such image: {a[-1]}", file=sys.stderr)
        sys.exit(1)
    fmt = flag("--format")
    if ".Id" in fmt:
        print(image["id"])
    else:
        label = fmt.split('"')[1]
        print(image["labels"].get(label, "<no value>"))
    sys.exit(0)
if a[0] == "ps":
    name = flag("--filter").split("=", 1)[1].strip("^$")
    if name in w["containers"]:
        print(w["containers"][name]["state"])
    sys.exit(0)
if a[0] == "run" and "-d" in a:
    name = flag("--name")
    if name in w["containers"]:
        print(f"docker: Error response from daemon: Conflict. The container name /{name} is already in use",
              file=sys.stderr)
        sys.exit(125)
    w["containers"][name] = {"state": "exited", "code": w["run_code"]}
    save()
    print("f00dfeed" * 8)
    sys.exit(0)
if a[0] == "run":
    gen = [c for c in a if c.endswith("/generate_testdata.sh")]
    stage = pathlib.Path(gen[0]).parent
    bench = a[a.index("--bench") + 1]
    (stage / f"{bench}.sf1").mkdir()
    (stage / f"{bench}.sf1" / "generated").write_text(bench)
    sys.exit(0)
if a[0] == "logs":
    print(w["run_output"], end="")
    sys.exit(0)
if a[0] == "wait":
    print(w["containers"][a[-1]]["code"])
    sys.exit(0)
if a[0] == "rm":
    w["containers"].pop(a[-1], None)
    save()
    sys.exit(0)
if a[0] == "create":
    cid = f"c{len(w['containers'])}"
    w["containers"][cid] = {"state": "created", "image": a[-1]}
    save()
    print(cid)
    sys.exit(0)
if a[0] == "cp":
    dst = pathlib.Path(a[2])
    (dst / "lib").mkdir(parents=True, exist_ok=True)
    (dst / "lib" / "libcudf.so").write_text(w["containers"][a[1].split(":")[0]]["image"])
    sys.exit(0)
sys.exit(f"fake docker: no answer for {a}")
'''

HOST_BIN = {
    "docker": FAKE_DOCKER,
    "nvidia-smi": '#!/usr/bin/env python3\nimport json, os\n'
                  'print(json.load(open(os.environ["FAKE_DOCKER_WORLD"]))["driver"])\n',
    "hostname": f'#!/bin/sh\necho {HOSTNAME}\n',
    "sudo": '#!/bin/sh\nprintf "%s\\n" "$*" >> "$FAKE_HOST_LOG"\nexec "$@"\n',
    "install": r'''#!/usr/bin/env python3
import os, pathlib, sys
a = sys.argv[1:]
mode, paths, i = None, [], 0
while i < len(a):
    if a[i] in ("-m", "-o", "-g"):
        mode = int(a[i + 1], 8) if a[i] == "-m" else mode
        i += 2
    else:
        paths.append(a[i]); i += 1
src, dst = paths
dst = pathlib.Path(os.environ["FAKE_HOST_ROOT"]) / dst.lstrip("/")
dst.parent.mkdir(parents=True, exist_ok=True)
dst.write_bytes(sys.stdin.buffer.read() if src == "/dev/stdin" else pathlib.Path(src).read_bytes())
''',
}
# Present and recording: a call is asserted on, or must never happen.
for _name in ("update-initramfs", "apt-get", "nvidia-ctk", "nsys", "systemctl", "usermod", "gpg"):
    HOST_BIN[_name] = f'#!/bin/sh\nprintf "%s\\n" "{_name} $*" >> "$FAKE_HOST_LOG"\n'
HOST_BIN["apt-get"] += "exit 1\n"

LOUD = '#!/bin/sh\nprintf "%s\\n" "$0 $*" >> "$FAKE_DEV_LOG"\nexit 97\n'


def label(tree, kind, change=None):
    """The image's peacockdb.inputs label for <tree>; with `change`, that input's value is another."""
    text = lib(tree, "image-inputs.sh", f'image_inputs "{tree}" {kind} {CUDF}')
    lines = [f"{ln.split('=')[0]}=0000" if change and ln.startswith(change + "=") else ln
             for ln in text.splitlines()]
    return " ".join(lines)


def log_lines(path):
    return [json.loads(ln) for ln in path.read_text().splitlines()] if path.exists() else []


class Host:
    """Dev with a tree, and the VM; `gpu` runs the tree's run-gpu.sh with the fakes."""

    def __enter__(self):
        self.tmp = tempfile.TemporaryDirectory()
        tmp = pathlib.Path(self.tmp.name)
        self.tree = tmp / "dev" / "chain-l"
        self.make_tree()
        self.laptop = tmp / "dev-home"
        (self.laptop / ".config" / "peacock").mkdir(parents=True)
        (self.laptop / ".config" / "peacock" / "peacock.env").write_text(
            "".join(f"{k}={v}\n" for k, v in KEYS.items()))
        self.home = tmp / "vm" / "home" / "dmitry"
        self.home.mkdir(parents=True)
        self.data = tmp / "vm" / "var-lib-peacock"
        self.data.mkdir()
        self.root = tmp / "vm" / "root"
        self.params = tmp / "vm" / "nvidia-params"
        self.params.write_text("ResmanDebugLevel: 4294967295\nRmProfilingAdminOnly: 0\n")
        self.host_bin, self.dev_bin = tmp / "vm-bin", tmp / "dev-bin"
        for d, files in ((self.host_bin, HOST_BIN), (self.dev_bin, {"ssh": LOUD, "docker": LOUD})):
            d.mkdir()
            for name, text in files.items():
                (d / name).write_text(text)
                (d / name).chmod(0o755)
        self.nebius = tmp / "nebius.sh"
        self.nebius.write_text(FAKE_NEBIUS)
        self.nebius.chmod(0o755)
        self.nebius_log, self.docker_log = tmp / "nebius.log", tmp / "docker.log"
        self.host_log, self.dev_log, self.fetch_log = tmp / "vm.log", tmp / "dev.log", tmp / "fetch.log"
        self.world_path = tmp / "world.json"
        self.world_path.write_text(json.dumps({
            "images": {
                RUN_REF: {"id": "sha256:run1", "labels": {"peacockdb.inputs": label(self.tree, "run"),
                                                          "peacockdb.driver-minimum": DRIVER_MINIMUM}},
                BUILD_REF: {"id": "sha256:build1", "labels": {"peacockdb.inputs": label(self.tree, "build")}},
            },
            "containers": {}, "driver": "550.54.15", "runtimes": ["nvidia", "runc"],
            "run_code": 0, "run_output": "==> run-tests.sh list: green\n",
        }))
        self.env = {
            "PATH": f"{self.dev_bin}:{os.environ['PATH']}", "HOME": str(self.laptop), "LANG": "C.UTF-8",
            "NEBIUS_SH": str(self.nebius), "FAKE_NEBIUS_LOG": str(self.nebius_log),
            "FAKE_DOCKER_LOG": str(self.docker_log), "FAKE_DOCKER_WORLD": str(self.world_path),
            "FAKE_HOST_LOG": str(self.host_log), "FAKE_DEV_LOG": str(self.dev_log),
            "FAKE_FETCH_LOG": str(self.fetch_log), "FAKE_HOST_HOME": str(self.home),
            "FAKE_HOST_PATH": f"{self.host_bin}:{os.environ['PATH']}", "FAKE_HOST_DATA": str(self.data),
            "FAKE_HOST_PARAMS": str(self.params), "FAKE_HOST_ROOT": str(self.root),
        }
        return self

    def __exit__(self, *exc):
        self.tmp.cleanup()

    def make_tree(self):
        t = self.tree
        for rel in TREE_FILES:
            (t / rel).parent.mkdir(parents=True, exist_ok=True)
            if (REPO / rel).exists():
                shutil.copy2(REPO / rel, t / rel)
        shutil.copytree(REPO / "docker", t / "docker")
        fetch = t / "scripts" / "env" / "fetch-sf40.sh"
        fetch.write_text(STUB_FETCH)
        fetch.chmod(0o755)
        committed = {"testdata/goldens/tpch.sf1/q01.txt": "golden\n", "testdata/tpch-queries/q01.sql": "select 1;\n",
                     "testdata/cost-registry.csv": "case,cost\n"}
        for rel, text in committed.items():
            (t / rel).parent.mkdir(parents=True, exist_ok=True)
            (t / rel).write_text(text)
        (t / ".gitmodules").write_text(
            '[submodule "third_party/cudf"]\n\tpath = third_party/cudf\n\turl = https://example.test/cudf.git\n')
        git(t, "init", "-q", "-b", "master")
        git(t, "add", "-A")
        git(t, "update-index", "--add", "--cacheinfo", f"160000,{'ab' * 20},third_party/cudf")
        git(t, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "tree")
        # Untracked: a dataset the tree generated, which never ships, and the staged install.
        (t / "testdata" / "tpch.sf1").mkdir()
        (t / "testdata" / "tpch.sf1" / "lineitem.parquet").write_text("local data")
        self.stage_install(t / "cpp" / "install")

    @staticmethod
    def stage_install(d, mark="staged"):
        for rel in ("lib/libpeacock_gpu.so", "bin/peacock_gpu_tests", "rust-tests/test_gpu_corpus"):
            (d / rel).parent.mkdir(parents=True, exist_ok=True)
            (d / rel).write_text(mark)
        return d

    def gpu(self, *args, env=None):
        cmd = [str(self.tree / "scripts" / "env" / "run-gpu.sh"), *map(str, args)]
        return subprocess.run(cmd, env={**self.env, **(env or {})}, stdin=subprocess.DEVNULL, capture_output=True,
                              text=True, timeout=120, cwd=self.tree)

    def world(self, **changes):
        w = json.loads(self.world_path.read_text())
        w.update(changes)
        self.world_path.write_text(json.dumps(w))
        return w

    def nebius_calls(self):
        return log_lines(self.nebius_log)

    def verbs(self):
        """Each nebius.sh call's verb, its options skipped."""
        out = []
        for a in self.nebius_calls():
            i = 0
            while i < len(a) and a[i] in ("--instance-id", "--profile"):
                i += 2
            out.append(a[i] if i < len(a) else None)
        return out

    def docker_calls(self):
        return log_lines(self.docker_log)

    def docker_runs(self):
        return [a for a in self.docker_calls() if a[:1] == ["run"] and "-d" in a]

    def host_lines(self):
        return self.host_log.read_text().splitlines() if self.host_log.exists() else []

    def fetches(self):
        return log_lines(self.fetch_log)

    def vm_dir(self, name="chain-l"):
        return self.home / "peacock" / name
