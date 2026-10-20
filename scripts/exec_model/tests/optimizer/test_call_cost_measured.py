"""The per-call model on the measured record: fitted from the sf40 record, it orders each query's
modes as the device did wherever the measurements clearly differ.

The record is not in git. `scripts/calibration/fetch_record.sh tpch.sf40` fetches the one this
checkout pins into `call_cost.RECORD`, and without it this fails saying so. So it is not in the
cheap tier: exec-model-corpus.yml's `call-cost` job fetches the record and runs it, and so can
any host whose aws CLI reaches the bucket.
"""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import itertools

from ..harness import main
from ...optimizer.call_cost import RECORD, calls, fit


def test_the_measured_record_orders_each_query_s_modes_as_they_were_measured():
    assert RECORD.exists(), (f"no record at {RECORD}: fetch it with "
                             "`scripts/calibration/fetch_record.sh tpch.sf40`, whose header says "
                             "where the aws CLI and its credentials come from")
    model, found = fit(), calls()
    found["predicted"] = [model[kind](volume) for kind, volume in zip(found.recipe_kind, found.volume)]
    cases = found.groupby(["query", "mode"]).agg(measured=("device_us", "sum"), predicted=("predicted", "sum"))
    pairs = wrong = 0
    for query, modes in cases.groupby(level=0):
        for a, b in itertools.combinations(modes.itertuples(), 2):
            if max(a.measured, b.measured) < 1.10 * min(a.measured, b.measured):
                continue  # a tie within the runs' spread says nothing about the model
            pairs += 1
            wrong += (a.measured < b.measured) != (a.predicted < b.predicted)
    assert pairs >= 15 and wrong == 0, (pairs, wrong)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
