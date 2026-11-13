"""`install-duckdb.sh`: the DuckDB CLI at its docker/pins.env version, its zip checked.

It runs from a copy of itself whose `docker/pins.env` pins a stand-in zip, holding a `duckdb` that
prints the version it was made with. `curl` is a fake that serves that zip; `sudo` a fake that
records its argv and runs it.
"""
import hashlib
import io
import json
import os
import pathlib
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
TREE = ["scripts/env/install-duckdb.sh", "scripts/lib/pinned.sh", "scripts/lib/peacock-config.sh"]
VERSION = "1.5.4"
URL = f"https://github.com/duckdb/duckdb/releases/download/v{VERSION}/duckdb_cli-linux-amd64.zip"

FAKE_CURL = r'''#!/usr/bin/env python3
import json, os, pathlib, shutil, sys
args = sys.argv[1:]
with open(os.environ["FAKE_CURL_LOG"], "a") as f:
    f.write(json.dumps(args) + "\n")
url = [a for a in args if a.startswith("https://")][-1]
source = pathlib.Path(os.environ["FAKE_CURL_SERVE"]) / url.rsplit("/", 1)[-1]
if not source.is_file() or "-o" not in args:
    sys.exit(22)
shutil.copyfile(source, args[args.index("-o") + 1])
'''

FAKE_SUDO = r'''#!/usr/bin/env python3
import json, os, subprocess, sys
with open(os.environ["FAKE_SUDO_LOG"], "a") as f:
    f.write(json.dumps(sys.argv[1:]) + "\n")
sys.exit(subprocess.run(sys.argv[1:]).returncode)
'''


def duckdb(version):
    return f'#!/bin/sh\n[ "$1" = --version ] && echo "v{version} (Andium) 0d84ccf478"\n'


def duckdb_zip(version=VERSION):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        info = zipfile.ZipInfo("duckdb")
        info.external_attr = 0o100755 << 16
        z.writestr(info, duckdb(version))
    return buf.getvalue()


class Box:
    def __init__(self, tmp, zip_bytes=None, pinned_sha=None):
        tmp = pathlib.Path(tmp)
        self.root = tmp / "repo"
        for rel in TREE:
            (self.root / rel).parent.mkdir(parents=True, exist_ok=True)
            if (REPO / rel).exists():
                shutil.copy2(REPO / rel, self.root / rel)
        self.script = self.root / "scripts/env/install-duckdb.sh"
        zip_bytes = zip_bytes or duckdb_zip()
        (self.root / "docker").mkdir()
        (self.root / "docker/pins.env").write_text(
            f"DUCKDB_VERSION={VERSION}\nDUCKDB_SHA256={pinned_sha or hashlib.sha256(zip_bytes).hexdigest()}\n")
        self.served = tmp / "served"
        self.served.mkdir()
        (self.served / "duckdb_cli-linux-amd64.zip").write_bytes(zip_bytes)
        self.prefix = tmp / "prefix"
        self.bin = tmp / "bin"
        self.bin.mkdir()
        for name, text in (("curl", FAKE_CURL), ("sudo", FAKE_SUDO)):
            (self.bin / name).write_text(text)
            (self.bin / name).chmod(0o755)
        self.curl_log = tmp / "curl.log"
        self.sudo_log = tmp / "sudo.log"
        system = path_without(tmp, {"curl", "sudo", "duckdb", "unzip"})
        self.env = {**os.environ, "HOME": str(tmp), "PATH": f"{self.bin}:{system}",
                    "FAKE_CURL_LOG": str(self.curl_log), "FAKE_CURL_SERVE": str(self.served),
                    "FAKE_SUDO_LOG": str(self.sudo_log)}

    def run(self, *args):
        return subprocess.run(["bash", str(self.script), *args], capture_output=True, text=True, timeout=60,
                              env=self.env)

    def install(self):
        return self.run("--prefix", str(self.prefix))

    def place(self, version):
        (self.prefix / "bin").mkdir(parents=True, exist_ok=True)
        (self.prefix / "bin/duckdb").write_text(duckdb(version))
        (self.prefix / "bin/duckdb").chmod(0o755)

    def version(self):
        return subprocess.run([str(self.prefix / "bin/duckdb"), "--version"], capture_output=True, text=True,
                              timeout=10).stdout.split()[0]

    def log(self, path):
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


def ok(r):
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def test_install_duckdb_installs_the_pinned_cli():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp)
        ok(box.install())
        assert box.version() == f"v{VERSION}"
        assert os.access(box.prefix / "bin/duckdb", os.X_OK)
        curls = box.log(box.curl_log)
        assert len(curls) == 1 and URL in curls[0], curls
        assert box.log(box.sudo_log) == []
        assert sorted(p.name for p in box.prefix.rglob("*")) == ["bin", "duckdb"], list(box.prefix.rglob("*"))


def test_install_duckdb_checks_the_sha():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp, pinned_sha="e" * 64)
        r = box.install()
        assert r.returncode == 1, (r.returncode, r.stdout, r.stderr)
        assert "duckdb_cli-linux-amd64.zip" in r.stderr and "sha256" in r.stderr, r.stderr
        assert not (box.prefix / "bin/duckdb").exists()


def test_install_duckdb_names_a_download_that_fails():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp)
        (box.served / "duckdb_cli-linux-amd64.zip").unlink()
        r = box.install()
        assert r.returncode == 1, (r.returncode, r.stdout, r.stderr)
        assert URL in r.stderr and "sha256" not in r.stderr, r.stderr
        assert not (box.prefix / "bin/duckdb").exists()


def test_install_duckdb_skips_the_pinned_version():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp)
        box.place(VERSION)
        before = (box.prefix / "bin/duckdb").stat().st_mtime_ns
        ok(box.install())
        assert box.log(box.curl_log) == []
        assert (box.prefix / "bin/duckdb").stat().st_mtime_ns == before


def test_install_duckdb_replaces_another_version():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp)
        box.place("1.4.0")
        ok(box.install())
        assert box.version() == f"v{VERSION}"


def test_install_duckdb_refuses_a_zip_of_another_version():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp, zip_bytes=duckdb_zip("1.4.0"))
        r = box.install()
        assert r.returncode == 1 and "v1.4.0" in r.stderr and f"v{VERSION}" in r.stderr, (r.returncode, r.stderr)
        assert not (box.prefix / "bin/duckdb").exists()


def test_install_duckdb_takes_sudo_where_the_prefix_is_not_writable():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp)
        (box.prefix / "bin").mkdir(parents=True)
        (box.prefix / "bin").chmod(0o555)
        try:
            if os.access(box.prefix / "bin", os.W_OK):
                raise AssertionError("running as root: the read-only prefix cannot be modelled")
            # The fake sudo runs as the caller, so it needs the dir writable to finish.
            sudo = f'#!/bin/sh\necho "$*" >> "$FAKE_SUDO_LOG"\nchmod 755 "{box.prefix / "bin"}"\nexec "$@"\n'
            (box.bin / "sudo").write_text(sudo)
            ok(box.install())
            calls = box.sudo_log.read_text().splitlines()
            assert calls and all(str(box.prefix / "bin") in c for c in calls), calls
            assert box.version() == f"v{VERSION}"
        finally:
            (box.prefix / "bin").chmod(0o755)


def test_install_duckdb_refuses_an_unknown_option():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp)
        r = box.run("--prefx", str(box.prefix))
        assert r.returncode == 2 and "--prefx" in r.stderr, (r.returncode, r.stderr)
        assert box.log(box.curl_log) == []


if __name__ == "__main__":
    raise SystemExit(main(globals()))
