"""Row estimates (`cardinality.estimate`) against the engine's own CPU runs: every join of every
planned query in a mode's `<mode>-mini.cpu.txt` — the whole sf1 dataset; `mini` is the budget the
plans were priced at, not a sample. q-error = max(estimate/true, true/estimate), floored at a row.

The step's bar is the median over tpch within 2×. The rest holds the estimator where it stands,
so a change to the formulas that loses ground goes red. A query whose tree in the golden differs
from its plan's is left out, by name. Needs `testdata/{tpch,tpcds}.sf1` for the footers only.
`PCK_MODE` picks the golden (default `tp1-single`).
"""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import os
import statistics
from dataclasses import dataclass

from ..corpus import cpu_rows
from ..harness import main
from ..plans.test_engine_answers import ROOT
from ...optimizer.cardinality import estimate
from ...plans.engine_plan import EngineNode, read_plans
from ...optimizer.stats import Statistics

MODE = os.environ.get("PCK_MODE", "tp1-single")
JOINS = ("GpuHashJoin", "GpuNestedLoopJoin", "GpuCrossJoin")


@dataclass(frozen=True)
class JoinError:
    query: str
    join_type: str
    depth: int  # joins in its subtree, itself included
    true: int
    estimate: float

    @property
    def q(self) -> float:
        est, true = max(self.estimate, 1.0), max(self.true, 1)
        return max(est / true, true / est)


def join_errors(bench: str, mode: str = MODE) -> tuple[list[JoinError], list[str]]:
    """Every join's error, and the queries left out because their trees differ."""
    truth = cpu_rows(ROOT / "goldens" / f"{bench}.sf1" / f"{mode}-mini.cpu.txt")
    stats = Statistics(ROOT / f"{bench}.sf1")
    found, skipped = [], []
    for query, plan in read_plans(ROOT / "goldens" / f"{bench}.sf1" / f"{mode}.plans.txt").items():
        if not isinstance(plan, EngineNode) or query not in truth:
            continue
        nodes = list(_preorder(plan))
        if [n.kind for n in nodes] != [kind for kind, _ in truth[query]]:
            skipped.append(query)
            continue
        estimates = estimate(plan, stats)
        for node, (_, true) in zip(nodes, truth[query]):
            if node.kind in JOINS:
                found.append(JoinError(query, node.fields.get("join_type", "Cross"), _depth(node), true,
                                       estimates[id(node)].rows))
    return found, skipped


def _preorder(node):
    yield node
    for child in node.children:
        yield from _preorder(child)


def _depth(node) -> int:
    return (node.kind in JOINS) + sum(_depth(child) for child in node.children)


def _summary(errors: list[JoinError]) -> tuple[float, float]:
    qs = [e.q for e in errors]
    return statistics.median(qs), sum(q <= 2 for q in qs) / len(qs)


def test_tpch_join_estimates_keep_their_accuracy():
    errors, skipped = join_errors("tpch")
    median, within = _summary(errors)
    print(f"       tpch {MODE}: {len(errors)} joins, median {median:.2f}, {within:.0%} within 2x, skipped {skipped}")
    assert median <= 1.2 and within >= 0.65, (median, within)


def test_tpcds_join_estimates_keep_their_accuracy():
    errors, skipped = join_errors("tpcds")
    median, within = _summary(errors)
    print(f"       tpcds {MODE}: {len(errors)} joins, median {median:.2f}, {within:.0%} within 2x, skipped {skipped}")
    assert median <= 1.3 and within >= 0.7, (median, within)


def test_a_foreign_key_join_without_filters_is_exact():
    # `lineitem ⋈ orders` on the order key: every line finds its order. With NDV = rows, as
    # DataFusion has it, the estimate is a quarter of the truth.
    [join] = [e for e in join_errors("tpch")[0] if e.query == "hash-join"]
    assert join.true == 6_001_215 and join.q < 1.01, join


if __name__ == "__main__":
    raise SystemExit(main(globals()))
