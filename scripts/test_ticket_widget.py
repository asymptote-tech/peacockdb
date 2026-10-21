#!/usr/bin/env python3
"""`ticket_widget.py`'s reading of the registry — a quoted oracle cell, a registry older than
the `duckdb_oracle` or `stored` column — and the four declaration columns it renders from it:
q2's split cells and what each header says. No cargo, no cost-report.

Run: python3 scripts/test_ticket_widget.py
"""

import os
import pathlib
import re
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import ticket_widget as tw  # noqa: E402

MODES = ",".join(f"{g}{m}" for g in ("", "cpu_", "gpu_")
                 for m in ("tp1_single", "tp1_rowgroup", "tp4_single", "tp4_rowgroup", "tp4_sized"))
STATES = ",".join(["enabled"] * 15)
Q61 = 'duckdb(no_sort, rel(1e-5, "promotional_sales.promotions / all_sales.total * Int64(100)"))'
Q2 = "duckdb(no_sort, exact, gpu: fingerprint)"


def tree(scratch: str, header_tail: str, row_tail: str) -> pathlib.Path:
    root = pathlib.Path(scratch)
    (root / "testdata").mkdir()
    (root / "testdata" / "cost-registry.csv").write_text(
        f"dataset,sf,query,{MODES},plan_status,features,tickets,{header_tail}\n"
        f"tpcds,1,q61,{STATES},ok,avg,,{row_tail}\n")
    return root


class Registry(unittest.TestCase):
    def test_a_quoted_oracle_cell_is_one_field(self):
        quoted = '"' + Q61.replace('"', '""') + '"'
        with tempfile.TemporaryDirectory() as scratch:
            root = tree(scratch, "stored,duckdb_oracle,cpu_oracle,gpu_oracle,schema_validation",
                        f'result,{quoted},"data_fusion(sort, exact)","golden(no_sort, exact)",'
                        "enabled")
            decl = tw.declarations(root)[("tpcds", "q61")]
        self.assertEqual(decl, ("result", Q61, "data_fusion(sort, exact)",
                                "golden(no_sort, exact)", "enabled"))
        oracles, validation = tw.declared_cells(decl)
        self.assertEqual(oracles.count("<td"), 4)
        self.assertIn(">rel 1e-5<", oracles)
        self.assertIn("promotional_sales.promotions / all_sales.total * Int64(100)", oracles)
        self.assertIn(">enabled<", validation)

    def test_a_registry_without_duckdb_oracle_still_renders(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = tree(scratch, "cpu_oracle,gpu_oracle,schema_validation",
                        "data_fusion_exact,golden_exact,enabled")
            decl = tw.declarations(root)[("tpcds", "q61")]
        self.assertEqual(decl, (None, None, "data_fusion_exact", "golden_exact", "enabled"))
        oracles, _ = tw.declared_cells(decl)
        cells = oracles.split("</td>")[:4]
        self.assertEqual(len(cells), 4)
        self.assertIn(">—", cells[0])
        self.assertIn(">—", cells[1])
        self.assertIn(">exact", cells[2])

    def test_a_registry_without_stored_still_renders(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = tree(scratch, "duckdb_oracle,cpu_oracle,gpu_oracle,schema_validation",
                        '"duckdb(sort, exact)","data_fusion(sort, exact)",'
                        '"golden(no_sort, exact)",enabled')
            decl = tw.declarations(root)[("tpcds", "q61")]
        self.assertEqual(decl, (None, "duckdb(sort, exact)", "data_fusion(sort, exact)",
                                "golden(no_sort, exact)", "enabled"))
        cells = tw.declared_cells(decl)[0].split("</td>")[:4]
        self.assertIn(">—", cells[0])
        self.assertIn(">sort exact", cells[1])
        self.assertIn(">sort exact", cells[2])
        self.assertIn(">no_sort exact", cells[3])

    def test_the_fallback_reads_a_line_on_the_grammar(self):
        """A tree whose registry has no oracle columns reads them off the include."""
        line = (f"corpus_query!(tpcds, 1, q61, all_modes, all_modes, {Q61}, "
                "data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);\n")
        with tempfile.TemporaryDirectory() as scratch:
            root = pathlib.Path(scratch)
            inc = root / "peacockdb-core" / "tests" / "common"
            inc.mkdir(parents=True)
            (inc / "corpus_cases.inc").write_text("// a comment (with parens)\n" + line)
            decl = tw.declarations(root)[("tpcds", "q61")]
        self.assertEqual(decl, (None, Q61, "data_fusion(sort, exact)", "golden(no_sort, exact)",
                                "enabled"))

    def test_no_declaration_renders_four_dashes(self):
        """The rows with no corpus line keep the table's shape: four cells, not three."""
        oracles, validation = tw.declared_cells(None)
        self.assertEqual(oracles.count("<td"), 4)
        self.assertEqual(oracles.count(">—</td>"), 4)
        self.assertIn(">—</td>", validation)


class Columns(unittest.TestCase):
    """The four declaration columns after Query, and the two cells q2 divides."""

    def test_q2s_two_cells_split(self):
        decl = ("result|result|fingerprint", Q2, "data_fusion(sort, exact)",
                "golden(no_sort, approx_std)", "enabled")
        oracles, _ = tw.declared_cells(decl)
        cells = oracles.split("</td>")[:4]
        self.assertEqual(oracles.count('class="decl split"'), 2)
        stored, duck = cells[0], cells[1]
        self.assertIn('class="decl split"', stored)
        self.assertEqual(stored.count('class="half"'), 3)
        for label, short in (("cpu", "result"), ("DuckDB", "result"), ("gpu", "fingerprint")):
            self.assertIn(f"<sub>{label}</sub> {short}</span>", stored)
        self.assertIn('class="decl split"', duck)
        self.assertEqual(duck.count('class="half"'), 2)
        self.assertIn("<sub>cpu</sub> no_sort exact</span>", duck)
        self.assertIn("<sub>gpu</sub> fingerprint</span>", duck)
        halves = dict(re.findall(r'<span class="half" title="([^"]*)"><sub>(\w+)</sub>', duck))
        titles = {label: title for title, label in halves.items()}
        self.assertIn("SHA-256", titles["gpu"])
        self.assertNotIn("SHA-256", titles["cpu"])
        self.assertNotIn("split", cells[2] + cells[3])

    def test_split_gpu_takes_the_part_off_q2s_spelling_only(self):
        self.assertEqual(tw.split_gpu(Q2), ("duckdb(no_sort, exact)", "fingerprint"))
        for spelled in ("duckdb(sort, exact)", Q61, "duckdb_fingerprint", "duckdb_none"):
            self.assertEqual(tw.split_gpu(spelled), (spelled, None))

    def test_a_one_word_stored_cell_does_not_split(self):
        decl = ("fingerprint", "duckdb_fingerprint", "data_fusion(sort, exact)", "live_cpu",
                "enabled")
        stored = tw.declared_cells(decl)[0].split("</td>")[0]
        self.assertNotIn("split", stored)
        self.assertTrue(stored.startswith('<td class="decl" title='), stored)
        self.assertTrue(stored.endswith(">fingerprint"), stored)

    def test_each_header_says_what_it_compares(self):
        table = ("<table>\n<tr><th>Query</th><th>Planning</th><th>CPU execution</th>"
                 "<th>GPU execution</th><th>features</th><th>tickets</th></tr>\n</table>")
        rewritten, _ = tw.rewrite_table("none", "none", table, pathlib.Path("."), "master", {},
                                        "o/r", {}, {})
        first = rewritten.split("</tr>")[0]
        names = ["Result stored", "DuckDB", "cpu ↔ DataFusion", "gpu ↔ cpu"]
        at = [first.index(f"<sub>{tw.html.escape(n)}</sub>") for n in names]
        self.assertEqual(at, sorted(at), "the four after Query, in this order")
        self.assertLess(first.index("Query"), at[0])
        for name in names:
            title = f'title="{tw.html.escape(tw.HEADER_HOW[name])}"'
            self.assertIn(f'<th rowspan="2" {title}><sub>{tw.html.escape(name)}</sub></th>',
                          first)

    def test_the_fingerprint_text_names_what_it_compares(self):
        for text in (tw.ORACLE_HOW["duckdb_fingerprint"], tw.FINGERPRINT_HOW):
            for said in ("sum, min and max", "1e-11", "decimals included", "SHA-256",
                         "Row order is not checked"):
                self.assertIn(said, text)


if __name__ == "__main__":
    unittest.main()
