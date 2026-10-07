"""`cost_report.py` without data: a query's costs as a section of `<mode>.costs.txt` and back, the
rules a report counts as fired, and `cost_report.html` rendered from hand-built rows — its columns,
a skipped and a failed run, the ratios' arithmetic, and that it is one file."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import pathlib
import tempfile
from html.parser import HTMLParser

from .harness import main
from .optimizer.test_report_text import EVERY_RULE
from ..cost_report import MODES, Bench, QueryCost, Row, collect, read_section, render, section
from ..optimizer.report import Fired, fired
from ..plans.cost_model import parse

MODEL = parse("scan_bytes 1.0 GpuLoadParquet\njoin_bytes 0.5 GpuHashJoin\nram_to_vram_bytes 0.0\n")
NOTHING = Fired()


def cost(scan: int, join: int = 0, memory: int = 0):
    return MODEL.price(f"GpuLoadParquet: output_bytes={scan}\nGpuHashJoin: output_bytes={join}\n"
                       f"GpuMemorySource: output_bytes={memory}\n", "hand")


def query_cost(planned: int, final: int, probes: int = 0, stopped: int = 0, rules: Fired = NOTHING):
    return QueryCost(cost(planned), cost(final) + cost(probes) + cost(stopped), cost(probes), cost(stopped), rules)


def test_a_report_counts_each_rule_that_fired():
    # Two scans narrowed by one probe plan; STAR solved below the plan order's C_out with one flip,
    # PAIR in the replan stopped at its budget, the plan's order kept; one replan and one refused.
    assert fired(EVERY_RULE) == Fired(probes=1, narrowed=2, dphyp=2, reordered=1, budget=1, flips=1, replans=1,
                                      refused=1)


def test_a_query_s_costs_are_a_section_that_reads_back_the_same():
    made = QueryCost(cost(100, 10), cost(60, 2, 9) + cost(7) + cost(20, 4), cost(7), cost(20, 4),
                     Fired(probes=1, narrowed=1, dphyp=2, reordered=1, flips=1, replans=1))
    text = section(made, MODEL)
    assert text == (
        "planned peacockdb_cost=105 scan_bytes=100 join_bytes=10 ram_to_vram_bytes=0\n"
        "optimized peacockdb_cost=90 scan_bytes=87 join_bytes=6 ram_to_vram_bytes=0\n"
        "probes peacockdb_cost=7 scan_bytes=7 join_bytes=0 ram_to_vram_bytes=0\n"
        "stopped peacockdb_cost=22 scan_bytes=20 join_bytes=4 ram_to_vram_bytes=0\n"
        "fired probes=1 narrowed=1 dphyp=2 reordered=1 budget=0 flips=1 replans=1 refused=0\n")
    assert read_section(text, MODEL) == made
    assert read_section("skipped: refused by datafusion: X\n", MODEL) == "skipped: refused by datafusion: X"


class Cells(HTMLParser):
    """Each `<tr>` of the page as its cells: (tag, attributes, text)."""

    def __init__(self, text):
        super().__init__()
        self.rows, self.tags, self._cell = [], [], None
        self.feed(text)

    def handle_starttag(self, tag, attrs):
        self.tags.append((tag, dict(attrs)))
        if tag == "tr":
            self.rows.append([])
        elif tag in ("td", "th"):
            self._cell = [tag, dict(attrs), ""]

    def handle_endtag(self, tag):
        if tag in ("td", "th") and self._cell:
            self.rows[-1].append(tuple(self._cell))
            self._cell = None

    def handle_data(self, data):
        if self._cell:
            self._cell[2] += data


ROWS = [
    # Two modes run: a saving at tp1-single, a regression at tp4-single, whose engine figure is the
    # lesser until each is scaled by its mode's ratio.
    # The chips count effects: a probe plan that narrowed nothing and DPhyp calls that kept the
    # plan's order are in the hover, not on a chip.
    Row("q3", {"tp1-single": query_cost(200, 150, rules=Fired(probes=1, narrowed=1, dphyp=1, reordered=1, flips=1)),
               "tp4-single": query_cost(400, 500, probes=50, stopped=50,
                                        rules=Fired(probes=2, dphyp=3, budget=1, replans=2))},
        engine={"tp4-single": 520, "tp1-single": 1000}, duckdb=500),
    Row("q12", {mode: "skipped: refused by datafusion: SanityCheckPlan" for mode in MODES}, {}, duckdb=80),
    Row("join-int", {"tp1-single": query_cost(10, 10), "tp4-single": "failed: AssertionError: 3 rows vs 4"},
        engine={"tp1-single": 30}, duckdb=None),
    # DPhyp ran and kept the plan's order: no chip, and the counts that say so in the hover.
    Row("q5", {"tp1-single": query_cost(10, 10, rules=Fired(dphyp=1))}, {}, duckdb=None),
]


def page():
    return render([Bench("tpch", ROWS)])


def test_the_header_names_each_mode_over_its_three_columns_then_the_rest():
    rows = Cells(page()).rows
    assert [(text, attrs.get("colspan")) for _, attrs, text in rows[0]] == \
        [("Query", None)] + [(mode, "3") for mode in MODES] + \
        [("Engine .cost.txt", None), ("DuckDB", None), ("Final ratio (projected)", None), ("Rules fired", None)]
    assert [text for _, _, text in rows[1]] == ["non-optimized", "optimized", "ratio"] * len(MODES)


def test_a_row_carries_each_mode_s_costs_and_ratio_and_the_final_ratio_of_the_least_projected_mode():
    q3 = Cells(page()).rows[2]
    texts = [text for _, _, text in q3]
    assert texts[0] == "q3"
    assert texts[1:4] == ["200 B", "150 B", "0.75"]
    # tp1-rowgroup was not run.
    assert texts[4:7] == ["—", "—", "—"]
    # Optimized at tp4-single is the final run's and the probes' and the stopped runs': 600.
    assert texts[7:10] == ["400 B", "600 B", "1.50"]
    # Projected: 1000 x 0.75 = 750 at tp1-single, 520 x 1.5 = 780 at tp4-single. The engine's figure
    # is the least projected mode's, and the final ratio 750 / 500.
    assert texts[16:19] == ["1000 B (tp1-single)", "500 B", "1.50 (tp1-single)"]
    assert texts[19] == "dynamic filters 1/2 DPhyp 1/2 flips 1/2 replans 1/2", texts[19]
    hover = next(attrs["title"] for tag, attrs in Cells(page()).tags if "title" in attrs and "dphyp 3" in attrs["title"])
    assert "tp4-single: probes 2, narrowed 0, dphyp 3, reordered 0, budget 1" in hover, hover
    ratio = {attrs.get("class") for _, attrs, text in q3 if text in ("0.75", "1.50")}
    assert {"num saving", "num regression"} <= ratio, ratio
    # Past the published report's 1.4 the row is red; a row with no final ratio is grey.
    assert [attrs.get("class") for tag, attrs in Cells(page()).tags if tag == "tr"] == \
        [None, None, "red", "grey", "grey", "grey"]


def test_a_query_skipped_at_every_mode_is_one_cell_and_a_failed_run_its_mode_s():
    rows = Cells(page()).rows
    skipped = rows[3]
    assert [text for _, _, text in skipped][:2] == ["q12", "skipped: refused by datafusion: SanityCheckPlan"]
    assert skipped[1][1]["colspan"] == str(3 * len(MODES))
    assert [text for _, _, text in skipped][2:5] == ["—", "80 B", "—"]
    named = [text for _, _, text in rows[4]]
    assert named[1:4] == ["10 B", "10 B", "1.00"]
    assert "failed: AssertionError: 3 rows vs 4" in named
    # No DuckDB cost for a named query: an engine figure, and no final ratio.
    assert named[-4:-1] == ["30 B (tp1-single)", "—", "—"]
    assert named[-1] == "nothing fired"


def test_a_rule_that_ran_and_changed_nothing_says_so_and_keeps_its_counts_in_the_hover():
    cells = Cells(page())
    assert [text for _, _, text in cells.rows[5]][-1] == "nothing changed the plan"
    hovers = [attrs.get("title") for tag, attrs in cells.tags if tag == "span"]
    assert "tp1-single: probes 0, narrowed 0, dphyp 1, reordered 0, budget 0, flips 0, replans 0, refused 0" in hovers


def test_the_page_is_one_file_with_nothing_fetched():
    text = page()
    assert text.startswith("<!doctype html>") and text.endswith("</html>\n")
    fetched = [(tag, attrs) for tag, attrs in Cells(text).tags
               if tag in ("link", "script", "img", "iframe") or "src" in attrs]
    assert fetched == []
    assert "bytes are pandas'" in text
    assert "<em>Final ratio (projected)</em> = min over modes of" in text


def test_the_rows_are_read_from_the_cost_files_the_engine_s_and_duckdb_s():
    with tempfile.TemporaryDirectory() as scratch:
        out, root = pathlib.Path(scratch) / "out", pathlib.Path(scratch) / "testdata"
        (out / "tpch").mkdir(parents=True)
        (root / "goldens" / "tpch.sf1").mkdir(parents=True)
        made = query_cost(200, 150)
        (out / "tpch" / "tp1-single.costs.txt").write_text(f"== q3\n{section(made, MODEL)}== q4\nfailed: X\n")
        (root / "goldens" / "tpch.sf1" / "tp1-single-mini.cost.txt").write_text(
            "== q3\nscan_bytes=1 # GpuLoadParquet\npeacockdb_cost=1234\n== q4\nskipped: no\n")
        (root / "goldens" / "tpch.sf1" / "q3.duckdb_cost.txt").write_text("TOP_N: x\nduckdb_cost=99\n")
        benches = collect(out, root, MODEL)
    assert benches == [Bench("tpch", [Row("q3", {"tp1-single": made}, {"tp1-single": 1234}, 99),
                                      Row("q4", {"tp1-single": "failed: X"}, {}, None)])]


if __name__ == "__main__":
    raise SystemExit(main(globals()))
