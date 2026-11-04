"""The watcher's decision, as a function of a process snapshot and a clock."""
import json, pathlib, subprocess, sys, tempfile, types
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import idle_watch as w  # noqa: E402
from harness import main  # noqa: E402

BOOT = 1_000_000.0
H = 3600.0


def tick(state, at, card=(), tests=(), build=False):
    return w.decide(state, BOOT + at, BOOT, list(card), list(tests), build)


def proc(pid, at, name="peacock_cpu_tests"):
    """A name-matched test process (from `ps`, not on the card) started at BOOT + at."""
    return {"pid": pid, "name": name, "start": BOOT + at}


def test_fresh_boot_keeps():
    _, d, _ = tick({}, 60)
    assert d == "keep"


def test_fresh_boot_over_old_state_keeps():
    old = {"last_test": BOOT - 7 * 24 * H, "last_build": BOOT - 7 * 24 * H, "boot": BOOT - 8 * 24 * H}
    _, d, _ = tick(old, 60)
    assert d == "keep"


def test_idle_hour_stops():
    s, _, _ = tick({}, 0)
    _, d, r = tick(s, H + 1)
    assert d == "idle-stop", r


def test_test_activity_resets():
    s, _, _ = tick({}, 0)
    s, _, _ = tick(s, 0.9 * H, tests=[proc(5, 0.8 * H)])
    _, d, _ = tick(s, 1.5 * H)
    assert d == "keep"


def test_build_extends_to_two_hours():
    s, _, _ = tick({}, 0)
    s, _, _ = tick(s, 0.5 * H, build=True)
    _, d, _ = tick(s, 1.2 * H)
    assert d == "keep"
    _, d, _ = tick(s, 2 * H + 1)
    assert d == "idle-stop"


def test_build_since_last_test_holds_two_hours_from_the_test():
    # The limit is 2 h from the last test, not 1 h from the last build.
    s, _, _ = tick({}, 0)
    s, _, _ = tick(s, 0.5 * H, build=True)
    s, d, _ = tick(s, 1.5 * H + 1)
    assert d == "keep", "build at 0.5 H, no test since 0: 1.5 H is inside the 2 h limit"
    s, d, _ = tick(s, 2 * H - 1)
    assert d == "keep"
    _, d, _ = tick(s, 2 * H + 1)
    assert d == "idle-stop"


def test_old_build_does_not_extend():
    s, _, _ = tick({}, 0)
    s, _, _ = tick(s, 0.1 * H, build=True)
    s, _, _ = tick(s, 1.5 * H, tests=[proc(5, 1.4 * H)])
    _, d, _ = tick(s, 2.6 * H)
    assert d == "idle-stop"


def test_card_process_is_test_activity():
    s, _, _ = tick({}, 0)
    card = [{"pid": 7, "name": "test_gpu_corpus", "start": BOOT + 0.5 * H}]
    s, _, _ = tick(s, 1.5 * H, card=card)
    _, d, _ = tick(s, 2.0 * H, card=card)
    assert d == "keep"


def test_four_hour_cap():
    card = [{"pid": 7, "name": "test_gpu_corpus", "start": BOOT + 10}]
    s, _, _ = tick({}, 0, card=card)
    _, d, r = tick(s, 4 * H + 11, card=card)
    assert d == "cap-stop" and "7" in r


def test_name_matched_test_process_is_capped_too():
    # A hung CPU-side test (an orphan of a cancelled CI job) is never on the card, yet it is
    # test activity every tick. Without the cap on it the VM would stay up forever.
    t = [proc(8, 10)]
    s, d, _ = tick({}, 0, tests=t)
    s, d, _ = tick(s, 4 * H - 60, tests=t)
    assert d == "keep"
    _, d, r = tick(s, 4 * H + 11, tests=t)
    assert d == "cap-stop" and "pid 8" in r and "peacock_cpu_tests" in r, r


def test_log_line_has_every_field():
    s, d, r = tick({}, 60)
    line = w.log_line(BOOT + 60, BOOT, [], [proc(8, 10)], False, s, d, r)
    for k in ("t", "boot", "card", "tests", "test_procs", "build", "last_test", "last_build", "decision", "reason"):
        assert k in line
    assert line["tests"] is True and line["test_procs"] == [proc(8, 10)]


# `ps -eo comm=` cuts names at 15 characters, so a long listed name only ever shows as its
# first 15.
def test_matches_truncated_gpu_lib():
    assert w.matches(["peacockdb_core_"], w.TEST_NAMES)


def test_matches_truncated_node_timing():
    assert w.matches(["test_node_timin"], w.TEST_NAMES)


def test_matches_full_short_names():
    assert w.matches(["cc1plus"], w.BUILD_NAMES)
    assert w.matches(["test_gpu_corpus"], w.TEST_NAMES)
    assert w.matches(["peacock_exec"], w.TEST_NAMES)


def test_parse_ps_takes_pid_etimes_and_comm():
    procs, problems = w.parse_ps("  1 900 systemd\n 4242 14411 peacock_cpu_tes\n 77 3 kworker/0:1 x\nbad line\n")
    assert procs == [(1, 900.0, "systemd"), (4242, 14411.0, "peacock_cpu_tes"), (77, 3.0, "kworker/0:1 x")]
    assert len(problems) == 1 and "bad line" in problems[0]


def test_matches_rejects_other_processes():
    assert not w.matches(["bash", "sshd", "python3"], w.TEST_NAMES)
    assert not w.matches(["bash", "sshd", "python3"], w.BUILD_NAMES)
    assert not w.matches([], w.TEST_NAMES)


# --- failure handling: a broken probe must neither hide the tick nor block the stop ---
def test_parse_card_normal_and_empty():
    assert w.parse_card("") == ([], [])
    assert w.parse_card("\n  \n") == ([], [])
    apps, problems = w.parse_card("1234, /usr/bin/test_gpu_corpus\n77, python3\n")
    assert apps == [(1234, "/usr/bin/test_gpu_corpus"), (77, "python3")] and problems == []


def test_parse_card_nvml_error_is_a_problem_not_a_crash():
    apps, problems = w.parse_card("Failed to initialize NVML: Driver/library version mismatch\n")
    assert apps == [] and len(problems) == 1 and "NVML" in problems[0]
    apps, problems = w.parse_card("No devices were found\n")
    assert apps == [] and problems
    apps, problems = w.parse_card("abc, name\n5, ok\n")
    assert apps == [(5, "ok")] and len(problems) == 1


class World:
    """Patches idle_watch's I/O: subprocess.run, state/log paths, clock, boot."""

    def __init__(self, smi=None, smi_exc=None, ps_exc=None, stop_exc=None, stop_rc=0, state=None):
        self.smi, self.smi_exc, self.ps_exc, self.stop_exc, self.stop_rc = smi or "", smi_exc, ps_exc, stop_exc, stop_rc
        self.calls = []
        self.etimes = 42
        self.ps = " 1 900 bash\n 2 900 sshd\n"
        self.dir = tempfile.mkdtemp()
        self.now = 5_000_000.0
        self.state_path, self.log_path = self.dir + "/state.json", self.dir + "/log"
        init = {"last_test": self.now - 2 * H, "last_build": 0.0} if state is None else state
        json.dump(init, open(self.state_path, "w"))

    def run(self, argv, **kw):
        self.calls.append((argv, kw))
        out = lambda s="", rc=0: types.SimpleNamespace(stdout=s, stderr="", returncode=rc)
        if argv[0] == "nvidia-smi":
            if self.smi_exc:
                raise self.smi_exc
            return out(self.smi)
        if argv[0] == "ps" and argv[1] == "-eo":
            if self.ps_exc:
                raise self.ps_exc
            assert argv == ["ps", "-eo", "pid=,etimes=,comm="], argv
            return out(self.ps)
        if argv[0] == "ps":
            return out(f"{self.etimes}\n")
        if argv[0].endswith("nebius"):
            if self.stop_exc:
                raise self.stop_exc
            return out("", self.stop_rc)
        raise AssertionError(argv)

    def stops(self):
        return [a for a, _ in self.calls if a[0].endswith("nebius")]

    def log(self):
        return [json.loads(l) for l in open(self.log_path)]

    def go(self):
        saved = (w.subprocess.run, w.STATE, w.LOG, w._boot_time, w.time.time)
        w.subprocess.run, w.STATE, w.LOG = self.run, self.state_path, self.log_path
        w._boot_time, w.time.time = (lambda: self.now - 10 * H), (lambda: self.now)
        try:
            return w.main()
        finally:
            w.subprocess.run, w.STATE, w.LOG, w._boot_time, w.time.time = saved


def test_main_nvidia_smi_missing_still_logs_and_stops():
    x = World(smi_exc=FileNotFoundError("nvidia-smi"))
    x.go()
    (line,) = x.log()
    assert line["decision"] == "idle-stop" and line["error"] and "nvidia-smi" in line["error"]
    assert line["card"] == []
    assert len(x.stops()) == 1


def test_main_nvml_error_output_still_logs_and_stops():
    x = World(smi="Failed to initialize NVML: Unknown Error\n")
    x.go()
    (line,) = x.log()
    assert line["decision"] == "idle-stop" and "NVML" in line["error"]
    assert len(x.stops()) == 1


def test_main_process_listing_failure_still_decides():
    x = World(ps_exc=subprocess.TimeoutExpired("ps", 30))
    x.go()
    (line,) = x.log()
    assert line["decision"] == "idle-stop" and line["error"]
    assert line["tests"] is False and line["build"] is False


def test_main_healthy_tick_has_no_error():
    x = World(state={"last_test": 5_000_000.0 - 60, "last_build": 0.0})
    x.go()
    (line,) = x.log()
    assert line["decision"] == "keep" and line["error"] is None and not x.stops()


def test_main_state_write_failure_does_not_block_the_stop():
    x = World()
    x.state_path = x.dir + "/missing/state.json"
    x.go()
    assert len(x.stops()) == 1
    (line,) = x.log()
    assert "state" in line["error"]


def test_main_log_write_failure_does_not_block_the_stop():
    x = World()
    x.log_path = x.dir + "/missing/log"
    x.go()
    assert len(x.stops()) == 1


def test_main_stop_timeout_is_caught_and_logged():
    x = World(stop_exc=subprocess.TimeoutExpired("nebius", 90))
    assert x.go() != 0
    (line,) = x.log()
    assert line["decision"] == "idle-stop" and "stop" in line["error"]


def test_main_stop_failure_is_logged():
    x = World(stop_rc=1)
    assert x.go() != 0
    (line,) = x.log()
    assert "stop" in line["error"]


def test_every_subprocess_has_a_timeout():
    x = World(smi="1234, /bin/test_gpu_corpus\n")
    x.etimes = 4 * H + 10  # past the cap, so the stop is issued
    x.go()
    assert len(x.calls) >= 4, x.calls
    for argv, kw in x.calls:
        assert kw.get("timeout"), argv
    stop = [kw for a, kw in x.calls if a[0].endswith("nebius")][0]
    assert stop["timeout"] == 90
    assert all(kw["timeout"] == 30 for a, kw in x.calls if not a[0].endswith("nebius"))


def test_main_hung_cpu_side_test_is_capped_and_logged():
    x = World(state={"last_test": 5_000_000.0 - 60, "last_build": 0.0})
    x.ps = " 1 900 bash\n 4242 14411 peacock_cpu_tes\n"
    x.go()
    (line,) = x.log()
    assert line["decision"] == "cap-stop" and "pid 4242" in line["reason"], line
    assert line["tests"] is True
    assert [p["pid"] for p in line["test_procs"]] == [4242]
    assert line["test_procs"][0]["start"] == x.now - 14411
    assert len(x.stops()) == 1


def test_main_young_test_process_is_activity():
    x = World()  # last test 2 h ago: idle without the process
    x.ps = " 1 900 bash\n 4242 60 test_gpu_corpus\n"
    x.go()
    (line,) = x.log()
    assert line["decision"] == "keep" and line["tests"] is True and not x.stops(), line


def test_service_has_a_start_timeout():
    unit = (pathlib.Path(__file__).resolve().parents[1] / "peacock-gpu-idle.service").read_text()
    assert "TimeoutStartSec=150" in unit.splitlines()


if __name__ == "__main__":
    raise SystemExit(main(globals()))
