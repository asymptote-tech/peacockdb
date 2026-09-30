"""Disassembly: a join order back into an engine plan — keys, residual and projection from the
MultiJoin's identities, wiring as the translator derives it — that answers as the plan it came
from did, in any order."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import pandas as pd

from .corpus import execute
from .harness import main, raises
from .test_multijoin import CHAIN, L, plan, scan
from ..disassembly import baseline, disassemble, reassembled
from ..engine_nodes import build
from ..multijoin import assemble, clusters, replaced

FRAMES = {
    "t1": pd.DataFrame({"a": [1, 2, 3, 4]}),
    "t2": pd.DataFrame({"b": [1, 2, 2, 3, 9]}),
    "t3": pd.DataFrame({"c": [2, 3, 3, 4]}),
}


class Tables:
    def frame(self, table, columns):
        return FRAMES[table][list(columns)].copy()

    def row_counts(self, table):
        return [len(FRAMES[table])]


def answer(tree) -> list:
    got, _ = execute(build(tree, Tables()))
    return sorted(map(tuple, got.to_numpy().tolist()))


def kinds(node, depth=0):
    yield "  " * depth + node.kind
    for child in node.children:
        yield from kinds(child, depth + 1)


def test_the_baseline_is_the_order_the_plan_has_and_disassembles_to_it():
    cluster = assemble(plan(CHAIN))
    assert baseline(cluster) == ((0, 1), 2)
    rebuilt = disassemble(cluster, baseline(cluster), lanes=1)
    assert list(kinds(rebuilt)) == list(kinds(cluster.root))
    assert rebuilt.schema == cluster.root.schema
    assert rebuilt.fields["on"] == "[(b@1, c@0)]" and rebuilt.fields["projection"] == "[a@0, c@2]"


def test_another_order_answers_the_same():
    unload = plan(f"GpuUnload\n" + "\n".join("  " + line for line in CHAIN.splitlines()))
    [cluster] = clusters(unload)
    for order in (((0, 1), 2), (0, (1, 2)), ((1, 2), 0), (2, (0, 1))):
        reordered = replaced(unload, cluster.root, disassemble(cluster, order, lanes=1))
        assert answer(reordered) == answer(unload) == [(2, 2), (2, 2), (3, 3), (3, 3)], order


def test_a_residual_goes_on_the_first_join_that_has_both_its_relations():
    text = CHAIN.replace("on=[(b@1, c@0)],", "on=[(b@1, c@0)], filter=(a@build:0 + c@probe:0) > 4,")
    cluster = assemble(plan(text))
    rebuilt = disassemble(cluster, (0, (1, 2)), lanes=1)
    # The probe side outputs what the cluster's output needs first, then its keys: (c, b).
    assert rebuilt.fields["filter"] == "(a@build:0 + c@probe:0) > 4"
    assert "filter" not in rebuilt.children[1].fields
    unload = plan("GpuUnload\n" + "\n".join("  " + line for line in text.splitlines()))
    [cluster] = clusters(unload)
    assert answer(replaced(unload, cluster.root, disassemble(cluster, (0, (1, 2)), lanes=1))) == \
        answer(unload) == [(3, 3), (3, 3)]


def test_every_cluster_is_reassembled_side_by_side_and_nested():
    # Two clusters under a union, the second with a third inside it, under an aggregate.
    nested = CHAIN.replace(
        "  GpuCoalesceAllBatches: lanes=1, batches=single, schema=[a:Int64, b:Int64]",
        "  GpuAggregateBatches: group_by=[a@0, b@1], aggs=[], lanes=1, batches=single, schema=[a:Int64, b:Int64]")
    union = "GpuUnion: lanes=2, batches=multiple, schema=[a:Int64, c:Int64]\n" + "\n".join(
        "  " + line for line in (CHAIN + "\n" + nested).splitlines())
    rebuilt = reassembled(plan(union), 1, lambda cluster: _swap(baseline(cluster)))
    # Every cluster came back with its sides swapped: each now builds from t3, the inner one
    # from t2.
    first, second = rebuilt.children
    assert [_table(first.children[0]), _table(second.children[0])] == ["t3", "t3"]
    inner = second.children[1].children[0]
    while inner.kind != "GpuHashJoin":
        inner = inner.children[0]
    assert _table(inner.children[0]) == "t2"


def _table(node):
    while node.kind != "GpuLoadParquet":
        node = node.children[0]
    return node.fields["table"]


def _swap(tree):
    return tree[1], tree[0]


def test_a_join_that_only_a_residual_connects_is_refused():
    text = CHAIN.replace("on=[(b@1, c@0)],", "on=[(b@1, c@0)], filter=(a@build:0 + c@probe:0) > 4,")
    cluster = assemble(plan(text))
    with raises(ValueError, match="no key"):
        disassemble(cluster, ((0, 2), 1), lanes=1)


def test_at_four_lanes_each_side_is_shuffled_on_its_keys_and_the_build_coalesced():
    four = CHAIN.replace("lanes=1", "lanes=4").replace("partition_groups=[[[0]]]", "partition_groups=[[[0]],[],[],[]]")
    cluster = assemble(plan(four))
    rebuilt = disassemble(cluster, (2, (0, 1)), lanes=4)
    build, probe = rebuilt.children
    assert [build.kind, build.children[0].kind, build.children[0].children[0].kind] == [
        "GpuCoalesceAllBatches", "GpuEmitPartitions", "GpuMergePartitions"]
    assert build.children[0].fields["hash"] == "[c@0]" and build.fields["hashed_on"] == "[c@0]"
    assert probe.kind == "GpuEmitPartitions" and probe.fields["hash"] == "[b@1]"
    assert rebuilt.fields["lanes"] == "4"


if __name__ == "__main__":
    raise SystemExit(main(globals()))
