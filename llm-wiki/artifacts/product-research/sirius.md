# Sirius — due diligence

- **Verdict: do not rebuild the product on Sirius's engine within the 6-month alpha.** Sirius is far ahead on spilling, caching, I/O, multi-GPU and testing. But a DataFusion-planned product can reach it only through an unstable Substrait FFI. On that path DuckDB re-plans the query, there is no CPU fallback, and two known bugs return wrong rows with no error [1][4][5][13].
- **A maintained fork is out (verified).** Upstream changed ≈1.1 M lines in 6 months, ≈92% of it by NVIDIA staff [10][11]. A one-time hard fork is legal (Apache-2.0, no CLA), but Sirius is 6.8× peacockdb's size, plus 24.6k lines of cuCascade [1][2].
- **Borrow, don't adopt (estimate).** cuCascade (NVIDIA, Apache-2.0) is the reusable piece for peacockdb's missing spill/tier layer [2][19]. A product hosted in DuckDB is a different choice: it would use Sirius directly and drop peacockdb.

Sirius code citations are `path:line` at SHA **b9e05ffc653c8ed031c789e5c1415ac1186e0619** (2026-10-28) [1]. cuCascade citations are at `1a9281093a0fe49634594d541d7ce327129f53d6` [2]. The legacy engine is at `f79a6f42` [3]. All sources were accessed 2026-10-28.

## What Sirius is

- **A GPU executor packaged as a DuckDB extension.** It routes supported SQL to the GPU "via cuDF/RMM/cuCascade" and falls back to DuckDB's CPU for the rest (`AGENTS.md:3-6`) [1].
- **DuckDB keeps the parser, binder, optimizer and catalog.** Sirius translates the optimized `LogicalOperator` tree into ~30 of its own physical operators, which run on a task-based GPU scheduler (`docs/super-sirius/architecture-overview.md`; `src/op/`) [1].
- **Hook points are standard extension APIs, so DuckDB is not forked:**
  - a pre-optimizer and a post-optimizer hook (`src/sirius_extension.cpp:3676-3677`);
  - `OnFinalizePrepare`, which swaps the physical plan for a single `PhysicalSiriusExecution` root and stashes DuckDB's plan as `cpu_fallback` (`src/sirius_context.cpp:1993-2021`) [1].
- **Interception is on by default.** Queries are intercepted automatically (`SET gpu_execution=false` turns it off). `CALL gpu_execution('…')` still exists but is no longer needed (`src/sirius_extension.cpp:3431`; `docs/super-sirius/README.md:5`) [1].
- **Side effect:** loading Sirius disables 5 DuckDB optimizer rules for the whole database, CPU queries included (`src/sirius_extension.cpp:3582-3595`) [1].
- **Hardware and OS:**
  - Linux amd64/arm64 with glibc ≥ 2.28;
  - NVIDIA Turing or newer (sm_75+);
  - CUDA 12.x or 13.x;
  - `io_uring`, and O_DIRECT for local Parquet;
  - no Jetson support (`docs/README.md:34-39`; `sirius-duckdb/README.md:105`) [1].
- **Pins:**
  - libcudf 26.10.02 (`pixi.toml:100-101`);
  - DuckDB submodule on the unreleased branch `v2.0-cyanoptera`, at `d54cbbe3` (`.gitmodules`);
  - the extension "requires the exact DuckDB revision pinned" (`docs/README.md:106-108`) [1].
- **Versions:**
  - CMake says `VERSION 0.0.0` (`CMakeLists.txt:21`) [1];
  - only two mutable pre-release tags exist: `stable` (DuckDB 1.5.6) and `latest` (DuckDB 2.0.0), both published 2026-10-02 [9].
- **Two engines, one deleted.** "Super Sirius" is the live engine. The legacy engine produced the ClickBench and paper results; it was deleted on 2026-09-30 (PR #1055, −39,427 lines) and survives only as branch `legacy-dev` (`docs/README.md:31-32`) [1][14].
- **The paper describes the old design.** The CIDR 2026 paper describes an in-memory prototype fed by Substrait. The current code captures DuckDB plans directly and adds out-of-core and multi-GPU execution. **The code is current; the paper is stale** [6][12][1].

## Cost-based optimizer

- **Sirius has no optimizer and no GPU cost model of its own.** Join order, pushdown, decorrelation and cardinalities all come from DuckDB. The post-optimizer hook copies DuckDB's plan and its estimates (`src/transparent/sirius_optimizer_extension.cpp:99-143`) [1]. The paper agrees (§4) [6].
- **The choice between GPU and CPU is about capability, not cost.** Whatever `create_plan()` can translate runs on the GPU, so tiny queries go there too. `gpu_admission.cpp` does no costing (`src/planner/gpu_admission.cpp:26-52`) [1].
- **Physical join choice is fixed.** Any equality predicate gives a hash join, and the build side is DuckDB's right side (`docs/super-sirius/physical-plan-generation.md:46-55`). Merge and inequality join selection is commented out (`src/planner/sirius_plan_comparison_join.cpp:754-782`) [1].
- **What Sirius adds at plan time** (local rewrites, not costed):
  - projection folding;
  - dense-count-join detection from DuckDB's estimates (`src/planner/sirius_plan_aggregate.cpp:424-480`);
  - wiring of dynamic (SIP) filters from build sides to scans (`src/planner/sirius_plan_comparison_join.cpp:581-648`);
  - a late-materialization pass [1].
- **What Sirius adds at run time** (adaptive choices; `docs/super-sirius/optimizations.md:97-112,192-204`) [1]:
  - BUILD_PROBE vs partitioned hash join, under a 500 MB build cap;
  - broadcast of small build sides;
  - a MARK-join build-side switch at an 8× row ratio.
- **The GPU cost:** nothing tells DuckDB that it is planning for a GPU. The rules DuckDB cannot use for GPU plans are switched off (`src/planner/unsupported_optimizers.hpp:25-32`) [1]. Plan quality is DuckDB's CPU-tuned quality (estimate).
- **On the embedder path, the build side comes from caller-supplied row counts** (`Fragment::declare_input_cardinality`, "Undeclared streams plan as 1 row"; `include/sirius/ffi.hpp:115-118`) [1].

## Cache in RAM and VRAM

- **The current engine has no automatic cross-query cache.** "Sirius reads table data from storage on every query" (`docs/README.md:150-153`) [1].
- **`pin_table` is an explicit SQL call.** It materializes chosen columns of a Parquet file or DuckDB table once, into VRAM (`tier='gpu'`) or pinned host RAM (`tier='host'`). Queries then match the pinned copy automatically, and it lasts until `unpin_table` (`docs/README.md:156-173`) [1].
  - **Format:** decoded cuDF columns. Simpatico compression (bitpack, ANS, delta, dictionary, snappy via nvCOMP) is optional and chosen per chunk (`docs/super-sirius/compressed-pinning.md:1-25`) [1].
  - **Pruning:** per-chunk zone maps skip pinned chunks (`docs/super-sirius/configuration.md:899-906`) [1].
  - **No eviction.** Pinned GPU entries cannot be spilled: an under-reserved pinned batch "deadlocks" (`src/op/scan/sirius_gpu_scan_operator_data.cpp:555-560`) [1].
  - **Pins live in memory only** (estimate). No on-disk persistence was found.
  - **DuckDB-native pins:** `UPDATE` and `MERGE` are rejected while a table is pinned, and a `CHECKPOINT` invalidates the pin (`docs/README.md:175-179`) [1].
- **Measured effect** (GB300, TPC-H SF1000 hot, Sirius's own numbers). All host-pinned uncompressed takes 20.74 s and all host-pinned compressed 22.20 s. Lineitem compressed on the GPU takes 17.05 s. Compression shrinks the 7 hottest lineitem columns from 264 GB to ~93 GB (`docs/super-sirius/compressed-pinning.md:46-69`) [1].
- **File-byte prefetch cache:**
  - O_DIRECT reads into pinned host blocks drawn from cuCascade's HOST tier;
  - LRU or idle-drop eviction at 0.8 of the pool, keyed by file identity;
  - **off by default** (`mode: none`; `src/io/cache/config.hpp:26-115`; `src/io/cache/types.cpp:36-70`) [1];
  - not used with the kvikio backend (`docs/super-sirius/configuration.md:446-447`) [1];
  - the downgrade executor cannot reclaim its memory yet (PR #1933 open) [14].
- **Intermediate results are not cached across queries.** Data repositories are query-scoped (`docs/super-sirius/data-management.md:58-70`) [1].
- **The legacy engine cached every scanned column automatically** in a pre-allocated GPU region, and the data had to fit (`docs/gpu_processing.md:3,37-45` at `f79a6f42`) [3][7]. **That is no longer current.**
- **Footprint:** by default Sirius claims 95% of VRAM and up to 90% of each NUMA node's RAM as pinned memory (`src/sirius_config.cpp:540,600`). The README's "8 GiB per NUMA node" is stale (`docs/README.md:222`) [1].

## Spilling to host

- **Tiers come from cuCascade:** GPU, then pinned HOST (1 MiB NUMA blocks), then DISK. Thresholds are trigger 0.8 / stop 0.6 on the GPU and 0.9 / 0.8 on the host (`src/sirius_config.cpp:540-543,600-605`; cuCascade `docs/memory-management.md:37-61`) [1][2].
- **Reservations:** each task reserves memory before it runs. Allocation goes through cuCascade's `reservation_aware_resource_adaptor` over an RMM async pool. A shortfall raises `oom_reschedule_exception` (`docs/super-sirius/memory-management.md:77-93`) [1].
- **Downgrade executor:** there is one per memory space. It moves *idle* `cudf::table` batches, held in inter-pipeline repositories or queued tasks, to a lower tier. It runs on demand or from a 10 ms monitor, picking the smallest batch that covers the deficit (`docs/super-sirius/memory-management.md:99-163`). Batches come back up lazily when a task needs them [1].
- **Out-of-core operators work Grace-style.** Joins, aggregates and sorts hash- or range-partition into ~768 MiB partitions whose batches can spill (`docs/super-sirius/operators.md:150-160,392-531`) [1].
- **State inside a running task never spills** (hash tables, groupby state, Bloom arrays). The task is rescheduled with a bigger reservation, up to 100 retries, and then the query fails (`src/pipeline/gpu_pipeline_executor.cpp:464-494`). Only the DuckDB-extension path can then replay the query on the CPU [1].
- **Disk spilling is off by default** (empty `downgrade_root_dirs`; `docs/super-sirius/configuration.md:178-186`) [1].
- **Spill compression** is optional and off by default. It merged on 2026-10-27 (PR #2204) [14].
- **The spill path is young:**
  - in place since ~2025-12, with cuCascade adopted on 2026-01-09 [1];
  - no preload of spilled data yet (PR #2361 open) [14];
  - exchange spilling is not done (PR #2376 open) [14];
  - the first reservation estimate is 8× too large, causing needless spills (issue #2301) [14];
  - no published benchmark measures a run with active spilling (gap).

## Substrait

- **What it takes:** a binary Substrait `Plan` protobuf (logical relations only), pinned to spec v0.89.0. The entry points are `ffi::Context::execute_substrait(bytes, ArrowArrayStream*)` and `ffi::Fragment::build(bytes)` (`include/sirius/ffi.hpp:67-73,96-146`; `src/sirius_ffi.cpp:123`) [1][4].
- **The DuckDB extension takes no Substrait.** `gpu_execution` binds SQL text (`src/sirius_extension.cpp:478-505`) [1].
- **DuckDB stays in the loop.** The chain is Substrait → the forked duckdb-substrait consumer → DuckDB `Planner` → DuckDB `Optimizer` (extra rules off) → Sirius planner (`src/sirius_ffi.cpp:106-147`) [1][4]. A native consumer is only proposed (#2192, on top of #1750) [14].
- **Accepted relations:**
  - Read, from a named table, parquet `local_files`, a virtual table or Iceberg;
  - Filter, Project, Aggregate and Sort;
  - Fetch, but **deprecated fields only**;
  - Join (no ANTI), Cross, and 2-input Set;
  - ConsistentPartitionWindowRel, Write and Reference.
  - Source: `from_substrait.cpp:1275-1306,539-575,801-1035` [4].
- **Rejected:** extension relations, Exchange, Expand, the physical joins, subquery and window-function expressions, and type arguments [4].
- **Functions match by bare name.** URIs are ignored and 15 names are remapped (`from_substrait.cpp:46-104`) [4]. In effect the consumer speaks DuckDB-dialect Substrait.
- **Types:** most scalars, decimal, date, timestamp (tz only at µs), and list/map/struct. Literals are missing i16, timestamp, binary and nested types (`from_substrait.cpp:107-195,325-409`) [4].
- **Then the GPU planner is a second gate,** and the FFI path has **no CPU fallback** (`src/sirius_ffi.cpp:200-226`) [1].
- **DataFusion 45's stock producer vs this consumer** (code reading, not executed) [5][4]:
  - **LIMIT n becomes LIMIT 0, a silent wrong result.** DF45 writes `count_expr`, which the consumer ignores. This is open issue #1963, and the fix is not in the pinned fork [14].
  - **SELECT DISTINCT loses its grouping, a silent wrong result** (estimate). DF45 uses a field that v0.89 removed.
  - **Anti joins, window expressions and leftover subqueries are hard errors.** LIKE has 3 arguments, which likely fails to bind (estimate).
  - **`Measure.filter` and aggregate phase are dropped silently** (estimate). DataFusion's producer also writes an unspecified phase [16].
  - **Function names and reads must be rewritten.** Names must be DuckDB's. `NamedTable` reads fail in the empty catalog unless rewritten to `local_files` or `sirius_stream_<id>`.
  - Corroboration: in the dated Substrait cross-producer matrix, DataFusion→DuckDB passes 66 of 101 function tests [15].
- **Non-DuckDB frontends exist only as experiments.** Each is a hand-written Rust translator from the host's Thrift fragments to DuckDB-dialect Substrait:
  - **Doris:** contributed by the Apache Doris PMC chair (PR #1842, merged 2026-10-06). TPC-H 22/22 run on the GPU. It is single node and one query at a time, with no interrupt (issue #2025) [14].
  - **StarRocks:** led by NVIDIA maintainers, started 2026-05-28. It translates per fragment, and the README still says "coming soon" (`docs/README.md:13`) [1].
  - Neither Doris nor StarRocks emits Substrait itself [1].
  - **No DataFusion, Calcite, Ibis or Gluten frontend exists**, in the tree or in the issues [14].

## API surface

- **The public API is small.** `include/sirius/ffi.hpp` is 216 lines (334 across all public headers) and defines two classes [1]:
  - `Context` runs a whole Substrait query and returns Arrow C stream output;
  - `Fragment` supports multi-fragment runs: declared input streams, hash or broadcast outputs, `relay_from`.
- **No narrow plan fragments below Substrait.** There is no call to run a single physical operator or to hand in or take out GPU-resident cuDF tables (estimate: no device entry point in `ffi.hpp`). Results cross an Arrow C stream boundary [1].
- **No narrow optimizer calls.** `Fragment::build` = lower, optimize with DuckDB, and plan, in one call. The only optimizer input is `declare_input_cardinality` (`include/sirius/ffi.hpp:115-118,140-146`) [1].
- **Library vs extension:**
  - today the "library" is the extension `.so` itself; Rust's `build.rs` symlinks it as `libsirius.so` (`rust/README.md:52-61`) [1];
  - the standalone library (#1728), separate packaging (#1734) and a physical plan builder (#1750) are all open [14].
- **Rust bindings are proof-of-life.** They cover context creation and `execute_substrait` only, with `Fragment` unbound and **one context per process** (`rust/crates/sirius/src/lib.rs:7-9,27-29`) [1].
- **Concurrency:** `build`, `run` and `execute_substrait` run one at a time per `Context` (`include/sirius/ffi.hpp:93-95`) [1].
- **The cost of lowering can exceed execution.** TPC-H Q21 SF100 spent 10 s lowering against 4.6 s executing, before a footer cache was added (`src/sirius_ffi.cpp:190-197`) [1].

## Activity and governance

- **Cadence:** 1,244 commits on `main` in 12 months (1,111 human). The pace is accelerating: 848 commits in the last 6 months, and 209 in Oct 2026 from 23 authors [10].
- **Churn:** +780k / −320k lines in 6 months, all files included [11].
- **Contributors:** 44 human authors in 12 months. **≈92% of human commits are by NVIDIA-listed maintainers**, ≈4% by UW–Madison and ≈4% by others [10]. 20 of 23 maintainers list NVIDIA (`MAINTAINERS.md`) [1], and the project lead now lists "UW-Madison, NVIDIA" [20].
- **Releases:** no versions, only the mutable `stable` and `latest` pre-releases (`docs/RELEASING.md`) [1][9]. There is no semver or API-stability statement [1].
- **Licence:** Apache-2.0 since 2025-06-25 (MIT before) [20]. There is no CLA or DCO (`CONTRIBUTING.md`) [1].
- **Governance:** lazy consensus among maintainers, with the aim of eventually moving to a neutral foundation. There is no cap on seats per employer (`GOVERNANCE.md`, added 2026-07-24) [1].
- **Roadmap:** there is no roadmap file and no milestones; direction lives in tracking issues [14]:
  - #545 decouple from DuckDB;
  - #1728 standalone library;
  - #1750 physical plan builder;
  - #1807 exchange;
  - #2025 Doris.
- **Responsiveness (last 6 months):**
  - external PRs: median first response 16.7 h, 31 of 38 closed ones merged;
  - external issues: median first response ~4.5 days, and 28 of 31 are still open [14];
  - a ROCm backend PR was closed (#1153) [14].
- **NVIDIA's stance:** the blog says "Led by the University of Wisconsin-Madison with support from NVIDIA" and mentions no commercial offering [8]. No product page, price or no-compete statement was found. The search was cut short (gap) [8][18].
- **Is a fork really out? Yes, as a maintained fork (verified).** One person plus two weekend contributors cannot rebase onto ≈50–60k changed lines a week. The churn sits in the planner, executor and scan code [11]. The engine was swapped out in Sep 2026 [14], and DuckDB is moving from 1.5 to 2.0 [1].
- **A one-time hard fork is legal but over budget.** It means owning 154k lines plus 24.6k of cuCascade, about 7.8× peacockdb, against a budget of 1–2× new engineering [1][2].
- **What building on top looks like (estimate):**
  - pin a `main` SHA, since the tags are mutable;
  - write a DuckDB-dialect Substrait emitter;
  - keep your own CPU fallback and a differential test harness;
  - absorb breaking changes until #1728 and #1750 land;
  - send missing operators upstream (external PRs close in ~5 days median) [14].
- **The overlap risk is real (estimate).** NVIDIA is building the same thin layers itself: the library, exchange, and Doris and StarRocks hosts [14].

## peacockdb vs Sirius

The cost column is for the side that is behind, in yardstick lines, and every figure is an estimate. "pdb" means peacockdb closing the gap to Sirius. "Sir" means a Sirius-based product closing the gap to peacockdb. peacockdb facts come from the inventory [21], and the PR #165 facts from its branch [22].

| area | peacockdb | Sirius | ahead | cost to close (estimate) |
|---|---|---|---|---|
| SQL coverage | TPC-H 44/44 and TPC-DS 82/99 on device. No window functions, no CPU fallback [21] | TPC-H 22/22 in CI. TPC-DS ~70–72/99 (no window, DISTINCT or set ops); the rest falls back silently to CPU (issue #1744) [14] | pdb on device; Sir on "always answers" | pdb: whole-query fallback through its DataFusion CPU path, 0.3–0.8k. Sir: DISTINCT and set ops upstream, 1–2k |
| optimizer (incl. PR #165) | DataFusion 45 join order, no reordering, trivial cardinalities [21]. PR #165 has DPhyp (287 Rust lines), estimation, dynamic filters and replanning, but only in Python (+5,172 lines). Of 405 TPC-DS runs, 388 got cheaper and 8 dearer, in model bytes [22] | DuckDB's optimizer, with no GPU cost model. Dynamic filters are in the engine (8,665 lines). Runtime choices of join mode and broadcast [1] | Sir in the engine; pdb in GPU-aware design | pdb: port PR #165 (stats layer, rewrite pass, replanning in the driver), 3–6k |
| cache | None. Each batch re-reads its row groups, and warm runs ride the OS page cache [21] | `pin_table` in VRAM or host (decoded, optional compression) plus a pinned file-byte prefetch cache, off by default (11,116 lines with the scan manager) [1] | Sir | pdb: a minimal pin cache (VRAM or host) plus pinned host buffers, 2–4k |
| spilling | None. Builds, sorts and aggregate state must fit, or the query fails [21] | GPU → host → disk through cuCascade. Grace-style partitioned operators; task state is not spillable [1][2] | Sir | pdb: adopt cuCascade, add tier-movable batches and partitioned join, aggregate and sort, 5–10k |
| multi-GPU | Test binaries only; no engine path [21] | Single-node multi-GPU on `main`, with 2×T4 CI. Distributed execution in open PR #2337, one host only so far [1][14] | Sir | pdb: worker per GPU, per-device pools, shuffle node, 3–6k |
| ingestion and scan | Local Parquet, one file per table, `read_parquet`, which is slower than the chunked reader (#232) [21] | cuDF Parquet over io_uring/O_DIRECT, S3, kvikio, Iceberg deletes, DuckDB-native GPU decode (44,678 lines) [1] | Sir | pdb: chunked reader, pinned buffers, multi-file tables, 0.5–1.5k. Full parity is 40k+ and not needed |
| plan IR | DataFusion physical plan → own node set → frozen FlatBuffers recipe (2,707 lines) [21] | DuckDB `LogicalOperator` inside; DuckDB-dialect Substrait in over FFI [1][4] | pdb for a DataFusion product | Sir: a DF45 → DuckDB-dialect Substrait emitter with fixes, 3–7k (Doris shim 7,340, StarRocks 6,616) |
| extensibility | Owned code; a `Backend` trait seam; 25-symbol C ABI [21] | 216-line FFI. No physical plan builder (#1750) and no device-table in/out. One context per process in Rust [1][14] | pdb | Sir: wait for #1728/#1750, or contribute upstream; not costable |
| maturity and tests | 57,659 test lines; DuckDB-oracle corpus over 5 modes; GPU checks run on a separate host [21] | 206,598 test lines, 5,056 test cases, GPU CI on every PR (CUDA 12 and 13), a SQL fuzzer [1] | Sir | pdb: a GPU CI runner is infrastructure, not lines |
| performance evidence | Engine end to end on H200 sf40 for 3 cases only (q6 48.2 ms). Other numbers come from hand-written cuDF pipelines [21] | Hot TPC-H SF100–1000 runs 2.7–5.7× faster than DuckDB on AWS. SF1000 in 8.18 s on GB300 with patched cuDF. All runs hot [1][17] | Sir | pdb: measurements, not lines |

## Sirius components

**Counting rule:**
- the same as peacockdb's yardstick: tracked `.rs .cpp .cu .h .hpp .cuh .fbs .cc .c .cxx .inl .proto`, non-blank and non-comment;
- test code = files under `test`/`tests`/`gpu_tests`, `*_test.*`, `test_*.*`, and inline `#[cfg(test)]`;
- submodules and build-time generated code are excluded;
- Python is excluded (25,959 lines) [1].

| Component (Sirius @ b9e05ffc) | yardstick | test |
|---|--:|--:|
| Scan operators (Parquet, DuckDB-native, Iceberg ingestibles) | 18,767 | 28,111 |
| Scan manager + pin cache | 11,116 | 15,899 |
| I/O layer (io_uring, kvikio, S3, prefetch cache) | 10,786 | 13,856 |
| Pipeline / task executor / scheduler | 10,301 | 14,517 |
| Dynamic (runtime join) filters | 8,665 | 14,704 |
| Context, config, common | 8,198 | 6,936 |
| Physical plan generation (from DuckDB plans) | 7,985 | 9,749 |
| Filter/projection + expression evaluator | 6,021 | 8,493 |
| DuckDB extension glue + interception | 5,539 | 2,202 |
| Join operators | 4,912 | 4,289 |
| DuckDB-native GPU decode kernels | 4,009 | 0 |
| Operator base + plumbing (partition, concat, union, CTE) | 3,987 | 6,203 |
| Memory tiers / spill / batches | 3,181 | 4,790 |
| Aggregate operators | 2,923 | 3,625 |
| Late materialization | 2,663 | 3,735 |
| Streaming fragments + C++ FFI (Substrait entry) | 2,133 | 1,317 |
| Compression integration | 1,954 | 2,320 |
| Sort / top-N / limit | 1,723 | 1,482 |
| Vector search (cuVS) | 1,145 | 1,687 |
| Logging + telemetry (C++) | 994 | 35 |
| Window (not wired to the planner, #1745) | 456 | 694 |
| Rust bindings | 177 | 194 |
| Integration tests + harness | 0 | 35,869 |
| **Subtotal: DuckDB-hosted engine** | **117,635** | **171,707** |
| Simpatico GPU compression (in-tree) | 17,635 | 11,530 |
| `experimental/doris` | 7,340 | 6,247 |
| `experimental/starrocks` | 6,616 | 7,163 |
| Rust telemetry crates | 4,886 | 951 |
| Tools | 101 | 0 |
| **Sirius total** | **154,213** | **206,598** |
| cuCascade @ `1a928109` (submodule, not in total; 21,908 without benchmarks) | 24,600 | 15,787 |
| Legacy engine `src/legacy/` @ `f79a6f42` (deleted from main) | 19,323 | n/a |

**Ratios to peacockdb's 22,830 lines:**

| Scope | Ratio |
|---|--:|
| Sirius total | **6.8×** |
| Engine subtotal | 5.2× |
| Total + cuCascade | 7.8× |
| Legacy engine | 0.85× |
| Engine without the scan, I/O and cache stack (≈73k, estimate) | ≈3.2× |

- **Scan, I/O and cache make up 29% of the total** (44,678 lines). The relational operators come to only ≈16k [1].
- **cuCascade duplicates part of Sirius's I/O.** It has its own io_uring/S3 stack (9,363 lines) beside Sirius's `src/io` [2].

## Evidence against

- **The integration path is the weakest part of Sirius, and it is the part this product needs.** Every non-DuckDB host goes through a forked DuckDB Substrait consumer [4][14]:
  - two failure modes return wrong answers with no error (LIMIT→0, issue #1963; DISTINCT, estimate);
  - DuckDB re-optimizes the DataFusion plan;
  - there is no CPU fallback.
- **The API is pre-release by its own account.** The library is a symlinked extension, the Rust side allows one context per process, and the Doris README says the boundary is "being designed" [1]. The fix (#1728, #1750, #2192) has no timeline [14].
- **The human's knowledge would be thrown away.** Building on Sirius swaps 22.8k lines the human knows for 154k they do not. Control passes to a ≈92% NVIDIA team that ships ≈200 commits a month, has no versions, and is mid-way through a DuckDB 1.5 → 2.0 move [1][10].
- **The headline performance belongs to the deleted engine or to hot, pinned runs.** The ClickBench results come from the legacy engine [1]. The GH200 cold sum is 208.5 s against a hot sum of 1.45 s [17]. No published run measures the current engine out of core with active spilling [1].
- **Coverage is not ahead.** About 72/99 TPC-DS, against peacockdb's 82/99. Window functions and FILTER aggregates are refused [1][14].
- **Out-of-core is about 10 months old and still being tuned.** Reservations are 8× too large (#2301), and preload and exchange spilling are open [14].
- **The overlap is direct.** NVIDIA is building the same thin layers itself (standalone library, exchange, Doris and StarRocks hosts). A small product on Sirius competes with whatever vendors build on the same core (estimate) [14].
- **Hard platform pins:** NVIDIA only, `io_uring`, O_DIRECT, an exact DuckDB revision, and a 95% VRAM / 90% RAM default claim [1].

## Follow-up experiments

1. **DataFusion 45 Substrait → Sirius's consumer, on the CPU.** Run DF45's TPC-H and TPC-DS plans through the fork's `from_substrait` at `922489d`. Count hard errors and diff results against DuckDB. This confirms or refutes the LIMIT, DISTINCT, LIKE and FILTER issues. No GPU is needed [4][5].
2. **Sirius on the first user's data, out of core.** Use the `stable` extension, data at 2–5× VRAM, and disk spill configured. Measure cold and warm time, spill bytes and failures against DuckDB CPU and peacockdb [1].
3. **Lowering overhead.** Time Substrait → DuckDB → Sirius plan per query against execution, at SF1 and SF100. The Q21 precedent was 10 s against 4.6 s [1].
4. **cuCascade standalone inside peacockdb.** Link cuCascade at `1a92810` and spill a join build through its HOST tier. This tests the "borrow, don't adopt" path [2].
5. **Pin vs no pin on PCIe.** Repeat Sirius's host and GPU pin comparison on a PCIe H100 or H200, not GB300 C2C. This shows whether host pins pay off without the C2C link (estimate) [1].
6. **Fragment input from outside Sirius.** Check whether external Arrow batches can feed a `Fragment` stream. If not, interleaving with peacockdb's executor needs host round-trips (gap) [1].

## Sources

1. Sirius source tree at b9e05ffc653c8ed031c789e5c1415ac1186e0619 (all `path:line` citations): https://github.com/sirius-db/sirius/tree/b9e05ffc653c8ed031c789e5c1415ac1186e0619 (accessed 2026-10-28)
2. cuCascade at 1a9281093a0fe49634594d541d7ce327129f53d6: https://github.com/NVIDIA/cuCascade/tree/1a9281093a0fe49634594d541d7ce327129f53d6 (accessed 2026-10-28)
3. Sirius legacy engine archive `legacy-dev` at f79a6f42: https://github.com/sirius-db/sirius/tree/f79a6f423fe69ef892ca0af03bef9c352d88cc79 (accessed 2026-10-28)
4. Sirius fork of duckdb-substrait at 922489d3: https://github.com/sirius-db/duckdb-substrait-extension/blob/922489d3c8c77140b189ad3847554de141378b04/src/from_substrait.cpp (accessed 2026-10-28)
5. datafusion-substrait 45.0.0 producer: https://docs.rs/crate/datafusion-substrait/45.0.0/source/src/logical_plan/producer.rs (accessed 2026-10-28)
6. Yogatama et al., "Rethinking Analytical Processing in the GPU Era", arXiv 2508.04701v3 (CIDR 2026): https://arxiv.org/pdf/2508.04701v3 (accessed 2026-10-28)
7. Same paper, v2 (§3.2.3 buffer manager, future work): https://arxiv.org/pdf/2508.04701v2 (accessed 2026-10-28)
8. NVIDIA Technical Blog, Sirius ClickBench record, 2025-12-15: https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/ (accessed 2026-10-28)
9. Sirius GitHub releases: https://github.com/sirius-db/sirius/releases (accessed 2026-10-28)
10. Sirius commits since 2025-10-28 (GitHub API): https://api.github.com/repos/sirius-db/sirius/commits?since=2025-10-28T00:00:00Z (accessed 2026-10-28)
11. Sirius code frequency (GitHub API): https://api.github.com/repos/sirius-db/sirius/stats/code_frequency (accessed 2026-10-28)
12. CIDR 2026 paper page: https://vldb.org/cidrdb/2026/rethinking-analytical-processing-in-the-gpu-era.html (accessed 2026-10-28)
13. Issue #1963, FetchRel count_expr ignored: https://github.com/sirius-db/sirius/issues/1963 (accessed 2026-10-28)
14. Sirius issues and PRs cited by number, at https://github.com/sirius-db/sirius/issues/<n> or https://github.com/sirius-db/sirius/pull/<n>: #545, #1055, #1153, #1728, #1734, #1744, #1745, #1750, #1807, #1842, #1933, #2025, #2192, #2204, #2301, #2337, #2361, #2376; responsiveness from the GraphQL API over all issues and PRs (accessed 2026-10-28)
15. Substrait consumer-testing results: https://raw.githubusercontent.com/substrait-io/consumer-testing/main/app/consumer_results.csv (accessed 2026-10-28)
16. apache/datafusion#25100, producer emits unspecified aggregate phase: https://github.com/apache/datafusion/issues/25100 (accessed 2026-10-28)
17. ClickBench Sirius results 2026-03-09: https://github.com/ClickHouse/ClickBench/tree/main/sirius/results/20260309 (accessed 2026-10-28)
18. Sirius project site: https://sirius-db.com (accessed 2026-10-28)
19. cuCascade repo metadata: https://github.com/NVIDIA/cuCascade (accessed 2026-10-28)
20. Commits b2961f8f (maintainer affiliations) and c9c0d81f (licence to Apache-2.0): https://github.com/sirius-db/sirius/commit/b2961f8f , https://github.com/sirius-db/sirius/commit/c9c0d81f (accessed 2026-10-28)
21. peacockdb inventory at master f25c250d: `llm-wiki/artifacts/product-research/inventory.md` (local repo; accessed 2026-10-28)
22. peacockdb PR #165, branch `origin/ENS-optimizer` at 561411b4: `scripts/exec_model/optimizer.md`, `peacockdb-dphyp/design.md` (local repo; accessed 2026-10-28)
