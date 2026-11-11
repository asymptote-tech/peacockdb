"""A GPU VM behind a fake scripts/nebius.sh, and a tree on dev, for run-gpu.sh's tests.

The tree is a git clone of the scripts run-gpu.sh ships and reads: docker/, the libraries, the
generator and a little committed testdata, with an untracked dataset that must never ship, and a
staged cpp/install. Its fetch-sf40.sh is a stub that records its argv and stdin and makes each
dataset's directory. `NEBIUS_SH` is a fake that records each call's argv and, for `ssh` and `rsh`,
runs the command on the VM: bash in the VM user's home, with the VM's fakes first on PATH, the data
directory as PEACOCK_DATA_DIR, and its own copy of /proc/driver/nvidia/params. Its `rsh` exits 255
before running anything while the Host has made no more than FAKE_NEBIUS_RSH_FAILS rsh calls, as a
dropped transport does. `rsync` is the real one, through that rsh. On the VM, docker answers from a
JSON world (images and their labels, containers), and reads a run's binds as machine.py's docker
does, refusing one into the VM user's home. A device run (`--gpus`) is attached: its container runs
in the world, prints the world's run_output, and holds while the file FAKE_HOLD names exists, then
ends with run_code; `docker stop` ends it with 143 and `docker rm -f` with 137. With run_record, it
first writes those rows to its calibration record, as a bench does. A second device run while one is
on the card fails, as two pools on one card do. nvidia-smi answers its one query from
the world's cards; sudo records and runs its command, install writes /etc into the VM's root, and
hostname names the machine. On dev, ssh and docker are fakes that fail loudly: no call may bypass
nebius.sh. The card lock is PEACOCK_CARD_LOCK, under the test's temp dir, shared by every VM user.
"""
import copy
import fcntl
import json
import os
import pathlib
import signal
import shutil
import subprocess
import tempfile
import time

from machine import KEYS, REPO, RUN_OPTIONS, git, lib

TREE_FILES = [
    "scripts/env/run-gpu.sh", "scripts/env/run-gpu-vm.sh", "scripts/env/check-build-image.sh", "scripts/env/sf40.sha256",
    "scripts/lib/image-inputs.sh", "scripts/lib/data-inputs.sh", "scripts/lib/machine.sh", "scripts/lib/peacock-config.sh",
    "scripts/lib/s3.sh", "scripts/lib/pinned.sh", "scripts/lib/ssh-retry.sh", "testdata/generate_testdata.sh",
    "cpp/CMakeLists.txt",
]
CUDF = "25.02"
H100 = ["--instance-id", "computeinstance-e00e6yena4nzjgaym0", "--profile", "editor-eu"]
L40S = ["--instance-id", "computeinstance-e00tnrse7ayntnzcyt"]
CPU_VM = ["--instance-id", "computeinstance-e01t2mm4265y8x88ey", "--profile", "eu-west1"]
HOSTNAME = "h100-test-node"
DRIVER_MINIMUM = "535.104.05"
DRIVER_MINIMUM_PTX = "575.51.03"

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
# FAKE_NEBIUS_RC_ON narrows the refusal to the calls whose argv holds that text.
if os.environ.get("FAKE_NEBIUS_RC") and os.environ.get("FAKE_NEBIUS_RC_ON", "") in " ".join(a):
    print("nebius: refused by the test", file=sys.stderr)
    sys.exit(int(os.environ["FAKE_NEBIUS_RC"]))

def verb_of(argv):
    i = 0
    while i < len(argv) and argv[i] in ("--instance-id", "--profile"):
        i += 2
    return i

i = verb_of(a)
verb, rest = a[i], a[i + 1:]
if verb == "ssh":
    command = " ".join(rest)
elif verb == "rsh":
    command = " ".join(rest[1:])
    calls = [json.loads(ln) for ln in open(os.environ["FAKE_NEBIUS_LOG"])]
    if sum(c[verb_of(c)] == "rsh" for c in calls) <= int(os.environ.get("FAKE_NEBIUS_RSH_FAILS", "0")):
        print("ssh: connect to host 203.0.113.7 port 22: Connection timed out", file=sys.stderr)
        sys.exit(255)
else:
    sys.exit(0)
env = {k: v for k, v in os.environ.items() if k.startswith("FAKE_")}
env.update(HOME=os.environ["FAKE_HOST_HOME"], PATH=os.environ["FAKE_HOST_PATH"], LANG="C.UTF-8",
           PEACOCK_DATA_DIR=os.environ["FAKE_HOST_DATA"], PEACOCK_NVIDIA_PARAMS=os.environ["FAKE_HOST_PARAMS"])
sys.exit(subprocess.run(["bash", "-c", command], env=env, cwd=os.environ["FAKE_HOST_HOME"]).returncode)
'''

FAKE_DOCKER = r'''#!/usr/bin/env python3
import fcntl, json, os, pathlib, re, sys, time
''' + RUN_OPTIONS + r'''
a = sys.argv[1:]
with open(os.environ["FAKE_DOCKER_LOG"], "a") as f:
    f.write(json.dumps(a) + "\n")
path = os.environ["FAKE_DOCKER_WORLD"]
lock = open(path + ".lock", "w")
fcntl.flock(lock, fcntl.LOCK_EX)
w = json.load(open(path))

def save():
    # Whole or not at all: nvidia-smi and the tests read the world without the lock.
    json.dump(w, open(path + ".new", "w"))
    os.replace(path + ".new", path)

def relocked(seconds):
    """The world let go for <seconds>, then taken and read again."""
    global w
    fcntl.flock(lock, fcntl.LOCK_UN)
    time.sleep(seconds)
    fcntl.flock(lock, fcntl.LOCK_EX)
    w = json.load(open(path))

def flag(name):
    return a[a.index(name) + 1]

def alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except (TypeError, ProcessLookupError):
        return False

# info_delays: seconds each next `docker info` takes, in turn.
if a[0] == "info":
    if w.get("info_delays"):
        delay = w["info_delays"].pop(0)
        save()
        relocked(delay)
    if "--format" in a:
        print(json.dumps({r: {} for r in w["runtimes"]}))
    sys.exit(0)
# A pull of an image the world lacks fails as dev's docker does on an unpublished tag, or with the
# world's pull_error, such as a network error.
if a[0] == "pull":
    if a[-1] in w["images"]:
        sys.exit(0)
    print(w.get("pull_error") or f'Error response from daemon: failed to resolve reference "{a[-1]}": {a[-1]}: not found',
          file=sys.stderr)
    sys.exit(1)
if a[:2] == ["image", "inspect"]:
    image = w["images"].get(a[-1])
    if image is None:
        print(f"Error: No such image: {a[-1]}", file=sys.stderr)
        sys.exit(1)
    fmt = flag("--format")

    def field(m):
        label = re.fullmatch(r'index \.Config\.Labels "([^"]+)"', m.group(1))
        if label:
            return image["labels"].get(label.group(1), "<no value>")
        if m.group(1) in (".Id", '"\\n"'):
            return image["id"] if m.group(1) == ".Id" else "\n"
        # A code nothing maps: 1 is "No such image", and 64 reads as nebius.sh's usage refusal.
        print(f"fake docker: no answer for the format {fmt}", file=sys.stderr)
        sys.exit(99)

    print(re.sub(r"\{\{(.*?)\}\}", field, fmt))
    # moves: {ref: [n, image]}, the tag another pull moves right after its nth read from now.
    move = w.get("moves", {}).get(a[-1])
    if move:
        move[0] -= 1
        if move[0] == 0:
            w["images"][a[-1]] = w["moves"].pop(a[-1])[1]
        save()
    sys.exit(0)
if a[0] == "ps":
    name = flag("--filter").split("=", 1)[1].strip("^$")
    if name in w["containers"]:
        c = w["containers"][name]

        def field(m):
            label = re.fullmatch(r'\.Label "([^"]+)"', m.group(1))
            if m.group(1) == ".State":
                return c["state"]
            if label:
                return c.get("labels", {}).get(label.group(1), "")
            print(f"fake docker: no answer for the format {flag('--format')}", file=sys.stderr)
            sys.exit(99)

        print(re.sub(r"\{\{(.*?)\}\}", field, flag("--format")))
    sys.exit(0)
if a[0] == "run":
    binds, _ = run_options(a)
# A device run, attached. create_delay: the client still making the container, before it exists.
if a[0] == "run" and "--gpus" in a:
    name = flag("--name")
    if w.get("create_delay"):
        relocked(w["create_delay"])
    if name in w["containers"]:
        print(f"docker: Error response from daemon: Conflict. The container name /{name} is already in use",
              file=sys.stderr)
        sys.exit(125)
    if w.get("on_card"):
        print(f"fake docker: {w['on_card']} is on the card: two pools on one card fail each other", file=sys.stderr)
        sys.exit(125)
    labels = dict(a[i + 1].split("=", 1) for i, x in enumerate(a) if x == "--label")
    # run_record: rows the run adds to its calibration record, as a bench does; refused, exit 2, while
    # the record holds rows, as run-tests.sh refuses an earlier run's.
    if w.get("run_record") is not None:
        record = pathlib.Path(host_path(binds, "/peacock/testdata/calibration/records.tsv"))
        if record.exists() and record.stat().st_size:
            print("run-tests.sh: testdata/calibration/records.tsv holds an earlier run's rows", file=sys.stderr)
            sys.exit(2)
        record.parent.mkdir(parents=True, exist_ok=True)
        with open(record, "a") as f:
            f.write(w["run_record"])
    print(w["run_output"], end="", flush=True)  # in the log before the world says it runs
    w["containers"][name] = {"state": "running", "code": None, "pid": os.getpid(), "labels": labels}
    w["on_card"] = name
    save()
    held = time.time() + 60  # a test that never lets go fails rather than hangs
    while True:
        c = w["containers"].get(name)
        if c is None or c.get("stop") or not os.path.exists(os.environ["FAKE_HOLD"]) or time.time() > held:
            break
        relocked(0.05)
    code = 137 if c is None else 143 if c.get("stop") else w["run_code"]
    if c is not None:
        w["containers"][name] = {"state": "exited", "code": code, "labels": labels}
    w["on_card"] = None
    save()
    sys.exit(code)
# `docker stop -t <s> <name>`: back once the container has ended.
if a[0] == "stop":
    name = a[-1]
    if name not in w["containers"]:
        print(f"Error response from daemon: No such container: {name}", file=sys.stderr)
        sys.exit(1)
    if w["containers"][name]["state"] == "running":
        w["containers"][name]["stop"] = True
        save()
        while name in w["containers"] and w["containers"][name]["state"] == "running":
            relocked(0.05)
    print(name)
    sys.exit(0)
if a[0] == "run":
    gen = [c for c in a if c.endswith("/generate_testdata.sh")]
    stage = pathlib.Path(host_path(binds, gen[0])).parent
    bench = a[a.index("--bench") + 1]
    (stage / f"{bench}.sf1").mkdir()
    (stage / f"{bench}.sf1" / "generated").write_text(bench)
    sys.exit(0)
# `docker rm [-f] <name>`: a running one only with -f, which kills it. vanish_on_rm: the container
# ended and went between the caller's look and its rm.
if a[0] == "rm":
    c = w["containers"].get(a[-1])
    if c is None or w.get("vanish_on_rm"):
        w["containers"].pop(a[-1], None)
        save()
        print(f"Error response from daemon: No such container: {a[-1]}", file=sys.stderr)
        sys.exit(1)
    if c["state"] == "running" and "-f" not in a:
        print(f"Error response from daemon: cannot remove container {a[-1]}: container is running", file=sys.stderr)
        sys.exit(1)
    del w["containers"][a[-1]]
    if w.get("on_card") == a[-1] and not alive(c.get("pid")):
        w["on_card"] = None  # its client was killed with its runner: nothing else lets the card go
    save()
    print(a[-1])
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
    # One line a card; a card given as a string is printed as it is. World smi_fails: [code, text],
    # printed on stdout as nvidia-smi prints its own failures.
    "nvidia-smi": r'''#!/usr/bin/env python3
import json, os, sys
if sys.argv[1:] != ["--query-gpu=driver_version,compute_cap", "--format=csv,noheader"]:
    sys.exit(f"fake nvidia-smi: no answer for {sys.argv[1:]}")
w = json.load(open(os.environ["FAKE_DOCKER_WORLD"]))
if w.get("smi_fails"):
    print(w["smi_fails"][1])
    sys.exit(w["smi_fails"][0])
for card in w["cards"]:
    print(card if isinstance(card, str) else ", ".join(card))
''',
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


def inputs_ref(tree, kind):
    """The image of the tree's own inputs, by its inputs tag: what the scripts default to."""
    return f"ghcr.io/asymptote-tech/peacockdb-{kind}:" + lib(tree, "image-inputs.sh",
                                                             f'image_inputs_tag "{tree}" {kind} {CUDF}')


def alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False


def started_at(pid):
    """The process's start time, field 22 of /proc/<pid>/stat, as run/state keeps it; None when gone."""
    try:
        return open(f"/proc/{pid}/stat").read().rsplit(") ", 1)[1].split()[19]
    except FileNotFoundError:
        return None


def until(condition, what, seconds=30):
    deadline = time.time() + seconds
    while not condition():
        assert time.time() < deadline, f"never: {what}"
        time.sleep(0.05)


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
        self.run_ref, self.build_ref = inputs_ref(self.tree, "run"), inputs_ref(self.tree, "build")
        self.world_path.write_text(json.dumps({
            "images": {
                self.run_ref: {"id": "sha256:run1",
                               "labels": {"peacockdb.inputs": label(self.tree, "run"),
                                          "peacockdb.driver-minimum": DRIVER_MINIMUM,
                                          "peacockdb.driver-minimum-ptx": DRIVER_MINIMUM_PTX}},
                self.build_ref: {"id": "sha256:build1", "labels": {"peacockdb.inputs": label(self.tree, "build")}},
            },
            "containers": {}, "cards": [["550.54.15", "8.9"]], "runtimes": ["nvidia", "runc"],
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
            "FAKE_HOLD": str(tmp / "hold"), "PEACOCK_CARD_LOCK": str(tmp / "vm" / "run-lock" / "peacock-card.lock"),
            "PEACOCK_SSH_PAUSES": "0 0 0",
        }
        (tmp / "vm" / "run-lock").mkdir()
        self.hold, self.card_lock = tmp / "hold", pathlib.Path(self.env["PEACOCK_CARD_LOCK"])
        self.background = []
        return self

    def __exit__(self, *exc):
        """Kills what a test left going: its background run-gpu.sh calls, and every runner a run/state
        names, with its process group (its docker client, its wait for the card): a pid of this boot
        whose start time is the state's, never another process that took it."""
        for p in self.background:
            if p.poll() is None:
                os.killpg(p.pid, signal.SIGKILL)
            p.communicate()
        boot = pathlib.Path("/proc/sys/kernel/random/boot_id").read_text().strip()
        runners = []
        for state in (pathlib.Path(self.tmp.name) / "vm" / "home").glob("*/peacock/*/run/state"):
            fields = state.read_text().split()
            pid, boot_id, start = fields[0], fields[1], fields[4] if len(fields) > 4 else None
            if boot_id == boot and start is not None and started_at(int(pid)) == start:
                runners.append(int(pid))
                try:
                    os.killpg(int(pid), signal.SIGKILL)
                except ProcessLookupError:
                    pass
        deadline = time.time() + 10
        while any(alive(pid) for pid in runners) and time.time() < deadline:
            time.sleep(0.05)
        self.tmp.cleanup()

    def second_user(self):
        """Another user of the same VM: a home of its own, the same world, data directory and card
        lock. Its runners are killed with the Host's."""
        other = copy.copy(self)
        other.home = self.home.parent / "second"
        other.home.mkdir(exist_ok=True)
        other.env = {**self.env, "FAKE_HOST_HOME": str(other.home)}
        return other

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

    def gpu(self, *args, env=None, vm=H100):
        """run-gpu.sh <args>, naming the VM by `vm` (the H100) unless the args name one already."""
        if vm and "--instance-id" not in args:
            args = (*vm, *args)
        cmd = [str(self.tree / "scripts" / "env" / "run-gpu.sh"), *map(str, args)]
        return subprocess.run(cmd, env={**self.env, **(env or {})}, stdin=subprocess.DEVNULL, capture_output=True,
                              text=True, timeout=120, cwd=self.tree)

    def gpu_going(self, *args, env=None, vm=H100):
        """run-gpu.sh <args> in the background, in a session of its own, as a terminal runs it."""
        if vm and "--instance-id" not in args:
            args = (*vm, *args)
        cmd = [str(self.tree / "scripts" / "env" / "run-gpu.sh"), *map(str, args)]
        p = subprocess.Popen(cmd, env={**self.env, **(env or {})}, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE, text=True, cwd=self.tree, start_new_session=True)
        self.background.append(p)
        return p

    def script(self, rel, *args, env=None):
        """The tree's script <rel> with the fakes, as `gpu` runs run-gpu.sh."""
        cmd = [str(self.tree / rel), *map(str, args)]
        return subprocess.run(cmd, env={**self.env, **(env or {})}, stdin=subprocess.DEVNULL, capture_output=True,
                              text=True, timeout=120, cwd=self.tree)

    def world(self, **changes):
        """The world, with <changes> made under the fake docker's lock: a runner's docker may be at it."""
        with open(f"{self.world_path}.lock", "w") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            w = json.loads(self.world_path.read_text())
            if changes:
                w.update(changes)
                pathlib.Path(f"{self.world_path}.new").write_text(json.dumps(w))
                os.replace(f"{self.world_path}.new", self.world_path)
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
        """The device runs: each `docker run` with `--gpus`."""
        return [a for a in self.docker_calls() if a[:1] == ["run"] and "--gpus" in a]

    def host_lines(self):
        return self.host_log.read_text().splitlines() if self.host_log.exists() else []

    def fetches(self):
        return log_lines(self.fetch_log)

    def vm_dir(self, name="chain-l"):
        return self.home / "peacock" / name
