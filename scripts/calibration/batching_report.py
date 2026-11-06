#!/usr/bin/env python3
"""The query-batching page's numbers: `llm-wiki/reports/query-batching.md`, between its markers.

    python3 scripts/calibration/batching_report.py --testdata testdata --report <md> [--check]

Reads the H100 `cases.csv` (through `device_cases`), the unfused and fused benchmark trees, the
DuckDB results and the `loads.tsv` profiles under `--testdata`. Every peacockdb total is
device-side, `load_us + execute_us`; DuckDB's is wall time. This script owns every number
between `BEGIN` and `END`; the prose around them is written by hand. `--check` exits 1 when the
page is stale. llm-wiki/tasks/batching-report-impl.md has the layout.
"""

import argparse
import pathlib
import re
import sys
from collections import Counter

import device_cases
import record

BATCH_SIZES = [("first3", 3), ("first10", 10), ("first20", 20), ("all", None)]  # query-fuser's
DATASETS = ["tpch", "tpcds"]
TITLES = {"tpch": "TPC-H", "tpcds": "TPC-DS"}
SF = "40"
DEVICE = "h100-80gb-hbm3"
DUCKDB_TREE = "duckdb-cpu128"
MODE = "tp1_single"
LOADS_MODE = "tp1-single"  # the case text's mode, as `calls.tsv` writes it
BEGIN = "<!-- batching_report.py: generated below, do not edit -->"
END = "<!-- batching_report.py: generated above -->"

# containers' `test_support/mod.rs`: a section body that is a marker, or a failed case's record.
SKIPPED = "skipped: "
FAILED = "status=failed reason="
WRONG = "wrong answer: "
TPCH_QUERY = re.compile(r"q([1-9]|1[0-9]|2[0-2])")
BLOCK = re.compile(r"--- (\w+) ---")
DUCKDB_COLUMNS = ("query", "ms_2nd_min", "runs_ms", "workset_bytes", "disk_read_bytes", "warm",
                  "status")
LOAD_COLUMNS = ("load_us", "host_read_us", "h2d_us", "decompress_us", "decode_us",
                "other_device_us", "unattributed_us")
CACHE_MOVES = ("fallbacks", "evictions", "reloads")
H100_KEYS = ("device", "driver", "cuda", "cudf", "host")
DUCKDB_KEYS = ("duckdb", "threads", "host", "cpu", "cores")
MERGE_MODEL = ("identical subtrees above a shared load, with members from two or more queries, "
               "run once and the dearest copy stays: Σ t − max t over each group, t the queries' "
               "own unfused node times (sf40-baseline)")


class Refused(Exception):
    """An input the page cannot be built from; the message names the file and what is wrong."""


# --- reading ---------------------------------------------------------------------------------


def sections(text):
    """`(name, body)` per `== name` section, in file order."""
    out = []
    for line in text.splitlines(keepends=True):
        if line.startswith("== "):
            out.append((line[3:].rstrip("\n"), []))
        elif out:
            out[-1][1].append(line)
        elif line.strip():
            raise ValueError(f"a line above the first `== ` section: {line!r}")
    return [(name, "".join(body)) for name, body in out]


def pairs(lines):
    """`key=value` lines as a dict, split at the first `=`; a key given twice is refused."""
    out = {}
    for line in lines:
        key, sep, value = line.partition("=")
        if not sep:
            raise ValueError(f"{line!r} is not `key=value`")
        if key in out:
            raise ValueError(f"{key} is given twice: {out[key]!r} and {value!r}")
        out[key] = value
    return out


def trailer(body):
    """A timed section's `--- run ---` keys; `None` for a marker or a failed record."""
    if body.startswith(SKIPPED) or body.startswith(FAILED):
        return None
    lines = body.splitlines()
    if lines.count("--- run ---") != 1:
        raise ValueError("a timed section has one `--- run ---` trailer, and this one has "
                         f"{lines.count('--- run ---')}")
    return pairs(line for line in lines[lines.index("--- run ---") + 1:] if line.strip())


def fused_batch(body):
    """A fused section's blocks (query-fuser-detail.md, "The fused format"); a marker gives
    `{"marker": …}` and a failed batch `{"failed": reason}`."""
    if body.startswith(SKIPPED):
        return {"marker": body.strip()}
    if body.startswith(FAILED):
        return {"failed": body.splitlines()[0][len(FAILED):]}
    blocks, block = {}, None
    for line in body.splitlines():
        opened = BLOCK.fullmatch(line)
        if opened:
            block = opened[1]
            if block in blocks:
                raise ValueError(f"the `--- {block} ---` block twice")
            blocks[block] = []
        elif block is None:
            raise ValueError(f"{line!r} lies above `--- batch ---`")
        elif line.strip():
            blocks[block].append(line)
    missing = [b for b in ("batch", "merge", "pipeline", "run") if b not in blocks]
    if missing:
        raise ValueError(f"a fused section without its {missing} blocks")
    batch = pairs(line for line in blocks["batch"] if not line.startswith("hold "))
    merge = pairs(line for line in blocks["merge"] if not line.startswith("entry "))
    entries = [int(pairs([line.split(" ")[1]])["saved_us"])
               for line in blocks["merge"] if line.startswith("entry ")]
    return {"queries": batch["queries"].strip("[]").split(","), "batch": batch,
            "merge": {"saved_us": int(merge["saved_us"]), "entries": entries},
            "pipeline": pairs(blocks["pipeline"]), "run": pairs(blocks["run"])}


def duckdb(text):
    """`(preamble, rows by query)` of a `duckdb.tsv`, by the rules of its owner reader,
    `parse_duckdb_results` (`test_support/duckdb_results.rs`); a failed row's `-` is `None`."""
    lines = text.splitlines()

    def refuse(number, line, why):
        raise ValueError(f"duckdb results line {number} {line!r}: {why}")

    first = lines[0] if lines else ""
    words = [word.partition("=") for word in first[2:].split(" ")]
    if not first.startswith("# ") or any(not key or not sep for key, sep, _ in words):
        refuse(1, first, "not a `# ` line of key=value words")
    preamble = {key: value for key, _, value in words}
    if len(lines) < 2 or lines[1] != "\t".join(DUCKDB_COLUMNS):
        refuse(2, lines[1] if len(lines) > 1 else "", "the column line is not the format's")
    rows = {}
    for number, line in enumerate(lines[2:], start=3):
        fields = line.split("\t")
        if len(fields) != 7:
            refuse(number, line, "not seven fields")
        query, ms, runs, workset, read, warm, status = fields
        try:
            values = dict(workset_bytes=None if workset == "-" else count(workset))
            if status == "ok":
                if warm not in ("yes", "partial", "NO"):
                    raise ValueError(f"warm {warm!r} is not yes, partial or NO")
                values.update(ms_2nd_min=millis(ms), runs_ms=[millis(r) for r in runs.split(",")],
                              disk_read_bytes=count(read), warm=warm, status="ok")
            elif status == "failed":
                if any(value != "-" for value in (ms, runs, read, warm)):
                    raise ValueError("a failed row has no time: ms_2nd_min, runs_ms, "
                                     "disk_read_bytes and warm are '-'")
                values.update(ms_2nd_min=None, runs_ms=[], disk_read_bytes=None, warm=warm,
                              status="failed")
            else:
                raise ValueError(f"unknown status {status!r} (expected ok or failed)")
        except ValueError as why:
            refuse(number, line, str(why))
        if query in rows:
            refuse(number, line, "the query is listed twice")
        rows[query] = {key: values[key] for key in DUCKDB_COLUMNS[1:]}
    return preamble, rows


def millis(text):
    try:
        value = float(text)
    except ValueError:
        value = -1.0
    if not 0 <= value < float("inf"):
        raise ValueError(f"{text!r} is not a number of milliseconds")
    return value


def count(text):
    if not text.isdigit():
        raise ValueError(f"{text!r} is not a count of bytes")
    return int(text)


def counted(cases, dataset):
    """The batch set, as query-fuser's `batch_set`: `ok` and `wrong` rows at tp1_single, in
    file order, TPC-H's q1-q22 only (the micro queries are not batched)."""
    return [case["query"] for case in cases
            if case["dataset"] == dataset and case["sf"] == SF
            and case["status"] in ("ok", "wrong") and MODE in case["modes"]
            and (dataset != "tpch" or TPCH_QUERY.fullmatch(case["query"]))]


def notes(cases, dataset):
    """`(query, note)` for every skipped and wrong-answer row of the dataset, micro queries too."""
    out = []
    for case in cases:
        if case["dataset"] != dataset or case["sf"] != SF:
            continue
        if case["status"] == "skipped":
            out.append((case["query"], f"skipped at sf{SF}: {case['note']}"))
        elif case["status"] == "wrong":
            out.append((case["query"], f"{WRONG}{case['note']}"))
    return out


def batches(queries):
    """Each batch's prefix of `queries`, as query-fuser's `batches`: `None` for a sized batch
    other than first3 that the list does not exceed ("same set as all")."""
    return [(name, None if size is not None and name != "first3" and len(queries) <= size
             else queries[:size]) for name, size in BATCH_SIZES]


def load_split(rows):
    """Σ over `loads.tsv` rows of the load's parts, decompress and decode as one."""
    total = {column: sum(int(row[column]) for row in rows) for column in LOAD_COLUMNS}
    total["decode_us"] += total.pop("decompress_us")
    return {key: total[key] for key in ("load_us", "host_read_us", "h2d_us", "decode_us",
                                        "other_device_us", "unattributed_us")}


# --- one dataset, gathered -------------------------------------------------------------------


def named(path):
    """A benchmark file's sections by name; a name given twice is refused."""
    by_name = {}
    for name, body in sections(path.read_text()):
        if name in by_name:
            raise Refused(f"{path}: the section `== {name}` is written twice")
        by_name[name] = body
    return by_name


def gather(testdata, dataset, cases):
    """Everything the page says about one dataset, checked against today's list."""
    device = testdata / "benchmark-results" / DEVICE
    unfused_path = device / f"{dataset}.sf{SF}" / f"{MODE}.benchmark.txt"
    fused_path = device / "fused" / f"{dataset}.sf{SF}" / f"{MODE}.fused.txt"
    tp4_path = device / f"{dataset}.sf{SF}" / "tp4_sized.benchmark.txt"
    duckdb_path = testdata / "benchmark-results" / DUCKDB_TREE / f"{dataset}.sf{SF}" / "duckdb.tsv"
    profiles = testdata / "calibration" / DEVICE
    loads_path = profiles / f"{dataset}.sf{SF}" / "loads.tsv"
    queries = counted(cases, dataset)
    unfused, fused = named(unfused_path), named(fused_path)
    extra = [name for name in fused if name not in dict(BATCH_SIZES)]
    if extra:
        raise Refused(f"{fused_path}: {extra} are not batches of {[n for n, _ in BATCH_SIZES]}")
    listed = batches(queries)
    for name, batch in listed:
        if name not in fused:
            raise Refused(f"{fused_path}: no `== {name}` section")
        check_batch(fused_path, name, batch, fused_batch(fused[name]), len(queries))
    timings = {query: timing(unfused_path, unfused, query) for query in queries}
    loads = [row for row in record.read_tsv(loads_path, "query", "mode", "host_read_source",
                                            *LOAD_COLUMNS) if row["mode"] == LOADS_MODE]
    by_query = {query: [row for row in loads if row["query"] == query] for query in queries}
    unprofiled = [query for query, rows in by_query.items() if not rows]
    if unprofiled:
        raise Refused(f"{loads_path}: no {LOADS_MODE} row for the counted {unprofiled}")
    preamble, duck = duckdb(duckdb_path.read_text())
    tp4 = named(tp4_path) if tp4_path.exists() else None
    fused_dir = profiles / "fused" / f"{dataset}.sf{SF}"
    failed_profiles = profile_failures(fused_dir / "failed.tsv")
    out = []
    for name, batch in listed:
        read = fused_batch(fused[name])
        entry = {"name": name, "queries": batch, "fused": read}
        if batch is not None:
            entry["unfused"] = [sum(int(timings[q][key]) for q in batch)
                                for key in ("load_us", "execute_us")]
            entry["duckdb"] = partial_sum(batch, {q: r["ms_2nd_min"] for q, r in duck.items()})
            if tp4 is not None:
                entry["tp4"] = [partial_sum(batch, tp4_times(tp4, key)) for key in
                                ("load_us", "execute_us")]
            entry["split"] = {"unfused": load_split([r for q in batch for r in by_query[q]])}
            if name in failed_profiles:
                entry["split"]["fused"] = failed_profiles[name]
            elif "run" in read and read["batch"]["outcome"] == "measured":
                path = fused_dir / f"{name}.loads.tsv"
                if not path.exists():
                    raise Refused(f"{path}: the measured batch {name} has no profile")
                entry["split"]["fused"] = load_split(
                    record.read_tsv(path, "query", "mode", *LOAD_COLUMNS))
        out.append(entry)
    return {"dataset": dataset, "queries": queries, "batches": out, "notes": notes(cases, dataset),
            "duckdb": (preamble, duck), "tp4": tp4, "unfused": unfused, "fused": fused,
            "sources": sorted({row["host_read_source"] for row in loads})}


def timing(path, by_name, query):
    """A counted query's unfused trailer; a missing or untimed one is refused, since
    sf40-baseline's writer makes those `skipped` rows, not `ok` ones."""
    if query not in by_name:
        raise Refused(f"{path}: the counted {query} has no section")
    read = trailer(by_name[query])
    if read is None or "load_us" not in read:
        first = by_name[query].splitlines()[0] if by_name[query] else ""
        raise Refused(f"{path}: the counted {query} is not timed: {first}")
    return read


def tp4_times(by_name, key):
    out = {}
    for query, body in by_name.items():
        read = trailer(body)
        out[query] = int(read[key]) / 1000 if read is not None else None
    return out


def partial_sum(batch, values):
    """`(Σ, n, missing)` over the batch's queries that have a value."""
    have = [values[q] for q in batch if values.get(q) is not None]
    return sum(have), len(have), [q for q in batch if values.get(q) is None]


def check_batch(path, name, batch, read, listed):
    """The fused section must be the batch today's list makes, or the sums would be of
    other queries than the fused run's."""
    if "failed" in read:
        return
    if batch is None and "marker" not in read:
        raise Refused(f"{path}: {name} ran as a batch, and today's list ({listed} queries) "
                      "makes it the same set as all")
    if batch is not None and "marker" in read:
        raise Refused(f"{path}: {name} is a marker, and today's list makes it {len(batch)} "
                      "queries")
    if batch is not None and read["queries"] != batch:
        raise Refused(f"{path}: {name} fused {read['queries']}, and today's list's prefix is "
                      f"{batch}: the list changed after the fused run")


def profile_failures(path):
    """`failed.tsv`'s batches (`batch<TAB>pass<TAB>reason`), each its passes' reasons."""
    out = {}
    if path.exists():
        for line in path.read_text().splitlines():
            batch, step, reason = line.split("\t", 2)
            out.setdefault(batch, []).append(f"{step}: {reason}")
    return {batch: "; ".join(reasons) for batch, reasons in out.items()}


# --- rendering -------------------------------------------------------------------------------


def ms(us):
    return f"{int(us) / 1000:.1f}"


def gib(size):
    return f"{int(size) / 2**30:.1f}"


def ratio(top, bottom):
    return f"{top / bottom:.2f}×"


def cell(text):
    return str(text).replace("|", "\\|")


def table_lines(columns, rows):
    lines = ["| " + " | ".join(columns) + " |",
             "|" + "|".join("---" if at < 2 else "--:" for at in range(len(columns))) + "|"]
    lines += ["| " + " | ".join(cell(c) for c in row) + " |" for row in rows]
    return lines


def distinct(values, unit=""):
    """Every distinct value with its count, in first-seen order."""
    return ", ".join(f"{value}{unit} ({n})" for value, n in Counter(values).items())


def counted_out(total, n, missing, of):
    return f"{total:.1f}" + (f" ({n}/{of})" if missing else "")


def main_table(data):
    tp4 = data["tp4"] is not None
    columns = ["batch", "queries", "fused load (ms)", "fused execute (ms)", "unfused load (ms)",
               "unfused execute (ms)"]
    columns += ["tp4_sized load (ms)", "tp4_sized execute (ms)"] if tp4 else []
    columns += ["DuckDB warm (ms)", "unfused / fused", "DuckDB / fused"]
    rows = []
    for entry in data["batches"]:
        batch, read = entry["queries"], entry["fused"]
        if batch is None:
            rows.append([entry["name"], "same set as all"] + [""] * (len(columns) - 2))
            continue
        fused_total = None
        if "failed" in read:
            fused_cells = [f"failed: {read['failed']}", ""]
        elif read["batch"]["outcome"] == "measured":
            load, execute = int(read["run"]["load_us"]), int(read["run"]["execute_us"])
            fused_total = load + execute
            fused_cells = [ms(load), ms(execute)]
        else:
            fused_cells = [f"out of memory (cap {read['batch']['cap']}); budget "
                           f"{gib(read['batch']['budget_bytes'])} GiB", ""]
        unfused = entry["unfused"]
        row = [entry["name"], str(len(batch)), *fused_cells, ms(unfused[0]), ms(unfused[1])]
        if tp4:
            row += [counted_out(t, n, miss, len(batch)) for t, n, miss in entry["tp4"]]
        duck_ms, n, missing = entry["duckdb"]
        row.append(counted_out(duck_ms, n, missing, len(batch)))
        if fused_total is None:
            row += ["", ""]
        else:
            row.append(ratio(sum(unfused), fused_total))
            row.append("—" if missing else ratio(duck_ms * 1000, fused_total))
        rows.append(row)
    return table_lines(columns, rows) + [""] + main_notes(data)


def main_notes(data):
    measured = [e for e in data["batches"] if e["queries"] is not None]
    lines = ["Totals are `load + execute` per side; ratios above 1 mean fused is faster. `(n/N)`: "
             "the sum covers n of the batch's N queries."]

    def holding(query):
        return ", ".join(e["name"] for e in measured if query in e["queries"])

    wrong = dict(data["notes"])
    for query in data["queries"]:
        if wrong.get(query, "").startswith(WRONG):
            lines.append(f"- {query}: counted, though its answer is wrong "
                         f"({wrong[query][len(WRONG):]}); in {holding(query)}.")
    preamble, duck = data["duckdb"]
    for query in data["queries"]:
        if query not in duck:
            lines.append(f"- DuckDB: {query} has no row; counted out of {holding(query)}.")
        elif duck[query]["status"] != "ok":
            lines.append(f"- DuckDB: {query} failed; counted out of {holding(query)}.")
    if data["tp4"] is not None:
        for query in data["queries"]:
            body = data["tp4"].get(query)
            if body is None:
                lines.append(f"- tp4_sized: {query} has no section; counted out of "
                             f"{holding(query)}.")
            elif trailer(body) is None:
                lines.append(f"- tp4_sized: {query} {body.splitlines()[0]}; counted out of "
                             f"{holding(query)}.")
    return lines


def hints_table(data):
    columns = ["batch", "cap", "estimate / budget (GiB)", "accountant's peak (GiB)",
               "merge saves (ms, estimate)", "entries", "pipelining saves (ms, estimate)",
               "pipelined estimate (ms)"]
    rows, models = [], []
    for entry in data["batches"]:
        read = entry["fused"]
        if entry["queries"] is None:
            rows.append([entry["name"], "same set as all"] + [""] * 6)
            continue
        if "failed" in read:
            rows.append([entry["name"], "failed"] + [""] * 6)
            continue
        batch, pipeline = read["batch"], read["pipeline"]
        models.append(pipeline["model"])
        measured = batch["outcome"] == "measured"
        budget = f"{gib(batch['estimate_bytes'])} / {gib(batch['budget_bytes'])}"
        if batch["over_budget"] == "true":
            budget += " (over budget)"
        rows.append([entry["name"], batch["cap"], budget,
                     gib(read["run"]["peak_bytes"]) if measured else "—",
                     ms(read["merge"]["saved_us"]), str(len(read["merge"]["entries"])),
                     ms(pipeline["saved_us"]),
                     ms(pipeline["pipelined_estimate_us"]) if measured else "—"])
    return table_lines(columns, rows) + [
        "",
        f"- Merge, an estimate: {MERGE_MODEL}.",
        f"- Pipelining, an estimate: {distinct(models) if len(set(models)) > 1 else models[0]}; "
        "the saving is serial − makespan, the pipelined estimate the fused measured total less "
        "it." if models else "- Pipelining: no batch carries the model.",
        "- The accountant's peak is a lower bound on device memory; the estimate and budget are "
        "the cap rule's (query-fuser).",
    ]


def split_table(data):
    columns = ["batch", "side", "Σ load", "host read", "H2D copy", "decompress+decode",
               "other device", "unattributed"]
    rows = []
    for entry in data["batches"]:
        if entry["queries"] is None:
            rows.append([entry["name"], "same set as all"] + [""] * 6)
            continue
        for side, split in entry["split"].items():
            if isinstance(split, str):
                rows.append([entry["name"], side, f"profile failed: {split}"] + [""] * 5)
            else:
                rows.append([entry["name"], side, *(ms(split[key]) for key in split)])
    sources = ", ".join(data["sources"]) or "none"
    return table_lines(columns, rows) + [
        "",
        "ms, from one profiled run per query process (per fused batch) under Nsight: each number "
        "is a per-execution median, summed over the batch's loads, and is not the timed tables' "
        f"time. Unfused `host_read_source`: {sources}. With `kvikio` the host read holds "
        "kvikio's bounce copies, which H2D counts too, so the two are not added.",
    ]


def timed_sections(gathered):
    """`(label, trailer, kind)` for every timed section the H100 wrote: unfused, tp4_sized and
    fused, in file order. `kind` is `tree` or `fused`."""
    out = []
    for data in gathered:
        ds = data["dataset"]
        trees = [("", data["unfused"])] + ([(" tp4_sized", data["tp4"])] if data["tp4"] else [])
        for suffix, by_name in trees:
            for name, body in by_name.items():
                read = trailer(body)
                if read is not None and "run_us" in read:
                    out.append((f"{ds} {name}{suffix}", read, "tree"))
        for name, body in data["fused"].items():
            read = fused_batch(body)
            if "run" in read and read["batch"]["outcome"] == "measured":
                out.append((f"{ds} {name} fused", read["run"], "fused"))
    return out


def cache_label(text):
    if text is None:
        return "unknown"
    return " ".join(word for word in text.split(" ")
                    if "=" not in word or word.startswith(("cap=", "source=")))


def cache_cell(timed):
    caches = [read.get("pinned_cache") for _, read, _ in timed]
    resident = [float(word[len("resident="):-len("GiB")]) for text in caches if text
                for word in text.split(" ") if word.startswith("resident=")]
    prefilled = [(int(read["prefill_bytes"]), read["prefill_us"], label)
                 for label, read, _ in timed if "prefill_bytes" in read]
    not_on = [label for label, read, _ in timed
              if not read.get("pinned_cache", "").startswith("on")]
    unfilled = [label for label, read, kind in timed if kind == "tree"
                and read.get("pinned_cache", "").startswith("on")
                and read.get("prefill_files") == "0"]
    parts = [distinct(cache_label(text) for text in caches)]
    parts.append(f"resident ≤ {max(resident):.1f}GiB" if resident else "resident: never on")
    if prefilled:
        size, us, label = max(prefilled, key=lambda p: p[0])
        parts.append(f"largest prefill {size} B in {us} µs ({label})")
    parts.append(f"cache not on: {', '.join(not_on) or 'none'}")
    parts.append(f"tree sections not prefilled: {', '.join(unfilled) or 'none'}")
    parts.append(f"fell back, evicted or reloaded: {', '.join(moved(timed)) or 'none'}")
    return "; ".join(parts)


def moved(timed):
    """Sections whose cache counters show a fallback, an eviction or a reload, with them: a
    run that read a file from disk fails its case, so a timed section should show none."""
    out = []
    for label, read, _ in timed:
        words = read.get("pinned_cache", "").split(" ")
        counters = [word for word in words if word.partition("=")[0] in CACHE_MOVES
                    and word.partition("=")[2] != "0"]
        if counters:
            out.append(f"{label} ({' '.join(counters)})")
    return out


def provenance(gathered):
    timed = timed_sections(gathered)
    lines = ["### Provenance: the H100", ""]
    rows = [[key, distinct(read.get(key, "unknown") for _, read, _ in timed)] for key in H100_KEYS]
    rows.append(["pinned host cache", cache_cell(timed)])
    lines += table_lines(["key", "values (sections)"], rows)
    lines += ["", "### Provenance: DuckDB", ""]
    preambles = [data["duckdb"][0] for data in gathered]
    lines += table_lines(["key", "values (datasets)"],
                         [[key, distinct(p.get(key, "unknown") for p in preambles)]
                          for key in DUCKDB_KEYS])
    tree_runs = [len(read["runs"].strip("[]").split(",")) for _, read, kind in timed
                 if kind == "tree"]
    fused_runs = "; ".join(
        f"{data['dataset']} " + ", ".join(
            f"{e['name']} {len(e['fused']['run']['runs'].strip('[]').split(','))}"
            for e in data["batches"] if e["queries"] is not None and "run" in e["fused"]
            and e["fused"]["batch"]["outcome"] == "measured")
        for data in gathered)
    duck_runs = [len(row["runs_ms"]) for data in gathered for row in data["duckdb"][1].values()
                 if row["status"] == "ok"]
    lines += ["", "### Provenance: protocol", "",
              "Each reported time is the 2nd-minimum of its runs. Runs: peacockdb unfused "
              f"{distinct(tree_runs, ' runs')}; fused, per batch: {fused_runs}; DuckDB "
              f"{distinct(duck_runs, ' runs')}.",
              "",
              "Before any run, each query process (each fused batch, its union) prefilled its "
              "files once into the pinned host cache, then ran 1 warm-up: every peacockdb run "
              "reads files resident in pinned host memory, DuckDB's its buffer manager over a "
              "warm page cache. The prefill's disk read is reported apart (the cache row above).",
              "", "### Provenance: skipped and wrong at sf40", ""]
    for data in gathered:
        lines += [f"- {data['dataset']} {query} — {note}" for query, note in data["notes"]]
    return lines


def render(testdata):
    """The generated region: provenance, then per dataset its table, hints and load split."""
    testdata = pathlib.Path(testdata)
    cases = device_cases.rows(testdata / "benchmark-results" / DEVICE / "cases.csv")
    gathered = [gather(testdata, dataset, cases) for dataset in DATASETS]
    lines = ["## Provenance", ""] + provenance(gathered)
    for data in gathered:
        title = f"{TITLES[data['dataset']]} sf{SF}"
        lines += ["", f"## {title}", "", f"### {title}: fused against unfused and DuckDB", ""]
        lines += main_table(data)
        lines += ["", f"### {title}: the two hints", ""] + hints_table(data)
        lines += ["", f"### {title}: load against decode", ""] + split_table(data)
    return "\n".join(lines) + "\n"


def splice(report, generated):
    """`report` with the text between its `BEGIN` and `END` lines replaced by `generated`."""
    for marker in (BEGIN, END):
        if report.count(marker) != 1:
            raise ValueError(f"the page holds the marker {marker!r} {report.count(marker)} times, "
                             "not once")
    start, stop = report.index(BEGIN), report.index(END)
    if stop < start:
        raise ValueError("the page's end marker lies above its begin marker")
    return report[:start] + BEGIN + "\n" + generated + report[stop:]


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--testdata", required=True, type=pathlib.Path)
    parser.add_argument("--report", required=True, type=pathlib.Path)
    parser.add_argument("--check", action="store_true", help="exit 1 when the page is stale")
    args = parser.parse_args(argv)
    try:
        old = args.report.read_text()
        new = splice(old, render(args.testdata))
    except (Refused, ValueError) as why:
        print(f"batching_report.py: {why}", file=sys.stderr)
        return 1
    if args.check:
        if new != old:
            print(f"batching_report.py: {args.report} is stale; rerun without --check",
                  file=sys.stderr)
            return 1
        return 0
    if new != old:
        args.report.write_text(new)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
