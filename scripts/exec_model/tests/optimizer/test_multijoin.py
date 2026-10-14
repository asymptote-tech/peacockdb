"""MultiJoin assembly: which nodes make a cluster, which root a relation, and every key, residual
and output column lifted to the relation column it is."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

from ..harness import main
from ...plans.engine_expr import Binary
from ...plans.engine_plan import parse_plans
from ...optimizer.multijoin import Edge, RelColumn, assemble, clusters, named

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


def test_a_filter_a_join_with_a_residual_it_cannot_place_and_a_computing_project_each_root_a_relation():
    filtered = CHAIN.replace(
        "  GpuCoalesceAllBatches: lanes=1, batches=single, schema=[a:Int64, b:Int64]",
        "  GpuFilter: predicate=a@0 IS NULL, lanes=1, batches=multiple, schema=[a:Int64, b:Int64]")
    assert [r.root.kind for r in assemble(plan(filtered)).relations] == ["GpuFilter", "GpuLoadParquet"]
    semi = CHAIN.replace("join_type=Inner, on=[(a@0, b@0)]",
                         "join_type=LeftSemi, on=[(a@0, b@0)], filter=a@build:0 < b@probe:0")
    semi_relations = assemble(plan(semi)).relations
    assert [r.root.kind for r in semi_relations] == ["GpuHashJoin", "GpuLoadParquet"]
    assert semi_relations[0].root.fields["join_type"] == "LeftSemi"
    computing = CHAIN.replace(
        "  GpuCoalesceAllBatches: lanes=1, batches=single, schema=[a:Int64, b:Int64]",
        "  GpuProject: exprs=[a@0, b@1 + 1 as b], lanes=1, batches=multiple, schema=[a:Int64, b:Int64]")
    assert [r.root.kind for r in assemble(plan(computing)).relations] == ["GpuProject", "GpuLoadParquet"]


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


def test_a_cross_join_a_nested_loop_join_and_a_filtered_semi_join_carry_no_id():
    text = f"""GpuCrossJoin: {L}, schema=[a:Int64, c:Int64]
  GpuNestedLoopJoin: join_type=Inner, filter=a@build:0 < b@probe:0, projection=[a@0], {L}, schema=[a:Int64]
{scan("t1", "a", indent=4)}
{scan("t2", "b", indent=4)}
  GpuHashJoin: join_type=LeftSemi, on=[(c@0, d@0)], filter=c@build:0 < d@probe:0, {L}, schema=[c:Int64]
{scan("t3", "c", indent=4)}
    GpuHashJoin: join_type=Inner, on=[(d@0, e@0)], projection=[d@0], {L}, schema=[d:Int64]
{scan("t4", "d", indent=6)}
{scan("t5", "e", indent=6)}"""
    tree = named(plan(text))
    assert [(n.kind, n.mj) for n in _walk(tree) if "Join" in n.kind] == [
        ("GpuCrossJoin", None), ("GpuNestedLoopJoin", None), ("GpuHashJoin", None), ("GpuHashJoin", "MJ1")]


def _walk(node):
    yield node
    for child in node.children:
        yield from _walk(child)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
