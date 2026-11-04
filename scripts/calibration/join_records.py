#!/usr/bin/env python3
"""A benchmark run's segments, one process per case, joined into one record.

    python3 scripts/calibration/join_records.py --record <segment>… --out <file>

`scripts/env/bench-each.sh` sets each process's record aside as `<ds>-<q>-<mode>.<attempt>.tsv`.
A case killed between its rows and its section is measured again, so its rows can be in two
segments: a case's rows come from the highest attempt holding it, a name without an attempt (a
joined record, joined again) counting as attempt 0. Cases keep the order the arguments first name
them in. Every segment is a plain run's (`capture=none`), and their headings agree but for the
pool's free memory, which each process reads at its start: the joined line gives it as
`<min>-<max>`. Anything else that differs is refused, naming the line, with nothing written.
"""

import argparse
import pathlib
import re
import sys

ALLOCATOR = "# run: allocator="
ATTEMPT = re.compile(r"\.([0-9]+)\.tsv$")
# The free memory `RmmPool`'s line reports: `of 79.1GiB free`, or a joined `79.0GiB-79.3GiB`.
FREE = re.compile(r"^([0-9.]+)([A-Za-z]+)(?:-([0-9.]+)\2)?$")


def attempt(path):
    found = ATTEMPT.search(pathlib.Path(path).name)
    return int(found.group(1)) if found else 0


def read(path):
    """`(heading, columns, rows by case)`, each line with its newline; cases in file order."""
    heading, columns, cases = [], None, {}
    with open(path, newline="") as fh:
        for line in fh:
            if columns is None and line.startswith("#"):
                heading.append(line)
            elif columns is None:
                columns = line
            else:
                fields = line.rstrip("\n").split("\t")
                names = columns.rstrip("\n").split("\t")
                case = tuple(fields[names.index(c)] for c in ("dataset", "sf", "query", "mode"))
                cases.setdefault(case, []).append(line)
    if columns is None:
        sys.exit(f"{path} has no column line")
    if "# run: capture=none\n" not in heading:
        sys.exit(f"{path} has no `# run: capture=none` line: only plain runs' records are joined")
    return heading, columns, cases


def free_of(line):
    """The allocator line's words, the free-memory word replaced by None, and its `(lo, hi, unit)`."""
    words = line.rstrip("\n").split(" ")
    if "free" not in words or words.index("free") == 0:
        return words, None
    at = words.index("free") - 1
    found = FREE.match(words[at])
    if not found:
        return words, None
    lo, unit, hi = found.group(1), found.group(2), found.group(3) or found.group(1)
    return words[:at] + [None] + words[at + 1:], (float(lo), float(hi), unit, lo, hi)


def joined_allocator(lines):
    """One allocator line for segments whose lines differ in the free memory alone, or None."""
    shapes = [free_of(line) for line in lines]
    words = {tuple(w) for w, _ in shapes}
    if len(words) != 1:
        return None
    spans = [span for _, span in shapes]
    if spans[0] is None:
        return lines[0] if len(set(lines)) == 1 else None
    if len({span[2] for span in spans}) != 1:
        return None
    lo = min(spans, key=lambda span: span[0])
    hi = max(spans, key=lambda span: span[1])
    token = f"{lo[3]}{lo[2]}" if lo[0] == hi[1] else f"{lo[3]}{lo[2]}-{hi[4]}{lo[2]}"
    return " ".join(token if w is None else w for w in shapes[0][0]) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--record", action="append", required=True, help="a segment; repeat")
    parser.add_argument("--out", required=True, help="the joined record")
    args = parser.parse_args(argv)

    segments = [(path, *read(path)) for path in args.record]
    first_path, first_heading, columns, _ = segments[0]
    others = [line for line in first_heading if not line.startswith(ALLOCATOR)]
    allocators = []
    for path, heading, cols, _ in segments:
        if cols != columns:
            sys.exit(f"{path}'s column line {cols.rstrip()!r} is not {first_path}'s {columns.rstrip()!r}")
        rest = [line for line in heading if not line.startswith(ALLOCATOR)]
        if rest != others:
            at = next((i for i, (a, b) in enumerate(zip(rest, others)) if a != b), min(len(rest), len(others)))
            differ = rest[at] if at < len(rest) else others[at]
            sys.exit(f"{path} and {first_path} differ at {differ.rstrip()!r}: two runs, not segments of one")
        allocators.append((path, [line for line in heading if line.startswith(ALLOCATOR)]))
    if any(len(lines) != 1 for _, lines in allocators):
        sys.exit(f"{first_path}: a segment's heading holds no `{ALLOCATOR}` line, or two")
    allocator = joined_allocator([lines[0] for _, lines in allocators])
    if allocator is None:
        names = ", ".join(f"{path}: {lines[0].rstrip()!r}" for path, lines in allocators)
        sys.exit(f"the segments' allocator= lines differ beyond the free memory: {names}")

    order, taken = [], {}
    for path, _, _, cases in segments:
        for case in cases:
            if case not in taken:
                order.append(case)
                taken[case] = None
    winners = {}
    for path, _, _, cases in sorted(segments, key=lambda s: attempt(s[0])):
        for case, rows in cases.items():
            if case in winners and winners[case][0] == attempt(path):
                sys.exit(f"{winners[case][1]} and {path} both hold {' '.join(case)} at attempt "
                         f"{attempt(path)}: which one its section describes is unknown")
            winners[case] = (attempt(path), path, rows)

    heading = [allocator if line.startswith(ALLOCATOR) else line for line in first_heading]
    out = pathlib.Path(args.out)
    with open(out, "w", newline="") as fh:
        fh.write("".join(heading) + columns + "".join("".join(winners[case][2]) for case in order))
    print(f"{out}: {len(order)} cases from {len(segments)} segments")


if __name__ == "__main__":
    main()
