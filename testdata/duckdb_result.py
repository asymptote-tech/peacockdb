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
    python3 testdata/duckdb_result.py [--dataset tpch|tpcds|pbench] [--only q1,q2]
"""

import argparse
import datetime
import decimal
import hashlib
import pathlib
import sys

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
    # datetime before date, which it subclasses. arrow-rs prints NaiveDateTime's Debug: a
    # 'T', and the fraction in groups of three digits with none at all when it is zero —
    # where isoformat() always prints six, which would be a divergence on every timestamp.
    if isinstance(value, datetime.datetime):
        base = value.strftime("%Y-%m-%dT%H:%M:%S")
        us = value.microsecond
        if us == 0:
            return base
        if us % 1000 == 0:
            return f"{base}.{us // 1000:03d}"
        return f"{base}.{us:06d}"
    if isinstance(value, datetime.date):
        return value.isoformat()
    return str(value)


def approx_number(x):
    """Rust's `{:.17e}`: the exponent plain, with no sign and no padding.

    A shortest-round-trip form would not do — Rust's and Python's notations differ — and the
    two writers' fingerprints are compared byte for byte.
    """
    if x != x:
        return "nan"
    if x == float("inf"):
        return "inf"
    if x == float("-inf"):
        return "-inf"
    mantissa, exponent = f"{x:.17e}".split("e")
    return f"{mantissa}e{int(exponent)}"


# The column types whose cells leave the hash, and those that stay in it. A FLOAT is the
# only approximate class: each engine prints a float to its own precision, while a DECIMAL
# renders at its declared scale on both sides and so does everything else here.
APPROXIMATE_TYPES = {"FLOAT", "REAL", "DOUBLE"}
EXACT_TYPES = {
    "BOOLEAN", "TINYINT", "SMALLINT", "INTEGER", "BIGINT", "HUGEINT", "UTINYINT",
    "USMALLINT", "UINTEGER", "UBIGINT", "UHUGEINT", "DECIMAL", "VARCHAR", "BLOB", "UUID",
    "DATE", "TIME", "TIMESTAMP", "TIMESTAMP_S", "TIMESTAMP_MS", "TIMESTAMP_NS", "INTERVAL",
}

# What separates two cells inside one hashed row; `fingerprint.rs`'s CELL_SEPARATOR writes
# the same byte. A separator that occurs in data makes ("a|b","c") and ("a","b|c") hash
# alike — two different answers agreeing, on the comparison that has no second opinion
# behind it — and `o_comment` is 1.5M hashed rows in anti-join and semi-join.
CELL_SEPARATOR = "\x01"


def is_approximate(declared):
    """Whether column type `declared` leaves the hash, read off the DECLARATION.

    `test_support/fingerprint.rs`'s `is_approximate` is the same rule over the arrow type.
    Classing off the cells instead agreed with it only while some row carried a float, so an
    all-NULL DOUBLE was exact here and approximate there and the section could never pass.

    Exhaustive, with no default: a type nobody listed would be classed silently, and the two
    writers would hash different columns with nothing going red.
    """
    name = str(declared).split("(")[0].upper()
    if name in APPROXIMATE_TYPES:
        return True
    if name in EXACT_TYPES:
        return False
    raise ValueError(
        f"duckdb_result.py: no fingerprint class for the column type {declared}. Add it to "
        "APPROXIMATE_TYPES or EXACT_TYPES, to match test_support/fingerprint.rs."
    )


def fingerprint(names, types, rows):
    """What a section holds instead of its rows when the rendering reached the cap.

    The same text `test_support/fingerprint.rs` writes, byte for byte. `types` are the
    declared column types, `cursor.description`'s second field, and they alone decide which
    column leaves the hash — a rendered `31.00` does not say whether it was a decimal or a
    double, and a column holding no value at all says nothing either.
    """
    cells = [[cell(v) for v in row] for row in rows]
    width = len(names)
    approximate = [is_approximate(t) for t in types]

    nonnull = [0] * width
    values = [[] for _ in range(width)]
    hashed = []
    for row in cells:
        exact = ""
        for at, text in enumerate(row):
            if text:
                nonnull[at] += 1
            if approximate[at]:
                if text:
                    values[at].append(float(text))
            else:
                exact += text + CELL_SEPARATOR
        hashed.append(exact)

    out = [f"fingerprint: rows={len(cells)}"]
    for at in range(width):
        line = f"col {at}: nonnull={nonnull[at]}"
        if approximate[at]:
            # Summed IN VALUE ORDER, so the two sides add in one sequence and float
            # reassociation cannot move the digits the comparison reads. An empty column sums
            # to zero with no min and no max; a NaN among the values makes the whole triple
            # NaN, since no sort places a NaN and min/max would follow the row order instead.
            ordered = sorted(values[at])
            nan = float("nan")
            if any(v != v for v in ordered):
                total, low, high = nan, nan, nan
            else:
                total = 0.0
                for v in ordered:
                    total += v
                low = ordered[0] if ordered else nan
                high = ordered[-1] if ordered else nan
            line += (
                f" sum={approx_number(total)} min={approx_number(low)}"
                f" max={approx_number(high)}"
            )
        out.append(line)
    hashed.sort()
    out.append("hash: " + hashlib.sha256("\n".join(hashed).encode()).hexdigest())
    return "\n".join(out) + "\n"


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


# DataFusion's `arrow_cast` to a second-unit timestamp is DuckDB's TIMESTAMP_S cast. Parquet
# has no second unit, so pbench's `timestamp-s-key-group` is the one query that reaches
# `Timestamp(Second)` at all, and it reaches it through a function DuckDB does not have.
# One entry, matched as the exact spelling that query writes: a general SQL translation layer
# here would be a second dialect to keep correct, and an oracle nobody can trust.
DUCKDB_SPELLING = {
    "arrow_cast(f_ts_s, 'Timestamp(Second, None)')": "f_ts_s::TIMESTAMP_S",
}


def duckdb_text(sql):
    for ours, theirs in DUCKDB_SPELLING.items():
        sql = sql.replace(ours, theirs)
    return sql


def query_files(dataset, only, root=ROOT):
    """The dataset's query files in golden order, or exit naming what is missing.

    `generate` rewrites the whole golden from the queries it ran, so an empty list is the one
    outcome that must not reach it: a `duckdb-result.txt` with no sections is read by every
    `duckdb_<ds>_<q>` case as "DuckDB does not answer this query", which passes a line
    declaring `duckdb_none` and fails every other with a message about the oracle rather than
    about the missing file. Three ways to get there, each refused by name here: a dataset with
    no query directory, a directory holding no `.sql`, and an `--only` that matches nothing.
    """
    directory = root / f"{dataset}-queries"
    if not directory.is_dir():
        sys.exit(f"duckdb_result.py: no query directory at {directory}")
    found = sorted(directory.glob("*.sql"), key=query_order)
    if not found:
        sys.exit(f"duckdb_result.py: {directory} holds no .sql files")
    if only is None:
        return found
    kept = [query for query in found if query.stem in only]
    if not kept:
        sys.exit(
            f"duckdb_result.py: --only {','.join(sorted(only))} names no query of {dataset}; "
            f"writing the golden from an empty list would delete every section it holds"
        )
    return kept


def generate(dataset, only):
    # DuckDB imported here rather than at the top, the way duckdb_cost.py defers
    # pyarrow.parquet: test_duckdb_result.py imports this module to check `cell` and
    # `fingerprint`, neither of which asks DuckDB anything, and the CI job that runs it
    # installs no duckdb wheel. A top-level import makes that test die before its first case.
    import duckdb

    data = ROOT / f"{dataset}.sf1"
    queries = query_files(dataset, only)
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
            cursor = con.execute(duckdb_text(query.read_text()))
            names = [d[0] for d in cursor.description]
            types = [d[1] for d in cursor.description]
            rows = cursor.fetchall()
        except duckdb.Error as error:
            out.append(f"failed: {str(error).splitlines()[0]}\n")
            print(f"{dataset}/{query.stem}: failed", file=sys.stderr)
            continue
        table = render(names, rows)
        if len(table.encode()) >= CAP:
            out.append(fingerprint(names, types, rows))
        else:
            out.append(table)
        print(f"{dataset}/{query.stem}: {len(rows)} rows", file=sys.stderr)
    target = ROOT / "goldens" / f"{dataset}.sf1" / "duckdb-result.txt"
    # A dataset's first golden arrives before its directory does, and this writer is the one
    # that may run before any Rust tier has written a thing.
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text("".join(out))
    print(f"wrote {target}", file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--dataset", choices=["tpch", "tpcds", "pbench"], action="append")
    parser.add_argument("--only", help="comma-separated query stems")
    args = parser.parse_args()
    import duckdb  # deferred for the reason `generate` gives

    if duckdb.__version__ != PINNED:
        sys.exit(f"duckdb {duckdb.__version__}; the oracle is pinned at {PINNED}")
    only = set(args.only.split(",")) if args.only else None
    for dataset in args.dataset or ["tpch", "tpcds", "pbench"]:
        generate(dataset, only)


if __name__ == "__main__":
    main()
