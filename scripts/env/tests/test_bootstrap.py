"""bootstrap-vm.sh: what it refuses before its first install. The installs themselves need a VM.

Run from a clone on the fake machine of `machine.py`, with a fake sudo and curl that record their
argv, so a refusal can be checked to come before anything is installed, and nothing is downloaded.
"""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from machine import FAKE_ROOT_ID, Machine, git, path_without  # noqa: E402

FAKE_SUDO = '#!/bin/sh\nprintf "%s\\n" "sudo $*" >> "$FAKE_SUDO_LOG"\nexit "${FAKE_SUDO_STATUS:-1}"\n'
FAKE_CURL = '#!/bin/sh\nprintf "%s\\n" "curl $*" >> "$FAKE_SUDO_LOG"\nexit 22\n'


def bootstrap(m, *args, **env):
    """Runs bootstrap-vm.sh; returns its result and what sudo and curl were asked, in order."""
    log = m.home.parent / "sudo.log"
    m.install("sudo", FAKE_SUDO)
    m.install("curl", FAKE_CURL)
    r = m.run("bootstrap-vm.sh", *args, env={"FAKE_SUDO_LOG": str(log), **env})
    return r, log.read_text() if log.exists() else ""


def test_bootstrap_has_help():
    with Machine() as m:
        r, sudo = bootstrap(m, "--help")
        assert r.returncode == 0 and "bootstrap-vm.sh" in r.stdout and "--cudf" in r.stdout, r
        assert sudo == "" and m.docker_calls() == []


def test_bootstrap_refuses_root():
    with Machine() as m:
        m.install("id", FAKE_ROOT_ID)
        r, sudo = bootstrap(m)
        assert r.returncode == 2 and "root" in r.stderr and len(r.stderr.strip().splitlines()) == 1, r
        assert sudo == "" and m.docker_calls() == []


def test_bootstrap_refuses_bad_arguments_before_any_install():
    for args in [("--bogus",), ("--cudf",), ("--cudf", "latest"), ("--ref",), ("stray",)]:
        with Machine() as m:
            r, sudo = bootstrap(m, *args)
            assert r.returncode == 2 and len(r.stderr.strip().splitlines()) == 1, (args, r.stderr)
            assert r.stderr.startswith("bootstrap-vm.sh: "), (args, r.stderr)
            assert sudo == "" and m.docker_calls() == [], (args, sudo)


def test_bootstrap_refuses_a_ref_the_clone_is_not_on():
    with Machine() as m:
        r, sudo = bootstrap(m, "--ref", "no-such-branch")
        assert r.returncode == 2 and str(m.clone) in r.stderr and "--ref" in r.stderr, r
        assert sudo == "" and m.docker_calls() == []


def test_bootstrap_takes_the_ref_the_clone_is_on():
    with Machine() as m:
        r, log = bootstrap(m, "--ref", git(m.clone, "branch", "--show-current"))
        assert "--ref" not in r.stderr, r.stderr
        assert log, f"the run stopped before its first install: {r.stderr}"


def test_bootstrap_updates_the_package_lists_once_before_any_install():
    with Machine() as m:
        system = path_without(m.home.parent, {"docker", "sudo", "curl", "gh"})
        m.bin.joinpath("docker").unlink()
        r, log = bootstrap(m, FAKE_SUDO_STATUS="0", PATH=f"{m.bin}:{system}")
        apt = [line for line in log.splitlines() if line.startswith("sudo apt-get")]
        assert apt and apt[0].startswith("sudo apt-get update"), log
        assert [a for a in apt if " update" in a] == [apt[0]], log
        assert any("install" in a and "docker.io" in a for a in apt[1:]), log


if __name__ == "__main__":
    raise SystemExit(main(globals()))
