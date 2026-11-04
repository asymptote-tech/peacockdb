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


def ten_row_record(directory, name="records.tsv", allocator="rmm-pool 1GiB", dataset="tpch"):
    """Five calls over two executions: enough for a spread, a load panel and a compute one."""
    rows = [
        [dataset, "40", "q6", "tp1-single", seq, kind, "0", seq, kind, "0", run,
         "1000", 8000 * (seq + 1), "900", 7000 * (seq + 1), 30 + seq, 100 + seq + run]
        for seq, kind in CALLS
        for run in range(RUNS)
    ]
    path = directory / name
    record.write_tsv(path, [f"run: timing_mode=events", f"run: allocator={allocator}"],
                     COLUMNS.split(), rows)
    return path


def hbm_rows(directory, datasets=("tpch",)):
    rows = [
        [dataset, "40", "q6", "tp1-single", seq, kind, "0", seq, kind, "0", "0",
         4000 * (seq + 1), 3000 * (seq + 1), 7000 * (seq + 1), "40", "25", "5"]
        for dataset in datasets
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
        # The version the panels embed, stated where a reader looks first: a claim in a
        # docstring drifts, a line the writer prints is what the PNGs were made with.
        import matplotlib
        assert f"rendered with matplotlib {matplotlib.__version__}" in page, page[:600]
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


def test_two_datasets_are_two_series_in_every_pooled_panel():
    """tpch and tpcds at one sf are two machines' worth of plans, not one cloud of points.

    Read off the legend of each saved figure: a pooled panel that put both datasets in one
    colour would draw the same points and say nothing about which came from where.
    """
    import matplotlib.figure

    legends = {}
    save = matplotlib.figure.Figure.savefig

    def recording(fig, path, *args, **kwargs):
        legends[pathlib.Path(path).parent.name] = legends.get(
            pathlib.Path(path).parent.name, set()) | {
            text.get_text() for ax in fig.axes if ax.get_legend()
            for text in ax.get_legend().get_texts()}
        return save(fig, path, *args, **kwargs)

    matplotlib.figure.Figure.savefig = recording
    try:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            draw("--record", str(ten_row_record(tmp)),
                 "--record", str(ten_row_record(tmp, name="ds.tsv", dataset="tpcds")),
                 "--hbm", str(hbm_rows(tmp, datasets=("tpch", "tpcds"))),
                 "--out-dir", str(tmp / "plots"))
    finally:
        matplotlib.figure.Figure.savefig = save
    for section in ("load", "compute", "spread", "hbm"):
        assert legends.get(section) == {"tpch.sf40", "tpcds.sf40"}, (section, legends)


def test_a_kind_one_dataset_makes_is_drawn_in_that_datasets_run_wide_colour():
    """One colour means one dataset in every panel: a map per panel would hand the first
    colour to whichever dataset a panel happens to hold alone, so the panel of a kind only
    tpcds makes would draw tpcds in tpch's colour. Read off the saved figure: its points'
    colour, and the legend that says which series it is.
    """
    import matplotlib.colors
    import matplotlib.figure

    drawn = {}
    save = matplotlib.figure.Figure.savefig

    def recording(fig, path, *args, **kwargs):
        ax = fig.axes[0]
        drawn[pathlib.Path(path).name] = (
            {matplotlib.colors.to_hex(c) for col in ax.collections
             for c in col.get_facecolors()},
            [t.get_text() for t in ax.get_legend().get_texts()] if ax.get_legend() else [])
        return save(fig, path, *args, **kwargs)

    matplotlib.figure.Figure.savefig = recording
    try:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            tpcds = ten_row_record(tmp, name="ds.tsv", dataset="tpcds")
            tpcds.write_text(tpcds.read_text().replace("CudfCoalescePartitions", "CudfSort"))
            draw("--record", str(ten_row_record(tmp)), "--record", str(tpcds),
                 "--out-dir", str(tmp / "plots"))
    finally:
        matplotlib.figure.Figure.savefig = save
    palette = plot.colours({"tpch.sf40", "tpcds.sf40"})
    colours, legend = drawn["CudfSort.png"]
    assert colours == {matplotlib.colors.to_hex(palette["tpcds.sf40"])}, (colours, palette)
    assert legend == ["tpcds.sf40"], legend
    colours, legend = drawn["CudfCoalescePartitions.png"]
    assert colours == {matplotlib.colors.to_hex(palette["tpch.sf40"])}, (colours, palette)
    assert legend == ["tpch.sf40"], legend


def test_an_execution_file_and_its_per_call_aggregate_draw_the_same_hbm_points():
    """hbm.tsv was a row per execution and is a row per call; the panel draws one point per
    call either way, the median over its executions, so the two shapes draw one picture.
    """
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        rows = record.read_tsv(ten_row_record(tmp))
        per_execution = [
            {**dict(zip(HBM_COLUMNS.split(), [
                "tpch", "40", "q6", "tp1-single", str(seq), kind, "0", str(seq), kind, "0",
                str(run)])),
             "hbm_read_bytes": str(4000 * (seq + 1) + 100 * run),
             "hbm_write_bytes": str(3000 * (seq + 1)),
             "hbm_bytes": str(7000 * (seq + 1) + 100 * run),
             "samples": str(9 + 2 * run), "device_busy_us": str(25 + run),
             "device_idle_us": "5"}
            for seq, kind in CALLS
            for run in range(3)
        ]
        aggregate = record.hbm_per_call(per_execution)
        assert len(aggregate) == len(CALLS) and {r["executions"] for r in aggregate} == {"3"}
        # The second execution of each call is its median: run 1's bytes, 11 samples.
        assert [r["hbm_bytes"] for r in aggregate] == [
            str(7000 * (seq + 1) + 100) for seq, _ in CALLS]
        assert plot.hbm_points(per_execution, rows) == plot.hbm_points(aggregate, rows)


def test_a_fold_takes_rows_of_both_shapes_each_by_its_own():
    """A row per call passes through and rows a row per execution fold, row by row, so a
    reader given files of both shapes reads each as what it is."""
    per_call = {**dict(zip(record.HBM_CALL, (
        "tpch", "40", "q1", "tp1-single", "0", "CudfScan", "0", "0", "CudfScan", "0"))),
        "executions": "10", **{c: "5" for c in record.HBM_VALUES}}
    per_execution = [
        {**dict(zip(record.HBM_CALL, (
            "tpch", "40", "q6", "tp1-single", "0", "CudfScan", "0", "0", "CudfScan", "0"))),
         "run_index": str(run), **{c: str(v) for c in record.HBM_VALUES}}
        for run, v in enumerate((1, 7, 3))
    ]
    folded = record.hbm_per_call([per_call] + per_execution)
    assert [(r["query"], r["executions"], r["hbm_bytes"]) for r in folded] == [
        ("q1", "10", "5"), ("q6", "3", "3")], folded


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
