"""`run.py` over the sf1 tables: two small tpch queries and a refused tpcds one at tp4-single, by two
workers, into a temp directory, each optimized answer held to the planned one, the costs, the
report and the markdown page written; then one of them again, alone. Needs the generated sf1
tables and the DPhyp library, so it runs in dataset-matrix, not the cheap tier."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import pathlib
import subprocess
import sys
import tempfile

from . import corpus
from .harness import main
from ..optimizer.report_md import preamble
from ..optimizer.report_text import ESTIMATES, NAMES
from ..run import ROOT, sections

RUN = pathlib.Path(__file__).resolve().parents[1] / "run.py"
MODE = "tp4-single"
QUERIES = ["filter-project", "join-int", "q27"]
KINDS = ("cpu", "optimizer", "costs", "plandiff")


def run(out: pathlib.Path, *queries: str) -> None:
    subprocess.run([sys.executable, str(RUN), "--mode", MODE, "--jobs", "2", "--out", str(out),
                    "--query", *queries], check=True, timeout=600)


def test_run_writes_every_file_per_bench_and_the_report_and_a_filtered_run_keeps_the_other_sections():
    with tempfile.TemporaryDirectory() as scratch:
        out = pathlib.Path(scratch)
        run(out, *QUERIES)
        files = {(bench, kind): (out / bench / f"{MODE}.{kind}.txt").read_text()
                 for bench in ("tpch", "tpcds") for kind in KINDS}
        for kind in KINDS:
            assert [name for name, _ in sections(files["tpch", kind])] == ["filter-project", "join-int"]
            assert files["tpcds", kind] == "== q27\nskipped: refused by datafusion: SanityCheckPlan\n"
        pages = {bench: (out / bench / f"{MODE}.optimizer.md").read_text() for bench in ("tpch", "tpcds")}
        assert [name for name, _ in sections(pages["tpch"], "## ")] == ["filter-project", "join-int"]
        assert pages["tpcds"] == preamble("tpcds", MODE) + "## q27\n\nskipped: refused by datafusion: SanityCheckPlan\n\n"
        # join-int's one join keeps its order: a line, and no table.
        assert dict(sections(pages["tpch"], "## "))["join-int"].startswith("\nMJ1 kept the plan's order: `")
        optimizer = dict(sections(files["tpch", "optimizer"]))
        assert optimizer["filter-project"] == "nothing fired\n"
        assert optimizer["join-int"].startswith(ESTIMATES + "\n" + NAMES + "\nMJ1: 2 relations, ordered before the run\n")
        assert optimizer["join-int"].endswith("plan unchanged\n")
        # The plan's diff is a file of its own; the log says where.
        assert dict(sections(files["tpch", "plandiff"])) == {"filter-project": "plan unchanged\n",
                                                             "join-int": "plan unchanged\n"}
        # Neither plan changed: the engine's tree, its answer's rows and each join's rows. A partial
        # aggregate emits per batch, so its rows follow the backend's batching and are not compared.
        engine = corpus.cpu_rows(ROOT / "goldens" / "tpch.sf1" / f"{MODE}-mini.cpu.txt")
        ours = corpus.cpu_rows(out / "tpch" / f"{MODE}.cpu.txt")
        for query in ("filter-project", "join-int"):
            assert [kind for kind, _ in ours[query]] == [kind for kind, _ in engine[query]], query
            assert ours[query][0] == engine[query][0], query
            assert [n for n in ours[query] if "Join" in n[0]] == [n for n in engine[query] if "Join" in n[0]], query
        assert all(body.startswith("early_exit=") for _, body in sections(files["tpch", "cpu"]))
        # Where nothing fired the optimized run is the planned one, byte for byte.
        planned, optimized = dict(sections(files["tpch", "costs"]))["filter-project"].splitlines()[:2]
        assert planned.startswith("planned peacockdb_cost=") and optimized == "optimized" + planned[len("planned"):]
        page = (out / "cost_report.html").read_text()
        assert all(f"<td>{query}</td>" in page for query in QUERIES), page[:300]

        # The query DPhyp is called for, alone: its sections made again byte for byte, the
        # other one's left.
        run(out, "join-int")
        for kind in KINDS:
            assert (out / "tpch" / f"{MODE}.{kind}.txt").read_text() == files["tpch", kind]
        assert (out / "tpch" / f"{MODE}.optimizer.md").read_text() == pages["tpch"]


if __name__ == "__main__":
    raise SystemExit(main(globals()))
