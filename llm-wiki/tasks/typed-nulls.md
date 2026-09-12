# A typed null stays null through the AST path

Kind: production

**This task closes [#198](../tickets.md#t198)** — a typed NULL literal inside an expression the
device evaluates as a cuDF AST becomes a typed *zero* — by removing the second reader of
`ScalarValue.is_null` rather than by correcting it.

## Why it happens

A literal crosses the wire as `ScalarValue`, and `is_null` is what tells a null apart from a zero:
the schema says so itself — *"Without this flag a null is indistinguishable from a zero/empty
value."* The C++ reads that flag in two places and only one of them reads it.

- `build_scalar` (`expr.cpp:456`) opens with `bool valid = !sv->is_null();` and passes it to all
  ten arms. Correct.
- `build_expr`'s `LiteralExpr` arm (`expr.cpp:158`) builds the same ten scalars again for the
  AST, and every one of them passes `true`.

So the answer depends on which path the expression took, and `is_ast_able` decides. A bare literal
short-circuits to `build_scalar` at `build_column:830` and is null. `CaseExprNode` and
`LikeExprNode` are refused by `is_ast_able` and take the column path, so they are null too.

**What reaches the bug is a binary op with a numeric null literal, where both operands infer to the
same non-decimal type** — `is_ast_able`'s binary arm refuses a string literal on either side, refuses
a decimal operand, and refuses a type mismatch, so what survives is exactly `col <op> NULL::T` for a
numeric `T` matching the column. Strings escape for a reason of their own: cuDF's AST has no string
literal.

The consequence differs by operator, and one of the two is worse than a wrong value:

- **arithmetic** — `col + NULL` yields `col`, because the literal is a zero. A wrong value in one
  column.
- **comparison** — `col = NULL` is *true* for every row where `col` is 0, where SQL says the
  predicate is unknown and the row does not survive. A wrong **row count**, which no value-level
  comparison downstream can detect.

The shape was first noticed as a two-engine disagreement on an outer join's pad — the CPU answering
`NULL` where the device answered `0`, Int64 only. That account is inherited rather than reproduced,
and reproducing it is not what this task turns on: the duplication above is visible in the source and
the test in step 4 pins it directly.

**The duplication is the bug, not the missing flag.** Ten arms written twice, one copy asking the
wire and the other assuming. Correcting the ten values leaves the shape that produced them, and the
eleventh type someone adds next year has two places to be added to and one test.

## The work

### 1. One scalar builder, not two

`build_expr`'s literal arm stops building scalars. It obtains one from `build_scalar` and wraps it
in a `cudf::ast::literal`.

The wrapping needs a downcast: `cudf::ast::literal`'s four constructors take a concrete
`numeric_scalar<T>&`, `timestamp_scalar<T>&`, `duration_scalar<T>&` or `string_scalar&`, never a
`cudf::scalar&`. A `cudf::type_dispatcher` over the scalar's own `type()` is the mechanism; the
scalar stays owned by `ExprContext::scalars` exactly as today, and the `literal` borrows it.

**One arm differs on purpose and must stay differing.** cuDF's AST has no fixed-point literal, so
`build_expr` today turns a `Decimal128` into a `double`, scaled — while `build_scalar` builds a real
`fixed_point_scalar`. Do not collapse that into a flag on `build_scalar`; a function that changes
what it returns based on who is asking is the antipattern `coding-style.md` names. Convert the
*`ScalarValue`* before the call instead — an AST-representable value of the same literal — so there
is still one scalar builder and one reader of `is_null`.

### 2. The LIKE pattern, decided rather than assumed

`build_column:901` builds `cudf::string_scalar pattern(psv->string_val()->str(), true)` — the
eleventh site, hardcoding validity like the ten. Its guard above refuses a pattern with no
`string_val`, which is what a typed null serializes as, so it is arguably valid by construction.
Say which it is in code: either it goes through `build_scalar` like the rest, or it keeps its own
construction under a line stating why the guard is sufficient. A comment asserting the conclusion
without the reasoning is what this task is repairing.

### 3. The gtest literal helpers address the fields they name

`test_plan_executor.cpp:50` calls

```cpp
fb::CreateScalarValue(fbb, fb::DataType_Int64, /*bool_val=*/false, /*int_val=*/val);
```

against a signature of `(fbb, type, is_null, bool_val, int_val, …)`. The comments name the fields
they meant; the arguments land one position earlier. `int_val` is therefore **always 0** — every
`make_int64_literal(fbb, 2)` in that file builds the literal `0` — and `make_float64_literal` puts
its double in `uint_val` while `float_val` stays `0.0`. Nothing catches it: all three parameters are
implicitly convertible.

This is the doc-comment-reassigned-by-an-insertion antipattern one level up. `is_null` was inserted
as field 2 of `ScalarValue` and the call sites kept their old positions. Use designated field
assignment through `ScalarValueBuilder`, as the Rust writer does with `ScalarValueArgs::default()`,
so a field inserted later cannot silently take an argument.

**Fix this before writing the test in step 4**, or the test asserts against a literal it did not
build.

**Existing assertions will move, and every move is reviewed rather than accepted.**
`PlanExecutor.FilterNationByRegion` currently runs `n_regionkey > 0` and asserts only
`0 < rows < 25`; with the literal repaired it runs the `> 2` it always claimed. A test whose
numbers change here was testing something other than what it said.

### 4. Tests

- **a typed null inside an AST expression is null and not zero** — the test that pins the fix.
  A project of `col + NULL::Int64` over a scan: every output row null. Today every output row equals
  `col`. Red before step 1.
- **a comparison against a typed null drops the row** — the worse half, and a separate test because
  it fails differently: a filter of `col = NULL::Int64` returns no rows. Today it returns the rows
  where `col` is 0, so a test asserting only "not all rows" would pass on data with no zeroes.
- **a bare typed null is still null** — the `build_column` short-circuit, which was always correct
  and which step 1 must not disturb.
- **a null decimal literal in an AST expression is null** — the arm step 1 keeps deliberately
  different, since cuDF's AST has no fixed-point literal and a `Decimal128` becomes a scaled
  `double`. It is the only arm the delegation does not cover by construction, so it is where the
  refactor breaks if it breaks. Assert the null, and assert the non-null case still carries the
  scaled value, or the conversion can be lost without the null test noticing.
- **every wire type reaching `build_expr` produces a literal** — the type dispatch in step 1
  downcasts a `cudf::scalar` to a concrete scalar class, and a type with no arm fails at run time
  rather than at compile time. Walk the types `convert_data_type` can emit and assert each yields a
  literal or a refusal that names the type. Without this, a type added to the wire later lands in a
  dispatch that quietly has no arm for it — the invisible-absence shape, in a new place.
- **the LIKE pattern, whichever way step 2 decides** — if it delegates, a null pattern is refused by
  the existing guard and the test says so; if it keeps its own construction, the test is that the
  guard actually refuses what the comment claims it refuses. One test either way, because the point
  of step 2 is that the answer stops being an assumption.
- **the two builders cannot diverge again** — with one builder there is nothing to assert; if the
  author keeps two for a reason found on the way, this test becomes mandatory and walks every
  `fb::DataType`, asserting both agree on validity.

## Scope of code changes

Every file this touches, and nothing else compiles differently.

| file | change |
|---|---|
| `cpp/src/expr.cpp` | `build_expr`'s `LiteralExpr` arm (`:157`–`:255`) loses its ten scalar constructions and delegates to `build_scalar` (`:453`), wrapping the result in a `cudf::ast::literal` through a type dispatch. `ExprContext::scalars` keeps ownership as it does today and the `literal` borrows. Plus the decimal pre-conversion in step 1 and the LIKE decision in step 2 (`:901`) |
| `cpp/tests/gpu/test_plan_executor.cpp` | `make_int64_literal` and `make_float64_literal` (`:49`–`:66`) rebuilt with designated fields; whatever assertions move as a result |
| `cpp/tests/gpu/` | the new tests in step 4, in this file or a sibling |

**No Rust changes at all.** `serialize.rs` already writes `is_null` correctly through
`ScalarValueArgs::default()`, and nothing on the Rust side reads a literal's validity.

**No wire change**, no `.fbs` edit, no regeneration: `ScalarValue.is_null` exists and is already
populated. **No ABI symbol**, no header change, no `TableResult` change. **No golden regeneration
expected** — see the table below, and a device result that does move is the finding rather than a
golden to accept.

Net: one C++ source file, one C++ test file, and a new test. If the type dispatch in step 1 turns out
to need a helper of its own, it lives in `expr.cpp` beside `build_scalar` and not in a new
translation unit — the two builders being in one file is half of why the duplication was invisible.

## Restriction

**Code and test changes are limited to what is written above.** No other arm of `build_expr`, no
change to `is_ast_able`'s routing, no Rust change, no ABI symbol, no cleanup of `expr.cpp`'s
neighbours while passing through. Anything else found on the way is a ticket.

## Goldens, and how each moves

| golden | how it moves | why |
|---|---|---|
| `*.plans.txt`, `recipe-payloads.txt` | **no change** | plan text renders the Rust tree; nothing about serialization moves |
| `<mode>-<tier>.cpu.txt`, `.cost.txt` | **no change** | the CPU engine is DataFusion and was always right about nulls |
| `<tier>.result.txt` | **no change expected** — verify | a device result that moves is a query that was answering wrongly, and is worth naming in the report |
| `testdata/cost-registry.csv` | **no change** | no cell is disabled against #198 |

## What it may close in the catalog

This task runs after both `declared-schemas` tasks, so their catalog exists when it starts, and
#198 has a shadow there. A typed NULL built *valid* yields a column with no nulls at all, and the
production exporter derives nullability from `col.has_nulls()` — so a plan declaring a nullable
column exports it **non-nullable**, which is a schema divergence in the catalog's own terms. The thin
test exporter carries the declaration instead and would not show it, which makes this one of the
cases the catalog is built to separate: *the two exporters disagreeing is itself the finding.*

So if the catalog files a `bug_` test for a nullable column exporting non-nullable in a plan carrying
a null literal, **this task deletes it**, in the change that fixes the cause. Check for one before
starting; its absence means the catalog had no query reaching the AST literal path, which is worth
saying in the report either way.

## Coverage

**This task claims no cells.** It is judged on correctness: two engines that disagreed now agree.
Stated here so it is not measured against a yardstick it was never meant to move — the registry is
how a refusal is scored, and a wrong answer is not a refusal.

## Device workflow

`build-test-shadgpu.sh`. The gtests are the gate and need one cycle each for red and green.
Then the enabled device cells, because a wrong answer that nothing refused could be sitting in one:
a result that moves is the finding, not a golden to accept.

## Completeness signoff

Solved under its constraints: one scalar builder and one reader of `is_null`, the decimal arm
converted before it, the LIKE guard decided in code, the gtest helpers built field by field with
`FilterNation`'s one moved assertion explained and tightened; the seven `Literals` gtests, every
wire type walked (twelve literals, ten named refusals); `is_ast_able`, the wire, the ABI and every
header untouched; no golden moved; #198 archived. Deviations, none a shortcut: task 9's two `bug_`
pins deleted in the fixing change and their green forms restored — "no Rust changes" read as
production code; `expr.h` declared two functions for one round, restored, the walk driven through
whole plans; the spec's bare-literal premise was false (#198) and test 3 exercises `build_expr`;
the `Literals` suite left in `test_plan_executor.cpp` past the sibling threshold; decimal-as-double
carried through by the spec's instruction, ticketed #210 with its `bug_` pin; #211 filed for substr.
