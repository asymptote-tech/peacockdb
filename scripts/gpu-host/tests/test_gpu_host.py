"""gpu-host.sh against fake nebius/ssh/ssh-keyscan: every exit code, and the host-key rule."""
import pathlib, subprocess, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from fakes import World  # noqa: E402
from harness import main  # noqa: E402

SCRIPT = str(pathlib.Path(__file__).resolve().parents[2] / "gpu-host.sh")


def run(world, *args, env=None, input=None):
    # stdin is /dev/null unless the test gives one: the fakes read stdin to the end.
    p = subprocess.run([SCRIPT, *args], env={**world.env(), **(env or {})}, capture_output=True, text=True,
                       timeout=60, input=input, stdin=None if input is not None else subprocess.DEVNULL)
    return p.returncode, p.stdout, p.stderr


def test_status_stopped_prints_no_ip():
    w = World(state="STOPPED")
    rc, out, _ = run(w, "status")
    assert rc == 0 and out.strip() == "STOPPED", out


def test_status_running_prints_ip():
    w = World(state="RUNNING")
    rc, out, _ = run(w, "status")
    assert rc == 0 and out.split() == ["RUNNING", "203.0.113.7"], out


def test_start_from_stopped_reaches_ssh():
    w = World(state="STOPPED")
    rc, _, err = run(w, "start")
    assert rc == 0, err
    assert any(c[:3] == ["nebius", "compute", "instance"] and "start" in c for c in w.read()["calls"])
    assert any(c[0] == "ssh" for c in w.read()["calls"])


def test_start_when_running_does_not_call_start():
    w = World(state="RUNNING")
    rc, _, err = run(w, "start")
    assert rc == 0, err
    assert not any(c[:4] == ["nebius", "compute", "instance", "start"] for c in w.read()["calls"])


def test_start_capacity_error_is_75():
    w = World(state="STOPPED", start_error="rpc error: code = ResourceExhausted desc = NotEnoughResources")
    rc, _, _ = run(w, "start")
    assert rc == 75


def test_start_never_running_is_75():
    w = World(state="STOPPED", states_after_start=["STARTING"] * 50)
    rc, _, _ = run(w, "start")
    assert rc == 75


def test_missing_instance_is_69():
    w = World(exists=False)
    for verb in ("status", "start", "stop"):
        rc, _, _ = run(w, verb)
        assert rc == 69, verb


def test_soft_stop_refused_while_busy():
    w = World(state="RUNNING", card="4242, test_gpu_corpus\n")
    rc, _, _ = run(w, "stop")
    assert rc == 2 and w.read()["state"] == "RUNNING"


def test_force_stop_while_busy():
    w = World(state="RUNNING", card="4242, test_gpu_corpus\n")
    rc, _, _ = run(w, "stop", "--force")
    assert rc == 0 and w.read()["state"] == "STOPPED"


def test_soft_stop_unreachable_host_stops():
    w = World(state="RUNNING", ssh="down")
    rc, _, _ = run(w, "stop")
    assert rc == 0 and w.read()["state"] == "STOPPED"


def test_stop_noop_when_stopped():
    w = World(state="STOPPED")
    rc, _, _ = run(w, "stop")
    assert rc == 0
    assert not any(c[:4] == ["nebius", "compute", "instance", "stop"] for c in w.read()["calls"])


def test_stop_waits_out_starting():
    w = World(state="STARTING", states_after_start=["STARTING", "RUNNING"])
    rc, _, _ = run(w, "stop", "--force")
    assert rc == 0 and w.read()["state"] == "STOPPED"


def test_first_contact_stores_key():
    w = World(state="RUNNING")
    rc, _, err = run(w, "ssh", "true")
    assert rc == 0, err
    kh = (w.dir / "state" / "known_hosts").read_text()
    assert "nebius-gpu ssh-ed25519 AAAAKEY1" in kh


def test_changed_key_replaced_and_logged():
    w = World(state="RUNNING")
    run(w, "ssh", "true")
    w.set(host_key="ssh-ed25519 AAAAKEY2")
    rc, _, err = run(w, "ssh", "true")
    assert rc == 0, err
    kh = (w.dir / "state" / "known_hosts").read_text()
    assert "AAAAKEY2" in kh and "AAAAKEY1" not in kh
    assert "host key replaced" in (w.dir / "state" / "gpu-host.log").read_text()


def test_changed_key_refused_when_the_ip_moved_between_calls():
    # The key changed AND the API now gives another IP: the old IP is no longer vouched for,
    # so no key is replaced and the caller is told to retry.
    w = World(state="RUNNING")
    run(w, "ssh", "true")
    w.set(host_key="ssh-ed25519 AAAAKEY2", ip_sequence=["203.0.113.7", "198.51.100.9"], calls=[])
    rc, _, err = run(w, "ssh", "true")
    assert rc == 1 and "IP moved" in err, err
    kh = (w.dir / "state" / "known_hosts").read_text()
    assert "AAAAKEY1" in kh and "AAAAKEY2" not in kh, kh
    assert not any(c[0] == "ssh-keyscan" for c in w.read()["calls"])
    assert "host key replaced" not in (w.dir / "state" / "gpu-host.log").read_text()


def test_ssh_refused_when_api_does_not_vouch():
    w = World(state="STOPPED")
    rc, _, err = run(w, "ssh", "true")
    assert rc == 1 and "not RUNNING" in err
    assert not any(c[0] == "ssh-keyscan" for c in w.read()["calls"])


def test_ip_is_resolved_per_call():
    w = World(state="RUNNING")
    run(w, "ssh", "true")
    w.set(ip="198.51.100.9")
    run(w, "ssh", "true")
    targets = [c[-2] if len(c) > 1 else "" for c in w.read()["calls"] if c[0] == "ssh"]
    assert any("198.51.100.9" in " ".join(c) for c in w.read()["calls"] if c[0] == "ssh"), targets


def test_absent_cli_is_1_not_69():
    w = World(state="RUNNING")
    for verb in ("status", "start", "stop"):
        rc, _, _ = run(w, verb, env={"NEBIUS": "nebius-absent-cli"})
        assert rc == 1, verb


def test_unrelated_not_found_text_is_1_not_69():
    w = World(state="RUNNING", get_error="Error: profile 'default' not found in config")
    rc, _, _ = run(w, "status")
    assert rc == 1


def test_failed_stop_is_logged_and_exits_without_polling():
    w = World(state="RUNNING", stop_error="Error: rpc error: code = PermissionDenied desc = nope")
    rc, _, _ = run(w, "stop", "--force")
    assert rc == 1
    assert "PermissionDenied" in (w.dir / "state" / "gpu-host.log").read_text()
    gets_after = [c for c in w.read()["calls"] if c[:4] == ["nebius", "compute", "instance", "get"]]
    assert len(gets_after) <= 2, "polled a failed stop"


def test_start_and_stop_pass_async():
    w = World(state="STOPPED")
    run(w, "start")
    run(w, "stop", "--force")
    calls = w.read()["calls"]
    for verb in ("start", "stop"):
        assert any(c[:3] == ["nebius", "compute", "instance"] and verb in c[3:4] and "--async" in c
                   for c in calls), verb


def test_hung_start_cli_is_75():
    w = World(state="STOPPED", hang=["start"])
    rc, _, _ = run(w, "start", env={"GPU_HOST_CLI_TIMEOUT": "1"})
    assert rc == 75


def test_hung_get_cli_is_1():
    w = World(state="RUNNING", hang=["get"])
    rc, _, _ = run(w, "status", env={"GPU_HOST_CLI_TIMEOUT": "1"})
    assert rc == 1


def test_start_while_stopping_waits_for_stopped():
    w = World(state="STOPPING", states_after_start=["STOPPING", "STOPPED"])
    rc, _, err = run(w, "start")
    assert rc == 0, err
    assert any(c[:4] == ["nebius", "compute", "instance", "start"] for c in w.read()["calls"])


def test_start_up_but_no_ssh_is_1_and_stops_what_it_started():
    # RUNNING but ssh never answers is a broken host, not a capacity problem: exit 1, never 75
    # (75 would have the coordinator retry for 10 h against a paid VM), and the VM this call
    # started is stopped again.
    w = World(state="STOPPED", ssh="down")
    rc, _, err = run(w, "start")
    assert rc == 1, (rc, err)
    assert w.read()["state"] == "STOPPED"
    assert any(c[:4] == ["nebius", "compute", "instance", "stop"] for c in w.read()["calls"])
    assert "no ssh" in (w.dir / "state" / "gpu-host.log").read_text()


def test_start_on_a_running_vm_without_ssh_is_1_and_leaves_it_running():
    # This call did not start the VM, so it is someone else's to stop.
    w = World(state="RUNNING", ssh="down")
    rc, _, err = run(w, "start")
    assert rc == 1, (rc, err)
    assert w.read()["state"] == "RUNNING"
    assert not any(c[:4] == ["nebius", "compute", "instance", "stop"] for c in w.read()["calls"])


def test_stdin_reaches_the_remote_command_not_the_cli():
    # `gpu-host.sh ssh … < patch` and rsync's -e transport feed the remote command on stdin.
    # The nebius CLI and ssh-keyscan run first; they must not eat it.
    w = World(state="RUNNING")
    rc, out, err = run(w, "ssh", "cat", input="patch bytes\n")
    assert rc == 0 and out == "patch bytes\n", (rc, out, err)
    rc, out, err = run(World(state="RUNNING"), "rsh", "nebius-gpu", "cat", input="rsync stream\n")
    assert rc == 0 and out == "rsync stream\n", (rc, out, err)


def install_watch(w):
    home = w.dir / "home"  # never the real one: install-watch names ~/.nebius/config.yaml
    home.mkdir(exist_ok=True)
    return run(w, "install-watch", env={"HOME": str(home)})


def test_install_watch_verifies_the_root_profile():
    w = World(state="RUNNING")
    rc, out, err = install_watch(w)
    assert rc == 0, (rc, err)
    checks = [" ".join(c) for c in w.read()["calls"] if c[0] == "ssh" and "HOME=/root" in " ".join(c)]
    assert len(checks) == 1, checks
    assert "sudo env HOME=/root" in checks[0] and "/usr/local/bin/nebius compute instance get --id" in checks[0]
    assert ">/dev/null" in checks[0]


def test_install_watch_fails_loudly_when_the_root_profile_is_broken():
    w = World(state="RUNNING", root_profile="broken")
    rc, out, err = install_watch(w)
    assert rc == 1, (rc, err)
    assert "root" in err and "profile" in err, err


if __name__ == "__main__":
    raise SystemExit(main(globals()))
