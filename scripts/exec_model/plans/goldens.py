"""Where the golden files are — the engine's under `testdata/goldens/<bench>.sf1/`, this
prototype's under `scripts/exec_model/testdata/goldens/<bench>/` — and the `== <query>` sections
every one of them is cut into, `## <query>` in a markdown page."""

from __future__ import annotations

import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[3] / "testdata"
OUT = pathlib.Path(__file__).resolve().parents[1] / "testdata" / "goldens"
BENCHES = ("tpch", "tpcds")


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
