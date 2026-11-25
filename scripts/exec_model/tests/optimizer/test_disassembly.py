"""Disassembly: a join order back into an engine plan — keys, residual and projection from the
MultiJoin's identities, wiring as the translator derives it — that answers as the plan it came
from did, in any order."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import pathlib
import re

import pandas as pd

from ..corpus import execute
from ..harness import main, raises
from .test_multijoin import CHAIN, F, L, MS, Q23, Q23_TP4, SHUFFLED, _final_aggregate, _indented, plan, scan
from ...operators.join_types import NULL_KEY_DROPPABLE, JoinType
from ...optimizer.disassembly import baseline, disassemble, planned_lanes, reassembled
from ...plans.engine_nodes import build
from ...plans.engine_plan import plan_text
from ...optimizer.multijoin import Edge, MultiJoin, RelColumn, Relation, assemble, clusters, named, replaced

FRAMES = {
    "t1": pd.DataFrame({"a": [1, 2, 3, 4]}),
    "t2": pd.DataFrame({"b": [1, 2, 2, 3, 9]}),
    "t3": pd.DataFrame({"c": [2, 3, 3, 4]}),
    "ms": pd.DataFrame({"a": [1, 2, 3]}),
    "cust": pd.DataFrame({"c": [1, 2, 3]}),
    "ss": pd.DataFrame({"sc": [1, 2, 2, 5], "q": [10, 20, 30, 40]}),
    "t": pd.DataFrame({"a": [5, 6]}),
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


def test_a_join_that_no_edge_crosses_is_refused():
    with raises(ValueError, match="no key"):
        disassemble(assemble(plan(CHAIN)), ((0, 2), 1), lanes=1)


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


def test_a_join_holding_null_equal_to_null_is_rebuilt_holding_it():
    text = CHAIN.replace("on=[(a@0, b@0)],", "on=[(a@0, b@0)], null_equals_null=true,")
    cluster = assemble(plan(text))
    rebuilt = disassemble(cluster, baseline(cluster), lanes=1)
    lower = rebuilt.children[0].children[0]
    assert lower.fields.get("null_equals_null") == "true" and "null_equals_null" not in rebuilt.fields


def test_edges_that_disagree_on_null_equality_are_not_joined_as_one():
    # t3 joins t1 and t2 both; t1 and t2 hold NULL equal. Joining t1 to t3 first leaves both
    # flags to the second join.
    text = CHAIN.replace("on=[(a@0, b@0)],", "on=[(a@0, b@0)], null_equals_null=true,") \
        .replace("on=[(b@1, c@0)],", "on=[(b@1, c@0), (a@0, c@0)],")
    cluster = assemble(plan(text))
    with raises(ValueError, match="NULL"):
        disassemble(cluster, ((0, 2), 1), lanes=1)


def test_reassembly_puts_the_null_key_filter_below_each_shuffle_of_a_key_the_plan_filtered():
    [cluster] = clusters(plan(SHUFFLED))
    assert list(kinds(disassemble(cluster, baseline(cluster), lanes=4))) == list(kinds(cluster.root))
    # t3 builds and t1 ⋈ t2 probes on b: the probe is shuffled on b, nullable, so the filter
    # goes below that shuffle; t3's c was never filtered, so its shuffle has none.
    build, probe = disassemble(cluster, (2, (0, 1)), lanes=4).children
    assert probe.kind == "GpuEmitPartitions" and probe.children[0].kind == "GpuFilter"
    assert probe.children[0].fields["predicate"] == "b@1 IS NOT NULL"
    assert "GpuFilter" not in "".join(kinds(build))


def test_a_side_whose_unmatched_rows_are_emitted_gets_no_null_key_filter():
    # As the planner lays a Left join out: its preserved build shuffled on b with no filter, though
    # b can hold a NULL (t2's shuffle filters it); its probe through the filter on c.
    left = plan(f"""GpuHashJoin: join_type=Left, on=[(b@1, c@0)], projection=[a@0, c@2], {F}, schema=[a:Int64, c:Int64]
  GpuCoalesceAllBatches: lanes=4, batches=single, hashed_on=[b@1], schema=[a:Int64, b:Int64]
    GpuEmitPartitions: hash=[b@1], {F}, hashed_on=[b@1], schema=[a:Int64, b:Int64]
      GpuMergePartitions: {L}, schema=[a:Int64, b:Int64]
        GpuHashJoin: join_type=Inner, on=[(a@0, b@0)], {F}, hashed_on=[a@0], schema=[a:Int64, b:Int64]
          GpuCoalesceAllBatches: lanes=4, batches=single, hashed_on=[a@0], schema=[a:Int64]
            GpuEmitPartitions: hash=[a@0], {F}, hashed_on=[a@0], schema=[a:Int64]
{scan("t1", "a", indent=14)}
          GpuEmitPartitions: hash=[b@0], {F}, hashed_on=[b@0], schema=[b:Int64]
            GpuFilter: predicate=b@0 IS NOT NULL, {L}, schema=[b:Int64]
{scan("t2", "b", indent=14)}
  GpuEmitPartitions: hash=[c@0], {F}, hashed_on=[c@0], schema=[c:Int64]
    GpuFilter: predicate=c@0 IS NOT NULL, {L}, schema=[c:Int64]
{scan("t3", "c", indent=6)}""")
    [cluster] = clusters(left)
    rebuilt = disassemble(cluster, baseline(cluster), lanes=4)
    assert list(kinds(rebuilt)) == list(kinds(cluster.root))
    build, probe = rebuilt.children
    assert build.children[0].children[0].kind == "GpuMergePartitions"
    assert probe.children[0].kind == "GpuFilter"


def test_the_null_key_drop_table_is_the_engines():
    source = (pathlib.Path(__file__).resolve().parents[4] / "peacockdb-core/src/plan/join.rs").read_text()
    body = source[source.index("fn null_key_droppable"):]
    body = body[:body.index("\n}\n")]
    engine = {}
    for arm, build, probe in re.findall(r"^\s*(JoinType::[\w| :]+?) => \((true|false), (true|false)\)", body, re.M):
        for name in re.findall(r"JoinType::(\w+)", arm):
            engine[name] = (build == "true", probe == "true")
    names = {t: "".join(part.title() for part in t.name.split("_")) for t in JoinType}
    assert {names[t]: v for t, v in NULL_KEY_DROPPABLE.items()} == engine, engine


def test_a_full_join_s_output_is_hashed_on_neither_side():
    # Either side's key is NULL where the join padded it, so neither hashes the output.
    full = f"""GpuHashJoin: join_type=Full, on=[(a@0, b@0)], lanes=4, batches=multiple, schema=[a:Int64, b:Int64]
  GpuCoalesceAllBatches: lanes=4, batches=single, hashed_on=[a@0], schema=[a:Int64]
    GpuEmitPartitions: hash=[a@0], lanes=4, batches=multiple, hashed_on=[a@0], schema=[a:Int64]
{scan("t1", "a", indent=6)}
  GpuEmitPartitions: hash=[b@0], lanes=4, batches=multiple, hashed_on=[b@0], schema=[b:Int64]
{scan("t2", "b", indent=4)}"""
    cluster = assemble(plan(full))
    rebuilt = disassemble(cluster, baseline(cluster), lanes=4)
    assert rebuilt.fields["join_type"] == "Full" and "hashed_on" not in rebuilt.fields


def test_the_baseline_of_a_one_row_cluster_is_the_plan_order():
    root = plan(Q23)
    [cluster] = clusters(root)
    assert baseline(cluster) == (0, (1, 2))
    # the project's pick goes on the join, as every rebuilt join takes it
    folded = Q23.replace(f"""  GpuProject: exprs=[q@2, c@0], {L}, schema=[q:Int64, c:Int64]
    GpuHashJoin: join_type=Inner, on=[(c@0, sc@0)], {L}, schema=[c:Int64, sc:Int64, q:Int64]
      GpuCoalesceAllBatches: lanes=1, batches=single, schema=[c:Int64]
{scan("cust", "c", indent=8)}
{scan("ss", "sc", "q", indent=6)}""", f"""  GpuHashJoin: join_type=Inner, on=[(c@0, sc@0)], projection=[q@2, c@0], {L}, schema=[q:Int64, c:Int64]
    GpuCoalesceAllBatches: lanes=1, batches=single, schema=[c:Int64]
{scan("cust", "c", indent=6)}
{scan("ss", "sc", "q", indent=4)}""")
    assert folded != Q23
    assert plan_text(reassembled(root, 1)) == plan_text(plan(folded))


def _unloaded(text):
    unload = named(plan("GpuUnload\n" + _indented(text, 2)))
    [cluster] = clusters(unload)
    return unload, cluster


# (max_store_sales m, store_sales q, customer c): ms counts three rows, ss's sc 5 is no customer's
Q23_ANSWER = [(3, 10, 1), (3, 20, 2), (3, 30, 2)]


def test_a_cross_join_is_rebuilt_on_one_lane():
    unload, cluster = _unloaded(Q23)
    rebuilt = disassemble(cluster, ((0, 1), 2), lanes=1)
    assert rebuilt.kind == "GpuHashJoin" and rebuilt.fields["on"] == "[(c@1, sc@0)]"
    assert rebuilt.schema == cluster.root.schema
    coalesce = rebuilt.children[0]
    cross = coalesce.children[0]
    assert [coalesce.kind, cross.kind] == ["GpuCoalesceAllBatches", "GpuCrossJoin"]
    one_row = cross.children[0]
    assert one_row is cluster.relations[0].root
    assert (one_row.fields["lanes"], one_row.fields["batches"]) == ("1", "single")
    assert _table(cross.children[1]) == "cust"
    assert (cross.fields["lanes"], cross.fields["batches"]) == ("1", "multiple")
    assert "hashed_on" not in cross.fields and "projection" not in cross.fields
    assert cross.mj == "MJ1"
    assert answer(replaced(unload, cluster.root, rebuilt)) == answer(unload) == Q23_ANSWER


def test_at_four_lanes_the_cross_join_merges_and_the_root_comes_back_to_one_lane():
    unload, cluster = _unloaded(Q23_TP4)
    rebuilt = disassemble(cluster, ((0, 1), 2), lanes=4)
    assert rebuilt.kind == "GpuMergePartitions" and rebuilt.fields["lanes"] == "1"
    assert rebuilt.schema == cluster.root.schema
    join = rebuilt.children[0]
    assert join.kind == "GpuHashJoin" and join.fields["lanes"] == "4"
    coalesce, emit = join.children[0], join.children[0].children[0]
    assert [coalesce.kind, emit.kind] == ["GpuCoalesceAllBatches", "GpuEmitPartitions"]
    assert emit.fields["hash"] == "[c@1]" and emit.fields["lanes"] == "4"
    cross = emit.children[0]
    assert cross.kind == "GpuCrossJoin" and cross.fields["lanes"] == "1" and "hashed_on" not in cross.fields
    assert [side.fields["lanes"] for side in cross.children] == ["1", "1"]
    assert answer(replaced(unload, cluster.root, rebuilt)) == answer(unload) == Q23_ANSWER


def test_a_cross_join_merges_sides_on_more_lanes_onto_one():
    # the one-row relation emitted onto four lanes, customer scanned on four
    four = Q23_TP4.replace(MS, f"""  GpuEmitPartitions: hash=[m@0], {F}, hashed_on=[m@0], schema=[m:Int64]
{_indented(MS, 2)}""").replace(
        scan("cust", "c", indent=12),
        f"            GpuMergePartitions: {L}, schema=[c:Int64]\n"
        + scan("cust", "c", indent=14).replace("partition_groups=[[[0]]], lanes=1",
                                               "partition_groups=[[[0]],[],[],[]], lanes=4"))
    unload, cluster = _unloaded(four)
    assert [int(r.root.fields["lanes"]) for r in cluster.relations] == [4, 4, 1]
    rebuilt = disassemble(cluster, ((0, 1), 2), lanes=4)
    cross = rebuilt.children[0].children[0].children[0].children[0]
    build, probe = cross.children
    assert [build.kind, build.children[0].kind, build.children[0].children[0].kind] == [
        "GpuCoalesceAllBatches", "GpuMergePartitions", "GpuEmitPartitions"]
    assert [probe.kind, probe.children[0].kind] == ["GpuMergePartitions", "GpuLoadParquet"]
    assert answer(replaced(unload, cluster.root, rebuilt)) == Q23_ANSWER


def test_a_key_edge_makes_a_hash_join_whatever_one_row_edges_cross():
    unload, cluster = _unloaded(Q23)
    rebuilt = disassemble(cluster, (1, (0, 2)), lanes=1)
    # customer builds on c; the cross join of max_store_sales onto store_sales probes on sc
    assert rebuilt.kind == "GpuHashJoin" and rebuilt.fields["on"] == "[(c@0, sc@2)]"
    cross = rebuilt.children[1]
    assert cross.kind == "GpuCrossJoin" and cross.fields["projection"] == "[m@0, q@2, sc@1]"
    assert answer(replaced(unload, cluster.root, rebuilt)) == Q23_ANSWER


def test_a_join_crossing_in_a_one_row_relation_keeps_the_lanes_the_plan_had():
    # at tp4 the planner kept customer ⋈ store_sales on one lane
    [cluster] = clusters(plan(Q23))
    rebuilt = disassemble(cluster, ((0, 1), 2), lanes=4)
    assert rebuilt.kind == "GpuHashJoin" and rebuilt.fields["lanes"] == "1"


def test_a_cross_join_of_two_larger_sides_is_refused():
    # t1 keys the one-row relation's count, so only that relation's edge joins t1 to t2. Assembly
    # walls such a cross join; built by hand, the cluster admits it.
    keyed = plan(f"""GpuHashJoin: join_type=Inner, on=[(a@0, n@0)], {L}, schema=[a:Int64, n:Int64, b:Int64]
  GpuCoalesceAllBatches: lanes=1, batches=single, schema=[a:Int64]
{scan("t1", "a", indent=4)}
  GpuCrossJoin: lanes=1, batches=multiple, schema=[n:Int64, b:Int64]
{_final_aggregate(indent=4)}
{scan("t2", "b", indent=4)}""")
    cross = keyed.children[1]
    relations = (keyed.children[0].children[0], *cross.children)
    edges = (Edge(0b001, 0b010, ((RelColumn(0, 0), RelColumn(1, 0)),)), Edge(0b010, 0b001), Edge(0b010, 0b100))
    cluster = MultiJoin(keyed, tuple(Relation(r) for r in relations), edges,
                        (RelColumn(0, 0), RelColumn(1, 0), RelColumn(2, 0)), (keyed, cross), None,
                        one_row_relations=0b010)
    with raises(ValueError, match="more than one row"):
        disassemble(cluster, ((0, 1), 2), lanes=1)


# a one-row relation crossed in below a hash join on four lanes, whose hash a node above reads
CROSSED_BELOW = f"""GpuHashJoin: join_type=Inner, on=[(a@0, b@1)], {F}, hashed_on=[a@0], schema=[a:Int64, n:Int64, b:Int64]
  GpuCoalesceAllBatches: lanes=4, batches=single, hashed_on=[a@0], schema=[a:Int64]
    GpuEmitPartitions: hash=[a@0], {F}, hashed_on=[a@0], schema=[a:Int64]
{scan("t1", "a", indent=6)}
  GpuEmitPartitions: hash=[b@1], {F}, hashed_on=[b@1], schema=[n:Int64, b:Int64]
    GpuCrossJoin: lanes=1, batches=multiple, schema=[n:Int64, b:Int64]
{_final_aggregate(indent=6)}
{scan("t2", "b", indent=6)}"""


def test_planned_lanes_leave_one_row_relations_out():
    [cluster] = clusters(plan(Q23_TP4))
    assert planned_lanes(cluster) == {0b110: 4}
    # crossed in below the hash join: the join keeps its four lanes under the mask without it
    [cluster] = clusters(plan(CROSSED_BELOW))
    assert cluster.one_row_relations == 0b010
    assert planned_lanes(cluster) == {0b101: 4}


def test_a_cross_join_rebuilt_on_top_is_shuffled_back_onto_the_hash_the_root_had():
    [cluster] = clusters(plan(CROSSED_BELOW))
    assert cluster.hash_read_above
    rebuilt = disassemble(cluster, (1, (0, 2)), lanes=4)
    assert rebuilt.kind == "GpuEmitPartitions" and rebuilt.fields["hash"] == "[a@0]"
    assert (rebuilt.fields["lanes"], rebuilt.fields["hashed_on"]) == ("4", "[a@0]")
    assert rebuilt.children[0].kind == "GpuCrossJoin" and rebuilt.schema == cluster.root.schema
    # run under a merge onto one lane, which reads no hash
    merged = plan(f"GpuUnload\n  GpuMergePartitions: {L}, schema=[a:Int64, n:Int64, b:Int64]\n"
                  + _indented(CROSSED_BELOW, 4))
    root = merged.children[0].children[0]
    assert answer(replaced(merged, root, rebuilt)) == answer(merged) == [
        (1, 2, 1), (2, 2, 2), (2, 2, 2), (3, 2, 3)]


if __name__ == "__main__":
    raise SystemExit(main(globals()))
