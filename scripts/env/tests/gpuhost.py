"""A built tree with gpu.sh, and a GPU host it reaches, for gpu.sh's and the adapters' tests.

The host is a directory reached through laptop.py's fake ssh and a fake rsync that maps
`<user>@<address>:<path>` there and runs the real rsync. Its `bin/` fakes docker, nvidia-smi,
hostname, id, sudo and curl, each answering only the exact query made. docker runs a device run as
a sleeper held while `hold` exists, logged in `runs.log` and refused beside another, and the sf40
fetch through a fake aws; `hold_gen` holds the generator, `hold_smi` the driver query. A container
reads a path through the mount that holds it, and refuses one in the host's homes, as the image
refuses its own /home/peacock to another uid. `docker stop` ends a run as killed; `create_delay`
slows a container's making, and `vanish_on_stop` ends one as its stop comes. sudo runs only
`install -d` and a usermod's grant; FAKE_RSYNC_GATE holds rsync. `nebius` is strict: start and stop
need `--async`, get `--format json`, a start while STOPPING fails, a transition takes two gets.
"""
import fcntl
import hashlib
import json
import os
import pathlib
import select
import shutil
import signal
import subprocess
import tempfile
import time

from laptop import FAKE_SSH
from machine import KEYS, REPO, git, lib, path_without

TREE_FILES = [
    "scripts/env/gpu.sh", "scripts/env/on-gpu-host.sh", "scripts/env/host/ssh.sh", "scripts/env/host/nebius.sh",
    "scripts/env/provision-vm.sh", "scripts/lib/peacock-config.sh", "scripts/lib/host.sh",
    "scripts/lib/machine.sh", "scripts/lib/data-inputs.sh", "scripts/lib/image-inputs.sh", "scripts/lib/s3.sh",
    "scripts/lib/portable.sh",
    "testdata/generate_testdata.sh", "cpp/CMakeLists.txt",
]
FIXTURES = ["testdata/tpch-queries/q1.sql", "testdata/tpcds-queries/q1.sql", "testdata/tpch.minimal/lineitem.parquet",
            "testdata/goldens/tpch/q1.cpu.txt", "testdata/tpch-vec-queries/query_params.jsonl",
            "testdata/pbench-queries/p1.sql", "testdata/pbench.sf1/t.parquet", "testdata/cost-registry.csv",
            "testdata/fixtures/README.md"]
INSTALL = ["cpp/install/lib/libpeacock_gpu.so", "cpp/install/bin/peacock_ffi_tests",
           "cpp/install/rust-tests/test_gpu_corpus", "cpp/install/rust-benchmarks/peacock_gpu_benchmarks"]
NAME, ADDRESS, USER = "l40s", "gpu1.test", "ubuntu"
INSTANCE = "dev1-A"
INSTANCE_ID = "computeinstance-fake0001"
GPU_PUB = "ssh-ed25519 AAAAgpukeyfortests peacock-gpu dev1"
HOSTNAME = "gpu-box-7"
DRIVER = "580.65.06"
CAP = "8.9"
RUN = "ghcr.io/asymptote-tech/peacockdb-run"
BUILD = "ghcr.io/asymptote-tech/peacockdb-build"
COMMIT = "0123456789ab"
SLUG = "h100-80gb-hbm3"
SECRET = "private-key-material-never-printed"
BUCKETS = {"tpch-sf40": {"lineitem.parquet": "tpch lineitem at sf40", "orders.parquet": "tpch orders"},
           "tpcds-sf40": {"store_sales.parquet": "tpcds store_sales at sf40"}}
DRIVER_QUERY = ["--query-gpu=driver_version,compute_cap", "--format=csv,noheader"]
APPS_QUERY = ["--query-compute-apps=pid", "--format=csv,noheader"]
LABEL_FORMAT = '{{index .Config.Labels "peacockdb.inputs"}}'

FAKE_RSYNC = r'''#!/usr/bin/env python3
import json, os, pathlib, re, shlex, subprocess, sys
a = sys.argv[1:]
with open(os.environ["FAKE_RSYNC_LOG"], "a") as f:
    f.write(json.dumps(a) + "\n")
with open(os.environ["FAKE_RSYNC_LOG"] + ".env", "a") as f:  # the client's one variable its server's argv shows
    f.write(json.dumps(os.environ.get("RSYNC_PROTECT_ARGS")) + "\n")
if set(a) & {"--ignore-missing-args", "--mkpath"}:  # as macOS's rsync 2.6.9 and openrsync answer
    sys.exit(f"rsync: {sorted(set(a) & {'--ignore-missing-args', '--mkpath'})[0]}: unknown option")
gate = os.environ.get("FAKE_RSYNC_GATE")
while gate and os.path.exists(gate):  # a test holds the transfer open while it looks
    __import__("time").sleep(0.05)
if os.environ.get("FAKE_RSYNC_EXIT"):
    sys.exit(int(os.environ["FAKE_RSYNC_EXIT"]))
out, rsh, i = [], None, 0
while i < len(a):
    if a[i] == "-e":
        rsh = shlex.split(a[i + 1]); i += 2; continue
    m = re.match(r"^([^@/:]+)@([^:/]+):(.*)$", a[i])
    if m:
        user, address, path = m.groups()
        host = pathlib.Path(os.environ["FAKE_VMS"]) / address
        home = next(ln.split(":")[5] for ln in (host / "passwd").read_text().splitlines() if ln.split(":")[0] == user)
        key = rsh[rsh.index("-i") + 1] if rsh and "-i" in rsh else None
        keys = pathlib.Path(home) / ".ssh" / "authorized_keys"
        if key is None or not keys.is_file() or \
                pathlib.Path(key + ".pub").read_text().strip() not in keys.read_text().splitlines():
            sys.exit(f"{user}@{address}: Permission denied (publickey).\nrsync: connection unexpectedly closed")
        out.append(path if path.startswith("/") else f"{home}/{path}")
    else:
        out.append(a[i])
    i += 1
sys.exit(subprocess.run([os.environ["FAKE_REAL_RSYNC"], *out]).returncode)
'''

FAKE_NEBIUS = r'''#!/usr/bin/env python3
import json, os, sys
a = sys.argv[1:]
with open(os.environ["FAKE_NEBIUS_LOG"], "a") as f:
    f.write(json.dumps(a) + "\n")
path = os.environ["FAKE_NEBIUS_STATE"]
st = json.load(open(path))
if "--config" not in a or not os.path.exists(a[a.index("--config") + 1]):
    sys.exit("Error: no profile configured")
words = [w for w in a if not w.startswith("-")]
verb = words[words.index("instance") + 1]
fail = st.get("fail") or {}
# `after`: the calls of that verb let through first, so a get can answer an ip and fail a status.
if fail.get("verb") in (verb, "any") and fail.get("seen", 0) >= fail.get("after", 0):
    print("Error: rpc error: code = InvalidArgument desc = the fake was told to fail", file=sys.stderr)
    sys.exit(fail["exit"])
if fail.get("verb") in (verb, "any"):
    fail["seen"] = fail.get("seen", 0) + 1
    json.dump(st, open(path, "w"))
if a[a.index("--id") + 1] != st["id"]:
    print(f'Error: rpc error: code = NotFound desc = instance not found by id "{a[a.index("--id") + 1]}"', file=sys.stderr)
    sys.exit(13)
def save():
    json.dump(st, open(path, "w"))
if verb == "get":
    if a[a.index("--format") + 1:a.index("--format") + 2] != ["json"]:
        sys.exit("fake nebius: get is called with --format json")
    print(json.dumps({"metadata": {"id": st["id"]}, "status": {"state": st["state"], "network_interfaces": [
        {"public_ip_address": {"address": st["ip"] + "/32"}}] if st["state"] == "RUNNING" else []}}))
    if st["state"] in ("STARTING", "STOPPING") and not st.get("stuck"):
        st["left"] = st.get("left", 2) - 1
        if st["left"] <= 0:
            st["state"] = "RUNNING" if st["state"] == "STARTING" else "STOPPED"
            st.pop("left")
        save()
elif verb in ("start", "stop"):
    if "--async" not in a:
        sys.exit("fake nebius: start and stop are called with --async")
    if verb == "start" and st["state"] in ("STOPPING", "STARTING", "RUNNING"):
        print(f"Error: rpc error: code = FailedPrecondition desc = instance is {st['state']}", file=sys.stderr)
        sys.exit(9)
    st["state"] = "STARTING" if verb == "start" else "STOPPING"
    st.pop("left", None)
    st.pop("stuck", None) if verb == "stop" else None
    save()
else:
    sys.exit(f"fake nebius: no answer for {a}")
'''

LIBTEST_GREEN = ("running 1 test\ntest bench_tpch_sf40_q1_tp1_single ... ok\n\n"
                 "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s")

HOST_DOCKER = r'''#!/usr/bin/env python3
import hashlib, json, os, pathlib, subprocess, sys, time
LIBTEST_GREEN = %r
host = pathlib.Path(os.environ["FAKE_VM"])
a = sys.argv[1:]
with open(host / "docker.log", "a") as f:
    f.write(json.dumps(a) + "\n")
images = json.loads((host / "images.json").read_text())
def load(name, empty):
    p = host / name
    return json.loads(p.read_text()) if p.exists() else empty
pulled, ids = load("pulled.json", []), load("ids.json", {})
if a[0] == "pull":
    if len(a) != 2 or a[1] not in images:
        sys.exit(f"Error response from daemon: manifest unknown: {a[-1]}")
    (host / "pulled.json").write_text(json.dumps(sorted(set(pulled + [a[1]]))))
    sys.exit(0)
if a[:2] == ["image", "inspect"]:
    if len(a) != 5 or a[2] != "--format" or a[3] not in ('{{.Id}}', '{{index .Config.Labels "peacockdb.inputs"}}'):
        sys.exit(f"fake docker: no answer for {a}")
    name = a[4]
    if name in ids:  # an image ID: the image it was when its ID was read
        label = ids[name]["label"]
        print(name if a[3] == "{{.Id}}" else (label if label is not None else "<no value>"))
        sys.exit(0)
    if name not in pulled:
        sys.exit(f"Error: No such image: {name}")
    label = images[name]
    if a[3] == "{{.Id}}":
        image_id = "sha256:" + hashlib.sha256(f"{name} {label}".encode()).hexdigest()
        ids[image_id] = {"ref": name, "label": label}
        (host / "ids.json").write_text(json.dumps(ids))
        print(image_id)
        moves = json.loads((host / "move.json").read_text()) if (host / "move.json").exists() else {}
        if name in moves:  # another tree pulls the tag right after this read
            images[name] = moves.pop(name)
            (host / "images.json").write_text(json.dumps(images))
            (host / "move.json").write_text(json.dumps(moves))
    else:
        print(label if label is not None else "<no value>")
    sys.exit(0)
if a[:2] == ["container", "inspect"]:
    if len(a) != 3:
        sys.exit(f"fake docker: no answer for {a}")
    sys.exit(0 if (host / "containers" / a[2]).exists() else 1)
def alive(box):  # a container file holds its run's pid; one no process runs is a killed runner's
    try:
        os.kill(int(box.read_text() or 0), 0)
        return box.read_text() != ""
    except (ProcessLookupError, ValueError):
        return False
if a[0] in ("stop", "rm"):  # `docker stop -t <seconds> <name>`, back once it has ended; `docker rm -f <name>`
    if a[:-1] not in (["stop", "-t", "30"], ["rm", "-f"]):
        sys.exit(f"fake docker: no answer for {a}")
    box = host / "containers" / a[-1]
    if a[0] == "stop" and (host / "vanish_on_stop").exists() and box.exists():  # it ended as stop was called
        box.unlink()
    if not box.exists():
        sys.exit(f"Error response from daemon: No such container: {a[-1]}")
    if alive(box):
        (host / "containers" / f"{a[-1]}.stop").write_text("")
        while box.exists():
            time.sleep(0.05)
    else:
        box.unlink()
    print(a[-1])
    sys.exit(0)
if a[0] != "run":
    sys.exit(f"fake docker: no answer for {a}")
mounts, env, i, workdir = [], {}, 1, None
while a[i].startswith("-"):
    if a[i] in ("-d", "--rm", "--init", "-i"):
        i += 1; continue
    flag, value = a[i], a[i + 1]
    if flag == "--mount":
        kv = dict(p.split("=", 1) if "=" in p else (p, True) for p in value.split(","))
        if not os.path.exists(kv["src"]):
            sys.exit(f"docker: invalid mount config: bind source path does not exist: {kv['src']}")
        mounts.append((kv["src"], kv["dst"], "readonly" in kv))
    elif flag == "-e":
        k, _, v = value.partition("="); env[k] = v
    elif flag == "-w":
        workdir = value
    i += 2
image, cmd = a[i], a[i + 1:]
if image not in pulled and image not in ids:
    sys.exit(f"Unable to find image '{image}' locally")
def closed(path):
    """The host's homes stand for the image's own /home/peacock, closed to every uid but its own: a
    container path there is refused, as it is in the image."""
    if path.startswith(str(host / "home") + "/"):
        sys.exit(f"bash: {path}: Permission denied")
for _, dst, _ in mounts:
    closed(dst)
if workdir:
    closed(workdir)
def outside(word):
    """A container path as the host path its mount holds."""
    closed(word)
    for src, dst, _ in sorted(mounts, key=lambda m: -len(m[1])):
        if word == dst or word.startswith(dst.rstrip("/") + "/"):
            return src + word[len(dst):]
    return word
cmd = [outside(w) for w in cmd]
gen = [c for c in cmd if c.endswith("/generate_testdata.sh")]
if gen:
    while (host / "hold_gen").exists():
        time.sleep(0.05)
    stage = pathlib.Path(gen[0]).parent
    bench = cmd[cmd.index("--bench") + 1]
    (stage / f"{bench}.sf1").mkdir()
    (stage / f"{bench}.sf1" / "generated").write_text(bench)
    sys.exit(0)
if "fetch" in cmd:
    run_env = {k: v for k, v in os.environ.items() if k.startswith("FAKE_")}
    run_env.update(env, PATH=os.environ["FAKE_CONTAINER_PATH"], HOME="/nonexistent")
    sys.exit(subprocess.run(cmd, env=run_env).returncode)
testdata = next(src for src, dst, ro in mounts if dst == "/peacock/testdata")
try:  # one card: a device run beside another is the failure the device lock exists to prevent
    os.close(os.open(host / "device.busy", os.O_CREAT | os.O_EXCL | os.O_WRONLY))
except FileExistsError:
    sys.exit("fake docker: another device run is on the card")
start = time.time()
name = a[a.index("--name") + 1] if "--name" in a else None
if name and (host / "create_delay").exists():  # the client still making the container
    time.sleep(float((host / "create_delay").read_text()))
if name:  # a named container stays on the host while it runs, whatever its client does
    (host / "containers").mkdir(exist_ok=True)
    (host / "containers" / name).write_text(str(os.getpid()))
with open(host / "runs.log", "a") as f:
    f.write(json.dumps({"event": "start", "at": start, "argv": a}) + "\n")
if cmd[:1] == ["bench"]:
    (pathlib.Path(testdata) / "calibration").mkdir(exist_ok=True)
    (pathlib.Path(testdata) / "calibration" / "records.tsv").write_text(f"# run: the host's record of {start}\n")
def stopped():
    return name is not None and (host / "containers" / f"{name}.stop").exists()
ends = time.time() + (float((host / "run_seconds").read_text()) if (host / "run_seconds").exists() else 0)
while time.time() < ends and not stopped():
    time.sleep(0.05)
held = time.time() + 60
while (host / "hold").exists() and time.time() < held and not stopped():  # a test releases the run when it has seen what it waits for
    time.sleep(0.05)
if stopped():  # docker stop's kill: the container ends where it was, its record as far as it got
    with open(host / "runs.log", "a") as f:
        f.write(json.dumps({"event": "stopped", "at": time.time(), "argv": a}) + "\n")
    os.unlink(host / "device.busy")
    os.unlink(host / "containers" / f"{name}.stop")
    os.unlink(host / "containers" / name)
    sys.exit(137)
if cmd[:1] == ["bench"]:
    out = pathlib.Path(testdata) / "benchmark-results" / os.environ["FAKE_SLUG"] / "tpch.sf40"
    out.mkdir(parents=True, exist_ok=True)
    (out / "tp1-single.benchmark.txt").write_text("q1 measured on the host\n")
if cmd[:1] == ["bench"]:  # the harness's libtest output, as a test writes it, or one passing case
    out = host / "bench_out"
    print(out.read_text() if out.exists() else LIBTEST_GREEN, flush=True)
print(f"==> run-tests.sh {cmd[0]}: green", flush=True)
with open(host / "runs.log", "a") as f:
    f.write(json.dumps({"event": "end", "at": time.time(), "argv": a}) + "\n")
os.unlink(host / "device.busy")
if name:
    os.unlink(host / "containers" / name)
sys.exit(int((host / "run_status").read_text()) if (host / "run_status").exists() else 0)
''' % LIBTEST_GREEN

HOST_SUDO = r'''#!/usr/bin/env python3
import os, pathlib, shutil, sys
host = pathlib.Path(os.environ["FAKE_VM"])
with open(host / "sudo.log", "a") as f:
    f.write(" ".join(sys.argv[1:]) + "\n")
a = sys.argv[1:]
if a[:2] == ["apt-get", "install"]:
    for package, binary in (("docker.io", "docker"), ("nvidia-container-toolkit", "nvidia-ctk"), ("rsync", "rsync")):
        if package in a:
            shutil.copy(pathlib.Path(os.environ["FAKE_STASH"]) / binary, host / "bin" / binary)
if a[:2] == ["usermod", "-aG"] and "docker" in a[2].split(","):
    # The group is the user's from their next login on, and with it what the group may: the host's
    # data directory, group docker, 2775. This session keeps the groups it started with.
    data = pathlib.Path(os.environ.get("PEACOCK_DATA_DIR", "/nonexistent"))
    grant = f'printf "%s docker\\n" "$(cat {host / "groups"})" > {host / "groups.new"}\n'
    grant += f'mv {host / "groups.new"} {host / "groups"}\n'
    if str(host) in str(data) and data.is_dir():
        grant += f"chmod 2775 {data}\n"
    (host / "next_session.sh").write_text(grant)
if a[:2] == ["install", "-d"] and str(host) in a[-1]:
    path = pathlib.Path(a[-1])
    path.mkdir(parents=True, exist_ok=True)
    path.chmod(int(a[a.index("-m") + 1], 8))
if not sys.stdin.isatty():
    sys.stdin.read()
'''

AWS = r'''#!/usr/bin/env python3
import hashlib, json, os, pathlib, shutil, sys
a = sys.argv[1:]
def sha(name):
    return hashlib.sha256(os.environ.get(name, "").encode()).hexdigest()
keys = sha("AWS_ACCESS_KEY_ID") == os.environ["FAKE_ID_SHA"] and sha("AWS_SECRET_ACCESS_KEY") == os.environ["FAKE_SECRET_SHA"]
with open(os.environ["FAKE_AWS_LOG"], "a") as f:
    f.write(json.dumps({"argv": a, "keys": keys}) + "\n")
if not keys:
    sys.exit("An error occurred (AccessDenied) when calling the ListObjectsV2 operation")
if os.environ.get("FAKE_AWS_EXIT"):
    print("aws: the fake was told to fail", file=sys.stderr)
    sys.exit(int(os.environ["FAKE_AWS_EXIT"]))
s3 = a.index("s3")
src, dst = [w for w in a[s3 + 2:] if not w.startswith("--")]
root = pathlib.Path(os.environ["FAKE_BUCKETS"]) / src.removeprefix("s3://").strip("/")
if a[s3 + 1] != "cp" or "--recursive" not in a or not root.is_dir():
    sys.exit(f"fake aws: no answer for {a}")
for p in root.iterdir():
    shutil.copy(p, pathlib.Path(dst) / p.name)
'''

NVIDIA_SMI = r'''#!/usr/bin/env python3
import os, pathlib, sys, time
host = pathlib.Path(os.environ["FAKE_VM"])
a = sys.argv[1:]
with open(host / "nvidia-smi.log", "a") as f:
    f.write(" ".join(a) + "\n")
answers = {" ".join(%r): "driver", " ".join(%r): "compute_apps"}
name = answers.get(" ".join(a))
while name == "driver" and (host / "hold_smi").exists():
    time.sleep(0.05)
if name is None:
    sys.exit(f"fake nvidia-smi: no answer for {a}")
if not (host / "driver").exists():
    sys.exit("NVIDIA-SMI has failed because it couldn't communicate with the NVIDIA driver.")
path = host / name
print(path.read_text() if path.exists() else "", end="")
''' % (DRIVER_QUERY, APPS_QUERY)

HOST_BIN = {
    "nvidia-smi": NVIDIA_SMI,
    "hostname": '#!/bin/sh\n[ $# = 0 ] || exit 1\ncat "$FAKE_VM/hostname"\n',
    "id": '#!/bin/sh\ncase "$1" in -nG) cat "$FAKE_VM/groups" ;; -un) echo "$FAKE_USER_NAME" ;; '
          '*) exec /usr/bin/id "$@" ;; esac\n',
    "curl": '#!/bin/sh\necho "deb https://nvidia.github.io/libnvidia-container/stable/deb/\\$(ARCH) /"\n',
    "sudo": HOST_SUDO,
}


def install(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    path.chmod(0o755)


class GpuRig:
    """A built tree with gpu.sh, a GPU host it reaches, and the buckets sf40 comes from."""

    def __enter__(self):
        self.tmp = tempfile.TemporaryDirectory()
        tmp = self.root = pathlib.Path(self.tmp.name)
        self.home = tmp / "instance-home"
        self.tree = tmp / "tree"
        self.hosts = tmp / "hosts"
        self.make_tree()
        self.key = tmp / "keys" / "gpu_ed25519"
        self.key.parent.mkdir()
        self.key.write_text("-----BEGIN OPENSSH PRIVATE KEY----- fake\n")
        self.key.chmod(0o600)
        (tmp / "keys" / "gpu_ed25519.pub").write_text(GPU_PUB + "\n")
        self.bin = tmp / "bin"
        for name, text in (("ssh", FAKE_SSH), ("rsync", FAKE_RSYNC), ("nebius", FAKE_NEBIUS)):
            install(self.bin / name, text)
        self.stash = tmp / "stash"
        install(self.stash / "docker", HOST_DOCKER)
        install(self.stash / "nvidia-ctk", "#!/bin/sh\nexit 0\n")
        install(self.stash / "rsync", "#!/bin/sh\nexit 0\n")
        install(tmp / "container-bin" / "aws", AWS)
        (tmp / "vm-bin").mkdir()
        base = path_without(tmp, {"docker", "nvidia-ctk", "nvidia-smi", "aws", "nebius", "rsync"})
        self.buckets = tmp / "buckets"
        for bucket, files in BUCKETS.items():
            (self.buckets / bucket).mkdir(parents=True)
            for name, text in files.items():
                (self.buckets / bucket / name).write_text(text)
            (self.buckets / bucket / "manifest.json").write_text(
                json.dumps({name: len(text) for name, text in files.items()}, indent=0))
        self.logs = {name: tmp / f"{name}.log" for name in ("ssh", "rsync", "nebius", "aws")}
        self.nebius_state = tmp / "nebius.json"
        self.nebius(state="STOPPED")
        self.env = {
            "PATH": f"{self.bin}:{base}", "HOME": str(self.home), "LANG": "C.UTF-8",
            "GPU_SSH_KEY_FILE": str(self.key), "PEACOCK_INSTANCE": INSTANCE, **KEYS,
            "FAKE_SSH_LOG": str(self.logs["ssh"]), "FAKE_RSYNC_LOG": str(self.logs["rsync"]),
            "FAKE_NEBIUS_LOG": str(self.logs["nebius"]), "FAKE_AWS_LOG": str(self.logs["aws"]),
            "FAKE_NEBIUS_STATE": str(self.nebius_state), "FAKE_VMS": str(self.hosts),
            "FAKE_VM_BIN": str(tmp / "vm-bin"), "FAKE_BASE_PATH": str(base),
            "FAKE_REAL_RSYNC": shutil.which("rsync"), "FAKE_STASH": str(self.stash),
            "FAKE_CONTAINER_PATH": f"{tmp / 'container-bin'}:{base}", "FAKE_BUCKETS": str(self.buckets),
            "FAKE_ID_SHA": hashlib.sha256(KEYS["AWS_ACCESS_KEY_ID"].encode()).hexdigest(),
            "FAKE_SECRET_SHA": hashlib.sha256(KEYS["AWS_SECRET_ACCESS_KEY"].encode()).hexdigest(),
            "FAKE_SLUG": SLUG, "PEACOCK_HOST_POLL": "0", "PEACOCK_HOST_WAIT": "5",
        }
        self.host = self.add_host(ADDRESS)
        self.write_host_file(NAME, PROVIDER="ssh", HOST=ADDRESS, USER=USER)
        return self

    def __exit__(self, *exc):
        self.tmp.cleanup()

    def make_tree(self):
        for rel in TREE_FILES:
            (self.tree / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPO / rel, self.tree / rel)
        shutil.copytree(REPO / "docker", self.tree / "docker")
        for rel in FIXTURES + INSTALL:
            (self.tree / rel).parent.mkdir(parents=True, exist_ok=True)
            (self.tree / rel).write_text(f"{rel}\n")
        git(self.tree, "init", "-q", "-b", "master")
        git(self.tree, "add", "-A")
        git(self.tree, "update-index", "--add", "--cacheinfo", f"160000,{'c' * 40},third_party/cudf")
        git(self.tree, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "tree")

    def add_host(self, address, driver=DRIVER, docker=True, toolkit=True, rsync=True, data=True):
        """A GPU host whose user has the GPU key: its driver, docker, the toolkit, rsync and data dir as asked."""
        host = self.hosts / address
        home = host / "home" / USER
        (home / ".ssh").mkdir(parents=True)
        (home / ".ssh" / "authorized_keys").write_text(GPU_PUB + "\n")
        (host / "passwd").write_text(f"root:x:0:0::/root:/bin/bash\n{USER}:x:1000:1000::{home}:/bin/bash\n")
        if data:
            self.data_dir(host).mkdir(parents=True)
            self.data_dir(host).chmod(0o2775)
        self.set_environment(host, PEACOCK_DATA_DIR=self.data_dir(host))
        (host / "hostname").write_text(HOSTNAME + "\n")
        (host / "groups").write_text(f"{USER} docker\n" if docker else f"{USER}\n")
        if driver is not None:
            (host / "driver").write_text(f"{driver}, {CAP}\n")
        for name, text in HOST_BIN.items():
            install(host / "bin" / name, text)
        if docker:
            install(host / "bin" / "docker", HOST_DOCKER)
        if toolkit:
            install(host / "bin" / "nvidia-ctk", "#!/bin/sh\nexit 0\n")
        if rsync:  # only its presence matters: the fake rsync on the tree's side does the copying
            install(host / "bin" / "rsync", "#!/bin/sh\nexit 0\n")
        self.publish_images(host)
        return host

    def set_environment(self, host, **values):
        """The host's session environment: <values>, and an install lock of the rig's own."""
        values = {"PEACOCK_INSTALL_LOCK": host / "install.lock", **values}
        (host / "environment").write_text("".join(f"{k}={v}\n" for k, v in values.items()))

    def publish_images(self, host, run_label=None, drop=()):
        """What the registry holds, as the host's docker pulls it: each version's images, labelled as the tree is."""
        images = {}
        for cudf in ("25.02", "26.02"):
            for tag in (f"cudf-{cudf}", f"cudf-{cudf}-{COMMIT}"):
                images[f"{RUN}:{tag}"] = run_label if run_label is not None else self.label("run", cudf)
                images[f"{BUILD}:{tag}"] = self.label("build", cudf)
        (host / "images.json").write_text(json.dumps({k: v for k, v in images.items() if k not in drop}))

    def label(self, kind, cudf):
        return lib(self.tree, "image-inputs.sh", f'image_inputs "{self.tree}" {kind} {cudf} | paste -sd" " -')

    def data_hash(self):
        return lib(self.tree, "data-inputs.sh", f'data_inputs "{self.tree}"')

    def write_host_file(self, name, **keys):
        d = self.home / ".config" / "peacock" / "hosts"
        d.mkdir(parents=True, exist_ok=True)
        (d / f"{name}.env").write_text("".join(f"{k}={v}\n" for k, v in keys.items()))

    def nebius_host(self, name="h100", config=True, **keys):
        """A nebius host file for the rig's host, and its CLI config, unreadable: only its path may reach the CLI."""
        keys = {"PROVIDER": "nebius", "HOST": ADDRESS, "USER": USER, "INSTANCE_ID": INSTANCE_ID, **keys}
        self.write_host_file(name, **{k: v for k, v in keys.items() if v is not None})
        path = self.home / ".config" / "peacock" / "hosts" / name / "config.yaml"
        if config:
            path.parent.mkdir(mode=0o700)
            path.write_text(f"private-key: {SECRET}\n")
            path.chmod(0)
        return path

    def nebius(self, **changes):
        st = json.loads(self.nebius_state.read_text()) if self.nebius_state.exists() else \
            {"id": INSTANCE_ID, "state": "STOPPED", "ip": ADDRESS}
        st.update(changes)
        self.nebius_state.write_text(json.dumps(st))
        return st

    def run(self, script, *args, env=None, background=False):
        cmd = [str(self.tree / "scripts" / "env" / script), *map(str, args)]
        full = {**self.env, **(env or {})}
        full = {k: v for k, v in full.items() if v is not None}
        # Under the umask a login shell commonly has, so what is left group-writable is the scripts' doing.
        if background:
            return subprocess.Popen(cmd, env=full, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                    stderr=subprocess.PIPE, text=True, cwd=self.root, start_new_session=True,
                                    umask=0o022)
        return subprocess.run(cmd, env=full, stdin=subprocess.DEVNULL, capture_output=True, text=True,
                              timeout=120, cwd=self.root, umask=0o022)

    def on_host_env(self, host=None):
        """The environment of an ssh session on the host."""
        host = host or self.host
        env = {k: v for k, v in self.env.items() if k.startswith("FAKE_")}
        env.update(HOME=str(self.host_home(host)), FAKE_VM=str(host), FAKE_USER_NAME=USER, LANG="C.UTF-8",
                   PATH=f"{host / 'bin'}:{self.env['FAKE_BASE_PATH']}", PEACOCK_DATA_DIR=str(self.data_dir(host)))
        return env

    def on_host(self, *args, host=None, stdin=subprocess.DEVNULL, background=False, instance=INSTANCE):
        """on-gpu-host.sh <args> in an instance's dir on the host, as an ssh session there runs it."""
        host = host or self.host
        env = self.on_host_env(host)
        cmd = ["bash", str(self.host_home(host) / "peacock" / instance / "scripts/env/on-gpu-host.sh"), *args]
        if background:
            return subprocess.Popen(cmd, env=env, stdin=stdin, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                    text=True, cwd=self.host_home(host))
        return subprocess.run(cmd, env=env, stdin=stdin, capture_output=True, text=True, timeout=60,
                              cwd=self.host_home(host))

    def gpu(self, *args, **kw):
        return self.run("gpu.sh", *args, **kw)

    def adapter(self, provider, *args, **kw):
        return self.run(f"host/{provider}.sh", *args, **kw)

    def calls(self, name):
        path = self.logs[name]
        return [json.loads(ln) for ln in path.read_text().splitlines()] if path.exists() else []

    def nebius_verbs(self):
        return [next(w for w in argv if w in ("get", "start", "stop")) for argv in self.calls("nebius")]

    def host_calls(self, host, name):
        path = host / f"{name}.log"
        if not path.exists():
            return []
        lines = path.read_text().splitlines()
        return [json.loads(ln) for ln in lines] if name in ("docker", "runs") else lines

    def host_home(self, host=None):
        return (host or self.host) / "home" / USER

    def data_dir(self, host=None):
        return (host or self.host) / "var" / "lib" / "peacock"

    def is_locked(self, path):
        """Whether another process holds flock on <path>."""
        with open(path, "a") as f:
            try:
                fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                return True
            fcntl.flock(f, fcntl.LOCK_UN)
            return False


def ok(r):
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def refused(r, *names, code=2):
    """Exit <code>, one stderr line from gpu.sh or the host's side, naming each of <names>."""
    assert r.returncode == code, (r.returncode, r.stdout, r.stderr)
    assert len(r.stderr.strip().splitlines()) == 1, r.stderr
    assert r.stderr.startswith(("gpu.sh: ", "on-gpu-host.sh: ")), r.stderr
    for name in names:
        assert name in r.stderr, (name, r.stderr)


def ship_dir(rig, instance=INSTANCE, host=None):
    return rig.host_home(host) / "peacock" / instance


def device_runs(rig, host=None):
    return [c for c in rig.host_calls(host or rig.host, "docker") if c[0] == "run" and "--gpus" in c]


def image_of(rig, run, host=None):
    """The tag a device run's image ID was read from."""
    from machine import command
    return json.loads(((host or rig.host) / "ids.json").read_text())[command(run)[0]]["ref"]


def prepared(rig, *extra, host=NAME, **kw):
    return ok(rig.gpu(host, "prepare", *extra, **kw))


def shipped(rig, host=NAME, **kw):
    prepared(rig, host=host, **kw)
    return ok(rig.gpu(host, "ship", **kw))


def until(condition, what, seconds=60):
    deadline = time.time() + seconds
    while not condition():
        assert time.time() < deadline, f"never: {what}"
        time.sleep(0.05)


def stderr_line(process, words, seconds=60):
    """Reads <process>'s stderr until a line holding <words>, and returns it."""
    deadline = time.time() + seconds
    while time.time() < deadline:
        ready, _, _ = select.select([process.stderr], [], [], 0.2)
        if ready:
            line = process.stderr.readline()
            assert line, f"{words!r} never came: the process ended"
            if words in line:
                return line
    raise AssertionError(f"{words!r} never came")


def dropped(rig, *args, env=None):
    """gpu.sh run <args>, its session dropped once the device run has started."""
    first = rig.gpu(NAME, "run", *args, env=env, background=True)
    until(lambda: rig.host_calls(rig.host, "runs"), "the run started")
    os.killpg(first.pid, signal.SIGKILL)
    first.wait()
