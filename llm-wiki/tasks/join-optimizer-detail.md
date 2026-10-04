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
- `test_call_cost`'s check against the measured record is not on the branch; it needs the
  sf40 record → Task 7.

## Hosts

- Local: `/usr/bin/python3` has pandas and pyarrow; the linuxbrew `python3` does not.
- shad-gpu: shared with work outside this repo. Corpus suites run from a scratch copy in
  `/tmp/peacock-opt` with its own Python 3.13 venv, DuckDB 1.5.4 and DPhyp built on the host
  (its glibc 2.31 is older than a local build needs). `/home/info/peacockdb` is the gate's
  checkout and is not touched. tpcds exists there at sf1 only.
