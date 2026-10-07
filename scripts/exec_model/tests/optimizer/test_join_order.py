"""What DPhyp is asked of a set of relations — the set's rows by `cardinality`'s formulas, the
columns the joins above need of it, its bytes — and which side of each join builds."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

from ..harness import main
from .test_cardinality import dataset, scan
from ...optimizer.cardinality import Column, estimator
from ...optimizer.cost import row_width
from ...plans.engine_plan import parse_plans
from ...optimizer.join_order import SetEstimates, batches, orient
from ...optimizer.multijoin import RelColumn, clusters

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


def test_a_build_copied_per_probe_batch_can_make_the_larger_side_the_cheaper_build():
    # 30 order batches: building the customers copies them 30 times (#152); building the
    # orders copies them once, per the customers' single batch. Without the copies the
    # smaller side builds again.
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


if __name__ == "__main__":
    raise SystemExit(main(globals()))
