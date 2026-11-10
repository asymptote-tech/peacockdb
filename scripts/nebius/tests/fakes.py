"""Fake `nebius`, `ssh` and `ssh-keyscan` for nebius.sh's tests, driven by one JSON world.

world = {
  "exists": True, "state": "STOPPED", "ip": "203.0.113.7",
  "start_error": "",          # stderr text and rc 1 on `start`
  "start_to": "RUNNING",      # the state `start` leads to
  "states_after_start": [],   # states `get` reports before start_to, one per call
  "ssh": "ok",                # ok | down | changed
  "ssh_refusals": 0,          # the next n ssh calls are refused before authenticating
  "ssh_seconds": 0,           # how long each ssh call takes
  "rsync_255": 0,             # the next n rsync calls exit 255, as when their transport drops
  "remote": None,             # {"stderr": text, "rc": n}: the remote command, once authenticated
  "host_key": "ssh-ed25519 AAAAKEY1",
  "card": "",                 # what nvidia-smi --query-compute-apps prints
  "no_gpu": False,            # a CPU VM: nvidia-smi is absent (exit 127); a probe that checks first prints cpu_busy
  "cpu_busy": "",             # what the probe's `pgrep -a -x duckdb` prints on a CPU VM
  "stop_error": "",           # stderr text and rc 1 on `stop`
  "get_error": "",            # stderr text and rc 1 on `get`
  "hang": [],                 # verbs ("get", "start", "stop") that sleep 5 s before answering
  "ip_sequence": [],          # IPs `get` reports, one per call, before settling on the last
  "root_profile": "ok",       # ok | broken: the VM's root nebius profile, as install-watch checks it
  "profiles": ["default"],    # the profiles in the config install-watch copies, and the active one
  "active_profile": "default",
  "profile_list": None,       # printed by `profile list` instead of the profiles, for an odd shape
  "profile_config": {...},    # what `config get <key>` prints for any profile; `config unset` clears
  "profile_key_paths": {},    # {profile: path}: that profile's key path, in place of profile_config's
  "uptime": "99999",          # what `cut -d. -f1 /proc/uptime` prints on the VM
  "watch_log": "",            # the watcher's last log line, what `tail -1 /var/log/peacock-gpu-idle.log` prints
  "calls": []                 # appended by every fake: argv lists
}

The fake profile commands answer as CLI 0.12.289 does (new-hosts-detail.md, Task 4): `profile
list` prints names, the active one with " [default]"; deleting the active profile is refused;
`profile update` with a key prompts, and so fails without a tty, unless it has the endpoint,
service account and public key id. `version` prints 0.12.293. Each profile, config or version call
prints FAKE-KEY-MATERIAL on stderr, and on stdout too when its answer is not used, so a caller that
passes the CLI's output on is caught. World.env() names an empty config for install-watch to copy,
and a HOME under the world, not made: nebius.sh reads ~/.config/peacock/ when nothing else is named.

The fake nebius and ssh-keyscan read their stdin to the end, as a CLI that reads stdin would:
a caller that does not give them /dev/null loses the bytes meant for the remote command. The
fake ssh echoes its stdin back when the remote command is `cat`. Its own lines go to its `-E` log
when it has one, with ssh's `Authenticated to` line once the key is accepted; `-G` prints the
`loglevel` and is not a call. The fake rsync only records, and exits as `rsync_255` says.
"""
import json, os, pathlib, stat, sys, tempfile

NEBIUS = r'''#!/usr/bin/env python3
import json, sys
w_path = __import__("os").environ["FAKE_WORLD"]
w = json.load(open(w_path)); a = sys.argv[1:]; w["calls"].append(["nebius"] + a)
if not sys.stdin.isatty(): sys.stdin.read()
def save(): json.dump(w, open(w_path, "w"))
if a[:3] == ["compute", "instance", "get"] and "get" in w["hang"]: save(); __import__("time").sleep(5)
if a[:3] == ["compute", "instance", "start"] and "start" in w["hang"]: save(); __import__("time").sleep(5)
if a[:3] == ["compute", "instance", "stop"] and "stop" in w["hang"]: save(); __import__("time").sleep(5)
if a[:3] == ["compute", "instance", "get"] and w["get_error"]:
    save(); print(w["get_error"], file=sys.stderr); sys.exit(1)
if a[:3] == ["compute", "instance", "get"]:
    if not w["exists"]:
        save(); print("Error: rpc error: code = NotFound desc = instance not found", file=sys.stderr); sys.exit(1)
    if w["states_after_start"]:
        w["state"] = w["states_after_start"].pop(0)
    if w["ip_sequence"]:
        w["ip"] = w["ip_sequence"].pop(0)
    ni = [{"public_ip_address": {"address": w["ip"] + "/32"}}] if w["state"] == "RUNNING" else []
    save(); print(json.dumps({"status": {"state": w["state"], "network_interfaces": ni}})); sys.exit(0)
if a[:3] == ["compute", "instance", "start"]:
    if w["start_error"]:
        save(); print(w["start_error"], file=sys.stderr); sys.exit(1)
    if w["state"] == "STOPPING":
        save(); print("Error: rpc error: code = FailedPrecondition desc = instance is stopping", file=sys.stderr); sys.exit(1)
    w["state"] = "STARTING" if w["states_after_start"] else w["start_to"]; save(); sys.exit(0)
if a[:3] == ["compute", "instance", "stop"]:
    if w["stop_error"]:
        save(); print(w["stop_error"], file=sys.stderr); sys.exit(1)
    w["state"] = "STOPPED"; save(); sys.exit(0)
cmd, i, profile = [], 0, None
while i < len(a):  # the command words, without the global flags install-watch adds
    if a[i] in ("-p", "--profile"): profile = a[i + 1]
    if a[i] in ("-c", "--config", "-p", "--profile"): i += 2; continue
    if a[i] != "--no-check-update": cmd.append(a[i])
    i += 1
profile = profile or w["active_profile"]
def fail(text, rc):
    save(); print(text, file=sys.stderr); sys.exit(rc)
if cmd[:1] in (["profile"], ["config"], ["version"]):
    print("FAKE-KEY-MATERIAL", file=sys.stderr)
    if cmd[:2] not in (["profile", "list"], ["profile", "active"], ["config", "get"], ["version"]):
        print("FAKE-KEY-MATERIAL")
if cmd == ["version"]:
    save(); print("0.12.293"); sys.exit(0)
if cmd[:2] == ["profile", "active"]:
    save(); print(w["active_profile"]); sys.exit(0)
if cmd[:2] == ["profile", "list"]:
    out = w["profile_list"]
    if out is None:
        out = "\n".join(p + (" [default]" if p == w["active_profile"] else "") for p in w["profiles"])
    save(); print(out); sys.exit(0)
if cmd[:2] in (["profile", "activate"], ["profile", "delete"], ["profile", "update"]) and cmd[2:3]:
    name = cmd[2]
    if name not in w["profiles"]:
        fail(f'Error: unknown profile "{name}"', 3)
    if cmd[1] == "delete":
        if name == w["active_profile"]:
            fail(f'Error: cannot delete default profile "{name}"', 3)
        w["profiles"].remove(name)
    elif cmd[1] == "update" and "--private-key-file" in a and not all(
            f in a for f in ("--endpoint", "--service-account-id", "--public-key-id")):
        fail("Error: ^D", 2)
    else:
        w["active_profile"] = name
    save(); sys.exit(0)
paths = w["profile_key_paths"]
if cmd == ["config", "get", "private-key-file-path"] and profile in paths:
    save(); print(paths[profile]); sys.exit(0)
if cmd == ["config", "unset", "private-key-file-path"] and profile in paths:
    paths[profile] = ""; save(); sys.exit(0)
if cmd[:2] == ["config", "get"] and cmd[2:3]:
    save(); print(w["profile_config"].get(cmd[2], "")); sys.exit(0)
if cmd[:2] == ["config", "unset"] and cmd[2:3]:
    w["profile_config"][cmd[2]] = ""; save(); sys.exit(0)
save(); print("fake nebius: unhandled " + " ".join(a), file=sys.stderr); sys.exit(3)
'''

SSH = r'''#!/usr/bin/env python3
import json, os, sys, time
args = sys.argv[1:]
if args[0] == "-G":
    levels = [a.split("=", 1)[1] for a in args if a.startswith("LogLevel=")]
    print("loglevel " + (levels[0] if levels else "INFO"))
    sys.exit(0)
w_path = os.environ["FAKE_WORLD"]; w = json.load(open(w_path)); w["calls"].append(["ssh"] + args)
refused = w["ssh_refusals"] > 0
w["ssh_refusals"] -= refused
json.dump(w, open(w_path, "w"))
time.sleep(w["ssh_seconds"])
at = next(i for i, a in enumerate(args) if "@" in a and not a.startswith("-"))
opts, ip = args[:at], args[at].split("@", 1)[1]
log = open(opts[opts.index("-E") + 1], "a") if "-E" in opts else sys.stderr
kh = next(a.split("=", 1)[1] for a in opts if a.startswith("UserKnownHostsFile="))
if w["ssh"] == "down" or refused:
    print(f"ssh: connect to host {ip} port 22: Connection timed out", file=log); sys.exit(255)
stored = open(kh).read() if os.path.exists(kh) else ""
if w["host_key"] not in stored:
    print("@@@ WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED! @@@\nHost key verification failed.", file=log)
    sys.exit(255)
if log is not sys.stderr:
    print(f'Authenticated to {ip} ([{ip}]:22) using "publickey".', file=log)
    log.close()
cmd = " ".join(args[at + 1:])
if w["remote"]:
    print(w["remote"]["stderr"], file=sys.stderr); sys.exit(w["remote"]["rc"])
if "nvidia-smi" in cmd and not w["no_gpu"]:
    print(w["card"], end="")
elif "nvidia-smi" in cmd:
    # A CPU VM: only a probe that looks for nvidia-smi first gets as far as its pgrep.
    if "command -v nvidia-smi" not in cmd:
        print("bash: line 1: nvidia-smi: command not found", file=sys.stderr); sys.exit(127)
    print(w["cpu_busy"], end="")
if cmd.strip() == "cat":
    sys.stdout.write(sys.stdin.read())
if "/proc/uptime" in cmd:
    print(w["uptime"])
if "peacock-gpu-idle.log" in cmd and w["watch_log"]:
    print(w["watch_log"])
if "HOME=/root" in cmd and "compute instance get" in cmd and w["root_profile"] != "ok":
    print("Error: rpc error: code = Unauthenticated desc = bad credentials", file=sys.stderr); sys.exit(1)
sys.exit(0)
'''

KEYSCAN = r'''#!/usr/bin/env python3
import json, os, sys
w_path = os.environ["FAKE_WORLD"]; w = json.load(open(w_path)); w["calls"].append(["ssh-keyscan"] + sys.argv[1:])
json.dump(w, open(w_path, "w"))
if not sys.stdin.isatty(): sys.stdin.read()
print(sys.argv[-1] + " " + w["host_key"])
'''

RSYNC = r'''#!/usr/bin/env python3
import json, os, sys
w_path = os.environ["FAKE_WORLD"]; w = json.load(open(w_path)); w["calls"].append(["rsync"] + sys.argv[1:])
dropped = w["rsync_255"] > 0
w["rsync_255"] -= dropped
json.dump(w, open(w_path, "w"))
if dropped:
    print("rsync: connection unexpectedly closed (0 bytes received so far) [sender]", file=sys.stderr)
    sys.exit(255)
'''

DEFAULT = {"exists": True, "state": "STOPPED", "ip": "203.0.113.7", "start_error": "",
           "start_to": "RUNNING", "states_after_start": [], "ssh": "ok",
           "ssh_refusals": 0, "ssh_seconds": 0, "rsync_255": 0, "remote": None,
           "host_key": "ssh-ed25519 AAAAKEY1", "card": "", "no_gpu": False, "cpu_busy": "",
           "stop_error": "", "get_error": "",
           "hang": [], "ip_sequence": [], "root_profile": "ok",
           "profiles": ["default"], "active_profile": "default", "profile_list": None,
           "profile_config": {"endpoint": "api.nebius.cloud", "service-account-id": "serviceaccount-fake",
                              "public-key-id": "publickey-fake", "private-key-file-path": ""},
           "profile_key_paths": {},
           "uptime": "99999", "watch_log": "",
           "calls": []}


class World:
    def __init__(self, **over):
        self.dir = pathlib.Path(tempfile.mkdtemp(prefix="gpuhost-"))
        self.path = self.dir / "world.json"
        self.write({**DEFAULT, **over, "calls": []})
        (self.dir / "nebius-config.yaml").write_text("")
        for name, body in (("nebius", NEBIUS), ("ssh", SSH), ("ssh-keyscan", KEYSCAN), ("rsync", RSYNC)):
            f = self.dir / name
            f.write_text(body)
            f.chmod(f.stat().st_mode | stat.S_IEXEC)

    def read(self):
        return json.loads(self.path.read_text())

    def write(self, w):
        self.path.write_text(json.dumps(w))

    def set(self, **kv):
        w = self.read(); w.update(kv); self.write(w)

    def env(self):
        return {**os.environ, "HOME": str(self.dir / "home"), "FAKE_WORLD": str(self.path),
                "NEBIUS": str(self.dir / "nebius"),
                "GPU_HOST_SSH": str(self.dir / "ssh"), "GPU_HOST_KEYSCAN": str(self.dir / "ssh-keyscan"),
                "GPU_HOST_RSYNC": str(self.dir / "rsync"),
                "GPU_HOST_NEBIUS_CONFIG": str(self.dir / "nebius-config.yaml"),
                "GPU_HOST_STATE_DIR": str(self.dir / "state"), "GPU_HOST_POLL": "0",
                "GPU_HOST_START_TIMEOUT": "5", "GPU_HOST_STOP_TIMEOUT": "5", "PEACOCK_SSH_PAUSES": "0 0 0"}
