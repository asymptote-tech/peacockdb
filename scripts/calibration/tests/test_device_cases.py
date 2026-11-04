"""`device_cases.py`: the scripts' one reader of a device's `cases.csv`, held to Rust's.

`parse_device_cases` in `test_support` owns the format. Both readers read the committed
`testdata/fixtures/device-cases.csv`, and each asserts the same values off it, so a drift fails the
drifting side: a comma in a reason, a `wrong` row read as no case.
"""

import pathlib
import subprocess
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import device_cases  # noqa: E402
from harness import main  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
FIXTURE = REPO / "testdata" / "fixtures" / "device-cases.csv"
SCRIPT = pathlib.Path(device_cases.__file__).resolve()


def case(query, modes, status, note, dataset="tpch"):
    return {"dataset": dataset, "sf": "40", "query": query, "modes": modes,
            "status": status, "note": note}


def cli(*args):
    return subprocess.run([sys.executable, str(SCRIPT), *map(str, args)],
                          capture_output=True, text=True, check=False)


def test_the_shared_fixture_reads_as_rust_reads_it():
    # The literal of `the_shared_fixture_reads_as_expected` (`device_cases/tests.rs`).
    assert device_cases.rows(FIXTURE) == [
        case("q1", ["tp1_single", "tp4_sized"], "ok", ""),
        case("aggregate_groupby", ["tp1_single"], "wrong", "#7"),
        case("q9", [], "skipped", "timeout: the case ran past 1800 s, and was killed"),
        case("q37", ["tp1_single"], "ok", "", dataset="tpcds"),
    ]


def test_cli_prints_ok_and_wrong_rows():
    done = cli(FIXTURE)
    assert done.returncode == 0, done.stderr
    assert done.stdout == ("tpch\t40\tq1\ttp1_single|tp4_sized\n"
                           "tpch\t40\taggregate_groupby\ttp1_single\n"
                           "tpcds\t40\tq37\ttp1_single\n")


def test_cli_mode_and_dataset_narrow():
    by_mode = cli(FIXTURE, "--mode", "tp4_sized")
    assert by_mode.returncode == 0, by_mode.stderr
    assert by_mode.stdout == "tpch\t40\tq1\ttp1_single|tp4_sized\n"
    by_dataset = cli(FIXTURE, "--dataset", "tpcds")
    assert by_dataset.returncode == 0, by_dataset.stderr
    assert by_dataset.stdout == "tpcds\t40\tq37\ttp1_single\n"
    both = cli(FIXTURE, "--dataset", "tpcds", "--mode", "tp4_sized")
    assert (both.returncode, both.stdout) == (0, ""), both.stderr


def test_a_refused_row_exits_1_naming_it():
    with tempfile.TemporaryDirectory() as tmp:
        path = pathlib.Path(tmp) / "cases.csv"
        path.write_text(device_cases.HEADER + "\ntpch,40,q1,tp1_single,wrong,7\n")
        done = cli(path)
    assert done.returncode == 1, (done.returncode, done.stdout, done.stderr)
    assert done.stdout == ""
    assert "line 2" in done.stderr and "tpch,40,q1,tp1_single,wrong,7" in done.stderr, done.stderr
    assert "#NN" in done.stderr, done.stderr


def test_rows_refuse_what_rust_refuses():
    refused = {
        "dataset,sf,query,modes,status\n": "the first line is not",
        "tpch,40,q9,tp1_single,skipped,too large\n": "a skipped row times no mode",
        "tpch,40,q9,,skipped,\n": "names its reason",
        "tpch,40,q1,,ok,\n": "names the modes it ran",
        "tpch,40,q1,tp1_single,ok,#7\n": "an ok row has no note",
        "tpch,40,q6,tp1-single,ok,\n": "unknown mode",
        "tpch,40,q1,tp1_single,passed,\n": "unknown status",
        "tpch,40,q6\n": "not six fields",
        "tpch,40,q1,tp1_single,ok,\ntpch,40,q1,tp4_sized,ok,\n": "listed twice, first at line 2",
    }
    with tempfile.TemporaryDirectory() as tmp:
        path = pathlib.Path(tmp) / "cases.csv"
        for body, why in refused.items():
            text = body if body.startswith("dataset") else device_cases.HEADER + "\n" + body
            path.write_text(text)
            try:
                device_cases.rows(path)
            except SystemExit as caught:
                assert why in str(caught.code), (body, caught.code)
            else:
                raise AssertionError(f"{body!r} was read")


def test_a_note_keeps_its_commas():
    with tempfile.TemporaryDirectory() as tmp:
        path = pathlib.Path(tmp) / "cases.csv"
        path.write_text(device_cases.HEADER + "\ntpch,40,q9,,skipped,error: a, b\n")
        assert device_cases.rows(path) == [case("q9", [], "skipped", "error: a, b")]


if __name__ == "__main__":
    raise SystemExit(main(globals()))
