"""The checker's verdict over the watcher's last log line."""
import json, pathlib, subprocess, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import check  # noqa: E402
from harness import main  # noqa: E402

NOW = 2_000_000.0
H = 3600.0


def line(**kv):
    base = {"t": NOW - 60, "boot": NOW - 5 * H, "card": [], "tests": False, "test_procs": [], "build": False,
            "last_test": NOW - 600, "last_build": NOW - 5 * H, "decision": "keep", "reason": ""}
    return json.dumps({**base, **kv})


def test_fresh_keep_is_none():
    assert check.verdict(line(), NOW) is None


def test_no_log_is_stop():
    assert "no watcher log" in check.verdict(None, NOW)


def test_stale_log_is_stop():
    assert "stale" in check.verdict(line(t=NOW - H - 1), NOW)


def test_idle_recomputed_is_stop():
    assert check.verdict(line(last_test=NOW - H - 120), NOW) is not None


def test_build_window_keeps():
    assert check.verdict(line(last_test=NOW - 1.5 * H, last_build=NOW - 0.5 * H), NOW) is None


def test_cap_recomputed_is_stop():
    card = [{"pid": 9, "name": "test_gpu_corpus", "start": NOW - 4 * H - 1}]
    assert "pid 9" in check.verdict(line(card=card, last_test=NOW - 60), NOW)


def test_cap_on_a_name_matched_test_process_is_stop():
    # A hung CPU-side test is not on the card; the checker recomputes its cap from test_procs.
    procs = [{"pid": 11, "name": "peacock_cpu_tests", "start": NOW - 4 * H - 1}]
    r = check.verdict(line(tests=True, test_procs=procs, last_test=NOW - 60), NOW)
    assert r is not None and "pid 11" in r, r


def test_young_name_matched_test_process_keeps():
    procs = [{"pid": 11, "name": "peacock_cpu_tests", "start": NOW - 2 * H}]
    assert check.verdict(line(tests=True, test_procs=procs, last_test=NOW - 60), NOW) is None


def test_line_without_test_procs_is_stop():
    # A watcher too old to log test_procs cannot be checked for the cap: re-run install-watch.
    d = json.loads(line())
    del d["test_procs"]
    r = check.verdict(json.dumps(d), NOW)
    assert r is not None and "malformed" in r, r


def test_watcher_said_stop_is_stop():
    assert check.verdict(line(decision="idle-stop", reason="x"), NOW) is not None


def test_garbage_line_is_stop():
    assert "unreadable" in check.verdict("not json", NOW)


def test_line_missing_a_field_is_stop():
    d = json.loads(line())
    del d["t"]
    r = check.verdict(json.dumps(d), NOW)
    assert r is not None and "malformed" in r


def test_json_that_is_not_an_object_is_stop():
    for text in ("null", "5", "[]", '"keep"'):
        assert check.verdict(text, NOW) is not None, text


def test_wrongly_typed_field_is_stop():
    assert check.verdict(line(card=[{"pid": 9}]), NOW) is not None
    assert check.verdict(line(last_test="x"), NOW) is not None


def test_non_finite_and_huge_numbers_are_stop():
    for text in ('{"t": 1999940, "last_test": Infinity}', '{"t": NaN}', '{"t": -Infinity}',
                 '{"t": 1999940, "boot": 1' + "0" * 400 + "}",
                 line().replace('"last_test": ', '"last_test": 1' + "0" * 400 + ", \"x\": ")):
        r = check.verdict(text, NOW)
        assert r is not None, text[:60]


def test_deeply_nested_line_is_stop():
    r = check.verdict("[" * 200000, NOW)
    assert r is not None and "unreadable" in r


CHECK = str(pathlib.Path(check.__file__).resolve())


def run_cli(stdin, *argv):
    p = subprocess.run([sys.executable, CHECK, *argv], input=stdin, capture_output=True, timeout=60)
    return p.returncode, p.stdout.decode()


def test_cli_healthy_line_keeps():
    rc, out = run_cli((line() + "\n").encode(), str(NOW))
    assert (rc, out.strip()) == (0, "keep")


def test_cli_garbage_bytes_stop_with_a_reason():
    for data in (b"\xff\xfe\x80{", b"\xff" * 100, b"not json\n", b"[" * 200000, b""):
        rc, out = run_cli(data, str(NOW))
        assert rc == 1 and out.strip(), data[:20]


def test_cli_bad_argv_stops_with_a_reason():
    for argv in ((), ("abc",)):
        rc, out = run_cli((line() + "\n").encode(), *argv)
        assert rc == 1 and out.strip(), argv


if __name__ == "__main__":
    raise SystemExit(main(globals()))
