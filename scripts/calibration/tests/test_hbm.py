"""`nsys_hbm.py` over a synthetic capture and the record it joins onto.

Two runs meet in this script — traffic from a capture under GPU memory counters, times and
coordinates from a clean run — so the two things it can get wrong are which call a byte is
charged to, and what a percentage of peak is worth in bytes.
"""

import pathlib
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import nsys_hbm  # noqa: E402
import record  # noqa: E402
from capture import Capture  # noqa: E402
from harness import main  # noqa: E402

CASE = "tpch.sf40 q6 tp1-single"
PEAK_BW = 1e12
PERIOD = 20
# The one call that moved anything, as a percentage of peak DRAM bandwidth per sample.
BUSY_CALL = 1
BUSY_READ = 40.0

COLUMNS = ("dataset sf query mode node_seq node_type lane recipe_seq recipe_kind "
           "call_index run_index in_rows in_bytes out_rows out_bytes host_us device_us")


def three_call_capture(directory):
    """One case, three calls, and metric samples in which only the middle one reads."""
    path = directory / "capture-hbm.sqlite"
    cap = Capture(path)
    spans = cap.case(CASE, [(0, 0, "CudfScan", ["read_parquet"]),
                            (1, 0, "CudfFilter", ["apply_boolean_mask"]),
                            (2, 0, "CudfProject", ["compute_column"])])
    busy = spans[BUSY_CALL]
    first, last = spans[0][0] - 500, spans[-1][1] + 500
    read, write = [], []
    for at in range(first, last, PERIOD):
        inside = busy[0] <= at < busy[1]
        read.append((at, BUSY_READ if inside else 0.0))
        write.append((at, 0.0))
    cap.metric(nsys_hbm.DRAM_READ, read)
    cap.metric(nsys_hbm.DRAM_WRITE, write)
    cap.close()
    return path


def three_row_record(directory, drop=None):
    """The clean run's rows for those calls: one per call, one execution."""
    path = directory / "records.tsv"
    rows = [
        ["tpch", "40", "q6", "tp1-single", seq, kind, "0", seq, kind, "0", "0",
         "100", "800", "100", "800", "7", "5"]
        for seq, kind in ((0, "CudfScan"), (1, "CudfFilter"), (2, "CudfProject"))
        if seq != drop
    ]
    record.write_tsv(path, ["run: timing_mode=events", "run: build=release"],
                     COLUMNS.split(), rows)
    return path


def hbm_of(capture, record_path, out, *args):
    sys.argv = ["nsys_hbm.py", "--capture", str(capture), "--record", str(record_path),
                "--out", str(out), *args]
    nsys_hbm.main()
    return record.read_tsv(out)


def test_traffic_lands_on_the_call_that_moved_it():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        rows = hbm_of(three_call_capture(tmp), three_row_record(tmp), tmp / "hbm.tsv",
                      "--peak-bw", str(PEAK_BW))

    assert [r["recipe_seq"] for r in rows] == ["0", "1", "2"], rows
    moved = [int(r["hbm_bytes"]) for r in rows]
    assert moved[0] == 0 and moved[2] == 0, moved
    # The integral is the percentage over the samples inside the call, so the answer is
    # checkable rather than merely non-zero: bytes = Σ pct/100 × peak × period.
    samples = int(rows[BUSY_CALL]["samples"])
    assert samples > 1, rows[BUSY_CALL]
    assert moved[BUSY_CALL] == round(samples * BUSY_READ / 100 * PEAK_BW * PERIOD / 1e9)
    # The coordinates are the record's own, not the capture's names.
    assert rows[BUSY_CALL]["query"] == "q6"
    assert rows[BUSY_CALL]["node_type"] == "CudfFilter"


def test_a_missing_peak_bw_is_refused():
    """A wrong peak is a silent scale error on every row, so it is not a display default."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        try:
            hbm_of(three_call_capture(tmp), three_row_record(tmp), tmp / "hbm.tsv")
        except SystemExit as refused:
            assert refused.code != 0, "exited zero having written nothing"
        else:
            raise AssertionError("--peak-bw is optional, so every row is scaled by a guess")


def test_a_capture_and_a_record_of_different_work_are_refused():
    """A call in one and not the other means they are not one run, whatever the row count."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        try:
            hbm_of(three_call_capture(tmp), three_row_record(tmp, drop=1),
                   tmp / "hbm.tsv", "--peak-bw", str(PEAK_BW))
        except SystemExit as refused:
            assert "do not describe the same calls" in str(refused), refused
        else:
            raise AssertionError("a captured call with no row was joined anyway")


if __name__ == "__main__":
    raise SystemExit(main(globals()))
