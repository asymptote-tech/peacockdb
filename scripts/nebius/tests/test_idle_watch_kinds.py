"""The watcher on any of the three VMs: its instance and kind come from its install file.

A CPU VM has no card. There, a DuckDB run, a data fetch and typing in an ssh terminal are use.
"""
import json, os, pathlib, sys, tempfile, types
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import idle_watch as w  # noqa: E402
from harness import main  # noqa: E402

H = 3600.0
GPU_ID = "computeinstance-e00tnrse7ayntnzcyt"
CPU_ID = "computeinstance-e01t2mm4265y8x88ey"


class World:
    """idle_watch's I/O patched: subprocess.run, the state, log and install paths, the
    terminals directory, the clock and boot. The last test was two hours ago."""

    def __init__(self, install=None, raw_install=None):
        self.calls = []
        self.ps = " 1 900 bash\n 2 900 sshd\n"
        self.dir = tempfile.mkdtemp()
        self.now = 5_000_000.0
        self.state_path, self.log_path = self.dir + "/state.json", self.dir + "/log"
        self.install_path, self.pts = self.dir + "/install.json", self.dir + "/pts"
        os.mkdir(self.pts)
        json.dump({"last_test": self.now - 2 * H, "last_build": 0.0}, open(self.state_path, "w"))
        if install is not None:
            raw_install = json.dumps(install)
        if raw_install is not None:
            pathlib.Path(self.install_path).write_text(raw_install)

    def terminal(self, name, input_ago):
        """A terminal whose last input was input_ago seconds before now."""
        p = os.path.join(self.pts, name)
        pathlib.Path(p).touch()
        os.utime(p, (self.now - input_ago, self.now - input_ago))

    def run(self, argv, **kw):
        self.calls.append(argv)
        out = lambda s="", rc=0: types.SimpleNamespace(stdout=s, stderr="", returncode=rc)
        if argv[0] == "nvidia-smi":
            return out("")
        if argv[:2] == ["ps", "-eo"]:
            return out(self.ps)
        if argv[0].endswith("nebius"):
            return out("")
        raise AssertionError(argv)

    def stops(self):
        return [a for a in self.calls if a[0].endswith("nebius")]

    def line(self):
        (line,) = [json.loads(l) for l in open(self.log_path)]
        return line

    def go(self):
        saved = (w.subprocess.run, w.STATE, w.LOG, w.INSTALL, w.PTS, w._boot_time, w.time.time)
        w.subprocess.run, w.STATE, w.LOG = self.run, self.state_path, self.log_path
        w.INSTALL, w.PTS = self.install_path, self.pts
        w._boot_time, w.time.time = (lambda: self.now - 10 * H), (lambda: self.now)
        try:
            return w.main()
        finally:
            w.subprocess.run, w.STATE, w.LOG, w.INSTALL, w.PTS, w._boot_time, w.time.time = saved


def cpu():
    return World(install={"instance_id": CPU_ID, "kind": "cpu"})


def test_install_file_names_the_instance_stopped():
    x = cpu()
    assert x.go() == 0
    (stop,) = x.stops()
    assert stop[stop.index("--id") + 1] == CPU_ID, stop
    line = x.line()
    assert line["decision"] == "idle-stop" and line["instance"] == CPU_ID and line["kind"] == "cpu", line


def test_missing_install_file_is_nebius_gpu():
    x = World()
    x.go()
    (stop,) = x.stops()
    assert stop[stop.index("--id") + 1] == GPU_ID, stop
    line = x.line()
    assert line["error"] is None and line["instance"] == GPU_ID and line["kind"] == "gpu", line


def test_broken_install_file_never_stops_a_guessed_vm():
    for raw in (json.dumps({"instance_id": "x"}), "not json",
                json.dumps({"instance_id": CPU_ID, "kind": "tpu"}), "[]"):
        x = World(raw_install=raw)
        assert x.go() == 1, raw
        assert not x.stops(), (raw, x.calls)
        line = x.line()
        assert "install" in line["error"] and "no instance id" in line["error"], (raw, line)
        assert line["instance"] is None and line["decision"] == "idle-stop", (raw, line)


def test_cpu_kind_never_runs_nvidia_smi():
    x = cpu()
    x.go()
    assert not [a for a in x.calls if a[0] == "nvidia-smi"], x.calls
    assert x.line()["card"] == []


def test_cpu_duckdb_is_activity_and_capped():
    x = cpu()
    x.ps = " 1 900 bash\n 7 60 duckdb\n"
    x.go()
    line = x.line()
    assert line["decision"] == "keep" and not x.stops(), line
    assert [p["pid"] for p in line["test_procs"]] == [7]
    x = cpu()
    x.ps = " 1 900 bash\n 7 14411 duckdb\n"
    x.go()
    line = x.line()
    assert line["decision"] == "cap-stop" and "pid 7" in line["reason"], line
    assert len(x.stops()) == 1


def test_a_fused_run_is_a_test_process():
    """`prototype_fused_benchmarks`, which `ps` shows by its 15-character comm."""
    x = World(install={"instance_id": GPU_ID, "kind": "gpu"})
    x.ps = " 1 900 bash\n 7 60 prototype_fused\n"
    x.go()
    line = x.line()
    assert line["decision"] == "keep" and not x.stops(), line
    assert [p["pid"] for p in line["test_procs"]] == [7]
    x = World(install={"instance_id": GPU_ID, "kind": "gpu"})
    x.ps = " 1 900 bash\n 7 14411 prototype_fused\n"
    x.go()
    line = x.line()
    assert line["decision"] == "cap-stop" and "pid 7" in line["reason"], line
    assert len(x.stops()) == 1


def test_cpu_fetch_is_activity():
    x = cpu()
    x.ps = " 1 900 bash\n 9 60 aws\n"
    x.go()
    assert x.line()["decision"] == "keep" and not x.stops()


def test_cpu_session_input_is_activity():
    x = cpu()
    x.terminal("0", 300)
    x.go()
    line = x.line()
    assert line["decision"] == "keep" and not x.stops(), line
    assert line["sessions"] == [x.now - 300], line
    x = cpu()
    x.terminal("0", 70 * 60)
    x.go()
    assert x.line()["decision"] == "idle-stop" and len(x.stops()) == 1


def test_cpu_ptmx_is_not_a_session():
    # /dev/pts holds ptmx beside the numbered terminals; it is no one's login.
    x = cpu()
    x.terminal("ptmx", 10)
    x.go()
    line = x.line()
    assert line["sessions"] == [] and line["decision"] == "idle-stop", line


def test_cpu_build_is_bootstrap_work():
    # The fresh-VM bootstrap runs apt-get, dpkg, docker, git and curl: the two-hour rule.
    def at_90_minutes(comm):
        x = cpu()
        json.dump({"last_test": x.now - 1.5 * H, "last_build": 0.0}, open(x.state_path, "w"))
        x.ps = f" 1 900 bash\n 3 30 {comm}\n"
        x.go()
        return x.line()
    assert at_90_minutes("sshd")["decision"] == "idle-stop"
    for comm in ("apt-get", "dpkg", "docker", "git", "curl", "cargo"):
        line = at_90_minutes(comm)
        assert line["build"] is True and line["decision"] == "keep", (comm, line)


def test_cpu_docker_daemons_are_not_a_build():
    # dockerd and docker-proxy run all day once Docker is installed. Read as a build, they
    # would hold every CPU VM to the two-hour limit.
    x = cpu()
    json.dump({"last_test": x.now - 1.5 * H, "last_build": 0.0}, open(x.state_path, "w"))
    x.ps = " 1 900 bash\n 3 9000 dockerd\n 4 9000 docker-proxy\n 5 9000 containerd\n"
    x.go()
    line = x.line()
    assert line["build"] is False and line["decision"] == "idle-stop", line


def test_gpu_kind_ignores_sessions():
    x = World(install={"instance_id": GPU_ID, "kind": "gpu"})
    x.terminal("0", 60)
    x.go()
    line = x.line()
    assert line["sessions"] == [] and line["decision"] == "idle-stop", line
    assert any(a[0] == "nvidia-smi" for a in x.calls)


def test_session_before_boot_or_ahead_of_now_is_not_activity():
    boot, now = 1_000_000.0, 1_000_000.0 + 10 * H
    state = {"last_test": now - 2 * H, "last_build": 0.0}
    for s in (boot - 60, now + 60):
        _, d, r = w.decide(state, now, boot, [], [], False, sessions=[s])
        assert d == "idle-stop", (s, r)
    _, d, _ = w.decide(state, now, boot, [], [], False, sessions=[now - 60])
    assert d == "keep"


def test_load_install_reads_both_kinds():
    d = tempfile.mkdtemp()
    for kind, iid in (("cpu", CPU_ID), ("gpu", "computeinstance-e00e6yena4nzjgaym0")):
        p = os.path.join(d, kind)
        pathlib.Path(p).write_text(json.dumps({"instance_id": iid, "kind": kind}))
        assert w.load_install(p) == (iid, kind, None)
    assert w.load_install(os.path.join(d, "absent")) == (GPU_ID, "gpu", None)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
