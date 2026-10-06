"""Every golden plan with every cluster disassembled in the order it has, without data.

The plan builds, validates, and each node declares the layout the engine prints for it — the
nodes disassembly made as much as those it kept. Each cluster is the nodes it was, in the same
order, but for two things disassembly does not reproduce: the projects between joins, which
fold into the joins' projections, and a shuffle the plan made only to merge it into one lane
under a join that collects its build (DataFusion's `CollectLeft`).
"""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

from ..harness import main
from ..plans.test_engine_corpus import GOLDENS, Shapes, engine_layout, prototype_layout
from ...optimizer.disassembly import baseline, disassemble, reassembled
from ...plans.engine_nodes import build
from ...plans.engine_plan import EngineNode, read_plans
from ...optimizer.multijoin import clusters
from ...engine.plan import Plan


def without_discarded_shuffles(node, parent=None) -> list[str]:
    """Kinds in pre-order, projects left out, and a shuffle a merge right above it undoes — the
    emit with the merge and coalesce `shuffled` put under it, and the merge too where what it
    merges is one lane."""
    if node.kind == "GpuMergePartitions" and node.children[0].kind == "GpuEmitPartitions" \
            and _shuffled(node.children[0]).fields.get("lanes") == "1":
        return without_discarded_shuffles(_shuffled(node.children[0]), node)
    if node.kind == "GpuEmitPartitions" and parent is not None and parent.kind == "GpuMergePartitions":
        return without_discarded_shuffles(_shuffled(node), parent)
    found = [] if node.kind == "GpuProject" else [node.kind]
    for child in node.children:
        found += without_discarded_shuffles(child, node)
    return found


def _shuffled(emit):
    below = emit.children[0]
    while below.kind in ("GpuMergePartitions", "GpuCoalesceAllBatches"):
        below = below.children[0]
    return below


def test_every_cluster_disassembles_to_a_plan_the_engine_would_lay_out_the_same():
    wrong, reshaped, checked = [], [], 0
    for path in sorted(GOLDENS.glob("*/*.plans.txt")):
        lanes = int(path.name[2])  # tp1-… / tp4-…
        for name, plan in read_plans(path).items():
            if not isinstance(plan, EngineNode) or not clusters(plan):
                continue
            for cluster in clusters(plan):
                checked += 1
                # Both sides: a relation is carried over whole, shuffles inside it and all.
                rebuilt = disassemble(cluster, baseline(cluster), lanes)
                if without_discarded_shuffles(rebuilt) != without_discarded_shuffles(cluster.root):
                    reshaped.append((path.name, name))
            whole = reassembled(plan, lanes)
            pairs = [(whole, build(whole, Shapes()))]
            Plan.build(pairs[0][1])
            while pairs:
                engine, prototype = pairs.pop()
                pairs.extend(zip(engine.children, prototype.children()))
                if engine.kind != "GpuUnload" and engine_layout(engine) != prototype_layout(prototype):
                    wrong.append((path.name, name, engine.kind, engine_layout(engine), prototype_layout(prototype)))
    assert checked > 1000, checked
    assert not reshaped, reshaped[:5]
    assert not wrong, (len(wrong), wrong[:5])


if __name__ == "__main__":
    raise SystemExit(main(globals()))
