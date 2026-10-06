"""Whole queries through the real operators: every partitioning config gives one answer.

This is the prototype's version of two-engine correctness. Each query is built at several
(partitions, row-group size, batch target) settings and every one must equal a single-shot
pandas oracle — so a bug that only appears once rows are split across batches or lanes has
somewhere to show. That is the class the whole mode exists to create: partial aggregates
merged out of order, a join whose finish pass runs per lane, a sort whose batches are
individually ordered and collectively not.

Joins live in `test_join_capability.py`, which shares this file's helpers — one case per
join mode on two backends outgrew this file.

pandas is imported unconditionally. If it is missing this file fails rather than skipping:
a skipped operator suite reads exactly like a passing one.
"""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import numpy as np
import pandas as pd

from .harness import main, raises
from .rescan import CheckedDriver
from ..errors import ResidentBudgetExceeded
from ..node import CpuBackendSelector
from ..operators import nodes as N
from ..operators.aggregates import GROUPING_ID, PlanAggregate, PlanCall
from ..operators.expressions import Alias, Binary, Case, Col, Lit, Sqrt
from ..plan import Plan

#: (n_partitions, rows_per_group, target_batch_rows). The first is the degenerate
#: single-partition single-batch case — the shape the oracle itself has.
CONFIGS = [(1, 1000, None), (1, 5, 10), (4, 5, 10), (3, 4, 4), (8, 3, 3)]


def fixture(n=60, seed=7):
    rng = np.random.default_rng(seed)
    return pd.DataFrame(
        {
            "k": rng.integers(0, 6, n),
            "g": rng.choice(list("xyz"), n),
            "v": rng.integers(1, 100, n),
        }
    )


#: Every plan here runs under a real budget rather than `None`, so the accounting path is
#: live on each call and a regression that blew the resident set fails rather than passing
#: quietly. Generous against these fixtures (peaks are kilobytes) and far from unbounded.
BUDGET = 8 * 1024 * 1024


def execute(root, budget: int | None = BUDGET, selector=None):
    driver = CheckedDriver(
        Plan.build(root), selector or CpuBackendSelector(), budget
    )
    driver.run()
    frames = [b.frame for b in driver.results if len(b.frame)]
    if not frames:
        return pd.DataFrame(), driver
    return pd.concat(frames, ignore_index=True), driver


def canonical(frame: pd.DataFrame) -> pd.DataFrame:
    """Row order is not part of the contract unless a sort says so — compare as a set."""
    if frame.empty:
        return frame
    return frame.sort_values(list(frame.columns)).reset_index(drop=True)


def same(got: pd.DataFrame, want: pd.DataFrame, label: str) -> None:
    got, want = canonical(got), canonical(want)
    assert list(got.columns) == list(want.columns), f"{label}: {list(got.columns)} vs {list(want.columns)}"
    assert len(got) == len(want), f"{label}: {len(got)} rows vs {len(want)}"
    for column in want.columns:
        left, right = got[column].to_numpy(), want[column].to_numpy()
        # pandas' own predicate, not np.issubdtype: from pandas 3 a string column is a
        # StringDtype rather than object, and numpy cannot interpret an extension dtype
        # at all — it raises instead of answering "not a number". CI installs the current
        # pandas, so the prototype has to hold across the 2/3 boundary.
        if pd.api.types.is_numeric_dtype(want[column]):
            assert np.allclose(left.astype(float), right.astype(float), equal_nan=True), (
                f"{label}: column {column} differs"
            )
        else:
            # NaN != NaN, so an object column holding nulls — which a masked grouping-set
            # key is — cannot be compared with ==. Normalize the nulls first.
            def nulls_alike(values):
                return [None if pd.isna(v) else v for v in values]

            assert nulls_alike(left) == nulls_alike(right), f"{label}: column {column} differs"


# -- aggregates, as the planner decomposes them -------------------------------------
#
# An init `GpuAggregate` per batch, a per-lane `GpuAggregateBatches` merging its state, a
# shuffle on the group keys, and a final `GpuAggregateBatches` merging again and finalizing:
# the sequence an engine plan states, over named columns rather than an engine plan's text.


def body(keys, calls, state, final=None, output=None, masks=None) -> PlanAggregate:
    """An aggregate node's body: `calls` as (func, args, outputs) over named columns, and
    `final` aliased to the outputs after the keys, as an engine plan names them."""
    output = tuple(output or state)
    if final is not None:
        final = tuple(Alias(expr, name) for expr, name in zip(final, output[len(keys):]))
    return PlanAggregate(tuple(Col(k) for k in keys),
                         tuple(PlanCall(f, args, outputs) for f, args, outputs in calls),
                         masks, tuple(state), final, output)


def typed(phase: PlanAggregate, rows: pd.DataFrame) -> dict:
    """The `{column: dtype}` a phase emits, read off a run over no rows: what a lane that
    received nothing must still emit."""
    return dict(phase.emit(phase.aggregate(rows.iloc[0:0])).dtypes)


def merge_of(init: PlanAggregate) -> list:
    """The calls that merge an init's state, in its order: a count merges by sum, and
    Welford's `x$count`, `x$mean` and `x$m2` together by one merge_m2."""
    welford = {call.outputs[0].rpartition("$")[0] for call in init.calls if call.func == "mean"}
    calls = []
    for call in init.calls:
        stem, _, part = call.outputs[0].rpartition("$")
        if stem not in welford:
            calls.append(({"count": "sum"}.get(call.func, call.func), (Col(call.outputs[0]),),
                          call.outputs))
        elif part == "mean":
            state = tuple(f"{stem}${p}" for p in ("count", "mean", "m2"))
            calls.append(("merge_m2", tuple(Col(o) for o in state), state))
    assert [o for _, _, outputs in calls for o in outputs] == [
        o for call in init.calls for o in call.outputs], "a merge emits its init's state"
    return calls


def aggregate_over(child, rows, lanes, keys, init_calls, final, output, masks=None):
    """The whole sequence above `child`, whose rows are `rows`' columns, finishing on `lanes`.
    `final` and `output` name the finalized columns after the keys; None emits the state."""
    keys = list(keys)
    group_keys = keys + ([GROUPING_ID] if masks else [])
    state = group_keys + [o for _, _, outputs in init_calls for o in outputs]
    init = body(keys, init_calls, state, masks=masks)
    merge = body(group_keys, merge_of(init), state)
    finish = body(group_keys, merge_of(init), state, final,
                  output and group_keys + list(output))
    partial = N.plan_aggregate("agg_init", child, init)
    compacted = N.plan_aggregate_batches("agg_batches", partial, merge, typed(init, rows))
    # GpuCoalesceAllBatches between the merge and the emit: the emit then makes one
    # scatter call and hands the final aggregate N batches rather than L*N. Hashed on the
    # user keys only where sets are expanded: the id is a group column the shuffle skips.
    shuffle_in = N.coalesce_all("shuffle_in", N.merge_partitions("merge", compacted))
    emitted = N.emit_partitions("emit", shuffle_in, keys, lanes)
    final_schema = dict(finish.emit(merge.aggregate(init.aggregate(rows.iloc[0:0]))).dtypes)
    return N.unload("unload", N.plan_aggregate_batches("agg_final", emitted, finish, final_schema))


def shuffled_aggregate(df, config, init_calls, final, output, keys=("g",), masks=None):
    parts, group, target = config
    scan = N.scan("scan", df, parts, group, target)
    filtered = N.filter_("filter", scan, Binary(">", Col("v"), Lit(20)))
    return aggregate_over(filtered, df, parts, keys, init_calls, final, output, masks)


#: sum(v), avg(v) and count(*), as the planner states them.
GROUPED = (
    [("sum", (Col("v"),), ("sum_v",)), ("sum", (Col("v"),), ("avg$sum",)),
     ("count", (Col("v"),), ("avg$count",)), ("count", (Lit(1),), ("n",))],
    (Col("sum_v"), Binary("/", Col("avg$sum"), Col("avg$count")), Col("n")),
    ("sum_v", "avg_v", "n"),
)


def test_grouped_aggregate_matches_the_oracle_at_every_config():
    df = fixture()
    sub = df[df.v > 20]
    want = (
        sub.groupby("g", dropna=False)
        .agg(sum_v=("v", "sum"), avg_v=("v", "mean"), n=("v", "size"))
        .reset_index()
    )
    for config in CONFIGS:
        got, _ = execute(shuffled_aggregate(df, config, *GROUPED))
        same(got, want, f"grouped aggregate {config}")


def test_keyless_aggregate_matches_the_oracle_at_every_config():
    df = fixture()
    calls = [("sum", (Col("v"),), ("sum_v",)), ("min", (Col("v"),), ("min_v",)),
             ("max", (Col("v"),), ("max_v",))]
    state = ["sum_v", "min_v", "max_v"]
    init = body([], calls, state)
    merge = body([], merge_of(init), state)
    want = pd.DataFrame([{"sum_v": df.v.sum(), "min_v": df.v.min(), "max_v": df.v.max()}])
    for parts, group, target in CONFIGS:
        scan = N.scan("scan", df, parts, group, target)
        compacted = N.plan_aggregate_batches("agg_batches", N.plan_aggregate("agg_init", scan, init),
                                             merge, typed(init, df))
        # Keyless needs no shuffle — collapse the lanes and finish once.
        collapsed = N.merge_partitions("merge", compacted)
        root = N.unload("unload", N.plan_aggregate_batches("agg_final", collapsed, merge))
        got, _ = execute(root)
        same(got, want, f"keyless aggregate {(parts, group, target)}")


def test_top_n_sort_matches_the_oracle_at_every_config():
    df = fixture()
    want = df.sort_values(["v", "k"], ascending=[False, True]).head(10).reset_index(drop=True)
    for parts, group, target in CONFIGS:
        scan = N.scan("scan", df, parts, group, target)
        per_batch = N.sort("sort", scan, ["v", "k"], ascending=[False, True], fetch=10)
        merged = N.merge_sorted_partitions(
            "merge_sorted", per_batch, ["v", "k"], ascending=[False, True], fetch=10
        )
        got, _ = execute(N.unload("unload", merged))
        # A top-N IS order-sensitive, so compare positionally rather than as a set.
        assert list(got.v) == list(want.v), f"top-n {(parts, group, target)}"


def test_a_single_lane_top_n_trims_at_every_stage():
    # The other half of the sort decomposition: within one lane it is
    # GpuAccumulateBatchesAndSort that carries the fetch, not GpuMergeSortedPartitions.
    # Without a fetch there the lane would accumulate and sort its whole stream to
    # return ten rows — the failure the limit lowering exists to avoid elsewhere.
    df = fixture()
    want = df.sort_values(["v", "k"], ascending=[False, True]).head(10).reset_index(drop=True)
    for parts, group, target in CONFIGS:
        scan = N.scan("scan", df, parts, group, target)
        per_batch = N.sort("sort", scan, ["v", "k"], ascending=[False, True], fetch=10)
        per_lane = N.accumulate_and_sort(
            "accum_sort", per_batch, ["v", "k"], ascending=[False, True], fetch=10,
            schema=dict(df.dtypes),
        )
        merged = N.merge_sorted_partitions(
            "merge_sorted", per_lane, ["v", "k"], ascending=[False, True], fetch=10
        )
        got, _ = execute(N.unload("unload", merged))
        assert list(got.v) == list(want.v), f"single-lane top-n {(parts, group, target)}"


def test_a_top_n_holds_only_the_fetch_at_each_stage():
    # What the per-stage fetch buys, asserted as residency rather than as rows: the
    # accumulator never holds more than the fetch per batch it has seen, so a top-10 over
    # a 60-row table is bounded by the limit and not by the input.
    df = fixture()
    scan = N.scan("scan", df, 1, 5, 10)
    per_batch = N.sort("sort", scan, ["v"], ascending=[False], fetch=3)
    per_lane = N.accumulate_and_sort("accum_sort", per_batch, ["v"], ascending=[False],
                                     fetch=3, schema=dict(df.dtypes))
    got, driver = execute(N.unload("unload", per_lane))
    assert list(got.v) == list(df.v.sort_values(ascending=False).head(3))
    # Each incoming batch was trimmed to 3 by the sort, so what the accumulator held is a
    # multiple of the fetch, never the 60 rows the table has.
    sorted_out = [e for e in driver.trace if e.node.startswith("sort#")]
    assert len(sorted_out) > 3, "the input needs several batches for this to mean anything"


def stddev(count, m2, ddof, root=True):
    """The planner's finalize of a Welford state: NULL where count - ddof <= 0."""
    divisor = Binary("-", Col(count), Lit(float(ddof)))
    value = Binary("/", Col(m2), divisor)
    return Case(whens=((Binary("<=", divisor, Lit(0.0)), Lit(np.nan)),),
                otherwise=Sqrt(value) if root else value)


def test_every_corpus_aggregate_matches_the_oracle_at_every_config():
    # The functions an engine plan's aggregates call: sum, count, min, max, and avg as sum
    # and count; stddev and var as Welford's count, mean and m2, merged by merge_m2 — the
    # only state whose merge is not a plain re-aggregation.
    df = fixture()
    v = (Col("v"),)
    calls = [
        ("sum", v, ("sum_v",)), ("count", (Lit(1),), ("n_rows",)), ("count", v, ("n_v",)),
        ("sum", v, ("avg$sum",)), ("count", v, ("avg$count",)),
        ("min", v, ("min_v",)), ("max", v, ("max_v",)),
        ("count", v, ("w$count",)), ("mean", v, ("w$mean",)), ("m2", v, ("w$m2",)),
    ]
    final = (
        Col("sum_v"), Col("n_rows"), Col("n_v"), Binary("/", Col("avg$sum"), Col("avg$count")),
        Col("min_v"), Col("max_v"),
        stddev("w$count", "w$m2", 1), stddev("w$count", "w$m2", 0),
        stddev("w$count", "w$m2", 1, root=False), stddev("w$count", "w$m2", 0, root=False),
    )
    output = ("sum_v", "n_rows", "n_v", "avg_v", "min_v", "max_v",
              "sd_samp", "sd_pop", "var_samp", "var_pop")
    sub = df[df.v > 20]
    want = (
        sub.groupby("g", dropna=False)
        .agg(
            sum_v=("v", "sum"), n_rows=("v", "size"), n_v=("v", "count"),
            avg_v=("v", "mean"), min_v=("v", "min"), max_v=("v", "max"),
            sd_samp=("v", lambda s: s.std(ddof=1)), sd_pop=("v", lambda s: s.std(ddof=0)),
            var_samp=("v", lambda s: s.var(ddof=1)), var_pop=("v", lambda s: s.var(ddof=0)),
        )
        .reset_index()
    )
    for config in CONFIGS:
        got, _ = execute(shuffled_aggregate(df, config, calls, final, output))
        same(got, want, f"every aggregate {config}")


def test_a_rollup_matches_the_oracle_at_every_config():
    # GROUP BY ROLLUP(g, k): the init expands into three sets in one batch, and every node
    # above groups on the keys plus __grouping_id as if it were an ordinary column. Nothing
    # in the sequence is grouping-set aware except that first node.
    df = fixture()
    calls = [("sum", (Col("v"),), ("sum_v",)), ("count", (Lit(1),), ("n",))]
    masks = ((False, False), (False, True), (True, True))
    sub = df[df.v > 20]
    sets = []
    for held, grouping_id in ((["g", "k"], 0), (["g"], 1), ([], 3)):
        if held:
            one = sub.groupby(held).agg(sum_v=("v", "sum"), n=("v", "size")).reset_index()
        else:
            one = pd.DataFrame([{"sum_v": sub.v.sum(), "n": len(sub)}])
        for key in ("g", "k"):
            if key not in held:
                one[key] = np.nan
        one[GROUPING_ID] = grouping_id
        sets.append(one[["g", "k", GROUPING_ID, "sum_v", "n"]])
    want = pd.concat(sets, ignore_index=True)
    for config in CONFIGS:
        got, _ = execute(shuffled_aggregate(df, config, calls, (Col("sum_v"), Col("n")),
                                            ("sum_v", "n"), keys=("g", "k"), masks=masks))
        same(got, want, f"rollup {config}")


def test_select_distinct_is_an_aggregate_with_no_aggregators():
    # `SELECT DISTINCT g, k` — group keys, no calls, no `final`. Dedup is idempotent and
    # associative, so per batch, per lane and post-shuffle all compose.
    df = fixture()
    want = df[df.v > 20][["g", "k"]].drop_duplicates().reset_index(drop=True)
    for config in CONFIGS:
        got, _ = execute(shuffled_aggregate(df, config, [], None, None, keys=("g", "k")))
        same(got, want, f"select distinct {config}")


def test_union_of_two_branches_matches_the_oracle():
    df = fixture()
    left_want = df[df.v > 60][["k", "v"]]
    right_want = df[df.v <= 10][["k", "v"]]
    want = pd.concat([left_want, right_want], ignore_index=True)
    for parts, group, target in CONFIGS:
        exprs = [Alias(Col("k"), "k"), Alias(Col("v"), "v")]
        left = N.project(
            "lp", N.filter_("lf", N.scan("ls", df, parts, group, target), Binary(">", Col("v"), Lit(60))), exprs
        )
        right = N.project(
            "rp", N.filter_("rf", N.scan("rs", df, parts, group, target), Binary("<=", Col("v"), Lit(10))), exprs
        )
        got, _ = execute(N.unload("unload", N.union("union", [left, right])))
        same(got, want, f"union {(parts, group, target)}")


def test_projection_arithmetic_matches_the_oracle():
    df = fixture()
    want = pd.DataFrame({"k": df.k, "double_v": df.v * 2, "flag": df.v > 50})
    for parts, group, target in CONFIGS:
        exprs = [
            Alias(Col("k"), "k"),
            Alias(Binary("*", Col("v"), Lit(2)), "double_v"),
            Alias(Binary(">", Col("v"), Lit(50)), "flag"),
        ]
        got, _ = execute(N.unload("unload", N.project("p", N.scan("s", df, parts, group, target), exprs)))
        same(got, want, f"projection {(parts, group, target)}")


def test_a_root_adjacent_limit_matches_the_oracle_at_every_config():
    # The lowering with no node, against real operators: `skip`/`fetch` on the unload, the
    # driver stopping part-way through a sorted stream. `test_limit.py` pins which calls
    # are made; this pins that the rows they bring back are the right ones.
    df = fixture()
    want = df.sort_values(["v", "k"], ascending=[False, True]).head(7).reset_index(drop=True)
    for parts, group, target in CONFIGS:
        scan = N.scan("scan", df, parts, group, target)
        per_batch = N.sort("sort", scan, ["v", "k"], ascending=[False, True], fetch=7)
        merged = N.merge_sorted_partitions(
            "merge_sorted", per_batch, ["v", "k"], ascending=[False, True]
        )
        got, driver = execute(N.unload("unload", merged, fetch=7))
        assert driver.plan.row_limit is not None, "the sink should carry the interval"
        assert list(got.v) == list(want.v), f"root-adjacent limit {(parts, group, target)}"


def test_a_mid_plan_limit_matches_the_oracle_at_every_config():
    # The other lowering: the limit's output feeds more work, so it stays a node,
    # streaming its one-partition input and holding nothing. `test_limit.py` pins that it
    # stops as soon as the interval is covered rather than reading the rest.
    df = fixture()
    top = df.sort_values(["v", "k"], ascending=[False, True]).iloc[2:9]
    want = pd.DataFrame({"k": top.k.to_numpy(), "double_v": top.v.to_numpy() * 2})
    for parts, group, target in CONFIGS:
        scan = N.scan("scan", df, parts, group, target)
        per_batch = N.sort("sort", scan, ["v", "k"], ascending=[False, True], fetch=9)
        merged = N.merge_sorted_partitions(
            "merge_sorted", per_batch, ["v", "k"], ascending=[False, True]
        )
        limited = N.limit("limit", merged, skip=2, fetch=7)
        exprs = [Alias(Col("k"), "k"), Alias(Binary("*", Col("v"), Lit(2)), "double_v")]
        got, driver = execute(N.unload("unload", N.project("p", limited, exprs)))
        assert driver.plan.row_limit is None, "a mid-plan limit stays a node of its own"
        assert list(got.double_v) == list(want.double_v), f"mid-plan limit {(parts, group, target)}"


def test_the_accountant_is_actually_engaged_in_these_runs():
    # A budget of None would make every plan above pass whatever the accounting did. This
    # asserts the budget is live: the same plan trips when the budget is small enough.
    df = fixture()
    driver = None
    for config in CONFIGS:
        _, driver = execute(shuffled_aggregate(df, config, *GROUPED))
        assert driver.accountant.peak > 0
        assert driver.accountant.peak <= BUDGET

    with raises(ResidentBudgetExceeded):
        execute(shuffled_aggregate(df, CONFIGS[0], *GROUPED), budget=1)


def test_every_config_agrees_with_every_other():
    # The single-partition single-batch config is the oracle's own shape, so agreeing with
    # it is the same claim as agreeing with pandas — but this states it directly, which is
    # what a regression in the batching policy would break first.
    df = fixture()
    baseline = None
    for config in CONFIGS:
        got, _ = execute(shuffled_aggregate(df, config, *GROUPED))
        if baseline is None:
            baseline = got
        else:
            same(got, baseline, f"config {config} against the single-batch baseline")


if __name__ == "__main__":
    raise SystemExit(main(globals()))
