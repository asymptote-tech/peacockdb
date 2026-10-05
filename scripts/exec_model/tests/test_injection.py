"""`LayoutInjector`'s own guarantees: each preset really is a different layout, an unshuffled
join keeps its lanes, the sources really do emit empty batches, and a seed reproduces.

`test_join_capability.py` runs joins at every preset and demands one answer, which an
injector that did nothing would pass. These are what rule that out. The plans read the
committed `testdata/tpch.minimal`, whose customer and nation are sf1's byte for byte in the
columns read here.
"""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import pathlib

from .corpus import BUDGET, ParquetTables, execute
from .harness import main
from .test_end_to_end import agg_schemas, same
from ..operators import aggregates as A
from ..operators import nodes as N
from ..operators.expressions import Binary, Col, Lit
from ..operators.injection import HashMode, LayoutInjector, LayoutPreset
from ..operators.joins import JoinType
from ..plan import Plan

TABLES = ParquetTables(pathlib.Path(__file__).resolve().parents[3] / "testdata" / "tpch.minimal")

#: Small enough for the row-wise hash to stay quick, large enough for several batches per lane
#: at the finest preset.
INJECT_ROWS = 5_000
#: Fixed, so a failing configuration reproduces exactly.
INJECT_SEED = 17
#: Per source call. High enough that empty batches land in most lanes at most presets.
EMPTY_BATCH_PROBABILITY = 0.3


def customers(columns):
    return TABLES.frame("customer", columns).head(INJECT_ROWS)


def nations():
    return TABLES.frame("nation", ["n_nationkey", "n_name"])


def sound(driver, label: str) -> None:
    """What must hold at the end of any run, whatever the layout was.

    `run()` already refuses to finish with a batch stranded in a queue; this adds the two
    things it cannot see. Every lane must have reached done — a lane the scheduler simply
    forgot would leave the queues clean — and the accountant's in-flight total must be back
    to zero, which is the statement that every batch that was held was also released.
    """
    assert driver.accountant.in_flight_bytes == 0, f"{label}: batches still held"
    assert 0 < driver.accountant.peak <= BUDGET, f"{label}: peak {driver.accountant.peak}"
    for state in driver.states:
        assert all(state.out_done), f"{label}: {state.info} did not finish every lane"
        assert state.queued_batches() == 0, f"{label}: {state.info} still holds batches"


def execution_shape(driver) -> tuple:
    """Enough of how the run went to tell two layouts apart."""
    return (
        tuple(info.n_lanes for info in driver.plan.nodes),
        len(driver.trace),
        driver.steps,
    )


def aggregate_plan(customer, keys):
    aggs = [
        A.Agg(A.SUM, "c_acctbal", "total"),
        A.Agg(A.MEAN, "c_acctbal", "avg_bal"),
        A.Agg(A.COUNT, None, "n"),
    ]
    state_schema, final_schema = agg_schemas(customer, keys, aggs)
    scan = N.scan("customer", customer, 4, 500, 1000)
    filtered = N.filter_("positive", scan, Binary(">", Col("c_acctbal"), Lit(0.0)))
    partial = N.partial_aggregate("agg_partial", filtered, keys, aggs)
    compacted = N.aggregate_batches("agg_batches", partial, keys, aggs, schema=state_schema)
    shuffle_in = N.coalesce_all("shuffle_in", N.merge_partitions("merge", compacted))
    emitted = N.emit_partitions("emit", shuffle_in, keys, 4)
    return N.unload(
        "unload",
        N.aggregate_batches("agg_final", emitted, keys, aggs,
                            A.finalize_exprs(aggs), schema=final_schema),
    )


def aggregate_oracle(customer, key):
    kept = customer[customer.c_acctbal > 0]
    return (
        kept.groupby(key, dropna=False)
        .agg(total=("c_acctbal", "sum"), avg_bal=("c_acctbal", "mean"), n=("c_acctbal", "size"))
        .reset_index()
    )


def shuffled_join_plan(nation, customer):
    """Both sides hashed on the join key, so the join is correct at any lane count."""
    build = N.coalesce_all(
        "nation_all",
        N.emit_partitions(
            "build_emit",
            N.merge_partitions("build_merge", N.scan("nation", nation, 2, 8)),
            ["n_nationkey"],
            4,
        ),
        schema=dict(nation.dtypes),
    )
    probe = N.emit_partitions(
        "probe_emit",
        N.merge_partitions("probe_merge", N.scan("customer", customer, 4, 500, 1000)),
        ["c_nationkey"],
        4,
    )
    return N.unload(
        "unload",
        N.hash_join("join", build, probe, JoinType.INNER, ["n_nationkey"], ["c_nationkey"]),
    )


def streamed_join_plan(nation, customer):
    """Neither side shuffled — the join's one lane is load-bearing, and stays one."""
    build = N.coalesce_all("nation_all", N.scan("nation", nation, 1, 25), schema=dict(nation.dtypes))
    probe = N.scan("customer", customer, 1, 500)
    return N.unload(
        "unload",
        N.hash_join("join", build, probe, JoinType.INNER, ["n_nationkey"], ["c_nationkey"]),
    )


def test_the_presets_really_do_execute_differently():
    # Without this a sweep could be five runs of one layout and still pass, which is the
    # failure mode a parameterized test has that a hand-written one does not.
    customer = customers(["c_custkey", "c_mktsegment", "c_acctbal"])
    want = aggregate_oracle(customer, "c_mktsegment")
    shapes = []
    for preset in LayoutPreset:
        injector = LayoutInjector(preset, HashMode.SPREAD, EMPTY_BATCH_PROBABILITY, INJECT_SEED)
        got, driver = execute(injector.apply(aggregate_plan(customer, ["c_mktsegment"])))
        same(got, want, f"distinctness {injector.label}")
        sound(driver, f"distinctness {injector.label}")
        shapes.append(execution_shape(driver))
    assert len(set(shapes)) == len(LayoutPreset), shapes
    lane_counts = {shape[0] for shape in shapes}
    assert len(lane_counts) == len(LayoutPreset), lane_counts


def test_an_unshuffled_join_keeps_the_lane_count_its_plan_was_written_with():
    # The injector may not repartition a join whose sides were never hashed on the key:
    # splitting nation and customer into 8 lanes each would join matching slices and
    # return roughly an eighth of the rows.
    nation, customer = nations(), customers(["c_custkey", "c_nationkey"])
    injector = LayoutInjector(LayoutPreset.MANY_SMALL_PARTITIONS, seed=INJECT_SEED)

    pinned = Plan.build(injector.apply(streamed_join_plan(nation, customer)))
    assert {info.n_lanes for info in pinned.nodes} == {1}

    # ...and that it is a decision about the join, not a refusal to touch joins at all.
    shuffled = Plan.build(injector.apply(shuffled_join_plan(nation, customer)))
    assert max(info.n_lanes for info in shuffled.nodes) == 8


def test_the_injected_sources_really_do_emit_empty_batches():
    # The empty-batch probability is the one injection with no structural trace, so it is
    # the one that could silently be doing nothing.
    customer = customers(["c_custkey", "c_acctbal"])
    plan = LayoutInjector(
        LayoutPreset.FEW_PARTITIONS_FEW_BATCHES,
        empty_batch_probability=EMPTY_BATCH_PROBABILITY,
        seed=INJECT_SEED,
    ).apply(N.unload("unload", N.scan("customer", customer, 4, 500, 1000)))
    _, driver = execute(plan)
    assert any(batch.num_rows() == 0 for batch in driver.results)
    assert sum(len(batch.frame) for batch in driver.results) == len(customer)


def test_the_same_injector_twice_runs_identically():
    # Randomized injection is only usable if a failure reproduces.
    customer = customers(["c_custkey", "c_mktsegment", "c_acctbal"])

    def once():
        injector = LayoutInjector(
            LayoutPreset.REBATCHED, HashMode.SKEWED, EMPTY_BATCH_PROBABILITY, INJECT_SEED
        )
        got, driver = execute(injector.apply(aggregate_plan(customer, ["c_mktsegment"])))
        return got, driver.trace

    first, first_trace = once()
    second, second_trace = once()
    assert first_trace == second_trace
    same(first, second, "the same injector twice")


if __name__ == "__main__":
    raise SystemExit(main(globals()))
