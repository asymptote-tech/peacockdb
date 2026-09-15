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

import sys

RUN_PREFIX = "# run: "


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
