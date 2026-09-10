#!/usr/bin/env python3
"""Every sentence of documentation, and every attribute, in peacockdb-core/src.

Why this exists. Hoisting an item into a facade can leave the `///` block and the `#[…]`
above it behind, and nothing goes red: the build is clean, the tests pass, the goldens do not
move, and a body-line conservation check reports full conservation because it filters comments
out. 35 lines went that way in the plan slice and only this comparison found them.

Line-level comparison is too literal to be the check. Prose that moves from a module header
to an item doc changes its marker, and prose that moves into a narrower indent gets rewrapped,
so both read as deleted. So the unit here is the sentence, with markers stripped and
whitespace collapsed: a sentence survives a move, a re-marking and a rewrap, and what a
comparison then shows is prose that is genuinely gone or genuinely reworded — the set a reader
has to judge one by one.

  doc-attr-check.py            # the working tree
  doc-attr-check.py <rev>      # that revision
"""
import pathlib
import re
import subprocess
import sys

MARKER = re.compile(r"^\s*(///|//!|//)\s?")
ATTR = re.compile(r"^\s*#\[")


def files(rev):
    if rev is None:
        # The working tree, not the index: half of what this task moves is untracked and half
        # of what the index still lists is gone from disk.
        for f in sorted(pathlib.Path("peacockdb-core/src").rglob("*.rs")):
            yield f.read_text()
        return
    out = subprocess.run(["git", "ls-tree", "-r", "--name-only", rev, "--", "peacockdb-core/src"],
                         capture_output=True, text=True).stdout
    for f in out.split():
        if f.endswith(".rs"):
            yield subprocess.run(["git", "show", f"{rev}:{f}"], capture_output=True, text=True).stdout


def units(rev):
    out, para = [], []
    for text in files(rev):
        for line in text.split("\n") + [""]:
            if ATTR.match(line):
                out.append(line.strip())
                continue
            m = MARKER.match(line)
            if m:
                para.append(line[m.end():].rstrip())
                continue
            if para:
                joined = re.sub(r"\s+", " ", " ".join(para)).strip()
                out += [s.strip() for s in re.split(r"(?<=[.:])\s+(?=[A-Z`\[])", joined) if s.strip()]
                para = []
    return sorted(out)


print("\n".join(units(sys.argv[1] if len(sys.argv) > 1 else None)))
