"""Where the golden files are — the engine's under `testdata/goldens/<bench>.sf1/`, this
prototype's under `scripts/exec_model/testdata/goldens/<bench>/` — the modes they are named by, and
the `== <query>` sections every one of them is cut into, `## <query>` in a markdown page, with the
anchor GitHub gives such a heading. Nothing here imports pandas: `cost_report.py` renders without it.
"""

from __future__ import annotations

import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[3] / "testdata"
OUT = pathlib.Path(__file__).resolve().parents[1] / "testdata" / "goldens"
BENCHES = ("tpch", "tpcds")
#: The engine's five modes, by the name their goldens carry.
MODES = ("tp1-single", "tp1-rowgroup", "tp4-single", "tp4-rowgroup", "tp4-sized")


def engine_plan_paths() -> list[pathlib.Path]:
    """The engine's `<mode>.plans.txt` of every bench the prototype runs, `BENCHES`. pbench's are out
    until the reader covers its types (`engine_ir.kind_of`) and the estimator its `GpuEmpty`; till
    then only the plan reader's own tests read them."""
    return sorted(path for bench in BENCHES for path in (ROOT / "goldens" / f"{bench}.sf1").glob("*.plans.txt"))


def sections(text: str, heading: str = "== ") -> list[tuple[str, str]]:
    """A file's `<heading><query>` sections, every one, in file order. What comes before the first
    heading is the file's own preamble — a markdown page's title — and no section's."""
    found = []
    for line in text.splitlines(keepends=True):
        if line.startswith(heading):
            found.append((line[len(heading):].strip(), ""))
        elif found:
            name, body = found[-1]
            found[-1] = (name, body + line)
    return found


def anchor(heading: str) -> str:
    """GitHub's anchor for a heading: lower case, every character but a letter, a digit, `-`, `_` or a
    space dropped, each space a `-`."""
    return "#" + re.sub(r"[^\w\- ]", "", heading.strip().lower()).replace(" ", "-")
