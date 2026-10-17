#!/usr/bin/env python3
"""A benchmark run's record cut into one file per dataset, `<out-dir>/<dataset>.sf<sf>/records.tsv`.

    python3 scripts/calibration/split_record.py --record testdata/calibration/records.tsv \
        --cases peacockdb-core/tests/common/corpus_benchmark_cases.inc --out-dir <dir>

Prints each `<dataset>.sf<sf>` it wrote, for `build-test-shadgpu.sh --pull-benchmarks` to publish:
the run's heading and that dataset's rows in order, byte for byte. Refused, writing nothing,
unless the heading says `capture=none` and each dataset's (query, mode) set is exactly what the
case file declares: a run truncates the record when it starts, so a filtered run's is part of
the full one under the same name. llm-wiki/build-test.md, *Benchmark data flow*, has the rest.
"""

import argparse
import pathlib
import sys

MACRO = "corpus_query_benchmark!"


def declared(path):
    """`{(dataset, sf): {(query, mode)}}` from the case file, spelled as the record spells them.

    Read as `test_corpus_goldens` reads it (`read_cases`): one invocation per line, its
    arguments up to the last `)`, and a trailing comment allowed. `none` declares no mode. Both
    names lose their `_` as `corpus_query_benchmark!` does (`peacock_gpu_benchmarks.rs`).
    """
    cases = {}
    for line in pathlib.Path(path).read_text().splitlines():
        text = line.strip()
        if not text.startswith(MACRO):
            continue
        args, _, tail = text[len(MACRO):].partition("(")[2].rpartition(")")
        if tail.partition("//")[0].strip() != ";" or args.count(",") != 3:
            sys.exit(f"{path}: {line!r} is not one {MACRO}(dataset, sf, query, modes);")
        dataset, sf, query, modes = (arg.strip() for arg in args.split(","))
        named = [] if modes == "none" else [m.strip().replace("_", "-") for m in modes.split("|")]
        query = query.replace("_", "-")
        cases.setdefault((dataset, sf), set()).update((query, mode) for mode in named)
    if not cases:
        sys.exit(f"{path} declares no {MACRO} case")
    return cases


def read(path):
    """The heading lines, the column line and the rows, each with its newline."""
    heading, columns, rows = [], None, []
    with open(path, newline="") as fh:
        for line in fh:
            if columns is None and line.startswith("#"):
                heading.append(line)
            elif columns is None:
                columns = line
            else:
                rows.append(line)
    if columns is None or not rows:
        sys.exit(f"{path} has no rows")
    if "# run: capture=none\n" not in heading:
        sys.exit(f"{path} has no `# run: capture=none` line: a captured run's microseconds are "
                 "a profiler's, and only a plain run's record is published")
    return heading, columns, rows


def by_dataset(columns, rows):
    """`{(dataset, sf): (rows, {(query, mode)})}`, rows in file order."""
    names = columns.rstrip("\n").split("\t")
    at = [names.index(column) for column in ("dataset", "sf", "query", "mode")]
    datasets = {}
    for line in rows:
        dataset, sf, query, mode = (line.rstrip("\n").split("\t")[i] for i in at)
        lines, timed = datasets.setdefault((dataset, sf), ([], set()))
        lines.append(line)
        timed.add((query, mode))
    return datasets


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--record", required=True, help="the run's records.tsv")
    parser.add_argument("--cases", required=True, help="corpus_benchmark_cases.inc")
    parser.add_argument("--out-dir", required=True, help="where the files are written")
    args = parser.parse_args(argv)

    cases = declared(args.cases)
    heading, columns, rows = read(args.record)
    datasets = by_dataset(columns, rows)

    problems = []
    for (dataset, sf), (_, timed) in sorted(datasets.items()):
        want = cases.get((dataset, sf), set())
        for query, mode in sorted(want - timed):
            problems.append(f"{dataset}.sf{sf}: {query} {mode} is declared and the run did not time it")
        for query, mode in sorted(timed - want):
            problems.append(f"{dataset}.sf{sf}: {query} {mode} was timed and is not declared")
    if problems:
        sys.exit(f"{args.record} is not a whole run of what {args.cases} declares, so it "
                 "would publish part of one:\n  "
                 + "\n  ".join(problems))

    for (dataset, sf), (lines, _) in sorted(datasets.items()):
        name = f"{dataset}.sf{sf}"
        path = pathlib.Path(args.out_dir) / name / "records.tsv"
        path.parent.mkdir(parents=True, exist_ok=True)
        with open(path, "w", newline="") as fh:
            fh.write("".join(heading) + columns + "".join(lines))
        print(name)


if __name__ == "__main__":
    main()
