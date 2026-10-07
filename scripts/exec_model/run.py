"""The corpus through the optimizer: each engine plan run by the prototype over the sf1 tables,
as planned and optimized (`optimizer/pipeline.py`), one task per (query, mode) over `--jobs`
processes, the optimized answer held to the planned one.

    python3 scripts/exec_model/run.py [--bench B ...] [--query Q ...] [--mode M ...] [--jobs N]

Writes under `testdata/goldens/<bench>/` here `<mode>.cpu.txt`, the optimized run as the engine
renders its own, `<mode>.optimizer.txt`, what each rule did, and `<mode>.costs.txt`, both runs
priced; a `== <query>` section each. Then `cost_report.html` from every costs file there.
README.md, "run.py", has the rest."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/run.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[2]))
    __package__ = "scripts.exec_model"

import argparse
import os
import pathlib
import re
import sys
import time
from concurrent.futures import ProcessPoolExecutor, as_completed
from dataclasses import dataclass
from enum import Enum

from .cost_report import QueryCost, section as cost_section, write as write_cost_report
from .optimizer import dphyp
from .optimizer.pipeline import MODES, mode_shape, run_optimized
from .optimizer.report import OptimizerReport, fired
from .optimizer.report_text import render
from .optimizer.stats import Statistics
from .plans.answers import same_answer
from .plans.cost_model import CostModel, load as load_cost_model
from .plans.engine_nodes import build
from .plans.engine_plan import EngineNode, read_plans
from .plans.engine_run import CORPUS_BUDGET, answer, execute, render_run
from .plans.goldens import BENCHES, OUT, ROOT, sections
from .plans.tables import ParquetTables


class Regeneration(Enum):
    """How much of a file a run owns — the engine's corpus goldens' rule (`corpus_golden.rs`)."""

    #: every query: a section no query of the corpus accounts for goes
    WHOLE = "whole"
    #: the queries named: every other section stays as it stands
    SECTIONS = "sections"


@dataclass(frozen=True)
class Task:
    bench: str
    mode: str
    query: str


@dataclass(frozen=True)
class Selection:
    work: list[Task]
    #: per (bench, mode), the corpus order of its file's sections
    order: dict[tuple[str, str], list[str]]
    #: per (bench, mode), the sections that need no run: each refused query's
    skipped: dict[tuple[str, str], dict[str, str]]


@dataclass(frozen=True)
class QueryRun:
    """What a worker returns: the runs in the engine's `cpu.txt` format — so their per-node rows
    and bytes can be priced — and the optimizer's report."""

    task: Task
    #: wall seconds of the planned run and of the optimized one, probes and replans included
    seconds: tuple[float, float]
    planned: str
    #: the optimized plan's run to its end: what `<mode>.cpu.txt` holds
    optimized: str
    #: each dynamic filter's probe plan's run, and each run a replan stopped
    probes: tuple[str, ...]
    stopped: tuple[str, ...]
    report: OptimizerReport


def tasks(benches, modes, queries: list[str] | None) -> Selection:
    """The runs asked for: every planned query of each bench and mode, or those of `queries`. A
    query no selected plan golden names is refused, before anything runs."""
    work, order, skipped, named = [], {}, {}, set()
    for bench in benches:
        for mode in modes:
            goldens = ROOT / "goldens" / f"{bench}.sf1"
            plans = read_plans(goldens / f"{mode}.plans.txt")
            engine = [query for query, _ in sections((goldens / f"{mode}-mini.cpu.txt").read_text())]
            order[bench, mode] = corpus_order(engine, list(plans))
            skipped[bench, mode] = {}
            for query in order[bench, mode]:
                if queries is not None and query not in queries:
                    continue
                named.add(query)
                if isinstance(plans[query], EngineNode):
                    work.append(Task(bench, mode, query))
                else:
                    skipped[bench, mode][query] = f"skipped: {plans[query].text.splitlines()[0]}\n"
    missing = [query for query in queries or () if query not in named]
    if missing:
        raise ValueError(f"no {' or '.join(benches)} plan golden names {' '.join(missing)}")
    return Selection(work, order, skipped)


def corpus_order(engine: list[str], planned: list[str]) -> list[str]:
    """The planned queries in the engine's registry order, as its `-mini.cpu.txt` has them; one it
    has no section for — the planner refused it — goes after the query that sorts before it."""
    order = [query for query in engine if query in planned]
    ranked = sorted(planned, key=_natural)
    for at, query in enumerate(ranked):
        if query not in order:
            order.insert(order.index(ranked[at - 1]) + 1 if at else 0, query)
    return order


def _natural(query: str):
    return [int(part) if part.isdigit() else part for part in re.split(r"(\d+)", query)]


def run_query(task: Task) -> QueryRun:
    """One worker's job: the plan as planned, then optimized, the two answers the same — the
    planned run's is DuckDB's (`test_engine_answers`), and no rule touches what sorts the rows."""
    plan = read_plans(ROOT / "goldens" / f"{task.bench}.sf1" / f"{task.mode}.plans.txt")[task.query]
    tables = ParquetTables(ROOT / f"{task.bench}.sf1")
    start = time.monotonic()
    planned, driver = execute(build(plan, tables), CORPUS_BUDGET)
    middle = time.monotonic()
    run = run_optimized(plan, Statistics(ROOT / f"{task.bench}.sf1"), tables, mode_shape(task.mode),
                        CORPUS_BUDGET)
    end = time.monotonic()
    adaptive = run.adaptive
    same_answer(task.bench, task.query, answer(adaptive.results), planned,
                f"{task.bench} {task.mode} {task.query} optimized against planned")
    return QueryRun(task, (middle - start, end - middle), render_run(plan, driver),
                    render_run(adaptive.plans[-1], adaptive.drivers[-1]),
                    tuple(render_run(side, probe) for side, probe in run.probes),
                    tuple(render_run(p, d) for p, d in zip(adaptive.plans[:-1], adaptive.drivers[:-1])),
                    run.report)


def merged(text: str, order: list[str], made: dict[str, str], regeneration: Regeneration) -> str:
    """`text` with the sections this run `made` put in: each query of `order` that has a section,
    in that order, then — where the run owns only its sections — those `order` does not name."""
    held = dict(sections(text)) | made
    kept = [(query, held[query]) for query in order if query in held]
    if regeneration is Regeneration.SECTIONS:
        kept += [(query, body) for query, body in held.items() if query not in order]
    return "".join(f"== {query}\n{body}" for query, body in kept)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--bench", nargs="+", choices=BENCHES, default=list(BENCHES))
    parser.add_argument("--query", nargs="+", help="the queries to run; every planned one by default")
    parser.add_argument("--mode", nargs="+", choices=list(MODES), default=list(MODES))
    parser.add_argument("--jobs", type=int, default=1, help="worker processes")
    parser.add_argument("--out", type=pathlib.Path, default=OUT, help=f"default {OUT}")
    args = parser.parse_args(argv)
    if args.jobs < 1:
        parser.error(f"--jobs {args.jobs}: at least one worker")
    try:
        selection = tasks(args.bench, args.mode, args.query)
        preflight(args.bench, ROOT)
    except (ValueError, RuntimeError) as error:
        parser.error(str(error))
    return generate(selection, args.jobs, args.out, Regeneration.SECTIONS if args.query else Regeneration.WHOLE)


def preflight(benches, root: pathlib.Path) -> None:
    """What every run needs and would otherwise miss only after the planned runs: each bench's sf1
    tables, and the DPhyp library."""
    missing = [bench for bench in benches if not (root / f"{bench}.sf1").is_dir()]
    if missing:
        raise ValueError(f"no {' '.join(f'{bench}.sf1' for bench in missing)} under {root}: "
                         "run testdata/generate_testdata.sh")
    dphyp.load()


def generate(selection: Selection, jobs: int, out: pathlib.Path, regeneration: Regeneration) -> int:
    """The selection's runs over `jobs` processes, then its files and the cost report written; 1
    where a run failed."""
    files = {suffix: {key: dict(made) for key, made in selection.skipped.items()}
             for suffix in ("cpu.txt", "optimizer.txt", "costs.txt")}
    model, failed, start = load_cost_model(), [], time.monotonic()
    with ProcessPoolExecutor(jobs) as pool:
        futures = {pool.submit(run_query, task): task for task in selection.work}
        for future in as_completed(futures):
            task = futures[future]
            key = (task.bench, task.mode)
            # One query failing must not cost the rest of a long run: its sections say so, and
            # the run exits non-zero.
            try:
                done = future.result()
                cost = priced(done, model)
            except Exception as error:
                failed.append(task)
                for made in files.values():
                    made[key][task.query] = f"failed: {type(error).__name__}: {error}\n"
                print(f"{task.bench} {task.mode} {task.query}: FAILED {error!r}", flush=True)
                continue
            files["cpu.txt"][key][task.query] = done.optimized
            files["optimizer.txt"][key][task.query] = render(done.report)
            files["costs.txt"][key][task.query] = cost_section(cost, model)
            print(f"{task.bench} {task.mode} {task.query}: planned {done.seconds[0]:.1f} s, "
                  f"optimized {done.seconds[1]:.1f} s, cost ratio {cost.ratio:.2f}", flush=True)
    for (bench, mode), order in selection.order.items():
        for suffix, made in files.items():
            if made[bench, mode]:
                _write(out / bench / f"{mode}.{suffix}", order, made[bench, mode], regeneration)
    write_cost_report(out)
    print(f"{len(selection.work)} runs in {time.monotonic() - start:.1f} s, {len(failed)} failed", flush=True)
    return 1 if failed else 0


def priced(done: QueryRun, model: CostModel) -> QueryCost:
    """The planned run's cost and the optimized pipeline's: its final run, and every probe plan and
    stopped run it made on the way, which are work done too."""
    context = f"{done.task.bench} {done.task.mode} {done.task.query}"
    probes = sum((model.price(text, f"{context} probe") for text in done.probes), model.price("", context))
    stopped = sum((model.price(text, f"{context} stopped") for text in done.stopped), model.price("", context))
    return QueryCost(model.price(done.planned, context), model.price(done.optimized, context) + probes + stopped,
                     probes, stopped, fired(done.report))


def _write(path: pathlib.Path, order: list[str], made: dict[str, str], regeneration: Regeneration) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    text = path.read_text() if path.exists() else ""
    staged = path.with_name(path.name + ".tmp")
    staged.write_text(merged(text, order, made, regeneration))
    os.replace(staged, path)


if __name__ == "__main__":
    sys.exit(main())
