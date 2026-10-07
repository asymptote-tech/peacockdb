"""An `OptimizerReport` as the body of its `.optimizer.txt` section: one block per rule that
fired, in the order they act — dynamic filters, the DPhyp calls with their orientation, the
replans — then the unified diff of the plan text. A report where nothing fired is one line.
"""

from __future__ import annotations

import datetime
import difflib

from .report import BuildMiss, JoinOrder, JoinTree, OptimizerReport


def render(report: OptimizerReport) -> str:
    fired = (report.candidates or report.pruned or report.probed or report.orders or report.replans
             or report.refused or report.before != report.after)
    if not fired:
        return "nothing fired\n"
    lines = []
    if report.candidates or report.pruned or report.probed:
        lines.append("dynamic filters")
        lines += [f"  candidate {c.build_key} -> {c.table}.{c.column}, clustering {c.clustering:.2f}"
                  for c in report.candidates]
        for scan in report.pruned:
            lines.append(f"  {scan.table}: row groups {len(scan.before)} -> {len(scan.after)}"
                         + (f" ({_ranges(scan.after)})" if scan.after else ""))
            lines += [f"    {column}{_keys(summary)}" for column, summary in scan.keys]
        lines += [f"  build {name} read from memory: {rows} rows" for name, rows in report.probed]
    lines += _orders(report.orders, "")
    for number, replan in enumerate(report.replans, 1):
        lines.append(f"replan {number}: {_miss(replan.miss)}")
        lines.append("  kept " + ", ".join(f"{name} ({rows} rows)" for name, rows in replan.kept))
        lines += _orders(replan.orders, "  ")
    lines += [f"replan refused: {_miss(miss)}" for miss in report.refused]
    if report.before == report.after:
        lines.append("plan unchanged")
    else:
        lines.append("plan diff")
        lines += difflib.unified_diff(report.before.splitlines(), report.after.splitlines(),
                                      "before", "after", lineterm="")
    return "\n".join(lines) + "\n"


def _orders(orders: tuple[JoinOrder, ...], indent: str) -> list[str]:
    lines = []
    for number, order in enumerate(orders, 1):
        lines.append(f"join order {number}: {len(order.relations)} relations")
        lines += [f"  r{i} {label}" for i, label in enumerate(order.relations)]
        lines += [f"  edge {_set(left)}-{_set(right)}: {keys}" for left, right, keys in order.edges]
        lines.append(f"  priced {len(order.priced)} sets")
        if order.unsolved is None:
            lines.append(f"  DPhyp {_tree(order.tree)}")
        else:
            lines.append(f"  DPhyp stopped at its budget of {order.max_pairs} pairs after pricing "
                         f"{len(order.priced)} sets: the plan's order kept")
        lines.append(f"  oriented {_tree(order.oriented)}, build first")
        lines += [f"  join {_set(j.build)} x {_set(j.probe)}: {j.rows:.1f} rows, {j.cost:.0f} bytes"
                  for j in order.joins]
        chosen = sum(j.cost for j in order.joins)
        lines.append(f"  C_out {chosen:.0f} bytes, the plan's order {order.plan_cost:.0f}")
        lines += [f"  flipped: {_set(build)} builds, {_set(probe)} probes" for build, probe in order.flipped]
    return [indent + line for line in lines]


def _miss(miss: BuildMiss) -> str:
    rows, estimate = max(miss.rows, 1.0), max(miss.estimate, 1.0)
    return (f"a build of {miss.rows} rows against {miss.estimate:.1f} estimated "
            f"(q-error {max(rows / estimate, estimate / rows):.2f}) for {miss.join}")


def _keys(summary) -> str:
    if summary.values == ():
        return ": no keys"
    text = f" in [{_value(summary.low)}, {_value(summary.high)}]"
    if summary.values is not None:
        text += ", values " + " ".join(_value(v) for v in summary.values)
    return text


def _value(value) -> str:
    """A key as its column's text: a date without its midnight, a whole float without `.0`."""
    if isinstance(value, datetime.datetime) and value.time() == datetime.time():
        return value.date().isoformat()
    if isinstance(value, float) and value.is_integer():
        return str(int(value))
    return str(value)


def _set(mask: int) -> str:
    return "+".join(f"r{i}" for i in range(mask.bit_length()) if mask >> i & 1)


def _tree(tree: JoinTree) -> str:
    return f"r{tree}" if isinstance(tree, int) else f"({_tree(tree[0])} {_tree(tree[1])})"


def _ranges(groups: tuple[int, ...]) -> str:
    """`0,1,2,4` as `0-2,4`."""
    runs = []
    for group in groups:
        if runs and group == runs[-1][1] + 1:
            runs[-1][1] = group
        else:
            runs.append([group, group])
    return ",".join(str(a) if a == b else f"{a}-{b}" for a, b in runs)
