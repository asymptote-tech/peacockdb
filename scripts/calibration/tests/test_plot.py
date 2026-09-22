"""`plot.py` over a small record, for the things a picture cannot show you.

A panel that silently did not draw looks like a panel with nothing in it, and an index page
listing five sections where six were asked for reads as a complete page. So what is checked
here is that every section of the page has a file behind it, and that a record whose
conditions differ from another's is refused rather than drawn as one set of samples.

Run with an interpreter that has matplotlib: `/usr/bin/python3 <file>` on a dev box, where
this repo's own python3 is a linuxbrew build without it. CI installs it for the step.
"""

import pathlib
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import plot  # noqa: E402
import record  # noqa: E402
from harness import main  # noqa: E402

COLUMNS = ("dataset sf query mode node_seq node_type lane recipe_seq recipe_kind "
           "call_index run_index in_rows in_bytes out_rows out_bytes host_us device_us")
HBM_COLUMNS = ("dataset sf query mode node_seq node_type lane recipe_seq recipe_kind "
               "call_index run_index hbm_read_bytes hbm_write_bytes hbm_bytes samples "
               "device_busy_us device_idle_us")
CALLS = ((0, "CudfScan"), (1, "CudfFilter"), (2, "CudfProject"), (3, "CudfAggregate"),
         (4, "CudfCoalescePartitions"))
RUNS = 2


def ten_row_record(directory, name="records.tsv", allocator="rmm-pool 1GiB"):
    """Five calls over two executions: enough for a spread, a load panel and a compute one."""
    rows = [
        ["tpch", "40", "q6", "tp1-single", seq, kind, "0", seq, kind, "0", run,
         "1000", 8000 * (seq + 1), "900", 7000 * (seq + 1), 30 + seq, 100 + seq + run]
        for seq, kind in CALLS
        for run in range(RUNS)
    ]
    path = directory / name
    record.write_tsv(path, [f"run: timing_mode=events", f"run: allocator={allocator}"],
                     COLUMNS.split(), rows)
    return path


def hbm_rows(directory):
    rows = [
        ["tpch", "40", "q6", "tp1-single", seq, kind, "0", seq, kind, "0", "0",
         4000 * (seq + 1), 3000 * (seq + 1), 7000 * (seq + 1), "40", "25", "5"]
        for seq, kind in CALLS
    ]
    path = directory / "hbm.tsv"
    record.write_tsv(path, ["hbm bytes per call"], HBM_COLUMNS.split(), rows)
    return path


def draw(*args):
    sys.argv = ["plot.py", *args]
    plot.main()


def test_every_section_of_the_index_has_a_panel_behind_it():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        out = tmp / "plots"
        draw("--record", str(ten_row_record(tmp)), "--hbm", str(hbm_rows(tmp)),
             "--out-dir", str(out))

        page = (out / "index.html").read_text()
        for section, _, _ in plot.SECTIONS:
            drawn = sorted((out / section).glob("*.png"))
            assert drawn, f"{section}/ has no panel, and index.html lists it anyway"
            for panel in drawn:
                assert f"{section}/{panel.name}" in page, f"{panel} is on disk, not on the page"


def test_two_records_taken_under_different_conditions_are_refused():
    """Their microseconds mean different things, so one set of samples they are not."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        first = ten_row_record(tmp)
        second = ten_row_record(tmp, name="other.tsv", allocator="cuda-default")
        try:
            draw("--record", str(first), "--record", str(second),
                 "--out-dir", str(tmp / "plots"))
        except SystemExit as refused:
            assert "allocator" in str(refused), refused
        else:
            raise AssertionError("two runs' rows were drawn as one set of samples")


def test_a_record_without_a_column_a_panel_reads_is_refused():
    """Refused at the door and naming the column.

    A panel drawn from a column that is not there is an empty panel, and an empty panel is
    what a call that cost nothing looks like.
    """
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        path = ten_row_record(tmp)
        text = path.read_text().replace("host_us", "peacock_host_us", 1)
        path.write_text(text)
        try:
            draw("--record", str(path), "--out-dir", str(tmp / "plots"))
        except SystemExit as refused:
            assert "host_us" in str(refused), refused
        else:
            raise AssertionError("the query panel drew a term the record does not carry")


if __name__ == "__main__":
    raise SystemExit(main(globals()))
