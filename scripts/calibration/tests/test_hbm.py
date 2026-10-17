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


def three_call_capture(directory, unit=""):
    """One case, three calls, and metric samples in which only the middle one reads.

    `unit` is what the capturing nsys appends to a metric's name: nothing before 2024,
    " [Throughput %]" since.
    """
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
    cap.metric(nsys_hbm.DRAM_READ + unit, read)
    cap.metric(nsys_hbm.DRAM_WRITE + unit, write)
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
    # One row per call, saying how many executions it stands for, and no run_index.
    assert {r["executions"] for r in rows} == {"1"}, rows
    assert "run_index" not in rows[0], rows[0]
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


def test_a_newer_nsys_names_the_unit_in_the_metric():
    # nsys 2024.5 writes 'DRAM Read Bandwidth [Throughput %]' where 2023.2 wrote
    # 'DRAM Read Bandwidth'; the samples mean the same, so the lookup takes either.
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        rows = hbm_of(three_call_capture(tmp, unit=" [Throughput %]"), three_row_record(tmp),
                      tmp / "hbm.tsv", "--peak-bw", str(PEAK_BW))
    moved = [int(r["hbm_bytes"]) for r in rows]
    assert moved[0] == 0 and moved[BUSY_CALL] > 0 and moved[2] == 0, moved


def test_a_capture_of_two_datasets_joins_onto_their_two_records():
    """A full run's record is published a file per dataset, and its capture is one file.

    Each record alone describes part of the capture, so a join that keeps one `--record`
    refuses the capture as different work. Given both, every row of each lands.
    """
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        path = tmp / "capture-hbm.sqlite"
        cap = Capture(path)
        spans = cap.case(CASE, [(0, 0, "CudfScan", ["read_parquet"])])
        spans += cap.case("tpcds.sf40 q37 tp1-single", [(0, 0, "CudfScan", ["read_parquet"])])
        stamps = range(spans[0][0] - 500, spans[-1][1] + 500, PERIOD)
        cap.metric(nsys_hbm.DRAM_READ, [(at, BUSY_READ) for at in stamps])
        cap.metric(nsys_hbm.DRAM_WRITE, [(at, 0.0) for at in stamps])
        cap.close()
        records = []
        for dataset, query in (("tpch", "q6"), ("tpcds", "q37")):
            records.append(tmp / f"{dataset}.tsv")
            record.write_tsv(records[-1], ["run: timing_mode=events", "run: build=release"],
                             COLUMNS.split(),
                             [[dataset, "40", query, "tp1-single", 0, "CudfScan", "0", 0,
                               "CudfScan", "0", "0", "100", "800", "100", "800", "7", "5"]])
        sys.argv = ["nsys_hbm.py", "--capture", str(path), "--record", str(records[0]),
                    "--record", str(records[1]), "--out", str(tmp / "hbm.tsv"),
                    "--peak-bw", str(PEAK_BW)]
        nsys_hbm.main()
        rows = record.read_tsv(tmp / "hbm.tsv")

    assert [(r["dataset"], r["query"]) for r in rows] == [("tpch", "q6"), ("tpcds", "q37")]
    assert all(int(r["hbm_bytes"]) > 0 for r in rows), rows


def scan_capture(directory, case, reads):
    """One case, one call, driven once per entry of `reads`, the percentage it read at."""
    path = directory / f"capture-{case.split()[1]}.sqlite"
    cap = Capture(path)
    spans = [(cap.case(case, [(0, 0, "CudfScan", ["read_parquet"])])[0], pct) for pct in reads]
    # Sampled without a break from before the first execution to after the last, as nsys
    # samples a run: a gap would be refused as a stalled collection.
    stamps = range(spans[0][0][0] - 500, spans[-1][0][1] + 500, PERIOD)
    reading = lambda at: next((pct for (a, b), pct in spans if a <= at <= b), 0.0)
    cap.metric(nsys_hbm.DRAM_READ, [(at, reading(at)) for at in stamps])
    cap.metric(nsys_hbm.DRAM_WRITE, [(at, 0.0) for at in stamps])
    cap.close()
    return path


def scan_record(directory, dataset, query, runs):
    path = directory / f"{dataset}.tsv"
    record.write_tsv(path, ["run: timing_mode=events", "run: build=release"], COLUMNS.split(),
                     [[dataset, "40", query, "tp1-single", 0, "CudfScan", "0", 0, "CudfScan",
                       "0", run, "100", "800", "100", "800", "7", "5"] for run in range(runs)])
    return path


def test_a_call_is_the_median_of_its_executions():
    """Three executions of one call give one row: their median, not their mean or the last.

    20, 40 and 90 percent: the median is the middle execution's bytes exactly, where the
    mean would be pulled half-way to the outlier.
    """
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        capture = scan_capture(tmp, CASE, [20.0, 40.0, 90.0])
        rows = hbm_of(capture, scan_record(tmp, "tpch", "q6", 3), tmp / "hbm.tsv",
                      "--peak-bw", str(PEAK_BW))
    assert len(rows) == 1 and rows[0]["executions"] == "3", rows
    samples = int(rows[0]["samples"])
    assert int(rows[0]["hbm_bytes"]) == round(samples * 40.0 / 100 * PEAK_BW * PERIOD / 1e9), rows


def test_cases_captured_apart_merge_into_one_file_that_covers_the_record():
    """One capture per case, each joined alone, then the parts merged against the record.

    A case joined on its own may hold only that case's calls, so its capture is checked
    against that case's rows; the merge is where the record as a whole is checked, so a
    part that never ran is refused there, naming the call it lacks.
    """
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        records = [scan_record(tmp, "tpch", "q6", 2), scan_record(tmp, "tpcds", "q37", 2)]
        record_args = [arg for path in records for arg in ("--record", str(path))]
        parts = []
        for case in (CASE, "tpcds.sf40 q37 tp1-single"):
            capture = scan_capture(tmp, case, [30.0, 30.0])
            parts.append(tmp / f"part-{case.split()[1]}.tsv")
            sys.argv = ["nsys_hbm.py", "--capture", str(capture), *record_args,
                        "--case", case, "--peak-bw", str(PEAK_BW), "--out", str(parts[-1])]
            nsys_hbm.main()

        merged = tmp / "hbm.tsv"
        sys.argv = ["nsys_hbm.py", "--parts", str(parts[0]), "--parts", str(parts[1]),
                    *record_args, "--out", str(merged)]
        nsys_hbm.main()
        rows = record.read_tsv(merged)
        assert [(r["dataset"], r["query"]) for r in rows] == [("tpch", "q6"), ("tpcds", "q37")]

        sys.argv = ["nsys_hbm.py", "--parts", str(parts[0]), *record_args, "--out", str(merged)]
        try:
            nsys_hbm.main()
        except SystemExit as refused:
            assert "q37" in str(refused), refused
        else:
            raise AssertionError("a merge missing a case's part was written anyway")

        # A part of a case the records do not hold, and a part given twice.
        sys.argv = ["nsys_hbm.py", "--parts", str(parts[0]), "--parts", str(parts[1]),
                    "--record", str(records[0]), "--out", str(merged)]
        try:
            nsys_hbm.main()
        except SystemExit as refused:
            assert "in no record" in str(refused) and "q37" in str(refused), refused
        else:
            raise AssertionError("a part of a case no record holds was merged")
        sys.argv = ["nsys_hbm.py", "--parts", str(parts[0]), "--parts", str(parts[0]),
                    *record_args, "--out", str(merged)]
        try:
            nsys_hbm.main()
        except SystemExit as refused:
            assert "overlap" in str(refused), refused
        else:
            raise AssertionError("a part given twice was merged")


def test_a_capture_of_another_case_than_named_is_refused():
    """Said as that, naming what the capture holds, before any call is compared: the
    join's own loss report would also refuse it, as calls with no row, which reads as a
    broken run rather than the wrong file."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        capture = scan_capture(tmp, CASE, [30.0])
        sys.argv = ["nsys_hbm.py", "--capture", str(capture),
                    "--record", str(scan_record(tmp, "tpcds", "q37", 1)),
                    "--case", "tpcds.sf40 q37 tp1-single", "--peak-bw", str(PEAK_BW),
                    "--out", str(tmp / "part.tsv")]
        try:
            nsys_hbm.main()
        except SystemExit as refused:
            assert "--case is" in str(refused) and "q6" in str(refused), refused
        else:
            raise AssertionError("a q6 capture was joined as q37")


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
