# typed-nulls — run record

Spec: [`typed-nulls.md`](typed-nulls.md). Plan: [`typed-nulls-impl.md`](typed-nulls-impl.md).
Branch `ENS-typed-nulls` off `ENS-declared-schemas` at `b86805b3`; PR against it when reviewing.

### 2026-09-12 — building: plan task 1 dispatched

The gtest literal helpers first, since every later test builds a literal through them. The
developer also settles whether `PCK_TEST_FILTER` reaches the gtest binaries and records it here.
#198's amendment stands over the spec: the second pin of task 9 shows a bare typed null in a
select list is a column of zeros, so the spec's "a bare literal short-circuits to null" is false
and plan task 8's test asserts what the device does after the fix, not what the spec assumed.

### 2026-09-12 — plan tasks 1–3 done: helpers repaired, two red tests on the device

**Task 1, the helpers.** `make_int64_literal` and `make_float64_literal`
(`cpp/tests/gpu/test_plan_executor.cpp`) now go through `fb::ScalarValueBuilder` and add the
field they name; `make_null_literal(fbb, type)` sits beside them and sets `is_null` with no
value, which is what `serialize.rs:92` writes for a non-decimal null (a null decimal also carries
precision and scale — plan task 5's helper, if it needs one). The plan's assumption held: the
generated `CreateScalarValue` is `(fbb, type, is_null, bool_val, int_val, uint_val, float_val,
…)`, so the old calls put `val` in `bool_val` (int) and in `uint_val` (float).

**Moved assertions, one line per helper call site** (there are three, in two tests):
- `PlanExecutor.FilterNation` — the spec and plan call it `FilterNationByRegion`; no test of
  that name exists, the `> 2` claim is `FilterNation`'s. It ran `n_regionkey > 0` (20 rows) and
  now runs `> 2` (10 rows). Its assertion `0 < rows < 25` held on both, so nothing failed on the
  device either way — which is the spec's finding: a range a wrong literal also satisfies. It now
  asserts `== 10` (regions 3 and 4, five nations each, checked against `nation.parquet` with
  duckdb locally and green on the run below). A tightening, not an adjustment to pass; the
  coordinator can drop it if it is judged beyond the task.
- `PlanExecutor.ProjectSqrtThroughTheColumnPath` — `make_int64_literal(fbb, 0)` and
  `make_float64_literal(fbb, 0.0)` built 0 and 0.0 before and after, so nothing moves; the
  `otherwise` arm is never taken on region anyway.

**Tasks 2 and 3, the red tests.** `Literals.ATypedNullInsideAnAstExpressionIsNullAndNotZero`
(project of `CAST(n_nationkey AS Int64) + NULL::Int64`, asserts `null_count == size`, size
pinned at 25) and `Literals.AComparisonAgainstATypedNullKeepsNoRows` (filter
`CAST(n_nationkey AS Int64) = NULL::Int64`, asserts zero rows). Both reuse the file's
`nation_scan_node`. They are in `test_plan_executor.cpp` (now 1470 lines, +93) as the plan's
tasks 2–8 all name that file; the ~150-line sibling threshold is for whoever adds task 5–8's
tests to decide, and a sibling would need the static helpers and `WholePlan` hoisted into a
header plus a CMake target, which no task has asked for yet.

Run `20260912T114700-371238` (`--build` rc 0 with 0 warnings, `--push-binaries --patch` rc 0,
`--run-detached`, gate exit 1 by design; every rmm pool built, 103 GiB free):
- `peacock_cpu_tests` 12 passed; `peacock_gpu_tests` 6 passed; `peacock_tpch_tests` 4 passed;
  `peacock_tpchv_tests` 4 passed.
- `peacock_plan_tests` 29 ran, 27 passed, 2 failed — the two new ones, exactly as the plan
  predicted: `view.null_count()` `Which is: 0` against `view.size()` `Which is: 25` at
  `test_plan_executor.cpp:1421`; `result.table->num_rows()` `Which is: 1` against `0` at `:1444`
  (the nation whose key is 0, so the `== 0` rather than `< 25` matters). `FilterNation` OK.
- rust, `PCK_TEST_FILTER=wire::gpu_tests::declared`: `peacockdb_core_gpu_lib` 15 passed, 1
  ignored, 837 filtered out; `test_gpu_corpus` 0 run, 8 filtered out.

**`PCK_TEST_FILTER` does not reach the gtest binaries.** `build-test-shadgpu.sh:353` runs each
`peacock_*_tests` with no arguments; the filter is forwarded only in the rust loop (`:374`
onward, through `rung_args` at `:383`). So the C++ suites run whole every cycle (about 50 s of device time, the sf40 pair
most of it), and the rust half is kept short with a real narrow filter such as
`wire::gpu_tests::declared`. `PCK_RUN_CPP=0` would skip the gtests, so it is not set on this
task. A filter matching no rust test trips the zero-test guard.

`build-test.md`: Plan-executor row 27 → 29 with the literal tests named in its text and a new
example link; C++ 67 → 69 and the grand total 1778 → 1780 moved with it (the header is the sum
of the N columns). No other wiki page counts these tests. `cpp/src/expr.cpp` untouched.
`peacock_plan_tests` stays red on this branch until plan task 4 lands the fix.

### 2026-09-12 — plan task 4 done: one scalar builder, both red tests green, the two pins deleted

**Steps 1–3, `cpp/src/expr.cpp`.** `literal_is_valid` is the one reader of `ScalarValue.is_null`
and `build_scalar` opens with it (its old two-line comment moved onto the new function).
`AstLiteralFor` + `ast_literal_for` sit above `build_expr`: a `cudf::type_dispatcher` on the
scalar's own `type()`, four `if constexpr` arms (`is_numeric`, which is `is_arithmetic` and so
covers `bool`; `is_timestamp`; `is_duration`; `string_view`) and a named throw for the rest —
which is where a `fixed_point_scalar` lands, since cuDF's `is_numeric` excludes fixed point. The
literal arm is four lines: `build_scalar(sv)`, keep the scalar in `ctx.scalars`, wrap the
borrowed reference. `build_scalar` needed a forward declaration beside the others at the file
head because it is defined after `build_expr`. Two includes added (`utilities/traits.hpp`,
`utilities/type_dispatcher.hpp`). One message corrected because the delegation made it false:
`build_scalar`'s refusal said "in column path" and now serves both paths, so it says
"unsupported scalar type: N". Nothing else in `expr.cpp` moved.

**The decimal arm went with the plan's step 3 as written**: `build_scalar(sv)` directly, no
double pre-conversion kept, so a `Decimal128` literal reaching `build_expr` today is refused by
the dispatch's named throw rather than turned into a scaled double. Every device suite stayed
green under that (below), which says no enabled cell puts a decimal literal on the AST path —
`is_ast_able`'s binary arm refuses any decimal operand, so only a bare decimal literal, or one
under a cast to Int64/Float64 or a unary, could. Plan task 5 restores the scaled-double form by
converting the `ScalarValue` before the call, and its test is what pins that.

**Runs** (`--build` rc 0, 0 warnings, twice; `--push-binaries --patch` rc 0, twice):
- `20260912T115352-373441`, narrow (`PCK_TEST_FILTER=wire::gpu_tests::declared`), exit 0:
  `peacock_cpu_tests` 12 passed; `peacock_gpu_tests` 6; `peacock_plan_tests` 29 passed with
  `Literals.ATypedNullInsideAnAstExpressionIsNullAndNotZero` and
  `Literals.AComparisonAgainstATypedNullKeepsNoRows` both `OK`; `peacock_tpch_tests` 4;
  `peacock_tpchv_tests` 4; `peacockdb_core_gpu_lib` 15 passed 1 ignored; `test_gpu_corpus` 0 run.
- `20260912T115535-373569`, filter empty, exit 1: the C++ five as above; `test_gpu_corpus` 8
  passed; `peacockdb_core_gpu_lib` 301 passed, 2 failed — exactly the two `bug_` pins for #198,
  each `cpu and gpu differ` at `compare.rs:35` where the "cpu" side is the wrong batch the pin
  expected (the unchanged `i32` column; a column of zeros) and the device now answers every row
  null in both. The device moved from wrong to right and nothing else moved.
- Both pins deleted from `gpu_tests/exec_cases.rs` in this change, per `coding-style.md`'s
  rule and the dispatch that named this as the change to do it in; `Int64Array` was theirs alone
  and left the import. `build-test.md`: their two Known-wrong rows removed, the `bug_` total 80 →
  78 in the header and the section (grand total unchanged, the two tables are never added).
  `tickets.md` #198: the "Pinned by" sentence now says the pins went red and were deleted and
  names the `Literals.*` gtests as what asserts the right answer; the ticket's move to the
  archive stays task 9's.
- `20260912T115856-374641`, filter empty, exit 0: the C++ five as above; `peacockdb_core_gpu_lib`
  301 passed 0 failed 1 ignored; `test_gpu_corpus` 8 passed. No golden moved.

`gpu_tests` is `#[cfg(all(test, feature = "gpu"))]`, so the rust-only rung cannot see the
deletion and was not run. `rustfmt --check` clean on `exec_cases.rs`. Comment caps: the two
new doc blocks are 4 and 3 lines; the one body comment 2.

### 2026-09-12 — plan tasks 5–8 done: the decimal arm, the wire-type walk, the LIKE line, the bare null

**Task 5, `ast_scalar`** (`expr.cpp`, below `ast_literal_for`). Shaped as the spec's §1 says
rather than as the plan's draft: the `Decimal128` wire value is rewritten as a Float64
`ScalarValue` (a local `FlatBufferBuilder`, `float_val` = the scaled double, `is_null` copied
through `!literal_is_valid(sv)`) and handed to `build_scalar`, so cuDF scalars are still built in
one function and the wire flag still has one reader. The plan's draft built a
`numeric_scalar<double>` in `ast_scalar` itself, a second scalar construction. The reassembly and
scaling lines are the deleted arm's, copied from `git show HEAD~1:cpp/src/expr.cpp`. The literal
arm calls `ast_scalar`; `build_scalar`'s refusal now names the type
(`fb::EnumNameDataType`) instead of its number, which is what task 6's test reads.

**The plan's decimal tests were the wrong shape, and passed for the wrong reason.** Run
`20260912T120625-376772` (helpers + tests 5a, 5b, 7, 8, no code change): `ANullDecimalLiteral…`
passed and `ADecimalLiteralStillCarriesItsScaledValue` failed with the output typed
`DECIMAL128` (id 27), not the plan's predicted throw. Cause: `is_ast_able` types operands from
the wire, `infer_expr_type` of a `Decimal128` literal is `DECIMAL128`, and the binary arm refuses
it before any conversion exists — so `CAST(col AS Float64) + <decimal literal>` takes the column
path, where `build_scalar` already honours the flag. The plan's own paragraph says so and then
prescribes that shape. What carries a decimal literal into `build_expr` is a bare literal, a
unary over it, or a cast to Int64/Float64 over it (`infer_expr_type` of a cast is its target,
and the cast arm recurses). Both tests now wrap the literal in `CAST(… AS Float64)`. Run
`20260912T120915-378573`: both red with `[in CudfProject] no cudf::ast::literal constructor for
cudf type id 27`, the plan's predicted failure; 7 and 8 green by design; `peacock_plan_tests`
31 passed 2 failed, the other four binaries green.

**Task 6, the header.** `ast_scalar` and `ast_literal_for` lost `static` and are declared in
`cpp/src/peacock/expr.h` under a three-line comment naming the test. The spec's only "no header
change" is the middle of "No ABI symbol, no header change, no `TableResult` change" — the ABI
triple, so the ABI header `cpp/include/peacock_gpu.h` — and its Restriction paragraph names no
header; `expr.h`'s own head says it is the private header that carries test-facing declarations
(`is_ast_able`, `binop_output_type` by the same route). The plan's file table said "unchanged —
no signature moves"; a new declaration moves none. `test_plan_executor.cpp` includes
`peacock/expr.h` (its include path already has `cpp/src`).

**Task 6, the walk** (`Literals.EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot`): all 22
`fb::DataType` enumerators as typed nulls, the list's length pinned to
`std::size(fb::EnumValuesDataType())`, and a refusal must contain the type's enum name. Outcome
per type on the device:
- literal (12): Boolean, Int8, Int16, Int32, Int64, Float32, Float64, Utf8, LargeUtf8,
  Utf8View, Date32, Decimal128 (as a Float64 through `ast_scalar`).
- refused by `build_scalar`, naming the type (10): Null, UInt8, UInt16, UInt32, UInt64, Float16,
  Binary, LargeBinary, Date64, BinaryView. `convert_data_type` can emit every one of these, so a
  planner that puts one in an AST expression is refused with its name; none reaches
  `ast_literal_for`, whose own throw (a scalar with no `ast::literal` constructor) nothing
  reaches today.

**Task 7.** The LIKE pattern keeps its own valid `string_scalar`; the line above it names the
guard (`!psv->string_val()` refuses a typed null, which serializes with no `string_val` —
`serialize.rs:92`) and the test. `Literals.ALikeWithANullPatternIsRefusedByTheGuard` runs a
filter `n_name LIKE NULL::Utf8` and asserts the refusal text `LIKE pattern must be a string
literal`. Green from the start: the guard predates this task.

**Task 8.** `Literals.ABareTypedNullIsStillNull`: a project of `NULL::Int64` alone comes back
`INT64` with 25 nulls. The path, read from `project.cpp:36-52` and `is_ast_able`: not a
`ColumnRef`, so `is_ast_able` is asked first; a numeric `LiteralExpr` is AST-able; so
`build_expr` + `compute_column`, never `build_column`'s literal short-circuit. The comment says
so. Green from the start now that task 4 landed, red before it (the deleted rust pin showed the
zeros).

**Runs**: `20260912T121217-380575`, narrow, exit 0 — `peacock_cpu_tests` 12, `peacock_gpu_tests`
6, `peacock_plan_tests` 34 passed (all seven `Literals.*` `OK`), `peacock_tpch_tests` 4,
`peacock_tpchv_tests` 4; `peacockdb_core_gpu_lib` 15 passed 1 ignored; `test_gpu_corpus` 0 run.
`20260912T121414-382391`, filter empty, exit 0 — the C++ five as above (the LIKE line and the
wiki counts went in between the two runs, so this run carries the final `expr.cpp`);
`peacockdb_core_gpu_lib` 301 passed 0 failed 1 ignored; `test_gpu_corpus` 8 passed. No golden
moved. Every `--build` rc 0 with 0 warnings; every rmm pool built.

`build-test.md`: Plan-executor 29 → 34 (five tests: two decimal, the walk, the LIKE guard, the
bare null), C++ 69 → 74, grand total 1780 → 1785; the row's text and its second example moved
with it. `test_plan_executor.cpp` is now 1648 lines with about 340 of them the `Literals` suite,
past the plan's ~150-line mark for a sibling `test_literals.cpp`; a sibling needs the nine
static builders, `get_scalar_value` and `WholePlan` hoisted into a test header plus a second
source on the `peacock_plan_tests` target, which no task lists, so it is left for the
coordinator to call. Comment caps: `ast_scalar`'s doc 6 lines, the `expr.h` block 3, the LIKE
line 2, the longest test comment 4.

### 2026-09-12 — plan task 9 done: the catalog check, the recount, #198 closed, architecture read

**1. The catalog.** `grep -rn 'bug_' peacockdb-core/src/wire/gpu_tests/` finds four `bug_` tests
in `declared.rs` (#183 Utf8View→Utf8, #187 the narrow decimal, #191 the extracted year, #200
Date64), none about nullability; nothing to delete. The catalog had no query reaching the AST
literal path: its one nullability finding is query 8, `SELECT n_nationkey, CASE WHEN
n_nationkey > 10 THEN n_name END AS maybe FROM nation`, whose CASE has no ELSE, and
`expr_writer.rs:136-139` writes no `else_expr` for that — so no null literal is on the wire at
all, and a CASE is refused by `is_ast_able` besides. Task 10 recorded that query's flag as the
exporter's `has_nulls()` limitation (`n_nationkey` has no null), which is unrelated to #198.

**2. The recount.** `grep -c '^TEST(' cpp/tests/gpu/test_plan_executor.cpp` = 34, the
Plan-executor row's N. The ten C++ rows: 12 + 6 + 34 + 4 + 4 + 4 + 1 + 4 + 4 + 1 = 74. The 68 N
cells of the two tables re-added by script: 1785, the grand total; Rust 1342 + C++ 74 + Python
369 = 1785. `bug_` rows in the Known-wrong table: 78, its total. Nothing on the page moved in
this step; the numbers were already right after tasks 4 and 5–8.

**3. #198 closed.** Its text moved from `tickets.md` to `archive/archived-tickets.md` under
Done, placed after #49 as #194 was, in the archive's form: the description as written, then a
`**Done 2026-09-12, by task 11 …**` paragraph naming the branch, the two commits, the shape of
the fix and the seven `Literals.*` gtests; the third paragraph's "is disabled / removes / is
false / shows" became past tense. `tickets.md`: the Critical correctness index row 23 → 22
without #198; the counter (210) untouched. Links repointed to
`archive/archived-tickets.md#t198`: `tasks/tasks.md` task 11's line (state word untouched),
`tasks/operator-cases.md:29` and `:72` (the latter now says the pins went red and were
deleted). `build-test.md` had no #198 link left after task 4. The one remaining
`tickets.md#t198` link is the frozen spec's own first line (`typed-nulls.md:5`), which only the
signoff may touch. The reports under `reports/` and other tasks' detail files keep their `#198`
text as records.

**4. `architecture.md`.** Read the node table's `CudfFilter`/`CudfProject` rows (`:820-821`),
the `ExprContext` paragraph (`:943-946`), the `cpp/src/` layout line (`:987`) and a sweep for
"literal", "valid", "twice", "fixed_point", "ScalarValue", "is_null". None of its sentences
describes the literal arm, the two builders, assumed validity or the decimal-as-double
conversion, so nothing was falsified and nothing changed.

### 2026-09-12 — reviewing: PR #152 opened against `ENS-declared-schemas`

Five commits, `13d07639` at the head. Reviewer round 1 dispatched.

### 2026-09-12 — review round 1: the private header left alone, the walk driven through the session

**Two claims in the tasks 5–8 entry were wrong, and the review caught them.** The spec's "no
header change" is not part of an ABI triple: `TableResult` is in the private
`cpp/src/plan_executor.h`, and `architecture.md:853` calls a `TableResult` change "no ABI
change", so the sentence forbids any header. And `is_ast_able`/`binop_output_type` are not a
precedent for `expr.h` — they are declared in `plan_executor_internal.h`, which `expr.h`'s own
head says is deliberately "NOT folded in", and `architecture.md:884-887` names that file as the
one thing tests reach into under `cpp/src/`. The `#include "peacock/expr.h"` in the test
falsified that sentence. Also corrected: "a numeric literal is AST-able and reaches
`build_expr`" holds, but the earlier entry's route for strings was not spelled out —
`is_string_like_literal` (`expr.cpp:308`) sends Utf8, LargeUtf8, Utf8View, Binary, LargeBinary
and BinaryView to `build_column`, whose literal arm (`:812`) is `build_scalar` +
`make_column_from_scalar`.

**What changed.** `ast_scalar` and `ast_literal_for` are `static` again; the two declarations
are gone from `cpp/src/peacock/expr.h`, which is byte-identical to the base branch; the test no
longer includes it. `Literals.EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot` now runs a bare
`NULL::T` project through `WholePlan` per type: `answer` is the cuDF type of the null column
expected back, or empty for a refusal whose message must contain the enum name; the
`EnumValuesDataType()` length pin stays. `AstLiteralFor`'s throw names the type through
`cudf::type_to_name` rather than a number (the build links it). The two history comments
(`literal_is_valid`'s "Two readers is what #198 was", `ast_scalar`'s "predates the one-builder
change, kept") are trimmed to the reason. The section comment says "The first two tests".

**Per-type outcome on the device, from the result** (run `20260912T123622-385841`):
- through `build_expr` and `compute_column`, a null column of the type named: Boolean →
  `BOOL8`, Int8/16/32/64 → `INT8`/`INT16`/`INT32`/`INT64`, Float32/64 → `FLOAT32`/`FLOAT64`,
  Date32 → `TIMESTAMP_DAYS` (a literal-only date through `compute_column` was untested ground;
  it answers 25 nulls of that type), Decimal128 → `FLOAT64` (the scaled-double arm).
- through `build_column`, a null `STRING` column: Utf8, LargeUtf8, Utf8View.
- refused by `build_scalar`'s named message, through the session as `[in CudfProject]
  unsupported scalar type: <name>`: Null, UInt8, UInt16, UInt32, UInt64, Float16, Date64 (via
  `build_expr`), Binary, LargeBinary, BinaryView (via `build_column`). Nothing reaches
  `ast_literal_for`'s own throw.

**Runs.** `--build` rc 0, 0 warnings. `20260912T123622-385841`, narrow, exit 0:
`peacock_cpu_tests` 12, `peacock_gpu_tests` 6, `peacock_plan_tests` 34 (all seven `Literals.*`
`OK`, the walk in 1196 ms), `peacock_tpch_tests` 4, `peacock_tpchv_tests` 4;
`peacockdb_core_gpu_lib` 15 passed 1 ignored; `test_gpu_corpus` 0 run. `20260912T123739-385965`,
filter empty, exit 0: the C++ five as above; `peacockdb_core_gpu_lib` 301 passed 0 failed 1
ignored; `test_gpu_corpus` 8 passed.

`build-test.md` unchanged: `TEST(` in `test_plan_executor.cpp` is still 34, so Plan-executor 34,
C++ 74, grand 1785, `bug_` 78 stand. `architecture.md:884-887` is true again: the tests'
only `peacock/` includes are `rmm_pool.hpp` and `partitioning.hpp`, both under `cpp/include/`.

### 2026-09-12 — completing: round 2 clean

Round 2 on `65790689`: 0 blocking, 0 important, 0 nits. Completeness pass dispatched — a reviewer
for what is wrong and a fresh analyst for what is missing, neither seeing the other's list.

### 2026-09-12 — completeness pass: the harness's typed-null shape restored, #210 pinned

**1. The two green forms** are back in `gpu_tests/exec_cases.rs` where the pins were, as
`operator-cases-impl.md:258-269` first wrote them: `a_typed_null_in_arithmetic_is_null`
(`i32 + NULL::Int32`, `.same(Order::AsEmitted)`) and `a_typed_null_literal_is_a_null_column`
(a bare `NULL::Int64` beside `id`). Both green on the device — the two engines agree on the
shape that #198 was about, which is the spec's coverage criterion.

**2. #210.** `bug_a_bare_decimal_literal_is_a_float64_column_on_the_device` beside them: a
`GpuProject` of `Decimal128(Some(15), 3, 1)` declared `Decimal128(3, 1)` beside `id`, asserting
the device's column is the cpu's cast to `Float64`, in the form of
`bug_decimal_arithmetic_is_exported_at_precision_38`. The device answered `Float64` with the
value intact, as the ticket states; no other finding. The walk's `Decimal128` row in
`test_plan_executor.cpp` carries `// #210` above it so `id::FLOAT64` reads as known-wrong.

**Proof.** `scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run`: 0
warnings; the binary's `--list gpu_tests::` (with `cpp/install/lib` and the rapids env on
`LD_LIBRARY_PATH`, or it lists nothing) is 305, 79 of them `bug_`; `exec_cases::` 33.
`rustfmt --check` clean on `exec_cases.rs`. Device: `--build` 0 warnings, push and patch rc 0.
Run `20260912T130448-389653`, `PCK_TEST_FILTER=tests::gpu_tests::exec_cases`, exit 0: the C++
five green (12 / 6 / 34 / 4 / 4); `peacockdb_core_gpu_lib` 33 passed with the three new cases
`ok`; `test_gpu_corpus` 0 run. Run `20260912T130552-389766`, filter empty, exit 0: the C++ five
as above; `peacockdb_core_gpu_lib` 304 passed 0 failed 1 ignored (305 listed);
`test_gpu_corpus` 8 passed.

**`build-test.md`.** Operator harness 154 → 156 (the two green cases; the `bug_` case counts
in the other table); the gpu block header 310 → 313 with `gpu_tests::` 302 → 305 (the header
counts `--list`, which includes the `bug_` names; the N column does not); Rust 1342 → 1344 and
the grand total 1785 → 1787; the known-wrong table gains the #210 row after
`bug_a_cast_to_text_is_refused_on_the_device`, 78 → 79 in the header and the section. The 68 N
cells re-added by script: 1787. Comment caps: the two rust comments are 2 lines each, the C++
row comment 1.

**Correction to the task-9 entry above.** It records `tasks/operator-cases.md:29` and `:72` as
repointed to the archive anchor; that file is a done task's frozen spec and `5660dcb0` restored
it, so those two links read `tickets.md#t198` again and resolve through the archive as
`test-layout.md`'s #49 link does. `tasks/tasks.md`'s task 11 line is the one repointed link
that stands.

### 2026-09-12 — completeness approved

Reviewer (what is wrong): 0 blocking, 2 important — the harness lost its typed-null shape when the
pins went; the walk's `Decimal128` row pinned a wrong declared type as the answer with no ticket.
Analyst (what is missing): 0 blocking, 3 important — the same two, and `substr`/`round` reading a
null argument's `int_val()` as 0; no sentence of architecture.md falsified. Green forms restored,
#210 filed and pinned, #211 filed unpinned. The signoff is on the spec. Awaiting CI.
