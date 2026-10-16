"""An `OptimizerReport` as a section of `<mode>.optimizer.md`, the page beside `.optimizer.txt` that
shows what changed rather than every figure. Each cluster a DPhyp call or a replan left other than
as planned is a table: a column for the plan's order, then one per call that changed it, in the
order they ran, with rows for the join tree, what changed it and its C_out; the calls that kept
the order are a line under it. A cluster no call changed is one line. The dynamic filters of both
passes follow, a probe plan's replans with them. Relations are named as `names.py` names them.

Headings are a query's name, `<query> MJ<n>` and `<query> dynamic filters`, so the anchors other
pages link to are `goldens.anchor` of them; a `·`, `:` or `→` in a heading would change its anchor.
"""

from __future__ import annotations

from dataclasses import dataclass

from .names import Words, joined
from .report import BuildMiss, FilterPass, JoinOrder, JoinTree, OptimizerReport, mask, subtree, unordered
from .report_text import group_ranges, key_text, missed

#: a key list longer than this is a range and a count
_VALUES_SHOWN = 10


def preamble(bench: str, mode: str) -> str:
    return f"""# {bench} {mode}: join orders and dynamic filters

What the optimizer changed in each query's plan. Every figure behind it is in
[{mode}.optimizer.txt]({mode}.optimizer.txt), from the same run.

A multi-join (MJ) is a cluster of joins whose order may change. One that changed has a table: a
column for the plan's order, then one for each call that changed it — DPhyp before the run, a
replan — in the order they ran; the calls that left it unchanged are named under the table. A
column headed `inside p2, replan 1` orders only the part of the cluster inside the build `p2`,
whose own run replanned before the main run; the main plan reads `p2` whole. A cluster no call
changed is a line.

A join tree writes the build side in brackets and the probe side bare, and each join's kind as SQL
names it, the side an outer, semi or anti join keeps first. `(f)` marks a filtered relation,
`agg(X)` an aggregate over X, `t#2` the plan's second read of table t, and `MJ2` alone a cluster
read whole, as one relation of another. `p0` is a build a dynamic filter ran ahead, `m0` one a
replan kept; the plan reads both from memory.

C_out is the bytes a cluster's joins are estimated to emit, under the estimates of the call that
made its column: compare within a column, not across columns.

"""


def note(text: str) -> str:
    """A section that is no report: a skipped or failed run's line."""
    return f"\n{text.rstrip()}\n\n"


def cluster_heading(query: str, mj: str) -> str:
    return f"{query} {mj}"


def filters_heading(query: str) -> str:
    return f"{query} dynamic filters"


@dataclass(frozen=True)
class _Call:
    """One DPhyp call over a cluster: the column it makes, the miss that replanned before it — none
    before the run — what it did, and the build a probe's call ordered the inside of."""

    column: str
    miss: BuildMiss | None
    order: JoinOrder
    build: str | None = None


def section(query: str, report: OptimizerReport) -> str:
    """The body under `## <query>`."""
    clusters = _calls(report)
    blocks = [f"{mj} kept the plan's order: `{_tree(calls[0].order, calls[0].order.planned)}`."
              for mj, calls in clusters.items() if not any(call.order.changed for call in calls)]
    blocks += [f"A replan was refused, work outside its builds having started: {missed(miss)}."
               for miss in report.refused]
    sources = _sources(report)
    for mj, calls in clusters.items():
        if any(call.order.changed for call in calls):
            blocks += _table(cluster_heading(query, mj), calls, sources)
    blocks += _filters(query, report.filters_before_order, report.filters_after_order)
    if not blocks:
        return note("Nothing fired.")
    return "\n" + "\n\n".join(blocks) + "\n\n"


def _calls(report: OptimizerReport) -> dict[str, list[_Call]]:
    """Each cluster's DPhyp calls in the order they ran, by cluster id in the order first met."""
    found: dict[str, list[_Call]] = {}

    def add(column, miss, orders, build=None):
        for order in orders:
            found.setdefault(order.mj, []).append(_Call(column, miss, order, build))

    for name, replan, number in _numbered(report.filters_before_order):
        add(f"inside {name}, replan {number}", replan.miss, replan.orders, name)
    add("DPhyp before the run", None, report.orders)
    for name, replan, number in _numbered(report.filters_after_order):
        add(f"inside {name}, replan {number}", replan.miss, replan.orders, name)
    for number, replan in enumerate(report.replans, 1):
        add(f"replan {number}", replan.miss, replan.orders)
    return found


def _numbered(filters: FilterPass):
    numbers: dict[str, int] = {}
    for name, replan in filters.replans:
        numbers[name] = numbers.get(name, 0) + 1
        yield name, replan, numbers[name]


def _sources(report: OptimizerReport) -> dict:
    """Each memory source by name — a `Probed` or a `Kept` — for a legend."""
    found = {}
    for filters in (report.filters_before_order, report.filters_after_order):
        found |= {p.name: p for p in filters.probed}
        found |= {k.name: k for _, replan in filters.replans for k in replan.kept}
    found |= {k.name: k for replan in report.replans for k in replan.kept}
    return found


def _legend(named: set[str], sources: dict) -> str:
    """Each source in `named` and each one their words name, as what it holds and its rows."""
    named = set(named)
    while (more := {inner for name in named for inner in sources[name].sources} - named):
        named |= more
    return " ".join(f"{name} = `{sources[name].holds}`, {sources[name].rows:,} rows."
                    for name in sorted(named, key=_natural))


def _table(heading: str, calls: list[_Call], sources: dict) -> list[str]:
    """The plan's order and each call that changed it; the calls that kept it are a line under."""
    first, changed = calls[0].order, [call for call in calls if call.order.changed]
    header = ["", "plan"] + [call.column for call in changed]
    trees = ["join tree", f"`{_tree(first, first.planned)}`"]
    trees += [f"`{_tree(call.order, call.order.oriented)}`" for call in changed]
    changes = ["changed by", ""] + [_change(call.miss, call.order) for call in changed]
    costs = ["C_out", f"{first.plan_cost:,.0f} B"] + [_cost(call.miss, call.order) for call in changed]
    rows = [_row(header), "|" + "---|" * len(header)] + [_row(row) for row in (trees, changes, costs)]
    blocks = [f"### {heading}", "\n".join(rows)]
    kept = [_unchanged(call) for call in calls if not call.order.changed]
    if kept:
        blocks.append(f"Calls that left the order unchanged: {'; '.join(kept)}.")
    read = {name for call in changed for r in call.order.relations for name in r.sources}
    read |= {call.build for call in calls if call.build is not None}
    if read:
        blocks.append(_legend(read, sources))
    return blocks


def _unchanged(call: _Call) -> str:
    """A call that left the order as it was, with the miss that replanned where it was this
    cluster's own: elsewhere on the page only the cluster it stopped shows it."""
    miss = call.miss
    if miss is None or miss.mj != call.order.mj:
        return call.column
    return f"{call.column} ({miss.mj}'s missed build came out {miss.rows} rows, est {miss.estimate:.1f})"


def _cost(miss: BuildMiss | None, order: JoinOrder) -> str:
    before = "previous order" if miss is not None else "plan's order"
    return f"{sum(j.cost for j in order.joins):,.0f} B ({before} {order.plan_cost:,.0f} B)"


def _row(cells: list[str]) -> str:
    return "|" + "|".join(f" {cell} " if cell else " " for cell in cells) + "|"


def _change(miss: BuildMiss | None, order: JoinOrder) -> str:
    """What a call did to the order before it: the miss that replanned, then DPhyp's and the flips'."""
    planned = "previous" if miss is not None else "plan's"
    if order.unsolved is not None:
        done = [f"DPhyp at its budget, the {planned} order kept"]
    elif unordered(order.tree) != unordered(order.planned):
        done = ["reordered by DPhyp"]
    else:
        done = []
    done += [f"flip: `{_tree(order, subtree(order.oriented, build))}` now builds" for build, _ in order.flipped]
    return "; ".join(([missed(miss)] if miss is not None else []) + (done or ["order kept"]))


def _filters(query: str, before: FilterPass, after: FilterPass) -> list[str]:
    passes = [("plan as written", before), ("after join order", after)]
    if all(filters == FilterPass() for _, filters in passes):
        return []
    rows = ["| pass | scan | column | build | keys | row groups |", "|---|---|---|---|---|---|"]
    for name, filters in passes:
        for scan in filters.pruned:
            groups = f"{len(scan.before)} → {len(scan.after)}"
            groups += f" ({group_ranges(scan.after)})" if scan.after else ""
            rows += [f"| {name} | {scan.alias or scan.table} | {by.column} | {by.build}.{by.build_key} | "
                     f"{_keys(by.keys)} | {groups} |" for by in scan.by]
    blocks = [f"### {filters_heading(query)}", "\n".join(rows)]
    probed = [p for _, filters in passes for p in filters.probed]
    if probed:
        blocks.append(" ".join(f"{p.name} = `{p.holds}`, {p.rows:,} rows"
                               + (f", holding {', '.join(p.clusters)}" if p.clusters else "") + "."
                               for p in probed))
    for _, filters in passes:
        for name, replan, number in _numbered(filters):
            kept = "; ".join(f"{k.name} = `{k.holds}`, {k.rows:,} rows" for k in replan.kept)
            blocks.append(f"Probe {name}, replan {number}: {missed(replan.miss)}; {kept}.")
        blocks += [f"Probe {name}, a replan refused: {missed(miss)}." for name, miss in filters.refused]
    return blocks


def _keys(summary) -> str:
    if summary.values == ():
        return "none: every row group pruned"
    if summary.values is not None and len(summary.values) <= _VALUES_SHOWN:
        return ", ".join(key_text(v) for v in summary.values)
    count = f"{len(summary.values)} values, " if summary.values is not None else ""
    return f"{count}{key_text(summary.low)} to {key_text(summary.high)}"


def _tree(order: JoinOrder, tree: JoinTree) -> str:
    return _words(order, tree).text


def _words(order: JoinOrder, tree: JoinTree) -> Words:
    if isinstance(tree, int):
        return Words(order.relations[tree].name)
    build, probe = tree
    return joined(order.join_type(mask(build), mask(probe)), _words(order, build), _words(order, probe))


def _natural(name: str):
    return name[0], int(name[1:])
