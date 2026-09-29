#!/usr/bin/env python3
"""DuckDB's answers to the corpus queries, as `goldens/<dataset>.sf1/duckdb-result.txt` (#235).

An oracle independent of DataFusion: the engine's `mini.result.txt` is written by our own cpu
engine and checked against DataFusion, so a defect the two share passes every tier. This file is
what the same queries answer on DuckDB, over the same sf1 parquet.

Rendered in the shape of `mini.result.txt` so the two read side by side: one `== <query>` section
per query in `testdata/<dataset>-queries/`, a `+---+` table with its data rows sorted as text, and
the same 262144-byte cap. A query DuckDB refuses gets `failed: <first line of the error>`. There
is no `mode=` line: DuckDB has no planning modes. Cells: NULL is empty, a decimal keeps its scale,
a float is Python's shortest round-trip form, a date is ISO.

The session takes DataFusion's two rules that would otherwise be divergences of convention rather
than of answers: NULLs last ascending and first descending, and integer `/` truncating.

Deterministic: threads=1, and rows are sorted after rendering.

Usage (DuckDB 1.5.4, the version CI pins for generate_testdata.sh):
    python3 testdata/duckdb_result.py [--dataset tpch|tpcds] [--only q1,q2]
"""

import argparse
import datetime
import decimal
import pathlib
import sys

import duckdb

ROOT = pathlib.Path(__file__).resolve().parent
CAP = 262144
PINNED = "1.5.4"


def cell(value):
    if value is None:
        return ""
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, float):
        return repr(value)
    if isinstance(value, decimal.Decimal):
        return format(value, "f")
    if isinstance(value, (datetime.date, datetime.datetime)):
        return value.isoformat()
    return str(value)


def render(names, rows):
    cells = [[cell(v) for v in row] for row in rows]
    widths = [len(n) for n in names]
    for row in cells:
        for i, c in enumerate(row):
            widths[i] = max(widths[i], len(c))
    border = "+" + "+".join("-" * (w + 2) for w in widths) + "+"
    line = lambda values: "| " + " | ".join(v.ljust(w) for v, w in zip(values, widths)) + " |"
    body = sorted(line(r) for r in cells)
    return "\n".join([border, line(names), border, *body, border]) + "\n"


def query_order(path):
    stem = path.stem
    if stem.startswith("q") and stem[1:].isdigit():
        return (0, int(stem[1:]), "")
    return (1, 0, stem)


def generate(dataset, only):
    data = ROOT / f"{dataset}.sf1"
    queries = sorted((ROOT / f"{dataset}-queries").glob("*.sql"), key=query_order)
    if only:
        queries = [q for q in queries if q.stem in only]
    con = duckdb.connect()
    con.execute("SET threads=1")
    con.execute("SET default_null_order='nulls_last_on_asc_first_on_desc'")
    con.execute("SET integer_division=true")
    for table in sorted(data.glob("*.parquet")):
        con.execute(f"CREATE VIEW {table.stem} AS SELECT * FROM read_parquet('{table}')")
    out = []
    for query in queries:
        out.append(f"== {query.stem}\n")
        try:
            cursor = con.execute(query.read_text())
            names = [d[0] for d in cursor.description]
            rows = cursor.fetchall()
        except duckdb.Error as error:
            out.append(f"failed: {str(error).splitlines()[0]}\n")
            print(f"{dataset}/{query.stem}: failed", file=sys.stderr)
            continue
        table = render(names, rows)
        if len(table.encode()) >= CAP:
            out.append(f"skipped: the result is at or above the {CAP}-byte cap\n")
        else:
            out.append(table)
        print(f"{dataset}/{query.stem}: {len(rows)} rows", file=sys.stderr)
    target = ROOT / "goldens" / f"{dataset}.sf1" / "duckdb-result.txt"
    target.write_text("".join(out))
    print(f"wrote {target}", file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--dataset", choices=["tpch", "tpcds"], action="append")
    parser.add_argument("--only", help="comma-separated query stems")
    args = parser.parse_args()
    if duckdb.__version__ != PINNED:
        sys.exit(f"duckdb {duckdb.__version__}; the oracle is pinned at {PINNED}")
    only = set(args.only.split(",")) if args.only else None
    for dataset in args.dataset or ["tpch", "tpcds"]:
        generate(dataset, only)


if __name__ == "__main__":
    main()
