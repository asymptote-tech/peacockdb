"""Join cardinality: each rule on tables built so that its answer is known, and so that the rule
it replaced would have missed it."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import json
import pathlib
import tempfile

import pyarrow as pa
import pyarrow.parquet as pq

from ..harness import main
from ...optimizer.cardinality import Certainty, certainty, estimate
from ...plans.engine_plan import parse_plans
from ...optimizer.stats import FORMAT, Statistics, fingerprint

TABLES = {
    # a dimension of 1000 keys over ten "years"; a fact whose 1000 rows cover year 5 only
    "dates": pa.table({"dk": list(range(1000)), "y": [k // 100 for k in range(1000)],
                       "m": [k % 7 for k in range(1000)]}),
    "sales": pa.table({"sk": [500 + i // 10 for i in range(1000)]}),
    # a fact over every tenth key of the same 1000
    "spread": pa.table({"pk": [(i // 10) * 10 for i in range(1000)]}),
    # ten customers; orders for the first five
    "cust": pa.table({"ck": list(range(10))}),
    "orders": pa.table({"ok": [i % 5 for i in range(40)]}),
    # line items keyed by (item, order), a day in 0..9 each; returns for every tenth line
    "lines": pa.table({"li": [i % 50 for i in range(1000)], "lo": [i // 50 for i in range(1000)],
                       "ld": [(i // 10) % 10 for i in range(1000)]}),
    "returns": pa.table({"ri": [i % 50 for i in range(0, 1000, 10)], "ro": [i // 50 for i in range(0, 1000, 10)]}),
}
#: the key sets counted whole, as `testdata/gen_stats.py` counts them
COMPOSITE = {"lines": [["li", "lo"]], "returns": [["ri", "ro"]]}


def dataset(row_group_size: int | dict | None = None, tables=None) -> Statistics:
    """`tables`, `TABLES` by default, written as a dataset with its statistics sidecar; a dict of
    `row_group_size` gives it per table."""
    root = pathlib.Path(tempfile.mkdtemp())
    directory = root / "toy.sf1"
    directory.mkdir()
    sidecar = {}
    for name, table in (tables or TABLES).items():
        size = row_group_size.get(name) if isinstance(row_group_size, dict) else row_group_size
        pq.write_table(table, directory / f"{name}.parquet", row_group_size=size)
        frame = table.to_pandas()
        sidecar[name] = {"fingerprint": fingerprint(pq.read_metadata(directory / f"{name}.parquet")),
                         "columns": {c: {"ndv": int(frame[c].nunique()), "method": "exact"} for c in frame},
                         "composite": [{"columns": k, "ndv": len(frame[k].drop_duplicates()), "method": "exact"}
                                       for k in COMPOSITE.get(name, [])]}
    (root / "stats").mkdir()
    (root / "stats" / "toy.sf1.json").write_text(json.dumps({"format": FORMAT, "tables": sidecar}))
    return Statistics(directory)


def scan(table, columns):
    projections = ", ".join(f"{c}@{i}" for i, c in enumerate(columns))
    schema = ", ".join(f"{c}:Int64" for c in columns)
    return (f"GpuLoadParquet: table={table}, projections=[{projections}], partition_groups=[[[0]]], "
            f"lanes=1, batches=multiple, schema=[{schema}]")


def rows_of(text: str) -> float:
    plan = parse_plans(f"== q\nGpuUnload\n{text}\n", "test")["q"]
    return estimate(plan, dataset())[id(plan)].rows


def join(join_type, key_build, key_probe, build, probe, out):
    return (f"  GpuHashJoin: join_type={join_type}, on=[({key_build}, {key_probe})], lanes=1, "
            f"batches=multiple, schema=[{out}]\n    {build}\n    {probe}")


def dim_filtered(predicate):
    return (f"GpuFilter: predicate={predicate}, projection=[dk@0], lanes=1, batches=multiple, schema=[dk:Int64]\n"
            f"      {scan('dates', ['dk', 'y', 'm'])}")


def test_a_dimension_wider_than_the_fact_does_not_dilute_the_match():
    # Year 5 is the fact's own 100 keys: every sale matches. The whole dimension's 1000 keys
    # as the domain (DuckDB's tdom) would say a tenth of them.
    text = join("Inner", "dk@0", "sk@0", dim_filtered("y@1 = 5"), scan("sales", ["sk"]), "dk:Int64, sk:Int64")
    assert abs(rows_of(text) - 1000) / 1000 < 0.02


def test_a_filter_unrelated_to_the_fact_spreads_over_the_whole_domain():
    # m = 0 keeps 143 keys spread over all 1000; the fact holds every tenth, so 15 match —
    # 150 rows. Containment in the fact's 100 keys would say all 1000.
    text = join("Inner", "dk@0", "pk@0", dim_filtered("m@2 = 0"), scan("spread", ["pk"]), "dk:Int64, pk:Int64")
    assert 100 < rows_of(text) < 200


def test_a_semi_join_keeps_the_values_the_other_side_holds_and_an_anti_join_the_rest():
    semi = join("LeftSemi", "ck@0", "ok@0", scan("cust", ["ck"]), scan("orders", ["ok"]), "ck:Int64")
    anti = join("LeftAnti", "ck@0", "ok@0", scan("cust", ["ck"]), scan("orders", ["ok"]), "ck:Int64")
    assert (rows_of(semi), rows_of(anti)) == (5, 5)


def test_a_left_join_keeps_its_unmatched_rows_and_they_are_null_on_the_other_side():
    left = join("Left", "ck@0", "ok@0", scan("cust", ["ck"]), scan("orders", ["ok"]), "ck:Int64, ok:Int64")
    assert rows_of(left) == 40 + 5
    unmatched = (f"  GpuFilter: predicate=ok@1 IS NULL, lanes=1, batches=multiple, schema=[ck:Int64, ok:Int64]\n"
                 + "\n".join("  " + line for line in left.splitlines()))
    assert abs(rows_of(unmatched) - 5) < 1e-9


def test_a_filtered_key_table_meets_its_references_over_the_whole_table_s_keys():
    # One day of the lines — 100 of the 1000 keys — against returns for every tenth line of all
    # days: the returns spread over the table's 1000 keys, 10 of them among the day's.
    text = f"""  GpuHashJoin: join_type=Inner, on=[(li@0, ri@0), (lo@1, ro@1)], lanes=1, batches=multiple, schema=[li:Int64, lo:Int64, ri:Int64, ro:Int64]
    GpuFilter: predicate=ld@2 = 3, projection=[li@0, lo@1], lanes=1, batches=multiple, schema=[li:Int64, lo:Int64]
      {scan('lines', ['li', 'lo', 'ld'])}
    {scan('returns', ['ri', 'ro'])}"""
    assert abs(rows_of(text) - 10) < 1


def test_a_filtered_column_holds_only_the_values_the_filter_left():
    # Grouping on the year after `y = 5` is one group, not the ten the table has.
    text = (f"  GpuAggregate: group_by=[y@0], lanes=1, batches=multiple, schema=[y:Int64]\n"
            f"    GpuFilter: predicate=y@0 = 5, lanes=1, batches=multiple, schema=[y:Int64]\n"
            f"      {scan('dates', ['y'])}")
    assert rows_of(text) == 1


#: a thousand keys: 100 on the preserved side, and 143 multiples of 7 on the other, 14 of them
#: among the 100 — a domain wider than either side's key count
WIDE_DOMAIN = {"pre": pa.table({"pk": list(range(1000)), "pv": [k % 10 for k in range(1000)]}),
               "nul": pa.table({"nk": [7 * i for i in range(143)]})}


def wide_domain_join(join_type):
    """`join_type` of the 100 keys `pv = 3` keeps against the 143 multiples of 7 — the 100 on the
    build side, or on the probe side for a Right type — its estimates, the join and the 100's node."""
    preserved = ("GpuCoalesceAllBatches: lanes=1, batches=single, schema=[pk:Int64]\n"
                 "      GpuFilter: predicate=pv@1 = 3, projection=[pk@0], lanes=1, batches=multiple, "
                 f"schema=[pk:Int64]\n        {scan('pre', ['pk', 'pv'])}")
    other = scan("nul", ["nk"])
    if join_type.startswith("Right"):
        out = "pk:Int64" if join_type == "RightAnti" else "nk:Int64, pk:Int64"
        text = join(join_type, "nk@0", "pk@0", other, preserved, out)
    else:
        out = "pk:Int64" if join_type == "LeftAnti" else "pk:Int64, nk:Int64"
        text = join(join_type, "pk@0", "nk@0", preserved, other, out)
    plan = parse_plans(f"== q\nGpuUnload\n{text}\n", "test")["q"]
    joined = plan.children[0]
    return estimate(plan, dataset(tables=WIDE_DOMAIN)), joined, joined.children[join_type.startswith("Right")]


def test_a_left_join_keeps_its_preserved_side_where_the_domain_is_wider_than_both_keys():
    # Taking the match share over the two key counts, not the domain, every preserved row
    # matches and the join keeps 14 of the 100.
    estimates, left, preserved = wide_domain_join("Left")
    assert estimates[id(left)].rows >= estimates[id(preserved)].rows
    assert abs(estimates[id(left)].rows - 100) < 1


def test_an_anti_join_keeps_the_unmatched_share_where_the_domain_is_wider_than_both_keys():
    estimates, anti, _ = wide_domain_join("LeftAnti")
    assert abs(estimates[id(anti)].rows - 86) < 2


def test_a_right_anti_join_keeps_the_probe_s_unmatched_share_where_the_domain_is_wider():
    # The probe side's share, which Right, RightSemi, RightAnti and Full read.
    estimates, anti, _ = wide_domain_join("RightAnti")
    assert abs(estimates[id(anti)].rows - 86) < 2


def test_rows_are_counted_at_a_scan_and_a_memory_source_and_estimated_past_a_filter_or_a_join():
    def known(text):
        return certainty(parse_plans(f"== q\n{text}\n", "test")["q"])

    source = "GpuMemorySource: name=p0, lanes=1, batches=single, schema=[sk:Int64]"
    assert known(f"GpuCoalesceAllBatches: lanes=1, batches=single, schema=[sk:Int64]\n  {scan('sales', ['sk'])}") \
        is Certainty.EXACT
    assert known(source) is Certainty.MEASURED
    assert known(f"GpuFilter: predicate=sk@0 = 5, lanes=1, batches=multiple, schema=[sk:Int64]\n  {scan('sales', ['sk'])}") \
        is Certainty.ESTIMATED
    union = "GpuUnion: lanes=1, batches=multiple, schema=[sk:Int64]\n  {}\n  {}"
    assert known(union.format(scan("sales", ["sk"]), source)) is Certainty.MEASURED
    assert known(union.format(scan("sales", ["sk"]), scan("spread", ["sk"]))) is Certainty.EXACT


if __name__ == "__main__":
    raise SystemExit(main(globals()))
