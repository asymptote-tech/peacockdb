#!/usr/bin/env python3
"""The statistics sidecar: exact distinct counts for every column of every table of a dataset,
plus the same-table composite join keys below, and the mean length of each string column's
values, into `testdata/stats/<bench>.sf<N>.json`.

Only what the footer lacks: rows, min/max and null counts are in each parquet footer already,
and two sources of one number drift; the footer's byte sizes are of the encoded pages, which
for a dictionary-encoded string column are its indices. A NULL is not a value, as in a join key. Each table carries a
fingerprint of its file (rows, bytes, footer sha256): a reader whose footer differs must refuse
the sidecar rather than fall back to guessing, since a stale NDV is exactly the error it exists
to remove.

    testdata/gen_stats.py --bench tpch --sf 1 [--data-dir DIR] [--duckdb PATH]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import struct
import subprocess
import sys

TESTDATA = pathlib.Path(__file__).resolve().parent

#: Join keys of two or more columns that all come from one table, per the plan goldens. A set
#: that spans tables exists only in an intermediate result and cannot be counted here.
COMPOSITE = {
    "tpch": {
        "lineitem": [("l_partkey", "l_suppkey")],
        "partsupp": [("ps_partkey", "ps_suppkey")],
        "supplier": [("s_nationkey", "s_suppkey")],
    },
    "tpcds": {
        "catalog_returns": [("cr_item_sk", "cr_order_number")],
        "catalog_sales": [("cs_bill_customer_sk", "cs_item_sk"), ("cs_item_sk", "cs_order_number")],
        "customer_address": [("ca_address_sk", "ca_zip"), ("ca_county", "ca_state")],
        "customer_demographics": [("cd_demo_sk", "cd_education_status", "cd_marital_status")],
        "item": [("i_brand_id", "i_category_id", "i_class_id")],
        "store": [("s_county", "s_state")],
        "store_returns": [("sr_customer_sk", "sr_item_sk"),
                          ("sr_customer_sk", "sr_item_sk", "sr_ticket_number"),
                          ("sr_item_sk", "sr_ticket_number")],
        "store_sales": [("ss_customer_sk", "ss_item_sk", "ss_ticket_number"),
                        ("ss_item_sk", "ss_ticket_number")],
        "web_returns": [("wr_item_sk", "wr_order_number")],
        "web_sales": [("ws_item_sk", "ws_order_number")],
    },
}


def fingerprint(path: pathlib.Path, rows: int) -> dict:
    """Rows, bytes and the footer's sha256; the footer is the file's last `4 + length + 8`
    bytes, its length the little-endian u32 before the closing magic."""
    size = path.stat().st_size
    with path.open("rb") as file:
        file.seek(size - 8)
        (length,) = struct.unpack("<I", file.read(4))
        file.seek(size - 8 - length)
        footer = file.read(length)
    return {"rows": rows, "bytes": size, "footer_sha256": hashlib.sha256(footer).hexdigest()}


def duckdb_rows(duckdb: str, sql: str) -> list[dict]:
    done = subprocess.run([duckdb, "-json", "-c", "SET threads=1; " + sql], capture_output=True, text=True)
    if done.returncode:
        sys.exit(f"duckdb failed: {done.stderr.strip()}\n  {sql[:300]}")
    return json.loads(done.stdout or "[]")


def quoted(name: str) -> str:
    return '"' + name.replace('"', '""') + '"'


def table_stats(duckdb: str, path: pathlib.Path, composite: list[tuple[str, ...]]) -> dict:
    source = f"read_parquet('{path}')"
    described = duckdb_rows(duckdb, f"DESCRIBE SELECT * FROM {source}")
    columns = [row["column_name"] for row in described]
    # Bytes, not characters: `strlen` of a VARCHAR, `octet_length` of a BLOB.
    strings = {row["column_name"]: {"VARCHAR": "strlen", "BLOB": "octet_length"}[row["column_type"]]
               for row in described if row["column_type"] in ("VARCHAR", "BLOB")}
    counts = ["count(*) AS rows"] + [f"count(DISTINCT {quoted(c)}) AS c{i}" for i, c in enumerate(columns)]
    counts += [f"avg({strings[c]}({quoted(c)})) AS b{i}" for i, c in enumerate(columns) if c in strings]
    for i, key in enumerate(composite):
        missing = [c for c in key if c not in columns]
        if missing:
            sys.exit(f"{path.stem}: composite key {key} names no column {missing}")
        present = " AND ".join(f"{quoted(c)} IS NOT NULL" for c in key)
        counts.append(f"count(DISTINCT ({', '.join(map(quoted, key))})) FILTER (WHERE {present}) AS k{i}")
    [row] = duckdb_rows(duckdb, f"SELECT {', '.join(counts)} FROM {source}")
    stats = {"fingerprint": fingerprint(path, row["rows"]),
             "columns": {c: {"ndv": row[f"c{i}"], "method": "exact"} for i, c in enumerate(columns)}}
    for i, c in enumerate(columns):
        if c in strings:
            stats["columns"][c]["avg_bytes"] = round(row[f"b{i}"] or 0.0, 3)
    if composite:
        stats["composite"] = [{"columns": list(key), "ndv": row[f"k{i}"], "method": "exact"}
                              for i, key in enumerate(composite)]
    return stats


def dumps(value, indent: str = "") -> str:
    """JSON with one line per column: a container of scalars, or of lists of them, prints on
    one line."""
    inner = indent + "  "
    nested = lambda v: isinstance(v, dict) or isinstance(v, list) and any(isinstance(x, (dict, list)) for x in v)
    if isinstance(value, dict) and any(nested(v) for v in value.values()):
        items = [f"{inner}{json.dumps(k)}: {dumps(v, inner)}" for k, v in value.items()]
        return "{\n" + ",\n".join(items) + f"\n{indent}}}"
    if isinstance(value, list) and any(nested(v) for v in value):
        return "[\n" + ",\n".join(inner + dumps(v, inner) for v in value) + f"\n{indent}]"
    return json.dumps(value)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--bench", required=True, choices=sorted(COMPOSITE))
    parser.add_argument("--sf", required=True)
    parser.add_argument("--data-dir", type=pathlib.Path)
    parser.add_argument("--duckdb", default=os.environ.get("DUCKDB", "duckdb"))
    args = parser.parse_args()
    data = args.data_dir or TESTDATA / f"{args.bench}.sf{args.sf}"
    files = sorted(data.glob("*.parquet"))
    if not files:
        sys.exit(f"no parquet under {data}")
    unknown = set(COMPOSITE[args.bench]) - {f.stem for f in files}
    if unknown:
        sys.exit(f"composite keys name tables not in {data}: {sorted(unknown)}")
    tables = {f.stem: table_stats(args.duckdb, f, COMPOSITE[args.bench].get(f.stem, [])) for f in files}
    out = TESTDATA / "stats" / f"{args.bench}.sf{args.sf}.json"
    out.parent.mkdir(exist_ok=True)
    out.write_text(dumps({"format": 1, "tables": tables}) + "\n")
    print(f"{out}: {len(tables)} tables, {sum(len(t['columns']) for t in tables.values())} columns")


if __name__ == "__main__":
    main()
