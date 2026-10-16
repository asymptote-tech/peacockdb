"""An `OptimizerReport` as the body of its `.optimizer.txt` section: one block per rule that fired,
in the order they act — dynamic filters, the DPhyp calls with their orientation, dynamic filters
on the order they made, the replans — then a line saying whether the plan changed. A section with
a DPhyp call opens with one line on what its estimates are made of; a report where nothing fired
is one line. Relations are `rN`, their index in their cluster, and a set of them `r0+r2`.
`plan_diff` is the unified diff of the plan text, the body of its `.plandiff.txt` section: four
fifths of the bytes, which kept the log too large to display beside it.
"""

from __future__ import annotations

import datetime
import difflib

from ..operators.join_types import JoinType
from .dynamic_filters import ORDERED_BELOW
from .estimator import DEFAULT
from .report import BuildMiss, EdgeColumn, EdgeReport, FilterPass, JoinOrder, JoinTree, OptimizerReport, unordered

#: what every figure of a DPhyp call is made of (`cardinality.py`, `join_order.SetEstimates`)
ESTIMATES = ("estimates: a filtered relation's rows by its predicate's selectivity; a join's |L| x |R| / "
             f"max(key NDVs, the key domain both share), x {DEFAULT} per residual, a non-inner join by "
             "its type; C_out = each join's rows x the bytes per row of the columns read above it, summed, "
             "in bytes; a relation's B/row is its whole row")
#: where a changed plan's diff is: the same query's section of `<mode>.plandiff.txt`
DIFF_ELSEWHERE = "plan diff: this query's section of <mode>.plandiff.txt"
#: how `names.py` names what a plan reads
NAMES = ("names: (f) a filtered relation, agg(X) an aggregate over X, t#2 the plan's second read of table t, "
         "p0 a build a dynamic filter ran first and m0 one a replan kept, both read back from memory")
_KEEPS = {JoinType.LEFT_SEMI: "a semi join keeping {}'s matches",
          JoinType.LEFT_ANTI: "an anti join keeping {}'s rows without a match",
          JoinType.LEFT: "a left join keeping every row of {}", JoinType.FULL: "a full join keeping every row"}


def render(report: OptimizerReport) -> str:
    fired = (report.filters_before_order != FilterPass() or report.orders
             or report.filters_after_order != FilterPass() or report.replans or report.refused
             or report.before != report.after)
    if not fired:
        return "nothing fired\n"
    lines = [ESTIMATES] if _calls(report) else []
    if lines or report.filters_before_order != FilterPass() or report.filters_after_order != FilterPass():
        lines.append(NAMES)
    lines += _filters(report.filters_before_order, "dynamic filters on the plan as written")
    lines += _orders(report.orders, "", "plan's")
    lines += _filters(report.filters_after_order, "dynamic filters after join order")
    for number, replan in enumerate(report.replans, 1):
        lines += _replan(f"replan {number}", replan, "")
    lines += [f"replan refused, work outside its builds had started: {missed(miss)}" for miss in report.refused]
    lines.append("plan unchanged" if report.before == report.after else DIFF_ELSEWHERE)
    return "\n".join(lines) + "\n"


def plan_diff(report: OptimizerReport) -> str:
    """The plan text before and after, as a unified diff, or `plan unchanged`."""
    if report.before == report.after:
        return "plan unchanged\n"
    lines = ["plan diff"] + list(difflib.unified_diff(report.before.splitlines(), report.after.splitlines(),
                                                      "before", "after", lineterm=""))
    return "\n".join(lines) + "\n"


def _calls(report: OptimizerReport) -> bool:
    passes = (report.filters_before_order, report.filters_after_order)
    return bool(report.orders or report.replans or any(p.replans for p in passes))


def _filters(filters: FilterPass, heading: str) -> list[str]:
    if filters == FilterPass():
        return []
    lines = [heading]
    lines += [f"  candidate {c.build_key} -> {c.table}.{c.column}: row group ranges sum to {c.clustering:.2f}x "
              f"the column's range, ordered (under {ORDERED_BELOW})" for c in filters.candidates]
    for scan in filters.pruned:
        lines.append(f"  {scan.alias or scan.table}: row groups {len(scan.before)} -> {len(scan.after)}"
                     + (f" ({group_ranges(scan.after)})" if scan.after else ""))
        lines += [f"    {by.column} by {by.build}.{by.build_key}{_keys(by.keys)}" for by in scan.by]
    lines += [f"  build {p.name} = {p.holds}, run first: {p.rows} rows, {p.size} B"
              + (f", holding {', '.join(p.clusters)}" if p.clusters else "") + "; the plan reads it from memory"
              for p in filters.probed]
    numbers: dict[str, int] = {}
    for name, replan in filters.replans:
        numbers[name] = numbers.get(name, 0) + 1
        lines += _replan(f"probe {name} replan {numbers[name]}", replan, "  ")
    lines += [f"  probe {name} replan refused, work outside its builds had started: {missed(miss)}"
              for name, miss in filters.refused]
    return lines


def _replan(heading: str, replan, indent: str) -> list[str]:
    lines = [f"{heading}: {missed(replan.miss)}",
             "  kept as memory sources: " + "; ".join(f"{k.name} ({k.rows} rows) = {k.holds}" for k in replan.kept)]
    return [indent + line for line in lines] + _orders(replan.orders, indent + "  ", "previous")


def _orders(orders: tuple[JoinOrder, ...], indent: str, planned: str) -> list[str]:
    """Each call's block; `planned` names the order it started from: the plan's, or in a replan the
    previous one."""
    lines = []
    for order in orders:
        lines.append(f"{order.mj}: {len(order.relations)} relations, "
                     + ("ordered before the run" if planned == "plan's" else "ordered again"))
        for i, r in enumerate(order.relations):
            ndv = ", ".join(f"{_column(name)} {_count(value)}" for name, value in r.ndv)
            lines.append(f"  r{i} {r.name}: rows {_count(r.rows)} {r.known.value}, {r.width:.1f} B/row"
                         + (f"; NDV {ndv}" if ndv else ""))
        lines += [_edge(edge) for edge in order.edges]
        lines.append(f"  {planned} order {_tree(order.planned)}, build first")
        if order.unsolved is None:
            same = unordered(order.tree) == unordered(order.planned)
            lines.append(f"  DPhyp priced {len(order.priced)} sets, chose {_tree(order.tree)}"
                         + (f": the {planned} order" if same else ""))
        else:
            lines.append(f"  DPhyp stopped at its budget of {order.max_pairs} pairs after pricing "
                         f"{len(order.priced)} sets: the {planned} order kept")
        lines.append(f"  oriented {_tree(order.oriented)}, build first")
        lines += [f"  join {_set(j.build)} x {_set(j.probe)}: {j.rows:.1f} rows x "
                  f"{j.cost / j.rows if j.rows else 0.0:.1f} B = {j.cost:.0f} B" for j in order.joins]
        chosen = sum(j.cost for j in order.joins)
        lines.append(f"  C_out {chosen:.0f} B; the {planned} order, priced by the same estimates, "
                     f"{order.plan_cost:.0f} B")
        lines += [f"  flipped: {_set(build)} builds, {_set(probe)} probes" for build, probe in order.flipped]
    return [indent + line for line in lines]


def _edge(edge: EdgeReport) -> str:
    sides = f"{_set(edge.left)}-{_set(edge.right)}"
    if not edge.keys:
        return (f"  residual {sides} on {', '.join(_marked(c) for c in edge.residual)}: keeps {DEFAULT} of the "
                "matches, not a DPhyp edge")
    text = f"  edge {sides}: " + " and ".join(f"{_marked(a)} = {_marked(b)}" for a, b in edge.keys)
    if edge.residual:
        text += f", then a residual on {', '.join(_marked(c) for c in edge.residual)} keeping {DEFAULT}"
    if edge.kind is not JoinType.INNER:
        text += ", " + _KEEPS[edge.kind].format(_set(edge.left))
    joined_first = [_set(side) for side in (edge.left, edge.right) if side & (side - 1)]
    if joined_first:
        text += f"; applies once {' and '.join(joined_first)} are joined"
    return text


def _named(column: EdgeColumn) -> str:
    return f"r{column.relation}.{_column(column.name)}"


def _marked(column: EdgeColumn) -> str:
    return _named(column) + (" (aggregate output)" if column.aggregate else "")


def _column(name: str) -> str:
    """A column's name, bracketed where it is an expression's text: `[min(t.v)]` is no call."""
    return name if name.isidentifier() else f"[{name}]"


def _count(value: float) -> str:
    """Rows or an NDV to a tenth, a whole one bare."""
    return f"{value:.1f}".removesuffix(".0")


def missed(miss: BuildMiss) -> str:
    """A replan's event: the build that came out off its estimate."""
    join = f"{miss.mj}'s {miss.join}" if miss.mj is not None else f"a {miss.join}"
    return (f"the build of {join} came out {miss.rows} rows, est {miss.estimate:.1f} "
            f"(q-error {miss.q_error:.2f})")


def _keys(summary) -> str:
    if summary.values == ():
        return ": no keys"
    text = f" in [{key_text(summary.low)}, {key_text(summary.high)}]"
    if summary.values is not None:
        text += ", values " + " ".join(key_text(v) for v in summary.values)
    return text


def key_text(key) -> str:
    """A key as its column's text: a date without its midnight, a whole float without `.0`."""
    if isinstance(key, datetime.datetime) and key.time() == datetime.time():
        return key.date().isoformat()
    if isinstance(key, float) and key.is_integer():
        return str(int(key))
    return str(key)


def _set(mask: int) -> str:
    return "+".join(f"r{i}" for i in range(mask.bit_length()) if mask >> i & 1)


def _tree(tree: JoinTree) -> str:
    return f"r{tree}" if isinstance(tree, int) else f"({_tree(tree[0])} {_tree(tree[1])})"


def group_ranges(groups: tuple[int, ...]) -> str:
    """`0,1,2,4` as `0-2,4`."""
    runs = []
    for group in groups:
        if runs and group == runs[-1][1] + 1:
            runs[-1][1] = group
        else:
            runs.append([group, group])
    return ",".join(str(a) if a == b else f"{a}-{b}" for a, b in runs)
