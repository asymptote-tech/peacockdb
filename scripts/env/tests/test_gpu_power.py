"""gpu.sh on a host its adapter powers: started for every command and soft-stopped when it ends.

Each test runs gpu.sh against a nebius host whose CLI is a strict fake (`gpuhost.py`): the soft
stop declines while the card or the host is in use, and a host powered by hand is left alone.
"""
import fcntl
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from gpuhost import (ADDRESS, CAP, NAME, GpuRig, device_runs, ok, prepared, refused, shipped,  # noqa: E402
                     until)

HOST = "h100"
B = {"PEACOCK_INSTANCE": "dev2-B"}


def starts_and_stops(rig):
    verbs = rig.nebius_verbs()
    return verbs.count("start"), verbs.count("stop")


def test_a_powered_host_is_started_and_soft_stopped_by_every_command():
    with GpuRig() as rig:
        rig.nebius_host()
        for n, verb in enumerate((("prepare",), ("ship",), ("run", "list"), ("pull",)), start=1):
            ok(rig.gpu(HOST, *verb))
            assert starts_and_stops(rig) == (n, n) and rig.nebius()["state"] == "STOPPED", (verb, rig.nebius_verbs())
        assert all("--async" in argv for argv in rig.calls("nebius") if "start" in argv or "stop" in argv)
        assert len(device_runs(rig)) == 1


def test_a_powered_host_is_stopped_on_every_exit():
    with GpuRig() as rig:
        rig.nebius_host()
        shipped(rig, host=HOST)
        sf40 = rig.data_dir() / "sf40" / "tpcds.sf40"
        for f in sf40.iterdir():
            f.unlink()
        sf40.rmdir()
        refused(rig.gpu(HOST, "run", "bench"), "tpcds.sf40")
        assert rig.nebius()["state"] == "STOPPED", rig.nebius_verbs()
        (rig.host / "driver").write_text(f"530.30.02, {CAP}\n")
        refused(rig.gpu(HOST, "run", "list"), "530.30.02", code=3)
        assert rig.nebius()["state"] == "STOPPED", rig.nebius_verbs()
        refused(rig.gpu(HOST, "prepare"), "530.30.02", code=3)
        assert rig.nebius()["state"] == "STOPPED" and starts_and_stops(rig) == (5, 5), rig.nebius_verbs()


def test_a_green_run_whose_soft_stop_fails_keeps_its_status():
    with GpuRig() as rig:
        rig.nebius_host()
        shipped(rig, host=HOST)
        rig.nebius(fail={"verb": "stop", "exit": 3})
        r = rig.gpu(HOST, "run", "list")
        assert r.returncode == 0 and "green" in r.stdout, (r.returncode, r.stderr)
        assert "stopping h100 failed" in r.stderr and rig.nebius()["state"] == "RUNNING", r.stderr


def test_a_host_without_power_is_left_as_it_is():
    with GpuRig() as rig:
        down = {"FAKE_SSH_DOWN": ADDRESS}
        refused(rig.gpu(NAME, "prepare", env=down), "by hand", NAME, code=1)
        refused(rig.gpu(NAME, "start"), "by hand", "ssh.sh")
        refused(rig.gpu(NAME, "stop"), "by hand", "ssh.sh")
        rig.nebius_host("bare-h100", config=False)
        refused(rig.gpu("bare-h100", "prepare", env=down), "by hand", "config.yaml", code=1)
        assert rig.calls("nebius") == []


def test_soft_stop_declines_while_the_card_is_busy():
    with GpuRig() as rig:
        rig.nebius_host()
        prepared(rig, host=HOST)
        rig.nebius(state="RUNNING")
        data = rig.data_dir()
        with open(data / ".device.lock", "a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            refused(rig.gpu(HOST, "stop"), "device run")
        (rig.host / "compute_apps").write_text("4242\n")
        refused(rig.gpu(HOST, "stop"), "process on the card")
        (rig.host / "compute_apps").unlink()
        with open(data / ".activity.lock", "a") as lock:
            fcntl.flock(lock, fcntl.LOCK_SH)
            refused(rig.gpu(HOST, "stop"), "gpu.sh step")
        assert rig.nebius()["state"] == "RUNNING", rig.nebius_verbs()
        ok(rig.gpu(HOST, "stop"))
        assert rig.nebius()["state"] == "STOPPED"
        rig.nebius(state="RUNNING")
        (rig.host / "compute_apps").write_text("4242\n")
        ok(rig.gpu(HOST, "stop", "--force"))
        assert rig.nebius()["state"] == "STOPPED"


def test_soft_stop_waits_for_another_instance_s_work():
    with GpuRig() as rig:
        rig.nebius_host()
        rig.nebius(state="RUNNING")
        (rig.host / "hold_gen").write_text("")
        b = rig.gpu(HOST, "prepare", env=B, background=True)
        until(lambda: any(c[0] == "run" for c in rig.host_calls(rig.host, "docker")), "B generates sf1")
        refused(rig.gpu(HOST, "stop"), "gpu.sh step")
        assert rig.nebius()["state"] == "RUNNING"
        (rig.host / "hold_gen").unlink()
        assert b.wait(timeout=60) == 0, b.communicate()
        assert rig.nebius()["state"] == "STOPPED", "B's own soft stop, once its work is done"

        ok(rig.gpu(HOST, "ship", env=B))
        rig.nebius(state="RUNNING")
        (rig.host / "hold_smi").write_text("")
        before = len(rig.host_calls(rig.host, "nvidia-smi"))
        b = rig.gpu(HOST, "run", "list", env=B, background=True)
        until(lambda: (rig.host / "nvidia-smi.log").exists() and
              len(rig.host_calls(rig.host, "nvidia-smi")) > before, "B checks the driver")
        refused(rig.gpu(HOST, "stop"), "gpu.sh step")
        (rig.host / "hold_smi").unlink()
        assert b.wait(timeout=60) == 0, b.communicate()
        assert len(device_runs(rig)) == 1 and rig.nebius()["state"] == "STOPPED"


def test_the_laptop_stops_a_host_with_its_own_gpu_key():
    with GpuRig() as rig:
        rig.nebius_host()
        rig.nebius(state="RUNNING")
        key = rig.home / ".config" / "peacock" / "gpu_ed25519"
        key.write_bytes(rig.key.read_bytes())
        key.chmod(0o600)
        key.with_suffix(".pub").write_bytes(rig.key.with_suffix(".pub").read_bytes())
        laptop = {"GPU_SSH_KEY_FILE": None, "PEACOCK_INSTANCE": None}
        ok(rig.gpu(HOST, "stop", env=laptop))
        assert rig.nebius()["state"] == "STOPPED", rig.nebius_verbs()
        assert rig.calls("ssh") and all(str(key) in c["argv"] for c in rig.calls("ssh")), rig.calls("ssh")


def test_stop_checks_the_card_unless_the_host_is_stopped():
    with GpuRig() as rig:
        rig.nebius_host()
        rig.nebius(state="RUNNING", ip="")
        refused(rig.gpu(HOST, "stop"), "RUNNING", "--force")
        assert "stop" not in rig.nebius_verbs()
        rig.nebius(state="STOPPED", ip=ADDRESS)
        ok(rig.gpu(HOST, "stop"))
        assert "stop" not in rig.nebius_verbs(), "a stopped host is not stopped again"


def test_a_stop_during_another_instance_s_prepare_is_refused():
    with GpuRig() as rig:
        rig.nebius_host()
        prepared(rig, host=HOST)
        rig.nebius(state="RUNNING")
        (rig.host / "hold_smi").write_text("")
        before = len(rig.host_calls(rig.host, "nvidia-smi"))
        b = rig.gpu(HOST, "prepare", env=B, background=True)
        until(lambda: len(rig.host_calls(rig.host, "nvidia-smi")) > before, "B checks the driver, before its install")
        refused(rig.gpu(HOST, "stop"), "gpu.sh step")
        assert rig.nebius()["state"] == "RUNNING"
        (rig.host / "hold_smi").unlink()
        assert b.wait(timeout=60) == 0, b.communicate()


def test_a_provider_cli_s_exit_3_is_exit_1():
    with GpuRig() as rig:
        rig.nebius_host()
        rig.nebius(fail={"verb": "start", "exit": 3})
        r = rig.gpu(HOST, "prepare")
        assert r.returncode == 1, (r.returncode, r.stderr)
        assert "nebius.sh h100 start failed" in r.stderr.splitlines()[-1], r.stderr


def test_an_adapter_that_cannot_answer_powers_is_not_read_as_by_hand():
    with GpuRig() as rig:
        rig.nebius_host(INSTANCE_ID=None)
        r = rig.gpu(HOST, "prepare")
        refused(r, "INSTANCE_ID")
        assert "by hand" not in r.stderr and rig.calls("nebius") == [], r.stderr


def test_a_host_with_a_config_and_no_cli_is_refused_not_read_as_by_hand():
    with GpuRig() as rig:
        rig.nebius_host()
        (rig.bin / "nebius").unlink()
        r = rig.gpu(HOST, "prepare")
        refused(r, "nebius CLI")
        assert "by hand" not in r.stderr and rig.calls("ssh") == [], r.stderr


def test_a_soft_stop_without_ssh_on_a_running_host_declines():
    with GpuRig() as rig:
        rig.nebius_host()
        prepared(rig, host=HOST)
        rig.nebius(state="RUNNING")
        refused(rig.gpu(HOST, "stop", env={"FAKE_SSH_DOWN": ADDRESS}), "RUNNING", "stop --force")
        assert rig.nebius()["state"] == "RUNNING" and rig.nebius_verbs().count("stop") == 1, rig.nebius_verbs()
        ok(rig.gpu(HOST, "stop", "--force", env={"FAKE_SSH_DOWN": ADDRESS}))
        assert rig.nebius()["state"] == "STOPPED"


def test_a_soft_stop_without_ssh_or_a_state_declines():
    with GpuRig() as rig:
        rig.nebius_host()
        prepared(rig, host=HOST)
        rig.nebius(state="RUNNING", fail={"verb": "get", "after": 1, "exit": 1})  # the ip answers, the status not
        refused(rig.gpu(HOST, "stop", env={"FAKE_SSH_DOWN": ADDRESS}), "unknown", "stop --force")
        assert rig.nebius()["state"] == "RUNNING" and rig.nebius_verbs().count("stop") == 1, rig.nebius_verbs()


def test_an_explicit_stop_without_an_address_or_a_state_declines():
    with GpuRig() as rig:
        rig.nebius_host()
        rig.nebius(state="RUNNING", fail={"verb": "get", "exit": 1})  # neither the ip nor the status answers
        refused(rig.gpu(HOST, "stop"), "unknown", "--force")
        assert "stop" not in rig.nebius_verbs() and rig.nebius()["state"] == "RUNNING", rig.nebius_verbs()


if __name__ == "__main__":
    raise SystemExit(main(globals()))
