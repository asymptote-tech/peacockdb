"""`batching_report.py` over a fixture testdata tree: the query-batching page's numbers.

What it can get quietly wrong is which queries a sum covers. A batch is a prefix of today's
case list, so a fused run of other queries, a query missing from a sum, a batch the list does
not exceed and a batch that ran out of memory must each show as what they are, never as a
number that looks like the others.
"""

import contextlib
import copy
import io
import pathlib
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import batching_report  # noqa: E402
from harness import main, raises  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
DUCKDB_FIXTURE = REPO / "testdata" / "fixtures" / "duckdb-results.tsv"

CACHE_ON = ("on cap=147.4GiB resident=1.0GiB files=2 fills=2 fill_us=5000 fallbacks=0 "
            "evictions=0 reloads=0 source=default")
LOADS_COLUMNS = ("dataset sf query mode recipe_seq executions calls load_us host_read_source "
                 "host_read_us h2d_count h2d_bytes h2d_us h2d_gbps h2d_pinned_bytes "
                 "h2d_pinned_gbps h2d_pageable_bytes h2d_pageable_gbps h2d_max_bytes "
                 "decompress_us decode_us other_device_us off_thread_device_us "
                 "pinned_alloc_count pinned_alloc_us unattributed_us").split()
PROVENANCE = ("device=NVIDIA H100 80GB HBM3", "driver=580.173.02", "cuda=12.2")
MARKER = "skipped: same set as all: the list holds {} queries"


def timed(load, execute, **knobs):
    """One timed trailer's values; `knobs` override any of them."""
    values = dict(load=load, execute=execute, runs=10, cache=CACHE_ON,
                  prefill=(2, 1000000, 5000), cudf="25.02.02", peak=1073741824)
    values.update(knobs)
    return values


def fused(queries, load, execute, **knobs):
    values = dict(queries=queries, outcome="measured", cap=3, estimate=10 * 2**30,
                  budget=60 * 2**30, over_budget="false", merge=(20, [20]),
                  pipeline=(3000, 2000, 1000), shared="lineitem:q1+q3",
                  reads="lineitem:1", rereads=0,
                  run=timed(load, execute, **knobs.pop("run", {})))
    values.update(knobs)
    return values


def load_row(load, host_read=0, h2d=0, decompress=0, decode=0, other=0, unattributed=0,
             source="none"):
    return dict(load_us=load, host_read_source=source, host_read_us=host_read, h2d_us=h2d,
                decompress_us=decompress, decode_us=decode, other_device_us=other,
                unattributed_us=unattributed)


def spec():
    """The default tree: tpch q1 q3 q6(wrong) q14 counted, q9 skipped, a micro query beside
    them; tpcds q1 q2 q3. first10 and first20 are markers in both."""
    return {
        "cases": [
            "tpch,40,q1,tp1_single,ok,",
            "tpch,40,q3,tp1_single,ok,",
            "tpch,40,q6,tp1_single,wrong,#7",
            "tpch,40,aggregate_groupby,tp1_single,ok,",
            "tpch,40,q9,,skipped,timeout: the case ran past 1800 s",
            "tpch,40,q14,tp1_single,ok,",
            "tpcds,40,q1,tp1_single,ok,",
            "tpcds,40,q2,tp1_single,ok,",
            "tpcds,40,q3,tp1_single,ok,",
        ],
        "unfused": {
            "tpch": {
                "aggregate-groupby": timed(100, 50),
                "q1": timed(1000, 500),
                "q3": timed(2000, 700),
                "q6": timed(3000, 300),
                "q9": "status=failed reason=timeout: the case ran past 1800 s\n",
                "q14": timed(4000, 400),
            },
            "tpcds": {"q1": timed(1500, 600), "q2": timed(2500, 800), "q3": timed(3500, 900)},
        },
        "fused": {
            "tpch": {
                "first3": fused(["q1", "q3", "q6"], 4000, 1400),
                "first10": MARKER.format(4),
                "first20": MARKER.format(4),
                "all": fused(["q1", "q3", "q6", "q14"], 7000, 1900, run={"runs": 3}),
            },
            "tpcds": {
                "first3": fused(["q1", "q2", "q3"], 5000, 2300),
                "first10": MARKER.format(3),
                "first20": MARKER.format(3),
                "all": fused(["q1", "q2", "q3"], 5100, 2300, run={"runs": 3}),
            },
        },
        "duckdb": {
            "tpch": {"q1": 100, "q3": 200, "q6": 300, "q14": 400, "aggregate_groupby": 50},
            "tpcds": {"q1": 150, "q2": 250, "q3": 350},
        },
        "duckdb_floor": {"tpch": 30, "tpcds": 36},
        "loads": {
            "tpch": {
                "aggregate-groupby": [load_row(70)],
                "q1": [load_row(800, h2d=100, decompress=200, decode=300, other=10,
                                unattributed=190),
                       load_row(200, h2d=50, decompress=50, decode=50, other=10,
                                unattributed=40)],
                "q3": [load_row(1500, h2d=310, decompress=500, decode=400, other=100,
                                unattributed=200)],
                "q6": [load_row(2500, h2d=500, decompress=900, decode=600, other=100,
                                unattributed=400)],
                "q14": [load_row(3000, h2d=600, decompress=1000, decode=800, other=200,
                                 unattributed=400)],
            },
            "tpcds": {q: [load_row(1000, h2d=200, decompress=300, decode=300, other=100,
                                   unattributed=100)] for q in ("q1", "q2", "q3")},
        },
        "fused_loads": {
            "tpch": {"first3": [load_row(3000, h2d=600, decompress=1000, decode=900)],
                     "all": [load_row(6000, h2d=1200, decompress=2000, decode=1800)]},
            "tpcds": {"first3": [load_row(4000, h2d=800, decompress=1200, decode=1000)],
                      "all": [load_row(4100, h2d=800, decompress=1200, decode=1000)]},
        },
        "fused_failed": {},
        "tp4": {},
    }


def run_text(values):
    load, execute = values["load"], values["execute"]
    device = load + execute
    runs = ",".join(str(device + 100 + i) for i in range(values["runs"]))
    files, size, us = values["prefill"]
    lines = [f"run_us={device + 100}", f"device_us={device}", f"load_us={load}",
             f"execute_us={execute}", f"peak_bytes={values['peak']}", f"runs=[{runs}]",
             "build=release",
             "allocator=rmm-pool initial=69.0GiB max=69.0GiB of 78.7GiB free on a discrete device"]
    if values["cache"] is not None:
        lines += [f"pinned_cache={values['cache']}", f"prefill_files={files}",
                  f"prefill_bytes={size}", f"prefill_us={us}"]
    lines += [*PROVENANCE, f"cudf={values['cudf']}", "host=h100-vm"]
    return "--- run ---\n" + "\n".join(lines) + "\n"


TREE = "GpuUnload\n  time_us=[[1]] total_us=1 batch_rows=[[1]] batch_bytes=[[8]]\n"


def section_text(body):
    if isinstance(body, str):
        return body
    return TREE + run_text(body)


def fused_text(body):
    if isinstance(body, str):
        return body + "\n"
    saved, entries = body["merge"]
    serial, makespan, pipe_saved = body["pipeline"]
    lines = ["--- batch ---", f"queries=[{','.join(body['queries'])}]",
             f"outcome={body['outcome']}"]
    if body["outcome"] == "out-of-memory":
        lines.append("reason=std::bad_alloc: out of memory")
    lines += [f"shared=[{body['shared']}]", "unshared=[]", f"reads=[{body['reads']}]",
              "buffers=1", "buffer_bytes=100", f"rereads={body['rereads']}",
              "hold query=q3 table=lineitem hold=GpuHashJoin choice=buffer above=7 bytes=100 "
              "reread_us=10 why=fits: 100 <= 900",
              f"cap={body['cap']}", "pool_bytes=74088185856", f"budget_bytes={body['budget']}",
              f"estimate_bytes={body['estimate']}", f"over_budget={body['over_budget']}"]
    measured = body["outcome"] == "measured"
    if measured:
        lines.append("answers=match")
    lines += ["--- merge ---", f"saved_us={saved}"]
    lines += [f"entry saved_us={s} members=q1:3,q3:5 node=GpuFilter: predicate=a = b"
              for s in entries]
    lines += ["--- pipeline ---",
              "model=one loader and one executor, each sequential, side by side",
              "load_order=[lineitem]", f"serial_us={serial}", f"makespan_us={makespan}",
              f"saved_us={pipe_saved}"]
    if not measured:
        provenance = [*PROVENANCE, "cudf=25.02.02", "host=h100-vm"]
        return "\n".join(lines + ["--- run ---", *provenance]) + "\n"
    run = body["run"]
    total = run["load"] + run["execute"]
    lines += [f"fused_measured_us={total}", f"pipelined_estimate_us={max(total - pipe_saved, 0)}",
              "--- tree ---"]
    return "\n".join(lines) + "\n" + TREE + run_text(run)


def loads_text(rows_by_query, dataset):
    lines = ["# Where one parquet load's time goes.", "\t".join(LOADS_COLUMNS)]
    for query, rows in rows_by_query.items():
        for seq, row in enumerate(rows):
            full = {column: "0" for column in LOADS_COLUMNS}
            full.update(dataset=dataset, sf="40", query=query, mode="tp1-single",
                        recipe_seq=str(seq), executions="10", calls="1")
            full.update({key: str(value) for key, value in row.items()})
            lines.append("\t".join(full[column] for column in LOADS_COLUMNS))
    return "\n".join(lines) + "\n"


def duckdb_text(rows, dataset, floor):
    """A row's value is its ms, `(ms, warm)`, or `None` for a failed row; `floor` `None`
    leaves `floor_ms` out of the preamble."""
    floor_key = "" if floor is None else f" floor_ms={floor}"
    lines = [f"# duckdb=1.5.4 threads=128 host=cpu-vm cpu=AMD_EPYC cores=128 memory_gib=503.7 "
             f"db=/var/lib/peacock/{dataset}.sf40.duckdb resident_pct=100.0{floor_key}",
             "query\tms_2nd_min\truns_ms\tworkset_bytes\tdisk_read_bytes\twarm\tstatus"]
    for query, value in rows.items():
        if value is None:
            lines.append(f"{query}\t-\t-\t1024\t-\t-\tfailed")
        else:
            ms, warm = value if isinstance(value, tuple) else (value, "yes")
            runs = ",".join(str(ms + i) for i in range(6))
            lines.append(f"{query}\t{ms}\t{runs}\t1024\t0\t{warm}\tok")
    return "\n".join(lines) + "\n"


def tree(tmp, s=None):
    """Writes `s` (default `spec()`) as a testdata dir under `tmp`; returns its path."""
    s = spec() if s is None else s
    root = pathlib.Path(tmp) / "testdata"
    results = root / "benchmark-results"
    device = results / "h100-80gb-hbm3"
    files = {device / "cases.csv": "dataset,sf,query,modes,status,note\n"
             + "\n".join(s["cases"]) + "\n"}
    for ds in ("tpch", "tpcds"):
        files[device / f"{ds}.sf40" / "tp1_single.benchmark.txt"] = "".join(
            f"== {name}\n{section_text(body)}" for name, body in s["unfused"][ds].items())
        files[device / "fused" / f"{ds}.sf40" / "tp1_single.fused.txt"] = "".join(
            f"== {name}\n{fused_text(body)}" for name, body in s["fused"][ds].items())
        files[results / "duckdb-cpu128" / f"{ds}.sf40" / "duckdb.tsv"] = duckdb_text(
            s["duckdb"][ds], ds, s["duckdb_floor"][ds])
        calibration = root / "calibration" / "h100-80gb-hbm3"
        files[calibration / f"{ds}.sf40" / "loads.tsv"] = loads_text(s["loads"][ds], ds)
        for batch, rows in s["fused_loads"][ds].items():
            files[calibration / "fused" / f"{ds}.sf40" / f"{batch}.loads.tsv"] = loads_text(
                {batch: rows}, ds)
        if ds in s["fused_failed"]:
            files[calibration / "fused" / f"{ds}.sf40" / "failed.tsv"] = s["fused_failed"][ds]
        if ds in s["tp4"]:
            files[device / f"{ds}.sf40" / "tp4_sized.benchmark.txt"] = "".join(
                f"== {name}\n{section_text(body)}" for name, body in s["tp4"][ds].items())
    for path, text in files.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    return root


def rendered(s=None):
    with tempfile.TemporaryDirectory() as tmp:
        return batching_report.render(tree(tmp, s))


def table(text, heading):
    """The rows of the first table under the `###`/`##` line `heading`, as cell lists."""
    lines = text.splitlines()
    at = lines.index(heading)
    rows = []
    for line in lines[at + 1:]:
        if line.startswith("#"):
            break
        if line.startswith("|") and not line.startswith("|---") and not line.startswith("|--"):
            rows.append([cell.strip() for cell in line.strip("|").split(" | ")])
        elif rows and not line.startswith("|"):
            break
    return rows


def row(text, heading, first):
    found = [r for r in table(text, heading) if r[0] == first]
    assert len(found) == 1, (heading, first, table(text, heading))
    return found[0]


def section_after(text, heading):
    """`heading`'s lines up to the next heading."""
    lines = text.splitlines()
    at = lines.index(heading)
    out = []
    for line in lines[at + 1:]:
        if line.startswith("#"):
            break
        out.append(line)
    return "\n".join(out)


TPCH = "### TPC-H sf40: fused against unfused and DuckDB"
TPCH_HINTS = "### TPC-H sf40: the two hints"
TPCH_SPLIT = "### TPC-H sf40: load against decode"
PROVENANCE_H100 = "### Provenance: the H100"


def run_main(root, report, *extra):
    err = io.StringIO()
    with contextlib.redirect_stderr(err):
        code = batching_report.main(["--testdata", str(root), "--report", str(report), *extra])
    return code, err.getvalue()


def refused(s):
    with tempfile.TemporaryDirectory() as tmp:
        root = tree(tmp, s)
        report = pathlib.Path(tmp) / "page.md"
        report.write_text(f"# page\n{batching_report.BEGIN}\n{batching_report.END}\n")
        return run_main(root, report)


# --- reading the inputs ----------------------------------------------------------------------


def test_counted_takes_tpch_q1_to_q22_in_file_order():
    with tempfile.TemporaryDirectory() as tmp:
        cases = batching_report.device_cases.rows(
            tree(tmp) / "benchmark-results" / "h100-80gb-hbm3" / "cases.csv")
    assert batching_report.counted(cases, "tpch") == ["q1", "q3", "q6", "q14"]
    assert batching_report.counted(cases, "tpcds") == ["q1", "q2", "q3"]
    notes = batching_report.notes(cases, "tpch")
    assert ("q6", "wrong answer: #7") in notes
    assert ("q9", "skipped at sf40: timeout: the case ran past 1800 s") in notes
    assert len(notes) == 2


def test_duckdb_reads_the_shared_fixture():
    # The values of `the_shared_fixture_reads_as_expected` (`duckdb_results/tests.rs`).
    preamble, rows = batching_report.duckdb(DUCKDB_FIXTURE.read_text())
    assert list(preamble.items()) == [
        ("duckdb", "1.5.4"), ("threads", "128"), ("host", "nebius-cpu128"),
        ("cpu", "AMD_EPYC_9654_96-Core_Processor"), ("cores", "128"), ("memory_gib", "1007.7"),
        ("db", "/var/lib/peacock/tpch.sf40.duckdb"), ("resident_pct", "100.0"),
        ("floor_ms", "31"),
    ]
    assert rows == {
        "q1": dict(ms_2nd_min=412.0, runs_ms=[431.0, 412.0, 409.0, 415.0, 420.0, 413.0],
                   workset_bytes=2147483648, disk_read_bytes=0, warm="yes", status="ok"),
        "aggregate_groupby": dict(ms_2nd_min=None, runs_ms=[], workset_bytes=1073741824,
                                  disk_read_bytes=None, warm="-", status="failed"),
    }


def test_duckdb_refuses_what_its_owner_refuses():
    head = "# duckdb=1.5.4 threads=8 host=h\n" + "\t".join(
        ("query", "ms_2nd_min", "runs_ms", "workset_bytes", "disk_read_bytes", "warm",
         "status")) + "\n"
    for body, why in {
        "q6\t17\t17,16\t0\tyes\tok\n": "not seven fields",
        "q6\t17\t17,16\t0\t0\tyes\tskipped\n": "unknown status",
        "q6\t17\t17,16\t0\t0\tyes\tok\nq6\t17\t17,16\t0\t0\tyes\tok\n": "listed twice",
        "q6\t17\t17\t0\t-\t-\tfailed\n": "a failed row has no time",
    }.items():
        with raises(ValueError, match=why):
            batching_report.duckdb(head + body)


def test_sections_and_trailer_read_bench_rows_format():
    trailer = ("--- run ---\nrun_us=162690\ndevice_us=162476\nload_us=101308\nexecute_us=61168\n"
               "peak_bytes=2147483648\nruns=[168555,167161]\nbuild=release\n"
               "allocator=rmm-pool initial=69.0GiB max=69.0GiB of 78.7GiB free\n"
               f"pinned_cache={CACHE_ON}\nprefill_files=8\nprefill_bytes=13207024435\n"
               "prefill_us=812345\ndevice=NVIDIA H100 80GB HBM3\ndriver=580\ncuda=12.2\n"
               "cudf=25.02.02\nhost=h\n")
    text = (f"== q3\n{TREE}{trailer}== q9\nskipped: not on this device\n"
            "== q17\nstatus=failed reason=prefill: no room\ndevice=NVIDIA H100 80GB HBM3\n")
    sections = batching_report.sections(text)
    assert [name for name, _ in sections] == ["q3", "q9", "q17"]
    read = batching_report.trailer(sections[0][1])
    assert len(read) == 17
    assert read["load_us"] == "101308"
    assert read["allocator"] == "rmm-pool initial=69.0GiB max=69.0GiB of 78.7GiB free"
    assert batching_report.trailer(sections[1][1]) is None
    assert batching_report.trailer(sections[2][1]) is None
    with raises(ValueError, match="load_us"):
        batching_report.trailer(TREE + trailer + "load_us=1\n")


def test_batches_take_prefixes_and_mark_short_ones():
    names = [f"q{n}" for n in range(1, 26)]
    lengths = [(name, len(batch)) for name, batch in batching_report.batches(names)]
    assert lengths == [("first3", 3), ("first10", 10), ("first20", 20), ("all", 25)]
    short = dict(batching_report.batches(names[:18]))
    assert short["first20"] is None and short["first10"] == names[:10]
    assert short["all"] == names[:18]
    two = dict(batching_report.batches(names[:2]))
    assert two["first3"] == names[:2]
    assert two["first10"] is None and two["first20"] is None


def test_fused_batch_reads_its_blocks():
    body = fused_text(fused(["q1", "q3"], 4000, 1400, merge=(30, [20, 10])))
    read = batching_report.fused_batch(body)
    assert read["queries"] == ["q1", "q3"]
    assert read["batch"]["outcome"] == "measured" and read["batch"]["cap"] == "3"
    assert read["merge"] == {"saved_us": 30, "entries": [20, 10]}
    assert read["pipeline"]["model"] == "one loader and one executor, each sequential, side by side"
    assert read["run"]["load_us"] == "4000"
    assert batching_report.fused_batch(MARKER.format(4)) == {"marker": MARKER.format(4)}
    assert batching_report.fused_batch("status=failed reason=prefill: no room\ncuda=12.2\n") == {
        "failed": "prefill: no room"}


def test_load_split_sums_columns_and_keeps_read_and_copy_apart():
    rows = [{k: str(v) for k, v in load_row(800, host_read=7, h2d=100, decompress=200,
                                            decode=300, other=10, unattributed=190).items()},
            {k: str(v) for k, v in load_row(200, host_read=3, h2d=50, decompress=50, decode=50,
                                            other=10, unattributed=40).items()}]
    assert batching_report.load_split(rows) == dict(
        load_us=1000, host_read_us=10, h2d_us=150, decode_us=600, other_device_us=20,
        unattributed_us=230)
    split = {r[1]: r[2:] for r in table(rendered(), TPCH_SPLIT) if r[0] == "first3"}
    assert split == {"unfused": ["5.0", "0.0", "1.0", "3.0", "0.2", "0.8"],
                     "fused": ["3.0", "0.0", "0.6", "1.9", "0.0", "0.0"]}, split
    footnote = section_after(rendered(), TPCH_SPLIT)
    assert "separate medians" in footnote and "overlap" in footnote, footnote


def test_a_failed_profile_with_its_loads_file_still_shows_the_split():
    s = spec()
    s["fused_failed"]["tpch"] = "first3\tmetrics\tthe capture failed\n"
    text = rendered(s)
    split = {r[1]: r[2:] for r in table(text, TPCH_SPLIT) if r[0] == "first3"}
    assert split["fused"] == ["3.0", "0.0", "0.6", "1.9", "0.0", "0.0"], split
    # The file may be an earlier run's, so the failure is said beside it, never dropped.
    footnote = section_after(text, TPCH_SPLIT).splitlines()
    assert ("- first3's fused row is read from `first3.loads.tsv`, though `failed.tsv` lists "
            "the batch: metrics: the capture failed.") in footnote, footnote
    assert not any("failed.tsv" in line for line in section_after(rendered(), TPCH_SPLIT)
                   .splitlines()), "no failure, no line"


def test_a_failed_profile_without_its_loads_file_shows_the_reason():
    s = spec()
    s["fused_failed"]["tpch"] = "first3\ttrace\tits capture did not come home\n"
    del s["fused_loads"]["tpch"]["first3"]
    split = {r[1]: r[2:] for r in table(rendered(s), TPCH_SPLIT) if r[0] == "first3"}
    assert split["fused"][0] == "profile failed: trace: its capture did not come home", split


def test_a_measured_batch_without_a_profile_or_a_failure_is_refused():
    s = spec()
    del s["fused_loads"]["tpch"]["first3"]
    code, err = refused(s)
    assert code == 1 and "first3.loads.tsv" in err, err


# --- the main table --------------------------------------------------------------------------


def test_the_first3_row_sums_its_three_queries():
    text = rendered()
    cells = row(text, TPCH, "first3")
    # fused 4000 + 1400 µs; unfused q1 q3 q6: load 6000, execute 1500; DuckDB 100 + 200 + 300
    # ms, and 600 − 3 × 30 with the floors out.
    assert cells == ["first3", "3", "4.0", "1.4", "6.0", "1.5", "600.0", "1.39×", "111.11×",
                     "94.44×"]


def test_the_duckdb_floor_is_named_and_its_column_says_so():
    text = rendered()
    assert table(text, TPCH)[0][-2:] == ["DuckDB / fused", "DuckDB / fused, floors out"]
    notes = section_after(text, TPCH)
    assert "`SELECT 1`" in notes and "30 ms" in notes, notes
    assert "36 ms" in section_after(text, "### TPC-DS sf40: fused against unfused and DuckDB")


def test_a_duckdb_file_without_its_floor_is_refused():
    s = spec()
    s["duckdb_floor"]["tpcds"] = None
    code, err = refused(s)
    assert code == 1 and "floor_ms" in err and "tpcds.sf40" in err, err


def test_a_duckdb_time_not_warm_is_named():
    s = spec()
    s["duckdb"]["tpch"]["q3"] = (200, "partial")
    notes = section_after(rendered(s), TPCH).splitlines()
    assert "- DuckDB: q3 ran warm=partial (first3, all)." in notes, notes
    assert not any("warm=" in line for line in section_after(rendered(), TPCH).splitlines())


def test_all_says_which_lines_of_the_list_it_holds():
    notes = section_after(rendered(), TPCH).splitlines()
    assert ("`all` is the list's 4 `ok` and `wrong` queries at tp1_single, of its 6 tpch "
            "lines: 1 skipped, 1 micro query not batched.") in notes, notes
    tpcds = section_after(rendered(), "### TPC-DS sf40: fused against unfused and DuckDB")
    assert "`all` is the list's 3 `ok` and `wrong` queries at tp1_single, of its 3 tpcds " \
           "lines." in tpcds.splitlines(), tpcds


def test_a_marker_batch_reads_same_set_as_all():
    text = rendered()
    cells = row(text, TPCH, "first20")
    assert cells[1] == "same set as all"
    assert not any(any(ch.isdigit() for ch in cell) for cell in cells[2:]), cells
    assert row(text, TPCH_HINTS, "first20")[1] == "same set as all"
    assert row(text, TPCH_SPLIT, "first20")[1] == "same set as all"


def test_an_out_of_memory_batch_shows_its_cap():
    s = spec()
    s["fused"]["tpch"]["all"] = fused(["q1", "q3", "q6", "q14"], 0, 0, outcome="out-of-memory",
                                      cap=1, budget=62 * 2**30)
    del s["fused_loads"]["tpch"]["all"]
    text = rendered(s)
    cells = row(text, TPCH, "all")
    assert cells[2].startswith("out of memory (cap 1)") and "62.0 GiB" in cells[2], cells
    assert cells[3] == ""
    assert not any("×" in cell for cell in cells), cells
    assert cells[4] == "10.0"  # the unfused sums still stand
    hints = row(text, TPCH_HINTS, "all")
    assert hints[1] == "1" and hints[3] == "—" and hints[7] == "—", hints
    split = [r for r in table(text, TPCH_SPLIT) if r[0] == "all"]
    assert [r[1] for r in split] == ["unfused"], split


def test_a_failed_fused_batch_shows_its_reason():
    s = spec()
    s["fused"]["tpch"]["all"] = "status=failed reason=prefill: needs 9 > limit 8\ncuda=12.2\n"
    del s["fused_loads"]["tpch"]["all"]
    cells = row(rendered(s), TPCH, "all")
    assert cells[2] == "failed: prefill: needs 9 > limit 8", cells
    assert not any("×" in cell for cell in cells), cells


def test_a_failed_duckdb_query_is_counted_out_and_named():
    s = spec()
    s["duckdb"]["tpch"]["q6"] = None
    text = rendered(s)
    cells = row(text, TPCH, "first3")
    assert cells[6].endswith("(2/3)"), cells
    assert cells[6].startswith("300.0"), cells
    assert cells[8] == cells[9] == "—", cells  # no DuckDB ratio over part of the batch
    assert row(text, TPCH, "all")[6].endswith("(3/4)")
    notes = section_after(text, TPCH).splitlines()
    assert any("DuckDB" in line and "q6" in line and "failed" in line for line in notes), notes


def test_a_counted_query_without_an_unfused_section_is_refused():
    s = spec()
    del s["unfused"]["tpch"]["q3"]
    code, err = refused(s)
    assert code == 1
    assert "q3" in err and "tp1_single.benchmark.txt" in err, err


def test_a_counted_query_with_a_failed_unfused_section_is_refused():
    s = spec()
    s["unfused"]["tpch"]["q14"] = "status=failed reason=oops\n"
    code, err = refused(s)
    assert code == 1 and "q14" in err, err


def test_a_counted_query_without_load_rows_is_refused():
    s = spec()
    del s["loads"]["tpcds"]["q2"]
    code, err = refused(s)
    assert code == 1 and "q2" in err and "loads.tsv" in err, err


def stale_lines(text, heading=TPCH):
    return [line for line in section_after(text, heading).splitlines() if "is stale" in line]


def test_nothing_is_stale_when_the_fused_runs_are_todays_prefixes():
    assert stale_lines(rendered()) == []


def test_a_fused_batch_in_another_order_is_named_stale_and_summed_as_run():
    s = spec()
    s["fused"]["tpch"]["first3"]["queries"] = ["q1", "q6", "q3"]
    text = rendered(s)
    assert stale_lines(text) == [
        "- first3 is stale: its fused run holds today's prefix in another order. Its numbers "
        "are the fused run's queries; rerun it on the H100."]
    assert row(text, TPCH, "first3")[4:6] == ["6.0", "1.5"]


def test_a_query_the_list_gained_names_the_batches_stale_rather_than_refusing():
    # An engine fix lets q2 run: today's list grows, the fused runs predate it, and q2 has no
    # timed unfused section yet. Each batch keeps the queries its fused run took.
    s = spec()
    s["cases"].insert(1, "tpch,40,q2,tp1_single,ok,")
    text = rendered(s)
    assert stale_lines(text) == [
        "- first3 is stale: today's prefix adds q2 and drops q6. Its numbers are the fused "
        "run's queries; rerun it on the H100.",
        "- all is stale: today's prefix adds q2. Its numbers are the fused run's queries; "
        "rerun it on the H100."]
    assert row(text, TPCH, "first3")[1:6] == ["3", "4.0", "1.4", "6.0", "1.5"]
    assert row(text, TPCH, "all")[1] == "4"
    with tempfile.TemporaryDirectory() as tmp:
        root = tree(tmp, s)
        report = pathlib.Path(tmp) / "page.md"
        report.write_text(f"# page\n{batching_report.BEGIN}\n{batching_report.END}\n")
        assert run_main(root, report) == (0, "")


def test_a_fused_marker_where_the_list_now_has_a_batch_is_named_stale():
    s = spec()
    s["cases"] += [f"tpch,40,q{n},tp1_single,ok," for n in (15, 16, 17, 18, 19, 20, 21)]
    text = rendered(s)
    assert row(text, TPCH, "first10")[1] == "same set as all"
    assert stale_lines(text)[0] == (
        "- first10 is stale: the fused file has it as the same set as all, and today's list "
        "makes it 10 queries. Its numbers are the fused run's queries; rerun it on the H100.")


def test_a_fused_batch_the_list_now_makes_a_marker_is_named_stale():
    s = spec()
    s["fused"]["tpch"]["first10"] = fused(["q1", "q3", "q6", "q14"], 7000, 1900)
    s["fused_loads"]["tpch"]["first10"] = s["fused_loads"]["tpch"]["all"]
    text = rendered(s)
    assert row(text, TPCH, "first10")[1] == "4"
    assert stale_lines(text)[0] == (
        "- first10 is stale: today's list (4 queries) makes it the same set as all. Its "
        "numbers are the fused run's queries; rerun it on the H100.")


def test_a_fused_query_without_an_unfused_section_is_refused():
    s = spec()
    s["fused"]["tpch"]["first3"]["queries"] = ["q1", "q3", "q99"]
    code, err = refused(s)
    assert code == 1 and "q99" in err and "tp1_single.benchmark.txt" in err, err


def test_a_missing_fused_section_is_refused():
    s = spec()
    del s["fused"]["tpcds"]["first20"]
    code, err = refused(s)
    assert code == 1 and "first20" in err and "tpcds" in err, err


def test_a_wrong_answer_query_is_counted_and_noted():
    text = rendered()
    notes = section_after(text, TPCH).splitlines()
    assert "- q6: counted, though its answer is wrong (#7); in first3, all." in notes, notes


# --- tp4_sized ---------------------------------------------------------------------------------


def test_tp4_columns_appear_only_with_their_file():
    header = table(rendered(), TPCH)[0]
    assert not any("tp4_sized" in cell for cell in header), header
    s = spec()
    s["tp4"]["tpch"] = {"q1": timed(900, 400), "q3": timed(1800, 600),
                        "q6": "status=failed reason=tpch/q6 at tp4-sized: refused\n",
                        "q14": timed(3500, 350)}
    text = rendered(s)
    header = table(text, TPCH)[0]
    assert header[6:8] == ["tp4_sized load (ms)", "tp4_sized execute (ms)"], header
    cells = row(text, TPCH, "first3")
    assert cells[6] == "2.7 (2/3)" and cells[7] == "1.0 (2/3)", cells
    assert any("tp4_sized" in line and "q6" in line
               for line in section_after(text, TPCH).splitlines())
    assert table(text, "### TPC-DS sf40: fused against unfused and DuckDB")[0] == table(
        rendered(), "### TPC-DS sf40: fused against unfused and DuckDB")[0]


# --- the hints and the provenance -------------------------------------------------------------


def test_the_hints_row_carries_both_estimates():
    s = spec()
    s["fused"]["tpch"]["first3"]["run"]["peak"] = 3 * 2**30
    text = rendered(s)
    assert row(text, TPCH_HINTS, "first3") == [
        "first3", "3", "10.0 / 60.0", "3.0", "0.0", "1", "1.0", "4.4", "1 (0)", "1 (0)"]
    body = section_after(text, TPCH_HINTS)
    assert "two or more queries" in body and "one loader and one executor" in body, body
    assert "no memory term" in body, body


def test_loads_count_each_table_read_and_the_rereads():
    s = spec()
    s["fused"]["tpch"]["first3"].update(reads="lineitem:2,orders:1", rereads=1)
    assert row(rendered(s), TPCH_HINTS, "first3")[9] == "3 (1)"


def test_folded_scans_count_a_querys_own_repeats_apart():
    s = spec()
    s["fused"]["tpch"]["first3"]["shared"] = "lineitem:q1+q3,region:q6+q6+q3"
    # lineitem folds 2 scans into 1 load; region 3 into 1, one of them q6 repeating itself.
    assert row(rendered(s), TPCH_HINTS, "first3")[8] == "3 (1)"
    body = section_after(rendered(s), TPCH_HINTS)
    assert "a query's own" in body and "a batch of one" in body, body


def test_provenance_lists_every_distinct_value():
    s = spec()
    s["unfused"]["tpch"]["q1"]["cudf"] = "25.02.01"
    cells = row(rendered(s), PROVENANCE_H100, "cudf")
    assert "25.02.01 (1)" in cells[1] and "25.02.02 (" in cells[1], cells


def test_the_largest_prefill_names_every_tie():
    s = spec()
    s["fused"]["tpch"]["first3"]["run"]["prefill"] = (8, 42442683465, 460368515)
    s["fused"]["tpch"]["all"]["run"]["prefill"] = (8, 42442683465, 20606716)
    cache = row(rendered(s), PROVENANCE_H100, "pinned host cache")[1]
    assert ("largest prefill 42442683465 B in 20606716-460368515 µs (tpch first3 fused, "
            "tpch all fused)") in cache, cache


def test_provenance_states_the_cache():
    s = spec()
    on = ("on cap=136.9GiB resident={} files=8 fills=8 fill_us=9 fallbacks=0 evictions=0 "
          "reloads=0 source=default")
    s["unfused"]["tpch"]["q1"].update(cache=on.format("12.3GiB"),
                                      prefill=(8, 13207024435, 812345))
    s["unfused"]["tpch"]["q3"].update(cache=on.format("14.0GiB"),
                                      prefill=(8, 15032385536, 901000))
    s["unfused"]["tpch"]["q9"] = timed(1, 1, cache="off source=env", prefill=(0, 0, 0))
    s["unfused"]["tpch"]["aggregate-groupby"]["cache"] = None  # a legacy section
    s["fused"]["tpch"]["all"]["run"]["prefill"] = (0, 0, 0)
    s["unfused"]["tpcds"]["q2"]["prefill"] = (0, 0, 0)
    s["unfused"]["tpcds"]["q3"]["cache"] = CACHE_ON.replace("reloads=0", "reloads=2")
    cells = row(rendered(s), PROVENANCE_H100, "pinned host cache")
    cache = cells[1]
    assert "on cap=136.9GiB source=default (2)" in cache, cache
    assert "off source=env (1)" in cache and "unknown (1)" in cache, cache
    assert "resident ≤ 14.0GiB" in cache, cache
    assert "15032385536 B in 901000 µs (tpch q3)" in cache, cache
    off = cache.split("cache not on: ")[1].split(";")[0]
    assert off == "tpch aggregate-groupby, tpch q9", cache
    unfilled = cache.split("tree sections not prefilled: ")[1].split(";")[0]
    assert unfilled == "tpcds q2", cache
    moved = cache.split("fell back, evicted or reloaded: ")[1].split(";")[0]
    assert moved == "tpcds q3 (reloads=2)", cache
    assert "fell back, evicted or reloaded: none" in row(rendered(), PROVENANCE_H100,
                                                         "pinned host cache")[1]


def test_provenance_holds_the_protocol_and_the_duckdb_run():
    text = rendered()
    protocol = section_after(text, "### Provenance: protocol")
    assert "2nd-minimum" in protocol and "prefilled" in protocol, protocol
    assert "all 3" in protocol and "first3 10" in protocol, protocol
    assert "6 runs (8)" in protocol, protocol  # DuckDB: 4 tpch + 3 tpcds + the micro query
    assert row(text, "### Provenance: DuckDB", "threads")[1] == "128 (2)"
    assert row(text, "### Provenance: DuckDB", "host")[1] == "cpu-vm (2)"
    assert row(text, "### Provenance: DuckDB", "floor_ms")[1] == "30 (1), 36 (1)"
    skipped = section_after(text, "### Provenance: skipped and wrong at sf40")
    assert "tpch q9 — skipped at sf40: timeout" in skipped, skipped


# --- the page ---------------------------------------------------------------------------------


def test_splice_replaces_only_between_markers():
    begin, end = batching_report.BEGIN, batching_report.END
    page = f"# title\nprose above\n{begin}\nold\n{end}\nprose below\n"
    spliced = batching_report.splice(page, "new\n")
    assert spliced == f"# title\nprose above\n{begin}\nnew\n{end}\nprose below\n"
    for broken in (f"# t\n{begin}\n", f"# t\n{end}\n", "# t\n"):
        with raises(ValueError, match="marker"):
            batching_report.splice(broken, "new\n")


def test_check_is_1_when_stale_and_0_after_writing():
    with tempfile.TemporaryDirectory() as tmp:
        root = tree(tmp)
        report = pathlib.Path(tmp) / "page.md"
        report.write_text(f"# page\n{batching_report.BEGIN}\n{batching_report.END}\nReading\n")
        code, err = run_main(root, report, "--check")
        assert code == 1 and "stale" in err, err
        assert run_main(root, report) == (0, "")
        assert run_main(root, report, "--check") == (0, "")
        assert report.read_text().endswith(f"{batching_report.END}\nReading\n")


def test_the_spec_is_not_changed_by_rendering():
    s = spec()
    before = copy.deepcopy(s)
    rendered(s)
    assert s == before


if __name__ == "__main__":
    raise SystemExit(main(globals()))
