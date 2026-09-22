#!/usr/bin/env python3
"""What a timed region spends its time on, one level down, from an Nsight capture.

    PEACOCK_BENCHMARK_CAPTURE=trace, which scripts/create_nsys_profile.sh --trace sets:
    scripts/calibration/nsys_calls.py --capture testdata/calibration/capture.sqlite \
        --plans-dir testdata/goldens/tpch.sf1 --out testdata/calibration/calls.tsv

The record has one number per region and one input size for it. For a hash join that size
is the sum over both children, and a join's cost does not depend on its two sides the same
way, so that sum cannot explain it. This reads the split back out of a capture instead of
adding columns to the record: libcudf pushes an NVTX range around every public call it
makes, so the build side (`hash_join`), the probe (`inner_join`) and the materialisation
(`gather`) are already three separate spans inside our region.

What a row is. One per (case, recipe_seq, call, depth), aggregated over the executions in
the capture. `recipe_seq` and `recipe_kind` are the fb seq and kind the region's range names,
the record's columns of the same name — not the plan node's `node_seq`, which the range
does not carry. The region is the `p<k>` range our own domain pushes per output partition;
`depth` is how deep the call sits inside it among calls of the same domain, so depth-0 rows
partition the region and deeper rows break those down. Summing across depths double-counts.
The case is read off the harness's own range — seq numbering restarts with every plan, so
two cases keyed without it merge into one row set that is self-consistent and wrong.

Every region also gets an `(unattributed)` row at depth 0: the part of it inside no call of
the traced domain at all. Without it a reader cannot tell a region explained by its calls
from one where they cover a third of the span, and the second is the interesting case — it
means the cost is in our code, not in cuDF's.

Host and device are different columns. `host_us` is the NVTX range itself, the wall time
the calling thread spent inside that cuDF call. `device_us` sums the kernel, memcpy and
memset durations whose launching runtime call falls inside the range, joined through
CUPTI's correlationId. A call that submits asynchronously and returns has a small host span
and a large device one; a call that synchronizes has host >= device. And `device_us` is a
sum over device operations, not a union of their spans, so concurrent work on several
streams is counted once per operation.

What this cannot see: a capture is not a measurement of the unprofiled run — nsys
serializes some of what it traces, which is why a captured run publishes no tree. Device
work launched from a thread other than the one that pushed the region lands in the
`(off-thread)` tally printed at the end; for the single-threaded execute path that should
be the parquet reader's and nothing else.
"""

import argparse
import bisect
import collections
import sqlite3
import statistics
import pathlib
import re
import sys

import nvtx_names
import record

# Our own domain. The two levels it carries, and how their names are read, are
# `nvtx_names` — shared with `nsys_hbm.py`, which reads the same capture for a different
# number.
OWN_DOMAIN = "peacockdb"

DEVICE_TABLES = ("CUPTI_ACTIVITY_KIND_KERNEL",
                 "CUPTI_ACTIVITY_KIND_MEMCPY",
                 "CUPTI_ACTIVITY_KIND_MEMSET")

UNATTRIBUTED = "(unattributed)"

NOTES = [
    "What one timed region spends its time on, one level down, from an Nsight capture.",
    "One row per (case, recipe_seq, call, depth), recipe_seq and recipe_kind being the fb",
    "  seq and kind of records.tsv. A region is one output partition of one",
    "  call, so a batched mode has several a run: `regions` counts them, `executions`",
    "  divides them out, and every microsecond below is a median over one region.",
    "depth 0 partitions the region and deeper rows break those down — summing across",
    "  depths double-counts. (unattributed) is the part of the region inside no call.",
    "host_us is the call's own NVTX range; device_us sums the device operations it",
    "  launched, so an async call has a small host span and a large device one.",
    "The times are a profiled run's and are not comparable with records.tsv's.",
]


def domain_ids(conn):
    """name -> domainId for every NVTX domain the capture knows."""
    return {
        name: did
        for did, name in conn.execute(
            f"""select e.domainId, coalesce(e.text, s.value) from NVTX_EVENTS e
                left join StringIds s on s.id = e.textId
                where e.eventType = {nvtx_names.DOMAIN_CREATE}"""
        )
    }


def ranges(conn, domain_id):
    """(start, end, text, tid) for one domain, in start order."""
    return list(conn.execute(
        f"""select e.start, e.end, coalesce(e.text, s.value), e.globalTid
            from NVTX_EVENTS e left join StringIds s on s.id = e.textId
            where e.eventType = {nvtx_names.PUSHPOP_RANGE} and e.domainId = ?
              and e.end is not null
            order by e.start""",
        (domain_id,),
    ))


def recipe_seqs(plans_path, query):
    """The seqs a query's `--- recipes ---` section names, as {seq: kind}.

    The golden is the planner's own statement of what the C++ will be asked to run, so
    checking a capture against it answers a question the capture alone cannot: whether the
    regions in it are the ones this plan was supposed to produce. A capture of a different
    query, or of a plan that has since moved, looks perfectly self-consistent.
    """
    text = pathlib.Path(plans_path).read_text()
    marker = f"\n== {query}\n"
    if marker not in text:
        sys.exit(f"{plans_path} has no `== {query}` section")
    section = text.split(marker, 1)[1].split("\n== ", 1)[0]
    if "--- recipes ---" not in section:
        sys.exit(f"{plans_path}: `{query}` has no recipes section")
    recipes = section.split("--- recipes ---", 1)[1].split("--- memory ---", 1)[0]
    # `execute_node(#4 CudfAggregate{Merge}, prior output)` -> 4, CudfAggregate. The brace
    # payload is the recipe's own annotation and is not what a capture's range carries.
    found = {}
    for seq, kind in re.findall(r"execute_\w+\(#(\d+) (\w+)", recipes):
        found[int(seq)] = kind
    return found


def check_against_recipes(regs, plans_path):
    """Every seq the recipes name was driven, with the kind they name, and no others.

    Which query is read off the capture, not off the command line. A plans golden holds
    every query of one mode, so the capture's own cases select their sections — and a case
    from another mode is skipped rather than checked against a plan it never ran from.
    """
    # `tp1-single.plans.txt` -> `tp1-single`. The mode is in the filename because
    # that is how the goldens are laid out.
    mode = pathlib.Path(plans_path).name.split(".plans.txt")[0]
    by_case = collections.defaultdict(dict)
    for case, seq, _, kind, _, _, _, _ in regs:
        # The export and the slice are left out: they publish no step, and the seq they
        # carry is the producing node's, whose own kind the plan states. Kept in, the last
        # of the two written wins and every case reports the plan and the capture as
        # disagreeing about a node they agree on.
        if not nvtx_names.is_bare_call(kind):
            by_case[case][seq] = kind
    checked = sorted(c for c in by_case if c[3] == mode)
    if not checked:
        sys.exit(
            f"{plans_path} is mode {mode!r} and the capture holds "
            f"{sorted({c[3] for c in by_case})} -- nothing to check it against."
        )
    for case in checked:
        _check_one(by_case[case], plans_path, case[2])


def _check_one(seen, plans_path, query):
    declared = recipe_seqs(plans_path, query)

    missing = sorted(set(declared) - set(seen))
    extra = sorted(set(seen) - set(declared))
    wrong = sorted((seq, declared[seq], seen[seq]) for seq in set(declared) & set(seen)
                   if declared[seq] != seen[seq])
    if missing or extra or wrong:
        parts = []
        if missing:
            parts.append(f"declared but never driven: {missing}")
        if extra:
            parts.append(f"driven but not declared: {extra}")
        if wrong:
            parts.append("kind differs: " + ", ".join(
                f"#{s} is {d} in the plan and {c} in the capture" for s, d, c in wrong))
        sys.exit(f"capture does not match {query}'s recipes -- " + "; ".join(parts))
    print(f"recipes: {len(declared)} seqs declared for {query}, all driven with the "
          f"kinds the plan names")


def regions(own):
    """Our domain's ranges -> [(case, seq, call_index, kind, partition, start, end, tid)].

    The three levels are told apart by their names rather than by nesting depth -- that
    rule is `nvtx_names`. What is added here is the containment check, twice: a partition
    range outside every call range of its thread, or a call outside every case range,
    means the ranges did not come from the code this script thinks they did, and a level
    rule alone cannot see that.

    `case` is `(dataset, sf, query, mode)`, read off the harness's own range. It used to
    be a `--query` the caller typed, which is the shape of every quiet mistake: a capture
    of q19 analysed under the name q6 is self-consistent all the way down, and even its
    seq numbers line up, because seq numbering restarts with every plan.
    """
    cases = [(a, b, nvtx_names.case_of(t)) for a, b, t, _ in own if nvtx_names.is_case(t)]
    if not cases:
        sys.exit(
            "the capture has no case range. It predates the harness pushing one, so the "
            "query a region belonged to cannot be recovered from it -- retake it with a "
            "build that does."
        )
    calls = [r for r in own
             if not nvtx_names.is_partition(r[2]) and not nvtx_names.is_case(r[2])]
    parts = [r for r in own if nvtx_names.is_partition(r[2])]
    starts = [c[0] for c in calls]

    out = []
    for start, end, text, tid in parts:
        i = bisect.bisect_right(starts, start) - 1
        if i < 0 or calls[i][1] < end or calls[i][3] != tid:
            sys.exit(f"partition range at {start} is inside no call range of its thread")
        case = next((c for a, b, c in cases if a <= calls[i][0] < b), None)
        if case is None:
            sys.exit(
                f"call range {calls[i][2]!r} at {calls[i][0]} is inside no case range. "
                "Every call the harness makes is inside the case it belongs to."
            )
        seq, call, kind = nvtx_names.call_of(calls[i][2])
        out.append((case, seq, call, kind, nvtx_names.partition_of(text), start, end, tid))
    return out


class DeviceWork:
    """Device nanoseconds, addressable by the host interval that launched them.

    A kernel does not carry the NVTX range it belongs to; it carries a correlationId
    pointing back at the CUDA runtime call that launched it. So the join is: sum every
    device operation per correlationId, look up where on the host that launch happened,
    and index those launch points by thread. Asking "how much device work did this
    range cause" is then a range sum over the launch points inside it.

    Attributing at the launch site and not at the kernel's own timestamps is the only
    choice that stays true for asynchronous work: the kernel a call submits may still be
    running after the call returns, and placing it by its own start would credit it to
    whatever range happened to be open on the host at that moment -- a different node.

    A nested call's launches lie inside the outer call's interval too, so an outer range
    counts its children's device work. That is deliberate: it makes `device_us` mean the
    same thing as `host_us`, which is a span and likewise contains its children.
    """

    def __init__(self, conn):
        per_corr = collections.Counter()
        for table in DEVICE_TABLES:
            for corr, ns in conn.execute(
                    f"select correlationId, sum(end - start) from {table} group by 1"):
                per_corr[corr] += ns

        launches = collections.defaultdict(list)
        for start, tid, corr in conn.execute(
                "select start, globalTid, correlationId "
                "from CUPTI_ACTIVITY_KIND_RUNTIME"):
            ns = per_corr.get(corr)
            if ns:
                launches[tid].append((start, ns))

        self.total = sum(per_corr.values())
        self.by_tid = {}
        for tid, rows in launches.items():
            rows.sort()
            starts = [r[0] for r in rows]
            # Prefix sums, so a range sum is two bisects and a subtraction. There are
            # ~200k launches and a region asks about every call it contains; a linear
            # scan per question would be quadratic on the big scans.
            running, total = [0], 0
            for _, ns in rows:
                total += ns
                running.append(total)
            self.by_tid[tid] = (starts, running)

    def span(self, tid, start, end):
        """Device ns launched from `tid` in [start, end)."""
        starts, running = self.by_tid.get(tid, ((), (0,)))
        return (running[bisect.bisect_left(starts, end)]
                - running[bisect.bisect_left(starts, start)])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--capture", required=True, help="sqlite export of the .nsys-rep")
    ap.add_argument("--domain", action="append", default=[],
                    help="NVTX domain to break regions down by; repeatable, "
                         "default libcudf")
    ap.add_argument("--out", required=True)
    ap.add_argument("--plans-dir",
                    help="a goldens directory; each case's <mode>.plans.txt is found in "
                         "it, so a capture spanning several modes checks against all of "
                         "them without the caller listing which")
    ap.add_argument("--top", type=int, default=12,
                    help="how many calls per node to print in the summary")
    args = ap.parse_args()
    call_domains = args.domain or ["libcudf"]

    conn = sqlite3.connect(args.capture)
    doms = domain_ids(conn)
    if OWN_DOMAIN not in doms:
        sys.exit(f"capture has no NVTX domain {OWN_DOMAIN!r}: the run needs "
                 "PEACOCK_BENCHMARK_CAPTURE=trace, which turns the harness's ranges on")
    missing = [d for d in call_domains if d not in doms]
    if missing:
        sys.exit(f"capture has no NVTX domain(s) {missing}; it has {sorted(doms)}")

    regs = regions(ranges(conn, doms[OWN_DOMAIN]))
    if not regs:
        sys.exit("capture has no partition ranges -- nothing was executed under them")

    calls = []
    for name in call_domains:
        calls += [(s, e, f"{name}:{t}" if len(call_domains) > 1 else t, tid)
                  for s, e, t, tid in ranges(conn, doms[name])]
    calls.sort()

    # Per thread, sorted by start: a region asks only about the calls on its own thread,
    # and inside a thread the ranges are properly nested, which is what lets the walk
    # below track depth with a stack instead of testing containment pairwise.
    by_tid = collections.defaultdict(list)
    for c in calls:
        by_tid[c[3]].append(c)
    call_index = {tid: ([c[0] for c in cs], cs) for tid, cs in by_tid.items()}

    device = DeviceWork(conn)

    # One bucket per (region identity, call name, depth), holding a list over the
    # capture's executions. A list and not a running total: executions differ by more
    # than noise — 7% at the median, 36% at the worst — and a mean hides which of them
    # a number came from.
    per_exec = collections.defaultdict(lambda: collections.defaultdict(
        lambda: [0, 0, 0]))  # exec key -> (call, depth) -> [count, host_ns, device_ns]
    region_span = collections.defaultdict(list)
    seen = collections.Counter()
    by_call = collections.defaultdict(set)
    in_regions_ns = 0

    for case, seq, call, kind, part, r_start, r_end, tid in regs:
        ident = (case, seq, kind, part)
        # `run` counts occurrences across the capture; `call` is the index within one
        # execution, which is what a record row carries. A session opens per run, so the
        # C++ counter restarts each time and a seq driven once per run is call 0 ten
        # times over. Together they say how many executions the capture holds.
        run = seen[ident]
        seen[ident] += 1
        by_call[ident].add(call)
        region_span[ident].append(r_end - r_start)
        bucket = per_exec[(ident, run)]

        region_dev = device.span(tid, r_start, r_end)
        in_regions_ns += region_dev

        starts, rows = call_index.get(tid, ((), ()))
        lo = bisect.bisect_left(starts, r_start)
        stack = []
        host_covered = dev_covered = 0
        for start, end, text, _ in rows[lo:]:
            if start >= r_end:
                break
            if end > r_end:
                sys.exit(f"call {text!r} straddles the end of region {ident}")
            # The stack holds the end timestamps of the calls still open at `start`;
            # everything that ended earlier is popped, so its height is the depth.
            while stack and stack[-1] <= start:
                stack.pop()
            depth = len(stack)
            dev = device.span(tid, start, end)
            if depth == 0:
                # Only the top level counts towards coverage. A nested call's time is
                # already inside its parent's span, and adding it would make the
                # residual below negative.
                host_covered += end - start
                dev_covered += dev
            slot = bucket[(text, depth)]
            slot[0] += 1
            slot[1] += end - start
            slot[2] += dev
            stack.append(end)

        # The residual: the part of the region that is in no traced call. It exists so
        # that the depth-0 rows add up to the region and a reader can see at a glance
        # how much of a node its cuDF calls actually explain.
        rest = bucket[(UNATTRIBUTED, 0)]
        rest[0] += 1
        rest[1] += (r_end - r_start) - host_covered
        rest[2] += region_dev - dev_covered

    # How many executions the capture holds, per region. A batched mode drives one seq
    # once per batch, so a region's occurrences are its executions times its calls per
    # execution — and comparing raw occurrence counts reads that as a run that died.
    executions = {}
    for ident, times in seen.items():
        per_run = len(by_call[ident])
        if times % per_run:
            sys.exit(
                f"region {ident} ran {times} times with {per_run} call indices, which is "
                "not a whole number of executions — an execution stopped partway through "
                "the node, and every median below would be short by the part it missed."
            )
        executions[ident] = times // per_run

    runs_seen = set(executions.values())
    if args.plans_dir:
        # One goldens file per mode, found rather than listed: the capture says which modes
        # are in it, and a caller retyping that list is the mistake naming the query on the
        # command line was. A mode with no golden is a refusal — this is the only thing
        # that says the regions are the plan's, and a check covering less is not one.
        for mode in sorted({case[3] for case, *_ in regs}):
            path = pathlib.Path(args.plans_dir) / f"{mode}.plans.txt"
            if not path.exists():
                sys.exit(f"{path} does not exist, so the {mode} cases in this capture "
                         "would go unchecked against the plan they claim to be.")
            check_against_recipes(regs, str(path))

    calls_per_exec = {len(v) for v in by_call.values()}
    print(f"call indices per region: {sorted(calls_per_exec)} "
          f"(1 means every execution drove each seq once)")
    print(f"{len(regs)} region ranges, {len(seen)} distinct regions, "
          f"{sorted(runs_seen)} executions each")
    if len(runs_seen) != 1:
        sys.exit(f"the capture's regions disagree about how many times they ran: "
                 f"{sorted(runs_seen)}. Every region of a case runs once per execution, so "
                 "an execution died partway — and the medians below would report the "
                 "short-changed regions at a fraction of their cost with nothing saying so.")

    # Median over occurrences, per (region, call, depth). Median rather than the mean: the
    # warm-up execution is in here, and on a first touch of a column the parquet reader
    # does work no later execution repeats.
    rows = []
    keys = {(ident, cd) for (ident, _), b in per_exec.items() for cd in b}
    for ident, (call, depth) in sorted(keys, key=lambda k: (k[0], k[1][1], k[1][0])):
        runs = [per_exec[(ident, r)].get((call, depth), [0, 0, 0])
                for r in range(seen[ident])]
        (dataset, sf, query, mode), seq, kind, part = ident
        rows.append(dict(
            dataset=dataset, sf=sf, query=query, mode=mode,
            recipe_seq=seq, recipe_kind=kind, partition=part, call=call, depth=depth,
            executions=executions[ident],
            regions=seen[ident],
            calls_per_region=statistics.median(r[0] for r in runs),
            host_us=round(statistics.median(r[1] for r in runs) / 1000),
            device_us=round(statistics.median(r[2] for r in runs) / 1000),
            region_us=round(statistics.median(region_span[ident]) / 1000),
        ))

    cols = ["dataset", "sf", "query", "mode",
            "recipe_seq", "recipe_kind", "partition", "call", "depth", "executions",
            "regions", "calls_per_region", "host_us", "device_us", "region_us"]
    record.write_tsv(args.out, NOTES, cols, [[r[c] for c in cols] for r in rows])

    print(f"{(device.total - in_regions_ns) / 1e6:.1f} ms of {device.total / 1e6:.1f} ms "
          "of device work was launched outside every region "
          "(reader threads, allocator warm-up, teardown)")

    for ident in sorted(seen):
        case, seq, kind, part = ident
        top = [r for r in rows if r["depth"] == 0
               and (r["dataset"], r["sf"], r["query"], r["mode"]) == case
               and (r["recipe_seq"], r["recipe_kind"], r["partition"]) == (seq, kind, part)]
        top.sort(key=lambda r: -r["host_us"])
        span = top[0]["region_us"] if top else 0
        print(f"\n{case[2]} {case[3]}  #{seq} {kind} p{part}  region {span} us")
        for r in top[:args.top]:
            share = 100 * r["host_us"] / span if span else 0
            print(f"    {r['host_us']:>9} us  {share:5.1f}%  "
                  f"dev {r['device_us']:>8} us  x{r['calls_per_region']:<5g} {r['call']}")
    print(f"\nwrote {args.out}")


if __name__ == "__main__":
    main()
