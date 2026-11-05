"""The GPU host adapters: scripts/env/host/<provider>.sh <name> start|stop|status|ip|powers|ssh <cmd>.

Each test runs an adapter from a built tree (`gpuhost.py`) against a host file in
~/.config/peacock/hosts/; ssh and the provider's CLI are fakes, so nothing leaves the machine.
"""
import os
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from gpuhost import ADDRESS, INSTANCE_ID, NAME, SECRET, USER, GpuRig  # noqa: E402
from machine import REPO  # noqa: E402

PROVIDERS = sorted(p.stem for p in (REPO / "scripts/env/host").glob("*.sh"))


def refused(r, *names):
    assert r.returncode == 2, (r.returncode, r.stdout, r.stderr)
    assert len(r.stderr.strip().splitlines()) == 1, r.stderr
    for name in names:
        assert name in r.stderr, (name, r.stderr)


def test_ssh_adapter_start_is_noop():
    with GpuRig() as rig:
        for verb in ("start", "stop"):
            r = rig.adapter("ssh", NAME, verb)
            assert r.returncode == 0 and r.stderr == "", (verb, r.returncode, r.stderr)
        assert rig.calls("ssh") == [], rig.calls("ssh")
        r = rig.adapter("ssh", NAME, "ip")
        assert r.returncode == 0 and r.stdout == f"{ADDRESS}\n", (r.stdout, r.stderr)
        r = rig.adapter("ssh", NAME, "powers")
        assert r.returncode == 1 and "by hand" in r.stdout, (r.returncode, r.stdout)
        assert rig.calls("ssh") == []


def test_ssh_adapter_status_is_reachable_over_ssh():
    with GpuRig() as rig:
        r = rig.adapter("ssh", NAME, "status")
        assert r.returncode == 0 and "reachable" in r.stdout and "unreachable" not in r.stdout, (r.stdout, r.stderr)
        argv = rig.calls("ssh")[-1]["argv"]
        assert f"{USER}@{ADDRESS}" in argv and str(rig.key) in argv and "BatchMode=yes" in argv, argv
        r = rig.adapter("ssh", NAME, "status", env={"FAKE_SSH_DOWN": ADDRESS})
        assert r.returncode == 1 and "unreachable" in r.stdout, (r.returncode, r.stdout, r.stderr)


def test_ssh_adapter_runs_a_command_on_the_host():
    with GpuRig() as rig:
        r = rig.adapter("ssh", NAME, "ssh", "hostname")
        assert r.returncode == 0 and r.stdout == "gpu-box-7\n", (r.returncode, r.stdout, r.stderr)


def test_nebius_adapter_needs_instance_id():
    with GpuRig() as rig:
        rig.nebius_host(INSTANCE_ID=None)
        for verb in ("status", "start", "stop", "ip", "powers"):
            refused(rig.adapter("nebius", "h100", verb), "INSTANCE_ID", "h100")
        assert rig.calls("nebius") == []


def test_nebius_adapter_powers_a_host_only_with_its_config():
    with GpuRig() as rig:
        rig.nebius_host(config=False)
        for verb in ("status", "start", "stop"):
            refused(rig.adapter("nebius", "h100", verb), "config.yaml", "--provider-config")
        r = rig.adapter("nebius", "h100", "powers")
        assert r.returncode == 1 and "config.yaml" in r.stdout, (r.returncode, r.stdout)
        r = rig.adapter("nebius", "h100", "ip")
        assert r.returncode == 0 and r.stdout == f"{ADDRESS}\n", (r.stdout, r.stderr)
        assert rig.calls("nebius") == []


def test_nebius_adapter_goes_through_its_cli():
    with GpuRig() as rig:
        config = rig.nebius_host()
        outputs = []
        r = rig.adapter("nebius", "h100", "powers")
        assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
        r = rig.adapter("nebius", "h100", "status")
        assert r.returncode == 0 and r.stdout.strip() == "STOPPED", (r.stdout, r.stderr)
        outputs.append(r)
        r = rig.adapter("nebius", "h100", "ip")
        assert r.returncode != 0 and "start" in r.stderr and r.stdout == "", (r.returncode, r.stdout, r.stderr)
        outputs.append(r)
        for verb in ("start", "ip", "status", "ssh", "stop", "status"):
            r = rig.adapter("nebius", "h100", verb, *(["hostname"] if verb == "ssh" else []))
            assert r.returncode == 0, (verb, r.returncode, r.stdout, r.stderr)
            outputs.append(r)
        assert [o.stdout.strip() for o in outputs[2:]] == ["h100 is up", ADDRESS, "RUNNING", "gpu-box-7",
                                                             "h100 is stopped", "STOPPED"], [o.stdout for o in outputs]
        for argv in rig.calls("nebius"):
            assert argv[argv.index("--config") + 1] == str(config), argv
            assert argv[argv.index("--id") + 1] == INSTANCE_ID, argv
        assert rig.calls("ssh")[-1]["argv"][-2:] == [f"{USER}@{ADDRESS}", "hostname"], rig.calls("ssh")[-1]
        for o in outputs:
            assert SECRET not in o.stdout + o.stderr


def test_nebius_start_waits_out_a_stop_and_skips_a_running_host():
    with GpuRig() as rig:
        rig.nebius_host()
        rig.nebius(state="STOPPING")
        r = rig.adapter("nebius", "h100", "start")
        assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
        assert rig.nebius_verbs().count("start") == 1 and rig.nebius()["state"] == "RUNNING", rig.nebius_verbs()
        r = rig.adapter("nebius", "h100", "start")
        assert r.returncode == 0 and rig.nebius_verbs().count("start") == 1, rig.nebius_verbs()
        rig.nebius(state="STARTING")
        r = rig.adapter("nebius", "h100", "start")
        assert r.returncode == 0 and rig.nebius_verbs().count("start") == 1, rig.nebius_verbs()
        r = rig.adapter("nebius", "h100", "stop")
        assert r.returncode == 0 and rig.nebius()["state"] == "STOPPED", r
        r = rig.adapter("nebius", "h100", "stop")
        assert r.returncode == 0 and rig.nebius_verbs().count("stop") == 1, rig.nebius_verbs()


def test_nebius_start_without_ssh_stops_only_what_it_started():
    with GpuRig() as rig:
        rig.nebius_host()
        down = {"FAKE_SSH_DOWN": ADDRESS, "PEACOCK_HOST_WAIT": "2"}
        r = rig.adapter("nebius", "h100", "start", env=down)
        assert r.returncode == 1 and "ssh" in r.stderr, (r.returncode, r.stderr)
        assert rig.nebius_verbs().count("stop") == 1 and rig.nebius()["state"] == "STOPPED", rig.nebius_verbs()
        rig.nebius(state="RUNNING")
        r = rig.adapter("nebius", "h100", "start", env=down)
        assert r.returncode == 1, r
        assert rig.nebius_verbs().count("stop") == 1 and rig.nebius()["state"] == "RUNNING", rig.nebius_verbs()


def test_nebius_stops_a_host_that_never_comes_up_and_one_still_starting():
    with GpuRig() as rig:
        rig.nebius_host()
        rig.nebius(stuck=True)
        r = rig.adapter("nebius", "h100", "start", env={"PEACOCK_HOST_WAIT": "2"})
        assert r.returncode == 1 and "STARTING" in r.stderr, (r.returncode, r.stderr)
        assert rig.nebius_verbs().count("stop") == 1 and rig.nebius()["state"] == "STOPPED", rig.nebius_verbs()
        rig.nebius(state="STARTING", stuck=True)
        r = rig.adapter("nebius", "h100", "stop", env={"PEACOCK_HOST_WAIT": "2"})
        assert r.returncode == 0 and rig.nebius()["state"] == "STOPPED", (r.returncode, r.stderr)
        assert rig.nebius_verbs().count("stop") == 2, rig.nebius_verbs()


def test_a_host_file_holds_only_its_keys():
    with GpuRig() as rig:
        rig.write_host_file("old", PROVIDER="ssh", HOST=ADDRESS, USER=USER, PASSWORD_FIRST_LOGIN="1")
        refused(rig.adapter("ssh", "old", "ip"), "PASSWORD_FIRST_LOGIN")
        rig.write_host_file("bare", PROVIDER="ssh", USER=USER)
        refused(rig.adapter("ssh", "bare", "ip"), "HOST")
        refused(rig.adapter("ssh", "missing", "ip"), "missing", "provision-vm.sh --gpu-host")
        rig.write_host_file("theirs", PROVIDER="nebius", HOST=ADDRESS, USER=USER, INSTANCE_ID=INSTANCE_ID)
        refused(rig.adapter("ssh", "theirs", "ip"), "nebius")
        assert rig.calls("ssh") == []


def test_adapters_refuse_bad_arguments_and_have_help():
    with GpuRig() as rig:
        for provider in PROVIDERS:
            r = rig.adapter(provider, "--help")
            assert r.returncode == 0 and f"{provider}.sh <name>" in r.stdout, (provider, r.stdout, r.stderr)
            assert not any(ln.startswith("#") for ln in r.stdout.splitlines()), r.stdout
            refused(rig.adapter(provider), "name")
            refused(rig.adapter(provider, NAME, "reboot"), "reboot")
            refused(rig.adapter(provider, "../etc", "ip"), "../etc")
            refused(rig.adapter(provider, NAME, "ssh"), "ssh")
        assert rig.adapter("ssh", "--needs").stdout == "" and rig.adapter("ssh", "--config-name").stdout == ""
        assert rig.adapter("nebius", "--needs").stdout == "INSTANCE_ID\n"
        assert rig.adapter("nebius", "--config-name").stdout == "config.yaml\n"


def test_no_script_but_an_adapter_names_a_provider_an_address_or_a_home():
    scripts = ["scripts/env/gpu.sh", "scripts/env/on-gpu-host.sh", "scripts/lib/host.sh",
               "scripts/env/provision-vm.sh", "scripts/env/up.sh", "scripts/lib/peacock-config.sh"]
    assert PROVIDERS and "ssh" in PROVIDERS, PROVIDERS
    named = [p for p in PROVIDERS if p != "ssh"]
    for rel in scripts:
        text = (REPO / rel).read_text()
        for provider in named:
            assert not re.search(provider, text, re.I), (rel, provider)
        assert not re.search(r"/home/[a-z]", text), rel
        assert not re.search(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b", text), rel
    for provider in named:
        assert provider in (REPO / f"scripts/env/host/{provider}.sh").read_text()


def test_the_config_stays_unread():
    if os.geteuid() == 0:
        print("SKIP: root reads a file of mode 0, so the check proves nothing")
        return
    with GpuRig() as rig:
        config = rig.nebius_host()
        for verb in ("powers", "status", "start", "stop"):
            r = rig.adapter("nebius", "h100", verb)
            assert r.returncode == 0 and SECRET not in r.stdout + r.stderr, (verb, r)
        assert config.stat().st_mode & 0o777 == 0, oct(config.stat().st_mode)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
