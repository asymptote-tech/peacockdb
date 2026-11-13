"""A laptop and the VMs it provisions, for provision-vm.sh's tests.

The laptop is a home with a git identity and a clone of the scripts whose origin is the GitHub
repository. `ssh` is a fake that records each call's argv and stdin and runs the remote command in a
fake VM: a directory per address holding passwd, group, the users' homes, and `root/` for what is
written under /etc and /var. A user is reached only with an authorized_keys. As ssh does, `-t` gives
the far end a terminal only when the laptop's stdin is one, and otherwise says so. Its own lines go
to its `-E` log when it has one, with `Authenticated to` once a user is reached; `-G` prints the
`loglevel` and is not a call. FAKE_SSH_REFUSALS=<n> refuses the first n calls before authenticating,
reading none of stdin and recording no call, and counts them in <ssh log>.refused. On a VM, sudo
runs its command as the caller; getent, groupadd, useradd, usermod and install act on that
directory, install refusing a source that is not a regular file onto a file that exists, as uutils'
does, and each VM's commands get a TMPDIR of its own, <vm>/tmp; gh keeps its login in
~/.config/gh/hosts.yml on the laptop and on a VM alike. The clone's bootstrap-vm.sh is a stub that
records its user and arguments, exits FAKE_BOOTSTRAP_RC when it is set, installs a fake claude,
whose login needs a terminal, into ~/.local/bin, which is not on ssh's PATH, and makes an empty
clone on the --ref branch; like the real one, it refuses a --ref other than the branch an existing
clone is on, naming both. The laptop holds a GPU key pair where provision-vm.sh looks by default,
and ssh-keygen is a fake that records any call. Its nebius is `nebius_cli.py`'s fake, first on PATH,
logging to <tmp>/nebius.log; `~/.config/peacock/nebius.yaml` holds editor-eu, active, and eu-west1,
both keys embedded; `~/.nebius/config.yaml` is mode 000."""
import json
import os
import pathlib
import pty
import shutil
import subprocess
import tempfile

from machine import KEYS, REPO, git
from nebius_cli import install_cli, nebius_config

LAPTOP_FILES = ["scripts/env/provision-vm.sh", "scripts/env/nebius-profiles.sh", "scripts/lib/peacock-config.sh",
                "scripts/lib/ssh-retry.sh"]
ORIGIN = "git@github.com:asymptote-tech/peacockdb.git"
PAT = "github_pat_11FAKETOKEN0value_Zq81"
LAPTOP_GITCONFIG = "[user]\n\tname = Ada Laptop\n\temail = ada@example.test\n"

FAKE_SSH = r'''#!/usr/bin/env python3
import json, os, pathlib, subprocess, sys
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
if "-G" in opts:
    levels = [o.split("=", 1)[1] for o in opts if o.startswith("LogLevel=")]
    print("loglevel " + (levels[0] if levels else "INFO"))
    sys.exit(0)
log = open(opts[opts.index("-E") + 1], "a") if "-E" in opts else sys.stderr
target, command = args[i], " ".join(args[i + 1:])
user, _, address = target.rpartition("@")
refused = pathlib.Path(os.environ["FAKE_SSH_LOG"] + ".refused")
n = int(refused.read_text()) if refused.exists() else 0
if n < int(os.environ.get("FAKE_SSH_REFUSALS", "0")):
    refused.write_text(str(n + 1))
    print(f"ssh: connect to host {address} port 22: Connection refused", file=log)
    sys.exit(255)
tty = "-t" in opts and sys.stdin.isatty()
if "-t" in opts and not tty:
    print("Pseudo-terminal will not be allocated because stdin is not a terminal.", file=log)
stdin = b"" if "-n" in opts or tty else sys.stdin.buffer.read()
with open(os.environ["FAKE_SSH_LOG"], "a") as f:
    f.write(json.dumps({"argv": args, "stdin": stdin.decode(errors="replace")}) + "\n")
vm = pathlib.Path(os.environ["FAKE_VMS"]) / address
if not vm.is_dir() or address in os.environ.get("FAKE_SSH_DOWN", "").split():
    print(f"ssh: connect to host {address} port 22: No route to host", file=log)
    sys.exit(255)
homes = {ln.split(":")[0]: ln.split(":")[5] for ln in (vm / "passwd").read_text().splitlines() if ln}
home = homes.get(user)
if home is None or not (pathlib.Path(home) / ".ssh" / "authorized_keys").is_file():
    print(f"{target}: Permission denied (publickey).", file=log)
    sys.exit(255)
if log is not sys.stderr:
    print(f'Authenticated to {address} ([{address}]:22) using "publickey".', file=log)
    log.close()
env = {k: v for k, v in os.environ.items() if k.startswith("FAKE_")}
(vm / "tmp").mkdir(exist_ok=True)
env.update(HOME=home, USER=user, LOGNAME=user, FAKE_VM=str(vm), FAKE_USER_NAME=user, LANG="C.UTF-8",
           PATH=f"{os.environ['FAKE_VM_BIN']}:{os.environ['FAKE_BASE_PATH']}", TMPDIR=str(vm / "tmp"))
if tty:
    env["FAKE_SSH_TTY"] = "1"
sys.exit(subprocess.run(["bash", "-c", command], input=stdin, env=env, cwd=home).returncode)
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
    if os.environ.get("FAKE_SSH_TTY") != "1":
        sys.exit("claude auth login: no terminal to finish the login in")
    d = pathlib.Path(os.environ.get("CLAUDE_CONFIG_DIR") or pathlib.Path(os.environ["HOME"], ".claude"))
    d.mkdir(parents=True, exist_ok=True)
    (d / ".credentials.json").write_text('{"claudeAiOauth": {}}')
'''

STUB_BOOTSTRAP = '''#!/usr/bin/env bash
set -eu
printf '%s %s\\n' "$(id -un)" "$*" >> "$FAKE_VM/bootstrap.log"
[ -z "${FAKE_BOOTSTRAP_RC:-}" ] || exit "$FAKE_BOOTSTRAP_RC"
mkdir -p "$HOME/.local/bin"
cp "$FAKE_CLAUDE" "$HOME/.local/bin/claude"
if [ "${1:-}" = --ref ] && [ -e "$HOME/peacockdb" ]; then
  on=$(git -C "$HOME/peacockdb" branch --show-current)
  if [ "$on" != "$2" ]; then
    echo "bootstrap-vm.sh: $HOME/peacockdb is on ${on:-a detached HEAD}, not $2" >&2
    exit 2
  fi
fi
ref=master
if [ "${1:-}" = --ref ]; then ref=$2; fi
[ -e "$HOME/peacockdb" ] || git init -q -b "$ref" "$HOME/peacockdb"
'''

FAKE_SSH_KEYGEN = '#!/bin/sh\nprintf "%s\\n" "$*" >> "$FAKE_SSH_KEYGEN_LOG"\nexit 1\n'
GPU_KEY_NAMES = {"gpu_ed25519", "gpu_ed25519.pub"}
# What every laptop holds in ~/.config/peacock before provision-vm.sh runs.
HELD_NAMES = GPU_KEY_NAMES | {"nebius.yaml"}
EMBEDDED = {"key_path": None}


def make_key(path, comment):
    """A fake ed25519 key pair at <path> and <path>.pub, mode 600; returns the private key's text."""
    path.parent.mkdir(parents=True, exist_ok=True)
    text = f"-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAB{comment}PrivateValue\n" \
           "-----END OPENSSH PRIVATE KEY-----\n"
    path.write_text(text)
    path.with_name(path.name + ".pub").write_text(f"ssh-ed25519 AAAAC3Nza{comment}Public {comment}\n")
    for p in (path, path.with_name(path.name + ".pub")):
        p.chmod(0o600)
    return text


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
    # As uutils' install 0.10 does (Ubuntu 26.04): a source that is not a regular file (/dev/stdin,
    # /dev/fd/N) onto a file that exists fails.
    if not pathlib.Path(src).is_file() and dst.exists():
        print("install: No such file or directory", file=sys.stderr)
        sys.exit(1)
    dst.parent.mkdir(parents=True, exist_ok=True)
    data = sys.stdin.buffer.read() if src == "/dev/stdin" else pathlib.Path(src).read_bytes()
    dst.unlink(missing_ok=True)  # as install does when it cannot open the destination
    dst.write_bytes(data)
    if mode is not None:
        dst.chmod(mode)
''',
}


def vm_path():
    """This machine's PATH less any directory holding a claude: a VM's has no ~/.local/bin, and a test
    must never start the real Claude Code."""
    return os.pathsep.join(d for d in os.environ["PATH"].split(os.pathsep)
                           if not os.path.exists(os.path.join(d, "claude")))


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
        self.config.mkdir(parents=True, mode=0o700)
        make_key(self.config / "gpu_ed25519", "Default")
        laptop_bin = {"ssh": FAKE_SSH, "gh": FAKE_GH, "ssh-keygen": FAKE_SSH_KEYGEN}
        for d, files in ((self.bin, laptop_bin), (self.vm_bin, {**VM_BIN, "gh": FAKE_GH})):
            d.mkdir()
            for name, text in files.items():
                (d / name).write_text(text)
                (d / name).chmod(0o755)
        install_cli(self.bin)
        self.nebius_yaml = nebius_config(self.config / "nebius.yaml", "editor-eu",
                                         {"editor-eu": EMBEDDED, "eu-west1": EMBEDDED})
        own = nebius_config(self.home / ".nebius" / "config.yaml", "old", {"old": EMBEDDED, "editor-eu": EMBEDDED})
        own.chmod(0)
        (tmp / "claude").write_text(FAKE_CLAUDE)
        (tmp / "claude").chmod(0o755)
        self.ssh_log, self.gh_log, self.keygen_log = tmp / "ssh.log", tmp / "gh.log", tmp / "ssh-keygen.log"
        self.nebius_log = tmp / "nebius.log"
        self.received = tmp / "received.env"
        self.write_env(KEYS)
        self.env = {"PATH": f"{self.bin}:{os.environ['PATH']}", "HOME": str(self.home), "LANG": "C.UTF-8",
                    "FAKE_SSH_LOG": str(self.ssh_log), "FAKE_GH_LOG": str(self.gh_log),
                    "FAKE_SSH_KEYGEN_LOG": str(self.keygen_log), "FAKE_NEBIUS_LOG": str(self.nebius_log),
                    "FAKE_VMS": str(self.vms), "FAKE_VM_BIN": str(self.vm_bin),
                    "FAKE_BASE_PATH": vm_path(), "FAKE_CLAUDE": str(tmp / "claude"),
                    "PEACOCK_SSH_PAUSES": "0 0 0"}
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

    def provision(self, *args, stdin="", env=None, tty=False):
        """Runs provision-vm.sh with `stdin` on a pipe; with `tty`, typed at a terminal; with None, /dev/null."""
        cmd = [str(self.clone / "scripts" / "env" / "provision-vm.sh"), *map(str, args)]
        return self.run(cmd, stdin, env, tty)

    def run(self, cmd, stdin="", env=None, tty=False):
        """Runs `cmd` on the laptop, with the fakes on PATH, as `provision` does."""
        kw = dict(env={**self.env, **(env or {})}, capture_output=True, text=True, timeout=120, umask=0o022)
        if not tty:
            if stdin is None:
                return subprocess.run(cmd, stdin=subprocess.DEVNULL, **kw)
            return subprocess.run(cmd, input=stdin, **kw)
        master, terminal = pty.openpty()
        try:
            os.write(master, (stdin or "").encode())
            return subprocess.run(cmd, stdin=terminal, **kw)
        finally:
            os.close(terminal)
            os.close(master)

    def ssh_calls(self):
        return [json.loads(ln) for ln in self.ssh_log.read_text().splitlines()] if self.ssh_log.exists() else []

    def gh_calls(self):
        return [json.loads(ln) for ln in self.gh_log.read_text().splitlines()] if self.gh_log.exists() else []

    def nebius_calls(self):
        return [json.loads(ln) for ln in self.nebius_log.read_text().splitlines()] if self.nebius_log.exists() else []

    def vm_home(self, address, user):
        return self.vms / address / "home" / user

    def group_members(self, address, group):
        for line in (self.vms / address / "group").read_text().splitlines():
            f = line.split(":")
            if f[0] == group:
                return [m for m in f[3].split(",") if m]
        return None
