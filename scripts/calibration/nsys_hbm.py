#!/usr/bin/env python3
"""hbm_bytes per timed cuDF call, from an Nsight Systems capture.

The calibration record deliberately has no hbm_bytes column: nothing inside the process
can count HBM traffic, so it comes from a profiled run and joins back on the record's own
coordinates. This is that join's producer.

Why it is a separate run. The capture distorts what it measures -- +7% on a query and
+11% on a heavy scan, measured -- so the times and the traffic cannot come from one file.
They come from two runs joined on the tuple, which is the whole reason the record carries
a tuple rather than a node number.

    scripts/create_nsys_profile.sh --metrics, which runs these per case, then the merge:
    nsys profile --trace=nvtx,cuda --sample=none --cpuctxsw=none \
                 --gpu-metrics-device=0 --gpu-metrics-set=<arch> \
                 --gpu-metrics-frequency=20000 -o cap -- <binary> <case's test> --exact
    nsys export --type=sqlite --force-overwrite=true -o cap.sqlite cap.nsys-rep
    scripts/calibration/nsys_hbm.py --capture cap.sqlite --case "tpch.sf40 q6 tp1-single" \
        --peak-bw <B/s> --record testdata/calibration/tpch.sf40/records.tsv \
        --record testdata/calibration/tpcds.sf40/records.tsv --out part.tsv
    scripts/calibration/nsys_hbm.py --parts part.tsv --parts … --record … --out hbm.tsv

The record is the clean run's, as published — a file per dataset, all of them given: its
coordinates are what the output carries, and the capture supplies only bytes. The two must
describe the same calls, which is checked both ways — a call in one and not the other means
they are not the same work. A join without --case takes every case the capture holds; with
it, the capture must hold that case alone and is checked against that case's rows, and the
merge of the parts is where the record as a whole is checked. The harness wraps each case in
a named NVTX range, so which query a call was in is read rather than typed.

The output is a row per call (`record.HBM_COLUMNS`): each reading the median over the call's
executions. Joined per execution first, against the record row with the same run_index,
then folded — so the check above holds execution by execution, and the file is the
record's calls long rather than its executions.

Nsight's general metric set reports DRAM traffic as a percentage of the device's peak
bandwidth, not as bytes, so bytes are the integral of that percentage over the sampling
period times a peak the caller supplies. That makes --peak-bw part of the measurement
rather than a display detail, and a wrong one is a silent scale error on every row.
Checked once against a region whose traffic is known exactly -- a cast of N rows from an
8-byte to a 16-byte type has to read 8N and write 16N -- which came out inside 2.4% on
both directions at the default peak.

Metric ids are looked up by name because they are a property of the capture, not of the
tool: a run whose ids were assumed rather than read produced a read column holding what
was really the write, and the error was only visible because a cast's traffic is known
in advance. Nothing downstream of here could have caught it.

The remaining checks are invariants a healthy capture cannot violate -- no sample above
100% of peak, no gap in the sampling cadence -- and they are asserted rather than trusted
because both failures degrade quietly: an interrupted collection leaves later regions
with an hbm_bytes of zero, which is what a region that moved no data also looks like.
"""

import argparse
import bisect
import sqlite3
import statistics
import sys

import nvtx_names
import record

# nsys metric ids within the general set. Names are checked against the capture, since a
# different --gpu-metrics-set numbers them differently. Named without a unit: nsys 2023.2
# wrote the name alone, 2024.5 appends " [Throughput %]", and metric_id takes either.
DRAM_READ = "DRAM Read Bandwidth"
DRAM_WRITE = "DRAM Write Bandwidth"

# The record's coordinates, in the order the output writes them. Every one of them comes
# from the record: the capture names a call `"<seq>.<call_index> <Kind>"` and nothing
# more, so which query and which plan node that was is knowable only from the row it
# pairs with. Listed once and used for the required-column check, the output header and
# the row copy, so the three cannot fall out of step.
TUPLE = (
    "dataset", "sf", "query", "mode",
    "node_seq", "node_type", "lane",
    "recipe_seq", "recipe_kind", "call_index", "run_index",
)


def metric_id(conn, name):
    rows = conn.execute(
        "select metricId from TARGET_INFO_GPU_METRICS"
        " where metricName = ? or metricName like ?", (name, name + " [%]")
    ).fetchall()
    if len(rows) > 1:
        sys.exit(f"capture names metric {name!r} {len(rows)} times; the lookup is ambiguous")
    row = rows[0] if rows else None
    if row is None:
        have = [r[0] for r in conn.execute("select metricName from TARGET_INFO_GPU_METRICS")]
        sys.exit(f"capture has no metric {name!r}; it has {have}")
    return row[0]


def samples(conn, mid):
    return list(
        conn.execute(
            "select timestamp, value from GPU_METRICS where metricId = ? order by timestamp",
            (mid,),
        )
    )


def busy_union(conn):
    """Disjoint device-busy intervals, merged across all streams.

    Merged rather than summed: the capture has 32 streams and the parquet reader keeps
    several of them busy at once, so summing durations would count concurrent work twice
    and report a region as busier than it was long.
    """
    spans = []
    for table in (
        "CUPTI_ACTIVITY_KIND_KERNEL",
        "CUPTI_ACTIVITY_KIND_MEMCPY",
        "CUPTI_ACTIVITY_KIND_MEMSET",
    ):
        spans += list(conn.execute(f"select start, end from {table}"))
    spans.sort()
    merged = []
    for start, end in spans:
        if merged and start <= merged[-1][1]:
            merged[-1][1] = max(merged[-1][1], end)
        else:
            merged.append([start, end])
    return merged


def cases_in(ranges):
    """The capture's case ranges, in time order: `(start, end, (dataset, sf, query, mode))`.

    A capture with none is refused rather than read as one case. It came from a harness
    from before the case range existed, and reading it as "whatever the record says" is
    exactly the guess this level was added to remove — a q19 scan under q6's coordinates
    is correct bytes under the wrong name, and nothing downstream can tell.
    """
    found = [
        (a, b, nvtx_names.case_of(t)) for a, b, t in ranges if nvtx_names.is_case(t)
    ]
    if not found:
        sys.exit(
            "the capture has no case range. It predates the harness pushing one, so the "
            "query a call belonged to cannot be recovered from it -- retake it with a "
            "build that does. (Before that range existed the reader had to be told the "
            "query, which is why it is now read rather than passed.)"
        )
    return found


def case_at(cases, start):
    """Which case range contains this call, by time.

    Containment rather than order: cases run one after another, but a reader that assumed
    so would attribute every call of a crashed case to the next one.
    """
    for a, b, case in cases:
        if a <= start < b:
            return case
    return None


def key_calls(calls):
    """Capture calls keyed as the record keys them: the case, then the call, then the run.

    `run_index` is not in the capture and is derived here, by the rule the record's own
    heading states: a repeat of a key is the next execution. Time order is what makes it
    derivable, and it is the only thing about the capture's order this reads.

    Deliberately not a pairing by position. The record is written in plan order — the
    driver walks pre-order, so its first row is the root's — and the capture is in
    execution order, where the scan comes first. The two are both complete and differently
    sorted, and a positional pairing silently reads one as the other. The old
    `(query, label, node_seq)` join got away with it because the legacy record happened to
    be in execution order.
    """
    keyed, made = {}, {}
    for a, b, case, (seq, call_index) in calls:
        within = (case, seq, call_index)
        at = made.get(within, 0)
        made[within] = at + 1
        keyed[case + (seq, call_index, at)] = (a, b)
    return keyed


def row_key(row):
    """A record row's key, in the order `key_calls` builds the capture's."""
    return (
        row["dataset"], row["sf"], row["query"], row["mode"],
        int(row["recipe_seq"]), int(row["call_index"]), int(row["run_index"]),
    )


def report_loss(keyed, rows, record_path):
    """Refuse a capture and a record that do not describe the same calls, saying which.

    Both directions and named rather than counted: a mismatch means the two are not one
    run, and "77 against 70" leaves the reader guessing which end is wrong.

    The failure this is really here for is the warm-up. It used to run with ranges on and
    is never written to the record, so the capture held one execution more than the file —
    every key present, an extra `run_index` on each. The harness now turns ranges on after
    the warm-up, and this is what says so if that stops being true.
    """
    want = {row_key(r) for r in rows}
    have = set(keyed)
    if have == want:
        return

    def runs(keys):
        return 1 + max((k[-1] for k in keys), default=-1)

    lines = ["the capture and the record do not describe the same calls."]
    extra, missing = sorted(have - want), sorted(want - have)
    if extra:
        lines.append(
            f"  {len(extra)} captured calls have no row, first {extra[0]}."
        )
    if missing:
        lines.append(
            f"  {len(missing)} rows have no captured call, first {missing[0]}."
        )
    lines.append(f"  {runs(have)} executions captured, {runs(want)} in {record_path}.")
    if extra and not missing and runs(have) == runs(want) + 1:
        lines.append(
            "  Exactly one execution more, and every key otherwise matched: that is the "
            "warm-up, which the record does not hold. Ranges must be turned on after it."
        )
    sys.exit("\n".join(lines))


NOTES = [
    "hbm_bytes per cuDF call, from an Nsight capture with GPU memory counters on.",
    "A row per call: the clean record's coordinates without run_index, and each reading",
    "  the median over `executions` executions of it. This file's times are not usable —",
    "  a capture costs the query ~7%, a heavy scan ~11% — so it carries traffic, and the",
    "  times come from the clean run.",
    "device_busy_us/device_idle_us are here as a reading of this capture, not as a",
    "  measurement to fit: idle inside a call is the device waiting through the host",
    "  prologue, and it says whether a thin sample count is a short call or a stalled one.",
    "samples = GPU metric samples inside the call. Under ~10 the integral is an",
    "  estimate from too few points; a reader should weight or drop those rows.",
]


def call_of_row(row):
    """A call's key in either file, the record's or this one's: its coordinates, no run."""
    return tuple(row[c] for c in record.HBM_CALL)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--capture", help="sqlite export of the .nsys-rep")
    ap.add_argument("--case", help="`<dataset>.sf<sf> <query> <mode>`: the one case the "
                                   "capture holds, joined against that case's rows alone")
    ap.add_argument("--parts", action="append", default=[],
                    help="per-case outputs to merge, in place of --capture; the merge is "
                         "checked against the whole record")
    ap.add_argument("--record", action="append", required=True,
                    help="record TSV of the clean run; repeat for each file it was published "
                         "as, one per dataset")
    ap.add_argument("--out", required=True)
    ap.add_argument("--domain", default="peacockdb", help="NVTX domain holding the regions")
    ap.add_argument(
        "--peak-bw",
        type=float,
        help="device peak HBM bandwidth in bytes/s (H200 SXM: 4.8e12). Required with "
             "--capture, not defaulted: bytes are the integral of a percentage of it, so a "
             "wrong one is a silent scale error on every row",
    )
    args = ap.parse_args()
    if bool(args.capture) == bool(args.parts):
        sys.exit("one of --capture (a join) or --parts (a merge of joins), not both or neither")
    if args.parts and (args.case or args.peak_bw is not None):
        sys.exit("--case and --peak-bw belong to a join; --parts merges finished ones")
    if args.capture and args.peak_bw is None:
        sys.exit("--peak-bw is required with --capture: bytes are the integral of a "
                 "percentage of it, so a guessed one scales every row silently")
    _, rows = record.read_records(args.record, *TUPLE)
    if args.parts:
        merge(args, rows)
    else:
        join(args, rows)


def merge(args, rows):
    """The per-case joins as one file, in the record's call order, covering it exactly.

    A case joined alone was checked against that case's rows only; this is where the record
    as a whole is, both ways and naming the call, so a case whose capture never ran or a
    part of another case set is refused rather than written as part of the corpus.
    """
    parts = {}
    for path in args.parts:
        for row in record.read_tsv(path, *record.HBM_COLUMNS):
            if call_of_row(row) in parts:
                sys.exit(f"{path} repeats call {call_of_row(row)}; the parts overlap")
            parts[call_of_row(row)] = row
    want = list(dict.fromkeys(call_of_row(r) for r in rows))
    missing = [k for k in want if k not in parts]
    extra = sorted(set(parts) - set(want))
    if missing or extra:
        sys.exit(
            "the parts and the record do not describe the same calls: "
            + (f"{len(missing)} record calls have no part row, first {missing[0]}. "
               if missing else "")
            + (f"{len(extra)} part rows are in no record, first {extra[0]}." if extra else "")
        )
    out = [parts[k] for k in want]
    record.write_tsv(args.out, NOTES, record.HBM_COLUMNS,
                     [[r[c] for c in record.HBM_COLUMNS] for r in out])
    print(f"{len(out)} calls from {len(args.parts)} parts; wrote {args.out}")


def join(args, rows):
    conn = sqlite3.connect(args.capture)
    dom = nvtx_names.domain_id(conn, args.domain)
    ranges = list(
        conn.execute(
            f"""select e.start, e.end, coalesce(e.text, s.value) from NVTX_EVENTS e
                left join StringIds s on s.id = e.textId
                where e.eventType = {nvtx_names.PUSHPOP_RANGE} and e.domainId = ?
                order by e.start""",
            (dom,),
        )
    )

    read = samples(conn, metric_id(conn, DRAM_READ))
    write = samples(conn, metric_id(conn, DRAM_WRITE))
    if not read:
        sys.exit("capture has no GPU metric samples -- was --gpu-metrics-device given?")
    stamps = [t for t, _ in read]
    deltas = [b - a for a, b in zip(stamps, stamps[1:])]
    period_ns = statistics.median(deltas)
    gaps = sum(1 for d in deltas if d > 2 * period_ns)
    if gaps:
        sys.exit(
            f"{gaps} gaps in GPU metric sampling at a median period of {period_ns / 1e3:.1f}us: "
            "collection stopped or stalled, so later regions would read as zero-traffic. "
            "Lower --gpu-metrics-frequency and recapture."
        )
    over = max(r[1] + w[1] for r, w in zip(read, write))
    if over > 100:
        sys.exit(
            f"a sample reports {over}% of peak DRAM bandwidth, which the device cannot do. "
            "The sampling frequency is above what it sustains; lower it and recapture."
        )

    cases = cases_in(ranges)
    if args.case:
        # Named rather than inferred, so a capture of the wrong case is refused instead of
        # being joined against whichever rows it happens to match.
        named = nvtx_names.case_of(args.case)
        held = sorted({case for _, _, case in cases})
        if held != [named]:
            sys.exit(f"--case is {args.case!r} and the capture holds {held}")
        rows = [r for r in rows if tuple(r[c] for c in TUPLE[:4]) == named]
        if not rows:
            sys.exit(f"no record given has a row for {args.case!r}")
    # Only the call ranges, each tagged with its containing case. The `p<k>` ranges nest
    # inside them, so integrating both would count the same bytes twice — and a partition
    # cannot be priced alone anyway, its shared prologue being charged to p0. Dropped
    # here, so every count and message below is about calls.
    skipped = sum(1 for _, _, t in ranges if nvtx_names.is_partition(t))
    calls = []
    for a, b, text in ranges:
        if nvtx_names.is_partition(text) or nvtx_names.is_case(text):
            continue
        case = case_at(cases, a)
        if case is None:
            sys.exit(
                f"call range {text!r} at {a} is inside no case range. Every call the "
                "harness makes is inside the case it belongs to, so one outside means "
                "the ranges did not come from the code this script thinks they did."
            )
        calls.append((a, b, case, nvtx_names.call_of(text)[:2]))
    if not calls:
        sys.exit(
            f"domain {args.domain!r} has {len(ranges)} ranges and none is named "
            '"<seq>.<call_index> <kind>" -- the capture predates the node-range '
            "recorder, or the ranges came from somewhere else"
        )
    keyed = key_calls(calls)
    report_loss(keyed, rows, ", ".join(args.record))

    merged = busy_union(conn)
    starts = [m[0] for m in merged]

    def integral(series, a, b):
        lo, hi = bisect.bisect_left(stamps, a), bisect.bisect_right(stamps, b)
        pct = sum(v for _, v in series[lo:hi])
        return pct / 100.0 * args.peak_bw * period_ns / 1e9, hi - lo

    def busy_ns(a, b):
        total = 0
        i = max(0, bisect.bisect_right(starts, a) - 1)
        while i < len(merged) and merged[i][0] < b:
            total += max(0, min(merged[i][1], b) - max(merged[i][0], a))
            i += 1
        return total

    # Joined per execution, against the record row with the same run_index — `report_loss`
    # has established the key sets are equal, so the lookup cannot miss — and only then
    # folded to a row per call, in the record's order.
    executions = []
    for row in rows:
        a, b = keyed[row_key(row)]
        r, n = integral(read, a, b)
        w, _ = integral(write, a, b)
        busy = busy_ns(a, b)
        executions.append({
            **{c: row[c] for c in record.HBM_CALL},
            "run_index": row["run_index"],
            "hbm_read_bytes": str(round(r)), "hbm_write_bytes": str(round(w)),
            "hbm_bytes": str(round(r + w)), "samples": str(n),
            "device_busy_us": str(busy // 1000), "device_idle_us": str((b - a - busy) // 1000),
        })
    out = record.hbm_per_call(executions)
    record.write_tsv(args.out, NOTES, record.HBM_COLUMNS,
                     [[r[c] for c in record.HBM_COLUMNS] for r in out])

    total = sum(int(r["hbm_bytes"]) for r in out)
    thin = [r for r in out if int(r["samples"]) < 10]
    print(f"{len(out)} calls over {len(executions)} executions, {total / 1e9:.2f} GB HBM "
          f"a call's median summed, sampling period {period_ns / 1e3:.1f}us")
    if skipped:
        print(f"{skipped} partition ranges skipped — their bytes are inside their call's")
    if thin and total:
        print(
            f"{len(thin)} calls under 10 samples, carrying "
            f"{sum(int(r['hbm_bytes']) for r in thin) / 1e9:.3f} GB "
            f"({100 * sum(int(r['hbm_bytes']) for r in thin) / total:.2f}%) "
            "-- too few to integrate, and a reader should weight or drop them"
        )
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
