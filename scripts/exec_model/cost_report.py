"""`cost_report.html`: per query and mode the cost of the plan as planned and as optimized, priced by
the engine's own function (`plans/cost_model.py`), beside the engine's and DuckDB's figures and the
rules that fired. `run.py` writes each query's costs as a section of `<bench>/<mode>.costs.txt` and
renders the page from every such file, so a run of a few queries still renders the whole corpus.

    python3 scripts/exec_model/cost_report.py [--out DIR]

renders the page again from the files alone. README.md, "run.py", says what each column is.
"""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/cost_report.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[2]))
    __package__ = "scripts.exec_model"

import argparse
import html
import os
import pathlib
from dataclasses import dataclass, fields

from .optimizer.pipeline import MODES as SHAPES
from .optimizer.report import Fired
from .plans.cost_model import CostModel, RunCost, load
from .plans.goldens import BENCHES, OUT, ROOT, sections

MODES = tuple(SHAPES)
#: The published cost report's threshold (`cost-report/src/main.rs`): a final ratio at or below it
#: is green, above it red.
RATIO_GREEN_MAX = 1.4
#: The run lines of a section, in order: `probes` and `stopped` are parts of `optimized`, which adds
#: the final plan's run to them.
RUNS = ("planned", "optimized", "probes", "stopped")


@dataclass(frozen=True)
class QueryCost:
    """One (query, mode): the plan as planned, and everything the optimized pipeline ran — the final
    plan, each dynamic filter's probe plan, each run a replan stopped. `probes` and `stopped` are
    those parts of `optimized`."""

    planned: RunCost
    optimized: RunCost
    probes: RunCost
    stopped: RunCost
    fired: Fired

    @property
    def ratio(self) -> float:
        return self.optimized.total / self.planned.total


#: A section that is not a run: `skipped: …` or `failed: …`, its first line.
Marker = str


@dataclass(frozen=True)
class Row:
    query: str
    runs: dict[str, QueryCost | Marker]
    #: the engine's `peacockdb_cost` by mode, where its `-mini.cost.txt` has a run of the query
    engine: dict[str, int]
    duckdb: int | None

    def best(self) -> str | None:
        """The mode whose engine figure, scaled by the optimizer's ratio there, is least."""
        projected = {mode: self.engine[mode] * run.ratio for mode, run in self.runs.items()
                     if isinstance(run, QueryCost) and mode in self.engine}
        return min(projected, key=lambda mode: (projected[mode], MODES.index(mode))) if projected else None

    def final_ratio(self) -> float | None:
        mode = self.best()
        if mode is None or self.duckdb is None:
            return None
        return self.engine[mode] * self.runs[mode].ratio / self.duckdb


@dataclass(frozen=True)
class Bench:
    name: str
    rows: list[Row]


def section(cost: QueryCost, model: CostModel) -> str:
    lines = [f"{name} " + _run_text(getattr(cost, name), model) for name in RUNS]
    lines.append("fired " + " ".join(f"{f.name}={getattr(cost.fired, f.name)}" for f in fields(Fired)))
    return "\n".join(lines) + "\n"


def _run_text(cost: RunCost, model: CostModel) -> str:
    return f"peacockdb_cost={cost.total} " + " ".join(
        f"{category.name}={n}" for category, n in zip(model.categories, cost.bytes))


def read_section(body: str, model: CostModel) -> QueryCost | Marker:
    """A `<mode>.costs.txt` section back; its bytes are totalled by `model`'s multipliers."""
    if not body.startswith(RUNS[0] + " "):
        return body.splitlines()[0]
    lines = dict(line.split(" ", 1) for line in body.splitlines())
    runs = {}
    for name in RUNS:
        counted = dict(pair.split("=") for pair in lines[name].split())
        runs[name] = RunCost(tuple(c.multiplier for c in model.categories),
                             tuple(int(counted[c.name]) for c in model.categories))
    rules = dict(pair.split("=") for pair in lines["fired"].split())
    return QueryCost(**runs, fired=Fired(**{f.name: int(rules[f.name]) for f in fields(Fired)}))


def collect(out: pathlib.Path, root: pathlib.Path, model: CostModel, benches=BENCHES) -> list[Bench]:
    """Each bench with a cost file under `out`: a row per query its files hold, in their order."""
    found = []
    for bench in benches:
        files = [(mode, out / bench / f"{mode}.costs.txt") for mode in MODES]
        runs: dict[str, dict] = {}
        for mode, path in files:
            for query, body in sections(path.read_text()) if path.exists() else ():
                runs.setdefault(query, {})[mode] = read_section(body, model)
        if not runs:
            continue
        goldens = root / "goldens" / f"{bench}.sf1"
        engine = {mode: _totals(goldens / f"{mode}-mini.cost.txt", "peacockdb_cost=") for mode in MODES}
        found.append(Bench(bench, [
            Row(query, by_mode, {mode: engine[mode][query] for mode in MODES if query in engine[mode]},
                _total(goldens / f"{query}.duckdb_cost.txt", "duckdb_cost="))
            for query, by_mode in runs.items()]))
    return found


def _totals(path: pathlib.Path, key: str) -> dict[str, int]:
    if not path.exists():
        return {}
    return {query: total for query, body in sections(path.read_text())
            if (total := _footer(body, key)) is not None}


def _total(path: pathlib.Path, key: str) -> int | None:
    return _footer(path.read_text(), key) if path.exists() else None


def _footer(text: str, key: str) -> int | None:
    # A golden carries its total once, on a line of its own.
    return next((int(line[len(key):]) for line in text.splitlines() if line.startswith(key)), None)


def write(out: pathlib.Path, root: pathlib.Path = ROOT) -> pathlib.Path:
    """`out/cost_report.html` rendered from the cost files under `out`."""
    path = out / "cost_report.html"
    staged = path.with_name(path.name + ".tmp")
    staged.write_text(render(collect(out, root, load())))
    os.replace(staged, path)
    return path


# -- the page ----------------------------------------------------------------------

#: The published report's look (`cost-report/src/main.rs`), less what this page has no use for.
STYLE = """\
body{font-family:-apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif;margin:2rem;color:#1b1f23;}
h1{font-size:1.5rem;}h2{margin-top:2rem;font-size:1.2rem;}
.summary{font-size:1.05rem;background:#f6f8fa;border:1px solid #d0d7de;border-radius:6px;padding:.6rem .9rem;}
table{border-collapse:collapse;width:100%;margin-top:.5rem;}
th,td{border:1px solid #d0d7de;padding:.3rem .45rem;text-align:left;font-variant-numeric:tabular-nums;}
th{background:#f6f8fa;}td.num{text-align:right;white-space:nowrap;}
th.sub,td.cost{font-size:.72rem;}td.cost{text-align:right;white-space:nowrap;}
td.saving{color:#1a7f37;font-weight:600;}td.regression{color:#cf222e;font-weight:600;}
tr.green td:first-child{border-left:4px solid #1a7f37;}
tr.red td:first-child{border-left:4px solid #cf222e;}
tr.grey td:first-child{border-left:4px solid #8c959f;}
tr.green{background:#e9f7ee;}tr.red{background:#ffe0e0;}tr.grey{background:#f3f4f6;}
td.span{font-size:.75rem;color:#57606a;font-style:italic;}td.failed{color:#cf222e;font-style:normal;}
td.rules{font-size:.68rem;}
.chip{display:inline-block;background:#eef2f6;border:1px solid #d0d7de;border-radius:10px;
padding:0 .4rem;color:#38434f;margin:.05rem 0;}
.legend{margin-top:.6rem;color:#57606a;font-size:.85rem;}
.caveat{margin-top:.8rem;background:#fff8c5;border:1px solid #d4a72c;border-radius:6px;padding:.6rem .9rem;font-size:.9rem;}
"""

CAVEAT = """\
<p class="caveat"><strong>Costs of two prototype runs, in pandas' bytes.</strong> Each cost is the
engine's own function — <code>testdata/cost_model.conf</code>, the one its <code>.cost.txt</code>
goldens are made by: Σ multiplier × output bytes per node category — applied to a run of the Python
prototype over the sf1 tables. The prototype's rows are the engine's, but its bytes are pandas', not
the device's, so these costs compare with each other and with nothing else.
<em>non-optimized</em> is the plan as the engine planned it; <em>optimized</em> is all the work the
optimizer's pipeline did: the optimized plan's run, each dynamic filter's probe plan and each run a
replan stopped. A build such a run made is read back through a memory source, which costs nothing:
the build was paid for once, where it was made.
<em>ratio</em> = optimized / non-optimized: green below 1, red above.</p>
<p class="legend"><em>Engine .cost.txt</em> is the engine's own CPU run's figure
(<code>&lt;mode&gt;-mini.cost.txt</code>, Arrow bytes) at the mode named. <em>DuckDB</em> is the
<code>duckdb_cost=</code> of <code>&lt;q&gt;.duckdb_cost.txt</code>, bytes materialized at pipeline
breakers; the named tpch queries have none. <em>Final ratio (projected)</em> = min over modes of
the engine's figure × that mode's ratio / DuckDB's: the engine's bytes as the optimizer is projected
to change them, not a measured engine cost — directional only, as on the published report. The row
is green at or below {green}, red above. <em>Rules fired</em>: each rule with the modes where it
changed the plan out of the modes run — dynamic filters that narrowed a scan, DPhyp trees cheaper
than the plan's order, flips, replans; hover for every count per mode, calls and probes included.</p>
"""


def render(benches: list[Bench]) -> str:
    parts = ["<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">",
             "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
             f"<title>PeacockDB join optimizer cost report</title><style>{STYLE}</style></head><body>",
             "<h1>PeacockDB join optimizer: cost as planned and optimized</h1>",
             f"<p class=\"summary\">{' '.join(_summary(bench) for bench in benches)}</p>",
             CAVEAT.format(green=RATIO_GREEN_MAX)]
    for bench in benches:
        parts.append(f"<h2>{bench.name}</h2><table>")
        parts.append("<tr><th rowspan=\"2\">Query</th>"
                     + "".join(f"<th colspan=\"3\">{mode}</th>" for mode in MODES)
                     + "<th rowspan=\"2\">Engine .cost.txt</th><th rowspan=\"2\">DuckDB</th>"
                       "<th rowspan=\"2\">Final ratio (projected)</th><th rowspan=\"2\">Rules fired</th></tr>")
        parts.append("<tr>" + "<th class=\"sub\">non-optimized</th><th class=\"sub\">optimized</th>"
                               "<th class=\"sub\">ratio</th>" * len(MODES) + "</tr>")
        parts += [_row(row) for row in bench.rows]
        parts.append("</table>")
    parts.append("</body></html>\n")
    return "\n".join(parts)


def _summary(bench: Bench) -> str:
    runs = [run for row in bench.rows for run in row.runs.values()]
    costs = [run for run in runs if isinstance(run, QueryCost)]
    # By the totals, not the ratio a cell shows: 1.0002 shows as 1.00 and is still dearer.
    cheaper = sum(run.optimized.total < run.planned.total for run in costs)
    dearer = sum(run.optimized.total > run.planned.total for run in costs)
    skipped = sum(isinstance(run, str) and run.startswith("skipped") for run in runs)
    failed = sum(isinstance(run, str) and run.startswith("failed") for run in runs)
    return (f"{bench.name}: {len(bench.rows)} queries, {len(costs)} (query, mode) runs — optimized cheaper "
            f"at {cheaper}, dearer at {dearer}, the same at {len(costs) - cheaper - dearer}; "
            f"{skipped} skipped, {failed} failed.")


def _row(row: Row) -> str:
    final, best = row.final_ratio(), row.best()
    bucket = "grey" if final is None else "green" if final <= RATIO_GREEN_MAX else "red"
    markers = set(row.runs.values())
    if len(row.runs) == len(MODES) and len(markers) == 1 and isinstance(next(iter(markers)), str):
        modes = f"<td class=\"span\" colspan=\"{3 * len(MODES)}\">{html.escape(next(iter(markers)))}</td>"
    else:
        modes = "".join(_mode_cells(row.runs.get(mode)) for mode in MODES)
    engine = f"{_bytes(row.engine[best])} ({best})" if best else "—"
    duckdb = _bytes(row.duckdb) if row.duckdb is not None else "—"
    ratio = f"{final:.2f} ({best})" if final is not None else "—"
    return (f"<tr class=\"{bucket}\"><td>{html.escape(row.query)}</td>{modes}<td class=\"num\">{engine}</td>"
            f"<td class=\"num\">{duckdb}</td><td class=\"num\">{ratio}</td><td class=\"rules\">{_rules(row)}</td></tr>")


def _mode_cells(run: QueryCost | Marker | None) -> str:
    if run is None:
        return "<td class=\"cost\">—</td>" * 3
    if isinstance(run, str):
        failed = " failed" if run.startswith("failed") else ""
        return f"<td class=\"span{failed}\" colspan=\"3\">{html.escape(run)}</td>"
    shown = round(run.ratio, 2)
    kind = " saving" if shown < 1 else " regression" if shown > 1 else ""
    parts = (f"of which probe plans {_bytes(run.probes.total)}, stopped runs {_bytes(run.stopped.total)}")
    return (f"<td class=\"cost\">{_bytes(run.planned.total)}</td>"
            f"<td class=\"cost\" title=\"{parts}\">{_bytes(run.optimized.total)}</td>"
            f"<td class=\"num{kind}\">{run.ratio:.2f}</td>")


#: A chip per rule that changed the plan: its label and the count in `Fired` that says it did. A
#: probe that narrowed nothing, a call that kept the plan's order, a budget hit or a refused replan
#: changed nothing, and shows only in the hover.
CHIPS = (("dynamic filters", "narrowed"), ("DPhyp", "reordered"), ("flips", "flips"), ("replans", "replans"))


def _rules(row: Row) -> str:
    runs = {mode: run for mode, run in row.runs.items() if isinstance(run, QueryCost)}
    if not runs:
        return "—"
    if all(run.fired == Fired() for run in runs.values()):
        return "nothing fired"  # no rule ran, as `.optimizer.txt` says
    counts = "; ".join(f"{mode}: " + ", ".join(f"{f.name} {getattr(run.fired, f.name)}" for f in fields(Fired))
                       for mode, run in runs.items())
    chips = [f"{label} {sum(getattr(run.fired, name) > 0 for run in runs.values())}/{len(runs)}"
             for label, name in CHIPS if any(getattr(run.fired, name) for run in runs.values())]
    shown = " ".join(f"<span class=\"chip\">{chip}</span>" for chip in chips) or "nothing changed the plan"
    return f"<span title=\"{counts}\">{shown}</span>"


def _bytes(n: int) -> str:
    """`cost-report`'s `fmt_bytes`: binary units, two decimals past bytes."""
    value, unit = float(n), 0
    while value >= 1024 and unit < 4:
        value, unit = value / 1024, unit + 1
    return f"{n} B" if unit == 0 else f"{value:.2f} {('B', 'KB', 'MB', 'GB', 'TB')[unit]}"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--out", type=pathlib.Path, default=OUT, help=f"default {OUT}")
    print(f"wrote {write(parser.parse_args(argv).out)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
