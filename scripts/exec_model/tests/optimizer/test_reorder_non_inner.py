"""Non-inner joins in a cluster: DPhyp moves an inner join below a left, semi or anti join where the
conflicts allow it — the four named tpch queries' shapes over toy tables — never where they do
not, and the plan answers the same. Needs the DPhyp library."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import pyarrow as pa

from ..corpus import ParquetTables, execute
from ..harness import main
from .test_cardinality import dataset, scan
from ...optimizer.cost import plan_cost
from ...optimizer.join_order import ordered
from ...plans.engine_nodes import build
from ...plans.engine_plan import parse_plans

TABLES = {
    # 25 nations; 1000 customers, 40 a nation; 10 000 orders for the first 667 customers alone;
    # three lines an order, a day in 0..29 each
    "nat": pa.table({"nk": list(range(25)), "nm": list(range(25))}),
    "cus": pa.table({"ck": list(range(1000)), "cn": [k % 25 for k in range(1000)]}),
    "ord": pa.table({"ok": list(range(10_000)), "oc": [i % 667 for i in range(10_000)]}),
    "lin": pa.table({"lo": [i % 10_000 for i in range(30_000)], "ld": [(i // 1000) % 30 for i in range(30_000)]}),
}
L = "lanes=1, batches=multiple"
NATION = f"""GpuCoalesceAllBatches: lanes=1, batches=single, schema=[nk:Int64]
      GpuFilter: predicate=nm@1 = 6, projection=[nk@0], {L}, schema=[nk:Int64]
        {scan('nat', ['nk', 'nm'])}"""
CUSTOMERS = f"""GpuCoalesceAllBatches: lanes=1, batches=single, schema=[ck:Int64, cn:Int64]
        {scan('cus', ['ck', 'cn'])}"""


def under_nation(join_type: str, columns: list[str]) -> str:
    """σ(nat) ⋈ (cus `join_type` ord), as the engine plans left-join-reorder and its two siblings:
    the lower join outputs `columns`, the upper all of them but the nation key."""
    schema = ", ".join(f"{c}:Int64" for c in columns)
    lower = ", ".join(f"{c}@{i}" for i, c in enumerate(columns))
    outer_schema = ", ".join(f"{c}:Int64" for c in columns if c != "cn")
    outer = ", ".join(f"{c}@{i + 1}" for i, c in enumerate(columns) if c != "cn")
    return f"""GpuUnload
  GpuHashJoin: join_type=Inner, on=[(nk@0, cn@1)], projection=[{outer}], {L}, schema=[{outer_schema}]
    {NATION}
    GpuHashJoin: join_type={join_type}, on=[(ck@0, oc@1)], projection=[{lower}], {L}, schema=[{schema}]
      {CUSTOMERS}
      {scan('ord', ['ok', 'oc'])}
"""


LEFT = under_nation("Left", ["ck", "cn", "ok"])
SEMI = under_nation("LeftSemi", ["ck", "cn"])
ANTI = under_nation("LeftAnti", ["ck", "cn"])
# σ(lin) ⋈ (cus ⟕ ord) on the orders' key: the left join must stay below.
FIXED = f"""GpuUnload
  GpuHashJoin: join_type=Inner, on=[(lo@0, ok@1)], projection=[ck@1, ok@2], {L}, schema=[ck:Int64, ok:Int64]
    GpuCoalesceAllBatches: lanes=1, batches=single, schema=[lo:Int64]
      GpuFilter: predicate=ld@1 = 3, projection=[lo@0], {L}, schema=[lo:Int64]
        {scan('lin', ['lo', 'ld'])}
    GpuHashJoin: join_type=Left, on=[(ck@0, oc@1)], projection=[ck@0, ok@2], {L}, schema=[ck:Int64, ok:Int64]
      {CUSTOMERS}
      {scan('ord', ['ok', 'oc'])}
"""


def optimized(text: str, row_group_size=None):
    stats = dataset(row_group_size, tables=TABLES)
    plan = parse_plans(f"== q\n{text}", "test")["q"]
    after, orders = ordered(plan, stats, lanes=1)
    data = ParquetTables(stats.directory)
    assert rows(after, data) == rows(plan, data)
    return plan, after, orders, stats


def rows(plan, tables):
    got, _ = execute(build(plan, tables))
    return sorted(tuple(-1 if v != v else v for v in row) for row in got.itertuples(index=False))


def sides(plan, join_types) -> list[tuple[set, set]]:
    """The tables under each side of every hash join of one of `join_types`."""
    found = []
    for node in _walk(plan):
        if node.kind == "GpuHashJoin" and node.fields["join_type"] in join_types:
            found.append(tuple({n.fields["table"] for n in _walk(child) if n.kind == "GpuLoadParquet"}
                               for child in node.children))
    return found


def _walk(node):
    yield node
    for child in node.children:
        yield from _walk(child)


def test_the_nation_join_goes_below_the_left_join_and_the_left_join_meets_one_nation_s_customers():
    plan, after, _, stats = optimized(LEFT)
    [(build_tables, probe_tables)] = sides(after, ("Left", "Right"))
    assert {frozenset(build_tables), frozenset(probe_tables)} == {frozenset({"nat", "cus"}), frozenset({"ord"})}
    assert plan_cost(after, stats) < plan_cost(plan, stats)


def test_the_nation_join_goes_below_the_semi_and_the_anti_join():
    for text, types in ((SEMI, ("LeftSemi", "RightSemi")), (ANTI, ("LeftAnti", "RightAnti"))):
        plan, after, _, stats = optimized(text)
        [(build_tables, probe_tables)] = sides(after, types)
        assert {frozenset(build_tables), frozenset(probe_tables)} == \
            {frozenset({"nat", "cus"}), frozenset({"ord"})}, types
        assert plan_cost(after, stats) < plan_cost(plan, stats), types


def test_a_left_join_whose_null_supplying_side_an_inner_join_reads_keeps_its_place():
    _, after, [order], _ = optimized(FIXED)
    [(build_tables, probe_tables)] = sides(after, ("Left", "Right"))
    assert {frozenset(build_tables), frozenset(probe_tables)} == {frozenset({"cus"}), frozenset({"ord"})}
    assert order.unsolved is None


def test_an_inner_residual_reading_both_sides_of_a_left_join_stays_on_the_inner_join():
    # The nation join's residual reads the orders: it may not land on the left join below.
    text = LEFT.replace("on=[(nk@0, cn@1)],", "on=[(nk@0, cn@1)], filter=nk@build:0 < ok@probe:2,")
    _, after, _, _ = optimized(text)
    [left] = [n for n in _walk(after) if n.kind == "GpuHashJoin" and n.fields["join_type"] in ("Left", "Right")]
    assert "filter" not in left.fields
    assert sides(after, ("Left", "Right")) == [({"cus"}, {"ord"})]


def test_a_non_inner_join_keeps_its_plan_type_and_build_side_where_copies_would_flip_it():
    # Orders in 30 batches: building the customers is copied once per batch (#152), which would
    # make the orders the cheaper build of an inner join. A left or semi join keeps its build.
    many = "partition_groups=[[" + ",".join(f"[{g}]" for g in range(30)) + "]]"  # 334 rows each
    for text, join_type in ((FIXED, "Left"), (LEFT, "Left"), (SEMI, "LeftSemi")):
        text = text.replace(f"{scan('ord', ['ok', 'oc'])}",
                            scan("ord", ["ok", "oc"]).replace("partition_groups=[[[0]]]", many))
        _, after, _, _ = optimized(text, {"ord": 334})
        [(build_tables, _)] = sides(after, (join_type, "Right", "RightSemi"))
        joins = [n.fields["join_type"] for n in _walk(after) if n.kind == "GpuHashJoin"]
        assert join_type in joins and "cus" in build_tables, (join_type, joins)


# q56's shape at four lanes: a final aggregate per lane over the semi join's output, which the plan
# leaves hashed on the group key. Reordered, the semi join goes below, and the cluster's new root
# joins on other keys.
GROUPED = """GpuUnload
  GpuAggregateBatches: group_by=[iid@0], aggs=[sum(s@1) as s], final=[s@1], lanes=4, batches=single, hashed_on=[iid@0], schema=[iid:Int64, s:Int64]
    GpuAggregate: group_by=[iid@1], aggs=[sum(v@0) as s], lanes=4, batches=multiple, hashed_on=[iid@0], schema=[iid:Int64, s:Int64]
      GpuHashJoin: join_type=RightSemi, on=[(sid@0, iid@1)], lanes=4, batches=multiple, hashed_on=[iid@1], schema=[v:Int64, iid:Int64]
        GpuCoalesceAllBatches: lanes=4, batches=single, hashed_on=[sid@0], schema=[sid:Int64]
          GpuEmitPartitions: hash=[sid@0], lanes=4, batches=multiple, hashed_on=[sid@0], schema=[sid:Int64]
            GpuLoadParquet: table=sel, projections=[sid@0], partition_groups=[[[0]]], lanes=1, batches=multiple, schema=[sid:Int64]
        GpuEmitPartitions: hash=[iid@1], lanes=4, batches=multiple, hashed_on=[iid@1], schema=[v:Int64, iid:Int64]
          GpuMergePartitions: lanes=1, batches=multiple, schema=[v:Int64, iid:Int64]
            GpuHashJoin: join_type=Inner, on=[(ik@0, fk@0)], projection=[v@3, iid@1], lanes=4, batches=multiple, schema=[v:Int64, iid:Int64]
              GpuCoalesceAllBatches: lanes=4, batches=single, hashed_on=[ik@0], schema=[ik:Int64, iid:Int64]
                GpuEmitPartitions: hash=[ik@0], lanes=4, batches=multiple, hashed_on=[ik@0], schema=[ik:Int64, iid:Int64]
                  GpuLoadParquet: table=itm, projections=[ik@0, iid@1], partition_groups=[[[0]]], lanes=1, batches=multiple, schema=[ik:Int64, iid:Int64]
              GpuEmitPartitions: hash=[fk@0], lanes=4, batches=multiple, hashed_on=[fk@0], schema=[fk:Int64, v:Int64]
                GpuLoadParquet: table=fact, projections=[fk@0, v@1], partition_groups=[[[0]]], lanes=1, batches=multiple, schema=[fk:Int64, v:Int64]
"""
GROUPED_TABLES = {
    "sel": pa.table({"sid": list(range(5))}),
    "itm": pa.table({"ik": list(range(200)), "iid": [k % 50 for k in range(200)]}),
    "fact": pa.table({"fk": [i % 200 for i in range(20_000)], "v": [1] * 20_000}),
}


def test_a_root_only_per_lane_nodes_read_is_not_shuffled_back_onto_its_hash():
    # q76's shape: a sort per lane then the unload, which co-locate nothing.
    text = GROUPED.replace(
        "  GpuAggregateBatches: group_by=[iid@0], aggs=[sum(s@1) as s], final=[s@1], lanes=4, batches=single, "
        "hashed_on=[iid@0], schema=[iid:Int64, s:Int64]\n"
        "    GpuAggregate: group_by=[iid@1], aggs=[sum(v@0) as s], lanes=4, batches=multiple, hashed_on=[iid@0], "
        "schema=[iid:Int64, s:Int64]\n",
        "  GpuSort: by=[iid@1 asc nulls_last], lanes=4, batches=multiple, schema=[v:Int64, iid:Int64]\n"
        "    GpuSort: by=[iid@1 asc nulls_last], lanes=4, batches=multiple, schema=[v:Int64, iid:Int64]\n")
    stats = dataset(tables=GROUPED_TABLES)
    plan = parse_plans(f"== q\n{text}", "test")["q"]
    after, _ = ordered(plan, stats, lanes=4)
    data = ParquetTables(stats.directory)
    assert rows(after, data) == rows(plan, data)
    lower_sort = [n for n in _walk(after) if n.kind == "GpuSort"][1]
    assert lower_sort.children[0].kind == "GpuHashJoin", lower_sort.children[0].kind


def test_a_rebuilt_cluster_root_keeps_the_hash_the_plan_above_it_relies_on():
    stats = dataset(tables=GROUPED_TABLES)
    plan = parse_plans(f"== q\n{GROUPED}", "test")["q"]
    after, _ = ordered(plan, stats, lanes=4)
    data = ParquetTables(stats.directory)
    assert rows(after, data) == rows(plan, data) == [(k, 400) for k in range(5)]
    [root] = [n for n in _walk(after) if n.kind == "GpuAggregate"]
    assert root.children[0].fields["hashed_on"] == "[iid@1]"


if __name__ == "__main__":
    raise SystemExit(main(globals()))
