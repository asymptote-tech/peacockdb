"""Dynamic filters, planning half: candidates, the probe plan, the host reducer."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import dataclasses
import functools

import pandas as pd
import pyarrow as pa

from ..corpus import ParquetTables, execute, execute_lanes
from .test_cardinality import dataset
from ..harness import main
from ...optimizer.dynamic_filters import (
    IN_LIMIT, Batching, Builds, KeySummary, Pruned, PrunedBy, apply, candidates, probe_plan, remap,
    row_group_survives, summarize,
)
from ...plans.engine_nodes import build
from ...plans.engine_plan import EngineNode, parse_plans
from ...optimizer.replan import WithMaterialized

#: facts in four row groups of 3: `d` ordered (a date key), `u` cycling through every group
FACTS = pd.DataFrame({"d": list(range(12)), "v": [10 * i for i in range(12)], "u": [i % 3 for i in range(12)]})
DIM = pd.DataFrame({"dk": list(range(12)), "y": [2000 if 3 <= i <= 5 else 1999 for i in range(12)]})
#: a second fact table laid out otherwise: its key second, ordered, in three row groups of 4
RETURNS = pd.DataFrame({"w": [100 + i for i in range(12)], "e": list(range(12))})
#: facts with NULLs in the ordered key `nd` and in `nu`, the key another join reads
NULLS = pd.DataFrame({"nd": [0, 1, 2, 3, 4, 5, 6, 7, None, 9, 10, None],
                      "nu": [0, 1, None, 0, 1, 2, 0, None, 2, 1, 2, 0], "nv": list(range(12))}, dtype="Int64")
HOLDERS = pd.DataFrame({"hk": [0, 2, None, 5], "hv": [100, 101, 102, 103]}, dtype="Int64")


@functools.cache
def statistics():
    """The tables below, written once with their statistics sidecar."""
    frames = {"f": (FACTS, 3), "dim": (DIM, 12), "g": (RETURNS, 4), "n": (NULLS, 3), "h": (HOLDERS, 4)}
    return dataset({name: size for name, (_, size) in frames.items()},
                   {name: pa.Table.from_pandas(rows, preserve_index=False) for name, (rows, _) in frames.items()})


def tables() -> ParquetTables:
    return ParquetTables(statistics().directory)


F = "schema=[d:Int64, v:Int64, u:Int64]"
FACT_SCAN = ("GpuLoadParquet: table=f, projections=[d@0, v@1, u@2], "
             f"partition_groups=[[[0,1,2,3]]], lanes=1, batches=multiple, {F}")
DIM_BUILD = """GpuCoalesceAllBatches: lanes=1, batches=single, schema=[dk:Int64]
      GpuFilter: predicate=y@1 = 2000, projection=[dk@0], lanes=1, batches=multiple, schema=[dk:Int64]
        GpuLoadParquet: table=dim, projections=[dk@0, y@1], partition_groups=[[[0]]], lanes=1, batches=multiple, schema=[dk:Int64, y:Int64]"""

N_COLUMNS = ["nd:Int64", "nu:Int64", "nv:Int64"]
H_COLUMNS = ["hk:Int64", "hv:Int64"]
N_SCAN = ("GpuLoadParquet: table=n, projections=[nd@0, nu@1, nv@2], partition_groups=[[[0,1,2,3]]], "
          f"lanes=1, batches=multiple, schema=[{', '.join(N_COLUMNS)}]")
H_SCAN = ("GpuLoadParquet: table=h, projections=[hk@0, hv@1], partition_groups=[[[0]]], "
          f"lanes=1, batches=multiple, schema=[{', '.join(H_COLUMNS)}]")


def plan(join_type="Inner", key="d@0", probe=FACT_SCAN, build=DIM_BUILD):
    text = f"""== q
GpuUnload
  GpuHashJoin: join_type={join_type}, on=[(dk@0, {key})], projection=[v@2], lanes=1, batches=multiple, schema=[v:Int64]
    {build}
    {probe}
"""
    return parse_plans(text, "test")["q"]


def test_a_filtered_build_prunes_an_ordered_fact_scan_by_its_join_key():
    [found] = candidates(plan(), tables(), Builds.FILTERED)
    assert (found.scan.fields["table"], found.column, found.row_groups) == ("f", "d", (0, 1, 2, 3))
    # Groups span 0–2, 3–5, 6–8, 9–11: their widths sum to 8 of the 11 the column covers.
    assert found.clustering == 8 / 11


def test_no_candidate_where_the_scan_is_not_ordered_by_the_key():
    # `u` spans every group's whole range: its clustering is the group count, 4.
    assert candidates(plan(key="u@2"), tables(), Builds.FILTERED) == []


def test_no_candidate_for_a_join_that_keeps_unmatched_probe_rows():
    assert candidates(plan(join_type="Right"), tables(), Builds.FILTERED) == []
    assert candidates(plan(join_type="Full"), tables(), Builds.FILTERED) == []


def test_no_candidate_without_a_filter_on_the_build_side():
    unfiltered = """GpuCoalesceAllBatches: lanes=1, batches=single, schema=[dk:Int64, y:Int64]
      GpuLoadParquet: table=dim, projections=[dk@0, y@1], partition_groups=[[[0]]], lanes=1, batches=multiple, schema=[dk:Int64, y:Int64]"""
    assert candidates(plan(build=unfiltered), tables(), Builds.FILTERED) == []


#: keys 0, 2 and 5: the dates a holder holds, by a join and no filter
JOINED_BUILD = f"""GpuCoalesceAllBatches: lanes=1, batches=single, schema=[dk:Int64]
      GpuHashJoin: join_type=Inner, on=[(hk@0, dk@0)], projection=[dk@2], lanes=1, batches=multiple, schema=[dk:Int64]
        GpuCoalesceAllBatches: lanes=1, batches=single, schema=[hk:Int64, hv:Int64]
          {H_SCAN}
        GpuLoadParquet: table=dim, projections=[dk@0], partition_groups=[[[0]]], lanes=1, batches=multiple, schema=[dk:Int64]"""


def test_a_build_holding_a_join_is_a_candidate_only_under_the_widened_rule():
    joined = plan(build=JOINED_BUILD)
    assert candidates(joined, tables(), Builds.FILTERED) == []
    [found] = candidates(joined, tables(), Builds.FILTERED_OR_JOINED)
    assert (found.scan.fields["table"], found.column) == ("f", "d")
    [filtered] = candidates(plan(), tables(), Builds.FILTERED_OR_JOINED)
    assert filtered.column == "d"
    t = tables()
    pruned, report, kept = apply(joined, t, Batching.ONE_PER_LANE, probe_on(t), Builds.FILTERED_OR_JOINED)
    # Keys 0, 2 and 5 reach groups 0 (0–2) and 1 (3–5).
    assert pruned_by_table(report) == {"f": ((0, 1, 2, 3), (0, 1))}
    assert sorted(run_on(t, kept)(pruned)["v"]) == sorted(run_on(t)(joined)["v"]) == [0, 20, 50]


def test_no_candidate_on_a_bare_build_under_either_rule():
    bare = plan(build="""GpuCoalesceAllBatches: lanes=1, batches=single, schema=[dk:Int64]
      GpuLoadParquet: table=dim, projections=[dk@0], partition_groups=[[[0]]], lanes=1, batches=multiple, schema=[dk:Int64]""")
    assert candidates(bare, tables(), Builds.FILTERED) == []
    assert candidates(bare, tables(), Builds.FILTERED_OR_JOINED) == []


def test_no_candidate_on_a_scan_of_one_row_group():
    one = FACT_SCAN.replace("partition_groups=[[[0,1,2,3]]]", "partition_groups=[[[2]]]")
    assert candidates(plan(probe=one), tables(), Builds.FILTERED) == []


def test_the_key_lifts_through_a_projection_and_an_inner_join():
    probe = f"""GpuProject: exprs=[v@1, d@0], lanes=1, batches=multiple, schema=[v:Int64, d:Int64]
      GpuHashJoin: join_type=Inner, on=[(u@0, u@2)], projection=[d@1, v@2], lanes=1, batches=multiple, schema=[d:Int64, v:Int64]
        GpuCoalesceAllBatches: lanes=1, batches=single, schema=[u:Int64]
          GpuLoadParquet: table=f, projections=[u@2], partition_groups=[[[0]]], lanes=1, batches=multiple, schema=[u:Int64]
        {FACT_SCAN}"""
    [found] = candidates(plan(key="d@1", probe=probe), tables(), Builds.FILTERED)
    assert (found.column, found.scan.fields["partition_groups"]) == ("d", "[[[0,1,2,3]]]")


def test_the_probe_plan_returns_the_filtered_keys_and_the_reducer_bounds_them():
    [found] = candidates(plan(), tables(), Builds.FILTERED)
    keys, _ = execute(build(probe_plan(found), tables()))
    assert sorted(keys["dk"]) == [3, 4, 5]
    assert summarize(keys["dk"]) == summarize(pd.Series([5, 3, 4]))
    assert summarize(pd.Series([5, 3, 4])).values == (3, 4, 5)


def test_the_reducer_drops_the_in_list_past_its_limit_and_reads_no_key_as_none():
    many = summarize(pd.Series(range(IN_LIMIT + 1)))
    assert (many.low, many.high, many.values) == (0, IN_LIMIT, None)
    nothing = summarize(pd.Series([None, None], dtype="float64"))
    assert (nothing.low, nothing.high, nothing.values) == (None, None, ())


G = "schema=[w:Int64, e:Int64]"
RETURNS_SCAN = ("GpuLoadParquet: table=g, projections=[w@0, e@1], "
                f"partition_groups=[[[0,1,2]]], lanes=1, batches=multiple, {G}")
DV = "schema=[d:Int64, v:Int64]"


def branches(kind="GpuUnion", lanes=2, returns_key="e@1"):
    """Facts and returns under one node, each renamed to `(d, v)`; the returns' key is
    `returns_key`."""
    return f"""GpuMergePartitions: lanes=1, batches=multiple, {DV}
      {kind}: lanes={lanes}, batches=multiple, {DV}
        GpuProject: exprs=[d@0, v@1], lanes=1, batches=multiple, {DV}
          {FACT_SCAN}
        GpuProject: exprs=[{returns_key} as d, w@0 as v], lanes=1, batches=multiple, {DV}
          {RETURNS_SCAN}"""


def pruned_by_table(report):
    return {scan.table: (scan.before, scan.after) for scan in report}


def test_the_key_lifts_through_a_union_into_every_branch_each_pruned_on_its_own_footers():
    union = plan(key="d@0", probe=branches())
    found = candidates(union, tables(), Builds.FILTERED)
    assert sorted((c.scan.fields["table"], c.column, c.row_groups) for c in found) == [
        ("f", "d", (0, 1, 2, 3)), ("g", "e", (0, 1, 2))]
    t = tables()
    pruned, report, kept = apply(union, t, Batching.ONE_PER_LANE, probe_on(t), Builds.FILTERED)
    # Keys 3–5: the facts' group 1 (3–5); the returns' groups 0 (0–3) and 1 (4–7).
    assert pruned_by_table(report) == {"f": ((0, 1, 2, 3), (1,)), "g": ((0, 1, 2), (0, 1))}
    assert len(kept) == 1, "the branches share one probe of the build"
    assert sorted(run_on(t, kept)(pruned)["v"]) == sorted(run_on(t)(union)["v"]) == [30, 40, 50, 103, 104, 105]


def test_a_branch_whose_key_is_computed_stays_whole_and_the_others_are_pruned():
    union = plan(key="d@0", probe=branches(returns_key="e@1 + 0"))
    [found] = candidates(union, tables(), Builds.FILTERED)
    assert found.scan.fields["table"] == "f"
    t = tables()
    pruned, report, kept = apply(union, t, Batching.ONE_PER_LANE, probe_on(t), Builds.FILTERED)
    assert pruned_by_table(report) == {"f": ((0, 1, 2, 3), (1,))}
    assert sorted(run_on(t, kept)(pruned)["v"]) == sorted(run_on(t)(union)["v"]) == [30, 40, 50, 103, 104, 105]


def test_the_key_lifts_through_an_interleave():
    interleaved = plan(key="d@0", probe=branches(kind="GpuInterleave", lanes=1))
    t = tables()
    pruned, report, kept = apply(interleaved, t, Batching.ONE_PER_LANE, probe_on(t), Builds.FILTERED)
    assert pruned_by_table(report) == {"f": ((0, 1, 2, 3), (1,)), "g": ((0, 1, 2), (0, 1))}
    assert sorted(run_on(t, kept)(pruned)["v"]) == sorted(run_on(t)(interleaved)["v"])


#: each join type with the side it outputs that holds `n`: True where `n` builds
OUTPUTS_N = [("Inner", True), ("Inner", False), ("Left", True), ("Left", False), ("Right", True),
             ("Right", False), ("Full", True), ("Full", False), ("LeftSemi", True), ("LeftAnti", True),
             ("LeftMark", True), ("RightSemi", False), ("RightAnti", False)]


def under_a_join(join_type: str, n_builds: bool, null_equals_null: bool) -> tuple[str, list[str]]:
    """`n` joined to `h` on `nu = hk` by `join_type`, and that join's output columns."""
    sides = [(N_SCAN, N_COLUMNS, "nu@1"), (H_SCAN, H_COLUMNS, "hk@0")]
    (build, build_columns, build_key), (probe, probe_columns, probe_key) = sides if n_builds else sides[::-1]
    if join_type in ("LeftSemi", "LeftAnti"):
        columns = build_columns
    elif join_type == "LeftMark":
        columns = build_columns + ["mark:Boolean"]
    elif join_type in ("RightSemi", "RightAnti"):
        columns = probe_columns
    else:
        columns = build_columns + probe_columns
    equal = ", null_equals_null=true" if null_equals_null else ""
    schema = f"schema=[{', '.join(columns)}]"
    text = f"""GpuHashJoin: join_type={join_type}, on=[({build_key}, {probe_key})]{equal}, lanes=1, batches=multiple, {schema}
      GpuCoalesceAllBatches: lanes=1, batches=single, schema=[{', '.join(build_columns)}]
        {build}
      {probe}"""
    return text, columns


def rows(frame: pd.DataFrame) -> list[tuple]:
    return sorted(tuple(str(value) for value in row) for row in frame.itertuples(index=False))


def test_the_key_lifts_into_the_side_each_join_type_outputs():
    t = tables()
    for join_type, n_builds in OUTPUTS_N:
        for null_equals_null in (False, True):
            probe, columns = under_a_join(join_type, n_builds, null_equals_null)
            text = f"""== q
GpuUnload
  GpuHashJoin: join_type=Inner, on=[(dk@0, nd@{columns.index('nd:Int64')})], lanes=1, batches=multiple, schema=[dk:Int64, {', '.join(columns)}]
    {DIM_BUILD}
    {probe}
"""
            joined = parse_plans(text, "test")["q"]
            label = (join_type, n_builds, null_equals_null)
            [found] = candidates(joined, t, Builds.FILTERED)
            assert (found.scan.fields["table"], found.column) == ("n", "nd"), label
            pruned, report, kept = apply(joined, t, Batching.ONE_PER_LANE, probe_on(t), Builds.FILTERED)
            # Keys 3–5 reach group 1 alone; groups 2 and 3 go with their NULL keys.
            assert pruned_by_table(report) == {"n": ((0, 1, 2, 3), (1,))}, label
            assert rows(run_on(t, kept)(pruned)) == rows(run_on(t)(joined)), label


def holders_join(join_type: str, predicate: str, null_equals_null: bool = False):
    """The holders whose `predicate` holds, built, joined by `join_type` to `n` on `hk = nd`."""
    columns = "hk:Int64, hv:Int64" + (", mark:Boolean" if join_type == "LeftMark" else "")
    if join_type == "Inner":
        columns += ", " + ", ".join(N_COLUMNS)
    equal = ", null_equals_null=true" if null_equals_null else ""
    return parse_plans(f"""== q
GpuUnload
  GpuHashJoin: join_type={join_type}, on=[(hk@0, nd@0)]{equal}, lanes=1, batches=multiple, schema=[{columns}]
    GpuCoalesceAllBatches: lanes=1, batches=single, schema=[hk:Int64, hv:Int64]
      GpuFilter: predicate={predicate}, lanes=1, batches=multiple, schema=[hk:Int64, hv:Int64]
        {H_SCAN}
    {N_SCAN}
""", "test")["q"]


#: a join matches NULL to NULL where the plan marks it so, whatever its type
EQUATING_NULLS = ("LeftAnti", "LeftMark", "Inner")


def test_a_join_that_equates_nulls_prunes_nothing_by_a_build_holding_a_null():
    # Holder 102's NULL key matches n's NULL keys, which sit in groups 2 and 3 beside keys 6–10.
    t = tables()
    for join_type in EQUATING_NULLS:
        joined = holders_join(join_type, "hv@1 >= 100", True)
        [found] = candidates(joined, t, Builds.FILTERED)
        pruned, report, kept = apply(joined, t, Batching.ONE_PER_LANE, probe_on(t), Builds.FILTERED)
        assert pruned_by_table(report) == {"n": ((0, 1, 2, 3), (0, 1, 2, 3))}, join_type
        assert rows(run_on(t, kept)(pruned)) == rows(run_on(t)(joined)), join_type


def test_a_join_that_equates_nulls_prunes_by_a_build_without_one():
    t = tables()
    for join_type in EQUATING_NULLS:
        joined = holders_join(join_type, "hk@0 >= 0", True)
        pruned, report, kept = apply(joined, t, Batching.ONE_PER_LANE, probe_on(t), Builds.FILTERED)
        # Keys 0, 2 and 5 reach groups 0 (0–2) and 1 (3–5).
        assert pruned_by_table(report) == {"n": ((0, 1, 2, 3), (0, 1))}, join_type
        assert rows(run_on(t, kept)(pruned)) == rows(run_on(t)(joined)), join_type


def test_an_anti_or_mark_join_prunes_by_a_build_holding_a_null_as_an_inner_join_does():
    # NULL equality is the plan's flag alone, for every join type: without it holder 102's NULL
    # key matches nothing, so it keeps no row group the non-null keys miss.
    t = tables()
    inner = holders_join("Inner", "hv@1 >= 100")
    _, inner_report, _ = apply(inner, t, Batching.ONE_PER_LANE, probe_on(t), Builds.FILTERED)
    assert pruned_by_table(inner_report) != {"n": ((0, 1, 2, 3), (0, 1, 2, 3))}
    for join_type in ("LeftAnti", "LeftMark"):
        joined = holders_join(join_type, "hv@1 >= 100")
        pruned, report, kept = apply(joined, t, Batching.ONE_PER_LANE, probe_on(t), Builds.FILTERED)
        assert pruned_by_table(report) == pruned_by_table(inner_report), join_type
        assert rows(run_on(t, kept)(pruned)) == rows(run_on(t)(joined)), join_type


# -- after the probes -------------------------------------------------------------


def test_a_row_group_goes_where_its_range_misses_the_keys_or_holds_none_of_them():
    bounded = KeySummary(3, 5, None)
    assert not row_group_survives((6, 8), bounded)
    assert row_group_survives((5, 8), bounded)
    few = KeySummary(3, 9, (3, 9))
    assert not row_group_survives((5, 7), few)  # inside the bounds, but neither value is
    assert row_group_survives(None, bounded)  # no statistics, nothing to reason with
    assert not row_group_survives((0, 100), KeySummary(None, None, ()))  # no key at all


def test_survivors_are_laid_out_as_the_engines_partitioner_lays_them():
    # partition.rs: 10 rows over 2 lanes wants 5 in the first; 3 then 6 is closer than
    # stopping at 3, and 9 is further than 6, so the first lane takes groups 0 and 1.
    assert remap([[[0]], [[1]]], [0, 1, 2, 3], [3, 3, 3, 1], Batching.ONE_PER_LANE) == [[[0, 1]], [[2, 3]]]
    assert remap([[[0]], [[1]]], [0, 1, 2, 3], [3, 3, 3, 1], Batching.ONE_PER_ROW_GROUP) == [
        [[0], [1]], [[2], [3]]]
    assert remap([[[0, 1], [2]], [[3]]], [1, 3], [3, 3, 3, 1], Batching.KEEP_BATCHES) == [[[1]], [[3]]]


def test_a_candidate_and_its_pruned_scan_carry_the_name_the_report_gives_the_scan():
    numbered = plan()
    join = numbered.children[0]
    join.children[1] = dataclasses.replace(join.children[1], alias="f#2")
    t = tables()
    [found] = candidates(numbered, t, Builds.FILTERED)
    assert found.scan_name == "f#2" and candidates(plan(), t, Builds.FILTERED)[0].scan_name == "f"
    _, [scanned], _ = apply(numbered, t, Batching.ONE_PER_LANE, probe_on(t), Builds.FILTERED)
    assert (scanned.table, scanned.alias) == ("f", "f#2")


def run_on(tables_, kept=None):
    frames = {name: lanes for name, (_, lanes) in (kept or {}).items()}
    return lambda engine_plan: execute(build(engine_plan, WithMaterialized(tables_, frames)))[0]


def probe_on(tables_):
    def probe(engine_plan, builds):
        frames = {name: lanes for name, (_, lanes) in builds.items()}
        return execute_lanes(build(engine_plan, WithMaterialized(tables_, frames)))

    return probe


def test_apply_reads_only_the_row_groups_the_keys_can_reach_and_answers_the_same():
    t = tables()
    pruned, report, kept = apply(plan(), t, Batching.ONE_PER_LANE, probe_on(t), Builds.FILTERED)
    # Pruned by the dimension's keys `dk`, read from the build p0.
    assert report == [Pruned("f", "d", (0, 1, 2, 3), (1,), (PrunedBy("d", KeySummary(3, 5, (3, 4, 5)), "p0", "dk"),))]
    assert "partition_groups=[[[1]]]" in "".join(f"{k}={v}" for k, v in pruned.children[0].children[1].fields.items())
    before, after = run_on(t)(plan()), run_on(t, kept)(pruned)
    assert sorted(after["v"]) == sorted(before["v"]) == [30, 40, 50]


def test_the_build_a_probe_plan_made_is_the_one_the_main_plan_reads():
    # Every row group survives, so nothing is pruned — and still the dimension is read once:
    # the main plan's build is the probe's, from memory, laid out as the side was.
    every = DIM_BUILD.replace("predicate=y@1 = 2000", "predicate=y@1 >= 1999")
    original = plan(build=every)
    replanned, [probed], kept = apply(original, tables(), Batching.ONE_PER_LANE, probe_on(tables()), Builds.FILTERED)
    assert probed.after == probed.before
    [(name, (side, lanes))] = kept.items()
    assert side is original.children[0].children[0] and sorted(lanes[0]["dk"]) == list(range(12))
    build_side, fact = replanned.children[0].children
    assert (build_side.kind, build_side.fields["name"], fact.fields["partition_groups"]) == \
        ("GpuMemorySource", name, original.children[0].children[1].fields["partition_groups"])
    assert sorted(run_on(tables(), kept)(replanned)["v"]) == sorted(run_on(tables())(original)["v"])


#: The facts' dates that are returns' dates in year 2000: the returns pruned by the year, a join
#: inside the facts' build.
NESTED_BUILD = f"""GpuCoalesceAllBatches: lanes=1, batches=single, schema=[dk:Int64]
      GpuHashJoin: join_type=Inner, on=[(dk@0, e@1)], projection=[dk@0], lanes=1, batches=multiple, schema=[dk:Int64]
        {DIM_BUILD.replace(chr(10), chr(10) + "    ")}
        {RETURNS_SCAN}"""


def counting(run):
    sides = []

    def counted(engine_plan, frames):
        sides.append(engine_plan)
        return run(engine_plan, frames)

    return counted, sides


def nodes(node: EngineNode):
    yield node
    for child in node.children:
        yield from nodes(child)


def test_nested_candidates_probe_the_inner_build_first():
    nested = plan(build=NESTED_BUILD)
    t = tables()
    run, sides = counting(probe_on(t))
    pruned, report, kept = apply(nested, t, Batching.ONE_PER_LANE, run, Builds.FILTERED_OR_JOINED)
    assert pruned_by_table(report) == {"g": ((0, 1, 2), (0, 1)), "f": ((0, 1, 2, 3), (1,))}
    # Each build probed once, the outer one over the inner one's build from memory and the
    # returns pruned.
    assert len(sides) == len(kept) == 2
    [(inner, _), (outer, _)] = kept.values()
    assert inner.kind == "GpuCoalesceAllBatches" and inner.children[0].kind == "GpuFilter"
    read = {node.kind: node for node in nodes(outer)}
    assert read["GpuMemorySource"].fields["name"] == list(kept)[0]
    assert read["GpuLoadParquet"].fields["partition_groups"] == "[[[0,1]]]"
    assert sorted(run_on(t, kept)(pruned)["v"]) == sorted(run_on(t)(nested)["v"]) == [30, 40, 50]


#: The facts, filtered and built, joined to the returns on date: the dimension's keys reach the
#: facts inside that build, and the build's keys the returns.
FACTS_BUILT = f"""GpuHashJoin: join_type=Inner, on=[(d@0, e@1)], projection=[d@0, v@1], lanes=1, batches=multiple, {DV}
      GpuCoalesceAllBatches: lanes=1, batches=single, {F}
        GpuFilter: predicate=v@1 >= 0, lanes=1, batches=multiple, {F}
          {FACT_SCAN}
      {RETURNS_SCAN}"""


def test_a_build_holding_a_scan_another_candidate_prunes_is_probed_after_the_pruning():
    built = plan(probe=FACTS_BUILT)
    t = tables()
    pruned, report, kept = apply(built, t, Batching.ONE_PER_LANE, probe_on(t), Builds.FILTERED)
    # The facts keep group 1 (3–5), so the build's keys 3–5 keep the returns' groups 0 and 1.
    assert pruned_by_table(report) == {"f": ((0, 1, 2, 3), (1,)), "g": ((0, 1, 2), (0, 1))}
    [_, (facts, _)] = kept.values()
    assert [n.fields["partition_groups"] for n in nodes(facts) if n.kind == "GpuLoadParquet"] == ["[[[1]]]"]
    assert rows(run_on(t, kept)(pruned)) == rows(run_on(t)(built))


def test_a_second_pass_names_its_builds_after_the_first_and_its_probes_read_them():
    t = tables()
    first, _, kept = apply(plan(build=NESTED_BUILD), t, Batching.ONE_PER_LANE, probe_on(t), Builds.FILTERED)
    second, report, more = apply(first, t, Batching.ONE_PER_LANE, probe_on(t), Builds.FILTERED_OR_JOINED, kept)
    assert list(kept) == ["p0"] and list(more) == ["p1"]
    assert pruned_by_table(report) == {"f": ((0, 1, 2, 3), (1,))}
    assert sorted(run_on(t, kept | more)(second)["v"]) == [30, 40, 50]


if __name__ == "__main__":
    raise SystemExit(main(globals()))
