"""Tests for s3_throughput.py's range split and its local-file path (no S3, no sudo)."""
import subprocess
import sys
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import s3_throughput  # noqa: E402


def test_ranges_cover_the_object_once():
    assert s3_throughput.ranges(10, 4) == [(0, 3), (4, 7), (8, 9)]
    assert s3_throughput.ranges(8, 4) == [(0, 3), (4, 7)]


def test_file_mode_prints_one_row_with_every_byte(tmp_path):
    f = tmp_path / "blob"
    f.write_bytes(b"x" * 3_000_000)
    out = subprocess.run(
        [sys.executable, str(SCRIPTS / "s3_throughput.py"), "--file", str(f), "--threads", "4",
         "--part-mb", "1", "--vm", "test", "--no-direct", "--no-drop-cache"],
        capture_output=True, text=True, timeout=60, check=True).stdout
    row = out.strip().split("\t")
    assert len(row) == 7
    vm, threads, nbytes, seconds, gbps, source, cpu = row
    assert (vm, threads, nbytes, source) == ("test", "4", "3000000", f"file:{f}")
    assert float(seconds) > 0 and float(gbps) > 0 and float(cpu) >= 0
