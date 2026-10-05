# join-optimizer — detail

## Where the task starts

`ENS-optimizer`, PR #165, three commits over `ENS-plan-text-names` (PR #166, the plan-text
fixes #236 and #237):

- `peacockdb-dphyp` — DPhyp as a dependency-free crate with a C ABI, six tests in CI.
- benchmarks — tpch q1 and q6 at every mode, q17 at `tp1_single`, their sf40 trees. Their
  per-call record (174,741 lines, 17 MB) is not in git: it is
  `s3://calibration/tpch.sf40/records.tsv`, sha256
  `3aab8a668a606f8bf523b6ff688427c278473f09feb530f939816878446e2a7c`.
  `testdata/calibration/records.tsv` in git is still the q6/q19 record.
- the optimizer in `scripts/exec_model`, its statistics sidecars and `gen_stats.py`, CI steps.

## What each task rests on

- Task 1. tpch's `*_embedding` columns differ between `--embeddings synthetic` and
  `external`; between two such hosts only the `part`/`partsupp` fingerprints and those
  columns' NDV differ. The stock columns' NDV agree.
- Task 2. `scripts/exec_model/tpcds.plans.txt` last changed because a union of single-batch
  branches now declares `single_batch`; tpch has no union, so its file did not move.
- Task 3. `operators/` grew to run the engine's plans: columns by position, each join type's
  own output, the planner's aggregate forms (ROLLUP, Welford states, final expressions), scans
  over real row groups, `IS NULL` and casts over NULL, an integer shuffle key hashed as the
  device hashes it.
- Task 6. DPhyp's input is a MultiJoin — not a plan node but a cluster of adjacent inner joins,
  its relations and key edges — and its output tree is rebuilt into `GpuHashJoin`s with the
  translator's wiring.
- Task 7. The bucket is reachable with the credentials on shad-gpu
  (`/home/info/bin/aws`, the endpoint `upload_datasets.sh` uses).

## Known state

- `test_cardinality_corpus` is red in dataset-matrix: with tpcds q64 in the corpus its 37 joins
  (median q-error 4.9, 14 % within 2×) take the share within 2× to 69.5 % against 70 %.
  Without q64: 599 joins, median 1.21, 73 %. → Task 5.
- Green otherwise: rust-only, DPhyp, the cheap tier (34 files), and q64 in the corpus answer,
  reassembled and optimized suites at every mode.
- CI run `37233204588`'s GPU job failed only because a neighbour held the card (the pool could
  not be built); its re-run passed.
- `test_call_cost`'s check against the measured record is not on the branch; it needs the
  sf40 record → Task 7.

## Hosts

- Local: `/usr/bin/python3` has pandas and pyarrow; the linuxbrew `python3` does not.
- shad-gpu: shared with work outside this repo. Corpus suites run from a scratch copy in
  `/tmp/peacock-opt` with its own Python 3.13 venv, DuckDB 1.5.4 and DPhyp built on the host
  (its glibc 2.31 is older than a local build needs). `/home/info/peacockdb` is the gate's
  checkout and is not touched. tpcds exists there at sf1 only.

## Task 1

- The fingerprint is `stats.fingerprint(metadata)`: the rows plus a sha256 over each flat
  column's per-row-group null count and raw min/max. `gen_stats.py` imports it, with
  `flat_columns`, from `scripts/exec_model/stats.py`, so the writer and the reader share one
  definition. A flat column is a footer path without a dot; a list leaf is `x.list.element`.
  The sidecar format is 2.
- Per row group, not whole-file aggregates: measured, external and synthetic tpch sf1 have the
  same row groups (part 2, partsupp 7, lineitem 49) and the same stock-column statistics, so
  the stricter hash still matches across modes. HEAD's `gen_stats.py` over the external data
  differed from the committed file only in part/partsupp's fingerprints and the three
  `*_embedding` NDVs. After the change, every other NDV and composite count is unchanged.
- The fingerprint cannot catch every stale NDV: data can change with no row group's null
  count, min or max moving (ps_tag with ten tags between the same extremes). So
  `test_stats_sidecar` recounts both sidecars with `gen_stats.py --out` in dataset-matrix
  and compares byte for byte, with the DuckDB the generator step unpacked (`DUCKDB` in the
  step's `env`). That also runs `gen_stats.py`'s import of `stats.py` in CI. Because of it,
  `test_stats_embeddings` keeps only the external case.
- Decimals: pyarrow before 23 cannot decode a decimal stored as INT32/INT64 (22 fails, 23
  works), and DuckDB stores them so. This was red in CI too, not only locally: dataset-matrix's
  image has an older pyarrow, `test_stats_sidecar` failed at `Statistics.column`, and `set -e`
  skipped `test_cardinality_corpus`. The reader now builds decimal bounds from
  `min_raw`/`max_raw` and the column's scale. The value is built from a string, because
  `Decimal.scaleb` rounds past the context's 28 digits. It matches pyarrow 25 on all 844
  decimal chunks of both datasets. Pinned by `test_a_decimal_s_bounds_are_read_whatever_its_storage`.
- `test_stats_embeddings.py` is run by hand. It links the generator into a temp dir and
  generates there (about 2 min locally), so `testdata/tpch.sf1` is left alone:
  `TMPDIR=/build/peacock/tmp DUCKDB=/build/peacock/duckdb-1.5.4/duckdb
  PEACOCK_EMBEDDINGS_CACHE=/build/peacock/embeddings/testdata/embeddings-cache
  /usr/bin/python3 scripts/exec_model/tests/test_stats_embeddings.py`.
  The cache was fetched by a symlinked `fetch_embeddings.sh`, which caches beside itself.
  It is not linked into the tree: the root `.gitignore` entry `testdata/embeddings-cache/`
  matches a directory only, so a symlink would show as untracked.
- Local tools: no DuckDB CLI is on the PATH; `/build/peacock/duckdb-1.5.4/duckdb` is the
  release build CI uses (08e34c447b). `/usr/bin/python3` has pyarrow 18.1;
  `/build/peacock/venv-exec-model` has pyarrow 25.0.1 and pandas 2.2.2.
