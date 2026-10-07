"""`split_record.py`: a run's record cut into one file per dataset, and refused unless whole.

A run is truncated at its start and writes only the cases it ran, so a filtered run's record
is a fraction of the full one under the same name; pinned, it would stand for the full one.
What is checked here is that a dataset's file is written only when the run timed every case
the checkout declares for it, under a plain run's heading, and that the file is the run's own
bytes — its heading and its rows, in the order the run wrote them.
"""

import contextlib
import io
import pathlib
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import split_record  # noqa: E402
from harness import main  # noqa: E402

COLUMNS = ("dataset sf query mode node_seq node_type lane recipe_seq recipe_kind "
           "call_index run_index in_rows in_bytes out_rows out_bytes host_us device_us").split()
HEADING = ("# peacockdb cost-model calibration record.\n"
           "# run: timing_mode=events\n"
           "# run: allocator=rmm-pool initial=1.0GiB\n"
           "# run: capture=none\n")
CASES = """\
// a comment naming corpus_query_benchmark!(tpch, 40, q99, tp1_single); is not a case
corpus_query_benchmark!(tpch, 40, q6, tp1_single | tp4_sized);
corpus_query_benchmark!(tpch, 40, q19, tp1_single); // #152 holds the other modes
corpus_query_benchmark!(tpch, 40, q2, none);
corpus_query_benchmark!(tpcds, 40, q82, tp1_single);
corpus_query_benchmark!(tpch, 40, scan_limit, tp1_single);
"""


def row(dataset, query, mode, seq=0):
    fields = [dataset, "40", query, mode, str(seq), "GpuScan", "0", str(seq), "CudfScan",
              "0", "0", "1", "8", "1", "8", "30", "25"]
    return "\t".join(fields) + "\n"


def write(directory, rows, name="records.tsv"):
    path = directory / name
    path.write_text(HEADING + "\t".join(COLUMNS) + "\n" + "".join(rows))
    return path


def split(directory, record):
    cases = directory / "cases.inc"
    cases.write_text(CASES)
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        split_record.main(["--record", str(record), "--cases", str(cases),
                           "--out-dir", str(directory / "out")])
    return out.getvalue().split()


def refusal(directory, record):
    try:
        split(directory, record)
    except SystemExit as refused:
        assert refused.code not in (None, 0), refused
        return str(refused)
    raise AssertionError("the record was split")


TPCH = [row("tpch", "q19", "tp1-single", 1), row("tpch", "q6", "tp1-single"),
        row("tpch", "q6", "tp4-sized"), row("tpch", "q19", "tp1-single", 2),
        row("tpch", "scan-limit", "tp1-single")]


def test_a_whole_run_of_one_dataset_is_its_file_byte_for_byte():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        record = write(tmp, TPCH)
        assert split(tmp, record) == ["tpch.sf40"]
        assert (tmp / "out/tpch.sf40/records.tsv").read_bytes() == record.read_bytes()


def test_two_datasets_in_one_run_are_two_files_each_under_the_run_s_heading():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tpcds = row("tpcds", "q82", "tp1-single")
        record = write(tmp, [TPCH[0], tpcds, *TPCH[1:]])
        assert split(tmp, record) == ["tpcds.sf40", "tpch.sf40"]
        head = HEADING + "\t".join(COLUMNS) + "\n"
        assert (tmp / "out/tpch.sf40/records.tsv").read_text() == head + "".join(TPCH)
        assert (tmp / "out/tpcds.sf40/records.tsv").read_text() == head + tpcds


def test_a_run_that_missed_a_declared_case_is_refused_naming_it_and_writes_nothing():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        refused = refusal(tmp, write(tmp, TPCH[1:3]))
        assert "q19 tp1-single" in refused, refused
        assert not (tmp / "out").exists(), "a refused split wrote a file"


def test_a_case_the_checkout_does_not_declare_is_refused_naming_it():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        refused = refusal(tmp, write(tmp, [*TPCH, row("tpch", "q1", "tp1-single")]))
        assert "q1 tp1-single" in refused, refused


def test_a_dataset_the_run_did_not_time_is_left_alone():
    """tpcds is declared and absent: a run filtered to one dataset is that dataset's whole run."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        assert split(tmp, write(tmp, TPCH)) == ["tpch.sf40"]
        assert not (tmp / "out/tpcds.sf40").exists()


def test_a_query_named_with_an_underscore_is_the_record_s_dashed_one():
    """The macro writes `stringify!($query).replace('_', "-")`, so `scan_limit` is `scan-limit`."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        assert split(tmp, write(tmp, TPCH)) == ["tpch.sf40"]
        refused = refusal(tmp, write(tmp, TPCH[:-1]))
        assert "scan-limit tp1-single" in refused, refused


def test_a_captured_run_s_record_is_refused():
    """Its microseconds are a profiler's; `capture=none` is the only heading published."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        record = write(tmp, TPCH)
        record.write_text(record.read_text().replace("capture=none", "capture=trace"))
        assert "capture=none" in refusal(tmp, record)


def test_a_record_with_no_rows_is_refused():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        assert "no rows" in refusal(tmp, write(tmp, []))


if __name__ == "__main__":
    raise SystemExit(main(globals()))
