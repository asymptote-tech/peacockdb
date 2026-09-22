"""`nsys_calls.py` over a synthetic capture, for the three things it can get quietly wrong.

The first of them is the reason this file exists. Seq numbering restarts with every plan,
so q6 and q19 both open with `0.0 CudfScan`; keyed without the case, two cases merge into
one row set that says the scan ran twice and averages two queries' numbers into one row.
It is self-consistent, it has the right number of columns, and nothing downstream can tell.
"""

import pathlib
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import nsys_calls  # noqa: E402
import record  # noqa: E402
from capture import Capture  # noqa: E402
from harness import main, raises  # noqa: E402

Q6 = "tpch.sf40 q6 tp1-single"
Q19 = "tpch.sf40 q19 tp1-single"
SCAN = (0, 0, "CudfScan", ["read_parquet", "cast"])
FILTER = (1, 0, "CudfFilter", ["apply_boolean_mask"])


def two_case_capture(directory):
    """Two cases whose plans start with the same seq, kind and call index."""
    path = directory / "capture.sqlite"
    cap = Capture(path)
    cap.case(Q6, [SCAN, FILTER])
    cap.case(Q19, [SCAN])
    cap.close()
    return path


def calls_of(capture, out, *args):
    sys.argv = ["nsys_calls.py", "--capture", str(capture), "--out", str(out), *args]
    nsys_calls.main()
    return record.read_tsv(out)


def test_two_cases_stay_two_row_sets():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        rows = calls_of(two_case_capture(tmp), tmp / "calls.tsv")

    assert {r["query"] for r in rows} == {"q6", "q19"}
    scans = [r for r in rows if r["call"] == "read_parquet" and r["depth"] == "0"]
    assert sorted(r["query"] for r in scans) == ["q19", "q6"], scans
    # One execution each. Merged, the shared `0.0 CudfScan` would report two.
    assert {r["executions"] for r in scans} == {"1"}, scans
    # And the case columns are the record's own four, so a row of one file keys against a
    # row of the other without either restating the convention.
    assert [r["dataset"] for r in scans] == ["tpch", "tpch"]
    assert {r["sf"] for r in scans} == {"40"}
    assert {r["mode"] for r in scans} == {"tp1-single"}


PLANS = """peacockdb plan goldens

== q6
--- plan ---
  (not read by this check)
--- recipes ---
GpuUnload: calling_lanes=1, per handle: result_from_handle(batch, row range)
  GpuProject: calling_lanes=1, per batch: execute_node(#1 CudfProject, batch)
    GpuLoadParquet: calling_lanes=1, per batch: execute_scan_rowgroups(#0 CudfScan, row groups)
--- memory ---
"""


def test_a_bare_call_is_not_checked_against_the_kind_of_the_seq_it_was_handed():
    """The export and the slice publish no step, so a seq carries two kinds in a capture.

    `result_from_handle` is named by the seq of the node whose output it was handed — the
    same seq that node's own `execute_node` range carries — so a reader that takes one kind
    per seq reports the plan and the capture as disagreeing about a node they agree on.
    """
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        goldens = tmp / "goldens"
        goldens.mkdir()
        (goldens / "tp1-single.plans.txt").write_text(PLANS)
        path = tmp / "capture.sqlite"
        cap = Capture(path)
        cap.case(Q6, [(0, 0, "CudfScan", ["read_parquet"]),
                      (1, 0, "CudfProject", ["compute_column"]),
                      (1, 1, "result_from_handle", ["to_arrow"])])
        cap.close()
        rows = calls_of(path, tmp / "calls.tsv", "--plans-dir", str(goldens))

    exported = [r for r in rows if r["recipe_kind"] == "result_from_handle"]
    assert exported, "the export's own region is not in the breakdown"
    assert {r["recipe_seq"] for r in exported} == {"1"}, exported


def test_a_mode_with_no_golden_is_refused():
    """A check that silently covers less than the capture has stopped being a check.

    And a refusal leaves no file: a `calls.tsv` written before the check is a table of the
    capture the check was about to reject, carrying the name of one it accepted.
    """
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        (tmp / "goldens").mkdir()
        with raises(SystemExit, match="tp1-single"):
            calls_of(two_case_capture(tmp), tmp / "calls.tsv",
                     "--plans-dir", str(tmp / "goldens"))
        assert not (tmp / "calls.tsv").exists(), "the refused capture left a calls.tsv"


def test_a_region_driven_several_times_an_execution_is_not_a_disagreement():
    """A batched mode drives one seq once per batch, so occurrences are not executions.

    q6 at tp4-sized has regions at four calls an execution beside regions at one. Compared
    as raw occurrence counts those look like a run that died partway, and the whole capture
    is refused — which is what the first real trace pass did.
    """
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        path = tmp / "capture.sqlite"
        cap = Capture(path)
        for _ in range(2):
            cap.case(Q6, [SCAN, (1, 0, "CudfFilter", ["apply_boolean_mask"]),
                          (1, 1, "CudfFilter", ["apply_boolean_mask"])])
        cap.close()
        rows = calls_of(path, tmp / "calls.tsv")

    filters = [r for r in rows if r["recipe_kind"] == "CudfFilter" and r["depth"] == "0"]
    assert filters, rows
    assert {r["executions"] for r in filters} == {"2"}, filters


def test_regions_that_ran_different_numbers_of_times_are_refused():
    """Every region of one case runs once per execution, so a disagreement is a broken run.

    The capture below is a case executed twice, the second time stopping after the scan —
    which is what an execution that died partway leaves behind. Averaged over "executions"
    the filter would then be reported at half its cost with nothing saying so.
    """
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        path = tmp / "capture.sqlite"
        cap = Capture(path)
        cap.case(Q6, [SCAN, FILTER])
        cap.case(Q6, [SCAN])
        cap.close()
        with raises(SystemExit, match="disagree"):
            calls_of(path, tmp / "calls.tsv")
        assert not (tmp / "calls.tsv").exists(), "the refused capture left a calls.tsv"


if __name__ == "__main__":
    raise SystemExit(main(globals()))
