"""One statistics sidecar whatever the embeddings: tpch sf1 generated with `--embeddings
external`, counted by `testdata/gen_stats.py`, gives the committed `testdata/stats/tpch.sf1.json`
byte for byte. The synthetic default is the dataset tier's own recount (`test_stats_sidecar`).

Generates the dataset in a temp dir — a minute or two, and about 1.5 GB while it runs — so it is
run by hand, on a host with the embeddings cache (`testdata/fetch_embeddings.sh`) and the pinned
DuckDB (`DUCKDB`, or `duckdb` on the PATH). Where the cache is absent it fails saying so.
`PEACOCK_EMBEDDINGS_CACHE` names a cache kept outside the tree.
"""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import os
import pathlib
import tempfile

from .harness import main
from .test_engine_answers import ROOT
from .test_stats_sidecar import assert_counts_the_committed_sidecar, run

CACHE = pathlib.Path(os.environ.get("PEACOCK_EMBEDDINGS_CACHE", ROOT / "embeddings-cache"))
#: what `generate_testdata.sh --embeddings external` reads at sf1
SOURCES = ("deep_base.sf1.fbin", "glove.6B.100d.txt")


def test_tpch_with_external_embeddings_gives_the_committed_sidecar():
    missing = [s for s in SOURCES if not (CACHE / s).exists()]
    assert not missing, (f"no {missing} in {CACHE}: run testdata/fetch_embeddings.sh, "
                         f"or name the cache with PEACOCK_EMBEDDINGS_CACHE")
    with tempfile.TemporaryDirectory() as scratch:
        # The generator writes beside itself and reads the cache from there, so a linked copy
        # of it in `scratch` keeps the tree's own testdata/tpch.sf1 untouched.
        testdata = pathlib.Path(scratch) / "testdata"
        testdata.mkdir()
        (testdata / "generate_testdata.sh").symlink_to(ROOT / "generate_testdata.sh")
        (testdata / "embeddings-cache").symlink_to(CACHE.resolve(), target_is_directory=True)
        run([testdata / "generate_testdata.sh", "--bench", "tpch", "--embeddings", "external"])
        assert_counts_the_committed_sidecar("tpch", testdata / "tpch.sf1")


if __name__ == "__main__":
    raise SystemExit(main(globals()))
