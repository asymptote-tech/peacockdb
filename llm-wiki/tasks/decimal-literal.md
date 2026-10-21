# A bare decimal literal is a decimal column on the device

Kind: production

**This task closes [#210](../tickets/corpus-coverage.md#t210)** (AST decimal literal). Tenth of
chain K, after value-case.

## Why

cuDF's AST has no fixed-point literal. So `ast_scalar` (`cpp/src/expr.cpp`) turns a `Decimal128`
literal into a scaled double. That is right under `CAST(… AS Float64)`. But
`cudf_ast_can_evaluate` also accepts a bare or unary-wrapped decimal literal, and then
`SELECT 1.5 FROM t` computes a `FLOAT64` column where the plan declares `Decimal128(2, 1)`. The
export refuses it by name. tpcds q5's `0.00 as return_amt` and `net_loss` hit this at every
mode, and #210 is the only ticket left on its device cells.

## The work

1. **`cudf_ast_can_evaluate` refuses a decimal literal**, in its `LiteralExpr` arm, the way its
   `BinaryExprNode` arm already refuses a decimal operand. The column path then builds a real
   `fixed_point_scalar`.
2. **A decimal literal directly under a `CAST` the AST takes (to `Float64` or `Int64`) stays on
   the AST path**, through `ast_scalar` as today. That keeps every current answer and golden
   where it is.
3. **A unary minus over a decimal literal is not this task's.** The column path has no
   `Negative` arm, and DataFusion folds `-1.5` into one literal, so the corpus never builds the
   shape.
4. `Literals.EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot`'s `Decimal128` row says why the
   literal is not an AST literal.

## Corpus

`tpcds/q5`: its five device cells turn on. `210` leaves its registry row. Its `gpu-result.txt`
sections are recorded at every mode.

## Scope

| path | change |
|---|---|
| `cpp/src/expr.cpp` | `cudf_ast_can_evaluate`'s literal and cast arms |
| `cpp/tests/gpu/test_plan_executor.cpp` | the `Literals` walk row; the CAST case |
| `peacockdb-core/src/tests/gpu_tests/exec_cases.rs` | the `bug_` pin becomes an agreement case |
| `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`, `testdata/report.html`, `testdata/goldens/tpcds.sf1/gpu-result.txt` | q5 on |
| `llm-wiki/tickets/`, `archive/archived-tickets.md`, `tickets.md`, `build-test.md` | #210 archived; counts |

Component-level API: none.

## Restriction

Which path a decimal literal takes, and nothing else. No cpu golden moves. A `gpu-result.txt`
section other than q5's that moves is a finding to explain, not to accept.

## Verification

- `bug_a_bare_decimal_literal_is_a_float64_column_on_the_device` becomes
  `a_bare_decimal_literal_agrees`: `Decimal128(15, 3, 1)` projected answers `Decimal128(3, 1)`
  `1.5` on both engines.
- gtests: `CAST(1.5 AS DOUBLE)` and `CAST(1.5 AS BIGINT)` stay AST-able and answer what they
  answer today.
- q5 green at all five device modes on nebius-gpu, DuckDB's device comparison included; the
  dispatched `GPU Tests (nebius-gpu)` CI job green.

## Signoff (2026-10-21)

Solved under its constraints. `cudf_ast_can_evaluate` refuses a bare `Decimal128` literal, so the
column path builds a `fixed_point_scalar` of the declared scale; one directly under a cast to
`Float64` or `Int64` stays on the AST and answers as before. Red then green: the walk row,
`a_bare_decimal_literal_agrees` and q5's cells; q5 on at all five device modes, only its sections
added to `gpu-result.txt`; #210 archived. Completeness: reviewer 0 blocking, 1 important (the
coordinator's own `architecture.md` wording, corrected); analyst no omissions. Shortcuts or
bandaids: none. Kept by design: the cast arm's scaled double, which can sit one ulp from arrow at
scale 2 and above. Not added: a host-only routing case for the predicate (the GPU tier pins it).
