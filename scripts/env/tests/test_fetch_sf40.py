"""`fetch-sf40.sh`: the sf40 sets from their buckets onto a VM, each object checked.

It runs from a copy of itself whose `scripts/env/sf40.sha256` and `docker/pins.env` pin small
stand-ins. `aws` is a fake on PATH that records its argv, refuses without the storage keys in its
environment, and serves objects from a directory per bucket. `curl` is a fake that serves the aws
CLI's zip, a stand-in whose installer puts the fake aws where the real one would go. Where strace
is on PATH, the endpoint test also reads every process's argv.
"""
import hashlib
import io
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from machine import path_without  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
TREE = ["scripts/env/fetch-sf40.sh", "scripts/lib/s3.sh", "scripts/lib/peacock-config.sh",
        "scripts/lib/pinned.sh"]
ENDPOINT = "https://s3.elsewhere.example.test:8443"
REGION = "test-south4"
KEY_ID = "AKIDFAKE0key0id"
SECRET = "s3cr3t/FAKE+value=x"
ENV_FILE = (f"# the storage keys\nAWS_ACCESS_KEY_ID={KEY_ID}\nAWS_SECRET_ACCESS_KEY={SECRET}\n"
            f"AWS_ENDPOINT_URL={ENDPOINT}\nAWS_REGION={REGION}\nGPU_SSH_KEY_FILE=/home/x/.config/peacock/gpu_ed25519\n")
AWS_CLI_VERSION = "2.38.3"
TPCH = {"lineitem.parquet": b"L" * 1000, "nation.parquet": b"N" * 25}
TPCDS = {"call_center.parquet": b"C" * 40, "store_sales.parquet": b"S" * 300}

FAKE_AWS = r'''#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
with open(os.environ["FAKE_AWS_LOG"], "a") as f:
    f.write(json.dumps({"argv": args, "exe": sys.argv[0]}) + "\n")
if (os.environ.get("AWS_ACCESS_KEY_ID") != os.environ["FAKE_KEY_ID"]
        or os.environ.get("AWS_SECRET_ACCESS_KEY") != os.environ["FAKE_SECRET"]):
    print("Unable to locate credentials. You can configure credentials by running \"aws configure\".",
          file=sys.stderr)
    sys.exit(253)
rest, i = [], 0
while i < len(args):
    if args[i] in ("--endpoint-url", "--region"):
        i += 2
        continue
    if args[i] != "--only-show-errors":
        rest.append(args[i])
    i += 1
if rest[:2] != ["s3", "cp"] or len(rest) != 4 or not rest[2].startswith("s3://"):
    print(f"fake aws: unexpected {rest}", file=sys.stderr)
    sys.exit(2)
bucket, _, key = rest[2][len("s3://"):].partition("/")
source = pathlib.Path(os.environ["FAKE_BUCKETS"]) / bucket / key
if not source.is_file():
    print(f"fatal error: An error occurred (404) when calling the HeadObject operation: Key \"{key}\" does not exist",
          file=sys.stderr)
    sys.exit(1)
data = source.read_bytes()
if key in os.environ.get("FAKE_AWS_SHORT", "").split():
    data = data[:len(data) // 2]
if rest[3] == "-":
    sys.stdout.buffer.write(data)
else:
    pathlib.Path(rest[3]).write_bytes(data)
'''

# The aws CLI v2 zip's shape: aws/install, run as `aws/install -i <dir> -b <bin dir>`, links
# <bin dir>/aws to the copy under <dir>.
FAKE_AWS_INSTALL = r'''#!/bin/bash
set -e
while [ $# -gt 0 ]; do
  case $1 in -i) i=$2; shift 2 ;; -b) b=$2; shift 2 ;; *) echo "fake install: $1" >&2; exit 2 ;; esac
done
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$i/v2/current/bin" "$b"
cp "$here/dist/aws" "$i/v2/current/bin/aws"
ln -sf "$i/v2/current/bin/aws" "$b/aws"
'''

FAKE_CURL = r'''#!/usr/bin/env python3
import json, os, pathlib, shutil, sys
args = sys.argv[1:]
keys = [k for k in ("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY") if k in os.environ]
with open(os.environ["FAKE_CURL_LOG"], "a") as f:
    f.write(json.dumps({"argv": args, "keys": keys}) + "\n")
url = [a for a in args if a.startswith("https://")][-1]
source = pathlib.Path(os.environ["FAKE_CURL_SERVE"]) / url.rsplit("/", 1)[-1]
if not source.is_file() or "-o" not in args:
    sys.exit(22)
shutil.copyfile(source, args[args.index("-o") + 1])
'''


def aws_zip():
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        for name, text in (("aws/install", FAKE_AWS_INSTALL), ("aws/dist/aws", FAKE_AWS)):
            info = zipfile.ZipInfo(name)
            info.external_attr = 0o100755 << 16
            z.writestr(info, text)
    return buf.getvalue()


AWS_ZIP = aws_zip()
AWS_ZIP_NAME = f"awscli-exe-linux-x86_64-{AWS_CLI_VERSION}.zip"


def aggregate(files):
    """`sha256sum *.parquet | sha256sum` in the C locale's order."""
    lines = "".join(f"{hashlib.sha256(data).hexdigest()}  {name}\n" for name, data in sorted(files.items()))
    return hashlib.sha256(lines.encode()).hexdigest()


class Vm:
    """A copy of the scripts, a home with the env file, a data dir, the buckets and the fakes."""

    def __init__(self, tmp, pins=None, aws_on_path=True, aws_zip_sha=None):
        tmp = pathlib.Path(tmp)
        self.tmp = tmp
        self.root = tmp / "repo"
        for rel in TREE:
            (self.root / rel).parent.mkdir(parents=True, exist_ok=True)
            if (REPO / rel).exists():
                shutil.copy2(REPO / rel, self.root / rel)
        self.script = self.root / "scripts/env/fetch-sf40.sh"
        if pins is None:
            pins = {"tpcds.sf40": aggregate(TPCDS)}
        (self.root / "scripts/env/sf40.sha256").write_text("".join(f"{s}  {d}\n" for d, s in pins.items()))
        (self.root / "docker").mkdir()
        (self.root / "docker/pins.env").write_text(
            f"AWS_CLI_VERSION={AWS_CLI_VERSION}\nAWS_CLI_SHA256={aws_zip_sha or hashlib.sha256(AWS_ZIP).hexdigest()}\n")
        self.home = tmp / "home"
        (self.home / ".config/peacock").mkdir(parents=True)
        (self.home / ".config/peacock/peacock.env").write_text(ENV_FILE)
        self.data = tmp / "data"
        self.data.mkdir()
        self.buckets = tmp / "buckets"
        for bucket, files in (("tpch-sf40", TPCH), ("tpcds-sf40", TPCDS)):
            self.bucket(bucket, files)
        self.served = tmp / "served"
        self.served.mkdir()
        (self.served / AWS_ZIP_NAME).write_bytes(AWS_ZIP)
        self.bin = tmp / "bin"
        self.bin.mkdir()
        self.install("curl", FAKE_CURL)
        if aws_on_path:
            self.install("aws", FAKE_AWS)
        self.aws_log = tmp / "aws.log"
        self.curl_log = tmp / "curl.log"
        system = path_without(tmp, {"aws", "curl"})
        self.env = {k: v for k, v in os.environ.items()
                    if not k.startswith("AWS_") and k not in ("PEACOCK_DATA_DIR", "LC_ALL", "LANG")}
        self.env.update(HOME=str(self.home), PATH=f"{self.bin}:{system}", LANG="C.UTF-8",
                        FAKE_AWS_LOG=str(self.aws_log), FAKE_CURL_LOG=str(self.curl_log),
                        FAKE_CURL_SERVE=str(self.served), FAKE_BUCKETS=str(self.buckets),
                        FAKE_KEY_ID=KEY_ID, FAKE_SECRET=SECRET)

    def bucket(self, name, files, manifest=None):
        d = self.buckets / name
        if d.exists():
            shutil.rmtree(d)
        d.mkdir(parents=True)
        for f, data in files.items():
            (d / f).write_bytes(data)
        (d / "manifest.json").write_text(json.dumps(manifest or {f: len(b) for f, b in files.items()}))

    def install(self, name, text):
        (self.bin / name).write_text(text)
        (self.bin / name).chmod(0o755)

    def run(self, *args, env=None, stdin=None, strace=None):
        argv = ["bash", str(self.script), *args]
        if strace:
            argv = ["strace", "-f", "-qq", "-s", "4096", "-e", "trace=execve", "-o", str(strace), *argv]
        return subprocess.run(argv, capture_output=True, text=True, timeout=60, input=stdin,
                              env={**self.env, **(env or {})})

    def fetch(self, *datasets, env=None):
        return self.run("--data-dir", str(self.data), *datasets, env=env)

    def aws_calls(self):
        if not self.aws_log.exists():
            return []
        return [json.loads(line) for line in self.aws_log.read_text().splitlines()]

    def curl_calls(self):
        if not self.curl_log.exists():
            return []
        return [json.loads(line) for line in self.curl_log.read_text().splitlines()]


def ok(r):
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def objects_copied(vm):
    """The bucket objects each aws call copied, manifests included."""
    return [a for c in vm.aws_calls() for a in c["argv"] if a.startswith("s3://")]


def held(vm, dataset, files):
    d = vm.data / dataset
    assert sorted(p.name for p in d.iterdir() if not p.name.startswith(".")) == sorted(files), list(d.iterdir())
    for name, data in files.items():
        assert (d / name).read_bytes() == data, name


def test_fetch_takes_endpoint_and_region_from_env():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp)
        trace = pathlib.Path(tmp) / "strace.out" if shutil.which("strace") else None
        r = ok(vm.run("--data-dir", str(vm.data), "tpch.sf40", "tpcds.sf40", strace=trace))
        held(vm, "tpch.sf40", TPCH)
        held(vm, "tpcds.sf40", TPCDS)
        calls = vm.aws_calls()
        assert len(calls) == 2 + len(TPCH) + len(TPCDS), calls
        for c in calls:
            pairs = list(zip(c["argv"], c["argv"][1:]))
            assert ("--endpoint-url", ENDPOINT) in pairs and ("--region", REGION) in pairs, c
            assert not any(KEY_ID in a or SECRET in a for a in c["argv"]), c
        assert KEY_ID not in r.stdout + r.stderr and SECRET not in r.stdout + r.stderr
        if trace:
            execs = [ln for ln in trace.read_text().splitlines() if "execve(" in ln]
            assert any("fetch-sf40.sh" in ln for ln in execs), execs[:3]
            assert not [ln for ln in execs if KEY_ID in ln or SECRET in ln], "a key in some argv"


def test_fetch_second_run_fetches_nothing():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp)
        ok(vm.fetch("tpch.sf40"))
        vm.aws_log.unlink()
        r = ok(vm.fetch("tpch.sf40"))
        assert objects_copied(vm) == ["s3://tpch-sf40/manifest.json"], objects_copied(vm)
        assert "tpch.sf40: fetched 0, kept 2" in r.stdout.splitlines(), r.stdout


def test_fetch_prints_what_it_fetched():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp)
        r = ok(vm.fetch("tpch.sf40", "tpcds.sf40"))
        lines = r.stdout.splitlines()
        assert "tpch.sf40: fetched 2, kept 0" in lines and "tpcds.sf40: fetched 2, kept 0" in lines, r.stdout


def test_fetch_refetches_a_short_object():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp)
        (vm.data / "tpch.sf40").mkdir()
        (vm.data / "tpch.sf40/lineitem.parquet").write_bytes(TPCH["lineitem.parquet"][:300])
        (vm.data / "tpch.sf40/nation.parquet").write_bytes(TPCH["nation.parquet"])
        r = ok(vm.fetch("tpch.sf40"))
        held(vm, "tpch.sf40", TPCH)
        assert "s3://tpch-sf40/lineitem.parquet" in objects_copied(vm), objects_copied(vm)
        assert "s3://tpch-sf40/nation.parquet" not in objects_copied(vm), objects_copied(vm)
        assert "tpch.sf40: fetched 1, kept 1" in r.stdout.splitlines(), r.stdout


def test_fetch_refuses_a_short_download_and_keeps_nothing_of_it():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp)
        r = vm.fetch("tpch.sf40", env={"FAKE_AWS_SHORT": "lineitem.parquet"})
        assert r.returncode == 1, (r.returncode, r.stdout, r.stderr)
        assert "tpch.sf40" in r.stderr and "lineitem.parquet" in r.stderr, r.stderr
        left = sorted(p.name for p in (vm.data / "tpch.sf40").iterdir() if not p.name.startswith("."))
        assert "lineitem.parquet" not in left and not [n for n in left if "lineitem" in n], left
        r = ok(vm.fetch("tpch.sf40"))
        held(vm, "tpch.sf40", TPCH)


def test_fetch_refuses_a_wrong_aggregate():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp, pins={"tpch.sf40": "0" * 64})
        r = vm.fetch("tpch.sf40")
        assert r.returncode == 1, (r.returncode, r.stdout, r.stderr)
        assert "tpch.sf40" in r.stderr and "0" * 64 in r.stderr and aggregate(TPCH) in r.stderr, r.stderr
        assert "tpch.sf40: fetched" not in r.stdout, r.stdout


def test_fetch_accepts_its_pinned_aggregate():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp)
        r = ok(vm.fetch("tpcds.sf40"))
        assert "tpcds.sf40: fetched 2, kept 0" in r.stdout.splitlines(), r.stdout


def test_fetch_prints_an_unpinned_aggregate():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp, pins={})
        r = ok(vm.fetch("tpch.sf40"))
        assert re.search(rf"^tpch\.sf40: aggregate {aggregate(TPCH)}, not pinned", r.stdout, re.M), r.stdout


def test_fetch_aggregate_is_in_the_c_locales_order():
    """The pin was taken in C order: in en_US, customer_address sorts before customer."""
    files = {"customer.parquet": b"c" * 10, "customer_address.parquet": b"a" * 20}
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp, pins={"tpch.sf40": aggregate(files)})
        vm.bucket("tpch-sf40", files)
        probe = pathlib.Path(tmp) / "probe"
        probe.mkdir()
        for name in files:
            (probe / name).touch()
        order = subprocess.run(["bash", "-c", "echo *.parquet"], cwd=probe, capture_output=True, text=True,
                               env={**vm.env, "LC_ALL": "en_US.UTF-8"}, timeout=10).stdout.split()
        assert order == ["customer_address.parquet", "customer.parquet"], f"en_US.UTF-8 not installed? {order}"
        ok(vm.fetch("tpch.sf40", env={"LC_ALL": "en_US.UTF-8"}))


def test_fetch_refuses_a_parquet_the_manifest_does_not_name():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp)
        (vm.data / "tpch.sf40").mkdir()
        (vm.data / "tpch.sf40/stale.parquet").write_bytes(b"old")
        r = vm.fetch("tpch.sf40")
        assert r.returncode == 1, (r.returncode, r.stdout, r.stderr)
        assert "stale.parquet" in r.stderr and "tpch.sf40" in r.stderr, r.stderr


def test_fetch_refuses_a_manifest_naming_a_path():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp)
        vm.bucket("tpch-sf40", TPCH, manifest={"../escape.parquet": 3, **{f: len(b) for f, b in TPCH.items()}})
        r = vm.fetch("tpch.sf40")
        assert r.returncode == 1, (r.returncode, r.stdout, r.stderr)
        assert "manifest" in r.stderr and "tpch-sf40" in r.stderr, r.stderr
        assert not (vm.data / "escape.parquet").exists()
        assert objects_copied(vm) == ["s3://tpch-sf40/manifest.json"], objects_copied(vm)


def test_fetch_refuses_an_unknown_dataset():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp)
        for args in (["tpch.sf1"], ["tpch.sf40", "tpch.sf200"], []):
            r = vm.fetch(*args)
            assert r.returncode == 2, (args, r.returncode, r.stdout, r.stderr)
            assert len(r.stderr.strip().splitlines()) == 1, r.stderr
        assert vm.aws_calls() == [], vm.aws_calls()
        assert not (vm.data / "tpch.sf40").exists()


def test_fetch_refuses_an_unknown_option():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp)
        r = vm.fetch("--dta-dir", "tpch.sf40")
        assert r.returncode == 2 and "--dta-dir" in r.stderr, (r.returncode, r.stderr)
        assert vm.aws_calls() == []


def test_fetch_keys_from_stdin():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp)
        (vm.home / ".config/peacock/peacock.env").unlink()
        r = vm.run("--data-dir", str(vm.data), "--env", "-", "tpch.sf40", stdin=ENV_FILE)
        ok(r)
        held(vm, "tpch.sf40", TPCH)


def test_fetch_keys_from_a_named_file():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp)
        (vm.home / ".config/peacock/peacock.env").unlink()
        named = pathlib.Path(tmp) / "keys.env"
        named.write_text(ENV_FILE)
        ok(vm.run("--data-dir", str(vm.data), "--env", str(named), "tpch.sf40"))
        held(vm, "tpch.sf40", TPCH)


def test_fetch_refuses_an_env_without_a_key():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp)
        no_secret = "".join(ln + "\n" for ln in ENV_FILE.splitlines() if not ln.startswith("AWS_SECRET_ACCESS_KEY="))
        r = vm.run("--data-dir", str(vm.data), "--env", "-", "tpch.sf40", stdin=no_secret)
        assert r.returncode == 2 and "AWS_SECRET_ACCESS_KEY" in r.stderr, (r.returncode, r.stderr)
        assert KEY_ID not in r.stderr, r.stderr
        assert vm.aws_calls() == []


def test_fetch_refuses_a_missing_env_file():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp)
        (vm.home / ".config/peacock/peacock.env").unlink()
        r = vm.fetch("tpch.sf40")
        assert r.returncode == 2 and "peacock.env" in r.stderr, (r.returncode, r.stderr)
        assert vm.aws_calls() == []


def test_fetch_keeps_the_keys_from_every_command_but_aws():
    """Keys the caller exported are the env file's from then on, and aws's alone: curl, which
    installs aws, sees none."""
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp, aws_on_path=False)
        ok(vm.fetch("tpch.sf40", env={"AWS_ACCESS_KEY_ID": "AKIDCALLER", "AWS_SECRET_ACCESS_KEY": "caller"}))
        curls = vm.curl_calls()
        assert len(curls) == 1 and curls[0]["keys"] == [], curls


def test_fetch_data_dir_defaults_to_peacock_data_dir():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp)
        ok(vm.run("tpch.sf40", env={"PEACOCK_DATA_DIR": str(vm.data)}))
        held(vm, "tpch.sf40", TPCH)


def test_fetch_installs_the_pinned_aws_cli_when_none_is_on_path():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp, aws_on_path=False)
        ok(vm.fetch("tpch.sf40"))
        held(vm, "tpch.sf40", TPCH)
        curls = vm.curl_calls()
        assert len(curls) == 1, curls
        assert f"https://awscli.amazonaws.com/{AWS_ZIP_NAME}" in curls[0]["argv"], curls
        assert curls[0]["keys"] == [], "the storage keys reached curl"
        installed = vm.home / ".local/aws-cli/bin/aws"
        assert installed.exists(), list((vm.home / ".local").rglob("*"))
        assert vm.aws_calls() and all(c["exe"] == str(installed) for c in vm.aws_calls()), vm.aws_calls()
        vm.curl_log.unlink()
        ok(vm.fetch("tpch.sf40"))
        assert vm.curl_calls() == [], vm.curl_calls()


def test_fetch_refuses_an_aws_cli_zip_of_another_sha():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp, aws_on_path=False, aws_zip_sha="f" * 64)
        r = vm.fetch("tpch.sf40")
        assert r.returncode == 1, (r.returncode, r.stdout, r.stderr)
        assert AWS_ZIP_NAME in r.stderr, r.stderr
        assert not (vm.home / ".local/aws-cli").exists()
        assert vm.aws_calls() == []


def test_fetch_takes_aws_from_the_environment():
    with tempfile.TemporaryDirectory() as tmp:
        vm = Vm(tmp, aws_on_path=False)
        elsewhere = pathlib.Path(tmp) / "elsewhere"
        elsewhere.mkdir()
        (elsewhere / "aws").write_text(FAKE_AWS)
        (elsewhere / "aws").chmod(0o755)
        ok(vm.fetch("tpch.sf40", env={"AWS": str(elsewhere / "aws")}))
        assert vm.curl_calls() == []
        assert all(c["exe"] == str(elsewhere / "aws") for c in vm.aws_calls()), vm.aws_calls()


def test_committed_pin_is_tpcds_sf40s():
    lines = (REPO / "scripts/env/sf40.sha256").read_text().splitlines()
    pins = dict(reversed(re.fullmatch(r"([0-9a-f]{64})  (\S+)", ln).groups()) for ln in lines)
    assert pins.get("tpcds.sf40") == "58610fc16152a12ea9769d645a6e72a4e03002e2498ec8d357862fcbd9bebb36", pins
    assert set(pins) <= {"tpch.sf40", "tpcds.sf40"}, pins


if __name__ == "__main__":
    raise SystemExit(main(globals()))
