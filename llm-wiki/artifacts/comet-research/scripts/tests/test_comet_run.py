"""Tests for comet_run.py's pure parts: the matrix, batches, prefix sums and conf.txt redaction.

Run: python3 -m pytest llm-wiki/artifacts/comet-research/scripts/tests
None of these needs Spark; `--matrix` must not import pyspark.
"""
import subprocess
import sys
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import comet_run  # noqa: E402

TPCH = [f"q{i}" for i in range(1, 23)]


def test_matrix_lists_the_nine_runs_in_priority_order():
    got = [(c.dataset, c.source, c.mode, c.batch, c.cache) for c in comet_run.matrix()]
    assert got == [
        ("tpch", "local", "sequential", "all", False),
        ("tpch", "local", "concurrent", "all", False),
        ("tpcds", "local", "sequential", "all", False),
        ("tpcds", "local", "concurrent", "all", False),
        ("tpch", "s3", "sequential", "all", False),
        ("tpch", "s3", "concurrent", "all", False),
        ("tpcds", "s3", "sequential", "all", False),
        ("tpch", "local", "sequential", "all", True),
        ("tpch", "local", "concurrent", "all", True),
    ]


def test_matrix_cli_prints_nine_rows_and_runs_nothing(tmp_path):
    out = subprocess.run(
        [sys.executable, str(SCRIPTS / "comet_run.py"), "--matrix", "--calibration",
         str(tmp_path / "absent.tsv")],
        capture_output=True, text=True, timeout=60, check=True,
    ).stdout
    rows = [l for l in out.splitlines() if l and not l.startswith("#") and not l.startswith("priority")]
    assert len(rows) == 9
    assert "pyspark" not in sys.modules


def test_matrix_estimates_come_from_calibration(tmp_path):
    cal = tmp_path / "calibration.tsv"
    cal.write_text("dataset\tquery\tms\n"
                   + "".join(f"tpch\t{q}\t60000\n" for q in TPCH)
                   + "".join(f"tpcds\t{q}\t1000\n" for q in comet_run.queries("tpcds", "all")))
    first = comet_run.matrix(cal)[0]
    # TPC-H local sequential: 2 measured runs of 22 minutes each, plus the start-up allowance.
    assert first.runs == 2
    assert first.estimate_min == 2 * 22 + comet_run.STARTUP_MIN


def write_runs(path, times):
    lines = ["query\trun\tms"]
    for run, ms in times.items():
        lines += [f"q{i}\t{run}\t{t}" for i, t in enumerate(ms, 1)]
        lines.append(f"batch\t{run}\t{sum(ms)}")
    lines.append("q1\tw1\t99999")  # a warm-up row never counts
    path.write_text("\n".join(lines) + "\n")


def test_prefix_sums_are_the_first_k_queries_summed(tmp_path):
    ms = [float(10 * i) for i in range(1, 23)]
    p = tmp_path / "runs.tsv"
    write_runs(p, {1: ms})
    assert comet_run.prefix_sums(p, [3, 10, 20]) == {3: sum(ms[:3]), 10: sum(ms[:10]), 20: sum(ms[:20])}


def test_prefix_sums_average_over_runs(tmp_path):
    p = tmp_path / "runs.tsv"
    write_runs(p, {1: [1.0] * 22, 2: [3.0] * 22})
    assert comet_run.prefix_sums(p, [3]) == {3: 6.0}
    assert comet_run.prefix_sums(p, [3], run=2) == {3: 9.0}


def test_tpch_batch_3_is_q1_q2_q3():
    assert comet_run.queries("tpch", "3") == ["q1", "q2", "q3"]
    assert comet_run.queries("tpch", "all") == TPCH


def test_tpcds_batch_3_is_the_first_three_lines_of_the_82():
    lines = (SCRIPTS / "tpcds-82.txt").read_text().split()
    assert len(lines) == 82
    assert comet_run.queries("tpcds", "3") == lines[:3]
    assert comet_run.queries("tpcds", "all") == lines


def test_every_listed_query_has_its_sql_file():
    for ds in ("tpch", "tpcds"):
        for q in comet_run.queries(ds, "all"):
            assert (comet_run.default_queries_dir(ds) / f"{q}.sql").is_file(), (ds, q)


def test_conf_txt_redacts_secret_keys(tmp_path):
    p = tmp_path / "conf.txt"
    comet_run.write_conf(p, {
        "spark.hadoop.fs.s3a.secret.key": "VERYSECRET",
        "spark.hadoop.fs.s3a.access.key": "AKIAEXAMPLE",
        "spark.app.name": "comet-tpch",
    }, {"host": "vm"})
    text = p.read_text()
    assert "spark.hadoop.fs.s3a.secret.key=<redacted>" in text
    assert "spark.hadoop.fs.s3a.access.key=<redacted>" in text
    assert "VERYSECRET" not in text and "AKIAEXAMPLE" not in text
    assert "spark.app.name=comet-tpch" in text
    assert "host=vm" in text


def test_statements_split_on_semicolons_and_backtick_double_quoted_names():
    text = ("-- a comment; with a semicolon\n"
            "SELECT count(*) AS \"order count\", 'it''s' AS s FROM t;\n"
            "SELECT 1;\n-- trailing comment\n")
    assert comet_run.statements(text) == [
        "-- a comment; with a semicolon\nSELECT count(*) AS `order count`, 'it''s' AS s FROM t",
        "SELECT 1",
    ]


def test_every_listed_query_gives_statements_with_no_double_quote_left():
    for ds in ("tpch", "tpcds"):
        for q in comet_run.queries(ds, "all"):
            stmts = comet_run.statements((comet_run.default_queries_dir(ds) / f"{q}.sql").read_text())
            assert stmts and all('"' not in s for s in stmts), (ds, q)
