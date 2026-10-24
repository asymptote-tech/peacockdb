#!/usr/bin/env python3
"""`ticket_widget.py`'s reading of the registry: a quoted oracle cell, and a registry older
than the `duckdb_oracle` column. No cargo, no cost-report.

Run: python3 scripts/test_ticket_widget.py
"""

import os
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import ticket_widget as tw  # noqa: E402

MODES = ",".join(f"{g}{m}" for g in ("", "cpu_", "gpu_")
                 for m in ("tp1_single", "tp1_rowgroup", "tp4_single", "tp4_rowgroup", "tp4_sized"))
STATES = ",".join(["enabled"] * 15)
Q61 = 'duckdb(no_sort, rel(1e-5, "promotional_sales.promotions / all_sales.total * Int64(100)"))'


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
            root = tree(scratch, "duckdb_oracle,cpu_oracle,gpu_oracle,schema_validation",
                        f'{quoted},"data_fusion(sort, exact)","golden(no_sort, exact)",enabled')
            decl = tw.declarations(root)[("tpcds", "q61")]
        self.assertEqual(decl, (Q61, "data_fusion(sort, exact)", "golden(no_sort, exact)",
                                "enabled"))
        oracles, validation = tw.declared_cells(decl)
        self.assertEqual(oracles.count("<td"), 3)
        self.assertIn(">rel 1e-5<", oracles)
        self.assertIn("promotional_sales.promotions / all_sales.total * Int64(100)", oracles)
        self.assertIn(">enabled<", validation)

    def test_a_registry_without_duckdb_oracle_still_renders(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = tree(scratch, "cpu_oracle,gpu_oracle,schema_validation",
                        "data_fusion_exact,golden_exact,enabled")
            decl = tw.declarations(root)[("tpcds", "q61")]
        self.assertEqual(decl, (None, "data_fusion_exact", "golden_exact", "enabled"))
        oracles, _ = tw.declared_cells(decl)
        cells = oracles.split("</td>")[:3]
        self.assertEqual(len(cells), 3)
        self.assertIn(">—", cells[0])
        self.assertIn(">exact", cells[1])

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
        self.assertEqual(decl, (Q61, "data_fusion(sort, exact)", "golden(no_sort, exact)",
                                "enabled"))


if __name__ == "__main__":
    unittest.main()
