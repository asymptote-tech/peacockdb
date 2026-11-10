"""bootstrap-vm.sh: what it refuses before its first install, and what a whole run leaves.

Run from a clone on the fake machine of `machine.py`, with a fake sudo and curl that record their
argv, so a refusal can be checked to come before anything is installed, and nothing is downloaded.
A whole run has curl serve a stand-in gh tarball and nebius binary, which the clone's pins then
name, and finds Claude Code and superpowers at their pins; sudo runs nothing. No PATH here holds
the host's nebius; a test that wants one installs its own.
"""
import hashlib
import json
import os
import pathlib
import pwd
import re
import shutil
import stat
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from machine import FAKE_ROOT_ID, NO_HOST, UNPUBLISHED, Machine, git, lib, path_without  # noqa: E402

FAKE_SUDO = '#!/bin/sh\nprintf "%s\\n" "sudo $*" >> "$FAKE_SUDO_LOG"\nexit "${FAKE_SUDO_STATUS:-1}"\n'
FAKE_CURL = '#!/bin/sh\nprintf "%s\\n" "curl $*" >> "$FAKE_SUDO_LOG"\nexit 22\n'
GH_TARBALL = b"a stand-in gh tarball"
NEBIUS_BINARY = b"a stand-in nebius binary"
# curl writing the nebius binary to its -o for a URL of Nebius's, unless FAKE_NEBIUS_MISSING, the
# gh tarball for any other, and failing a download to stdout (the Claude Code installer).
SERVING_CURL = r'''#!/bin/sh
printf "%s\n" "curl $*" >> "$FAKE_SUDO_LOG"
body=$FAKE_GH_TARBALL
for a in "$@"; do
  case "$a" in *artifacts.nebius.cloud*) [ -z "$FAKE_NEBIUS_MISSING" ] || exit 22; body=$FAKE_NEBIUS_BINARY ;; esac
done
while [ $# -gt 0 ]; do
  if [ "$1" = -o ]; then printf %s "$body" > "$2"; exit 0; fi
  shift
done
exit 22
'''
NEBIUS_INSTALL = re.compile(r"^sudo install -m 755 \S+ /usr/local/bin/nebius$", re.M)


def bootstrap(m, *args, **env):
    """Runs bootstrap-vm.sh; returns its result and what sudo and curl were asked, in order."""
    log = m.home.parent / "sudo.log"
    m.install("sudo", FAKE_SUDO)
    m.install("curl", FAKE_CURL)
    r = m.run("bootstrap-vm.sh", *args, env={"FAKE_SUDO_LOG": str(log), **env})
    return r, log.read_text() if log.exists() else ""


def pin(m, name):
    return next(ln.split("=", 1)[1] for ln in (m.clone / "docker" / "pins.env").read_text().splitlines()
                if ln.startswith(name + "="))


def set_pin(m, name, value):
    pins = m.clone / "docker" / "pins.env"
    pins.write_text("".join(f"{name}={value}\n" if ln.startswith(name + "=") else ln + "\n"
                            for ln in pins.read_text().splitlines()))


def whole_run_env(m):
    """Readies `m` for a whole run of bootstrap-vm.sh; returns the environment it runs with."""
    set_pin(m, "GH_SHA256", hashlib.sha256(GH_TARBALL).hexdigest())
    set_pin(m, "NEBIUS_SHA256", hashlib.sha256(NEBIUS_BINARY).hexdigest())
    m.world(label=m.label("25.02"))
    record = m.home / ".claude" / "plugins" / "installed_plugins.json"
    record.parent.mkdir(parents=True, exist_ok=True)
    record.write_text(json.dumps({"plugins": {"superpowers@claude-plugins-official": [
        {"version": pin(m, "SUPERPOWERS_VERSION"), "gitCommitSha": pin(m, "SUPERPOWERS_SHA")}]}}))
    m.install("sudo", FAKE_SUDO)
    m.install("curl", SERVING_CURL)
    tmpdir = m.home.parent / "tmpdir"
    tmpdir.mkdir(exist_ok=True)
    return {"FAKE_SUDO_LOG": str(m.home.parent / "sudo.log"), "FAKE_SUDO_STATUS": "0",
            "FAKE_GH_TARBALL": GH_TARBALL.decode(), "FAKE_NEBIUS_BINARY": NEBIUS_BINARY.decode(),
            "TMPDIR": str(tmpdir)}


def nebius_url(m):
    return f"https://artifacts.nebius.cloud/cli/release/{pin(m, 'NEBIUS_VERSION')}/linux/x86_64/nebius"


def bootstrap_whole(m):
    """Runs bootstrap-vm.sh to its end on `m`; returns the result."""
    r = m.run("bootstrap-vm.sh", env=whole_run_env(m))
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def generator_runs(m):
    return [c for c in m.docker_calls() if c[0] == "run" and any(a.endswith("/generate_testdata.sh") for a in c)]


def data_hash(m):
    return lib(m.clone, "data-inputs.sh", f'data_inputs "{m.clone}"')


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
        assert "no-such-branch" in r.stderr and "master" in r.stderr, r.stderr
        assert sudo == "" and m.docker_calls() == []


def test_bootstrap_takes_the_ref_the_clone_is_on():
    with Machine() as m:
        r, log = bootstrap(m, "--ref", git(m.clone, "branch", "--show-current"))
        assert "--ref" not in r.stderr, r.stderr
        assert log, f"the run stopped before its first install: {r.stderr}"


def test_bootstrap_updates_the_package_lists_once_before_any_install():
    with Machine() as m:
        system = path_without(m.home.parent, {"docker", "sudo", "curl", "gh", "nebius"})
        m.bin.joinpath("docker").unlink()
        r, log = bootstrap(m, FAKE_SUDO_STATUS="0", PATH=f"{m.bin}:{system}")
        apt = [line for line in log.splitlines() if line.startswith("sudo apt-get")]
        assert apt and apt[0].startswith("sudo apt-get update"), log
        assert [a for a in apt if " update" in a] == [apt[0]], log
        assert any("install" in a and "docker.io" in a for a in apt[1:]), log


def test_bootstrap_makes_the_machine_wide_data_directory():
    with Machine() as m:
        data = m.home.parent / "machine-data"
        r, log = bootstrap(m, FAKE_SUDO_STATUS="0", PEACOCK_DATA_DIR=str(data))
        lines = log.splitlines()
        user = pwd.getpwuid(os.getuid()).pw_name
        made = f"sudo install -d -o {user} -g peacock -m 2775 {data}"
        assert made in lines, log
        downloads = [i for i, line in enumerate(lines) if line.startswith("curl ")]
        assert not downloads or lines.index(made) < downloads[0], log


def test_the_data_directory_is_machine_wide_by_default():
    with Machine() as m:
        env = {k: v for k, v in m.env.items() if k != "PEACOCK_DATA_DIR"}
        out = subprocess.run(["bash", "-c", f'. "{m.clone}/scripts/lib/machine.sh"; echo "$MACHINE_DATA"'],
                             env=env, check=True, capture_output=True, text=True, timeout=30).stdout
        assert out.strip() == "/var/lib/peacock", out


def test_bootstrap_leaves_no_marker():
    with Machine() as m:
        bootstrap_whole(m)
        assert not (m.home / ".config" / "peacock").exists(), list((m.home / ".config" / "peacock").iterdir())
        bashrc = (m.home / ".bashrc").read_text()
        assert "CLAUDE_CONFIG_DIR" not in bashrc, bashrc
        assert "export DISABLE_AUTOUPDATER=1" in bashrc.splitlines(), bashrc


def scripts_named(text):
    """Every script `text` names but bootstrap-vm.sh and provision-vm.sh."""
    return set(re.findall(r"[\w.-]+\.sh\b", text)) - {"bootstrap-vm.sh", "provision-vm.sh"}


def test_bootstrap_names_no_launcher():
    with Machine() as m:
        r = bootstrap_whole(m)
        assert scripts_named(r.stdout + r.stderr) == set(), (r.stdout, r.stderr)
    with Machine() as m:
        m.install("id", FAKE_ROOT_ID)
        r, _ = bootstrap(m)
        assert r.returncode == 2 and "instance" not in r.stderr, r.stderr
        assert scripts_named(r.stderr) == set(), r.stderr


def test_bootstrap_installs_the_pinned_nebius_cli():
    """With none on PATH, and with one of another version."""
    for installed in [None, "0.12.1"]:
        with Machine() as m:
            env = whole_run_env(m)
            if installed:
                m.install("nebius", f"#!/bin/sh\necho {installed}\n")
            r = m.run("bootstrap-vm.sh", env=env)
            assert r.returncode == 0, (installed, r.returncode, r.stderr)
            log = (m.home.parent / "sudo.log").read_text()
            assert f" {nebius_url(m)}\n" in log, (installed, log)
            assert len(NEBIUS_INSTALL.findall(log)) == 1, (installed, log)
            assert list((m.home.parent / "tmpdir").iterdir()) == [], (installed, log)


def test_bootstrap_refuses_a_nebius_cli_of_another_sha():
    with Machine() as m:
        env = whole_run_env(m)
        set_pin(m, "NEBIUS_SHA256", hashlib.sha256(b"another build").hexdigest())
        m.world(label=m.label("25.02"))
        r = m.run("bootstrap-vm.sh", env=env)
        assert r.returncode == 2 and "bootstrap-vm.sh: " in r.stderr, (r.returncode, r.stderr)
        assert "NEBIUS_SHA256" in r.stderr and nebius_url(m) in r.stderr, r.stderr
        log = (m.home.parent / "sudo.log").read_text()
        assert not NEBIUS_INSTALL.search(log), log
        assert list((m.home.parent / "tmpdir").iterdir()) == [], log
        assert [c for c in m.docker_calls() if c[0] == "run"] == [], m.docker_calls()


def test_bootstrap_names_a_nebius_cli_it_cannot_download():
    with Machine() as m:
        r = m.run("bootstrap-vm.sh", env={**whole_run_env(m), "FAKE_NEBIUS_MISSING": "1"})
        assert r.returncode == 2 and f"cannot download {nebius_url(m)}" in r.stderr, (r.returncode, r.stderr)
        log = (m.home.parent / "sudo.log").read_text()
        assert not NEBIUS_INSTALL.search(log), log
        assert list((m.home.parent / "tmpdir").iterdir()) == [], log


def test_bootstrap_leaves_no_download_when_the_nebius_install_fails():
    with Machine() as m:
        env = whole_run_env(m)
        m.install("sudo", FAKE_SUDO.replace('exit "${FAKE_SUDO_STATUS:-1}"',
                                            '[ "$1" != install ] || exit 1; exit "${FAKE_SUDO_STATUS:-1}"'))
        r = m.run("bootstrap-vm.sh", env=env)
        log = (m.home.parent / "sudo.log").read_text()
        assert r.returncode != 0 and NEBIUS_INSTALL.search(log), (r.returncode, r.stderr, log)
        assert list((m.home.parent / "tmpdir").iterdir()) == [], log


def test_bootstrap_skips_the_nebius_cli_at_its_pin():
    with Machine() as m:
        env = whole_run_env(m)
        m.install("nebius", f"#!/bin/sh\n[ \"$*\" = version ] && echo {pin(m, 'NEBIUS_VERSION')}\n")
        r = m.run("bootstrap-vm.sh", env=env)
        assert r.returncode == 0, (r.returncode, r.stderr)
        log = (m.home.parent / "sudo.log").read_text()
        assert "artifacts.nebius.cloud" not in log and "/usr/local/bin/nebius" not in log, log


def test_bootstrap_refuses_an_image_of_other_inputs():
    with Machine() as m:
        env = whole_run_env(m)
        label = m.label("25.02").replace(f"third_party/cudf={m.cudf_shas[0]}", f"third_party/cudf={m.cudf_shas[1]}")
        m.world(label=label)
        r = m.run("bootstrap-vm.sh", env=env)
        assert r.returncode == 2 and "third_party/cudf" in r.stderr and "docker/pins.env" not in r.stderr, r.stderr
        assert [c for c in m.docker_calls() if c[0] == "run"] == [], m.docker_calls()


def test_a_plain_build_is_refused_naming_image_sh():
    """A plain `docker build` has no peacockdb.inputs label."""
    with Machine() as m:
        env = whole_run_env(m)
        m.world(label=None)
        r = m.run("bootstrap-vm.sh", env=env)
        assert r.returncode == 2, (r.returncode, r.stderr)
        assert "peacockdb.inputs" in r.stderr and "scripts/env/image.sh" in r.stderr, r.stderr
        assert [c for c in m.docker_calls() if c[0] == "run"] == [], m.docker_calls()


def test_bootstrap_uses_the_image_of_the_clones_inputs():
    """By the inputs tag, so a clone whose inputs master's :cudf-<v> no longer holds still finds its image."""
    with Machine() as m:
        bootstrap_whole(m)
        tag = lib(m.clone, "image-inputs.sh", f'image_inputs_tag "{m.clone}" build 25.02')
        ref = f"ghcr.io/asymptote-tech/peacockdb-build:{tag}"
        calls = m.docker_calls()
        assert [c for c in calls if c[0] == "pull"] == [["pull", ref]], calls
        runs = [c for c in calls if c[0] == "run"]
        assert runs and all(ref in c for c in runs), runs


def test_bootstrap_names_the_publish_of_an_unpublished_image():
    for message in UNPUBLISHED:
        with Machine() as m:
            env = whole_run_env(m)
            tag = lib(m.clone, "image-inputs.sh", f'image_inputs_tag "{m.clone}" build 25.02')
            ref = f"ghcr.io/asymptote-tech/peacockdb-build:{tag}"
            m.world(pull_error=message.format(ref=ref))
            r = m.run("bootstrap-vm.sh", env=env)
            branch = git(m.clone, "branch", "--show-current")
            assert r.returncode == 2 and f"{ref} is not published" in r.stderr, (r.returncode, r.stderr)
            assert f"gh workflow run images.yml --ref {branch} -f kind=build -f cudf=25.02" in r.stderr, r.stderr
            assert [c for c in m.docker_calls() if c[0] == "run"] == [], m.docker_calls()
    with Machine() as m:
        env = whole_run_env(m)
        m.world(pull_error=NO_HOST)
        r = m.run("bootstrap-vm.sh", env=env)
        assert r.returncode == 1 and "cannot pull" in r.stderr and NO_HOST in r.stderr, (r.returncode, r.stderr)


def test_bootstrap_refuses_a_clone_older_than_the_inputs_tag():
    """A --ref of a branch before the inputs tag: its image-inputs.sh has no image_inputs_tag."""
    with Machine() as m:
        env = whole_run_env(m)
        lib_file = m.clone / "scripts" / "lib" / "image-inputs.sh"
        text = lib_file.read_text()
        lib_file.write_text(text[:text.index("# image_inputs_tag ")])
        r = m.run("bootstrap-vm.sh", env=env)
        assert r.returncode == 2 and "image_inputs_tag" in r.stderr and "--ref" in r.stderr, (r.returncode, r.stderr)
        assert [c for c in m.docker_calls() if c[0] in ("pull", "run")] == [], m.docker_calls()


def test_bootstrap_fetches_the_registry_with_the_clone_read_only():
    with Machine() as m:
        bootstrap_whole(m)
        [fetch] = [c for c in m.docker_calls() if c[0] == "run" and "cargo" in c]
        registry = m.home / ".cache" / "peacock" / "cargo-registry"
        assert fetch[-3:] == ["cargo", "fetch", "--locked"], fetch
        assert f"type=bind,src={m.clone},dst=/peacock/tree,readonly" in fetch, fetch
        assert fetch[fetch.index("-w") + 1] == "/peacock/tree", fetch
        assert f"type=bind,src={registry},dst=/opt/rust/cargo/registry" in fetch, fetch
        assert fetch[fetch.index("--user") + 1] == f"{os.getuid()}:{os.getgid()}", fetch


def test_the_generator_runs_at_a_container_path():
    with Machine() as m:
        bootstrap_whole(m)
        stage = m.data_dir / f"{data_hash(m)}.partial"
        runs = generator_runs(m)
        assert len(runs) == 2, runs
        for run in runs:
            assert f"type=bind,src={stage},dst=/peacock/stage" in run, run
            assert run[run.index("-w") + 1] == "/peacock/stage", run
            assert "/peacock/stage/generate_testdata.sh" in run, run


def test_bootstrap_generates_sf1_by_generator_inputs_once():
    with Machine() as m:
        bootstrap_whole(m)
        data_set = m.data_dir / data_hash(m)
        assert sorted(p.name for p in data_set.iterdir()) == ["tpcds.sf1", "tpch.sf1"], list(data_set.iterdir())
        assert not list(m.data_dir.glob("*.partial")), list(m.data_dir.iterdir())
        for run in generator_runs(m):
            assert run[run.index("--network") + 1] == "none" and "PEACOCK_NO_FETCH=1" in run, run
        bootstrap_whole(m)
        assert len(generator_runs(m)) == 2, generator_runs(m)


def test_bootstrap_keeps_one_cudf_checkout_a_commit():
    with Machine() as m:
        (m.clone / "third_party" / "cudf" / "VERSION").write_text("the clone's own, never read\n")
        bootstrap_whole(m)
        checkout = m.data_dir / "cudf" / m.cudf_sha
        assert (checkout / "VERSION").read_text() == "25.02.00\n"
        assert (checkout / "cmake" / "rapids_config.cmake").is_file()
        assert not (checkout / "README.md").exists(), "a path outside the sparse set was checked out"
        shutil.move(m.cudf_origin, m.home.parent / "cudf-origin-gone")
        bootstrap_whole(m)


def test_bootstrap_refuses_a_cudf_commit_it_cannot_fetch():
    with Machine() as m:
        missing = "0123456789abcdef0123456789abcdef01234567"
        git(m.clone, "update-index", "--cacheinfo", f"160000,{missing},third_party/cudf")
        r, _ = bootstrap(m, FAKE_SUDO_STATUS="0")
        assert r.returncode == 2 and missing in r.stderr, r.stderr
        assert not (m.data_dir / "cudf" / missing).exists()
        assert not list((m.data_dir / "cudf").glob("*.partial")), list((m.data_dir / "cudf").iterdir())
        assert m.docker_calls() == [], m.docker_calls()


def test_bootstrap_leaves_the_shared_directory_writable_by_the_group():
    with Machine() as m:
        bootstrap_whole(m)
        made = [m.data_dir / "cudf", m.data_dir / "cudf" / m.cudf_sha, m.data_dir / data_hash(m),
                m.data_dir / data_hash(m) / "tpch.sf1"]
        for path in made:
            assert path.stat().st_mode & stat.S_IWGRP, (path, oct(path.stat().st_mode))


def test_two_bootstraps_at_once_generate_once_and_fetch_in_turn():
    with Machine() as m:
        env = {**m.env, **whole_run_env(m)}
        m.world(seconds=1)
        script = str(m.clone / "scripts" / "env" / "bootstrap-vm.sh")
        runs = [subprocess.Popen([script], env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, text=True) for _ in range(2)]
        for run in runs:
            out, err = run.communicate(timeout=120)
            assert run.returncode == 0, (out, err)
        assert len(generator_runs(m)) == 2, generator_runs(m)
        assert m.world()["fetch_overlap"] is False


if __name__ == "__main__":
    raise SystemExit(main(globals()))
