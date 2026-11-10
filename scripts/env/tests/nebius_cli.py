"""A fake `nebius` CLI that keeps its state, as JSON, in the config file it is given, and a fixture
writer for that file.

The config is `-c`/`--config`'s, else `$HOME/.nebius/config.yaml`; one it cannot read fails as the
CLI does. It answers `version`, `profile list|active|activate|delete|update` and
`config get|unset`, the profile `--profile`/`-p` names or the active one. Deleting the active
profile is refused, and `update` needs the three ids and a `--private-key-file` that exists, as
CLI 0.12 does without a terminal. `update` embeds the key and keeps the profile's key path, which
`config unset private-key-file-path` drops. Every argv is appended to `FAKE_NEBIUS_LOG` as a JSON
line; `FAKE_NEBIUS_LIST` replaces `profile list`'s output and `FAKE_NEBIUS_VERSION` `version`'s.
`FAKE_NEBIUS_SLOW=<verb>` makes that call read its config, log `["slow", <pid>]`, sleep
`FAKE_NEBIUS_SLOW_SECONDS` (30), then act and write the config by its path, as a CLI left running
would; `FAKE_NEBIUS_FAIL=<verb>` fails it; the verb is the word after `profile` or `config` (`update`, `activate`, `delete`…), or `version`.
FAKE-KEY-MATERIAL stands in for a CLI that echoes a key, so a script that passes any of it on is
caught: a call whose answer a caller may use prints it on stderr, its answer alone on stdout;
every other call prints it on both.
"""
import json
import pathlib
import stat

FAKE_NEBIUS_CLI = r'''#!/usr/bin/env python3
import json, os, sys
a = sys.argv[1:]
if os.environ.get("FAKE_NEBIUS_LOG"):
    with open(os.environ["FAKE_NEBIUS_LOG"], "a") as f:
        f.write(json.dumps(a) + "\n")
config, profile, words, flags, i = None, None, [], {}, 0
while i < len(a):
    if a[i] in ("-c", "--config"): config = a[i + 1]; i += 2
    elif a[i] in ("-p", "--profile"): profile = a[i + 1]; i += 2
    elif a[i] in ("--endpoint", "--service-account-id", "--public-key-id", "--private-key-file"):
        flags[a[i]] = a[i + 1]; i += 2
    elif a[i] in ("--no-check-update", "--skip-auth"): i += 1
    else: words.append(a[i]); i += 1
def fail(text, rc=1):
    print("Error: " + text, file=sys.stderr); sys.exit(rc)
def noise(stdout=True):
    if stdout: print("FAKE-KEY-MATERIAL")
    print("FAKE-KEY-MATERIAL", file=sys.stderr)
verb = words[1] if len(words) > 1 and words[0] in ("profile", "config") else (words or [""])[0]
if os.environ.get("FAKE_NEBIUS_FAIL") == verb:
    noise(); fail(f"{verb} failed")
if words == ["version"]:
    noise(stdout=False); print(os.environ.get("FAKE_NEBIUS_VERSION", "0.12.293")); sys.exit(0)
config = config or os.path.join(os.environ["HOME"], ".nebius", "config.yaml")
try:
    state = json.load(open(config))
except OSError as e:
    fail(f"load config: {e}")
profiles = state["profiles"]
def save():
    json.dump(state, open(config, "w"))
if os.environ.get("FAKE_NEBIUS_SLOW") == verb:
    with open(os.environ["FAKE_NEBIUS_LOG"], "a") as f:
        f.write(json.dumps(["slow", os.getpid()]) + "\n")
    __import__("time").sleep(float(os.environ.get("FAKE_NEBIUS_SLOW_SECONDS", "30")))
if words[:2] in (["profile", "list"], ["profile", "active"], ["config", "get"]):
    noise(stdout=False)
if words == ["profile", "list"]:
    out = os.environ.get("FAKE_NEBIUS_LIST")
    if out is None:
        out = "\n".join(p + (" [default]" if p == state["active"] else "") for p in profiles)
    print(out); sys.exit(0)
if words == ["profile", "active"]:
    print(state["active"]); sys.exit(0)
name = words[2] if words[:1] == ["profile"] and len(words) == 3 else profile or state["active"]
if name not in profiles:
    noise(); fail(f'profile "{name}" not found', 3)
p = profiles[name]
if words[:2] == ["config", "get"] and len(words) == 3:
    print(p.get(words[2]) or ""); sys.exit(0)
noise()
if words == ["config", "unset", "private-key-file-path"]:
    p["private-key-file-path"] = ""
elif words == ["profile", "activate", name]:
    state["active"] = name
elif words == ["profile", "delete", name]:
    if name == state["active"]:
        fail(f'cannot delete default profile "{name}"', 3)
    del profiles[name]
elif words == ["profile", "update", name]:
    if not all(f in flags for f in ("--endpoint", "--service-account-id", "--public-key-id")):
        fail("^D", 2)
    if not os.path.isfile(flags.get("--private-key-file", "")):
        fail(f"read private key file: {flags.get('--private-key-file')}: no such file")
    p.update({"endpoint": flags["--endpoint"], "service-account-id": flags["--service-account-id"],
              "public-key-id": flags["--public-key-id"], "embedded-from": flags["--private-key-file"]})
else:
    fail("fake nebius: unhandled " + " ".join(a), 3)
save()
'''


def install_cli(directory):
    """Writes the fake as `<directory>/nebius`, executable, and returns its path."""
    directory = pathlib.Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    cli = directory / "nebius"
    cli.write_text(FAKE_NEBIUS_CLI)
    cli.chmod(cli.stat().st_mode | stat.S_IEXEC)
    return cli


def nebius_config(path, active, profiles):
    """Writes a config the fake reads: `profiles = {name: {"key_path": str | None}}`, in order."""
    path = pathlib.Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps({"active": active, "profiles": {
        name: {"endpoint": "api.nebius.cloud", "service-account-id": f"serviceaccount-{name}",
               "public-key-id": f"publickey-{name}", "private-key-file-path": p["key_path"] or ""}
        for name, p in profiles.items()}}))
    path.chmod(0o600)
    return path
