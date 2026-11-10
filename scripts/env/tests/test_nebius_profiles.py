"""`scripts/env/nebius-profiles.sh` and the nebius functions of `scripts/lib/peacock-config.sh`:
the developer's two profiles cut from a copy of `~/.nebius/config.yaml`, through the CLI alone.

Each test's HOME holds a source config of four profiles: `editor-eu`, its key embedded;
`eu-west1`, its key in `~/.nebius/eu-west1-sa.pem`; `ci` and `old`, `old` the active one. The CLI
is `nebius_cli.py`'s fake, on a PATH that has no other `nebius`. The PEM holds FAKE-KEY-MATERIAL,
which the fake also prints for every call whose answer is not used.
"""
import hashlib
import json
import os
import pathlib
import signal
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from machine import path_without  # noqa: E402
from nebius_cli import install_cli, nebius_config  # noqa: E402

SCRIPTS = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = SCRIPTS / "env" / "nebius-profiles.sh"
LIB = SCRIPTS / "lib" / "peacock-config.sh"
INSTALLER = "curl -sSL https://storage.eu-north1.nebius.cloud/cli/install.sh | bash"


class Laptop:
    def __init__(self, profiles=None, cli="path"):
        self.tmp = pathlib.Path(tempfile.mkdtemp(prefix="nebius-profiles-"))
        self.home = self.tmp / "home"
        self.pem = self.home / ".nebius" / "eu-west1-sa.pem"
        self.pem.parent.mkdir(parents=True)
        self.pem.write_text("-----BEGIN PRIVATE KEY-----\nFAKE-KEY-MATERIAL\n-----END PRIVATE KEY-----\n")
        if profiles is None:
            profiles = {"editor-eu": {"key_path": None}, "eu-west1": {"key_path": str(self.pem)},
                        "ci": {"key_path": None}, "old": {"key_path": None}}
        self.source = nebius_config(self.home / ".nebius" / "config.yaml", "old", profiles)
        self.dest = self.home / ".config" / "peacock" / "nebius.yaml"
        self.log = self.tmp / "nebius.log"
        bin_dir = {"path": self.tmp / "bin", "dot-nebius": self.home / ".nebius" / "bin"}.get(cli)
        self.cli = install_cli(bin_dir) if bin_dir else None
        self.env = {"PATH": f"{self.tmp / 'bin'}:{path_without(self.tmp, {'nebius'})}",
                    "HOME": str(self.home), "LANG": "C.UTF-8", "FAKE_NEBIUS_LOG": str(self.log)}

    def run(self, *args, shell=(), **extra):
        p = subprocess.run([*shell, str(SCRIPT), *args], env={**self.env, **extra}, capture_output=True,
                           text=True, timeout=60, stdin=subprocess.DEVNULL)
        return p.returncode, p.stdout, p.stderr

    def lib(self, call, **extra):
        p = subprocess.run(["bash", "-c", f'set -euo pipefail; . "{LIB}"; {call}'], env={**self.env, **extra},
                           capture_output=True, text=True, timeout=60, stdin=subprocess.DEVNULL)
        return p.returncode, p.stdout, p.stderr

    def calls(self):
        if not self.log.exists():
            return []
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def ask(self, config, *args):
        """The fake CLI's answer on <config>, as a test reads it; not logged."""
        env = {**self.env}
        del env["FAKE_NEBIUS_LOG"]
        p = subprocess.run([str(self.cli), "--config", str(config), *args], env=env, capture_output=True,
                           text=True, timeout=60, stdin=subprocess.DEVNULL, check=True)
        return p.stdout

    def left_in_peacock(self):
        d = self.dest.parent
        return sorted(f.name for f in d.iterdir()) if d.exists() else []


def sha(path):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def test_cuts_the_two_profiles_out_of_several():
    m = Laptop()
    before = sha(m.source)
    rc, out, err = m.run()
    assert rc == 0, (rc, err)
    assert m.ask(m.dest, "profile", "list") == "editor-eu [default]\neu-west1\n"
    for p in ("editor-eu", "eu-west1"):
        assert m.ask(m.dest, "--profile", p, "config", "get", "private-key-file-path") == "\n", p
    updates = [c for c in m.calls() if "update" in c]
    assert len(updates) == 1, m.calls()
    u = updates[0]
    assert u[u.index("update") + 1] == "eu-west1" and u[u.index("--private-key-file") + 1] == str(m.pem), u
    copy = pathlib.Path(u[u.index("--config") + 1])
    assert copy.name == "nebius.yaml" and copy.parent.name.startswith("nebius-cut."), u
    assert m.dest.stat().st_mode & 0o777 == 0o600, oct(m.dest.stat().st_mode)
    assert sha(m.source) == before
    assert out == f"{m.dest}: editor-eu (active), eu-west1; 2 other profiles left out\n", out
    assert m.left_in_peacock() == ["nebius.yaml"], m.left_in_peacock()


def test_every_call_is_on_a_copy_never_the_source():
    m = Laptop()
    rc, out, err = m.run()
    assert rc == 0, (rc, err)
    assert m.calls(), "no CLI call"
    for c in m.calls():
        assert "--config" in c and "--no-check-update" in c, c
        config = pathlib.Path(c[c.index("--config") + 1])
        assert config.parent.parent == m.dest.parent and config.parent.name.startswith("nebius-cut."), c


def test_refuses_a_missing_profile_naming_it():
    m = Laptop(profiles={"editor-eu": {"key_path": None}, "ci": {"key_path": None}, "old": {"key_path": None}})
    rc, out, err = m.run()
    assert rc == 2, (rc, err)
    assert "eu-west1" in err, err
    assert m.left_in_peacock() == [], m.left_in_peacock()


def test_refuses_a_key_path_naming_no_file():
    m = Laptop()
    m.pem.unlink()
    rc, out, err = m.run()
    assert rc == 2, (rc, err)
    assert "eu-west1" in err and str(m.pem) in err, err
    assert m.left_in_peacock() == [], m.left_in_peacock()


def test_a_rerun_replaces_the_file():
    m = Laptop()
    m.dest.parent.mkdir(parents=True)
    m.dest.write_text("other bytes\n")
    m.dest.chmod(0o644)
    before = sha(m.dest)
    rc, out, err = m.run()
    assert rc == 0, (rc, err)
    assert sha(m.dest) != before
    assert m.dest.stat().st_mode & 0o777 == 0o600, oct(m.dest.stat().st_mode)
    assert m.ask(m.dest, "profile", "list") == "editor-eu [default]\neu-west1\n"


def test_prints_only_the_names_and_a_count():
    m = Laptop()
    rc, out, err = m.run()
    assert rc == 0, (rc, err)
    assert "FAKE-KEY-MATERIAL" not in out + err, (out, err)


def test_a_late_refusal_leaves_nothing_and_prints_no_key():
    for verb in ("update", "activate", "delete"):
        m = Laptop()
        rc, out, err = m.run(FAKE_NEBIUS_FAIL=verb)
        assert rc == 2, (verb, rc, err)
        assert "FAKE-KEY-MATERIAL" not in out + err, (verb, out, err)
        assert m.left_in_peacock() == [], (verb, m.left_in_peacock())


def cut_signalled(sig, group):
    """A cut signalled while the fake sleeps in `update`, to the process group or to bash alone, which
    leaves the CLI running to write the copy by its path; returns once that CLI has ended."""
    m = Laptop()
    m.dest.parent.mkdir(parents=True)
    m.dest.write_text("the earlier cut\n")
    p = subprocess.Popen([str(SCRIPT)], env={**m.env, "FAKE_NEBIUS_SLOW": "update", "FAKE_NEBIUS_SLOW_SECONDS": "2"},
                         stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                         start_new_session=True)
    deadline = time.monotonic() + 30
    while not any(c[0] == "slow" for c in m.calls()):
        assert time.monotonic() < deadline and p.poll() is None, (sig, p.poll(), m.calls())
        time.sleep(0.05)
    cli = next(c[1] for c in m.calls() if c[0] == "slow")
    (os.killpg if group else os.kill)(p.pid, sig)
    out, err = p.communicate(timeout=60)
    assert p.returncode != 0, (sig, group, p.returncode, err)
    while True:
        try:
            os.kill(cli, 0)
        except ProcessLookupError:
            break
        assert time.monotonic() < deadline + 30, (sig, group, "the CLI did not end")
        time.sleep(0.05)
    return m


def test_a_signal_mid_cut_leaves_only_what_was_there():
    for sig, group in ((signal.SIGTERM, True), (signal.SIGHUP, True), (signal.SIGINT, True),
                       (signal.SIGTERM, False), (signal.SIGHUP, False)):
        m = cut_signalled(sig, group)
        assert m.left_in_peacock() == ["nebius.yaml"], (sig, group, m.left_in_peacock())
        assert m.dest.read_text() == "the earlier cut\n", (sig, group)


def test_the_copy_is_600_while_the_cut_runs():
    m = Laptop()
    m.source.chmod(0o644)
    p = subprocess.Popen([str(SCRIPT)], env={**m.env, "FAKE_NEBIUS_SLOW": "update"}, stdin=subprocess.DEVNULL,
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True)
    try:
        deadline = time.monotonic() + 30
        while not any(c[0] == "slow" for c in m.calls()):
            assert time.monotonic() < deadline and p.poll() is None, (p.poll(), m.calls())
            time.sleep(0.05)
        update = next(c for c in m.calls() if "update" in c)
        mode = pathlib.Path(update[update.index("--config") + 1]).stat().st_mode & 0o777
    finally:
        os.killpg(p.pid, signal.SIGTERM)
        p.communicate(timeout=60)
    assert mode == 0o600, oct(mode)


def test_no_cli_answer_is_traced_under_bash_x():
    m = Laptop()
    rc, out, err = m.run(shell=("bash", "-x"))
    assert rc == 0 and "+ " in err, (rc, err)
    assert "FAKE-KEY-MATERIAL" not in out + err, err
    rc, out, err = m.run(shell=("bash", "-x"), FAKE_NEBIUS_LIST="old [default]\n  key: FAKE-KEY-MATERIAL")
    assert rc == 2, (rc, err)
    assert "FAKE-KEY-MATERIAL" not in out + err, err


def test_refuses_a_missing_source_config():
    m = Laptop()
    m.source.unlink()
    rc, out, err = m.run()
    assert rc == 2, (rc, err)
    assert str(m.source) in err, err
    assert not m.calls() and m.left_in_peacock() == [], (m.calls(), m.left_in_peacock())


def test_needs_a_cli_naming_the_installer():
    m = Laptop(cli=None)
    rc, out, err = m.run()
    assert rc == 2, (rc, err)
    assert INSTALLER in err and "scripts/env/bootstrap-vm.sh" in err, err
    assert not m.dest.exists()


def test_takes_the_cli_from_dot_nebius_bin():
    m = Laptop(cli="dot-nebius")
    rc, out, err = m.run()
    assert rc == 0, (rc, err)
    assert m.calls(), "the CLI in ~/.nebius/bin was not called"
    assert m.ask(m.dest, "profile", "list") == "editor-eu [default]\neu-west1\n"


def test_refuses_a_profile_list_it_cannot_read_naming_the_version():
    m = Laptop()
    rc, out, err = m.run(FAKE_NEBIUS_LIST="old [default]\n    private-key: |\n        SECRETLINE")
    assert rc == 2, (rc, err)
    assert "0.12.293" in err, err
    assert "SECRETLINE" not in out + err and "private-key" not in out + err, err
    assert m.left_in_peacock() == [], m.left_in_peacock()


def test_a_version_of_another_shape_is_named_unknown():
    m = Laptop()
    rc, out, err = m.run(FAKE_NEBIUS_LIST="old [default]\n  key: x", FAKE_NEBIUS_VERSION="key: FAKE-KEY-MATERIAL")
    assert rc == 2 and "version unknown" in err, (rc, err)
    assert "FAKE-KEY-MATERIAL" not in out + err, err


def test_takes_only_help():
    m = Laptop()
    rc, out, err = m.run("--help")
    assert rc == 0 and "nebius-profiles.sh" in out and not m.calls(), (rc, out, err)
    rc, out, err = m.run("--bogus")
    assert rc == 2 and "--bogus" in err and not m.calls(), (rc, err)


def test_the_profile_list_parser_gives_the_names_in_order():
    m = Laptop()
    rc, out, err = m.lib("printf 'b\\na [default]\\nc.d_e-f\\n' | peacock_nebius_profiles")
    assert (rc, out) == (0, "b\na\nc.d_e-f\n"), (rc, out, err)


def test_the_profile_list_parser_refuses_another_shape_printing_nothing():
    m = Laptop()
    for text in ("a [default]\\n  key: x", "a [default]\\nb [default]", "a [active]", "a\\n\\nb"):
        rc, out, err = m.lib(f"printf '{text}\\n' | peacock_nebius_profiles")
        assert rc == 1 and out == "", (text, rc, out, err)


def checked(m, config, **extra):
    rc, out, err = m.lib(f'peacock_nebius_config_checked "{m.cli}" "{config}"', **extra)
    assert "FAKE-KEY-MATERIAL" not in out + err, (out, err)
    return rc, out, err


def test_a_cut_config_passes_the_check():
    m = Laptop()
    assert m.run()[0] == 0
    m.log.unlink()
    rc, out, err = checked(m, m.dest)
    assert rc == 0, (rc, err)
    assert m.calls(), "the check called no CLI"
    for c in m.calls():
        assert c[c.index("--config") + 1] == str(m.dest), c


def test_the_check_refuses_a_missing_config_naming_nebius_profiles():
    m = Laptop()
    rc, out, err = checked(m, m.dest)
    assert rc == 2 and "scripts/env/nebius-profiles.sh" in err, (rc, err)
    assert not m.calls()


def test_the_check_names_each_extra_and_missing_profile():
    m = Laptop()
    config = nebius_config(m.tmp / "two.yaml", "editor-eu", {"editor-eu": {"key_path": None},
                                                             "stray": {"key_path": None}})
    rc, out, err = checked(m, config)
    assert rc == 2 and "stray" in err and "eu-west1" in err, (rc, err)


def test_the_check_refuses_a_profile_that_reads_its_key_from_a_file():
    m = Laptop()
    config = nebius_config(m.tmp / "two.yaml", "editor-eu", {"editor-eu": {"key_path": None},
                                                             "eu-west1": {"key_path": str(m.pem)}})
    rc, out, err = checked(m, config)
    assert rc == 2 and "eu-west1" in err and "nebius-profiles.sh" in err, (rc, err)


def test_the_check_refuses_a_list_it_cannot_read_naming_the_version():
    m = Laptop()
    assert m.run()[0] == 0
    rc, out, err = checked(m, m.dest, FAKE_NEBIUS_LIST="SECRETLINE: x")
    assert rc == 2 and "0.12.293" in err and "SECRETLINE" not in err, (rc, err)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
