"""Row estimates (`cardinality.estimate`) against the engine's own CPU runs: every join of every
planned query in `tp1-single-mini.cpu.txt` — the whole sf1 dataset; `mini` is the budget the plans
were priced at, not a sample. q-error = max(estimate/true, true/estimate), floored at a row.

Each join is one line of a golden, `scripts/exec_model/testdata/goldens/<bench>/tp1-single.cardinality.txt`,
so a worse estimate is a named line in the diff and a new query is new lines. `UPDATE_CANONICAL=1`
rewrites it. One mode: every join the other four run as planned is one of tp1-single's, by query,
type, keys, joins below it and true rows — `test_cardinality_modes.py` holds them to it. A query the
engine did not run at the mode, or whose run's tree differs from its plan, is named in the header.
Needs `testdata/{tpch,tpcds}.sf1` for the footers only.
"""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import difflib
import os
import pathlib
import re
import statistics
from dataclasses import dataclass

from ..corpus import cpu_rows
from ..harness import main
from ..plans.test_engine_answers import ROOT
from ...optimizer.cardinality import estimate
from ...plans.engine_expr import parse_join_keys
from ...plans.engine_plan import EngineNode, read_plans
from ...optimizer.stats import Statistics

MODE = "tp1-single"
GOLDENS = pathlib.Path(__file__).resolve().parents[2] / "testdata" / "goldens"
JOINS = ("GpuHashJoin", "GpuNestedLoopJoin", "GpuCrossJoin")


@dataclass(frozen=True)
class JoinError:
    query: str
    ordinal: int  # its place among the query's joins, in pre-order from 1
    kind: str
    join_type: str
    keys: str
    joins: int  # joins in its subtree, itself included
    true: int
    estimate: float

    @property
    def q(self) -> float:
        est, true = max(self.estimate, 1.0), max(self.true, 1)
        return max(est / true, true / est)

    def line(self, query_width: int) -> str:
        # Every column but the query's has a fixed width, and that one is the bench's longest
        # name: a line moves when its join does or a longer name joins the corpus. One decimal
        # of a row and two of a ratio: a last-bit difference in `pow` between libms moves neither.
        join = f"{self.kind.removeprefix('Gpu').removesuffix('Join')}/{self.join_type}"
        return (f"{self.query:<{query_width}} {self.ordinal:>2} {join:<16} {self.joins:>2} {self.true:>10} "
                f"{self.estimate:>12.1f} {self.q:>8.2f}  {self.keys}")


def run_joins(bench: str, mode: str) -> tuple[dict, dict[str, list[str]]]:
    """Each query the engine ran at `mode` as planned, by name: its plan and its joins in pre-order
    with their true rows. And the queries left out: not run at the mode, or run as another tree."""
    truth = cpu_rows(ROOT / "goldens" / f"{bench}.sf1" / f"{mode}-mini.cpu.txt")
    found, skipped = {}, {"not run": [], "tree differs": []}
    for query, plan in read_plans(ROOT / "goldens" / f"{bench}.sf1" / f"{mode}.plans.txt").items():
        if not isinstance(plan, EngineNode):
            continue
        if not truth.get(query):
            skipped["not run"].append(query)
            continue
        nodes = list(_preorder(plan))
        if [n.kind for n in nodes] != [kind for kind, _ in truth[query]]:
            skipped["tree differs"].append(query)
            continue
        found[query] = plan, [(node, true) for node, (_, true) in zip(nodes, truth[query])
                              if node.kind in JOINS]
    return found, skipped


def described(node: EngineNode) -> tuple[str, str, str, int]:
    """A join as the golden names it: kind, type, keys by the plan's names, joins in its subtree."""
    keys = "-" if "on" not in node.fields else \
        ",".join(f"{b.name}={p.name}" for b, p in parse_join_keys(node.fields["on"]))
    return node.kind, node.fields.get("join_type", "Cross"), keys, _subtree_joins(node)


def join_errors(bench: str) -> tuple[list[JoinError], dict[str, list[str]]]:
    """Every join's error at `MODE`, and the queries left out."""
    stats = Statistics(ROOT / f"{bench}.sf1")
    found, skipped = run_joins(bench, MODE)
    errors = []
    for query, (plan, joins) in found.items():
        estimates = estimate(plan, stats)
        errors += [JoinError(query, ordinal, *described(node), true, estimates[id(node)].rows)
                   for ordinal, (node, true) in enumerate(joins, 1)]
    return errors, skipped


def _preorder(node):
    yield node
    for child in node.children:
        yield from _preorder(child)


def _subtree_joins(node) -> int:
    return (node.kind in JOINS) + sum(_subtree_joins(child) for child in node.children)


def _natural(query: str):
    return [int(part) if part.isdigit() else part for part in re.split(r"(\d+)", query)]


def golden_text(bench: str, errors: list[JoinError], skipped: dict[str, list[str]]) -> str:
    qs = [e.q for e in errors]
    rows = sorted(errors, key=lambda e: (_natural(e.query), e.ordinal))
    width = max(len(e.query) for e in errors)
    return "".join(line + "\n" for line in [
        f"# {bench} {MODE}: each join's estimate against its rows in {MODE}-mini.cpu.txt",
        f"# {len(qs)} joins: median q-error {statistics.median(qs):.2f}, "
        f"{100 * sum(q <= 2 for q in qs) / len(qs):.1f}% within 2x",
        *(f"# {why}: {' '.join(sorted(queries, key=_natural)) or '-'}" for why, queries in skipped.items()),
        "# n: the join's place among its query's, in pre-order; "
        "joins: the joins in its subtree, itself included",
        "# query n kind/type joins true estimate q-error keys",
        *(e.line(width) for e in rows),
    ])


def _matches_golden(bench: str):
    path = GOLDENS / bench / f"{MODE}.cardinality.txt"
    got = golden_text(bench, *join_errors(bench))
    if "UPDATE_CANONICAL" in os.environ:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(got)
        print(f"       wrote {path}")
        return
    regenerate = "Run with UPDATE_CANONICAL=1 over the sf1 tables to"
    assert path.exists(), f"golden not found: {path}\n{regenerate} generate it."
    want = path.read_text()
    diff = difflib.unified_diff(want.splitlines(), got.splitlines(), str(path), "estimated now",
                                n=0, lineterm="")
    assert got == want, "\n".join([*diff, f"{regenerate} regenerate it."])


def test_tpch_join_estimates_match_the_golden():
    _matches_golden("tpch")


def test_tpcds_join_estimates_match_the_golden():
    _matches_golden("tpcds")


def test_a_foreign_key_join_without_filters_is_exact():
    # `lineitem ⋈ orders` on the order key: every line finds its order. With NDV = rows, as
    # DataFusion has it, the estimate is a quarter of the truth.
    [join] = [e for e in join_errors("tpch")[0] if e.query == "hash-join"]
    assert join.true == 6_001_215 and join.q < 1.01, join


if __name__ == "__main__":
    raise SystemExit(main(globals()))
