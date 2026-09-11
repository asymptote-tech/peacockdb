# Typed nulls implementation plan

**Goal:** Remove the second reader of `ScalarValue.is_null` so a typed NULL literal is null on every
path, not only on the one that asks the wire.

**Architecture:** `build_expr`'s literal arm stops constructing scalars and delegates to
`build_scalar`, wrapping the result in a `cudf::ast::literal` through a `cudf::type_dispatcher`
downcast. One arm stays different on purpose — cuDF's AST has no fixed-point literal — and is handled
by converting after `build_scalar` rather than by a flag on it.

**Tech stack:** C++17, cuDF 25.02, gtest, flatbuffers. Device tests on `shad-gpu`.

**Spec:** [`typed-nulls.md`](typed-nulls.md) — frozen. Read it before Task 1.

## Global constraints

- Tests are gtests under `cpp/tests/gpu/`, run on `shad-gpu`; there is no local GPU.
- One device cycle per red/green pair. Batch where the spec allows; never claim green without the
  run output.
- **No Rust changes, no `.fbs` edit, no ABI symbol, no header change.** See the spec's scope table.
- A test name is a claim, not a description of a mechanism (`coding-style.md`, Names).
- Commit messages at most 10 lines including the subject.

## Device cycle

The loop every task below ends with:

```bash
./scripts/build-test-shadgpu.sh --build --push-binaries --patch --run
```

Filtering: `PCK_TEST_FILTER=<pattern>` narrows the run. **Confirm on Task 1 that the filter reaches
the gtest binaries** — the documented example (`PCK_TEST_FILTER=q6`) is a Rust corpus filter, and if
it does not reach gtest, run the C++ suite unfiltered and read the relevant lines. Record which it
was in `typed-nulls-detail.md` so no later task re-derives it.

## File structure

| file | responsibility |
|---|---|
| `cpp/src/expr.cpp` | the change: `literal_is_valid`, the dispatch functor, the delegating literal arm, the decimal conversion, the LIKE decision |
| `cpp/src/peacock/expr.h` | unchanged — no signature moves |
| `cpp/tests/gpu/test_plan_executor.cpp` | the helper repair, and the new tests unless they grow past ~150 lines, in which case a sibling `test_literals.cpp` |

---

### Task 1: The gtest literal helpers address the fields they name

This is first because every later test builds a literal, and today the helpers build the wrong one.

**Files:**
- Modify: `cpp/tests/gpu/test_plan_executor.cpp:49-66`

**Interfaces:**
- Produces: `make_int64_literal(fbb, val)`, `make_float64_literal(fbb, val)` — unchanged signatures,
  corrected bodies. Task 2 onward relies on `val` actually reaching `int_val`.

- [ ] **Step 1: Read the generated signature and confirm the defect**

```bash
grep -n 'CreateScalarValue' -A 14 cpp/build/generated/gpu_plan_generated.h | head -20
```

Expected: `(fbb, type, is_null, bool_val, int_val, uint_val, float_val, …)`. The call site passes
`(fbb, DataType_Int64, false, val)`, so `false` lands on `is_null` and `val` on `bool_val`, leaving
`int_val` at 0.

- [ ] **Step 2: Rewrite both helpers with designated fields**

```cpp
/// Build an Expr wrapping an Int64 literal.
///
/// Built field by field rather than positionally: `ScalarValue` gained `is_null` as its
/// second field, and the positional call that predated it silently moved every argument
/// one place left.
static flatbuffers::Offset<fb::Expr> make_int64_literal(
    flatbuffers::FlatBufferBuilder& fbb, int64_t val) {
  fb::ScalarValueBuilder sb(fbb);
  sb.add_type(fb::DataType_Int64);
  sb.add_int_val(val);
  auto sv = sb.Finish();
  auto lit = fb::CreateLiteralExpr(fbb, sv);
  return fb::CreateExpr(fbb, fb::ExprNode_LiteralExpr, lit.Union());
}

/// Build an Expr wrapping a Float64 literal.
static flatbuffers::Offset<fb::Expr> make_float64_literal(
    flatbuffers::FlatBufferBuilder& fbb, double val) {
  fb::ScalarValueBuilder sb(fbb);
  sb.add_type(fb::DataType_Float64);
  sb.add_float_val(val);
  auto sv = sb.Finish();
  auto lit = fb::CreateLiteralExpr(fbb, sv);
  return fb::CreateExpr(fbb, fb::ExprNode_LiteralExpr, lit.Union());
}
```

- [ ] **Step 3: Add a typed-null helper beside them**

```cpp
/// A typed NULL literal: `is_null` set and no value, which is how the Rust serializer
/// writes one.
static flatbuffers::Offset<fb::Expr> make_null_literal(
    flatbuffers::FlatBufferBuilder& fbb, fb::DataType type) {
  fb::ScalarValueBuilder sb(fbb);
  sb.add_type(type);
  sb.add_is_null(true);
  auto sv = sb.Finish();
  auto lit = fb::CreateLiteralExpr(fbb, sv);
  return fb::CreateExpr(fbb, fb::ExprNode_LiteralExpr, lit.Union());
}
```

- [ ] **Step 4: Run the suite and read every moved assertion**

Run the device cycle. `PlanExecutor.FilterNationByRegion` was running `n_regionkey > 0` and now runs
the `> 2` it always claimed, so its row count changes.

**Do not adjust an assertion to make it pass.** For each test whose numbers move, work out what it
was asserting before and what it asserts now, and write one line per test into
`typed-nulls-detail.md`. A test that cannot be explained this way was testing something other than
what it said, and that is a finding for the report.

- [ ] **Step 5: Commit**

```bash
git add cpp/tests/gpu/test_plan_executor.cpp
git commit -m "the literal helpers address the fields they name

ScalarValue gained is_null as field 2 and the positional calls kept their
old offsets, so every make_int64_literal built the literal 0 and every
make_float64_literal put its double in uint_val. Built field by field now.
FilterNationByRegion runs the > 2 it always claimed."
```

---

### Task 2: A typed null in an AST expression is null, not zero — the failing test

**Files:**
- Modify: `cpp/tests/gpu/test_plan_executor.cpp`

**Interfaces:**
- Consumes: `make_null_literal`, `make_col_ref`, `make_binary_expr`, `make_schema`,
  `make_plan_node`, `finish_plan`, `WholePlan` — all already in the file.
- Produces: nothing later tasks consume.

- [ ] **Step 1: Write the test**

A project of `n_nationkey + NULL::Int64`. The cast keeps both operands `Int64`, which is what
`is_ast_able`'s binary arm requires — an operand type mismatch would route to the column path and the
test would pass for the wrong reason.

```cpp
TEST(Literals, ATypedNullInsideAnAstExpressionIsNullAndNotZero) {
  flatbuffers::FlatBufferBuilder fbb;
  auto path = fbb.CreateString(parquet_path("nation"));
  auto paths = fbb.CreateVector(
      std::vector<flatbuffers::Offset<flatbuffers::String>>{path});
  auto schema = make_schema(fbb, {
      {"n_nationkey", fb::DataType_Int32},
      {"n_name", fb::DataType_Utf8View},
      {"n_regionkey", fb::DataType_Int32},
      {"n_comment", fb::DataType_Utf8View},
  });
  auto scan = fb::CreateCudfScan(fbb, paths, schema);
  auto scan_node = make_plan_node(fbb, fb::PlanNodeKind_CudfScan, scan.Union());

  // Both operands Int64 so is_ast_able takes the AST path: a type mismatch would
  // route to build_column, which reads is_null correctly and would pass regardless.
  auto col0 = make_col_ref(fbb, 0, "n_nationkey");
  auto cast0 = make_cast_expr(fbb, col0, fb::DataType_Int64);
  auto null_lit = make_null_literal(fbb, fb::DataType_Int64);
  auto sum = make_binary_expr(fbb, cast0, fb::BinaryOp_Plus, null_lit);

  auto exprs = fbb.CreateVector(
      std::vector<flatbuffers::Offset<fb::Expr>>{sum});
  auto alias = fbb.CreateString("keyplusnull");
  auto aliases = fbb.CreateVector(
      std::vector<flatbuffers::Offset<flatbuffers::String>>{alias});
  auto proj = fb::CreateCudfProject(fbb, exprs, aliases, scan_node);
  auto proj_node =
      make_plan_node(fbb, fb::PlanNodeKind_CudfProject, proj.Union());
  auto buf = finish_plan(fbb, proj_node);

  WholePlan plan(buf);
  const auto& result = plan.result();

  ASSERT_EQ(result.table->num_columns(), 1);
  auto view = result.table->view().column(0);
  ASSERT_GT(view.size(), 0);
  // Every row null: x + NULL is NULL for every x.
  EXPECT_EQ(view.null_count(), view.size());
}
```

- [ ] **Step 2: Run it and watch it fail**

Run the device cycle. Expected: `null_count()` is 0 against a `size()` of 25 — the literal was built
valid, so it is a zero and `x + 0` is `x`.

**If it passes**, the expression took the column path. Check `is_ast_able`: a string literal on either
side, a decimal operand, or an operand type mismatch all route away from `build_expr`. Fix the test,
not the code.

- [ ] **Step 3: Commit the red test**

```bash
git add cpp/tests/gpu/test_plan_executor.cpp
git commit -m "a typed null in an AST expression is null and not zero

Red: build_expr builds its literal valid, so x + NULL::Int64 is x. Pins
#198 before the fix."
```

---

### Task 3: A comparison against a typed null drops the row — the second failing test

Separate from Task 2 because it fails differently: the wrong answer is a row count, not a value.

**Files:**
- Modify: `cpp/tests/gpu/test_plan_executor.cpp`

- [ ] **Step 1: Write the test**

```cpp
TEST(Literals, AComparisonAgainstATypedNullKeepsNoRows) {
  flatbuffers::FlatBufferBuilder fbb;
  auto path = fbb.CreateString(parquet_path("nation"));
  auto paths = fbb.CreateVector(
      std::vector<flatbuffers::Offset<flatbuffers::String>>{path});
  auto schema = make_schema(fbb, {
      {"n_nationkey", fb::DataType_Int32},
      {"n_name", fb::DataType_Utf8View},
      {"n_regionkey", fb::DataType_Int32},
      {"n_comment", fb::DataType_Utf8View},
  });
  auto scan = fb::CreateCudfScan(fbb, paths, schema);
  auto scan_node = make_plan_node(fbb, fb::PlanNodeKind_CudfScan, scan.Union());

  // nation has an n_nationkey of 0, so a literal-zero bug keeps exactly one row
  // and an assertion of "fewer rows than before" would pass on the bug.
  auto col0 = make_col_ref(fbb, 0, "n_nationkey");
  auto cast0 = make_cast_expr(fbb, col0, fb::DataType_Int64);
  auto null_lit = make_null_literal(fbb, fb::DataType_Int64);
  auto predicate = make_binary_expr(fbb, cast0, fb::BinaryOp_Eq, null_lit);

  auto filter = fb::CreateCudfFilter(fbb, predicate, scan_node);
  auto filter_node =
      make_plan_node(fbb, fb::PlanNodeKind_CudfFilter, filter.Union());
  auto buf = finish_plan(fbb, filter_node);

  WholePlan plan(buf);
  const auto& result = plan.result();

  // SQL: `x = NULL` is unknown for every x, so no row survives.
  EXPECT_EQ(result.table->num_rows(), 0);
}
```

- [ ] **Step 2: Run it and watch it fail**

Expected: 1 row — the nation whose `n_nationkey` is 0, matched against a literal zero. That single row
is why the test asserts `== 0` rather than "fewer than 25".

- [ ] **Step 3: Commit the red test**

```bash
git add cpp/tests/gpu/test_plan_executor.cpp
git commit -m "a comparison against a typed null keeps no rows

Red: the literal is a zero, so n_nationkey = NULL matches the nation whose
key is 0. A wrong row count rather than a wrong value."
```

---

### Task 4: One scalar builder — the fix

**Files:**
- Modify: `cpp/src/expr.cpp:157-255` (the `LiteralExpr` arm), and add two helpers above `build_expr`

**Interfaces:**
- Consumes: `build_scalar(const fb::ScalarValue*)` at `expr.cpp:453`, returning
  `std::unique_ptr<cudf::scalar>`.
- Produces: `literal_is_valid(const fb::ScalarValue*) -> bool` and
  `ast_literal_for(cudf::scalar&) -> std::unique_ptr<cudf::ast::literal>`, both file-local. Task 5
  extends the first's caller set; Task 6 tests the second.

- [ ] **Step 1: Add the single validity reader, and make `build_scalar` use it**

`build_scalar` currently opens with `bool valid = !sv->is_null();`. Replace that line with a call, so
the wire flag is read in exactly one function:

```cpp
/// The one place `ScalarValue.is_null` is read. A typed NULL literal is encoded with the
/// flag set and its value fields unused, and the scalar is built invalid so cuDF treats it
/// as a null of `type`. Two readers is what #198 was: the AST path built its own scalars
/// and assumed validity.
static bool literal_is_valid(const fb::ScalarValue* sv) { return !sv->is_null(); }
```

```cpp
static std::unique_ptr<cudf::scalar> build_scalar(const fb::ScalarValue* sv) {
  bool valid = literal_is_valid(sv);
  switch (sv->type()) {
    // ... arms unchanged
```

- [ ] **Step 2: Add the dispatch that wraps a scalar as an AST literal**

`cudf::ast::literal` takes a concrete scalar reference — `numeric_scalar<T>&`,
`timestamp_scalar<T>&`, `duration_scalar<T>&` or `string_scalar&` — never a `cudf::scalar&`, so the
downcast has to be made by type.

```cpp
/// `cudf::ast::literal` has four constructors and none of them takes a `cudf::scalar&`, so
/// the concrete type has to be recovered. Dispatching on the scalar's own `type()` rather
/// than on the fb tag keeps this honest about what was actually built.
struct AstLiteralFor {
  template <typename T>
  std::unique_ptr<cudf::ast::literal> operator()(cudf::scalar& s) const {
    if constexpr (cudf::is_numeric<T>()) {
      return std::make_unique<cudf::ast::literal>(
          static_cast<cudf::numeric_scalar<T>&>(s));
    } else if constexpr (cudf::is_timestamp<T>()) {
      return std::make_unique<cudf::ast::literal>(
          static_cast<cudf::timestamp_scalar<T>&>(s));
    } else if constexpr (cudf::is_duration<T>()) {
      return std::make_unique<cudf::ast::literal>(
          static_cast<cudf::duration_scalar<T>&>(s));
    } else if constexpr (std::is_same_v<T, cudf::string_view>) {
      return std::make_unique<cudf::ast::literal>(
          static_cast<cudf::string_scalar&>(s));
    } else {
      // Named rather than defaulted: a type cuDF gains a scalar for lands here
      // instead of silently producing no literal.
      throw std::runtime_error(
          "no cudf::ast::literal constructor for cudf type id " +
          std::to_string(static_cast<int>(s.type().id())));
    }
  }
};

static std::unique_ptr<cudf::ast::literal> ast_literal_for(cudf::scalar& s) {
  return cudf::type_dispatcher(s.type(), AstLiteralFor{}, s);
}
```

- [ ] **Step 3: Replace the literal arm with the delegation**

The whole `switch (sv->type())` block inside `case fb::ExprNode_LiteralExpr:` — ten arms — becomes:

```cpp
    case fb::ExprNode_LiteralExpr: {
      auto* lit = expr->node_as_LiteralExpr();
      auto* sv = lit->value();
      if (!sv) throw std::runtime_error("LiteralExpr has no value");

      auto scalar = ast_scalar(sv);
      auto& ref = *scalar;
      ctx.scalars.push_back(std::move(scalar));
      return ctx.keep(ast_literal_for(ref));
    }
```

`ast_scalar` is Task 5's; until then, use `build_scalar(sv)` directly and expect the decimal test to
be the one that fails.

Ownership is unchanged: `ExprContext::scalars` is already
`std::vector<std::unique_ptr<cudf::scalar>>`, which is exactly what `build_scalar` returns, and the
`literal` borrows the reference as it does today.

- [ ] **Step 4: Run Tasks 2 and 3 green**

Run the device cycle. Both new tests pass, and the whole gtest suite stays green — read the summary,
do not assume.

- [ ] **Step 5: Commit**

```bash
git add cpp/src/expr.cpp
git commit -m "one scalar builder, not two

build_expr built the same ten scalars again for the AST and passed valid
for all of them, so a typed NULL inside an expression was a typed zero
(#198). It now delegates to build_scalar and wraps the result through a
type dispatch. literal_is_valid is the only reader of the wire flag."
```

---

### Task 5: The decimal arm, which stays different on purpose

cuDF's AST has no fixed-point literal, so a `Decimal128` cannot be wrapped by Task 4's dispatch —
`AstLiteralFor` throws for it. The existing behaviour converts to a scaled `double`, and that must
survive, with validity still read in one place.

**Files:**
- Modify: `cpp/src/expr.cpp`
- Modify: `cpp/tests/gpu/test_plan_executor.cpp`

**Interfaces:**
- Produces: `ast_scalar(const fb::ScalarValue*) -> std::unique_ptr<cudf::scalar>`, which Task 4's arm
  calls.

- [ ] **Step 1: Write the two failing tests first**

First the helper, beside the others in Task 1:

```cpp
/// A Decimal128 literal. `hi`/`lo` form the 128-bit signed value; Arrow scale is the
/// count of fractional digits, so 250 at scale 2 is 2.50.
static flatbuffers::Offset<fb::Expr> make_decimal_literal(
    flatbuffers::FlatBufferBuilder& fbb, int64_t hi, uint64_t lo,
    uint8_t precision, int8_t scale) {
  fb::ScalarValueBuilder sb(fbb);
  sb.add_type(fb::DataType_Decimal128);
  sb.add_decimal_hi(hi);
  sb.add_decimal_lo(lo);
  sb.add_decimal_precision(precision);
  sb.add_decimal_scale(scale);
  auto sv = sb.Finish();
  auto lit = fb::CreateLiteralExpr(fbb, sv);
  return fb::CreateExpr(fbb, fb::ExprNode_LiteralExpr, lit.Union());
}

/// The same, as a typed NULL: `is_null` set, and the value fields left out because a
/// consumer that reads them for a null is the bug this task removes.
static flatbuffers::Offset<fb::Expr> make_null_decimal_literal(
    flatbuffers::FlatBufferBuilder& fbb, uint8_t precision, int8_t scale) {
  fb::ScalarValueBuilder sb(fbb);
  sb.add_type(fb::DataType_Decimal128);
  sb.add_is_null(true);
  sb.add_decimal_precision(precision);
  sb.add_decimal_scale(scale);
  auto sv = sb.Finish();
  auto lit = fb::CreateLiteralExpr(fbb, sv);
  return fb::CreateExpr(fbb, fb::ExprNode_LiteralExpr, lit.Union());
}
```

Then the two tests. Both cast the column to `Float64` so that after `ast_scalar`'s conversion both
operands are doubles — `is_ast_able` refuses a decimal operand outright, so a decimal on either side
of the comparison would route to the column path and prove nothing about `build_expr`.

```cpp
/// The scan both decimal tests run over, factored out because the only difference
/// between them is the literal.
static flatbuffers::Offset<fb::PlanNode> nation_scan(
    flatbuffers::FlatBufferBuilder& fbb) {
  auto path = fbb.CreateString(parquet_path("nation"));
  auto paths = fbb.CreateVector(
      std::vector<flatbuffers::Offset<flatbuffers::String>>{path});
  auto schema = make_schema(fbb, {
      {"n_nationkey", fb::DataType_Int32},
      {"n_name", fb::DataType_Utf8View},
      {"n_regionkey", fb::DataType_Int32},
      {"n_comment", fb::DataType_Utf8View},
  });
  auto scan = fb::CreateCudfScan(fbb, paths, schema);
  return make_plan_node(fbb, fb::PlanNodeKind_CudfScan, scan.Union());
}

TEST(Literals, ANullDecimalLiteralInAnAstExpressionIsNull) {
  flatbuffers::FlatBufferBuilder fbb;
  auto scan_node = nation_scan(fbb);

  // A Decimal128 reaches the AST as a scaled double, which is the one conversion the
  // delegation does not cover — so this is the arm that breaks if the refactor breaks.
  auto col0 = make_col_ref(fbb, 0, "n_nationkey");
  auto as_double = make_cast_expr(fbb, col0, fb::DataType_Float64);
  auto null_dec = make_null_decimal_literal(fbb, /*precision=*/15, /*scale=*/2);
  auto sum = make_binary_expr(fbb, as_double, fb::BinaryOp_Plus, null_dec);

  auto exprs = fbb.CreateVector(
      std::vector<flatbuffers::Offset<fb::Expr>>{sum});
  auto alias = fbb.CreateString("keyplusnull");
  auto aliases = fbb.CreateVector(
      std::vector<flatbuffers::Offset<flatbuffers::String>>{alias});
  auto proj = fb::CreateCudfProject(fbb, exprs, aliases, scan_node);
  auto proj_node =
      make_plan_node(fbb, fb::PlanNodeKind_CudfProject, proj.Union());
  auto buf = finish_plan(fbb, proj_node);

  WholePlan plan(buf);
  const auto& result = plan.result();

  ASSERT_EQ(result.table->num_columns(), 1);
  auto view = result.table->view().column(0);
  ASSERT_GT(view.size(), 0);
  EXPECT_EQ(view.null_count(), view.size());
}

TEST(Literals, ADecimalLiteralStillCarriesItsScaledValue) {
  flatbuffers::FlatBufferBuilder fbb;
  auto scan_node = nation_scan(fbb);

  // The non-null half of the same conversion. 250 at scale 2 is 2.50, so every row is
  // its key plus 2.5 — a test asserting only "not null" would pass with the scaling
  // dropped, which is how the conversion could be lost silently.
  auto col0 = make_col_ref(fbb, 0, "n_nationkey");
  auto as_double = make_cast_expr(fbb, col0, fb::DataType_Float64);
  auto dec = make_decimal_literal(fbb, /*hi=*/0, /*lo=*/250,
                                  /*precision=*/15, /*scale=*/2);
  auto sum = make_binary_expr(fbb, as_double, fb::BinaryOp_Plus, dec);

  auto exprs = fbb.CreateVector(
      std::vector<flatbuffers::Offset<fb::Expr>>{sum});
  auto alias = fbb.CreateString("keyplustwofifty");
  auto aliases = fbb.CreateVector(
      std::vector<flatbuffers::Offset<flatbuffers::String>>{alias});
  auto proj = fb::CreateCudfProject(fbb, exprs, aliases, scan_node);
  auto proj_node =
      make_plan_node(fbb, fb::PlanNodeKind_CudfProject, proj.Union());
  auto buf = finish_plan(fbb, proj_node);

  WholePlan plan(buf);
  const auto& result = plan.result();

  ASSERT_EQ(result.table->num_columns(), 1);
  auto host = cudf::test::to_host<double>(result.table->view().column(0));
  ASSERT_GT(host.first.size(), 0u);
  // nation's keys are 0..24, so the smallest answer is 0 + 2.50.
  EXPECT_DOUBLE_EQ(host.first[0], 2.50);
}
```

If `cudf::test::to_host` is not already reachable from this file, copy whatever the neighbouring
value-asserting tests use — do not add a new test dependency for one assertion.

- [ ] **Step 2: Run and watch them fail**

Expected after Task 4: a thrown `std::runtime_error` naming the cuDF type id, because
`AstLiteralFor` has no fixed-point arm. That is the right failure — it means the dispatch refuses
loudly rather than producing a wrong literal.

- [ ] **Step 3: Add `ast_scalar`**

```cpp
/// The scalar the AST can hold for this literal. Identical to `build_scalar` except for
/// `Decimal128`, which cuDF's AST has no literal for and which therefore crosses as a
/// scaled double — the behaviour that predates this change, kept.
///
/// A separate function rather than a flag on `build_scalar`: a function that returns a
/// different type depending on who is asking is the antipattern `coding-style.md` names.
/// Validity still comes from `literal_is_valid`, so the wire flag has one reader.
static std::unique_ptr<cudf::scalar> ast_scalar(const fb::ScalarValue* sv) {
  if (sv->type() != fb::DataType_Decimal128) return build_scalar(sv);

  __int128_t val = (static_cast<__int128_t>(sv->decimal_hi()) << 64) |
                   static_cast<__int128_t>(sv->decimal_lo());
  int8_t scale = sv->decimal_scale();
  double dval = static_cast<double>(val);
  for (int8_t i = 0; i < scale; ++i) dval /= 10.0;
  for (int8_t i = 0; i > scale; --i) dval *= 10.0;
  return std::make_unique<cudf::numeric_scalar<double>>(dval, literal_is_valid(sv));
}
```

Copy the exact reassembly and scaling from the arm Task 4 deleted — read it out of
`git show HEAD~1:cpp/src/expr.cpp` rather than retyping it, so the conversion is identical.

- [ ] **Step 4: Point Task 4's arm at `ast_scalar` and run green**

- [ ] **Step 5: Commit**

```bash
git add cpp/src/expr.cpp cpp/tests/gpu/test_plan_executor.cpp
git commit -m "the decimal literal keeps its own conversion

cuDF's AST has no fixed-point literal, so a Decimal128 crosses as a scaled
double. ast_scalar is that one difference, named, with validity still read
by literal_is_valid alone."
```

---

### Task 6: The dispatch has no silent gap

**Files:**
- Modify: `cpp/tests/gpu/test_plan_executor.cpp`

- [ ] **Step 1: Write the test**

The dispatch resolves at run time, so a type with no arm fails only when something reaches it. Walk
the types the wire can carry and assert each yields a literal or a refusal naming the type.

The dispatch resolves at run time. Rather than plan a whole query per type, drive
`ast_scalar` + `ast_literal_for` directly — they are file-local to `expr.cpp`, so expose them to the
test through the existing private header `cpp/src/peacock/expr.h`, which is what that header is for.

```cpp
// In cpp/src/peacock/expr.h, beside build_expr — a declaration, not a move:
//
//   // Exposed for EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot: the dispatch
//   // resolves at run time, so a missing arm is only found by reaching it.
//   std::unique_ptr<cudf::scalar> ast_scalar(const fb::ScalarValue* sv);
//   std::unique_ptr<cudf::ast::literal> ast_literal_for(cudf::scalar& s);
//
// and drop `static` from both definitions in expr.cpp.

TEST(Literals, EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot) {
  // One row per type the wire can carry. `expect_literal` is false where cuDF has no
  // ast::literal for it: strings are routed away by is_ast_able before build_expr sees
  // them, and the EMPTY class has no cudf type at all.
  struct Case {
    fb::DataType type;
    bool expect_literal;
  };
  const std::vector<Case> cases = {
      {fb::DataType_Boolean, true},   {fb::DataType_Int8, true},
      {fb::DataType_Int16, true},     {fb::DataType_Int32, true},
      {fb::DataType_Int64, true},     {fb::DataType_Float32, true},
      {fb::DataType_Float64, true},   {fb::DataType_Date32, true},
      {fb::DataType_Decimal128, true},
      {fb::DataType_Utf8, true},      {fb::DataType_LargeUtf8, true},
      {fb::DataType_Utf8View, true},
      {fb::DataType_Null, false},     {fb::DataType_Float16, false},
      {fb::DataType_Binary, false},   {fb::DataType_LargeBinary, false},
      {fb::DataType_BinaryView, false},
  };

  for (const auto& c : cases) {
    flatbuffers::FlatBufferBuilder fbb;
    fb::ScalarValueBuilder sb(fbb);
    sb.add_type(c.type);
    sb.add_is_null(true);  // a typed null needs no value fields for any type
    auto sv_off = sb.Finish();
    fbb.Finish(sv_off);
    const auto* sv = flatbuffers::GetRoot<fb::ScalarValue>(fbb.GetBufferPointer());

    if (c.expect_literal) {
      std::unique_ptr<cudf::scalar> s;
      ASSERT_NO_THROW(s = peacock::ast_scalar(sv))
          << "ast_scalar refused " << fb::EnumNameDataType(c.type);
      EXPECT_NO_THROW(peacock::ast_literal_for(*s))
          << "no ast::literal arm for " << fb::EnumNameDataType(c.type);
    } else {
      // Refusing is correct; refusing *silently* is not. The message must name the
      // type, or a type added to the wire later lands in a dispatch with no arm and
      // the failure says nothing about which.
      try {
        auto s = peacock::ast_scalar(sv);
        peacock::ast_literal_for(*s);
        ADD_FAILURE() << "expected a refusal for "
                      << fb::EnumNameDataType(c.type);
      } catch (const std::exception& e) {
        EXPECT_NE(std::string(e.what()).find("type"), std::string::npos)
            << "the refusal for " << fb::EnumNameDataType(c.type)
            << " does not name a type: " << e.what();
      }
    }
  }
}
```

**The `cases` list duplicates `convert_data_type`'s arms, and that is the weakness of this test.**
The spec's preferred shape reads the source instead, as `exports.rs` tried to — and that is also
what broke a branch when the file moved. So: keep the list, and add the one assertion that makes it
self-correcting — that its length equals the number of `fb::DataType` enumerators, using
`fb::EnumValuesDataType()`. A type added to the schema then fails this test by count, naming nothing,
which is enough to send the next reader here.

- [ ] **Step 2: Run it and read which types refuse**

Some will refuse, and that is fine — `is_ast_able` already routes strings and decimals away, and the
`EMPTY` class has no cuDF type at all. Record the outcome per type in `typed-nulls-detail.md`; that
table is the test's real product.

- [ ] **Step 3: Commit**

```bash
git add cpp/tests/gpu/test_plan_executor.cpp
git commit -m "every wire type either makes an ast literal or says why not

The type dispatch resolves at run time, so a missing arm fails only when
something reaches it. The outcome per type is in typed-nulls-detail.md."
```

---

### Task 7: The LIKE pattern, decided rather than assumed

**Files:**
- Modify: `cpp/src/expr.cpp:901`
- Modify: `cpp/tests/gpu/test_plan_executor.cpp`

- [ ] **Step 1: Establish what the guard actually refuses**

```bash
sed -n '890,910p' cpp/src/expr.cpp
```

The guard above the pattern refuses a `LikeExprNode` whose pattern has no `string_val`. A typed null
serializes as `is_null` set with no value, so the guard does refuse it — confirm that by reading
`serialize.rs`'s null arm, not by assuming.

- [ ] **Step 2: Decide, and write the decision as code or as one line**

Two acceptable outcomes. Either route it through `build_scalar` like everything else, or keep the
local construction under a single line stating why the guard is sufficient — naming the guard, not
restating the conclusion.

- [ ] **Step 3: Write the test that matches the decision**

If it delegates: a LIKE with a null pattern is refused, and the test asserts the refusal message. If
it keeps its own construction: the test asserts the guard refuses a pattern with no `string_val`, so
the claim is checked rather than asserted in prose.

- [ ] **Step 4: Run, then commit**

```bash
git add cpp/src/expr.cpp cpp/tests/gpu/test_plan_executor.cpp
git commit -m "the LIKE pattern's validity is decided, not assumed

It was the eleventh site hardcoding a valid scalar. The guard above it does
refuse a typed null, and a test now says so."
```

---

### Task 8: A bare typed null is still null

The `build_column:830` short-circuit was always correct. Task 4 must not have disturbed it, and
nothing currently proves that.

**Files:**
- Modify: `cpp/tests/gpu/test_plan_executor.cpp`

- [ ] **Step 1: Write the test**

```cpp
TEST(Literals, ABareTypedNullIsStillNull) {
  flatbuffers::FlatBufferBuilder fbb;
  auto scan_node = nation_scan(fbb);

  // No binary op: build_column takes its literal short-circuit at :830 and never
  // reaches build_expr. Always correct, and this is the guard that it stays so.
  auto null_lit = make_null_literal(fbb, fb::DataType_Int64);
  auto exprs = fbb.CreateVector(
      std::vector<flatbuffers::Offset<fb::Expr>>{null_lit});
  auto alias = fbb.CreateString("justnull");
  auto aliases = fbb.CreateVector(
      std::vector<flatbuffers::Offset<flatbuffers::String>>{alias});
  auto proj = fb::CreateCudfProject(fbb, exprs, aliases, scan_node);
  auto proj_node =
      make_plan_node(fbb, fb::PlanNodeKind_CudfProject, proj.Union());
  auto buf = finish_plan(fbb, proj_node);

  WholePlan plan(buf);
  const auto& result = plan.result();

  ASSERT_EQ(result.table->num_columns(), 1);
  auto view = result.table->view().column(0);
  // The type matters as much as the nulls: a typed null that comes back as some other
  // type is still wrong, and nothing downstream would notice on an all-null column.
  EXPECT_EQ(view.type().id(), cudf::type_id::INT64);
  ASSERT_GT(view.size(), 0);
  EXPECT_EQ(view.null_count(), view.size());
}
```

- [ ] **Step 2: Run it — it should pass immediately**

This one is green from the start, which is the point: it is a regression guard, not a red test. Say
so in the commit message so a later reader does not mistake it for TDD gone wrong.

- [ ] **Step 3: Commit**

```bash
git add cpp/tests/gpu/test_plan_executor.cpp
git commit -m "a bare typed null is still null

Green from the start by design: the build_column short-circuit was always
correct and this is the guard that it stays so."
```

---

### Task 9: The wiki, and the catalog check

**Files:**
- Modify: `llm-wiki/tickets.md` (#198), `llm-wiki/build-test.md` (gtest counts)
- Modify: `llm-wiki/tasks/typed-nulls.md` — the completeness signoff, appended once

- [ ] **Step 1: Look for the catalog's `bug_` test**

```bash
grep -rn 'bug_' peacockdb-core/src/wire/gpu_tests/ 2>/dev/null
```

The spec's "What it may close in the catalog": a nullable column exporting non-nullable in a plan
carrying a null literal. If such a test exists, delete it in this change and say so. If none exists,
say that too — it means the catalog had no query reaching the AST literal path, which is worth the
one line either way.

- [ ] **Step 2: Update the gtest count in `build-test.md`**

The Plan-executor row carries a count. Recount; do not adjust by the number of tests you think you
added.

- [ ] **Step 3: Close #198**

Move it to `llm-wiki/archive/archived-tickets.md` under Done, with one line naming this task. Numbers
are never reused.

- [ ] **Step 4: Run the full suite one more time, unfiltered**

The last filtered run proves the new tests; this proves nothing else moved. Paste the summary line
into the report.

- [ ] **Step 5: Commit**

```bash
git add llm-wiki/
git commit -m "close #198 and square the wiki

The gtest count, the ticket's move to the archive, and whether the schema
catalog had a bug_ test for the nullability shadow."
```

---

## Self-review against the spec

- **§1 One scalar builder** — Tasks 4 and 5.
- **§2 The LIKE pattern** — Task 7.
- **§3 The gtest helpers** — Task 1, first, as the spec requires.
- **§4 Tests** — all seven: arithmetic (2), comparison (3), decimal null and decimal value (5),
  dispatch exhaustiveness (6), LIKE (7), bare null (8). The conditional "two builders cannot diverge"
  test is not scheduled because Tasks 4 and 5 leave one builder; if the author keeps two, it becomes
  mandatory and belongs in Task 5.
- **Scope table** — no Rust, no `.fbs`, no ABI, no header: nothing in any task touches them.
- **Goldens** — none expected to move. A device result that does move is a finding for the report,
  per the spec's own table, and Task 9 step 4 is where it would show.
