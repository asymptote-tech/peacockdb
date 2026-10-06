"""The engine planner's plan goldens, read back into trees.

`testdata/goldens/<bench>.sf<N>/<mode>.plans.txt` is the Rust planner's output with nothing
executed: per query a `== <name>` header, then either the node tree — one line per node, two
spaces of indent per level — followed by `--- recipes ---` and `--- memory ---`, or the refusal
the planner or DataFusion gave instead. The tree and the refusals are read; the sections after
the tree describe the same nodes again and are skipped. Field values stay verbatim, except the
schema, which every consumer needs as (name, type) pairs, the name with its quoting undone.
"""

from __future__ import annotations

import pathlib
from dataclasses import dataclass, field

from .engine_expr import parse_name
from ..errors import EnginePlanFormatError

#: The engine's node kinds (`peacockdb-core/src/plan/mod.rs`). A kind outside the set is text
#: this reader was not taught, never a node to pass through.
KINDS = frozenset({
    "GpuAccumulateBatchesAndSort", "GpuAggregate", "GpuAggregateBatches",
    "GpuCoalesceAllBatches", "GpuCrossJoin", "GpuEmitPartitions", "GpuFilter", "GpuHashJoin",
    "GpuInterleave", "GpuLimit", "GpuLoadParquet", "GpuMergePartitions",
    "GpuMergeSortedPartitions", "GpuNestedLoopJoin", "GpuProject", "GpuSort", "GpuUnion",
    "GpuUnload",
})
#: The prototype's own kinds, which the engine does not have yet: a materialization kept from a
#: stopped run and fed to the replanned one.
PROTOTYPE_KINDS = frozenset({"GpuMemorySource"})

_OPEN, _CLOSE = "([{", ")]}"


def memory_source(name: str, side: EngineNode) -> EngineNode:
    """The `GpuMemorySource` a materialization of `side` becomes: its lanes, one batch each,
    hashed as `side` was — so nothing above shuffles it again."""
    fields = {"name": name, "lanes": side.fields["lanes"], "batches": "single"}
    if "hashed_on" in side.fields:
        fields["hashed_on"] = side.fields["hashed_on"]
    fields["schema"] = side.fields["schema"]
    return EngineNode("GpuMemorySource", fields, side.schema)


@dataclass
class EngineNode:
    """One plan line: its kind, its fields in line order, and the nodes indented under it."""

    kind: str
    fields: dict[str, str]
    schema: tuple[tuple[str, str], ...]
    children: list[EngineNode] = field(default_factory=list)


@dataclass(frozen=True)
class Refusal:
    """What the planner or DataFusion said instead of a plan: every line of the section."""

    text: str


def plan_text(plan: EngineNode) -> str:
    """`plan` as a golden's tree lines: what `parse_plans` reads back into it."""
    lines, stack = [], [(plan, 0)]
    while stack:
        node, depth = stack.pop()
        fields = ", ".join(f"{key}={value}" for key, value in node.fields.items())
        lines.append("  " * depth + node.kind + (f": {fields}" if fields else ""))
        stack.extend((child, depth + 1) for child in reversed(node.children))
    return "\n".join(lines) + "\n"


def read_plans(path: pathlib.Path) -> dict[str, EngineNode | Refusal]:
    """Every section of a plan golden, by query name, in file order."""
    return parse_plans(path.read_text(), str(path))


def parse_plans(text: str, source: str) -> dict[str, EngineNode | Refusal]:
    plans: dict[str, EngineNode | Refusal] = {}
    header = None
    body: list[tuple[int, str]] = []
    for number, line in enumerate(text.splitlines(), 1):
        if line.startswith("== "):
            if header is not None:
                _add(plans, source, header, body)
            header, body = (number, line[3:]), []
        elif header is None:
            raise EnginePlanFormatError(f"{source}:{number}: text before the first `== ` header")
        else:
            body.append((number, line))
    if header is not None:
        _add(plans, source, header, body)
    return plans


def _add(plans, source, header, body) -> None:
    number, name = header
    where = f"{source}:{number}"
    if name in plans:
        raise EnginePlanFormatError(f"{where}: second section named `{name}`")
    if not body:
        raise EnginePlanFormatError(f"{where}: section `{name}` is empty")
    if body[0][1].startswith("refused"):
        plans[name] = Refusal("\n".join(line for _, line in body))
        return
    # The tree ends at the first `--- ` line: a node line starts with an indent or `Gpu`, never
    # with `---`, so nothing after the first marker can belong to the tree.
    tree = []
    for line_number, line in body:
        if line.startswith("--- "):
            break
        tree.append((line_number, line))
    if not tree:
        raise EnginePlanFormatError(f"{where}: section `{name}` has no node lines")
    plans[name] = _tree(source, name, tree)


def _tree(source, name, lines) -> EngineNode:
    root = None
    chain: list[EngineNode] = []  # from the root to the last node read
    for number, line in lines:
        at = f"{source}:{number}"
        text = line.lstrip(" ")
        indent = len(line) - len(text)
        if indent % 2:
            raise EnginePlanFormatError(f"{at}: indent of {indent} spaces is not a whole level")
        depth = indent // 2
        if depth > len(chain):
            raise EnginePlanFormatError(
                f"{at}: indented {depth} levels with no node at level {depth - 1} above it"
            )
        if depth == 0 and root is not None:
            raise EnginePlanFormatError(f"{at}: a second root in section `{name}`")
        node = _node(at, text)
        del chain[depth:]
        if chain:
            chain[-1].children.append(node)
        else:
            root = node
        chain.append(node)
    return root


def _node(at, text) -> EngineNode:
    # No kind contains ": ", so the first one ends the kind; a node without fields is a bare kind.
    kind, _, rest = text.partition(": ")
    if kind not in KINDS | PROTOTYPE_KINDS:
        raise EnginePlanFormatError(f"{at}: unknown node kind `{kind}`")
    fields: dict[str, str] = {}
    for part in _split(at, rest, ", ") if rest else []:
        # Keys are identifiers and a value may hold ` = ` (`predicate=a@0 = b`): the first `=`
        # is the key's. A non-identifier key is how a split in the wrong place shows itself.
        key, eq, value = part.partition("=")
        if not eq or not key.isidentifier():
            raise EnginePlanFormatError(f"{at}: `{part}` is not a `key=value` field")
        if key in fields:
            raise EnginePlanFormatError(f"{at}: field `{key}` given twice")
        fields[key] = value
    return EngineNode(kind, fields, _schema(at, fields.get("schema")))


def _schema(at, text) -> tuple[tuple[str, str], ...]:
    if text is None:
        return ()
    if not (text.startswith("[") and text.endswith("]")):
        raise EnginePlanFormatError(f"{at}: schema `{text}` is not a bracketed list")
    inner = text[1:-1]
    if not inner:
        return ()
    columns = []
    for entry in _split(at, inner, ", "):
        parts = _split(at, entry, ":")
        if len(parts) != 2:
            raise EnginePlanFormatError(f"{at}: schema entry `{entry}` is not one `name:type`")
        try:
            name = parse_name(parts[0])
        except EnginePlanFormatError as error:
            raise EnginePlanFormatError(f"{at}: {error}") from None
        columns.append((name, parts[1]))
    return tuple(columns)


def _split(at, text, separator) -> list[str]:
    """`text` cut at every `separator` outside brackets and backticks."""
    parts, depth, quoted, start, i = [], 0, False, 0, 0
    while i < len(text):
        ch = text[i]
        if ch == "`":
            quoted = not quoted
        elif not quoted and ch in _OPEN:
            depth += 1
        elif not quoted and ch in _CLOSE:
            depth -= 1
            if depth < 0:
                raise EnginePlanFormatError(f"{at}: `{ch}` closes nothing in `{text}`")
        if not quoted and depth == 0 and text.startswith(separator, i):
            parts.append(text[start:i])
            i += len(separator)
            start = i
            continue
        i += 1
    if depth or quoted:
        raise EnginePlanFormatError(f"{at}: unbalanced brackets or backticks in `{text}`")
    parts.append(text[start:])
    return parts
