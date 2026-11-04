#!/usr/bin/env python3
"""A device's case list, `testdata/benchmark-results/<device>/cases.csv`, read for scripts.

    python3 scripts/calibration/device_cases.py <csv> [--dataset <ds>] [--mode <mode>]

Prints one line per `ok` and `wrong` row, in file order: `dataset<TAB>sf<TAB>query<TAB>modes`, the
modes `|`-joined. `--dataset` keeps that dataset's rows, `--mode` the rows whose modes carry it. A
`skipped` row is no case. `parse_device_cases` (Rust, `test_support`) owns the format and these
are its rules; both readers are tested against `testdata/fixtures/device-cases.csv`.
llm-wiki/build-test.md, "Corpus benchmarks", has the format.
"""

import argparse
import pathlib
import re
import sys

HEADER = "dataset,sf,query,modes,status,note"

# `MODES`' idents in `test_support/mod.rs`, in its order.
MODES = ("tp1_single", "tp1_rowgroup", "tp4_single", "tp4_rowgroup", "tp4_sized")

STATUSES = ("ok", "skipped", "wrong")

TICKET = re.compile(r"#[0-9]+")


def row(line):
    """One row as a dict, or the reason the format refuses it."""
    fields = line.split(",", 5)
    if len(fields) != 6:
        return f"not six fields: {HEADER}"
    dataset, sf, query, modes, status, note = fields
    modes = modes.split("|") if modes else []
    unknown = [mode for mode in modes if mode not in MODES]
    if unknown:
        return f"unknown mode {unknown[0]!r} (expected one of {list(MODES)})"
    if status not in STATUSES:
        return f"unknown status {status!r} (expected ok, skipped or wrong)"
    if status == "skipped" and modes:
        return "a skipped row times no mode; its modes are empty"
    if status == "skipped" and not note:
        return "a skipped row names its reason in the note"
    if status != "skipped" and not modes:
        return "an ok or wrong row names the modes it ran"
    if status == "ok" and note:
        return "an ok row has no note"
    if status == "wrong" and not TICKET.fullmatch(note):
        return "a wrong row's note is its ticket, #NN"
    return {"dataset": dataset, "sf": sf, "query": query, "modes": modes, "status": status,
            "note": note}


def rows(path):
    """Every row of the list at `path`, in file order; exits naming the line on one it refuses."""
    lines = pathlib.Path(path).read_text().splitlines()
    header = lines[0] if lines else ""
    if header != HEADER:
        sys.exit(f"{path}: device case list line 1 {header!r}: the first line is not {HEADER!r}")
    read = []
    first_at = {}
    for number, line in enumerate(lines[1:], start=2):
        case = row(line)
        if isinstance(case, str):
            sys.exit(f"{path}: device case list line {number} {line!r}: {case}")
        key = (case["dataset"], case["sf"], case["query"])
        if key in first_at:
            sys.exit(f"{path}: device case list line {number} {line!r}: the case is listed "
                     f"twice, first at line {first_at[key]}")
        first_at[key] = number
        read.append(case)
    return read


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("csv", help="a device's cases.csv")
    parser.add_argument("--dataset", help="keep this dataset's rows")
    parser.add_argument("--mode", choices=MODES, help="keep the rows whose modes carry it")
    args = parser.parse_args(argv)
    for case in rows(args.csv):
        if case["status"] == "skipped":
            continue
        if args.dataset is not None and case["dataset"] != args.dataset:
            continue
        if args.mode is not None and args.mode not in case["modes"]:
            continue
        print("\t".join((case["dataset"], case["sf"], case["query"], "|".join(case["modes"]))))


if __name__ == "__main__":
    main()
