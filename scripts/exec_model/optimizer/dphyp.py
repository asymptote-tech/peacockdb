"""DPhyp from the `peacockdb-dphyp` crate, through its C ABI.

The library is found by `PEACOCK_DPHYP_LIB`; without it the call refuses, naming the build — a
prototype test that skipped would read exactly like one that passed. The tree comes back as
disassembly takes it: a relation's index, or a `(build, probe)` pair, which side builds being
decided after (the crate names an edge per join, and disassembly finds every edge between two
sides itself).
"""

from __future__ import annotations

import ctypes
import os
from dataclasses import dataclass
from typing import Callable

BUILD = "CARGO_TARGET_DIR=/build/peacock/rust-only-target cargo build --release -p peacockdb-dphyp"
_SET_COST = ctypes.CFUNCTYPE(ctypes.c_double, ctypes.c_uint64, ctypes.c_void_p)
_REASONS = {1: "budget", 2: "disconnected", -1: "relation count", -2: "edge", -3: "capacity"}


class Unsolved(Exception):
    """DPhyp gave no tree: `reason` is `budget` or `disconnected`, or names the input it refused."""

    def __init__(self, reason: str):
        super().__init__(reason)
        self.reason = reason


@dataclass(frozen=True)
class Solved:
    tree: int | tuple
    #: `(build, probe)` postfix as the crate wrote it — each join's edge index, for diagnostics
    postfix: tuple[int, ...]


def solve(n: int, edges: list[tuple[int, int]], set_cost: Callable[[int], float],
          max_pairs: int = 2**32 - 1) -> Solved:
    """The cheapest tree over relations `0..n` joined by `edges` (pairs of relation masks), a set
    costing `set_cost(mask)`; `max_pairs` bounds the enumeration."""
    lib = _library()
    lefts = (ctypes.c_uint64 * max(len(edges), 1))(*[left for left, _ in edges])
    rights = (ctypes.c_uint64 * max(len(edges), 1))(*[right for _, right in edges])
    callback = _SET_COST(lambda s, _ctx: float(set_cost(s)))
    capacity = max(2 * n - 1, 1)
    out = (ctypes.c_int32 * capacity)()
    length = ctypes.c_uint32(0)
    code = lib.dphyp_solve(n, lefts, rights, len(edges), callback, None, min(max_pairs, 2**32 - 1),
                           out, capacity, ctypes.byref(length))
    if code != 0:
        raise Unsolved(_REASONS.get(code, f"code {code}"))
    postfix = tuple(out[: length.value])
    return Solved(_tree(postfix), postfix)


def _tree(postfix) -> int | tuple:
    stack = []
    for x in postfix:
        if x >= 0:
            stack.append(x)
        else:
            probe, build = stack.pop(), stack.pop()
            stack.append((build, probe))
    [tree] = stack
    return tree


_LOADED = None


def _library():
    global _LOADED
    if _LOADED is None:
        path = os.environ.get("PEACOCK_DPHYP_LIB")
        if not path or not os.path.exists(path):
            raise RuntimeError(f"PEACOCK_DPHYP_LIB names no library ({path!r}): build it with `{BUILD}` "
                               "and point it at libpeacockdb_dphyp.so")
        lib = ctypes.CDLL(path)
        lib.dphyp_solve.restype = ctypes.c_int32
        lib.dphyp_solve.argtypes = [
            ctypes.c_uint32, ctypes.POINTER(ctypes.c_uint64), ctypes.POINTER(ctypes.c_uint64), ctypes.c_uint32,
            _SET_COST, ctypes.c_void_p, ctypes.c_uint32,
            ctypes.POINTER(ctypes.c_int32), ctypes.c_uint32, ctypes.POINTER(ctypes.c_uint32),
        ]
        _LOADED = lib
    return _LOADED
