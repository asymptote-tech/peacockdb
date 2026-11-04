"""The checker over a CPU VM's watcher line: terminals and DuckDB, judged by the watcher's rule."""
import json, pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import check  # noqa: E402
from harness import main  # noqa: E402

NOW = 2_000_000.0
H = 3600.0


def line(**kv):
    base = {"t": NOW - 60, "boot": NOW - 5 * H, "card": [], "tests": False, "test_procs": [], "build": False,
            "last_test": NOW - 2 * H, "last_build": NOW - 5 * H, "decision": "keep", "reason": "",
            "sessions": [], "instance": "computeinstance-e01t2mm4265y8x88ey", "kind": "cpu"}
    return json.dumps({**base, **kv})


def test_session_in_the_line_keeps():
    assert check.verdict(line(sessions=[NOW - 300]), NOW) is None


def test_old_session_in_the_line_is_stop():
    r = check.verdict(line(sessions=[NOW - 70 * 60]), NOW)
    assert r is not None and "idle-stop" in r, r


def test_line_without_sessions_judged_as_before():
    d = json.loads(line(last_test=NOW - 600))
    for k in ("sessions", "instance", "kind"):
        del d[k]
    assert check.verdict(json.dumps(d), NOW) is None
    d["last_test"] = NOW - 2 * H
    r = check.verdict(json.dumps(d), NOW)
    assert r is not None and "idle-stop" in r, r


def test_mistyped_sessions_is_stop():
    for bad in ("x", ["x"], [None], 5):
        r = check.verdict(line(sessions=bad), NOW)
        assert r is not None and "malformed" in r, (bad, r)


def test_cpu_duckdb_cap_recomputed_is_stop():
    procs = [{"pid": 7, "name": "duckdb", "start": NOW - 4 * H - 1}]
    r = check.verdict(line(tests=True, test_procs=procs, last_test=NOW - 60), NOW)
    assert r is not None and "pid 7" in r and "duckdb" in r, r


if __name__ == "__main__":
    raise SystemExit(main(globals()))
