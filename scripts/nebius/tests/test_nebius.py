"""nebius.sh against fake nebius/ssh/ssh-keyscan: every exit code, and the host-key rule."""
import os, pathlib, subprocess, sys, tempfile, time
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from fakes import World  # noqa: E402
from harness import main  # noqa: E402

SCRIPT = str(pathlib.Path(__file__).resolve().parents[2] / "nebius.sh")


def run(world, *args, env=None, input=None, unset=()):
    # stdin is /dev/null unless the test gives one: the fakes read stdin to the end.
    env = {k: v for k, v in {**world.env(), **(env or {})}.items() if k not in unset}
    p = subprocess.run([SCRIPT, *args], env=env, capture_output=True, text=True,
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


def test_soft_stop_refused_while_the_card_lock_is_held():
    # The holder may have nothing on the card yet: a run whose container is still starting, a capture.
    w = World(state="RUNNING", card_lock="held")
    rc, _, err = run(w, "stop")
    assert rc == 2 and w.read()["state"] == "RUNNING", (rc, err)
    assert "card lock /run/lock/peacock-card.lock held" in err, err
    rc, _, err = run(w, "stop", "--force")
    assert rc == 0 and w.read()["state"] == "STOPPED", (rc, err)


def test_soft_stop_stops_on_another_user_s_free_card_lock():
    w = World(state="RUNNING", card_lock="another's")
    rc, _, err = run(w, "stop")
    assert rc == 0 and w.read()["state"] == "STOPPED", (rc, err)


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
    assert "host key replaced" in (w.dir / "state" / "nebius.log").read_text()


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
    assert "host key replaced" not in (w.dir / "state" / "nebius.log").read_text()


def test_ssh_retries_a_refused_connection():
    w = World(state="RUNNING", ssh_refusals=2)
    rc, _, err = run(w, "ssh", "true")
    assert rc == 0, (rc, err)
    assert len([c for c in w.read()["calls"] if c[0] == "ssh"]) == 3, w.read()["calls"]
    assert "attempt 2 of 4 failed before authenticating" in err, err


def test_a_changed_host_key_is_repaired_without_a_retry():
    w = World(state="RUNNING")
    run(w, "ssh", "true")
    w.set(host_key="ssh-ed25519 AAAAKEY2", calls=[])
    rc, _, err = run(w, "ssh", "true")
    assert rc == 0, (rc, err)
    order = [c[0] for c in w.read()["calls"] if c[0] != "nebius"]
    assert order == ["ssh", "ssh-keyscan", "ssh"], order
    assert "attempt" not in err, err


def test_rsh_is_one_attempt_for_the_sync_around_it_to_retry():
    w = World(state="RUNNING", ssh_refusals=1)
    rc, _, err = run(w, "rsh", "nebius-gpu", "true")
    ssh = [c for c in w.read()["calls"] if c[0] == "ssh"]
    assert rc == 255 and len(ssh) == 1, (rc, ssh)
    assert "attempt" not in err, err


def test_a_command_that_prints_a_host_key_refusal_is_never_run_again():
    # The text comes from the remote command, after ssh authenticated: no key is repaired.
    for verb in (["ssh"], ["rsh", "nebius-gpu"]):
        w = World(state="RUNNING")
        run(w, "ssh", "true")
        w.set(remote={"stderr": "Host key verification failed.", "rc": 255}, calls=[])
        rc, _, err = run(w, *verb, "true")
        assert rc == 255 and "Host key verification failed." in err, (verb, rc, err)
        calls = [c[0] for c in w.read()["calls"] if c[0] != "nebius"]
        assert calls == ["ssh"], (verb, calls)
        assert "host key replaced" not in (w.dir / "state" / "nebius.log").read_text()


def test_start_bounds_its_ssh_wait_by_the_wall_clock():
    # Each wait is four attempts of a second; the bound is checked between waits, not counted.
    w = World(state="STOPPED", ssh="down", ssh_seconds=1)
    t0 = time.monotonic()
    rc, _, err = run(w, "start", env={"GPU_HOST_START_TIMEOUT": "3"})
    assert rc == 1, (rc, err)
    assert time.monotonic() - t0 < 15, time.monotonic() - t0
    assert w.read()["state"] == "STOPPED"


def test_an_unknown_verb_prints_the_whole_usage():
    rc, _, err = run(World(), "bogus")
    assert rc == 64 and "nebius.sh rsh <host>" in err and "1 other; 64 usage." in err, (rc, err)
    assert "NEVER" not in err, err


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
    assert "PermissionDenied" in (w.dir / "state" / "nebius.log").read_text()
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
    assert "no ssh" in (w.dir / "state" / "nebius.log").read_text()


def test_start_on_a_running_vm_without_ssh_is_1_and_leaves_it_running():
    # This call did not start the VM, so it is someone else's to stop.
    w = World(state="RUNNING", ssh="down")
    rc, _, err = run(w, "start")
    assert rc == 1, (rc, err)
    assert w.read()["state"] == "RUNNING"
    assert not any(c[:4] == ["nebius", "compute", "instance", "stop"] for c in w.read()["calls"])


def test_stdin_reaches_the_remote_command_not_the_cli():
    # `nebius.sh ssh … < patch` and rsync's -e transport feed the remote command on stdin.
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


def nebius_calls(w):
    return [c for c in w.read()["calls"] if c[0] == "nebius"]


def test_every_cli_call_names_the_config():
    w = World(state="STOPPED")
    config = w.env()["GPU_HOST_NEBIUS_CONFIG"]
    for args in (["status"], ["start"], ["ip"], ["ssh", "true"], ["--profile", "eu-west1", "status"],
                 ["stop", "--force"]):
        rc, _, err = run(w, *args)
        assert rc == 0, (args, rc, err)
    assert nebius_calls(w), w.read()["calls"]
    for c in nebius_calls(w):
        assert c.count("--config") == 1 and c[c.index("--config") + 1] == config, c


def test_a_missing_config_is_refused_naming_nebius_profiles():
    w = World(state="RUNNING")
    absent = str(w.dir / "no-such.yaml")
    for args in (["status"], ["start"], ["stop", "--force"], ["ssh", "true"], ["rsh", "vm", "true"],
                 ["install-watch"]):
        rc, out, err = run(w, *args, env={"GPU_HOST_NEBIUS_CONFIG": absent})
        assert rc == 1, (args, rc, err)
        assert "scripts/env/nebius-profiles.sh" in err and absent in err, (args, err)
        assert w.read()["calls"] == [], (args, w.read()["calls"])
    # A usage error stays 64 whatever is missing: the config, and the CLI.
    rc, _, err = run(w, "bogus", env={"GPU_HOST_NEBIUS_CONFIG": absent, "PATH": str(path_without_nebius(w))},
                     unset=("NEBIUS",))
    assert rc == 64 and "1 other; 64 usage." in err, (rc, err)


def peacock_home(w):
    """A HOME with ~/.config/peacock/nebius.yaml, and ~/.nebius/config.yaml at mode 000."""
    home = w.dir / "home"
    (home / ".config" / "peacock").mkdir(parents=True, exist_ok=True)
    (home / ".config" / "peacock" / "nebius.yaml").write_text("")
    (home / ".nebius").mkdir(exist_ok=True)
    (home / ".nebius" / "config.yaml").write_text("")
    (home / ".nebius" / "config.yaml").chmod(0)
    return home


def test_the_default_config_is_peacock_s():
    w = World(state="RUNNING")
    home = peacock_home(w)
    rc, out, err = run(w, "status", env={"HOME": str(home)}, unset=("GPU_HOST_NEBIUS_CONFIG",))
    assert rc == 0 and out.split() == ["RUNNING", "203.0.113.7"], (rc, out, err)
    want = str(home / ".config" / "peacock" / "nebius.yaml")
    assert nebius_calls(w) and all(c[c.index("--config") + 1] == want for c in nebius_calls(w)), nebius_calls(w)


def identities(w):
    return [c[i + 1] for c in w.read()["calls"] if c[0] == "ssh" for i, a in enumerate(c) if a == "-i"]


def test_the_peacock_gpu_key_is_the_identity_without_gpu_host_key():
    w = World(state="RUNNING")
    rc, _, err = run(w, "ssh", "true", unset=("GPU_HOST_KEY",))
    assert rc == 0 and identities(w) == [], (rc, err, identities(w))
    key = peacock_home(w) / ".config" / "peacock" / "gpu_ed25519"
    key.write_text("")
    w.set(calls=[])
    rc, _, err = run(w, "ssh", "true", unset=("GPU_HOST_KEY",))
    assert rc == 0 and identities(w) == [str(key)], (rc, err, identities(w))


def test_gpu_host_key_wins_over_the_peacock_key():
    w = World(state="RUNNING")
    (peacock_home(w) / ".config" / "peacock" / "gpu_ed25519").write_text("")
    rc, _, err = run(w, "ssh", "true", env={"GPU_HOST_KEY": "/ci/nebius_gpu"})
    assert rc == 0 and identities(w) == ["/ci/nebius_gpu"], (rc, err, identities(w))


def path_without_nebius(w):
    """A PATH directory linking every command on PATH but nebius, as on a machine without one."""
    out = pathlib.Path(tempfile.mkdtemp(prefix="system-bin-", dir=w.dir))
    for d in os.environ["PATH"].split(os.pathsep):
        for entry in os.listdir(d) if os.path.isdir(d) else ():
            if entry != "nebius" and not (out / entry).is_symlink():
                (out / entry).symlink_to(os.path.join(d, entry))
    return out


def cli_at(w, directory, mark):
    """A nebius in <directory> that writes <mark> to w.dir/which and runs the world's fake."""
    directory.mkdir(parents=True, exist_ok=True)
    cli = directory / "nebius"
    cli.write_text(f'#!/bin/sh\necho {mark} >> "{w.dir / "which"}"\nexec "{w.dir / "nebius"}" "$@"\n')
    cli.chmod(0o755)


def lookup(w, path_dir=None):
    system = path_without_nebius(w)
    path = f"{path_dir}:{system}" if path_dir else str(system)
    rc, out, err = run(w, "status", env={"PATH": path}, unset=("NEBIUS",))
    which = (w.dir / "which").read_text().split() if (w.dir / "which").exists() else []
    return rc, err, which


def test_a_cli_on_path_wins_over_the_nebius_bin_one():
    w = World(state="RUNNING")
    cli_at(w, w.dir / "home" / ".nebius" / "bin", "nebius-bin")
    cli_at(w, w.dir / "on-path", "path")
    rc, err, which = lookup(w, w.dir / "on-path")
    assert rc == 0 and which and set(which) == {"path"}, (rc, err, which)


def test_the_nebius_bin_cli_without_one_on_path():
    w = World(state="RUNNING")
    cli_at(w, w.dir / "home" / ".nebius" / "bin", "nebius-bin")
    rc, err, which = lookup(w)
    assert rc == 0 and which and set(which) == {"nebius-bin"}, (rc, err, which)


def test_no_cli_is_1_naming_the_installer():
    w = World(state="RUNNING")
    rc, err, which = lookup(w)
    assert rc == 1, (rc, err)
    assert "install.sh" in err and "scripts/env/bootstrap-vm.sh" in err, err
    assert w.read()["calls"] == [] and which == [], w.read()["calls"]


if __name__ == "__main__":
    raise SystemExit(main(globals()))
