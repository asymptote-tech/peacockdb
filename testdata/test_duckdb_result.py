#!/usr/bin/env python3
"""`duckdb_result.py`'s rendering, held to arrow-rs's (#235). No DuckDB and no parquet.

Two things only this side can get wrong. A timestamp: arrow-rs prints `NaiveDateTime`'s
`Debug` form and `isoformat()` does not, so a millisecond timestamp would be a false
divergence on every row that carries one. And the over-cap fingerprint, which both result
writers produce: the expected text here is what `test_golden_format.rs`'s
`an_exact_column_is_hashed_and_an_approximate_one_is_summed` pins on the Rust side, so a
change to either writer that does not reach the other goes red here.

Run: python3 testdata/test_duckdb_result.py
"""

import datetime
import decimal
import os
import sys
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
    """The text `test_golden_format.rs` pins on the Rust side, byte for byte."""

    RUST = (
        "fingerprint: rows=2\n"
        "col 0: nonnull=2\n"
        "col 1: nonnull=2\n"
        "col 2: nonnull=2 sum=4.00000000000000000e0 min=1.50000000000000000e0"
        " max=2.50000000000000000e0\n"
        "hash: a6005c86bd5307686acd561b309b2b6d5c670c935fe527ca4ad10023851dd239\n"
    )

    def test_both_writers_fingerprint_the_same_rows_the_same_way(self):
        rows = [(1, "a", 1.5), (2, "b", 2.5)]
        self.assertEqual(dr.fingerprint(["id", "s", "x"], rows), self.RUST)

    def test_the_row_order_does_not_reach_the_fingerprint(self):
        rows = [(2, "b", 2.5), (1, "a", 1.5)]
        self.assertEqual(dr.fingerprint(["id", "s", "x"], rows), self.RUST)

    def test_rows_paired_differently_differ_in_the_hash(self):
        swapped = dr.fingerprint(["id", "s", "x"], [(1, "b", 1.5), (2, "a", 2.5)])
        self.assertNotEqual(swapped, self.RUST)
        self.assertEqual(swapped.splitlines()[:4], self.RUST.splitlines()[:4])

    def test_a_null_is_counted_out_of_its_column(self):
        text = dr.fingerprint(["x"], [(1.5,), (None,), (2.5,)])
        self.assertEqual(text.splitlines()[0], "fingerprint: rows=3")
        self.assertEqual(
            text.splitlines()[1],
            "col 0: nonnull=2 sum=4.00000000000000000e0 min=1.50000000000000000e0"
            " max=2.50000000000000000e0",
        )

    def test_an_integer_column_is_hashed_and_not_summed(self):
        text = dr.fingerprint(["n"], [(1,), (2,)])
        self.assertEqual(text.splitlines()[1], "col 0: nonnull=2")

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


if __name__ == "__main__":
    unittest.main()
