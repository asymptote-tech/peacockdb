"""A fake CPU VM for the launcher's tests: a home, a clone of the scripts, and fakes on PATH.

The machine user's home holds a Claude Code install and login, gh's config and a git identity;
`~/peacockdb` is a git clone holding the scripts under test and the image inputs, committed. The
docker CLI is a fake that records its argv and answers from a JSON world: containers by name,
the image's label and environment, and its /etc/passwd and /etc/group. It refuses a bind whose
source is missing, as docker does. A `docker run` of `generate_testdata.sh` makes the dataset
directory it would have made; `cargo fetch` records in the world whether two ran at once; an
instance's `docker run -d` runs FAKE_CONTAINER_CMD when set, in the paths its mounts give it,
refusing a path that is not mounted at its own machine path. cuDF's origin is a local repository
of two commits, named by the clone's .gitmodules; the clone pins the first, its own
`third_party/cudf` an empty directory, as a fresh clone leaves it. The clone's origin is a bare
repository whose master holds a board of chains A, B and E (E with no task); `commit` pushes to it.
The machine carries the marker of a bootstrapped VM and a machine-wide data directory, setgid.
`other_user` adds a second developer: their own home and clone, the same data directory and docker.
"""
import json
import os
import pathlib
import shutil
import subprocess
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[3]
CLONE_FILES = [
    "scripts/env/up.sh", "scripts/env/down.sh", "scripts/env/bootstrap-vm.sh", "scripts/env/image.sh",
    "scripts/env/provision-vm.sh", "scripts/lib/peacock-config.sh", "scripts/lib/board.sh",
    "scripts/lib/image-inputs.sh", "scripts/lib/data-inputs.sh", "scripts/lib/machine.sh",
    "scripts/ensemble-watchdog.sh", "testdata/generate_testdata.sh", "cpp/CMakeLists.txt",
]
IMAGE_PASSWD = "root:x:0:0:root:/root:/bin/bash\npeacock:x:1000:1000::/home/peacock:/bin/bash\n"
IMAGE_GROUP = "root:x:0:\npeacock:x:1000:\n"
GH_HELPER = "!/usr/local/bin/gh auth git-credential"
GH_HOSTS = ("github.com:\n    oauth_token: github_pat_FAKE0123456789abcdef\n"
            "    user: machine-user\n    git_protocol: https\n")
GITCONFIG = ("[user]\n\tname = Machine User\n\temail = machine@example.test\n"
             f'[credential "https://github.com"]\n\thelper =\n\thelper = {GH_HELPER}\n')
IMAGE_PATH = "/opt/venv/bin:/opt/rust/cargo/bin:/usr/local/bin:/usr/bin:/bin"
REGISTRY = "ghcr.io/asymptote-tech/peacockdb-build"
BOARD = """# Board

## Chain A (base: master)

### 1. [`alpha.md`](alpha.md) — state: approved to build

## Chain B (base: master)

### 2. [`beta-two.md`](beta-two.md) — state: new

### 1. [`beta.md`](beta.md) — state: approved to build

## Chain E (base: master)

Nothing yet.
"""

FAKE_DOCKER = r'''#!/usr/bin/env python3
import fcntl, json, os, pathlib, subprocess, sys, time
a = sys.argv[1:]
with open(os.environ["FAKE_DOCKER_LOG"], "a") as f:
    f.write(json.dumps(a) + "\n")
world_path = os.environ["FAKE_DOCKER_WORLD"]

def update(fn):
    with open(world_path + ".lock", "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        w = json.load(open(world_path))
        out = fn(w)
        json.dump(w, open(world_path, "w"))
        return out

w = update(lambda w: dict(w))
if a[:2] == ["container", "inspect"]:
    name = a[-1]
    if name not in w["containers"]:
        print(f"Error: No such container: {name}", file=sys.stderr); sys.exit(1)
    print(w["containers"][name]["state"]); sys.exit(0)
if a[0] == "ps":
    wanted = [a[i + 1].removeprefix("label=").partition("=")[::2] for i, x in enumerate(a) if x == "--filter"]
    found = [(name, c) for name, c in w["containers"].items() if all(c["labels"].get(k) == v for k, v in wanted)]
    time.sleep(w["ps_seconds"])
    fmt = a[a.index("--format") + 1]
    for name, c in found:
        print(name if fmt == "{{.Names}}" else f'{c["labels"].get("peacockdb.user", "")} {name} {c["state"]}')
    sys.exit(0)
if a[0] == "pull":
    sys.exit(0)
if a[:2] == ["image", "inspect"]:
    fmt = a[a.index("--format") + 1]
    if "peacockdb.inputs" in fmt:
        print(w["label"] if w["label"] is not None else "<no value>")
    elif ".Config.Env" in fmt:
        print(f"PATH={w['image_path']}\nCARGO_HOME=/opt/rust/cargo\n")
    else:
        sys.exit(f"fake docker: no answer for {a}")
    sys.exit(0)
if a[0] in ("stop", "rm"):
    name = a[-1]
    if name not in w["containers"]:
        print(f"Error: No such container: {name}", file=sys.stderr); sys.exit(1)
    def settle(w):
        if a[0] == "rm":
            del w["containers"][name]
        else:
            w["containers"][name]["state"] = "exited"
    update(settle)
    sys.exit(0)
if a[0] == "run":
    mounts, env, labels, i, workdir = [], {}, {}, 1, None
    while i < len(a) and a[i].startswith("-"):
        flag = a[i]
        if flag in ("-d", "--rm", "--init"):
            i += 1; continue
        value = a[i + 1]
        if flag == "--mount":
            kv = dict(p.split("=", 1) if "=" in p else (p, True) for p in value.split(","))
            if not os.path.exists(kv["src"]):
                print(f"docker: invalid mount config: bind source path does not exist: {kv['src']}", file=sys.stderr)
                sys.exit(125)
            mounts.append((kv["src"], kv["dst"], "readonly" in kv))
        elif flag == "-e":
            k, _, v = value.partition("="); env[k] = v
        elif flag == "-w":
            workdir = value
        elif flag == "--label":
            k, _, v = value.partition("="); labels[k] = v
        i += 2
    image, cmd = a[i], a[i + 1:]
    if cmd[:2] == ["bash", "-c"] and cmd[2].startswith("umask 002 &&"):
        os.umask(0o002)
    if cmd[:1] == ["cat"]:
        print(w["etc"][cmd[1]], end=""); sys.exit(0)
    if cmd[:2] == ["cargo", "fetch"]:
        def begin(w):
            w["fetching"] += 1
            w["fetch_overlap"] = w["fetch_overlap"] or w["fetching"] > 1
        update(begin)
        time.sleep(w["fetch_seconds"])
        def end(w):
            w["fetching"] -= 1
        update(end)
        sys.exit(0)
    gen = [c for c in cmd if c.endswith("/generate_testdata.sh")]
    if gen:
        stage = pathlib.Path(gen[0]).parent
        bench = cmd[cmd.index("--bench") + 1]
        time.sleep(w["gen_seconds"])
        (stage / f"{bench}.sf1").mkdir()
        (stage / f"{bench}.sf1" / "generated").write_text(bench)
        sys.exit(0)
    if "-d" in a:
        name = a[a.index("--name") + 1]
        script = os.environ.get("FAKE_CONTAINER_CMD")
        if script:
            def own_path(path):
                inner = max((m for m in mounts if path == m[1] or path.startswith(m[1].rstrip("/") + "/")),
                            key=lambda m: len(m[1]), default=None)
                if inner is None or inner[0] != inner[1] or inner[2]:
                    sys.exit(f"fake docker: {path} is not mounted read-write at its own path: {inner}")
            own_path(workdir)
            common = subprocess.run(["git", "-C", workdir, "rev-parse", "--path-format=absolute",
                                     "--git-common-dir"], capture_output=True, text=True, check=True).stdout.strip()
            own_path(common)
            gitconfig = env["HOME"] + "/.gitconfig"
            if (gitconfig, gitconfig, True) not in mounts:
                sys.exit("fake docker: the machine's ~/.gitconfig is not mounted read-only at its own path")
            subprocess.run(["sh", "-c", script], cwd=workdir, check=True,
                           env={"HOME": env["HOME"], "PATH": os.environ["PATH"], "GIT_CONFIG_NOSYSTEM": "1"})
        def add(w):
            w["containers"][name] = {"state": "running", "labels": labels}
        update(add)
        print("0123456789ab"); sys.exit(0)
    sys.exit(0)
sys.exit(f"fake docker: no answer for {a}")
'''

FAKE_GH = '''#!/bin/sh
printf '%s\\n' "gh $*" >> "$FAKE_GH_LOG"
# A token in GH_TOKEN logs gh in on the machine alone: the container never sees it.
[ "$1 $2" = "auth status" ] && [ -n "${GH_TOKEN:-}" ] && exit 0
[ "$1 $2" = "auth status" ] && exit "${FAKE_GH_STATUS:-0}"
exit 0
'''

FAKE_ROOT_ID = '''#!/bin/sh
case "$1" in -u|-g) echo 0 ;; *) exec /usr/bin/id "$@" ;; esac
'''

FAKE_OTHER_ID = '''#!/bin/sh
case "$1" in -un|-gn) echo "$FAKE_USER_NAME" ;; *) exec /usr/bin/id "$@" ;; esac
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
    """A temporary machine; `up` and `down` run the clone's scripts with the fakes on PATH."""

    def __enter__(self):
        self.tmp = tempfile.TemporaryDirectory()
        tmp = pathlib.Path(self.tmp.name)
        self.home = tmp / "home"
        self.clone = self.home / "peacockdb"
        self.origin = tmp / "origin.git"
        self.data_dir = tmp / "shared-data"
        self.data_dir.mkdir()
        self.data_dir.chmod(0o2775)
        self.make_home(self.home)
        self.make_clone()
        self.bin = tmp / "bin"
        self.bin.mkdir()
        self.install("docker", FAKE_DOCKER)
        self.install("gh", FAKE_GH)
        self.docker_log, self.gh_log, self.world_path = tmp / "docker.log", tmp / "gh.log", tmp / "world.json"
        self.docker_log.touch()
        self.gh_log.touch()
        self.world_path.write_text(json.dumps({
            "containers": {}, "label": self.label("25.02"), "image_path": IMAGE_PATH, "gen_seconds": 0,
            "fetch_seconds": 0, "ps_seconds": 0, "fetching": 0, "fetch_overlap": False,
            "etc": {"/etc/passwd": IMAGE_PASSWD, "/etc/group": IMAGE_GROUP}}))
        self.env = {"PATH": f"{self.bin}:{os.environ['PATH']}", "HOME": str(self.home), "LANG": "C.UTF-8",
                    "FAKE_DOCKER_LOG": str(self.docker_log), "FAKE_DOCKER_WORLD": str(self.world_path),
                    "FAKE_GH_LOG": str(self.gh_log), "PEACOCK_DATA_DIR": str(self.data_dir)}
        self.keys_file = tmp / "keys.env"
        self.write_keys(KEYS)
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
        (h / ".config" / "gh").mkdir(parents=True)
        (h / ".config" / "gh" / "hosts.yml").write_text(GH_HOSTS)
        (h / ".config" / "peacock" / "hosts").mkdir(parents=True)
        (h / ".config" / "peacock" / "machine").write_text("bootstrapped\n")
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
        (self.clone / "third_party" / "cudf").mkdir(parents=True)  # an uninitialised submodule, kept by commit -a
        (self.clone / "llm-wiki" / "tasks").mkdir(parents=True)
        (self.clone / "llm-wiki" / "tasks" / "tasks.md").write_text(BOARD)
        git(self.clone, "init", "-q", "-b", "master")
        git(self.clone, "add", "-A")
        self.pin_cudf(self.cudf_sha)
        subprocess.run(["git", "clone", "-q", "--bare", str(self.clone), str(self.origin)], check=True)
        git(self.clone, "remote", "add", "origin", str(self.origin))
        git(self.clone, "fetch", "-q", "origin")

    def pin_cudf(self, sha):
        """Commits a third_party/cudf gitlink at <sha> in the clone."""
        git(self.clone, "update-index", "--add", "--cacheinfo", f"160000,{sha},third_party/cudf")
        git(self.clone, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", f"cudf at {sha}")
        self.push()

    def commit(self, message):
        git(self.clone, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-am", message)
        self.push()

    def push(self, *refs):
        """Pushes master, or <refs>, to origin once it exists."""
        if self.origin.exists():
            git(self.clone, "push", "-q", "origin", *(refs or ["master"]))

    def push_branch(self, name, board, base="master"):
        """Pushes <name> to origin: <base> and a commit whose board is <board>; the clone keeps no such branch."""
        git(self.clone, "checkout", "-q", "-b", name, base)
        (self.clone / "llm-wiki" / "tasks" / "tasks.md").write_text(board)
        git(self.clone, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-am", f"{name}'s board")
        sha = git(self.clone, "rev-parse", "HEAD")
        git(self.clone, "push", "-q", "origin", name)
        git(self.clone, "checkout", "-q", "master")
        git(self.clone, "branch", "-q", "-D", name)
        return sha

    def other_user(self, name):
        """A second developer on this machine: their home, a clone of origin, `id -un` answering name."""
        home = self.home.parent / f"home-{name}"
        self.make_home(home)
        subprocess.run(["git", "clone", "-q", str(self.origin), str(home / "peacockdb")], check=True)
        bin_dir = self.home.parent / f"bin-{name}"
        bin_dir.mkdir()
        (bin_dir / "id").write_text(FAKE_OTHER_ID)
        (bin_dir / "id").chmod(0o755)
        return OtherUser(self, name, home, {"HOME": str(home), "FAKE_USER_NAME": name,
                                            "PATH": f"{bin_dir}:{self.env['PATH']}"})

    def install(self, name, text):
        (self.bin / name).write_text(text)
        (self.bin / name).chmod(0o755)

    def write_keys(self, keys, extra=""):
        self.keys_file.write_text("".join(f"{k}={v}\n" for k, v in keys.items()) + extra)

    def label(self, cudf, tree=None):
        tree = tree or self.clone
        return lib(tree, "image-inputs.sh", f'image_inputs "{tree}" build {cudf} | paste -sd" " -')

    def data_hash(self, tree=None):
        return lib(tree or self.clone, "data-inputs.sh", f'data_inputs "{tree or self.clone}"')

    def world(self, **changes):
        w = json.loads(self.world_path.read_text())
        w.update(changes)
        self.world_path.write_text(json.dumps(w))
        return w

    def run(self, script, *args, env=None, background=False, cwd=None):
        full_env = {**self.env, **(env or {})}
        clone = pathlib.Path(full_env["HOME"]) / "peacockdb"
        cmd = [str(clone / "scripts" / "env" / script), *map(str, args)]
        # Under the umask a login shell commonly has, so what a script leaves group-writable is its doing.
        if background:
            return subprocess.Popen(cmd, env=full_env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                    stderr=subprocess.PIPE, text=True, cwd=cwd, umask=0o022)
        return subprocess.run(cmd, env=full_env, stdin=subprocess.DEVNULL, capture_output=True, text=True,
                              timeout=120, cwd=cwd, umask=0o022)

    def up(self, *args, **kw):
        return self.run("up.sh", *args, **kw)

    def down(self, *args, **kw):
        return self.run("down.sh", *args, **kw)

    def docker_calls(self):
        return [json.loads(line) for line in self.docker_log.read_text().splitlines()]

    def runs(self):
        return [c for c in self.docker_calls() if c[0] == "run"]

    def instance_runs(self):
        return [c for c in self.runs() if "-d" in c]


class OtherUser:
    """Another developer on a Machine: `up` and `down` run their clone's scripts as them."""

    def __init__(self, machine, name, home, env):
        self.machine, self.name, self.home, self.env = machine, name, home, env

    def up(self, *args, **kw):
        return self.machine.up(*args, env={**self.env, **kw.pop("env", {})}, **kw)

    def down(self, *args, **kw):
        return self.machine.down(*args, env={**self.env, **kw.pop("env", {})}, **kw)


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


def flag_values(argv, flag):
    return [argv[i + 1] for i, token in enumerate(argv[:-1]) if token == flag]


def mounts(argv):
    """Each --mount of a docker run as (src, dst, readonly)."""
    out = []
    for value in flag_values(argv, "--mount"):
        kv = dict(p.split("=", 1) if "=" in p else (p, True) for p in value.split(","))
        assert kv.get("type") == "bind", value
        out.append((kv["src"], kv["dst"], "readonly" in kv))
    return out


def command(argv):
    """The image and the command after it, of a docker run."""
    i = 1
    while argv[i].startswith("-"):
        i += 1 if argv[i] in ("-d", "--rm", "--init") else 2
    return argv[i], argv[i + 1:]
