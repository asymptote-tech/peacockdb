#!/usr/bin/env python3
"""Enumerate every `pub` / `pub(...)` item in peacockdb-core/src with its declaring file.

The module-layout baseline (spec item 3) and the per-slice comparison run against it: the
task moves declarations and removes none, so the (kind, name) multiset must not change.
Records are `<scope> <vis> <kind> <name>\t<file>`, where scope is `top` for an item
declared directly in a module and `impl` for one inside an `impl` block.

Usage:
  visibility-dump.py [root]            all records, sorted
  visibility-dump.py --items [root]    just `<scope> <kind> <name>`, the move-invariant set
"""
import re
import sys
from pathlib import Path

KIND = r"(?:pub\s*(?:\([^)]*\))?)\s+(?:default\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+\"[^\"]*\"\s+)?(fn|struct|enum|trait|union|type|const|static|mod|use)\b"
DECL = re.compile(r"^\s*(pub\s*(?:\([^)]*\))?)\s+(?:default\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+\"[^\"]*\"\s+)?(fn|struct|enum|trait|union|type|const|static|mod|use)\s+([A-Za-z_][A-Za-z0-9_]*)?")
IMPL = re.compile(r"^\s*(?:unsafe\s+)?impl\b")


def strip(src: str) -> list[str]:
    """Blank out string/char literals and comments, keeping line structure intact.

    Brace depth decides top-level vs inside-an-impl, so a `{` inside a format string or a
    doc comment would shift every item below it into the wrong scope.
    """
    out, i, n = [], 0, len(src)
    line, block, instr, raw_hashes, inchar = [], 0, False, None, False
    while i < n:
        c = src[i]
        nxt = src[i + 1] if i + 1 < n else ""
        if c == "\n":
            out.append("".join(line))
            line = []
            i += 1
            if block == 0 and not instr and raw_hashes is None:
                pass
            continue
        if block:
            if c == "/" and nxt == "*":
                block += 1
                i += 2
                line.append("  ")
                continue
            if c == "*" and nxt == "/":
                block -= 1
                i += 2
                line.append("  ")
                continue
            line.append(" ")
            i += 1
            continue
        if raw_hashes is not None:
            if c == '"' and src[i + 1 : i + 1 + raw_hashes] == "#" * raw_hashes:
                i += 1 + raw_hashes
                line.append(" " * (1 + raw_hashes))
                raw_hashes = None
                continue
            line.append(" ")
            i += 1
            continue
        if instr:
            if c == "\\":
                i += 2
                line.append("  ")
                continue
            if c == '"':
                instr = False
            line.append(" ")
            i += 1
            continue
        if inchar:
            if c == "\\":
                i += 2
                line.append("  ")
                continue
            if c == "'":
                inchar = False
            line.append(" ")
            i += 1
            continue
        if c == "/" and nxt == "/":
            j = src.find("\n", i)
            j = n if j < 0 else j
            line.append(" " * (j - i))
            i = j
            continue
        if c == "/" and nxt == "*":
            block = 1
            i += 2
            line.append("  ")
            continue
        m = re.match(r'r(#*)"', src[i:])
        if m:
            raw_hashes = len(m.group(1))
            line.append(" " * m.end())
            i += m.end()
            continue
        if c == '"':
            instr = True
            line.append(" ")
            i += 1
            continue
        # A lifetime (`'a`) is not a char literal; a char literal's next-but-one is a quote
        # or the escape makes it longer.
        if c == "'" and re.match(r"'(?:\\.|[^\\'])'", src[i:]):
            inchar = True
            line.append(" ")
            i += 1
            continue
        line.append(c)
        i += 1
    out.append("".join(line))
    return out


def records(path: Path, rel: str):
    lines = strip(path.read_text())
    depth = 0
    pending_impl = False
    # Brace depth of each open `impl` body, so an item is `impl`-scoped only while one is.
    impl_depths: list[int] = []
    for raw in lines:
        m = DECL.match(raw)
        if m:
            vis = re.sub(r"\s+", "", m.group(1))
            kind, name = m.group(2), m.group(3) or "?"
            scope = "impl" if impl_depths else "top"
            if kind == "use":
                name = raw.strip().split("use", 1)[1].strip().rstrip(";")
            yield f"{scope} {vis} {kind} {name}\t{rel}"
        if IMPL.match(raw):
            pending_impl = True
        for c in raw:
            if c == "{":
                depth += 1
                if pending_impl:
                    impl_depths.append(depth)
                    pending_impl = False
            elif c == "}":
                if impl_depths and impl_depths[-1] == depth:
                    impl_depths.pop()
                depth -= 1
            elif c == ";" and pending_impl:
                pending_impl = False


def main():
    args = [a for a in sys.argv[1:]]
    items_only = "--items" in args
    args = [a for a in args if a != "--items"]
    root = Path(args[0]) if args else Path("peacockdb-core/src")
    out = []
    for f in sorted(root.rglob("*.rs")):
        out.extend(records(f, str(f)))
    if items_only:
        out = [r.split("\t")[0] for r in out]
        out = [" ".join(p for i, p in enumerate(r.split(" ")) if i != 1) for r in out]
    print("\n".join(sorted(out)))


main()
