#!/usr/bin/env python3
"""Every name the crate's own surface has to keep `pub` for.

Two readers are outside `peacockdb-core`: the `peacockdb` CLI, and the eighteen integration
targets, which are separate crates and see the library the way crates.io would. Anything they
name has to stay `pub`; everything else can be `pub(crate)` at most. Printed as a sorted set so
the narrowing is a comparison rather than a reading.
"""
import pathlib
import re
import sys

ROOTS = ["peacockdb-core/tests", "peacockdb/src"]
# `peacockdb_core::a::b::{C, d}` / `peacockdb_core::a::C` / `peacockdb_core::{C, d}`
PATH = re.compile(r"peacockdb_core::((?:[a-z_][a-z0-9_]*::)*)(\{[^}]*\}|[A-Za-z_][A-Za-z0-9_]*)",
                  re.S)

names = set()
for root in ROOTS:
    for f in sorted(pathlib.Path(root).rglob("*.rs")):
        text = f.read_text()
        for m in PATH.finditer(text):
            tail = m.group(2)
            if tail.startswith("{"):
                for part in tail[1:-1].split(","):
                    part = part.strip().split(" as ")[0].strip()
                    if part and part != "self":
                        names.add(part)
            else:
                names.add(tail)
        # a module path is itself a name the crate exposes
        for seg in m.group(1).strip(":").split("::") if (m := PATH.search(text)) else []:
            pass
for root in ROOTS:
    for f in sorted(pathlib.Path(root).rglob("*.rs")):
        for m in PATH.finditer(f.read_text()):
            for seg in [s for s in m.group(1).split("::") if s]:
                names.add(seg)
print("\n".join(sorted(names)))
