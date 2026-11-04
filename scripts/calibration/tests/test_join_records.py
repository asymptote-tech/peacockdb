"""`join_records.py`: one benchmark run's segments, a process per case, joined into one record.

`bench-each.sh` runs each case in its own process and sets each process's record aside as
`<ds>-<q>-<mode>.<attempt>.tsv`. A case killed between its rows and its section is measured
again, so its rows are in two segments; the later attempt is the one its section describes. The
segments' headings agree but for the pool's free memory, which each process reads at its start.
"""

import contextlib
import io
import pathlib
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import join_records  # noqa: E402
from harness import main  # noqa: E402

COLUMNS = ("dataset sf query mode node_seq node_type lane recipe_seq recipe_kind "
           "call_index run_index in_rows in_bytes out_rows out_bytes host_us device_us").split()


def heading(free="79.1GiB", capture="none", device="NVIDIA H100 80GB HBM3"):
    return ("# peacockdb cost-model calibration record.\n"
            "# run: timing_mode=events\n"
            "# run: build=release\n"
            f"# run: allocator=rmm-pool initial=69.0GiB max=69.0GiB of {free} free on a discrete device\n"
            f"# run: capture={capture}\n"
            f"# run: device={device}\n"
            "# run: driver=580.95.05\n"
            "# run: cuda=12.9\n"
            "# run: cudf=25.02.02\n"
            "# run: host=h100-node\n")


def row(query, device_us, dataset="tpch", mode="tp1-single", run=0):
    fields = [dataset, "40", query, mode, "0", "GpuScan", "0", "0", "CudfScan", "0", str(run),
              "1", "8", "1", "8", "30", str(device_us)]
    return "\t".join(fields) + "\n"


def segment(directory, name, rows, **heading_args):
    path = directory / name
    path.write_text(heading(**heading_args) + "\t".join(COLUMNS) + "\n" + "".join(rows))
    return path


def join(directory, *records):
    out = directory / "joined.out.tsv"
    with contextlib.redirect_stdout(io.StringIO()):
        join_records.main([*(a for r in records for a in ("--record", str(r))), "--out", str(out)])
    return out.read_text()


def refused(directory, *records):
    out = directory / "joined.out.tsv"
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            join_records.main([*(a for r in records for a in ("--record", str(r))), "--out", str(out)])
    except SystemExit as caught:
        assert caught.code not in (0, None), caught.code
        assert not out.exists(), "a refused join writes nothing"
        return str(caught.code)
    raise AssertionError("the join was not refused")


def body(text):
    return [line for line in text.splitlines(keepends=True) if not line.startswith("#")][1:]


def test_join_takes_the_later_segment_of_a_case():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        first = segment(tmp, "tpch-q1-tp1_single.1.tsv", [row("q1", 10), row("q1", 11, run=1)])
        q6 = segment(tmp, "tpch-q6-tp1_single.1.tsv", [row("q6", 5)])
        second = segment(tmp, "tpch-q1-tp1_single.2.tsv", [row("q1", 20), row("q1", 21, run=1)])
        joined = join(tmp, first, q6, second)
        assert body(joined) == [row("q1", 20), row("q1", 21, run=1), row("q6", 5)], joined
        assert joined.startswith(heading() + "\t".join(COLUMNS) + "\n"), joined


def test_join_orders_segments_by_attempt_not_argv():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        two = segment(tmp, "tpch-q1-tp1_single.2.tsv", [row("q1", 20)])
        one = segment(tmp, "tpch-q1-tp1_single.1.tsv", [row("q1", 10)])
        assert body(join(tmp, two, one)) == [row("q1", 20)]
        ten = segment(tmp, "tpch-q6-tp1_single.10.tsv", [row("q6", 100)])
        nine = segment(tmp, "tpch-q6-tp1_single.9.tsv", [row("q6", 90)])
        assert body(join(tmp, ten, nine)) == [row("q6", 100)]
        assert body(join(tmp, nine, ten)) == [row("q6", 100)]


def test_join_spans_the_free_memory():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        a = segment(tmp, "tpch-q1-tp1_single.1.tsv", [row("q1", 1)], free="79.3GiB")
        b = segment(tmp, "tpch-q6-tp1_single.1.tsv", [row("q6", 1)], free="79.1GiB")
        c = segment(tmp, "tpch-q3-tp1_single.1.tsv", [row("q3", 1)], free="79.2GiB")
        joined = join(tmp, a, b, c)
        assert ("# run: allocator=rmm-pool initial=69.0GiB max=69.0GiB of 79.1GiB-79.3GiB free on a "
                "discrete device\n") in joined, joined
        same = join(tmp, b, segment(tmp, "tpch-q9-tp1_single.1.tsv", [row("q9", 1)], free="79.1GiB"))
        assert "of 79.1GiB free on" in same, same


def test_join_refuses_an_allocator_that_differs_beyond_the_free_memory():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        a = segment(tmp, "tpch-q1-tp1_single.1.tsv", [row("q1", 1)])
        b = tmp / "tpch-q6-tp1_single.1.tsv"
        b.write_text(heading().replace("initial=69.0GiB", "initial=60.0GiB") + "\t".join(COLUMNS) + "\n"
                     + row("q6", 1))
        caught = refused(tmp, a, b)
        assert "allocator=" in caught and str(b) in caught, caught


def test_join_takes_a_joined_record():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        earlier = segment(tmp, "tpch-q1-tp1_single.1.tsv", [row("q1", 10)], free="79.0GiB")
        later = segment(tmp, "tpch-q6-tp1_single.1.tsv", [row("q6", 5)], free="79.2GiB")
        published = tmp / "published.tsv"
        published.write_text(join(tmp, earlier, later))
        rerun = segment(tmp, "tpch-q6-tp1_single.1.tsv", [row("q6", 7)], free="79.4GiB")
        joined = join(tmp, published, rerun)
        assert "of 79.0GiB-79.4GiB free on" in joined, joined
        assert body(joined) == [row("q1", 10), row("q6", 7)], joined


def test_join_refuses_another_device():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        a = segment(tmp, "tpch-q1-tp1_single.1.tsv", [row("q1", 1)])
        b = segment(tmp, "tpch-q6-tp1_single.1.tsv", [row("q6", 1)], device="NVIDIA H200")
        caught = refused(tmp, a, b)
        assert "# run: device=NVIDIA H200" in caught, caught


def test_join_refuses_a_capture():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        a = segment(tmp, "tpch-q1-tp1_single.1.tsv", [row("q1", 1)], capture="trace")
        b = segment(tmp, "tpch-q6-tp1_single.1.tsv", [row("q6", 1)], capture="trace")
        caught = refused(tmp, a, b)
        assert "capture=none" in caught, caught


def test_join_refuses_two_segments_of_one_attempt():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        (tmp / "a").mkdir()
        (tmp / "b").mkdir()
        a = segment(tmp / "a", "tpch-q1-tp1_single.1.tsv", [row("q1", 1)])
        b = segment(tmp / "b", "tpch-q1-tp1_single.1.tsv", [row("q1", 2)])
        caught = refused(tmp, a, b)
        assert "attempt 1" in caught, caught


if __name__ == "__main__":
    raise SystemExit(main(globals()))
