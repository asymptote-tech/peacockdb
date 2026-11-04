"""Tests for comet_bytes.py on a hand-made two-stage event log.

The fixture is one query (tpch q1, run 1): stage 0 scans and writes shuffle, stage 1 reads it
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

HEADER = ["dataset", "query", "run", "scan_bytes", "shuffle_bytes", "spill_bytes", "s3_gets",
          "driver_ms", "scheduler_ms"]


def test_sums_equal_the_fixture_stage_metrics():
    rows = comet_bytes.summarize([FIXTURE / "eventlog-small.json"])
    assert rows == [{
        "dataset": "tpch", "query": "q1", "run": "1",
        "scan_bytes": 1000, "shuffle_bytes": 300, "spill_bytes": 50, "s3_gets": 4,
        "driver_ms": 100, "scheduler_ms": 40,
    }]


def test_cli_prints_the_tsv(tmp_path):
    (tmp_path / "app-1").write_bytes((FIXTURE / "eventlog-small.json").read_bytes())
    out = subprocess.run([sys.executable, str(SCRIPTS / "comet_bytes.py"), str(tmp_path)],
                         capture_output=True, text=True, timeout=60, check=True).stdout
    lines = [l.split("\t") for l in out.splitlines()]
    assert lines[0] == HEADER
    assert lines[1] == ["tpch", "q1", "1", "1000", "300", "50", "4", "100", "40"]
