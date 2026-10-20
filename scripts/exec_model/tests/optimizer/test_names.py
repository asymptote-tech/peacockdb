"""How the report names what a plan reads: a relation by its table, a filter and an aggregate on it,
a table one cluster reads twice numbered, and a join tree written with its build in brackets and
its kind as SQL names it."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

from ..harness import main
from .test_cardinality import scan
from ...optimizer.multijoin import clusters, named
from ...optimizer.names import (
    aliased, described, described_sources, dropped_by_source, is_aggregate_output, join_words, memory_sources,
    relation_name, sourced,
)
from ...plans.engine_plan import parse_plans

L = "lanes=1, batches=multiple"


def plan(text):
    return parse_plans(f"== q\n{text}\n", "test")["q"]


def indented(text, by):
    return "\n".join(" " * by + line for line in text.splitlines())


def filtered(table, columns):
    return f"GpuFilter: predicate={columns[-1]}@{len(columns) - 1} = 1, {L}, schema=[{', '.join(f'{c}:Int64' for c in columns)}]\n" \
           f"  {scan(table, columns)}"


def hash_join(join_type, key, build, probe, schema):
    return f"GpuHashJoin: join_type={join_type}, on=[({key}@0, {key}@0)], {L}, schema=[{schema}]\n" \
           f"{indented(build, 2)}\n{indented(probe, 2)}"


# A two-phase aggregate over a cluster of two scans, and a filter on its output: a HAVING.
HAVING = f"""GpuFilter: predicate=n@1 > 3, {L}, schema=[k:Int64, n:Int64]
  GpuAggregateBatches: group_by=[k@0], aggs=[sum(n@1) as n], final=[n@1], lanes=1, batches=single, schema=[k:Int64, n:Int64]
    GpuEmitPartitions: hash=[k@0], lanes=1, batches=multiple, schema=[k:Int64, n:Int64]
      GpuAggregate: group_by=[k@0], aggs=[count(k@0) as n], {L}, schema=[k:Int64, n:Int64]
{indented(hash_join("Inner", "k", scan("a", ["k"]), scan("b", ["k"]), "k:Int64"), 8)}"""


def test_a_relation_is_its_table_marked_where_filtered_and_an_aggregate_names_the_cluster_under_it():
    assert relation_name(plan(scan("nation", ["n"]))) == "nation"
    assert relation_name(plan(filtered("region", ["r", "x"]))) == "region (f)"
    having = named(plan(HAVING))
    # Both phases are one aggregate; the filter above it is on its output.
    assert relation_name(having) == "agg(MJ1) (f)"
    assert relation_name(having.children[0]) == "agg(MJ1)"
    # Over no cluster, an aggregate names what it reads.
    over_scan = plan(f"GpuAggregate: group_by=[k@0], aggs=[], {L}, schema=[k:Int64]\n"
                     + indented(filtered("lines", ["k", "d"]), 2))
    assert relation_name(over_scan) == "agg(lines (f))"
    source = plan("GpuMemorySource: name=p0, lanes=1, batches=single, schema=[k:Int64]")
    assert relation_name(source) == "p0"
    union = plan(f"GpuUnion: lanes=1, batches=multiple, schema=[k:Int64]\n  {scan('s', ['k'])}\n  {scan('w', ['k'])}")
    assert relation_name(union) == "union(s, w)"


# t ⋈ (t ⋈ σt): one cluster reading t three times, and u, read once, joined above it.
THRICE = f"""GpuHashJoin: join_type=Inner, on=[(k@0, k@0)], {L}, schema=[k:Int64]
  {scan("u", ["k"])}
{indented(hash_join("Inner", "k", scan("t", ["k"]), hash_join("Inner", "k", scan("t", ["k"]), filtered("t", ["k", "x"]), "k:Int64, k:Int64, x:Int64"), "k:Int64"), 2)}"""


def test_a_table_the_plan_reads_more_than_once_is_numbered_in_plan_order():
    numbered = named(aliased(plan(THRICE)))
    [cluster] = clusters(numbered)
    assert [relation_name(r.root) for r in cluster.relations] == ["u", "t#1", "t#2", "t#3 (f)"]
    # The plan given is not changed.
    assert [relation_name(r.root) for r in clusters(plan(THRICE))[0].relations] == ["u", "t", "t", "t (f)"]


def test_a_table_read_once_by_each_of_two_clusters_is_numbered_across_the_plan():
    # So a scan has one name wherever the report shows it: a cluster, a build, a pruned scan.
    inner = hash_join("Inner", "k", scan("t", ["k"]), scan("u", ["k"]), "k:Int64")
    twice = f"""GpuHashJoin: join_type=Inner, on=[(k@0, k@0)], {L}, schema=[k:Int64]
  {scan("t", ["k"])}
  GpuAggregate: group_by=[k@0], aggs=[], {L}, schema=[k:Int64]
{indented(inner, 4)}"""
    outer, nested = clusters(named(aliased(plan(twice))))
    assert [relation_name(r.root) for r in outer.relations] == ["t#1", "agg(MJ2)"]
    assert [relation_name(r.root) for r in nested.relations] == ["t#2", "u"]


def test_a_join_is_written_with_its_build_bracketed_and_the_kind_sql_gives_its_preserved_side():
    # customer left join orders, the orders building, under nation's filter joined to customer
    inner = hash_join("Inner", "k", filtered("nation", ["k", "x"]), scan("customer", ["k"]), "k:Int64, x:Int64, k:Int64")
    left = plan(hash_join("Right", "k", scan("orders", ["k"]), inner, "k:Int64, k:Int64, x:Int64, k:Int64"))
    assert described(left) == "([nation (f)] join customer) left join [orders]"
    kinds = {"Inner": "[a] join b", "Left": "[a] left join b", "Right": "b left join [a]",
             "Full": "[a] full join b", "LeftSemi": "[a] semi join b", "RightSemi": "b semi join [a]",
             "LeftAnti": "[a] anti join b", "RightAnti": "b anti join [a]", "LeftMark": "[a] mark join b"}
    for kind, words in kinds.items():
        assert described(plan(hash_join(kind, "k", scan("a", ["k"]), scan("b", ["k"]), "k:Int64"))) == words, kind
    cross = plan(f"GpuCrossJoin: {L}, schema=[k:Int64, k:Int64]\n  {scan('a', ['k'])}\n  {scan('b', ['k'])}")
    assert described(cross) == "[a] cross join b"
    # A join on the build side is bracketed whole; a probe one is parenthesised.
    nested = plan(hash_join("Inner", "k", hash_join("Inner", "k", scan("a", ["k"]), scan("b", ["k"]), "k:Int64"),
                            hash_join("LeftSemi", "k", scan("c", ["k"]), scan("d", ["k"]), "k:Int64"), "k:Int64"))
    assert described(nested) == "[[a] join b] join ([c] semi join d)"


def shuffled(table, *, null_key):
    """`table(k, x)` shuffled on `k`, through the planner's `k IS NOT NULL` or a query's `x = 1`."""
    predicate = "k@0 IS NOT NULL" if null_key else "x@1 = 1"
    return f"""GpuCoalesceAllBatches: lanes=4, batches=single, hashed_on=[k@0], schema=[k:Int64, x:Int64]
  GpuEmitPartitions: hash=[k@0], lanes=4, batches=multiple, hashed_on=[k@0], schema=[k:Int64, x:Int64]
    GpuFilter: predicate={predicate}, {L}, schema=[k:Int64, x:Int64]
      {scan(table, ["k", "x"])}"""


def test_the_planner_s_null_key_filter_is_no_filter_in_a_name():
    # As the cluster reads it: a NULL key the join drops matches nothing, so no row is filtered out.
    join = plan(hash_join("Inner", "k", shuffled("a", null_key=True), shuffled("b", null_key=False),
                          "k:Int64, x:Int64, k:Int64, x:Int64"))
    assert described(join) == "[a] join b (f)"
    # A Left join emits its build's unmatched rows: there the filter is the query's.
    left = plan(hash_join("Left", "k", shuffled("a", null_key=True), shuffled("b", null_key=True),
                          "k:Int64, x:Int64, k:Int64, x:Int64"))
    assert described(left) == "[a (f)] left join b"
    # A side named on its own takes the keys the join above it drops.
    assert described(plan(shuffled("a", null_key=True)), frozenset({0})) == "a"
    assert described(plan(shuffled("a", null_key=True))) == "a (f)"
    # Those keys are the ones the join reading the side's memory source drops, found where it is.
    source = "GpuMemorySource: name={}, lanes=1, batches=single, schema=[k:Int64]"
    inner = plan(hash_join("Inner", "k", source.format("p0"), scan("b", ["k"]), "k:Int64"))
    outer = plan(hash_join("Left", "k", source.format("p1"), scan("c", ["k"]), "k:Int64"))
    assert dropped_by_source(inner, outer) == {"p0": frozenset({0}), "p1": frozenset()}


def test_the_memory_sources_a_relation_names_are_those_outside_the_clusters_it_names_by_id():
    source = "GpuMemorySource: name={}, lanes=1, batches=single, schema=[k:Int64]"
    over_cluster = named(plan(f"""GpuAggregate: group_by=[k@0], aggs=[], {L}, schema=[k:Int64]
{indented(hash_join("Inner", "k", source.format("m3"), scan("item", ["k"]), "k:Int64"), 2)}"""))
    assert relation_name(over_cluster) == "agg(MJ1)" and memory_sources(over_cluster) == ()
    over_source = plan(f"GpuAggregate: group_by=[k@0], aggs=[], {L}, schema=[k:Int64]\n  {source.format('m3')}")
    assert relation_name(over_source) == "agg(m3)" and memory_sources(over_source) == ("m3",)
    union = plan(f"GpuUnion: lanes=1, batches=multiple, schema=[k:Int64]\n  {source.format('p0')}\n  {source.format('m1')}")
    assert memory_sources(union) == ("p0", "m1")


def test_an_aggregate_s_output_column_is_told_from_its_keys_through_projects_and_filters():
    agg = f"""GpuProject: exprs=[min(t.v)@1, k@0], lanes=1, batches=single, schema=[min(t.v):Int64, k:Int64]
  GpuAggregateBatches: group_by=[k@0], aggs=[min(v@1) as min(t.v)], final=[min(t.v)@1], lanes=1, batches=single, schema=[k:Int64, min(t.v):Int64]
    {scan("t", ["k", "v"])}"""
    root = plan(agg)
    assert is_aggregate_output(root, 0) and not is_aggregate_output(root, 1)
    assert not is_aggregate_output(plan(scan("t", ["k", "v"])), 1)
    # Read back from memory, the column is the aggregate's still.
    source = sourced("m16", root)
    assert source.kind == "GpuMemorySource" and is_aggregate_output(source, 0) and not is_aggregate_output(source, 1)


def test_a_description_names_the_sources_it_shows_and_not_those_inside_an_aggregate_s_cluster():
    source = "GpuMemorySource: name={}, lanes=1, batches=single, schema=[k:Int64]"
    under_agg = f"""GpuAggregate: group_by=[k@0], aggs=[], {L}, schema=[k:Int64]
{indented(hash_join("Inner", "k", source.format("m3"), scan("item", ["k"]), "k:Int64"), 2)}"""
    side = named(plan(hash_join("Inner", "k", source.format("p0"), under_agg, "k:Int64")))
    assert described(side) == "[p0] join agg(MJ2)" and described_sources(side) == ("p0",)


def test_a_join_that_missed_is_worded_by_its_kind_and_keys():
    joins = {
        f"GpuCrossJoin: {L}, schema=[k:Int64, k:Int64]": "cross join",
        f"GpuNestedLoopJoin: join_type=Left, {L}, schema=[k:Int64, k:Int64]": "nested loop left join",
        f"GpuHashJoin: join_type=RightSemi, on=[(a@0, b@0), (c@1, d@1)], {L}, schema=[b:Int64, d:Int64]":
            "semi join on a = b and c = d",
    }
    for line, words in joins.items():
        assert join_words(plan(f"{line}\n  {scan('x', ['a', 'c'])}\n  {scan('y', ['b', 'd'])}")) == words, line


if __name__ == "__main__":
    raise SystemExit(main(globals()))
