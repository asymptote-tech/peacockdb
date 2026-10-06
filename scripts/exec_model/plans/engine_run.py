"""A prototype run of an engine plan — `run_plan`, read as one answer or by lane — and the run
rendered as the engine renders its own (`cpu.txt`).

`plan_text/run_text.rs`'s shape: an `early_exit=` line, then per node the plan golden's line
minus its schema plus `output_rows` and `output_bytes`, and under it `in_rows` — per child
slot, the rows taken from each of that child's lanes — `batch_rows` and `batch_bytes`, per
lane the size of every batch emitted. What differs from the engine's run is only what the
backend is: bytes are pandas', and a lane's rows follow the prototype's hash.
"""

from __future__ import annotations

import pandas as pd

from .engine_plan import EngineNode
from ..engine.node import CpuBackendSelector
from ..engine.partitioned_driver import PartitionedDriver, partitioned_driver
from ..engine.plan import Plan
from ..operators.frame import concatenate

#: What a corpus query runs under. The legacy tiers' "mini" device is 2 GiB and this is the
#: same number, which is the point: a row costs more in pandas than in cuDF, so a budget
#: that binds here binds harder than the GPU's would — a plan that fits is not flattered.
CORPUS_BUDGET = 2 * 1024 * 1024 * 1024


def run_plan(root, budget: int | None) -> PartitionedDriver:
    """`root`, a built plan, run to its end: the driver holds its results and every node's
    batches."""
    driver = partitioned_driver(Plan.build(root), CpuBackendSelector(), budget)
    driver.run()
    return driver


def execute(root, budget: int | None) -> tuple[pd.DataFrame, PartitionedDriver]:
    """`root` run to its answer, and the driver that ran it."""
    driver = run_plan(root, budget)
    return answer(driver.results), driver


def execute_lanes(root, budget: int | None) -> list[pd.DataFrame]:
    """`root` run, its output by lane — a build side's batch per lane, as a probe plan keeps it."""
    return by_lane(run_plan(root, budget))


def answer(batches) -> pd.DataFrame:
    """A run's result batches as one frame. The plan's own concatenate, not pandas': it keeps the
    first batch's schema when every batch is empty, so a query whose answer is the empty set still
    reports its columns (tpcds q17)."""
    frames = [batch.frame for batch in batches]
    return concatenate(frames) if frames else pd.DataFrame()


def by_lane(driver: PartitionedDriver) -> list[pd.DataFrame]:
    return [concatenate([batch.frame for batch in lane]) for lane in driver.root_lanes]


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
