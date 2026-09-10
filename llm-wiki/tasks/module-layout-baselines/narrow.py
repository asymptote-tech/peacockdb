#!/usr/bin/env python3
"""Narrow every `pub` the crate's outside does not need.

`pub` on an item nothing outside the crate names is a claim the compiler cannot check: the
item is reachable, so `dead_code` never fires on it and no lint says the visibility is wider
than the use. This reads the two outside readers — the CLI and the integration targets — and
narrows everything else to `pub(crate)`.

Deliberately conservative on method and field names: an external file mentioning the bare
identifier anywhere is treated as a use, because `.bytes()` at a call site carries no path to
match on. Over-keeping is a `pub` that should have narrowed; under-keeping is a broken build,
and the three shapes are what would report it.

  narrow.py --dry     what would change
  narrow.py           change it
"""
import pathlib
import re
import subprocess
import sys

SRC = pathlib.Path("peacockdb-core/src")
OUTSIDE = ["peacockdb-core/tests", "peacockdb/src"]
TOP = re.compile(r"^(pub)(\s+)(?:(?:default|const|async|unsafe)\s+)*"
                 r"(fn|struct|enum|trait|union|type|const|static|mod)\s+([A-Za-z_][A-Za-z0-9_]*)")
INNER = re.compile(r"^(    pub)(\s+)(?:(?:default|const|async|unsafe)\s+)*"
                   r"(fn|const|type)\s+([A-Za-z_][A-Za-z0-9_]*)")


def external_identifiers():
    words = set()
    for root in OUTSIDE:
        for f in pathlib.Path(root).rglob("*.rs"):
            words |= set(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", f.read_text()))
    return words


def main():
    dry = "--dry" in sys.argv
    keep = external_identifiers()
    changed = 0
    for f in sorted(SRC.rglob("*.rs")):
        lines = f.read_text().split("\n")
        out, hit = [], False
        for line in lines:
            m = TOP.match(line) or INNER.match(line)
            if m and m.group(4) not in keep and m.group(3) != "mod":
                out.append(line.replace(m.group(1), m.group(1) + "(crate)", 1))
                hit = True
                if dry:
                    print(f"{f}: {m.group(3)} {m.group(4)}")
                continue
            out.append(line)
        if hit and not dry:
            f.write_text("\n".join(out))
            changed += 1
    print(("would change" if dry else "changed"), changed, "files")


main()
