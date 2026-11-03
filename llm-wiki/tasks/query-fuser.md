# Prototype: fuse N queries' plans into one DAG that loads each table once, and run batches of them

Kind: prototype

On sf40 a query's load is 400-690 ms against 19-48 ms of execute (`reports/benchmark-minimal.md`).
Queries that read the same tables pay that load again each time. This prototype takes the plans of
N queries in tp1_single mode and fuses them into one plan, built on dag-driver, in which each table
is read once and feeds every query that needs it. It runs batches of the first 3, 10, 20 and all
queries of each dataset on the H100, and records what batching saves.

## Done when

1. **The fuser.** It takes N queries' tp1_single plans and returns one DAG with N roots. Scans of
   the same parquet file become one `GpuLoadParquet`:
   - its projection is the union of the queries' projections;
   - its row groups are the union of their surviving row groups, with batch sizing redone;
   - each consumer sees exactly the columns and rows its own plan expected, through a zero-copy
     column selection and through the filters its plan already carries.

   If a query's pruning dropped row groups that its filters do not reject, sharing could change
   its answer. The fuser then does not share that scan, and says so. Nothing but sources is
   merged.
1a. **Buffer or re-read, per held reader.** Where a reader of a shared scan lies inside a join's
   hold, the fuser makes one of two choices, case by case, and records which and why:
   - **re-read:** that reader gets its own `GpuLoadParquet`, loaded once the hold lifts. This
     costs the table's load time again, from the baseline's load time and `loads.tsv`.
   - **buffer:** a `GpuHoldBuffer` (dag-driver) goes on the reader's path. This costs the bytes
     the buffer keeps.

   Placement of a buffer: on the path from the shared scan up to the held join's probe input,
   the edge whose recorded `batch_bytes` in the sf40-baseline run is smallest. That is after the
   filters and projections that shrink the batch. A buffer above an inner join on that path runs
   that join early, so the choice also charges that join's build table. It is placed above a
   join only when the recorded rows say the join did not increase cardinality there. The choice
   between re-read and buffer is a plain function of those numbers and the memory budget,
   testable without a device. Its inputs come from the sf40 benchmark files. The prototype runs
   at sf40 only, and has no fallback for a scale without them.
1b. **Reads per table.** For each batch, the fuser reports how many times each table is read:
   once, plus one per re-read it chose.
2. **The mergeability hint.** For each batch, the fuser lists the identical subtrees above the
   shared sources: same node, same parameters, same inputs, across queries. That includes a
   filter, a join or an aggregate several queries compute alike. Each entry carries the time
   merging it would save, read from the sf40-baseline per-node times. It is a report, not an
   execution.
3. **The pipelining hint.** For each batch, it estimates the further saving if execution
   overlapped loading: a query starts as soon as its own tables are on the device, and loads are
   ordered to start queries early. The estimate is a schedule computed from the baseline's per-node
   load and execute times, against the fused run's measured load + execute. The model is stated.
4. **The concurrency cap.** The outer code reads each query's `peak_bytes` and per-node
   `batch_bytes` from the sf40-baseline results and passes dag-driver a cap that keeps the
   estimated live bytes under the device's memory less a margin. The rule is written down and
   is a plain function, testable without a device.
5. **The runs.** For TPC-H and TPC-DS, the first 3, 10, 20 and all queries of sf40-baseline's
   ordered list, in tp1_single on the H100. That list is the H100 tree's
   `testdata/benchmark-results/h100-80gb-hbm3/cases.csv`, read when the batch runs, not compiled
   in: per dataset, its `ok` and `wrong` rows carrying `tp1_single`, in file order, TPC-H
   restricted to q1-q22. A query sf40-baseline skipped is a `skipped` row there, so it is not in
   a batch. Each batch records:
   - fused load time and fused execute time, measured separately;
   - the cap used, and peak device memory;
   - the two hints.

   The protocol is the benchmark's warm-up and 2nd-min, with fewer measured runs allowed for the
   largest batches, stated. A batch that runs out of device memory is a recorded outcome, not a
   failure of the task. The smaller batches are the comparison.
6. **Answers.** Every query's answer in a fused batch equals its unfused answer on the same
   device, checked for every batch that completes. On the CPU backend at sf1, a fused batch's
   answers equal the corpus goldens, in a test.
7. The results are under `testdata/benchmark-results/h100-80gb-hbm3/fused/{tpch,tpcds}.sf40/`, in a format
   batching-report reads, documented in the detail file.
8. bench-rows' report page, `testdata/benchmark-report.html`, gains a fused-batches table per
   dataset, read through the fused format's own section reader: per batch, its queries, outcome,
   fused load, execute and accountant's peak (a lower bound), cap, reads per table, and a link to
   the section. The page is regenerated in the commit that adds the fused files.

**Profiles.** Every fused batch this task runs is also profiled on the H100. Both Nsight passes run
(`create_nsys_profile.sh`): the trace pass's `calls.tsv` and pinned-loads' load breakdown, and the
metrics pass's `hbm.tsv`. In each case's profile, a reader sees how much of every load is reading
and copying, against decoding. The derived files are committed per dataset under
`testdata/calibration/h100-80gb-hbm3/fused/`. Captures stay on the host or in the scratch area; they are too large to
commit. A case whose capture fails is listed with the reason, as a failed benchmark case is.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/prototypes/fuser/` (new) | the fuser, the hints, the cap rule, buffer placement |
| `peacockdb-core/tests/prototype_fused_benchmarks.rs` (new test target, features `gpu` + `test-support`; its case code in `src/prototypes/fuser/`) | the runs: eight fixed tests, `bench_fused_{tpch,tpcds}_sf40_{first3,first10,first20,all}_tp1_single`, each reading the H100 `cases.csv` at run time for its batch. Not in `peacock_gpu_benchmarks` |
| `scripts/env/run-gpu.sh`, `scripts/env/stage-install.sh` | stage and run `prototype_fused_benchmarks` by name. No CI step runs it |
| `cost-report/tests/test_source_formatting/` (`ci_coverage.rs`) | the CI wiring guard's explicit "manual, prototype" entry for the target, checked not wired into CI |
| `testdata/benchmark-results/h100-80gb-hbm3/fused/` | results |
| `testdata/calibration/h100-80gb-hbm3/fused/` | the profiles' derived files, one capture per batch |
| `scripts/create_nsys_profile.sh` (`--fused`), `peacockdb-core/src/plan/rebuild.rs` | profiling fused batches; the plan rebuild helper moved out of tests |
| `peacockdb-core/tests/test_corpus_goldens/benchmark.rs`, `testdata/benchmark-report.html` | the page's fused-batches table |

Component-level API: none beyond what dag-driver adds.

## Constraints

- Prototype: the coordinator skips the reviewer and the PR. The code is tested enough to trust
  the numbers: the answer checks in item 6 and a unit test for each hint's arithmetic.
- No planner change for single queries; every existing golden is unchanged.
- **Prototype code lives apart from the engine.** Rust: `peacockdb-core/src/prototypes/<name>/`,
  compiled only under `test-support`, as `test_support/` is, so `cargo build` cannot name it.
  Each prototype's module doc says what it is, that it runs
  at sf40 only, and that no engine module may depend on it. A source-formatting test in
  `cost-report` (`tests/test_source_formatting/`) enforces that no file outside `prototypes/` and
  the tests names `prototypes::`. `architecture.md` gains a short "Prototypes" section with that
  rule.
