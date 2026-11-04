"""How often each rule acted on one run, as `report.fired` counts it and `<mode>.costs.txt` keeps it.
Its own module, with no project import, so that `cost_report.py` reads the counts back without pandas."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Fired:
    """How often each rule acted on one run: probe plans run and scans they narrowed, before and
    after join order; DPhyp calls — before the run and in replans — those whose tree's C_out is
    below the plan order's, and those stopped at their budget; joins flipped, replans and replans
    refused, the main run's and the probe plans'."""

    probes: int = 0
    narrowed: int = 0
    probes_after_order: int = 0
    narrowed_after_order: int = 0
    dphyp: int = 0
    reordered: int = 0
    budget: int = 0
    flips: int = 0
    replans: int = 0
    refused: int = 0
