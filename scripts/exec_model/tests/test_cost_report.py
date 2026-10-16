"""`cost_report.py` without data: a query's costs as a section of `<mode>.costs.txt` and back, the
rules a report counts as fired, what a `.optimizer.txt` section shows and on which lines, and
`cost_report.html` rendered from hand-built rows — its columns, a skipped and a failed run, the
ratios' arithmetic, its links, and that it is one file rendered without pandas."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import dataclasses
import pathlib
import subprocess
import sys
import tempfile
from html.parser import HTMLParser

from .harness import main, raises
from .optimizer.test_report_text import EVERY_RULE, PAIR, STAR
from ..cost_report import (MODES, Bench, Evidence, Filters, QueryCost, Row, collect, main as cost_report_main,
                           read_log, read_section, render, section)
from ..optimizer.fired import Fired
from ..optimizer.report import BuildMiss, FilterPass, Kept, OptimizerReport, Replan, fired
from ..optimizer.report_text import render as log_text
from ..plans.cost_model import parse

MODEL = parse("scan_bytes 1.0 GpuLoadParquet\njoin_bytes 0.5 GpuHashJoin\nram_to_vram_bytes 0.0\n")
NOTHING = Fired()


def cost(scan: int, join: int = 0, memory: int = 0):
    return MODEL.price(f"GpuLoadParquet: output_bytes={scan}\nGpuHashJoin: output_bytes={join}\n"
                       f"GpuMemorySource: output_bytes={memory}\n", "hand")


def query_cost(planned: int, final: int, probes: int = 0, stopped: int = 0, rules: Fired = NOTHING):
    return QueryCost(cost(planned), cost(final) + cost(probes) + cost(stopped), cost(probes), cost(stopped), rules)


def test_a_report_counts_each_rule_that_fired():
    # Two scans narrowed by one probe plan, and one by a probe plan after join order; STAR solved
    # below the plan order's C_out, PAIR in the main run's replan and in the probe's stopped at its
    # budget, the plan's order kept and its join flipped; a replan and a refused one in the main
    # run, and one of each in a probe plan.
    assert fired(EVERY_RULE) == Fired(probes=1, narrowed=2, probes_after_order=1, narrowed_after_order=1, dphyp=3,
                                      reordered=1, budget=2, flips=2, replans=2, refused=2)


def test_a_query_s_costs_are_a_section_that_reads_back_the_same():
    made = QueryCost(cost(100, 10), cost(60, 2, 9) + cost(7) + cost(20, 4), cost(7), cost(20, 4),
                     Fired(probes=1, narrowed=1, dphyp=2, reordered=1, flips=1, replans=1))
    text = section(made, MODEL)
    assert text == (
        "planned peacockdb_cost=105 scan_bytes=100 join_bytes=10 ram_to_vram_bytes=0\n"
        "optimized peacockdb_cost=90 scan_bytes=87 join_bytes=6 ram_to_vram_bytes=0\n"
        "probes peacockdb_cost=7 scan_bytes=7 join_bytes=0 ram_to_vram_bytes=0\n"
        "stopped peacockdb_cost=22 scan_bytes=20 join_bytes=4 ram_to_vram_bytes=0\n"
        "fired probes=1 narrowed=1 probes_after_order=0 narrowed_after_order=0 dphyp=2 reordered=1 budget=0 "
        "flips=1 replans=1 refused=0\n")
    assert read_section(text, MODEL, "hand") == made
    assert read_section("skipped: refused by datafusion: X\n", MODEL, "hand") == "skipped: refused by datafusion: X"


class Anchors(HTMLParser):
    """Each `<a>` of the page as (href, text), in page order."""

    def __init__(self, text):
        super().__init__()
        self.links, self._href = [], None
        self.feed(text)

    def handle_starttag(self, tag, attrs):
        if tag == "a":
            self._href = dict(attrs)["href"]
            self.links.append((self._href, ""))

    def handle_endtag(self, tag):
        if tag == "a":
            self._href = None

    def handle_data(self, data):
        if self._href is not None:
            href, text = self.links[-1]
            self.links[-1] = (href, text + data)


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
        elif tag == "br" and self._cell:
            self._cell[2] += "\n"

    def handle_endtag(self, tag):
        if tag in ("td", "th") and self._cell:
            self.rows[-1].append(tuple(self._cell))
            self._cell = None

    def handle_data(self, data):
        if self._cell:
            self._cell[2] += data


ROWS = [
    # Two modes run: a saving at tp1-single, a regression at tp4-single, whose engine figure is the
    # lesser until each is scaled by its mode's ratio. At tp4-single DPhyp kept the plan's order, the
    # first pass's probe narrowed nothing and the second's one scan, and two replans ran.
    Row("q3", {"tp1-single": query_cost(200, 150, rules=Fired(probes=1, narrowed=1, dphyp=1, reordered=1, flips=1)),
               "tp4-single": query_cost(400, 500, probes=50, stopped=50,
                                        rules=Fired(probes=2, probes_after_order=1, narrowed_after_order=1,
                                                    dphyp=3, budget=1, replans=2))},
        engine={"tp4-single": 520, "tp1-single": 1000}, duckdb=500,
        evidence={"tp1-single": Evidence(reordered=1, orders=(8, 30), passes=(Filters(1, 2, (4, 20)),), changed=True),
                  "tp4-single": Evidence(orders=(10, 40), passes=(Filters(0, 1, (5, 9)), Filters(1, 1, (41, 50))),
                                         replans=2, replanned=(12, 60), changed=True)}),
    Row("q12", {mode: "skipped: refused by datafusion: SanityCheckPlan" for mode in MODES}, {}, duckdb=80),
    Row("join-int", {"tp1-single": query_cost(10, 10), "tp4-single": "failed: AssertionError: 3 rows vs 4"},
        engine={"tp1-single": 30}, duckdb=None),
    # DPhyp ran and kept the plan's order, which its log says is unchanged: the counts in the hover.
    Row("q5", {"tp1-single": query_cost(10, 10, rules=Fired(dphyp=1))}, {}, duckdb=None,
        evidence={"tp1-single": Evidence(orders=(2, 12))}),
    # No multi-join reordered and no scan reached, and a replan changed the plan.
    Row("q41", {"tp1-single": query_cost(10, 10, rules=Fired(dphyp=2, replans=1))}, {}, duckdb=None,
        evidence={"tp1-single": Evidence(orders=(3, 20), replans=1, replanned=(9, 20), changed=True)}),
    # The plan changed and the log shows none of what is counted.
    Row("q7", {"tp1-single": query_cost(10, 10, rules=Fired(dphyp=1))}, {}, duckdb=None,
        evidence={"tp1-single": Evidence(orders=(2, 12), changed=True)}),
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
    assert texts[19] == "tp1-single: 1 MJ, 1/2 DF\ntp4-single: 0/1 + 1/1 DF, 2 replans", texts[19]
    hover = next(attrs["title"] for tag, attrs in Cells(page()).tags if "title" in attrs and "dphyp 3" in attrs["title"])
    assert ("tp4-single: probes 2, narrowed 0, probes_after_order 1, narrowed_after_order 1, dphyp 3, reordered 0, "
            "budget 1") in hover, hover
    ratio = {attrs.get("class") for _, attrs, text in q3 if text in ("0.75", "1.50")}
    assert {"num saving", "num regression"} <= ratio, ratio
    # Past the published report's 1.4 the row is red; a row with no final ratio is grey.
    assert [attrs.get("class") for tag, attrs in Cells(page()).tags if tag == "tr"] == \
        [None, None, "red", "grey", "grey", "grey", "grey", "grey"]


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
    # Where the log says the plan changed, the line says what did, or only that it did.
    assert [text for _, _, text in cells.rows[6]][-1] == "tp1-single: 1 replan"
    assert [text for _, _, text in cells.rows[7]][-1] == "tp1-single: the plan changed"
    hovers = [attrs.get("title") for tag, attrs in cells.tags if tag == "span"]
    assert ("tp1-single: probes 0, narrowed 0, probes_after_order 0, narrowed_after_order 0, dphyp 1, reordered 0, "
            "budget 0, flips 0, replans 0, refused 0") in hovers


def test_a_ratio_other_than_1_00_links_to_its_query_s_section_of_the_mode_s_page():
    pages = [(href, text) for href, text in Anchors(page()).links if ".optimizer.md" in href]
    # join-int's, q5's, q41's and q7's 1.00, the dashes, the skipped and the failed runs stay plain.
    assert pages == [("tpch/tp1-single.optimizer.md#q3", "0.75"), ("tpch/tp4-single.optimizer.md#q3", "1.50")], pages
    assert sum(text == "1.00" for row in Cells(page()).rows for _, _, text in row) == 4


def test_with_a_sha_every_link_is_that_commit_s_file_on_github():
    at = "https://github.com/asymptote-tech/peacockdb/blob/0123abc/scripts/exec_model/testdata/goldens/tpch/"
    links = Anchors(render([Bench("tpch", ROWS)], sha="0123abc")).links
    assert links[0] == (at + "tp1-single.optimizer.md#q3", "0.75"), links
    assert all(href.startswith(at) for href, _ in links), links
    assert len(links) == len(Anchors(page()).links)


def test_rules_fired_is_a_line_per_mode_its_parts_linking_to_their_lines_of_the_mode_s_log():
    # A fraction per filter pass that reached a scan, each to its own block.
    assert [link for link in Anchors(page()).links if ".optimizer.txt" in link[0]] == [
        ("tpch/tp1-single.optimizer.txt#L8-L30", "1 MJ"), ("tpch/tp1-single.optimizer.txt#L4-L20", "1/2"),
        ("tpch/tp4-single.optimizer.txt#L5-L9", "0/1"), ("tpch/tp4-single.optimizer.txt#L41-L50", "1/1"),
        ("tpch/tp4-single.optimizer.txt#L12-L60", "2 replans"), ("tpch/tp1-single.optimizer.txt#L9-L20", "1 replan")]


def test_a_sha_that_is_not_a_commit_s_is_refused():
    with tempfile.TemporaryDirectory() as scratch, raises(SystemExit):
        cost_report_main(["--out", scratch, "--sha", "master"])


#: Two sections of a `.optimizer.txt`, the second with no block to link to. MJ1 is reordered before
#: the run and again in a replan, MJ2 kept; the first pass narrows its scan, the second not.
LOG = """\
== q1
estimates: e
names: n
dynamic filters on the plan as written
  sales: row groups 4 -> 2 (0-1)
    sk by p0.dk in [1, 2]
  build p0 = dates (f), run first: 5 rows, 40 B; the plan reads it from memory
MJ1: 3 relations, ordered before the run
  plan's order ((r0 r1) r2), build first
  DPhyp priced 3 sets, chose ((r0 r2) r1)
  oriented ((r2 r0) r1), build first
MJ2: 2 relations, ordered before the run
  plan's order (r0 r1), build first
  DPhyp priced 1 sets, chose (r0 r1): the plan's order
  oriented (r0 r1), build first
dynamic filters after join order
  returns: row groups 3 -> 3 (0-2)
    rk by p1.dk in [1, 2]
replan 1: the build of MJ1's join on dk = sk came out 100 rows, est 10.0 (q-error 10.00)
  kept as memory sources: m0 (100 rows) = p0
  MJ1: 3 relations, ordered again
    previous order ((r2 r0) r1), build first
    DPhyp priced 3 sets, chose ((r0 r1) r2)
    oriented ((r0 r1) r2), build first
plan diff: this query's section of <mode>.plandiff.txt
== q2
nothing fired
"""


Q1 = Evidence(reordered=1, orders=(8, 24), passes=(Filters(1, 1, (4, 7)), Filters(0, 1, (16, 18))), replans=1,
              replanned=(19, 24), changed=True)


def test_a_log_section_gives_its_counts_and_the_first_and_last_line_of_each_kind_of_block():
    # Join order: MJ1 to the replan's last line, the filters after join order between them. Each
    # filter pass its own block.
    assert read_log(LOG) == {"q1": Q1, "q2": Evidence()}


def test_the_log_is_read_back_as_report_text_writes_it():
    # EVERY_RULE: MJ1 reordered, MJ2 flipped in a probe's replan and in the main run's; three scans
    # reached and narrowed, two in the first pass.
    text = "== q\n" + log_text(EVERY_RULE)
    lines = text.splitlines()
    found = read_log(text)["q"]
    rules = fired(EVERY_RULE)
    assert found.reordered == 2 and found.replans == rules.replans == 2 and found.changed
    assert [(p.narrowed, p.applied) for p in found.passes] == [(rules.narrowed, 2), (rules.narrowed_after_order, 1)]
    first, last = found.orders
    assert lines[first - 1].startswith("MJ1: ") and lines[last - 1].startswith("replan refused"), found
    before, after = found.passes
    assert lines[before.lines[0] - 1] == "dynamic filters on the plan as written", found
    assert lines[before.lines[1] - 1].startswith("  build p0 = "), found
    assert lines[after.lines[0] - 1] == "dynamic filters after join order", found
    assert lines[after.lines[1] - 1].startswith("  probe p1 replan refused"), found
    first, last = found.replanned
    assert lines[first - 1].startswith("  probe p1 replan 1: ") and lines[last].startswith("replan refused"), found


#: As the pipeline orders them: STAR's call before the run, a probe plan's replan in the filters after
#: join order changing MJ3, which no other call orders, and the main run's replan keeping the order
#: MJ4 had, which no other call changes.
INSIDE_A_PROBE = OptimizerReport(
    FilterPass(), (STAR,),
    FilterPass(replans=(("p1", Replan(BuildMiss("join on a = b", 40, 4.0, "MJ3"), (Kept("m0", 40, "x"),),
                                      (dataclasses.replace(PAIR, mj="MJ3"),))),)),
    (Replan(BuildMiss("join on c = d", 40, 4.0, "MJ1"), (Kept("m1", 40, "y"),),
            (dataclasses.replace(PAIR, mj="MJ4", oriented=(0, 1), flipped=()),)),),
    (), "a\n", "b\n")


def test_a_call_inside_a_probe_s_replan_counts_and_a_replan_that_keeps_the_previous_order_does_not():
    found = read_log("== q\n" + log_text(INSIDE_A_PROBE))["q"]
    assert (found.reordered, found.replans) == (2, 2), found


def test_the_committed_page_renders_with_pandas_pyarrow_and_numpy_blocked():
    # The cost-report job's python3 has pandas from the prototype's step, so CI's render alone
    # would not show that the page needs none; blocked here, an import of one anywhere fails.
    root = pathlib.Path(__file__).resolve().parents[3]
    blocked = ("pandas", "pyarrow", "numpy")
    probe = (f"import sys; sys.path.insert(0, {str(root)!r})\n"
             + "".join(f"sys.modules[{name!r}] = None\n" for name in blocked)
             + "from scripts.exec_model.cost_report import collect, render\n"
               "from scripts.exec_model.plans.cost_model import load\n"
               "from scripts.exec_model.plans.goldens import OUT, ROOT\n"
               "assert '<a href=' in render(collect(OUT, ROOT, load()), '0' * 40)\n")
    done = subprocess.run([sys.executable, "-I", "-c", probe], capture_output=True, text=True, timeout=60)
    assert done.returncode == 0, done.stderr


def test_the_page_is_one_file_with_nothing_fetched():
    text = page()
    assert text.startswith("<!doctype html>") and text.endswith("</html>\n")
    fetched = [(tag, attrs) for tag, attrs in Cells(text).tags
               if tag in ("link", "script", "img", "iframe") or "src" in attrs]
    assert fetched == []
    assert "bytes are pandas'" in text
    assert "<em>Final ratio (projected)</em> = min over modes of" in text


def test_the_summary_counts_a_run_cheaper_or_dearer_by_its_totals_not_its_shown_ratio():
    # 0.996 and 1.0002 both show as 1.00; the totals still differ.
    runs = {"tp1-single": query_cost(1000, 996), "tp4-single": query_cost(10000, 10002),
            "tp4-sized": query_cost(100, 100)}
    text = render([Bench("tpch", [Row("q3", runs, {}, None)])])
    assert "optimized cheaper at 1, dearer at 1, the same at 1;" in text, text[:600]
    shown = [text for _, _, text in Cells(text).rows[2] if text.startswith("1.00")]
    assert shown == ["1.00", "1.00", "1.00"]
    # A ratio that shows as 1.00 is not linked.
    assert [href for href, _ in Anchors(text).links if ".optimizer.md" in href] == []


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
        (out / "tpch" / "tp1-single.optimizer.txt").write_text(LOG.replace("== q1", "== q3").replace("== q2", "== q4"))
        benches = collect(out, root, MODEL)
    assert benches == [Bench("tpch", [Row("q3", {"tp1-single": made}, {"tp1-single": 1234}, 99, {"tp1-single": Q1}),
                                      Row("q4", {"tp1-single": "failed: X"}, {}, None, {"tp1-single": Evidence()})])]


def test_a_cost_file_priced_under_another_conf_is_refused_naming_it():
    other = parse("scan_bytes 1.0 GpuLoadParquet\nshuffle_bytes 1.0 GpuHashJoin\nram_to_vram_bytes 0.0\n")
    with tempfile.TemporaryDirectory() as scratch:
        out = pathlib.Path(scratch) / "out"
        (out / "tpch").mkdir(parents=True)
        (out / "tpch" / "tp1-single.costs.txt").write_text(f"== q3\n{section(query_cost(200, 150), other)}")
        with raises(ValueError, match=r"tpch/tp1-single\.costs\.txt q3: priced under another "
                                      r"cost_model\.conf.*run\.py"):
            collect(out, pathlib.Path(scratch) / "testdata", MODEL)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
