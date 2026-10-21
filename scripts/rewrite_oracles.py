#!/usr/bin/env python3
"""Move `corpus_cases.inc` and the registry onto oracle-cleanup's oracle grammar.

    scripts/rewrite_oracles.py --inc peacockdb-core/tests/common/corpus_cases.inc \
        --registry testdata/cost-registry.csv [--no-sort dataset/query ...]

Old spellings map to the grammar (`duckdb_exact` -> `duckdb(sort, exact)`, ...); #251's
`duckdb_divergent(251, <positions>)` becomes `duckdb(no_sort, rel(1e-5, "<column>", ...))`, the
names read off the query's `mini.result.txt` header. `--no-sort` sets `no_sort` on a query's
`duckdb(...)` and `golden(...)`. The registry's three oracle cells are then copied from the
lines. Idempotent. Deleted at the end of the task that needed it.
"""

import argparse
import csv
import io
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ticket_widget import call, split_top  # noqa: E402

PLAIN = {
    "duckdb_exact": "duckdb(sort, exact)",
    "duckdb_approx": "duckdb(sort, approx)",
    "data_fusion_exact": "data_fusion(sort, exact)",
    "data_fusion_approximate": "data_fusion(sort, approx)",
    "golden_exact": "golden(sort, exact)",
    "golden_approx_std": "golden(sort, approx_std)",
}
REL_251 = "1e-5"
LINE = re.compile(r"^(?P<lead>(?:// )?)corpus_query!\(")


def closing(text: str, start: int) -> int:
    """The index of the `)` closing the `(` before `start`, outside quotes."""
    depth, quoted = 1, False
    for at in range(start, len(text)):
        c = text[at]
        if c == '"':
            quoted = not quoted
        elif not quoted and c == "(":
            depth += 1
        elif not quoted and c == ")":
            depth -= 1
            if depth == 0:
                return at
    raise SystemExit(f"unclosed corpus_query!: {text}")


def header_names(goldens: Path, dataset: str, sf: str, query: str) -> list:
    text = (goldens / f"{dataset}.sf{sf}" / "mini.result.txt").read_text()
    body = text.split(f"\n== {query}\n", 1)[1].splitlines()
    rows = [l for l in body if l.startswith("|")]
    return [c.strip() for c in rows[0].strip("|").split("|")]


def with_order(spelled: str, order: str) -> str:
    name, args = call(spelled)
    return f"{name}({', '.join([order] + args[1:])})"


def rewrite_duckdb(spelled: str, key: tuple, goldens: Path) -> str:
    name, args = call(spelled)
    if name == "duckdb_divergent":
        if args[0] != "251":
            raise SystemExit(f"{key}: only #251's divergences have a rel mapping: {spelled}")
        names = header_names(goldens, key[0], key[1], key[2].replace("_", "-"))
        columns = ", ".join(f'"{names[int(p)]}"' for p in args[1:])
        return f"duckdb(no_sort, rel({REL_251}, {columns}))"
    return PLAIN.get(spelled, spelled)


def rewrite(inc: Path, registry: Path, no_sort: set) -> int:
    goldens = registry.parent / "goldens"
    out, declared, changed = [], {}, 0
    for line in inc.read_text().splitlines(keepends=True):
        m = LINE.match(line)
        if not m:
            out.append(line)
            continue
        end = closing(line, m.end())
        args = split_top(line[m.end():end])
        key = (args[0], args[1], args[2])
        args[5] = rewrite_duckdb(args[5], key, goldens)
        args[6] = PLAIN.get(args[6], args[6])
        args[7] = PLAIN.get(args[7], args[7])
        if f"{args[0]}/{args[2].replace('_', '-')}" in no_sort:
            for at, head in ((5, "duckdb"), (7, "golden")):
                if call(args[at])[0] == head:
                    args[at] = with_order(args[at], "no_sort")
        new = f"{m.group('lead')}corpus_query!({', '.join(args)}{line[end:]}"
        changed += new != line
        out.append(new)
        if not m.group("lead"):
            declared[(args[0], args[1], args[2])] = (args[5], args[6], args[7])
    inc.write_text("".join(out))

    rows = list(csv.reader(registry.read_text().splitlines()))
    at = {c: rows[0].index(c) for c in ("dataset", "sf", "query", "duckdb_oracle",
                                         "cpu_oracle", "gpu_oracle")}
    for row in rows[1:]:
        spelled = declared.get((row[at["dataset"]], row[at["sf"]], row[at["query"]]))
        if spelled:
            row[at["duckdb_oracle"]], row[at["cpu_oracle"]], row[at["gpu_oracle"]] = spelled
    buf = io.StringIO()
    csv.writer(buf, lineterminator="\n", quoting=csv.QUOTE_MINIMAL).writerows(rows)
    registry.write_text(buf.getvalue())
    return changed


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--inc", type=Path, required=True)
    ap.add_argument("--registry", type=Path, required=True)
    ap.add_argument("--no-sort", nargs="*", default=[], metavar="DATASET/QUERY")
    args = ap.parse_args()
    print(f"{rewrite(args.inc, args.registry, set(args.no_sort))} lines changed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
