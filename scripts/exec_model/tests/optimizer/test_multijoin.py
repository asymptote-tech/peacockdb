"""MultiJoin assembly: which nodes make a cluster, which root a relation, and every key, residual
and output column lifted to the relation column it is."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import pyarrow as pa

from ..harness import main
from .test_cardinality import dataset
from ...plans.engine_expr import Binary
from ...plans.engine_plan import parse_plans, plan_text
from ...optimizer.join_order import ordered
from ...optimizer.multijoin import Edge, RelColumn, assemble, at_most_one_row, clusters, named

L = "lanes=1, batches=multiple"


def scan(table, *columns, indent):
    projections = ", ".join(f"{c}@{i}" for i, c in enumerate(columns))
    schema = ", ".join(f"{c}:Int64" for c in columns)
    return (" " * indent + f"GpuLoadParquet: table={table}, projections=[{projections}], "
            f"partition_groups=[[[0]]], {L}, schema=[{schema}]")


def plan(text):
    return parse_plans(f"== q\n{text}\n", "test")["q"]


# t1(a) ⋈ t2(b) on a = b, under a coalesce, then ⋈ t3(c) on b = c, projected to (a, c)
CHAIN = f"""GpuHashJoin: join_type=Inner, on=[(b@1, c@0)], projection=[a@0, c@2], {L}, schema=[a:Int64, c:Int64]
  GpuCoalesceAllBatches: lanes=1, batches=single, schema=[a:Int64, b:Int64]
    GpuHashJoin: join_type=Inner, on=[(a@0, b@0)], {L}, schema=[a:Int64, b:Int64]
      GpuCoalesceAllBatches: lanes=1, batches=single, schema=[a:Int64]
{scan("t1", "a", indent=8)}
{scan("t2", "b", indent=6)}
{scan("t3", "c", indent=2)}"""


def test_a_chain_lifts_every_key_and_output_to_its_relation():
    cluster = assemble(plan(CHAIN))
    assert [r.root.fields["table"] for r in cluster.relations] == ["t1", "t2", "t3"]
    assert cluster.edges == (Edge(0b001, 0b010, ((RelColumn(0, 0), RelColumn(1, 0)),)),
                             Edge(0b010, 0b100, ((RelColumn(1, 0), RelColumn(2, 0)),)))
    assert cluster.output == (RelColumn(0, 0), RelColumn(2, 0))


def test_keys_between_one_pair_of_relations_are_one_edge_and_others_their_own():
    # q5's shape: supplier joins lineitem on the supplier and customer on the nation.
    text = f"""GpuHashJoin: join_type=Inner, on=[(s@0, ls@1), (sn@1, cn@0)], {L}, schema=[s:Int64, sn:Int64, cn:Int64, ls:Int64]
{scan("sup", "s", "sn", indent=2)}
  GpuHashJoin: join_type=Inner, on=[(ck@0, lk@0)], projection=[cn@1, ls@3], {L}, schema=[cn:Int64, ls:Int64]
{scan("cust", "ck", "cn", indent=4)}
{scan("line", "lk", "ls", indent=4)}"""
    edges = assemble(plan(text)).edges
    assert edges[1:] == (Edge(0b001, 0b100, ((RelColumn(0, 0), RelColumn(2, 1)),)),
                         Edge(0b001, 0b010, ((RelColumn(0, 1), RelColumn(1, 1)),)))
    composite = text.replace("on=[(s@0, ls@1), (sn@1, cn@0)]", "on=[(s@0, ls@1), (sn@1, ls@1)]")
    [_, whole] = assemble(plan(composite)).edges
    assert whole.keys == ((RelColumn(0, 0), RelColumn(2, 1)), (RelColumn(0, 1), RelColumn(2, 1)))


def test_a_residual_is_an_edge_over_the_relations_it_reads():
    text = CHAIN.replace("on=[(b@1, c@0)],", "on=[(b@1, c@0)], filter=a@build:0 > c@probe:0,")
    residual = assemble(plan(text)).edges[-1]
    assert (residual.left, residual.right, residual.keys) == (0b001, 0b100, ())
    assert isinstance(residual.residual, Binary) and residual.residual.op == ">"


def test_a_filter_a_mark_join_and_a_computing_project_each_root_a_relation():
    filtered = CHAIN.replace(
        "  GpuCoalesceAllBatches: lanes=1, batches=single, schema=[a:Int64, b:Int64]",
        "  GpuFilter: predicate=a@0 IS NULL, lanes=1, batches=multiple, schema=[a:Int64, b:Int64]")
    assert [r.root.kind for r in assemble(plan(filtered)).relations] == ["GpuFilter", "GpuLoadParquet"]
    # A mark join's mark column is no relation's.
    mark = CHAIN.replace("join_type=Inner, on=[(a@0, b@0)], {L}, schema=[a:Int64, b:Int64]".format(L=L),
                         f"join_type=LeftMark, on=[(a@0, b@0)], {L}, schema=[a:Int64, b:Boolean]")
    mark_relations = assemble(plan(mark)).relations
    assert [r.root.kind for r in mark_relations] == ["GpuHashJoin", "GpuLoadParquet"]
    assert mark_relations[0].root.fields["join_type"] == "LeftMark"
    computing = CHAIN.replace(
        "  GpuCoalesceAllBatches: lanes=1, batches=single, schema=[a:Int64, b:Int64]",
        "  GpuProject: exprs=[a@0, b@1 + 1 as b], lanes=1, batches=multiple, schema=[a:Int64, b:Int64]")
    assert [r.root.kind for r in assemble(plan(computing)).relations] == ["GpuProject", "GpuLoadParquet"]


def test_a_non_inner_join_with_a_residual_joins_the_cluster_its_residual_on_its_edge():
    # The engine plans and runs a residual on every join type.
    semi = plan(f"""GpuHashJoin: join_type=Inner, on=[(a@0, c@0)], {L}, schema=[a:Int64, c:Int64]
  GpuCoalesceAllBatches: lanes=1, batches=single, schema=[a:Int64]
    GpuHashJoin: join_type=LeftSemi, on=[(a@0, b@0)], filter=a@build:0 < b@probe:0, {L}, schema=[a:Int64]
      GpuCoalesceAllBatches: lanes=1, batches=single, schema=[a:Int64]
{scan("t1", "a", indent=8)}
{scan("t2", "b", indent=6)}
{scan("t3", "c", indent=2)}""")
    cluster = assemble(semi)
    assert [r.root.fields["table"] for r in cluster.relations] == ["t1", "t2", "t3"]
    [semi_edge] = [e for e in cluster.edges if e.residual is not None]
    assert (semi_edge.left, semi_edge.right) == (0b001, 0b010) and semi_edge.residual.op == "<"


# CHAIN at four lanes as the planner lays it out: every side shuffled on its key, and b, which can
# hold a NULL, through the null-key filter the planner puts below each shuffle on it
F = "lanes=4, batches=multiple"
SHUFFLED = f"""GpuHashJoin: join_type=Inner, on=[(b@1, c@0)], projection=[a@0, c@2], {F}, schema=[a:Int64, c:Int64]
  GpuCoalesceAllBatches: lanes=4, batches=single, hashed_on=[b@1], schema=[a:Int64, b:Int64]
    GpuEmitPartitions: hash=[b@1], {F}, hashed_on=[b@1], schema=[a:Int64, b:Int64]
      GpuFilter: predicate=b@1 IS NOT NULL, {L}, schema=[a:Int64, b:Int64]
        GpuMergePartitions: {L}, schema=[a:Int64, b:Int64]
          GpuHashJoin: join_type=Inner, on=[(a@0, b@0)], {F}, hashed_on=[a@0], schema=[a:Int64, b:Int64]
            GpuCoalesceAllBatches: lanes=4, batches=single, hashed_on=[a@0], schema=[a:Int64]
              GpuEmitPartitions: hash=[a@0], {F}, hashed_on=[a@0], schema=[a:Int64]
{scan("t1", "a", indent=16)}
            GpuEmitPartitions: hash=[b@0], {F}, hashed_on=[b@0], schema=[b:Int64]
              GpuFilter: predicate=b@0 IS NOT NULL, {L}, schema=[b:Int64]
{scan("t2", "b", indent=16)}
  GpuEmitPartitions: hash=[c@0], {F}, hashed_on=[c@0], schema=[c:Int64]
{scan("t3", "c", indent=4)}"""


def test_a_null_key_filter_under_a_shuffle_is_wiring_the_cluster_reads_through():
    # tp4 holds the cluster tp1 does: the filter the planner puts below a shuffle splits
    # nothing, and the cluster keeps the column it tested.
    [cluster] = clusters(plan(SHUFFLED))
    assert [r.root.fields["table"] for r in cluster.relations] == ["t1", "t2", "t3"]
    assert cluster.nullable_keys == frozenset({RelColumn(1, 0)})


def test_a_filter_below_a_shuffle_on_a_column_it_does_not_hash_roots_a_relation():
    # A query's own `WHERE w IS NOT NULL` over a one-lane input reaches the shuffle as it is; only
    # a filter on the shuffle's own keys is the planner's.
    text = f"""GpuHashJoin: join_type=Inner, on=[(a@0, k@0)], {F}, schema=[a:Int64, k:Int64, w:Int64]
  GpuCoalesceAllBatches: lanes=4, batches=single, hashed_on=[a@0], schema=[a:Int64]
    GpuEmitPartitions: hash=[a@0], {F}, hashed_on=[a@0], schema=[a:Int64]
{scan("t1", "a", indent=6)}
  GpuEmitPartitions: hash=[k@0], {F}, hashed_on=[k@0], schema=[k:Int64, w:Int64]
    GpuFilter: predicate=w@1 IS NOT NULL, {L}, schema=[k:Int64, w:Int64]
{scan("t2", "k", "w", indent=6)}"""
    assert [r.root.kind for r in assemble(plan(text)).relations] == ["GpuLoadParquet", "GpuFilter"]


def test_a_key_filter_on_a_side_whose_unmatched_rows_are_emitted_roots_a_relation():
    # A Left join emits its build's unmatched rows, so the planner drops no NULL key there: a filter
    # on that side's key is the query's.
    left = SHUFFLED.replace("GpuHashJoin: join_type=Inner, on=[(b@1, c@0)]", "GpuHashJoin: join_type=Left, on=[(b@1, c@0)]")
    outer, inner = clusters(plan(left))
    assert [r.root.kind for r in outer.relations] == ["GpuFilter", "GpuLoadParquet"]
    assert [r.root.fields["table"] for r in inner.relations] == ["t1", "t2"]


def test_a_key_filter_under_a_join_that_equates_nulls_roots_a_relation():
    # There a NULL key matches a NULL, so the planner drops none: a filter on the key is the query's.
    equal = SHUFFLED.replace("on=[(b@1, c@0)],", "on=[(b@1, c@0)], null_equals_null=true,")
    outer, inner = clusters(plan(equal))
    assert [r.root.kind for r in outer.relations] == ["GpuFilter", "GpuLoadParquet"]
    assert outer.nullable_keys == frozenset()
    assert [r.root.fields["table"] for r in inner.relations] == ["t1", "t2"]


def test_a_null_key_filter_on_a_column_the_join_does_not_key_roots_a_relation():
    # The join keys its build on a; the filter tests b, which a shuffle further down hashes, so a
    # row it drops for a NULL b is one the join could emit.
    text = f"""GpuHashJoin: join_type=Inner, on=[(a@0, c@0)], {F}, schema=[a:Int64, b:Int64, c:Int64]
  GpuCoalesceAllBatches: lanes=4, batches=single, hashed_on=[a@0], schema=[a:Int64, b:Int64]
    GpuEmitPartitions: hash=[a@0], {F}, hashed_on=[a@0], schema=[a:Int64, b:Int64]
      GpuMergePartitions: {L}, schema=[a:Int64, b:Int64]
        GpuEmitPartitions: hash=[b@1], {F}, hashed_on=[b@1], schema=[a:Int64, b:Int64]
          GpuFilter: predicate=b@1 IS NOT NULL, {L}, schema=[a:Int64, b:Int64]
{scan("t1", "a", "b", indent=12)}
  GpuEmitPartitions: hash=[c@0], {F}, hashed_on=[c@0], schema=[c:Int64]
{scan("t3", "c", indent=4)}"""
    assert [r.root.kind for r in assemble(plan(text)).relations] == ["GpuFilter", "GpuLoadParquet"]


def test_a_cluster_inside_a_relation_is_found_on_its_own():
    nested = CHAIN.replace(
        "  GpuCoalesceAllBatches: lanes=1, batches=single, schema=[a:Int64, b:Int64]",
        "  GpuAggregateBatches: group_by=[a@0, b@1], aggs=[], lanes=1, batches=single, schema=[a:Int64, b:Int64]")
    outer, inner = clusters(plan(nested))
    assert [r.root.kind for r in outer.relations] == ["GpuAggregateBatches", "GpuLoadParquet"]
    assert [r.root.fields["table"] for r in inner.relations] == ["t1", "t2"]


# CHAIN with t1 an aggregate over t1 ⋈ t4: two clusters, the outer one of two joins.
TWO = f"""GpuHashJoin: join_type=Inner, on=[(b@1, c@0)], projection=[a@0, c@2], {L}, schema=[a:Int64, c:Int64]
  GpuCoalesceAllBatches: lanes=1, batches=single, schema=[a:Int64, b:Int64]
    GpuHashJoin: join_type=Inner, on=[(a@0, b@0)], {L}, schema=[a:Int64, b:Int64]
      GpuAggregateBatches: group_by=[a@0], aggs=[], lanes=1, batches=single, schema=[a:Int64]
        GpuHashJoin: join_type=Inner, on=[(a@0, d@0)], projection=[a@0], {L}, schema=[a:Int64]
{scan("t1", "a", indent=10)}
{scan("t4", "d", indent=10)}
{scan("t2", "b", indent=6)}
{scan("t3", "c", indent=2)}"""


def test_each_cluster_is_named_in_plan_order_outermost_first_on_every_join_it_holds():
    tree = named(plan(TWO))
    assert [(n.kind, n.mj) for n in _walk(tree) if n.mj or "Join" in n.kind] == [
        ("GpuHashJoin", "MJ1"), ("GpuHashJoin", "MJ1"), ("GpuHashJoin", "MJ2")]
    assert [c.mj for c in clusters(tree)] == ["MJ1", "MJ2"]
    assert named(tree) is tree


def test_a_cluster_inside_a_relation_reads_its_hash_where_the_enclosing_join_may_key_on_it():
    # t1 joined to a computing project over t2 ⋈ t3, the project hashed on its first column.
    def nested(hashed: str):
        return plan(f"""GpuHashJoin: join_type=Inner, on=[(a@0, b@0)], {L}, schema=[a:Int64, b:Int64, c:Int64]
{scan("t1", "a", indent=2)}
  GpuProject: exprs=[b@0, c@1 + 1 as c], lanes=1, batches=multiple, hashed_on=[{hashed}], schema=[b:Int64, c:Int64]
    GpuHashJoin: join_type=Inner, on=[(b@0, c@0)], {L}, schema=[b:Int64, c:Int64]
{scan("t2", "b", indent=6)}
{scan("t3", "c", indent=6)}""")

    assert [c.hash_read_above for c in clusters(nested("b@0"))] == [False, True]
    assert [c.hash_read_above for c in clusters(nested("c@1"))] == [False, False]


def test_the_hash_a_plan_s_root_declares_is_read_by_what_consumes_the_plan():
    # A probe plan is a build side: the join its build feeds reads the hash its root declares.
    def side(hashed: str):
        return plan(f"""GpuCoalesceAllBatches: lanes=1, batches=single{hashed}, schema=[b:Int64, c:Int64]
  GpuHashJoin: join_type=Inner, on=[(b@0, c@0)], {L}, schema=[b:Int64, c:Int64]
{scan("t2", "b", indent=4)}
{scan("t3", "c", indent=4)}""")

    assert [c.hash_read_above for c in clusters(side(", hashed_on=[b@0]"))] == [True]
    assert [c.hash_read_above for c in clusters(side(""))] == [False]


def test_a_cross_join_a_nested_loop_join_and_a_mark_join_carry_no_id_and_a_filtered_semi_join_one():
    text = f"""GpuCrossJoin: {L}, schema=[a:Int64, c:Int64]
  GpuNestedLoopJoin: join_type=Inner, filter=a@build:0 < b@probe:0, projection=[a@0], {L}, schema=[a:Int64]
{scan("t1", "a", indent=4)}
{scan("t2", "b", indent=4)}
  GpuHashJoin: join_type=LeftSemi, on=[(c@0, d@0)], filter=c@build:0 < d@probe:0, {L}, schema=[c:Int64]
{scan("t3", "c", indent=4)}
    GpuHashJoin: join_type=LeftMark, on=[(d@0, e@0)], projection=[d@0], {L}, schema=[d:Int64]
{scan("t4", "d", indent=6)}
{scan("t5", "e", indent=6)}"""
    tree = named(plan(text))
    assert [(n.kind, n.mj) for n in _walk(tree) if "Join" in n.kind] == [
        ("GpuCrossJoin", None), ("GpuNestedLoopJoin", None), ("GpuHashJoin", "MJ1"), ("GpuHashJoin", None)]


def _one_row(text):
    return at_most_one_row(plan(text))


def _final_aggregate(kind="GpuAggregateBatches", group_by="[]", indent=0, lanes=1, batches="single", table="t"):
    pad = " " * indent
    return (f"{pad}{kind}: group_by={group_by}, aggs=[count(1) as n], final=[n@0], lanes={lanes}, "
            f"batches={batches}, schema=[n:Int64]\n{scan(table, 'a', indent=indent + 2)}")


def _partial_aggregate(lanes, indent=0, batches=None):
    # q23 at tp4 has the four-lane partial aggregate single-batch, tpch q22 at tp1 the one-lane multiple
    batches = batches or ("single" if lanes > 1 else "multiple")
    return (f"{' ' * indent}GpuAggregate: group_by=[], aggs=[count(1) as n], lanes={lanes}, "
            f"batches={batches}, schema=[n:Int64]\n{scan('t', 'a', indent=indent + 2)}")


def test_a_final_aggregate_without_group_by_is_one_row():
    assert _one_row(_final_aggregate())
    # q23 at tp1 runs the final aggregate itself, on one lane
    assert _one_row(_final_aggregate("GpuAggregate"))


def test_a_partial_aggregate_is_not_one_row():
    # a partial aggregate emits a row per input batch: tpch q22 at tp1, q23 at tp4 on four lanes
    assert not _one_row(_partial_aggregate(lanes=4))
    assert not _one_row(_partial_aggregate(lanes=1))
    # only the missing `final` rules this one out: one lane, one batch
    assert not _one_row(_partial_aggregate(lanes=1, batches="single"))
    merged = f"GpuMergePartitions: lanes=1, batches=single, schema=[n:Int64]\n{_partial_aggregate(4, 2)}"
    assert not _one_row(merged)


def test_fetch_and_empty_are_at_most_one_row():
    def limit(fetch):
        return (f"GpuLimit: skip=0, fetch={fetch}, lanes=1, batches=single, schema=[a:Int64]\n"
                f"{scan('t', 'a', indent=2)}")

    assert _one_row(limit(1)) and _one_row(limit(0))
    assert not _one_row(limit(2))
    assert _one_row("GpuEmpty: lanes=1, batches=single, schema=[a:Int64]")


def test_a_fetch_per_lane_or_per_batch_is_not_one_row():
    def sort(layout):
        return (f"GpuSort: by=[a@0 asc nulls_first], fetch=1, {layout}, schema=[a:Int64]\n"
                f"{scan('t', 'a', indent=2)}")

    assert not _one_row(sort("lanes=4, batches=single"))
    assert not _one_row(sort("lanes=1, batches=multiple"))
    assert _one_row(sort("lanes=1, batches=single"))
    assert not _one_row(_final_aggregate(lanes=4))
    assert not _one_row(_final_aggregate(batches="multiple"))


def test_what_passes_one_row_on_is_one_row():
    final = _final_aggregate(indent=2)
    schema = "lanes=1, batches=single, schema=[n:Int64]"
    assert _one_row(f"GpuFilter: predicate=n@0 > 1, {schema}\n{final}")
    assert _one_row(f"GpuProject: exprs=[n@0 + 1 as n], {schema}\n{final}")
    assert _one_row(f"GpuCoalesceAllBatches: {schema}\n{final}")
    assert _one_row(f"GpuMergePartitions: {schema}\n{final}")
    # tpch q15 at tp4 hashes the one row onto four lanes
    emit = "GpuEmitPartitions: hash=[n@0], lanes=4, batches=multiple, hashed_on=[n@0], schema=[n:Int64]"
    assert _one_row(f"{emit}\n{final}")
    scanned = scan("t", "a", indent=2)
    assert not _one_row(f"GpuFilter: predicate=a@0 > 1, {schema}\n{scanned}")
    assert not _one_row(f"GpuProject: exprs=[a@0 + 1 as a], {schema}\n{scanned}")
    cross = f"GpuCrossJoin: lanes=1, batches=single, schema=[n:Int64, a:Int64]\n{final}\n"
    assert _one_row(cross + _final_aggregate(indent=2))
    assert not _one_row(cross + scan("t2", "a", indent=2))


def test_scans_joins_and_grouped_aggregates_are_not_one_row():
    assert not _one_row(scan("t", "a", indent=0))
    assert not _one_row(CHAIN)
    assert not _one_row(_final_aggregate(group_by="[a@0]"))


def test_an_edge_without_keys_or_residual_is_one_row():
    key = (RelColumn(0, 0), RelColumn(1, 0))
    assert Edge(0b01, 0b10).one_row
    assert not Edge(0b01, 0b10, (key,)).one_row
    assert not Edge(0b01, 0b10, residual=object()).one_row


# tpcds q23's best_ss_customer at tp1: the one-row max_store_sales crossed onto customer ⋈ store_sales
# — its final aggregate reads one batch, as q23's does, so the prototype runs it as one
MS = f"""  GpuProject: exprs=[n@0 as m], lanes=1, batches=single, schema=[m:Int64]
    GpuAggregate: group_by=[], aggs=[count(1) as n], final=[n@0], lanes=1, batches=single, schema=[n:Int64]
      GpuCoalesceAllBatches: lanes=1, batches=single, schema=[a:Int64]
{scan("ms", "a", indent=8)}"""
Q23 = f"""GpuCrossJoin: lanes=1, batches=multiple, schema=[m:Int64, q:Int64, c:Int64]
{MS}
  GpuProject: exprs=[q@2, c@0], {L}, schema=[q:Int64, c:Int64]
    GpuHashJoin: join_type=Inner, on=[(c@0, sc@0)], {L}, schema=[c:Int64, sc:Int64, q:Int64]
      GpuCoalesceAllBatches: lanes=1, batches=single, schema=[c:Int64]
{scan("cust", "c", indent=8)}
{scan("ss", "sc", "q", indent=6)}"""
# at tp4: the hash join on four lanes, merged onto one for the cross join
Q23_TP4 = f"""GpuCrossJoin: lanes=1, batches=multiple, schema=[m:Int64, q:Int64, c:Int64]
{MS}
  GpuMergePartitions: {L}, schema=[q:Int64, c:Int64]
    GpuProject: exprs=[q@2, c@0], {F}, hashed_on=[c@1], schema=[q:Int64, c:Int64]
      GpuHashJoin: join_type=Inner, on=[(c@0, sc@0)], {F}, hashed_on=[c@0], schema=[c:Int64, sc:Int64, q:Int64]
        GpuCoalesceAllBatches: lanes=4, batches=single, hashed_on=[c@0], schema=[c:Int64]
          GpuEmitPartitions: hash=[c@0], {F}, hashed_on=[c@0], schema=[c:Int64]
{scan("cust", "c", indent=12)}
        GpuEmitPartitions: hash=[sc@0], {F}, hashed_on=[sc@0], schema=[sc:Int64, q:Int64]
          GpuFilter: predicate=sc@0 IS NOT NULL, {L}, schema=[sc:Int64, q:Int64]
{scan("ss", "sc", "q", indent=12)}"""


def test_a_one_row_cross_join_joins_the_cluster_below_it():
    root = plan(Q23)
    [cluster] = clusters(root)
    assert cluster.root is root and cluster.relations[0].root is root.children[0]
    assert [r.root.fields.get("table") for r in cluster.relations[1:]] == ["cust", "ss"]
    assert cluster.one_row_relations == 0b001
    assert cluster.edges == (Edge(0b010, 0b100, ((RelColumn(1, 0), RelColumn(2, 0)),)),
                             Edge(0b001, 0b010), Edge(0b001, 0b100))
    assert cluster.output == (RelColumn(0, 0), RelColumn(2, 1), RelColumn(1, 0))


def test_a_one_row_cross_join_below_a_hash_join_is_in_its_cluster():
    text = f"""GpuHashJoin: join_type=Inner, on=[(a@0, b@1)], {L}, schema=[a:Int64, n:Int64, b:Int64]
{scan("t1", "a", indent=2)}
  GpuCrossJoin: lanes=1, batches=multiple, schema=[n:Int64, b:Int64]
{_final_aggregate(indent=4)}
{scan("t2", "b", indent=4)}"""
    [cluster] = clusters(plan(text))
    assert len(cluster.relations) == 3 and cluster.relations[1].root.kind == "GpuAggregateBatches"
    assert cluster.one_row_relations == 0b010
    assert cluster.edges == (Edge(0b001, 0b100, ((RelColumn(0, 0), RelColumn(2, 0)),)),
                             Edge(0b010, 0b001), Edge(0b010, 0b100))


def test_a_cross_join_s_projection_is_lifted_through():
    text = Q23.replace("GpuCrossJoin: lanes=1, batches=multiple, schema=[m:Int64, q:Int64, c:Int64]",
                       "GpuCrossJoin: projection=[c@2, m@0], lanes=1, batches=multiple, schema=[c:Int64, m:Int64]")
    [cluster] = clusters(plan(text))
    assert cluster.output == (RelColumn(1, 0), RelColumn(0, 0))


# a key join of ta and tb under one or two one-row relations
AB = f"""GpuHashJoin: join_type=Inner, on=[(a@0, b@0)], {L}, schema=[a:Int64, b:Int64]
{scan("ta", "a", indent=2)}
{scan("tb", "b", indent=2)}"""


def _indented(text, by):
    return "\n".join(" " * by + line for line in text.splitlines())


def test_two_one_row_relations_crossed_in_one_after_the_other_share_one_edge():
    text = f"""GpuCrossJoin: lanes=1, batches=multiple, schema=[n:Int64, n:Int64, a:Int64, b:Int64]
{_final_aggregate(indent=2, table="t1")}
  GpuCrossJoin: lanes=1, batches=multiple, schema=[n:Int64, a:Int64, b:Int64]
{_final_aggregate(indent=4, table="t2")}
{_indented(AB, 4)}"""
    [cluster] = clusters(plan(text))
    assert len(cluster.relations) == 4 and cluster.one_row_relations == 0b0011
    assert cluster.edges[1:] == (Edge(0b0001, 0b0010), Edge(0b0001, 0b0100), Edge(0b0001, 0b1000),
                                 Edge(0b0010, 0b0100), Edge(0b0010, 0b1000))


def test_a_cross_join_of_two_one_row_relations_is_one_relation():
    text = f"""GpuCrossJoin: lanes=1, batches=multiple, schema=[n:Int64, n:Int64, a:Int64, b:Int64]
  GpuCrossJoin: lanes=1, batches=multiple, schema=[n:Int64, n:Int64]
{_final_aggregate(indent=4, table="t1")}
{_final_aggregate(indent=4, table="t2")}
{_indented(AB, 2)}"""
    [cluster] = clusters(plan(text))
    assert [r.root.kind for r in cluster.relations] == ["GpuCrossJoin", "GpuLoadParquet", "GpuLoadParquet"]
    assert cluster.one_row_relations == 0b001
    assert cluster.edges == (Edge(0b010, 0b100, ((RelColumn(1, 0), RelColumn(2, 0)),)),
                             Edge(0b001, 0b010), Edge(0b001, 0b100))


def test_cross_joins_of_one_row_relations_alone_form_no_cluster():
    # tpcds q88's shape: a cluster needs a hash join
    text = f"""GpuCrossJoin: lanes=1, batches=multiple, schema=[n:Int64, n:Int64, n:Int64]
{_final_aggregate(indent=2, table="t1")}
  GpuCrossJoin: lanes=1, batches=multiple, schema=[n:Int64, n:Int64]
{_final_aggregate(indent=4, table="t2")}
{_final_aggregate(indent=4, table="t3")}"""
    assert clusters(plan(text)) == []


def test_a_one_row_cross_join_beside_a_non_inner_join_stays_a_wall():
    semi = plan(f"""GpuCrossJoin: lanes=1, batches=multiple, schema=[m:Int64, c:Int64]
{MS}
  GpuProject: exprs=[c@0], {L}, schema=[c:Int64]
    GpuHashJoin: join_type=LeftSemi, on=[(c@0, sc@0)], {L}, schema=[c:Int64]
      GpuCoalesceAllBatches: lanes=1, batches=single, schema=[c:Int64]
{scan("cust", "c", indent=8)}
{scan("ss", "sc", "q", indent=6)}""")
    join = semi.children[1].children[0]
    [cluster] = clusters(semi)
    assert cluster.root is join and cluster.joins == (join,) and cluster.one_row_relations == 0
    assert [r.root.fields["table"] for r in cluster.relations] == ["cust", "ss"]
    # under the semi join the cross join roots a relation
    above = plan(f"""GpuHashJoin: join_type=LeftSemi, on=[(a@0, b@1)], {L}, schema=[a:Int64]
{scan("t1", "a", indent=2)}
  GpuCrossJoin: lanes=1, batches=multiple, schema=[n:Int64, b:Int64]
{_final_aggregate(indent=4)}
{scan("t2", "b", indent=4)}""")
    [cluster] = clusters(above)
    assert [r.root.kind for r in cluster.relations] == ["GpuLoadParquet", "GpuCrossJoin"]
    assert cluster.joins == (above,) and cluster.one_row_relations == 0
    assert cluster.edges[0].keys == ((RelColumn(0, 0), RelColumn(1, 1)),)


# q23's shape with a join filter on max_store_sales' m: a cross join carries no residual
FILTERED = f"""GpuHashJoin: join_type=Inner, on=[(c@0, sc@1)], filter=c@build:0 > m@probe:0, {L}, schema=[c:Int64, m:Int64, sc:Int64, q:Int64]
  GpuCoalesceAllBatches: lanes=1, batches=single, schema=[c:Int64]
{scan("cust", "c", indent=4)}
  GpuCrossJoin: lanes=1, batches=multiple, schema=[m:Int64, sc:Int64, q:Int64]
{_indented(MS, 2)}
{scan("ss", "sc", "q", indent=4)}"""


def test_a_residual_reading_a_one_row_relation_keeps_the_wall():
    root = plan(FILTERED)
    [cluster] = clusters(root)
    assert cluster.joins == (root,) and cluster.one_row_relations == 0
    assert [r.root.kind for r in cluster.relations] == ["GpuLoadParquet", "GpuCrossJoin"]
    stats = dataset(tables={"ms": pa.table({"a": [1, 2, 3]}), "cust": pa.table({"c": [1, 2, 3]}),
                            "ss": pa.table({"sc": [1, 2, 2, 5], "q": [10, 20, 30, 40]})})
    assert plan_text(ordered(root, stats, 1)[0]) == plan_text(root)


def test_a_key_reading_a_one_row_relation_keeps_the_wall():
    # admitted, t2 would join t1 only through the one-row count, and every order of the three
    # would cross the two
    root = plan(f"""GpuHashJoin: join_type=Inner, on=[(n@1, b@0)], {L}, schema=[a:Int64, n:Int64, b:Int64]
  GpuCoalesceAllBatches: lanes=1, batches=single, schema=[a:Int64, n:Int64]
    GpuCrossJoin: {L}, schema=[a:Int64, n:Int64]
{scan("t1", "a", indent=6)}
{_final_aggregate(indent=6)}
{scan("t2", "b", indent=2)}""")
    stats = dataset(tables={"t1": pa.table({"a": [1, 2, 3]}), "t": pa.table({"a": [1, 2]}),
                            "t2": pa.table({"b": list(range(20))})})
    assert plan_text(ordered(root, stats, 1)[0]) == plan_text(root)
    [cluster] = clusters(root)
    assert cluster.joins == (root,) and cluster.one_row_relations == 0
    assert [r.root.kind for r in cluster.relations] == ["GpuCrossJoin", "GpuLoadParquet"]


def test_only_the_cross_join_whose_one_row_relation_a_residual_reads_is_a_wall():
    root = plan(f"""GpuCrossJoin: lanes=1, batches=multiple, schema=[n:Int64, c:Int64, m:Int64, sc:Int64, q:Int64]
{_final_aggregate(indent=2)}
{_indented(FILTERED, 2)}""")
    [cluster] = clusters(root)
    assert cluster.joins == (root, root.children[1]) and cluster.one_row_relations == 0b001
    assert [r.root.kind for r in cluster.relations] == ["GpuAggregateBatches", "GpuLoadParquet", "GpuCrossJoin"]


def test_named_gives_the_cross_join_its_cluster_id():
    tree = named(plan(Q23))
    assert [(n.kind, n.mj) for n in _walk(tree) if "Join" in n.kind] == [
        ("GpuCrossJoin", "MJ1"), ("GpuHashJoin", "MJ1")]


def _walk(node):
    yield node
    for child in node.children:
        yield from _walk(child)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
