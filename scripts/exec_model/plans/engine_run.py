"""A prototype run of an engine plan, rendered as the engine renders its own (`cpu.txt`).

`plan_text/run_text.rs`'s shape: an `early_exit=` line, then per node the plan golden's line
minus its schema plus `output_rows` and `output_bytes`, and under it `in_rows` — per child
slot, the rows taken from each of that child's lanes — `batch_rows` and `batch_bytes`, per
lane the size of every batch emitted. What differs from the engine's run is only what the
backend is: bytes are pandas', and a lane's rows follow the prototype's hash.
"""

from __future__ import annotations

from .engine_plan import EngineNode
from ..engine.partitioned_driver import PartitionedDriver


def render_run(plan: EngineNode, driver: PartitionedDriver) -> str:
    """`plan` as `driver` ran it — the prototype plan `engine_nodes.build` made of it."""
    ids = {id(info.node): info.id for info in driver.plan.nodes}
    post_order = _post_order(plan)
    lines = [f"early_exit={_early_exit(plan, driver, ids, post_order)}"]
    pairs = [(plan, driver.plan.nodes[driver.plan.root].node, 0)]
    while pairs:
        node, prototype, depth = pairs.pop()
        _render_node(node, driver, ids[id(prototype)], depth, lines)
        pairs.extend(reversed([
            (child, built, depth + 1) for child, built in zip(node.children, prototype.children())
        ]))
    return "\n".join(lines) + "\n"


def _render_node(node, driver, at, depth, lines) -> None:
    emitted = driver.emitted[at]
    parts = [f"{key}={value}" for key, value in node.fields.items() if key != "schema"]
    parts.append(f"output_rows={sum(rows for lane in emitted for rows, _ in lane)}")
    parts.append(f"output_bytes={sum(size for lane in emitted for _, size in lane)}")
    indent = "  " * depth
    lines.append(f"{indent}{node.kind}" + (f": {', '.join(parts)}" if parts else ""))
    detail = (f"{indent}  in_rows={_nested(driver.consumed[at])} "
              f"batch_rows={_nested([[r for r, _ in lane] for lane in emitted])} "
              f"batch_bytes={_nested([[b for _, b in lane] for lane in emitted])}")
    if driver.rows_skipped[at]:
        detail += f" rows_skipped={driver.rows_skipped[at]}"
    lines.append(detail)


def _early_exit(plan, driver, ids, post_order) -> str:
    """The limits a run satisfied, `name@post-order` as the engine addresses them, or none."""
    done, satisfied = set(driver.satisfied()), []
    stack = [(plan, driver.plan.nodes[driver.plan.root].node)]
    while stack:
        node, prototype = stack.pop()
        if ids[id(prototype)] in done:
            satisfied.append(f"{node.kind}@{post_order[id(node)]}")
        stack.extend(zip(node.children, prototype.children()))
    return ",".join(sorted(satisfied, key=lambda name: int(name.split("@")[1]))) or "none"


def _post_order(plan: EngineNode) -> dict[int, int]:
    order, stack = {}, [(plan, False)]
    while stack:
        node, visited = stack.pop()
        if visited:
            order[id(node)] = len(order)
        else:
            stack.append((node, True))
            stack.extend((child, False) for child in reversed(node.children))
    return order


def _nested(lanes) -> str:
    return "[" + ",".join("[" + ",".join(str(value) for value in lane) + "]" for lane in lanes) + "]"
