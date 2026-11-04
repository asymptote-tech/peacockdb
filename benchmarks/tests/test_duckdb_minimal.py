"""`duckdb_minimal.sh` on the corpus: a device list's queries timed, answered and written as TSV.

Runs the real script against the real DuckDB CLI over the sf1 parquet under `testdata/`, linked
into a temporary `--dir`, so the import, the warm-up, the timing loop and the answers pass are the
ones the 128-core VM runs at sf40. Needs `duckdb` 1.5.4 on PATH (`scripts/env/install-duckdb.sh`)
and the generated sf1 tpch and tpcds; a missing one fails rather than skips, since a skipped file
reads as a passing one. Two timed runs per query instead of six, to keep it short.
"""
import atexit
import pathlib
import shutil
import socket
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.append(str(REPO / "scripts" / "calibration" / "tests"))
from harness import main  # noqa: E402

SCRIPT = REPO / "benchmarks" / "duckdb_minimal.sh"
DUCKDB = shutil.which("duckdb")
CASES = """\
dataset,sf,query,modes,status,note
tpch,40,q6,tp1_single,ok,
tpch,40,q1,tp1_single,wrong,#7
tpch,40,q9,,skipped,timeout: the case ran past 1800 s, and was killed
tpch,40,aggregate_groupby,tp1_single,ok,
tpcds,40,q37,tp1_single,ok,
"""
COLUMNS = "query\tms_2nd_min\truns_ms\tworkset_bytes\tdisk_read_bytes\twarm\tstatus"

_shared = None


def shared():
    """One data directory for the whole file, so tpch and tpcds are imported once."""
    global _shared
    if _shared is None:
        assert DUCKDB, "no duckdb on PATH: scripts/env/install-duckdb.sh --prefix <dir>, then PATH=<dir>/bin:$PATH"
        version = subprocess.run([DUCKDB, "--version"], capture_output=True, text=True, check=True).stdout
        assert version.startswith("v1.5.4 "), version
        _shared = pathlib.Path(tempfile.mkdtemp(prefix="duckdb-minimal-"))
        atexit.register(shutil.rmtree, _shared, True)
        linked(_shared)
    return _shared


def linked(directory):
    for ds in ("tpch", "tpcds"):
        source = REPO / "testdata" / f"{ds}.sf1"
        assert (source / ("lineitem.parquet" if ds == "tpch" else "store_sales.parquet")).is_file(), \
            f"no generated {source}: testdata/generate_testdata.sh"
        (directory / f"{ds}.sf1").symlink_to(source)
    return directory


def run(*args, directory=None):
    cmd = ["bash", str(SCRIPT), "--sf", "1", "--skip-download", "--runs", "2", "--duckdb", DUCKDB or "duckdb",
           "--dir", str(directory or shared()), *map(str, args)]
    return subprocess.run(cmd, capture_output=True, text=True, timeout=900)


def ok(r):
    assert r.returncode == 0, (r.returncode, r.stdout[-3000:], r.stderr[-3000:])
    return r


def cases(directory):
    path = directory / "cases.csv"
    path.write_text(CASES)
    return path


def results(path):
    lines = path.read_text().splitlines()
    preamble = dict(token.split("=", 1) for token in lines[0][2:].split(" "))
    return lines[0], preamble, lines[1], [dict(zip(lines[1].split("\t"), line.split("\t"))) for line in lines[2:]]


def sql(db, query):
    return subprocess.run([DUCKDB, str(db), "-csv", "-noheader", "-c", query], capture_output=True, text=True,
                          check=True).stdout.split()


def test_corpus_cases_time_every_line_in_order():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        r = ok(run("--cases", cases(tmp), "--results", tmp / "duckdb.tsv"))
        _, _, columns, rows = results(tmp / "duckdb.tsv")
        assert columns == COLUMNS, columns
        assert [row["query"] for row in rows] == ["q6", "q1", "aggregate_groupby"], rows
        for row in rows:
            assert row["status"] == "ok", row
            runs = [int(ms) for ms in row["runs_ms"].split(",")]
            assert len(runs) == 2 and int(row["ms_2nd_min"]) == sorted(runs)[1], row
            assert row["warm"] in ("yes", "partial", "NO") and int(row["disk_read_bytes"]) >= 0, row
            assert int(row["workset_bytes"]) > 0, row
        assert "q9" not in r.stdout.split("=== results ===")[1], r.stdout
        assert r.stdout.count("no goldens") == 1, r.stdout


def test_answers_hold_one_table_per_query():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        answers = tmp / "answers-tpch.duckdb"
        ok(run("--cases", cases(tmp), "--answers", answers, "--results", tmp / "duckdb.tsv"))
        tables = sql(answers, "SELECT table_name FROM duckdb_tables() ORDER BY 1")
        assert tables == ["aggregate-groupby", "q1", "q6"], tables
        assert sql(answers, 'SELECT count(*) FROM "q1"') == ["4"]
        flags = sql(answers, 'SELECT l_returnflag || l_linestatus FROM "q1" ORDER BY rowid')
        assert flags == ["AF", "NF", "NO", "RF"], flags


def test_answers_take_the_oracle_s_integer_division():
    """`integer_division` is a parser setting: in the `-c` that holds the query it is read too
    late, and the answer is real division where the oracle's is integer. tpcds q61's unaliased
    `/` is named `//` when it took effect, as `testdata/goldens/tpcds.sf1/duckdb-result.txt` has it."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        listed = tmp / "cases.csv"
        listed.write_text("dataset,sf,query,modes,status,note\ntpcds,40,q61,tp1_single,ok,\n")
        answers = tmp / "answers-tpcds.duckdb"
        ok(run("--dataset", "tpcds", "--cases", listed, "--answers", answers))
        names = sql(answers, "SELECT column_name FROM duckdb_columns() WHERE table_name = 'q61'")
        assert any("//" in name for name in names), names


def test_results_name_version_threads_host():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        ok(run("--cases", cases(tmp), "--results", tmp / "duckdb.tsv"))
        first, preamble, _, _ = results(tmp / "duckdb.tsv")
        assert first.startswith("# duckdb=1.5.4 threads="), first
        assert list(preamble) == ["duckdb", "threads", "host", "cpu", "cores", "memory_gib", "db",
                                  "resident_pct"], preamble
        threads = subprocess.run([DUCKDB, "-csv", "-noheader", "-c", "SELECT current_setting('threads')"],
                                 capture_output=True, text=True, check=True).stdout.strip()
        assert preamble["threads"] == threads, (preamble, threads)
        assert preamble["host"] == socket.gethostname(), preamble
        assert preamble["db"] == str(shared() / "tpch.sf1.duckdb"), preamble
        assert " " not in preamble["cpu"] and preamble["cpu"], preamble
        ok(run("--cases", cases(tmp), "--results", tmp / "four.tsv", "--threads", "4"))
        assert results(tmp / "four.tsv")[1]["threads"] == "4"


def test_results_have_the_fixtures_shape():
    """`parse_duckdb_results` (test_support) owns the format and is tested over
    `testdata/fixtures/duckdb-results.tsv`; what the script writes has that file's keys and columns."""
    fixture = (REPO / "testdata" / "fixtures" / "duckdb-results.tsv").read_text().splitlines()
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        ok(run("--cases", cases(tmp), "--results", tmp / "duckdb.tsv"))
        written = (tmp / "duckdb.tsv").read_text().splitlines()
    keys = [[token.split("=", 1)[0] for token in lines[0][2:].split(" ")] for lines in (written, fixture)]
    assert written[0].startswith("# ") and keys[0] == keys[1], keys
    assert written[1] == fixture[1], (written[1], fixture[1])
    for line in written[2:]:
        assert len(line.split("\t")) == len(fixture[1].split("\t")), line


def test_tpcds_tables_from_the_parquet():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        r = ok(run("--dataset", "tpcds", "--cases", cases(tmp), "--results", tmp / "duckdb.tsv"))
        stems = sorted(p.stem for p in (REPO / "testdata" / "tpcds.sf1").glob("*.parquet"))
        assert sql(shared() / "tpcds.sf1.duckdb", "SELECT table_name FROM duckdb_tables() ORDER BY 1") == stems
        assert [row["query"] for row in results(tmp / "duckdb.tsv")[3]] == ["q37"]
        assert "TPC-DS sf1" in r.stdout, r.stdout[:500]


def test_answers_without_cases_is_2():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        for args in (["--answers", tmp / "a.duckdb"],
                     ["--cases", cases(tmp), "--answers", tmp / "a.duckdb", "--skip-verify"],
                     ["--dataset", "tpcds"],
                     ["--dataset", "ssb", "--build-only"],
                     ["--cases", tmp / "missing.csv"]):
            r = run(*args, directory=tmp)
            assert r.returncode == 2, (args, r.returncode, r.stdout[-500:], r.stderr)
        assert not (tmp / "a.duckdb").exists()
        assert not list(tmp.glob("*.sf1.duckdb"))


def test_build_only_runs_no_query():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = linked(pathlib.Path(tmp))
        r = ok(run("--dataset", "tpcds", "--build-only", directory=tmp))
        assert "row counts verified against the parquet" in r.stdout, r.stdout[-2000:]
        assert "=== timing" not in r.stdout and "=== page cache" not in r.stdout, r.stdout[-2000:]
        assert (tmp / "tpcds.sf1.duckdb").is_file()
        for extra in (["--cases", cases(tmp)], ["--answers", tmp / "a.duckdb"], ["--results", tmp / "r.tsv"]):
            r = run("--dataset", "tpcds", "--build-only", *extra, directory=tmp)
            assert r.returncode == 2, (extra, r.returncode, r.stderr)


def test_default_run_is_unchanged():
    r = ok(run())
    assert "=== duckdb_minimal.sh — TPC-H sf1 on native DuckDB storage ===" in r.stdout, r.stdout[:300]
    table = r.stdout.split("=== results ===")[1]
    named = [line.split()[0] for line in table.splitlines() if line[:1] == "q" and not line.startswith("query ")]
    assert named == ["q6", "q1", "q3", "q8", "q11v/img_000", "q11v/img_017", "q11v/img_034", "q12v/txt_000",
                     "q10v/txt_017", "q9v/txt_034"], named
    assert "=== verify against committed goldens (threads=1) ===" in r.stdout


if __name__ == "__main__":
    raise SystemExit(main(globals()))
