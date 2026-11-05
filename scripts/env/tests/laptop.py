"""A laptop and the VMs it provisions, for provision-vm.sh's tests.

The laptop is a home with a git identity and a clone of the scripts whose origin is GitHub. `ssh`
is a fake that records argv and stdin and runs the remote command in a fake VM, a directory per
address: passwd, group, homes, and `root/` for /etc and /var. A user is reached with an
authorized_keys holding the `.pub` of the key `-i` names (any file stands for the laptop's key); a
host with a `password` file lets a call without BatchMode in; `drop_on` and `exit_on` end a command.
A host's `bin/` is first on PATH, its `environment` in every session, its `next_session.sh` run as
the next session starts; stdin streams. sudo, getent, groupadd, useradd, usermod, install and gh act
on the VM; bootstrap-vm.sh is a stub.
"""
import json
import os
import pathlib
import shutil
import subprocess
import tempfile

from machine import KEYS, REPO, git

LAPTOP_FILES = [
    "scripts/env/provision-vm.sh", "scripts/env/up.sh", "scripts/env/down.sh",
    "scripts/lib/peacock-config.sh", "scripts/lib/board.sh", "scripts/lib/image-inputs.sh",
    "scripts/lib/data-inputs.sh", "scripts/lib/machine.sh", "scripts/lib/host.sh", "scripts/lib/portable.sh",
    "scripts/env/host/ssh.sh", "scripts/env/host/nebius.sh",
]
ORIGIN = "git@github.com:asymptote-tech/peacockdb.git"
PAT = "github_pat_11FAKETOKEN0value_Zq81"
LAPTOP_GITCONFIG = "[user]\n\tname = Ada Laptop\n\temail = ada@example.test\n"

FAKE_SSH = r'''#!/usr/bin/env python3
import json, os, pathlib, subprocess, sys, threading
args = sys.argv[1:]
WITH_VALUE = {"-o", "-i", "-p", "-l", "-F", "-E", "-J", "-b", "-c", "-D", "-L", "-R", "-W", "-w", "-e", "-m"}
i, opts = 0, []
while i < len(args) and args[i].startswith("-"):
    if args[i] == "--":
        i += 1
        break
    step = 2 if args[i] in WITH_VALUE else 1
    opts += args[i:i + step]
    i += step
target, command = args[i], " ".join(args[i + 1:])
user, _, address = target.rpartition("@")
vm = pathlib.Path(os.environ["FAKE_VMS"]) / address
reachable = vm.is_dir() and address not in os.environ.get("FAKE_SSH_DOWN", "").split()
home, authorized, password = None, False, False
if reachable:
    homes = {ln.split(":")[0]: ln.split(":")[5] for ln in (vm / "passwd").read_text().splitlines() if ln}
    home = homes.get(user)
    keys = pathlib.Path(home or "/nonexistent") / ".ssh" / "authorized_keys"
    # With -i, that key's .pub must be authorized; without, the laptop's own key, which any file stands for.
    key = opts[opts.index("-i") + 1] if "-i" in opts else None
    authorized = keys.is_file() and (key is None or pathlib.Path(key + ".pub").read_text().strip()
                                     in keys.read_text().splitlines())
    batch = any(opts[k + 1] == "BatchMode=yes" for k, o in enumerate(opts) if o == "-o")
    if home is not None and not authorized and not batch and (vm / "password").is_file():
        authorized = password = True  # the person types the password at ssh's prompt
drop = vm / "drop_on"
dropped = reachable and drop.is_file() and drop.read_text().strip() in command
# exit_on: `<words> <status>`, a command holding <words> ends with <status> without running.
forced = None
if reachable and (vm / "exit_on").is_file():
    words, _, code = (vm / "exit_on").read_text().strip().rpartition(" ")
    forced = int(code) if words in command else None
def log(stdin):
    with open(os.environ["FAKE_SSH_LOG"], "a") as f:
        f.write(json.dumps({"argv": args, "stdin": stdin.decode(errors="replace"), "password": password}) + "\n")
# As ssh, a session that never runs its command reads none of its stdin.
if authorized and forced is not None:
    log(b"")
    sys.exit(forced)
if not reachable or not authorized or dropped:
    log(b"")
    if not reachable:
        print(f"ssh: connect to host {address} port 22: No route to host", file=sys.stderr)
    elif dropped:
        print(f"Connection to {address} closed by remote host.", file=sys.stderr)
    else:
        print(f"{target}: Permission denied (publickey).", file=sys.stderr)
    sys.exit(255)
env = {k: v for k, v in os.environ.items() if k.startswith("FAKE_")}
own_bin = f"{vm / 'bin'}:" if (vm / "bin").is_dir() else ""
env.update(HOME=home, USER=user, LOGNAME=user, FAKE_VM=str(vm), FAKE_USER_NAME=user, LANG="C.UTF-8",
           PATH=f"{own_bin}{os.environ['FAKE_VM_BIN']}:{os.environ['FAKE_BASE_PATH']}")
if (vm / "environment").is_file():  # what the host's sshd puts in every session
    env.update(ln.split("=", 1) for ln in (vm / "environment").read_text().splitlines() if "=" in ln)
# What takes effect only at a new login, a group joined among them: run once, as this session starts.
if (vm / "next_session.sh").is_file():
    subprocess.run(["bash", str(vm / "next_session.sh")], check=True)
    (vm / "next_session.sh").unlink()
# stdin streams to the remote command, as ssh's does: a session can be held open by its caller.
child = subprocess.Popen(["bash", "-c", command], stdin=subprocess.PIPE, env=env, cwd=home)
seen = bytearray()
def pump():
    try:
        while "-n" not in opts:
            chunk = os.read(0, 65536)
            if not chunk:
                break
            seen.extend(chunk)
            child.stdin.write(chunk)
            child.stdin.flush()
    except (BrokenPipeError, ValueError, OSError):
        pass
    try:
        child.stdin.close()
    except (BrokenPipeError, OSError):
        pass
threading.Thread(target=pump, daemon=True).start()
code = child.wait()
log(bytes(seen))
os._exit(code)
'''

FAKE_GH = r'''#!/usr/bin/env python3
import json, os, pathlib, subprocess, sys
a = sys.argv[1:]
with open(os.environ["FAKE_GH_LOG"], "a") as f:
    f.write(json.dumps({"argv": a, "host": os.environ.get("FAKE_VM", "laptop"),
                        "token": os.environ.get("GH_TOKEN")}) + "\n")
hosts = pathlib.Path(os.environ["HOME"]) / ".config" / "gh" / "hosts.yml"
if a[:2] == ["auth", "login"]:
    token = sys.stdin.read().strip()
    hosts.parent.mkdir(parents=True, exist_ok=True)
    hosts.write_text(f"github.com:\n    oauth_token: {token}\n    git_protocol: https\n")
    sys.exit(0)
if a[:2] == ["auth", "status"]:
    sys.exit(0 if hosts.is_file() and "oauth_token:" in hosts.read_text() else 1)
if a[:2] == ["auth", "setup-git"]:
    subprocess.run(["git", "config", "--global", "credential.https://github.com.helper",
                    "!/usr/local/bin/gh auth git-credential"], check=True)
    sys.exit(0)
if a[0] == "api":
    token = os.environ.get("GH_TOKEN", "")
    ok = token.startswith("github_pat_") and token not in os.environ.get("FAKE_GH_DENIED", "").split()
    if ok and a[1] == "repos/asymptote-tech/peacockdb":
        sys.exit(0)
    print("gh: Not Found (HTTP 404)", file=sys.stderr)
    sys.exit(1)
sys.exit(f"fake gh: no answer for {a}")
'''

FAKE_CLAUDE = r'''#!/usr/bin/env python3
import json, os, pathlib, sys
with open(os.path.join(os.environ["FAKE_VM"], "claude.log"), "a") as f:
    f.write(json.dumps({"argv": sys.argv[1:], "config": os.environ.get("CLAUDE_CONFIG_DIR")}) + "\n")
if sys.argv[1:3] == ["auth", "login"]:
    d = pathlib.Path(os.environ.get("CLAUDE_CONFIG_DIR") or pathlib.Path(os.environ["HOME"], ".claude"))
    d.mkdir(parents=True, exist_ok=True)
    (d / ".credentials.json").write_text('{"claudeAiOauth": {}}')
'''

STUB_BOOTSTRAP = '''#!/usr/bin/env bash
set -eu
printf '%s %s\\n' "$(id -un)" "$*" >> "$FAKE_VM/bootstrap.log"
mkdir -p "$HOME/.local/bin" "$HOME/.config/peacock"
cp "$FAKE_CLAUDE" "$HOME/.local/bin/claude"
ref=master
if [ "${1:-}" = --ref ]; then ref=$2; fi
git init -q -b "$ref" "$HOME/peacockdb"
echo bootstrapped > "$HOME/.config/peacock/machine"
'''

VM_BIN = {
    "sudo": '#!/bin/sh\nprintf "%s\\n" "$*" >> "$FAKE_VM/sudo.log"\nexec "$@"\n',
    "id": '#!/bin/sh\ncase "$1" in -un|-gn) echo "$FAKE_USER_NAME" ;; *) exec /usr/bin/id "$@" ;; esac\n',
    "getent": r'''#!/usr/bin/env python3
import os, sys
db, name = sys.argv[1:3]
for line in open(os.path.join(os.environ["FAKE_VM"], db)).read().splitlines():
    if line.split(":")[0] == name:
        print(line)
        sys.exit(0)
sys.exit(2)
''',
    "groupadd": r'''#!/usr/bin/env python3
import os, sys
path = os.path.join(os.environ["FAKE_VM"], "group")
lines = open(path).read().splitlines()
name = sys.argv[-1]
if any(ln.split(":")[0] == name for ln in lines):
    sys.exit(f"groupadd: group '{name}' already exists")
open(path, "a").write(f"{name}:x:{2000 + len(lines)}:\n")
''',
    "useradd": r'''#!/usr/bin/env python3
import os, pathlib, sys
vm = pathlib.Path(os.environ["FAKE_VM"])
open(vm / "useradd.log", "a").write(" ".join(sys.argv[1:]) + "\n")
name = sys.argv[-1]
lines = (vm / "passwd").read_text().splitlines()
if any(ln.split(":")[0] == name for ln in lines):
    sys.exit(f"useradd: user '{name}' already exists")
home = vm / "home" / name
if "--create-home" in sys.argv or "-m" in sys.argv:
    home.mkdir(parents=True)
uid = 3000 + len(lines)
open(vm / "passwd", "a").write(f"{name}:x:{uid}:{uid}::{home}:/bin/bash\n")
open(vm / "group", "a").write(f"{name}:x:{uid}:\n")
''',
    "usermod": r'''#!/usr/bin/env python3
import os, sys
path = os.path.join(os.environ["FAKE_VM"], "group")
a = sys.argv[1:]
assert a[0] == "-aG", a
groups, user = a[1].split(","), a[2]
out = []
for line in open(path).read().splitlines():
    f = line.split(":")
    if f[0] in groups:
        members = [m for m in f[3].split(",") if m]
        if user not in members:
            members.append(user)
        f[3] = ",".join(members)
    out.append(":".join(f))
open(path, "w").write("\n".join(out) + "\n")
''',
    "install": r'''#!/usr/bin/env python3
import os, pathlib, shutil, sys
vm = pathlib.Path(os.environ["FAKE_VM"])
a = sys.argv[1:]
open(vm / "install.log", "a").write(" ".join(a) + "\n")
directory, mode, paths, i = False, None, [], 0
while i < len(a):
    if a[i] == "-d":
        directory = True; i += 1
    elif a[i] in ("-m", "-o", "-g"):
        if a[i] == "-m":
            mode = int(a[i + 1], 8)
        i += 2
    else:
        paths.append(a[i]); i += 1
def place(p):
    return vm / "root" / p.lstrip("/") if p.startswith(("/etc/", "/var/")) else pathlib.Path(p)
if directory:
    for p in map(place, paths):
        p.mkdir(parents=True, exist_ok=True)
        if mode is not None:
            p.chmod(mode)
else:
    src, dst = paths[0], place(paths[1])
    dst.parent.mkdir(parents=True, exist_ok=True)
    data = sys.stdin.buffer.read() if src == "/dev/stdin" else pathlib.Path(src).read_bytes()
    dst.unlink(missing_ok=True)  # as install does when it cannot open the destination
    dst.write_bytes(data)
    if mode is not None:
        dst.chmod(mode)
''',
}


class Laptop:
    """A temporary laptop; `provision` runs its clone's provision-vm.sh with the fakes on PATH."""

    def __enter__(self):
        self.tmp = tempfile.TemporaryDirectory()
        tmp = pathlib.Path(self.tmp.name)
        self.home = tmp / "laptop"
        self.config = self.home / ".config" / "peacock"
        self.clone = self.home / "peacockdb"
        self.vms = tmp / "vms"
        self.vms.mkdir()
        self.home.mkdir()
        (self.home / ".gitconfig").write_text(LAPTOP_GITCONFIG)
        for rel in LAPTOP_FILES:
            (self.clone / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPO / rel, self.clone / rel)
        stub = self.clone / "scripts" / "env" / "bootstrap-vm.sh"
        stub.write_text(STUB_BOOTSTRAP)
        stub.chmod(0o755)
        git(self.clone, "init", "-q", "-b", "master")
        git(self.clone, "add", "-A")
        git(self.clone, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "scripts")
        git(self.clone, "remote", "add", "origin", ORIGIN)
        self.bin, self.vm_bin = tmp / "bin", tmp / "vm-bin"
        for d, files in ((self.bin, {"ssh": FAKE_SSH, "gh": FAKE_GH}), (self.vm_bin, {**VM_BIN, "gh": FAKE_GH})):
            d.mkdir()
            for name, text in files.items():
                (d / name).write_text(text)
                (d / name).chmod(0o755)
        (tmp / "claude").write_text(FAKE_CLAUDE)
        (tmp / "claude").chmod(0o755)
        self.ssh_log, self.gh_log = tmp / "ssh.log", tmp / "gh.log"
        self.received = tmp / "received.env"
        self.write_env(KEYS)
        self.env = {"PATH": f"{self.bin}:{os.environ['PATH']}", "HOME": str(self.home), "LANG": "C.UTF-8",
                    "FAKE_SSH_LOG": str(self.ssh_log), "FAKE_GH_LOG": str(self.gh_log),
                    "FAKE_VMS": str(self.vms), "FAKE_VM_BIN": str(self.vm_bin),
                    "FAKE_BASE_PATH": os.environ["PATH"], "FAKE_CLAUDE": str(tmp / "claude")}
        return self

    def __exit__(self, *exc):
        self.tmp.cleanup()

    def write_env(self, keys, extra=""):
        self.received.write_text("".join(f"{k}={v}\n" for k, v in keys.items()) + extra)

    def add_vm(self, address, login="ubuntu"):
        """A fresh VM reached as <login>, who has the laptop's key."""
        vm = self.vms / address
        home = vm / "home" / login
        (home / ".ssh").mkdir(parents=True)
        (home / ".ssh" / "authorized_keys").write_text("ssh-ed25519 AAAAfakelaptopkey ada@laptop\n")
        (vm / "passwd").write_text(f"root:x:0:0::/root:/bin/bash\n{login}:x:1000:1000::{home}:/bin/bash\n")
        (vm / "group").write_text(f"root:x:0:\n{login}:x:1000:\n")
        return vm

    def add_gpu_host(self, address, user="ubuntu"):
        """A GPU host that takes only a password for <user>: no authorized_keys yet."""
        host = self.vms / address
        (host / "home" / user).mkdir(parents=True)
        (host / "passwd").write_text(f"root:x:0:0::/root:/bin/bash\n{user}:x:1000:1000::{host / 'home' / user}:/bin/bash\n")
        (host / "password").write_text("")
        return host

    def provision(self, *args, stdin="", env=None):
        cmd = [str(self.clone / "scripts" / "env" / "provision-vm.sh"), *map(str, args)]
        return subprocess.run(cmd, env={**self.env, **(env or {})}, input=stdin, capture_output=True, text=True,
                              timeout=120, umask=0o022)

    def ssh_calls(self):
        return [json.loads(ln) for ln in self.ssh_log.read_text().splitlines()] if self.ssh_log.exists() else []

    def gh_calls(self):
        return [json.loads(ln) for ln in self.gh_log.read_text().splitlines()] if self.gh_log.exists() else []

    def vm_home(self, address, user):
        return self.vms / address / "home" / user

    def group_members(self, address, group):
        for line in (self.vms / address / "group").read_text().splitlines():
            f = line.split(":")
            if f[0] == group:
                return [m for m in f[3].split(",") if m]
        return None
