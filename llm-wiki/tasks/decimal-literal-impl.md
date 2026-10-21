# decimal-literal implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A bare decimal literal on the device is a `Decimal128` column of the declared scale
rather than a `FLOAT64` one. tpcds q5's five device cells turn on (#210).

**Architecture:** Two arms of `cudf_ast_can_evaluate` (`cpp/src/expr.cpp`, ~399) change.
- **The `LiteralExpr` arm refuses a `Decimal128` literal.** `make_column` handles a bare literal
  first, through `build_scalar`, so it gets a real `fixed_point_scalar`. Only `project.cpp`
  sends a bare literal to the AST today.
- **The `CastExprNode` arm keeps a decimal literal under an AST-able cast on the AST path.** The
  cast must be directly over the literal, to `Float64` or `Int64`, and the literal goes through
  `ast_scalar`'s scaled double, as today. So no current answer moves.

The corpus case is q5. DataFusion folds `cast(0 AS decimal(7,2))` there, from a float literal
beside decimal columns in a UNION, into one `Decimal128(7,2)` literal.

**Tech stack:** C++20/cuDF 25.02 (gtest in `peacock_plan_tests`), the Rust device harness, the
corpus, nebius-gpu.

**Spec:** [`decimal-literal.md`](decimal-literal.md). Task 10 of chain K, after value-case.

## Global Constraints

- Which path a decimal literal takes, and nothing else. No cpu golden moves.
- A unary minus over a decimal literal is out: the column path has no `Negative` arm, and
  DataFusion folds `-1.5` into one literal.
- A `gpu-result.txt` section other than q5's that moves is a finding to explain, not to accept.
- GPU: nebius-gpu, `~/peacockdb-K`, through `value-case-impl.md`'s *Device cycle*: the plan
  gtests are in `peacock_plan_tests`, the corpus edits travel in the same sync, and there is no
  `--pull-results`. Here `FIX=cpp/src/expr.cpp` and the patch is
  `/tmp/decimal-literal-red.patch`.

## Review Focus

1. **A decimal literal under a cast the AST takes** (`CAST(1.5 AS DOUBLE)`,
   `CAST(1.5 AS BIGINT)`). It must stay on the AST path and answer what it answers today:
   1.5, and 1, since both the AST's `CAST_TO_INT64` and arrow truncate. Pinned by
   `Literals.ACastDecimalLiteralStaysOnTheAst`.
2. **A decimal literal inside a binary expression.** The binary arm already refuses a decimal
   operand, so nothing changes. Covered by the existing
   `Literals.ADecimalLiteralStillCarriesItsScaledValue` staying green.
3. **A bare `NULL::Decimal128`.** Through `build_scalar` it is an invalid `fixed_point_scalar`,
   so it answers a `DECIMAL128` null column. Pinned by the `Literals` walk's `Decimal128` row.
4. **Precision at export.** The declared `Decimal128(3, 1)` must pass the export's precision
   check that refused it before. Pinned by `a_bare_decimal_literal_agrees`.
5. **A filter over a decimal literal** (`WHERE 1.5 > 1`). `filter.cpp` asks the same predicate,
   and the binary arm decides there, so nothing changes. Covered by the cpu-vs-device filter
   cases staying green.

---

### Task 1: A bare decimal literal takes the column path, and q5 turns on

**Files:**
- Modify: `cpp/src/expr.cpp`: `cudf_ast_can_evaluate`'s `LiteralExpr` and `CastExprNode` arms,
  and a new `is_decimal_literal`.
- Test: `cpp/tests/gpu/test_plan_executor.cpp`:
  - the row in `Literals.EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot` and its header
    comment;
  - a new `Literals.ACastDecimalLiteralStaysOnTheAst`.
- Modify: `peacockdb-core/src/tests/gpu_tests/exec_cases.rs` (the `#210` block, ~424)
- Modify: `peacockdb-core/tests/common/corpus_cases.inc` (q5's line, and the comments that cite
  #210), and `testdata/cost-registry.csv` (q5's row)
- Regenerate: `testdata/goldens/tpcds.sf1/gpu-result.txt`, `testdata/report.html`
- Modify: `llm-wiki/build-test.md` (counts: `test_cpu_corpus` +6, `test_gpu_corpus` +5,
  `peacock_plan_tests` +1), and #210 from `tickets/corpus-coverage.md` to
  `archive/archived-tickets.md`, with `tickets.md`

**Interfaces:**
- Produces: `static bool is_decimal_literal(const fb::Expr*)` in `expr.cpp`, beside
  `is_string_like_literal`.

- [ ] **Step 1: Change the walk test's row** to `{fb::DataType_Decimal128, id::DECIMAL128}`.
  - The row's comment says a decimal literal takes the column path, which builds the declared
    fixed point.
  - The test's header comment loses "a decimal crosses as a scaled double, so FLOAT64".
- [ ] **Step 2: Write `Literals.ACastDecimalLiteralStaysOnTheAst`:**
  - a `nation_project` of `CAST(1.5 AS DOUBLE)`, then one of `CAST(1.5 AS BIGINT)`;
  - `cudf_ast_can_evaluate` is true for each, through `plan_executor_internal.h`;
  - the answers are `FLOAT64` 1.5 and `INT64` 1.
- [ ] **Step 3: Replace `bug_a_bare_decimal_literal_is_a_float64_column_on_the_device`** with
  `a_bare_decimal_literal_agrees`: the same `Decimal128(Some(15), 3, 1)`, projected as
  `Decimal128(3, 1)`, then `run_both(…).same(Order::AsEmitted)`.
- [ ] **Step 4: Implement.**
  - The `LiteralExpr` arm returns `!is_string_like_literal(expr) && !is_decimal_literal(expr)`.
  - The `CastExprNode` arm accepts an `INT64` or `FLOAT64` target whose child is a decimal
    literal, without recursing into the literal arm.
  - The comment names #210 and says why the cast keeps the AST path: `ast_scalar`'s scaled
    double is the value the cast asks for.
- [ ] **Step 5: Turn q5's cells on.**
  - Its line becomes `all_modes, all_modes`, and each comment citing #210 or q5's device cells
    says what is true now.
  - Its registry row: five `gpu_*` cells `enabled`, and `210` struck from `tickets`.
  - Then `python3 scripts/ticket_widget.py`.
- [ ] **Step 6: Run locally:**
  - `--lib` (rust-only);
  - `--test test_cpu_corpus -- device_order_tpcds_q5`;
  - compile `--features gpu --no-run`.

  Expected: PASS.
- [ ] **Step 7: The device, red then green.**
  - **Red build:** these fail:
    - `peacock_plan_tests --gtest_filter='Literals.EveryWireType*'`, on `FLOAT64` against
      `DECIMAL128`;
    - `peacockdb_core_gpu_lib gpu_tests::exec_cases::a_bare_decimal_literal_agrees`, on the
      export's precision refusal.

    `ACastDecimalLiteralStaysOnTheAst` passes, which is expected, since it pins today's
    behaviour.
  - **Green build:** all three pass, and so do `peacock_plan_tests --gtest_filter='Literals.*'`
    and `peacockdb_core_gpu_lib gpu_tests::exec_cases`.
  - **Recording:** `$R PCK_WRITE_GPU_RESULT=1 cpp/install/rust-tests/test_gpu_corpus
    --test-threads=1 tpcds_q5_` runs 5 tests, all of which pass. Then the `rsync`: only q5's
    five sections are added. A failing cell goes back off under a ticket, and its section is
    removed.
- [ ] **Step 8: Run locally** `--test test_cpu_corpus` (rust-only). Expected: PASS, including
  `duckdb_gpu_tpcds_q5_*`.
- [ ] **Step 9: Archive #210 (AST decimal literal)** under Done, and update the index and counts.
- [ ] **Step 10: Commit**, in two commits:
  - `expr: a bare decimal literal is a decimal column on the device (#210)`;
  - `corpus: tpcds q5 on the device; #210 archived`.
- [ ] **Step 11: Final device run:** the dispatched `GPU Tests (nebius-gpu)` CI job, green.
