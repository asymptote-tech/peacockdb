"""Where the golden files are — the engine's under `testdata/goldens/<bench>.sf1/`, this
prototype's under `scripts/exec_model/testdata/goldens/<bench>/` — and the `== <query>` sections
every one of them is cut into."""

from __future__ import annotations

import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[3] / "testdata"
OUT = pathlib.Path(__file__).resolve().parents[1] / "testdata" / "goldens"
BENCHES = ("tpch", "tpcds")


def sections(text: str) -> list[tuple[str, str]]:
    """A file's `== <query>` sections, every one, in file order."""
    found = []
    for line in text.splitlines(keepends=True):
        if line.startswith("== "):
            found.append((line[3:].strip(), ""))
        else:
            name, body = found[-1]
            found[-1] = (name, body + line)
    return found
