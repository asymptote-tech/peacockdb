# join-optimizer — implementation plan

**Goal:** the prototype's optimizer as [`join-optimizer.md`](join-optimizer.md) states it:
the engine's plans only, a component layout, reproducible sidecars, `run.py` with its outputs,
the record in S3, the device-enabled benchmarks, cardinality per join, three documents; then
the cost of forwarders, named multi-joins, TES, wider dynamic filters, DuckDB's cost for the
named queries, readable reports with links, and `optimizer.md`.

**Spec:** `llm-wiki/tasks/join-optimizer.md`. **Where it starts:** `join-optimizer-detail.md`.

## Global constraints

- Read `llm-wiki/coding-style.md` before the first edit. Python: plain module names, no leading
  underscores in file names; comment caps as for Rust.
- The prototype is judged on CPU. No step measures the optimizer on the device; only Task 8
  touches the device, and it measures the engine.
- Every answer stays equal to DuckDB's: the corpus answer suites are the bar for any step that
  changes behaviour.
- Builds go to `/build/peacock`, never `./target`. DPhyp:
  `CARGO_TARGET_DIR=/build/peacock/rust-only-target cargo build --release -p peacockdb-dphyp`,
  then `PEACOCK_DPHYP_LIB=/build/peacock/rust-only-target/release/libpeacockdb_dphyp.so`.
- Every foreground build, test or ssh command under `timeout`.
- Commit per task, message at most ten lines; `build-test.md`'s counts move in the commit that
  moves a test.

## Proving commands

- **cheap tier** — the loop in `pipeline.yml`'s exec-model step: every
  `scripts/exec_model/tests/**/test_*.py` not on its `elsewhere` list, `python3 <file>`.
- **dataset tier** — the files `pipeline.yml` runs in dataset-matrix (`test_stats_sidecar.py`,
  `test_cardinality_corpus.py`, …), over the generated sf1 tables.
- **corpus tier** — the files `exec-model-corpus.yml` names, at every mode (`PCK_MODE`), on
  shad-gpu or by that workflow's manual dispatch; `PCK_SHARD=k/n` (k from 0) splits a file,
  and a substring argument narrows it to queries.
- **rust-only** — `CARGO_TARGET_DIR=/build/peacock/rust-only-target cargo test -p
  peacockdb-core --features rust-only --lib --test test_corpus_goldens --test test_ci_coverage`.

---

### Task 1: one statistics sidecar for both embedding modes

**Files:** `testdata/gen_stats.py`, `scripts/exec_model/stats.py`, `testdata/stats/*.json`,
the sidecar's tests.

- [ ] `gen_stats.py` skips list and fixed-size-list columns; `fingerprint` becomes the row
  count plus each kept column's footer min/max/null count, hashed — not the footer's bytes.
  `stats.py` computes the same function; a mismatch is still refused, table by table.
- [ ] Regenerate both sidecars. Test: generating tpch from `--embeddings synthetic` and from
  `--embeddings external` gives one byte-identical file. External data is per host
  (`fetch_embeddings.sh`); where it is absent the test says so and fails, it does not skip.
- [ ] Proving: dataset tier, rust-only untouched.

### Task 2: the handwritten lowerings go

**Files:** delete `scripts/exec_model/tests/plans*.py`, `tests/plan_helpers.py`,
`scripts/exec_model/{tpch,tpcds}.plans.txt`, `tests/test_tpch.py`, `tests/test_tpch_corpus.py`,
`tests/test_tpcds.py`; edit `pipeline.yml`, `exec-model-corpus.yml`, `README.md`, `build-test.md`.

- [ ] Before deleting, list every import of each file; a suite that imports a lowering for a
  fixture rather than to run it gets the fixture from an engine golden or a mock plan.
- [ ] Remove the CI steps and the `elsewhere` entries; the guard in the loop must still fail on
  a missing name.
- [ ] Proving: cheap and dataset tiers green; corpus tier unchanged (it never read these).

### Task 3: operators the engine's plans do not use go

**Files:** `scripts/exec_model/operators/*`, `plan_text.py` if only the lowerings used it.

- [ ] With Task 2 in, find what nothing reaches: coverage over the cheap and corpus tiers, then
  read each unreached path. Name-addressed column access and the simplified node shapes are
  the expected residue.
- [ ] Delete them; record in the detail file what was removed and what stayed and why — it is
  the input to `design.md`'s operators section.
- [ ] Proving: all tiers, corpus at every mode.

### Task 3a: the driver's step cap follows from the plan

**Files:** `scripts/exec_model/partitioned_driver.py`, its tests.

- [ ] `DEFAULT_MAX_STEPS` (100,000) is a fixed safety valve, and the row-group modes legitimately
  need more: tpch q8 at `tp4-rowgroup` takes 137,760 steps and then matches DuckDB. Each step
  moves a batch or finishes a lane, so derive the cap from the plan — source batches times nodes,
  plus lanes times nodes — and keep it a guard against a driver that stops making progress.
- [ ] A driver test of an honest run that passes a small fixed cap, and one that still trips
  on a run that does not progress.
- [ ] Proving: cheap tier; `test_engine_answers.py` at `tp4-rowgroup` over the whole corpus on
  shad-gpu (q8 green).

### Task 4: one folder per component

**Files:** every `scripts/exec_model/*.py`, `tests/`, the two workflows, `README.md`,
`build-test.md`'s links, `testdata/gen_stats.py` (it imports `scripts.exec_model.stats`).

- [ ] Move into `engine/`, `plans/`, `optimizer/` as the spec lists, with relative imports
  fixed and nothing else changed; tests into `tests/{engine,operators,plans,optimizer}/` with
  `harness.py`, `corpus.py`, `mocks.py`, `rescan.py` where their users are or in `tests/`.
  `errors.py` goes where its callers are.
- [ ] The test runner loop in `pipeline.yml` globs the subfolders; the `elsewhere` names gain
  their folder.
- [ ] Proving: the same test count before and after, every tier green; `git diff -M --stat`
  shows renames with small similarity loss only.

### Task 5: cardinality per join

**Files:** `tests/optimizer/test_cardinality_corpus.py`, a new golden under
`scripts/exec_model/testdata/`, `design.md` notes.

- [ ] The test writes, per (bench, mode), one line per join: query, depth, join type, true
  rows, estimate, q-error — sorted, deterministic — and compares with the golden; an
  `UPDATE_CANONICAL=1` run rewrites it. The aggregate thresholds go.
- [ ] q64: find which joins miss and why (which predicates correlate; which NDV is used).
  Fix in the estimator if the fix is general; otherwise the limit goes into `design.md` with
  the numbers. The golden is regenerated after.
- [ ] Proving: dataset tier green in dataset-matrix.

### Task 6: run.py, its goldens and the report

Two steps: 6a is `run.py`, the optimizer's report as data, `<mode>.cpu.txt` and
`<mode>.optimizer.txt`; 6b is the cost, `cost_report.html` and the timed full run.

**Files:** new `scripts/exec_model/run.py`, `scripts/exec_model/optimizer/report.py` (or
beside `run.py`), `scripts/exec_model/testdata/goldens/{tpch,tpcds}/*`, `README.md`.

- [ ] `run.py --bench --query --mode --jobs`: per (query, mode) one worker runs the plan
  without and with the optimizer, renders `cpu.txt` from the optimized run in the engine's
  format, and returns the `.optimizer.txt` section and both runs' costs. The parent writes the
  files whole, in corpus order.
- [ ] The optimizer reports what it did as data, not text: dynamic filters, each DPhyp call
  (input relations and edges, the sets priced, the tree), orientation flips, replans, and the
  plan before and after. `.optimizer.txt` renders that; the diff is a unified diff of the two
  plan texts.
- [ ] Cost: the engine's per-node byte components from `cost_model.conf`, as the cost
  widget computes them, over each prototype run's per-node rows and bytes. DuckDB cost from
  the committed `.duckdb_cost.txt`; the engine's from `.cost.txt`.
- [ ] `cost_report.html`: the columns of the spec; self-contained, one file.
- [ ] Time a full run on shad-gpu (`--jobs` at the core count). Past forty minutes, profile
  with `cProfile` over a heavy tpcds query; Polars only if pandas dominates — record the
  profile in the detail file either way.
- [ ] Proving: a unit test of the `.optimizer.txt` renderer over a small plan; `run.py` over
  three queries at one mode in the cheap tier's time budget; the full run on shad-gpu.

### Task 7: the record to S3

**Files:** `testdata/.gitignore`, `testdata/calibration/records.tsv` (deleted), a fetch script
under `scripts/calibration/`, `scripts/build-test-shadgpu.sh`, `scripts/lib/shadgpu-env.sh`,
`peacockdb-core/tests/test_corpus_goldens/benchmark.rs`, `tests/optimizer/test_call_cost.py`,
`build-test.md`.

- [ ] Bucket `calibration` on the Nebius endpoint `upload_datasets.sh` uses; credentials live
  on shad-gpu. Key `<dataset>.sf<sf>/records.tsv`; the sf40 record is already there (the
  detail file has its sha256). The q6/q19 record in git is superseded by it and is deleted.
- [ ] `--pull-benchmarks` uploads the record it pulled; the fetch script downloads by key into
  `testdata/calibration/` (ignored). `records.tsv` is deleted from git.
- [ ] The preamble test checks `record_header()` against a record written by the harness's own
  writer in a temp dir, not a committed file.
- [ ] The measured-record check in `test_call_cost` comes back, reading the fetched record,
  in the corpus tier beside the other suites that need data; it asserts at least fifteen mode
  pairs ordered as measured.
- [ ] Proving: rust-only and cheap tier green with no record on disk; the call-cost check on
  shad-gpu with the record fetched.

### Task 8: every device-enabled query in the benchmark

**Files:** `peacockdb-core/tests/common/corpus_benchmark_cases.inc`,
`peacockdb-core/tests/peacock_gpu_benchmarks.rs`, `testdata/benchmark-results/**`,
`build-test.md`.

- [ ] Generate tpcds sf40 on shad-gpu beside tpch sf40 (`generate_testdata.sh`); the harness
  symlinks it as it does tpch's. If generation does not fit the host, the task blocks on it
  and the tpcds cases wait.
- [ ] One case per query enabled on the device in `corpus_cases.inc`, at its modes: the named
  tpch queries and tpcds q82, q84, q85. The rust-only test that holds the timed set inside the
  device-enabled set stays green.
- [ ] Run on shad-gpu when the device is free (`nvidia-smi` first; a neighbour is #178).
  Commit the trees; the records go to S3 by Task 7's path.
- [ ] Proving: the benchmark binary's harness assertions on `gpu-tests`; the trees' trailer
  arithmetic test rust-only.

### Task 9: documents

**Files:** new `scripts/exec_model/design.md`, `scripts/exec_model/README.md`, new
`peacockdb-dphyp/design.md`.

- [ ] `design.md`: plan text → tree → estimates → dynamic filters → clusters → DPhyp →
  orientation → disassembly → adaptive run and replan; each component's shape and the
  alternative it beat; what the prototype does not model (device bytes, concurrency). Name the
  imports that run against the folders' direction — `engine/adaptive` → `plans` and
  `optimizer.observed`, `plans/engine_nodes` → `optimizer.cardinality` and `optimizer.stats` —
  and why they stay. The estimator's limits, with their numbers, are in the detail file's
  `## Task 5`.
- [ ] `README.md`: environment (Python, pandas, pyarrow, DuckDB pin), DPhyp build and
  `PEACOCK_DPHYP_LIB`, the three tiers and their data, `run.py` and its outputs, the record
  fetch.
- [ ] `peacockdb-dphyp/design.md` within 200 lines: the Rust API and the C ABI — relation
  masks, edges, the cost callback, the budget, the postfix tree, return codes — the algorithm
  in brief, the tests.
- [ ] Proving: every command in the README run once as written.

### Task 10: forwarders cost nothing, scatters cost their bytes

**Files:** `testdata/cost_model.conf`, `scripts/exec_model/plans/cost_model.py`,
`peacockdb-core/src/test_support/cost_model.rs` only if it names a category, the engine's
`testdata/goldens/**/*.cost.txt`, the `cost-report` crate if it names one, the exec-model
goldens, `scripts/exec_model/design.md`.

- [ ] `cuda_scatter_bytes 1.0 GpuEmitPartitions`; `cuda_forward_bytes 0.0 GpuMergePartitions,
  GpuUnion, GpuInterleave`; `cuda_shuffle_bytes` gone. A test that a forwarder adds nothing to
  `peacockdb_cost` and a scatter adds its bytes, red before the conf moves.
- [ ] The engine's cost goldens regenerated by the command that writes them; the cost report
  builds. `run.py` over the corpus; `.costs.txt` and `cost_report.html` move.
- [ ] `design.md`: what each side charges — DuckDB's streaming operators at zero
  (`duckdb_cost.py`'s `STREAMING_OPS`), the engine's forwarders at zero.
- [ ] Proving: rust-only, the cost-report crate's tests, cheap tier, `run.py` full.

### Task 11: join clusters are named

**Files:** `scripts/exec_model/optimizer/multijoin.py`, `plans/engine_run.py`,
`optimizer/report_text.py`, tests, goldens.

- [ ] An id per cluster, `MJ<n>` per query in plan order, outermost first, carried on each join
  node it holds through reorder, flip and replan. `.cpu.txt` renders `mj=MJ<n>` on those join
  nodes; `.optimizer.txt` heads each DPhyp call with it.
- [ ] Tests: a two-cluster plan numbers both and keeps the numbers across a reorder; a cross join
  carries none.
- [ ] Proving: cheap tier, `run.py` full.

### Task 12: non-inner joins reorder under TES

**Files:** `scripts/exec_model/optimizer/{multijoin,join_order,dphyp}.py`, a new module for
conflict detection if it earns one, `peacockdb-dphyp` only if a hyperedge it must take is
refused, new named queries in `testdata/tpch-queries/` with their registration and goldens,
tests, goldens.

- [ ] First: the named queries — the left-join one in the spec and two or three more (semi,
  anti, a left join whose inner predicate reads the null-supplying side, which must not move) —
  registered as the existing named queries are, their plans in the engine's goldens.
- [ ] CD-C over each operator: SES, TES, conflict rules; a non-inner join enters its cluster as
  a hyperedge on its TES. Cross joins stay out.
- [ ] Tests per operator kind: a legal reorder found, an illegal one never produced; the spec's
  query reorders to `(customer ⋈ σ(nation)) ⟕ orders`.
- [ ] Every answer equal to DuckDB's at every mode; q58 before and after in the detail file.
- [ ] Proving: cheap tier, corpus tier at every mode, `run.py` full.

### Task 13: dynamic filters from any build, through unions

**Files:** `scripts/exec_model/optimizer/dynamic_filters.py`, tests, goldens.

- [ ] `lift` passes `GpuUnion` and `GpuInterleave`, the key mapped into every branch, each
  branch's scan pruned on its own footers; and a non-inner join into the side it outputs. A join
  that equates NULLs, anti and mark included, prunes nothing by a build key holding a NULL.
- [ ] A second pass on the DPhyp-ordered plan, where a build holding a filter or a join
  qualifies; memory-source names unique across passes and replans; estimates of a build that
  reads a first-pass source; a candidate inside another's build probed over its pruned scan.
- [ ] Probe plans run adaptively: a build that misses inside one replans, its stopped runs priced
  as the main run's.
- [ ] Tests for both, red first. tpch q5, tpcds q5 and q58 at every mode, before and after, in
  the detail file. Answers equal to DuckDB's.
- [ ] Proving: cheap tier, corpus tier, `run.py` full.

### Task 14: DuckDB's cost for the named queries

**Files:** `testdata/gen_duckdb_cost.sh`, `testdata/goldens/tpch.sf1/*.duckdb_cost.txt`,
`testdata/duckdb-profiles/`, `testdata/duckdb-dynfilters/`.

- [x] Generation and extraction iterate every corpus query, the named ones included, with the
  pinned DuckDB and pyarrow. The engine's cost report and `cost_report.html` show their final
  ratio.
- [x] Which named queries leave room for the optimizer, and why, in the detail file.
- [x] Proving: `gen_duckdb_cost.sh --gen` run as written; the cost-report crate and
  `cost_report.py` render.

### Task 15: `.optimizer.txt` that reads without the code

**Files:** `scripts/exec_model/optimizer/{report,report_text}.py`, tests, goldens.

- [x] Headings by multi-join id; per relation estimated rows, row width, edge-column NDV; an
  aggregate output column marked as one; the ordering score and the memory-read build worded as
  what they are; `C_out` with units and the plan's order named as such.
- [x] Proving: cheap tier, `run.py` full; the q2 section quoted in the detail file.

### Task 16: a markdown page per mode

**Files:** new `scripts/exec_model/optimizer/report_md.py`, `run.py`, tests, goldens. One commit
with Task 15: the page and the log share the report data both changed.

- [x] `<mode>.optimizer.md` from the report data: per query, per multi-join the evolution table
  (tree / what changed it / cost, the plan's order first); the dynamic filters applied.
- [x] A test over a report with a reorder, a flip and a replan in one cluster.
- [ ] Proving: cheap tier, `run.py` full; the page renders on GitHub.

### Task 17: the report links to its evidence

**Files:** `scripts/exec_model/cost_report.py`, `run.py`, `.github/workflows/pipeline.yml`
(`cost-report`, `deploy-pages`), tests.

- [x] `--sha`: ratio cells other than 1.00 link to `<mode>.optimizer.md#<query>` at that sha;
  rules fired per mode — multi-joins reordered, dynamic filters applied — linking to their lines
  of `<mode>.optimizer.txt`. Without `--sha` the links are relative.
- [x] CI renders it from the committed `.costs.txt` with the commit's sha and publishes it beside
  the engine's page.
- [x] Proving: cheap tier.
- [ ] Proving: the published page's links resolve.

### Task 18: `optimizer.md`

**Files:** new `scripts/exec_model/optimizer.md`, `design.md`, `README.md`.

- [x] At most 300 lines: dynamic filters (trigger, lift nodes, values against range), clusters
  and TES, the DPhyp cost and its selectivities, orientation, replan trigger and scope. Every
  constant with its file. `design.md` points here for the rules.
- [x] Proving: every constant quoted checked against the code.
