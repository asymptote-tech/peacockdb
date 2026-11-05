#!/usr/bin/env python3
"""`duckdb_result.py`'s rendering, held to arrow-rs's (#235). No DuckDB and no parquet, but for
`Answers`, which needs both and is skipped without them (CI has neither).

Two things only this side can get wrong. A timestamp: arrow-rs prints `NaiveDateTime`'s
`Debug` form and `isoformat()` does not, so a millisecond timestamp would be a false
divergence on every row that carries one. And the over-cap fingerprint, which both result
writers produce: the expected text here, the hash included, is what `test_golden_format.rs`'s
`an_exact_column_is_hashed_and_an_approximate_one_is_summed` pins on the Rust side, so a
change to either writer that does not reach the other goes red here.

Run: python3 testdata/test_duckdb_result.py
"""

import datetime
import decimal
import importlib.util
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import duckdb_result as dr  # noqa: E402


class Cells(unittest.TestCase):
    def test_a_timestamp_renders_as_arrow_rs_prints_it(self):
        t = datetime.datetime(2024, 1, 1, 0, 0, 0)
        self.assertEqual(dr.cell(t), "2024-01-01T00:00:00")
        self.assertEqual(dr.cell(t.replace(microsecond=1000)), "2024-01-01T00:00:00.001")
        self.assertEqual(dr.cell(t.replace(microsecond=1500)), "2024-01-01T00:00:00.001500")
        self.assertEqual(dr.cell(t.replace(microsecond=999999)), "2024-01-01T00:00:00.999999")

    def test_a_date_is_not_a_timestamp(self):
        self.assertEqual(dr.cell(datetime.date(2024, 1, 2)), "2024-01-02")

    def test_the_other_cells_keep_their_rendering(self):
        self.assertEqual(dr.cell(None), "")
        self.assertEqual(dr.cell(True), "true")
        self.assertEqual(dr.cell(False), "false")
        self.assertEqual(dr.cell(1.5), "1.5")
        self.assertEqual(dr.cell(decimal.Decimal("25.522005")), "25.522005")
        self.assertEqual(dr.cell(7), "7")


class Fingerprint(unittest.TestCase):
    """The text `test_golden_format.rs` pins on the Rust side, byte for byte — the
    `hash:` line included, which is the field carrying the row pairing."""

    TYPES = ["BIGINT", "VARCHAR", "DOUBLE"]

    RUST = (
        "fingerprint: rows=2\n"
        "col 0: nonnull=2\n"
        "col 1: nonnull=2\n"
        "col 2: nonnull=2 sum=4.00000000000000000e0 min=1.50000000000000000e0"
        " max=2.50000000000000000e0\n"
        "hash: 55d02283b07cc29ea0d3abeea4a1938ba4843ad8966bf6ee14b61e8d4b9f18b3\n"
    )

    def test_both_writers_fingerprint_the_same_rows_the_same_way(self):
        rows = [(1, "a", 1.5), (2, "b", 2.5)]
        self.assertEqual(dr.fingerprint(["id", "s", "x"], self.TYPES, rows), self.RUST)

    def test_the_row_order_does_not_reach_the_fingerprint(self):
        rows = [(2, "b", 2.5), (1, "a", 1.5)]
        self.assertEqual(dr.fingerprint(["id", "s", "x"], self.TYPES, rows), self.RUST)

    def test_rows_paired_differently_differ_in_the_hash(self):
        swapped = dr.fingerprint(["id", "s", "x"], self.TYPES, [(1, "b", 1.5), (2, "a", 2.5)])
        self.assertNotEqual(swapped, self.RUST)
        self.assertEqual(swapped.splitlines()[:4], self.RUST.splitlines()[:4])

    def test_a_null_is_counted_out_of_its_column(self):
        text = dr.fingerprint(["x"], ["DOUBLE"], [(1.5,), (None,), (2.5,)])
        self.assertEqual(text.splitlines()[0], "fingerprint: rows=3")
        self.assertEqual(
            text.splitlines()[1],
            "col 0: nonnull=2 sum=4.00000000000000000e0 min=1.50000000000000000e0"
            " max=2.50000000000000000e0",
        )

    def test_an_integer_column_is_hashed_and_not_summed(self):
        text = dr.fingerprint(["n"], ["BIGINT"], [(1,), (2,)])
        self.assertEqual(text.splitlines()[1], "col 0: nonnull=2")

    def test_a_decimal_is_hashed_and_a_double_is_summed(self):
        """The declaration decides, not the value: both render with a point."""
        rows = [(decimal.Decimal("1.50"), 1.5)]
        text = dr.fingerprint(["d", "f"], ["DECIMAL(15,2)", "DOUBLE"], rows)
        self.assertEqual(text.splitlines()[1], "col 0: nonnull=1")
        self.assertTrue(text.splitlines()[2].startswith("col 1: nonnull=1 sum="))

    def test_an_all_null_double_column_is_approximate_by_its_declaration(self):
        """Classed off the rows it would be exact here and approximate on the Rust side,
        which reads the arrow type, and no section could ever pass.

        The text is what `triple_of` leaves for a float column holding no value: an empty
        sum, and a min and max that are the absent value.
        """
        text = dr.fingerprint(["x"], ["DOUBLE"], [(None,), (None,)])
        self.assertEqual(
            text.splitlines()[1], "col 0: nonnull=0 sum=0.00000000000000000e0 min=nan max=nan"
        )

    def test_a_column_type_neither_list_classes_is_refused(self):
        """Exhaustive rather than defaulted: a type classed silently by a fallback is a
        column the two writers would hash differently with nothing going red."""
        with self.assertRaises(ValueError) as caught:
            dr.fingerprint(["x"], ["STRUCT(a INTEGER)"], [(None,)])
        self.assertIn("STRUCT", str(caught.exception))

    def test_two_answers_a_separator_in_a_cell_would_merge_hash_differently(self):
        """`("a|b", "c")` and `("a", "b|c")` join to one string under a `|` separator and
        so hash to one digest: two different answers agreeing, on the comparison that has
        no second opinion behind it. `\x01` cannot occur in a rendered cell."""
        names, types = ["l", "r"], ["VARCHAR", "VARCHAR"]
        left = dr.fingerprint(names, types, [("a|b", "c")])
        right = dr.fingerprint(names, types, [("a", "b|c")])
        self.assertNotEqual(left, right)
        self.assertEqual(left.splitlines()[:3], right.splitlines()[:3])

    def test_a_nan_settles_the_triple_whatever_the_row_order(self):
        """`sorted()` leaves a NaN wherever the rows put it and Rust's `partial_cmp` arm does
        the same, so min and max were whatever each side's row order made them. A NaN
        anywhere makes the whole triple NaN on both writers; `fingerprint.rs`'s `triple_of`
        is the other half."""
        nan = float("nan")
        one = dr.fingerprint(["x"], ["DOUBLE"], [(1.0,), (nan,), (2.0,)])
        other = dr.fingerprint(["x"], ["DOUBLE"], [(nan,), (2.0,), (1.0,)])
        self.assertEqual(one, other)
        self.assertEqual(one.splitlines()[1], "col 0: nonnull=3 sum=nan min=nan max=nan")

    def test_the_exponent_is_written_as_rust_writes_it(self):
        """Read off `format!("{x:.17e}")` for each value, including the two extremes."""
        self.assertEqual(dr.approx_number(0.0), "0.00000000000000000e0")
        self.assertEqual(dr.approx_number(-0.0), "-0.00000000000000000e0")
        self.assertEqual(dr.approx_number(-1.5), "-1.50000000000000000e0")
        self.assertEqual(dr.approx_number(1e17), "1.00000000000000000e17")
        self.assertEqual(dr.approx_number(1.23e-5), "1.23000000000000008e-5")
        self.assertEqual(dr.approx_number(1 / 3), "3.33333333333333315e-1")
        self.assertEqual(dr.approx_number(1e308), "1.00000000000000001e308")
        self.assertEqual(dr.approx_number(5e-324), "4.94065645841246544e-324")
        self.assertEqual(dr.approx_number(float("inf")), "inf")
        self.assertEqual(dr.approx_number(float("-inf")), "-inf")
        self.assertEqual(dr.approx_number(float("nan")), "nan")


class RowOrder(unittest.TestCase):
    def test_render_keeps_order_when_asked(self):
        """A query whose line says `no_sort` keeps DuckDB's row order; any other is sorted."""
        rows = [(2, "b"), (1, "a")]
        kept = dr.render(["k", "v"], rows, keep_order=True).splitlines()
        self.assertEqual([kept[3], kept[4]], ["| 2 | b |", "| 1 | a |"])
        sorted_ = dr.render(["k", "v"], rows).splitlines()
        self.assertEqual([sorted_[3], sorted_[4]], ["| 1 | a |", "| 2 | b |"])

    def test_a_quoted_oracle_cell_is_one_field(self):
        """The registry's quoted spelling reads whole, so q61's `no_sort` is seen and a
        comma inside its column name does not shift the columns; q2's `no_sort` is read past
        its `gpu: fingerprint` part."""
        name = "promotional_sales.promotions / all_sales.total * Int64(100)"
        spelled = f'duckdb(no_sort, rel(1e-5, "{name}"))'
        quoted = '"' + spelled.replace('"', '""') + '"'
        with tempfile.TemporaryDirectory() as scratch:
            registry = pathlib.Path(scratch) / "cost-registry.csv"
            registry.write_text(
                "dataset,sf,query,tickets,stored,duckdb_oracle,cpu_oracle\n"
                f'tpcds,1,q61,288,result,{quoted},"data_fusion(sort, exact)"\n'
                "tpcds,1,q1,,result,duckdb(sort, exact),na\n"
                "tpcds,1,q2,,result|result|fingerprint,"
                "\"duckdb(no_sort, exact, gpu: fingerprint)\",\"data_fusion(sort, exact)\"\n"
                "tpch,1,q3,,result,\"duckdb(no_sort, exact)\",na\n")
            self.assertEqual(dr.ordered_queries("tpcds", registry), {"q61", "q2"})


class QueryFiles(unittest.TestCase):
    """The one thing an empty answer file cannot be told apart from: a dataset whose query
    directory is missing or holds nothing. `generate` used to write the golden from whatever
    the glob returned, so `--dataset pbench` over an empty `pbench-queries/` wrote an EMPTY
    `duckdb-result.txt` — a golden every later comparison reads as "DuckDB answers nothing",
    which is a worse error than argparse refusing the name."""

    def test_the_three_datasets_resolve_their_queries(self):
        """The two benchmarks are a fixed set; pbench grows as the join-rewrite chain lands its
        queries, so it gets a floor rather than a count nobody would remember to update."""
        for dataset, count in [("tpch", 48), ("tpcds", 99)]:
            self.assertEqual(len(dr.query_files(dataset, None)), count, dataset)
        pbench = [q.stem for q in dr.query_files("pbench", None)]
        self.assertGreater(len(pbench), 50)
        self.assertIn("bool-key-group", pbench)

    def test_the_queries_that_panic_the_planner_are_not_in_the_tree(self):
        """#255: the planner panics on a Struct or Interval column, and a query file here is read
        by every plan-golden renderer — so one of these three in the directory aborts all five of
        pbench's plan goldens rather than landing with a ticket on its row. They arrive with the
        fix; this is what says so to whoever wonders where they went."""
        pbench = {q.stem for q in dr.query_files("pbench", None)}
        for held in ["struct-key-join", "struct-through-join", "interval-through-join"]:
            self.assertNotIn(held, pbench, f"{held} panics the planner (#255)")

    def test_a_dataset_with_no_query_directory_is_refused(self):
        with self.assertRaises(SystemExit) as caught:
            dr.query_files("nosuchbench", None)
        self.assertIn("nosuchbench-queries", str(caught.exception))

    def test_a_query_directory_holding_no_sql_is_refused(self):
        import tempfile
        with tempfile.TemporaryDirectory() as scratch:
            (pathlib.Path(scratch) / "hollow-queries").mkdir()
            with self.assertRaises(SystemExit) as caught:
                dr.query_files("hollow", None, root=pathlib.Path(scratch))
            self.assertIn("no .sql", str(caught.exception))

    def test_an_only_filter_that_names_nothing_is_refused(self):
        """`generate` rewrites the whole golden from the queries it ran, so an `--only` that
        matches no query would replace the file with one holding no sections."""
        with self.assertRaises(SystemExit) as caught:
            dr.query_files("tpch", {"q4242"})
        self.assertIn("q4242", str(caught.exception))

    def test_the_queries_come_back_in_the_goldens_order(self):
        names = [q.stem for q in dr.query_files("tpch", None)]
        self.assertEqual(names[:3], ["q1", "q2", "q3"])
        self.assertEqual(names[-1], "shuffle-stddev")


HAS_DUCKDB = importlib.util.find_spec("duckdb") is not None
NEEDS_DUCKDB = unittest.skipUnless(
    HAS_DUCKDB and (dr.ROOT / "tpch.sf1").is_dir(),
    "needs the duckdb 1.5.4 wheel and testdata/tpch.sf1 (dev, the build image); CI has neither",
)


def answers_db(path, dataset, stems):
    """An answers database as `duckdb_minimal.sh --answers` writes one: each query's answer a
    table named by its stem, created under the oracle's session rules."""
    import duckdb

    con = duckdb.connect()
    con.execute("SET threads=1")
    con.execute("SET default_null_order='nulls_last_on_asc_first_on_desc'")
    con.execute("SET integer_division=true")
    for table in sorted((dr.ROOT / f"{dataset}.sf1").glob("*.parquet")):
        con.execute(f"CREATE VIEW {table.stem} AS SELECT * FROM read_parquet('{table}')")
    con.execute(f"ATTACH '{path}' AS answers")
    for stem in stems:
        sql = dr.duckdb_text((dr.ROOT / f"{dataset}-queries" / f"{stem}.sql").read_text())
        con.execute(f'CREATE OR REPLACE TABLE answers."{stem}" AS {sql.strip().rstrip(";")}')
    con.close()


class Answers(unittest.TestCase):
    """sf40's answers come from an answers database rather than from parquet: DuckDB at sf40
    runs on the 128-core VM, and this renders what it stored."""

    @NEEDS_DUCKDB
    def test_answers_render_as_the_parquet_path_does(self):
        """q1 is a `no_sort` line, so its rows keep DuckDB's order through the table."""
        with tempfile.TemporaryDirectory() as scratch:
            db = pathlib.Path(scratch) / "answers.duckdb"
            answers_db(db, "tpch", ["q1", "q3"])
            got = dr.answer_sections("tpch", db, None)
        want = dr.parquet_sections("tpch", {"q1", "q3"})
        self.assertTrue(want.startswith("== q1\n+"), want[:80])
        self.assertEqual(got, want)

    @NEEDS_DUCKDB
    def test_a_missing_answer_is_failed(self):
        with tempfile.TemporaryDirectory() as scratch:
            db = pathlib.Path(scratch) / "answers.duckdb"
            answers_db(db, "tpch", ["q1"])
            text = dr.answer_sections("tpch", db, {"q1", "q3"})
        self.assertIn("== q3\nfailed: no answer recorded\n", text)
        self.assertTrue(text.startswith("== q1\n+"), text[:80])

    @NEEDS_DUCKDB
    def test_an_answers_database_holding_no_answer_is_refused(self):
        """An empty golden reads as "DuckDB answers nothing" to every comparison."""
        import duckdb

        with tempfile.TemporaryDirectory() as scratch:
            db = pathlib.Path(scratch) / "answers.duckdb"
            duckdb.connect(str(db)).close()
            with self.assertRaises(SystemExit) as caught:
                dr.answer_sections("tpch", db, None)
        self.assertIn("no table", str(caught.exception))

    @NEEDS_DUCKDB
    def test_answers_are_written_beside_the_sf40_goldens(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = pathlib.Path(scratch)
            db = root / "answers.duckdb"
            answers_db(db, "tpch", ["q6"])
            dr.generate("tpch", "40", None, answers=db, root=root)
            text = (root / "goldens/tpch.sf40/duckdb-result.txt").read_text()
        self.assertTrue(text.startswith("== q6\n+"), text)

    def test_a_streamed_fingerprint_is_the_in_memory_one(self):
        """An answer too large to hold, fingerprinted batch by batch through an outside sort,
        writes the same text: nulls, a float column, a non-ASCII cell and the separator's
        neighbours included."""
        names = ["id", "s", "x"]
        types = ["BIGINT", "VARCHAR", "DOUBLE"]
        rows = [(3, "z", 0.5), (1, "é", None), (2, None, 2.5), (10, "a\x02", -1.0), (1, "é", 7.0)]
        batches = [rows[:2], rows[2:3], rows[3:]]
        self.assertEqual(dr.fingerprint_stream(names, types, iter(batches)),
                         dr.fingerprint(names, types, rows))
        self.assertEqual(dr.fingerprint_stream(names, types, iter([])),
                         dr.fingerprint(names, types, []))

    @NEEDS_DUCKDB
    def test_an_answer_over_the_cap_by_its_row_count_is_fingerprinted_unrendered(self):
        """At sf40 an answer can be 96M rows (tpch filter-project): rendered first and measured
        after, it outgrows the host. Its row count already puts it over the cap."""
        import duckdb

        with tempfile.TemporaryDirectory() as scratch:
            db = pathlib.Path(scratch) / "answers.duckdb"
            con = duckdb.connect(str(db))
            con.execute('CREATE TABLE "big" AS SELECT range AS k, (range % 7)::DECIMAL(15,2) AS q '
                        "FROM range(30000)")
            want = dr.fingerprint(*dr.described(con.execute('SELECT * FROM "big" ORDER BY rowid')))
            con.close()
            render = dr.render
            dr.render = lambda *a, **k: self.fail("an answer over the cap was rendered")
            try:
                text = dr.answer_sections("tpch", db, None)
            finally:
                dr.render = render
        self.assertEqual(text, "== big\n" + want)

    def test_answers_need_a_scale(self):
        """`--answers` is sf40's source and parquet is sf1's: each without the other's scale, and
        an answers file for more than one dataset, is refused before DuckDB is asked anything."""
        script = str(dr.ROOT / "duckdb_result.py")
        cases = [
            (["--dataset", "tpch", "--answers", "a.duckdb"], ["--answers", "--sf"]),
            (["--dataset", "tpch", "--sf", "1", "--answers", "a.duckdb"], ["--answers", "--sf 1"]),
            (["--dataset", "tpch", "--sf", "40"], ["--sf 40", "--answers"]),
            (["--sf", "40", "--answers", "a.duckdb"], ["--answers", "one --dataset"]),
        ]
        for argv, words in cases:
            r = subprocess.run([sys.executable, "-I", script, *argv], capture_output=True, text=True,
                               timeout=60)
            self.assertEqual(r.returncode, 2, (argv, r.stderr))
            for word in words:
                self.assertIn(word, r.stderr, (argv, r.stderr))


if __name__ == "__main__":
    unittest.main()
