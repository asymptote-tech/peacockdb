"""The calibration record's format, read in one place and written in one place.

The record is the only file three scripts share: `nsys_hbm.py` joins a capture onto it,
`plot.py` draws it, and `nsys_calls.py` writes a file in the same shape. Two readings of
one format disagree the first time a column moves, and the record's columns have already
moved twice.

Everything is read by name. A reader that counts fields survives a column change quietly,
drawing or joining whatever slid into the slot it wanted.

The `# run:` lines are the conditions the run measured under — timing mode, build,
allocator, capture. They are not columns because they are constant across a file, but each
one changes what the microseconds mean, so two files that disagree about them cannot be
read as one set of samples. `record.rs` refuses to append across them; this refuses to read
across them.
"""

import statistics
import sys

RUN_PREFIX = "# run: "

# hbm.tsv's shape: a row per call — the record's coordinates without `run_index` — then how
# many executions the row stands for, then the counters' readings, each their median over
# those executions.
HBM_CALL = ("dataset", "sf", "query", "mode", "node_seq", "node_type", "lane",
            "recipe_seq", "recipe_kind", "call_index")
HBM_VALUES = ("hbm_read_bytes", "hbm_write_bytes", "hbm_bytes", "samples",
              "device_busy_us", "device_idle_us")
HBM_COLUMNS = HBM_CALL + ("executions",) + HBM_VALUES


def read_record(path, *required):
    """`(run, rows)` for one record file: its conditions, and its rows as dicts.

    A second value for a condition already stated is refused rather than taking the last:
    a file holding two runs' headings is two measurements concatenated, and every row after
    the seam was taken under conditions the first heading does not describe.
    """
    run, rows, names = {}, [], None
    with open(path) as fh:
        for line in fh:
            if line.startswith(RUN_PREFIX):
                key, _, value = line[len(RUN_PREFIX):].strip().partition("=")
                if key in run and run[key] != value:
                    sys.exit(
                        f"{path} states {key}={run[key]!r} and {key}={value!r}. One file is "
                        "one run; this one holds two, and the rows do not say which."
                    )
                run[key] = value
                continue
            if line.startswith("#"):
                continue
            fields = line.rstrip("\n").split("\t")
            if names is None:
                names = fields
                continue
            if len(fields) != len(names):
                sys.exit(f"{path}: row of {len(fields)} fields against {len(names)} columns")
            rows.append(dict(zip(names, fields)))
    if names is None:
        sys.exit(f"{path} has no column line")
    require(rows, path, *required)
    return run, rows


def read_records(paths, *required):
    """`(run, rows)` over several record files read as one: a run published a file per
    dataset, say. Their conditions must agree — two files that disagree about a `# run:`
    line are two measurements, not one set of samples — and their rows are concatenated in
    the order the paths are given.
    """
    conditions, taken_from, rows = None, None, []
    for path in paths:
        run, got = read_record(path, *required)
        if conditions is not None and run != conditions:
            differ = sorted(k for k in set(run) | set(conditions)
                            if run.get(k) != conditions.get(k))
            sys.exit(
                f"{path} and {taken_from} were taken under different conditions "
                f"({', '.join(differ)}), so their microseconds do not mean the same thing. "
                "Read them separately."
            )
        conditions, taken_from = run, path
        rows += got
    return conditions, rows


def hbm_per_call(rows):
    """hbm rows a row per call, as `HBM_COLUMNS`, in the order calls first appear.

    Decided row by row: a row carrying `executions` is a call already and passes through; a
    row carrying `run_index` is one execution, folded into its call — each reading the
    median over the executions, `executions` how many. The median rather than the mean
    because a call's traffic is a handful of metric samples and one execution in ten can
    read several times the others; it drags a mean and not a median.
    """
    by_call = {}
    for row in rows:
        key = tuple(row[c] for c in HBM_CALL)
        if "executions" in row:
            by_call[key] = row
        else:
            by_call.setdefault(key, []).append(row)
    out = []
    for key, group in by_call.items():
        if isinstance(group, dict):
            out.append(group)
            continue
        merged = dict(zip(HBM_CALL, key), executions=str(len(group)))
        for column in HBM_VALUES:
            merged[column] = str(round(statistics.median(int(r[column]) for r in group)))
        out.append(merged)
    return out


def read_tsv(path, *required):
    """The rows of a derived file — `hbm.tsv`, `calls.tsv` — which carry no conditions."""
    _, rows = read_record(path, *required)
    return rows


def require(rows, path, *columns):
    if not rows:
        sys.exit(f"{path} has a heading and no rows")
    missing = [c for c in columns if c not in rows[0]]
    if missing:
        sys.exit(f"{path} has no {missing}; it has {sorted(rows[0])}")


def write_tsv(path, notes, columns, rows):
    """One derived file: a `#` preamble, the column line, then the rows.

    The preamble is what a reader has instead of this script, so it says what the file is
    rather than how it was made. `rows` are sequences in `columns` order.
    """
    with open(path, "w") as fh:
        for line in notes:
            fh.write(f"# {line}\n" if line else "#\n")
        fh.write("\t".join(columns) + "\n")
        for row in rows:
            fh.write("\t".join(str(cell) for cell in row) + "\n")
