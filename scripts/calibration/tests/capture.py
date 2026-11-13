"""A synthetic Nsight export, for the calibration readers' tests.

`nsys_calls.py`, `nsys_loads.py` and `nsys_hbm.py` read one capture for different numbers,
out of the same tables: the NVTX ranges our domain, libcudf and kvikio push, CUPTI's device
and API rows, the OS runtime calls, and — for the counters pass — the GPU metric samples.
Building those here is what lets the readers be tested without a device, an nsys and half an
hour.

Only the columns the readers select exist; a real export has hundreds more. Times are
nanoseconds on one arbitrary clock, which is what an export carries.
"""

import sqlite3

PUSHPOP_RANGE = 59
DOMAIN_CREATE = 75

SCHEMA = """
create table StringIds (id integer, value text);
create table NVTX_EVENTS (start integer, end integer, text text, textId integer,
                          eventType integer, domainId integer, globalTid integer);
create table CUPTI_ACTIVITY_KIND_KERNEL (start integer, end integer, correlationId integer,
                                         demangledName integer);
create table CUPTI_ACTIVITY_KIND_MEMCPY (start integer, end integer, correlationId integer,
                                         bytes integer, copyKind integer, srcKind integer);
create table CUPTI_ACTIVITY_KIND_MEMSET (start integer, end integer, correlationId integer);
create table CUPTI_ACTIVITY_KIND_RUNTIME (start integer, end integer, globalTid integer,
                                          correlationId integer, nameId integer);
create table TARGET_INFO_GPU (id integer, name text);
create table TARGET_INFO_GPU_METRICS (metricId integer, metricName text);
create table GPU_METRICS (timestamp integer, value real, metricId integer);
create table ENUM_CUDA_MEM_KIND (id integer, name text, label text);
"""

# The export's own numbering of a copy's memory kind, as nsys 2025 writes it. It is not
# CUPTI's (1 pageable, 2 pinned), and the readers decode `srcKind` through this table.
MEM_KINDS = [(0, "CUDA_MEMOPR_MEMORY_KIND_PAGEABLE", "Pageable"),
             (1, "CUDA_MEMOPR_MEMORY_KIND_PINNED", "Pinned"),
             (2, "CUDA_MEMOPR_MEMORY_KIND_DEVICE", "Device")]
PAGEABLE, PINNED = 0, 1


class Capture:
    """One capture under construction. `close()` when the file is complete."""

    def __init__(self, path, gpus=("NVIDIA H200",)):
        self.conn = sqlite3.connect(path)
        self.conn.executescript(SCHEMA)
        self.conn.executemany("insert into TARGET_INFO_GPU (id, name) values (?, ?)",
                              list(enumerate(gpus)))
        self.conn.executemany("insert into ENUM_CUDA_MEM_KIND (id, name, label) values (?, ?, ?)",
                              MEM_KINDS)
        self.domains = {}
        self.clock = 1_000_000
        self.correlation = 0

    def domain(self, name):
        if name not in self.domains:
            self.domains[name] = len(self.domains) + 1
            self.conn.execute(
                "insert into NVTX_EVENTS (eventType, domainId, text) values (?, ?, ?)",
                (DOMAIN_CREATE, self.domains[name], name),
            )
        return self.domains[name]

    def push(self, domain, text, start, end, tid=1):
        self.conn.execute(
            "insert into NVTX_EVENTS (start, end, text, eventType, domainId, globalTid) "
            "values (?, ?, ?, ?, ?, ?)",
            (start, end, text, PUSHPOP_RANGE, self.domain(domain), tid),
        )

    def device_work(self, launched_at, duration, tid=1):
        """A runtime launch and the kernel it correlates to.

        Two rows and not one: the reader joins them on the correlation id rather than by
        time, because a kernel outlives the call that submitted it.
        """
        self.correlation += 1
        self.conn.execute(
            "insert into CUPTI_ACTIVITY_KIND_RUNTIME (start, globalTid, correlationId) "
            "values (?, ?, ?)",
            (launched_at, tid, self.correlation),
        )
        self.conn.execute(
            "insert into CUPTI_ACTIVITY_KIND_KERNEL (start, end, correlationId) "
            "values (?, ?, ?)",
            (launched_at, launched_at + duration, self.correlation),
        )

    def string(self, value):
        """`value`'s id in StringIds, added on first use, as an export interns names."""
        row = self.conn.execute("select id from StringIds where value = ?", (value,)).fetchone()
        if row:
            return row[0]
        string_id = self.conn.execute("select count(*) from StringIds").fetchone()[0] + 1
        self.conn.execute("insert into StringIds (id, value) values (?, ?)", (string_id, value))
        return string_id

    def runtime_call(self, name, start, end, tid=1):
        """One CUDA runtime call, its own correlation id; returns that id."""
        self.correlation += 1
        self.conn.execute(
            "insert into CUPTI_ACTIVITY_KIND_RUNTIME "
            "(start, end, globalTid, correlationId, nameId) values (?, ?, ?, ?, ?)",
            (start, end, tid, self.correlation, self.string(name)),
        )
        return self.correlation

    def kernel(self, launched_at, duration, name, tid=1):
        """A named kernel, starting on the device the moment its launch does."""
        corr = self.runtime_call("cudaLaunchKernel_v7000", launched_at, launched_at + 5, tid)
        self.conn.execute(
            "insert into CUPTI_ACTIVITY_KIND_KERNEL (start, end, correlationId, demangledName) "
            "values (?, ?, ?, ?)",
            (launched_at, launched_at + duration, corr, self.string(name)),
        )

    def memcpy(self, launched_at, duration, nbytes, src_kind, tid=1, copy_kind=1):
        """A copy of `nbytes`; `copy_kind` is 1 for HtoD, `src_kind` an `ENUM_CUDA_MEM_KIND` id."""
        corr = self.runtime_call("cudaMemcpyAsync_v3020", launched_at, launched_at + 5, tid)
        self.conn.execute(
            "insert into CUPTI_ACTIVITY_KIND_MEMCPY "
            "(start, end, correlationId, bytes, copyKind, srcKind) values (?, ?, ?, ?, ?, ?)",
            (launched_at, launched_at + duration, corr, nbytes, copy_kind, src_kind),
        )

    def osrt(self, name, start, end, tid=1):
        """One OS runtime call. The table exists only once one is added, as in an export
        taken without `--trace=osrt`."""
        self.conn.execute(
            "create table if not exists OSRT_API "
            "(start integer, end integer, nameId integer, globalTid integer)"
        )
        self.conn.execute(
            "insert into OSRT_API (start, end, nameId, globalTid) values (?, ?, ?, ?)",
            (start, end, self.string(name), tid),
        )

    def load(self, text, seq, call_index, start, end, tid=1):
        """One case range around one `CudfScan` call and its `p0` range, all `[start, end)`.

        For tests that place device work by absolute time, where `case` lays calls out by
        its own clock. The clock moves past `end`, so a later `case` does not overlap it.
        """
        self.push("peacockdb", text, start, end, tid)
        self.push("peacockdb", f"{seq}.{call_index} CudfScan", start, end, tid)
        self.push("peacockdb", "p0", start, end, tid)
        self.clock = max(self.clock, end + 1000)

    def metric(self, name, samples):
        """One GPU metric's samples, as `(timestamp, percent of peak)`."""
        metric_id = self.conn.execute(
            "select count(*) from TARGET_INFO_GPU_METRICS"
        ).fetchone()[0] + 1
        self.conn.execute(
            "insert into TARGET_INFO_GPU_METRICS (metricId, metricName) values (?, ?)",
            (metric_id, name),
        )
        self.conn.executemany(
            "insert into GPU_METRICS (timestamp, value, metricId) values (?, ?, ?)",
            [(at, value, metric_id) for at, value in samples],
        )

    def case(self, text, calls, tid=1, device_us=1):
        """One benchmark case: its range, and inside it one call range per entry.

        `calls` is `[(seq, call_index, kind, [libcudf call names])]`, one per call the
        harness made, in the order it made them. Each call gets the `p0` partition range
        the instrument opens per output partition, and each libcudf name a range inside
        that — the three levels a reader tells apart by name.
        """
        spans = []
        case_start = self.clock
        self.clock += 10
        for seq, call_index, kind, inner in calls:
            call_start = self.clock
            self.push("peacockdb", f"{seq}.{call_index} {kind}", call_start,
                      call_start + 100 * (len(inner) + 2), tid)
            self.push("peacockdb", "p0", call_start + 5,
                      call_start + 100 * (len(inner) + 2) - 5, tid)
            at = call_start + 10
            for name in inner:
                self.push("libcudf", name, at, at + 80, tid)
                self.device_work(at + 10, device_us * 1000, tid)
                at += 100
            spans.append((call_start, call_start + 100 * (len(inner) + 2)))
            self.clock = call_start + 100 * (len(inner) + 2) + 10
        self.push("peacockdb", text, case_start, self.clock, tid)
        self.clock += 1000
        return spans

    def close(self):
        self.conn.commit()
        self.conn.close()
