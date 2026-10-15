"""Fake `nebius`, `ssh` and `ssh-keyscan` for gpu-host.sh's tests, driven by one JSON world.

world = {
  "exists": True, "state": "STOPPED", "ip": "203.0.113.7",
  "start_error": "",          # stderr text and rc 1 on `start`
  "start_to": "RUNNING",      # the state `start` leads to
  "states_after_start": [],   # states `get` reports before start_to, one per call
  "ssh": "ok",                # ok | down | changed
  "host_key": "ssh-ed25519 AAAAKEY1",
  "card": "",                 # what nvidia-smi --query-compute-apps prints
  "stop_error": "",           # stderr text and rc 1 on `stop`
  "get_error": "",            # stderr text and rc 1 on `get`
  "hang": [],                 # verbs ("get", "start", "stop") that sleep 5 s before answering
  "ip_sequence": [],          # IPs `get` reports, one per call, before settling on the last
  "root_profile": "ok",       # ok | broken: the VM's root nebius profile, as install-watch checks it
  "calls": []                 # appended by every fake: argv lists
}

The fake nebius and ssh-keyscan read their stdin to the end, as a CLI that reads stdin would:
a caller that does not give them /dev/null loses the bytes meant for the remote command. The
fake ssh echoes its stdin back when the remote command is `cat`. The fake rsync only records.
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
save(); print("fake nebius: unhandled " + " ".join(a), file=sys.stderr); sys.exit(3)
'''

SSH = r'''#!/usr/bin/env python3
import json, os, sys
w_path = os.environ["FAKE_WORLD"]; w = json.load(open(w_path)); w["calls"].append(["ssh"] + sys.argv[1:])
json.dump(w, open(w_path, "w"))
args = sys.argv[1:]
kh = next(a.split("=", 1)[1] for i, a in enumerate(args) if a.startswith("UserKnownHostsFile="))
if w["ssh"] == "down":
    print("ssh: connect to host: Connection timed out", file=sys.stderr); sys.exit(255)
stored = open(kh).read() if os.path.exists(kh) else ""
if w["host_key"] not in stored:
    print("@@@ WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED! @@@\nHost key verification failed.", file=sys.stderr)
    sys.exit(255)
at = next(i for i, a in enumerate(args) if "@" in a and not a.startswith("-"))
cmd = " ".join(args[at + 1:])
if "nvidia-smi" in cmd:
    print(w["card"], end="")
if cmd.strip() == "cat":
    sys.stdout.write(sys.stdin.read())
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
json.dump(w, open(w_path, "w"))
'''

DEFAULT = {"exists": True, "state": "STOPPED", "ip": "203.0.113.7", "start_error": "",
           "start_to": "RUNNING", "states_after_start": [], "ssh": "ok",
           "host_key": "ssh-ed25519 AAAAKEY1", "card": "", "stop_error": "", "get_error": "",
           "hang": [], "ip_sequence": [], "root_profile": "ok", "calls": []}


class World:
    def __init__(self, **over):
        self.dir = pathlib.Path(tempfile.mkdtemp(prefix="gpuhost-"))
        self.path = self.dir / "world.json"
        self.write({**DEFAULT, **over, "calls": []})
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
        return {**os.environ, "FAKE_WORLD": str(self.path), "NEBIUS": str(self.dir / "nebius"),
                "GPU_HOST_SSH": str(self.dir / "ssh"), "GPU_HOST_KEYSCAN": str(self.dir / "ssh-keyscan"),
                "GPU_HOST_RSYNC": str(self.dir / "rsync"),
                "GPU_HOST_STATE_DIR": str(self.dir / "state"), "GPU_HOST_POLL": "0",
                "GPU_HOST_START_TIMEOUT": "5", "GPU_HOST_STOP_TIMEOUT": "5"}
