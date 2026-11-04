"""Tests for comet_data.py's decisions: reuse a local copy, and what to copy or upload."""
import sys
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import comet_data  # noqa: E402

LISTING = {"lineitem.parquet": 100, "orders.parquet": 50, "manifest.json": 7}


def make(d: Path, files: dict[str, int]):
    d.mkdir(parents=True, exist_ok=True)
    for name, size in files.items():
        (d / name).write_bytes(b"\0" * size)


def test_a_local_copy_is_reused_only_if_every_parquet_size_matches(tmp_path):
    make(tmp_path / "ok", {"lineitem.parquet": 100, "orders.parquet": 50})
    make(tmp_path / "short", {"lineitem.parquet": 100, "orders.parquet": 49})
    make(tmp_path / "missing", {"lineitem.parquet": 100})
    assert comet_data.reusable(LISTING, tmp_path / "ok")
    assert not comet_data.reusable(LISTING, tmp_path / "short")
    assert not comet_data.reusable(LISTING, tmp_path / "missing")
    assert not comet_data.reusable(LISTING, tmp_path / "absent")


def test_to_transfer_skips_objects_already_there_at_the_right_size():
    dst = {"lineitem.parquet": 100, "orders.parquet": 10}
    assert comet_data.to_transfer(LISTING, dst) == ["orders.parquet"]
    assert comet_data.to_transfer(LISTING, {}) == ["lineitem.parquet", "orders.parquet"]
