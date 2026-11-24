"""What DPhyp is asked of a set of relations — the set's rows by `cardinality`'s formulas, the
columns the joins above need of it, its bytes — and which side of each join builds."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import functools
import math

import numpy as np
import pyarrow as pa

from ..harness import main
from .test_multijoin import AB, Q23, Q23_TP4, _final_aggregate, _indented, plan
from .test_reorder_non_inner import ANTI, LEFT, SEMI, TABLES as NON_INNER_TABLES
from .test_cardinality import dataset, scan
from ...optimizer.cardinality import Column, estimator
from ...optimizer.conflicts import joining
from ...optimizer.cost import row_width
from ...optimizer.disassembly import planned_lanes
from ...plans.engine_plan import parse_plans
from ...optimizer.join_order import SetEstimates, batches, orient, ordered
from ...optimizer.multijoin import RelColumn, clusters
from ...optimizer.report import unordered

L = "lanes=1, batches=multiple"
# A star around the dates: sales and spread each join them on the date key.
STAR = f"""GpuUnload
  GpuHashJoin: join_type=Inner, on=[(dk@0, pk@0)], projection=[dk@0, sk@1], {L}, schema=[dk:Int64, sk:Int64]
    GpuHashJoin: join_type=Inner, on=[(dk@0, sk@0)], {L}, schema=[dk:Int64, sk:Int64]
      {scan('dates', ['dk'])}
      {scan('sales', ['sk'])}
    {scan('spread', ['pk'])}
"""


def setup():
    plan = parse_plans(f"== q\n{STAR}", "test")["q"]
    est = estimator(plan, dataset())
    [cluster] = clusters(plan)
    return cluster, est, SetEstimates(cluster, est)


def test_the_whole_set_in_the_plan_s_own_order_is_the_plan_s_estimate():
    cluster, est, sets = setup()
    assert [r.root.fields["table"] for r in cluster.relations] == ["dates", "sales", "spread"]
    assert abs(sets.rows(0b111) - est.estimates[id(cluster.root)].rows) < 1e-6
    # A pair is the join of the two, as cardinality has it for the plan's own lower join.
    lower = cluster.root.children[0]
    assert abs(sets.rows(0b011) - est.estimates[id(lower)].rows) < 1e-6


def test_a_set_needs_the_cluster_s_output_and_the_keys_of_every_edge_that_leaves_it():
    _, _, sets = setup()
    # The cluster outputs dates.dk and sales.sk; dates also keys the edge to spread.
    assert sets.needed(0b011) == [RelColumn(0, 0), RelColumn(1, 0)]
    # Dates and spread pass on dates.dk: the output and the key to sales are the same column.
    assert sets.needed(0b101) == [RelColumn(0, 0)]
    assert sets.needed(0b111) == [RelColumn(0, 0), RelColumn(1, 0)]


def test_a_set_costs_its_rows_times_the_width_of_what_it_passes_on():
    _, _, sets = setup()
    two_int64 = row_width([("dk", "Int64"), ("sk", "Int64")], (Column(None, 1.0), Column(None, 1.0)))
    assert abs(sets.cost(0b011) - sets.rows(0b011) * two_int64) < 1e-6


def non_inner(text):
    plan = parse_plans(f"== q\n{text}", "test")["q"]
    est = estimator(plan, dataset(tables=NON_INNER_TABLES))
    [cluster] = clusters(plan)
    return cluster, est, SetEstimates(cluster, est)


def test_a_set_made_by_a_left_semi_or_anti_join_is_estimated_as_cardinality_estimates_that_join():
    for text in (LEFT, SEMI, ANTI):
        cluster, est, sets = non_inner(text)
        # nation r0, customers r1, orders r2: the plan's own lower join is customers with orders.
        assert abs(sets.rows(0b110) - est.estimates[id(cluster.root.children[1])].rows) < 1e-6
    _, _, left = non_inner(LEFT)
    _, _, semi = non_inner(SEMI)
    _, _, anti = non_inner(ANTI)
    # A left join keeps every row of the one nation's customers, a semi or an anti join some.
    assert left.rows(0b111) >= left.rows(0b011)
    assert semi.rows(0b111) + anti.rows(0b111) <= semi.rows(0b011) * (1 + 1e-9)


def pair(orders_batches: int):
    """customers (10 rows) and orders (40), the orders built as the plan has it, the orders
    scan emitting `orders_batches` batches."""
    groups = "[[" + ",".join(["[0]"] * orders_batches) + "]]"
    orders = scan("orders", ["ok"]).replace("partition_groups=[[[0]]]", f"partition_groups={groups}")
    text = f"""GpuUnload
  GpuHashJoin: join_type=Inner, on=[(ok@0, ck@0)], {L}, schema=[ok:Int64, ck:Int64]
    {orders}
    {scan('cust', ['ck'])}
"""
    plan = parse_plans(f"== q\n{text}", "test")["q"]
    [cluster] = clusters(plan)
    return SetEstimates(cluster, estimator(plan, dataset()))


def test_the_smaller_side_builds_where_the_probe_s_batches_cost_nothing_more():
    sets = pair(orders_batches=1)
    assert orient((0, 1), sets) == (1, 0)  # 10 customers build, 40 orders probe
    assert orient((1, 0), sets) == (1, 0)


def test_the_probe_s_batches_can_make_the_larger_side_the_cheaper_build():
    # 30 order batches: the customers' build is priced at 1 + 30 times its bytes, the orders' at
    # 1 + 1, per the customers' single batch (`copies`, #294). Without the term the smaller side
    # builds again.
    sets = pair(orders_batches=30)
    assert orient((1, 0), sets, copies=1) == (0, 1)
    assert orient((0, 1), sets, copies=0) == (1, 0)


def test_a_probe_shuffled_onto_the_join_s_lanes_is_cut_into_as_many_batches_as_it_has_rows_for():
    # 5 order batches. At one lane the customers build: 10 · (1 + 5) against 40 · (1 + 1). At four
    # each probe is shuffled first, every batch cut into four up to one a row: building the
    # customers now pays for min(5 · 4, 40) = 20 order batches, 10 · 21 = 210, and building the
    # orders for min(1 · 4, 10) = 4 customer batches, 40 · 5 = 200.
    sets = pair(orders_batches=5)
    assert orient((0, 1), sets, lanes=1) == (1, 0)
    assert orient((0, 1), sets, lanes=4) == (0, 1)


def test_a_join_emits_a_batch_per_probe_batch_and_a_single_batch_node_one_per_lane():
    sets = pair(orders_batches=30)
    orders, cust = (r.root for r in sets.cluster.relations)
    assert (batches(orders), batches(cust), batches(sets.cluster.root)) == (30, 1, 1)
    coalesced = parse_plans("== q\nGpuCoalesceAllBatches: lanes=4, batches=single, schema=[ck:Int64]\n"
                            f"  {scan('cust', ['ck'])}\n", "test")["q"]
    assert batches(coalesced) == 4


@functools.cache
def one_row_dataset():
    """tpcds q23's best_ss_customer at sf1: 100,000 customers, 2,750,000 store sales of one customer
    each; tables for the key join of `AB` under two one-row relations, one row each; and
    `ONE_ROW_STAR`'s, two dimension keys of 100 values in 1,000 fact rows, two of each in a dimension;
    `ONE_ROW_CHAIN`'s three tables of keys 0..99."""
    sales, fact = np.arange(2_750_000), np.arange(1000)
    return dataset(row_group_size=2_750_000, tables={
        "ms": pa.table({"a": [1, 2, 3]}),
        "cust": pa.table({"c": list(range(100_000))}),
        "ss": pa.table({"sc": sales % 100_000, "q": sales}),
        "t1": pa.table({"a": [1]}), "t2": pa.table({"a": [1]}),
        "ta": pa.table({"a": [1]}), "tb": pa.table({"b": [1]}),
        "r": pa.table({"a": [1]}), "da": pa.table({"a": [0, 1]}), "dc": pa.table({"c": [0, 1]}),
        "fact": pa.table({"ka": fact % 100, "kc": fact // 10}),
        **{f"x{c}": pa.table({c: list(range(100))}) for c in "abc"},
    })


def one_row_sets(text):
    root = plan(text)
    [cluster] = clusters(root)
    return cluster, SetEstimates(cluster, estimator(root, one_row_dataset()))


def test_a_one_row_relation_multiplies_the_rows_of_the_set_it_joins():
    # max_store_sales r0, customer r1, store_sales r2
    cluster, sets = one_row_sets(Q23)
    assert [sets.rows(1 << r) for r in range(3)] == [1, 100_000, 2_750_000]
    assert sets.rows(0b110) == 2_750_000
    assert sets.rows(0b011) == 100_000
    assert sets.rows(0b111) == sets.rows(0b110)
    needed = sets.needed(0b011)
    assert needed == [RelColumn(0, 0), RelColumn(1, 0)]
    two_int64 = row_width([("m", "Int64"), ("c", "Int64")], (Column(None, 1.0), Column(None, 1.0)))
    assert sets.cost(0b011) == 100_000 * two_int64


def orders(text, lanes=1):
    """`ordered`'s DPhyp calls on the plan `text`."""
    return ordered(plan(text), one_row_dataset(), lanes)[1]


def test_dphyp_crosses_the_one_row_relation_onto_the_smaller_side():
    [order] = orders(Q23)
    assert unordered(order.tree) == unordered(((0, 1), 2))
    assert sum(join.cost for join in order.joins) < order.plan_cost


# t1's one row two columns wide and t2's one, crossed onto ta ⋈ tb, one row each
TWO_ONE_ROW = f"""GpuCrossJoin: lanes=1, batches=multiple, schema=[n:Int64, w:Int64, n:Int64, a:Int64, b:Int64]
  GpuProject: exprs=[n@0, n@0 + 1 as w], lanes=1, batches=single, schema=[n:Int64, w:Int64]
{_final_aggregate(indent=4, table="t1")}
  GpuCrossJoin: lanes=1, batches=multiple, schema=[n:Int64, a:Int64, b:Int64]
{_final_aggregate(indent=4, table="t2")}
{_indented(AB, 4)}"""


def test_the_one_row_side_builds():
    _, sets = one_row_sets(Q23)
    assert orient(((1, 0), 2), sets) == orient((2, (1, 0)), sets) == ((0, 1), 2)
    assert orient((0, (1, 2)), sets) == orient(((2, 1), 0), sets) == (0, (1, 2))
    # Where it costs more: t1's 16 bytes build before t2's 8 as DPhyp's left, the one-row pair's 24
    # over the 16 of ta ⋈ tb, and t2 over ta, as wide.
    cluster, sets = one_row_sets(TWO_ONE_ROW)
    assert cluster.one_row == 0b0011
    assert orient((0, 1), sets) == (0, 1) and orient((1, 0), sets) == (1, 0)
    assert orient(((2, 3), (0, 1)), sets) == ((0, 1), (2, 3))
    assert orient((2, 1), sets) == (1, 2)


def test_lanes_of_a_join_ignore_its_one_row_relations():
    # Store sales in 40 batches. On one lane max_store_sales ⋈ customer builds, 1.6 MB · (1 + 40)
    # against 44.7 MB · (1 + 1). On the hash join's four lanes each probe batch is cut in four:
    # 1.6 MB · (1 + 160) against 44.7 MB · (1 + 4), and store sales builds.
    groups = "[[" + ",".join(["[0]"] * 40) + "]]"
    text = Q23_TP4.replace("table=ss, projections=[sc@0, q@1], partition_groups=[[[0]]]",
                           f"table=ss, projections=[sc@0, q@1], partition_groups={groups}")
    cluster, sets = one_row_sets(text)
    assert planned_lanes(cluster) == {0b110: 4}
    assert orient(((0, 1), 2), sets, lanes=1) == ((0, 1), 2)
    assert orient(((0, 1), 2), sets, lanes=1, planned=planned_lanes(cluster)) == (2, (0, 1))
    # The cross join runs on one lane, its 1 customer batch merged, not cut: at tp4 the hash join
    # above it sees 4 of them, not 16, and store sales still builds.
    assert orient(((0, 1), 2), sets, lanes=4, planned=planned_lanes(cluster)) == (2, (0, 1))


# r one row over xa ⋈ (xb ⋈ xc), all three keys of one class
ONE_ROW_CHAIN = f"""GpuCrossJoin: lanes=1, batches=multiple, schema=[n:Int64, a:Int64, b:Int64, c:Int64]
{_final_aggregate(indent=2, table="r")}
  GpuHashJoin: join_type=Inner, on=[(a@0, c@1)], {L}, schema=[a:Int64, b:Int64, c:Int64]
    {scan('xa', ['a'])}
    GpuHashJoin: join_type=Inner, on=[(b@0, c@0)], {L}, schema=[b:Int64, c:Int64]
      {scan('xb', ['b'])}
      {scan('xc', ['c'])}"""


def test_a_one_row_relation_multiplies_every_key_connected_set():
    # Split off last, c would leave r crossing xa with xb, 10,000 rows c's two keys of one class
    # would join back as one.
    cluster, sets = one_row_sets(ONE_ROW_CHAIN)
    assert cluster.one_row == 0b0001
    for joined in (0b0010, 0b0100, 0b1000, 0b0110, 0b1010, 0b1100, 0b1110):
        assert sets.rows(joined | 1) == sets.rows(joined) * sets.rows(1), bin(joined)


# r one row; dimensions da and dc two rows each, keyed to the fact on ka and kc
ONE_ROW_STAR = f"""GpuCrossJoin: lanes=1, batches=multiple, schema=[n:Int64, a:Int64, ka:Int64, kc:Int64, c:Int64]
{_final_aggregate(indent=2, table="r")}
  GpuHashJoin: join_type=Inner, on=[(kc@2, c@0)], {L}, schema=[a:Int64, ka:Int64, kc:Int64, c:Int64]
    GpuHashJoin: join_type=Inner, on=[(a@0, ka@0)], {L}, schema=[a:Int64, ka:Int64, kc:Int64]
      {scan('da', ['a'])}
      {scan('fact', ['ka', 'kc'])}
    {scan('dc', ['c'])}"""


def test_one_row_edges_never_cross_two_larger_relations():
    # r0, da r1, fact r2, dc r3. Crossing da with dc makes 4 rows where da ⋈ fact makes 20, so
    # without the rule DPhyp would join them across r's edges alone.
    cluster, sets = one_row_sets(ONE_ROW_STAR)
    assert cluster.one_row == 0b0001
    assert sets.cost(0b1011) == math.inf
    assert math.isfinite(sets.cost(0b0011)) and math.isfinite(sets.cost(0b0111))
    [order] = orders(ONE_ROW_STAR)
    for join in order.joins:
        if join.build & ~cluster.one_row and join.probe & ~cluster.one_row:
            assert any(e.keys for e in joining(cluster.edges, join.build, join.probe)), join
    assert all(math.isfinite(join.cost) for join in order.joins) and math.isfinite(order.plan_cost)
    # a residual between da and dc is no key
    filtered, sets = one_row_sets(ONE_ROW_STAR.replace("on=[(kc@2, c@0)],",
                                                       "on=[(kc@2, c@0)], filter=a@build:0 < c@probe:0,"))
    assert any(e.residual is not None and e.left | e.right == 0b1010 for e in filtered.edges)
    assert sets.cost(0b1011) == math.inf

if __name__ == "__main__":
    raise SystemExit(main(globals()))
