"""`nsys_loads.py` over a synthetic capture: where one parquet load's time goes.

The question it answers is whether a load is its copy or its decode, and the copy's rate is
the number the pinned-memory work is judged by. So the things it can get quietly wrong are
which load a device operation is charged to — the reader launches from threads of its own —
and what a copy's rate is when a load has several copies, of two memory kinds, or none.
"""

import contextlib
import io
import pathlib
import sqlite3
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import nsys_loads  # noqa: E402
import record  # noqa: E402
from capture import PAGEABLE, PINNED, Capture  # noqa: E402
from harness import main, raises  # noqa: E402

Q6 = "tpch.sf40 q6 tp1-single"


def loads_of(capture, out):
    sys.argv = ["nsys_loads.py", "--capture", str(capture), "--out", str(out)]
    nsys_loads.main()
    return record.read_tsv(out)


def assert_columns(row, **expected):
    """Numbers compared as numbers, so `0` and `0.0` are one rate."""
    got = {}
    for name, value in expected.items():
        got[name] = float(row[name]) if isinstance(value, (int, float)) else row[name]
    assert got == expected, {k: (got[k], v) for k, v in expected.items() if got[k] != v}


def q6_load(cap):
    """One load holding every kind of time a load has, once each. Returns its start."""
    t = cap.clock
    cap.load(Q6, 0, 0, t, t + 1_000_000)
    cap.push("libkvikio", "posix_device_read()", t + 100_000, t + 300_000, tid=2)
    cap.memcpy(t + 250_000, 80_000, 4_000_000, PINNED, tid=2)
    cap.memcpy(t + 400_000, 100_000, 1_000_000, PAGEABLE)
    cap.kernel(t + 500_000, 100_000, "cudf::io::unsnap_kernel")
    cap.kernel(t + 600_000, 150_000,
               "void cudf::io::parquet::detail::gpuDecodePageData<int>()")
    cap.kernel(t + 750_000, 50_000, "cudf::detail::cast_fn")
    cap.runtime_call("cudaHostAlloc_v3020", t + 800_000, t + 820_000)
    return t


def test_load_row_splits_copy_from_decode():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        cap = Capture(tmp / "capture.sqlite")
        q6_load(cap)
        cap.close()
        rows = loads_of(tmp / "capture.sqlite", tmp / "loads.tsv")

    assert len(rows) == 1, rows
    assert_columns(
        rows[0], dataset="tpch", sf="40", query="q6", mode="tp1-single", recipe_seq=0,
        executions=1, calls=1, load_us=1000, host_read_source="kvikio", host_read_us=200,
        h2d_count=2, h2d_bytes=5_000_000, h2d_us=180, h2d_gbps=27.8,
        h2d_pinned_bytes=4_000_000, h2d_pinned_gbps=50.0,
        h2d_pageable_bytes=1_000_000, h2d_pageable_gbps=10.0, h2d_max_bytes=4_000_000,
        decompress_us=100, decode_us=150, other_device_us=50, off_thread_device_us=80,
        pinned_alloc_count=1, pinned_alloc_us=20, unattributed_us=350)


def test_off_thread_work_outside_a_load_is_not_its_own():
    """The reader thread's copy during the next node belongs to that node, not the load."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        cap = Capture(tmp / "capture.sqlite")
        t = cap.clock
        cap.load(Q6, 0, 0, t, t + 400_000)
        start, end = t + 500_000, t + 900_000
        cap.push("peacockdb", Q6, start, end)
        cap.push("peacockdb", "1.0 CudfFilter", start, end)
        cap.push("peacockdb", "p0", start, end)
        cap.memcpy(start + 100_000, 50_000, 1_000_000, PINNED, tid=2)
        cap.close()
        rows = loads_of(tmp / "capture.sqlite", tmp / "loads.tsv")

    assert [r["recipe_seq"] for r in rows] == ["0"], rows
    assert_columns(rows[0], h2d_count=0, off_thread_device_us=0)


def test_work_reaching_past_the_load_is_clipped_to_it():
    """A read begun before the load and a kernel still running after it: only the part
    inside the window is the load's wall time, though the kernel's duration is its own."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        cap = Capture(tmp / "capture.sqlite")
        t = cap.clock
        cap.load(Q6, 0, 0, t, t + 500_000)
        cap.push("libkvikio", "posix_device_read()", t - 100_000, t + 100_000, tid=2)
        cap.kernel(t + 400_000, 300_000, "cudf::io::parquet::detail::gpuDecodePageData")
        cap.close()
        rows = loads_of(tmp / "capture.sqlite", tmp / "loads.tsv")

    assert_columns(rows[0], load_us=500, host_read_us=100, decode_us=300, unattributed_us=300)


def test_executions_take_the_median():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        cap = Capture(tmp / "capture.sqlite")
        for span in (900_000, 1_000_000, 5_000_000):
            t = cap.clock
            cap.load(Q6, 0, 0, t, t + span)
        cap.close()
        rows = loads_of(tmp / "capture.sqlite", tmp / "loads.tsv")

    assert len(rows) == 1, rows
    assert_columns(rows[0], executions=3, calls=1, load_us=1000)


def test_batched_load_sums_its_calls():
    """A batched mode drives one load once per batch; an execution's load is all of them."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        cap = Capture(tmp / "capture.sqlite")
        for _ in range(2):
            t = cap.clock
            cap.load(Q6, 0, 0, t, t + 400_000)
            cap.load(Q6, 0, 1, t + 500_000, t + 900_000)
        cap.close()
        rows = loads_of(tmp / "capture.sqlite", tmp / "loads.tsv")

    assert len(rows) == 1, rows
    assert_columns(rows[0], executions=2, calls=2, load_us=800)


def test_osrt_preferred_when_present():
    """With OS runtime calls traced, the read is the reads themselves, not kvikio's ranges
    around them, and a call that is not a read is not one."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        cap = Capture(tmp / "capture.sqlite")
        t = q6_load(cap)
        cap.osrt("pread64", t + 100_000, t + 150_000, tid=2)
        cap.osrt("ioctl", t + 160_000, t + 190_000, tid=2)
        cap.close()
        rows = loads_of(tmp / "capture.sqlite", tmp / "loads.tsv")

    assert_columns(rows[0], host_read_source="osrt", host_read_us=50)


def test_load_without_copies_has_zero_rates():
    """A `rows_only` scan copies nothing, and an export without copies has no MEMCPY table."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        path = tmp / "capture.sqlite"
        cap = Capture(path)
        t = cap.clock
        cap.load(Q6, 0, 0, t, t + 500_000)
        cap.kernel(t + 100_000, 200_000, "cudf::io::parquet::detail::gpuDecodePageData")
        cap.close()
        with contextlib.closing(sqlite3.connect(path)) as conn:
            conn.execute("drop table CUPTI_ACTIVITY_KIND_MEMCPY")
            conn.commit()
        rows = loads_of(path, tmp / "loads.tsv")

    assert len(rows) == 1, rows
    assert_columns(rows[0], h2d_count=0, h2d_bytes=0, h2d_gbps=0, h2d_pinned_gbps=0,
                   h2d_pageable_gbps=0, h2d_max_bytes=0, decode_us=200)


def test_concurrent_loads_leave_off_thread_work_unattributed():
    """Two loads open at once: a copy from a third thread could be either's, so it is
    neither's, and the run says how much of that there was."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        cap = Capture(tmp / "capture.sqlite")
        t = cap.clock
        cap.load(Q6, 0, 0, t, t + 1_000_000, tid=1)
        cap.load(Q6, 2, 0, t + 500_000, t + 1_500_000, tid=3)
        cap.memcpy(t + 700_000, 50_000, 1_000_000, PINNED, tid=2)
        cap.close()
        printed = io.StringIO()
        with contextlib.redirect_stdout(printed):
            rows = loads_of(tmp / "capture.sqlite", tmp / "loads.tsv")

    assert sorted(r["recipe_seq"] for r in rows) == ["0", "2"], rows
    for row in rows:
        assert_columns(row, h2d_count=0, off_thread_device_us=0)
    assert "ambiguous" in printed.getvalue(), printed.getvalue()


def test_capture_without_kernel_names_is_refused():
    """Without names every kernel is `other`, and a load would read as all overhead."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        path = tmp / "capture.sqlite"
        cap = Capture(path)
        q6_load(cap)
        cap.close()
        with contextlib.closing(sqlite3.connect(path)) as conn:
            conn.execute("alter table CUPTI_ACTIVITY_KIND_KERNEL drop column demangledName")
            conn.commit()
        with raises(SystemExit, match="demangledName"):
            loads_of(path, tmp / "loads.tsv")
        assert not (tmp / "loads.tsv").exists(), "the refused capture left a loads.tsv"


def test_memory_kinds_are_read_off_the_export_s_enum():
    """`srcKind` means what the export's `ENUM_CUDA_MEM_KIND` says. nsys numbers pinned 1,
    CUPTI numbers it 2, and reading one as the other swaps pinned and pageable."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        path = tmp / "capture.sqlite"
        cap = Capture(path)
        t = cap.clock
        cap.load(Q6, 0, 0, t, t + 1_000_000)
        cap.memcpy(t + 100_000, 80_000, 4_000_000, 7)
        cap.memcpy(t + 300_000, 100_000, 1_000_000, 5)
        cap.close()
        with contextlib.closing(sqlite3.connect(path)) as conn:
            conn.execute("update ENUM_CUDA_MEM_KIND set id = id + 5")
            conn.execute("update ENUM_CUDA_MEM_KIND set id = 7 where name like '%PINNED'")
            conn.commit()
        rows = loads_of(path, tmp / "loads.tsv")

    assert_columns(rows[0], h2d_pinned_bytes=4_000_000, h2d_pinned_gbps=50.0,
                   h2d_pageable_bytes=1_000_000, h2d_pageable_gbps=10.0)


def test_capture_without_the_memory_kind_enum_is_refused():
    """Without the table a copy's kind cannot be read, and guessing it is the swap above."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        path = tmp / "capture.sqlite"
        cap = Capture(path)
        q6_load(cap)
        cap.close()
        with contextlib.closing(sqlite3.connect(path)) as conn:
            conn.execute("drop table ENUM_CUDA_MEM_KIND")
            conn.commit()
        with raises(SystemExit, match="ENUM_CUDA_MEM_KIND"):
            loads_of(path, tmp / "loads.tsv")
        assert not (tmp / "loads.tsv").exists(), "the refused capture left a loads.tsv"


def test_capture_without_loads_is_refused():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        cap = Capture(tmp / "capture.sqlite")
        cap.case(Q6, [(1, 0, "CudfFilter", ["apply_boolean_mask"])])
        cap.close()
        with raises(SystemExit, match="CudfScan"):
            loads_of(tmp / "capture.sqlite", tmp / "loads.tsv")
        assert not (tmp / "loads.tsv").exists(), "the refused capture left a loads.tsv"


if __name__ == "__main__":
    raise SystemExit(main(globals()))
