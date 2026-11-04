"""Tests for comet_bytes.py on a hand-made two-stage event log.

The fixture is one query (tpch q1, run 1) of the app `comet-tpch-local-sequential-ball`, so its
config is `tpch-local-sequential-ball`: stage 0 scans and writes shuffle, stage 1 reads it
and spills. Its numbers are chosen so each column has one hand-checkable answer:
- scan_bytes 1000, shuffle_bytes 300, spill_bytes 50, s3_gets 4 (the stages' accumulables);
- driver_ms 100: the SQL execution's 900 ms less the 800 ms when a stage was open;
- scheduler_ms 40: per stage, its open time less its tasks' span (500-480, 300-280).
"""
import subprocess
import sys
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1]
FIXTURE = Path(__file__).resolve().parent / "fixtures"
sys.path.insert(0, str(SCRIPTS))

import comet_bytes  # noqa: E402

HEADER = ["config", "dataset", "query", "run", "scan_bytes", "shuffle_bytes", "spill_bytes", "s3_gets",
          "driver_ms", "scheduler_ms"]


def test_sums_equal_the_fixture_stage_metrics():
    rows = comet_bytes.summarize([FIXTURE / "eventlog-small.json"])
    assert rows == [{
        "config": "tpch-local-sequential-ball", "dataset": "tpch", "query": "q1", "run": "1",
        "scan_bytes": 1000, "shuffle_bytes": 300, "spill_bytes": 50, "s3_gets": 4,
        "driver_ms": 100, "scheduler_ms": 40,
    }]


def test_cli_prints_the_tsv(tmp_path):
    (tmp_path / "app-1").write_bytes((FIXTURE / "eventlog-small.json").read_bytes())
    out = subprocess.run([sys.executable, str(SCRIPTS / "comet_bytes.py"), str(tmp_path)],
                         capture_output=True, text=True, timeout=60, check=True).stdout
    lines = [l.split("\t") for l in out.splitlines()]
    assert lines[0] == HEADER
    assert lines[1] == ["tpch-local-sequential-ball", "tpch", "q1", "1", "1000", "300", "50", "4", "100", "40"]


def write_rolled(d, parts):
    """A Spark 4 rolling event log: eventlog_v2_<app>/events_<n>_<app>, plus its status files."""
    app = d / "eventlog_v2_app-1"
    app.mkdir()
    for n, text in parts.items():
        (app / f"events_{n}_app-1").write_text(text)
    (app / ".appstatus_app-1.crc").write_bytes(b"\x00\x01crc\xff\xfe\x00\x07")
    (app / "appstatus_app-1").write_bytes(b"")
    return app


def test_a_rolled_log_is_one_stream_and_status_files_are_skipped(tmp_path):
    # The fixture split mid-query across two parts gives the same row as the whole file.
    lines = (FIXTURE / "eventlog-small.json").read_text().splitlines(keepends=True)
    write_rolled(tmp_path, {1: "".join(lines[:6]), 2: "".join(lines[6:])})
    out = subprocess.run([sys.executable, str(SCRIPTS / "comet_bytes.py"), str(tmp_path)],
                         capture_output=True, text=True, timeout=60, check=True).stdout
    assert out.splitlines()[1].split("\t") == ["tpch-local-sequential-ball", "tpch", "q1", "1", "1000", "300", "50", "4", "100", "40"]


def test_rolled_parts_are_read_in_numeric_order(tmp_path):
    app = write_rolled(tmp_path, {1: "", 2: "", 10: ""})
    (tmp_path / "app-2").write_text("")  # a single-file (v1) log is its own stream
    assert comet_bytes.log_streams(tmp_path) == [  # apps in path order
        [tmp_path / "app-2"],
        [app / "events_1_app-1", app / "events_2_app-1", app / "events_10_app-1"],
    ]
