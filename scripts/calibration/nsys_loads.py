#!/usr/bin/env python3
"""Where a parquet load's time goes, from an Nsight trace capture: read, copy, decode.

    scripts/calibration/nsys_loads.py --capture testdata/calibration/capture.sqlite \
        --out testdata/calibration/loads.tsv

`nsys_calls.py` gives each cuDF call one `device_us`, a sum of kernels and copies, so a
load's copy cannot be told from its decode there. This splits each load node: host file
read, H2D copies by memory kind, decompression and decode kernels, the rest. A device
operation belongs to a load when its launching API call starts inside one of the load's
partition windows, on any thread: the reader launches from threads of its own, which
`nsys_calls.py` leaves as `(off-thread)`. A launch where two loads are open is neither's.
The columns are in `llm-wiki/build-test.md`, "Benchmark data flow".
"""

import argparse
import bisect
import collections
import re
import sqlite3
import statistics
import sys

import nsys_calls
import record

# A load is every partition region of a call of this kind: `GpuLoadParquet`'s recipe.
LOAD_KIND = "CudfScan"

# CUPTI's CUpti_ActivityMemcpyKind, which the export keeps. A copy's memory kind is not
# CUPTI's numbering: the export renumbers it, so `memory_kinds` reads it off the export.
MEMCPY_HTOD = 1
MEM_KIND_ENUM = "ENUM_CUDA_MEM_KIND"
MEM_KIND_PAGEABLE = "CUDA_MEMOPR_MEMORY_KIND_PAGEABLE"
MEM_KIND_PINNED = "CUDA_MEMOPR_MEMORY_KIND_PINNED"

# Tried in this order on a kernel's demangled name; a kernel matching neither is `other`.
DECOMPRESS_RE = re.compile(r"unsnap|snappy|inflate|zstd|lz4|brotli|nvcomp|decompress", re.I)
DECODE_RE = re.compile(
    r"cudf::io::parquet|gpuDecode|decode_page|DecodePage|ComputePage|page_header")
PINNED_ALLOC_RE = re.compile(r"^(cudaHostAlloc|cudaMallocHost|cudaHostRegister"
                             r"|cuMemHostAlloc|cuMemAllocHost|cuMemHostRegister)")
OSRT_READ_RE = re.compile(r"^(pread\w*|read)$")
KVIKIO_DOMAIN = "libkvikio"

API_TABLES = ("CUPTI_ACTIVITY_KIND_RUNTIME", "CUPTI_ACTIVITY_KIND_DRIVER")
KERNEL_TABLE = "CUPTI_ACTIVITY_KIND_KERNEL"

COLUMNS = ["dataset", "sf", "query", "mode", "recipe_seq", "executions", "calls", "load_us",
           "host_read_source", "host_read_us", "h2d_count", "h2d_bytes", "h2d_us", "h2d_gbps",
           "h2d_pinned_bytes", "h2d_pinned_gbps", "h2d_pageable_bytes", "h2d_pageable_gbps",
           "h2d_max_bytes", "decompress_us", "decode_us", "other_device_us",
           "off_thread_device_us", "pinned_alloc_count", "pinned_alloc_us", "unattributed_us"]
RATES = {"h2d_gbps", "h2d_pinned_gbps", "h2d_pageable_gbps"}

NOTES = [
    "Where one parquet load's time goes, from an Nsight trace capture.",
    "One row per (case, recipe_seq) load node: every partition region of a CudfScan call.",
    "  An execution's load is all its calls (one per batch); each number is computed per",
    "  execution, then the median over executions, column by column.",
    "A device operation is the load's when its launching API call starts in one of the",
    "  load's windows, on any thread; one launched where two loads are open is neither's.",
    "h2d_gbps is bytes over summed copy time, the per-copy rate, 0 with no copies.",
    "unattributed_us is load_us less the union of host reads, device operations and",
    "  pinned allocations inside the windows.",
    "The times are a profiled run's and are not comparable with records.tsv's.",
]

Op = collections.namedtuple("Op", "launch tid start end kind bytes copy_kind src_kind name")


def columns_of(conn, table):
    """The table's column names; empty when the export has no such table."""
    return {row[1] for row in conn.execute(f"pragma table_info({table})")}


def api_calls(conn):
    """(start, end, tid, correlationId, name) of every runtime and driver API call."""
    out = []
    for table in API_TABLES:
        if columns_of(conn, table):
            out += conn.execute(
                f"""select a.start, a.end, a.globalTid, a.correlationId, s.value
                    from {table} a left join StringIds s on s.id = a.nameId""").fetchall()
    return out


def memory_kinds(conn):
    """`(pageable id, pinned id)` as this export numbers `srcKind`; exits without the table."""
    if not columns_of(conn, MEM_KIND_ENUM):
        sys.exit(f"capture has no {MEM_KIND_ENUM} table, so a copy's srcKind cannot be read "
                 "as pinned or pageable. Export it with an nsys that writes its enums.")
    ids = dict(conn.execute(f"select name, id from {MEM_KIND_ENUM}").fetchall())
    return ids.get(MEM_KIND_PAGEABLE), ids.get(MEM_KIND_PINNED)


def device_ops(conn, calls):
    """Every kernel, memcpy and memset, placed on the host where it was launched."""
    launched = {corr: (start, tid) for start, _, tid, corr, _ in calls}
    queries = [
        ("kernel", KERNEL_TABLE, f"""select k.start, k.end, k.correlationId, 0, 0, 0, s.value
            from {KERNEL_TABLE} k left join StringIds s on s.id = k.demangledName"""),
        ("memcpy", "CUPTI_ACTIVITY_KIND_MEMCPY", """select start, end, correlationId, bytes,
            copyKind, srcKind, null from CUPTI_ACTIVITY_KIND_MEMCPY"""),
        ("memset", "CUPTI_ACTIVITY_KIND_MEMSET", """select start, end, correlationId, 0, 0, 0,
            null from CUPTI_ACTIVITY_KIND_MEMSET"""),
    ]
    ops = []
    for kind, table, query in queries:
        if not columns_of(conn, table):
            continue
        for start, end, corr, nbytes, copy_kind, src_kind, name in conn.execute(query):
            if corr in launched:
                ops.append(Op(*launched[corr], start, end, kind, nbytes, copy_kind, src_kind,
                              name))
    return ops


def host_reads(conn, doms):
    """(source, [(start, end)]): OS read calls if traced, else kvikio's ranges, else none.

    kvikio's ranges contain its bounce copies into pinned memory, so with that source the
    read and the H2D columns overlap.
    """
    if columns_of(conn, "OSRT_API") and conn.execute("select 1 from OSRT_API").fetchone():
        rows = conn.execute("""select o.start, o.end, s.value from OSRT_API o
                               left join StringIds s on s.id = o.nameId""")
        return "osrt", [(a, b) for a, b, name in rows if OSRT_READ_RE.match(name or "")]
    if KVIKIO_DOMAIN in doms:
        return "kvikio", [(a, b) for a, b, _, _ in nsys_calls.ranges(conn, doms[KVIKIO_DOMAIN])]
    return "none", []


def classify(name):
    if name and DECOMPRESS_RE.search(name):
        return "decompress"
    if name and DECODE_RE.search(name):
        return "decode"
    return "other"


def union_ns(intervals):
    total, reach = 0, None
    for start, end in sorted(intervals):
        if reach is None or start > reach:
            total += end - start
            reach = end
        elif end > reach:
            total += end - reach
            reach = end
    return total


def merged(windows):
    """The windows' union as disjoint sorted (start, end) pairs."""
    out = []
    for start, end, *_ in sorted(windows):
        if out and start <= out[-1][1]:
            out[-1] = (out[-1][0], max(out[-1][1], end))
        else:
            out.append((start, end))
    return out


class Spans:
    """Host intervals, clipped to a set of windows by bisect rather than a scan."""

    def __init__(self, spans):
        self.spans = sorted(spans)
        self.starts = [s for s, _ in self.spans]
        self.longest = max((e - s for s, e in self.spans), default=0)

    def within(self, windows):
        """The parts of these intervals inside `windows`, which are disjoint."""
        out = []
        for lo, hi in windows:
            first = bisect.bisect_left(self.starts, lo - self.longest)
            for start, end in self.spans[first:bisect.bisect_left(self.starts, hi)]:
                if end > lo:
                    out.append((max(start, lo), min(end, hi)))
        return out


def load_windows(regs):
    """{(case, seq): {execution: [(start, end, tid)]}}, and {(case, seq): call indices}.

    An execution drives a load once per call index, so occurrences of one partition region
    divided by the call indices are executions, as `nsys_calls.main` counts them.
    """
    loads = [r for r in regs if r[3] == LOAD_KIND]
    calls = collections.defaultdict(set)
    for case, seq, call, *_ in loads:
        calls[(case, seq)].add(call)
    seen = collections.Counter()
    windows = collections.defaultdict(lambda: collections.defaultdict(list))
    for case, seq, _, _, part, start, end, tid in loads:
        run = seen[(case, seq, part)]
        seen[(case, seq, part)] += 1
        windows[(case, seq)][run // len(calls[(case, seq)])].append((start, end, tid))
    for (case, seq, part), times in seen.items():
        if times % len(calls[(case, seq)]):
            sys.exit(f"load #{seq} p{part} of {case} ran {times} times with "
                     f"{len(calls[(case, seq)])} call indices: an execution stopped partway.")
    return windows, calls


class Owners:
    """Which loads' windows are open at a host instant, and on which threads."""

    def __init__(self, windows):
        self.points = sorted({p for per_exec in windows.values() for ws in per_exec.values()
                              for s, e, _ in ws for p in (s, e)})
        self.open = [{} for _ in self.points]
        for key, per_exec in windows.items():
            for execution, ws in per_exec.items():
                for start, end, tid in ws:
                    for i in range(bisect.bisect_left(self.points, start),
                                   bisect.bisect_left(self.points, end)):
                        self.open[i].setdefault((key, execution), set()).add(tid)

    def at(self, instant):
        i = bisect.bisect_right(self.points, instant) - 1
        return self.open[i] if i >= 0 else {}


def rate(nbytes, ns):
    """Bytes per nanosecond is GB/s."""
    return nbytes / ns if ns else 0.0


class Attributed:
    """What one execution of one load launched or called, from any thread."""

    def __init__(self):
        self.ops, self.allocs, self.off_thread_ns = [], [], 0


def execution_values(found, windows, reads, kinds):
    """One execution of one load, in the columns' units, before the median."""
    ops, allocs = found.ops, found.allocs
    clip = merged(windows)
    read_spans = reads.within(clip)
    htod, by_class = [], collections.Counter()
    for op in ops:
        if op.kind == "memcpy" and op.copy_kind == MEMCPY_HTOD:
            htod.append(op)
        else:
            by_class[classify(op.name) if op.kind == "kernel" else "other"] += op.end - op.start
    pageable_id, pinned_id = kinds
    pinned = [op for op in htod if op.src_kind == pinned_id]
    pageable = [op for op in htod if op.src_kind == pageable_id]
    load_ns = sum(end - start for start, end, _ in windows)
    covered = union_ns(read_spans + Spans([(op.start, op.end) for op in ops]).within(clip)
                       + Spans(allocs).within(clip))

    def copies(chosen):
        return sum(op.bytes for op in chosen), sum(op.end - op.start for op in chosen)

    h2d_bytes, h2d_ns = copies(htod)
    pinned_bytes, pinned_ns = copies(pinned)
    pageable_bytes, pageable_ns = copies(pageable)
    return dict(
        load_us=load_ns / 1000, host_read_us=union_ns(read_spans) / 1000,
        h2d_count=len(htod), h2d_bytes=h2d_bytes, h2d_us=h2d_ns / 1000,
        h2d_gbps=rate(h2d_bytes, h2d_ns),
        h2d_pinned_bytes=pinned_bytes, h2d_pinned_gbps=rate(pinned_bytes, pinned_ns),
        h2d_pageable_bytes=pageable_bytes, h2d_pageable_gbps=rate(pageable_bytes, pageable_ns),
        h2d_max_bytes=max((op.bytes for op in htod), default=0),
        decompress_us=by_class["decompress"] / 1000, decode_us=by_class["decode"] / 1000,
        other_device_us=by_class["other"] / 1000,
        off_thread_device_us=found.off_thread_ns / 1000,
        pinned_alloc_count=len(allocs), pinned_alloc_us=sum(b - a for a, b in allocs) / 1000,
        unattributed_us=(load_ns - covered) / 1000,
    )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--capture", required=True, help="sqlite export of the .nsys-rep")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    conn = sqlite3.connect(args.capture)
    doms = nsys_calls.domain_ids(conn)
    if nsys_calls.OWN_DOMAIN not in doms:
        sys.exit(f"capture has no NVTX domain {nsys_calls.OWN_DOMAIN!r}: the run needs "
                 "PEACOCK_BENCHMARK_CAPTURE=trace, which turns the harness's ranges on")
    if "demangledName" not in columns_of(conn, KERNEL_TABLE):
        sys.exit(f"capture's {KERNEL_TABLE} has no demangledName column, so a decode kernel "
                 "cannot be told from any other and every load would read as `other`. "
                 "Export it with an nsys that writes kernel names.")
    windows, calls = load_windows(nsys_calls.regions(nsys_calls.ranges(
        conn, doms[nsys_calls.OWN_DOMAIN])))
    if not windows:
        sys.exit(f"capture has no {LOAD_KIND} region, so it holds no parquet load to break down")

    owners = Owners(windows)
    api = api_calls(conn)
    found = collections.defaultdict(Attributed)
    ambiguous = [0, 0]
    for op in device_ops(conn, api):
        open_now = owners.at(op.launch)
        if len(open_now) > 1:
            ambiguous[0] += 1
            ambiguous[1] += op.end - op.start
        elif open_now:
            (owner, tids), = open_now.items()
            found[owner].ops.append(op)
            if op.tid not in tids:
                found[owner].off_thread_ns += op.end - op.start
    for start, end, _, _, name in api:
        open_now = owners.at(start)
        if name and PINNED_ALLOC_RE.match(name) and len(open_now) == 1:
            found[next(iter(open_now))].allocs.append((start, end))
    source, reads = host_reads(conn, doms)
    reads = Spans(reads)
    kinds = memory_kinds(conn) if columns_of(conn, "CUPTI_ACTIVITY_KIND_MEMCPY") else (None, None)

    rows = []
    for key in sorted(windows):
        per_exec = [execution_values(found[(key, e)], ws, reads, kinds)
                    for e, ws in sorted(windows[key].items())]
        (dataset, sf, query, mode), seq = key
        row = dict(dataset=dataset, sf=sf, query=query, mode=mode, recipe_seq=seq,
                   executions=len(per_exec), calls=len(calls[key]), host_read_source=source)
        for column in per_exec[0]:
            value = statistics.median(v[column] for v in per_exec)
            row[column] = round(value, 1) if column in RATES else round(value)
        rows.append(row)
    record.write_tsv(args.out, NOTES, COLUMNS, [[r[c] for c in COLUMNS] for r in rows])

    print(f"{ambiguous[0]} device operations, {ambiguous[1] / 1e6:.1f} ms, were launched where "
          "two loads were open: ambiguous, so credited to neither")
    print(f"host read from {source}")
    for r in rows:
        print(f"{r['query']} {r['mode']}  #{r['recipe_seq']}  load {r['load_us']} us  "
              f"read {r['host_read_us']}  h2d {r['h2d_us']} us at {r['h2d_gbps']} GB/s "
              f"(pinned {r['h2d_pinned_gbps']}, pageable {r['h2d_pageable_gbps']})  "
              f"decompress {r['decompress_us']}  decode {r['decode_us']}  "
              f"other {r['other_device_us']}  unattributed {r['unattributed_us']}")
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
