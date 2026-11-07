"""nebius.sh and nebius-sync.sh on a VM other than nebius-gpu: --instance-id, --profile, the
host-key alias per instance, and a soft stop on a VM without a card."""
import pathlib, subprocess, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from fakes import World  # noqa: E402
from harness import main  # noqa: E402

SCRIPTS = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = str(SCRIPTS / "nebius.sh")
SYNC = str(SCRIPTS / "nebius-sync.sh")
H100 = "computeinstance-e00e6yena4nzjgaym0"
CPU = "computeinstance-e01t2mm4265y8x88ey"


def run(world, *args, env=None, input=None, script=SCRIPT):
    # stdin is /dev/null unless the test gives one: the fakes read stdin to the end.
    p = subprocess.run([script, *args], env={**world.env(), **(env or {})}, capture_output=True, text=True,
                       timeout=60, input=input, stdin=None if input is not None else subprocess.DEVNULL)
    return p.returncode, p.stdout, p.stderr


def nebius_calls(w):
    return [c for c in w.read()["calls"] if c[0] == "nebius"]


def known_hosts(w):
    return (w.dir / "state" / "known_hosts").read_text()


def holds_pair(argv, flag, value):
    return any(a == flag and b == value for a, b in zip(argv, argv[1:]))


def test_instance_id_reaches_every_cli_call():
    w = World(state="STOPPED")
    for verb in (["status"], ["start"], ["stop", "--force"]):
        rc, _, err = run(w, "--instance-id", H100, *verb)
        assert rc == 0, (verb, err)
    calls = nebius_calls(w)
    assert any(c[3:4] == ["start"] for c in calls) and any(c[3:4] == ["stop"] for c in calls), calls
    for c in calls:
        assert holds_pair(c, "--id", H100), c


def test_profile_reaches_every_cli_call():
    w = World(state="STOPPED")
    for verb in (["start"], ["stop", "--force"]):
        rc, _, err = run(w, "--profile", "eu-west1", *verb)
        assert rc == 0, (verb, err)
    calls = nebius_calls(w)
    assert calls
    for c in calls:
        assert holds_pair(c, "--profile", "eu-west1"), c
    w = World(state="STOPPED")
    run(w, "start")
    calls = nebius_calls(w)
    assert calls and not any("--profile" in c for c in calls), calls


def test_each_instance_has_its_own_alias():
    w = World(state="RUNNING")
    assert run(w, "ssh", "true")[0] == 0
    assert run(w, "--instance-id", H100, "ssh", "true")[0] == 0
    lines = known_hosts(w).splitlines()
    gpu = [l for l in lines if l.startswith("nebius-gpu ")]
    h100 = [l for l in lines if l.startswith(f"nebius-{H100} ")]
    assert gpu == ["nebius-gpu ssh-ed25519 AAAAKEY1"] and h100 == [f"nebius-{H100} ssh-ed25519 AAAAKEY1"], lines
    ssh = [c for c in w.read()["calls"] if c[0] == "ssh"]
    assert f"HostKeyAlias=nebius-{H100}" in ssh[-1], ssh[-1]

    # The H100 is reprovisioned: its key changes, and only its own line moves.
    w.set(host_key="ssh-ed25519 AAAAKEY2")
    rc, _, err = run(w, "--instance-id", H100, "ssh", "true")
    assert rc == 0, err
    lines = known_hosts(w).splitlines()
    assert [l for l in lines if l.startswith("nebius-gpu ")] == gpu, lines
    assert [l for l in lines if l.startswith(f"nebius-{H100} ")] == [f"nebius-{H100} ssh-ed25519 AAAAKEY2"], lines
    assert len(lines) == 2, lines


def test_explicit_default_id_keeps_the_nebius_gpu_alias():
    w = World(state="RUNNING")
    rc, _, err = run(w, "--instance-id", "computeinstance-e00tnrse7ayntnzcyt", "ssh", "true")
    assert rc == 0, err
    assert known_hosts(w).splitlines() == ["nebius-gpu ssh-ed25519 AAAAKEY1"]
    ssh = [c for c in w.read()["calls"] if c[0] == "ssh"]
    assert "HostKeyAlias=nebius-gpu" in ssh[-1], ssh[-1]


def test_bad_instance_id_or_profile_is_64():
    cases = [
        (["--instance-id", "x;rm", "status"], "--instance-id"),
        (["--instance-id", "Computeinstance-e00e6yena4nzjgaym0", "status"], "--instance-id"),
        (["--instance-id"], "--instance-id"),
        (["--profile", "", "status"], "--profile"),
        (["--profile", "eu west1", "status"], "--profile"),
        (["--instance-id", H100, "--instance-id", H100, "status"], "--instance-id"),
        (["--profile", "eu-west1", "--profile", "eu-west1", "status"], "--profile"),
    ]
    for args, flag in cases:
        w = World(state="RUNNING")
        rc, _, err = run(w, *args)
        assert rc == 64, (args, rc, err)
        assert flag in err, (args, err)
        assert not nebius_calls(w), (args, nebius_calls(w))


def test_soft_stop_refused_while_duckdb_runs_on_a_cpu_vm():
    w = World(state="RUNNING", no_gpu=True, cpu_busy="4242 duckdb\n")
    rc, _, err = run(w, "--instance-id", CPU, "--profile", "eu-west1", "stop")
    assert rc == 2, (rc, err)
    assert w.read()["state"] == "RUNNING"
    assert not any(c[3:4] == ["stop"] for c in nebius_calls(w))
    log = (w.dir / "state" / "nebius.log").read_text()
    assert "busy: 4242 duckdb" in log, log


def test_soft_stop_on_an_idle_cpu_vm_stops():
    w = World(state="RUNNING", no_gpu=True, cpu_busy="")
    rc, _, err = run(w, "--instance-id", CPU, "--profile", "eu-west1", "stop")
    assert rc == 0, (rc, err)
    assert w.read()["state"] == "STOPPED"
    # Stopped because the VM is idle, not because the probe failed and read as unreachable.
    assert "unreachable" not in (w.dir / "state" / "nebius.log").read_text()


def rsync_transport(w):
    rs = [c for c in w.read()["calls"] if c[0] == "rsync"]
    assert len(rs) == 1, rs
    return rs[0][rs[0].index("-e") + 1]


def test_sync_passes_the_instance_through():
    w = World(state="RUNNING")
    rc, out, err = run(w, "--instance-id", H100, "--profile", "editor-eu", "--dest", "d", script=SYNC)
    assert rc == 0, err
    assert out == "d\n", out
    assert rsync_transport(w) == f"{SCRIPT} --instance-id {H100} --profile editor-eu rsh"


def test_sync_without_options_is_todays_transport():
    w = World(state="RUNNING")
    rc, out, err = run(w, "--dest", "d", script=SYNC)
    assert rc == 0 and out == "d\n", (rc, out, err)
    assert rsync_transport(w) == f"{SCRIPT} rsh"


def generated(w):
    return [c for c in w.read()["calls"] if c[0] == "ssh" and "generate_testdata" in " ".join(c)]


def test_sync_no_data_skips_generation():
    w = World(state="RUNNING")
    rc, out, err = run(w, "--instance-id", CPU, "--profile", "eu-west1", "--dest", "d", "--no-data", script=SYNC)
    assert rc == 0, err
    assert out == "d\n", out
    assert not generated(w), generated(w)
    assert rsync_transport(w) == f"{SCRIPT} --instance-id {CPU} --profile eu-west1 rsh"


def test_sync_never_generates_data():
    # sf1 on a VM is run-gpu.sh prepare's or bootstrap-vm.sh's, in the build image. The sync is
    # the tree alone, and --no-data stays accepted so every caller keeps working.
    for args in ([], ["--no-data"]):
        w = World(state="RUNNING")
        rc, out, err = run(w, "--dest", "d", *args, script=SYNC)
        assert rc == 0 and out == "d\n", (args, rc, out, err)
        assert not generated(w), (args, generated(w))
        assert not [c for c in w.read()["calls"] if c[0] == "ssh"], (args, w.read()["calls"])
        assert rsync_transport(w) == f"{SCRIPT} rsh", args


def test_sync_unknown_option_is_64():
    w = World(state="RUNNING")
    rc, _, err = run(w, "--bogus", script=SYNC)
    assert rc == 64 and "--bogus" in err, (rc, err)
    assert not w.read()["calls"]


if __name__ == "__main__":
    sys.exit(main(globals()))
