#!/usr/bin/env python3
"""Per-query bytes and overheads from Spark event logs: `comet_bytes.py EVENTLOG_DIR > bytes.tsv`.

Columns: dataset query run scan_bytes shuffle_bytes spill_bytes s3_gets driver_ms scheduler_ms.

A query is a job group `dataset:query:run`, which comet_run.py sets for each query. Its stages are
its jobs' stages; its SQL executions are those its jobs name. Per query:
- scan_bytes: internal.metrics.input.bytesRead. Comet adds its native scans' bytes_scanned there
  (CometMetricNode.reportScanInputMetrics @ab46419f).
- shuffle_bytes: internal.metrics.shuffle.write.bytesWritten.
- spill_bytes: internal.metrics.diskBytesSpilled (bytes on disk, not the in-memory size).
- s3_gets: Comet's SQL metric "S3 GETs observed by the HTTP connector" (scan_io_http_observed_gets).
- driver_ms: the SQL executions' wall time less the time when one of the query's stages was open:
  planning, AQE re-planning and driver-side work.
- scheduler_ms: per stage, the time it was open less its tasks' span (first launch to last
  finish), summed: task scheduling and stage bookkeeping.
Each stage counts once, by its last completed attempt. Logs may be plain or compressed
(.zst/.zstd needs `zstandard`, .lz4 needs `lz4`).
"""
import json
import sys
from pathlib import Path

COLUMNS = ["dataset", "query", "run", "scan_bytes", "shuffle_bytes", "spill_bytes", "s3_gets",
           "driver_ms", "scheduler_ms"]
ACCUMS = {
    "internal.metrics.input.bytesRead": "scan_bytes",
    "internal.metrics.shuffle.write.bytesWritten": "shuffle_bytes",
    "internal.metrics.diskBytesSpilled": "spill_bytes",
    "S3 GETs observed by the HTTP connector": "s3_gets",
}
SQL_START = "org.apache.spark.sql.execution.ui.SparkListenerSQLExecutionStart"
SQL_END = "org.apache.spark.sql.execution.ui.SparkListenerSQLExecutionEnd"


def open_log(path: Path):
    name = path.name
    if name.endswith((".zst", ".zstd")):
        import io
        import zstandard
        return io.TextIOWrapper(zstandard.ZstdDecompressor().stream_reader(open(path, "rb")))
    if name.endswith(".lz4"):
        import lz4.frame
        return lz4.frame.open(path, "rt")
    return open(path)


def events(path: Path):
    with open_log(path) as f:
        for line in f:
            line = line.strip()
            if line:
                yield json.loads(line)


def union_ms(intervals):
    total, end = 0, None
    for a, b in sorted(intervals):
        if end is None or a > end:
            total += b - a
            end = b
        elif b > end:
            total += b - end
            end = b
    return total


def summarize(paths):
    """One dict per (dataset, query, run), in the order the queries first appear."""
    out = []
    for path in paths:
        group_of_stage, groups, execs = {}, {}, {}
        stages, task_span = {}, {}
        for e in events(path):
            kind = e.get("Event")
            if kind == "SparkListenerJobStart":
                props = e.get("Properties") or {}
                g = props.get("spark.jobGroup.id", "")
                if g.count(":") != 2:
                    continue
                groups.setdefault(g, set())
                for s in e.get("Stage IDs", []):
                    group_of_stage[s] = g
                ex = props.get("spark.sql.execution.id")
                if ex is not None:
                    groups[g].add(int(ex))
            elif kind == "SparkListenerTaskEnd":
                key = (e["Stage ID"], e.get("Stage Attempt ID", 0))
                info = e["Task Info"]
                lo, hi = task_span.get(key, (info["Launch Time"], info["Finish Time"]))
                task_span[key] = (min(lo, info["Launch Time"]), max(hi, info["Finish Time"]))
            elif kind == "SparkListenerStageCompleted":
                si = e["Stage Info"]
                if "Submission Time" in si and "Completion Time" in si:
                    stages[si["Stage ID"]] = si  # a later attempt replaces an earlier one
            elif kind == SQL_START:
                execs.setdefault(e["executionId"], [None, None])[0] = e["time"]
            elif kind == SQL_END:
                execs.setdefault(e["executionId"], [None, None])[1] = e["time"]
        for g in groups:
            dataset, query, run = g.split(":")
            row = {"dataset": dataset, "query": query, "run": run}
            row.update({c: 0 for c in COLUMNS[3:]})
            open_intervals = []
            for sid, si in stages.items():
                if group_of_stage.get(sid) != g:
                    continue
                for acc in si.get("Accumulables", []):
                    col = ACCUMS.get(acc.get("Name"))
                    if col:
                        row[col] += int(acc.get("Value") or 0)
                a, b = si["Submission Time"], si["Completion Time"]
                open_intervals.append((a, b))
                lo, hi = task_span.get((sid, si.get("Stage Attempt ID", 0)), (a, a))
                row["scheduler_ms"] += max(0, (b - a) - (hi - lo))
            wall = sum(t1 - t0 for x, (t0, t1) in execs.items()
                       if x in groups[g] and t0 is not None and t1 is not None)
            row["driver_ms"] = max(0, wall - union_ms(open_intervals))
            out.append(row)
    return out


def main(argv):
    if len(argv) != 2:
        sys.exit("usage: comet_bytes.py EVENTLOG_DIR > bytes.tsv")
    d = Path(argv[1])
    paths = sorted(p for p in d.rglob("*") if p.is_file() and not p.name.endswith(".inprogress"))
    print("\t".join(COLUMNS))
    for row in summarize(paths):
        print("\t".join(str(row[c]) for c in COLUMNS))


if __name__ == "__main__":
    main(sys.argv)
