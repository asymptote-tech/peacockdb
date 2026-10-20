"""The engine's cost function (`peacockdb-core/src/test_support/cost_model.rs`) over a run in the
`cpu.txt` format: each node's `output_bytes` binned into a category of `testdata/cost_model.conf`,
the total Σ multiplier × bytes. The conf is read, never copied, so a retuned multiplier moves both
engines' figures. Read, summed and rounded as the Rust is, so the two agree to the byte.
"""

from __future__ import annotations

import math
import pathlib
from dataclasses import dataclass

CONF = pathlib.Path(__file__).resolve().parents[3] / "testdata" / "cost_model.conf"

#: The prototype's own kinds, priced at nothing and outside the conf: a memory source hands back a
#: build that a probe plan or a stopped run made, was priced for, and kept where the plan reads it.
#: Reading it again is free by nature, not by a multiplier. Any other kind the conf lacks raises.
FREE = frozenset({"GpuMemorySource"})


@dataclass(frozen=True)
class Category:
    name: str
    multiplier: float
    nodes: tuple[str, ...]


@dataclass(frozen=True)
class RunCost:
    """Bytes per category, in the conf's order, and the multipliers that total them."""

    multipliers: tuple[float, ...]
    bytes: tuple[int, ...]

    @property
    def total(self) -> int:
        # `cost_text_from_cpu`'s loop: added in order (`sum` compensates on 3.12+), then
        # `total.round() as u64` — half away from zero on the double, saturating: a negative or NaN
        # total is 0, one past the range or infinite is u64::MAX.
        weighted = 0.0
        for m, b in zip(self.multipliers, self.bytes):
            weighted += m * b
        if not weighted >= 0:
            return 0
        if weighted >= 2**64:
            return 2**64 - 1
        whole = math.floor(weighted)
        return int(whole) + (weighted - whole >= 0.5)

    def __add__(self, other: RunCost) -> RunCost:
        return RunCost(self.multipliers, tuple(a + b for a, b in zip(self.bytes, other.bytes)))


@dataclass(frozen=True)
class CostModel:
    categories: tuple[Category, ...]

    def price(self, cpu_text: str, context: str) -> RunCost:
        """`cpu_text`, one run, priced; a node kind no category takes is refused, as the engine
        refuses it."""
        counted = [0] * len(self.categories)
        for line in _lines(cpu_text):
            node = _node_line(line)
            if node is None:
                continue
            name, fields = node
            if name in FREE:
                continue
            # A node line carries each field once, so the first `output_bytes` is the one.
            output = next((value for key, value in fields if key == "output_bytes"), None)
            if output is not None:
                counted[self._category_of(name, context)] += int(output)
        return RunCost(tuple(c.multiplier for c in self.categories), tuple(counted))

    def _category_of(self, kind: str, context: str) -> int:
        for at, category in enumerate(self.categories):
            if kind in category.nodes:
                return at
        raise ValueError(f"{context}: node kind '{kind}' is not in the cost taxonomy")


def load(path: pathlib.Path = CONF) -> CostModel:
    return parse(path.read_text())


def parse(text: str) -> CostModel:
    """Each line `<category> <multiplier> [comma,separated,kinds]`; `#` starts a comment."""
    categories = []
    for line in _lines(text):
        columns = line.split("#")[0].split()
        if not columns:
            continue
        nodes = tuple(columns[2].split(",")) if len(columns) > 2 else ()
        categories.append(Category(columns[0], float(columns[1]), nodes))
    return CostModel(tuple(categories))


def _lines(text: str) -> list[str]:
    """Rust's `str::lines`: split at `\n` and `\r\n` alone. `splitlines` would also split at a
    form feed or a line separator inside a field."""
    *ended, last = text.split("\n")
    return [line.removesuffix("\r") for line in ended] + ([last] if last else [])


def _node_line(line: str) -> tuple[str, list[tuple[str, str]]] | None:
    """`golden_text.rs::parse_node_line`: an indented capitalized kind, then nothing, `:` or `,`
    and its fields; any other line — a detail line, `early_exit=` — is not a node."""
    trimmed = line.lstrip()
    if not trimmed[:1].isascii() or not trimmed[:1].isupper():
        return None
    end = next((at for at, c in enumerate(trimmed) if not c.isalnum()), len(trimmed))
    name, rest = trimmed[:end], trimmed[end:]
    if rest and rest[0] not in ":,":
        return None
    return name, _fields(rest[1:])


def _fields(rest: str) -> list[tuple[str, str]]:
    """Split at the commas outside brackets and double quotes, each at its first `=`."""
    parts, nesting, quoted, start = [], 0, False, 0
    for at, c in enumerate(rest):
        if c == '"':
            quoted = not quoted
        elif not quoted and c in "([{":
            nesting += 1
        elif not quoted and c in ")]}":
            nesting = max(nesting - 1, 0)
        elif c == "," and not quoted and nesting == 0:
            parts.append(rest[start:at])
            start = at + 1
    parts.append(rest[start:])
    fields = []
    for part in parts:
        key, _, value = part.strip().partition("=")
        if key.strip() or value.strip():
            fields.append((key.strip(), value.strip()))
    return fields
