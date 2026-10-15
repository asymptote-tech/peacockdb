#!/usr/bin/env python3
"""The hourly checker's verdict over the idle watcher's last log line (gpu-host-watch.yml).

A reason to hard-stop, or None. It reuses idle_watch.decide, so the two guards apply one rule;
on top of it, a watcher whose log is over an hour old is itself a reason. A line that parses
but is not a watcher line (missing or mistyped fields) is a reason too, never an exception:
the checker is the guard that runs when everything else is broken.
"""
from __future__ import annotations

import json, pathlib, sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import idle_watch  # noqa: E402

STALE = 3600


def _no_constant(name):
    raise ValueError(f"non-finite number {name}")


def verdict(last_line, now):
    if not last_line:
        return "no watcher log on the VM"
    try:
        l = json.loads(last_line, parse_constant=_no_constant)
    except Exception:  # ValueError, RecursionError, anything: a line we cannot read is a stop
        return "the watcher's last log line is unreadable"
    try:
        return _judge(l, now)
    except Exception as e:  # OverflowError, KeyError, TypeError...: never a crash, always a stop
        return f"the watcher's last log line is malformed: {type(e).__name__}: {e}"


def _judge(l, now):
    if now - l["t"] > STALE:
        return f"the watcher's log is stale: last line {int(now - l['t'])}s old"
    if l["decision"] != "keep":
        return f"the watcher decided {l['decision']} ({l['reason']}) and the VM still runs"
    state = {"last_test": float(l["last_test"]), "last_build": float(l["last_build"])}
    procs = lambda key: [{"pid": p["pid"], "name": p["name"], "start": float(p["start"])} for p in l[key]]
    # test_procs carries the name-matched test processes, so the cap is recomputed on a hung
    # CPU-side test too. A line without it (a watcher older than this checker) is malformed.
    _, decision, reason = idle_watch.decide(state, now, float(l["boot"]), procs("card"), procs("test_procs"), False)
    return None if decision == "keep" else f"{decision}: {reason}"


def main():
    lines = sys.stdin.buffer.read().decode("utf-8", errors="replace").strip().splitlines()
    return verdict(lines[-1] if lines else None, float(sys.argv[1]))


if __name__ == "__main__":
    # Any failure here, bad argv or stdin included, is a stop with a reason on stdout: a crash
    # would exit 1 with nothing printed, which the workflow could read as keep.
    try:
        r = main()
    except BaseException as e:
        r = f"the checker itself failed: {type(e).__name__}: {e}"
    if r:
        print(r)
        sys.exit(1)
    print("keep")
