# Engine inventory — what exists, sized

Master at `f25c250d5f025c78508b210fa29094bbbdec662e`. Every path is relative to the repo root.
The component map is `llm-wiki/architecture.md`. This page sizes each component so a product's
cost can be stated as a fraction of what exists.

## How lines were counted

- No tokei, cloc or scc on the host. A small awk counter did the work: it counts tracked
  (`git ls-files`) `.rs .cpp .cu .h .hpp .cuh .fbs` lines that are not blank and not
  comment-only (`//`, `///`, `//!`, and `/* … */` blocks). Raw `wc -l` over the same files
  is 103,238.
- **Test code**: any file under a directory named `tests`, `gpu_tests` or `ffi_tests`; any file
  named `tests.rs`, `*_tests.rs` or `test_*.rs`; all of `cpp/tests/`; the inline
  `#[cfg(test)] mod tests` in `cost-report/src/main.rs` (lines 1638–2735). The feature-gated
  `peacockdb-core/src/test_support/` (corpus runner, DuckDB oracle, goldens) counts as test code,
  except `test_support/cost_model.rs`, which counts toward the cost model.
- **Generated**: not tracked. flatc writes `gpu_plan_generated.rs` (6,953 lines, per the header of
  `peacockdb-core/src/wire/generated.rs`) and the C++ header into the build dir. The 2-line
  `wire/generated.rs` wrapper is excluded.
- Python (`scripts/`) is not in the yardstick. It is counted only for PR #165, separately.

## Planner

- **Paths**: `peacockdb-core/src/planner/` — `translator/` (incl. `scan_mapping/`, `aggregate/`),
  `memory_estimation.rs`, `pipeline.rs`, `not_in.rs`, `nullability.rs`, `parquet_nulls.rs`.
- **Size**: 2,952 yardstick, 5,066 test. Translator 2,129; memory estimation 239; pipeline 64;
  the rest 520.
- **Does today**: takes DataFusion 45's physical plan and translates it node by node into the
  engine's own tree (architecture.md "Planning"). Unknown node kinds are a plan-time error.
  It maps Parquet row groups to (lane, batch) once at plan time (`scan_mapping/`), prunes row
  groups from footer stats, drops small tables to one lane, inserts shuffles, and derives a batch
  size from a memory budget (`memory_estimation.rs`, "Batch sizing").
- **Maturity**: plans 82/99 TPC-DS, 44/44 TPC-H-family, 77/78 pbench (cost-registry.csv). Cardinality
  estimates are trivial: one row per input row for aggregates (architecture.md "Batch sizing",
  #19). No join reordering; join sides are DataFusion's choice (architecture.md "Joins").
- **Reuse as is**: the DataFusion-to-batched-tree translation, the row-group mapping, the budgeted
  batch sizing. A product inherits DataFusion 45 as its SQL frontend.

## Node vocabulary

- **Paths**: `peacockdb-core/src/plan/` (the nodes, `validate.rs`, `layout.rs`, aggregates, joins)
  and `peacockdb-core/src/plan_text/` (the text renders goldens use).
- **Size**: 4,095 yardstick, 2,871 test.
- **Does today**: 19 node kinds (architecture.md "The node set"): Parquet load, filter,
  project, per-batch sort, sorted merge, coalesce, aggregate (init/merge/final), hash, cross and
  nested-loop join, hash emit, merge, union, interleave, limit, unload. Each node declares its
  lane layout and schema; validation runs before execution.
- **Maturity**: stable; every kind has unit cases. No window node (#143).
- **Reuse as is**: yes — it is backend-neutral and already drives both backends.

## Wire format

- **Paths**: `flatbuffers/gpu_plan.fbs`, `peacockdb-core/src/wire/`, `peacockdb-core/build.rs`.
- **Size**: 2,707 yardstick, 2,787 test (plus ~7k generated lines not counted).
- **Does today**: writes a "recipe" plan: one FlatBuffers node per call a driver will make, with
  post-order seq numbers (architecture.md "The wire format"). 13 wire kinds; 3 have no writer.
  Payload bytes are pinned by `testdata/goldens/recipe-payloads.txt`.
- **Maturity**: deliberately frozen. Each runtime-varying parameter has cost a new ABI symbol
  (architecture.md "What the frozen surface costs").
- **Reuse as is**: yes, for a planner-in-Rust / executor-in-C++ split. It is not a general plan
  interchange format (not Substrait).

## Executor traits and shared executor code

- **Paths**: `peacockdb-core/src/executor/mod.rs`, `errors.rs`, `forwarder.rs`, `row_range.rs`,
  `instrument.rs`.
- **Size**: 722 yardstick, 303 test.
- **Does today**: seven executor categories (Source, Exec, BatchAccumulator, PartitionAccumulator,
  PartitionEmitter, Join, Unload) as associated types on `Backend`. Batches move by value; illegal
  call orders are compile errors (architecture.md "Traits"). Any failed call ends the query.
- **Reuse as is**: yes. It is the seam a new backend plugs into.

## Driver

- **Paths**: `peacockdb-core/src/executor/driver/` — `scheduler.rs` 131, `single_partition.rs` 432,
  `partitioned.rs` 828, `accounting.rs` 168, `index.rs` 122, `measurements.rs` 74, `mod.rs` 65.
- **Size**: 1,820 yardstick, 4,240 test.
- **Does today**: one single-threaded, push-based, deterministic scheduler. It picks the lowest
  runnable node, runs all its lanes, holds a join's probe subtree until the build is done, and
  stops subtrees under a satisfied limit (architecture.md "The scheduling rule", "Early exit").
  Lanes are logical partitions on one device, not threads or GPUs.
- **Memory accounting**: plan-time estimate per node, plus a run-time accountant that pre-checks
  each call's resident + scratch bytes against a budget and fails the query cleanly if over
  (architecture.md "Memory accounting"). No budgeted run survives on real plans today; enforcement
  is exercised only by unit cases over a mock backend (#182, `llm-wiki/tickets/memory.md`).
- **Reuse as is**: the scheduler and accountant, for one query on one device.

## CPU backend

- **Paths**: `peacockdb-core/src/executor/cpu_backend/`, `executor/cpu_batch.rs`.
- **Size**: 2,086 yardstick, 4,376 test.
- **Does today**: runs each node by relaying to DataFusion operators over Arrow batches. It is the
  reference and oracle path, and the only one the CLI runs (`peacockdb/src/main.rs`).
- **Maturity**: runs every query that plans (cost-registry.csv cpu cells: 997 enabled, 8 disabled,
  100 n/a).
- **Reuse as is**: as an oracle. As a product engine it adds nothing over DataFusion itself.

## GPU backend (Rust) and peacockdb-ffi

- **Paths**: `peacockdb-core/src/executor/gpu_backend/` (1,719), `executor/gpu_batch.rs` (71),
  `peacockdb-ffi/src/lib.rs` (156).
- **Size**: 1,946 yardstick, 2,375 test.
- **Does today**: maps each node to C ABI calls. A `GpuBatch` is a `u64` handle into the C++
  session. The source makes one `execute_scan_rowgroups` call per batch.
- **Maturity**: reached only from the test harness (`test_support/gpu_session.rs`,
  `corpus_gpu.rs`) and the benchmark harness (`peacockdb-core/tests/peacock_gpu_benchmarks.rs`).
  There is no CLI flag, server or library entry point for the GPU (`peacockdb/src/main.rs`
  header).
- **Reuse as is**: yes, behind a new entry point.

## C++/cuDF executor

- **Paths**: `cpp/include/peacock_gpu.h` (25 ABI symbols), `cpp/include/peacock/partitioning.hpp`,
  `rmm_pool.hpp`; `cpp/src/` — `node_session.cpp` 627, `expr.cpp` 772, `gpu_executor.cpp` 465,
  `spark_hash_partition.cu` 225, `operators/` 1,883 (join session 1,036 of it).
- **Size**: 4,586 yardstick, 10,577 test. About 4,800 of the test lines are not tests of the
  engine: hand-written cuDF TPC-H/TPC-H+V pipelines used as benchmark vehicles
  (`cpp/tests/gpu/test_tpch*.cpp`, `test_tpchv.cpp`, `tpch_golden.hpp`, `hand_tables.hpp`: 2,443)
  and the multi-GPU path (`multi_gpu.*`, `test_*multi_gpu*.cpp`: 2,375).
- **Does today**: a node-by-node session over cuDF: Parquet scan of given row groups, filter,
  project, sort/merge, groupby aggregate (incl. Welford, grouping sets), a stateful join session
  (build/probe/finish, residuals, all join types), Spark-compatible murmur3 hash repartition,
  limit/slice, export to Arrow. Builds against libcudf 25.02 and 26.02.
- **Reuse as is**: the operator set and the join session. `partitioning.hpp` is Spark-hash
  compatible, which matters for any Spark-adjacent product.

## Cost model and cost-report

- **Paths**: `cost-report/src/main.rs` (lines 1–1637), `peacockdb-core/src/test_support/cost_model.rs`,
  `testdata/cost_model.conf`, `testdata/cost-registry.csv`, `testdata/report.html`.
- **Size**: 1,300 yardstick, 3,848 test.
- **Does today**: not a planner cost model. It is Σ output bytes per node from the CPU goldens,
  compared with bytes DuckDB's profile says it materialized (architecture.md "Cost model and
  DuckDB's cost oracle"). Nothing is timed. It renders the coverage/cost page for CI.
- **Reuse as is**: as a CI coverage report. Not as an optimizer input.

## CLI

- **Paths**: `peacockdb/src/main.rs`. **Size**: 44 yardstick, 0 test.
- **Does today**: plans one SQL query over a directory of Parquet files and runs it on the CPU.
- **Reuse**: none worth naming; a product needs its own front door.

## Shared glue and test infrastructure

- `peacockdb-core/src/lib.rs`, `common.rs`: 171 yardstick. Table registration: each local Parquet
  file becomes a DataFusion `ListingTable` (`lib.rs`).
- `cpp/benchmarks/` (`bench_ram_vram.cu`, `dgx_spark_probe.cu`): 401 yardstick. Bandwidth probes.
- `test_support/` 6,711 and `src/tests/` + `peacockdb-core/tests/` 14,494 test lines. This is a
  DuckDB-oracled corpus harness across 5 modes and 3 datasets. It is the strongest reusable asset
  for correctness work.
- `scripts/exec_model/` on master: a Python model of the scheduler, drivers and accountant
  (3,924 lines + 10,049 test, non-blank non-`#` lines). Not in the yardstick.

## GPU maturity from testdata/cost-registry.csv

Columns: one row per (dataset, query) at sf1. Five plan-mode cells (`tp1_single` … `tp4_sized`),
then `cpu_*` and `gpu_*` cells for the same five modes, each `enabled`, `disabled` or `na`
(not planned). `report.html` calls a query "GPU-operational" when its gpu cells are enabled.

| Dataset | Queries | Runs on device, all 5 modes | Some modes | None | Why not |
|---|--:|--:|--:|--:|---|
| TPC-H (q1–q22 + 22 named shapes) | 44 | 44 | 44 | 0 | — |
| TPC-DS (q1–q99) | 99 | 82 | 82 | 17 | 15 window functions (#32/#143); q27, q72 do not plan on DataFusion 45 (#23) |
| pbench (feature probes) | 78 | 71 | 72 | 6 | float keys -0.0/NaN (#243, 3); LIKE column pattern (#246); nullable IN subquery (#250); computed group key (#264) |

- gpu cells overall: 987 enabled, 18 disabled, 100 n/a, out of 1,105.
- `not_in_uncorrelated` runs at tp1 only (#267).
- All of this is sf1 correctness. Device timing of the engine exists for 3 cases at sf40 only
  (`testdata/benchmark-results/tpch.sf40/`).

## Cross-cutting capabilities

- **Multi-GPU**: none in the engine. "No engine path reaches a second device today"
  (architecture.md "Multi-GPU notes"). A worker-per-GPU pool, per-device RMM pools and a
  cross-GPU hash shuffle exist only in `cpp/tests/gpu/multi_gpu.*` and run hand-written TPC-H
  pipelines. Several queries per process at G≥2 are flaky.
- **Spilling to host**: none. A join build side is coalesced into one batch per lane and must fit
  on device; a sort accumulates every batch and merges once; aggregate state stays resident
  (architecture.md "The node set", "Joins"). Over budget, the query fails; there is no retry with
  smaller batches (#142). The only spill in the tree is DataFusion's own sort on the CPU path.
- **Caching in host RAM or VRAM**: none. Each batch re-reads its row groups from the file
  (`gpu_backend/source.rs`, `cpp/src/operators/scan.cpp`). Nothing stays resident across
  queries. Warm numbers rely on the OS page cache.
- **Allocator**: the engine installs no RMM pool, and `gpu_memory_limit` is accepted and ignored
  (#148). Benchmarks install one themselves. Host buffers for Parquet are pageable, not pinned
  (#149).
- **Memory accounting**: present (see Driver). Plan-time estimate and run-time pre-checks; batch
  sizes derive from a budget (CLI default 2 GiB). Known gaps: #182, #179, #177, #167, #229, #265.
- **How data gets in**: local Parquet files only. One file per table, registered by file stem.
  Row groups are pruned by footer stats and mapped to (lane, batch) at plan time. The device
  reads them with `cudf::io::read_parquet`, which is several times slower than the chunked reader
  (#232). No object store, no other format (CSV/Arrow IPC/Iceberg), no ingest or writes, no
  indexes.
- **SQL surface**: whatever DataFusion 45 parses and plans, minus refused node kinds. TPC-H
  22/22; TPC-DS 82/99 on device (no window functions; `PlaceholderRowExec` refused, #158).
  Read-only SELECT.
- **What else does not exist**: concurrent queries or a server; a GPU entry point outside tests;
  a cost-based or join-order optimizer on master; cardinality estimation; vector search in the
  engine (cuVS appears only in `cpp/tests/gpu/test_tpchv.cpp` and the multi-GPU tests);
  adaptive re-planning.

## PR #165 — join-order optimizer (branch ENS-optimizer, not on master)

- Read at `origin/ENS-optimizer` = `561411b4` (2026-10-21), which is the PR's head per
  `gh pr view 165` on 2026-10-28. Merge base `623073da`; master is 41 commits
  ahead, and the branch has 14 commits. The diff is 434 files, mostly goldens and plots.
- **Size** (non-blank, non-`#` Python lines, docstrings included; Rust as above):
  - `scripts/exec_model/` on the branch: 9,093 source (of which `optimizer/` 2,964) and 8,274
    test. Against the merge base that is +5,172 source lines.
  - `peacockdb-dphyp/`: 287 source (`lib.rs`, `enumerate.rs`, `ffi.rs`) and 218 test.
- **What it does**: reads the engine's plan goldens as text into the Python prototype. It
  estimates each join from stats sidecars and Parquet footers, reorders inner-join clusters by
  DPhyp on C_out, admits non-inner joins via conflict detection (CD-A), prunes fact-scan row
  groups with dynamic filters from a build's key range, and replans when a build misses its
  estimate (`scripts/exec_model/optimizer.md` on the branch). Answers match DuckDB on every corpus
  query.
- **Result**: in its own report (`scripts/exec_model/testdata/goldens/cost_report.html` on the
  branch), optimized runs are cheaper in 80 of 215 TPC-H (query, mode) runs and dearer in 0; for
  TPC-DS, 388 of 405 cheaper and 8 dearer. Cost is pandas bytes from the prototype, not device
  time.
- **How far from the engine**: far. Nothing runs in Rust or on the device. The DPhyp crate is
  ready to link (it has an rlib), but nothing in Rust calls it (`peacockdb-dphyp/design.md`).
  The estimator, dynamic filters and replanning exist only in Python. Porting would mean a
  planner-side stats layer, a plan rewrite pass, and runtime build-then-probe scheduling with
  replanning in the driver. The PR has merge conflicts against master, which has since changed
  its join design (`llm-wiki/tasks/join-optimizer.md` on the branch, "On master as it stands").

## Benchmark evidence

Two caveats first. Most query numbers are **hand-written cuDF pipelines**
(`cpp/tests/gpu/test_tpch*.cpp`), not the engine. "Execute" excludes the Parquet→VRAM load.

1. H200, TPC-H sf40, cuDF execute vs DuckDB warm (128-core VM): q6 19.2 vs 137 ms, q1 240 vs
   915, q3 40 vs 577, q8 48 vs 1126 (benchmark-minimal.md "TPC-H (sf40, H200)").
2. The load is ~20× the execute: 398–741 ms. So q6 end to end is ~419 ms against DuckDB's 137.
   The GPU wins only if the load is amortized across queries ("Reading the comparison honestly").
3. The engine itself, H200 sf40, end to end including the Parquet read, 2nd-min of 10: q6 48.2 ms
   at tp1-single (row-group pruning reads 298 of ~1,954 groups), 132.6 ms at tp4-sized; q19
   321.9 ms (`testdata/benchmark-results/tpch.sf40/`, commit `446962ac`).
4. Vector range queries (cuVS): 12–60 ms on GPU vs 559–1,631 ms DuckDB, but the load is 1.1–2.0 s
   ("TPC-H+V").
5. Getting data to the GPU, H200: H2D pageable 10.6 GB/s, pinned 47.3 GB/s; device-memory read
   4,484 GB/s. Compute-read over load is 95× ("RAM ↔ VRAM, GB10 against H200").
6. Parquet load materializes columns at only 6.8–8.7 GB/s on H200. On q11v, page decode plus
   Snappy is 80% of GPU kernel time ("What the vector loads actually move", "The GPU decompresses
   them anyway").
7. On GB10, `chunked_parquet_reader` loads q6 in 61 ms vs 340 ms for `read_parquet`, which the
   engine uses (dgx-spark.md §2, #232).
8. Multi-GPU (test binaries only), 8×H100 NVLink: q1 8.27×, q6 5.82×, q8 2.31×, q3 1.89× at G=8.
   A cross-GPU shuffle limits q3 (benchmark-minimal.md "Multi-GPU scaling").
9. libcudf 26.02 groupby cliff: 200M rows vs 100M costs 4.6× on H200 and 61× on GB10. Streaming
   in ~8M-row batches avoids it: q1 1.0 s vs 73.5 s (dgx-spark.md §6).
10. One query saturates GB10: 4 concurrent copies give 1.83× throughput at 3.7× latency
    (dgx-spark.md §8).

## Yardstick

Non-test, non-generated, non-blank, non-comment Rust + C++/CUDA + FlatBuffers lines at `f25c250d`.

| Component | yardstick lines | test lines |
|---|--:|--:|
| Planner (translator, memory estimation, pipeline) | 2,952 | 5,066 |
| Node vocabulary (plan/, plan_text/) | 4,095 | 2,871 |
| Wire format (gpu_plan.fbs, wire/) | 2,707 | 2,787 |
| Executor traits + shared | 722 | 303 |
| Driver (scheduler, single_partition, partitioned, accounting) | 1,820 | 4,240 |
| CPU backend | 2,086 | 4,376 |
| GPU backend (Rust) + peacockdb-ffi | 1,946 | 2,375 |
| C++/cuDF executor (cpp/include, cpp/src) | 4,586 | 10,577 |
| Cost model + cost-report | 1,300 | 3,848 |
| CLI | 44 | 0 |
| Core glue (lib.rs, common.rs) | 171 | 11 |
| Bandwidth probes (cpp/benchmarks) | 401 | 0 |
| Test harness (test_support) | 0 | 6,711 |
| Cross-cutting tests (src/tests, tests/) | 0 | 14,494 |
| **Total** | **22,830** | **57,659** |
| PR #165, not in total: `scripts/exec_model` Python (branch state; +5,172 vs base) | 9,093 | 8,274 |
| PR #165, not in total: `peacockdb-dphyp` Rust | 287 | 218 |
