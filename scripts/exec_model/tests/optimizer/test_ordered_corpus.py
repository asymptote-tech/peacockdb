"""Every golden plan at every mode ordered by the optimizer, without running it: the plan builds and
validates — a hash join at more than one lane refuses a side not hashed on its keys — and every
final aggregate at more than one lane gets its rows laid out hashed on some of its group keys, so no
group is split across lanes. Checked on the finished plan, whatever decided the shuffles. Needs the
sf1 tables, for their statistics, and the DPhyp library."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

from ..harness import main
from ..plans.test_engine_corpus import GOLDENS, Shapes, prototype_layout
from ...engine.plan import Plan
from ...optimizer.join_order import ordered
from ...optimizer.multijoin import clusters
from ...optimizer.stats import Statistics
from ...plans.engine_expr import ColumnRef, parse_exprs
from ...plans.engine_nodes import build
from ...plans.engine_plan import EngineNode, read_plans

MODES = ("tp1-single", "tp1-rowgroup", "tp4-single", "tp4-rowgroup", "tp4-sized")


def split_groups(plan: EngineNode) -> list[str]:
    """Each final aggregate of `plan` at more than one lane whose input is not hashed on a subset of
    its group keys, as the prototype lays the input out."""
    prototype = build(plan, Shapes())
    Plan.build(prototype)
    found, pairs = [], [(plan, prototype)]
    while pairs:
        engine, node = pairs.pop()
        pairs.extend(zip(engine.children, node.children()))
        if engine.kind.startswith("GpuAggregate") and "final" in engine.fields \
                and int(engine.fields["lanes"]) > 1 and engine.fields.get("group_by", "[]") != "[]":
            keys = {key.index for key in parse_exprs(engine.fields["group_by"]) if isinstance(key, ColumnRef)}
            hashed = set(prototype_layout(node.children()[0])[2])
            if not hashed or not hashed <= keys:
                found.append(f"group_by {sorted(keys)} over input hashed on {sorted(hashed)}")
    return found


def _test(bench: str, at_least: int):
    def test():
        stats = Statistics(GOLDENS.parent / f"{bench}.sf1")
        problems, checked = [], 0
        for mode in MODES:
            lanes = int(mode[2])  # tp1-… / tp4-…
            for name, plan in read_plans(GOLDENS / f"{bench}.sf1" / f"{mode}.plans.txt").items():
                if not isinstance(plan, EngineNode) or not clusters(plan):
                    continue
                checked += 1
                try:
                    problems += [(mode, name, p) for p in split_groups(ordered(plan, stats, lanes)[0])]
                except Exception as refusal:  # a plan that will not build is a problem like any other
                    problems.append((mode, name, f"{type(refusal).__name__}: {refusal}"[:200]))
        assert checked >= at_least, checked
        assert not problems, (len(problems), problems[:5])

    test.__name__ = f"test_every_{bench}_plan_ordered_builds_and_splits_no_aggregate_s_groups"
    return test


for _bench, _plans in (("tpch", 150), ("tpcds", 350)):
    _found = _test(_bench, _plans)
    globals()[_found.__name__] = _found

if __name__ == "__main__":
    raise SystemExit(main(globals()))
