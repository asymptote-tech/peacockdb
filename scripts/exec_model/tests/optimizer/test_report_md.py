"""`<mode>.optimizer.md`'s section, rendered from a hand-built report: a cluster reordered before the
run, flipped at a replan and ordered again unchanged, as one table of the orders it had; a cluster
that kept its order as one line; the dynamic filters of both passes with a probe's replan; and the
anchors GitHub gives its headings."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import re

from ..harness import main
from ...operators.join_types import JoinType
from ...optimizer.cardinality import Certainty
from ...optimizer.dynamic_filters import KeySummary, Pruned, PrunedBy
from ...optimizer.report import (
    BuildMiss, ChosenJoin, FilterPass, JoinOrder, Kept, OptimizerReport, Probed, RelationEstimate, Replan,
)
from ...optimizer.report_md import cluster_heading, filters_heading, note, preamble, section
from ...plans.goldens import anchor


def relations(*names):
    """Relations by name, a memory source's name reading that source."""
    return tuple(RelationEstimate(name, 10.0, Certainty.EXACT, 8.0, (), (name,) if name in ("m0", "p0") else ())
                 for name in names)


def order(mj, names, planned, oriented, cost, plan_cost, flipped=(), types=(), unsolved=None):
    return JoinOrder(mj, relations(*names), (), (), 10_000, oriented, unsolved, planned, oriented,
                     (ChosenJoin(1, 2, 1.0, cost),), plan_cost, flipped, types)


# The dates, filtered, move below the sales before the run; at the replan the dates and sales are
# one memory source, and item turns round to build.
BEFORE = order("MJ1", ("date_dim (f)", "store_sales", "item"), ((1, 2), 0), ((0, 1), 2), 300.0, 9000.0)
REPLANNED = order("MJ1", ("m0", "item"), (0, 1), (1, 0), 400.0, 900.0, flipped=((0b10, 0b01),))
AGAIN = order("MJ1", ("m0", "item"), (1, 0), (1, 0), 400.0, 400.0)
OUTER = order("MJ2", ("customer", "agg(MJ1)"), (0, 1), (0, 1), 50.0, 50.0)
# Inside the second pass's build: its own probe plan's replan orders it again, unchanged.
WEEK = order("MJ3", ("date_dim#1 (f)", "date_dim#2"), (0, 1), (0, 1), 5.0, 5.0)
MISS = BuildMiss("join on d_date_sk = ss_sold_date_sk", 5000, 50.0, "MJ1")
OTHER = BuildMiss("join on c_customer_sk = ss_customer_sk", 900, 90.0, "MJ2")
OWN = BuildMiss("join on i_item_sk = ss_item_sk", 70, 7.0, "MJ1")

REPORT = OptimizerReport(
    filters_before_order=FilterPass(
        pruned=(Pruned("store_sales", "ss_sold_date_sk", tuple(range(24)), (17, 18, 23),
                       (PrunedBy("ss_sold_date_sk", KeySummary(2452276, 2452395, None), "p0", "d_date_sk"),),
                       "store_sales#1"),),
        probed=(Probed("p0", 120, 960, (), "date_dim (f)"),)),
    orders=(OUTER, BEFORE),
    filters_after_order=FilterPass(
        pruned=(Pruned("web_sales", "ws_sold_date_sk", tuple(range(6)), (2,),
                       (PrunedBy("ws_sold_date_sk", KeySummary(2451541, 2451543, (2451541, 2451542, 2451543)), "p1",
                                 "d_date_sk"),)),
                Pruned("catalog_sales", "cs_sold_date_sk", (0, 1), (),
                       (PrunedBy("cs_sold_date_sk", KeySummary(None, None, ()), "p1", "d_date_sk"),))),
        probed=(Probed("p1", 3, 24, ("MJ3",), "[date_dim#1 (f)] join date_dim#2"),),
        replans=(("p1", Replan(BuildMiss("join on d_week_seq = d_week_seq", 3, 30.0, "MJ3"),
                               (Kept("m1", 3, "date_dim#1 (f)"),), (WEEK,))),),
        refused=(("p1", BuildMiss("join on d_date = d_date", 9, 90.0, "MJ3")),)),
    replans=(Replan(MISS, (Kept("m0", 5000, "[p0] join store_sales", ("p0",)),), (OUTER, REPLANNED)),
             Replan(OTHER, (), (AGAIN,)), Replan(OWN, (), (AGAIN,))),
    refused=(BuildMiss("nested loop join", 3, 12.0),),
    before="a\n", after="b\n")

EXPECTED = """
MJ2 kept the plan's order: `[customer] join agg(MJ1)`.

MJ3 kept the plan's order: `[date_dim#1 (f)] join date_dim#2`.

A replan was refused, work outside its builds having started: the build of a nested loop join came out 3 rows, est 12.0 (q-error 4.00).

### q5 MJ1

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[[store_sales] join item] join date_dim (f)` | `[[date_dim (f)] join store_sales] join item` | `[item] join m0` |
| changed by | | reordered by DPhyp | the build of MJ1's join on d_date_sk = ss_sold_date_sk came out 5000 rows, est 50.0 (q-error 100.00); flip: `item` now builds |
| C_out | 9,000 B | 300 B (plan's order 9,000 B) | 400 B (previous order 900 B) |

Calls that left the order unchanged: replan 2; replan 3 (MJ1's missed build came out 70 rows, est 7.0).

m0 = `[p0] join store_sales`, 5,000 rows. p0 = `date_dim (f)`, 120 rows.

### q5 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales#1 | ss_sold_date_sk | p0.d_date_sk | 2452276 to 2452395 | 24 → 3 (17-18,23) |
| after join order | web_sales | ws_sold_date_sk | p1.d_date_sk | 2451541, 2451542, 2451543 | 6 → 1 (2) |
| after join order | catalog_sales | cs_sold_date_sk | p1.d_date_sk | none: every row group pruned | 2 → 0 |

p0 = `date_dim (f)`, 120 rows. p1 = `[date_dim#1 (f)] join date_dim#2`, 3 rows, holding MJ3.

Probe p1, replan 1: the build of MJ3's join on d_week_seq = d_week_seq came out 3 rows, est 30.0 (q-error 10.00); m1 = `date_dim#1 (f)`, 3 rows.

Probe p1, a replan refused: the build of MJ3's join on d_date = d_date came out 9 rows, est 90.0 (q-error 10.00).

"""


def test_a_reorder_a_flip_and_a_replan_of_one_cluster_are_one_table_of_its_orders():
    assert section("q5", REPORT) == EXPECTED, section("q5", REPORT)


def test_a_tree_writes_the_build_in_brackets_and_the_kind_sql_gives_it():
    # customer left join orders, the orders building, under nation's filter joined to customer
    left = order("MJ1", ("nation (f)", "customer", "orders"), (0, (1, 2)), (2, (0, 1)), 1.0, 2.0,
                 types=((0b010, 0b100, JoinType.LEFT), (0b100, 0b011, JoinType.RIGHT)))
    page = section("left-join-reorder", OptimizerReport(FilterPass(), (left,), FilterPass(), (), (), "a\n", "b\n"))
    assert "| join tree | `[nation (f)] join ([customer] left join orders)` | " \
           "`([nation (f)] join customer) left join [orders]` |" in page, page


def test_a_memory_source_a_relation_reads_under_its_aggregate_is_in_the_legend():
    replanned = order("MJ1", ("agg(m3)", "item"), (0, 1), (1, 0), 1.0, 2.0, flipped=((0b10, 0b01),))
    reads = RelationEstimate("agg(m3)", 10.0, Certainty.ESTIMATED, 8.0, (), ("m3",))
    replanned = JoinOrder(**{**replanned.__dict__, "relations": (reads,) + replanned.relations[1:]})
    report = OptimizerReport(FilterPass(), (BEFORE,), FilterPass(),
                             (Replan(MISS, (Kept("m3", 7, "[p0] join catalog_sales"),), (replanned,)),), (), "a\n", "b\n")
    assert "\n\nm3 = `[p0] join catalog_sales`, 7 rows.\n\n" in section("q", report), section("q", report)


def test_a_probe_s_replan_is_a_column_of_the_part_of_the_cluster_inside_its_build():
    whole = order("MJ1", ("a", "b", "c"), ((0, 1), 2), ((0, 1), 2), 9.0, 9.0)
    inside = order("MJ1", ("a", "b"), (0, 1), (1, 0), 1.0, 1.0, flipped=((0b10, 0b01),))
    after = FilterPass(probed=(Probed("p0", 1, 8, ("MJ1",), "[a] join b"),),
                       replans=(("p0", Replan(MISS, (), (inside,))),))
    page = section("q", OptimizerReport(FilterPass(), (whole,), after, (), (), "a\n", "b\n"))
    assert "| | plan | inside p0, replan 1 |\n" in page and "Calls that left the order unchanged: DPhyp before the run." in page, page
    # The build a column names is in the legend, whether or not a relation reads it.
    assert "\n\np0 = `[a] join b`, 1 rows.\n\n" in page, page


def test_a_build_named_only_by_a_call_that_kept_the_order_is_in_the_legend():
    changed = order("MJ1", ("a", "b"), (0, 1), (1, 0), 1.0, 1.0, flipped=((0b10, 0b01),))
    kept = order("MJ1", ("a", "b"), (0, 1), (0, 1), 1.0, 1.0)
    after = FilterPass(probed=(Probed("p0", 1, 8, ("MJ1",), "[a] join b"),),
                       replans=(("p0", Replan(MISS, (), (kept,))),))
    page = section("q", OptimizerReport(FilterPass(), (changed,), after, (), (), "a\n", "b\n"))
    assert ("Calls that left the order unchanged: inside p0, replan 1 (MJ1's missed build came out 5000 rows, "
            "est 50.0).\n\np0 = `[a] join b`, 1 rows.\n") in page, page


def test_a_call_at_its_budget_keeps_the_plan_s_order_and_says_so():
    stopped = order("MJ1", ("a", "b"), (0, 1), (1, 0), 1.0, 1.0, flipped=((0b10, 0b01),), unsolved="budget")
    page = section("q", OptimizerReport(FilterPass(), (stopped,), FilterPass(), (), (), "a\n", "b\n"))
    assert "| changed by | | DPhyp at its budget, the plan's order kept; flip: `b` now builds |" in page, page


def test_a_report_where_nothing_fired_says_so_and_a_skipped_run_its_reason():
    plan = "a\n"
    assert section("q1", OptimizerReport(FilterPass(), (), FilterPass(), (), (), plan, plan)) == "\nNothing fired.\n\n"
    assert note("skipped: refused by datafusion: SanityCheckPlan\n") == \
        "\nskipped: refused by datafusion: SanityCheckPlan\n\n"


def test_the_preamble_names_the_page_and_the_log_beside_it():
    text = preamble("tpcds", "tp4-single")
    assert text.startswith("# tpcds tp4-single: join orders and dynamic filters\n\n")
    assert "[tp4-single.optimizer.txt](tp4-single.optimizer.txt)" in text and "## " not in text
    assert "`MJ2` alone" in text


def test_the_anchors_are_the_ones_github_gives_the_headings():
    assert anchor("q2") == "#q2" and anchor("left-join-reorder") == "#left-join-reorder"
    assert anchor(cluster_heading("q2", "MJ1")) == "#q2-mj1"
    assert anchor(cluster_heading("left-join-reorder", "MJ12")) == "#left-join-reorder-mj12"
    assert anchor(filters_heading("q58")) == "#q58-dynamic-filters"
    # What the headings keep out: GitHub drops a `·`, a `:` or a `→` and leaves the spaces round it.
    assert anchor("q2 · MJ1") == "#q2--mj1" and anchor("q2: MJ1") == "#q2-mj1" and anchor("a → b") == "#a--b"


def test_every_heading_is_its_anchor_with_nothing_dropped_and_none_repeats():
    page = preamble("tpch", "tp1-single") + "".join(f"## {q}\n{section(q, REPORT)}" for q in ("q2", "q5"))
    headings = re.findall(r"^#+ (.*)$", page, re.M)[1:]
    assert headings == ["q2", "q2 MJ1", "q2 dynamic filters", "q5", "q5 MJ1", "q5 dynamic filters"], headings
    assert all(anchor(h) == "#" + h.lower().replace(" ", "-") for h in headings)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
