# Sirius (github.com/sirius-db/sirius): size by component, SQL coverage, multi-GPU, ingestion, tests, performance

Code facts are cited as `path:line` at SHA `b9e05ffc653c8ed031c789e5c1415ac1186e0619` (commit date 2026-10-28), in the clone
`/tmp/claude-1000/-media-data-peacockdb/ba2658c9-3cbe-4aa7-bcb8-c0a874d6fde0/scratchpad/ext/sirius`, a shallow clone of depth 200. Web facts carry a URL and "(accessed 2026-10-28)".
Repository metadata from `gh api repos/sirius-db/sirius` (accessed 2026-10-28): created 2024-06-19, Apache-2.0, 1,101 stars, 145 forks, 350 open issues and PRs, 46 contributors (GitHub contributors API). The 200 commits in the clone run from 2026-10-02 to 2026-10-28, about 8 a day, by 24 distinct authors.

## Q1. Sirius size by component, in peacockdb yardstick units

### Takeaway
At HEAD, Sirius has **154,213 yardstick lines** (225,577 raw `wc -l`) and **206,598 test lines**. That is 6.8× peacockdb's 22,830 yardstick lines and 3.6× its 57,659 test lines.
The DuckDB-hosted single-node engine alone has **117,635 lines**, 5.2× peacockdb. That count excludes the in-tree Simpatico compression library (17,635), the experimental Doris/StarRocks Rust shims (13,956), the Rust telemetry crates (4,886) and one tool (101).
cuCascade, the NVIDIA tiered-memory library pulled in as a submodule, adds another 24,600 yardstick lines (21,908 without its benchmarks) and 15,787 test lines.
The biggest single area is scan and ingestion: scan operators, native decode kernels, the scan manager and pin cache, and I/O add up to 44,678 lines, 29% of the total. The relational operators (filter/projection/expressions, join, aggregate, sort, window) add up to only about 16k.

### How the lines were counted (same rule as the peacockdb yardstick)
- **Files:** `git ls-files` with extensions `.rs .cpp .cu .h .hpp .cuh .fbs .cc .c .cxx .inl .proto`. Of the extra extensions, only `.h` (4 files) and one `.c` file (`src/compression/simpatico_codegen/src/util/isoc23_compat.c`) occur. There are no tracked `.cc`, `.cxx`, `.inl`, `.proto` or `.fbs` files. In total 1,379 files.
- **Lines:** counted per file with `scratchpad/sloc.awk`: lines that are not blank and not comment-only (`//`, `/* … */`).
- **Test code:**
  - any file under a directory named `test`, `tests` or `gpu_tests`;
  - any file named `*_test.*`, `test_*.*` or `tests.rs`;
  - inline Rust `#[cfg(test)]` items. These were split out by brace-matching (34 `.rs` files had them; mostly the experimental Doris/StarRocks crates).
  - All of `test/` counts as test, including `test/io/*` (3,802 lines). Those files are I/O micro-benchmarks, not tests.
- **Generated or vendored:** nothing generated or vendored is tracked.
  - Submodules (duckdb, substrait, cucascade, doris, starrocks/brpc, vcpkg, extension-ci-tools, sirius-duckdb/duckdb, .claude/claude-tools) are not counted.
  - Thrift, protobuf and Substrait bindings are generated at build time into `OUT_DIR` or the build dir and are untracked (e.g. `experimental/starrocks/src/proto.rs` is a 12-line `include!` wrapper).
  - `cccl_embedded_headers.h` and `embedded_header.h` are hand-written declarations (14 lines, included); the generated `.cpp` they declare is untracked.
  - `src/compression/simpatico_codegen/` (Simpatico) is developed in-tree by the same team, not vendored: it is built and tested as part of Sirius according to its own README. It is counted, but as a separate row.
- **Mapping:** `scratchpad/sirius_sloc/map.py` applies ordered regex rules; the first match wins. The per-file result is in `scratchpad/sirius_sloc/mapped.tsv` and the per-file raw counts in `scratchpad/sirius_sloc/files.tsv`.

**File-to-component mapping, non-test files (first match wins):**
1. `src/compression/simpatico_codegen/**` → Simpatico.
2. `src/compression/*` → Compression integration.
3. Dynamic filters: `src/op/dynamic_filter/**`, `src/planner/dynamic_filter/**`, `src/cuda/dynamic_filter*`, `src/cuda/sirius_dynamic_*`, `src/op/scan/dynamic_filter_*`, `src/op/scan/sirius_physical_dynamic_filter*`.
4. Late materialization: `src/late_mat/**`, `src/planner/late_mat_plan_pass*`, `src/scan_manager/late_mat_resolver*`.
5. Streaming fragments + FFI: `src/exec/{stream_*,streaming_fragment*,batch_stream*}`, `src/op/sirius_physical_streaming_{source,sink}*`, `src/sirius_ffi.cpp`, `include/sirius/**`, `src/planner/substrait_scan_ranges*`.
6. `rust/crates/sirius{,-sys}` → Rust bindings. `rust/crates/telemetry/**` → Rust telemetry. `experimental/doris/**` and `experimental/starrocks/**` each get their own row.
7. Scan manager + pin cache: `src/scan_manager/**`, `src/pin_table.*`, `src/pin_snapshot_identity*`, `src/duckdb_table_identity*`.
8. `src/io/**` → I/O layer.
9. `src/cuda/scan/**` → DuckDB-native decode kernels.
10. Scan operators: `src/op/scan/**` (the rest) and `src/op/sirius_physical_{table_scan,column_data_scan,dummy_scan,gpu_values}*`.
11. Filter/projection/expressions: `src/expression*/**`, `src/op/sirius_physical_{filter,projection}*`, `src/cuda/{date_cast,sirius_like_multiliteral,sirius_round_floating_point}*`.
12. Joins: `src/op/sirius_physical_{hash_join,nested_loop_join,delim_join,dense_count_join}*`, `src/op/{cross_join_slicing,key_preparation}*`, `src/cuda/{dense_count_join_impl,sirius_rmm_cuco_allocator}*`, `src/op/aggregate/dense_count_join_impl*`, `src/planner/join_key_nullability*`.
13. Aggregates: `src/op/aggregate/**` and `src/op/sirius_physical_{grouped,ungrouped}_aggregate*` (including the merges).
14. Sort / top-N / limit: `src/op/{order,merge}/**`, `src/op/cudf_sort_order*`, `src/op/sirius_physical_{order,top_n,merge_sort,sort_sample,sort_partition,limit}*`.
15. `src/op/window/**` → Window.
16. `src/vss/**` and `src/cuda/vss/**` → Vector search.
17. Remaining `src/op/**` → Operator base + plumbing: partition, concat, union, CTE, delim scan, result collector, passthrough, operator base class.
18. Remaining `src/planner/**` → Physical plan generation.
19. `src/{pipeline,exec,parallel,creator}/**` → Executor.
20. `src/{memory,downgrade,data}/**` and `src/cuda/{device_copy_batch,device_transfer,device_health}*` → Memory.
21. DuckDB glue: `src/sirius_{extension,extension_entry,interface,registration,sql_rewrite,library_anchor}*`, `src/transparent/**`, `sirius-duckdb/**`.
22. Context and config: `src/{sirius_context,sirius_engine,sirius_config,config,context_config,yaml_reader,query_id,utils,debug_utils}*` and `src/{sirius,common,helper,util,cudf,event}/**`.
23. `src/{log,telemetry}/**` → Logging + C++ telemetry.
24. `utils/**` → Tools.

**Test files** follow the same component names:
- `test/cpp/<dir>/` maps to the matching component. `test/cpp/operator/*` is split by filename keyword (join, aggregate, order, filter, dynamic_filter, and so on). `test/cpp/scan/*dynamic_filter*` goes to dynamic filters.
- `test/cpp/integration/` has its own row; `test/cpp/utils` and `test/cpp/*.cpp` form the test harness.
- `simpatico_codegen/tests` → Simpatico. `experimental/*/tests` and the inline `#[cfg(test)]` blocks → their crate.
- No file is left unmapped.

### Per-component table (Sirius @ b9e05ffc)

| Component | yardstick lines | test lines | raw wc -l (src) | raw wc -l (test) |
|---|--:|--:|--:|--:|
| Scan operators (`src/op/scan`: Parquet / DuckDB-native / Iceberg ingestibles, scan contracts; TABLE_SCAN, VALUES) | 18,767 | 28,111 | 26,749 | 33,564 |
| Scan: DuckDB-native GPU decode kernels (`src/cuda/scan`: RLE, bitpack, ALP, FSST, dictionary) | 4,009 | 0 | 6,170 | 0 |
| Scan manager + pinned-table cache (`src/scan_manager`, `pin_table`) | 11,116 | 15,899 | 15,953 | 18,868 |
| I/O layer (`src/io`: io_uring, kvikio, REST/S3 + SigV4, pinned prefetching cache) | 10,786 | 13,856 | 16,535 | 17,369 |
| Pipeline / task executor / scheduler / admission (`src/pipeline`, `exec`, `parallel`, `creator`) | 10,301 | 14,517 | 16,195 | 19,082 |
| Dynamic (runtime join) filters: Bloom / IN-list / zone-map, SIP | 8,665 | 14,704 | 12,853 | 17,286 |
| Context, config, common helpers | 8,198 | 6,936 | 12,463 | 8,697 |
| Physical plan generation (DuckDB logical plan → Sirius pipelines, `src/planner`) | 7,985 | 9,749 | 11,057 | 12,378 |
| Filter/projection + expression evaluator (`src/expression*`, cuDF AST / JIT) | 6,021 | 8,493 | 9,585 | 10,997 |
| DuckDB extension glue + transparent interception (`sirius_extension.cpp`, `src/transparent`, `sirius-duckdb/`) | 5,539 | 2,202 | 7,233 | 2,536 |
| Join operators (hash with STANDARD / BUILD_PROBE / MIXED, NLJ, delim, mark, dense-count) | 4,912 | 4,289 | 6,896 | 5,643 |
| Operator base + plumbing (partition/exchange, concat, union, CTE, result) | 3,987 | 6,203 | 6,266 | 7,994 |
| Memory tiers / spill (downgrade executor) / data batches / cross-GPU transfer | 3,181 | 4,790 | 5,552 | 6,380 |
| Aggregate operators (+ merge) | 2,923 | 3,625 | 4,053 | 4,728 |
| Late materialization | 2,663 | 3,735 | 4,736 | 4,897 |
| Streaming fragments + C++ FFI (Substrait entry: `sirius_ffi.cpp`, `include/sirius/ffi.hpp`) | 2,133 | 1,317 | 3,721 | 1,999 |
| Compression integration (compressed pin/spill representations) | 1,954 | 2,320 | 3,209 | 3,157 |
| Sort / top-N / limit (sample → partition → merge sort) | 1,723 | 1,482 | 2,583 | 2,094 |
| Vector search (cuVS) | 1,145 | 1,687 | 2,070 | 2,305 |
| Logging + C++ telemetry (NVTX / Quent) | 994 | 35 | 1,755 | 41 |
| Window (whole-partition evaluator; not wired into the planner, see Q2) | 456 | 694 | 622 | 745 |
| Rust bindings (`rust/crates/sirius`, `sirius-sys`) | 177 | 194 | 340 | 243 |
| End-to-end SQL integration tests (`test/cpp/integration`) | 0 | 30,777 | 0 | 39,670 |
| Test harness (`test/cpp/utils`, main) + `test/cmake` probes | 0 | 5,092 | 0 | 7,168 |
| **Subtotal: DuckDB-hosted engine** | **117,635** | **171,707** (estimate: includes all of test/) | | |
| Simpatico GPU compression library (in-tree, `src/compression/simpatico_codegen`, incl. CLI 1,108) | 17,635 | 11,530 | 25,546 | 14,055 |
| `experimental/doris` (Rust Doris BE shim → Substrait) | 7,340 | 6,247 | 9,177 | 6,816 |
| `experimental/starrocks` (Rust StarRocks CN shim → Substrait) | 6,616 | 7,163 | 8,408 | 8,192 |
| Rust telemetry crates (Quent model / analyzer / bridge / store) | 4,886 | 951 | 5,646 | 997 |
| Tools (`utils/stream_check`) | 101 | 0 | 204 | 0 |
| **Sirius total** | **154,213** | **206,598** | **225,577** | **257,901** |
| *Separate row:* cuCascade @ `1a928109` (2026-10-21), NVIDIA tiered-memory library (submodule, not in the total) | 24,600 (21,908 excl. `benchmark/` 2,692) | 15,787 | 40,246 | 21,063 |
| *Not in total:* legacy Sirius engine `src/legacy/` at `legacy-dev` `f79a6f42` (2026-09-23), removed from main since | 19,323 (124 files) | n/a | 26,889 | n/a |

**cuCascade breakdown.** Same counting rule; clone at `scratchpad/ext/cucascade`, checked out at the submodule SHA `1a9281093a0f…`. Each area combines `include/cucascade/<x>` and `src/<x>`:
- io (uring, REST/S3, kvikio, cache): 9,363
- memory tiers / reservations / allocators: 5,587
- cudf table converters / spill formats: 2,628
- exec (thread pools, futures): 2,176
- data batches / repository: 1,716
- misc: 438
- benchmark: 2,692
- tests (`test/`): 15,787

cuCascade has its own io_uring/REST I/O stack (9.4k lines) in addition to Sirius's `src/io` (10.8k). Its README: "GPU memory management library for data-intensive applications requiring intelligent tiered memory allocation across GPU, host, and disk storage" ([cuCascade README](https://github.com/NVIDIA/cuCascade), accessed 2026-10-28; also `ext/cucascade/README.md`).

**Python** is not in the yardstick: 25,959 non-blank, non-`#` lines tracked. The main pieces:
- `test/fuzz` 8,517 (SQL fuzzer)
- `test/cpp` 5,781 (helpers)
- `test/tpch_performance` 5,760 (TPC-H power/throughput runners, benchmark drivers)
- `tools/log_analyzer` 1,759
- `test/scripts` 755
- `src/compression` 634
- `experimental/doris` 559
- agent skills 1,116 (`.claude/skills` 558, `.agents/skills` 558)

**Test counts:**
- 5,056 Catch2 `TEST_CASE*`/`SCENARIO` macros in `test/` plus `simpatico_codegen/tests`, of which 1,210 are in `test/cpp/integration`.
- 406 Rust `#[test]` functions.
- One sqllogictest file (`sirius-duckdb/test/sql/sirius.test`); sqllogictest is not the main harness.

### Inferences
- Comparing like for like, peacockdb's 22,830 lines cover what Sirius splits across planner, executor, operators and glue. The closest Sirius analogue to peacockdb's whole engine is the "DuckDB-hosted engine" subtotal minus the scan/IO/cache stack: 117,635 − 44,678 ≈ 73k (estimate). That is still about 3× peacockdb.
- Half of Sirius's size is machinery peacockdb lacks entirely: the I/O stack, the pinned cache, spill/downgrade, dynamic filters, late materialization, DuckDB-native decode, Iceberg and S3. Before counting cuCascade, those components total roughly 60k lines (estimate: scan manager + I/O + dynamic filters + late mat + compression + Simpatico + native decode + memory).
- Development velocity is high: ~200 commits in 26 days by 24 authors at HEAD. The legacy engine (19k lines) was replaced by "Super Sirius" within months. A component table for Sirius dated today will be stale within weeks.

### Gaps
- The line where `src/legacy/` was removed between `f79a6f42` (2026-09-23) and HEAD could not be located because of the shallow history. The legacy count is taken at the archive SHA the README names (`docs/README.md:32`).
- The test-line subtotal for the engine row counts all of `test/`, including I/O benchmarks. This is marked as an estimate in the table.

## Q2. SQL coverage: TPC-H, TPC-DS, ClickBench, operators, types, functions

### Takeaway
- **TPC-H:** 22/22 runs on the GPU and is validated in CI at SF1.
- **TPC-DS:** about 70–72 of 99 queries on `main` per the maintainers' 2026-10-09 classification. The rest fall back to CPU: 14 need window functions, 9 need DISTINCT/INTERSECT/EXCEPT, plus a few others.
- **ClickBench:** all 43 queries have published results (legacy engine). The README flags Q28's regex rewrite as needing CPU fallback on the current engine.
- **Window functions are not supported:** the planner throws "Window not supported".
- **Not supported either:** aggregates with a FILTER clause, ASOF/ANY/positional/IE joins, UNNEST, SAMPLE, recursive CTE, variable-length LIST/STRUCT and DML.
- **Unsupported plans fall back silently to DuckDB CPU.**

### Cited Findings
- **Operators the README claims:** FILTER, PROJECTION, JOIN (Hash / Nested Loop / Delim), GROUP-BY, ORDER-BY, AGGREGATION, TOP-N, LIMIT, CTE. Window and ASOF JOIN are "working on" (`docs/README.md:268`). Types claimed: INTEGER, BIGINT, FLOAT, DOUBLE, VARCHAR, DATE, TIMESTAMP, DECIMAL; nested types are future work (`docs/README.md:267`).
- **Planner refusals** in `src/planner/sirius_physical_plan_generator.cpp` (each a `NotImplementedException`, which makes DuckDB run the CPU plan):
  - Window (line 2422), Unnest (2426), Sample (2433), Copy to file (2443), Any join (2450), Asof join (2454), Positional join (2464).
  - "Aggregates with a FILTER clause not supported in GPU" (`src/planner/sirius_plan_aggregate.cpp:817`).
  - Ungrouped distinct aggregates other than `COUNT(DISTINCT col)` are refused (`src/op/sirius_physical_ungrouped_aggregate.cpp:168`).
  - Also present elsewhere in `src/`: piecewise-merge, blockwise-NLJ and inequality joins, percentage LIMIT, recursive CTE, and all DML/DDL (insert, update, delete, create); 119 `NotImplementedException` sites in total (`git grep` count).
- **Window evaluator:** `src/op/window/whole_partition.cpp` computes SUM/AVG/COUNT/MIN/MAX over a whole partition with groupby plus a join-back (`docs/super-sirius/operators.md:427-430`). It is called only from its unit test (`test/cpp/operator/window/test_whole_partition.cpp`), not from the planner. Window support is tracked in [issue #1745](https://github.com/sirius-db/sirius/issues/1745) (open, accessed 2026-10-28).
- **Scalar functions:** the registry `src/expression/function_id.cpp` maps about 30 distinct SQL names: arithmetic, `substring/substr`, `~~`/`!~~` (LIKE), `contains`, `prefix`, `suffix`, `strlen/length`, `regexp_replace`, `concat/||`, `year…microsecond`, `date_trunc`, `row`, `struct_pack`, `upper`, `lower`, `round`, `abs`, `negate`, `constant_or_null`, `error`.
- **Aggregate functions:** sum, count, count_star, min, max, avg, first, stddev_samp (`src/expression/aggregate_id.cpp`).
- **Type mapping:** `src/helper/type_conversions.cpp:31-62` maps BOOLEAN, all signed and unsigned ints including HUGEINT/UHUGEINT, FLOAT, DOUBLE, DATE, TIMESTAMP_{SEC,MS,US,NS}, VARCHAR, DECIMAL, STRUCT, LIST/MAP, ARRAY.
  - The DuckDB-native scan refuses HUGEINT/DECIMAL128 and nested STRUCT/LIST. Fixed-size ARRAY of fixed-width types is the exception: it maps to a cuDF LIST (`docs/super-sirius/scan.md:465`, `:484`).
  - Varchar columns that may hold a single string of ≥4 KB (DuckDB overflow blocks) fall back to CPU (`docs/super-sirius/scan.md:484`).
- **TPC-H:** CI runs `test/tpch_performance/benchmark_and_validate.sh 1`, the full 22-query SF1 suite on the GPU, validated against answers (`.github/workflows/test.yml:298-304`; expected answers for q1–q22 in `test/answers/tpch/`). `test/cpp/integration/test_gpu_execution_tpch.cpp` holds 437 test-case macros that compare GPU against CPU over DuckDB-native and Parquet fixtures (e.g. lines 454–467).
- **TPC-Havoc** (22 TPC-H queries × 10 rewrites = 220 variants, 184 distinct; `test/README.md:36-48`): 36 variant labels are listed as expected not to run on the GPU (`test/tpch_havoc/expectations.yaml`):
  - 17 FILTER-clause aggregates (issue 2285)
  - 16 window functions (issue 1745)
  - 1 ANY join (issue 2289)
  - 1 `list()` aggregate (issue 2290)
  - 1 native-scan binder skip (issue 2294)
  Every other variant must match DuckDB's CPU result.
- **TPC-DS**, maintainer classification on `main` at `f48d1485` ([issue #1744 comment, 2026-10-09](https://github.com/sirius-db/sirius/issues/1744), accessed 2026-10-28): "70 of 99 queries run on the GPU", 72 after #2084 merged, 75 with the open PRs #2060 and #1975. "Every query that completes on the GPU matches the DuckDB CPU result." The failures by cause:
  - Window: q12, 20, 36, 44, 47, 51, 53, 57, 63, 67, 70, 86, 89, 98.
  - DISTINCT / INTERSECT / EXCEPT: q6, 8, 14, 38, 41, 49, 54, 75, 87.
  - Large cross product: q77.
  - `stddev_samp`: q17, 39. This aggregate now appears in `aggregate_id.cpp`, so it has likely landed since the comment (estimate).
  - `DATE + INTEGER`: q72.
- **Older, possibly stale TPC-DS list:** `test/tpcds_performance/run_tpcds_super_gpu.sh:3-16` lists only 15 TPC-DS queries "confirmed to run cleanly on GPU" (3 7 22 26 32 37 42 52 55 62 82 85 92 93 97). Its date cannot be determined in the shallow clone.
- **Nightly TPC-DS:** a functional nightly TPC-DS SF1 run with fallback disabled is proposed but not yet in CI ([issue #2065](https://github.com/sirius-db/sirius/issues/2065), accessed 2026-10-28).
- **ClickBench:** the published Sirius results (2026-03-09, lambda-GH200 and p5.4xlarge) have timings for all 43 queries, with no nulls ([ClickBench sirius/results/20260309/lambda-GH200.json](https://raw.githubusercontent.com/ClickHouse/ClickBench/main/sirius/results/20260309/lambda-GH200.json), accessed 2026-10-28). On the current engine, the DuckDB v2 rewrite of ClickBench Q28's regex "currently require[s] CPU fallback" ([issue #2146](https://github.com/sirius-db/sirius/issues/2146); `docs/README.md:270`).
- **Paper (legacy engine):** supports "most of the basic SQL operators and data types". LIST/nested types, ASOF joins and UDFs are future work. Window functions are not mentioned. Distributed (Doris) mode ran only TPC-H Q1, Q3 and Q6 and lacked e.g. `avg` ([arXiv 2508.04701v3, §3.4, §4.3](https://arxiv.org/html/2508.04701v3), accessed 2026-10-28).
- **Doris shim:** TPC-H 22/22 validate on the GPU through `experimental/doris` (Doris FE → Substrait → Sirius). The translator refuses window functions, set operations and functions outside an allowlist ([issue #2025](https://github.com/sirius-db/sirius/issues/2025), accessed 2026-10-28; `experimental/doris/docs/semantics-gaps.md`).

### Inferences
- On TPC-DS, peacockdb (82/99 on device per its inventory) and Sirius (about 72/99) are in the same range, and both are blocked mainly by window functions. Sirius additionally misses DISTINCT and set operations, while peacockdb plans them through DataFusion.
- Sirius's coverage is easier to grow incrementally because every refusal falls back to DuckDB CPU: a user always gets an answer. peacockdb has no CPU fallback in the GPU path.

### Gaps
- No machine-readable TPC-DS pass list was found at HEAD; the 70–72/99 figure comes from an issue comment dated 2026-10-09.
- ClickBench coverage on the current (non-legacy) engine is not published; the README points ClickBench to the legacy archive (`docs/README.md:32`).

## Q3. Multi-GPU and distributed

### Takeaway
- **Single-node multi-GPU (SNMG) is shipped on main.** It uses per-GPU memory pools, ready-device task placement and peer-copy or host-staged transfers through cuCascade, with no NCCL.
- **Multi-node distributed execution is not on main.** PR #2337 (open, +9.9k lines) adds exchange operators over NIXL/UCX with one rank per GPU, and reports TPC-H at SF30000 on 4×GB200 on one host. No multi-host run has been done yet.
- **The Doris and StarRocks shims run on a single backend:** the Doris shim stitches all fragments into one plan.
- **The paper's Doris numbers come from the legacy engine,** which used NCCL exchange on 4×A100 nodes, for TPC-H Q1, Q3 and Q6 only.

### Cited Findings
- "The engine assumes a single process pinning the configured subset of visible GPUs … There is no notion of distributed multi-node execution in this codebase" (`docs/super-sirius/multi-gpu-architecture.md:21-23`).
- **Memory tiers:** GPU, NUMA-local pinned HOST and DISK, per GPU (`docs/super-sirius/multi-gpu-architecture.md` "Tier Hierarchy").
- **Cross-GPU transfer:** `cudaMemcpyPeerAsync` or explicit host staging via `cucascade::convert_gpu_to_gpu` (`docs/super-sirius/multi-gpu-architecture.md:189,193`).
- There are no NCCL/UCX/NIXL references in `src/`, `cmake/` or `CMakeLists.txt` at HEAD (`git grep`).
- **Multi-GPU in CI:** the GPU job runs on a self-hosted `gpu-2xt4` runner (2× T4) with `run_unit_tests.py --require-gpus 2`. There are `[multi_gpu]` tests and `*_mgpu` operator tests, such as `test/cpp/operator/test_physical_hash_join_mgpu.cpp` (`.github/workflows/test.yml:172,245`; `AGENTS.md:47`).
- Single-node multi-GPU support was tracked by issue #724, closed 2026-06-10 ([issue #724](https://github.com/sirius-db/sirius/issues/724), accessed 2026-10-28).
- **PR #2337** "feat(exchange): distributed query execution across GPUs", OPEN, created 2026-10-26, +9,878/−166 ([PR #2337](https://github.com/sirius-db/sirius/pull/2337), accessed 2026-10-28):
  - **Design:** one process per GPU (a rank), each scanning a byte range of the Parquet files. `distribute_plan()` inserts hash, range, broadcast or gather exchanges; data moves GPU to GPU over NIXL; rank 0 returns the result; there is "no central scheduler/orchestrator yet".
  - **Reported warm results:**
    - SF1000 on 2 GPUs: 1.91× vs main on 1 GPU.
    - SF3000 on 4× GB200: 2.49× the 1-GPU speed distributed, vs 1.56× with SNMG.
    - SF10000 on 4× GB200: 507 s unpinned, 275 s with rank-pinned tables.
    - SF30000: all 22 queries in 1,697 s.
- **Follow-ups opened 2026-10-28:** a multi-host run, a TCP-only path, a gateway/coordinator, and peer-loss survival. "Every run in #2337 … used one host" ([issue #2372](https://github.com/sirius-db/sirius/issues/2372), accessed 2026-10-28; also #2373, #2378).
- **Doris shim:** "One node only. Every fragment of a query must reach this one backend, which stitches them back into a single plan. Nothing runs across backends or GPUs." Also one query at a time and `local()` Parquet only ([issue #2025](https://github.com/sirius-db/sirius/issues/2025), accessed 2026-10-28).
- **Paper (legacy):** 4 nodes, each with one A100 and InfiniBand NDR. Exchange uses NCCL broadcast, shuffle, merge and multicast. At SF100: Q1 12.5×, Q3 2.5×, Q6 2.4× vs Doris; exchange dominates Q3 ([arXiv 2508.04701v3 §3.2.4, §4.3, Table 2](https://arxiv.org/html/2508.04701v3), accessed 2026-10-28).

### Inferences
- Sirius is several months ahead of peacockdb on multi-GPU. peacockdb has multi-GPU only in test binaries; Sirius has an engine path on main and a near-merge distributed path with SF30000 results.
- Sirius has not yet run distributed across hosts.

### Gaps
- No public multi-host distributed result exists for the current engine.
- The SNMG scaling numbers on main come only from PR #2337's comparison; no standalone SNMG benchmark was found in docs.

## Q4. Data ingestion and scan

### Takeaway
The current engine ("Super Sirius") decodes on the GPU:
- **Parquet:** read through cuDF's Parquet reader (`cudf::io::read_parquet`, plus `hybrid_scan_reader` for column-chunk ranges), fed by Sirius's own I/O stack: io_uring with O_DIRECT, an S3 REST client, or kvikio with GDS capability.
- **DuckDB-native `.duckdb` tables:** decoded on the GPU from DuckDB's storage segments by custom CUDA kernels.
- **Iceberg:** handled through the Parquet path plus delete files and deletion vectors.

This is a change from the paper and the ClickBench-era legacy engine, where DuckDB's CPU scan read the data and Sirius copied it to the GPU, with hot runs served from a GPU cache. Data can also be pinned explicitly (`pin_table`) in GPU or host memory, optionally Simpatico-compressed.

### Cited Findings
- **Formats:** Parquet (local or object store) via `parquet_gpu_ingestible` using "`cudf::io::read_parquet` over row-group slices", and DuckDB-native tables via "GPU decode of per-row-group storage segments" (`docs/super-sirius/scan.md:9-12`). `hybrid_scan_reader` is used for column-chunk ranges (`src/op/scan/parquet_materialize.cpp:23,36`).
- **Iceberg:** `iceberg_gpu_ingestible` composes `parquet_scan_core`. It handles positional deletes, equality deletes and V3 deletion vectors (Puffin), with manifest parsing delegated to DuckDB's iceberg/avro extensions (`docs/super-sirius/scan.md:145-156`).
- **DuckDB-native decode codecs:** UNCOMPRESSED, CONSTANT, RLE, BITPACKING, ALP/ALPRD; for strings DICTIONARY, FSST, DICT_FSST. ROARING validity is decoded on the host (`docs/super-sirius/scan.md:459-480`). The kernels are in `src/cuda/scan/` (4,009 lines).
- **I/O backends** (`docs/super-sirius/scan.md:547-595`):
  - io_uring with 256 KiB–16 MiB ops and O_DIRECT for local files;
  - REST / libcurl-multi for `s3://` (AWS S3, MinIO), with no registered `gs://` or `azure://` backend;
  - a kvikio fallback, "GDS-capable for local files";
  - a pinned-memory prefetching cache.
  - "S3 scans require GPU execution and have no DuckDB CPU fallback" (`docs/super-sirius/scan.md:599`).
- **Runtime requirements:** `io_uring` and an O_DIRECT-capable filesystem for local Parquet (`docs/README.md` "Requirements").
- **Pinning:** "Sirius reads table data from storage on every query. For the best hot-run performance, pin frequently queried tables": `CALL pin_table(..., tier = 'gpu'|'host')`.
  - Pinned DuckDB tables reconcile deletes and committed inserts per query; UPDATE and MERGE are rejected while pinned (`docs/README.md:150-180`).
  - Compressed pinning uses Simpatico plans, with bundled TPC-H SF1000 plans (`docs/README.md` "Compression with Simpatico").
- **CSV and other table functions:** there is no CSV `gpu_ingestible`, so CSV and other DuckDB table functions presumably fall back to CPU (estimate, based on the scan table listing only Parquet, DuckDB-native and Iceberg).
- **Data loading:** there is no ingest/write path. DML/DDL is refused (`NotImplementedException` for Insert, Update, Delete and Create in `src/`). Data is loaded by DuckDB itself.
- **Legacy and paper:** "Currently, Sirius relies on the host database to read data from disk". A buffer manager caches the data on the GPU (50% of GPU memory for cache, 50% for processing), and GPU-native readers and GPUDirect are future work ([arXiv 2508.04701v3 §3.2.3, §4.1](https://arxiv.org/html/2508.04701v3), accessed 2026-10-28). The NVIDIA blog likewise says DuckDB's scan reads tables on the CPU and then the data moves to GPU memory ([NVIDIA blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/), accessed 2026-10-28).
- **ClickBench harness:** `load` builds `hits.db` with the DuckDB CLI, then calls the server's `/load`, which does "gpu_buffer_init", i.e. caches the data on the GPU ([ClickBench sirius/load](https://github.com/ClickHouse/ClickBench/tree/main/sirius), accessed 2026-10-28).

### Inferences
- On ingestion, Sirius is now a superset of peacockdb: both use cuDF's Parquet reader, and Sirius adds a tuned I/O layer, S3, Iceberg, DuckDB-native decode and a pin cache.
- peacockdb's #232 (`read_parquet` being slow vs the chunked reader) is the kind of problem Sirius addressed with its own datasource and prefetching cache.

### Gaps
- No published cold-run (storage → GPU) throughput figures for the new I/O stack were found, apart from PR #2337's GPFS cold/warm numbers (SF10000: 807 s cold vs 507 s warm with the small-read cap).

## Q5. Test coverage and CI

### Takeaway
- **Large test suite:** 206,598 test lines (1.34× the code), 5,056 Catch2 test cases, 1,210 of them end-to-end SQL integration cases comparing GPU against DuckDB CPU, and 406 Rust tests.
- **Real GPU CI on every PR:** self-hosted 2×T4 runners execute the unit tests, Iceberg conformance, S3 integration and a TPC-H SF1 validate-and-time run, for both CUDA 12 and 13.
- **Fuzzer:** there is also a SQL fuzzer (8.5k lines of Python).
- **sqllogictest is barely used:** one `.test` file.

### Cited Findings
- **CI** (`.github/workflows/test.yml`):
  - Build on self-hosted `cpu-xl` runners (line 51).
  - Matrix: CUDA 13 on ubuntu-24.04 and CUDA 12 on ubuntu-22.04 (lines 37–45 region).
  - GPU job on self-hosted `gpu-2xt4` (`pr-gpu-2xt4` for pull requests) (line 172).
  - Steps:
    - `nvidia-smi`;
    - `python3 scripts/run_unit_tests.py --require-gpus 2` (lines 239–245);
    - Iceberg conformance (247);
    - S3 integration and S3 large (291–294);
    - "TPC-H performance snapshot" `benchmark_and_validate.sh 1` (298–304).
  - Other workflows: check, distribution, docs, experimental (Doris/StarRocks without a GPU), release, validate (1,586 workflow lines in total).
- **Test inventory:**
  - `test/cpp/` has 30 subdirectories mirroring `src/`.
  - The integration tests include TPC-H (`test_gpu_execution_tpch.cpp`), TPC-Havoc, TPC-DS null handling, S3 TPC-H, a multi-GPU TPC-H audit, `pin_table` MVCC insert/delete/update, and transparent-fallback verdicts (see `ls test/cpp/integration`).
  - Integration YAMLs exist for 2-GPU, GB10, S3-cache and shard configurations (`test/cpp/integration/integration-*.yaml`).
- **Unit runner:** runs shards, 2 per GPU, then the `[multi_gpu]` and late-materialization tests (`AGENTS.md:47`).
- **Fuzzing:** `test/fuzz/` (siriusfuzz) with `known_issues.toml`; currently one known-issue entry, ungrouped COUNT(DISTINCT) (#1218).
- **Translator corpora:** `experimental/doris/tests/` and `starrocks-plan-translator/tests/` hold captured FE dispatches for all 22 TPC-H queries plus snapshot diffs (`experimental/doris/README.md` "Layout").
- **Sanitizers and reproduction kits:** ASan/TSan builds (`tsan.supp`, `docs/super-sirius/debugging.md`); compute-sanitizer gates in `bench/sf1000-repro/verify-memcheck-sf1.sh`.

### Inferences
- Sirius's CI exercises real GPUs and multi-GPU on every PR. peacockdb's GPU verification runs on a separate host (shad-gpu) rather than in CI (estimate based on the peacockdb inventory's description).
- The T4 runners (sm_75, 16 GB) mean CI checks correctness, not performance at scale.

### Gaps
- No coverage-percentage report was found.
- Run frequency or flake rate of the GPU CI could not be checked without Actions history (not queried).

## Q6. Performance evidence

### Takeaway
All published Sirius numbers are **hot runs**: data is cached or pinned on the GPU, and load time is excluded.
- **Current engine:**
  - README, on AWS: TPC-H SF100–SF1000 at 2.7–5.7× faster than DuckDB 1.5.5 on Graviton5, and 2.1–5.3× cheaper per run.
  - SF1000 reproduction kit: 8.18 s on GB300 with a patched cuDF.
  - PR #2337: SF30000 in 1,697 s on 4×GB200.
- **Legacy engine (paper and NVIDIA blog):** about 8.3× TPC-H SF100 cost-efficiency vs DuckDB, the top ClickBench hot result on GH200 (7.2–7.4× cost-efficiency), and up to 12.5× vs Doris on 3 queries.
- **Cold numbers are much worse:** the ClickBench GH200 cold sum is 208.5 s against a hot sum of 1.45 s.

### Cited Findings
- **README chart, current engine** ("TPC-H hot runs on AWS, 22 queries · best Sirius g7e size vs DuckDB on m9g.16xlarge"; `docs/README.md:27`, image `docs/super-sirius-perf.png`). Sirius ran on g7e 2xl/4xl/8xl with an RTX PRO 6000 96 GB; DuckDB v1.5.5 ran on Graviton5 m9g.16xlarge.

  | Scale factor | Sirius | DuckDB | Faster | Cheaper |
  |---|---|---|--:|--:|
  | SF100 | 1.71 s, $0.0016/run | 9.37 s, $0.0081/run | 5.5× | 5.1× |
  | SF300 | 4.48 s | 25.72 s | 5.7× | 5.3× |
  | SF500 | 8.14 s | 42.25 s | 5.2× | 4.8× |
  | SF1000 | 31.79 s, $0.0353 | 85.91 s, $0.0747 | 2.7× | 2.1× |

  Whether the hot runs use `pin_table` is not stated (estimate: they do, given the README's own advice).
- **SF1000 reproduction kit** (`bench/sf1000-repro/README.md`): "8.180 s TPC-H SF1000 suite (22 queries, best-of-3, 22/22 byte-identical)", down from a 15.99 s starting point.
  - Hardware: GB300 (256 GB HBM, 72-core Grace), CUDA 13.2, measured 2026-08-01.
  - Caveats: it needs a patched libcudf fork (`felipeblazing/cudf`) and a non-merged branch; "This branch is for reproduction only".
  - Official power/throughput run at SF1000 with 7 streams and refresh functions: "Power@Size 7,165,616 · Throughput@Size 4,951,261 · QphH@Size 5,956,411". This is not an audited TPC result.
- **Compressed pinning** (`docs/super-sirius/compressed-pinning.md:46-57`), GB300 SF1000 hot suite: 17.05 s with dimension tables raw on the GPU and lineitem compressed on the GPU, vs 20.48 s mixed-tier uncompressed and 22.20 s all host-pinned compressed.
- **PR #2337, warm** ([PR #2337](https://github.com/sirius-db/sirius/pull/2337), accessed 2026-10-28):
  - SF3000 on 4× GB200: distributed 2.49× vs SNMG 1.56× over 1 GPU.
  - SF30000: 1,697 s for 22 queries.
  - Single GPU at SF1000: Q3 drops from 10.4 s to 3.6 s warm (26 s to 3.8 s cold) after a Bloom-cap fix.
  - SF10000 on GPFS: cold 807 s vs warm 507 s (with the small-read cap).
- **Paper, legacy engine** ([arXiv 2508.04701v3](https://arxiv.org/html/2508.04701v3), accessed 2026-10-28; v1 2025-08-06, v3 2025-12-20; listed at CIDR 2026: [vldb.org/cidrdb/2026](https://vldb.org/cidrdb/2026/rethinking-analytical-processing-in-the-gpu-era.html), accessed 2026-10-28):
  - Abstract: "8.3× and 7.4× better cost efficiency" on TPC-H and ClickBench with single-node DuckDB, and "up to 12.5× speedup" with Doris.
  - TPC-H SF100: GH200 ($1.5/h) vs DuckDB and ClickHouse on c8i.8xlarge (stated at $1.5/h). 8.3× vs DuckDB and 38× vs ClickHouse (ClickHouse Q9 did not finish, Q21 was unsupported, correlated subqueries were rewritten).
  - Caveat: "hot runs when the data is already cached in the GPU memory"; 50% of GPU memory is reserved for caching.
  - The paper is internally inconsistent on GH200 price ($3.2/h in §2.1 vs $1.5/h elsewhere) and GPU memory (96 vs 92 GB).
  - Per-query numbers appear only in figures.
- **NVIDIA blog**, 2025-12-15 (Yu, Yogatama, Yang, Aramburu) ([NVIDIA blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/), accessed 2026-10-28):
  - "#1 on ClickBench" with "at least 7.2x higher cost-efficiency". Setup: GH200 at $1.5/h vs c6a.metal ($7.3/h), c8g.metal-48xl ($7.6/h) and c7a.metal-48xl ($9.8/h). Hot runs only.
  - Weak queries: Q23 (string contains), Q24 and Q26 (top-N), Q27 (large aggregation).
  - Q28 ran 13× faster with cuDF's JIT transform.
  - The paper says "11% faster than Umbra" in relative runtime (§4.4).
- **ClickBench published results**, 2026-03-09 ([sirius/results/20260309](https://github.com/ClickHouse/ClickBench/tree/main/sirius/results/20260309), accessed 2026-10-28):
  - Both runs are tagged `"tuned": "no"` and "in-memory", with 43/43 queries.
  - lambda-GH200: load_time 25 s; sum of first (cold) runs 208.5 s; sum of best hot runs 1.45 s.
  - p5.4xlarge (H100): load_time 237 s; cold sum 977.9 s; hot sum 1.50 s.
  - The sums are my arithmetic over the JSON.
  - The ClickBench `install` script clones `sirius-db/sirius` main at install time, while the Sirius README says ClickBench results correspond to the legacy archive `f79a6f42` (`docs/README.md:31-32`).
- **Claim in the repo:** "As of August 2026, Sirius leads ClickBench hot-run results" (`docs/sirius-for-research.md`, "Why use Sirius", item 3). The live leaderboard was not checked; it is JS-rendered.

### Inferences
- Like peacockdb's own evidence, Sirius's headline numbers exclude the load. The gap between the ClickBench cold and hot sums (about 140×) shows the end-to-end story is dominated by getting data onto the GPU. Sirius answers this with `pin_table` and a serving model (a long-running DuckDB process holding pinned data), which peacockdb lacks.
- The current engine's SF1000 single-GPU results range from 8.18 s (GB300 with a patched cuDF) to 31.79 s (RTX PRO 6000, stock). These are hardware- and patch-dependent, and should be cited with the hardware.

### Gaps
- No independent (third-party) reproduction of Sirius numbers was found.
- No cold-run TPC-H numbers for the current engine on a single GPU were found, except PR #2337's Q3 and SF10000 cold figures.
- The ClickBench leaderboard rank as of today was not verified.
