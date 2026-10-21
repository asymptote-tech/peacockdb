# decimal-literal — run record

## Run log

- 2026-10-21: branched `ENS-decimal-literal` off `ENS-value-case`. Developer dispatched. verda:
  no address, CPU tests local. GPU: nebius-gpu, tree `~/peacockdb-K`. Lesson from value-case:
  regenerate `testdata/report.html` only after `gpu-result.txt` is copied home.

## Developer, round 1 (2026-10-21)

**What changed.** `cudf_ast_can_evaluate` (`cpp/src/expr.cpp`): a new `is_decimal_literal`
beside `is_string_like_literal`; the `LiteralExpr` arm refuses one; the `CastExprNode` arm
accepts a decimal literal directly under an `INT64`/`FLOAT64` target without recursing into the
literal arm. Nothing else in the C++ moved. Red patch: `/tmp/decimal-literal-red.patch`
(`git diff -R -- cpp/src/expr.cpp`).

**Subtle.**
- Without the cast arm, `CAST(dec AS DOUBLE)` would recurse into the literal arm and leave the
  AST. That breaks no answer by itself, but it moves `ADecimalLiteralStillCarriesItsScaledValue`'s
  route, and the spec keeps every current answer on the AST.
- The new gtest asks `cudf_ast_can_evaluate` with an empty `cudf::table_view{}`: a cast over a
  literal reads no column, so no table is needed.
- `git clang-format --diff HEAD` wants to reflow the walk test's two-per-line `cases` table, only
  because the edited row sits in the same statement. Not applied: those lines are not this
  task's. The rest of the C++ diff is clang-format clean.
- `rustfmt --check --edition 2024 exec_cases.rs` reports one pre-existing diff, `project_over`
  at ~172, outside this change.
- The widget links q5's ✔ to the tp4-sized section, `gpu-result.txt#L1472`, the same as every
  other row.
- architecture.md's `CudfProject` row (~1047) lists LIKE/CASE/scalar functions as what takes
  `build_column`. That list was already incomplete (a bare string literal, a decimal operand)
  and now also misses a bare decimal literal. It is the coordinator's to decide.

**Local CPU (verda unavailable).**
- `cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2`: 844 passed,
  2 ignored.
- Before the rsync, `--test test_cpu_corpus -- tpcds_q5_ device_order_tpcds_q5
  every_enabled_device_cell`: 13 passed. 6 failed on "holds no section for this cell": the five
  `duckdb_gpu_tpcds_q5_*` and the coverage guard, as expected. `--list` shows 2233.
- `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh test -p peacockdb-core
  --lib --features gpu --no-run`: rc 0, no warnings.
- After the rsync, all with `--test-threads=2`: `test_cpu_corpus` 2233/2233,
  `test_module_layout` 18, `test_ci_coverage` 11, `test_corpus_goldens` 28, `test_cost_model` 3,
  and `cargo test -p cost-report` 41. No cpu golden moved, and no plan golden moved.

**GPU (nebius-gpu, `~/peacockdb-K`).** Before building: 24 GB free of 96, card idle. cuDF lib:
`~/data/miniforge3/envs/rapids-cuda-12.2/lib`.
- Red build (patch applied, rc 0):
  - `peacock_plan_tests --gtest_filter='Literals.EveryWireType*:Literals.ACastDecimal*'` ran 2.
    The walk failed at its `Decimal128` row, type_id 10 (FLOAT64) against 27 (DECIMAL128).
    `ACastDecimalLiteralStaysOnTheAst` passed, as expected, since it pins today's behaviour.
  - `peacockdb_core_gpu_lib gpu_tests::exec_cases::a_bare_decimal_literal_agrees` ran 1 and
    failed: `export: column one_and_a_half is not a decimal but was given precision 3`.
  - `test_gpu_corpus tpcds_q5_tp1_single` failed: `the output hook refused a batch: 4 return_amt:
    Decimal128(7, 2) vs FLOAT64; 5 net_loss: …`. That is #210 as written. The binary holds 914.
- Green build (patch reversed, rc 0, 0 warnings): `Literals.*` 10/10, `gpu_tests::exec_cases::`
  59/59, `PCK_WRITE_GPU_RESULT=1 test_gpu_corpus tpcds_q5_` 5/5.
- The rsync home moved only `tpcds.sf1/gpu-result.txt`: +525 lines, five `== q5 mode=…`
  sections, nothing removed, no other section moved.
- Then `python3 scripts/ticket_widget.py`. Its diff, classified by a script against HEAD: q5's
  row (five ✔, #210 gone), #210's by-ticket row removed, the four summary counts (TPC-DS 79→80/99,
  12→11 tickets), and 76 TPC-DS rows, q6 to q99, whose gpu-result anchors shift by exactly +525.
  Nothing else moved.
- `gpu-host-test.sh run` (the L40S list) on the green build: `ALL rc=0`. peacock_cpu_tests 26,
  peacock_gpu_tests 2, peacock_join_session_tests 54, peacock_plan_tests 118,
  peacock_gpu_benchmarks 8 (3 `bench_` skipped), peacockdb_core_gpu_lib 739, test_gpu_corpus 914,
  test_node_timing 1. Soft stop, rc 0. Deferred, per the dispatch: sf40, benchmarks.
- Counts in `build-test.md`: `test_cpu_corpus` +6 (2233), `test_gpu_corpus` +5 (914),
  `peacock_plan_tests` +1 (118); grand total 5515. Each is matched by a run above.
- 2026-10-21: developer green; committed; PR opened; state reviewing. Coordinator updated architecture.md's CudfProject row (decimal literal takes build_column).
- 2026-10-21: review round 1: 0 blocking, 0 important, 4 nits. Coordinator fixed three (architecture.md CudfProject row framed as examples; project.cpp comment; corpus_cases.inc reflow, both comment-only). Dropped: a host-only AstRouting.CudfAstCanEvaluate case for decimal literals (cpu-tier coverage of the predicate; the GPU tier pins it). State completing; completeness pass dispatched.
