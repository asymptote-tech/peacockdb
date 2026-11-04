#!/usr/bin/env python3
"""Classify Comet 1.2.0's native plans against peacockdb's node and expression vocabulary.

    plan_census.py PLANS_DIR

PLANS_DIR holds `tpch/` and `tpcds/` as `comet_run.py --dump-plans` wrote them: one protobuf-text
`<q>-NN-<Op>.txt` per native block, `<q>.plan.txt` (Spark's formatted plan) and `fallbacks.tsv`.
Writes three TSVs into PLANS_DIR: `census.tsv` (every operator, expression, aggregate and scalar
function, counted per query and classed), `eligible.tsv` (per query, the operators the GPU could
run under split.md's rule) and `columns.tsv` (the Parquet columns each query scans).

The class tables below are the translatability judgement; translatability.md explains each row.
"""
import re
import sys
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path

# Operator.op_struct, operator.proto:49-83 @ab46419f.
OP_KINDS = {
    "scan", "projection", "filter", "sort", "hash_agg", "limit", "shuffle_writer", "expand",
    "sort_merge_join", "hash_join", "window", "native_scan", "iceberg_scan", "parquet_writer",
    "explode", "csv_scan", "shuffle_scan", "broadcast_nested_loop_join", "sample",
    "window_group_limit", "iceberg_write", "range_scan", "merge_rows", "arrow_python_udf",
    "contrib_scan",
}
# proto3 text omits a field at its default, so these name the enum's zero value.
DEFAULT_ATTRS = {
    "hash_join": {"join_type": "Inner", "build_side": "BuildLeft"},
    "broadcast_nested_loop_join": {"join_type": "Inner", "build_side": "BuildLeft"},
    "sort_merge_join": {"join_type": "Inner"},
    "hash_agg": {"mode": "Partial"},
}
ATTR_KEYS = {"join_type", "build_side", "mode", "null_aware_anti_join"}

OP_CLASS = {
    "projection": "supported", "filter": "supported", "sort": "supported", "limit": "supported",
    "broadcast_nested_loop_join": "supported",
    "native_scan": "adaptable", "scan": "adaptable", "shuffle_scan": "adaptable",
    "hash_agg": "adaptable", "expand": "adaptable", "sort_merge_join": "adaptable",
}
JOIN_SUPPORTED = {"Inner", "LeftOuter", "RightOuter", "FullOuter", "LeftSemi", "LeftAnti"}

EXPR_CLASS = {
    **dict.fromkeys([
        "literal", "bound", "unbound", "eq", "neq", "gt", "gt_eq", "lt", "lt_eq", "and", "or",
        "not", "is_null", "is_not_null", "eqNullSafe", "neqNullSafe", "like", "caseWhen", "if",
        "in", "sort_order", "unary_minus", "bitwiseAnd", "bitwiseOr", "bitwiseXor",
        "bitwiseShiftLeft", "bitwiseShiftRight"], "supported"),
    **dict.fromkeys([
        "add", "subtract", "multiply", "divide", "remainder", "integral_divide", "cast",
        "check_overflow", "subquery", "normalize_nan_and_zero", "hour", "minute", "second",
        "at_least_n_non_nulls"], "adaptable"),
    **dict.fromkeys([
        "rlike", "bloom_filter_might_contain", "create_named_struct", "get_struct_field",
        "to_json", "list_extract", "get_array_struct_fields", "array_insert", "to_pretty_string",
        "rand", "randn", "spark_partition_id", "monotonically_increasing_id", "unix_timestamp",
        "from_json", "to_csv", "hours_transform", "arrays_zip", "jvm_scalar_udf",
        "precise_timestamp_conversion", "shuffle", "rand_str", "uuid", "native_scalar_udf",
        "truncTimestamp"], "missing"),
}
FUNC_CLASS = {
    **dict.fromkeys(["substring", "abs", "round", "lower", "upper", "concat", "coalesce"],
                    "supported"),
    **dict.fromkeys(["datepart", "date_part", "year", "month", "day", "starts_with",
                     "ends_with", "contains", "unscaled_value", "make_decimal"], "adaptable"),
}
AGG_CLASS = {
    **dict.fromkeys(["sum", "count", "min", "max", "avg", "stddev", "variance"], "supported"),
    **dict.fromkeys([
        "first", "last", "bitAndAgg", "bitOrAgg", "bitXorAgg", "covariance", "correlation",
        "bloomFilterAgg", "collectSet", "collectList", "percentile", "approxPercentile", "hllpp",
        "regr", "mode", "maxBy", "minBy", "listAgg", "hllSketchAgg", "hllUnionAgg",
        "pivotFirst"], "missing"),
}


@dataclass
class Block:
    name: str
    scalars: list = field(default_factory=list)
    children: list = field(default_factory=list)


@dataclass
class Op:
    kind: str
    attrs: dict
    exprs: list
    aggs: list
    children: list


def parse(text: str) -> Block:
    """Protobuf text format into nested blocks; the root is an unnamed block."""
    root = Block("")
    stack = [root]
    for raw in text.splitlines():
        line = raw.strip()
        if not line:
            continue
        if line.endswith("{"):
            b = Block(line[:-1].strip())
            stack[-1].children.append(b)
            stack.append(b)
        elif line == "}":
            stack.pop()
        else:
            k, _, v = line.partition(":")
            stack[-1].scalars.append((k.strip(), v.strip()))
    return root


def _exprs(b: Block, in_aggs: bool, exprs: list, aggs: list):
    for c in b.children:
        if c.name == "scalarFunc":
            name = dict(c.scalars).get("func", '""').strip('"')
            exprs.append(f"func:{name}")
        elif c.name in EXPR_CLASS:
            exprs.append(c.name)
        elif in_aggs and c.name in AGG_CLASS:
            aggs.append(c.name)
        _exprs(c, c.name in ("agg_exprs", "agg_func"), exprs, aggs)


def operator_tree(b: Block) -> Op:
    """The Operator message `b` (a parsed block's root, or a `children` block) as an Op tree."""
    body = next(c for c in b.children if c.name in OP_KINDS)
    attrs = dict(DEFAULT_ATTRS.get(body.name, {}))
    attrs.update((k, v) for k, v in body.scalars if k in ATTR_KEYS)
    exprs, aggs = [], []
    _exprs(body, False, exprs, aggs)
    kids = [operator_tree(c) for c in b.children if c.name == "children"]
    return Op(body.name, attrs, sorted(exprs), sorted(aggs), kids)


def op_class(kind: str, attrs: dict) -> str:
    if kind == "hash_join":
        if attrs.get("null_aware_anti_join") == "true" or attrs.get("join_type") == "Existence":
            return "adaptable"
        return "supported" if attrs.get("join_type") in JOIN_SUPPORTED else "missing"
    return OP_CLASS.get(kind, "missing")


def expr_class(name: str) -> str:
    if name.startswith("func:"):
        return FUNC_CLASS.get(name[5:], "missing")
    return EXPR_CLASS.get(name, "missing")


def agg_class(name: str) -> str:
    return AGG_CLASS.get(name, "missing")


def _gpu_ok(op: Op) -> bool:
    return (op_class(op.kind, op.attrs) != "missing"
            and all(expr_class(e) != "missing" for e in op.exprs)
            and all(agg_class(a) != "missing" for a in op.aggs))


def eligible_ops(op: Op) -> Counter:
    """Operators in maximal GPU subtrees: an operator runs on the GPU only if it, its
    expressions and every operator below it in the block can (split.md's rule)."""
    out = Counter()

    def walk(o: Op) -> bool:
        kids_ok = [walk(c) for c in o.children]
        ok = all(kids_ok) and _gpu_ok(o)
        if ok:
            out[o.kind] += 1
        return ok

    walk(op)
    return out


def count_ops(op: Op) -> int:
    return 1 + sum(count_ops(c) for c in op.children)


def all_ops(op: Op):
    yield op
    for c in op.children:
        yield from all_ops(c)


SCAN_RE = re.compile(r"^\(\d+\) (?:CometNativeScan|Scan) parquet", re.M)


def scan_columns(plan_text: str) -> set:
    """(table, column) pairs read by the scans of Spark's formatted plan."""
    out = set()
    for m in SCAN_RE.finditer(plan_text):
        section = plan_text[m.end():].split("\n\n", 1)[0]
        cols = re.search(r"^Output \[\d+\]: \[(.*)\]$", section, re.M)
        loc = re.search(r"^Location: \S+ \[(.*?)\]$", section, re.M)
        table = Path(loc.group(1).split(",")[0].strip()).stem
        for c in cols.group(1).split(", "):
            out.add((table, re.sub(r"#\d+L?$", "", c.strip())))
    return out


def query_key(name: str) -> int:
    return int(name[1:])


def census(plans: Path):
    census_rows, eligible_rows, column_rows = [], [], []
    for ds in ("tpch", "tpcds"):
        d = plans / ds
        spark_ops = {}
        for line in (d / "fallbacks.tsv").read_text().splitlines()[1:]:
            q, _, s_ops, _ = line.split("\t")
            spark_ops[q] = int(s_ops)
        queries = sorted({p.name.split(".")[0] for p in d.glob("q*.plan.txt")}, key=query_key)
        for q in queries:
            counts = Counter()
            total, elig, missing = 0, Counter(), Counter()
            blocks = sorted(d.glob(f"{q}-[0-9][0-9]-*.txt"))
            for f in blocks:
                tree = operator_tree(parse(f.read_text()))
                total += count_ops(tree)
                elig += eligible_ops(tree)
                for o in all_ops(tree):
                    counts[("op", o.kind, op_class(o.kind, o.attrs))] += 1
                    if o.kind in ("hash_join", "sort_merge_join", "broadcast_nested_loop_join"):
                        counts[("join", o.attrs["join_type"], op_class(o.kind, o.attrs))] += 1
                    if op_class(o.kind, o.attrs) == "missing":
                        missing[o.kind] += 1
                    for e in o.exprs:
                        counts[("expr", e, expr_class(e))] += 1
                        if expr_class(e) == "missing":
                            missing[e] += 1
                    for a in o.aggs:
                        counts[("agg", a, agg_class(a))] += 1
                        if agg_class(a) == "missing":
                            missing[a] += 1
            for (kind, name, cls), n in sorted(counts.items()):
                census_rows.append((ds, q, kind, name, cls, n))
            s_ops = spark_ops.get(q, 0)
            n_elig = sum(elig.values())
            frac = n_elig / (total + s_ops) if total + s_ops else 0.0
            eligible_rows.append((ds, q, len(blocks), total, s_ops, n_elig, f"{frac:.3f}",
                                  " ".join(f"{k}:{v}" for k, v in sorted(missing.items()))))
            for t, c in sorted(scan_columns((d / f"{q}.plan.txt").read_text())):
                column_rows.append((ds, q, t, c))
    return census_rows, eligible_rows, column_rows


def write(path: Path, header, rows):
    with open(path, "w") as f:
        f.write("\t".join(header) + "\n")
        for r in rows:
            f.write("\t".join(str(x) for x in r) + "\n")


def main(argv):
    plans = Path(argv[1])
    c, e, cols = census(plans)
    write(plans / "census.tsv", ["dataset", "query", "kind", "name", "class", "count"], c)
    write(plans / "eligible.tsv", ["dataset", "query", "blocks", "native_ops", "spark_ops",
                                   "gpu_ops", "eligible_fraction", "missing"], e)
    write(plans / "columns.tsv", ["dataset", "query", "table", "column"], cols)
    print(f"census {len(c)} rows, eligible {len(e)} queries, columns {len(cols)} rows")


if __name__ == "__main__":
    main(sys.argv)
