"""The cardinality golden covers one mode, tp1-single, because the other four run no join it lacks:
every join they run as planned is one of tp1-single's by query, kind, type, keys, joins in its
subtree and true rows; and none of them runs a query as another tree than planned, whose joins
would go uncompared. Text only — the plan goldens and `-mini.cpu.txt` — so it needs no dataset.
"""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

from ..harness import main
from .test_cardinality_corpus import MODE, described, run_joins

OTHER_MODES = ("tp1-rowgroup", "tp4-single", "tp4-rowgroup", "tp4-sized")


def run_join_shapes(bench: str, mode: str) -> tuple[set[tuple], list[str]]:
    """The joins run as planned at `mode`, and the queries whose run's tree differs from the plan."""
    found, skipped = run_joins(bench, mode)
    shapes = {(query, *described(node), true) for query, (_, joins) in found.items() for node, true in joins}
    return shapes, skipped["tree differs"]


def _covered_by_tp1_single(bench: str):
    covered, _ = run_join_shapes(bench, MODE)
    for mode in OTHER_MODES:
        shapes, differs = run_join_shapes(bench, mode)
        # Such a query's joins are never compared, so it would leave the guard silently.
        assert not differs, f"{bench} {mode}: run as another tree than planned, joins uncompared: {differs}"
        assert shapes, f"{bench} {mode}: no join run as planned"
        extra = sorted(shapes - covered)
        assert not extra, (f"{bench} {mode} runs joins {MODE} does not, so the cardinality golden "
                           f"misses them: {extra}")


def test_every_tpch_mode_runs_only_joins_tp1_single_runs():
    _covered_by_tp1_single("tpch")


def test_every_tpcds_mode_runs_only_joins_tp1_single_runs():
    _covered_by_tp1_single("tpcds")


if __name__ == "__main__":
    raise SystemExit(main(globals()))
