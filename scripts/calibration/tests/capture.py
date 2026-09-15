"""A synthetic Nsight export, for the calibration readers' tests.

`nsys_calls.py` and `nsys_hbm.py` read one capture for two different numbers, out of the
same tables: the NVTX ranges our domain and libcudf push, CUPTI's device rows, and — for
the counters pass — the GPU metric samples. Building those here is what lets both readers
be tested without a device, an nsys and half an hour.

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
create table CUPTI_ACTIVITY_KIND_KERNEL (start integer, end integer, correlationId integer);
create table CUPTI_ACTIVITY_KIND_MEMCPY (start integer, end integer, correlationId integer);
create table CUPTI_ACTIVITY_KIND_MEMSET (start integer, end integer, correlationId integer);
create table CUPTI_ACTIVITY_KIND_RUNTIME (start integer, globalTid integer,
                                          correlationId integer);
create table TARGET_INFO_GPU_METRICS (metricId integer, metricName text);
create table GPU_METRICS (timestamp integer, value real, metricId integer);
"""


class Capture:
    """One capture under construction. `close()` when the file is complete."""

    def __init__(self, path):
        self.conn = sqlite3.connect(path)
        self.conn.executescript(SCHEMA)
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
